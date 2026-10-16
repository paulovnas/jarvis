//! Subscription execution stays in the official CLI; Jarvis never reads Muse credentials.
use serde::Serialize;
use serde_json::{json, Value};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};
use tauri::{Emitter, Manager};
use tokio::sync::{watch, Mutex};

pub(crate) mod session;
pub(super) mod transport;

mod metadata;
pub(crate) mod usage;

pub(crate) use metadata::RuntimeStatus;
use transport::{MuseControl, MuseProcess, MuseSpawnConfig};

#[derive(Clone, Default)]
pub(crate) struct MuseState {
    runtime: Arc<Mutex<metadata::Cache>>,
    usage: Arc<Mutex<()>>,
    login: Arc<Mutex<Option<PendingLogin>>>,
}

struct PendingLogin {
    id: String,
    control: MuseControl,
    stop: watch::Sender<bool>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LoginFlow {
    id: String,
    verification_url: String,
    user_code: String,
    expires_at: i64,
}

#[tauri::command]
pub(crate) async fn get_muse_runtime(
    state: tauri::State<'_, MuseState>,
    system: tauri::State<'_, crate::system::SystemState>,
) -> Result<RuntimeStatus, String> {
    let mut status = metadata::cached(&state, false).await;
    status.preferences = system.muse_preferences()?;
    Ok(status)
}

#[tauri::command]
pub(crate) async fn refresh_muse_runtime(
    app: tauri::AppHandle,
    state: tauri::State<'_, MuseState>,
    system: tauri::State<'_, crate::system::SystemState>,
) -> Result<RuntimeStatus, String> {
    let mut status = metadata::cached(&state, true).await;
    status.preferences = system.muse_preferences()?;
    let _ = app.emit("muse-runtime:changed", &status);
    Ok(status)
}

#[tauri::command]
pub(crate) async fn get_muse_usage(
    state: tauri::State<'_, MuseState>,
    system: tauri::State<'_, crate::system::SystemState>,
) -> Result<crate::openai_codex::usage::AccountUsage, String> {
    if !system.muse_preferences()?.enabled {
        return Err("Ative o Muse Code em Configurações → Provedores.".into());
    }
    Ok(usage::current(&state).await)
}

#[tauri::command]
pub(crate) async fn start_muse_login(
    app: tauri::AppHandle,
    state: tauri::State<'_, MuseState>,
) -> Result<LoginFlow, String> {
    let mut active = state.login.lock().await;
    if active.is_some() {
        return Err(
            "Já existe um login do Muse em andamento. Conclua ou cancele essa tentativa.".into(),
        );
    }
    let directory = tempfile::tempdir().map_err(|_| metadata::UNAVAILABLE)?;
    let (mut process, _) = connect_with_info(directory.path()).await?;
    let control = process.control();
    let response = tokio::time::timeout(
        Duration::from_secs(25),
        control.request("account/loginStart", &json!({"type":"deviceCode"})),
    )
    .await;
    let response = match response {
        Ok(Ok(response)) => response,
        _ => {
            let _ = process.cancel().await;
            return Err(metadata::UNSUPPORTED.into());
        }
    };
    let flow = crate::claude::new_session_id()
        .map_err(|_| metadata::UNAVAILABLE.to_owned())
        .and_then(|id| {
            let now =
                crate::openai_codex::current_time_millis().map_err(|_| metadata::UNAVAILABLE)?;
            login_flow(&response, id, now)
        });
    let flow = match flow {
        Ok(flow) => flow,
        Err(error) => {
            let _ = tokio::time::timeout(
                Duration::from_secs(5),
                control.request("account/loginCancel", &json!({})),
            )
            .await;
            let _ = process.cancel().await;
            return Err(error);
        }
    };
    let (stop, stopped) = watch::channel(false);
    *active = Some(PendingLogin {
        id: flow.id.clone(),
        control,
        stop,
    });
    let state = state.inner().clone();
    let id = flow.id.clone();
    tauri::async_runtime::spawn(async move {
        let _directory = directory;
        finish_login(app, state, id, process, stopped).await;
    });
    Ok(flow)
}

#[tauri::command]
pub(crate) async fn cancel_muse_login(
    state: tauri::State<'_, MuseState>,
    id: String,
) -> Result<(), String> {
    cancel_login(&state, &id).await
}

#[tauri::command]
pub(crate) async fn disconnect_muse_account(
    app: tauri::AppHandle,
    state: tauri::State<'_, MuseState>,
) -> Result<(), String> {
    let home = user_home()?;
    usage::clear(&home);
    // Serialize a deliberate logout with discovery and completion of any pending login.
    let mut cache = state.runtime.lock().await;
    let mut active = state.login.lock().await;
    if let Some(pending) = active.take() {
        let _ = cancel_pending(pending).await;
    }
    cache.attempted = None;
    let directory = tempfile::tempdir().map_err(|_| metadata::UNAVAILABLE)?;
    let (mut process, info) = connect_profile(&home, directory.path()).await?;
    let control = process.control();
    let result = tokio::time::timeout(Duration::from_secs(25), async {
        control
            .request("account/logout", &json!({}))
            .await
            .map_err(|_| {
                "O Muse não confirmou a desconexão. Atualize o status antes de tentar novamente."
            })?;
        let account = control
            .request("account/read", &json!({}))
            .await
            .map_err(|_| metadata::UNAVAILABLE)?;
        if account["state"] != "loggedOut" {
            return Err(
                "O Muse não confirmou a desconexão. Atualize o status antes de tentar novamente."
                    .to_owned(),
            );
        }
        let mut status = RuntimeStatus {
            installed: true,
            version: metadata::text(&info["serverInfo"], "version", 120),
            ..RuntimeStatus::default()
        };
        metadata::account_from_status(&mut status, &account);
        Ok(status)
    })
    .await;
    let cleanup = process.cancel().await;
    usage::clear(&home);
    let mut status = result.map_err(|_| metadata::UNAVAILABLE)??;
    if cleanup.is_err() {
        status.error = Some(metadata::UNAVAILABLE.into());
    }
    status.preferences = app
        .state::<crate::system::SystemState>()
        .muse_preferences()?;
    cache.attempted = Some(Instant::now());
    cache.value = Some(status.clone());
    let _ = app.emit("muse-runtime:changed", &status);
    Ok(())
}

pub(crate) fn validate_selection(model: &str, reasoning: Option<&str>) -> Result<(), String> {
    if model.trim().is_empty()
        || model.len() > 256
        || model.starts_with('-')
        || model.chars().any(char::is_control)
    {
        return Err("Informe um modelo válido do Muse Code.".into());
    }
    if reasoning.is_some_and(|value| {
        !matches!(
            value,
            "none" | "minimal" | "low" | "medium" | "high" | "xhigh" | "max" | "ultra"
        )
    }) {
        return Err("O esforço do Muse não é reconhecido. Atualize o catálogo de modelos.".into());
    }
    Ok(())
}

pub(crate) fn validate_available_model(home: &Path, model: &str) -> Result<(), String> {
    validate_selection(model, None)?;
    let preferences = crate::system::backup_preferences(home)?.muse;
    if !preferences.enabled {
        return Err("Ative o Muse Code em Configurações → Provedores.".into());
    }
    if !preferences.allows(model) {
        return Err("Disponibilize este modelo do Muse Code em Configurações → Provedores.".into());
    }
    Ok(())
}

pub(crate) fn executable() -> Option<PathBuf> {
    metadata::executable()
}

pub(crate) async fn connect(cwd: &Path) -> Result<MuseProcess, String> {
    connect_with_home(&user_home()?, cwd).await
}

pub(crate) async fn connect_with_home(home: &Path, cwd: &Path) -> Result<MuseProcess, String> {
    connect_profile(home, cwd).await.map(|(process, _)| process)
}

async fn connect_with_info(cwd: &Path) -> Result<(MuseProcess, Value), String> {
    connect_profile(&user_home()?, cwd).await
}

fn user_home() -> Result<PathBuf, String> {
    std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
        .map(PathBuf::from)
        .filter(|path| path.is_absolute() && path.is_dir())
        .ok_or_else(|| {
            "Não foi possível localizar a pasta pessoal para o perfil Muse do Jarvis.".into()
        })
}

async fn connect_profile(home: &Path, cwd: &Path) -> Result<(MuseProcess, Value), String> {
    let executable = executable()
        .ok_or("Muse Code não encontrado. Instale o CLI oficial e atualize o status.")?;
    connect_config(runtime_config(executable, home, cwd)?).await
}

fn runtime_config(executable: PathBuf, home: &Path, cwd: &Path) -> Result<MuseSpawnConfig, String> {
    let mut config = MuseSpawnConfig::new(executable, cwd.to_owned());
    config.args = vec!["serve".into(), "--provider".into(), "meta".into()];
    session::configure(&mut config, home, &[])?;
    Ok(config)
}

async fn connect_config(config: MuseSpawnConfig) -> Result<(MuseProcess, Value), String> {
    let mut process = MuseProcess::spawn(config).map_err(|_| metadata::UNAVAILABLE)?;
    let result = tokio::time::timeout(
        Duration::from_secs(20),
        process.initialize(&json!({"capabilities":{"experimentalApi":true}})),
    )
    .await;
    match result {
        Ok(Ok(info)) if info["experimentalApi"] == true => Ok((process, info)),
        _ => {
            let _ = process.cancel().await;
            Err(metadata::UNSUPPORTED.into())
        }
    }
}

async fn reject_action(control: &MuseControl, event: &Value) -> Result<(), String> {
    if let Some(id) = event.get("id") {
        control
            .reject(id, -32601, "A consulta de conta não executa ações.")
            .await?;
    }
    Ok(())
}

fn login_flow(value: &Value, id: String, now: i64) -> Result<LoginFlow, String> {
    let verification_url = metadata::verification_url(
        value["verificationUrl"]
            .as_str()
            .ok_or(metadata::UNSUPPORTED)?,
    )?;
    let user_code = value["userCode"]
        .as_str()
        .filter(|code| {
            !code.trim().is_empty() && code.len() <= 256 && !code.chars().any(char::is_control)
        })
        .ok_or(metadata::UNSUPPORTED)?
        .to_owned();
    // The host publishes no TTL: this is Jarvis's local waiting deadline, not provider expiry.
    let expires_at = now.checked_add(600_000).ok_or(metadata::UNAVAILABLE)?;
    Ok(LoginFlow {
        id,
        verification_url,
        user_code,
        expires_at,
    })
}

async fn take_login(state: &MuseState, id: &str) -> Option<PendingLogin> {
    let mut active = state.login.lock().await;
    if active.as_ref().is_some_and(|pending| pending.id == id) {
        active.take()
    } else {
        None
    }
}

async fn apply_login(state: &MuseState, id: &str, status: RuntimeStatus) -> Option<RuntimeStatus> {
    let mut cache = state.runtime.lock().await;
    let mut active = state.login.lock().await;
    if active.as_ref().is_none_or(|pending| pending.id != id) {
        return None;
    }
    active.take();
    cache.attempted = Some(Instant::now());
    cache.value = Some(status.clone());
    Some(status)
}

async fn cancel_pending(pending: PendingLogin) -> Result<(), String> {
    let result = tokio::time::timeout(
        Duration::from_secs(5),
        pending.control.request("account/loginCancel", &json!({})),
    )
    .await;
    let _ = pending.stop.send(true);
    match result {
        Ok(Ok(_)) => Ok(()),
        _ => Err(
            "O Muse não confirmou o cancelamento. Atualize o status antes de tentar novamente."
                .into(),
        ),
    }
}

async fn cancel_login(state: &MuseState, id: &str) -> Result<(), String> {
    if let Some(pending) = take_login(state, id).await {
        cancel_pending(pending).await?;
    }
    Ok(())
}

async fn wait_login(
    process: &mut MuseProcess,
    stopped: &mut watch::Receiver<bool>,
) -> Result<bool, String> {
    let control = process.control();
    loop {
        tokio::select! {
            _ = stopped.changed() => return Ok(false),
            event = process.next_event() => match event {
                Ok(Some(event)) if event["method"] == "account/loginCompleted" => {
                    return match event["params"]["outcome"].as_str() {
                        Some("granted") => Ok(true),
                        Some("denied") => Err("O login do Muse foi recusado.".into()),
                        Some("expired") => Err("O código de login do Muse expirou. Inicie uma nova tentativa.".into()),
                        Some("cancelled") => Err("O login do Muse foi cancelado.".into()),
                        _ => Err("Não foi possível concluir o login do Muse. Atualize o status.".into()),
                    };
                }
                Ok(Some(event)) => reject_action(&control, &event).await?,
                _ => return Err(metadata::UNAVAILABLE.into()),
            }
        }
    }
}

async fn finish_login(
    app: tauri::AppHandle,
    state: MuseState,
    id: String,
    mut process: MuseProcess,
    mut stopped: watch::Receiver<bool>,
) {
    let outcome = tokio::time::timeout(
        Duration::from_secs(600),
        wait_login(&mut process, &mut stopped),
    )
    .await;
    let result = match outcome {
        Ok(Ok(false)) => {
            let _ = process.cancel().await;
            return;
        }
        Ok(Ok(true)) => Ok(refresh_granted_login(&mut process, metadata::discover()).await),
        Ok(Err(error)) => Err(error),
        Err(_) => {
            let _ = tokio::time::timeout(
                Duration::from_secs(5),
                process.control().request("account/loginCancel", &json!({})),
            )
            .await;
            Err("O prazo local de espera pelo login terminou. Atualize o status ou inicie uma nova tentativa.".into())
        }
    };
    let mut status = match result {
        Ok(status) => status,
        Err(error) => {
            let mut status =
                state
                    .runtime
                    .lock()
                    .await
                    .value
                    .clone()
                    .unwrap_or_else(|| RuntimeStatus {
                        installed: true,
                        ..RuntimeStatus::default()
                    });
            status.error = Some(error);
            status
        }
    };
    if let Ok(preferences) = app.state::<crate::system::SystemState>().muse_preferences() {
        status.preferences = preferences;
    }
    if let Some(status) = apply_login(&state, &id, status).await {
        let _ = app.emit("muse-runtime:changed", &status);
    }
    let _ = process.cancel().await;
}

async fn refresh_granted_login(
    process: &mut MuseProcess,
    discovery: impl std::future::Future<Output = RuntimeStatus>,
) -> RuntimeStatus {
    let cleanup = process.cancel().await;
    // The login host may retain its pre-login catalog; a new host reads the persisted account.
    let mut status = discovery.await;
    if cleanup.is_err() {
        status.error = Some(metadata::UNAVAILABLE.into());
    }
    status
}

#[cfg(test)]
mod tests;
