use process_wrap::tokio::{ChildWrapper, CommandWrap, KillOnDrop};
use serde_json::{json, Value};
use std::{
    collections::{HashMap, HashSet},
    io::Write,
    path::{Path, PathBuf},
    process::Stdio,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::Duration,
};
use tempfile::NamedTempFile;
use tokio::{
    io::{AsyncBufRead, AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    process::{ChildStdin, Command},
    sync::{mpsc, oneshot, Mutex},
    task::JoinHandle,
};

const MAX_FRAME_BYTES: usize = 8 * 1024 * 1024;
const CONTROL_TIMEOUT: Duration = Duration::from_secs(60);
const WRITE_TIMEOUT: Duration = Duration::from_secs(10);
type PendingControls = HashMap<String, oneshot::Sender<Result<Value, String>>>;

pub(crate) struct RunOptions {
    pub cwd: PathBuf,
    pub session_id: String,
    pub resume: bool,
    pub model: String,
    pub effort: Option<String>,
    pub append_system_prompt: String,
    /// Only Jarvis-managed SDK MCP servers are configured for this run.
    pub mcp_servers: Value,
}

#[derive(Clone)]
pub(crate) struct ClaudeControl {
    stdin: Arc<Mutex<Option<ChildStdin>>>,
    pending: Arc<Mutex<PendingControls>>,
    incoming: Arc<Mutex<HashSet<String>>>,
    counter: Arc<AtomicU64>,
}

pub(crate) struct ClaudeProcess {
    control: ClaudeControl,
    events: mpsc::Receiver<Result<Value, String>>,
    child: Option<Box<dyn ChildWrapper>>,
    reader: JoinHandle<()>,
    stderr_reader: JoinHandle<()>,
    _files: Vec<NamedTempFile>,
}

fn temporary_file(content: &[u8]) -> Result<NamedTempFile, String> {
    let mut file = NamedTempFile::new().map_err(|error| error.to_string())?;
    file.write_all(content).map_err(|error| error.to_string())?;
    file.flush().map_err(|error| error.to_string())?;
    Ok(file)
}

pub(super) fn valid_session_id(id: &str) -> bool {
    id.len() == 36
        && id.bytes().enumerate().all(|(index, byte)| {
            if matches!(index, 8 | 13 | 18 | 23) {
                byte == b'-'
            } else {
                byte.is_ascii_hexdigit()
            }
        })
}

pub(super) fn command_for(
    executable: &Path,
    options: &RunOptions,
    metadata: bool,
) -> Result<(Command, Vec<NamedTempFile>), String> {
    super::validate_selection(&options.model, options.effort.as_deref())?;
    if !metadata && !valid_session_id(&options.session_id) {
        return Err("Identificador da sessão Claude inválido.".into());
    }
    if !options.cwd.is_dir() || !options.cwd.is_absolute() {
        return Err("A pasta de execução do Claude precisa existir e ser absoluta.".into());
    }
    if !options.mcp_servers.is_object() {
        return Err("Configuração de ferramentas Claude inválida.".into());
    }
    let mut command = crate::background::tokio_command(executable);
    command.current_dir(&options.cwd).args([
        "-p",
        "--input-format",
        "stream-json",
        "--output-format",
        "stream-json",
        "--verbose",
        "--include-partial-messages",
        "--permission-prompt-tool",
        "stdio",
        "--permission-mode",
        "default",
        "--tools",
        "",
        "--strict-mcp-config",
        "--settings",
        "{\"disableAllHooks\":true}",
    ]);
    // Keep upstream auth/provider variables intact. Only remove nested-client detection.
    command
        .env_remove("CLAUDECODE")
        .env("CLAUDE_CODE_SDK_READS_SESSION_STATE", "1");
    let mut files = Vec::new();
    if !options.append_system_prompt.is_empty() {
        let prompt = temporary_file(options.append_system_prompt.as_bytes())?;
        command
            .arg("--append-system-prompt-file")
            .arg(prompt.path());
        files.push(prompt);
    }
    let mcp = temporary_file(
        json!({"mcpServers":options.mcp_servers})
            .to_string()
            .as_bytes(),
    )?;
    command.arg("--mcp-config").arg(mcp.path());
    files.push(mcp);
    if metadata {
        // Model discovery sends initialize only: no prompt, tools, hooks or persisted turn.
        command.arg("--no-session-persistence");
    } else if options.resume {
        command.arg(format!("--resume={}", options.session_id));
    } else {
        command.arg(format!("--session-id={}", options.session_id));
    }
    if options.model != "default" {
        command.arg(format!("--model={}", options.model));
    }
    if let Some(effort) = &options.effort {
        command.arg(format!("--effort={effort}"));
    }
    Ok((command, files))
}

impl ClaudeProcess {
    pub(crate) fn spawn(options: RunOptions) -> Result<Self, String> {
        let executable = super::metadata::executable().ok_or_else(|| {
            "Claude Code não encontrado. Instale o CLI oficial e faça login pelo terminal."
                .to_string()
        })?;
        let (command, files) = command_for(&executable, &options, false)?;
        Self::spawn_command(command, files)
    }

    pub(super) fn spawn_command(
        mut command: Command,
        files: Vec<NamedTempFile>,
    ) -> Result<Self, String> {
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut wrapped = CommandWrap::from(command);
        #[cfg(unix)]
        wrapped.wrap(process_wrap::tokio::ProcessGroup::leader());
        #[cfg(windows)]
        crate::background::windows_job(&mut wrapped);
        wrapped.wrap(KillOnDrop);
        let mut child = wrapped
            .spawn()
            .map_err(|error| format!("Não foi possível iniciar o Claude Code: {error}"))?;
        let stdin = child
            .stdin()
            .take()
            .ok_or("Entrada do Claude indisponível.")?;
        let stdout = child
            .stdout()
            .take()
            .ok_or("Saída do Claude indisponível.")?;
        let stderr_pipe = child
            .stderr()
            .take()
            .ok_or("Diagnóstico do Claude indisponível.")?;
        let control = ClaudeControl {
            stdin: Arc::new(Mutex::new(Some(stdin))),
            pending: Arc::new(Mutex::new(HashMap::new())),
            incoming: Arc::new(Mutex::new(HashSet::new())),
            counter: Arc::new(AtomicU64::new(1)),
        };
        let (send, events) = mpsc::channel(64);
        let reader_control = control.clone();
        let reader = tokio::spawn(async move {
            let mut reader = BufReader::new(stdout);
            let result = read_events(&mut reader, &reader_control, &send).await;
            let failed = result.is_err();
            let error = result
                .err()
                .unwrap_or_else(|| "O processo Claude encerrou o canal de controle.".into());
            for (_, pending) in reader_control.pending.lock().await.drain() {
                let _ = pending.send(Err(error.clone()));
            }
            reader_control.incoming.lock().await.clear();
            if failed {
                let _ = send.send(Err(error)).await;
            }
        });
        let stderr_reader = tokio::spawn(async move {
            // Drain without retaining raw diagnostics: a native CLI may print
            // credentials or private provider responses before failing.
            let mut pipe = stderr_pipe;
            let mut chunk = [0_u8; 4096];
            while let Ok(count) = pipe.read(&mut chunk).await {
                if count == 0 {
                    break;
                }
            }
        });
        Ok(Self {
            control,
            events,
            child: Some(child),
            reader,
            stderr_reader,
            _files: files,
        })
    }

    pub(crate) fn control(&self) -> ClaudeControl {
        self.control.clone()
    }

    pub(crate) async fn next_event(&mut self) -> Result<Option<Value>, String> {
        self.events.recv().await.transpose()
    }

    #[cfg(test)]
    pub(crate) async fn wait(&mut self) -> Result<std::process::ExitStatus, String> {
        self.child
            .as_mut()
            .ok_or("Processo Claude já encerrado.")?
            .wait()
            .await
            .map_err(|error| error.to_string())
    }

    pub(crate) async fn cancel(&mut self) -> Result<(), String> {
        // Kill first: a large write may hold the stdin lock while the CLI is not reading.
        if let Some(child) = self.child.as_mut() {
            child.start_kill().map_err(|error| error.to_string())?;
        }
        self.control.close().await;
        self.reader.abort();
        self.stderr_reader.abort();
        if let Some(mut child) = self.child.take() {
            tokio::time::timeout(Duration::from_secs(3), child.wait())
                .await
                .map_err(|_| "O processo Claude não confirmou o encerramento.".to_string())?
                .map_err(|error| error.to_string())?;
        }
        Ok(())
    }
}

impl Drop for ClaudeProcess {
    fn drop(&mut self) {
        self.reader.abort();
        self.stderr_reader.abort();
        if let Some(mut child) = self.child.take() {
            let _ = child.start_kill();
            if let Ok(runtime) = tokio::runtime::Handle::try_current() {
                let control = self.control.clone();
                runtime.spawn(async move {
                    control.close().await;
                    let _ = tokio::time::timeout(Duration::from_secs(3), child.wait()).await;
                });
            }
        }
    }
}

impl ClaudeControl {
    async fn write(&self, message: Value) -> Result<(), String> {
        self.write_with_timeout(message, WRITE_TIMEOUT).await
    }

    pub(super) async fn write_with_timeout(
        &self,
        message: Value,
        timeout: Duration,
    ) -> Result<(), String> {
        let mut encoded = serde_json::to_vec(&message).map_err(|error| error.to_string())?;
        if encoded.len() > MAX_FRAME_BYTES {
            return Err("Mensagem para Claude excede o limite do transporte.".into());
        }
        encoded.push(b'\n');
        tokio::time::timeout(timeout, async {
            // Include contention in the deadline: a blocked large frame must
            // not leave later approval/control writes waiting indefinitely.
            let mut guard = self.stdin.lock().await;
            let stdin = guard.as_mut().ok_or("A sessão Claude foi encerrada.")?;
            stdin.write_all(&encoded).await.map_err(|error| error.to_string())?;
            stdin.flush().await.map_err(|error| error.to_string())
        }).await.map_err(|_| "Claude não recebeu a mensagem no prazo; a sessão precisa ser retomada antes de continuar.".to_string())?
    }

    pub(crate) async fn initialize(&self, hooks: Value) -> Result<Value, String> {
        self.request(json!({"subtype":"initialize","hooks":hooks}))
            .await
    }

    pub(crate) async fn request(&self, request: Value) -> Result<Value, String> {
        let id = format!("jarvis_{}", self.counter.fetch_add(1, Ordering::Relaxed));
        let (send, receive) = oneshot::channel();
        self.pending.lock().await.insert(id.clone(), send);
        if let Err(error) = self
            .write(json!({"type":"control_request","request_id":id,"request":request}))
            .await
        {
            self.pending.lock().await.remove(&id);
            return Err(error);
        }
        let result = tokio::time::timeout(CONTROL_TIMEOUT, receive).await;
        self.pending.lock().await.remove(&id);
        result
            .map_err(|_| "Claude não respondeu à solicitação de controle.".to_string())?
            .map_err(|_| "O controle da sessão Claude foi encerrado.".to_string())?
    }

    pub(crate) async fn send_user(
        &self,
        content: Value,
        uuid: Option<String>,
    ) -> Result<(), String> {
        if !(content.is_string() || content.is_array()) {
            return Err("Conteúdo da mensagem Claude inválido.".into());
        }
        let mut message = json!({"type":"user","message":{"role":"user","content":content},"parent_tool_use_id":null});
        if let Some(uuid) = uuid {
            message["uuid"] = json!(uuid);
        }
        self.write(message).await
    }

    pub(crate) async fn respond_control(
        &self,
        request_id: &str,
        response: Result<Value, String>,
    ) -> Result<(), String> {
        let mut incoming = self.incoming.lock().await;
        if !incoming.remove(request_id) {
            return Err("Solicitação Claude desconhecida, respondida ou cancelada.".into());
        }
        drop(incoming);
        let response = match response {
            Ok(value) => json!({"subtype":"success","request_id":request_id,"response":value}),
            Err(error) => json!({"subtype":"error","request_id":request_id,"error":error}),
        };
        self.write(json!({"type":"control_response","response":response}))
            .await
    }

    pub(crate) async fn is_pending(&self, request_id: &str) -> bool {
        self.incoming.lock().await.contains(request_id)
    }

    pub(crate) async fn close(&self) {
        self.stdin.lock().await.take();
        self.incoming.lock().await.clear();
        for (_, pending) in self.pending.lock().await.drain() {
            let _ = pending.send(Err("A sessão Claude foi encerrada.".into()));
        }
    }
}

async fn read_events<R: AsyncBufRead + Unpin>(
    reader: &mut R,
    control: &ClaudeControl,
    events: &mpsc::Sender<Result<Value, String>>,
) -> Result<(), String> {
    while let Some(line) = read_frame(reader).await? {
        if line.iter().all(u8::is_ascii_whitespace) {
            continue;
        }
        let message: Value = serde_json::from_slice(&line)
            .map_err(|_| "Claude retornou uma mensagem estruturada inválida.".to_string())?;
        let kind = message["type"]
            .as_str()
            .ok_or("Claude retornou um evento sem tipo.")?;
        match kind {
            "control_response" => {
                let response = &message["response"];
                let id = response["request_id"]
                    .as_str()
                    .ok_or("Resposta de controle Claude sem identificador.")?;
                if let Some(pending) = control.pending.lock().await.remove(id) {
                    let result = if response["subtype"] == "success" {
                        Ok(response["response"].clone())
                    } else {
                        Err(response["error"]
                            .as_str()
                            .unwrap_or("Claude recusou a solicitação de controle.")
                            .to_owned())
                    };
                    let _ = pending.send(result);
                }
                continue;
            }
            "control_request" => {
                let id = message["request_id"]
                    .as_str()
                    .filter(|id| !id.is_empty())
                    .ok_or("Solicitação de controle Claude sem identificador.")?;
                if !message["request"].is_object() {
                    return Err("Solicitação de controle Claude inválida.".into());
                }
                if !control.incoming.lock().await.insert(id.to_owned()) {
                    return Err(
                        "Claude repetiu um identificador de controle ainda pendente.".into(),
                    );
                }
            }
            "control_cancel_request" => {
                if let Some(id) = message["request_id"].as_str() {
                    control.incoming.lock().await.remove(id);
                }
            }
            _ => {}
        }
        if events.send(Ok(message)).await.is_err() {
            break;
        }
    }
    Ok(())
}

pub(super) async fn read_frame<R: AsyncBufRead + Unpin>(
    reader: &mut R,
) -> Result<Option<Vec<u8>>, String> {
    let mut line = Vec::new();
    loop {
        let bytes = reader.fill_buf().await.map_err(|error| error.to_string())?;
        if bytes.is_empty() {
            return Ok((!line.is_empty()).then_some(line));
        }
        let newline = bytes.iter().position(|byte| *byte == b'\n');
        let count = newline.map_or(bytes.len(), |index| index + 1);
        if line.len() + count > MAX_FRAME_BYTES {
            return Err("Evento Claude excede o limite do transporte.".into());
        }
        line.extend_from_slice(&bytes[..count]);
        reader.consume(count);
        if newline.is_some() {
            return Ok(Some(line));
        }
    }
}
