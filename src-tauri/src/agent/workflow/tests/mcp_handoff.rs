use super::*;
use crate::agent::claude_executor::bridge::tests::fixture_bridge;

#[tokio::test]
async fn claude_handoff_waits_for_canonical_mcp_recovery_and_bounds_incomplete_completion() {
    for resolve in [false, true] {
        let (fixture, hub) = hub();
        let calls = fixture.root.join("mcp-calls");
        let prepared = crate::plugins::preview(&fixture.root, 0, crate::plugins::Operation::Create {
            draft: serde_json::from_value(json!({"name":"handoff-mcp","description":"Offline handoff fixture",
                "mcpServers":{"documents":{"command":"node","args":[PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/mcp/fixtures/server.mjs")],"env":{"CALLS_FILE":calls}}}})).unwrap(),
        }).await.unwrap();
        crate::plugins::apply(&fixture.root, &prepared).unwrap();
        let mut worker = job(&hub, Role::Builder, ".");
        worker.status = Status::Running;
        worker.options.approval_mode = ApprovalMode::Yolo;
        hub.manifest
            .lock()
            .unwrap()
            .jobs
            .insert(worker.id.clone(), worker.clone());
        let execution = Execution {
            hub: hub.clone(),
            id: worker.id.clone(),
            role: Role::Builder,
            flow: Flow::Complete,
            scope: vec![".".into()],
        };
        let mut bridge = fixture_bridge(
            &hub.root,
            TurnRuntime {
                grants: &hub.env.grants,
                state: &hub.env.state,
                oauth: &hub.env.oauth,
                mcp: &hub.env.mcp,
                home: &fixture.root,
            },
            worker.options.clone(),
            hub.root_signal.clone(),
        );
        bridge.execution = Some(execution.clone());
        bridge.clients = crate::mcp::runtime::TurnClients::discover_for_intent(
            &hub.env.mcp,
            &hub.env.state,
            &fixture.root,
            &fixture.root,
            &crate::mcp::McpIntent::default(),
            hub.root_signal.clone(),
        )
        .await
        .unwrap();
        let tool = |id: &str, name: &str, args: Value| ToolCall {
            id: id.into(),
            name: name.into(),
            args,
            status: "pending".into(),
            output: String::new(),
            duration_ms: 0,
        };
        bridge
            .call(&tool(
                "activate",
                "mcp_activate",
                json!({"server":"handoff-mcp@local: documents"}),
            ))
            .await
            .unwrap();
        let definitions = bridge.definitions().await.unwrap();
        let canonical = definitions
            .iter()
            .find(|definition| {
                definition["name"].as_str().is_some_and(|name| {
                    bridge
                        .clients
                        .tool_metadata(name)
                        .is_some_and(|(_, original, _)| original == "lookup")
                })
            })
            .unwrap()["name"]
            .as_str()
            .unwrap()
            .to_owned();
        assert_eq!(
            bridge
                .call(&tool("unknown", "lookup", json!({"query":"board"})))
                .await
                .unwrap_err()
                .code,
            "tool_unavailable"
        );
        let handoff = tool(
            "handoff",
            "hub_complete",
            json!({
                "verdict":"completed", "summary":"Documentation inspected", "outcomes":["Requested evidence checked"],
                "evidence":["One canonical MCP lookup"], "validation":[], "limitations":[], "taskIds":[],
            }),
        );
        let blocked = bridge.call(&handoff).await.unwrap_err();
        assert_eq!(blocked.code, "mcp_tool_name_recovery");
        let feedback: Value =
            serde_json::from_str(blocked.tool_result.as_deref().unwrap()).unwrap();
        assert_eq!(feedback["error"]["code"], "mcp_tool_name_recovery");
        assert_eq!(feedback["error"]["tool"], "hub_complete");
        assert_eq!(feedback["executed"], false);
        assert!(!execution.has_handoff().unwrap());
        assert!(!calls.exists());
        if !resolve {
            let second = ToolCall {
                id: "second-handoff".into(),
                ..handoff
            };
            assert_eq!(
                bridge.call(&second).await.unwrap_err().code,
                "mcp_tool_unavailable"
            );
            assert!(!execution.has_handoff().unwrap());
            assert!(!calls.exists());
            continue;
        }
        let output = bridge
            .call(&tool("canonical", &canonical, json!({"query":"board"})))
            .await
            .unwrap();
        assert!(output.contains("Documentation: board"));
        bridge
            .call(&ToolCall {
                id: "completed-handoff".into(),
                ..handoff
            })
            .await
            .unwrap();
        assert!(execution.has_handoff().unwrap());
        assert_eq!(std::fs::read_to_string(&calls).unwrap(), "lookup\n");
    }
}
