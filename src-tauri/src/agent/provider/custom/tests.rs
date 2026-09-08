use super::*;

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
        account: "My.OpenRouter".into(),
        model: "vendor/model-v4".into(),
        reasoning: None,
        mode: Mode::Build,
        workflow: None,
        approval_mode: ApprovalMode::Yolo,
    }
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
    let body = crate::agent::provider::request_body(&options(), "", replay, vec![], "session");
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
    assert_eq!(response.usage.unwrap().input_tokens, 60);
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
