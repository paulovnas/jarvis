//! Account-scoped, read-only quota probes. Secrets and wire responses stay in Rust.
use super::{
    antigravity, current_time_millis, CodexCredential, OpenAiCodexState, ProviderError, UsageAlert,
    UsageAlertWindow,
};
use crate::persistence::{AppState, ProviderAccountRecord};
use serde::Serialize;
use serde_json::Value;
use std::{
    collections::HashMap,
    io::Read,
    path::Path,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

mod parse;
#[cfg(test)]
mod tests;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageWindow {
    id: String,
    group: String,
    third_party: bool,
    label: String,
    duration_seconds: Option<f64>,
    remaining_percent: Option<f64>,
    resets_at: Option<i64>,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResetCredits {
    available_count: u64,
    expirations: Vec<Option<i64>>,
    details_available: bool,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountUsage {
    alias: String,
    fetched_at: Option<i64>,
    email: Option<String>,
    plan: Option<String>,
    windows: Vec<UsageWindow>,
    reset_credits: Option<ResetCredits>,
    error: Option<String>,
}

#[derive(Debug, PartialEq, Eq)]
struct AlertNotice {
    window_id: String,
    resets_at: i64,
    threshold: u8,
    title: String,
    body: String,
}

fn alert_notices(
    record: &ProviderAccountRecord,
    usage: &AccountUsage,
    now: i64,
) -> Vec<AlertNotice> {
    let Some(alert) = UsageAlert::from_record(record) else {
        return vec![];
    };
    if usage.error.is_some() || usage.fetched_at.is_none() {
        return vec![];
    }
    let expected_duration = match alert.window {
        UsageAlertWindow::FiveHour => 18_000.0,
        UsageAlertWindow::Weekly => 604_800.0,
    };
    let provider = if record.provider_kind == "antigravity" {
        "Antigravity"
    } else {
        "OpenAI Codex"
    };
    let alias = record
        .alias
        .strip_prefix("openai-codex-")
        .or_else(|| record.alias.strip_prefix("antigravity-"))
        .unwrap_or(&record.alias);
    usage
        .windows
        .iter()
        .filter(|window| {
            window
                .duration_seconds
                .is_some_and(|duration| (duration - expected_duration).abs() < 1.0)
                && window
                    .remaining_percent
                    .is_some_and(|remaining| remaining <= f64::from(alert.remaining_percent))
                && window.resets_at.is_some_and(|reset| reset > now)
                && (record.provider_kind != "openai-codex" || window.group == "Codex")
                && (!window.third_party || record.show_third_party_usage)
        })
        .filter_map(|window| {
            let reset = window.resets_at?;
            let remaining = window.remaining_percent?.round().clamp(0.0, 100.0) as u8;
            Some(AlertNotice {
                window_id: window.id.clone(),
                resets_at: reset,
                threshold: alert.remaining_percent,
                title: format!("Limite do {provider}"),
                body: format!(
                    "{alias} · {} · {}: restam {remaining}% do limite.",
                    window.group, window.label
                ),
            })
        })
        .collect()
}

fn claim_alert_delivery(
    connection: &rusqlite::Connection,
    alias: &str,
    notice: &AlertNotice,
) -> Result<bool, ProviderError> {
    connection
        .execute(
            "INSERT INTO provider_usage_alert_deliveries(alias, window_id, resets_at, threshold) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(alias, window_id) DO UPDATE SET resets_at=excluded.resets_at, threshold=excluded.threshold
             WHERE provider_usage_alert_deliveries.resets_at <> excluded.resets_at OR provider_usage_alert_deliveries.threshold <> excluded.threshold",
            rusqlite::params![alias, notice.window_id, notice.resets_at, notice.threshold],
        )
        .map(|changed| changed == 1)
        .map_err(|_| ProviderError::database())
}

#[derive(Default)]
struct Cached {
    attempted: Option<Instant>,
    value: Option<AccountUsage>,
}
#[derive(Default)]
pub(super) struct UsageCache(Mutex<HashMap<String, Arc<Mutex<Cached>>>>);
impl UsageCache {
    pub(super) fn invalidate(&self, alias: &str) {
        if let Ok(mut entries) = self.0.lock() {
            let prefix = format!("{alias}/");
            entries.retain(|key, _| !key.starts_with(&prefix));
        }
    }

    fn entry(&self, record: &ProviderAccountRecord) -> Result<Arc<Mutex<Cached>>, ProviderError> {
        let key = format!(
            "{}/{}/{}",
            record.alias, record.account_id, record.created_at
        );
        let mut entries = self.0.lock().map_err(|_| ProviderError::internal())?;
        Ok(entries.entry(key).or_default().clone())
    }
}

fn unavailable() -> ProviderError {
    ProviderError::new(
        "usage_unavailable",
        "Não foi possível consultar os limites agora.",
    )
}
fn response_json(response: reqwest::blocking::Response) -> Result<Value, ProviderError> {
    if !response.status().is_success() {
        return Err(unavailable());
    }
    let mut bytes = Vec::new();
    response
        .take(2_000_001)
        .read_to_end(&mut bytes)
        .map_err(|_| unavailable())?;
    if bytes.len() > 2_000_000 {
        return Err(unavailable());
    }
    serde_json::from_slice(&bytes).map_err(|_| unavailable())
}

fn codex(
    client: &reqwest::blocking::Client,
    base: &str,
    credential: &CodexCredential,
    usage: &mut AccountUsage,
    now: i64,
) -> Result<(), ProviderError> {
    let get = |path: &str| -> Result<Value, ProviderError> {
        response_json(
            client
                .get(format!("{base}{path}"))
                .bearer_auth(&credential.access)
                .header("ChatGPT-Account-Id", &credential.account_id)
                .header(
                    "User-Agent",
                    format!("codex_cli_rs/{}", super::OPENAI_CODEX_CLIENT_VERSION),
                )
                .send()
                .map_err(|_| unavailable())?,
        )
    };
    let data = get("/wham/usage")?;
    usage.windows = parse::codex_windows(&data, now);
    usage.plan = data["plan_type"]
        .as_str()
        .map(str::to_owned)
        .or(usage.plan.take());
    usage.reset_credits = parse::credit_count(&data);
    if usage
        .reset_credits
        .as_ref()
        .is_some_and(|credits| credits.available_count > 0)
    {
        usage.reset_credits = get("/wham/rate-limit-reset-credits")
            .ok()
            .and_then(|details| parse::credit_details(&details, now))
            .or(usage.reset_credits.take());
    }
    Ok(())
}

fn google_request(
    client: &reqwest::blocking::Client,
    endpoints: &[&str],
    path: &str,
    credential: &CodexCredential,
) -> Result<Value, ProviderError> {
    for endpoint in endpoints {
        let response = client
            .post(format!("{endpoint}/v1internal:{path}"))
            .bearer_auth(&credential.access)
            .header("User-Agent", antigravity::user_agent())
            .json(&if path == "loadCodeAssist" {
                serde_json::json!({"cloudaicompanionProject":credential.project_id,"metadata":{"ideType":"ANTIGRAVITY"}})
            } else { serde_json::json!({"project":credential.project_id}) })
            .send();
        if let Ok(response) = response {
            let transient =
                response.status().is_server_error() || response.status().as_u16() == 429;
            match response_json(response) {
                Ok(data) => return Ok(data),
                Err(error) if !transient => return Err(error),
                _ => {}
            }
        }
    }
    Err(unavailable())
}
fn google(
    client: &reqwest::blocking::Client,
    endpoints: &[&str],
    credential: &CodexCredential,
    usage: &mut AccountUsage,
) -> Result<(), ProviderError> {
    if let Ok(data) = google_request(client, endpoints, "retrieveUserQuotaSummary", credential) {
        usage.windows = parse::google_summary(&data);
    }
    if usage.windows.is_empty() {
        let data = google_request(client, endpoints, "fetchAvailableModels", credential)?;
        usage.windows = parse::google_models(&data);
    }
    if usage.windows.is_empty() {
        return Err(unavailable());
    }
    if let Ok(profile) = google_request(client, endpoints, "loadCodeAssist", credential) {
        usage.plan = parse::google_plan(&profile).or(usage.plan.take());
    }
    Ok(())
}

impl OpenAiCodexState {
    fn usage_credential(
        &self,
        state: &AppState,
        home: &Path,
        record: &ProviderAccountRecord,
    ) -> Result<CodexCredential, ProviderError> {
        let _guard = self
            .manager
            .credentials_guard
            .lock()
            .map_err(|_| ProviderError::internal())?;
        let records = state
            .list_provider_accounts(home)
            .map_err(|_| ProviderError::database())?;
        if !records
            .iter()
            .any(|current| current == record && current.enabled)
        {
            return Err(unavailable());
        }
        let mut credential = self.manager.secret_store.load(&record.alias).map_err(|_| {
            ProviderError::new(
                "credential_missing",
                "Reconecte a conta para consultar os limites.",
            )
        })?;
        if credential.account_id != record.account_id
            || (record.provider_kind == "antigravity") != credential.project_id.is_some()
        {
            return Err(unavailable());
        }
        if credential.expires <= current_time_millis()? + 60_000 {
            credential = super::refresh_credential(&self.manager.endpoints, &credential)?;
            self.manager
                .secret_store
                .store(&record.alias, &credential)
                .map_err(|_| ProviderError::internal())?;
        }
        Ok(credential)
    }

    fn account_usage(
        &self,
        state: &AppState,
        home: &Path,
        alias: &str,
    ) -> Result<AccountUsage, ProviderError> {
        super::validate_provider_alias(alias).map_err(|_| ProviderError::invalid_alias())?;
        let record = state
            .list_provider_accounts(home)
            .map_err(|_| ProviderError::database())?
            .into_iter()
            .find(|record| {
                record.alias == alias && record.enabled && record.provider_kind != "custom"
            })
            .ok_or_else(unavailable)?;
        let entry = self.manager.usage_cache.entry(&record)?;
        // Single-flight only this account; other accounts and inference can proceed.
        let mut cached = entry.lock().map_err(|_| ProviderError::internal())?;
        if let Some(value) = cached.value.as_ref().filter(|_| {
            cached
                .attempted
                .is_some_and(|last| last.elapsed() < Duration::from_secs(60))
        }) {
            return Ok(value.clone());
        }
        let mut usage = AccountUsage {
            alias: alias.into(),
            fetched_at: None,
            email: None,
            plan: None,
            windows: vec![],
            reset_credits: None,
            error: None,
        };
        let result = (|| {
            let credential = self.usage_credential(state, home, &record)?;
            usage.email = credential.email.clone();
            usage.plan = credential.plan_type.clone();
            let client = reqwest::blocking::Client::builder()
                .timeout(Duration::from_secs(10))
                .connect_timeout(Duration::from_secs(5))
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .map_err(|_| unavailable())?;
            let now = current_time_millis()?;
            if record.provider_kind == "antigravity" {
                google(&client, &antigravity::ENDPOINTS, &credential, &mut usage)?;
            } else {
                codex(
                    &client,
                    super::OPENAI_CODEX_BASE_URL,
                    &credential,
                    &mut usage,
                    now,
                )?;
            }
            usage.fetched_at = Some(now);
            Ok::<_, ProviderError>(())
        })();
        if let Err(error) = result {
            usage = cached.value.clone().unwrap_or(usage);
            usage.error = Some(error.message);
        }
        cached.attempted = Some(Instant::now());
        cached.value = Some(usage.clone());
        Ok(usage)
    }
}

#[tauri::command]
pub async fn get_provider_usage(
    app: tauri::AppHandle,
    persistence_state: tauri::State<'_, AppState>,
    oauth_state: tauri::State<'_, OpenAiCodexState>,
    alias: String,
) -> Result<AccountUsage, ProviderError> {
    let home = super::home_dir(&app)?;
    let state = persistence_state.inner().clone();
    let oauth = oauth_state.inner().clone();
    let query_state = state.clone();
    let query_home = home.clone();
    let (usage, record) = tauri::async_runtime::spawn_blocking(move || {
        let usage = oauth.account_usage(&query_state, &query_home, &alias)?;
        let record = query_state
            .list_provider_accounts(&query_home)
            .map_err(|_| ProviderError::database())?
            .into_iter()
            .find(|record| record.alias == alias)
            .ok_or_else(unavailable)?;
        Ok::<_, ProviderError>((usage, record))
    })
    .await
    .map_err(|_| ProviderError::internal())??;
    let now = current_time_millis()?;
    let notices = alert_notices(&record, &usage, now);
    if crate::system::notifications_enabled(&app) && !notices.is_empty() {
        let account = record.alias.clone();
        let claimed = tauri::async_runtime::spawn_blocking(move || {
            state.with_connection(&home, |connection| {
                notices
                    .into_iter()
                    .filter_map(|notice| {
                        match claim_alert_delivery(connection, &account, &notice) {
                            Ok(true) => Some(Ok(notice)),
                            Ok(false) => None,
                            Err(error) => Some(Err(error)),
                        }
                    })
                    .collect::<Result<Vec<_>, _>>()
            })
        })
        .await
        .map_err(|_| ProviderError::internal())??;
        for notice in claimed {
            crate::system::notify_usage_limit(&app, &notice.title, &notice.body);
        }
    }
    Ok(usage)
}

pub(super) fn save_visibility(
    connection: &rusqlite::Connection,
    alias: &str,
    show_usage: bool,
    third_party: bool,
) -> Result<(), ProviderError> {
    let changed = connection
        .execute(
            "UPDATE provider_accounts SET show_usage=?2, show_third_party_usage=?3 WHERE alias=?1",
            rusqlite::params![alias, show_usage, third_party],
        )
        .map_err(|_| ProviderError::database())?;
    if changed != 1 {
        return Err(unavailable());
    }
    Ok(())
}

#[tauri::command]
pub async fn set_provider_usage_visibility(
    app: tauri::AppHandle,
    persistence_state: tauri::State<'_, AppState>,
    alias: String,
    show_usage: bool,
    show_third_party_usage: bool,
) -> Result<(), ProviderError> {
    super::validate_provider_alias(&alias).map_err(|_| ProviderError::invalid_alias())?;
    let home = super::home_dir(&app)?;
    let state = persistence_state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        state.with_connection(&home, |connection| {
            save_visibility(connection, &alias, show_usage, show_third_party_usage)
        })
    })
    .await
    .map_err(|_| ProviderError::internal())?
}

pub(super) fn save_alert(
    connection: &rusqlite::Connection,
    alias: &str,
    alert: Option<UsageAlert>,
) -> Result<(), ProviderError> {
    if alert.is_some_and(|value| !(1..=100).contains(&value.remaining_percent)) {
        return Err(ProviderError::new(
            "invalid_usage_alert",
            "A porcentagem do alerta deve ficar entre 1% e 100%.",
        ));
    }
    let window = alert.map(|value| value.window.as_storage());
    let threshold = alert.map(|value| value.remaining_percent);
    let transaction = connection
        .unchecked_transaction()
        .map_err(|_| ProviderError::database())?;
    let changed = transaction
        .execute(
            "UPDATE provider_accounts SET usage_alert_window=?2, usage_alert_threshold=?3 WHERE alias=?1 AND provider_kind IN ('openai-codex', 'antigravity')",
            rusqlite::params![alias, window, threshold],
        )
        .map_err(|_| ProviderError::database())?;
    if changed != 1 {
        return Err(unavailable());
    }
    if alert.is_none() {
        transaction
            .execute(
                "DELETE FROM provider_usage_alert_deliveries WHERE alias=?1",
                [alias],
            )
            .map_err(|_| ProviderError::database())?;
    }
    transaction.commit().map_err(|_| ProviderError::database())
}

#[tauri::command]
pub async fn set_provider_usage_alert(
    app: tauri::AppHandle,
    persistence_state: tauri::State<'_, AppState>,
    alias: String,
    alert: Option<UsageAlert>,
) -> Result<(), ProviderError> {
    super::validate_provider_alias(&alias).map_err(|_| ProviderError::invalid_alias())?;
    let home = super::home_dir(&app)?;
    let state = persistence_state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        state.with_connection(&home, |connection| save_alert(connection, &alias, alert))
    })
    .await
    .map_err(|_| ProviderError::internal())?
}
