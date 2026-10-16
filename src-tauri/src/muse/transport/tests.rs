use super::*;
use serde_json::json;

fn fixture(mode: &str) -> Result<MuseProcess, String> {
    let mut config = MuseSpawnConfig::new("node".into(), std::env::temp_dir());
    config.args = vec![
        "-e".into(),
        include_str!("fixtures/host.cjs").into(),
        mode.into(),
        include_str!("fixtures/official-handshake.ndjson").into(),
    ];
    config.request_timeout = Duration::from_secs(1);
    MuseProcess::spawn(config)
}

async fn initialized(mode: &str) -> MuseProcess {
    let mut process = fixture(mode).unwrap();
    process.initialize(&json!({})).await.unwrap();
    event(&mut process).await;
    process
}

async fn event(process: &mut MuseProcess) -> Value {
    tokio::time::timeout(Duration::from_secs(5), process.next_event())
        .await
        .unwrap()
        .unwrap()
        .unwrap()
}

#[tokio::test]
async fn native_subscription_spawn_removes_api_key_and_preserves_the_user_environment() {
    let mut config = MuseSpawnConfig::new("node".into(), std::env::temp_dir());
    config.env = vec![
        ("META_API_KEY".into(), "fake-key-must-be-removed".into()),
        ("JARVIS_MSP_TEST_SETTING".into(), "private-profile".into()),
    ];
    config.args = vec!["-e".into(), "process.stdout.write(JSON.stringify({hasApiKey:'META_API_KEY' in process.env,path:process.env.PATH,setting:process.env.JARVIS_MSP_TEST_SETTING}))".into()];
    let mut command = command_for(&config).unwrap();
    assert!(command
        .as_std()
        .get_envs()
        .any(|(key, value)| key == "META_API_KEY" && value.is_none()));
    let output = command.output().await.unwrap();
    assert!(output.status.success());
    let environment: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(environment["hasApiKey"], false);
    assert_eq!(environment["setting"], "private-profile");
    assert_eq!(
        environment["path"].as_str(),
        std::env::var("PATH").ok().as_deref()
    );
}

#[tokio::test]
async fn handshake_uses_native_identity_and_sends_initialized_before_other_requests() {
    let result = fixture("");
    assert!(
        result.is_ok(),
        "official MSP child must be launchable: {}",
        result.as_ref().err().cloned().unwrap_or_default()
    );
    let mut process = result.unwrap();
    let result = process.initialize(&json!({
        "clientInfo": {"name": "foreign-client", "version": "1"},
        "capabilities": {"experimentalApi": true, "requestedCapabilities": ["sessionListStream"]}
    })).await.unwrap();
    assert_eq!(result["schema"]["version"], 1);
    assert_eq!(result["grantedCapabilities"], json!(["sessionListStream"]));
    let notification = event(&mut process).await;
    assert_eq!(notification["method"], "fixture/initialized");
    assert_eq!(
        notification["params"]["initialization"]["clientInfo"]["name"],
        "jarvis"
    );
    assert_eq!(
        notification["params"]["initialization"]["clientInfo"]["title"],
        "Jarvis"
    );
    assert!(process
        .control()
        .request("fixture/echo", &json!({"ok":true}))
        .await
        .is_ok());
    assert!(process.initialize(&json!({})).await.is_err());
    process.cancel().await.unwrap();
}

#[tokio::test]
async fn commands_are_gated_until_handshake_and_unknown_envelope_versions_are_refused() {
    let process = fixture("").unwrap();
    assert!(process
        .control()
        .request("fixture/echo", &json!({}))
        .await
        .is_err());
    assert!(process
        .control()
        .notify("fixture/echo", &json!({}))
        .await
        .is_err());
    let process = fixture("version").unwrap();
    assert!(process.initialize(&json!({})).await.is_err());
    assert!(process
        .control()
        .request("fixture/echo", &json!({}))
        .await
        .is_err());
}

#[tokio::test]
async fn official_pinned_handshake_transcript_accepts_a_different_compatible_fingerprint() {
    // Transcript: meta-models/muse-code-sdk@bb863c9071d42d9f67fcc3561690425646f727c8,
    // schema/msp/transcripts/handshake-liststream-granted/transcript.ndjson.
    let mut process = fixture("official").unwrap();
    let result = process
        .initialize(&json!({
            "capabilities": {"requestedCapabilities":["sessionListStream"]}
        }))
        .await
        .unwrap();
    assert_eq!(
        result["schema"]["fingerprint"],
        "sha256:c8d1a2a1866814e220fd396d382a9a75861412feee884b5021b2ee359bd3dc59"
    );
    assert_eq!(event(&mut process).await["method"], "fixture/initialized");
    process.cancel().await.unwrap();
}

#[tokio::test]
async fn concurrent_requests_match_exact_ids_despite_reversed_responses() {
    let mut process = initialized("").await;
    let control = process.control();
    let first_params = json!({"order":1});
    let second_params = json!({"order":2});
    let (first, second) = tokio::join!(
        control.request("fixture/reverse", &first_params),
        control.request("fixture/reverse", &second_params)
    );
    assert_eq!(first.unwrap(), json!({"order":1}));
    assert_eq!(second.unwrap(), json!({"order":2}));
    assert_eq!(
        control
            .request("fixture/wrongType", &json!({}))
            .await
            .unwrap(),
        json!({"correct":true})
    );
    process.cancel().await.unwrap();
}

#[tokio::test]
async fn peer_requests_preserve_numeric_and_string_ids_and_can_only_be_answered_once() {
    let mut process = initialized("").await;
    let control = process.control();
    control.request("fixture/peer", &json!({})).await.unwrap();
    let number = event(&mut process).await;
    let string = event(&mut process).await;
    assert_eq!(number["id"], 9);
    assert_eq!(string["id"], "9");
    assert!(control
        .respond(&number["id"], &json!({"large":"x".repeat(10_485_760)}))
        .await
        .is_err());
    control
        .respond(&number["id"], &json!({"ok":true}))
        .await
        .unwrap();
    control
        .reject(&string["id"], -32601, "Unsupported method")
        .await
        .unwrap();
    assert!(control.respond(&number["id"], &json!({})).await.is_err());
    let replies = event(&mut process).await;
    assert_eq!(replies["params"]["replies"][0]["id"], 9);
    assert_eq!(replies["params"]["replies"][1]["id"], "9");
    process.cancel().await.unwrap();
}

#[tokio::test]
async fn rpc_errors_keep_codes_without_echoing_host_secrets() {
    let mut process = initialized("").await;
    let error = process
        .control()
        .request("fixture/error", &json!({}))
        .await
        .unwrap_err();
    assert!(error.contains("-32601"));
    assert!(!error.contains("private-secret"));
    assert!(process
        .control()
        .request("fixture/echo", &json!({}))
        .await
        .is_ok());
    process.cancel().await.unwrap();
}

#[tokio::test]
async fn malformed_oversized_ambiguous_and_partial_frames_settle_pending_requests() {
    for method in [
        "fixture/malformed",
        "fixture/oversize",
        "fixture/invalid",
        "fixture/partial",
        "fixture/eof",
    ] {
        let mut process = initialized("").await;
        let control = process.control();
        let error =
            tokio::time::timeout(Duration::from_secs(3), control.request(method, &json!({})))
                .await
                .unwrap()
                .unwrap_err();
        assert!(!error.contains("private-secret"));
        assert!(control.request("fixture/echo", &json!({})).await.is_err());
        process.cancel().await.unwrap();
    }
}

#[tokio::test]
async fn timeout_does_not_retry_or_leave_pending_request_entries() {
    let mut process = initialized("").await;
    let control = process.control();
    assert!(control
        .request("fixture/hold", &json!({}))
        .await
        .unwrap_err()
        .contains("prazo"));
    assert_eq!(
        control.request("fixture/count", &json!({})).await.unwrap()["holds"],
        1
    );
    assert!(control
        .request("fixture/echo", &json!({"still": "usable"}))
        .await
        .is_ok());
    process.cancel().await.unwrap();
}

#[tokio::test]
async fn dropping_request_futures_releases_pending_slots() {
    let mut process = initialized("").await;
    let control = process.control();
    for _ in 0..70 {
        assert!(tokio::time::timeout(
            Duration::from_millis(5),
            control.request("fixture/hold", &json!({}))
        )
        .await
        .is_err());
    }
    assert!(control.request("fixture/echo", &json!({})).await.is_ok());
    process.cancel().await.unwrap();
}

#[tokio::test]
async fn crlf_and_blank_lines_are_tolerated_without_changing_utf8_payloads() {
    let mut process = initialized("crlf").await;
    let content = json!({"text":"Olá 👩🏽‍💻\nsegunda linha"});
    assert_eq!(
        process
            .control()
            .request("fixture/echo", &content)
            .await
            .unwrap(),
        json!({"params":content})
    );
    process.cancel().await.unwrap();
}

#[tokio::test]
async fn blocked_stdin_times_out_and_cancellation_never_waits_for_the_writer() {
    let mut process = initialized("blocked").await;
    let control = process.control();
    let error = tokio::time::timeout(
        Duration::from_secs(3),
        control.request(
            "fixture/echo",
            &json!({"large":"x".repeat(5 * 1024 * 1024)}),
        ),
    )
    .await
    .unwrap()
    .unwrap_err();
    assert!(error.contains("prazo"));
    assert!(control.request("fixture/echo", &json!({})).await.is_err());
    tokio::time::timeout(Duration::from_secs(2), process.cancel())
        .await
        .unwrap()
        .unwrap();
}

#[tokio::test]
async fn invalid_method_params_and_oversized_outbound_frames_are_not_written() {
    let mut process = initialized("").await;
    let control = process.control();
    for (method, params) in [
        ("", json!({})),
        ("fixture/echo\n", json!({})),
        ("fixture/echo", Value::Null),
        ("fixture/echo", json!([])),
        ("fixture/echo", json!({"large":"x".repeat(10_485_760)})),
    ] {
        assert!(control.request(method, &params).await.is_err());
    }
    assert!(control.request("fixture/echo", &json!({})).await.is_ok());
    process.cancel().await.unwrap();
}

#[tokio::test]
async fn event_backpressure_closes_transport_instead_of_blocking_rpc_forever() {
    let mut process = initialized("").await;
    let control = process.control();
    assert!(control.request("fixture/burst", &json!({})).await.is_err());
    let mut received = 0;
    loop {
        match process.next_event().await {
            Ok(Some(_)) => received += 1,
            Err(error) => {
                assert!(error.contains("fila"));
                break;
            }
            Ok(None) => panic!("event overflow must report why the host was disconnected"),
        }
    }
    assert!(received <= 64);
    process.cancel().await.unwrap();
}

#[tokio::test]
async fn cloned_control_cancellation_settles_inflight_requests() {
    let mut process = initialized("").await;
    let control = process.control();
    let other = control.clone();
    let request = tokio::spawn(async move { other.request("fixture/pending", &json!({})).await });
    assert_eq!(event(&mut process).await["method"], "fixture/holding");
    control.cancel().await.unwrap();
    assert!(request.await.unwrap().is_err());
    process.cancel().await.unwrap();
}

#[cfg(unix)]
#[tokio::test]
async fn cancellation_and_drop_stop_only_the_owned_process_group() {
    for cancel in [true, false] {
        let mut process = initialized("").await;
        let pids = process
            .control()
            .request("fixture/tree", &json!({}))
            .await
            .unwrap();
        let parent = pids["parent"].as_i64().unwrap() as libc::pid_t;
        let child = pids["child"].as_i64().unwrap() as libc::pid_t;
        assert_eq!(unsafe { libc::getpgid(parent) }, parent);
        assert_eq!(unsafe { libc::getpgid(child) }, parent);
        assert_ne!(unsafe { libc::getpgrp() }, parent);
        if cancel {
            process.cancel().await.unwrap();
        }
        drop(process);
        tokio::time::timeout(Duration::from_secs(5), async {
            while unsafe { libc::kill(parent, 0) } == 0 || unsafe { libc::kill(child, 0) } == 0 {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .expect("owned host and descendant must terminate");
    }
}
