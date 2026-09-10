use super::{cancelled, tools, AgentError, ToolCall};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, HashMap},
    path::{Path, PathBuf},
    process::Stdio,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{
    io::{AsyncBufRead, AsyncBufReadExt, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader},
    process::{Child, ChildStdin, ChildStdout},
    sync::watch,
};
use url::Url;

const MAX_MESSAGE_BYTES: usize = 4 * 1024 * 1024;
const MAX_OUTPUT_BYTES: usize = 32_000;
const MAX_RESULTS: usize = 200;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(12);
const DIAGNOSTIC_WAIT: Duration = Duration::from_millis(700);

fn error(message: impl Into<String>) -> AgentError {
    AgentError::new("lsp_error", &message.into())
}

fn argument<'a>(args: &'a Value, key: &str) -> Result<&'a str, AgentError> {
    args[key]
        .as_str()
        .ok_or_else(|| error(format!("Informe o campo '{key}' para a ferramenta LSP.")))
}

pub(super) fn definitions() -> Vec<Value> {
    let path = json!({"type":"string","description":"Arquivo UTF-8 dentro do projeto."});
    let line = json!({"type":"integer","minimum":1,"description":"Linha iniciando em 1."});
    let column =
        json!({"type":"integer","minimum":1,"description":"Coluna de caractere iniciando em 1."});
    vec![
        tools::definition(
            "lsp_definition",
            "Find the definition of the symbol at a source position using a project-local or Jarvis-managed language server. Prefer this over text search for code navigation.",
            json!({"path":path,"line":line,"column":column}),
            &["path", "line", "column"],
        ),
        tools::definition(
            "lsp_references",
            "Find references to the symbol at a source position using a project-local or Jarvis-managed language server. Results outside the project are omitted.",
            json!({"path":path,"line":line,"column":column,"includeDeclaration":{"type":"boolean","default":true}}),
            &["path", "line", "column"],
        ),
        tools::definition(
            "lsp_symbols",
            "List structural symbols in one source file using a project-local or Jarvis-managed language server. Optionally filter the bounded result by name.",
            json!({"path":path,"query":{"type":"string","maxLength":200}}),
            &["path"],
        ),
        tools::definition(
            "lsp_diagnostics",
            "Read current compiler and language-server diagnostics for one source file. Opening the file may start a project-local or Jarvis-managed server.",
            json!({"path":path}),
            &["path"],
        ),
    ]
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum ServerKind {
    TypeScript,
    Rust,
    Go,
    Python,
}

impl ServerKind {
    fn for_path(path: &Path) -> Result<Self, AgentError> {
        let extension = path
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        match extension.as_str() {
            "ts" | "tsx" | "js" | "jsx" | "mjs" | "cjs" | "mts" | "cts" => {
                Ok(Self::TypeScript)
            }
            "rs" => Ok(Self::Rust),
            "go" => Ok(Self::Go),
            "py" | "pyi" => Ok(Self::Python),
            _ => Err(error(format!(
                "Não há integração LSP configurada para a extensão .{extension}. Use leitura e busca de texto neste arquivo."
            ))),
        }
    }

    fn program(self) -> &'static str {
        match self {
            Self::TypeScript => "typescript-language-server",
            Self::Rust => "rust-analyzer",
            Self::Go => "gopls",
            Self::Python => "pyright-langserver",
        }
    }

    fn arguments(self) -> &'static [&'static str] {
        match self {
            Self::TypeScript | Self::Python => &["--stdio"],
            Self::Rust => &[],
            Self::Go => &["serve"],
        }
    }

    fn language_id(self, path: &Path) -> &'static str {
        match self {
            Self::TypeScript => match path.extension().and_then(|value| value.to_str()) {
                Some("tsx") => "typescriptreact",
                Some("js" | "mjs" | "cjs") => "javascript",
                Some("jsx") => "javascriptreact",
                _ => "typescript",
            },
            Self::Rust => "rust",
            Self::Go => "go",
            Self::Python => "python",
        }
    }

    fn install_hint(self) -> &'static str {
        match self {
            Self::TypeScript => "Repare Servidores LSP em Configurações → Ferramentas → Core.",
            Self::Rust => "Instale rust-analyzer e disponibilize-o no PATH.",
            Self::Go => "Instale gopls e disponibilize-o no PATH.",
            Self::Python => "Repare Servidores LSP em Configurações → Ferramentas → Core.",
        }
    }
}

#[derive(Clone)]
struct Document {
    uri: String,
    version: i64,
    text: String,
}

struct Server {
    root: PathBuf,
    root_uri: String,
    kind: ServerKind,
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    stderr: Arc<Mutex<String>>,
    stderr_task: tokio::task::JoinHandle<()>,
    next_id: u64,
    documents: HashMap<PathBuf, Document>,
    diagnostics: HashMap<String, Vec<Value>>,
    pull_diagnostics: bool,
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.start_kill();
        self.stderr_task.abort();
    }
}

impl Server {
    async fn start(root: &Path, home: &Path, kind: ServerKind) -> Result<Self, AgentError> {
        let mut argv = server_command(root, home, kind);
        argv.extend(kind.arguments().iter().map(|value| (*value).to_owned()));
        let mut command = crate::mcp::executable::local_command(&argv, &BTreeMap::new(), root)
            .map_err(|_| {
                error(format!(
                    "Servidor LSP '{}' não encontrado. {}",
                    kind.program(),
                    kind.install_hint()
                ))
            })?;
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        let mut child = command.spawn().map_err(|_| {
            error(format!(
                "Não foi possível iniciar o servidor LSP '{}'. {}",
                kind.program(),
                kind.install_hint()
            ))
        })?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| error("Entrada LSP indisponível."))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| error("Saída LSP indisponível."))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| error("Saída de erro LSP indisponível."))?;
        let stderr_buffer = Arc::new(Mutex::new(String::new()));
        let capture = Arc::clone(&stderr_buffer);
        let stderr_task = tokio::spawn(async move {
            let mut stream = BufReader::new(stderr);
            let mut chunk = [0_u8; 1024];
            while let Ok(count) = stream.read(&mut chunk).await {
                if count == 0 {
                    break;
                }
                if let Ok(mut output) = capture.lock() {
                    if output.len() < 4096 {
                        let keep = count.min(4096 - output.len());
                        output.push_str(&String::from_utf8_lossy(&chunk[..keep]));
                    }
                }
            }
        });
        let root_uri = Url::from_directory_path(root)
            .map_err(|()| error("Não foi possível representar a pasta do projeto para o LSP."))?
            .to_string();
        let mut server = Self {
            root: root.to_path_buf(),
            root_uri,
            kind,
            child,
            stdin,
            stdout: BufReader::new(stdout),
            stderr: stderr_buffer,
            stderr_task,
            next_id: 0,
            documents: HashMap::new(),
            diagnostics: HashMap::new(),
            pull_diagnostics: false,
        };
        let workspace_name = root
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("projeto");
        let initialized = server
            .request(
                "initialize",
                json!({
                    "processId": std::process::id(),
                    "clientInfo":{"name":"Jarvis","version":env!("CARGO_PKG_VERSION")},
                    "rootUri":server.root_uri,
                    "workspaceFolders":[{"uri":server.root_uri,"name":workspace_name}],
                    "capabilities":{
                        "general":{"positionEncodings":["utf-16"]},
                        "textDocument":{
                            "definition":{"dynamicRegistration":false,"linkSupport":true},
                            "references":{"dynamicRegistration":false},
                            "documentSymbol":{"dynamicRegistration":false,"hierarchicalDocumentSymbolSupport":true},
                            "publishDiagnostics":{"relatedInformation":true,"versionSupport":true}
                        },
                        "workspace":{"workspaceFolders":true}
                    },
                    "trace":"off"
                }),
            )
            .await?;
        server.pull_diagnostics = !initialized["capabilities"]["diagnosticProvider"].is_null();
        server.notify("initialized", json!({})).await?;
        Ok(server)
    }

    async fn request(&mut self, method: &str, params: Value) -> Result<Value, AgentError> {
        self.next_id += 1;
        let id = self.next_id;
        write_message(
            &mut self.stdin,
            &json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}),
        )
        .await?;
        let deadline = tokio::time::Instant::now() + REQUEST_TIMEOUT;
        loop {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            if remaining.is_zero() {
                return Err(self.timeout_error(method));
            }
            let message =
                match tokio::time::timeout(remaining, read_message(&mut self.stdout)).await {
                    Ok(result) => result?,
                    Err(_) => return Err(self.timeout_error(method)),
                };
            if message["id"].as_u64() == Some(id) {
                if let Some(cause) = message.get("error") {
                    let detail = cause["message"]
                        .as_str()
                        .unwrap_or("O servidor recusou a consulta.");
                    return Err(error(format!("Falha no LSP: {detail}")));
                }
                return Ok(message.get("result").cloned().unwrap_or(Value::Null));
            }
            self.handle_message(message).await?;
        }
    }

    fn timeout_error(&mut self, method: &str) -> AgentError {
        let stopped = self.child.try_wait().ok().flatten().is_some();
        let stderr = self
            .stderr
            .lock()
            .ok()
            .map(|value| value.trim().chars().take(500).collect::<String>())
            .unwrap_or_default();
        let detail = if stopped && !stderr.is_empty() {
            format!(" O processo foi encerrado: {stderr}")
        } else if stopped {
            " O processo foi encerrado.".to_owned()
        } else {
            String::new()
        };
        error(format!(
            "O servidor LSP não respondeu a '{method}' dentro do limite.{detail}"
        ))
    }

    async fn notify(&mut self, method: &str, params: Value) -> Result<(), AgentError> {
        write_message(
            &mut self.stdin,
            &json!({"jsonrpc":"2.0","method":method,"params":params}),
        )
        .await
    }

    async fn handle_message(&mut self, message: Value) -> Result<(), AgentError> {
        if message["method"] == "textDocument/publishDiagnostics" {
            if let (Some(uri), Some(items)) = (
                message["params"]["uri"].as_str(),
                message["params"]["diagnostics"].as_array(),
            ) {
                self.diagnostics.insert(uri.to_owned(), items.clone());
            }
            return Ok(());
        }
        let Some(id) = message
            .get("id")
            .filter(|_| message.get("method").is_some())
        else {
            return Ok(());
        };
        let result = match message["method"].as_str().unwrap_or_default() {
            "workspace/configuration" => json!(vec![
                Value::Null;
                message["params"]["items"]
                    .as_array()
                    .map_or(0, Vec::len)
            ]),
            "workspace/workspaceFolders" => json!([{
                "uri":self.root_uri,
                "name":self.root.file_name().and_then(|value| value.to_str()).unwrap_or("projeto")
            }]),
            "workspace/applyEdit" => {
                json!({"applied":false,"failureReason":"Jarvis não aceita edições iniciadas pelo servidor LSP."})
            }
            _ => Value::Null,
        };
        write_message(
            &mut self.stdin,
            &json!({"jsonrpc":"2.0","id":id,"result":result}),
        )
        .await
    }

    async fn sync_document(&mut self, path: &Path, text: String) -> Result<String, AgentError> {
        let uri = file_uri(path)?;
        if let Some(document) = self.documents.get_mut(path) {
            if document.text != text {
                document.version += 1;
                document.text.clone_from(&text);
                let version = document.version;
                self.notify(
                    "textDocument/didChange",
                    json!({"textDocument":{"uri":uri,"version":version},"contentChanges":[{"text":text}]}),
                )
                .await?;
            }
        } else {
            self.notify(
                "textDocument/didOpen",
                json!({"textDocument":{"uri":uri,"languageId":self.kind.language_id(path),"version":1,"text":text}}),
            )
            .await?;
            self.documents.insert(
                path.to_path_buf(),
                Document {
                    uri: uri.clone(),
                    version: 1,
                    text,
                },
            );
        }
        Ok(uri)
    }

    async fn close_document(&mut self, path: &Path) -> Result<(), AgentError> {
        if let Some(document) = self.documents.remove(path) {
            self.diagnostics.remove(&document.uri);
            self.notify(
                "textDocument/didClose",
                json!({"textDocument":{"uri":document.uri}}),
            )
            .await?;
        }
        Ok(())
    }

    async fn drain_notifications(&mut self, duration: Duration) -> Result<(), AgentError> {
        let deadline = tokio::time::Instant::now() + duration;
        loop {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            if remaining.is_zero() {
                return Ok(());
            }
            match tokio::time::timeout(remaining, read_message(&mut self.stdout)).await {
                Ok(Ok(message)) => self.handle_message(message).await?,
                Ok(Err(cause)) => return Err(cause),
                Err(_) => return Ok(()),
            }
        }
    }
}

pub(super) struct Registry {
    root: PathBuf,
    home: PathBuf,
    servers: HashMap<ServerKind, Server>,
}

impl Registry {
    pub(super) fn new(root: &Path, home: &Path) -> Result<Self, AgentError> {
        if std::fs::canonicalize(root).ok().as_deref() != Some(root) || !root.is_dir() {
            return Err(error(
                "A pasta original do projeto não está disponível para o LSP.",
            ));
        }
        Ok(Self {
            root: root.to_path_buf(),
            home: home.to_path_buf(),
            servers: HashMap::new(),
        })
    }

    pub(super) async fn execute(
        &mut self,
        tool: &ToolCall,
        mut signal: watch::Receiver<bool>,
    ) -> Result<String, AgentError> {
        let path = tools::scoped(&self.root, argument(&tool.args, "path")?, false)?;
        let text = tools::read_text(&path)?;
        let kind = ServerKind::for_path(&path)?;
        if !self.servers.contains_key(&kind) {
            let root = self.root.clone();
            let home = self.home.clone();
            let server = tokio::select! {
                _ = cancelled(&mut signal) => return Err(AgentError::cancelled()),
                result = Server::start(&root, &home, kind) => result?,
            };
            self.servers.insert(kind, server);
        }
        let relative = relative_path(&self.root, &path)?;
        let server = self
            .servers
            .get_mut(&kind)
            .ok_or_else(AgentError::internal)?;
        tokio::select! {
            _ = cancelled(&mut signal) => Err(AgentError::cancelled()),
            result = execute_tool(server, tool, &path, &relative, text) => result,
        }
    }

    pub(super) async fn refresh(&mut self, relative: &str) -> Result<(), AgentError> {
        let candidate = self.root.join(relative);
        let Ok(kind) = ServerKind::for_path(&candidate) else {
            return Ok(());
        };
        let Some(server) = self.servers.get_mut(&kind) else {
            return Ok(());
        };
        match tools::scoped(&self.root, relative, false)
            .and_then(|path| tools::read_text(&path).map(|text| (path, text)))
        {
            Ok((path, text)) => {
                server.sync_document(&path, text).await?;
            }
            Err(_) => {
                server.close_document(&candidate).await?;
            }
        }
        Ok(())
    }

    pub(super) async fn diagnostics_after_changes(
        &mut self,
        paths: &[String],
        signal: watch::Receiver<bool>,
    ) -> String {
        let mut reports = Vec::new();
        let mut unavailable = Vec::new();
        for path in paths.iter().take(8) {
            if ServerKind::for_path(Path::new(path)).is_err() {
                continue;
            }
            let tool = ToolCall {
                id: format!("patch-diagnostics-{path}"),
                name: "lsp_diagnostics".into(),
                args: json!({"path":path}),
                status: "pending".into(),
                output: String::new(),
                duration_ms: 0,
            };
            match self.execute(&tool, signal.clone()).await {
                Ok(output) => {
                    if serde_json::from_str::<Value>(&output)
                        .ok()
                        .and_then(|value| value["count"].as_u64())
                        .is_some_and(|count| count > 0)
                    {
                        reports.push(output);
                    }
                }
                Err(cause) if cause.code == "cancelled" => break,
                Err(cause) => unavailable.push(format!("{path}: {}", cause.message)),
            }
        }
        let mut output = String::new();
        if !reports.is_empty() {
            output.push_str("\n\nDiagnósticos LSP pós-patch:\n");
            output.push_str(&reports.join("\n"));
        }
        if !unavailable.is_empty() {
            output.push_str("\n\nDiagnóstico LSP indisponível:\n");
            output.push_str(&unavailable.join("\n"));
        }
        if paths.len() > 8 {
            output.push_str("\n\nDiagnósticos limitados aos primeiros 8 arquivos alterados.");
        }
        if output.len() > MAX_OUTPUT_BYTES {
            let mut boundary = MAX_OUTPUT_BYTES;
            while !output.is_char_boundary(boundary) {
                boundary -= 1;
            }
            output.truncate(boundary);
            output.push_str("\n[Diagnósticos pós-patch truncados.]\n");
        }
        output
    }
}

async fn execute_tool(
    server: &mut Server,
    tool: &ToolCall,
    path: &Path,
    relative: &str,
    text: String,
) -> Result<String, AgentError> {
    let uri = server.sync_document(path, text.clone()).await?;
    match tool.name.as_str() {
        "lsp_definition" | "lsp_references" => {
            let position = position(&text, &tool.args)?;
            let params = if tool.name == "lsp_references" {
                json!({"textDocument":{"uri":uri},"position":position,"context":{"includeDeclaration":tool.args["includeDeclaration"].as_bool().unwrap_or(true)}})
            } else {
                json!({"textDocument":{"uri":uri},"position":position})
            };
            let method = if tool.name == "lsp_references" {
                "textDocument/references"
            } else {
                "textDocument/definition"
            };
            let result = server.request(method, params).await?;
            format_locations(&server.root, result)
        }
        "lsp_symbols" => {
            let result = server
                .request(
                    "textDocument/documentSymbol",
                    json!({"textDocument":{"uri":uri}}),
                )
                .await?;
            format_symbols(&server.root, relative, result, tool.args["query"].as_str())
        }
        "lsp_diagnostics" => {
            if server.pull_diagnostics {
                let result = server
                    .request(
                        "textDocument/diagnostic",
                        json!({"textDocument":{"uri":uri}}),
                    )
                    .await?;
                if let Some(items) = result["items"].as_array() {
                    server.diagnostics.insert(uri.clone(), items.clone());
                }
            } else {
                server.drain_notifications(DIAGNOSTIC_WAIT).await?;
            }
            format_diagnostics(
                relative,
                server.diagnostics.get(&uri).cloned().unwrap_or_default(),
            )
        }
        _ => Err(error("Ferramenta LSP desconhecida.")),
    }
}

fn local_program(root: &Path, name: &str) -> PathBuf {
    let bin = root.join("node_modules").join(".bin");
    #[cfg(windows)]
    for extension in ["cmd", "exe", "com", "bat"] {
        let candidate = bin.join(name).with_extension(extension);
        if candidate.is_file() {
            return candidate;
        }
    }
    #[cfg(not(windows))]
    {
        let candidate = bin.join(name);
        if candidate.is_file() {
            return candidate;
        }
    }
    PathBuf::from(name)
}

fn server_command(root: &Path, home: &Path, kind: ServerKind) -> Vec<String> {
    let local = local_program(root, kind.program());
    if local != Path::new(kind.program()) {
        return vec![local.to_string_lossy().into_owned()];
    }
    crate::core::lsp::command(home, kind.program())
        .unwrap_or_else(|| vec![kind.program().to_owned()])
}

fn file_uri(path: &Path) -> Result<String, AgentError> {
    Url::from_file_path(path)
        .map(|value| value.to_string())
        .map_err(|()| error("Não foi possível representar o caminho para o LSP."))
}

fn relative_path(root: &Path, path: &Path) -> Result<String, AgentError> {
    path.strip_prefix(root)
        .map(|value| value.to_string_lossy().replace('\\', "/"))
        .map_err(|_| error("O servidor LSP retornou um caminho fora do projeto."))
}

fn position(text: &str, args: &Value) -> Result<Value, AgentError> {
    let line = args["line"]
        .as_u64()
        .filter(|value| *value > 0)
        .ok_or_else(|| error("A linha LSP deve iniciar em 1."))?;
    let column = args["column"]
        .as_u64()
        .filter(|value| *value > 0)
        .ok_or_else(|| error("A coluna LSP deve iniciar em 1."))?;
    let line_index = usize::try_from(line - 1).map_err(|_| error("Linha LSP inválida."))?;
    let column_index = usize::try_from(column - 1).map_err(|_| error("Coluna LSP inválida."))?;
    let source_line = text
        .split('\n')
        .nth(line_index)
        .ok_or_else(|| error("A linha informada não existe no arquivo."))?
        .trim_end_matches('\r');
    if source_line.chars().count() < column_index {
        return Err(error("A coluna informada ultrapassa o fim da linha."));
    }
    let utf16_column: usize = source_line
        .chars()
        .take(column_index)
        .map(char::len_utf16)
        .sum();
    Ok(json!({"line":line_index,"character":utf16_column}))
}

fn flatten_locations(value: Value) -> Vec<Value> {
    match value {
        Value::Null => vec![],
        Value::Array(items) => items,
        item => vec![item],
    }
}

fn location_parts(value: &Value) -> Option<(&str, &Value)> {
    if let Some(uri) = value["uri"].as_str() {
        return Some((uri, &value["range"]));
    }
    value["targetUri"]
        .as_str()
        .map(|uri| (uri, &value["targetSelectionRange"]))
}

fn normalize_location(root: &Path, value: &Value) -> Option<Value> {
    let (uri, range) = location_parts(value)?;
    let path = Url::parse(uri)
        .ok()?
        .to_file_path()
        .ok()?
        .canonicalize()
        .ok()?;
    let relative = relative_path(root, &path).ok()?;
    Some(json!({
        "path":relative,
        "start":human_position(&range["start"]),
        "end":human_position(&range["end"])
    }))
}

fn human_position(value: &Value) -> Value {
    json!({
        "line":value["line"].as_u64().unwrap_or(0) + 1,
        "column":value["character"].as_u64().unwrap_or(0) + 1
    })
}

fn bounded_json(value: &Value) -> Result<String, AgentError> {
    let mut output = serde_json::to_string_pretty(value)
        .map_err(|_| error("Não foi possível formatar o resultado LSP."))?;
    if output.len() > MAX_OUTPUT_BYTES {
        let mut boundary = MAX_OUTPUT_BYTES;
        while !output.is_char_boundary(boundary) {
            boundary -= 1;
        }
        output.truncate(boundary);
        output.push_str("\n[Resultado LSP truncado; refine a consulta.]\n");
    }
    Ok(output)
}

fn format_locations(root: &Path, value: Value) -> Result<String, AgentError> {
    let locations: Vec<_> = flatten_locations(value)
        .iter()
        .filter_map(|item| normalize_location(root, item))
        .take(MAX_RESULTS)
        .collect();
    bounded_json(&json!({"locations":locations,"count":locations.len()}))
}

fn format_symbols(
    root: &Path,
    relative: &str,
    value: Value,
    query: Option<&str>,
) -> Result<String, AgentError> {
    let query = query.unwrap_or_default().trim().to_ascii_lowercase();
    let mut symbols = Vec::new();
    if let Some(items) = value.as_array() {
        for item in items {
            collect_symbol(root, relative, item, &query, &mut symbols);
            if symbols.len() >= MAX_RESULTS {
                break;
            }
        }
    }
    bounded_json(&json!({"symbols":symbols,"count":symbols.len()}))
}

fn collect_symbol(root: &Path, relative: &str, item: &Value, query: &str, output: &mut Vec<Value>) {
    if output.len() >= MAX_RESULTS {
        return;
    }
    let name = item["name"].as_str().unwrap_or_default();
    let detail = item["detail"].as_str().unwrap_or_default();
    let matched = query.is_empty()
        || name.to_ascii_lowercase().contains(query)
        || detail.to_ascii_lowercase().contains(query);
    if matched {
        let location = item
            .get("location")
            .and_then(|location| normalize_location(root, location));
        let (path, range) = if let Some(location) = location {
            (
                location["path"].clone(),
                json!({"start":location["start"],"end":location["end"]}),
            )
        } else {
            (
                json!(relative),
                json!({
                    "start":human_position(&item["selectionRange"]["start"]),
                    "end":human_position(&item["selectionRange"]["end"])
                }),
            )
        };
        output.push(json!({
            "name":name,
            "detail":detail,
            "kind":item["kind"].as_u64(),
            "path":path,
            "start":range["start"],
            "end":range["end"]
        }));
    }
    if let Some(children) = item["children"].as_array() {
        for child in children {
            collect_symbol(root, relative, child, query, output);
            if output.len() >= MAX_RESULTS {
                break;
            }
        }
    }
}

fn format_diagnostics(relative: &str, items: Vec<Value>) -> Result<String, AgentError> {
    let diagnostics: Vec<_> = items
        .into_iter()
        .take(MAX_RESULTS)
        .map(|item| {
            json!({
                "severity":match item["severity"].as_u64() {
                    Some(1) => "error",
                    Some(2) => "warning",
                    Some(3) => "information",
                    Some(4) => "hint",
                    _ => "unknown",
                },
                "message":item["message"].as_str().unwrap_or_default(),
                "source":item["source"].as_str(),
                "code":item.get("code"),
                "start":human_position(&item["range"]["start"]),
                "end":human_position(&item["range"]["end"])
            })
        })
        .collect();
    bounded_json(&json!({"path":relative,"diagnostics":diagnostics,"count":diagnostics.len()}))
}

async fn write_message(
    writer: &mut (impl AsyncWrite + Unpin),
    message: &Value,
) -> Result<(), AgentError> {
    let body = serde_json::to_vec(message).map_err(|_| error("Mensagem LSP inválida."))?;
    if body.len() > MAX_MESSAGE_BYTES {
        return Err(error("Mensagem LSP excedeu o limite seguro."));
    }
    writer
        .write_all(format!("Content-Length: {}\r\n\r\n", body.len()).as_bytes())
        .await
        .map_err(|_| error("Falha ao enviar uma mensagem ao servidor LSP."))?;
    writer
        .write_all(&body)
        .await
        .map_err(|_| error("Falha ao enviar uma mensagem ao servidor LSP."))?;
    writer
        .flush()
        .await
        .map_err(|_| error("Falha ao enviar uma mensagem ao servidor LSP."))
}

async fn read_message(reader: &mut (impl AsyncBufRead + Unpin)) -> Result<Value, AgentError> {
    let mut content_length = None;
    for _ in 0..32 {
        let mut line = String::new();
        let count = reader
            .read_line(&mut line)
            .await
            .map_err(|_| error("Falha ao ler a resposta do servidor LSP."))?;
        if count == 0 {
            return Err(error("O servidor LSP encerrou a conexão."));
        }
        if line == "\r\n" || line == "\n" {
            break;
        }
        if let Some(value) = line
            .strip_prefix("Content-Length:")
            .or_else(|| line.strip_prefix("content-length:"))
        {
            content_length = value.trim().parse::<usize>().ok();
        }
    }
    let size = content_length
        .filter(|size| *size <= MAX_MESSAGE_BYTES)
        .ok_or_else(|| error("Resposta LSP sem tamanho válido."))?;
    let mut body = vec![0_u8; size];
    reader
        .read_exact(&mut body)
        .await
        .map_err(|_| error("Resposta LSP incompleta."))?;
    serde_json::from_slice(&body).map_err(|_| error("O servidor LSP retornou JSON inválido."))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::tests::Fixture;

    #[test]
    fn exposes_bounded_read_only_navigation_tools() {
        let definitions = definitions();
        let names: Vec<_> = definitions
            .iter()
            .filter_map(|item| item["name"].as_str())
            .collect();
        assert_eq!(
            names,
            [
                "lsp_definition",
                "lsp_references",
                "lsp_symbols",
                "lsp_diagnostics"
            ]
        );
        assert!(definitions
            .iter()
            .all(|item| item["parameters"]["additionalProperties"].as_bool() == Some(false)));
    }

    #[test]
    fn maps_supported_languages_and_prefers_project_node_binaries() {
        let fixture = Fixture::new();
        for (name, expected) in [
            ("component.tsx", ServerKind::TypeScript),
            ("lib.rs", ServerKind::Rust),
            ("main.go", ServerKind::Go),
            ("worker.py", ServerKind::Python),
        ] {
            assert_eq!(ServerKind::for_path(Path::new(name)).unwrap(), expected);
        }
        assert!(ServerKind::for_path(Path::new("README.md")).is_err());
        let bin = fixture.root.join("node_modules/.bin");
        std::fs::create_dir_all(&bin).unwrap();
        #[cfg(windows)]
        let executable = bin.join("typescript-language-server.cmd");
        #[cfg(not(windows))]
        let executable = bin.join("typescript-language-server");
        std::fs::write(&executable, "fixture").unwrap();
        assert_eq!(
            local_program(&fixture.root, "typescript-language-server"),
            executable
        );
    }

    #[test]
    fn resolves_project_then_managed_then_path_language_servers() {
        let fixture = Fixture::new();
        let home = tempfile::tempdir().unwrap();
        let package = crate::core::root(home.path()).join("lsp/fixture");
        let node_relative = if cfg!(windows) {
            "runtime/node.exe"
        } else {
            "runtime/bin/node"
        };
        let server_relative = "node_modules/typescript-language-server/lib/cli.mjs";
        for relative in [node_relative, server_relative] {
            let path = package.join(relative);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, "fixture").unwrap();
        }
        std::fs::write(
            crate::core::root(home.path()).join("manifest.json"),
            serde_json::to_vec(&json!({
                "installations": {
                    "lsp": {
                        "version": "1.0.0",
                        "directory": "lsp/fixture",
                        "files": [node_relative, server_relative]
                    }
                }
            }))
            .unwrap(),
        )
        .unwrap();

        let managed = server_command(&fixture.root, home.path(), ServerKind::TypeScript);
        assert_eq!(
            managed,
            vec![
                package.join(node_relative).to_string_lossy().into_owned(),
                package.join(server_relative).to_string_lossy().into_owned(),
            ]
        );

        let bin = fixture.root.join("node_modules/.bin");
        std::fs::create_dir_all(&bin).unwrap();
        #[cfg(windows)]
        let local = bin.join("typescript-language-server.cmd");
        #[cfg(not(windows))]
        let local = bin.join("typescript-language-server");
        std::fs::write(&local, "fixture").unwrap();
        assert_eq!(
            server_command(&fixture.root, home.path(), ServerKind::TypeScript),
            vec![local.to_string_lossy().into_owned()]
        );

        let empty_home = tempfile::tempdir().unwrap();
        assert_eq!(
            server_command(&fixture.root, empty_home.path(), ServerKind::Rust),
            vec!["rust-analyzer"]
        );
    }

    #[test]
    fn translates_human_columns_to_utf16_positions() {
        let source = "zero\na😀b\n";
        assert_eq!(
            position(source, &json!({"line":2,"column":3})).unwrap(),
            json!({"line":1,"character":3})
        );
        assert!(position(source, &json!({"line":5,"column":1})).is_err());
        assert!(position(source, &json!({"line":2,"column":9})).is_err());
    }

    #[test]
    fn filters_locations_outside_the_project() {
        let fixture = Fixture::new();
        let inside = fixture.root.join("inside.rs");
        std::fs::write(&inside, "fn inside() {}\n").unwrap();
        let outside = tempfile::NamedTempFile::new().unwrap();
        let range = json!({"start":{"line":0,"character":3},"end":{"line":0,"character":8}});
        let output = format_locations(
            &fixture.root,
            json!([
                {"uri":file_uri(&inside).unwrap(),"range":range},
                {"uri":file_uri(outside.path()).unwrap(),"range":range}
            ]),
        )
        .unwrap();
        let parsed: Value = serde_json::from_str(&output).unwrap();
        assert_eq!(parsed["count"], 1);
        assert_eq!(parsed["locations"][0]["path"], "inside.rs");
        assert_eq!(parsed["locations"][0]["start"]["line"], 1);
    }

    #[tokio::test]
    async fn json_rpc_framing_round_trips_unicode() {
        let (client, server) = tokio::io::duplex(2048);
        let (client_read, mut client_write) = tokio::io::split(client);
        let (server_read, mut server_write) = tokio::io::split(server);
        let message = json!({"jsonrpc":"2.0","id":1,"result":"olá 😀"});
        let expected = message.clone();
        let send = tokio::spawn(async move { write_message(&mut client_write, &message).await });
        let mut reader = BufReader::new(server_read);
        assert_eq!(read_message(&mut reader).await.unwrap(), expected);
        send.await.unwrap().unwrap();

        server_write
            .write_all(b"Content-Length: 2\r\n\r\n{}")
            .await
            .unwrap();
        let mut client_reader = BufReader::new(client_read);
        assert_eq!(read_message(&mut client_reader).await.unwrap(), json!({}));
    }
}
