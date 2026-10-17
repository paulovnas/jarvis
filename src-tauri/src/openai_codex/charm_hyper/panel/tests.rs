use super::*;

fn response() -> Value {
    json!({"userId":"12345678-1234-4234-8234-123456789012","teams":[{"id":"23456789-1234-4234-8234-123456789012","name":"My team"}],"teamId":"23456789-1234-4234-8234-123456789012","dashboard":{"known":true}})
}

#[test]
fn panel_identity_accepts_only_the_returned_team_and_no_extra_fields() {
    assert!(probe(response(), None, None).is_ok());
    assert!(probe(
        response(),
        Some("23456789-1234-4234-8234-123456789012"),
        Some("12345678-1234-4234-8234-123456789012")
    )
    .is_ok());
    assert!(probe(response(), Some("another-team"), None).is_err());
    assert!(probe(response(), None, Some("another-user")).is_err());
    let mut changed = response();
    changed["teamId"] = json!("34567890-1234-4234-8234-123456789012");
    assert!(probe(changed, None, None).is_err());
    let mut changed = response();
    changed["token"] = json!("must-never-return-a-token");
    assert!(probe(changed, None, None).is_err());
    let mut changed = response();
    changed["teams"][0]["id"] = json!("../../another-account");
    assert!(probe(changed, None, None).is_err());
}

#[test]
fn login_and_account_changes_are_explicit_and_never_expose_server_errors() {
    for error_code in [
        "login_required",
        "different_user",
        "different_team",
        "wrong_origin",
        "secret-server-response",
    ] {
        let error = probe(json!({"error":error_code}), None, None)
            .err()
            .unwrap();
        assert_eq!(error.code, "charm_hyper");
        assert!(!error.message.contains("secret-server-response"));
    }
}

#[test]
fn panel_binding_is_optional_in_old_credentials_and_survives_secure_serialization() {
    let mut credential =
        CodexCredential::new("test-key", "", i64::MAX, "charm-hyper:local-id", None, None);
    let serialized = serde_json::to_value(&credential).unwrap();
    assert!(serialized.get("hyperPanel").is_none());
    let old: CodexCredential = serde_json::from_value(serialized).unwrap();
    assert!(old.hyper_panel.is_none());
    credential.hyper_panel = Some(Connection {
        user_id: "user".into(),
        team_id: "team".into(),
        team_name: "My team".into(),
    });
    let saved: CodexCredential =
        serde_json::from_value(serde_json::to_value(&credential).unwrap()).unwrap();
    assert!(credential == saved);
    assert_eq!(saved.access, "test-key");
    assert_eq!(
        scope(&credential.account_id),
        "hyper-panel:charm-hyper:local-id"
    );
}

#[test]
fn stale_authorization_cannot_bind_a_replaced_key_or_account() {
    let record = ProviderAccountRecord {
        alias: "charm-hyper-test".into(),
        display_name: None,
        provider_kind: "charm-hyper".into(),
        account_id: "charm-hyper:local-id".into(),
        created_at: 1234,
        enabled: true,
        show_usage: true,
        show_five_hour_usage: true,
        show_weekly_usage: true,
        show_third_party_usage: false,
        usage_alert_window: None,
        usage_alert_threshold: None,
    };
    let credential = CodexCredential::new(
        "initial-key",
        "",
        i64::MAX,
        record.account_id.clone(),
        None,
        None,
    );
    let flow = Flow {
        id: "flow".into(),
        account_id: record.account_id.clone(),
        created_at: record.created_at,
        key_hash: Sha256::digest(credential.access.as_bytes()).to_vec(),
        started: Instant::now(),
    };
    assert!(matches(&flow, &record, &credential));
    let mut changed = credential.clone();
    changed.access = "replacement-key".into();
    assert!(!matches(&flow, &record, &changed));
    let mut recreated = record.clone();
    recreated.created_at += 1;
    assert!(!matches(&flow, &recreated, &credential));
    let expired = Flow {
        started: Instant::now() - Duration::from_secs(601),
        ..flow
    };
    assert!(!matches(&expired, &record, &credential));
}
