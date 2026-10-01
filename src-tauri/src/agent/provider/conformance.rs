use super::*;
use crate::openai_codex::custom::{AuthMode, Config, Model, Protocol, Reasoning, TokenField};
use serde_json::json;

#[derive(Debug, Clone, Copy)]
enum FixtureProvider {
    Codex,
    Antigravity,
    CustomResponses,
    CustomCompletions,
    CustomMessages,
}

const PROVIDERS: [FixtureProvider; 5] = [
    FixtureProvider::Codex,
    FixtureProvider::Antigravity,
    FixtureProvider::CustomResponses,
    FixtureProvider::CustomCompletions,
    FixtureProvider::CustomMessages,
];

fn options(model: &str) -> TurnOptions {
    TurnOptions {
        executor: crate::claude::Executor::Jarvis,
        account: "fixture".into(),
        model: model.into(),
        reasoning: None,
        mode: super::super::Mode::Build,
        workflow: None,
        custom_workflow_id: None,
        custom_agent_id: None,
        approval_mode: super::super::ApprovalMode::Manual,
        manual_validation: false,
        automatic_publication: None,
    }
}

#[tokio::test]
#[ignore = "Requires a selected connected Codex account; sends one text-only schema diagnostic, never executes tools"]
async fn live_codex_accepts_native_graft_tools() {
    assert_eq!(
        crate::data_dir::profile(),
        crate::data_dir::Profile::Production,
        "Run this explicit diagnostic with --release to use the production account"
    );
    let mut options =
        options(&std::env::var("JARVIS_LIVE_MODEL").expect("Set JARVIS_LIVE_MODEL explicitly"));
    options.account =
        std::env::var("JARVIS_LIVE_ACCOUNT").expect("Set JARVIS_LIVE_ACCOUNT to a connected alias");
    options.reasoning = Some("max".into());
    let home = std::path::PathBuf::from(std::env::var_os("HOME").unwrap());
    let auth_options = options.clone();
    let credential = tokio::task::spawn_blocking(move || {
        crate::openai_codex::OpenAiCodexState::default().inference_credential(
            &crate::persistence::AppState::default(),
            &home,
            &auth_options.account,
            &auth_options.model,
            auth_options.reasoning.as_deref(),
        )
    })
    .await
    .unwrap()
    .unwrap();
    let session = crate::library::new_id().unwrap();
    let body = request_body(
        &options,
        &ModelCapabilities::resolve_for_options(&credential, &options),
        "Respond only OK. Do not call any tools.",
        vec![json!({"role":"user","content":"Responda somente OK, sem usar ferramentas."})],
        crate::core::graft::definitions(),
        &session,
    );
    let mut response = authenticated_request(&credential, &session, &body, Duration::from_secs(90))
        .unwrap()
        .send()
        .await
        .unwrap();
    if !response.status().is_success() {
        let status = response.status();
        // This probe contains only the public tool schema and a fixed message;
        // never send project history or attachments through this diagnostic.
        let detail = response.text().await.unwrap();
        panic!(
            "Native Graft schema rejected ({status}): {}",
            detail.chars().take(2000).collect::<String>()
        );
    }
    let mut parser = Sse::default();
    let mut completed = false;
    while let Some(chunk) = response.chunk().await.unwrap() {
        for event in parser.push(&chunk).unwrap() {
            match event["type"].as_str() {
                Some("error" | "response.failed") => {
                    panic!("Native Graft schema diagnostic failed: {event}");
                }
                Some("response.completed") => completed = true,
                _ => {}
            }
        }
    }
    assert!(
        completed,
        "The text-only schema diagnostic did not complete"
    );
}

fn custom_config(protocol: Protocol) -> Config {
    Config {
        base_url: "https://example.com/v1".into(),
        protocol,
        auth_mode: if protocol == Protocol::AnthropicMessages {
            AuthMode::XApiKey
        } else {
            AuthMode::Bearer
        },
        token_field: TokenField::MaxTokens,
        replay_unsigned_thinking: false,
        models: vec![Model {
            id: "fixture-model".into(),
            name: "Fixture".into(),
            context_window: 128_000,
            max_output_tokens: 4_000,
            supports_images: true,
            supports_tools: true,
            reasoning: Reasoning::None,
            reasoning_levels: vec![],
            default_reasoning_level: None,
            thinking_budget: None,
        }],
    }
}

#[tokio::test]
async fn interrupted_turn_compacts_reloads_and_continues_through_the_transport() {
    use crate::agent::{
        compaction, context_manager::StepContext, history, session_writer, tests, Session,
        TurnStatus,
    };
    use futures_util::{SinkExt, StreamExt};
    use std::sync::{Arc, Mutex};
    use tokio_tungstenite::{accept_async, tungstenite::Message};

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = tokio::spawn(async move {
        let mut requests = Vec::new();
        for index in 0..2 {
            let (socket, _) = listener.accept().await.unwrap();
            let mut socket = accept_async(socket).await.unwrap();
            let message = socket.next().await.unwrap().unwrap().into_text().unwrap();
            requests.push(serde_json::from_str::<Value>(&message).unwrap());
            socket.send(Message::Text(json!({"type":"response.completed","response":{
                "id":format!("response-{index}"),"status":"completed",
                "output":[{"type":"message","role":"assistant","content":[{"type":"output_text","text":"Continued from saved progress"}]}]
            }}).to_string().into())).await.unwrap();
        }
        requests
    });
    let mut config = custom_config(Protocol::OpenaiResponses);
    config.base_url = format!("http://127.0.0.1:{port}");
    let mut credential = CodexCredential::new("fixture", "", 0, "fixture", None, None);
    credential.custom = Some(config);
    let model = ProviderModel {
        id: "fixture-model".into(),
        name: "Fixture".into(),
        reasoning_levels: vec![],
        default_reasoning_level: None,
        context_window: Some(128_000),
        multi_agent_reasoning_effort: None,
    };
    let provider = TurnSession::new(
        credential,
        &model,
        "fixture".into(),
        super::super::telemetry::TraceContext::new("fixture", "turn"),
    )
    .unwrap();
    let fixture = tests::Fixture::new();
    let session = tests::session(&fixture);
    let options = options(&model.id);
    session
        .reserve(
            "Finish the requested backend change.".into(),
            options.clone(),
        )
        .unwrap();
    session.update(true, |data| {
        let turn = data.turns.last_mut().unwrap();
        turn.turn.context_window = model.context_window;
        turn.wire.extend([
            json!({"type":"function_call","call_id":"uncertain","name":"bash","arguments":"{\"command\":\"apply-migration\"}"}),
            json!({"role":"user","content":"Preserve the frontend; verify the migration before repeating it."}),
            json!({"type":"function_call","call_id":"confirmed","name":"write","arguments":"{\"path\":\"backend.rs\"}"}),
            json!({"type":"function_call_output","call_id":"confirmed","output":"Backend change saved"}),
            json!({"type":"function_call","call_id":"inspection","name":"read","arguments":"{}"}),
            json!({"type":"function_call_output","call_id":"inspection","output":"Verified state. ".repeat(20_000)}),
        ]);
    }).unwrap();
    super::super::finish(
        &session,
        Err(AgentError::new(
            "provider_transport_interrupted",
            "Interrupted",
        )),
    );
    let (_cancel, signal) = watch::channel(false);
    assert!(
        compaction::ensure_with(&session, 0, true, signal, |_| async {
            Ok("The backend change is saved; verify uncertain effects and finish.".into())
        })
        .await
        .unwrap()
    );
    // Manual compaction of a failed turn must retain its recovery state in memory.
    let before_reload =
        StepContext::capture(&session, &options, "Continue", &[], provider.capabilities()).unwrap();
    let path = session.journal.clone();
    session.flush_async().await.unwrap();
    drop(session);

    let replay = history::HistoryState::default()
        .load_replay(&path, &fixture.root)
        .unwrap();
    let latest = replay
        .turns
        .last()
        .expect("Compaction must not discard a resumable turn");
    assert_eq!(latest.turn.status, TurnStatus::Error);
    let turn_id = latest.turn.id.clone();
    let writer =
        session_writer::SessionWriter::start(path.clone(), "fixture".into(), Some(latest.clone()))
            .unwrap();
    let session = Arc::new(Session {
        id: "fixture".into(),
        journal: path,
        root: fixture.root.clone(),
        journal_maintenance: Default::default(),
        writer,
        emit: Arc::new(|_| {}),
        data: Mutex::new(super::super::SessionData {
            turns: replay.turns,
            turn_base: replay.turn_base,
            wire_base: replay.wire_base,
            inherited_mcp_intent: replay.inherited_mcp_intent,
            extras: replay.extras,
            active: None,
            recovery: None,
            revision: 1,
            storage_failed: false,
            last_emit: std::time::Instant::now(),
            compacting: false,
            manual_compaction: false,
        }),
    });
    let after_reload =
        StepContext::capture(&session, &options, "Continue", &[], provider.capabilities()).unwrap();
    assert_eq!(after_reload.input(), before_reload.input());
    let (signal, _) = session.retry_failed_turn(&turn_id).unwrap();
    for index in 0..2 {
        let step =
            StepContext::capture(&session, &options, "Continue", &[], provider.capabilities())
                .unwrap();
        let response = tokio::time::timeout(
            Duration::from_secs(10),
            provider.stream(&step, signal.clone(), |_| Ok(())),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(response.text, "Continued from saved progress");
        session
            .update(true, |data| {
                data.turns.last_mut().unwrap().wire.extend(response.output)
            })
            .unwrap();
        if index == 0 {
            assert!(
                compaction::ensure_with(&session, 0, true, signal.clone(), |_| async {
                    Ok("Backend saved. Verify migration, then finish.".into())
                })
                .await
                .unwrap()
            );
        }
    }
    let requests = server.await.unwrap();
    for request in &requests {
        assert!(
            request.get("previous_response_id").is_none(),
            "Compaction invalidates the prior transport prefix"
        );
        let input = request["input"].to_string();
        assert!(input.contains("Finish the requested backend change."));
        assert!(input.contains("Preserve the frontend"));
        assert!(input.contains("Backend change saved"));
        assert!(input.contains(super::super::journal::UNKNOWN_TOOL_OUTPUT));
    }
    let (turns, extras) = super::super::journal::read_only(&session.journal).unwrap();
    assert_eq!(turns.len(), 1);
    assert_eq!(turns[0].turn.id, turn_id);
    extras.context.unwrap().validate(&turns).unwrap();
}

fn response(provider: FixtureProvider) -> Result<Response, AgentError> {
    match provider {
        FixtureProvider::Codex | FixtureProvider::CustomResponses => {
            let fixture = json!({
                "status":"completed",
                "output":[
                    {"type":"message","role":"assistant","content":[{"type":"output_text","text":"Done"}]},
                    {"type":"function_call","call_id":"call-1","name":"read","arguments":"{\"path\":\"a\"}"},
                    {"type":"function_call","call_id":"call-2","name":"search","arguments":"{\"query\":\"b\"}"}
                ],
                "usage":{"input_tokens":20,"output_tokens":5}
            });
            if matches!(provider, FixtureProvider::Codex) {
                completed(&fixture)
            } else {
                custom::fixture_response(Protocol::OpenaiResponses, &[fixture])
            }
        }
        FixtureProvider::Antigravity => antigravity::fixture_response(
            &[json!({"response":{
                "candidates":[{"content":{"parts":[
                    {"text":"Done"},
                    {"functionCall":{"id":"call-1","name":"read","args":{"path":"a"}}},
                    {"functionCall":{"id":"call-2","name":"search","args":{"query":"b"}}}
                ]},"finishReason":"STOP"}],
                "usageMetadata":{"promptTokenCount":20,"candidatesTokenCount":5}
            }})],
            "gemini-fixture",
        ),
        FixtureProvider::CustomCompletions => custom::fixture_response(
            Protocol::OpenaiCompletions,
            &[
                json!({"choices":[{"index":0,"delta":{"content":"Done","tool_calls":[
                    {"index":0,"id":"call-1","function":{"name":"read","arguments":"{\"path\":\"a\"}"}},
                    {"index":1,"id":"call-2","function":{"name":"search","arguments":"{\"query\":\"b\"}"}}
                ]}}]}),
                json!({"choices":[{"index":0,"delta":{},"finish_reason":"tool_calls"}],"usage":{"prompt_tokens":20,"completion_tokens":5}}),
            ],
        ),
        FixtureProvider::CustomMessages => custom::fixture_response(
            Protocol::AnthropicMessages,
            &[
                json!({"type":"message_start","message":{"usage":{"input_tokens":20}}}),
                json!({"type":"content_block_start","index":0,"content_block":{"type":"text","text":"Done"}}),
                json!({"type":"content_block_stop","index":0}),
                json!({"type":"content_block_start","index":1,"content_block":{"type":"tool_use","id":"call-1","name":"read","input":{"path":"a"}}}),
                json!({"type":"content_block_stop","index":1}),
                json!({"type":"content_block_start","index":2,"content_block":{"type":"tool_use","id":"call-2","name":"search","input":{"query":"b"}}}),
                json!({"type":"content_block_stop","index":2}),
                json!({"type":"message_delta","delta":{"stop_reason":"tool_use"},"usage":{"output_tokens":5}}),
                json!({"type":"message_stop"}),
            ],
        ),
    }
}

#[test]
fn equivalent_text_parallel_tools_and_usage_share_one_internal_semantics() {
    for provider in PROVIDERS {
        let response = response(provider).unwrap_or_else(|error| panic!("{provider:?}: {error:?}"));
        assert_eq!(response.text, "Done", "{provider:?}");
        assert_eq!(
            response
                .tool_calls()
                .iter()
                .map(|call| (call.id.as_str(), call.name.as_str()))
                .collect::<Vec<_>>(),
            [("call-1", "read"), ("call-2", "search")],
            "{provider:?}"
        );
        let usage = response.usage.unwrap();
        assert_eq!((usage.input_tokens, usage.output_tokens), (20, 5));
    }
}

#[test]
fn legacy_cli_orphan_results_cannot_break_primary_or_secondary_provider_requests() {
    let input = provider_input(vec![
        json!({"role":"user","content":"Improve the button color."}),
        json!({"type":"function_call","call_id":"read-a","name":"read","arguments":"{}"}),
        json!({"type":"function_call","call_id":"read-b","name":"read","arguments":"{}"}),
        json!({"type":"function_call_output","call_id":"read-a","output":"Confirmed A"}),
        json!({"type":"function_call_output","call_id":"toolu_legacy","output":"Confirmed legacy receipt"}),
        json!({"type":"function_call_output","call_id":"read-b","output":"Confirmed B"}),
        json!({"role":"user","content":"Preserve the implementation."}),
    ]);
    assert_eq!(input[4]["call_id"], "read-b");
    assert_eq!(input[5]["role"], "assistant");
    assert_eq!(provider_input(input.clone()), input);
    let credential = CodexCredential::new("fixture", "", 0, "fixture", None, None);
    for provider in PROVIDERS {
        let body = match provider {
            FixtureProvider::Antigravity => {
                antigravity::fixture_request(&credential, &options("gemini-fixture"), &input, &[])
                    .unwrap()
            }
            FixtureProvider::Codex => {
                let options = options("gpt-fixture");
                request_body(
                    &options,
                    &ModelCapabilities::resolve_for_options(&credential, &options),
                    "Fixture instructions",
                    input.clone(),
                    vec![],
                    "fixture",
                )
            }
            _ => {
                let protocol = match provider {
                    FixtureProvider::CustomResponses => Protocol::OpenaiResponses,
                    FixtureProvider::CustomCompletions => Protocol::OpenaiCompletions,
                    FixtureProvider::CustomMessages => Protocol::AnthropicMessages,
                    _ => unreachable!(),
                };
                custom::fixture_request(
                    &custom_config(protocol),
                    &options("fixture-model"),
                    input.clone(),
                    vec![],
                )
                .unwrap()
            }
        };
        let serialized = body.to_string();
        for receipt in [
            "Confirmed A",
            "Confirmed B",
            "Confirmed legacy receipt",
            "Preserve the implementation.",
        ] {
            assert!(
                serialized.contains(receipt),
                "{provider:?}: missing {receipt}"
            );
        }
        if matches!(provider, FixtureProvider::CustomMessages) {
            let messages = body["messages"].as_array().unwrap();
            let results = messages
                .iter()
                .find(|message| message["content"][0]["type"] == "tool_result")
                .unwrap();
            assert_eq!(results["content"].as_array().unwrap().len(), 2);
        }
    }
}

#[test]
fn malformed_or_incomplete_streams_converge_to_structured_protocol_errors() {
    let failures = [
        completed(&json!({"status":"incomplete","output":[]})),
        antigravity::fixture_response(
            &[json!({"response":{"candidates":[{"content":{"parts":[{"text":"partial"}]}}]}})],
            "gemini-fixture",
        ),
        custom::fixture_response(
            Protocol::OpenaiResponses,
            &[json!({"status":"completed","output":[{"type":"unknown"}]})],
        ),
        custom::fixture_response(
            Protocol::OpenaiCompletions,
            &[json!({"choices":[{"index":1,"delta":{"content":"wrong choice"}}]})],
        ),
        custom::fixture_response(
            Protocol::AnthropicMessages,
            &[
                json!({"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"orphan"}}),
            ],
        ),
    ];
    for failure in failures {
        let error = failure.unwrap_err();
        assert_eq!(error.code, "provider_protocol");
        assert!(retry::retryable(&error));
        assert!(error.provider_metadata.is_none());
    }
}

#[test]
fn overflow_is_classified_without_retrying_or_exposing_provider_payloads() {
    for value in [
        json!({"error":{"code":"context_length_exceeded","message":"private-a"}}),
        json!({"error":{"type":"invalid_request_error","message":"maximum context length private-b"}}),
        json!({"response":{"error":{"code":"context_window_exceeded","message":"private-c"}}}),
    ] {
        assert!(context_overflow(&value));
    }
    let error = overflow_error();
    assert_eq!(error.code, "context_overflow");
    assert!(!retry::retryable(&error));
    assert!(!error.message.contains("private"));
}

#[test]
fn compacted_and_interrupted_context_reaches_every_protocol_without_private_markers() {
    let input = provider_input(vec![
        json!({"role":"user","content":"Original request"}),
        json!({"role":"user","_jarvis_runtime":true,"content":"Continuation summary: edit src/app.ts"}),
        json!({"type":"function_call","call_id":"missing","name":"read","arguments":"{}"}),
        json!({"type":"function_call","call_id":"confirmed","name":"read","arguments":"{}"}),
        json!({"type":"function_call_output","call_id":"confirmed","output":"Confirmed file contents"}),
        json!({"type":"function_call","call_id":"next","name":"read","arguments":"{}"}),
        json!({"type":"function_call_output","call_id":"next","output":"Next confirmed result"}),
        json!({"role":"user","content":"Continue from the saved progress."}),
    ]);
    let tool = json!({"type":"function","name":"read","description":"Read","parameters":{"type":"object"}});

    let codex_options = options("gpt-fixture");
    let codex_credential = CodexCredential::new("", "", 0, "", None, None);
    let codex_capabilities =
        ModelCapabilities::resolve_for_options(&codex_credential, &codex_options);
    let mut bodies = vec![request_body(
        &codex_options,
        &codex_capabilities,
        "Fixture instructions",
        input.clone(),
        vec![tool.clone()],
        "fixture-session",
    )];

    let mut antigravity_credential = CodexCredential::new("", "", 0, "", None, None);
    antigravity_credential.project_id = Some("project".into());
    let antigravity_options = options("gemini-fixture");
    bodies.push(
        antigravity::fixture_request(
            &antigravity_credential,
            &antigravity_options,
            &input,
            std::slice::from_ref(&tool),
        )
        .unwrap(),
    );

    for protocol in [
        Protocol::OpenaiResponses,
        Protocol::OpenaiCompletions,
        Protocol::AnthropicMessages,
    ] {
        bodies.push(
            custom::fixture_request(
                &custom_config(protocol),
                &options("fixture-model"),
                input.clone(),
                vec![tool.clone()],
            )
            .unwrap(),
        );
    }

    for body in bodies {
        let serialized = body.to_string();
        assert!(serialized.contains("Continuation summary: edit src/app.ts"));
        assert!(serialized.contains("Confirmed file contents"));
        assert!(serialized.contains(super::super::journal::UNKNOWN_TOOL_OUTPUT));
        assert!(serialized.contains("Continue from the saved progress."));
        assert!(!serialized.contains("_jarvis_runtime"));
        if let Some(messages) = body["messages"].as_array() {
            let assistant = messages
                .iter()
                .position(|message| message["role"] == "assistant")
                .unwrap();
            if let Some(calls) = messages[assistant]["tool_calls"].as_array() {
                assert_eq!(calls.len(), 2);
                assert_eq!(messages[assistant + 1]["tool_call_id"], "confirmed");
                assert_eq!(messages[assistant + 2]["tool_call_id"], "missing");
            } else {
                assert_eq!(messages[assistant]["content"].as_array().unwrap().len(), 2);
                assert_eq!(
                    messages[assistant + 1]["content"][0]["tool_use_id"],
                    "confirmed"
                );
                assert_eq!(
                    messages[assistant + 1]["content"][1]["tool_use_id"],
                    "missing"
                );
            }
        }
    }
}
