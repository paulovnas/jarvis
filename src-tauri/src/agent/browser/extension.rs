//! Authenticated, local-only transport for the externally installed Chromium extension.
use super::AgentError;
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};
use tauri::{Emitter, EventTarget, Manager};
use tauri_plugin_opener::OpenerExt;
use tokio::{
    net::{TcpListener, TcpStream},
    sync::{mpsc, oneshot, Semaphore},
    time::timeout,
};
use tokio_tungstenite::{
    accept_hdr_async_with_config,
    tungstenite::{
        handshake::server::{ErrorResponse, Request, Response},
        protocol::WebSocketConfig,
        Message,
    },
};

const DEFAULT_PORT: u16 = 17373;
const MAX_MESSAGE: usize = 32 * 1024 * 1024;
const MAX_PENDING: usize = 32;
const RESPONSE_TIMEOUT: Duration = Duration::from_secs(60);

fn failure(code: &str, message: &str) -> AgentError {
    AgentError::new(code, message)
}
fn storage_error() -> AgentError {
    failure(
        "browser_extension_storage",
        "Não foi possível salvar a conexão com a extensão.",
    )
}
fn secret() -> Result<String, AgentError> {
    let mut bytes = [0_u8; 32];
    getrandom::fill(&mut bytes).map_err(|_| AgentError::internal())?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Pairing {
    token: String,
    port: u16,
    origin: Option<String>,
    instance_id: Option<String>,
}
impl Pairing {
    fn fresh(port: u16) -> Result<Self, AgentError> {
        Ok(Self {
            token: secret()?,
            port,
            origin: None,
            instance_id: None,
        })
    }
    fn endpoint(&self) -> String {
        format!("ws://127.0.0.1:{}/extension", self.port)
    }
    fn save(&self, path: &Path) -> Result<(), AgentError> {
        let parent = path.parent().ok_or_else(storage_error)?;
        fs::create_dir_all(parent).map_err(|_| storage_error())?;
        let mut file = tempfile::NamedTempFile::new_in(parent).map_err(|_| storage_error())?;
        serde_json::to_writer(&mut file, self).map_err(|_| storage_error())?;
        file.flush()
            .and_then(|()| file.as_file().sync_all())
            .map_err(|_| storage_error())?;
        file.persist(path).map_err(|_| storage_error())?;
        Ok(())
    }
    fn load(path: &Path) -> Result<Self, AgentError> {
        match fs::symlink_metadata(path) {
            Ok(metadata) if metadata.is_file() && metadata.len() <= 4096 => {
                let value: Self =
                    serde_json::from_slice(&fs::read(path).map_err(|_| storage_error())?)
                        .map_err(|_| storage_error())?;
                if value.token.len() != 64
                    || !value.token.bytes().all(|byte| byte.is_ascii_hexdigit())
                    || value.port == 0
                    || value
                        .origin
                        .as_deref()
                        .is_some_and(|origin| !valid_origin(origin))
                    || value.origin.is_some() != value.instance_id.is_some()
                {
                    return Err(storage_error());
                }
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
                        .map_err(|_| storage_error())?;
                }
                Ok(value)
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Self::fresh(DEFAULT_PORT),
            _ => Err(storage_error()),
        }
    }
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtensionStatus {
    state: &'static str,
    endpoint: String,
    profile_label: Option<String>,
    extension_version: Option<String>,
    error: Option<String>,
}
struct Pending {
    reply: oneshot::Sender<Result<Value, AgentError>>,
    mutation: bool,
}
struct Connection {
    id: String,
    outgoing: mpsc::Sender<Message>,
    label: String,
    version: String,
}
#[derive(Default)]
struct Inner {
    pairing: Option<Pairing>,
    path: Option<PathBuf>,
    connection: Option<Connection>,
    pending: BTreeMap<String, Pending>,
    error: Option<String>,
}
#[derive(Default)]
pub struct ExtensionState {
    startup: tokio::sync::Mutex<()>,
    inner: Arc<Mutex<Inner>>,
}
impl Inner {
    fn status(&self) -> ExtensionStatus {
        ExtensionStatus {
            state: if self.connection.is_some() {
                "connected"
            } else if self.error.is_some() {
                "error"
            } else {
                "listening"
            },
            endpoint: self
                .pairing
                .as_ref()
                .map(Pairing::endpoint)
                .unwrap_or_default(),
            profile_label: self
                .connection
                .as_ref()
                .map(|connection| connection.label.clone()),
            extension_version: self
                .connection
                .as_ref()
                .map(|connection| connection.version.clone()),
            error: self.error.clone(),
        }
    }
    fn disconnect(&mut self) {
        if let Some(connection) = self.connection.take() {
            let _ = connection.outgoing.try_send(Message::Close(None));
        }
        for (_, pending) in std::mem::take(&mut self.pending) {
            let _ = pending.reply.send(Err(interrupted(pending.mutation)));
        }
    }
}
fn status_changed(app: &tauri::AppHandle, inner: &Arc<Mutex<Inner>>) {
    if let Ok(inner) = inner.lock() {
        let _ = app.emit_to(
            EventTarget::webview("main"),
            "browser-extension:changed",
            inner.status(),
        );
    }
}

/// Restoring a paired browser must not delay application bootstrap.
pub(crate) fn start_if_configured(app: &tauri::AppHandle) {
    let configured = app.path().app_data_dir().ok().is_some_and(|directory| {
        directory
            .join("browser-extension-connection.json")
            .is_file()
    });
    if !configured {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        if let Err(error) = ensure_listener(&app).await {
            let state = app.state::<ExtensionState>();
            if let Ok(mut inner) = state.inner.lock() {
                inner.error = Some(error.message().to_owned());
            }
            status_changed(&app, &state.inner);
        }
    });
}

fn ensure_listener(
    app: &tauri::AppHandle,
) -> futures_util::future::BoxFuture<'_, Result<(), AgentError>> {
    Box::pin(async move {
        let state = app.state::<ExtensionState>();
        let _starting = state.startup.lock().await;
        if state
            .inner
            .lock()
            .map_err(|_| AgentError::internal())?
            .pairing
            .is_some()
        {
            return Ok(());
        }
        let path = app
            .path()
            .app_data_dir()
            .map_err(|_| storage_error())?
            .join("browser-extension-connection.json");
        let mut pairing = Pairing::load(&path)?;
        let listener = match TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, pairing.port)).await
        {
            Ok(listener) => listener,
            Err(_) => TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
                .await
                .map_err(|_| {
                    failure(
                        "browser_extension_listener",
                        "Não foi possível abrir a conexão local com o navegador.",
                    )
                })?,
        };
        let previous_port = pairing.port;
        pairing.port = listener
            .local_addr()
            .map_err(|_| AgentError::internal())?
            .port();
        pairing.save(&path)?;
        {
            let mut inner = state.inner.lock().map_err(|_| AgentError::internal())?;
            if previous_port != pairing.port && pairing.instance_id.is_some() {
                inner.error = Some("A porta anterior está ocupada. Prepare a extensão novamente e cole o novo código de conexão nas opções dela.".into());
            }
            inner.path = Some(path);
            inner.pairing = Some(pairing);
        }
        let app = app.clone();
        let inner = state.inner.clone();
        tauri::async_runtime::spawn(async move {
            let handshakes = Arc::new(Semaphore::new(8));
            loop {
                let (stream, peer) = match listener.accept().await {
                    Ok(connection) => connection,
                    Err(_) => {
                        if let Ok(mut inner) = inner.lock() {
                            inner.error = Some(
                                "A conexão local foi interrompida. Tentando reconectar…".into(),
                            );
                        }
                        status_changed(&app, &inner);
                        tokio::time::sleep(Duration::from_secs(1)).await;
                        continue;
                    }
                };
                if !peer.ip().is_loopback() {
                    continue;
                }
                let Ok(permit) = handshakes.clone().try_acquire_owned() else {
                    continue;
                };
                let app = app.clone();
                let inner = inner.clone();
                tauri::async_runtime::spawn(async move {
                    serve(app, inner, stream, permit).await;
                });
            }
        });
        Ok(())
    })
}

fn valid_origin(origin: &str) -> bool {
    origin
        .strip_prefix("chrome-extension://")
        .is_some_and(|id| id.len() == 32 && id.bytes().all(|byte| (b'a'..=b'p').contains(&byte)))
}
fn matches_secret(left: &str, right: &str) -> bool {
    left.len() == right.len()
        && left
            .bytes()
            .zip(right.bytes())
            .fold(0_u8, |difference, (left, right)| {
                difference | (left ^ right)
            })
            == 0
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Hello {
    #[serde(rename = "type")]
    kind: String,
    version: u8,
    token: String,
    instance_id: String,
    epoch: String,
    label: String,
    extension_version: String,
}
fn authorized(pairing: &Pairing, origin: &str, hello: &Hello) -> bool {
    hello.kind == "hello"
        && hello.version == 1
        && matches_secret(&pairing.token, &hello.token)
        && valid_origin(origin)
        && pairing
            .origin
            .as_deref()
            .is_none_or(|bound| bound == origin)
        && pairing
            .instance_id
            .as_deref()
            .is_none_or(|bound| bound == hello.instance_id)
        && !hello.instance_id.is_empty()
        && hello.instance_id.len() <= 128
        && !hello.epoch.is_empty()
        && hello.epoch.len() <= 128
        && hello.label.len() <= 160
        && hello.extension_version.len() <= 32
}
#[allow(clippy::result_large_err)] // tungstenite's handshake callback requires this concrete response.
fn validate_upgrade(
    request: &Request,
    response: Response,
    pairing: &Pairing,
) -> Result<Response, ErrorResponse> {
    let origin = request
        .headers()
        .get("origin")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    let host = request
        .headers()
        .get("host")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    if request.uri().path() != "/extension"
        || request.uri().query().is_some()
        || host != format!("127.0.0.1:{}", pairing.port)
        || !valid_origin(origin)
        || pairing
            .origin
            .as_deref()
            .is_some_and(|bound| bound != origin)
    {
        return Err(Response::builder()
            .status(403)
            .body(Some("Forbidden".into()))
            .expect("valid response"));
    }
    Ok(response)
}
#[allow(clippy::result_large_err)] // tungstenite fixes the handshake callback error to an HTTP response.
async fn serve(
    app: tauri::AppHandle,
    inner: Arc<Mutex<Inner>>,
    stream: TcpStream,
    permit: tokio::sync::OwnedSemaphorePermit,
) {
    let pairing = match inner.lock().ok().and_then(|inner| inner.pairing.clone()) {
        Some(pairing) => pairing,
        None => return,
    };
    let mut origin = String::new();
    let upgrade = accept_hdr_async_with_config(
        stream,
        |request: &Request, response| {
            let response = validate_upgrade(request, response, &pairing)?;
            origin = request.headers()["origin"]
                .to_str()
                .unwrap_or_default()
                .to_owned();
            Ok(response)
        },
        Some(
            WebSocketConfig::default()
                .max_message_size(Some(MAX_MESSAGE))
                .max_frame_size(Some(MAX_MESSAGE)),
        ),
    );
    let Ok(Ok(mut socket)) = timeout(Duration::from_secs(5), upgrade).await else {
        return;
    };
    let Ok(Some(Ok(Message::Text(text)))) = timeout(Duration::from_secs(5), socket.next()).await
    else {
        return;
    };
    if text.len() > 4096 {
        return;
    }
    let Ok(hello) = serde_json::from_str::<Hello>(&text) else {
        return;
    };
    let Ok(connection_id) = secret() else {
        return;
    };
    let (outgoing, mut receiver) = mpsc::channel(MAX_PENDING + 4);
    let accepted = (|| -> Result<(), AgentError> {
        let mut state = inner.lock().map_err(|_| AgentError::internal())?;
        if state.connection.is_some() {
            return Err(failure(
                "browser_extension_busy",
                "Outro navegador já está conectado.",
            ));
        }
        let mut pairing = state.pairing.clone().ok_or_else(AgentError::internal)?;
        if !authorized(&pairing, &origin, &hello) {
            return Err(failure("browser_extension_auth", "Conexão não autorizada."));
        }
        pairing.origin = Some(origin);
        pairing.instance_id = Some(hello.instance_id);
        pairing.save(state.path.as_deref().ok_or_else(storage_error)?)?;
        state.pairing = Some(pairing);
        state.error = None;
        state.connection = Some(Connection {
            id: connection_id.clone(),
            outgoing,
            label: hello.label,
            version: hello.extension_version,
        });
        Ok(())
    })();
    if accepted.is_err() {
        return;
    }
    drop(permit);
    status_changed(&app, &inner);
    let ready = Message::Text(json!({"type":"ready","version":1}).to_string().into());
    if matches!(
        timeout(Duration::from_secs(5), socket.send(ready)).await,
        Ok(Ok(()))
    ) {
        let reconcile_app = app.clone();
        let reconcile: std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>> =
            Box::pin(async move {
                let _ = super::prune(&reconcile_app).await;
            });
        tauri::async_runtime::spawn(reconcile);
        loop {
            tokio::select! {
                message = receiver.recv() => {
                    let Some(message) = message else { break; };
                    let current = inner.lock().is_ok_and(|state| state.connection.as_ref().is_some_and(|connection| connection.id == connection_id));
                    if !current { break; }
                    // A cancelled request still waiting in the queue must never reach the page.
                    if let Message::Text(text) = &message {
                        if let Ok(value) = serde_json::from_str::<Value>(text) {
                                if let Some(id) = value.get("id").and_then(Value::as_str).filter(|_| value.get("type").and_then(Value::as_str) == Some("request")) {
                                if !inner.lock().is_ok_and(|state| state.pending.contains_key(id)) { continue; }
                            }
                        }
                    }
                    let closing = message.is_close();
                    if !matches!(timeout(Duration::from_secs(10), socket.send(message)).await, Ok(Ok(()))) || closing { break; }
                }
                message = timeout(Duration::from_secs(65), socket.next()) => {
                    match message {
                        Ok(Some(Ok(Message::Text(text)))) => {
                            let Ok(message) = serde_json::from_str::<Value>(&text) else { break; };
                            if message.get("type").and_then(Value::as_str) == Some("ping") {
                                if !matches!(timeout(Duration::from_secs(5), socket.send(Message::Text("{\"type\":\"pong\"}".into()))).await, Ok(Ok(()))) { break; }
                            } else { receive(&app, &inner, &connection_id, message); }
                        }
                        Ok(Some(Ok(Message::Ping(bytes)))) => {
                            if !matches!(timeout(Duration::from_secs(5), socket.send(Message::Pong(bytes))).await, Ok(Ok(()))) { break; }
                        }
                        Ok(Some(Ok(Message::Pong(_)))) => {}
                        _ => break,
                    }
                }
            }
        }
    }
    if let Ok(mut state) = inner.lock() {
        if state
            .connection
            .as_ref()
            .is_some_and(|connection| connection.id == connection_id)
        {
            state.disconnect();
        }
    }
    status_changed(&app, &inner);
}

fn receive(app: &tauri::AppHandle, inner: &Arc<Mutex<Inner>>, connection_id: &str, message: Value) {
    let Ok(mut state) = inner.lock() else {
        return;
    };
    if !state
        .connection
        .as_ref()
        .is_some_and(|connection| connection.id == connection_id)
    {
        return;
    }
    match message.get("type").and_then(Value::as_str) {
        Some("changed") => {
            if let Some(conversation) = message
                .get("conversationId")
                .and_then(Value::as_str)
                .filter(|id| !id.is_empty() && id.len() <= 128)
            {
                let _ = app.emit_to(
                    EventTarget::webview("main"),
                    "browser:changed",
                    json!({"conversationId":conversation}),
                );
            }
        }
        Some("result") => {
            let Some(id) = message.get("id").and_then(Value::as_str) else {
                return;
            };
            let Some(pending) = state.pending.remove(id) else {
                return;
            };
            let result = if message.get("ok").and_then(Value::as_bool) == Some(true) {
                message
                    .get("result")
                    .cloned()
                    .ok_or_else(|| interrupted(pending.mutation))
            } else if let Some(error) = message.get("error") {
                let code = error
                    .get("code")
                    .and_then(Value::as_str)
                    .filter(|code| {
                        code.len() <= 80
                            && code
                                .bytes()
                                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
                    })
                    .unwrap_or("browser_extension");
                let message = error
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("O navegador não concluiu a ação.");
                Err(failure(
                    code,
                    &message.chars().take(2000).collect::<String>(),
                ))
            } else {
                Err(interrupted(pending.mutation))
            };
            let _ = pending.reply.send(result);
        }
        _ => {}
    }
}
fn mutation(action: &str) -> bool {
    !matches!(
        action,
        "list" | "discover" | "snapshot" | "console" | "network" | "response_body" | "screenshot"
    )
}
fn interrupted(mutation: bool) -> AgentError {
    if mutation {
        failure("browser_outcome_unknown", "A conexão não confirmou o resultado da ação. Ela pode ter ocorrido. Inspecione a aba antes de decidir o próximo passo; não repita a ação automaticamente.")
    } else {
        failure("browser_extension_disconnected", "A extensão não respondeu. Verifique se o navegador está aberto e conectado ao Jarvis e tente consultar a aba novamente.")
    }
}
struct PendingGuard {
    inner: Arc<Mutex<Inner>>,
    id: String,
}
impl Drop for PendingGuard {
    fn drop(&mut self) {
        if let Ok(mut inner) = self.inner.lock() {
            if inner.pending.remove(&self.id).is_some() {
                if let Some(connection) = &inner.connection {
                    // Best effort: stop queued/remaining steps, never undo a dispatched action.
                    let _ = connection.outgoing.try_send(Message::Text(
                        json!({"type":"cancel","id":self.id}).to_string().into(),
                    ));
                }
            }
        }
    }
}
pub(crate) fn is_connected(app: &tauri::AppHandle) -> bool {
    app.state::<ExtensionState>()
        .inner
        .lock()
        .is_ok_and(|inner| inner.connection.is_some())
}
pub(crate) async fn request(
    app: &tauri::AppHandle,
    conversation: &str,
    request: Value,
) -> Result<Value, AgentError> {
    ensure_listener(app).await?;
    let paired = app
        .state::<ExtensionState>()
        .inner
        .lock()
        .is_ok_and(|inner| {
            inner
                .pairing
                .as_ref()
                .is_some_and(|pairing| pairing.instance_id.is_some())
        });
    if should_launch(
        request.get("action").and_then(Value::as_str),
        paired,
        is_connected(app),
    ) {
        // Opening a tab is explicit browser intent. Restore its installed browser
        // before dispatch, without replaying any action sent on an older connection.
        let application = app
            .state::<crate::system::SystemState>()
            .browser_preferences()
            .map_err(|message| failure("browser_application", &message))?
            .application;
        let application = match application {
            super::BrowserApplication::Chrome => "chrome",
            super::BrowserApplication::Edge => "edge",
            super::BrowserApplication::Brave => "brave",
            super::BrowserApplication::Chromium => "chromium",
        };
        launch_browser(application).await?;
    }
    if paired && !is_connected(app) {
        // Reconnection is safe before dispatch; sent operations are never replayed.
        let _ = timeout(Duration::from_secs(5), async {
            while !is_connected(app) {
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        })
        .await;
    }
    if conversation.len() > 128 || request.to_string().len() > 1_000_000 {
        return Err(failure(
            "browser_extension_request",
            "A solicitação ao navegador excedeu o limite de tamanho.",
        ));
    }
    let action = request
        .get("action")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            failure(
                "browser_extension_request",
                "A ação do navegador é obrigatória.",
            )
        })?;
    let mutation = mutation(action);
    let id = secret()?;
    let inner = app.state::<ExtensionState>().inner.clone();
    let (reply, response) = oneshot::channel();
    {
        let mut state = inner.lock().map_err(|_| AgentError::internal())?;
        if state.pending.len() >= MAX_PENDING {
            return Err(failure(
                "browser_extension_busy",
                "O navegador está ocupado. Aguarde as ações em andamento.",
            ));
        }
        let connection = state.connection.as_ref().ok_or_else(|| {
            failure(
                "browser_extension_disconnected",
                "Conecte a extensão nas configurações do Jarvis e mantenha o navegador aberto.",
            )
        })?;
        connection.outgoing.try_send(Message::Text(json!({"type":"request","id":id,"conversationId":conversation,"request":request}).to_string().into()))
            .map_err(|_| failure("browser_extension_busy", "A fila do navegador está indisponível. Reconecte a extensão antes de tentar novamente."))?;
        state
            .pending
            .insert(id.clone(), Pending { reply, mutation });
    }
    let _pending = PendingGuard { inner, id };
    match timeout(RESPONSE_TIMEOUT, response).await {
        Ok(Ok(result)) => result,
        _ => Err(interrupted(mutation)),
    }
}

fn should_launch(action: Option<&str>, paired: bool, connected: bool) -> bool {
    action == Some("open") && paired && !connected
}

#[tauri::command]
pub async fn get_browser_extension_status(
    app: tauri::AppHandle,
) -> Result<ExtensionStatus, AgentError> {
    ensure_listener(&app).await?;
    Ok(app
        .state::<ExtensionState>()
        .inner
        .lock()
        .map_err(|_| AgentError::internal())?
        .status())
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtensionInstallation {
    path: String,
    connection_code: String,
}

fn copy_assets(
    source: &Path,
    destination: &Path,
    total: &mut (usize, u64),
) -> Result<(), AgentError> {
    for entry in fs::read_dir(source).map_err(|_| storage_error())? {
        let entry = entry.map_err(|_| storage_error())?;
        let metadata = fs::symlink_metadata(entry.path()).map_err(|_| storage_error())?;
        total.0 += 1;
        total.1 = total.1.saturating_add(metadata.len());
        if metadata.file_type().is_symlink() || total.0 > 1000 || total.1 > 32 * 1024 * 1024 {
            return Err(failure(
                "browser_extension_assets",
                "Os arquivos da extensão são inválidos.",
            ));
        }
        let target = destination.join(entry.file_name());
        if metadata.is_dir() {
            fs::create_dir(&target).map_err(|_| storage_error())?;
            copy_assets(&entry.path(), &target, total)?;
        } else if metadata.is_file() {
            fs::copy(entry.path(), target).map_err(|_| storage_error())?;
        } else {
            return Err(storage_error());
        }
    }
    Ok(())
}
fn install_assets(source: &Path, target: &Path) -> Result<(), AgentError> {
    if !source.join("manifest.json").is_file() {
        return Err(failure(
            "browser_extension_assets",
            "Os arquivos da extensão não estão disponíveis nesta instalação. Atualize o Jarvis.",
        ));
    }
    let parent = target.parent().ok_or_else(storage_error)?;
    fs::create_dir_all(parent).map_err(|_| storage_error())?;
    let staged = tempfile::tempdir_in(parent).map_err(|_| storage_error())?;
    copy_assets(source, staged.path(), &mut (0, 0))?;
    let old = parent.join(format!(".browser-extension-{}", secret()?));
    let exists = match fs::symlink_metadata(target) {
        Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => true,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
        _ => return Err(storage_error()),
    };
    if exists {
        fs::rename(target, &old).map_err(|_| storage_error())?;
    }
    if fs::rename(staged.path(), target).is_err() {
        if exists {
            let _ = fs::rename(&old, target);
        }
        return Err(storage_error());
    }
    if exists {
        let _ = fs::remove_dir_all(old);
    }
    Ok(())
}
#[tauri::command]
pub async fn prepare_browser_extension(
    app: tauri::AppHandle,
) -> Result<ExtensionInstallation, AgentError> {
    ensure_listener(&app).await?;
    let resource = app
        .path()
        .resource_dir()
        .map_err(|_| storage_error())?
        .join("browser-extension");
    #[cfg(debug_assertions)]
    let resource = if resource.join("manifest.json").is_file() {
        resource
    } else {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../browser-extension/dist")
    };
    let path = app
        .path()
        .app_data_dir()
        .map_err(|_| storage_error())?
        .join("browser-extension");
    let target = path.clone();
    tauri::async_runtime::spawn_blocking(move || install_assets(&resource, &target))
        .await
        .map_err(|_| AgentError::internal())??;
    let state = app.state::<ExtensionState>();
    let inner = state.inner.lock().map_err(|_| AgentError::internal())?;
    let pairing = inner.pairing.as_ref().ok_or_else(AgentError::internal)?;
    Ok(ExtensionInstallation {
        path: path.to_string_lossy().into_owned(),
        connection_code: json!({"version":1,"endpoint":pairing.endpoint(),"token":pairing.token})
            .to_string(),
    })
}
#[tauri::command]
pub fn open_browser_extension_directory(app: tauri::AppHandle) -> Result<(), AgentError> {
    let path = app
        .path()
        .app_data_dir()
        .map_err(|_| storage_error())?
        .join("browser-extension");
    if !path.join("manifest.json").is_file() {
        return Err(failure(
            "browser_extension_assets",
            "Prepare a instalação da extensão primeiro.",
        ));
    }
    app.opener()
        .open_path(path.to_string_lossy(), None::<&str>)
        .map_err(|_| {
            failure(
                "browser_extension_open",
                "Não foi possível abrir a pasta da extensão.",
            )
        })
}
#[tauri::command]
pub async fn revoke_browser_extension(
    app: tauri::AppHandle,
) -> Result<ExtensionStatus, AgentError> {
    ensure_listener(&app).await?;
    {
        let state = app.state::<ExtensionState>();
        let mut inner = state.inner.lock().map_err(|_| AgentError::internal())?;
        let pairing = Pairing::fresh(
            inner
                .pairing
                .as_ref()
                .ok_or_else(AgentError::internal)?
                .port,
        )?;
        pairing.save(inner.path.as_deref().ok_or_else(storage_error)?)?;
        inner.pairing = Some(pairing);
        inner.disconnect();
        inner.error = None;
    }
    status_changed(&app, &app.state::<ExtensionState>().inner);
    get_browser_extension_status(app).await
}

#[tauri::command]
pub async fn open_browser_application(application: String) -> Result<(), AgentError> {
    if !matches!(
        application.as_str(),
        "chrome" | "edge" | "brave" | "chromium"
    ) {
        return Err(failure(
            "browser_application",
            "Selecione um navegador Chromium válido.",
        ));
    }
    launch_browser(&application).await
}
#[cfg(target_os = "macos")]
async fn launch_browser(application: &str) -> Result<(), AgentError> {
    let name = match application {
        "chrome" => "Google Chrome",
        "edge" => "Microsoft Edge",
        "brave" => "Brave Browser",
        _ => "Chromium",
    };
    let result = timeout(
        Duration::from_secs(10),
        tokio::process::Command::new("/usr/bin/open")
            .args(["-a", name])
            .kill_on_drop(true)
            .output(),
    )
    .await;
    if matches!(result, Ok(Ok(output)) if output.status.success()) {
        Ok(())
    } else {
        Err(failure(
            "browser_application",
            "Não foi possível abrir o navegador. Verifique se ele está instalado.",
        ))
    }
}
#[cfg(not(target_os = "macos"))]
async fn launch_browser(application: &str) -> Result<(), AgentError> {
    let candidates = browser_candidates(application);
    for candidate in candidates {
        let mut command = tokio::process::Command::new(candidate);
        command
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        #[cfg(windows)]
        command.creation_flags(0x08000000);
        if let Ok(mut child) = command.spawn() {
            tokio::spawn(async move {
                let _ = child.wait().await;
            });
            return Ok(());
        }
    }
    Err(failure(
        "browser_application",
        "Não foi possível abrir o navegador. Verifique se ele está instalado.",
    ))
}
#[cfg(target_os = "linux")]
fn browser_candidates(application: &str) -> Vec<PathBuf> {
    let names: &[&str] = match application {
        "chrome" => &["google-chrome", "google-chrome-stable"],
        "edge" => &["microsoft-edge", "microsoft-edge-stable"],
        "brave" => &["brave-browser", "brave"],
        _ => &["chromium", "chromium-browser"],
    };
    names.iter().map(PathBuf::from).collect()
}
#[cfg(windows)]
fn browser_candidates(application: &str) -> Vec<PathBuf> {
    let suffix = match application {
        "chrome" => "Google/Chrome/Application/chrome.exe",
        "edge" => "Microsoft/Edge/Application/msedge.exe",
        "brave" => "BraveSoftware/Brave-Browser/Application/brave.exe",
        _ => "Chromium/Application/chrome.exe",
    };
    ["LOCALAPPDATA", "PROGRAMFILES", "PROGRAMFILES(X86)"]
        .into_iter()
        .filter_map(std::env::var_os)
        .map(|root| PathBuf::from(root).join(suffix))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_explicit_open_launches_an_already_paired_disconnected_browser() {
        assert!(should_launch(Some("open"), true, false));
        assert!(!should_launch(Some("list"), true, false));
        assert!(!should_launch(Some("click"), true, false));
        assert!(!should_launch(Some("open"), false, false));
        assert!(!should_launch(Some("open"), true, true));
    }
    fn hello(token: &str) -> Hello {
        Hello {
            kind: "hello".into(),
            version: 1,
            token: token.into(),
            instance_id: "profile-1".into(),
            epoch: "session-1".into(),
            label: "Chrome".into(),
            extension_version: "0.1.0".into(),
        }
    }
    #[test]
    fn pairing_authenticates_token_and_binds_exact_origin_and_profile() {
        let mut pairing = Pairing::fresh(DEFAULT_PORT).unwrap();
        let origin = format!("chrome-extension://{}", "a".repeat(32));
        assert!(authorized(&pairing, &origin, &hello(&pairing.token)));
        assert!(!authorized(&pairing, &origin, &hello("wrong")));
        assert!(!authorized(
            &pairing,
            "https://localhost",
            &hello(&pairing.token)
        ));
        assert!(!valid_origin(&format!("{origin}/")));
        pairing.origin = Some(origin.clone());
        pairing.instance_id = Some("profile-1".into());
        assert!(!authorized(
            &pairing,
            &origin.replace('a', "b"),
            &hello(&pairing.token)
        ));
        let mut other_profile = hello(&pairing.token);
        other_profile.instance_id = "profile-2".into();
        assert!(!authorized(&pairing, &origin, &other_profile));
        let old_hello = hello(&pairing.token);
        let rotated = Pairing::fresh(pairing.port).unwrap();
        assert!(!authorized(&rotated, &origin, &old_hello));
    }
    #[test]
    fn upgrade_rejects_web_origins_wrong_paths_and_credentials_in_query() {
        let pairing = Pairing::fresh(DEFAULT_PORT).unwrap();
        let origin = format!("chrome-extension://{}", "a".repeat(32));
        for (path, origin, host, accepted) in [
            ("/extension", origin.as_str(), "127.0.0.1:17373", true),
            (
                "/extension?token=secret",
                origin.as_str(),
                "127.0.0.1:17373",
                false,
            ),
            ("/cdp", origin.as_str(), "127.0.0.1:17373", false),
            ("/extension", "http://localhost", "127.0.0.1:17373", false),
            ("/extension", origin.as_str(), "attacker.example", false),
        ] {
            let request = Request::builder()
                .uri(path)
                .header("origin", origin)
                .header("host", host)
                .body(())
                .unwrap();
            assert_eq!(
                validate_upgrade(&request, Response::new(()), &pairing).is_ok(),
                accepted
            );
        }
    }
    #[test]
    fn pairing_roundtrip_is_private_and_status_never_contains_token() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("pairing.json");
        let pairing = Pairing::fresh(DEFAULT_PORT).unwrap();
        pairing.save(&path).unwrap();
        assert_eq!(Pairing::load(&path).unwrap().token, pairing.token);
        let inner = Inner {
            pairing: Some(pairing.clone()),
            ..Default::default()
        };
        assert!(!serde_json::to_string(&inner.status())
            .unwrap()
            .contains(&pairing.token));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
    }
    #[tokio::test]
    async fn disconnect_marks_mutation_uncertain_and_cancellation_removes_pending() {
        let inner = Arc::new(Mutex::new(Inner::default()));
        let (reply, response) = oneshot::channel();
        inner.lock().unwrap().pending.insert(
            "click".into(),
            Pending {
                reply,
                mutation: true,
            },
        );
        inner.lock().unwrap().disconnect();
        assert_eq!(
            response.await.unwrap().unwrap_err().code,
            "browser_outcome_unknown"
        );
        let (reply, _response) = oneshot::channel();
        inner.lock().unwrap().pending.insert(
            "query".into(),
            Pending {
                reply,
                mutation: false,
            },
        );
        drop(PendingGuard {
            inner: inner.clone(),
            id: "query".into(),
        });
        assert!(inner.lock().unwrap().pending.is_empty());
        assert!(!mutation("network"));
        assert!(mutation("evaluate"));
    }
    #[test]
    fn unfinished_requests_send_cancel_but_completed_requests_do_not() {
        let (outgoing, mut receiver) = mpsc::channel(4);
        let inner = Arc::new(Mutex::new(Inner {
            connection: Some(Connection {
                id: "connection".into(),
                outgoing,
                label: "Chrome".into(),
                version: "0.1.0".into(),
            }),
            ..Default::default()
        }));
        let (reply, _response) = oneshot::channel();
        inner.lock().unwrap().pending.insert(
            "queued-click".into(),
            Pending {
                reply,
                mutation: true,
            },
        );
        drop(PendingGuard {
            inner: inner.clone(),
            id: "queued-click".into(),
        });
        let Message::Text(message) = receiver.try_recv().unwrap() else {
            panic!("expected cancellation");
        };
        assert_eq!(
            serde_json::from_str::<Value>(&message).unwrap(),
            json!({"type":"cancel","id":"queued-click"})
        );
        assert!(inner.lock().unwrap().pending.is_empty());
        let (reply, _response) = oneshot::channel();
        inner.lock().unwrap().pending.insert(
            "completed".into(),
            Pending {
                reply,
                mutation: false,
            },
        );
        inner
            .lock()
            .unwrap()
            .pending
            .remove("completed")
            .unwrap()
            .reply
            .send(Ok(json!({"ok":true})))
            .unwrap();
        drop(PendingGuard {
            inner,
            id: "completed".into(),
        });
        assert!(receiver.try_recv().is_err());
    }
    #[test]
    fn installation_replaces_assets_without_leaking_connection_code() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("source");
        let target = root.path().join("installed");
        fs::create_dir(&source).unwrap();
        fs::write(source.join("manifest.json"), "{}").unwrap();
        install_assets(&source, &target).unwrap();
        fs::write(source.join("worker.js"), "export {}").unwrap();
        install_assets(&source, &target).unwrap();
        assert!(target.join("worker.js").is_file());
        assert_eq!(fs::read_dir(target).unwrap().count(), 2);
    }
    #[cfg(unix)]
    #[test]
    fn invalid_assets_preserve_existing_installation_and_never_follow_symlinks() {
        use std::os::unix::fs::symlink;
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("source");
        let target = root.path().join("installed");
        fs::create_dir(&source).unwrap();
        fs::write(source.join("manifest.json"), "original").unwrap();
        install_assets(&source, &target).unwrap();
        fs::write(source.join("manifest.json"), "replacement").unwrap();
        symlink(root.path(), source.join("escape")).unwrap();
        assert!(install_assets(&source, &target).is_err());
        assert_eq!(
            fs::read_to_string(target.join("manifest.json")).unwrap(),
            "original"
        );
        assert!(!target.join("escape").exists());
        let alias = root.path().join("alias");
        symlink(&target, &alias).unwrap();
        assert!(install_assets(&target, &alias).is_err());
        assert_eq!(
            fs::read_to_string(target.join("manifest.json")).unwrap(),
            "original"
        );
    }
}
