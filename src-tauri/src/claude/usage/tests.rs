use super::*;

fn payload() -> Value {
    json!({
        "subscription_type":"max", "rate_limits_available":true,
        "session":{"total_cost_usd":123,"model_usage":{"tokens":999999}},
        "rate_limits":{
            "five_hour":{"utilization":25,"resets_at":"2026-09-26T08:00:00Z"},
            "seven_day":{"utilization":80,"resets_at":"2026-09-30T12:00:00+02:00"},
            "seven_day_opus":null,
            "seven_day_sonnet":{"utilization":100,"resets_at":null},
            "model_scoped":[{"display_name":"Fable","utilization":12,"resets_at":null}],
            "extra_usage":{"is_enabled":true,"used_credits":250}
        }
    })
}

#[test]
fn native_subscription_windows_are_not_session_tokens_or_spending() {
    let usage = parse(&payload()).unwrap();
    assert_eq!(usage.alias, "Claude Code");
    assert_eq!(usage.plan.as_deref(), Some("max"));
    assert_eq!(usage.windows.len(), 4);
    assert_eq!(usage.windows[0].remaining_percent, Some(75.0));
    assert_eq!(usage.windows[0].duration_seconds, Some(18_000.0));
    assert_eq!(usage.windows[1].remaining_percent, Some(20.0));
    assert_eq!(usage.windows[1].duration_seconds, Some(604_800.0));
    assert_eq!(usage.windows[1].resets_at, Some(1_790_762_400_000));
    assert_eq!(usage.windows[2].remaining_percent, Some(0.0));
    assert_eq!(usage.windows[2].resets_at, None);
    assert_eq!(usage.windows[3].label, "Fable · 7d");
    assert_eq!(usage.windows[3].remaining_percent, Some(88.0));
    assert!(usage.reset_credits.is_none());
    assert!(usage.error.is_none());
}

#[test]
fn missing_and_invalid_limits_never_become_free_quota() {
    let unsupported = parse(&json!({"rate_limits_available":false,"rate_limits":null})).unwrap();
    assert!(unsupported.windows.is_empty());
    assert_eq!(unsupported.fetched_at, None);
    assert_eq!(unsupported.error.as_deref(), Some(UNSUPPORTED));
    for value in [
        json!({}),
        json!({"rate_limits_available":true,"rate_limits":null}),
    ] {
        assert!(parse(&value).is_err());
    }
    let mut value = payload();
    value["rate_limits"]["five_hour"]["utilization"] = json!("wrong");
    value["rate_limits"]["seven_day"]["utilization"] = json!(-1);
    value["rate_limits"]["seven_day_sonnet"] = json!({"utilization":101,"resets_at":"invalid"});
    let usage = parse(&value).unwrap();
    assert_eq!(usage.windows.len(), 3);
    assert_eq!(usage.windows[0].remaining_percent, None);
    assert_eq!(usage.windows[1].remaining_percent, None);
}

#[test]
fn native_probe_cache_throttles_errors_and_clears_data_when_no_longer_available() {
    let mut cache = Cache::default();
    assert!(cache.fresh().is_none());
    let first = cache.store(parse(&payload()));
    assert_eq!(cache.fresh().unwrap().windows.len(), 4);
    let failed = cache.store(Err(UNAVAILABLE.into()));
    assert_eq!(failed.fetched_at, first.fetched_at);
    assert_eq!(failed.windows[0].remaining_percent, Some(75.0));
    assert_eq!(cache.fresh().unwrap().error.as_deref(), Some(UNAVAILABLE));
    cache.attempted = Some(Instant::now() - Duration::from_secs(61));
    assert!(cache.fresh().is_none());
    cache.store(parse(&json!({"rate_limits_available":false})));
    assert!(cache.fresh().unwrap().windows.is_empty());
}

#[tokio::test]
async fn quota_probe_uses_controls_only_and_rejects_actions() {
    let mut process = super::super::tests::fixture(
        r#"
        const lines = require('node:readline').createInterface({ input: process.stdin });
        const send = value => process.stdout.write(JSON.stringify(value) + '\n');
        let count = 0;
        lines.on('line', line => {
            const message = JSON.parse(line);
            if (message.type === 'control_response') {
                if (message.response.subtype !== 'error') process.exit(97);
                return;
            }
            if (message.type !== 'control_request') process.exit(98);
            const type = message.request.subtype;
            if (type !== ['initialize','get_usage'][count++]) process.exit(99);
            if (type === 'initialize') send({type:'control_request',request_id:'denied',request:{subtype:'can_use_tool',tool_name:'Bash'}});
            send({type:'control_response',response:{subtype:'success',request_id:message.request_id,response:type === 'initialize' ? {account:{email:'account@example.test',accessToken:'do-not-expose'}} : {
                rate_limits_available:true,subscription_type:'pro',rate_limits:{five_hour:{utilization:40,resets_at:null}}
            }}});
        });
    "#,
    );
    let usage = tokio::time::timeout(Duration::from_secs(5), read_usage(&mut process))
        .await
        .unwrap()
        .unwrap();
    process.cancel().await.unwrap();
    assert_eq!(usage.email.as_deref(), Some("account@example.test"));
    assert_eq!(usage.windows[0].remaining_percent, Some(60.0));
    assert!(!serde_json::to_string(&usage)
        .unwrap()
        .contains("do-not-expose"));
}

#[tokio::test]
async fn unsupported_native_control_is_sanitized_without_sending_a_prompt() {
    let mut process = super::super::tests::fixture(
        r#"
        require('node:readline').createInterface({input:process.stdin}).on('line', line => {
            const message=JSON.parse(line);
            if (message.type!=='control_request') process.exit(99);
            process.stdout.write(JSON.stringify({type:'control_response',response:{subtype:'error',request_id:message.request_id,error:'Unsupported; private details'}})+'\n');
        });
    "#,
    );
    let result = read_usage(&mut process).await;
    process.cancel().await.unwrap();
    assert_eq!(result.unwrap_err(), UNAVAILABLE);
}

#[tokio::test]
#[ignore = "Requires the installed Claude Code CLI and its native authentication environment"]
async fn native_usage_probe_uses_installed_cli_without_inference() {
    let usage = query().await.unwrap();
    assert_eq!(usage.alias, "Claude Code");
    assert!(usage.error.as_deref() == Some(UNSUPPORTED) || !usage.windows.is_empty());
}
