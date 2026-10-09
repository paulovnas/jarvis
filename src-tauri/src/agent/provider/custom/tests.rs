use super::*;

#[test]
fn messages_and_completions_keep_terminal_response_validation() {
    let mut messages = messages::Stream::default();
    let mut ready = vec![];
    for event in [
        json!({"type":"message_start", "message":{"usage":{}}}),
        json!({"type":"content_block_start", "index":0, "content_block":{"type":"tool_use", "id":"read1", "name":"read", "input":{}}}),
        json!({"type":"content_block_delta", "index":0, "delta":{"type":"input_json_delta", "partial_json":"{\"path\":\"README.md\"}"}}),
    ] {
        messages
            .event(&event, &mut |delta| {
                if let Delta::ToolReady(call) = delta {
                    ready.push(call);
                }
                Ok(())
            })
            .unwrap();
    }
    assert!(ready.is_empty());
    messages
        .event(
            &json!({"type":"content_block_stop", "index":0}),
            &mut |delta| {
                if let Delta::ToolReady(call) = delta {
                    ready.push(call);
                }
                Ok(())
            },
        )
        .unwrap();
    assert!(ready.is_empty());
    assert!(messages.finish(&json!({})).is_err());
    let mut completions = completions::Stream::default();
    completions.event(&json!({"choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"id":"c1","type":"function","function":{"name":"read","arguments":"{\"path\":\"README.md\"}"}}]}, "finish_reason":null}]}), &mut |delta| { assert!(!matches!(delta, Delta::ToolReady(_))); Ok(()) }).unwrap();
    assert!(completions.finish(&json!({})).is_err());
}

#[tokio::test]
#[ignore = "Uses an explicitly authorized Custom account for a tiny synthetic request; never reads project content"]
async fn live_custom_completion_smoke() {
    let alias = std::env::var("JARVIS_CUSTOM_ACCOUNT").expect("Select a configured account");
    let home = std::path::PathBuf::from(std::env::var_os("HOME").unwrap());
    let (credential, config) = tokio::task::spawn_blocking(move || {
        let state = crate::persistence::AppState::default();
        let config = crate::openai_codex::custom::load(&state, &home, &alias).unwrap();
        let auth = crate::openai_codex::OpenAiCodexState::default()
            .inference_credential(&state, &home, &alias, &config.models[0].id, None)
            .unwrap();
        (auth, config)
    })
    .await
    .unwrap();
    assert_eq!(config.protocol, Protocol::OpenaiCompletions);
    let mut options = options();
    options.account = std::env::var("JARVIS_CUSTOM_ACCOUNT").unwrap();
    options.model = config.models[0].id.clone();
    options.reasoning = config.models[0].default_reasoning_level.clone();
    let (_cancel, signal) = watch::channel(false);
    let result = stream(
        &credential,
        &config,
        "synthetic-session",
        &options,
        "Responda apenas PONG.",
        vec![json!({"role":"user","content":"Ping"})],
        vec![],
        signal.clone(),
        |_| Ok(()),
    )
    .await
    .unwrap();
    assert!(result.text.to_uppercase().contains("PONG"));
    assert!(result
        .usage
        .is_some_and(|usage| usage.input_tokens > 0 && usage.output_tokens > 0));
    let instructions = "Use get_marker exatamente uma vez para obter o marcador. Depois de receber o resultado, responda apenas com ele. Não invente o marcador.";
    let tools = vec![
        json!({"type":"function","name":"get_marker","description":"Obtain the synthetic validation marker. No filesystem or external effects.","parameters":{"type":"object","properties":{},"additionalProperties":false}}),
    ];
    let mut input =
        vec![json!({"role":"user","content":"Obtenha o marcador usando a ferramenta."})];
    let result = stream(
        &credential,
        &config,
        "synthetic-session",
        &options,
        instructions,
        input.clone(),
        tools.clone(),
        signal.clone(),
        |_| Ok(()),
    )
    .await
    .unwrap();
    let calls = super::super::tool_calls(&result.output).unwrap();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].name, "get_marker");
    input.extend(result.output);
    input.push(
        json!({"type":"function_call_output","call_id":calls[0].id,"output":"JARVIS_SMOKE_OK"}),
    );
    let result = stream(
        &credential,
        &config,
        "synthetic-session",
        &options,
        instructions,
        input,
        tools,
        signal,
        |_| Ok(()),
    )
    .await
    .unwrap();
    assert!(result.text.contains("JARVIS_SMOKE_OK"));
    assert!(super::super::tool_calls(&result.output).unwrap().is_empty());
    assert!(result.usage.is_some_and(|usage| usage.input_tokens > 0));
    println!("Configured Completions: text, usage and synthetic tool round trip passed; no project content sent.");
}
use crate::{
    agent::{ApprovalMode, Mode},
    openai_codex::custom::{Reasoning, TokenField},
};

fn options() -> TurnOptions {
    TurnOptions {
        service_tier: None,
        executor: crate::claude::Executor::Jarvis,
        account: "My.OpenRouter".into(),
        model: "vendor/model-v4".into(),
        reasoning: None,
        mode: Mode::Build,
        workflow: None,
        custom_workflow_id: None,
        custom_agent_id: None,
        approval_mode: ApprovalMode::Yolo,
        manual_validation: false,
        automatic_publication: None,
        model_selection: None,
    }
}
#[test]
fn cache_affinity_is_stable_and_limited_to_documented_provider_hosts() {
    let credential = CodexCredential::new("key", "refresh", 0, "account", None, None);
    for protocol in [
        Protocol::OpenaiCompletions,
        Protocol::OpenaiResponses,
        Protocol::AnthropicMessages,
    ] {
        for host in [
            "https://openrouter.ai/api/v1",
            "https://api.openai.com/v1",
            "https://api.openai.com.evil.test/v1",
            "https://gateway.example/v1",
        ] {
            let mut config = config(protocol);
            config.base_url = host.into();
            let request = session_request(
                &credential,
                &config,
                json!({"model":"model"}),
                "stable-session",
            )
            .unwrap()
            .build()
            .unwrap();
            let body: Value =
                serde_json::from_slice(request.body().unwrap().as_bytes().unwrap()).unwrap();
            let native_openai =
                host == "https://api.openai.com/v1" && protocol != Protocol::AnthropicMessages;
            assert_eq!(
                body["prompt_cache_key"].as_str(),
                native_openai.then_some("stable-session")
            );
            assert_eq!(
                request
                    .headers()
                    .get("x-session-id")
                    .map(|v| v.to_str().unwrap()),
                (host == "https://openrouter.ai/api/v1").then_some("stable-session")
            );
        }
    }
}

#[test]
fn go_requests_use_own_identity_stable_session_and_protocol_specific_auth_only_on_official_origin()
{
    let credential = CodexCredential::new(
        "synthetic-key",
        "",
        i64::MAX,
        "opencode-go:test",
        None,
        None,
    );
    for protocol in [
        Protocol::OpenaiCompletions,
        Protocol::OpenaiResponses,
        Protocol::AnthropicMessages,
    ] {
        for host in [
            "https://opencode.ai/zen/go/v1",
            "https://opencode.ai.evil.test/zen/go/v1",
            "https://opencode.ai/zen/v1",
        ] {
            let mut config = config(protocol);
            config.base_url = host.into();
            config.auth_mode = if protocol == Protocol::AnthropicMessages {
                AuthMode::XApiKey
            } else {
                AuthMode::Bearer
            };
            let request = session_request(
                &credential,
                &config,
                json!({"model":"fixture"}),
                "conversation-stable",
            )
            .unwrap()
            .build()
            .unwrap();
            let official = host == "https://opencode.ai/zen/go/v1";
            assert_eq!(
                request
                    .headers()
                    .get("x-opencode-session")
                    .map(|v| v.to_str().unwrap()),
                official.then_some("conversation-stable")
            );
            assert_eq!(
                request
                    .headers()
                    .get("user-agent")
                    .map(|v| v.to_str().unwrap()),
                official.then_some(crate::openai_codex::opencode_go::user_agent().as_str())
            );
            if protocol == Protocol::AnthropicMessages {
                assert_eq!(request.headers()["x-api-key"], "synthetic-key");
                assert!(!request.headers().contains_key("authorization"));
            } else {
                assert_eq!(request.headers()["authorization"], "Bearer synthetic-key");
                assert!(!request.headers().contains_key("x-api-key"));
            }
            for header in ["chatgpt-account-id", "openai-beta", "x-openrouter-title"] {
                assert!(!request.headers().contains_key(header));
            }
        }
    }
}

#[test]
fn go_toggle_does_not_invent_an_effort_parameter_and_messages_replay_unsigned_thinking() {
    use crate::openai_codex::custom::Reasoning;
    let mut config = config(Protocol::AnthropicMessages);
    config.base_url = crate::openai_codex::opencode_go::BASE_URL.into();
    config.auth_mode = AuthMode::XApiKey;
    config.replay_unsigned_thinking = true;
    config.models[0].reasoning = Reasoning::Toggle;
    config.models[0].reasoning_levels = vec!["off".into(), "on".into()];
    config.models[0].default_reasoning_level = Some("on".into());
    config.validate().unwrap();
    let options = options();
    let scope = request::scope(&config, &options);
    let input = vec![
        json!({"role":"user","content":"Task"}),
        json!({"type":"reasoning","summary":[],"_custom":{"scope":scope,"blocks":[{"type":"thinking","thinking":"Preserved reasoning"}]}}),
        json!({"type":"function_call","call_id":"c1","name":"read","arguments":"{}"}),
        json!({"type":"function_call_output","call_id":"c1","output":"result"}),
    ];
    let body = request::body(
        &config,
        &config.models[0],
        &options,
        "Instructions",
        input,
        vec![tool()],
    )
    .unwrap();
    assert_eq!(body["thinking"], json!({"type":"adaptive"}));
    assert!(body.get("reasoning_effort").is_none());
    assert!(body.get("output_config").is_none());
    assert_eq!(
        body["messages"][1]["content"][0]["thinking"],
        "Preserved reasoning"
    );
}

#[test]
fn go_auxiliary_requests_keep_the_conversation_header_without_mixing_replay_scopes() {
    let credential = CodexCredential::new("key", "", i64::MAX, "opencode-go:fixture", None, None);
    let mut config = config(Protocol::OpenaiResponses);
    config.base_url = crate::openai_codex::opencode_go::BASE_URL.into();
    for session in ["conversation", "conversation:title", "conversation-vision"] {
        let request = session_request(&credential, &config, json!({"model":"fixture"}), session)
            .unwrap()
            .build()
            .unwrap();
        assert_eq!(request.headers()["x-opencode-session"], "conversation");
    }
}

#[test]
fn qwen_messages_efforts_use_enabled_thinking_without_a_conflicting_budget() {
    use crate::openai_codex::custom::Reasoning;
    let mut config = config(Protocol::AnthropicMessages);
    config.base_url = crate::openai_codex::opencode_go::BASE_URL.into();
    config.auth_mode = AuthMode::XApiKey;
    config.models[0].reasoning = Reasoning::EnabledEffort;
    config.models[0].reasoning_levels =
        vec!["off".into(), "low".into(), "medium".into(), "xhigh".into()];
    config.models[0].default_reasoning_level = Some("medium".into());
    config.validate().unwrap();
    for effort in ["off", "low", "medium", "xhigh"] {
        let mut options = options();
        options.reasoning = Some(effort.into());
        let body = request::body(
            &config,
            &config.models[0],
            &options,
            "Instructions",
            vec![json!({"role":"user","content":"Task"})],
            vec![],
        )
        .unwrap();
        assert_eq!(
            body["thinking"]["type"],
            if effort == "off" {
                "disabled"
            } else {
                "enabled"
            }
        );
        assert!(body["thinking"].get("budget_tokens").is_none());
        if effort == "off" {
            assert!(body.get("output_config").is_none());
        } else {
            assert_eq!(body["output_config"]["effort"], effort);
        }
    }
}

#[test]
fn cache_markers_only_target_documented_claude_endpoints() {
    for protocol in [
        Protocol::AnthropicMessages,
        Protocol::OpenaiCompletions,
        Protocol::OpenaiResponses,
    ] {
        let mut config = config(protocol);
        config.models[0].id = "anthropic/claude-sonnet-4.6".into();
        let mut options = options();
        options.model = config.models[0].id.clone();
        for host in [
            "https://gateway.example/v1",
            "https://openrouter.ai/api/v1",
            "https://openrouter.ai.evil.test/v1",
        ] {
            config.base_url = host.into();
            let body = request::body(
                &config,
                &config.models[0],
                &options,
                "Stable instructions",
                vec![json!({"role":"user","content":"Task"})],
                vec![tool()],
            )
            .unwrap();
            let supported =
                host == "https://openrouter.ai/api/v1" && protocol != Protocol::OpenaiResponses;
            assert_eq!(body.to_string().contains("cache_control"), supported);
            if supported && protocol == Protocol::AnthropicMessages {
                assert_eq!(body["system"][0]["text"], "Stable instructions");
                assert_eq!(body["messages"][0]["content"][0]["text"], "Task");
                assert_eq!(body["tools"][0]["cache_control"]["type"], "ephemeral");
            }
        }
        config.base_url = "https://openrouter.ai/api/v1".into();
        config.models[0].id = "z-ai/glm-5.3".into();
        let body = request::body(
            &config,
            &config.models[0],
            &options,
            "System",
            vec![],
            vec![],
        )
        .unwrap();
        assert!(!body.to_string().contains("cache_control"));
    }
}

#[test]
fn completions_cache_reporting_preserves_absence_zero_and_deepseek_alias() {
    for (details, expected) in [
        (json!({}), None),
        (
            json!({"prompt_tokens_details":{"cached_tokens":0,"cache_write_tokens":5}}),
            Some(0),
        ),
        (
            json!({"prompt_tokens_details":{"cached_tokens":60}}),
            Some(60),
        ),
        (json!({"prompt_cache_hit_tokens":60}), Some(60)),
    ] {
        let mut stream = completions::Stream::default();
        let mut usage = json!({"prompt_tokens":100,"completion_tokens":10});
        usage
            .as_object_mut()
            .unwrap()
            .extend(details.as_object().unwrap().clone());
        stream.event(&json!({"choices":[{"index":0,"delta":{"content":"Answer"},"finish_reason":"stop"}],"usage":usage}), &mut |_| Ok(())).unwrap();
        let usage = stream.finish(&json!({})).unwrap().usage.unwrap();
        assert_eq!(usage.input_tokens, 100);
        assert_eq!(usage.cache_read_tokens, expected);
        assert_eq!(
            usage.cache_write_tokens,
            details["prompt_tokens_details"]["cache_write_tokens"].as_u64()
        );
    }
}

#[test]
fn messages_final_cache_counters_replace_partial_counters() {
    let mut stream = messages::Stream::default();
    for event in [
        json!({"type":"message_start","message":{"usage":{"input_tokens":10,"cache_read_input_tokens":0}}}),
        json!({"type":"content_block_start","index":0,"content_block":{"type":"text","text":"Answer"}}),
        json!({"type":"content_block_stop","index":0}),
        json!({"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"cache_read_input_tokens":20,"cache_creation_input_tokens":30,"output_tokens":5}}),
    ] {
        stream.event(&event, &mut |_| Ok(())).unwrap();
    }
    let usage = stream.finish(&json!({})).unwrap().usage.unwrap();
    assert_eq!(usage.input_tokens, 60);
    assert_eq!(usage.cache_read_tokens, Some(20));
    assert_eq!(usage.cache_write_tokens, Some(30));
}
fn config(protocol: Protocol) -> Config {
    Config {
        base_url: "https://gateway.example/api/v1".into(),
        protocol,
        auth_mode: AuthMode::Bearer,
        token_field: TokenField::MaxTokens,
        replay_unsigned_thinking: false,
        models: vec![Model {
            id: "vendor/model-v4".into(),
            name: "Model".into(),
            context_window: 64_000,
            max_output_tokens: 4000,
            supports_images: true,
            supports_tools: true,
            reasoning: Reasoning::None,
            reasoning_levels: vec![],
            default_reasoning_level: None,
            thinking_budget: None,
        }],
    }
}
fn tool() -> Value {
    json!({"type":"function","name":"read","description":"Read a file","parameters":{"type":"object","properties":{"path":{"type":"string"}},"required":["path"]}})
}
fn history() -> Vec<Value> {
    vec![
        json!({"role":"user","content":"Read file"}),
        json!({"type":"function_call","call_id":"c1","name":"read","arguments":"{\"path\":\"README.md\"}"}),
        json!({"type":"function_call_output","call_id":"c1","output":"content"}),
    ]
}

#[test]
fn openrouter_attribution_identifies_jarvis_for_each_protocol_only_on_its_official_origin() {
    let credential = CodexCredential::new("test-key", "", 0, "", None, None);
    for protocol in [
        Protocol::OpenaiCompletions,
        Protocol::OpenaiResponses,
        Protocol::AnthropicMessages,
    ] {
        let mut config = config(protocol);
        for base_url in [
            "https://openrouter.ai/api/v1",
            "https://OPENROUTER.ai:443/api/v1/",
        ] {
            config.base_url = base_url.into();
            let request = authenticated_request(&credential, &config, &json!({}))
                .unwrap()
                .build()
                .unwrap();
            assert_eq!(
                request.headers()["http-referer"],
                "https://github.com/paulovnas/jarvis"
            );
            assert_eq!(request.headers()["x-openrouter-title"], "Jarvis");
            assert_eq!(request.headers()["x-openrouter-app-visibility"], "hidden");
            assert_eq!(request.headers()["authorization"], "Bearer test-key");
            config.base_url = request.url().to_string();
            let explicit_endpoint = authenticated_request(&credential, &config, &json!({}))
                .unwrap()
                .build()
                .unwrap();
            assert_eq!(explicit_endpoint.headers()["x-openrouter-title"], "Jarvis");
        }
        for base_url in [
            "https://gateway.example/v1",
            "http://localhost:8080/v1",
            "https://openrouter.ai.other.example/api/v1",
            "https://openrouter.ai:8443/api/v1",
            "https://gateway.example/openrouter.ai/api/v1",
        ] {
            config.base_url = base_url.into();
            let request = authenticated_request(&credential, &config, &json!({}))
                .unwrap()
                .build()
                .unwrap();
            for header in [
                "http-referer",
                "x-openrouter-title",
                "x-openrouter-app-visibility",
            ] {
                assert!(!request.headers().contains_key(header));
            }
        }
    }
}

#[test]
fn native_optional_schemas_keep_responses_opt_out_without_leaking_it_to_other_protocols() {
    let native = super::super::super::tools::definition(
        "optional_reference",
        "Read the default reference when file is omitted.",
        json!({"topic":{"type":"string"},"file":{"type":"string"}}),
        &["topic"],
    );
    for protocol in [
        Protocol::OpenaiResponses,
        Protocol::OpenaiCompletions,
        Protocol::AnthropicMessages,
    ] {
        let config = config(protocol);
        let body = request::body(
            &config,
            &config.models[0],
            &options(),
            "instructions",
            vec![],
            vec![native.clone()],
        )
        .unwrap();
        let tool = &body["tools"][0];
        let (parameters, description) = match protocol {
            Protocol::OpenaiResponses => {
                assert_eq!(tool, &native);
                assert_eq!(tool["strict"], false);
                (&tool["parameters"], &tool["description"])
            }
            Protocol::OpenaiCompletions => {
                assert!(tool.get("strict").is_none());
                assert!(tool["function"].get("strict").is_none());
                (
                    &tool["function"]["parameters"],
                    &tool["function"]["description"],
                )
            }
            Protocol::AnthropicMessages => {
                assert!(tool.get("strict").is_none());
                (&tool["input_schema"], &tool["description"])
            }
        };
        assert_eq!(parameters, &native["parameters"]);
        assert_eq!(parameters["required"], json!(["topic"]));
        assert_eq!(description, &native["description"]);
    }
}

#[test]
fn anthropic_replay_keeps_historical_calls_paired_with_bounded_unique_ids() {
    let anthropic = config(Protocol::AnthropicMessages);
    let shared = "call_1234567890abcdef12345678".repeat(4);
    let ids = [
        format!("{shared}_first"),
        format!("{shared}_second"),
        "foreign/call|item:1".into(),
        "toolu_valid-1".into(),
        "a".repeat(64),
    ];
    let mut input = vec![json!({"role":"user","content":"Continue the confirmed task."})];
    for (index, id) in ids.iter().enumerate() {
        input.push(json!({"type":"function_call","call_id":id,"name":"read","arguments":format!("{{\"path\":\"file-{index}.txt\"}}" )}));
    }
    for (index, id) in ids.iter().enumerate() {
        input.push(json!({"type":"function_call_output","call_id":id,"output":format!("Confirmed result {index}")}));
    }
    let original = input.clone();
    let build = || {
        request::body(
            &anthropic,
            &anthropic.models[0],
            &options(),
            "instructions",
            input.clone(),
            vec![tool()],
        )
        .unwrap()
    };
    let body = build();
    let calls = body["messages"][1]["content"].as_array().unwrap();
    let results = body["messages"][2]["content"].as_array().unwrap();
    let emitted: Vec<_> = calls
        .iter()
        .map(|call| call["id"].as_str().unwrap())
        .collect();
    assert_eq!(emitted.len(), ids.len());
    assert_eq!(
        emitted
            .iter()
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        ids.len()
    );
    for (index, id) in emitted.iter().enumerate() {
        assert!(!id.is_empty() && id.len() <= 64);
        assert!(id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-')));
        assert_eq!(results[index]["tool_use_id"], *id);
        assert_eq!(calls[index]["input"]["path"], format!("file-{index}.txt"));
        assert_eq!(
            results[index]["content"],
            format!("Confirmed result {index}")
        );
    }
    assert_eq!(emitted[3], ids[3]);
    assert_eq!(emitted[4], ids[4]);
    assert_eq!(body, build());
    assert_eq!(input, original);

    for protocol in [Protocol::OpenaiCompletions, Protocol::OpenaiResponses] {
        let config = config(protocol);
        let body = request::body(
            &config,
            &config.models[0],
            &options(),
            "instructions",
            input.clone(),
            vec![tool()],
        )
        .unwrap();
        for (index, id) in ids.iter().enumerate() {
            match protocol {
                Protocol::OpenaiCompletions => {
                    assert_eq!(body["messages"][2]["tool_calls"][index]["id"], *id);
                    assert_eq!(body["messages"][3 + index]["tool_call_id"], *id);
                }
                Protocol::OpenaiResponses => {
                    assert_eq!(body["input"][1 + index]["call_id"], *id);
                    assert_eq!(body["input"][1 + ids.len() + index]["call_id"], *id);
                }
                Protocol::AnthropicMessages => unreachable!(),
            }
        }
    }
}

#[test]
fn anthropic_request_projects_runtime_feedback_after_the_paired_historical_result() {
    let config = config(Protocol::AnthropicMessages);
    let id = "call_1234567890abcdef12345678".repeat(4);
    let input = vec![
        json!({"role":"user","content":"Continue the requested task."}),
        json!({"type":"function_call","call_id":id,"name":"read","arguments":"{}"}),
        json!({"role":"user","_jarvis_runtime":true,"_jarvis_learning":true,"content":"Validated internal feedback"}),
        json!({"type":"function_call_output","call_id":id,"output":"Confirmed historical result"}),
        json!({"role":"user","content":"Use the updated requirement."}),
    ];
    let original = input.clone();
    let body = request::body(
        &config,
        &config.models[0],
        &options(),
        "instructions",
        super::super::provider_input(input.clone()),
        vec![tool()],
    )
    .unwrap();
    let call = &body["messages"][1]["content"][0];
    let result = &body["messages"][2]["content"][0];
    assert_eq!(call["type"], "tool_use");
    assert!(call["id"].as_str().unwrap().len() <= 64);
    assert_eq!(body["messages"][2]["role"], "user");
    assert_eq!(result["type"], "tool_result");
    assert_eq!(result["tool_use_id"], call["id"]);
    assert_eq!(result["content"], "Confirmed historical result");
    assert_eq!(
        body["messages"][3]["content"],
        "Validated internal feedback"
    );
    assert_eq!(
        body["messages"][4]["content"],
        "Use the updated requirement."
    );
    assert_eq!(input, original);
}

#[test]
fn anthropic_replay_reserves_valid_ids_before_normalizing_foreign_ids() {
    let config = config(Protocol::AnthropicMessages);
    let invalid = "call_1234567890abcdef12345678".repeat(4);
    let mut input = vec![
        json!({"role":"user","content":"Continue."}),
        json!({"type":"function_call","call_id":invalid,"name":"read","arguments":"{}"}),
        json!({"type":"function_call_output","call_id":invalid,"output":"First confirmed result"}),
    ];
    let build = |input: Vec<Value>| {
        request::body(
            &config,
            &config.models[0],
            &options(),
            "instructions",
            input,
            vec![tool()],
        )
        .unwrap()
    };
    let initial = build(input.clone());
    let reserved = initial["messages"][1]["content"][0]["id"].as_str().unwrap();
    assert_ne!(reserved, invalid);
    input.insert(
        2,
        json!({"type":"function_call","call_id":reserved,"name":"read","arguments":"{}"}),
    );
    input.push(json!({"type":"function_call_output","call_id":reserved,"output":"Second confirmed result"}));
    let body = build(input.clone());
    let calls = &body["messages"][1]["content"];
    let results = &body["messages"][2]["content"];
    assert_ne!(calls[0]["id"], calls[1]["id"]);
    assert_eq!(calls[1]["id"], reserved);
    for index in 0..2 {
        let id = calls[index]["id"].as_str().unwrap();
        assert!(id.len() <= 64);
        assert_eq!(results[index]["tool_use_id"], id);
    }
    assert_eq!(body, build(input));
}

#[test]
fn protocols_translate_tools_history_images_and_output_limits_without_codex_fields() {
    for protocol in [
        Protocol::OpenaiCompletions,
        Protocol::OpenaiResponses,
        Protocol::AnthropicMessages,
    ] {
        let config = config(protocol);
        let body = request::body(
            &config,
            &config.models[0],
            &options(),
            "instructions",
            history(),
            vec![tool()],
        )
        .unwrap();
        assert_eq!(body["model"], "vendor/model-v4");
        assert!(body.get("prompt_cache_key").is_none());
        assert!(body.get("include").is_none());
        assert!(body.get("parallel_tool_calls").is_none());
        match protocol {
            Protocol::OpenaiCompletions => {
                assert_eq!(body["max_tokens"], 4000);
                assert_eq!(
                    body["messages"][2]["tool_calls"][0]["function"]["name"],
                    "read"
                );
                assert_eq!(body["messages"][3]["tool_call_id"], "c1");
                assert_eq!(body["tools"][0]["function"]["name"], "read");
            }
            Protocol::OpenaiResponses => {
                assert_eq!(body["max_output_tokens"], 4000);
                assert_eq!(body["input"][1]["call_id"], "c1");
                assert_eq!(body["tools"][0]["name"], "read");
            }
            Protocol::AnthropicMessages => {
                assert_eq!(body["max_tokens"], 4000);
                assert_eq!(
                    body["messages"][1]["content"][0]["input"]["path"],
                    "README.md"
                );
                assert_eq!(body["messages"][2]["content"][0]["tool_use_id"], "c1");
                assert_eq!(body["tools"][0]["input_schema"]["type"], "object");
            }
        }
        let credential = CodexCredential::new("private-key", "", i64::MAX, "custom:1", None, None);
        let request = authenticated_request(&credential, &config, &body)
            .unwrap()
            .build()
            .unwrap();
        assert_eq!(request.headers()["authorization"], "Bearer private-key");
        for header in [
            "chatgpt-account-id",
            "openai-beta",
            "originator",
            "session_id",
        ] {
            assert!(!request.headers().contains_key(header));
        }
    }
    let mut config = config(Protocol::AnthropicMessages);
    config.auth_mode = AuthMode::XApiKey;
    let input = vec![
        json!({"role":"user","content":[{"type":"input_text","text":"Describe"},{"type":"input_image","image_url":"data:image/png;base64,YQ=="}]}),
    ];
    let body = request::body(
        &config,
        &config.models[0],
        &options(),
        "instructions",
        input,
        vec![],
    )
    .unwrap();
    assert_eq!(
        body["messages"][0]["content"][1]["source"]["media_type"],
        "image/png"
    );
    let request = authenticated_request(
        &CodexCredential::new("key", "", 0, "", None, None),
        &config,
        &body,
    )
    .unwrap()
    .build()
    .unwrap();
    assert_eq!(request.headers()["x-api-key"], "key");
    assert!(!request.headers().contains_key("authorization"));
}
#[test]
fn configured_reasoning_is_sent_only_in_selected_protocol_format() {
    for (protocol, reasoning, field) in [
        (
            Protocol::OpenaiCompletions,
            Reasoning::Effort,
            "reasoning_effort",
        ),
        (
            Protocol::OpenaiCompletions,
            Reasoning::Openrouter,
            "reasoning",
        ),
        (Protocol::OpenaiCompletions, Reasoning::Deepseek, "thinking"),
        (Protocol::OpenaiResponses, Reasoning::Effort, "reasoning"),
        (Protocol::AnthropicMessages, Reasoning::Adaptive, "thinking"),
    ] {
        let mut config = config(protocol);
        config.models[0].reasoning = reasoning;
        config.models[0].reasoning_levels = vec!["high".into()];
        config.models[0].default_reasoning_level = Some("high".into());
        let body = request::body(
            &config,
            &config.models[0],
            &options(),
            "",
            history(),
            vec![],
        )
        .unwrap();
        assert!(body.get(field).is_some());
        let mut invalid = options();
        invalid.reasoning = Some("unknown".into());
        assert!(
            request::body(&config, &config.models[0], &invalid, "", history(), vec![]).is_err()
        );
    }
}
#[test]
fn messages_sends_each_selected_effort_without_openai_fields_or_invented_budgets() {
    let mut config = config(Protocol::AnthropicMessages);
    config.models[0].reasoning = Reasoning::Adaptive;
    config.models[0].reasoning_levels = ["off", "low", "medium", "high", "xhigh", "max"]
        .map(str::to_owned)
        .to_vec();
    config.models[0].default_reasoning_level = Some("max".into());
    config.validate().unwrap();
    for level in &config.models[0].reasoning_levels {
        let mut options = options();
        options.reasoning = Some(level.clone());
        let body =
            request::body(&config, &config.models[0], &options, "", history(), vec![]).unwrap();
        if level == "off" {
            assert_eq!(body["thinking"], json!({"type":"disabled"}));
            assert!(body.get("output_config").is_none());
        } else {
            assert_eq!(body["thinking"], json!({"type":"adaptive"}));
            assert_eq!(body["output_config"], json!({"effort":level}));
        }
        assert!(body.get("reasoning").is_none());
        assert!(body.get("reasoning_effort").is_none());
        assert!(body["thinking"].get("budget_tokens").is_none());
    }
}
#[test]
fn completions_replay_preserves_reasoning_and_fragmented_tool_arguments_only_in_its_scope() {
    let config = config(Protocol::OpenaiCompletions);
    let scope = request::scope(&config, &options());
    let mut stream = completions::Stream::default();
    let mut deltas = vec![];
    for delta in [
        json!({"reasoning_content":"Inspect", "reasoning_details":[{"index":0,"type":"reasoning.encrypted","data":"abc"}]}),
        json!({"tool_calls":[{"index":0,"id":"c1","function":{"name":"read","arguments":"{\"path\":"}}]}),
        json!({"tool_calls":[{"index":0,"function":{"arguments":"\"README.md\"}"}}]}),
    ] {
        stream
            .event(
                &json!({"choices":[{"index":0,"delta":delta}]}),
                &mut |delta| {
                    deltas.push(delta);
                    Ok(())
                },
            )
            .unwrap();
    }
    stream
        .event(
            &json!({"choices":[{"index":0,"delta":{},"finish_reason":"tool_calls"}]}),
            &mut |_| Ok(()),
        )
        .unwrap();
    stream
        .event(
            &json!({"choices":[],"usage":{"prompt_tokens":100,"completion_tokens":20}}),
            &mut |_| Ok(()),
        )
        .unwrap();
    let response = stream.finish(&scope).unwrap();
    assert_eq!(response.usage.unwrap().input_tokens, 100);
    assert_eq!(response.summary, "Inspect");
    let mut replay = vec![json!({"role":"user","content":"Read"})];
    replay.extend(response.output);
    replay.push(json!({"type":"function_call_output","call_id":"c1","output":"done"}));
    let body = request::body(
        &config,
        &config.models[0],
        &options(),
        "",
        replay.clone(),
        vec![tool()],
    )
    .unwrap();
    assert_eq!(body["messages"][2]["reasoning_content"], "Inspect");
    assert_eq!(body["messages"][2]["reasoning_details"][0]["data"], "abc");
    assert_eq!(body["messages"][2]["tool_calls"][0]["id"], "c1");
    let mut foreign = options();
    foreign.account = "other".into();
    let body = request::body(
        &config,
        &config.models[0],
        &foreign,
        "",
        replay.clone(),
        vec![],
    )
    .unwrap();
    assert!(!body.to_string().contains("Inspect"));
    assert!(!body.to_string().contains("_custom"));
    let codex_options = options();
    let credential = CodexCredential::new("", "", 0, "", None, None);
    let capabilities = ModelCapabilities::resolve_for_options(&credential, &codex_options);
    let body = crate::agent::provider::request_body(
        &codex_options,
        &capabilities,
        "",
        replay,
        vec![],
        "session",
    );
    assert!(!body.to_string().contains("_custom"));
}

fn go_deepseek_fixture(protocol: Protocol) -> (Config, TurnOptions) {
    let mut config = config(protocol);
    config.base_url = crate::openai_codex::opencode_go::BASE_URL.into();
    config.token_field = TokenField::MaxCompletionTokens;
    let model = &mut config.models[0];
    model.id = if protocol == Protocol::OpenaiResponses {
        "deepseek-v4-flash"
    } else {
        "deepseek-v4.1-flash"
    }
    .into();
    model.context_window = 1_000_000;
    model.max_output_tokens = 32_000;
    model.reasoning = Reasoning::Effort;
    model.reasoning_levels = ["low", "high", "max"].map(str::to_owned).to_vec();
    model.default_reasoning_level = Some("high".into());
    let mut options = options();
    options.account = "opencode-go-personal".into();
    options.model = model.id.clone();
    options.reasoning = Some("high".into());
    config.validate().unwrap();
    (config, options)
}

#[test]
fn go_v41_first_greeting_keeps_high_reasoning_and_the_completions_gateway_envelope() {
    let (config, options) = go_deepseek_fixture(Protocol::OpenaiCompletions);
    let body = request::body(
        &config,
        &config.models[0],
        &options,
        "System instructions",
        vec![json!({"role":"user","content":"Oi, boa noite"})],
        vec![tool()],
    )
    .unwrap();
    assert_eq!(
        config.endpoint().unwrap().as_str(),
        "https://opencode.ai/zen/go/v1/chat/completions"
    );
    assert_eq!(body["max_completion_tokens"], 32_000);
    assert_eq!(body["reasoning_effort"], "high");
    assert_eq!(body["messages"][1]["content"], "Oi, boa noite");
    assert_eq!(body["tools"][0]["function"]["name"], "read");
    for field in [
        "max_tokens",
        "thinking",
        "tool_choice",
        "parallel_tool_calls",
    ] {
        assert!(body.get(field).is_none(), "unexpected {field}");
    }
}

#[test]
fn go_completions_replays_scoped_reasoning_and_adds_empty_fields_to_other_assistant_turns() {
    let (config, options) = go_deepseek_fixture(Protocol::OpenaiCompletions);
    let scope = request::scope(&config, &options);
    let mut stream = completions::Stream::default();
    stream.event(&json!({"choices":[{"index":0,"delta":{"reasoning_content":"Inspect the project first.","tool_calls":[{"index":0,"id":"read-1","function":{"name":"read","arguments":"{\"path\":\"README.md\"}"}}]},"finish_reason":"tool_calls"}]}), &mut |_| Ok(())).unwrap();
    let response = stream.finish(&scope).unwrap();
    let mut input = vec![
        json!({"role":"user","content":"Greeting"}),
        json!({"type":"message","role":"assistant","content":[{"type":"output_text","text":"Earlier reply from another model."}]}),
        json!({"role":"user","content":"Read the project"}),
    ];
    input.extend(response.output);
    input
        .push(json!({"type":"function_call_output","call_id":"read-1","output":"Readme contents"}));
    input.push(json!({"type":"function_call","call_id":"read-2","name":"read","arguments":"{}"}));
    input.push(json!({"type":"function_call_output","call_id":"read-2","output":"Another result"}));
    let body = request::body(
        &config,
        &config.models[0],
        &options,
        "System",
        input,
        vec![tool()],
    )
    .unwrap();
    let assistant: Vec<_> = body["messages"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|item| item["role"] == "assistant")
        .collect();
    assert_eq!(assistant.len(), 3);
    assert_eq!(assistant[0]["reasoning_content"], "");
    assert_eq!(
        assistant[1]["reasoning_content"],
        "Inspect the project first."
    );
    assert_eq!(assistant[1]["tool_calls"][0]["id"], "read-1");
    assert_eq!(assistant[2]["reasoning_content"], "");
    assert_eq!(assistant[2]["tool_calls"][0]["id"], "read-2");
    assert!(body.to_string().contains("Readme contents"));
    assert!(!body.to_string().contains("_custom"));
}

#[test]
fn go_deepseek_native_decision_keeps_published_result_and_reasoning_in_order() {
    let (config, mut options) = go_deepseek_fixture(Protocol::OpenaiCompletions);
    options.reasoning = Some("max".into());
    let scope = request::scope(&config, &options);
    let mut stream = completions::Stream::default();
    stream.event(&json!({"choices":[{"index":0,"delta":{"reasoning_content":"Publish the approved change.","tool_calls":[{"index":0,"id":"publish","function":{"name":"jarvis_propose_publication","arguments":"{}"}}]},"finish_reason":"tool_calls"}]}), &mut |_| Ok(())).unwrap();
    let mut input = vec![json!({"role":"user","content":"Try again"})];
    input.extend(stream.finish(&scope).unwrap().output);
    input.push(json!({"role":"user","_jarvis_runtime":true,"_jarvis_authoring_decision":true,"content":"Native approval recorded"}));
    let output =
        json!({"status":"published","repositories":[{"commit":"fixture-commit","push":"normal"}]})
            .to_string();
    input.push(json!({"type":"function_call_output","call_id":"publish","output":output}));
    let body = request::body(
        &config,
        &config.models[0],
        &options,
        "System",
        super::super::provider_input(input),
        vec![json!({"type":"function","name":"jarvis_propose_publication","parameters":{"type":"object","properties":{}}})],
    ).unwrap();
    let messages = body["messages"].as_array().unwrap();
    assert_eq!(
        messages
            .iter()
            .map(|item| item["role"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["system", "user", "assistant", "tool", "user"]
    );
    assert_eq!(
        messages[2]["reasoning_content"],
        "Publish the approved change."
    );
    assert_eq!(messages[2]["tool_calls"][0]["id"], "publish");
    assert_eq!(messages[3]["tool_call_id"], "publish");
    assert_eq!(messages[3]["content"], output);
    assert_eq!(messages[4]["content"], "Native approval recorded");
    assert_eq!(body["reasoning_effort"], "max");
    assert!(!body.to_string().contains("_jarvis_"));
}

#[test]
fn go_completions_never_replays_foreign_reasoning_or_ciphertext() {
    let (config, options) = go_deepseek_fixture(Protocol::OpenaiCompletions);
    for (field, foreign) in [
        ("account", json!("other-account")),
        ("model", json!("deepseek-v4-pro")),
        ("endpoint", json!("https://api.deepseek.com/v1")),
        ("protocol", json!("openai-responses")),
    ] {
        let mut scope = request::scope(&config, &options);
        scope[field] = foreign;
        let body = request::body(
            &config,
            &config.models[0],
            &options,
            "",
            vec![
                json!({"role":"user","content":"Read"}),
                json!({"type":"reasoning","encrypted_content":"foreign-cipher","_custom":{"scope":scope,"reasoning_content":"foreign-thinking","reasoning_details":[{"type":"reasoning.encrypted","data":"foreign-private-data"}]}}),
                json!({"type":"function_call","call_id":"read-1","name":"read","arguments":"{}"}),
                json!({"type":"function_call_output","call_id":"read-1","output":"Confirmed result"}),
            ],
            vec![tool()],
        )
        .unwrap();
        assert_eq!(body["messages"][2]["reasoning_content"], "");
        assert_eq!(body["messages"][2]["tool_calls"][0]["id"], "read-1");
        assert!(body.to_string().contains("Confirmed result"));
        assert!(!body.to_string().contains("foreign-"));
    }
}

#[test]
fn go_responses_replays_native_plain_reasoning_and_preserves_the_tool_result() {
    let (config, options) = go_deepseek_fixture(Protocol::OpenaiResponses);
    let native_reasoning = json!({"type":"reasoning","id":"reason-1","summary":[],"content":[{"type":"reasoning_text","text":"Inspect the project before answering."}]});
    let mut response = fixture_response(
        Protocol::OpenaiResponses,
        &[json!({"status":"completed","output":[native_reasoning,{"type":"function_call","call_id":"read-1","name":"read","arguments":"{}"}]})],
    )
    .unwrap();
    scope_responses_output(&config, &options, &mut response);
    let mut input = vec![json!({"role":"user","content":"Read"})];
    input.extend(response.output);
    input.push(
        json!({"type":"function_call_output","call_id":"read-1","output":"Confirmed result"}),
    );
    let body = request::body(
        &config,
        &config.models[0],
        &options,
        "",
        input,
        vec![tool()],
    )
    .unwrap();
    assert_eq!(body["reasoning"]["effort"], "high");
    assert_eq!(body["input"][1], native_reasoning);
    assert_eq!(body["input"][2]["call_id"], "read-1");
    assert_eq!(body["input"][3]["output"], "Confirmed result");
    assert!(!body.to_string().contains("reasoning unavailable"));
    assert!(!body.to_string().contains("_custom"));
}

#[test]
fn go_responses_uses_an_honest_missing_history_marker_without_foreign_replay() {
    let (config, options) = go_deepseek_fixture(Protocol::OpenaiResponses);
    let mut foreign_scope = request::scope(&config, &options);
    foreign_scope["account"] = json!("other-account");
    let body = request::body(
        &config,
        &config.models[0],
        &options,
        "",
        vec![
            json!({"role":"user","content":"Task"}),
            json!({"type":"reasoning","encrypted_content":"foreign-cipher","summary":[],"content":[{"type":"reasoning_text","text":"foreign-thinking"}],"_custom":{"scope":foreign_scope}}),
            json!({"type":"message","role":"assistant","content":[{"type":"output_text","text":"Previous answer"}]}),
            json!({"role":"user","content":"Continue"}),
            json!({"type":"function_call","call_id":"read-1","name":"read","arguments":"{}"}),
            json!({"type":"function_call_output","call_id":"read-1","output":"Confirmed result"}),
        ],
        vec![tool()],
    )
    .unwrap();
    let input = body["input"].as_array().unwrap();
    let reasoning: Vec<_> = input
        .iter()
        .filter(|item| item["type"] == "reasoning")
        .collect();
    assert_eq!(reasoning.len(), 2);
    for item in reasoning {
        assert_eq!(item["content"][0]["text"], "reasoning unavailable");
        assert!(item.get("encrypted_content").is_none());
    }
    assert_eq!(input[1]["type"], "reasoning");
    assert_eq!(input[2]["role"], "assistant");
    assert_eq!(input[4]["type"], "reasoning");
    assert_eq!(input[5]["call_id"], "read-1");
    assert_eq!(input[6]["output"], "Confirmed result");
    assert!(!body.to_string().contains("foreign-"));
}

#[test]
fn go_reasoning_history_compatibility_never_leaks_to_other_hosts_models_or_disabled_turns() {
    for protocol in [Protocol::OpenaiCompletions, Protocol::OpenaiResponses] {
        let (baseline, baseline_options) = go_deepseek_fixture(protocol);
        for (host, model, reasoning) in [
            (
                "https://opencode.ai.evil.test/zen/go/v1",
                baseline_options.model.as_str(),
                "high",
            ),
            (
                "https://opencode.ai:444/zen/go/v1",
                baseline_options.model.as_str(),
                "high",
            ),
            (
                "https://opencode.ai/zen/v1",
                baseline_options.model.as_str(),
                "high",
            ),
            (
                "https://gateway.example/v1",
                baseline_options.model.as_str(),
                "high",
            ),
            (
                crate::openai_codex::opencode_go::BASE_URL,
                "other-model",
                "high",
            ),
            (
                crate::openai_codex::opencode_go::BASE_URL,
                baseline_options.model.as_str(),
                "none",
            ),
        ] {
            let mut config = baseline.clone();
            config.base_url = host.into();
            config.models[0].id = model.into();
            config.models[0].reasoning_levels.push("none".into());
            let mut options = baseline_options.clone();
            options.model = model.into();
            options.reasoning = Some(reasoning.into());
            let body = request::body(
                &config,
                &config.models[0],
                &options,
                "",
                vec![json!({"type":"message","role":"assistant","content":[{"type":"output_text","text":"Previous answer"}]})],
                vec![],
            )
            .unwrap();
            assert!(!body.to_string().contains("reasoning unavailable"));
            assert!(!body.to_string().contains("reasoning_content"));
        }
    }
}

#[test]
fn ordinary_responses_keeps_exact_scoped_encrypted_replay_and_rejects_plain_or_foreign_items() {
    let config = config(Protocol::OpenaiResponses);
    let options = options();
    let scope = request::scope(&config, &options);
    let body = request::body(
        &config,
        &config.models[0],
        &options,
        "",
        vec![
            json!({"type":"reasoning","encrypted_content":"native-cipher","summary":[],"_custom":{"scope":scope}}),
            json!({"type":"reasoning","content":[{"type":"reasoning_text","text":"plain-private"}],"_custom":{"scope":scope}}),
            json!({"type":"reasoning","encrypted_content":"foreign-cipher","_custom":{"scope":{"account":"other"}}}),
            json!({"type":"message","role":"assistant","content":[{"type":"output_text","text":"Previous answer"}]}),
        ],
        vec![],
    )
    .unwrap();
    let input = body["input"].as_array().unwrap();
    assert_eq!(input.len(), 2);
    assert_eq!(input[0]["encrypted_content"], "native-cipher");
    assert_eq!(input[1]["role"], "assistant");
    assert!(!body.to_string().contains("plain-private"));
    assert!(!body.to_string().contains("foreign-cipher"));
    assert!(!body.to_string().contains("_custom"));
}

#[test]
fn incomplete_streams_never_return_dispatchable_tools() {
    let mut stream = completions::Stream::default();
    stream.event(&json!({"choices":[{"delta":{"tool_calls":[{"index":0,"id":"c1","function":{"name":"read","arguments":"{"}}]}}]}), &mut |_| Ok(())).unwrap();
    assert!(stream.finish(&json!({})).is_err());
    let mut stream = completions::Stream::default();
    assert!(stream
        .event(
            &json!({"choices":[{"delta":{},"finish_reason":"length"}]}),
            &mut |_| Ok(())
        )
        .is_err());
    let mut stream = messages::Stream::default();
    stream
        .event(
            &json!({"type":"message_start","message":{"usage":{}}}),
            &mut |_| Ok(()),
        )
        .unwrap();
    stream.event(&json!({"type":"content_block_start","index":0,"content_block":{"type":"tool_use","id":"c","name":"read","input":{}}}), &mut |_| Ok(())).unwrap();
    assert!(stream.finish(&json!({})).is_err());
}

#[test]
fn openrouter_duplicate_terminal_usage_is_accepted_without_accepting_late_content() {
    let finish = json!({"choices":[{"index":0,"delta":{"role":"assistant","content":""},"finish_reason":"stop"}],"usage":{"prompt_tokens":12,"completion_tokens":66}});
    for late in [
        finish.clone(),
        json!({"usage":{"prompt_tokens":12,"completion_tokens":66}}),
    ] {
        let mut stream = completions::Stream::default();
        stream
            .event(
                &json!({"choices":[{"index":0,"delta":{"content":"PONG"}}]}),
                &mut |_| Ok(()),
            )
            .unwrap();
        stream.event(&json!({"choices":[{"index":0,"delta":{"role":"assistant","content":"","reasoning":null},"finish_reason":"stop"}]}), &mut |_| Ok(())).unwrap();
        stream.event(&late, &mut |_| Ok(())).unwrap();
        let response = stream.finish(&json!({})).unwrap();
        assert_eq!(response.text, "PONG");
        assert_eq!(response.usage.unwrap().output_tokens, 66);
    }
    for delta in [
        json!({"content":"unexpected"}),
        json!({"tool_calls":[{"index":0}]}),
        json!({"reasoning":"late"}),
        json!("malformed delta"),
    ] {
        let mut stream = completions::Stream::default();
        stream.event(&finish, &mut |_| Ok(())).unwrap();
        assert!(stream
            .event(
                &json!({"choices":[{"index":0,"delta":delta,"finish_reason":"stop"}]}),
                &mut |_| Ok(())
            )
            .is_err());
    }
    let mut stream = completions::Stream::default();
    stream.event(&finish, &mut |_| Ok(())).unwrap();
    assert!(stream
        .event(
            &json!({"choices":[{"index":0,"delta":{},"finish_reason":"tool_calls"}]}),
            &mut |_| Ok(())
        )
        .is_err());
}

#[test]
fn unsigned_thinking_requires_gateway_compatibility_and_tools_follow_capabilities() {
    let mut config = config(Protocol::AnthropicMessages);
    let thinking = json!({"type":"reasoning","_custom":{"scope":request::scope(&config, &options()),"blocks":[{"type":"thinking","thinking":"private replay"}]}});
    let mut replay = vec![thinking];
    replay.extend(history());
    let strict = request::body(
        &config,
        &config.models[0],
        &options(),
        "",
        replay.clone(),
        vec![tool()],
    )
    .unwrap();
    assert!(!strict.to_string().contains("private replay"));
    config.replay_unsigned_thinking = true;
    config.models[0].supports_tools = false;
    let gateway = request::body(
        &config,
        &config.models[0],
        &options(),
        "",
        replay,
        vec![tool()],
    )
    .unwrap();
    assert_eq!(gateway["messages"][0]["content"][0]["signature"], "");
    assert!(gateway.to_string().contains("private replay"));
    assert!(gateway.get("tools").is_none());
}
#[test]
fn messages_replay_keeps_signed_thinking_and_cache_usage() {
    let mut stream = messages::Stream::default();
    for event in [
        json!({"type":"message_start","message":{"usage":{"input_tokens":10,"cache_read_input_tokens":20,"cache_creation_input_tokens":30}}}),
        json!({"type":"content_block_start","index":0,"content_block":{"type":"thinking","thinking":"Plan","signature":""}}),
        json!({"type":"content_block_delta","index":0,"delta":{"type":"signature_delta","signature":"opaque"}}),
        json!({"type":"content_block_stop","index":0}),
        json!({"type":"content_block_start","index":1,"content_block":{"type":"tool_use","id":"c1","name":"read","input":{}}}),
        json!({"type":"content_block_delta","index":1,"delta":{"type":"input_json_delta","partial_json":"{\"path\":\"README.md\"}"}}),
        json!({"type":"content_block_stop","index":1}),
        json!({"type":"message_delta","delta":{"stop_reason":"tool_use"},"usage":{"output_tokens":5}}),
    ] {
        stream.event(&event, &mut |_| Ok(())).unwrap();
    }
    let config = config(Protocol::AnthropicMessages);
    let response = stream.finish(&request::scope(&config, &options())).unwrap();
    let usage = response.usage.unwrap();
    assert_eq!(usage.input_tokens, 60);
    assert_eq!(usage.cache_read_tokens, Some(20));
    assert_eq!(usage.cache_write_tokens, Some(30));
    let body = request::body(
        &config,
        &config.models[0],
        &options(),
        "",
        response.output,
        vec![tool()],
    )
    .unwrap();
    assert_eq!(body["messages"][0]["content"][0]["signature"], "opaque");
    assert_eq!(body["messages"][0]["content"][1]["type"], "tool_use");
}
#[tokio::test]
async fn custom_http_errors_only_expose_known_protocol_parameters() {
    use std::io::{Read, Write};

    for (status, parameter, expected) in [
        (400, Some("max_tokens"), Some("max_tokens")),
        (
            400,
            Some("messages[0].reasoning_content"),
            Some("messages[0].reasoning_content"),
        ),
        (400, Some("sk_test_private_credential"), None),
        (400, Some("messages[0].content.private_text"), None),
        (400, Some("messages[0].content.private text"), None),
        (
            400,
            Some("tools[0].function.parameters.properties.customer_name"),
            None,
        ),
        (400, None, None),
        (401, Some("max_tokens"), None),
        (429, Some("max_tokens"), None),
    ] {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let body = json!({
            "error": {
                "type": "invalid_request_error",
                "param": parameter,
                "message": "Echoed sk_test_private_credential and private text; field max_tokens",
            }
        })
        .to_string();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let _ = stream.read(&mut [0; 4096]);
            write!(stream, "HTTP/1.1 {status} Test\r\nContent-Length: {}\r\nX-Request-ID: req-safe-custom\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
        });
        let (_cancel, signal) = watch::channel(false);
        let error = receive(
            reqwest::Client::new().post(url),
            Protocol::OpenaiCompletions,
            &json!({}),
            signal,
            |_| Ok(()),
        )
        .await
        .err()
        .unwrap();

        if let Some(parameter) = expected {
            assert_eq!(
                error.message,
                format!("O provedor recusou o campo {parameter} da solicitação. O progresso foi preservado."),
            );
        } else {
            assert!(!error.message.contains("campo"));
        }
        assert!(!error.message.contains("private"));
        assert!(!error.message.contains("customer_name"));
        assert_eq!(
            error.code,
            match status {
                401 => "provider_auth",
                429 => "provider_limit",
                _ => "provider_request",
            },
        );
        let metadata = error.provider_metadata.unwrap();
        assert_eq!(metadata.http_status, Some(status));
        assert_eq!(
            metadata.upstream_code.as_deref(),
            Some("invalid_request_error"),
        );
        assert_eq!(metadata.request_id.as_deref(), Some("req-safe-custom"));
        server.join().unwrap();
    }
}

#[tokio::test]
async fn local_sse_reads_trailing_usage_and_classifies_errors_without_contacting_custom_accounts() {
    use std::io::{Read, Write};
    for (status, body, code) in [(200, "data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Olá\"},\"finish_reason\":\"stop\"}]}\n\ndata: {\"choices\":[],\"usage\":{\"prompt_tokens\":20,\"completion_tokens\":2}}\n\ndata: [DONE]\n\n", "ok"), (400, "{\"error\":{\"message\":\"prompt is too long\"}}", "context_overflow"), (401, "private key invalid", "provider_auth"), (200, "data: {\"error\":{\"code\":\"context_length_exceeded\"}}\n\n", "context_overflow")] {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap(); let url = format!("http://{}", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || { let (mut stream, _) = listener.accept().unwrap(); let _ = stream.read(&mut [0;4096]); write!(stream,"HTTP/1.1 {status} Test\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).unwrap(); });
        let (_send, signal) = watch::channel(false); let result = receive(reqwest::Client::new().post(url), Protocol::OpenaiCompletions, &json!({}), signal, |_| Ok(())).await;
        if code == "ok" { let result = result.unwrap(); assert_eq!(result.text,"Olá"); assert_eq!(result.usage.unwrap().input_tokens,20); }
        else { let error = result.err().unwrap(); assert_eq!(error.code,code); assert!(!error.message.contains("private")); }
        server.join().unwrap();
    }
}
