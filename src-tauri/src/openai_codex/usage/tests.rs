use super::*;
use serde_json::json;

#[test]
fn codex_classifies_actual_duration_without_inventing_a_five_hour_window() {
    let result = parse::codex_windows(
        &json!({"rate_limit":{"primary_window":null,"secondary_window":{"used_percent":"64", "limit_window_seconds":604800,"reset_after_seconds":60}},"additional_rate_limits":[{"limit_name":"Review","rate_limit":{"primary_window":{"used_percent":null}}}]}),
        1000,
    );
    let weekly = result
        .iter()
        .find(|window| window.group == "Codex")
        .unwrap();
    assert_eq!(weekly.label, "7d");
    assert_eq!(weekly.remaining_percent, Some(36.0));
    assert_eq!(weekly.resets_at, Some(61000));
    assert!(!result.iter().any(|window| window.label == "5h"));
    assert_eq!(result[1].remaining_percent, None);
    let clamped = parse::codex_windows(
        &json!({"rate_limit":{"primary_window":{"used_percent":110,"reset_at":1900000000000_i64}}}),
        0,
    );
    assert_eq!(clamped[0].remaining_percent, Some(0.0));
    assert_eq!(clamped[0].resets_at, Some(1900000000000));
}

#[test]
fn saved_credits_exclude_redeemed_expired_and_keep_unknown_expirations_unknown() {
    let credits = parse::credit_details(
        &json!({"credits":[
            {"id":"a","status":"available","expires_at":"2030-10-02T12:00:00Z"},
            {"id":"b","status":"redeemed"},
            {"id":"c","status":"available","expires_at":"2001-01-01T00:00:00Z"},
            {"id":"d","status":"available"}, {"status":"available"}
        ]}),
        1800000000000,
    )
    .unwrap();
    assert_eq!(credits.available_count, 2);
    assert!(credits.expirations[0].unwrap() > 1800000000000);
    assert_eq!(credits.expirations[1], None);
    assert!(parse::credit_details(&json!({}), 0).is_none());
    assert!(
        !parse::credit_count(&json!({"rate_limit_reset_credits":{"available_count":3}}))
            .unwrap()
            .details_available
    );
}

#[test]
fn google_summary_keeps_shared_third_party_separate_from_gemini() {
    assert_eq!(
        parse::google_plan(
            &json!({"paidTier":{"name":"Google AI Pro"},"currentTier":{"id":"free-tier"}})
        ),
        Some("Google AI Pro".into())
    );
    assert!(parse::google_plan(&json!({})).is_none());
    let result = parse::google_summary(&json!({"groups":[
        {"displayName":"Gemini models","buckets":[
            {"bucketId":"gemini-weekly","window":"weekly","remainingFraction":0.3},
            {"bucketId":"gemini-5h","window":"5h","remainingFraction":0.76,"resetTime":"2030-01-01T00:00:00Z"},
            {"bucketId":"disabled","disabled":true,"remainingFraction":1}]},
        {"displayName":"Claude and GPT models","buckets":[{"bucketId":"3p-5h","window":"5h","remainingFraction":1}]}
    ],"buckets":[{"bucketId":"duplicate","remainingFraction":1}]}));
    assert_eq!(result.len(), 3);
    assert_eq!(result[0].label, "5h");
    assert_eq!(result[0].remaining_percent, Some(76.0));
    assert!(!result[0].third_party);
    assert_eq!(result[1].label, "7d");
    assert!(result[2].third_party);
    assert_eq!(result[2].group, "Outros");
}

#[test]
fn google_fallback_preserves_model_buckets_without_guessing_window_lengths() {
    let result = parse::google_models(&json!({"models":{
        "gemini-pro":{"displayName":"Gemini Pro","quotaInfo":{"remainingFraction":0.4,"resetTime":"2030-01-01T00:00:00Z"},"weeklyQuotaInfo":{"remainingFraction":0.1}},
        "claude-opus":{"quotaInfo":{"remainingFraction":0.8}},
        "unknown":{"quotaInfo":{"remainingFraction":null}}
    }}));
    assert_eq!(result.len(), 4);
    assert_eq!(
        result.iter().filter(|window| !window.third_party).count(),
        2
    );
    assert!(result
        .iter()
        .any(|window| window.label == "Cota" && window.duration_seconds.is_none()));
    assert!(result
        .iter()
        .any(|window| window.remaining_percent.is_none()));
}

#[test]
fn usage_preferences_migrate_and_remain_isolated_by_alias() {
    let mut connection = rusqlite::Connection::open_in_memory().unwrap();
    crate::persistence::initialize_database(&mut connection).unwrap();
    let first =
        crate::persistence::insert_provider_account(&connection, "openai-codex-a", "one").unwrap();
    let second =
        crate::persistence::insert_provider_account(&connection, "antigravity-b", "two").unwrap();
    assert!(first.show_usage);
    assert!(!second.show_third_party_usage);
    save_visibility(&connection, &second.alias, false, true).unwrap();
    let records = crate::persistence::list_provider_accounts(&connection).unwrap();
    assert!(
        records
            .iter()
            .find(|record| record.alias == first.alias)
            .unwrap()
            .show_usage
    );
    assert!(
        records
            .iter()
            .find(|record| record.alias == second.alias)
            .unwrap()
            .show_third_party_usage
    );
    assert!(save_visibility(&connection, "openai-codex-missing", true, false).is_err());
    let cache = UsageCache::default();
    let a = cache.entry(&first).unwrap();
    assert!(Arc::ptr_eq(&a, &cache.entry(&first).unwrap()));
    assert!(!Arc::ptr_eq(&a, &cache.entry(&second).unwrap()));
    let mut reconnected = first.clone();
    reconnected.account_id = "new".into();
    assert!(!Arc::ptr_eq(&a, &cache.entry(&reconnected).unwrap()));
}

#[test]
fn usage_alerts_persist_and_only_match_the_configured_available_window() {
    let mut connection = rusqlite::Connection::open_in_memory().unwrap();
    crate::persistence::initialize_database(&mut connection).unwrap();
    let account =
        crate::persistence::insert_provider_account(&connection, "openai-codex-a", "one").unwrap();
    save_alert(
        &connection,
        &account.alias,
        Some(crate::openai_codex::UsageAlert {
            window: crate::openai_codex::UsageAlertWindow::Weekly,
            remaining_percent: 20,
        }),
    )
    .unwrap();
    let record = crate::persistence::list_provider_accounts(&connection)
        .unwrap()
        .remove(0);
    assert_eq!(record.usage_alert_window.as_deref(), Some("weekly"));
    assert_eq!(record.usage_alert_threshold, Some(20));

    let usage = AccountUsage {
        alias: record.alias.clone(),
        fetched_at: Some(1_000),
        email: None,
        plan: None,
        windows: vec![
            UsageWindow {
                id: "codex/primary_window".into(),
                group: "Codex".into(),
                third_party: false,
                label: "5h".into(),
                duration_seconds: Some(18_000.0),
                remaining_percent: Some(10.0),
                resets_at: Some(2_000),
            },
            UsageWindow {
                id: "codex/secondary_window".into(),
                group: "Codex".into(),
                third_party: false,
                label: "7d".into(),
                duration_seconds: Some(604_800.0),
                remaining_percent: Some(19.6),
                resets_at: Some(3_000),
            },
        ],
        reset_credits: None,
        error: None,
    };
    let notices = alert_notices(&record, &usage, 1_500);
    assert_eq!(notices.len(), 1);
    assert!(notices[0].body.contains("7d: restam 20%"));
    assert_eq!(notices[0].window_id, "codex/secondary_window");
    assert_eq!(notices[0].resets_at, 3_000);
    assert_eq!(notices[0].threshold, 20);
    assert!(claim_alert_delivery(&connection, &record.alias, &notices[0]).unwrap());
    assert!(!claim_alert_delivery(&connection, &record.alias, &notices[0]).unwrap());
    let next_cycle = AccountUsage {
        windows: usage
            .windows
            .iter()
            .cloned()
            .map(|mut window| {
                if window.label == "7d" {
                    window.resets_at = Some(4_000);
                }
                window
            })
            .collect(),
        ..usage
    };
    let next_notice = alert_notices(&record, &next_cycle, 1_500).remove(0);
    assert_eq!(next_notice.resets_at, 4_000);
    assert!(claim_alert_delivery(&connection, &record.alias, &next_notice).unwrap());

    save_alert(&connection, &record.alias, None).unwrap();
    assert!(
        crate::persistence::list_provider_accounts(&connection).unwrap()[0]
            .usage_alert_window
            .is_none()
    );
    assert_eq!(
        connection
            .query_row(
                "SELECT count(*) FROM provider_usage_alert_deliveries WHERE alias=?1",
                [&record.alias],
                |row| row.get::<_, i64>(0),
            )
            .unwrap(),
        0
    );
    assert!(save_alert(
        &connection,
        &record.alias,
        Some(crate::openai_codex::UsageAlert {
            window: crate::openai_codex::UsageAlertWindow::Weekly,
            remaining_percent: 0,
        })
    )
    .is_err());
}

#[test]
fn antigravity_alerts_respect_third_party_visibility_and_ignore_stale_data() {
    let mut connection = rusqlite::Connection::open_in_memory().unwrap();
    crate::persistence::initialize_database(&mut connection).unwrap();
    let account =
        crate::persistence::insert_provider_account(&connection, "antigravity-a", "one").unwrap();
    save_alert(
        &connection,
        &account.alias,
        Some(crate::openai_codex::UsageAlert {
            window: crate::openai_codex::UsageAlertWindow::FiveHour,
            remaining_percent: 25,
        }),
    )
    .unwrap();
    let record = crate::persistence::list_provider_accounts(&connection)
        .unwrap()
        .remove(0);
    let mut usage = AccountUsage {
        alias: record.alias.clone(),
        fetched_at: Some(1_000),
        email: None,
        plan: None,
        windows: vec![UsageWindow {
            id: "third-party".into(),
            group: "Outros".into(),
            third_party: true,
            label: "5h".into(),
            duration_seconds: Some(18_000.0),
            remaining_percent: Some(10.0),
            resets_at: Some(3_000),
        }],
        reset_credits: None,
        error: None,
    };
    assert!(alert_notices(&record, &usage, 1_500).is_empty());
    let mut visible = record.clone();
    visible.show_third_party_usage = true;
    assert_eq!(alert_notices(&visible, &usage, 1_500).len(), 1);
    usage.error = Some("offline".into());
    assert!(alert_notices(&visible, &usage, 1_500).is_empty());
}

#[test]
fn codex_http_probe_is_read_only_and_scopes_requests_to_the_selected_account() {
    use std::io::{BufRead, BufReader, Write};
    use std::net::TcpListener;
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let worker = std::thread::spawn(move || {
        for (path, body) in [
            (
                "/wham/usage",
                json!({"plan_type":"pro","rate_limit":{"primary_window":{"used_percent":24,"limit_window_seconds":18000}},"rate_limit_reset_credits":{"available_count":1}}),
            ),
            (
                "/wham/rate-limit-reset-credits",
                json!({"available_count":0,"credits":[]}),
            ),
        ] {
            let (mut socket, _) = listener.accept().unwrap();
            let mut lines = BufReader::new(&socket).lines();
            assert!(lines
                .next()
                .unwrap()
                .unwrap()
                .starts_with(&format!("GET {path} ")));
            let headers: Vec<_> = lines
                .map_while(Result::ok)
                .take_while(|line| !line.is_empty())
                .collect();
            assert!(headers
                .iter()
                .any(|line| line.to_lowercase() == "chatgpt-account-id: account-one"));
            assert!(headers
                .iter()
                .any(|line| line.to_lowercase() == "authorization: bearer private-token"));
            let body = body.to_string();
            write!(socket,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).unwrap();
        }
    });
    let credential = CodexCredential::new(
        "private-token",
        "private-refresh",
        i64::MAX,
        "account-one",
        None,
        None,
    );
    let mut usage = AccountUsage {
        alias: "openai-codex-a".into(),
        fetched_at: None,
        email: None,
        plan: None,
        windows: vec![],
        reset_credits: None,
        error: None,
    };
    codex(
        &reqwest::blocking::Client::new(),
        &base,
        &credential,
        &mut usage,
        0,
    )
    .unwrap();
    worker.join().unwrap();
    assert_eq!(usage.windows[0].remaining_percent, Some(76.0));
    assert_eq!(usage.reset_credits.as_ref().unwrap().available_count, 0);
    let serialized = serde_json::to_string(&usage).unwrap();
    assert!(!serialized.contains("private"));
    assert!(!serialized.contains("account-one"));
}
