use serde::Serialize;
use serde_json::{json, Value};
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use super::{transport::MuseProcess, MuseState};

pub(super) const UNSUPPORTED: &str = "Esta versão do Muse Code não oferece a integração de conta. Atualize o CLI para entrar no perfil Muse privado do Jarvis.";
pub(super) const UNAVAILABLE: &str =
    "Não foi possível consultar o Muse Code. Verifique o CLI e atualize o status.";
pub(super) const SUBSCRIPTION_REQUIRED: &str = "O perfil Muse do Jarvis está usando uma chave de API. Conecte sua assinatura neste perfil; este executor não usa cobrança por API.";

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Model {
    pub id: String,
    pub name: String,
    pub description: String,
    pub reasoning_levels: Vec<String>,
    pub default_reasoning: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RuntimeStatus {
    pub preferences: crate::claude::ProviderPreferences,
    pub installed: bool,
    pub authenticated: bool,
    pub version: Option<String>,
    pub models: Vec<Model>,
    pub error: Option<String>,
    pub auth_method: Option<String>,
    pub email: Option<String>,
    pub subscription_type: Option<String>,
}

#[derive(Default)]
pub(super) struct Cache {
    pub(super) attempted: Option<Instant>,
    pub(super) value: Option<RuntimeStatus>,
}

pub(super) async fn cached(state: &MuseState, refresh: bool) -> RuntimeStatus {
    let mut cache = state.runtime.lock().await;
    if !refresh
        && cache
            .attempted
            .is_some_and(|at| at.elapsed() < Duration::from_secs(60))
    {
        if let Some(status) = &cache.value {
            return status.clone();
        }
    }
    let status = discover().await;
    cache.attempted = Some(Instant::now());
    cache.value = Some(status.clone());
    status
}

pub(super) fn executable() -> Option<PathBuf> {
    let name = if cfg!(windows) { "muse.exe" } else { "muse" };
    let mut directories: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|path| {
            std::env::split_paths(&path)
                .filter(|path| path.is_absolute())
                .collect()
        })
        .unwrap_or_default();
    if let Some(home) = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }) {
        directories.push(PathBuf::from(home).join(".local/bin"));
    }
    #[cfg(unix)]
    directories.extend([
        PathBuf::from("/usr/local/bin"),
        PathBuf::from("/opt/homebrew/bin"),
    ]);
    directories
        .into_iter()
        .map(|directory| directory.join(name))
        .find(|path| {
            std::fs::metadata(path).is_ok_and(|metadata| {
                if !metadata.is_file() {
                    return false;
                }
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    metadata.permissions().mode() & 0o111 != 0
                }
                #[cfg(not(unix))]
                {
                    true
                }
            })
        })
}

pub(super) fn account_from_status(status: &mut RuntimeStatus, value: &Value) {
    status.auth_method = value["state"]
        .as_str()
        .filter(|state| matches!(*state, "accountLogin" | "apiKey" | "envKey" | "loggedOut"))
        .map(str::to_owned);
    status.authenticated = value["state"] == "accountLogin";
    status.email = if status.authenticated {
        text(value, "label", 320)
    } else {
        None
    };
    status.subscription_type = None;
    status.models.clear();
    status.error = match value["state"].as_str() {
        Some("accountLogin") => None,
        Some("apiKey" | "envKey") => Some(SUBSCRIPTION_REQUIRED.into()),
        Some("loggedOut") => None,
        _ => Some("O Muse Code retornou um estado de conta desconhecido. Atualize o CLI.".into()),
    };
}

pub(super) fn models_from_list(value: &Value) -> Vec<Model> {
    let mut seen = HashSet::new();
    value["models"]
        .as_array()
        .into_iter()
        .flatten()
        .take(256)
        .filter_map(|entry| {
            let id = entry["modelId"].as_str()?;
            if entry["providerId"] != "meta"
                || super::validate_selection(id, None).is_err()
                || !seen.insert(id.to_owned())
            {
                return None;
            }
            let mut reasoning_levels = Vec::new();
            for level in entry["variants"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
            {
                if super::validate_selection(id, Some(level)).is_ok()
                    && !reasoning_levels.iter().any(|existing| existing == level)
                {
                    reasoning_levels.push(level.to_owned());
                }
            }
            let default_reasoning = text(entry, "defaultReasoningEffort", 20)
                .filter(|level| reasoning_levels.contains(level));
            Some(Model {
                id: id.to_owned(),
                name: text(entry, "displayLabel", 200).unwrap_or_else(|| id.to_owned()),
                description: text(entry, "description", 1000).unwrap_or_default(),
                reasoning_levels,
                default_reasoning,
            })
        })
        .collect()
}

pub(super) fn text(value: &Value, key: &str, limit: usize) -> Option<String> {
    value[key]
        .as_str()
        .filter(|text| !text.trim().is_empty())
        .map(|text| {
            text.chars()
                .filter(|ch| !ch.is_control())
                .take(limit)
                .collect()
        })
}

pub(super) fn verification_url(value: &str) -> Result<String, String> {
    let url = reqwest::Url::parse(value).map_err(|_| "Endereço de login do Muse inválido.")?;
    if value.len() > 4096
        || value.chars().any(char::is_control)
        || value.trim() != value
        || url.scheme() != "https"
        || url.host_str() != Some("auth.meta.com")
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port_or_known_default() != Some(443)
    {
        return Err("O Muse retornou um endereço de login não autorizado.".into());
    }
    Ok(url.to_string())
}

pub(super) async fn discover() -> RuntimeStatus {
    let mut status = RuntimeStatus::default();
    if executable().is_none() {
        status.error =
            Some("Muse Code não encontrado. Instale o CLI oficial e atualize o status.".into());
        return status;
    }
    status.installed = true;
    let Ok(directory) = tempfile::tempdir() else {
        status.error = Some(UNAVAILABLE.into());
        return status;
    };
    let (mut process, info) = match super::connect_with_info(directory.path()).await {
        Ok(connection) => connection,
        Err(error) => {
            status.error = Some(error);
            return status;
        }
    };
    let version = text(&info["serverInfo"], "version", 120);
    status.version = version.clone();
    let result =
        tokio::time::timeout(Duration::from_secs(25), read_status(&mut process, version)).await;
    match result {
        Ok(Ok(discovered)) => status = discovered,
        Ok(Err(error)) => status.error = Some(error),
        Err(_) => status.error = Some(UNAVAILABLE.into()),
    }
    if process.cancel().await.is_err() {
        status.error = Some(UNAVAILABLE.into());
    }
    status
}

pub(super) async fn read_status(
    process: &mut MuseProcess,
    version: Option<String>,
) -> Result<RuntimeStatus, String> {
    read_status_at(process, version, &super::user_home()?).await
}

pub(super) async fn read_account(process: &mut MuseProcess, home: &Path) -> Result<Value, String> {
    let control = process.control();
    let params = json!({});
    let query = control.request("account/read", &params);
    tokio::pin!(query);
    let mut pending_usage: Option<(i64, Value)> = None;
    loop {
        tokio::select! {
            biased;
            event = process.next_event() => match event {
                Ok(Some(event)) if event["method"] == "usage/changed" => {
                    let params = &event["params"];
                    if let Some(at) = params["observedAtMs"].as_i64().filter(|at| *at > 0) {
                        if pending_usage.as_ref().is_none_or(|(previous, _)| *previous < at) {
                            pending_usage = Some((at, params.clone()));
                        }
                    }
                }
                Ok(Some(event)) => super::reject_action(&control, &event).await?,
                _ => return Err(UNAVAILABLE.into()),
            },
            result = &mut query => {
                let account = result.map_err(|_| UNSUPPORTED)?;
                super::usage::observed(home, &account);
                if let Some((_, usage)) = pending_usage {
                    let _ = super::usage::observe(home, &account, &usage);
                }
                return Ok(account);
            }
        }
    }
}

pub(super) async fn read_status_at(
    process: &mut MuseProcess,
    version: Option<String>,
    home: &Path,
) -> Result<RuntimeStatus, String> {
    let account = read_account(process, home).await?;
    let control = process.control();
    let query = async {
        let mut status = RuntimeStatus {
            installed: true,
            version,
            ..RuntimeStatus::default()
        };
        account_from_status(&mut status, &account);
        if status.authenticated {
            match control.request("model/list", &json!({})).await {
                Ok(catalog) if catalog["models"].is_array() => status.models = models_from_list(&catalog),
                _ => status.error = Some("A conta Muse está conectada, mas o catálogo de modelos está indisponível. Atualize o status.".into()),
            }
            let direct = if let Ok(value) = control.request("usage/read", &json!({})).await {
                let _ = super::usage::observe(home, &account, &value);
                super::usage::parse(&value).ok()
            } else {
                None
            };
            status.subscription_type = super::usage::latest(home, &account)
                .or(direct)
                .and_then(|usage| usage.plan);
        }
        Ok(status)
    };
    tokio::pin!(query);
    loop {
        tokio::select! {
            biased;
            event = process.next_event() => match event {
                Ok(Some(event)) if event["method"] == "usage/changed" => { let _ = super::usage::observe(home, &account, &event["params"]); }
                Ok(Some(event)) => super::reject_action(&control, &event).await?,
                _ => return Err(UNAVAILABLE.into()),
            },
            result = &mut query => return result,
        }
    }
}
