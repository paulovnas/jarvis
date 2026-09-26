//! Read-only native control request: Claude retains its credentials and fetches its own quotas.
use super::{metadata, transport::command_for, ClaudeProcess, ClaudeState, RunOptions};
use crate::openai_codex::usage::{AccountUsage, UsageWindow};
use serde_json::{json, Value};
use std::time::{Duration, Instant};

const UNAVAILABLE: &str = "Não foi possível consultar os limites do Claude Code. Verifique a conexão e atualize o CLI se necessário.";
const UNSUPPORTED: &str = "A conexão atual do Claude Code não informa cotas de assinatura. Verifique o login no CLI; contas via API ou serviços externos podem não fornecer esses limites.";

#[derive(Default)]
pub(super) struct Cache {
    attempted: Option<Instant>,
    value: Option<AccountUsage>,
}

impl Cache {
    fn fresh(&self) -> Option<AccountUsage> {
        self.value.clone().filter(|_| {
            self.attempted
                .is_some_and(|at| at.elapsed() < Duration::from_secs(60))
        })
    }

    fn store(&mut self, result: Result<AccountUsage, String>) -> AccountUsage {
        let usage = match result {
            Ok(usage) => usage,
            Err(error) => {
                let mut usage = self.value.clone().unwrap_or_else(empty);
                usage.error = Some(error);
                usage
            }
        };
        self.attempted = Some(Instant::now());
        self.value = Some(usage.clone());
        usage
    }
}

pub(super) async fn cached(state: &ClaudeState) -> AccountUsage {
    // A single-flight probe for this one local provider, independent of running conversations.
    let mut cache = state.usage.lock().await;
    if let Some(usage) = cache.fresh() {
        return usage;
    }
    cache.store(query().await)
}

fn empty() -> AccountUsage {
    AccountUsage {
        alias: "Claude Code".into(),
        fetched_at: None,
        email: None,
        plan: None,
        windows: vec![],
        reset_credits: None,
        error: None,
    }
}

async fn query() -> Result<AccountUsage, String> {
    let executable = metadata::executable().ok_or(UNAVAILABLE)?;
    let directory = tempfile::tempdir().map_err(|_| UNAVAILABLE)?;
    let (command, files) = command_for(
        &executable,
        &RunOptions {
            cwd: directory.path().to_owned(),
            session_id: String::new(),
            resume: false,
            model: "default".into(),
            effort: None,
            append_system_prompt: String::new(),
            mcp_servers: json!({}),
        },
        true,
    )?;
    let mut process = ClaudeProcess::spawn_command(command, files).map_err(|_| UNAVAILABLE)?;
    let result = tokio::time::timeout(Duration::from_secs(25), read_usage(&mut process)).await;
    // Always reap this isolated probe, including unsupported controls and timeouts.
    let cleanup = process.cancel().await;
    cleanup.map_err(|_| UNAVAILABLE)?;
    result.map_err(|_| UNAVAILABLE)?
}

async fn read_usage(process: &mut ClaudeProcess) -> Result<AccountUsage, String> {
    let control = process.control();
    let query = async {
        let initialized = control.initialize(json!({})).await?;
        // No user message, model inference, tools or session persistence.
        let value = control.request(json!({"subtype":"get_usage"})).await?;
        let mut usage = parse(&value)?;
        usage.email = initialized["account"]["email"]
            .as_str()
            .map(|email| email.chars().take(320).collect());
        Ok(usage)
    };
    tokio::pin!(query);
    loop {
        tokio::select! {
            result = &mut query => return result.map_err(|_: String| UNAVAILABLE.to_string()),
            event = process.next_event() => match event {
                Ok(Some(event)) if event["type"] == "control_request" => {
                    let id = event["request_id"].as_str().ok_or(UNAVAILABLE)?;
                    control.respond_control(id, Err("A consulta de limites não executa ações.".into())).await.map_err(|_| UNAVAILABLE)?;
                }
                Ok(Some(_)) => {},
                _ => return Err(UNAVAILABLE.into()),
            }
        }
    }
}

fn parse(value: &Value) -> Result<AccountUsage, String> {
    let available = value["rate_limits_available"]
        .as_bool()
        .ok_or(UNAVAILABLE)?;
    let mut usage = empty();
    usage.plan = value["subscription_type"]
        .as_str()
        .map(|plan| plan.chars().take(100).collect());
    if !available {
        usage.error = Some(UNSUPPORTED.into());
        return Ok(usage);
    }
    let limits = &value["rate_limits"];
    for (key, label, duration) in [
        ("five_hour", "5h", 18_000.0),
        ("seven_day", "7d", 604_800.0),
        ("seven_day_oauth_apps", "Apps · 7d", 604_800.0),
        ("seven_day_opus", "Opus · 7d", 604_800.0),
        ("seven_day_sonnet", "Sonnet · 7d", 604_800.0),
    ] {
        if let Some(window) = window(&limits[key], key, label.into(), duration) {
            usage.windows.push(window);
        }
    }
    for (index, bucket) in limits["model_scoped"]
        .as_array()
        .into_iter()
        .flatten()
        .take(32)
        .enumerate()
    {
        let Some(name) = bucket["display_name"]
            .as_str()
            .filter(|name| !name.trim().is_empty())
        else {
            continue;
        };
        let label = format!(
            "{} · 7d",
            name.chars()
                .filter(|ch| !ch.is_control())
                .take(60)
                .collect::<String>()
        );
        if usage.windows.iter().any(|window| window.label == label) {
            continue;
        }
        if let Some(window) = window(bucket, &format!("model/{index}"), label, 604_800.0) {
            usage.windows.push(window);
        }
    }
    if usage.windows.is_empty() {
        return Err(UNAVAILABLE.into());
    }
    usage.fetched_at = Some(crate::openai_codex::current_time_millis().map_err(|_| UNAVAILABLE)?);
    Ok(usage)
}

fn window(bucket: &Value, id: &str, label: String, duration: f64) -> Option<UsageWindow> {
    let used = bucket["utilization"]
        .as_f64()
        .filter(|value| (0.0..=100.0).contains(value));
    let resets_at = bucket["resets_at"].as_str().and_then(|text| {
        time::OffsetDateTime::parse(text, &time::format_description::well_known::Rfc3339)
            .ok()
            .and_then(|date| i64::try_from(date.unix_timestamp_nanos() / 1_000_000).ok())
            .filter(|value| *value > 0)
    });
    if used.is_none() && resets_at.is_none() {
        return None;
    }
    Some(UsageWindow {
        id: format!("claude/{id}"),
        group: "Claude".into(),
        third_party: id == "seven_day_oauth_apps",
        label,
        duration_seconds: Some(duration),
        remaining_percent: used.map(|value| 100.0 - value),
        resets_at,
    })
}

#[cfg(test)]
mod tests;
