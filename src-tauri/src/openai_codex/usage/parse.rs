use super::{ResetCredits, UsageWindow};
use serde_json::Value;

fn number(value: &Value) -> Option<f64> {
    value
        .as_f64()
        .or_else(|| value.as_str()?.parse().ok())
        .filter(|value| value.is_finite())
}
fn timestamp(value: &Value) -> Option<i64> {
    time::OffsetDateTime::parse(
        value.as_str()?,
        &time::format_description::well_known::Rfc3339,
    )
    .ok()
    .map(|date| (date.unix_timestamp_nanos() / 1_000_000) as i64)
}
fn duration_label(seconds: Option<f64>, fallback: &str) -> String {
    match seconds {
        Some(value) if value >= 86400.0 && value % 86400.0 == 0.0 => {
            format!("{}d", value / 86400.0)
        }
        Some(value) if value >= 3600.0 && value % 3600.0 == 0.0 => format!("{}h", value / 3600.0),
        Some(value) => format!("{}m", (value / 60.0).ceil()),
        None => fallback.to_owned(),
    }
}
pub(super) fn codex_windows(data: &Value, now: i64) -> Vec<UsageWindow> {
    let mut windows = vec![];
    let mut add = |rate: &Value, group: &str, prefix: &str| {
        for (key, fallback) in [
            ("primary_window", "Principal"),
            ("secondary_window", "Secundária"),
        ] {
            let value = &rate[key];
            if !value.is_object() {
                continue;
            }
            let duration = number(&value["limit_window_seconds"]).filter(|value| *value > 0.0);
            let resets_at = number(&value["reset_at"])
                .filter(|value| *value > 0.0)
                .map(|value| {
                    if value > 1e12 {
                        value as i64
                    } else {
                        (value * 1000.0) as i64
                    }
                })
                .or_else(|| {
                    number(&value["reset_after_seconds"])
                        .filter(|value| *value >= 0.0)
                        .map(|value| now.saturating_add((value * 1000.0) as i64))
                });
            windows.push(UsageWindow {
                id: format!("{prefix}/{key}"),
                group: group.into(),
                third_party: false,
                label: duration_label(duration, fallback),
                duration_seconds: duration,
                remaining_percent: number(&value["used_percent"])
                    .map(|used| (100.0 - used).clamp(0.0, 100.0)),
                resets_at,
            });
        }
    };
    add(&data["rate_limit"], "Codex", "codex");
    if let Some(extras) = data["additional_rate_limits"].as_array() {
        for (index, extra) in extras.iter().enumerate() {
            let group = extra["limit_name"]
                .as_str()
                .or(extra["metered_feature"].as_str())
                .unwrap_or("Adicional");
            add(&extra["rate_limit"], group, &format!("extra/{index}"));
        }
    }
    sort(&mut windows);
    windows
}
pub(super) fn credit_count(data: &Value) -> Option<ResetCredits> {
    let count = number(&data["rate_limit_reset_credits"]["available_count"])
        .filter(|value| *value >= 0.0 && value.fract() == 0.0)?;
    Some(ResetCredits {
        available_count: count as u64,
        expirations: vec![],
        details_available: count == 0.0,
    })
}
pub(super) fn credit_details(data: &Value, now: i64) -> Option<ResetCredits> {
    let credits = data["credits"].as_array()?;
    let expirations: Vec<_> = credits
        .iter()
        .filter(|credit| {
            credit["id"].as_str().is_some_and(|id| !id.is_empty())
                && credit["status"].as_str().unwrap_or("available") == "available"
        })
        .map(|credit| timestamp(&credit["expires_at"]))
        .filter(|expiry| expiry.is_none_or(|expiry| expiry > now))
        .collect();
    let count = number(&data["available_count"])
        .filter(|value| *value >= 0.0 && value.fract() == 0.0)
        .map(|value| value as u64)
        .unwrap_or(expirations.len() as u64);
    Some(ResetCredits {
        available_count: count,
        expirations,
        details_available: true,
    })
}
fn google_window(value: &Value, hint: &str) -> (String, Option<f64>) {
    let text = format!(
        "{} {} {} {}",
        value["window"].as_str().unwrap_or(""),
        value["windowId"].as_str().unwrap_or(""),
        value["displayName"]
            .as_str()
            .or(value["windowLabel"].as_str())
            .unwrap_or(""),
        hint
    )
    .to_lowercase()
    .replace(['_', '-'], " ");
    let duration = if text.contains("week") || text.contains("7d") || text.contains("7 day") {
        Some(604800.0)
    } else if text.contains("5h") || text.contains("five hour") || text.contains("5 hour") {
        Some(18000.0)
    } else if text.contains("daily") || text.contains("24h") || text.contains("day") {
        Some(86400.0)
    } else {
        None
    };
    (duration_label(duration, "Cota"), duration)
}
fn remaining(value: &Value) -> Option<f64> {
    number(&value["remainingFraction"])
        .map(|value| value.clamp(0.0, 1.0) * 100.0)
        .or_else(|| (number(&value["remainingAmount"]) == Some(0.0)).then_some(0.0))
}
fn family(text: &str) -> (&'static str, bool) {
    let text = text.to_lowercase();
    if text.contains("3p")
        || text.contains("claude")
        || text.contains("gpt")
        || text.contains("third party")
    {
        ("Outros", true)
    } else if text.contains("gemini") || text.contains("google") {
        ("Gemini", false)
    } else {
        ("Outros", true)
    }
}
pub(super) fn google_plan(data: &Value) -> Option<String> {
    let tier = if data["paidTier"].is_object() {
        &data["paidTier"]
    } else {
        &data["currentTier"]
    };
    tier["name"]
        .as_str()
        .or(tier["id"].as_str())
        .map(str::trim)
        .filter(|name| !name.is_empty() && name.len() <= 128)
        .map(str::to_owned)
}
pub(super) fn google_summary(data: &Value) -> Vec<UsageWindow> {
    let mut windows = vec![];
    let mut add = |bucket: &Value, group: &str| {
        if bucket["disabled"] == true {
            return;
        }
        let id = bucket["bucketId"].as_str().unwrap_or("");
        let (group, third_party) = family(&format!("{group} {id}"));
        let (label, duration_seconds) = google_window(bucket, id);
        windows.push(UsageWindow {
            id: format!("{group}/{id}/{}", windows.len()),
            group: group.into(),
            third_party,
            label,
            duration_seconds,
            remaining_percent: remaining(bucket),
            resets_at: timestamp(&bucket["resetTime"]),
        });
    };
    let groups = data["groups"].as_array().filter(|groups| {
        groups.iter().any(|group| {
            group["buckets"]
                .as_array()
                .is_some_and(|buckets| !buckets.is_empty())
        })
    });
    if let Some(groups) = groups {
        for group in groups {
            if let Some(buckets) = group["buckets"].as_array() {
                for bucket in buckets {
                    add(bucket, group["displayName"].as_str().unwrap_or(""));
                }
            }
        }
    } else if let Some(buckets) = data["buckets"].as_array() {
        for bucket in buckets {
            add(bucket, "");
        }
    }
    sort(&mut windows);
    windows
}
pub(super) fn google_models(data: &Value) -> Vec<UsageWindow> {
    let mut windows = vec![];
    let Some(models) = data["models"].as_object() else {
        return windows;
    };
    for (id, model) in models {
        let (group, third_party) = family(&format!(
            "{id} {}",
            model["modelProvider"].as_str().unwrap_or("")
        ));
        let mut add = |value: &Value, hint: &str| {
            let values = value.as_array().cloned().unwrap_or_else(|| {
                if value.is_object() {
                    vec![value.clone()]
                } else {
                    vec![]
                }
            });
            for (index, value) in values.iter().enumerate() {
                let (label, duration_seconds) = google_window(value, hint);
                windows.push(UsageWindow {
                    id: format!("{id}/{hint}/{index}"),
                    group: format!("{group} · {}", model["displayName"].as_str().unwrap_or(id)),
                    third_party,
                    label,
                    duration_seconds,
                    remaining_percent: remaining(value),
                    resets_at: timestamp(&value["resetTime"]),
                });
            }
        };
        for key in [
            "quotaInfo",
            "quotaInfos",
            "dailyQuotaInfo",
            "dailyQuotaInfos",
            "weeklyQuotaInfo",
            "weeklyQuotaInfos",
        ] {
            add(&model[key], key);
        }
        for key in ["quotaInfoByWindow", "quotaInfosByWindow", "quotaInfoByTier"] {
            if let Some(map) = model[key].as_object() {
                for (hint, value) in map {
                    add(value, hint);
                }
            }
        }
    }
    sort(&mut windows);
    windows
}
fn sort(windows: &mut [UsageWindow]) {
    windows.sort_by(|a, b| {
        a.third_party
            .cmp(&b.third_party)
            .then_with(|| a.group.cmp(&b.group))
            .then_with(|| {
                a.duration_seconds
                    .unwrap_or(f64::MAX)
                    .total_cmp(&b.duration_seconds.unwrap_or(f64::MAX))
            })
    });
}
