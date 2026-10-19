use super::*;
use serde_json::json;
use std::{collections::HashSet, path::PathBuf, time::Duration};

const SESSION: &str = "01a14128-b34c-7281-b020-39f7fdc25e02";
const TURN: &str = "01a14128-b4f0-7a4c-b599-adbf43e8c467";

#[test]
fn private_profile_keeps_native_auth_and_all_mutable_settings_inside_jarvis() {
    let home = tempfile::tempdir().unwrap();
    let global = home.path().join(".config/muse");
    std::fs::create_dir_all(&global).unwrap();
    std::fs::write(global.join("auth.json"), "fixture-global-credential").unwrap();
    let mut config = MuseSpawnConfig::new("muse".into(), home.path().into());
    configure(&mut config, home.path(), &[]).unwrap();
    let base = crate::data_dir::root(home.path()).join("muse");
    for (key, suffix) in [
        ("XDG_CONFIG_HOME", "config"),
        ("XDG_DATA_HOME", "data"),
        ("XDG_STATE_HOME", "state"),
    ] {
        let value = config
            .env
            .iter()
            .find(|(name, _)| name == key)
            .unwrap()
            .1
            .clone();
        assert_eq!(PathBuf::from(value), base.join(suffix));
        assert!(base.join(suffix).is_dir());
    }
    assert_eq!(
        PathBuf::from(
            config
                .env
                .iter()
                .find(|(k, _)| k == "MUSE_AUTH_PATH")
                .unwrap()
                .1
                .clone()
        ),
        base.join("config/muse/auth.json")
    );
    assert!(!base.join("config/muse/auth.json").exists());
    assert_eq!(
        std::fs::read_to_string(global.join("auth.json")).unwrap(),
        "fixture-global-credential"
    );
    assert!(!config
        .env
        .iter()
        .any(|(k, _)| k == "HOME" || k == "CODEX_HOME"));
    let args: Vec<_> = config
        .args
        .iter()
        .map(|arg| arg.to_string_lossy())
        .collect();
    for required in [
        "--disable-write",
        "--disable-shell",
        "run.toolset=[]",
        "run.reminder_roster=[]",
        "run.parallel_tool_calls=false",
        "run.workflow_trigger_mode=\"off\"",
        "context.foreign_personal_rules=false",
        "context.foreign_personal_skills=false",
        "tools.web_search.mode=\"off\"",
    ] {
        assert!(args.iter().any(|arg| arg == required), "missing {required}");
    }
    assert!(!args
        .iter()
        .any(|arg| arg == "--trust-workspace" || arg == "--no-session-log"));
    let mut search = MuseSpawnConfig::new("muse".into(), home.path().into());
    configure(&mut search, home.path(), &["web_search".into()]).unwrap();
    assert!(search
        .args
        .iter()
        .any(|arg| arg == "tools.web_search.mode=\"hosted\""));
    assert!(configure(&mut search, home.path(), &["shell".into()]).is_err());
}

#[test]
fn jarvis_mcp_keeps_write_and_shell_in_jarvis_without_native_read_only_guidance() {
    let home = tempfile::tempdir().unwrap();
    let mut config = MuseSpawnConfig::new("muse".into(), home.path().into());
    configure(&mut config, home.path(), &["mcp__jarvis__*".into()]).unwrap();
    let args: Vec<_> = config
        .args
        .iter()
        .map(|arg| arg.to_string_lossy())
        .collect();
    for forbidden in [
        "--disable-write",
        "--disable-shell",
        "--trust-workspace",
        "--disable-sandbox",
    ] {
        assert!(
            !args.iter().any(|arg| arg == forbidden),
            "unexpected {forbidden}"
        );
    }
    assert!(args
        .iter()
        .any(|arg| arg == "run.toolset=[\"mcp__jarvis__*\"]"));
    assert!(args.iter().any(|arg| arg == "--disable-request-user-input"));
}

#[test]
fn command_ids_are_fresh_uuid_v7_values() {
    let mut ids = HashSet::new();
    for _ in 0..100 {
        let id = command_id().unwrap();
        assert_eq!(id.len(), 36);
        assert_eq!(&id[14..15], "7");
        assert!(matches!(id.as_bytes()[19], b'8' | b'9' | b'a' | b'b'));
        assert!(ids.insert(id));
    }
}

#[test]
fn turn_input_is_ordered_typed_and_rejects_executable_or_invalid_parts() {
    let image = json!({"type":"image","base64Data":"aGVsbG8=","mediaType":"image/png","width":1,"height":1});
    assert_eq!(
        turn_input(
            "System task",
            &json!([{"type":"text","text":"User input"},image])
        )
        .unwrap(),
        json!([{"type":"text","text":"System task"},{"type":"text","text":"User input"},image])
    );
    for input in [
        Value::Null,
        json!([]),
        json!([{"type":"skill","selector":"unsafe"}]),
        json!([{"type":"image","base64Data":"%%%","mediaType":"image/png"}]),
        json!([{"type":"image","base64Data":"aGVsbG8=","mediaType":"image/png","width":1}]),
    ] {
        assert!(turn_input("", &input).is_err());
    }
}

#[test]
fn subscription_and_exact_fresh_model_effort_are_required() {
    assert!(validate_account(&json!({"state":"accountLogin"})).is_ok());
    for state in ["apiKey", "envKey", "loggedOut", "unknown"] {
        assert!(validate_account(&json!({"state":state})).is_err());
    }
    let catalog = json!({"providerId":"meta","models":[{"providerId":"meta","modelId":"muse-fixture","variants":["low","high"]}]});
    assert!(validate_model(&catalog, "muse-fixture", Some("low")).is_ok());
    assert!(validate_model(&catalog, "another-model", None).is_err());
    assert!(validate_model(&catalog, "muse-fixture", Some("ultra")).is_err());
    assert!(validate_model(
        &json!({"providerId":"echo","models":catalog["models"]}),
        "muse-fixture",
        None
    )
    .is_err());
}

#[test]
fn session_identity_cannot_switch_workspace_model_or_provider() {
    let root = tempfile::tempdir().unwrap();
    let value = json!({"viewCursor":"v1","session":{"sessionId":SESSION,"workspaceRoot":root.path(),"providerId":"meta","modelId":"muse-fixture"}});
    assert_eq!(
        session_id(&value, root.path(), "muse-fixture", Some(SESSION)).unwrap(),
        SESSION
    );
    for (field, bad) in [
        ("workspaceRoot", json!(std::env::temp_dir())),
        ("modelId", json!("another-model")),
        ("providerId", json!("echo")),
        ("sessionId", json!("bad-id")),
    ] {
        let mut changed = value.clone();
        changed["session"][field] = bad;
        assert!(session_id(&changed, root.path(), "muse-fixture", None).is_err());
    }
    assert!(session_id(&value, root.path(), "muse-fixture", Some(TURN)).is_err());
}

#[test]
fn turn_acknowledgement_requires_one_confirmed_new_turn_for_the_requested_command() {
    let ack = json!({"commandId":SESSION,"turnId":TURN,"status":"accepted","disposition":"started","startedNewTurn":true});
    assert_eq!(turn_id(&ack, SESSION).unwrap(), TURN);
    for (field, value) in [
        ("commandId", json!(TURN)),
        ("turnId", json!("invalid")),
        ("status", json!("rejected")),
        ("disposition", json!("queued")),
        ("startedNewTurn", json!(false)),
    ] {
        let mut changed = ack.clone();
        changed[field] = value;
        assert!(turn_id(&changed, SESSION).is_err());
    }
}

fn notification(method: &str, item: Value) -> Value {
    json!({"jsonrpc":"2.0","method":method,"params":{"sessionId":SESSION,"item":item}})
}
fn answer(text: &str) -> Value {
    notification(
        "item/completed",
        json!({"itemId":"answer","kind":"agentMessage","turnId":TURN,"status":"completed","messagePhase":"finalAnswer","text":text}),
    )
}
fn receipt(results: Value) -> Value {
    notification(
        "item/completed",
        json!({"itemId":"search","kind":"webSearch","turnId":TURN,"status":"completed","action":"search","providerStatus":"completed","query":"official URL","results":results}),
    )
}
fn terminal(state: &str) -> Value {
    json!({"method":"turn/completed","params":{"sessionId":SESSION,"turnId":TURN,"terminal":state,"error":{"message":"private-secret"}}})
}

#[test]
fn search_sources_come_only_from_completed_native_receipts_and_safe_urls() {
    let mut collector = Collector::new(SESSION, TURN, true, 2);
    collector
        .apply(&answer("Verified answer, without fabricated links."))
        .unwrap();
    collector
        .apply(&receipt(json!([
            {"title":"Official","url":"https://example.org/official"},
            {"title":"duplicate","url":"https://example.org/official"},
            {"title":"bad","url":"javascript:alert(1)"},
            {"title":"credentials","url":"https://user:secret@example.org/"},
            {"title":"Second","url":"http://example.net/second"},
            {"title":"Third","url":"https://example.net/third"}
        ])))
        .unwrap();
    assert!(collector.apply(&terminal("completed")).unwrap());
    let output = collector.finish().unwrap();
    assert_eq!(output.answer, "Verified answer, without fabricated links.");
    assert_eq!(output.sources,json!([{"title":"Official","url":"https://example.org/official"},{"title":"Second","url":"http://example.net/second"}]).as_array().unwrap().clone());
}

#[test]
fn unobserved_failed_or_foreign_searches_never_report_success() {
    for kind in ["absent", "failed", "foreign"] {
        let mut collector = Collector::new(SESSION, TURN, true, 3);
        collector
            .apply(&answer("Plausible answer https://invented.invalid"))
            .unwrap();
        if kind != "absent" {
            let mut e = receipt(json!([{"title":"Source","url":"https://example.org"}]));
            if kind == "failed" {
                e["params"]["item"]["providerStatus"] = json!("failed");
            } else {
                e["params"]["sessionId"] = json!("foreign-session");
            }
            collector.apply(&e).unwrap();
        }
        collector.apply(&terminal("completed")).unwrap();
        assert!(collector.finish().is_err());
    }
}

#[test]
fn authoritative_final_text_is_not_duplicated_or_replaced_by_commentary() {
    let mut c = Collector::new(SESSION, TURN, false, 0);
    c.apply(&json!({"method":"item/delta","params":{"sessionId":SESSION,"itemId":"answer","delta":"part"}})).unwrap();
    c.apply(&answer("Final answer")).unwrap();
    c.apply(&answer("Final answer")).unwrap();
    let mut commentary = answer("Background detail");
    commentary["params"]["item"]["itemId"] = json!("commentary");
    commentary["params"]["item"]["messagePhase"] = json!("commentary");
    c.apply(&commentary).unwrap();
    c.apply(&terminal("completed")).unwrap();
    assert_eq!(c.finish().unwrap().answer, "Final answer");
    let mut c = Collector::new(SESSION, TURN, false, 0);
    assert!(c.apply(&answer(&"x".repeat(262_145))).is_err());
    let mut c = Collector::new(SESSION, TURN, false, 0);
    let error = c.apply(&terminal("failed")).unwrap_err();
    assert!(!error.contains("private-secret"));
}

#[test]
fn native_echo_recording_replays_the_authoritative_completion_contract() {
    // Captured from official Muse 1.4.9 with the verified private-profile baseline, 2026-10-15.
    let frames: Vec<Value> = include_str!("fixtures/native-echo.ndjson")
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let terminal = frames
        .iter()
        .find(|frame| frame["method"] == "turn/completed")
        .unwrap();
    let mut collector = Collector::new(
        terminal["params"]["sessionId"].as_str().unwrap(),
        terminal["params"]["turnId"].as_str().unwrap(),
        false,
        0,
    );
    for frame in frames {
        collector.apply(&frame).unwrap();
    }
    assert_eq!(
        collector.finish().unwrap().answer,
        "echo: Hello isolated Jarvis."
    );
}

fn host(root: &Path, mode: &str) -> MuseSpawnConfig {
    let mut c = MuseSpawnConfig::new("node".into(), root.into());
    c.args = vec![
        "-e".into(),
        include_str!("fixtures/host.cjs").into(),
        mode.into(),
    ];
    c.request_timeout = Duration::from_secs(2);
    c
}

#[tokio::test]
async fn jarvis_tools_require_the_hosts_explicit_session_mcp_grant() {
    let root = tempfile::tempdir().unwrap();
    for mode in ["ok", "noSessionMcp"] {
        let mut process = MuseProcess::spawn(host(root.path(), mode)).unwrap();
        let result = initialize_tools(&mut process).await;
        assert_eq!(result.is_ok(), mode == "ok");
        process.cancel().await.unwrap();
    }
}

#[tokio::test]
async fn synthesis_uses_one_exact_session_and_search_requires_a_receipt() {
    let root = tempfile::tempdir().unwrap();
    let (_sender, signal) = watch::channel(false);
    let out = run_config(
        root.path(),
        host(root.path(), "noSearch"),
        "muse-fixture",
        Some("low"),
        "Task",
        &json!([{"type":"text","text":"Input"}]),
        signal.clone(),
        None,
    )
    .await
    .unwrap();
    assert_eq!(out.answer, "Fixture verified answer.");
    let out = run_config(
        root.path(),
        host(root.path(), "ok"),
        "muse-fixture",
        Some("low"),
        "Search",
        &json!([{"type":"text","text":"official source"}]),
        signal.clone(),
        Some(2),
    )
    .await
    .unwrap();
    assert_eq!(out.sources.len(), 1);
    assert!(run_config(
        root.path(),
        host(root.path(), "noSearch"),
        "muse-fixture",
        Some("low"),
        "Search",
        &json!([{"type":"text","text":"source"}]),
        signal.clone(),
        Some(2)
    )
    .await
    .is_err());
    for mode in ["changedModel", "changedProvider"] {
        assert!(run_config(
            root.path(),
            host(root.path(), mode),
            "muse-fixture",
            Some("low"),
            "Task",
            &json!([]),
            signal.clone(),
            None,
        )
        .await
        .is_err());
    }
}

#[tokio::test]
async fn api_key_lane_is_rejected_before_session_creation() {
    let root = tempfile::tempdir().unwrap();
    let marker = root.path().join("marker");
    let mut c = host(root.path(), "apiKey");
    c.env.push((
        "JARVIS_MSP_SESSION_MARKER".into(),
        marker.clone().into_os_string(),
    ));
    let (_sender, signal) = watch::channel(false);
    assert!(run_config(
        root.path(),
        c,
        "muse-fixture",
        Some("low"),
        "Task",
        &json!([]),
        signal,
        None
    )
    .await
    .is_err());
    assert!(!marker.exists());
}

#[tokio::test]
async fn hosted_search_uses_each_jarvis_language_and_returns_confirmed_sources() {
    let root = tempfile::tempdir().unwrap();
    let (_sender, signal) = watch::channel(false);
    for language in ["pt-BR", "en", "es", "fr", "de", "it", "ja", "zh-CN"] {
        let language: crate::system::ResponseLanguage =
            serde_json::from_value(json!(language)).unwrap();
        let mut config = host(root.path(), "ok");
        config.env.push((
            "JARVIS_MSP_EXPECT_LANGUAGE".into(),
            language.prompt_instruction().into(),
        ));
        let output = search_with_config(
            root.path(),
            config,
            "muse-fixture",
            "official source",
            2,
            language,
            signal.clone(),
        )
        .await
        .unwrap();
        assert_eq!(output["accountAlias"], "Muse Code");
        assert_eq!(output["answer"], "Fixture verified answer.");
        assert_eq!(output["sources"][0]["url"], "https://example.org/official");
        let usage = super::super::usage::observed(
            root.path(),
            &json!({"state":"accountLogin","label":"session-fixture"}),
        )
        .unwrap();
        assert!(usage.fetched_at.is_some());
        assert_eq!(usage.windows.len(), 2);
    }
}

#[tokio::test]
async fn search_cancellation_interrupts_the_confirmed_turn_before_killing_owned_host() {
    let root = tempfile::tempdir().unwrap();
    let marker = root.path().join("marker");
    let mut c = host(root.path(), "hold");
    c.env.push((
        "JARVIS_MSP_SESSION_MARKER".into(),
        marker.clone().into_os_string(),
    ));
    let (sender, signal) = watch::channel(false);
    let home = root.path().to_owned();
    let running = tokio::spawn(async move {
        search_with_config(
            &home,
            c,
            "muse-fixture",
            "official source",
            2,
            crate::system::ResponseLanguage::PortugueseBrazil,
            signal,
        )
        .await
    });
    tokio::time::timeout(Duration::from_secs(5), async {
        while super::super::usage::latest(
            root.path(),
            &json!({"state":"accountLogin","label":"session-fixture"}),
        )
        .is_none()
        {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    sender.send(true).unwrap();
    assert!(running.await.unwrap().is_err());
    assert!(std::fs::read_to_string(marker)
        .unwrap()
        .contains("turn/interrupt"));
}

#[tokio::test]
async fn search_rejects_unbounded_or_empty_queries_without_spawning() {
    let home = tempfile::tempdir().unwrap();
    let (_sender, signal) = watch::channel(false);
    for (query, limit) in [("", 2), ("query", 0), ("query", 11), (&"x".repeat(2001), 2)] {
        assert!(search(
            home.path(),
            "muse-fixture",
            query,
            limit,
            crate::system::ResponseLanguage::PortugueseBrazil,
            signal.clone()
        )
        .await
        .is_err());
    }
}
