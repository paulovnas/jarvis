use super::*;

#[tokio::test]
async fn native_mcp_actions_use_jarvis_receipts_and_disconnect_never_replays_them() {
    for mode in [
        "complete",
        "disconnect",
        "sequential",
        "changedModel",
        "writeAndBash",
    ] {
        let fixture = crate::agent::tests::Fixture::new();
        let session = crate::agent::tests::session(&fixture);
        std::fs::write(session.root.join("proof.txt"), "fixture-proof").unwrap();
        let state = AppState::default();
        let oauth = OpenAiCodexState::default();
        let mcp = crate::mcp::McpState::default();
        let grants = execution_grants::GrantStore::default();
        let mut options = crate::agent::tests::options(ApprovalMode::Yolo);
        options.executor = crate::claude::Executor::Muse;
        options.account.clear();
        options.model = "muse-fixture".into();
        let signal = session
            .reserve("Ler proof.txt".into(), options.clone())
            .unwrap();
        let mut bridge = claude_executor::bridge::tests::fixture_bridge(
            &session,
            TurnRuntime {
                grants: &grants,
                state: &state,
                oauth: &oauth,
                mcp: &mcp,
                home: &fixture.root,
            },
            options,
            signal,
        );
        let tools = claude_executor::mcp_tools(&mut bridge).await.unwrap();
        let mut server = server::Server::start(tools).await.unwrap();
        let mut config = MuseSpawnConfig::new("node".into(), session.root.clone());
        config.args = vec![
            "-e".into(),
            include_str!("fixtures/host.cjs").into(),
            mode.into(),
        ];
        config.request_timeout = Duration::from_secs(5);
        let mut process = MuseProcess::spawn(config).unwrap();
        native::initialize_tools(&mut process).await.unwrap();
        let control = process.control();
        let started = control
            .request(
                "session/start",
                &json!({"workspaceRoot":session.root,
            "config":{"mcpServers":{"jarvis":server.config()}}}),
            )
            .await
            .unwrap();
        let id = started["session"]["sessionId"].as_str().unwrap();
        let account = json!({"state":"accountLogin","label":"executor-fixture"});
        let _ = crate::muse::usage::observed(&fixture.root, &account);
        let mut active = None;
        let result = tokio::time::timeout(
            Duration::from_secs(15),
            drive(
                &mut bridge,
                &mut process,
                &control,
                &account,
                id,
                &mut json!([{"type":"text","text":"Ler arquivo"}]),
                &mut server,
                &mut active,
            ),
        )
        .await
        .unwrap();
        if matches!(mode, "complete" | "sequential" | "writeAndBash") {
            result.unwrap();
        } else {
            assert!(result.is_err());
        }
        process.cancel().await.unwrap();
        let data = session.data.lock().unwrap();
        let turn = data.turns.last().unwrap();
        let calls: Vec<_> = turn
            .turn
            .steps
            .iter()
            .flat_map(|step| &step.tools)
            .collect();
        let count = match mode {
            "sequential" => 2,
            "writeAndBash" => 5,
            "changedModel" => 0,
            _ => 1,
        };
        assert_eq!(calls.len(), count);
        for (index, call) in calls.iter().enumerate() {
            let name = if mode == "writeAndBash" {
                ["update_tasks", "read", "write", "bash", "update_tasks"][index]
            } else {
                "read"
            };
            assert_eq!(call.name, name);
            assert_eq!(call.status, "completed");
            if name == "read" {
                assert!(call.output.contains("fixture-proof"));
            }
            if name == "bash" {
                assert!(call.output.contains("updated-proof"));
            }
        }
        if mode == "writeAndBash" {
            assert_eq!(
                std::fs::read_to_string(session.root.join("proof.txt")).unwrap(),
                "updated-proof"
            );
        }
        assert_eq!(
            calls
                .iter()
                .map(|call| &call.id)
                .collect::<HashSet<_>>()
                .len(),
            count
        );
        assert_eq!(
            turn.wire
                .iter()
                .filter(|item| item["type"] == "function_call_output")
                .count(),
            count
        );
        assert!(crate::muse::usage::observed(&fixture.root, &account).is_some());
        if matches!(mode, "complete" | "sequential" | "writeAndBash") {
            assert_eq!(turn.turn.steps.last().unwrap().text, "Arquivo lido.");
            assert_eq!(
                turn.turn.steps.last().unwrap().generation,
                Some(crate::agent::generation::Metrics {
                    output_tokens: 7,
                    duration_ms: 875,
                    estimated: false,
                })
            );
            assert_eq!(
                turn.turn
                    .steps
                    .last()
                    .unwrap()
                    .usage
                    .as_ref()
                    .unwrap()
                    .output_tokens,
                7
            );
        }
    }
}

#[test]
fn muse_history_is_scoped_and_preserves_current_guidance() {
    let fixture = crate::agent::tests::Fixture::new();
    let session = crate::agent::tests::session(&fixture);
    let mut options = crate::agent::tests::options(ApprovalMode::Yolo);
    options.executor = crate::claude::Executor::Muse;
    session.reserve("Objetivo atual".into(), options).unwrap();
    session
        .update(false, |data| {
            data.turns
                .last_mut()
                .unwrap()
                .wire
                .push(json!({"_jarvis_muse_session":"native-id","_jarvis_runtime":true}))
        })
        .unwrap();
    session
        .update(false, |data| {
            data.turns.last_mut().unwrap().wire.push(
                json!({"role":"user","_jarvis_runtime":true,"content":"Selected skill reference"}),
            )
        })
        .unwrap();
    let data = session.data.lock().unwrap();
    assert_eq!(reference(&data).as_deref(), Some("native-id"));
    assert!(initial_input(&data, true)
        .unwrap()
        .contains("Objetivo atual"));
    assert!(initial_input(&data, true)
        .unwrap()
        .contains("Selected skill reference"));
    drop(data);
    session
        .update(false, |data| {
            data.turns.last_mut().unwrap().turn.options.executor = crate::claude::Executor::Jarvis
        })
        .unwrap();
    assert!(reference(&session.data.lock().unwrap()).is_none());
}
