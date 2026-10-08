//! Explicit browser OAuth for MCPs. Secrets remain in the native vault.
use super::{coded_error, storage_error, Config, McpError, McpState, Secrets, Server};
use crate::persistence::AppState;
use async_trait::async_trait;
use rmcp::transport::auth::{
    AuthClient, AuthError, AuthorizationManager, AuthorizationRequest, AuthorizationSession,
    CredentialRefreshGuard, CredentialStore, StoredCredentials,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, OnceLock,
    },
};
use tauri::Manager;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    sync::{watch, Mutex},
    time::Duration,
};

const MAX_CREDENTIAL: usize = 1024 * 1024;
const CALLBACK_TIMEOUT: Duration = Duration::from_secs(600);
type Locks = Mutex<HashMap<String, Arc<Mutex<()>>>>;
static LOCKS: OnceLock<Locks> = OnceLock::new();
static FLOWS: OnceLock<Mutex<HashMap<String, Arc<Flow>>>> = OnceLock::new();

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Status {
    pub authenticated: bool,
    pub state: &'static str,
    pub error: Option<String>,
}
impl Status {
    fn disconnected() -> Self {
        Self {
            authenticated: false,
            state: "disconnected",
            error: None,
        }
    }
    fn connected() -> Self {
        Self {
            authenticated: true,
            state: "connected",
            error: None,
        }
    }
    fn failure(message: &str) -> Self {
        Self {
            authenticated: false,
            state: "error",
            error: Some(message.into()),
        }
    }
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Start {
    pub flow_id: String,
    pub authorization_url: String,
}
struct Flow {
    key: String,
    cancel: watch::Sender<bool>,
    cancelled: Arc<AtomicBool>,
    result: watch::Sender<Option<Status>>,
}

#[derive(Clone)]
struct Store {
    key: String,
    secrets: Arc<dyn Secrets>,
    guard: Arc<Mutex<()>>,
    issuer: Option<String>,
    cancelled: Option<Arc<AtomicBool>>,
}
impl Store {
    async fn new(server: &Server, url: &str, secrets: Arc<dyn Secrets>) -> Self {
        let key = format!(
            "oauth:{:x}",
            Sha256::digest(format!("{}\0{url}", server.id).as_bytes())
        );
        let guard = LOCKS
            .get_or_init(Mutex::default)
            .lock()
            .await
            .entry(key.clone())
            .or_default()
            .clone();
        Self {
            key,
            secrets,
            guard,
            issuer: None,
            cancelled: None,
        }
    }
}
fn store_error() -> AuthError {
    AuthError::CredentialStoreError(
        "Não foi possível acessar a credencial MCP com segurança.".into(),
    )
}
#[async_trait]
impl CredentialStore for Store {
    async fn load(&self) -> Result<Option<StoredCredentials>, AuthError> {
        let (secrets, key, issuer) = (self.secrets.clone(), self.key.clone(), self.issuer.clone());
        tokio::task::spawn_blocking(move || {
            let Some(raw) = secrets.load_optional(&key).map_err(|_| store_error())? else {
                return Ok(None);
            };
            if raw.len() > MAX_CREDENTIAL {
                return Err(store_error());
            }
            let value: StoredCredentials = serde_json::from_str(&raw).map_err(|_| store_error())?;
            if issuer
                .as_ref()
                .is_some_and(|expected| value.issuer.as_ref() != Some(expected))
            {
                return Err(AuthError::AuthorizationRequired);
            }
            Ok(Some(value))
        })
        .await
        .map_err(|_| store_error())?
    }
    async fn save(&self, value: StoredCredentials) -> Result<(), AuthError> {
        if self
            .cancelled
            .as_ref()
            .is_some_and(|flag| flag.load(Ordering::Acquire))
        {
            return Err(AuthError::AuthorizationRequired);
        }
        if self
            .issuer
            .as_ref()
            .is_some_and(|expected| value.issuer.as_ref() != Some(expected))
        {
            return Err(AuthError::AuthorizationRequired);
        }
        let raw = serde_json::to_string(&value).map_err(|_| store_error())?;
        if raw.len() > MAX_CREDENTIAL {
            return Err(store_error());
        }
        let (secrets, key, cancelled) = (
            self.secrets.clone(),
            self.key.clone(),
            self.cancelled.clone(),
        );
        tokio::task::spawn_blocking(move || {
            if cancelled
                .as_ref()
                .is_some_and(|flag| flag.load(Ordering::Acquire))
            {
                return Err(AuthError::AuthorizationRequired);
            }
            secrets.store(&key, &raw).map_err(|_| store_error())
        })
        .await
        .map_err(|_| store_error())?
    }
    async fn clear(&self) -> Result<(), AuthError> {
        let (secrets, key) = (self.secrets.clone(), self.key.clone());
        tokio::task::spawn_blocking(move || secrets.delete(&key).map_err(|_| store_error()))
            .await
            .map_err(|_| store_error())?
    }
    async fn acquire_refresh_guard(&self) -> Result<Option<CredentialRefreshGuard>, AuthError> {
        Ok(Some(CredentialRefreshGuard::new(
            self.guard.clone().lock_owned().await,
        )))
    }
}

fn remote_url(config: &Config) -> Result<&str, McpError> {
    match config {
        Config::Remote {
            url,
            oauth: Some(true),
            headers,
            ..
        } => {
            if headers
                .keys()
                .any(|key| key.eq_ignore_ascii_case("authorization"))
            {
                return Err(coded_error(
                    "mcp_oauth_configuration",
                    "Remova o header Authorization para usar OAuth neste MCP.",
                ));
            }
            let parsed = url::Url::parse(url).map_err(|_| storage_error())?;
            let loopback = parsed.host_str().is_some_and(|host| {
                host == "localhost"
                    || host
                        .parse::<std::net::IpAddr>()
                        .is_ok_and(|ip| ip.is_loopback())
            });
            if parsed.scheme() != "https" && !(parsed.scheme() == "http" && loopback) {
                return Err(coded_error(
                    "mcp_oauth_configuration",
                    "OAuth requer HTTPS, exceto em servidores locais.",
                ));
            }
            Ok(url)
        }
        _ => Err(coded_error(
            "mcp_oauth_configuration",
            "Este MCP não utiliza OAuth.",
        )),
    }
}
fn auth_error() -> McpError {
    coded_error(
        "mcp_auth_required",
        "Conecte sua conta nas configurações deste MCP e tente novamente.",
    )
}
async fn manager(
    server: &Server,
    url: &str,
    secrets: Arc<dyn Secrets>,
) -> Result<(AuthorizationManager, Store), McpError> {
    let mut manager = AuthorizationManager::new(url).await.map_err(|_| {
        coded_error(
            "mcp_oauth_discovery",
            "Não foi possível descobrir a autenticação deste MCP.",
        )
    })?;
    let metadata = manager
        .resolve_metadata()
        .await
        .map_err(|_| {
            coded_error(
                "mcp_oauth_discovery",
                "O servidor não forneceu informações válidas de autenticação.",
            )
        })?
        .metadata;
    for endpoint in [&metadata.authorization_endpoint, &metadata.token_endpoint] {
        let parsed = url::Url::parse(endpoint).map_err(|_| auth_error())?;
        let local = parsed.host_str().is_some_and(|host| {
            host == "localhost"
                || host
                    .parse::<std::net::IpAddr>()
                    .is_ok_and(|ip| ip.is_loopback())
        });
        if parsed.scheme() != "https" && !(local && parsed.scheme() == "http") {
            return Err(auth_error());
        }
    }
    if metadata.issuer.as_deref().is_none_or(str::is_empty) {
        return Err(auth_error());
    }
    let mut store = Store::new(server, url, secrets).await;
    store.issuer = metadata.issuer.clone();
    manager.set_metadata(metadata);
    manager.set_credential_store(store.clone());
    Ok((manager, store))
}

pub(super) async fn client(
    mcp: &McpState,
    server: &Server,
    config: &Config,
) -> Result<AuthClient<reqwest::Client>, McpError> {
    let url = remote_url(config)?;
    let store = Store::new(server, url, mcp.0.secrets.clone()).await;
    let mut manager = if store.load().await.map_err(|_| storage_error())?.is_some() {
        manager(server, url, mcp.0.secrets.clone()).await?.0
    } else {
        let mut manager = AuthorizationManager::new(url)
            .await
            .map_err(|_| auth_error())?;
        manager.set_credential_store(store);
        manager
    };
    // An unauthenticated public MCP can work; only its actual 401 requires login.
    manager
        .initialize_from_store()
        .await
        .map_err(|_| auth_error())?;
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(10))
        .build()
        .map_err(|_| storage_error())?;
    Ok(AuthClient::new(client, manager))
}

fn flow_id() -> Result<String, McpError> {
    let mut bytes = [0_u8; 16];
    getrandom::fill(&mut bytes).map_err(|_| storage_error())?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}
async fn selected(
    mcp: &McpState,
    state: &AppState,
    home: &Path,
    id: &str,
) -> Result<(Server, Config), McpError> {
    let (mcp, state, home, id) = (
        mcp.clone(),
        state.clone(),
        home.to_path_buf(),
        id.to_owned(),
    );
    tokio::task::spawn_blocking(move || {
        let server = mcp
            .list(&state, &home)?
            .into_iter()
            .find(|server| server.id == id)
            .ok_or_else(auth_error)?;
        mcp.active_config(&state, &home, &server)?
            .ok_or_else(auth_error)
    })
    .await
    .map_err(|_| storage_error())?
}
async fn start(
    mcp: McpState,
    state: AppState,
    home: std::path::PathBuf,
    id: &str,
) -> Result<Start, McpError> {
    let (server, config) = selected(&mcp, &state, &home, id).await?;
    let url = remote_url(&config)?;
    let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
        .await
        .map_err(|_| storage_error())?;
    let id = flow_id()?;
    let route = format!("/mcp/callback/{id}");
    let redirect = format!(
        "http://127.0.0.1:{}{route}",
        listener.local_addr().map_err(|_| storage_error())?.port()
    );
    let (mut manager, mut store) = manager(&server, url, mcp.0.secrets.clone()).await?;
    let cancelled = Arc::new(AtomicBool::new(false));
    store.cancelled = Some(cancelled.clone());
    manager.set_credential_store(store.clone());
    let session = AuthorizationSession::new(
        manager,
        AuthorizationRequest::new(&redirect).with_client_name("Jarvis"),
    )
    .await
    .map_err(|_| {
        coded_error(
            "mcp_oauth_registration",
            "O servidor não permitiu registrar o Jarvis para autenticação.",
        )
    })?;
    let authorization_url = session.get_authorization_url().to_owned();
    let (cancel, mut signal) = watch::channel(false);
    let (result, _) = watch::channel(None);
    let flow = Arc::new(Flow {
        key: store.key.clone(),
        cancel,
        cancelled,
        result,
    });
    let mut flows = FLOWS.get_or_init(Mutex::default).lock().await;
    if flows.len() >= 64 {
        let completed = flows
            .iter()
            .find(|(_, flow)| flow.result.borrow().is_some())
            .map(|(id, _)| id.clone());
        if let Some(id) = completed {
            flows.remove(&id);
        }
    }
    if flows.len() >= 64 {
        return Err(coded_error(
            "mcp_oauth_busy",
            "Finalize ou cancele uma conexão antes de iniciar outra.",
        ));
    }
    flows.insert(id.clone(), flow.clone());
    drop(flows);
    tokio::spawn(async move {
        let received = tokio::select! {
            _ = signal.changed() => Err("A conexão foi cancelada."),
            result = tokio::time::timeout(CALLBACK_TIMEOUT, callback(listener, &redirect, &route)) => match result { Ok(value) => value, Err(_) => Err("A autenticação expirou. Tente conectar novamente.") },
        };
        let status = match received {
            Ok(callback) => {
                let _guard = store.guard.lock().await;
                if *signal.borrow() || !mcp.current(&state, &home, &server) {
                    Status::failure("O MCP foi alterado ou desconectado durante a autenticação.")
                } else {
                    match session.handle_callback_url(&callback).await {
                        Ok(_) => Status::connected(),
                        Err(_) => Status::failure(
                            "Não foi possível validar a autorização do MCP. Tente novamente.",
                        ),
                    }
                }
            }
            Err(message) => Status::failure(message),
        };
        flow.result.send_replace(Some(status));
    });
    Ok(Start {
        flow_id: id,
        authorization_url,
    })
}

async fn callback(
    listener: TcpListener,
    redirect: &str,
    route: &str,
) -> Result<String, &'static str> {
    loop {
        let (mut stream, peer) = listener
            .accept()
            .await
            .map_err(|_| "Não foi possível receber a autenticação.")?;
        if !peer.ip().is_loopback() {
            continue;
        }
        let mut request = Vec::new();
        let read = tokio::time::timeout(Duration::from_secs(2), async {
            let mut byte = [0_u8; 1024];
            while request.len() < 16 * 1024 {
                let size = stream.read(&mut byte).await?;
                if size == 0 {
                    break;
                }
                request.extend_from_slice(&byte[..size]);
                if request.windows(4).any(|part| part == b"\r\n\r\n") {
                    break;
                }
            }
            Ok::<_, std::io::Error>(())
        })
        .await;
        let parsed = if matches!(read, Ok(Ok(()))) {
            callback_url(&request, redirect, route)
        } else {
            None
        };
        let (status, body) = if parsed.is_some() {
            (
                "200 OK",
                "Autenticação recebida. Você pode voltar ao Jarvis.",
            )
        } else {
            ("400 Bad Request", "Retorno inválido.")
        };
        let response = format!("HTTP/1.1 {status}\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
        let _ = tokio::time::timeout(
            Duration::from_secs(2),
            stream.write_all(response.as_bytes()),
        )
        .await;
        if let Some(url) = parsed {
            return Ok(url);
        }
    }
}
fn callback_url(request: &[u8], redirect: &str, route: &str) -> Option<String> {
    if request.len() >= 16 * 1024 || !request.windows(4).any(|part| part == b"\r\n\r\n") {
        return None;
    }
    let text = std::str::from_utf8(request).ok()?;
    let mut parts = text.lines().next()?.split_whitespace();
    if parts.next()? != "GET" {
        return None;
    }
    let target = parts.next()?;
    if !target.starts_with('/') || target.starts_with("//") {
        return None;
    }
    let mut url = url::Url::parse(redirect).ok()?;
    let incoming = url.join(target).ok()?;
    if incoming.path() != route || incoming.origin() != url.origin() {
        return None;
    }
    let pairs: Vec<_> = incoming.query_pairs().collect();
    for field in ["code", "state", "iss", "error"] {
        if pairs.iter().filter(|(key, _)| key == field).count() > 1 {
            return None;
        }
    }
    if !pairs
        .iter()
        .any(|(key, value)| key == "state" && !value.is_empty())
        || !pairs
            .iter()
            .any(|(key, value)| (key == "code" || key == "error") && !value.is_empty())
    {
        return None;
    }
    url.set_query(incoming.query());
    Some(url.into())
}

#[tauri::command]
pub(crate) async fn start_mcp_oauth(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    mcp: tauri::State<'_, McpState>,
    id: String,
) -> Result<Start, McpError> {
    let home = app.path().home_dir().map_err(|_| storage_error())?;
    start(mcp.inner().clone(), state.inner().clone(), home, &id).await
}
#[tauri::command]
pub(crate) async fn wait_mcp_oauth(flow_id: String) -> Result<Status, McpError> {
    let flow = FLOWS
        .get_or_init(Mutex::default)
        .lock()
        .await
        .get(&flow_id)
        .cloned()
        .ok_or_else(auth_error)?;
    let mut result = flow.result.subscribe();
    loop {
        if let Some(status) = result.borrow_and_update().clone() {
            return Ok(status);
        }
        result.changed().await.map_err(|_| storage_error())?;
    }
}
#[tauri::command]
pub(crate) async fn cancel_mcp_oauth(flow_id: String) -> Result<(), McpError> {
    let flow = FLOWS
        .get_or_init(Mutex::default)
        .lock()
        .await
        .get(&flow_id)
        .cloned()
        .ok_or_else(auth_error)?;
    flow.cancelled.store(true, Ordering::Release);
    flow.cancel.send_replace(true);
    Ok(())
}
#[tauri::command]
pub(crate) async fn mcp_oauth_status(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    mcp: tauri::State<'_, McpState>,
    id: String,
) -> Result<Status, McpError> {
    let home = app.path().home_dir().map_err(|_| storage_error())?;
    status(mcp.inner(), state.inner(), &home, &id).await
}

async fn status(
    mcp: &McpState,
    state: &AppState,
    home: &Path,
    id: &str,
) -> Result<Status, McpError> {
    let (server, config) = selected(mcp, state, home, id).await?;
    let store = Store::new(&server, remote_url(&config)?, mcp.0.secrets.clone()).await;
    match store.load().await.map_err(|_| storage_error())? {
        Some(value) if value.token_response.is_some() => Ok(Status::connected()),
        _ => Ok(Status::disconnected()),
    }
}
#[tauri::command]
pub(crate) async fn disconnect_mcp_oauth(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    mcp: tauri::State<'_, McpState>,
    id: String,
) -> Result<Status, McpError> {
    let home = app.path().home_dir().map_err(|_| storage_error())?;
    disconnect(mcp.inner(), state.inner(), &home, &id).await
}
async fn disconnect(
    mcp: &McpState,
    state: &AppState,
    home: &Path,
    id: &str,
) -> Result<Status, McpError> {
    let (server, config) = selected(mcp, state, home, id).await?;
    let store = Store::new(&server, remote_url(&config)?, mcp.0.secrets.clone()).await;
    for flow in FLOWS
        .get_or_init(Mutex::default)
        .lock()
        .await
        .values()
        .filter(|flow| flow.key == store.key && flow.result.borrow().is_none())
    {
        flow.cancelled.store(true, Ordering::Release);
        flow.cancel.send_replace(true);
    }
    let _guard = store.guard.lock().await;
    store.clear().await.map_err(|_| storage_error())?;
    Ok(Status::disconnected())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[derive(Clone)]
    struct TestIssuer {
        base: String,
        requests: Arc<Mutex<Vec<String>>>,
    }

    async fn issuer_response(
        axum::extract::State(issuer): axum::extract::State<TestIssuer>,
        uri: axum::http::Uri,
        body: axum::body::Bytes,
    ) -> axum::response::Response {
        use axum::response::IntoResponse;
        use serde_json::json;
        let base = &issuer.base;
        if uri.path() == "/mcp" {
            // The protected MCP resource publishes a challenge, not a successful
            // empty metadata document. Discovery must follow its advertised URL.
            return (
                axum::http::StatusCode::UNAUTHORIZED,
                [(
                    axum::http::header::WWW_AUTHENTICATE,
                    format!(
                        "Bearer resource_metadata=\"{base}/.well-known/oauth-protected-resource\", scope=\"read\""
                    ),
                )],
            )
                .into_response();
        }
        let value = if uri.path().contains("oauth-protected-resource") {
            json!({"resource":format!("{base}/mcp"),"authorization_servers":[base],"scopes_supported":["read"]})
        } else if uri.path().contains("oauth-authorization-server")
            || uri.path().contains("openid-configuration")
        {
            json!({"issuer":base,"authorization_endpoint":format!("{base}/authorize"),"token_endpoint":format!("{base}/token"),"registration_endpoint":format!("{base}/register"),"response_types_supported":["code"],"grant_types_supported":["authorization_code","refresh_token"],"token_endpoint_auth_methods_supported":["none"],"code_challenge_methods_supported":["S256"],"scopes_supported":["read"]})
        } else if uri.path() == "/register" {
            let request: serde_json::Value = serde_json::from_slice(&body).unwrap();
            json!({"client_id":"native-fixture-client","redirect_uris":request["redirect_uris"],"grant_types":["authorization_code","refresh_token"],"response_types":["code"],"token_endpoint_auth_method":"none"})
        } else if uri.path() == "/token" {
            issuer
                .requests
                .lock()
                .await
                .push(String::from_utf8(body.to_vec()).unwrap());
            json!({"access_token":"native-fixture-access","refresh_token":"native-fixture-refresh","token_type":"Bearer","expires_in":3600,"scope":"read"})
        } else {
            return axum::http::StatusCode::NOT_FOUND.into_response();
        };
        axum::Json(value).into_response()
    }

    #[tokio::test]
    async fn native_oauth_roundtrip_validates_state_pkce_and_disconnects_pending_callbacks() {
        use serde_json::json;
        let home = tempfile::tempdir().unwrap();
        let state = AppState::default();
        let secrets = Arc::new(super::super::tests::MemorySecrets::default());
        let mcp = McpState(Arc::new(super::super::Manager {
            guard: std::sync::Mutex::new(()),
            secrets: secrets.clone(),
            apps_context: std::sync::Mutex::new(None),
        }));
        let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .await
            .unwrap();
        let base = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
        let requests = Arc::new(Mutex::new(Vec::new()));
        let app = axum::Router::new()
            .fallback(issuer_response)
            .with_state(TestIssuer {
                base: base.clone(),
                requests: requests.clone(),
            });
        let server_task = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        let raw = json!({"native-auth":{"type":"remote","url":format!("{base}/mcp"),"oauth":true}})
            .to_string();
        let server = mcp
            .save(&state, home.path(), None, &raw)
            .unwrap()
            .into_iter()
            .find(|server| server.name == "native-auth")
            .unwrap();
        let http = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(5))
            .build()
            .unwrap();
        for valid in [false, true] {
            let start = start(
                mcp.clone(),
                state.clone(),
                home.path().to_owned(),
                &server.id,
            )
            .await
            .unwrap();
            let authorization = url::Url::parse(&start.authorization_url).unwrap();
            let params: HashMap<_, _> = authorization.query_pairs().into_owned().collect();
            assert_eq!(params["code_challenge_method"], "S256");
            assert!(!params["code_challenge"].is_empty());
            let mut callback = url::Url::parse(&params["redirect_uri"]).unwrap();
            callback
                .query_pairs_mut()
                .append_pair(
                    "state",
                    if valid {
                        &params["state"]
                    } else {
                        "wrong-state"
                    },
                )
                .append_pair("code", "native-fixture-code")
                .append_pair("iss", &base);
            assert!(http
                .get(callback)
                .send()
                .await
                .unwrap()
                .status()
                .is_success());
            let status =
                tokio::time::timeout(Duration::from_secs(5), wait_mcp_oauth(start.flow_id))
                    .await
                    .unwrap()
                    .unwrap();
            assert_eq!(status.authenticated, valid);
            assert_eq!(requests.lock().await.len(), usize::from(valid));
        }
        let config = mcp.config(home.path(), &server).unwrap();
        let store = Store::new(&server, remote_url(&config).unwrap(), secrets).await;
        let credential = store.load().await.unwrap().unwrap();
        assert_eq!(credential.issuer.as_deref(), Some(base.as_str()));
        assert!(credential.token_response.is_some());
        assert!(
            status(&mcp, &state, home.path(), &server.id)
                .await
                .unwrap()
                .authenticated
        );
        let token_request = requests.lock().await[0].clone();
        let token_fields: HashMap<_, _> = url::form_urlencoded::parse(token_request.as_bytes())
            .into_owned()
            .collect();
        assert_eq!(token_fields["grant_type"], "authorization_code");
        assert!(!token_fields["code_verifier"].is_empty());
        assert_eq!(token_fields["resource"], format!("{base}/mcp"));
        assert!(
            !disconnect(&mcp, &state, home.path(), &server.id)
                .await
                .unwrap()
                .authenticated
        );
        assert!(store.load().await.unwrap().is_none());
        let pending = start(
            mcp.clone(),
            state.clone(),
            home.path().to_owned(),
            &server.id,
        )
        .await
        .unwrap();
        disconnect(&mcp, &state, home.path(), &server.id)
            .await
            .unwrap();
        let cancelled =
            tokio::time::timeout(Duration::from_secs(5), wait_mcp_oauth(pending.flow_id))
                .await
                .unwrap()
                .unwrap();
        assert!(!cancelled.authenticated);
        assert!(store.load().await.unwrap().is_none());
        assert_eq!(requests.lock().await.len(), 1);
        server_task.abort();
        state.close();
    }

    #[test]
    fn callback_keeps_issuer_and_rejects_duplicates_wrong_route_and_absolute_targets() {
        let redirect = "http://127.0.0.1:1234/mcp/callback/test";
        let request = |path: &str| format!("GET {path} HTTP/1.1\r\nHost: localhost\r\n\r\n");
        let valid = callback_url(
            request("/mcp/callback/test?code=secret&state=random&iss=https%3A%2F%2Fissuer.example")
                .as_bytes(),
            redirect,
            "/mcp/callback/test",
        )
        .unwrap();
        assert_eq!(
            url::Url::parse(&valid)
                .unwrap()
                .query_pairs()
                .find(|(key, _)| key == "iss")
                .unwrap()
                .1,
            "https://issuer.example"
        );
        for path in [
            "/mcp/callback/test?code=x&state=a&state=b",
            "/mcp/callback/test?code=x&code=y&state=a",
            "/other?code=x&state=a",
            "https://evil.test/mcp/callback/test?code=x&state=a",
            "//evil.test/mcp/callback/test?code=x&state=a",
        ] {
            assert!(
                callback_url(request(path).as_bytes(), redirect, "/mcp/callback/test").is_none()
            );
        }
    }
    #[tokio::test]
    async fn vault_is_scoped_and_storage_failures_are_not_missing_credentials() {
        let secrets = Arc::new(super::super::tests::MemorySecrets::default());
        let server = Server {
            id: "plugin-mcp:one".into(),
            name: "One".into(),
            kind: "remote".into(),
            enabled: true,
            configured: true,
            revision: 1,
            last_check: None,
        };
        let first = Store::new(&server, "https://one.test/mcp", secrets.clone()).await;
        let second = Store::new(&server, "https://two.test/mcp", secrets.clone()).await;
        assert_ne!(first.key, second.key);
        first
            .save(StoredCredentials::new(
                "registered".into(),
                None,
                vec![],
                None,
            ))
            .await
            .unwrap();
        assert!(first.load().await.unwrap().is_some());
        assert!(second.load().await.unwrap().is_none());
        assert!(!serde_json::to_string(&Status::connected())
            .unwrap()
            .contains("registered"));
        secrets.fail.store(true, Ordering::Relaxed);
        assert!(second.load().await.is_err());
        let mut cancelled = first.clone();
        cancelled.cancelled = Some(Arc::new(AtomicBool::new(true)));
        assert!(cancelled
            .save(StoredCredentials::new(
                "must-not-save".into(),
                None,
                vec![],
                None
            ))
            .await
            .is_err());
    }
}
