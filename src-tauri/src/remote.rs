//! Opt-in LAN transport. The browser receives a dedicated UI and a typed runtime API.
use axum::{
    body::{to_bytes, Body},
    extract::{Extension, State},
    http::{header, HeaderMap, Method, Request, StatusCode},
    response::{IntoResponse, Response},
    Router,
};
use hyper_util::{
    rt::{TokioIo, TokioTimer},
    service::TowerToHyperService,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tauri::{Emitter, Manager};
use tokio::{
    net::TcpListener,
    sync::{watch, Semaphore},
    time::{interval, timeout, MissedTickBehavior},
};

const DEFAULT_PORT: u16 = 47731;
const PAIR_TTL: u64 = 5 * 60 * 1000;
const SESSION_TTL: u64 = 7 * 24 * 60 * 60 * 1000;
const COOKIE: &str = "jarvis_remote";
const MAX_BODY: usize = 256 * 1024;
const MAX_DEVICES: usize = 32;
const MAX_ACTIONS: usize = 128;
const CSP: &str = "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; font-src 'self'; connect-src 'self'; frame-ancestors 'none'; base-uri 'none'; form-action 'none'; object-src 'none'";

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
fn secret() -> Result<String, String> {
    let mut bytes = [0_u8; 32];
    getrandom::fill(&mut bytes)
        .map_err(|_| "Não foi possível gerar uma conexão segura.".to_string())?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}
fn digest(value: &[u8]) -> String {
    Sha256::digest(value)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
fn same_secret(left: &str, right: &str) -> bool {
    left.len() == right.len()
        && left
            .bytes()
            .zip(right.bytes())
            .fold(0_u8, |difference, (a, b)| difference | (a ^ b))
            == 0
}
fn local_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => ip.is_private() || ip.is_loopback() || ip.is_link_local(),
        IpAddr::V6(_) => false,
    }
}
fn interface_addresses() -> Result<Vec<IpAddr>, String> {
    if_addrs::get_if_addrs()
        .map(|interfaces| {
            interfaces
                .into_iter()
                .map(|interface| interface.ip())
                .collect()
        })
        .map_err(|_| "Não foi possível descobrir o endereço na rede local.".into())
}
fn network_urls(port: u16, addresses: impl IntoIterator<Item = IpAddr>) -> Vec<String> {
    let mut ips: Vec<_> = addresses
        .into_iter()
        .filter(|ip| local_ip(*ip) && !ip.is_loopback())
        .collect();
    ips.sort();
    ips.dedup();
    ips.push(IpAddr::V4(Ipv4Addr::LOCALHOST));
    ips.into_iter()
        .map(|ip| format!("http://{ip}:{port}"))
        .collect()
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Config {
    version: u8,
    enabled: bool,
    port: u16,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            version: 1,
            enabled: false,
            port: DEFAULT_PORT,
        }
    }
}
impl Config {
    fn load(path: &Path) -> Result<Self, String> {
        let bytes = match fs::symlink_metadata(path) {
            Ok(metadata)
                if metadata.is_file()
                    && !metadata.file_type().is_symlink()
                    && metadata.len() <= 1024 =>
            {
                fs::read(path).map_err(|_| "Não foi possível ler o acesso remoto.")?
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self::default())
            }
            _ => return Err("As preferências de acesso remoto são inválidas.".into()),
        };
        let config: Self = serde_json::from_slice(&bytes)
            .map_err(|_| "As preferências de acesso remoto são inválidas.")?;
        if config.version != 1 || config.port == 0 {
            return Err("As preferências de acesso remoto são inválidas.".into());
        }
        Ok(config)
    }
    fn save(&self, path: &Path) -> Result<(), String> {
        let persist = || -> Result<(), Box<dyn std::error::Error>> {
            let parent = path
                .parent()
                .ok_or("Missing remote preferences directory")?;
            fs::create_dir_all(parent)?;
            let mut file = tempfile::NamedTempFile::new_in(parent)?;
            serde_json::to_writer(&mut file, self)?;
            file.flush()?;
            file.as_file().sync_all()?;
            file.persist(path)?;
            Ok(())
        };
        persist().map_err(|_| "Não foi possível salvar o acesso remoto.".into())
    }
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceStatus {
    id: String,
    name: String,
    connected_at: u64,
    last_seen_at: u64,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteStatus {
    enabled: bool,
    running: bool,
    port: Option<u16>,
    urls: Vec<String>,
    pairing_url: Option<String>,
    pairing_expires_at: Option<u64>,
    devices: Vec<DeviceStatus>,
    error: Option<String>,
}
struct Pairing {
    token: String,
    expires_at: u64,
}
struct Action {
    fingerprint: String,
    result: watch::Sender<Option<Value>>,
}
struct Device {
    status: DeviceStatus,
    expires_at: u64,
    revoked: watch::Sender<bool>,
    actions: BTreeMap<String, Action>,
}
struct Rate {
    window: u64,
    requests: u16,
    pairs: u8,
}
#[derive(Default)]
struct Inner {
    config: Config,
    path: Option<PathBuf>,
    port: Option<u16>,
    urls: Vec<String>,
    pairing: Option<Pairing>,
    devices: BTreeMap<String, Device>,
    rates: BTreeMap<IpAddr, Rate>,
    stop: Option<watch::Sender<bool>>,
    error: Option<String>,
}
impl Inner {
    fn update_interfaces(&mut self, addresses: impl IntoIterator<Item = IpAddr>) -> bool {
        let Some(port) = self.port else {
            return false;
        };
        let urls = network_urls(port, addresses);
        if self.urls == urls {
            return false;
        }
        self.urls = urls;
        true
    }
    fn status(&self) -> RemoteStatus {
        RemoteStatus {
            enabled: self.config.enabled,
            running: self.port.is_some(),
            port: self.port,
            urls: self.urls.clone(),
            pairing_url: self.pairing.as_ref().and_then(|pair| {
                self.urls
                    .first()
                    .map(|url| format!("{url}/#pair={}", pair.token))
            }),
            pairing_expires_at: self.pairing.as_ref().map(|pair| pair.expires_at),
            devices: self
                .devices
                .values()
                .map(|device| device.status.clone())
                .collect(),
            error: self.error.clone(),
        }
    }
    fn rotate_pairing(&mut self, time: u64) -> Result<(), String> {
        self.pairing = Some(Pairing {
            token: secret()?,
            expires_at: time + PAIR_TTL,
        });
        Ok(())
    }
    fn close(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(true);
        }
        for device in self.devices.values() {
            let _ = device.revoked.send(true);
        }
        self.devices.clear();
        self.pairing = None;
        self.port = None;
        self.urls.clear();
        self.rates.clear();
    }
    fn expire_devices(&mut self, time: u64) {
        self.devices.retain(|_, device| {
            if time >= device.expires_at {
                let _ = device.revoked.send(true);
                false
            } else {
                true
            }
        });
    }
    fn rate(&mut self, peer: IpAddr, pairing: bool, time: u64) -> bool {
        self.rates
            .retain(|_, rate| time.saturating_sub(rate.window) < 60_000);
        if self.rates.len() >= 256 && !self.rates.contains_key(&peer) {
            return false;
        }
        let rate = self.rates.entry(peer).or_insert(Rate {
            window: time,
            requests: 0,
            pairs: 0,
        });
        rate.requests = rate.requests.saturating_add(1);
        if pairing {
            rate.pairs = rate.pairs.saturating_add(1);
        }
        rate.requests <= 240 && rate.pairs <= 10
    }
    fn session(
        &mut self,
        headers: &HeaderMap,
        time: u64,
    ) -> Option<(String, watch::Receiver<bool>)> {
        self.expire_devices(time);
        let key = session_key(headers)?;
        let device = self.devices.get_mut(&key)?;
        device.status.last_seen_at = time;
        Some((key, device.revoked.subscribe()))
    }
    fn pair(&mut self, input: PairInput, time: u64) -> Result<(String, Value), &'static str> {
        let name = input.name.trim();
        if name.is_empty() || name.len() > 80 || name.chars().any(char::is_control) {
            return Err("invalid_name");
        }
        let pair = self.pairing.as_ref().ok_or("pairing_expired")?;
        if time >= pair.expires_at || !same_secret(&pair.token, &input.token) {
            return Err("pairing_expired");
        }
        self.expire_devices(time);
        if self.devices.len() >= MAX_DEVICES {
            return Err("device_limit");
        }
        // Generate all credentials before consuming the one-use invitation.
        let cookie = secret().map_err(|_| "internal")?;
        let id = secret().map_err(|_| "internal")?;
        self.rotate_pairing(time).map_err(|_| "internal")?;
        let metadata = json!({"deviceId": id, "name": name});
        self.devices.insert(
            digest(cookie.as_bytes()),
            Device {
                status: DeviceStatus {
                    id,
                    name: name.to_owned(),
                    connected_at: time,
                    last_seen_at: time,
                },
                expires_at: time + SESSION_TTL,
                revoked: watch::channel(false).0,
                actions: BTreeMap::new(),
            },
        );
        Ok((cookie, metadata))
    }
}

pub struct RemoteState {
    startup: tokio::sync::Mutex<()>,
    inner: Arc<Mutex<Inner>>,
    jobs: Arc<Semaphore>,
}
impl Default for RemoteState {
    fn default() -> Self {
        Self {
            startup: tokio::sync::Mutex::new(()),
            inner: Arc::new(Mutex::new(Inner::default())),
            jobs: Arc::new(Semaphore::new(16)),
        }
    }
}
impl RemoteState {
    pub(crate) fn shutdown(&self) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.close();
        }
    }
}
fn changed(app: &tauri::AppHandle, inner: &Arc<Mutex<Inner>>) {
    if let Ok(inner) = inner.lock() {
        let _ = app.emit("remote:changed", inner.status());
    }
}

pub(crate) fn setup(app: &tauri::AppHandle) {
    let state = app.state::<RemoteState>();
    let loaded = app
        .path()
        .home_dir()
        .map_err(|_| "Pasta pessoal indisponível.".to_string())
        .and_then(|home| {
            let path = crate::data_dir::root(&home).join("remote.json");
            Config::load(&path).map(|config| (path, config))
        });
    let start = match state.inner.lock() {
        Ok(mut inner) => match loaded {
            Ok((path, config)) => {
                let enabled = config.enabled;
                inner.path = Some(path);
                inner.config = config;
                enabled
            }
            Err(error) => {
                inner.error = Some(error);
                false
            }
        },
        Err(_) => false,
    };
    if start {
        let handle = app.clone();
        tauri::async_runtime::spawn(async move {
            let _ = configure(&handle, true).await;
        });
    }
}

async fn configure(app: &tauri::AppHandle, enabled: bool) -> Result<RemoteStatus, String> {
    let state = app.state::<RemoteState>();
    let _startup = state.startup.lock().await;
    let (path, mut config) = {
        let inner = state
            .inner
            .lock()
            .map_err(|_| "Acesso remoto indisponível.")?;
        if inner.config.enabled == enabled && (!enabled || inner.port.is_some()) {
            return Ok(inner.status());
        }
        (
            inner
                .path
                .clone()
                .ok_or("Preferências de acesso remoto indisponíveis.")?,
            inner.config.clone(),
        )
    };
    config.enabled = enabled;
    if !enabled {
        config.save(&path)?;
        {
            let mut inner = state
                .inner
                .lock()
                .map_err(|_| "Acesso remoto indisponível.")?;
            inner.config = config;
            inner.close();
            inner.error = None;
        }
        changed(app, &state.inner);
        return state
            .inner
            .lock()
            .map(|inner| inner.status())
            .map_err(|_| "Acesso remoto indisponível.".into());
    }
    let result = async {
        let listener = match TcpListener::bind((Ipv4Addr::UNSPECIFIED, config.port)).await {
            Ok(listener) => listener,
            Err(error) if error.kind() == std::io::ErrorKind::AddrInUse => {
                TcpListener::bind((Ipv4Addr::UNSPECIFIED, 0))
                    .await
                    .map_err(|_| "Não foi possível abrir o acesso remoto.")?
            }
            Err(_) => return Err("Não foi possível abrir o acesso remoto.".to_string()),
        };
        config.port = listener
            .local_addr()
            .map_err(|_| "Porta local indisponível.")?
            .port();
        let urls = network_urls(config.port, interface_addresses()?);
        let pair = Pairing {
            token: secret()?,
            expires_at: now() + PAIR_TTL,
        };
        config.save(&path)?;
        let (stop, stopped) = watch::channel(false);
        {
            let mut inner = state
                .inner
                .lock()
                .map_err(|_| "Acesso remoto indisponível.")?;
            inner.close();
            inner.config = config.clone();
            inner.port = Some(config.port);
            inner.urls = urls;
            inner.pairing = Some(pair);
            inner.stop = Some(stop);
            inner.error = None;
        }
        let context = HttpContext {
            app: app.clone(),
            inner: state.inner.clone(),
            jobs: state.jobs.clone(),
        };
        tauri::async_runtime::spawn(serve(listener, context, stopped));
        Ok::<(), String>(())
    }
    .await;
    if let Err(error) = result {
        let mut inner = state
            .inner
            .lock()
            .map_err(|_| "Acesso remoto indisponível.")?;
        inner.close();
        inner.error = Some(error.clone());
        drop(inner);
        changed(app, &state.inner);
        return Err(error);
    }
    changed(app, &state.inner);
    state
        .inner
        .lock()
        .map(|inner| inner.status())
        .map_err(|_| "Acesso remoto indisponível.".into())
}

#[tauri::command]
pub fn get_remote_status(state: tauri::State<'_, RemoteState>) -> Result<RemoteStatus, String> {
    let mut inner = state
        .inner
        .lock()
        .map_err(|_| "Acesso remoto indisponível.")?;
    inner.expire_devices(now());
    if inner.port.is_some()
        && inner
            .pairing
            .as_ref()
            .is_none_or(|pair| now() >= pair.expires_at)
    {
        inner.rotate_pairing(now())?;
    }
    Ok(inner.status())
}
#[tauri::command]
pub async fn set_remote_enabled(
    app: tauri::AppHandle,
    enabled: bool,
) -> Result<RemoteStatus, String> {
    configure(&app, enabled).await
}
#[tauri::command]
pub fn refresh_remote_pairing(
    app: tauri::AppHandle,
    state: tauri::State<'_, RemoteState>,
) -> Result<RemoteStatus, String> {
    {
        let mut inner = state
            .inner
            .lock()
            .map_err(|_| "Acesso remoto indisponível.")?;
        if inner.port.is_none() {
            return Err("Ative o acesso remoto para gerar o QR code.".into());
        }
        inner.rotate_pairing(now())?;
    }
    changed(&app, &state.inner);
    state
        .inner
        .lock()
        .map(|inner| inner.status())
        .map_err(|_| "Acesso remoto indisponível.".into())
}
#[tauri::command]
pub fn revoke_remote_device(
    app: tauri::AppHandle,
    state: tauri::State<'_, RemoteState>,
    device_id: String,
) -> Result<RemoteStatus, String> {
    {
        let mut inner = state
            .inner
            .lock()
            .map_err(|_| "Acesso remoto indisponível.")?;
        inner.devices.retain(|_, device| {
            if device.status.id == device_id {
                let _ = device.revoked.send(true);
                false
            } else {
                true
            }
        });
    }
    changed(&app, &state.inner);
    state
        .inner
        .lock()
        .map(|inner| inner.status())
        .map_err(|_| "Acesso remoto indisponível.".into())
}

#[derive(Clone)]
struct HttpContext {
    app: tauri::AppHandle,
    inner: Arc<Mutex<Inner>>,
    jobs: Arc<Semaphore>,
}
async fn serve(listener: TcpListener, context: HttpContext, mut stopped: watch::Receiver<bool>) {
    let router = Router::new().fallback(handler).with_state(context.clone());
    let connections = Arc::new(Semaphore::new(64));
    let mut refresh = interval(Duration::from_secs(5));
    refresh.set_missed_tick_behavior(MissedTickBehavior::Skip);
    loop {
        let accepted = tokio::select! {
            _ = stopped.changed() => break,
            _ = refresh.tick() => {
                // The listener covers every interface. Keep QR addresses and the
                // Host allowlist in sync after Wi-Fi changes without dropping sessions.
                if let Ok(addresses) = interface_addresses() {
                    let updated = context.inner.lock().is_ok_and(|mut inner| inner.update_interfaces(addresses));
                    if updated { changed(&context.app, &context.inner); }
                }
                continue;
            }
            accepted = listener.accept() => accepted,
        };
        let (stream, peer) = match accepted {
            Ok(value) => value,
            Err(_) => {
                if let Ok(mut inner) = context.inner.lock() {
                    inner.close();
                    inner.error = Some("A conexão de acesso remoto foi interrompida.".into());
                }
                changed(&context.app, &context.inner);
                break;
            }
        };
        if !local_ip(peer.ip()) {
            continue;
        }
        let Ok(permit) = connections.clone().try_acquire_owned() else {
            continue;
        };
        let router = router.clone().layer(Extension(peer));
        let mut connection_stop = stopped.clone();
        tauri::async_runtime::spawn(async move {
            let _permit = permit;
            let mut http = hyper::server::conn::http1::Builder::new();
            http.timer(TokioTimer::new())
                .header_read_timeout(Duration::from_secs(5))
                .max_buf_size(16 * 1024)
                .keep_alive(false);
            let connection =
                http.serve_connection(TokioIo::new(stream), TowerToHyperService::new(router));
            tokio::select! {
                _ = connection_stop.changed() => {},
                _ = timeout(Duration::from_secs(35), connection) => {},
            }
        });
    }
}
fn success(data: Value) -> Value {
    json!({"ok": true, "data": data})
}
fn failure(code: &str, message: &str) -> Value {
    json!({"ok": false, "error": {"code": code, "message": message}})
}
fn json_response(status: StatusCode, value: Value) -> Response {
    (status, axum::Json(value)).into_response()
}
fn rejected(status: StatusCode, code: &str, message: &str) -> Response {
    json_response(status, failure(code, message))
}
fn session_expired() -> Response {
    rejected(
        StatusCode::UNAUTHORIZED,
        "session_expired",
        "A conexão expirou. Escaneie um novo QR code no Jarvis.",
    )
}
fn protect(mut response: Response) -> Response {
    let headers = response.headers_mut();
    for (name, value) in [
        (header::CACHE_CONTROL, "no-store"),
        (header::CONTENT_SECURITY_POLICY, CSP),
        (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
        (header::X_FRAME_OPTIONS, "DENY"),
        (header::REFERRER_POLICY, "no-referrer"),
    ] {
        headers.insert(name, value.parse().expect("static security header"));
    }
    response
}
fn session_key(headers: &HeaderMap) -> Option<String> {
    let mut tokens = headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|header| header.to_str().ok())
        .flat_map(|cookie| cookie.split(';'))
        .filter_map(|part| part.trim().split_once('='))
        .filter_map(|(name, value)| (name == COOKIE).then_some(value));
    let token = tokens.next()?;
    if tokens.next().is_some()
        || token.len() != 64
        || !token.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return None;
    }
    Some(digest(token.as_bytes()))
}
fn valid_request(inner: &Inner, peer: IpAddr, headers: &HeaderMap, method: &Method) -> bool {
    if !inner.config.enabled || inner.port.is_none() || !local_ip(peer) {
        return false;
    }
    let Some(host) = headers
        .get(header::HOST)
        .and_then(|value| value.to_str().ok())
    else {
        return false;
    };
    if headers.get_all(header::HOST).iter().count() != 1 {
        return false;
    }
    let origin = format!("http://{host}");
    if !inner.urls.contains(&origin) {
        return false;
    }
    let origins = headers.get_all(header::ORIGIN);
    if origins.iter().count() > 1 {
        return false;
    }
    let supplied = origins.iter().next().and_then(|value| value.to_str().ok());
    if supplied.is_some_and(|value| value != origin)
        || (method == Method::POST && supplied != Some(origin.as_str()))
    {
        return false;
    }
    if headers
        .get("sec-fetch-site")
        .and_then(|value| value.to_str().ok())
        .is_some_and(|site| !matches!(site, "same-origin" | "none"))
    {
        return false;
    }
    method != Method::POST
        || headers
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| {
                value
                    .split(';')
                    .next()
                    .is_some_and(|mime| mime.trim() == "application/json")
            })
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PairInput {
    token: String,
    name: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RpcInput {
    method: String,
    params: Value,
    request_id: Option<String>,
}
fn mutation(method: &str) -> Option<bool> {
    match method {
        "library" | "chat" | "history" => Some(false),
        "message" | "question" | "approval" | "validation" | "authoring" | "cancel" => Some(true),
        _ => None,
    }
}
async fn handler(
    State(context): State<HttpContext>,
    Extension(peer): Extension<SocketAddr>,
    request: Request<Body>,
) -> Response {
    protect(handle(context, peer, request).await)
}
async fn handle(context: HttpContext, peer: SocketAddr, request: Request<Body>) -> Response {
    let path = request.uri().path().to_owned();
    let method = request.method().clone();
    let headers = request.headers().clone();
    if request.uri().query().is_some()
        || request.uri().scheme().is_some()
        || request.uri().authority().is_some()
    {
        return rejected(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "Use o endereço de conexão sem parâmetros.",
        );
    }
    {
        let Ok(mut inner) = context.inner.lock() else {
            return session_expired();
        };
        if !valid_request(&inner, peer.ip(), &headers, &method) {
            return rejected(
                StatusCode::FORBIDDEN,
                "invalid_origin",
                "Esta origem não pode acessar o Jarvis.",
            );
        }
        if !inner.rate(peer.ip(), path == "/api/pair", now()) {
            return rejected(
                StatusCode::TOO_MANY_REQUESTS,
                "rate_limited",
                "Aguarde um minuto antes de tentar novamente.",
            );
        }
    }
    if !path.starts_with("/api/") {
        if method != Method::GET {
            return rejected(
                StatusCode::METHOD_NOT_ALLOWED,
                "invalid_method",
                "Método indisponível.",
            );
        }
        return asset(&context.app, &path);
    }
    if path == "/api/session" && method == Method::GET {
        let Ok(mut inner) = context.inner.lock() else {
            return session_expired();
        };
        let Some((key, _)) = inner.session(&headers, now()) else {
            return session_expired();
        };
        let device = &inner.devices[&key];
        return json_response(
            StatusCode::OK,
            success(json!({"deviceId": device.status.id, "name": device.status.name})),
        );
    }
    if method != Method::POST {
        return rejected(
            StatusCode::METHOD_NOT_ALLOWED,
            "invalid_method",
            "Método indisponível.",
        );
    }
    let body = match timeout(
        Duration::from_secs(5),
        to_bytes(request.into_body(), MAX_BODY),
    )
    .await
    {
        Ok(Ok(body)) => body,
        Ok(Err(_)) => {
            return rejected(
                StatusCode::PAYLOAD_TOO_LARGE,
                "body_limit",
                "A solicitação excedeu o tamanho permitido.",
            )
        }
        Err(_) => {
            return rejected(
                StatusCode::REQUEST_TIMEOUT,
                "request_timeout",
                "A solicitação demorou demais para chegar.",
            )
        }
    };
    if path == "/api/pair" {
        let Ok(input) = serde_json::from_slice::<PairInput>(&body) else {
            return rejected(
                StatusCode::BAD_REQUEST,
                "invalid_request",
                "Informe um nome e um QR code válido.",
            );
        };
        let result = {
            let Ok(mut inner) = context.inner.lock() else {
                return session_expired();
            };
            // The service may have been disabled while the body was arriving.
            if !inner.config.enabled || inner.port.is_none() {
                return session_expired();
            }
            inner.pair(input, now())
        };
        let (token, metadata) = match result {
            Ok(result) => result,
            Err("invalid_name") => {
                return rejected(
                    StatusCode::BAD_REQUEST,
                    "invalid_name",
                    "Use um nome de até 80 caracteres para o dispositivo.",
                )
            }
            Err("device_limit") => {
                return rejected(
                    StatusCode::CONFLICT,
                    "device_limit",
                    "Remova um dispositivo no Jarvis antes de conectar outro.",
                )
            }
            Err(_) => {
                return rejected(
                    StatusCode::UNAUTHORIZED,
                    "pairing_expired",
                    "Este QR code expirou ou já foi usado. Gere outro no Jarvis.",
                )
            }
        };
        let mut response = json_response(StatusCode::OK, success(metadata));
        response.headers_mut().insert(
            header::SET_COOKIE,
            format!(
                "{COOKIE}={token}; HttpOnly; SameSite=Strict; Path=/api; Max-Age={}",
                SESSION_TTL / 1000
            )
            .parse()
            .expect("hex session cookie"),
        );
        changed(&context.app, &context.inner);
        return response;
    }
    let (key, revoked) = {
        let Ok(mut inner) = context.inner.lock() else {
            return session_expired();
        };
        if !inner.config.enabled || inner.port.is_none() {
            return session_expired();
        }
        let Some(session) = inner.session(&headers, now()) else {
            return session_expired();
        };
        session
    };
    if path == "/api/logout" {
        if let Ok(mut inner) = context.inner.lock() {
            if let Some(device) = inner.devices.remove(&key) {
                let _ = device.revoked.send(true);
            }
        }
        let mut response = json_response(StatusCode::OK, success(Value::Null));
        response.headers_mut().insert(
            header::SET_COOKIE,
            format!("{COOKIE}=; HttpOnly; SameSite=Strict; Path=/api; Max-Age=0")
                .parse()
                .expect("static cookie"),
        );
        changed(&context.app, &context.inner);
        return response;
    }
    if path != "/api/rpc" {
        return rejected(StatusCode::NOT_FOUND, "not_found", "Endpoint indisponível.");
    }
    let Ok(input) = serde_json::from_slice::<RpcInput>(&body) else {
        return rejected(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "A solicitação é inválida.",
        );
    };
    rpc(context, key, revoked, input).await
}

async fn dispatch(app: tauri::AppHandle, method: String, params: Value) -> Value {
    match timeout(
        Duration::from_secs(60),
        crate::agent::remote::dispatch(app, &method, params),
    )
    .await
    {
        Ok(Ok(data)) => success(data),
        Ok(Err(error)) => json!({"ok": false, "error": error}),
        Err(_) => failure(
            "outcome_unknown",
            "O resultado ainda não foi confirmado. Atualize a conversa antes de enviar outra ação.",
        ),
    }
}
async fn wait_action(
    mut result: watch::Receiver<Option<Value>>,
    mut revoked: watch::Receiver<bool>,
) -> Response {
    if *revoked.borrow() {
        return session_expired();
    }
    let existing = result.borrow().clone();
    if let Some(value) = existing {
        return json_response(StatusCode::OK, value);
    }
    tokio::select! {
        _ = revoked.changed() => session_expired(),
        update = timeout(Duration::from_secs(12), result.changed()) => {
            if update.is_err() { return rejected(StatusCode::ACCEPTED, "request_pending", "A ação foi recebida. Atualize a conversa para confirmar o resultado."); }
            let value = result.borrow().clone().unwrap_or_else(|| failure("outcome_unknown", "Atualize a conversa para confirmar o resultado da ação."));
            if *revoked.borrow() { session_expired() } else { json_response(StatusCode::OK, value) }
        }
    }
}
async fn rpc(
    context: HttpContext,
    key: String,
    mut revoked: watch::Receiver<bool>,
    input: RpcInput,
) -> Response {
    let Some(mutation) = mutation(&input.method) else {
        return rejected(
            StatusCode::BAD_REQUEST,
            "unsupported_method",
            "Este comando não está disponível no acesso remoto.",
        );
    };
    if !input.params.is_object() {
        return rejected(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "Use parâmetros válidos para a solicitação.",
        );
    }
    if !mutation {
        let Ok(_permit) = context.jobs.clone().try_acquire_owned() else {
            return rejected(
                StatusCode::SERVICE_UNAVAILABLE,
                "busy",
                "O Jarvis está ocupado. Aguarde alguns segundos.",
            );
        };
        return tokio::select! {
            _ = revoked.changed() => session_expired(),
            value = dispatch(context.app, input.method, input.params) => {
                if *revoked.borrow() { session_expired() } else { json_response(StatusCode::OK, value) }
            }
        };
    }
    let Some(request_id) = input.request_id.filter(|id| {
        !id.is_empty()
            && id.len() <= 128
            && id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    }) else {
        return rejected(
            StatusCode::BAD_REQUEST,
            "request_id_required",
            "Esta ação precisa de um identificador único.",
        );
    };
    let fingerprint = digest(json!([input.method, input.params]).to_string().as_bytes());
    let (result, new_action) = {
        let Ok(mut inner) = context.inner.lock() else {
            return session_expired();
        };
        let Some(device) = inner.devices.get_mut(&key) else {
            return session_expired();
        };
        if let Some(action) = device.actions.get(&request_id) {
            if action.fingerprint != fingerprint {
                return rejected(
                    StatusCode::CONFLICT,
                    "request_id_conflict",
                    "Este identificador já foi usado para outra ação.",
                );
            }
            (action.result.subscribe(), None)
        } else {
            // ponytail: bounded session ledger; reconnect after 128 actions rather than evicting IDs and risking a repeated mutation.
            if device.actions.len() >= MAX_ACTIONS {
                return rejected(
                    StatusCode::CONFLICT,
                    "action_limit",
                    "Reconecte este dispositivo pelo QR code para continuar.",
                );
            }
            let Ok(permit) = context.jobs.clone().try_acquire_owned() else {
                return rejected(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "busy",
                    "O Jarvis está ocupado. Aguarde alguns segundos.",
                );
            };
            let (send, result) = watch::channel(None);
            device.actions.insert(
                request_id,
                Action {
                    fingerprint,
                    result: send.clone(),
                },
            );
            (result, Some((send, permit)))
        }
    };
    if let Some((send, permit)) = new_action {
        // The request may disconnect after acceptance. Its result and ID remain
        // owned by the service; a retry can only observe the original outcome.
        tauri::async_runtime::spawn(async move {
            let _permit = permit;
            let active = context
                .inner
                .lock()
                .is_ok_and(|inner| inner.config.enabled && inner.devices.contains_key(&key));
            let value = if active {
                dispatch(context.app, input.method, input.params).await
            } else {
                failure("session_expired", "A conexão foi encerrada.")
            };
            let _ = send.send(Some(value));
        });
    }
    wait_action(result, revoked).await
}

fn asset_path(path: &str) -> Option<(&str, &'static str)> {
    let path = if path == "/" || path == "/remote.html" {
        "remote.html"
    } else {
        path.strip_prefix('/')?
    };
    if path.len() > 240
        || !path.is_ascii()
        || path.contains(['%', '\\', '\0'])
        || path
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return None;
    }
    if path == "remote.html" {
        return Some((path, "text/html; charset=utf-8"));
    }
    if !path.starts_with("assets/") && !path.starts_with("fonts/") {
        return None;
    }
    let extension = path.rsplit_once('.')?.1;
    let mime = match extension {
        "js" if path.starts_with("assets/") => "text/javascript; charset=utf-8",
        "css" if path.starts_with("assets/") => "text/css; charset=utf-8",
        "svg" if path.starts_with("assets/") => "image/svg+xml",
        "png" if path.starts_with("assets/") => "image/png",
        "webp" if path.starts_with("assets/") => "image/webp",
        "woff2" => "font/woff2",
        "woff" => "font/woff",
        "ttf" => "font/ttf",
        _ => return None,
    };
    Some((path, mime))
}
fn asset(app: &tauri::AppHandle, path: &str) -> Response {
    let Some((path, mime)) = asset_path(path) else {
        return rejected(StatusCode::NOT_FOUND, "not_found", "Arquivo indisponível.");
    };
    let resolver = app.asset_resolver();
    // Tauri's resolver intentionally falls back to index.html. A remote URL
    // must resolve an exact asset, never the privileged desktop entry point.
    #[cfg(not(debug_assertions))]
    let exists = resolver
        .iter()
        .any(|(key, _)| key.trim_start_matches('/') == path);
    #[cfg(debug_assertions)]
    let exists = {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../dist");
        root.canonicalize()
            .ok()
            .zip(root.join(path).canonicalize().ok())
            .is_some_and(|(root, file)| file.starts_with(root) && file.is_file())
    };
    if !exists {
        return rejected(
            StatusCode::NOT_FOUND,
            "assets_missing",
            "A interface remota não está disponível. Compile o frontend do Jarvis.",
        );
    }
    let Some(asset) = resolver.get(path.to_owned()) else {
        return rejected(
            StatusCode::NOT_FOUND,
            "assets_missing",
            "A interface remota não está disponível.",
        );
    };
    let mut response = Response::new(Body::from(asset.bytes));
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        mime.parse().expect("static MIME type"),
    );
    response
}

#[cfg(test)]
mod tests;
