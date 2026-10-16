//! Last-observed quotas from the official Muse host; credentials never leave the CLI.
use super::{metadata, MuseState};
use crate::openai_codex::usage::{AccountUsage, UsageWindow};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{LazyLock, Mutex},
    time::{Duration, Instant},
};

const UNAVAILABLE: &str =
    "Não foi possível consultar os limites do Muse Code. Atualize o CLI e tente novamente.";

const NO_OBSERVATION: &str = "O Muse ainda não informou cotas para esta conta. Os limites aparecerão após uma observação do host.";
const MAX_PROFILES: usize = 32;

struct Observation {
    account: Option<String>,
    usage: Option<AccountUsage>,
    cleared_at: i64,
    touched: Instant,
}

static OBSERVATIONS: LazyLock<Mutex<HashMap<PathBuf, Observation>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn account_label(account: &Value) -> Option<String> {
    if account["state"] != "accountLogin" {
        return None;
    }
    account["label"]
        .as_str()
        .filter(|label| {
            !label.trim().is_empty()
                && label.len() <= 1280
                && label.chars().count() <= 320
                && !label.chars().any(char::is_control)
        })
        .map(str::to_owned)
}

fn entry(entries: &mut HashMap<PathBuf, Observation>, profile: PathBuf) -> &mut Observation {
    if !entries.contains_key(&profile) && entries.len() >= MAX_PROFILES {
        if let Some(oldest) = entries
            .iter()
            .min_by_key(|(_, value)| value.touched)
            .map(|(key, _)| key.clone())
        {
            entries.remove(&oldest);
        }
    }
    let entry = entries.entry(profile).or_insert_with(|| Observation {
        account: None,
        usage: None,
        cleared_at: 0,
        touched: Instant::now(),
    });
    entry.touched = Instant::now();
    entry
}

/// Called only after the host has confirmed its current account with account/read.
pub(crate) fn observed(home: &Path, account: &Value) -> Option<AccountUsage> {
    if !home.is_absolute() {
        return None;
    }
    let Some(label) = account_label(account) else {
        clear(home);
        return None;
    };
    let mut entries = OBSERVATIONS.lock().ok()?;
    let entry = entry(&mut entries, crate::data_dir::root(home).join("muse"));
    if entry.account.as_deref() != Some(label.as_str()) {
        entry.account = Some(label);
        entry.usage = None;
    }
    entry.usage.clone()
}

/// Accepts usage/changed params or a usage/read result; it never fetches or mints credentials.
pub(crate) fn observe(home: &Path, account: &Value, value: &Value) -> Result<(), String> {
    if !home.is_absolute() {
        return Err(UNAVAILABLE.into());
    }
    let Some(label) = account_label(account) else {
        return Ok(());
    };
    let mut usage = parse(value)?;
    let Some(at) = usage.fetched_at else {
        return Ok(());
    };
    if usage.windows.is_empty() {
        return Err(UNAVAILABLE.into());
    }
    let mut entries = OBSERVATIONS.lock().map_err(|_| UNAVAILABLE)?;
    let entry = entry(&mut entries, crate::data_dir::root(home).join("muse"));
    if entry.account.is_none() && entry.cleared_at == 0 {
        entry.account = Some(label.clone());
    }
    // An old host cannot rebind a changed account or repopulate a logout tombstone.
    if entry.account.as_deref() != Some(label.as_str())
        || at <= entry.cleared_at
        || entry
            .usage
            .as_ref()
            .and_then(|usage| usage.fetched_at)
            .is_some_and(|previous| previous >= at)
    {
        return Ok(());
    }
    usage.email = Some(label);
    entry.usage = Some(usage);
    Ok(())
}

pub(crate) fn clear(home: &Path) {
    if !home.is_absolute() {
        return;
    }
    if let Ok(mut entries) = OBSERVATIONS.lock() {
        let entry = entry(&mut entries, crate::data_dir::root(home).join("muse"));
        entry.account = None;
        entry.usage = None;
        entry.cleared_at = crate::openai_codex::current_time_millis().unwrap_or(i64::MAX);
    }
}

pub(super) fn latest(home: &Path, account: &Value) -> Option<AccountUsage> {
    let label = account_label(account)?;
    let entries = OBSERVATIONS.lock().ok()?;
    let entry = entries.get(&crate::data_dir::root(home).join("muse"))?;
    if entry.account.as_deref() == Some(label.as_str()) {
        entry.usage.clone()
    } else {
        None
    }
}

pub(super) async fn current(state: &MuseState) -> AccountUsage {
    let _single_flight = state.usage.lock().await;
    query().await.unwrap_or_else(|error| {
        let mut usage = empty();
        usage.error = Some(error);
        usage
    })
}

fn empty() -> AccountUsage {
    AccountUsage {
        alias: "Muse Code".into(),
        fetched_at: None,
        email: None,
        plan: None,
        windows: vec![],
        reset_credits: None,
        error: None,
    }
}

async fn query() -> Result<AccountUsage, String> {
    let home = super::user_home()?;
    let directory = tempfile::tempdir().map_err(|_| UNAVAILABLE)?;
    let mut process = super::connect(directory.path()).await?;
    let result =
        tokio::time::timeout(Duration::from_secs(25), read_current(&mut process, &home)).await;
    let cleanup = process.cancel().await;
    let mut usage = result.map_err(|_| UNAVAILABLE)??;
    if cleanup.is_err() {
        usage.error = Some(UNAVAILABLE.into())
    }
    Ok(usage)
}

pub(super) async fn read_current(
    process: &mut super::transport::MuseProcess,
    home: &Path,
) -> Result<AccountUsage, String> {
    let account = metadata::read_account(process, home).await?;
    let mut status = metadata::RuntimeStatus::default();
    metadata::account_from_status(&mut status, &account);
    if !status.authenticated {
        return Err(status
            .error
            .unwrap_or_else(|| "Conecte sua assinatura no perfil Muse privado do Jarvis.".into()));
    }
    let control = process.control();
    let query = async {
        let response = control.request("usage/read", &json!({})).await;
        if let Ok(value) = &response {
            let _ = observe(home, &account, value);
        }
        let mut usage = latest(home, &account)
            .or_else(|| {
                // A label-less account may display this host's response, but never reuse another host's data.
                if account_label(&account).is_none() {
                    response
                        .as_ref()
                        .ok()
                        .and_then(|value| parse(value).ok())
                        .filter(|usage| usage.fetched_at.is_some())
                } else {
                    None
                }
            })
            .unwrap_or_else(empty);
        usage.email = metadata::text(&account, "label", 320);
        if response.is_err() {
            usage.error = Some(UNAVAILABLE.into());
        } else if usage.fetched_at.is_none() {
            usage.error = Some(NO_OBSERVATION.into());
        }
        Ok(usage)
    };
    tokio::pin!(query);
    loop {
        tokio::select! {
            biased;
            event = process.next_event() => match event {
                Ok(Some(event)) if event["method"] == "usage/changed" => { let _ = observe(home, &account, &event["params"]); }
                Ok(Some(event)) => super::reject_action(&control, &event).await?,
                _ => return Err(UNAVAILABLE.to_owned()),
            },
            result = &mut query => return result,
        }
    }
}

pub(super) fn parse(value: &Value) -> Result<AccountUsage, String> {
    let mut usage = empty();
    let Some(value) = value
        .get("usage")
        .or_else(|| value.get("observedAtMs").map(|_| value))
    else {
        return Ok(usage);
    };
    usage.fetched_at = Some(
        value["observedAtMs"]
            .as_i64()
            .filter(|at| *at > 0)
            .ok_or(UNAVAILABLE)?,
    );
    usage.plan = metadata::text(value, "tier", 100)
        .filter(|tier| !tier.trim().bytes().all(|byte| byte.is_ascii_digit()));
    for (key, label, duration) in [
        ("window", "Janela atual", None),
        ("weekly", "7d", Some(604_800.0)),
    ] {
        let Some(bucket) = value.get(key) else {
            continue;
        };
        if !bucket.is_object() {
            return Err(UNAVAILABLE.into());
        }
        let remaining_percent = bucket["usedPercent"]
            .as_u64()
            .map(|used| (100.0 - used as f64).clamp(0.0, 100.0));
        let resets_at = bucket["resetsAtMs"].as_i64().filter(|at| *at > 0);
        let duration_seconds = duration.or_else(|| {
            bucket["windowDurationMins"]
                .as_u64()
                .filter(|minutes| *minutes > 0)
                .map(|minutes| minutes as f64 * 60.0)
        });
        if remaining_percent.is_none() && resets_at.is_none() {
            continue;
        }
        usage.windows.push(UsageWindow {
            id: format!("muse/{key}"),
            group: "Muse Code".into(),
            third_party: false,
            label: if key == "window" && duration_seconds == Some(18_000.0) {
                "5h".into()
            } else {
                label.into()
            },
            duration_seconds,
            remaining_percent,
            resets_at,
        });
    }
    Ok(usage)
}
