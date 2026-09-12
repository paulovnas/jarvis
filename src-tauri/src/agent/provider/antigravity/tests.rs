use super::*;
fn options(model: &str) -> TurnOptions {
    TurnOptions {
        account: "antigravity-test".into(),
        model: model.into(),
        reasoning: Some("low".into()),
        mode: crate::agent::Mode::Build,
        workflow: None,
        custom_workflow_id: None,
        custom_agent_id: None,
        approval_mode: crate::agent::ApprovalMode::Manual,
        manual_validation: false,
    }
}

#[test]
fn grounding_uses_selected_gemini_and_only_verified_source_metadata() {
    let body = grounded_body(
        &credential(),
        "session",
        "gemini-3.8-flash",
        "Tauri docs",
        crate::system::ResponseLanguage::English,
    )
    .unwrap();
    assert_eq!(body["model"], "gemini-3.8-flash");
    assert_eq!(body["request"]["tools"], json!([{"googleSearch":{}}]));
    assert!(grounded_body(
        &credential(),
        "session",
        "claude-opus",
        "Tauri docs",
        crate::system::ResponseLanguage::English
    )
    .is_err());
    assert!(body["request"]["systemInstruction"]["parts"][0]["text"]
        .as_str()
        .is_some_and(|text| text.contains("Use English for user-facing prose")));
    let mut output = Output::default();
    output.event(&json!({"response":{"candidates":[{"content":{"parts":[{"text":"Documentação"}]},"groundingMetadata":{"groundingChunks":[{"web":{"uri":"https://v2.tauri.app/","title":"Tauri"}}]},"finishReason":"STOP"}]}}), &mut |_| Ok(())).unwrap();
    let response = output.finish("gemini-3.8-flash").unwrap();
    assert_eq!(response.output[0]["type"], "web_search_call");
    assert_eq!(
        response.output[0]["action"]["sources"][0]["url"],
        "https://v2.tauri.app/"
    );
    let mut plain = Output::default();
    plain.event(&json!({"candidates":[{"content":{"parts":[{"text":"Sem pesquisa"}]},"finishReason":"STOP"}]}), &mut |_| Ok(())).unwrap();
    assert!(!plain
        .finish("gemini")
        .unwrap()
        .output
        .iter()
        .any(|item| item["type"] == "web_search_call"));
}
fn credential() -> CodexCredential {
    let mut c = CodexCredential::new("test", "test", 0, "google:1", None, None);
    c.project_id = Some("project".into());
    c
}

#[test]
fn streaming_preserves_text_thought_signatures_parallel_calls_and_usage() {
    let mut state = Output::default();
    let mut visible = String::new();
    let mut delta = |d| {
        if let Delta::Text(t) = d {
            visible.push_str(&t);
        }
        Ok(())
    };
    state.event(&json!({"response":{"candidates":[{"content":{"parts":[{"text":"Resumo","thought":true,"thoughtSignature":"private-signature"},{"text":"Olá "}]}}]}}),&mut delta).unwrap();
    state.event(&json!({"response":{"candidates":[{"content":{"parts":[{"text":"mundo"},{"functionCall":{"name":"read_file","args":{"path":"a"}}},{"functionCall":{"name":"read_file","args":{"path":"b"}}}]},"finishReason":"STOP"}],"usageMetadata":{"promptTokenCount":100,"cachedContentTokenCount":60,"candidatesTokenCount":10,"thoughtsTokenCount":5}}}),&mut delta).unwrap();
    let response = state.finish("gemini-3.7-flash").unwrap();
    assert_eq!(visible, "Olá mundo");
    assert_eq!(response.summary, "Resumo");
    let usage = response.usage.unwrap();
    assert_eq!((usage.input_tokens, usage.output_tokens), (100, 15));
    assert_eq!(usage.cache_read_tokens, Some(60));
    assert_eq!(usage.cache_write_tokens, None);
    let calls = tool_calls(&response.output).unwrap();
    assert_eq!(calls.len(), 2);
    assert_ne!(calls[0].id, calls[1].id);
    let mut replay = vec![json!({"role":"user","content":"Read"})];
    replay.extend(response.output);
    for call in calls {
        replay.push(json!({"type":"function_call_output","call_id":call.id,"output":"ok"}));
    }
    let contents = contents(&replay, "gemini-3.7-flash").unwrap();
    assert_eq!(contents.len(), 3);
    assert_eq!(
        contents[1]["parts"][0]["thoughtSignature"],
        "private-signature"
    );
    assert_eq!(contents[2]["parts"].as_array().unwrap().len(), 2);
    let codex = super::super::request_body(
        &options("gpt-5.6-luna"),
        "System",
        replay,
        vec![],
        "session",
    );
    let serialized = codex.to_string();
    assert!(!serialized.contains("private-signature"));
    assert!(!serialized.contains("_antigravity"));
}

#[test]
fn incomplete_or_rejected_streams_never_execute_tools() {
    let mut state = Output::default();
    state.event(&json!({"response":{"candidates":[{"content":{"parts":[{"functionCall":{"name":"write_file","args":{}}}]}}]}}),&mut |_|Ok(())).unwrap();
    assert!(state.finish("claude").is_err());
    let mut state = Output::default();
    assert!(state
        .event(
            &json!({"response":{"candidates":[{"finishReason":"SAFETY"}]}}),
            &mut |_| Ok(())
        )
        .is_err());
    assert!(overflow(
        &json!({"error":{"message":"The input token count exceeds the maximum allowed"}})
    ));
}

#[test]
fn finish_reasons_preserve_diagnostics_and_do_not_accept_partial_tools() {
    for (reason, code) in [
        ("MAX_TOKENS", "provider_output_limit"),
        ("MALFORMED_FUNCTION_CALL", "provider_incomplete"),
        ("UNEXPECTED_TOOL_CALL", "provider_incomplete"),
        ("SAFETY", "provider_blocked"),
        ("RECITATION", "provider_blocked"),
    ] {
        let mut state = Output::default();
        let error = state.event(&json!({"candidates":[{"content":{"parts":[{"functionCall":{"name":"write","args":{"path":"a"}}}]},"finishReason":reason}]}), &mut |_| Ok(())).unwrap_err();
        assert_eq!(error.code, code);
        assert!(error.message.contains(reason));
        assert!(state.finish("gemini-3.8-flash").is_err());
    }
}

#[test]
fn envelope_converts_tools_and_uses_specific_thinking_transports() {
    let mut credential = credential();
    credential.antigravity_models.insert(
        "gemini-3.7-flash".into(),
        json!({"supportsThinking":true,"maxOutputTokens":65536}),
    );
    let body=request_body(&credential,"1234",&options("gemini-3.7-flash"),"System",&[json!({"role":"user","content":"Hi"})],&[json!({"name":"read","description":"Read","parameters":{"type":"object","additionalProperties":false,"properties":{"path":{"type":"string"}},"required":["path"]}})]).unwrap();
    assert_eq!(body["requestType"], "agent");
    assert_eq!(body["project"], "project");
    assert_eq!(
        body["request"]["generationConfig"]["thinkingConfig"]["thinkingLevel"],
        "LOW"
    );
    assert_eq!(
        body["request"]["toolConfig"]["functionCallingConfig"]["mode"],
        "VALIDATED"
    );
    assert!(
        body["request"]["tools"][0]["functionDeclarations"][0]["parameters"]
            .get("additionalProperties")
            .is_none()
    );
    assert!(body["request"]["sessionId"]
        .as_str()
        .unwrap()
        .parse::<i64>()
        .is_ok());
    credential.antigravity_models.insert(
        "claude-opus-thinking".into(),
        json!({"supportsThinking":true,"maxOutputTokens":100000}),
    );
    let config = generation(&credential, &options("claude-opus-thinking"));
    assert_eq!(config["maxOutputTokens"], 64000);
    assert_eq!(config["thinkingConfig"]["thinkingBudget"], 1024);
}

#[test]
fn logical_model_routes_effort_and_preserves_prior_execution_link() {
    let mut c = credential();
    c.antigravity_models.insert("gemini-3.7-flash".into(),json!({"supportsThinking":true,"_thinking_mode":"level","_routes":{"low":"gemini-3.7-flash-low","high":"gemini-3.7-flash-high"},"_wire_metadata":{"gemini-3.7-flash-high":{"maxOutputTokens":65536}}}));
    let mut options = options("gemini-3.7-flash");
    let input = vec![
        json!({"role":"user","content":"Hello"}),
        json!({"type":"message","role":"assistant","content":[{"text":"Hi"}],"_antigravity_model":"gemini-3.7-flash","_antigravity_execution":"execution-before","_antigravity_step":7}),
    ];
    let low = request_body(&c, "session", &options, "System", &input, &[]).unwrap();
    assert_eq!(low["model"], "gemini-3.7-flash-low");
    options.reasoning = None;
    let high = request_body(&c, "session", &options, "System", &input, &[]).unwrap();
    assert_eq!(high["model"], "gemini-3.7-flash-high");
    assert_eq!(
        high["request"]["generationConfig"]["thinkingConfig"]["thinkingLevel"],
        "HIGH"
    );
    assert_eq!(
        high["request"]["labels"]["last_execution_id"],
        "execution-before"
    );
    assert_eq!(high["request"]["labels"]["last_step_index"], "7");
    assert_eq!(low["request"]["sessionId"], high["request"]["sessionId"]);
}

#[test]
fn schema_resolves_refs_without_losing_property_names_or_nullable_values() {
    let input = json!({"type":"object","$defs":{"Path":{"type":"string"}},"properties":{"type":{"$ref":"#/$defs/Path"},"choice":{"anyOf":[{"type":"integer"},{"type":"null"}]}}});
    let result = schema(&input, &input, 0);
    assert_eq!(result["properties"]["type"]["type"], "string");
    assert_eq!(result["properties"]["choice"]["nullable"], true);
}

#[test]
fn switching_models_discards_old_signatures_and_pairs_tool_results() {
    let input = vec![
        json!({"role":"user","content":"read"}),
        json!({"type":"function_call","call_id":"call-1","name":"read_file","arguments":"{}","_antigravity_model":"claude","_antigravity_part":{"functionCall":{"name":"read_file","args":{}},"thoughtSignature":"old-signature"}}),
        json!({"type":"function_call_output","call_id":"call-1","output":"data"}),
    ];
    let result = contents(&input, "gemini-3.7-flash").unwrap();
    assert!(!serde_json::to_string(&result)
        .unwrap()
        .contains("old-signature"));
    assert_eq!(
        result[1]["parts"][0]["thoughtSignature"],
        "skip_thought_signature_validator"
    );
    assert_eq!(
        result[2]["parts"][0]["functionResponse"]["name"],
        "read_file"
    );
}

async fn http_response(status: &str, body: &str) -> reqwest::Response {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let response=format!("HTTP/1.1 {status}\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len());
    std::thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut buffer = [0; 2048];
        assert!(socket.read(&mut buffer).unwrap() > 0);
        socket.write_all(response.as_bytes()).unwrap();
    });
    reqwest::Client::new()
        .get(url)
        .timeout(Duration::from_secs(5))
        .send()
        .await
        .unwrap()
}

#[tokio::test]
async fn http_stream_accepts_final_frame_at_eof_and_retains_execution_id() {
    let body="data: {\"response\":{\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"Olá\"}]},\"finishReason\":\"STOP\"}],\"responseId\":\"execution-1\",\"usageMetadata\":{\"promptTokenCount\":80,\"candidatesTokenCount\":2}}}";
    let response = http_response("200 OK", body).await;
    let (_tx, rx) = watch::channel(false);
    let mut text = String::new();
    let result = receive(response, "gemini-3.7-flash", rx, &mut |d| {
        if let Delta::Text(delta) = d {
            text.push_str(&delta);
        }
        Ok(())
    })
    .await
    .unwrap();
    assert_eq!(result.text, "Olá");
    assert_eq!(text, "Olá");
    assert_eq!(result.output[0]["_antigravity_execution"], "execution-1");
}

#[tokio::test]
async fn http_errors_trigger_compaction_without_leaking_upstream_body() {
    let response = http_response(
        "400 Bad Request",
        "{\"error\":{\"message\":\"input token count exceeds maximum test-private-data\"}}",
    )
    .await;
    let (_tx, rx) = watch::channel(false);
    let error = receive(response, "gemini", rx, &mut |_| Ok(()))
        .await
        .err()
        .unwrap();
    assert_eq!(error.code, "context_overflow");
    assert!(!error.message.contains("test-private-data"));
    let response = http_response(
        "403 Forbidden",
        "{\"error\":{\"message\":\"test-private-data\"}}",
    )
    .await;
    let (_tx, rx) = watch::channel(false);
    let error = receive(response, "gemini", rx, &mut |_| Ok(()))
        .await
        .err()
        .unwrap();
    assert_eq!(error.code, "provider_auth");
    assert!(!error.message.contains("test-private-data"));
}

#[tokio::test]
async fn cancelled_stream_does_not_finish_or_emit_output() {
    let response=http_response("200 OK","data: {\"response\":{\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"No\"}]},\"finishReason\":\"STOP\"}]}}\n\n").await;
    let (_tx, rx) = watch::channel(true);
    let error = receive(response, "gemini", rx, &mut |_| Err(AgentError::internal()))
        .await
        .err()
        .unwrap();
    assert_eq!(error.code, "cancelled");
}
