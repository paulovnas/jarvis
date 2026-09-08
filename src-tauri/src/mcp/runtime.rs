use super::{config::Config, error, Check, McpError, McpState, Server};
use crate::persistence::AppState;
use rmcp::{
    model::{CallToolRequestParams, Tool},
    service::{NotificationContext, RunningService},
    transport::{
        streamable_http_client::StreamableHttpClientTransportConfig, StreamableHttpClientTransport,
    },
    ClientHandler, RoleClient, ServiceExt,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::sync::watch;

const MAX_TOOLS: usize = 96;
const MAX_OUTPUT: usize = 48_000;

#[derive(Clone, Default)]
pub struct Handler {
    changed: Arc<AtomicBool>,
}
impl ClientHandler for Handler {
    async fn on_tool_list_changed(&self, _: NotificationContext<RoleClient>) {
        self.changed.store(true, Ordering::Relaxed);
    }
}

pub struct Client {
    pub server: Server,
    service: RunningService<RoleClient, Handler>,
    config: Config,
    tools: Vec<RegisteredTool>,
}
struct RegisteredTool {
    definition: Value,
    original: String,
    validator: jsonschema::Validator,
    read_only: bool,
}

async fn cancelled(signal: &mut watch::Receiver<bool>) {
    loop {
        if *signal.borrow_and_update() {
            return;
        }
        if signal.changed().await.is_err() {
            return;
        }
    }
}
fn protocol_error() -> McpError {
    error("O MCP não respondeu corretamente. Confira a configuração, a conexão e as credenciais.")
}

// No retries for tool execution: a lost response does not mean the action failed.
pub async fn connect(
    server: Server,
    config: Config,
    root: &Path,
    mut signal: watch::Receiver<bool>,
) -> Result<Client, McpError> {
    if !server.enabled || !server.configured || !config.configured() {
        return Err(error("Ative e configure o MCP antes de testar."));
    }
    let task = async {
        let handler = Handler::default();
        let service = match &config {
            Config::Local {
                command,
                cwd,
                environment,
                ..
            } => {
                let directory = cwd
                    .as_ref()
                    .map_or_else(|| root.to_path_buf(), |cwd| root.join(cwd));
                let mut cmd = super::executable::local_command(command, environment, &directory)?;
                crate::background::prepare_node(&mut cmd)
                    .map_err(|_| error("Não foi possível preparar o runtime do MCP."))?;
                let transport = super::stdio::spawn(cmd).map_err(|_| {
                    error(
                        "Não foi possível iniciar o MCP. Verifique se o executável está instalado.",
                    )
                })?;
                handler
                    .serve(transport)
                    .await
                    .map_err(|_| protocol_error())?
            }
            Config::Remote { url, headers, .. } => {
                let mut custom_headers = HashMap::new();
                for (key, value) in headers {
                    custom_headers.insert(
                        reqwest::header::HeaderName::from_bytes(key.as_bytes())
                            .map_err(|_| protocol_error())?,
                        reqwest::header::HeaderValue::from_str(value)
                            .map_err(|_| protocol_error())?,
                    );
                }
                let client = reqwest::Client::builder()
                    .redirect(reqwest::redirect::Policy::none())
                    .connect_timeout(Duration::from_secs(10))
                    .build()
                    .map_err(|_| protocol_error())?;
                let transport = StreamableHttpClientTransport::with_client(
                    client,
                    StreamableHttpClientTransportConfig::with_uri(url.clone())
                        .custom_headers(custom_headers)
                        .max_sse_event_size(1024 * 1024)
                        .reinit_on_expired_session(false),
                );
                handler
                    .serve(transport)
                    .await
                    .map_err(|_| protocol_error())?
            }
        };
        let mut client = Client {
            server,
            service,
            config: config.clone(),
            tools: Vec::new(),
        };
        client.refresh().await?;
        Ok(client)
    };
    tokio::select! {
        _ = cancelled(&mut signal) => Err(error("Conexão MCP interrompida.")),
        result = tokio::time::timeout(config.timeout(), task) => result.map_err(|_| error("O MCP excedeu o tempo limite. Confira a configuração ou aumente timeout."))?,
    }
}

pub(crate) fn wire_name(server: &Server, name: &str) -> String {
    let digest = Sha256::digest(format!("{}\0{}", server.id, name).as_bytes());
    let suffix: String = digest[..8]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    let safe = |s: &str, limit: usize| {
        s.chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '_' {
                    c
                } else {
                    '_'
                }
            })
            .take(limit)
            .collect::<String>()
    };
    format!(
        "mcp_{}_{}_{}",
        safe(&server.name, 16),
        safe(name, 24),
        suffix
    )
}

// Core subprocesses carry project paths and runtime options in their environment.
// Redact credentials without erasing legitimate paths from Context-mode results.
fn redact_core(mut text: String, config: &Config) -> String {
    if let Config::Local { environment, .. } = config {
        for (name, value) in environment {
            if value.len() >= 4
                && ["KEY", "TOKEN", "SECRET", "PASSWORD", "CREDENTIAL"]
                    .iter()
                    .any(|part| name.to_ascii_uppercase().contains(part))
            {
                text = text.replace(value, "[redacted]");
            }
        }
    }
    text
}
#[test]
fn core_output_redacts_keys_but_preserves_project_paths() {
    let config = Config::Local {
        command: vec!["node".into()],
        cwd: None,
        enabled: true,
        timeout: 1000,
        environment: std::collections::BTreeMap::from([
            ("CONTEXT7_API_KEY".into(), "secret-test-key".into()),
            ("PWD".into(), "/my/project".into()),
        ]),
    };
    assert_eq!(
        redact_core(
            "Documentation at /my/project with secret-test-key".into(),
            &config
        ),
        "Documentation at /my/project with [redacted]"
    );
}
impl Client {
    pub(crate) fn core_definitions(&self) -> Vec<Value> {
        self.tools
            .iter()
            .map(|tool| {
                let mut definition = tool.definition.clone();
                definition["name"] = json!(tool.original);
                definition
            })
            .collect()
    }
    pub(crate) async fn core_call(
        &self,
        name: &str,
        args: &Value,
        mut signal: watch::Receiver<bool>,
    ) -> Result<String, McpError> {
        let tool = self
            .tools
            .iter()
            .find(|tool| tool.original == name)
            .ok_or_else(protocol_error)?;
        if !tool.validator.is_valid(args) || args.to_string().len() > 256 * 1024 {
            return Err(error("Argumentos inválidos para a ferramenta do Core."));
        }
        let request = self.service.call_tool(
            CallToolRequestParams::new(tool.original.clone())
                .with_arguments(args.as_object().cloned().ok_or_else(protocol_error)?),
        );
        let response = tokio::select! {
            _ = cancelled(&mut signal) => { self.service.cancellation_token().cancel(); return Err(error("Ferramenta do Core interrompida; confira o resultado antes de repetir a ação.")); },
            result = tokio::time::timeout(self.config.timeout(), request) => result.map_err(|_| error("A ferramenta do Core excedeu o tempo limite; confira o resultado antes de repetir a ação."))?.map_err(|_| protocol_error())?,
        };
        let value = serde_json::to_value(&response).map_err(|_| protocol_error())?;
        let text = value["content"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|item| item["text"].as_str())
            .collect::<Vec<_>>()
            .join("\n\n");
        let text: String = redact_core(text, &self.config)
            .chars()
            .take(MAX_OUTPUT)
            .collect();
        if response.is_error == Some(true) {
            return Err(error(&text));
        }
        Ok(text)
    }
    async fn refresh(&mut self) -> Result<(), McpError> {
        self.service
            .service()
            .changed
            .store(false, Ordering::Relaxed);
        let mut cursor = None;
        let mut cursors = HashSet::new();
        let mut tools: Vec<Tool> = Vec::new();
        for _ in 0..16 {
            let mut params = rmcp::model::PaginatedRequestParams::default();
            params.cursor = cursor;
            let page = self
                .service
                .list_tools(Some(params))
                .await
                .map_err(|_| protocol_error())?;
            tools.extend(page.tools);
            if tools.len() > MAX_TOOLS {
                return Err(error("O MCP excedeu o limite de 96 ferramentas."));
            }
            cursor = page.next_cursor;
            if cursor.is_none() {
                break;
            }
            if !cursors.insert(cursor.clone()) || cursors.len() == 16 {
                return Err(protocol_error());
            }
        }
        let mut registered = Vec::new();
        let mut names = HashSet::new();
        for tool in tools {
            let read_only = tool.annotations.as_ref().is_some_and(|hints| {
                hints.read_only_hint == Some(true) && hints.destructive_hint != Some(true)
            });
            if tool.name.is_empty() || tool.name.len() > 200 || !names.insert(tool.name.clone()) {
                return Err(protocol_error());
            }
            let schema = Value::Object((*tool.input_schema).clone());
            if schema.to_string().len() > 32_000 || schema["type"] != "object" {
                return Err(error(
                    "O MCP retornou um esquema de ferramenta incompatível.",
                ));
            }
            // Remote/file schema resolution is disabled in Cargo features.
            let validator = jsonschema::validator_for(&schema)
                .map_err(|_| error("O MCP retornou um esquema de argumentos inválido."))?;
            let description: String = tool
                .description
                .unwrap_or_default()
                .chars()
                .take(4000)
                .collect();
            let definition = json!({"type":"function", "name":wire_name(&self.server, &tool.name), "description":format!("MCP {} / {}. {}", self.server.name, tool.name, description), "parameters":schema});
            registered.push(RegisteredTool {
                definition,
                original: tool.name.into_owned(),
                validator,
                read_only,
            });
        }
        self.tools = registered;
        Ok(())
    }
    pub fn tool_count(&self) -> usize {
        self.tools.len()
    }
    fn tool_names(&self) -> Vec<String> {
        self.tools
            .iter()
            .map(|tool| self.redact(tool.original.clone()))
            .collect()
    }
    fn redact(&self, text: String) -> String {
        self.config
            .secrets()
            .iter()
            .fold(text, |text, secret| text.replace(secret, "[redigido]"))
    }
    pub async fn close(&mut self) {
        let _ = self
            .service
            .close_with_timeout(Duration::from_secs(2))
            .await;
    }
}

#[derive(Default)]
pub struct TurnClients {
    clients: Vec<Client>,
}
impl TurnClients {
    pub async fn discover(
        mcp: &McpState,
        state: &AppState,
        home: &Path,
        root: &Path,
        signal: watch::Receiver<bool>,
    ) -> Result<Self, McpError> {
        let (m, s, h) = (mcp.clone(), state.clone(), home.to_path_buf());
        let configs = tauri::async_runtime::spawn_blocking(move || m.active_configs(&s, &h))
            .await
            .map_err(|_| protocol_error())??;
        let mut jobs = tokio::task::JoinSet::new();
        for (server, config) in configs {
            let (root, signal) = (root.to_path_buf(), signal.clone());
            jobs.spawn(async move {
                let result = connect(server.clone(), config, &root, signal).await;
                (server, result)
            });
        }
        let mut clients = Vec::new();
        while let Some(result) = jobs.join_next().await {
            if let Ok((server, result)) = result {
                match result {
                    Ok(client) => {
                        mcp.record_check(
                            state,
                            home,
                            &server,
                            Check {
                                tool_count: client.tool_count(),
                                tools: client.tool_names(),
                                error: None,
                            },
                        );
                        if mcp.current(state, home, &server) {
                            clients.push(client);
                        }
                    }
                    Err(err) => mcp.record_check(
                        state,
                        home,
                        &server,
                        Check {
                            tool_count: 0,
                            tools: vec![],
                            error: Some(err.message),
                        },
                    ),
                }
            }
        }
        clients.sort_by(|a, b| a.server.name.cmp(&b.server.name));
        Ok(Self { clients })
    }
    pub async fn definitions(
        &mut self,
        mcp: &McpState,
        state: &AppState,
        home: &Path,
        read_only: bool,
    ) -> Vec<Value> {
        let mut definitions = Vec::new();
        for client in &mut self.clients {
            if !mcp.current(state, home, &client.server) || client.service.is_closed() {
                client.tools.clear();
                client.close().await;
                continue;
            }
            if client.service.service().changed.load(Ordering::Relaxed) {
                if matches!(
                    tokio::time::timeout(client.config.timeout(), client.refresh()).await,
                    Ok(Ok(()))
                ) {
                    mcp.record_check(
                        state,
                        home,
                        &client.server,
                        Check {
                            tool_count: client.tool_count(),
                            tools: client.tool_names(),
                            error: None,
                        },
                    );
                } else {
                    client.tools.clear();
                    mcp.record_check(
                        state,
                        home,
                        &client.server,
                        Check {
                            tool_count: 0,
                            tools: vec![],
                            error: Some("Não foi possível atualizar as ferramentas.".into()),
                        },
                    );
                }
            }
            // Keep the complete request below OpenAI's tool count limit.
            definitions.extend(
                client
                    .tools
                    .iter()
                    .filter(|tool| !read_only || tool.read_only)
                    .map(|tool| tool.definition.clone())
                    .take(MAX_TOOLS - definitions.len()),
            );
        }
        definitions
    }
    #[allow(clippy::too_many_arguments)] // Explicit execution policy and current registry are checked at dispatch.
    pub async fn execute(
        &self,
        mcp: &McpState,
        state: &AppState,
        home: &Path,
        name: &str,
        args: &Value,
        read_only: bool,
        mut signal: watch::Receiver<bool>,
    ) -> Result<String, McpError> {
        let (client, tool) = self
            .clients
            .iter()
            .find_map(|client| {
                client
                    .tools
                    .iter()
                    .find(|tool| tool.definition["name"] == name)
                    .map(|tool| (client, tool))
            })
            .ok_or_else(|| error("A ferramenta MCP não está disponível nesta interação."))?;
        if !mcp.current(state, home, &client.server) {
            return Err(error("O MCP foi desativado, editado ou removido. Envie uma nova mensagem para atualizar as ferramentas."));
        }
        if read_only && !tool.read_only {
            return Err(error(
                "Esta ferramenta MCP não está disponível no modo Plan.",
            ));
        }
        if !tool.validator.is_valid(args) || args.to_string().len() > 64 * 1024 {
            return Err(error(
                "Os argumentos não correspondem ao esquema da ferramenta MCP.",
            ));
        }
        let request = client.service.call_tool(
            CallToolRequestParams::new(tool.original.clone())
                .with_arguments(args.as_object().cloned().ok_or_else(protocol_error)?),
        );
        let result = tokio::select! {
            biased;
            _ = cancelled(&mut signal) => {
                client.service.cancellation_token().cancel();
                return Err(error("Execução MCP interrompida; a ação pode já ter sido realizada pelo servidor."));
            },
            result = tokio::time::timeout(client.config.timeout(), request) => match result {
                Ok(Ok(result)) => result,
                other => {
                    client.service.cancellation_token().cancel();
                    let failure = if other.is_err() { error("A ferramenta MCP excedeu o tempo limite. A ação pode já ter sido realizada; confira antes de repetir.") } else { protocol_error() };
                    mcp.record_check(state, home, &client.server, Check { tool_count: 0, tools: vec![], error: Some(failure.message.clone()) });
                    return Err(failure);
                }
            },
        };
        let value = serde_json::to_value(&result).map_err(|_| protocol_error())?;
        // Only textual/resource text and structured data enter the conversation.
        let texts: Vec<_> = value["content"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|item| {
                item["text"]
                    .as_str()
                    .or_else(|| item["resource"]["text"].as_str())
            })
            .collect();
        let content = client.redact(format!(
            "{}{}",
            texts.join("\n\n"),
            result
                .structured_content
                .as_ref()
                .map(|value| format!("\n{}", value))
                .unwrap_or_default()
        ));
        let mut content: String = content.chars().take(MAX_OUTPUT).collect();
        if content.len() >= MAX_OUTPUT {
            content.push_str("\n[Resultado abreviado pelo Jarvis]");
        }
        if content.is_empty() {
            content = "MCP concluído sem conteúdo textual.".into();
        }
        if result.is_error == Some(true) {
            return Err(error(&format!(
                "A ferramenta MCP retornou um erro:\n{content}"
            )));
        }
        Ok(content)
    }
}

#[tauri::command]
pub async fn test_mcp_server(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    mcp: tauri::State<'_, McpState>,
    id: String,
) -> Result<Check, McpError> {
    use tauri::Manager;
    let home = app.path().home_dir().map_err(|_| protocol_error())?;
    let (state, mcp) = (state.inner().clone(), mcp.inner().clone());
    let (s, m, h) = (state.clone(), mcp.clone(), home.clone());
    let (server, config) = tauri::async_runtime::spawn_blocking(move || {
        let _guard = m.0.guard.lock().map_err(|_| protocol_error())?;
        let server = s.with_connection(&h, |connection| super::find(connection, &id))?;
        let config = m.config(&server)?;
        Ok::<_, McpError>((server, config))
    })
    .await
    .map_err(|_| protocol_error())??;
    let (_sender, signal) = watch::channel(false);
    let check = match connect(server.clone(), config, &home, signal).await {
        Ok(mut client) => {
            let count = client.tool_count();
            let tools = client.tool_names();
            client.close().await;
            Check {
                tool_count: count,
                tools,
                error: None,
            }
        }
        Err(err) => Check {
            tool_count: 0,
            tools: vec![],
            error: Some(err.message),
        },
    };
    mcp.record_check(&state, &home, &server, check.clone());
    Ok(check)
}
