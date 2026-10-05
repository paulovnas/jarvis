//! Subscription percentages come from the gateway, never estimated token costs.
use super::{response_json, unavailable, ProviderError, UsageWindow};
use serde::Deserialize;

#[derive(Deserialize)]
struct Report {
    usage: Windows,
}

#[derive(Deserialize)]
struct Windows {
    rolling: Window,
    weekly: Window,
    monthly: Window,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Window {
    status: Status,
    percent: f64,
    resets_at: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
enum Status {
    Ok,
    RateLimited,
}

fn parse(data: serde_json::Value) -> Result<Vec<UsageWindow>, ProviderError> {
    let report: Report = serde_json::from_value(data).map_err(|_| unavailable())?;
    let descriptors = [
        ("five_hour", "5h", Some(18_000.0), report.usage.rolling),
        ("weekly", "Semanal", Some(604_800.0), report.usage.weekly),
        // The service resets this window on the subscription anniversary.
        // Its duration cannot be inferred from a single reset timestamp.
        ("monthly", "Mensal", None, report.usage.monthly),
    ];
    descriptors
        .into_iter()
        .map(|(id, label, duration_seconds, window)| {
            if !window.percent.is_finite() || !(0.0..=100.0).contains(&window.percent) {
                return Err(unavailable());
            }
            let reset = time::OffsetDateTime::parse(
                &window.resets_at,
                &time::format_description::well_known::Rfc3339,
            )
            .map_err(|_| unavailable())?;
            let resets_at = i64::try_from(reset.unix_timestamp_nanos() / 1_000_000)
                .map_err(|_| unavailable())?;
            // Both valid statuses carry the authoritative consumed percentage.
            let _status = window.status;
            Ok(UsageWindow {
                id: id.into(),
                group: "OpenCode Go".into(),
                third_party: false,
                label: label.into(),
                duration_seconds,
                remaining_percent: Some(100.0 - window.percent),
                resets_at: Some(resets_at),
            })
        })
        .collect()
}

pub(crate) fn fetch_windows(
    client: &reqwest::blocking::Client,
    base: &str,
    access: &str,
    session: &str,
) -> Result<Vec<UsageWindow>, ProviderError> {
    let response = client
        .get(format!("{}/usage", base.trim_end_matches('/')))
        .bearer_auth(access)
        .header("Accept", "application/json")
        .header("User-Agent", crate::openai_codex::opencode_go::user_agent())
        .header("x-opencode-session", session)
        .send()
        .map_err(|_| unavailable())?;
    match response.status().as_u16() {
        401 => Err(ProviderError::new(
            "opencode_go_key_invalid",
            "A chave do OpenCode Go não foi aceita. Atualize-a nas configurações do provedor.",
        )),
        403 => Err(ProviderError::new(
            "opencode_go_subscription_required",
            "Esta chave não possui uma assinatura OpenCode Go ativa no espaço do OpenCode.",
        )),
        _ => parse(response_json(response)?),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    pub(super) fn report() -> serde_json::Value {
        json!({"usage": {
            "rolling":{"status":"ok","percent":24.5,"resetsAt":"2030-01-01T05:00:00Z"},
            "weekly":{"status":"rate-limited","percent":100,"resetsAt":"2030-01-07T00:00:00Z"},
            "monthly":{"status":"ok","percent":0,"resetsAt":"2030-01-31T09:30:00-03:00"}
        }})
    }

    #[test]
    fn reports_three_authoritative_windows_with_anniversary_reset() {
        let windows = parse(report()).unwrap();
        assert_eq!(windows.len(), 3);
        assert_eq!(windows[0].remaining_percent, Some(75.5));
        assert_eq!(windows[0].duration_seconds, Some(18_000.0));
        assert_eq!(windows[1].remaining_percent, Some(0.0));
        assert_eq!(windows[1].label, "Semanal");
        assert_eq!(windows[2].remaining_percent, Some(100.0));
        assert_eq!(windows[2].label, "Mensal");
        assert_eq!(windows[2].duration_seconds, None);
        assert_eq!(windows[2].resets_at, Some(1_896_093_000_000));
        assert!(windows.iter().all(|window| !window.third_party));
    }

    #[test]
    fn partial_or_malformed_reports_cannot_replace_the_last_good_snapshot() {
        for key in ["rolling", "weekly", "monthly"] {
            let mut data = report();
            data["usage"].as_object_mut().unwrap().remove(key);
            assert!(parse(data).is_err());
            for (field, value) in [
                ("percent", json!(-1)),
                ("percent", json!(101)),
                ("percent", json!("24")),
                ("percent", json!(null)),
                ("status", json!("unknown")),
                ("resetsAt", json!("tomorrow")),
            ] {
                let mut data = report();
                data["usage"][key][field] = value;
                assert!(parse(data).is_err(), "{key}/{field}");
            }
        }
        assert!(parse(json!({})).is_err());
    }
}
