use super::*;
use crate::agent::{context_manager::StepContext, provider};
use crate::openai_codex::{custom, CodexCredential, ProviderModel};
use std::{fs, time::Duration};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[tokio::test]
async fn github_agents_receive_mcp_schemas_on_the_next_provider_request() {
    for flow in [Flow::Custom, Flow::Publication] {
        let (fixture, hub) = hub();
        let calls_file = fixture.root.join("mcp-calls");
        let server_name = "github-mcp-fixture@local: database";
        let prepared = crate::plugins::preview(
            &fixture.root,
            0,
            crate::plugins::Operation::Create {
                draft: serde_json::from_value(json!({
                    "name":"github-mcp-fixture", "description":"Offline MCP regression fixture",
                    "mcpServers":{"database":{
                        "command":"node",
                        "args":[PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/mcp/fixtures/server.mjs")],
                        "env":{"EXTRA_TOOLS":"48", "CALLS_FILE":calls_file}
                    }}
                }))
                .unwrap(),
            },
        )
        .await
        .unwrap();
        crate::plugins::apply(&fixture.root, &prepared).unwrap();
        let mut options = super::super::super::tests::options(ApprovalMode::Yolo);
        options.workflow = Some(flow);
        options.model = "fixture-model".into();
        if flow == Flow::Custom {
            options.custom_agent_id = Some("builtin:github".into());
        }
        {
            let mut manifest = hub.manifest.lock().unwrap();
            manifest.flow = flow;
            manifest.options = options.clone();
            manifest.custom_agent = (flow == Flow::Custom).then(|| {
                catalog::Catalog::default()
                    .resolve_agent("builtin:github")
                    .unwrap()
            });
        }
        let execution = Execution {
            hub: hub.clone(),
            id: "main".into(),
            role: Role::Github,
            flow,
            scope: vec![".".into()],
        };
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            let mut requests = Vec::new();
            for connection in 0..5 {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut bytes = Vec::new();
                let header_end = loop {
                    let mut chunk = [0; 4096];
                    let count = socket.read(&mut chunk).await.unwrap();
                    assert!(count > 0);
                    bytes.extend_from_slice(&chunk[..count]);
                    if let Some(end) = bytes.windows(4).position(|part| part == b"\r\n\r\n") {
                        break end + 4;
                    }
                };
                let headers = String::from_utf8(bytes[..header_end].to_vec()).unwrap();
                if connection == 0 {
                    // Match the existing Responses HTTP fixture: reject the optional
                    // WebSocket upgrade, then serve authoritative HTTP/SSE replay.
                    assert!(headers.starts_with("GET "));
                    socket.write_all(b"HTTP/1.1 426 Upgrade Required\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").await.unwrap();
                    continue;
                }
                assert!(headers.starts_with("POST "));
                let length: usize = headers
                    .lines()
                    .find_map(|line| {
                        line.to_lowercase()
                            .strip_prefix("content-length:")
                            .map(|value| value.trim().parse().unwrap())
                    })
                    .unwrap();
                while bytes.len() < header_end + length {
                    let mut chunk = [0; 4096];
                    let count = socket.read(&mut chunk).await.unwrap();
                    assert!(count > 0);
                    bytes.extend_from_slice(&chunk[..count]);
                }
                let request: Value =
                    serde_json::from_slice(&bytes[header_end..header_end + length]).unwrap();
                let step = connection - 1;
                let output = match step {
                    0 => {
                        json!({"type":"function_call","id":"activate","call_id":"activate","name":"mcp_activate","arguments":json!({"server":server_name}).to_string()})
                    }
                    1 => {
                        json!({"type":"function_call","id":"search","call_id":"search","name":"mcp_search_tools","arguments":json!({"query":"catalog tool 37","server":server_name,"limit":3}).to_string()})
                    }
                    2 => {
                        let tool = request["tools"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .find(|tool| {
                                tool["description"]
                                    .as_str()
                                    .is_some_and(|text| text.contains("catalog operation 37 "))
                            })
                            .expect(
                                "the next provider request must expose the selected MCP schema",
                            );
                        json!({"type":"function_call","id":"lookup","call_id":"lookup","name":tool["name"],"arguments":"{\"query\":\"archive evidence\"}"})
                    }
                    _ => {
                        json!({"type":"message","role":"assistant","content":[{"type":"output_text","text":"Integration evidence received."}]})
                    }
                };
                requests.push(request);
                let event = json!({"type":"response.completed","response":{
                    "id":format!("response-{step}"),"status":"completed","output":[output]
                }});
                let sse = format!("data: {event}\n\n");
                socket
                    .write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{sse}", sse.len()).as_bytes())
                    .await
                    .unwrap();
            }
            requests
        });
        let mut credential = CodexCredential::new("fixture", "", 0, "fixture", None, None);
        credential.custom = Some(custom::Config {
            base_url: endpoint,
            protocol: custom::Protocol::OpenaiResponses,
            auth_mode: custom::AuthMode::Bearer,
            token_field: custom::TokenField::MaxTokens,
            replay_unsigned_thinking: false,
            models: vec![custom::Model {
                id: options.model.clone(),
                name: "Fixture".into(),
                context_window: 128_000,
                max_output_tokens: 4_000,
                supports_images: false,
                supports_tools: true,
                reasoning: custom::Reasoning::None,
                reasoning_levels: vec![],
                default_reasoning_level: None,
                thinking_budget: None,
            }],
        });
        let model = ProviderModel {
            supports_fast: false,
            id: options.model.clone(),
            name: "Fixture".into(),
            reasoning_levels: vec![],
            default_reasoning_level: None,
            context_window: Some(128_000),
            multi_agent_reasoning_effort: None,
        };
        let provider = provider::TurnSession::new(
            credential,
            &model,
            hub.root.id.clone(),
            super::super::super::telemetry::TraceContext::new(&hub.root.id, "mcp-test"),
        )
        .unwrap();
        let mut clients = crate::mcp::runtime::TurnClients::discover_for_intent(
            &hub.env.mcp,
            &hub.env.state,
            &fixture.root,
            &fixture.root,
            &crate::mcp::McpIntent::default(),
            hub.root_signal.clone(),
        )
        .await
        .unwrap();
        let run = async {
            for step in 0..4 {
                let mut definitions = clients
                    .definitions_with(&hub.env.mcp, &hub.env.state, &fixture.root, false, |name| {
                        execution.allowed(name)
                    })
                    .await;
                execution.filter(&mut definitions);
                if step == 0 {
                    assert!(
                        definitions
                            .iter()
                            .any(|tool| tool["name"] == "mcp_activate"),
                        "{flow:?}: GitHub must receive MCP discovery"
                    );
                    assert!(!calls_file.exists());
                }
                let context = StepContext::capture(
                    &hub.root,
                    &options,
                    &clients.instructions(),
                    &definitions,
                    provider.capabilities(),
                )
                .unwrap();
                let response = provider
                    .stream(&context, hub.root_signal.clone(), |_| Ok(()))
                    .await
                    .unwrap();
                let calls = response.tool_calls().to_vec();
                hub.root
                    .update(true, |data| {
                        data.turns.last_mut().unwrap().wire.extend(response.output);
                    })
                    .unwrap();
                if step == 3 {
                    assert!(calls.is_empty());
                    assert_eq!(response.text, "Integration evidence received.");
                    break;
                }
                assert_eq!(calls.len(), 1);
                let call = &calls[0];
                super::super::super::tool_contract::Catalog::new(&definitions)
                    .validate(call)
                    .unwrap();
                let output = clients
                    .execute(
                        &hub.env.mcp,
                        &hub.env.state,
                        &fixture.root,
                        &call.name,
                        &call.args,
                        false,
                        hub.root_signal.clone(),
                    )
                    .await
                    .unwrap();
                if step == 2 {
                    assert!(output.contains("Documentation: archive evidence"));
                }
                hub.root.update(true, |data| {
                    data.turns.last_mut().unwrap().wire.push(json!({"type":"function_call_output","call_id":call.id,"output":output}));
                }).unwrap();
            }
        };
        tokio::time::timeout(Duration::from_secs(20), run)
            .await
            .unwrap();
        let requests = server.await.unwrap();
        assert_eq!(requests.len(), 4);
        let selector = requests[0]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .find(|tool| tool["name"] == "mcp_activate")
            .expect("the provider must receive the MCP activation schema");
        assert_eq!(
            selector["parameters"]["properties"]["server"]["enum"],
            json!(["github-mcp-fixture@local: database"])
        );
        let tools = requests[2]["tools"].as_array().unwrap();
        let selected = tools
            .iter()
            .find(|tool| {
                tool["description"]
                    .as_str()
                    .is_some_and(|text| text.contains("catalog operation 37 "))
            })
            .unwrap();
        assert_eq!(
            selected["parameters"],
            json!({
                "type":"object", "properties":{"query":{"type":"string","description":"Search query for catalog operation 37"}},
                "required":["query"], "additionalProperties":false
            })
        );
        assert_eq!(
            fs::read_to_string(&calls_file).unwrap(),
            "catalog_tool_37\n"
        );
        assert!(requests[3]["input"].as_array().unwrap().iter().any(|item| {
            item["type"] == "function_call_output"
                && item["output"]
                    .as_str()
                    .is_some_and(|output| output.contains("Documentation: archive evidence"))
        }));
    }
}
