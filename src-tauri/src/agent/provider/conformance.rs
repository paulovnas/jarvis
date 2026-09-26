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
    }
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
