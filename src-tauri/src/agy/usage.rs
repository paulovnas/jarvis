//! Native /quota is a local CLI command with zero model turns, not an inference request.
use super::{metadata, AgyState};
use crate::openai_codex::usage::{AccountUsage, UsageWindow};
use serde_json::Value;
use std::time::{Duration, Instant};

const UNAVAILABLE: &str = "Não foi possível consultar as cotas do Antigravity CLI. Verifique a conexão e o login executando agy no terminal.";

#[derive(Default)]
pub(super) struct Cache {
    attempted: Option<Instant>,
    value: Option<AccountUsage>,
}

pub(super) async fn cached(state: &AgyState) -> AccountUsage {
    let mut cache = state.usage.lock().await;
    if cache
        .attempted
        .is_some_and(|at| at.elapsed() < Duration::from_secs(60))
    {
        if let Some(usage) = &cache.value {
            return usage.clone();
        }
    }
    let usage = match query().await {
        Ok(usage) => usage,
        Err(error) => {
            let mut usage = cache.value.clone().unwrap_or_else(empty);
            usage.error = Some(error);
            usage
        }
    };
    cache.attempted = Some(Instant::now());
    cache.value = Some(usage.clone());
    usage
}

fn empty() -> AccountUsage {
    AccountUsage {
        alias: "Antigravity CLI".into(),
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
    let value = metadata::probe(
        &executable,
        directory.path(),
        &["-p=/quota", "--output-format", "json"],
    )
    .await?;
    parse_output(&value)
}

pub(super) fn parse_output(output: &str) -> Result<AccountUsage, String> {
    let value: Value = output
        .lines()
        .rev()
        .find(|line| !line.trim().is_empty())
        .and_then(|line| serde_json::from_str(line).ok())
        .ok_or(UNAVAILABLE)?;
    parse(&value)
}

fn parse(value: &Value) -> Result<AccountUsage, String> {
    // Fail closed: metadata probes must never accidentally accept an inferred answer.
    if value["status"] != "SUCCESS"
        || value["command"]["name"] != "usage"
        || value["num_turns"].as_u64() != Some(0)
        || value.get("error").is_some_and(|error| !error.is_null())
    {
        return Err(UNAVAILABLE.into());
    }
    let mut usage = empty();
    let mut seen = std::collections::HashSet::new();
    for group in value["command"]["data"]["groups"]
        .as_array()
        .into_iter()
        .flatten()
        .take(32)
    {
        let Some(group_name) = group["name"]
            .as_str()
            .filter(|name| !name.trim().is_empty())
        else {
            continue;
        };
        let group_name: String = group_name
            .chars()
            .filter(|ch| !ch.is_control())
            .take(100)
            .collect();
        for bucket in group["buckets"].as_array().into_iter().flatten().take(32) {
            let Some(id) = bucket["id"].as_str().filter(|id| !id.trim().is_empty()) else {
                continue;
            };
            if !seen.insert(id.to_owned()) {
                continue;
            }
            let remaining_percent = bucket["remaining_fraction"]
                .as_f64()
                .filter(|value| value.is_finite() && (0.0..=1.0).contains(value))
                .map(|fraction| fraction * 100.0);
            let resets_at = bucket["reset_time"].as_str().and_then(|text| {
                time::OffsetDateTime::parse(text, &time::format_description::well_known::Rfc3339)
                    .ok()
                    .and_then(|date| i64::try_from(date.unix_timestamp_nanos() / 1_000_000).ok())
                    .filter(|value| *value > 0)
            });
            if remaining_percent.is_none() && resets_at.is_none() {
                continue;
            }
            let (label, duration_seconds) = match bucket["window"].as_str() {
                Some("weekly") => ("7d".to_owned(), Some(604_800.0)),
                Some("5h") => ("5h".to_owned(), Some(18_000.0)),
                _ => (
                    bucket["name"]
                        .as_str()
                        .unwrap_or(id)
                        .chars()
                        .filter(|ch| !ch.is_control())
                        .take(80)
                        .collect(),
                    None,
                ),
            };
            usage.windows.push(UsageWindow {
                id: format!("agy/{}", id.chars().take(100).collect::<String>()),
                group: group_name.clone(),
                third_party: id.starts_with("3p-"),
                label: format!("{group_name} · {label}"),
                duration_seconds,
                remaining_percent,
                resets_at,
            });
        }
    }
    if usage.windows.is_empty() {
        return Err(UNAVAILABLE.into());
    }
    usage.fetched_at = Some(crate::openai_codex::current_time_millis().map_err(|_| UNAVAILABLE)?);
    Ok(usage)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn native_quota_preserves_fraction_precision_and_real_reset_windows() {
        let value = json!({"status":"SUCCESS","num_turns":0,"command":{"name":"usage","data":{"groups":[{"name":"Gemini Models","buckets":[{"id":"gemini-weekly","window":"weekly","remaining_fraction":0.6651924848556519,"reset_time":"2026-10-01T19:24:19Z"},{"id":"gemini-5h","window":"5h","remaining_fraction":0.0}]},{"name":"Claude and GPT models","buckets":[{"id":"3p-5h","window":"5h","remaining_fraction":1.0}]}]}}});
        let usage = parse(&value).unwrap();
        assert_eq!(usage.windows.len(), 3);
        assert_eq!(usage.windows[0].duration_seconds, Some(604_800.0));
        assert_eq!(usage.windows[0].remaining_percent, Some(66.51924848556519));
        assert_eq!(usage.windows[0].resets_at, Some(1_790_882_659_000));
        assert_eq!(usage.windows[1].remaining_percent, Some(0.0));
        assert!(usage.windows[2].third_party);
        assert_eq!(usage.windows[2].remaining_percent, Some(100.0));
        assert!(usage.plan.is_none());
    }

    #[test]
    fn quota_rejects_inference_failures_and_invalid_percentages() {
        for value in [
            json!({"status":"SUCCESS","num_turns":1,"command":{"name":"usage"}}),
            json!({"status":"ERROR","num_turns":0,"command":{"name":"usage"}}),
            json!({"status":"SUCCESS","num_turns":0,"error":"private-error","command":{"name":"usage"}}),
            json!({"status":"SUCCESS","num_turns":0,"command":{"name":"usage","data":{"groups":[{"name":"Gemini","buckets":[{"id":"gemini-5h","remaining_fraction":2}]}]}}}),
        ] {
            assert!(parse(&value).is_err());
        }
        assert!(parse_output("credentials-not-json").is_err());
    }
}
