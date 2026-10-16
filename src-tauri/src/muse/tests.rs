use super::transport::{MuseProcess, MuseSpawnConfig};
use super::{metadata, usage};
use serde_json::{json, Value};
use std::time::Duration;

fn host_config(mode: &str) -> MuseSpawnConfig {
    let mut config = MuseSpawnConfig::new("node".into(), std::env::temp_dir());
    config.args = vec![
        "-e".into(),
        include_str!("fixtures/host.cjs").into(),
        mode.into(),
        concat!(env!("CARGO_MANIFEST_DIR"), "/src/muse/fixtures").into(),
    ];
    config.request_timeout = Duration::from_secs(1);
    config
}

async fn host(mode: &str) -> MuseProcess {
    let process = MuseProcess::spawn(host_config(mode)).unwrap();
    process
        .initialize(&json!({"capabilities":{"experimentalApi":true}}))
        .await
        .unwrap();
    process
}

async fn pending(state: &super::MuseState, process: &MuseProcess, id: &str) {
    let (stop, _stopped) = tokio::sync::watch::channel(false);
    *state.login.lock().await = Some(super::PendingLogin {
        id: id.into(),
        control: process.control(),
        stop,
    });
}

fn runtime() -> metadata::RuntimeStatus {
    metadata::RuntimeStatus {
        preferences: crate::claude::ProviderPreferences::default(),
        installed: true,
        authenticated: false,
        version: None,
        models: vec![],
        error: None,
        auth_method: None,
        email: None,
        subscription_type: None,
    }
}

#[test]
fn subscription_login_is_the_only_authenticated_credential_lane() {
    let account: Value = serde_json::from_str(include_str!("fixtures/account.json")).unwrap();
    let mut status = runtime();
    metadata::account_from_status(&mut status, &account);
    assert!(status.authenticated);
    assert_eq!(status.auth_method.as_deref(), Some("accountLogin"));
    assert_eq!(status.email.as_deref(), Some("member@example.test"));
    for lane in ["apiKey", "envKey", "loggedOut", "futureUnknown"] {
        metadata::account_from_status(
            &mut status,
            &json!({"state":lane,"credentialRequired":true}),
        );
        assert!(
            !status.authenticated,
            "credential lane {lane} must not use subscription execution"
        );
        assert!(status.email.is_none());
        assert!(status.models.is_empty());
    }
}

#[test]
fn native_catalog_preserves_declared_efforts_and_truthful_unknowns() {
    let catalog: Value = serde_json::from_str(include_str!("fixtures/models.json")).unwrap();
    let models = metadata::models_from_list(&catalog);
    assert_eq!(models.len(), 2);
    assert_eq!(models[0].id, "muse-example");
    assert_eq!(models[0].reasoning_levels, ["none", "low", "high", "ultra"]);
    assert_eq!(models[0].default_reasoning.as_deref(), Some("high"));
    assert!(models[1].reasoning_levels.is_empty());
    assert!(models[1].default_reasoning.is_none());
    assert!(metadata::models_from_list(&json!({"models":[]})).is_empty());
}

#[test]
fn malformed_or_foreign_models_never_become_subscription_choices() {
    let models = metadata::models_from_list(&json!({"models":[
        {"modelId":"--unsafe","providerId":"meta"},
        {"modelId":"foreign","providerId":"other"},
        {"modelId":"safe","providerId":"meta","variants":["low","future","low"],"defaultReasoningEffort":"ultra"},
        {"modelId":"safe","providerId":"meta"}
    ]}));
    assert_eq!(models.len(), 1);
    assert_eq!(models[0].id, "safe");
    assert_eq!(models[0].reasoning_levels, ["low"]);
    assert!(models[0].default_reasoning.is_none());
}

#[test]
fn device_login_only_exposes_the_official_https_verification_origin() {
    assert!(metadata::verification_url("https://auth.meta.com/device?code=EXAMPLE").is_ok());
    for url in [
        "http://auth.meta.com/device",
        "https://auth.meta.com.attacker.test/device",
        "https://user:secret@auth.meta.com/device",
        "https://auth.meta.com:444/device",
        "javascript:alert(1)",
        "https://api.meta.ai/device",
        "https://auth.meta.com/device\n",
    ] {
        assert!(
            metadata::verification_url(url).is_err(),
            "unsafe URL: {url:?}"
        );
    }
}

#[test]
fn usage_keeps_the_host_observation_time_and_clamps_over_quota() {
    let value: Value = serde_json::from_str(include_str!("fixtures/usage.json")).unwrap();
    let usage = usage::parse(&value).unwrap();
    assert_eq!(usage.alias, "Muse Code");
    assert_eq!(usage.fetched_at, Some(1_792_094_400_000));
    assert_eq!(usage.plan.as_deref(), Some("example-plan"));
    assert_eq!(usage.windows.len(), 2);
    assert_eq!(usage.windows[0].remaining_percent, Some(77.0));
    assert_eq!(usage.windows[0].duration_seconds, Some(18_000.0));
    assert_eq!(usage.windows[0].resets_at, Some(1_792_112_400_000));
    assert_eq!(usage.windows[1].remaining_percent, Some(0.0));
}

#[test]
fn absent_usage_never_becomes_a_new_zero_usage_observation() {
    let usage = usage::parse(&json!({})).unwrap();
    assert!(usage.fetched_at.is_none());
    assert!(usage.windows.is_empty());
    assert!(usage::parse(&json!({"usage":{"observedAtMs":-1}})).is_err());
}

#[test]
fn subscription_selection_accepts_the_msp_vocabulary_and_refuses_flags() {
    for effort in [
        "none", "minimal", "low", "medium", "high", "xhigh", "max", "ultra",
    ] {
        assert!(super::validate_selection("muse-example", Some(effort)).is_ok());
    }
    for model in ["", " ", "--provider", "model\nsecret"] {
        assert!(super::validate_selection(model, None).is_err());
    }
    assert!(super::validate_selection("muse-example", Some("future")).is_err());
}

#[test]
fn device_login_response_has_a_bounded_local_deadline_and_no_provider_secrets() {
    let flow = super::login_flow(&json!({"verificationUrl":"https://auth.meta.com/device","userCode":"TEST-CODE","secret":"never-render","expiresAt":9}), "attempt-1".into(), 1_000).unwrap();
    assert_eq!(flow.id, "attempt-1");
    assert_eq!(flow.user_code, "TEST-CODE");
    assert!(flow.expires_at > 1_000 && flow.expires_at <= 601_000);
    assert!(!serde_json::to_string(&flow)
        .unwrap()
        .contains("never-render"));
    assert!(super::login_flow(
        &json!({"verificationUrl":"https://auth.meta.com/device"}),
        "attempt-1".into(),
        1_000
    )
    .is_err());
}

#[tokio::test]
async fn connection_negotiates_experimental_accounts_and_refuses_unsupported_hosts() {
    let (mut process, info) = super::connect_config(host_config("")).await.unwrap();
    assert_eq!(info["experimentalApi"], true);
    process.cancel().await.unwrap();
    assert!(super::connect_config(host_config("unsupported"))
        .await
        .is_err());
}

#[tokio::test]
async fn runtime_and_account_hosts_share_only_the_private_jarvis_muse_profile() {
    let home = tempfile::tempdir().unwrap();
    let base = crate::data_dir::root(home.path()).join("muse");
    for _ in 0..3 {
        let mut config =
            super::runtime_config("node".into(), home.path(), &std::env::temp_dir()).unwrap();
        config.args = host_config("").args;
        let (mut process, _) = super::connect_config(config).await.unwrap();
        let environment = process
            .control()
            .request("fixture/environment", &json!({}))
            .await
            .unwrap();
        assert_eq!(environment["config"].as_str(), base.join("config").to_str());
        assert_eq!(environment["data"].as_str(), base.join("data").to_str());
        assert_eq!(environment["state"].as_str(), base.join("state").to_str());
        if let Some(auth) = environment["auth"].as_str() {
            assert!(std::path::Path::new(auth).starts_with(&base));
            assert_ne!(
                auth,
                home.path().join(".config/muse/auth.json").to_str().unwrap()
            );
        }
        assert_eq!(
            environment["home"].as_str(),
            std::env::var("HOME").ok().as_deref()
        );
        assert_eq!(environment["hasApiKey"], false);
        process.cancel().await.unwrap();
    }
    assert!(!home.path().join(".config/muse/auth.json").exists());
}

#[tokio::test]
async fn account_discovery_uses_only_read_only_host_requests_without_starting_a_session() {
    let home = tempfile::tempdir().unwrap();
    let mut process = host("").await;
    let status = metadata::read_status_at(&mut process, Some("fixture-1".into()), home.path())
        .await
        .unwrap();
    assert!(status.authenticated);
    assert_eq!(status.models.len(), 2);
    assert_eq!(status.subscription_type.as_deref(), Some("example-plan"));
    let methods = process
        .control()
        .request("fixture/methods", &json!({}))
        .await
        .unwrap();
    assert_eq!(
        methods["methods"],
        json!([
            "initialize",
            "initialized",
            "account/read",
            "model/list",
            "usage/read",
            "fixture/methods"
        ])
    );
    process.cancel().await.unwrap();
}

#[tokio::test]
async fn api_credentials_do_not_trigger_subscription_catalog_or_quota_queries() {
    let home = tempfile::tempdir().unwrap();
    let mut process = host("apiKey").await;
    let status = metadata::read_status_at(&mut process, None, home.path())
        .await
        .unwrap();
    assert!(!status.authenticated);
    assert!(status.models.is_empty());
    let methods = process
        .control()
        .request("fixture/methods", &json!({}))
        .await
        .unwrap();
    assert_eq!(
        methods["methods"],
        json!([
            "initialize",
            "initialized",
            "account/read",
            "fixture/methods"
        ])
    );
    process.cancel().await.unwrap();
}

#[tokio::test]
async fn a_catalog_error_preserves_the_confirmed_subscription_account() {
    let home = tempfile::tempdir().unwrap();
    let mut process = host("modelsUnavailable").await;
    let status = metadata::read_status_at(&mut process, None, home.path())
        .await
        .unwrap();
    assert!(status.authenticated);
    assert_eq!(status.email.as_deref(), Some("member@example.test"));
    assert!(status.models.is_empty());
    assert!(!status.error.unwrap().contains("private-account-data"));
    process.cancel().await.unwrap();
}

#[tokio::test]
async fn granted_login_refreshes_a_stale_catalog_from_a_new_host_without_repeating_auth() {
    let home = tempfile::tempdir().unwrap();
    let mut process = host("staleCatalogUntilRestart").await;
    let control = process.control();
    control
        .request("account/loginStart", &json!({"type":"deviceCode"}))
        .await
        .unwrap();
    control
        .request("fixture/loginCompleted", &json!({"outcome":"granted"}))
        .await
        .unwrap();
    let (_stop, mut stopped) = tokio::sync::watch::channel(false);
    assert!(super::wait_login(&mut process, &mut stopped).await.unwrap());
    let stale = metadata::read_status_at(&mut process, None, home.path())
        .await
        .unwrap();
    assert!(stale.authenticated);
    assert!(stale.models.is_empty());
    let state = super::MuseState::default();
    pending(&state, &process, "active").await;
    let status = super::refresh_granted_login(&mut process, async {
        assert!(control
            .request("fixture/methods", &json!({}))
            .await
            .is_err());
        let mut fresh = host("staleCatalogUntilRestart").await;
        let status = metadata::read_status_at(&mut fresh, None, home.path())
            .await
            .unwrap();
        let calls = fresh
            .control()
            .request("fixture/methods", &json!({}))
            .await
            .unwrap();
        assert!(!calls["methods"]
            .as_array()
            .unwrap()
            .iter()
            .any(|method| method == "account/loginStart" || method == "account/logout"));
        fresh.cancel().await.unwrap();
        status
    })
    .await;
    assert!(status.authenticated);
    assert_eq!(status.models.len(), 2);
    assert_eq!(
        super::apply_login(&state, "active", status.clone())
            .await
            .unwrap()
            .models
            .len(),
        2
    );
    pending(&state, &process, "cancelled").await;
    super::take_login(&state, "cancelled").await.unwrap();
    assert!(super::apply_login(&state, "cancelled", status)
        .await
        .is_none());
}

#[test]
fn opaque_numeric_subscription_tiers_are_not_rendered_as_plan_names() {
    let usage = usage::parse(&json!({"usage":{
        "observedAtMs":1_792_094_400_000_i64,"tier":"27681527378179523",
        "window":{"usedPercent":0,"resetsAtMs":1_792_112_400_000_i64,"windowDurationMins":300},
        "weekly":{"usedPercent":0,"resetsAtMs":1_792_699_200_000_i64}
    }}))
    .unwrap();
    assert!(usage.plan.is_none());
    assert_eq!(usage.fetched_at, Some(1_792_094_400_000));
    assert_eq!(usage.windows.len(), 2);
    assert_eq!(usage.windows[0].remaining_percent, Some(100.0));
    assert_eq!(usage.windows[0].duration_seconds, Some(18_000.0));
    assert_eq!(usage.windows[1].remaining_percent, Some(100.0));
}

#[tokio::test]
async fn cancelled_and_late_logins_cannot_replace_the_current_account() {
    let mut process = host("").await;
    let state = super::MuseState::default();
    pending(&state, &process, "newer").await;
    let mut status = runtime();
    status.authenticated = true;
    status.email = Some("current@example.test".into());
    assert!(super::apply_login(&state, "older", status.clone())
        .await
        .is_none());
    assert!(super::take_login(&state, "older").await.is_none());
    assert!(super::apply_login(&state, "newer", status.clone())
        .await
        .is_some());
    pending(&state, &process, "cancelled").await;
    assert!(super::take_login(&state, "cancelled").await.is_some());
    status.email = Some("late@example.test".into());
    assert!(super::apply_login(&state, "cancelled", status)
        .await
        .is_none());
    assert_eq!(
        state
            .runtime
            .lock()
            .await
            .value
            .as_ref()
            .unwrap()
            .email
            .as_deref(),
        Some("current@example.test")
    );
    process.cancel().await.unwrap();
}

#[tokio::test]
async fn cancelling_device_login_only_cancels_the_matching_flow_and_never_logs_out() {
    let mut process = host("").await;
    let state = super::MuseState::default();
    pending(&state, &process, "active").await;
    super::cancel_login(&state, "stale").await.unwrap();
    assert!(state.login.lock().await.is_some());
    super::cancel_login(&state, "active").await.unwrap();
    assert!(state.login.lock().await.is_none());
    let methods = process
        .control()
        .request("fixture/methods", &json!({}))
        .await
        .unwrap();
    assert_eq!(
        methods["methods"],
        json!([
            "initialize",
            "initialized",
            "account/loginCancel",
            "fixture/methods"
        ])
    );
    process.cancel().await.unwrap();
}

#[tokio::test]
async fn quota_probe_errors_preserve_only_the_confirmed_account_observation() {
    let home = tempfile::tempdir().unwrap();
    let account: Value = serde_json::from_str(include_str!("fixtures/account.json")).unwrap();
    let value: Value = serde_json::from_str(include_str!("fixtures/usage.json")).unwrap();
    usage::observed(home.path(), &account);
    usage::observe(home.path(), &account, &value["usage"]).unwrap();
    let mut process = host("usageUnavailable").await;
    let usage = usage::read_current(&mut process, home.path())
        .await
        .unwrap();
    assert_eq!(usage.fetched_at, Some(1_792_094_400_000));
    assert_eq!(usage.windows[0].remaining_percent, Some(77.0));
    assert!(usage.error.is_some());
    process.cancel().await.unwrap();
}

#[tokio::test]
async fn a_new_host_without_usage_keeps_the_observed_turn_quota_for_the_same_account() {
    let home = tempfile::tempdir().unwrap();
    let account: Value = serde_json::from_str(include_str!("fixtures/account.json")).unwrap();
    let value: Value = serde_json::from_str(include_str!("fixtures/usage.json")).unwrap();
    usage::observed(home.path(), &account);
    usage::observe(home.path(), &account, &value["usage"]).unwrap();
    let mut process = host("usageEmpty").await;
    let usage = usage::read_current(&mut process, home.path())
        .await
        .unwrap();
    assert_eq!(usage.fetched_at, Some(1_792_094_400_000));
    assert_eq!(usage.windows[0].remaining_percent, Some(77.0));
    assert!(usage.error.is_none());
    process.cancel().await.unwrap();
}

#[test]
fn observations_are_scoped_to_the_current_account_and_cleared_on_logout() {
    let home = tempfile::tempdir().unwrap();
    let account: Value = serde_json::from_str(include_str!("fixtures/account.json")).unwrap();
    let other =
        json!({"state":"accountLogin","credentialRequired":true,"label":"other@example.test"});
    let value: Value = serde_json::from_str(include_str!("fixtures/usage.json")).unwrap();
    usage::observed(home.path(), &account);
    usage::observe(home.path(), &account, &value).unwrap();
    assert!(usage::observed(home.path(), &account).is_some());
    assert!(usage::observed(home.path(), &other).is_none());
    usage::observe(home.path(), &account, &value).unwrap();
    assert!(usage::observed(home.path(), &other).is_none());
    usage::observe(home.path(), &other, &value).unwrap();
    assert!(usage::observed(home.path(), &other).is_some());
    usage::clear(home.path());
    usage::observe(home.path(), &other, &value).unwrap();
    assert!(usage::observed(home.path(), &other).is_none());
    assert!(usage::observed(
        home.path(),
        &json!({"state":"loggedOut","credentialRequired":true})
    )
    .is_none());
}

#[test]
fn older_usage_events_never_overwrite_a_newer_provider_observation() {
    let home = tempfile::tempdir().unwrap();
    let account: Value = serde_json::from_str(include_str!("fixtures/account.json")).unwrap();
    let value: Value = serde_json::from_str(include_str!("fixtures/usage.json")).unwrap();
    let mut newer = value["usage"].clone();
    newer["observedAtMs"] = json!(1_792_094_405_000_i64);
    newer["window"]["usedPercent"] = json!(20);
    usage::observed(home.path(), &account);
    usage::observe(home.path(), &account, &newer).unwrap();
    usage::observe(home.path(), &account, &value["usage"]).unwrap();
    let usage = usage::observed(home.path(), &account).unwrap();
    assert_eq!(usage.fetched_at, Some(1_792_094_405_000));
    assert_eq!(usage.windows[0].remaining_percent, Some(80.0));
}

#[test]
fn malformed_newer_usage_does_not_replace_confirmed_windows() {
    let home = tempfile::tempdir().unwrap();
    let account: Value = serde_json::from_str(include_str!("fixtures/account.json")).unwrap();
    let value: Value = serde_json::from_str(include_str!("fixtures/usage.json")).unwrap();
    usage::observed(home.path(), &account);
    usage::observe(home.path(), &account, &value).unwrap();
    assert!(usage::observe(
        home.path(),
        &account,
        &json!({"observedAtMs":1_792_094_405_000_i64})
    )
    .is_err());
    assert_eq!(
        usage::observed(home.path(), &account).unwrap().fetched_at,
        Some(1_792_094_400_000)
    );
}

#[test]
fn observations_are_not_shared_with_another_profile_or_an_unidentified_account() {
    let home = tempfile::tempdir().unwrap();
    let other_home = tempfile::tempdir().unwrap();
    let account: Value = serde_json::from_str(include_str!("fixtures/account.json")).unwrap();
    let value: Value = serde_json::from_str(include_str!("fixtures/usage.json")).unwrap();
    usage::observed(home.path(), &account);
    usage::observe(home.path(), &account, &value).unwrap();
    assert!(usage::observed(other_home.path(), &account).is_none());
    let anonymous = json!({"state":"accountLogin","credentialRequired":true});
    assert!(usage::observed(home.path(), &anonymous).is_none());
    usage::observe(home.path(), &anonymous, &value).unwrap();
    assert!(usage::observed(home.path(), &account).is_none());
}

#[tokio::test]
async fn metadata_retains_usage_notifications_that_arrive_before_account_read() {
    let home = tempfile::tempdir().unwrap();
    let mut process = host("notificationUsageOnly").await;
    let status = metadata::read_status_at(&mut process, None, home.path())
        .await
        .unwrap();
    assert_eq!(status.subscription_type.as_deref(), Some("example-plan"));
    let account: Value = serde_json::from_str(include_str!("fixtures/account.json")).unwrap();
    assert_eq!(
        usage::observed(home.path(), &account).unwrap().fetched_at,
        Some(1_792_094_400_000)
    );
    process.cancel().await.unwrap();
}

#[tokio::test]
async fn terminal_login_events_are_sanitized_and_local_cancellation_stops_waiting() {
    for (outcome, granted) in [
        ("granted", true),
        ("denied", false),
        ("expired", false),
        ("failed", false),
    ] {
        let mut process = host("").await;
        let (_stop, mut stopped) = tokio::sync::watch::channel(false);
        process
            .control()
            .request("fixture/loginCompleted", &json!({"outcome":outcome}))
            .await
            .unwrap();
        let result = super::wait_login(&mut process, &mut stopped).await;
        if granted {
            assert!(result.unwrap());
        } else {
            assert!(!result.unwrap_err().contains("private-account-data"));
        }
        process.cancel().await.unwrap();
    }
    let mut process = host("").await;
    let (stop, mut stopped) = tokio::sync::watch::channel(false);
    stop.send(true).unwrap();
    assert!(!super::wait_login(&mut process, &mut stopped).await.unwrap());
    process.cancel().await.unwrap();
}
