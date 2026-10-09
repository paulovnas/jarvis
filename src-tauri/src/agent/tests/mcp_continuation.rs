use super::*;
use crate::agent::{context_manager::StepContext, provider, tool_contract::Orchestrator};
use crate::openai_codex::{custom, CodexCredential, ProviderModel};
use axum::{extract::State, http::header::CONTENT_TYPE, routing::post, Json};

#[derive(Clone)]
struct Script {
    requests: Arc<Mutex<Vec<Value>>>,
    server_name: String,
}

async fn completion(
    State(script): State<Script>,
    Json(request): Json<Value>,
) -> impl axum::response::IntoResponse {
    let step = {
        let mut requests = script.requests.lock().unwrap();
        let step = requests.len();
        requests.push(request.clone());
        step
    };
    let call = |index: usize, id: &str, name: &str, args: Value| json!({"index":index,"id":id,"type":"function","function":{"name":name,"arguments":args.to_string()}});
    let (delta, reason) = match step {
        0 => (
            json!({"tool_calls":[call(0, "activate", "mcp_activate", json!({"server":script.server_name}))]}),
            "tool_calls",
        ),
        1 => (
            json!({"tool_calls":[
                call(0, "original", "lookup", json!({"query":"creative board"})),
                call(1, "wrapper", "invoke", json!({"name":"lookup","arguments":{"query":"creative board"}})),
                call(2, "qualified-wrapper", "mcp_invoke", json!({"name":"lookup","arguments":{"query":"creative board"}}))
            ]}),
            "tool_calls",
        ),
        2 => (
            json!({"content":"Envie qualquer mensagem para eu continuar com o MCP."}),
            "stop",
        ),
        3 => {
            let canonical = request["tools"]
                .as_array()
                .unwrap()
                .iter()
                .find(|tool| {
                    tool["function"]["description"]
                        .as_str()
                        .is_some_and(|text| text.contains(" / lookup."))
                })
                .expect("the current Chat Completions catalog must expose the canonical MCP tool");
            (
                json!({"tool_calls":[call(0, "canonical", canonical["function"]["name"].as_str().unwrap(), json!({"query":"creative board"}))]}),
                "tool_calls",
            )
        }
        _ => (
            json!({"content":"Quadro consultado no mesmo turno."}),
            "stop",
        ),
    };
    let sse = format!(
        "data: {}\n\ndata: {}\n\ndata: [DONE]\n\n",
        json!({"choices":[{"index":0,"delta":delta,"finish_reason":null}]}),
        json!({"choices":[{"index":0,"delta":{},"finish_reason":reason}]}),
    );
    ([(CONTENT_TYPE, "text/event-stream")], sse)
}

#[tokio::test]
async fn custom_completions_mcp_recovery_finishes_without_another_user_message() {
    let fixture = Fixture::new();
    let session = session(&fixture);
    let state = AppState::default();
    let mcp = crate::mcp::McpState::default();
    let calls_file = fixture.root.join("mcp-calls");
    let server_name = "creative-mcp-fixture@local: creative";
    let plugin = crate::plugins::preview(&fixture.root, 0, crate::plugins::Operation::Create {
        draft: serde_json::from_value(json!({"name":"creative-mcp-fixture","description":"Offline recovery fixture",
            "mcpServers":{"creative":{"command":"node","args":[PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/mcp/fixtures/server.mjs")],"env":{"CALLS_FILE":calls_file}}}})).unwrap(),
    }).await.unwrap();
    crate::plugins::apply(&fixture.root, &plugin).unwrap();
    let mut options = options(ApprovalMode::Yolo);
    options.model = "mimo-fixture".into();
    let signal = session
        .reserve(
            "Consulte o quadro pelo MCP Creative Production.".into(),
            options.clone(),
        )
        .unwrap();
    session
        .update(true, |data| {
            data.turns.last_mut().unwrap().mcp_intent = Some(crate::mcp::McpIntent::default());
        })
        .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let requests = Arc::new(Mutex::new(Vec::new()));
    let app = axum::Router::new()
        .route("/chat/completions", post(completion))
        .with_state(Script {
            requests: requests.clone(),
            server_name: server_name.into(),
        });
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let mut credential = CodexCredential::new("fixture", "", 0, "fixture", None, None);
    credential.custom = Some(serde_json::from_value::<custom::Config>(json!({
        "baseUrl":endpoint,"protocol":"openai-completions","authMode":"bearer","tokenField":"max_tokens",
        "models":[{"id":options.model,"name":"MiMo fixture","contextWindow":128000,"maxOutputTokens":4000,
            "supportsImages":false,"supportsTools":true,"reasoning":"none","reasoningLevels":[],
            "defaultReasoningLevel":null,"thinkingBudget":null}]
    })).unwrap());
    let model: ProviderModel = serde_json::from_value(json!({
        "id":options.model,"name":"MiMo fixture","reasoningLevels":[],"defaultReasoningLevel":null,"contextWindow":128000
    })).unwrap();
    let provider = provider::TurnSession::new(
        credential,
        &model,
        session.id.clone(),
        telemetry::TraceContext::new(&session.id, "mcp-continuation"),
    )
    .unwrap();
    let mut clients = crate::mcp::runtime::TurnClients::discover_for_intent(
        &mcp,
        &state,
        &fixture.root,
        &fixture.root,
        &crate::mcp::McpIntent::default(),
        signal.clone(),
    )
    .await
    .unwrap();
    let mut pending = None;
    let mut rejected = None;
    let mut reminded = false;
    let mut continuations = 0;
    let run = async {
        for _ in 0..5 {
            let definitions = clients
                .definitions_with(&mcp, &state, &fixture.root, false, |_| true)
                .await;
            let step = StepContext::capture(
                &session,
                &options,
                &clients.instructions(),
                &definitions,
                provider.capabilities(),
            )
            .unwrap();
            let response = provider
                .stream(&step, signal.clone(), |_| Ok(()))
                .await
                .unwrap();
            let calls = response.tool_calls().to_vec();
            session
                .update(true, |data| {
                    data.turns.last_mut().unwrap().wire.extend(response.output)
                })
                .unwrap();
            if calls.is_empty() {
                if continue_mcp_tool_recovery(
                    &session,
                    &clients,
                    &definitions,
                    &mut pending,
                    &mut reminded,
                )
                .await
                .unwrap()
                {
                    continuations += 1;
                    assert_eq!(
                        continue_mcp_tool_recovery(
                            &session,
                            &clients,
                            &definitions,
                            &mut pending,
                            &mut reminded,
                        )
                        .await
                        .unwrap_err()
                        .code,
                        "mcp_tool_unavailable"
                    );
                    continue;
                }
                return response.text;
            }
            let mut contract = Orchestrator::new(&definitions);
            for tool in &definitions {
                let name = tool["name"].as_str().unwrap();
                contract.register_external(name, clients.requires_active_task(name));
            }
            for call in calls {
                let output = match contract.preflight(&call) {
                    Ok(_) => {
                        let output = clients
                            .execute(
                                &mcp,
                                &state,
                                &fixture.root,
                                &call.name,
                                &call.args,
                                false,
                                signal.clone(),
                            )
                            .await
                            .unwrap();
                        if pending
                            .as_ref()
                            .is_some_and(|(_, names)| names.contains(&call.name))
                        {
                            pending = None;
                        }
                        let current = clients
                            .definitions_with(&mcp, &state, &fixture.root, false, |_| true)
                            .await;
                        clients.discovery_output(&call.name, &call.args, &output, &current)
                    }
                    Err(error) => {
                        assert_eq!(error.code, "tool_unavailable");
                        assert!(
                            !calls_file.exists(),
                            "invalid names must not reach the MCP peer"
                        );
                        let feedback = clients
                            .unavailable_tool_feedback(&call.name, &call.args, &definitions)
                            .unwrap();
                        pending = Some((call.clone(), feedback.tool_names.clone()));
                        rejected = pending.clone();
                        feedback.output
                    }
                };
                session.update(true, |data| data.turns.last_mut().unwrap().wire.push(json!({"type":"function_call_output","call_id":call.id,"output":output}))).unwrap();
            }
        }
        panic!("the recovery must finish within five provider requests");
    };
    let result = tokio::time::timeout(Duration::from_secs(20), run).await;
    server.abort();
    assert_eq!(result.unwrap(), "Quadro consultado no mesmo turno.");
    assert_eq!(continuations, 1);
    assert_eq!(fs::read_to_string(calls_file).unwrap(), "lookup\n");
    let mut empty_snapshot = rejected.clone();
    assert!(
        mcp_tool_recovery_feedback(&clients, &[], &mut empty_snapshot, &mut reminded)
            .unwrap()
            .is_none()
    );
    assert!(empty_snapshot.is_none());
    let unrelated: Vec<_> = clients
        .definitions_with(&mcp, &state, &fixture.root, false, |_| true)
        .await
        .into_iter()
        .filter(|tool| {
            clients
                .tool_metadata(tool["name"].as_str().unwrap())
                .is_some_and(|(_, original, _)| original == "mutate")
        })
        .collect();
    assert!(
        !unrelated.is_empty(),
        "the unrelated MCP tool remains available"
    );
    assert!(
        mcp_tool_recovery_feedback(&clients, &unrelated, &mut rejected, &mut reminded)
            .unwrap()
            .is_none()
    );
    assert!(rejected.is_none(), "a revoked snapshot must clear recovery");
    let data = session.data.lock().unwrap();
    assert_eq!(data.turns.len(), 1);
    assert!(data.turns[0].turn.auxiliary_messages.is_empty());
    assert_eq!(
        data.turns[0]
            .wire
            .iter()
            .filter(|item| item["role"] == "user" && item["_jarvis_runtime"] == true)
            .count(),
        1
    );
    let requests = requests.lock().unwrap();
    assert_eq!(requests.len(), 5);
    let activation = requests[1]["messages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|message| message["role"] == "tool" && message["tool_call_id"] == "activate")
        .unwrap();
    let receipt: Value = serde_json::from_str(activation["content"].as_str().unwrap()).unwrap();
    assert!(receipt["schemas"]
        .as_array()
        .unwrap()
        .iter()
        .any(|schema| schema["inputSchema"]["properties"]["query"]["type"] == "string"));
    let continuation = requests[3]["messages"].as_array().unwrap().last().unwrap();
    assert_eq!(continuation["role"], "user");
    assert!(continuation["content"]
        .as_str()
        .unwrap()
        .contains("inputSchema"));
    assert!(requests[4]["messages"]
        .to_string()
        .contains("Documentation: creative board"));
}
