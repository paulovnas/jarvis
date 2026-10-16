//! Bounded MSP over an owned official `muse serve` process.
// Wire contract: meta-models/muse-code-sdk@bb863c9071d42d9f67fcc3561690425646f727c8.
use process_wrap::tokio::{ChildWrapper, CommandWrap, KillOnDrop};
use serde_json::{json, Value};
use std::{
    collections::{HashMap, HashSet},
    ffi::OsString,
    path::PathBuf,
    process::Stdio,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex as StateMutex, MutexGuard,
    },
    time::Duration,
};
use tokio::{
    io::{AsyncBufRead, AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::{ChildStdin, Command},
    sync::{mpsc, oneshot, watch, Mutex},
    task::JoinHandle,
    time::{timeout, Instant},
};

const MAX_FRAME_BYTES: usize = 10_485_760;
const MAX_PENDING: usize = 64;
const WRITE_TIMEOUT: Duration = Duration::from_secs(10);
const STOP_TIMEOUT: Duration = Duration::from_secs(3);
const CLOSED: &str = "A conexão MSP do Muse foi encerrada.";
type Pending = HashMap<String, oneshot::Sender<Result<Value, String>>>;

pub(crate) struct MuseSpawnConfig {
    pub executable: PathBuf,
    pub cwd: PathBuf,
    pub args: Vec<OsString>,
    pub env: Vec<(OsString, OsString)>,
    pub request_timeout: Duration,
}

impl MuseSpawnConfig {
    pub(crate) fn new(executable: PathBuf, cwd: PathBuf) -> Self {
        Self {
            executable,
            cwd,
            args: vec!["serve".into()],
            env: Vec::new(),
            request_timeout: Duration::from_secs(60),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum Phase {
    #[default]
    New,
    Initializing,
    Finishing,
    Ready,
    Closed,
}

#[derive(Default)]
struct State {
    phase: Phase,
    error: Option<String>,
    pending: Pending,
    incoming: HashSet<String>,
}

#[derive(Clone)]
pub(crate) struct MuseControl {
    stdin: Arc<Mutex<Option<ChildStdin>>>,
    state: Arc<StateMutex<State>>,
    counter: Arc<AtomicU64>,
    shutdown: watch::Sender<bool>,
    request_timeout: Duration,
}

pub(crate) struct MuseProcess {
    control: MuseControl,
    events: mpsc::Receiver<Value>,
    reader: JoinHandle<()>,
    supervisor: Option<JoinHandle<Result<(), String>>>,
}

// Keep process-group cleanup synchronous even when the runtime drops its tasks.
struct OwnedChild(Box<dyn ChildWrapper>);

impl Drop for OwnedChild {
    fn drop(&mut self) {
        let _ = self.0.start_kill();
    }
}

struct PendingRequest {
    control: MuseControl,
    key: String,
}

impl Drop for PendingRequest {
    fn drop(&mut self) {
        self.control.lock().pending.remove(&self.key);
    }
}

// Cancelling a partial write cannot leave the next frame appended to its prefix.
struct FrameWrite {
    control: MuseControl,
    complete: bool,
}

impl Drop for FrameWrite {
    fn drop(&mut self) {
        if !self.complete {
            self.control.finish(Some(
                "O envio MSP do Muse foi interrompido; confirme o estado antes de continuar."
                    .into(),
            ));
        }
    }
}

fn command_for(config: &MuseSpawnConfig) -> Result<Command, String> {
    if !config.cwd.is_absolute() || !config.cwd.is_dir() || config.request_timeout.is_zero() {
        return Err("Pasta ou prazo de execução do Muse inválido.".into());
    }
    let mut command = crate::background::tokio_command(&config.executable);
    command
        .current_dir(&config.cwd)
        .args(&config.args)
        .envs(config.env.iter().cloned())
        // The subscription host must not silently select the direct API-key lane.
        .env_remove("META_API_KEY")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    Ok(command)
}

fn encode_frame(message: Value) -> Result<Vec<u8>, String> {
    let mut bytes = serde_json::to_vec(&message).map_err(|_| "Mensagem MSP inválida.")?;
    if bytes.len() > MAX_FRAME_BYTES {
        return Err("Mensagem MSP excede o limite do transporte.".into());
    }
    bytes.push(b'\n');
    Ok(bytes)
}

impl MuseProcess {
    pub(crate) fn spawn(config: MuseSpawnConfig) -> Result<Self, String> {
        let mut wrapped = CommandWrap::from(command_for(&config)?);
        #[cfg(unix)]
        wrapped.wrap(process_wrap::tokio::ProcessGroup::leader());
        #[cfg(windows)]
        crate::background::windows_job(&mut wrapped);
        wrapped.wrap(KillOnDrop);
        let mut child = OwnedChild(
            wrapped
                .spawn()
                .map_err(|error| format!("Não foi possível iniciar o host Muse: {error}"))?,
        );
        let stdin = child
            .0
            .stdin()
            .take()
            .ok_or("Entrada do Muse indisponível.")?;
        let stdout = child
            .0
            .stdout()
            .take()
            .ok_or("Saída do Muse indisponível.")?;
        let (shutdown, mut stopped) = watch::channel(false);
        let control = MuseControl {
            stdin: Arc::new(Mutex::new(Some(stdin))),
            state: Arc::new(StateMutex::new(State::default())),
            counter: Arc::new(AtomicU64::new(1)),
            shutdown,
            request_timeout: config.request_timeout,
        };
        let (send, events) = mpsc::channel(MAX_PENDING);
        let reader_control = control.clone();
        let reader = tokio::spawn(async move {
            let error = read_events(&mut BufReader::new(stdout), &reader_control, &send)
                .await
                .err();
            reader_control.finish(error);
        });
        let supervisor_control = control.clone();
        let supervisor = tokio::spawn(async move {
            let result = tokio::select! {
                biased;
                _ = stopped.changed() => {
                    let _ = child.0.start_kill();
                    timeout(STOP_TIMEOUT, child.0.wait()).await
                        .map_err(|_| "O host Muse não confirmou o encerramento.".to_string())?
                        .map(|_| ())
                        .map_err(|_| "Não foi possível aguardar o encerramento do Muse.".to_string())
                }
                result = child.0.wait() => result.map(|_| ())
                    .map_err(|_| "Não foi possível observar o encerramento do Muse.".to_string()),
            };
            if let Err(error) = &result {
                supervisor_control.finish(Some(error.clone()));
            }
            // Let stdout drain before its reader settles the remaining requests.
            supervisor_control.stdin.lock().await.take();
            result
        });
        Ok(Self {
            control,
            events,
            reader,
            supervisor: Some(supervisor),
        })
    }

    pub(crate) fn control(&self) -> MuseControl {
        self.control.clone()
    }

    pub(crate) async fn initialize(&self, params: &Value) -> Result<Value, String> {
        let mut params = params
            .as_object()
            .cloned()
            .ok_or("Parâmetros MSP inválidos.")?;
        {
            let mut state = self.control.lock();
            if state.phase != Phase::New {
                return Err("O host Muse só pode ser inicializado uma vez por conexão.".into());
            }
            state.phase = Phase::Initializing;
        }
        params.insert(
            "clientInfo".into(),
            json!({"name":"jarvis", "title":"Jarvis", "version":env!("CARGO_PKG_VERSION")}),
        );
        let result = self
            .control
            .request_inner("initialize", &Value::Object(params), true)
            .await;
        let handshake = match result.and_then(validate_handshake) {
            Ok(handshake) => handshake,
            Err(error) => {
                self.control.finish(Some(error.clone()));
                return Err(error);
            }
        };
        {
            let mut state = self.control.lock();
            if state.phase == Phase::Closed {
                return Err(state.error.clone().unwrap_or_else(|| CLOSED.into()));
            }
            state.phase = Phase::Finishing;
        }
        self.control
            .write(json!({"jsonrpc":"2.0", "method":"initialized"}))
            .await?;
        let mut state = self.control.lock();
        if state.phase == Phase::Closed {
            return Err(state.error.clone().unwrap_or_else(|| CLOSED.into()));
        }
        state.phase = Phase::Ready;
        Ok(handshake)
    }

    pub(crate) async fn next_event(&mut self) -> Result<Option<Value>, String> {
        match self.events.recv().await {
            Some(event) => Ok(Some(event)),
            None => match self.control.lock().error.clone() {
                Some(error) => Err(error),
                None => Ok(None),
            },
        }
    }

    pub(crate) async fn cancel(&mut self) -> Result<(), String> {
        self.control.cancel().await?;
        if let Some(mut supervisor) = self.supervisor.take() {
            let result = timeout(STOP_TIMEOUT + Duration::from_secs(1), &mut supervisor).await;
            self.reader.abort();
            match result {
                Ok(result) => {
                    result.map_err(|_| "A supervisão do Muse foi interrompida.".to_string())?
                }
                Err(_) => {
                    supervisor.abort();
                    Err("O host Muse não confirmou o encerramento.".into())
                }
            }
        } else {
            Ok(())
        }
    }
}

impl Drop for MuseProcess {
    fn drop(&mut self) {
        self.control.finish(None);
        self.reader.abort();
        // The supervisor retains the owned child until it has killed and reaped it.
    }
}

impl MuseControl {
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|error| error.into_inner())
    }

    fn finish(&self, error: Option<String>) {
        let mut state = self.lock();
        if state.phase != Phase::Closed {
            state.phase = Phase::Closed;
            state.error = error;
            state.incoming.clear();
            let error = state.error.clone().unwrap_or_else(|| CLOSED.into());
            for (_, pending) in state.pending.drain() {
                let _ = pending.send(Err(error.clone()));
            }
        }
        let _ = self.shutdown.send(true);
    }

    fn ready(&self) -> Result<(), String> {
        let state = self.lock();
        if state.phase != Phase::Ready {
            return Err(state
                .error
                .clone()
                .unwrap_or_else(|| "A conexão MSP do Muse ainda não está disponível.".into()));
        }
        Ok(())
    }

    async fn write(&self, message: Value) -> Result<(), String> {
        self.write_frame(encode_frame(message)?).await
    }

    async fn write_frame(&self, bytes: Vec<u8>) -> Result<(), String> {
        let result = timeout(self.request_timeout.min(WRITE_TIMEOUT), async {
            let mut stdin = self.stdin.lock().await;
            if self.lock().phase == Phase::Closed {
                return Err(CLOSED.to_string());
            }
            let stdin = stdin.as_mut().ok_or(CLOSED)?;
            let mut frame = FrameWrite {
                control: self.clone(),
                complete: false,
            };
            stdin
                .write_all(&bytes)
                .await
                .map_err(|_| "Não foi possível enviar a mensagem MSP.".to_string())?;
            stdin
                .flush()
                .await
                .map_err(|_| "Não foi possível concluir o envio MSP.".to_string())?;
            frame.complete = true;
            Ok(())
        })
        .await
        .unwrap_or_else(|_| {
            Err(
                "O Muse não recebeu a mensagem no prazo; confirme o estado antes de continuar."
                    .into(),
            )
        });
        if let Err(error) = &result {
            self.finish(Some(error.clone()));
        }
        result
    }

    pub(crate) async fn request(&self, method: &str, params: &Value) -> Result<Value, String> {
        if matches!(method, "initialize" | "initialized") {
            return Err("Use a negociação inicial da conexão Muse.".into());
        }
        self.request_inner(method, params, false).await
    }

    async fn request_inner(
        &self,
        method: &str,
        params: &Value,
        handshake: bool,
    ) -> Result<Value, String> {
        validate_call(method, params)?;
        let started = Instant::now();
        let id = self
            .counter
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
            .map_err(|_| "O espaço de identificadores MSP foi esgotado.".to_string())?;
        let key = format!("number:{id}");
        let (send, receive) = oneshot::channel();
        {
            let mut state = self.lock();
            let expected = if handshake {
                Phase::Initializing
            } else {
                Phase::Ready
            };
            if state.phase != expected {
                return Err(state
                    .error
                    .clone()
                    .unwrap_or_else(|| "A conexão MSP do Muse ainda não está disponível.".into()));
            }
            if state.pending.len() >= MAX_PENDING {
                return Err("O Muse atingiu o limite de solicitações MSP pendentes.".into());
            }
            state.pending.insert(key.clone(), send);
        }
        let _pending = PendingRequest {
            control: self.clone(),
            key,
        };
        let mut message = json!({"jsonrpc":"2.0", "id":id, "method":method});
        if !params.as_object().is_some_and(serde_json::Map::is_empty) {
            message["params"] = params.clone();
        }
        self.write(message).await?;
        timeout(self.request_timeout.saturating_sub(started.elapsed()), receive).await
            .map_err(|_| "O Muse não confirmou a solicitação no prazo; confirme o estado antes de tentar novamente.".to_string())?
            .map_err(|_| CLOSED.to_string())?
    }

    #[cfg(test)]
    pub(crate) async fn notify(&self, method: &str, params: &Value) -> Result<(), String> {
        self.ready()?;
        validate_call(method, params)?;
        if matches!(method, "initialize" | "initialized") {
            return Err("Use a negociação inicial da conexão Muse.".into());
        }
        let mut message = json!({"jsonrpc":"2.0", "method":method});
        if !params.as_object().is_some_and(serde_json::Map::is_empty) {
            message["params"] = params.clone();
        }
        self.write(message).await
    }

    #[cfg(test)]
    pub(crate) async fn respond(&self, id: &Value, result: &Value) -> Result<(), String> {
        if !result.is_object() {
            return Err("O resultado MSP precisa ser um objeto.".into());
        }
        self.reply(id, json!({"jsonrpc":"2.0", "id":id, "result":result}))
            .await
    }

    pub(crate) async fn reject(&self, id: &Value, code: i64, message: &str) -> Result<(), String> {
        self.reply(
            id,
            json!({"jsonrpc":"2.0", "id":id, "error":{"code":code, "message":message}}),
        )
        .await
    }

    async fn reply(&self, id: &Value, message: Value) -> Result<(), String> {
        self.ready()?;
        let key = id_key(id)?;
        let bytes = encode_frame(message)?;
        if !self.lock().incoming.remove(&key) {
            return Err("Solicitação MSP desconhecida ou já respondida.".into());
        }
        self.write_frame(bytes).await
    }

    pub(crate) async fn cancel(&self) -> Result<(), String> {
        // Signal the owned supervisor before touching stdin: a blocked write holds its lock.
        self.finish(None);
        Ok(())
    }
}

fn validate_call(method: &str, params: &Value) -> Result<(), String> {
    if method.is_empty()
        || method.len() > 256
        || !method
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'_' | b'.' | b'-'))
        || !params.is_object()
    {
        return Err("Método ou parâmetros MSP inválidos.".into());
    }
    Ok(())
}

fn id_key(id: &Value) -> Result<String, String> {
    match id {
        Value::String(id) => Ok(format!("string:{id}")),
        Value::Number(id) if id.is_i64() || id.is_u64() => Ok(format!("number:{id}")),
        _ => Err("Identificador de solicitação MSP inválido.".into()),
    }
}

fn validate_handshake(result: Value) -> Result<Value, String> {
    let fingerprint = result["schema"]["fingerprint"].as_str().unwrap_or_default();
    if result["schema"]["version"].as_u64() != Some(1)
        || !fingerprint.strip_prefix("sha256:").is_some_and(|hash| {
            hash.len() == 64
                && hash
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
        })
        || !result["experimentalApi"].is_boolean()
        || !result["grantedCapabilities"]
            .as_array()
            .is_some_and(|items| items.iter().all(Value::is_string))
        || !result["serverInfo"]["name"].is_string()
        || !result["serverInfo"]["version"].is_string()
    {
        return Err("O host Muse retornou uma negociação MSP incompatível.".into());
    }
    // A different stable-surface fingerprint is diagnostic; additive hosts remain compatible.
    Ok(result)
}

async fn read_events<R: AsyncBufRead + Unpin>(
    reader: &mut R,
    control: &MuseControl,
    events: &mpsc::Sender<Value>,
) -> Result<(), String> {
    while let Some(line) = read_frame(reader).await? {
        if line.iter().all(u8::is_ascii_whitespace) {
            continue;
        }
        let frame: Value = serde_json::from_slice(&line)
            .map_err(|_| "O Muse retornou JSON inválido no canal MSP.".to_string())?;
        if !frame.is_object() || frame["jsonrpc"] != "2.0" {
            return Err("O Muse retornou um envelope JSON-RPC inválido.".into());
        }
        if let Some(method) = frame.get("method") {
            let method = method.as_str().ok_or("Método MSP inválido.")?;
            validate_call(method, frame.get("params").unwrap_or(&json!({})))?;
            if frame.get("result").is_some() || frame.get("error").is_some() {
                return Err("O Muse misturou solicitação e resposta MSP.".into());
            }
            {
                let mut state = control.lock();
                if !matches!(state.phase, Phase::Finishing | Phase::Ready) {
                    return Err(
                        "O Muse enviou um evento antes de concluir a negociação MSP.".into(),
                    );
                }
                if let Some(id) = frame.get("id") {
                    if state.incoming.len() >= MAX_PENDING || !state.incoming.insert(id_key(id)?) {
                        return Err("O Muse repetiu ou excedeu solicitações MSP pendentes.".into());
                    }
                }
            }
            events
                .try_send(frame)
                .map_err(|_| "A fila de eventos MSP do Muse excedeu sua capacidade.".to_string())?;
        } else {
            let key = id_key(frame.get("id").ok_or("Resposta MSP sem identificador.")?)?;
            let response = match (frame.get("result"), frame.get("error")) {
                (Some(result), None) if result.is_object() => Ok(result.clone()),
                (None, Some(error))
                    if error.is_object()
                        && error["code"].as_i64().is_some()
                        && error["message"].is_string()
                        && error
                            .get("data")
                            .is_none_or(|data| data.is_object() && data["kind"].is_string()) =>
                {
                    Err(format!(
                        "O Muse recusou a solicitação MSP (código {}).",
                        error["code"]
                    ))
                }
                _ => return Err("O Muse retornou uma resposta MSP inválida.".into()),
            };
            if let Some(pending) = control.lock().pending.remove(&key) {
                let _ = pending.send(response);
            }
            // Late responses to timed-out/cancelled callers are never matched to a new request.
        }
    }
    Err("O host Muse encerrou o canal MSP.".into())
}

async fn read_frame<R: AsyncBufRead + Unpin>(reader: &mut R) -> Result<Option<Vec<u8>>, String> {
    let mut line = Vec::new();
    loop {
        let bytes = reader
            .fill_buf()
            .await
            .map_err(|_| "Falha ao ler o canal MSP do Muse.".to_string())?;
        if bytes.is_empty() {
            return if line.is_empty() {
                Ok(None)
            } else {
                Err("O Muse encerrou um frame MSP sem a quebra de linha.".into())
            };
        }
        let newline = bytes.iter().position(|byte| *byte == b'\n');
        let count = newline.unwrap_or(bytes.len());
        if line.len() + count > MAX_FRAME_BYTES + 1 {
            return Err("O frame MSP do Muse excedeu o limite do transporte.".into());
        }
        line.extend_from_slice(&bytes[..count]);
        reader.consume(count + usize::from(newline.is_some()));
        if newline.is_some() {
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            if line.len() > MAX_FRAME_BYTES {
                return Err("O frame MSP do Muse excedeu o limite do transporte.".into());
            }
            return Ok(Some(line));
        }
        if line.len() > MAX_FRAME_BYTES && line.last() != Some(&b'\r') {
            return Err("O frame MSP do Muse excedeu o limite do transporte.".into());
        }
    }
}

#[cfg(test)]
#[path = "transport/tests.rs"]
mod tests;
