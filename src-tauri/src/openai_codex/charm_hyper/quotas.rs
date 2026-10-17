use crate::openai_codex::{
    usage::{AccountUsage, UsageWindow},
    ProviderError,
};
use serde_json::Value;

fn invalid() -> ProviderError {
    super::error(
        "O painel Charm Hyper retornou cotas inválidas. Atualize os limites para tentar novamente.",
    )
}

fn flag(data: &Value, field: &str) -> Result<bool, ProviderError> {
    data[field].as_bool().ok_or_else(invalid)
}

fn amount(data: &Value, field: &str) -> Result<f64, ProviderError> {
    data[field]
        .as_f64()
        .filter(|value| value.is_finite() && *value >= 0.0)
        .ok_or_else(invalid)
}

fn percentage(data: &Value, remaining: &str, total: &str) -> Result<f64, ProviderError> {
    let total = amount(data, total)?;
    if total <= 0.0 {
        return Err(invalid());
    }
    Ok((amount(data, remaining)? / total * 100.0).clamp(0.0, 100.0))
}

fn duration(data: &Value, field: &str, seconds: f64) -> Result<f64, ProviderError> {
    let value = amount(data, field)? * seconds;
    (value.is_finite() && value > 0.0)
        .then_some(value)
        .ok_or_else(invalid)
}

fn future(data: &Value, field: &str, now: i64) -> Result<Option<i64>, ProviderError> {
    match data.get(field) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => {
            let value = value
                .as_i64()
                .filter(|value| *value >= 0)
                .ok_or_else(invalid)?;
            Ok((value > now).then_some(value))
        }
    }
}

pub(in crate::openai_codex) fn apply(
    data: &Value,
    usage: &mut AccountUsage,
) -> Result<(), ProviderError> {
    let plan = match data.get("plan_name") {
        None | Some(Value::Null) => None,
        Some(Value::String(plan)) => Some(plan.clone()),
        Some(_) => return Err(invalid()),
    };
    let mut windows = vec![];
    if flag(data, "subscribed")? && flag(data, "known")? {
        let now = data["now"]
            .as_i64()
            .filter(|value| *value >= 0)
            .ok_or_else(invalid)?;
        if flag(data, "limited")? {
            windows.push(UsageWindow {
                id: "five_hour".into(),
                group: "Charm Hyper".into(),
                third_party: false,
                label: "5h".into(),
                duration_seconds: Some(duration(data, "window_minutes", 60.0)?),
                remaining_percent: Some(percentage(data, "available", "cap")?),
                resets_at: None,
                next_recovery_at: future(data, "returns_at", now)?,
            });
        }
        windows.push(UsageWindow {
            id: "weekly".into(),
            group: "Charm Hyper".into(),
            third_party: false,
            label: "Semanal".into(),
            duration_seconds: Some(duration(data, "refresh_hours", 3600.0)?),
            remaining_percent: Some(percentage(data, "remaining", "allowance")?),
            resets_at: future(data, "refresh_at", now)?,
            next_recovery_at: None,
        });
    }
    usage.plan = plan;
    usage.windows = windows;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const NOW: i64 = 1_800_000_000_000;

    fn dashboard() -> Value {
        json!({
            "now": NOW, "window_minutes": 300, "subscribed": true,
            "plan_name": "Hyper Max", "period_end": NOW + 30 * 86_400_000,
            "limited": true, "known": true, "available": 4.0, "remaining": 60.0,
            "cap": 10.0, "prepaid": 999.0, "refresh_at": NOW + 302_400_000,
            "returns_at": NOW + 9_000_000, "allowance": 100.0, "refresh_hours": 168
        })
    }

    fn usage() -> AccountUsage {
        AccountUsage {
            alias: "charm-hyper-pessoal".into(),
            fetched_at: None,
            email: None,
            plan: None,
            windows: vec![],
            reset_credits: None,
            credit_balance: Some(173.5),
            error: None,
        }
    }

    #[test]
    fn rolling_capacity_recovery_is_distinct_from_weekly_renewal_and_credits() {
        let mut usage = usage();
        apply(&dashboard(), &mut usage).unwrap();
        assert_eq!(usage.plan.as_deref(), Some("Hyper Max"));
        assert_eq!(usage.credit_balance, Some(173.5));
        assert_eq!(usage.windows.len(), 2);
        let rolling = &usage.windows[0];
        assert_eq!(rolling.id, "five_hour");
        assert_eq!(rolling.group, "Charm Hyper");
        assert_eq!(rolling.duration_seconds, Some(18_000.0));
        assert_eq!(rolling.remaining_percent, Some(40.0));
        assert_eq!(rolling.resets_at, None);
        assert_eq!(rolling.next_recovery_at, Some(NOW + 9_000_000));
        let weekly = &usage.windows[1];
        assert_eq!(weekly.id, "weekly");
        assert_eq!(weekly.duration_seconds, Some(604_800.0));
        assert_eq!(weekly.remaining_percent, Some(60.0));
        assert_eq!(weekly.resets_at, Some(NOW + 302_400_000));
        assert_eq!(weekly.next_recovery_at, None);
    }

    #[test]
    fn authenticated_dashboard_fixture_preserves_zero_capacity_and_fractional_weekly_quota() {
        let data = json!({
            "now": 1_792_205_393_916_i64, "window_minutes": 300,
            "subscribed": true, "limited": true, "known": true,
            "available": 0, "remaining": 78.85102987399999, "cap": 100,
            "allowance": 180, "plan_name": "Shred Diet", "period_end": 1_794_867_376_000_i64,
            "returns_at": 1_792_207_798_000_i64, "refresh_hours": 168,
            "prepaid": 0, "refresh_at": 1_792_281_600_000_i64
        });
        let mut usage = usage();
        apply(&data, &mut usage).unwrap();
        assert_eq!(usage.windows[0].remaining_percent, Some(0.0));
        assert_eq!(usage.windows[0].resets_at, None);
        assert_eq!(usage.windows[0].next_recovery_at, Some(1_792_207_798_000));
        let weekly = usage.windows[1].remaining_percent.unwrap();
        assert!((weekly - 43.80612770777777).abs() < 1e-9);
        assert_eq!(weekly.round(), 44.0);
        assert_eq!(usage.windows[1].resets_at, Some(1_792_281_600_000));
        assert_eq!(usage.credit_balance, Some(173.5));
    }

    #[test]
    fn unsubscribed_and_unknown_reports_do_not_invent_windows_or_keep_old_ones() {
        for flag in ["subscribed", "known"] {
            let mut usage = usage();
            apply(&dashboard(), &mut usage).unwrap();
            assert_eq!(usage.windows.len(), 2);
            let mut data = dashboard();
            data[flag] = json!(false);
            apply(&data, &mut usage).unwrap();
            assert!(usage.windows.is_empty(), "{flag}");
            assert_eq!(usage.credit_balance, Some(173.5));
        }
    }

    #[test]
    fn unlimited_rolling_capacity_still_preserves_the_known_weekly_allowance() {
        let mut data = dashboard();
        data["limited"] = json!(false);
        data["cap"] = Value::Null;
        data["available"] = Value::Null;
        let mut usage = usage();
        apply(&data, &mut usage).unwrap();
        assert_eq!(usage.windows.len(), 1);
        assert_eq!(usage.windows[0].id, "weekly");
        assert_eq!(usage.windows[0].remaining_percent, Some(60.0));
    }

    #[test]
    fn only_future_timestamps_are_exposed_and_missing_recovery_is_not_a_reset() {
        for timestamp in [Value::Null, json!(0), json!(NOW - 1), json!(NOW)] {
            let mut data = dashboard();
            data["returns_at"] = timestamp.clone();
            data["refresh_at"] = timestamp;
            let mut usage = usage();
            apply(&data, &mut usage).unwrap();
            assert!(usage
                .windows
                .iter()
                .all(|window| { window.resets_at.is_none() && window.next_recovery_at.is_none() }));
        }
        let mut data = dashboard();
        data.as_object_mut().unwrap().remove("returns_at");
        let mut usage = usage();
        apply(&data, &mut usage).unwrap();
        assert_eq!(usage.windows[0].next_recovery_at, None);
        assert_eq!(usage.windows[0].resets_at, None);
    }

    #[test]
    fn zero_and_excess_capacity_remain_bounded_without_changing_the_period() {
        for (available, remaining, expected) in [(0.0, 0.0, 0.0), (15.0, 120.0, 100.0)] {
            let mut data = dashboard();
            data["available"] = json!(available);
            data["remaining"] = json!(remaining);
            let mut usage = usage();
            apply(&data, &mut usage).unwrap();
            assert_eq!(usage.windows.len(), 2);
            assert!(usage
                .windows
                .iter()
                .all(|window| window.remaining_percent == Some(expected)));
        }
    }

    #[test]
    fn invalid_known_contracts_fail_without_partially_overwriting_reported_usage() {
        for (field, value) in [
            ("subscribed", Value::Null),
            ("known", json!("true")),
            ("limited", json!(1)),
            ("now", json!(-1)),
            ("window_minutes", json!(0)),
            ("refresh_hours", json!(1e308)),
            ("available", json!(-1)),
            ("cap", json!(0)),
            ("remaining", json!(-1)),
            ("allowance", json!(0)),
            ("available", json!("4")),
            ("cap", Value::Null),
            ("returns_at", json!("tomorrow")),
            ("refresh_at", json!(-1)),
            ("plan_name", json!(1)),
        ] {
            let mut data = dashboard();
            data[field] = value;
            let mut usage = usage();
            assert!(apply(&data, &mut usage).is_err(), "{field}");
            assert!(usage.windows.is_empty(), "{field}");
            assert_eq!(usage.plan, None, "{field}");
            assert_eq!(usage.credit_balance, Some(173.5), "{field}");
        }
    }
}
