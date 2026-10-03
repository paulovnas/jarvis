use super::*;
fn options(model: &str) -> TurnOptions {
    TurnOptions {
        executor: crate::claude::Executor::Jarvis,
        account: "antigravity-test".into(),
        model: model.into(),
        reasoning: Some("low".into()),
        mode: crate::agent::Mode::Build,
        workflow: None,
        custom_workflow_id: None,
        custom_agent_id: None,
        approval_mode: crate::agent::ApprovalMode::Manual,
        manual_validation: false,
        automatic_publication: None,
        model_selection: None,
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
fn capabilities(credential: &CodexCredential, options: &TurnOptions) -> ModelCapabilities {
    ModelCapabilities::resolve_for_options(credential, options)
}

#[test]
fn replay_preserves_signed_calls_and_applies_cca_bypass_only_to_each_groups_first_unsigned_call() {
    let model = "gemini-3.8-flash";
    let call = |id: &str, signature: Option<&str>| {
        let mut part = json!({"functionCall":{"name":"read_file","args":{"path":id}}});
        if let Some(signature) = signature {
            part["thoughtSignature"] = json!(signature);
        }
        json!({"type":"function_call","call_id":id,"name":"read_file","arguments":json!({"path":id}).to_string(),"_antigravity_model":model,"_antigravity_part":part})
    };
    let bypass = "skip_thought_signature_validator";
    let signed = call("first", Some("c2lnbmVk"));
    let input = vec![
        json!({"role":"user","content":"Read the files"}),
        signed.clone(),
        call("second", Some(bypass)),
        json!({"type":"function_call_output","call_id":"first","output":"a"}),
        json!({"type":"function_call_output","call_id":"second","output":"b"}),
        call("third", Some("")),
        call("fourth", Some("c2lnbmVkLTI=")),
        call("fifth", Some(bypass)),
    ];
    let replay = contents(&input, model).unwrap();
    assert_eq!(replay[1]["parts"][0], signed["_antigravity_part"]);
    assert!(replay[1]["parts"][1].get("thoughtSignature").is_none());
    assert_eq!(replay[2]["parts"].as_array().unwrap().len(), 2);
    assert_eq!(replay[3]["parts"][0]["thoughtSignature"], bypass);
    assert_eq!(replay[3]["parts"][1]["thoughtSignature"], "c2lnbmVkLTI=");
    assert!(replay[3]["parts"][2].get("thoughtSignature").is_none());

    let switched = contents(&input, "gemini-3.7-flash").unwrap();
    assert_eq!(switched[1]["parts"][0]["thoughtSignature"], bypass);
    assert!(switched[1]["parts"][1].get("thoughtSignature").is_none());
    assert_eq!(switched[3]["parts"][0]["thoughtSignature"], bypass);
    assert!(switched[3]["parts"][1].get("thoughtSignature").is_none());
    assert!(switched[3]["parts"][2].get("thoughtSignature").is_none());
    assert_eq!(input[2]["_antigravity_part"]["thoughtSignature"], bypass);
}

#[test]
fn unsigned_thought_replay_is_readable_for_gemini_and_omitted_for_claude() {
    for model in ["gemini-3.8-flash", "claude-sonnet-4-6"] {
        let signed =
            json!({"thought":true,"text":"Signed reasoning","thoughtSignature":"c2lnbmVk"});
        let input = vec![
            json!({"role":"user","content":"Continue"}),
            json!({"type":"reasoning","summary":[{"text":"Prior plan"}],"_antigravity_model":model,"_antigravity_part":{"thought":true,"text":"Prior plan"}}),
            json!({"type":"reasoning","summary":[{"text":"Signed reasoning"}],"_antigravity_model":model,"_antigravity_part":signed}),
        ];
        let replay = contents(&input, model).unwrap();
        let parts = replay[1]["parts"].as_array().unwrap();
        assert_eq!(parts.last().unwrap(), &input[2]["_antigravity_part"]);
        if model.starts_with("gemini") {
            assert_eq!(parts.len(), 2);
            assert!(parts[0]["text"].as_str().unwrap().contains("Prior plan"));
            assert!(parts[0]["text"]
                .as_str()
                .unwrap()
                .contains("reference only"));
            assert!(parts[0].get("thought").is_none());
            assert!(parts[0].get("thoughtSignature").is_none());
        } else {
            assert_eq!(parts.len(), 1);
        }
    }
}

#[test]
fn schema_keeps_removed_constraints_visible_without_sending_unsupported_keywords() {
    let input = json!({"type":"object","additionalProperties":false,"description":"Tool arguments","properties":{"path":{"type":"string","description":"Relative path","minLength":2,"maxLength":120,"pattern":"^[a-z]+$"},"count":{"type":"integer","minimum":1,"maximum":5},"values":{"type":"array","items":{"type":"string"},"minItems":1,"uniqueItems":true}},"allOf":[{"required":["path"]}]});
    let lowered = schema(&input, &input, 0);
    assert!(lowered.get("additionalProperties").is_none());
    assert!(lowered.get("allOf").is_none());
    let description = lowered["description"].as_str().unwrap();
    assert!(description.starts_with("Tool arguments\n"));
    assert!(description.contains("additionalProperties: false"));
    assert!(description.contains("allOf: [{\"required\":[\"path\"]}]"));
    let path = &lowered["properties"]["path"];
    for key in ["minLength", "maxLength", "pattern"] {
        assert!(path.get(key).is_none());
        assert!(path["description"].as_str().unwrap().contains(key));
    }
    assert!(path["description"]
        .as_str()
        .unwrap()
        .starts_with("Relative path\n"));
    assert!(lowered["properties"]["count"]["description"]
        .as_str()
        .unwrap()
        .contains("minimum: 1"));
    assert!(lowered["properties"]["values"]["description"]
        .as_str()
        .unwrap()
        .contains("uniqueItems: true"));
    assert_eq!(input["properties"]["path"]["minLength"], 2);
}

#[test]
fn gemini_38_uses_requested_thinking_level_when_transport_metadata_is_missing() {
    for model in ["gemini-3.8-flash", "gemini-3.8-pro"] {
        let mut auth = credential();
        auth.antigravity_models
            .insert(model.into(), json!({"supportsThinking":true}));
        for effort in ["low", "medium", "high"] {
            let mut selected = options(model);
            selected.reasoning = Some(effort.into());
            let config = generation(&auth, &selected, &capabilities(&auth, &selected));
            assert_eq!(
                config["thinkingConfig"]["thinkingLevel"],
                effort.to_uppercase()
            );
            assert!(config["thinkingConfig"].get("thinkingBudget").is_none());
            assert_eq!(config["maxOutputTokens"], 65536);
        }
        auth.antigravity_models.remove(model);
        let mut selected = options(model);
        selected.reasoning = Some("high".into());
        let mut known_capabilities = capabilities(&auth, &selected);
        known_capabilities.reasoning.supported = true;
        let config = generation(&auth, &selected, &known_capabilities);
        assert_eq!(config["thinkingConfig"]["thinkingLevel"], "HIGH");
        auth.antigravity_models
            .insert(model.into(), json!({"supportsThinking":false}));
        assert!(generation(&auth, &selected, &known_capabilities)
            .get("thinkingConfig")
            .is_none());
    }
}

#[test]
fn streaming_preserves_text_thought_signatures_parallel_calls_and_usage() {
    let mut state = Output::default();
    let mut visible = String::new();
    let mut delta = |d| {
        assert!(
            !matches!(d, Delta::ToolReady(_)),
            "Antigravity keeps terminal validation before dispatch"
        );
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
    let codex_options = options("gpt-5.6-luna");
    let codex_credential = CodexCredential::new("", "", 0, "", None, None);
    let codex_capabilities = capabilities(&codex_credential, &codex_options);
    let codex = super::super::request_body(
        &codex_options,
        &codex_capabilities,
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
    let selected = options("gemini-3.7-flash");
    let selected_capabilities = capabilities(&credential, &selected);
    let body=request_body(&credential,"1234",&selected,&selected_capabilities,"System",&[json!({"role":"user","content":"Hi"})],&[json!({"name":"read","description":"Read","parameters":{"type":"object","additionalProperties":false,"properties":{"path":{"type":"string"}},"required":["path"]}})]).unwrap();
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
    let claude = options("claude-opus-thinking");
    let claude_capabilities = capabilities(&credential, &claude);
    let config = generation(&credential, &claude, &claude_capabilities);
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
    let low_capabilities = capabilities(&c, &options);
    let low = request_body(
        &c,
        "session",
        &options,
        &low_capabilities,
        "System",
        &input,
        &[],
    )
    .unwrap();
    assert_eq!(low["model"], "gemini-3.7-flash-low");
    options.reasoning = None;
    let high_capabilities = capabilities(&c, &options);
    let high = request_body(
        &c,
        "session",
        &options,
        &high_capabilities,
        "System",
        &input,
        &[],
    )
    .unwrap();
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
    assert!(high["request"].get("tools").is_none());
    assert!(high["request"].get("toolConfig").is_none());
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
    let result = receive(response, "gemini-3.7-flash", rx, None, &mut |d| {
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
    let error = receive(response, "gemini", rx, None, &mut |_| Ok(()))
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
    let error = receive(response, "gemini", rx, None, &mut |_| Ok(()))
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
    let error = receive(response, "gemini", rx, None, &mut |_| {
        Err(AgentError::internal())
    })
    .await
    .err()
    .unwrap();
    assert_eq!(error.code, "cancelled");
}

#[tokio::test]
async fn buffered_tool_call_can_arrive_after_two_minutes_of_reasoning_silence() {
    let super::super::tests::ControlledSseServer {
        request,
        connected,
        chunks,
        task: server,
    } = super::super::tests::controlled_sse_server_with_client(
        super::super::http_client_for(&credential()).unwrap(),
    );
    let (_cancel, signal) = watch::channel(false);
    let (deltas, mut observed) = tokio::sync::mpsc::unbounded_channel();
    let task = tokio::spawn(async move {
        receive(
            request.send().await.unwrap(),
            "gemini-3.8-flash",
            signal,
            None,
            &mut |delta| {
                assert!(!matches!(delta, Delta::ToolReady(_)));
                if let Delta::Summary(summary) = delta {
                    deltas.send(summary).unwrap();
                }
                Ok(())
            },
        )
        .await
    });
    connected.await.unwrap();
    chunks.send("HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\ndata: {\"response\":{\"candidates\":[{\"content\":{\"parts\":[{\"thought\":true,\"text\":\"Preparing the component\"}]}}]}}\n\n".into()).unwrap();
    assert_eq!(
        observed.recv().await.as_deref(),
        Some("Preparing the component")
    );
    tokio::time::pause();
    tokio::time::advance(Duration::from_secs(121)).await;
    chunks.send("data: {\"response\":{\"candidates\":[{\"content\":{\"parts\":[{\"functionCall\":{\"name\":\"write\",\"args\":{\"path\":\"component.tsx\",\"content\":\"complete component\"}}}]},\"finishReason\":\"STOP\"}],\"usageMetadata\":{\"promptTokenCount\":100,\"candidatesTokenCount\":5}}}\n\n".into()).unwrap();
    tokio::time::resume();
    let result = tokio::time::timeout(Duration::from_secs(2), task)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(result.calls.len(), 1);
    assert_eq!(result.calls[0].name, "write");
    assert_eq!(result.usage.unwrap().output_tokens, 5);
    assert!(
        !server.is_finished(),
        "STOP must not wait for the server to close SSE"
    );
    drop(chunks);
    server.await.unwrap();
}

#[tokio::test]
async fn stalled_flash_streams_distinguish_startup_from_buffered_output_and_preserve_request_id() {
    for (send_reasoning, keepalives) in [(false, false), (false, true), (true, false), (true, true)]
    {
        let super::super::tests::ControlledSseServer {
            request,
            connected,
            chunks,
            task: server,
        } = super::super::tests::controlled_sse_server_with_client(
            super::super::http_client_for(&credential()).unwrap(),
        );
        let (_cancel, signal) = watch::channel(false);
        let (ready, started) = tokio::sync::oneshot::channel();
        let (deltas, mut observed) = tokio::sync::mpsc::unbounded_channel();
        let task = tokio::spawn(async move {
            let response = request.send().await.unwrap();
            ready.send(()).unwrap();
            receive(response, "gemini-3.8-flash", signal, None, &mut |delta| {
                if let Delta::Summary(summary) = delta {
                    deltas.send(summary).unwrap();
                }
                Ok(())
            })
            .await
        });
        connected.await.unwrap();
        chunks.send("HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nx-request-id: request-test\r\nConnection: close\r\n\r\n".into()).unwrap();
        started.await.unwrap();
        if send_reasoning {
            chunks.send("data: {\"response\":{\"candidates\":[{\"content\":{\"parts\":[{\"thought\":true,\"text\":\"Thinking\"}]}}]}}\n\n".into()).unwrap();
            assert_eq!(observed.recv().await.as_deref(), Some("Thinking"));
        }
        let timeout = if send_reasoning {
            STREAM_IDLE_TIMEOUT
        } else {
            first_response_timeout("gemini-3.8-flash")
        };
        tokio::time::pause();
        for _ in 0..2 {
            tokio::time::advance(timeout / 3).await;
            if keepalives {
                chunks.send(": keepalive\n\ndata: {}\n\n".into()).unwrap();
                // Let the local socket deliver its keepalive before advancing time.
                for _ in 0..16 {
                    tokio::task::yield_now().await;
                }
            }
        }
        assert!(
            !task.is_finished(),
            "Keepalives must not change the deadline"
        );
        tokio::time::advance(timeout / 3 + Duration::from_secs(1)).await;
        let error = task.await.unwrap().unwrap_err();
        assert_eq!(error.code, "provider_timeout");
        assert!(error
            .message
            .contains(&format!("{} segundos", timeout.as_secs())));
        assert!(error.message.contains(if send_reasoning {
            "continuação"
        } else {
            "primeiro evento"
        }));
        let metadata = error.provider_metadata.unwrap();
        assert_eq!(metadata.http_status, Some(200));
        assert_eq!(metadata.request_id.as_deref(), Some("request-test"));
        drop(chunks);
        server.await.unwrap();
        tokio::time::resume();
    }
}

#[tokio::test]
async fn fragmented_tool_arguments_keep_working_beyond_five_minutes_without_early_execution() {
    let super::super::tests::ControlledSseServer {
        request,
        connected,
        chunks,
        task: server,
    } = super::super::tests::controlled_sse_server_with_client(
        super::super::http_client_for(&credential()).unwrap(),
    );
    let (_cancel, signal) = watch::channel(false);
    let (deltas, mut observed) = tokio::sync::mpsc::unbounded_channel();
    let task = tokio::spawn(async move {
        receive(
            request.send().await.unwrap(),
            "gemini-3.8-flash",
            signal,
            None,
            &mut |delta| {
                assert!(
                    !matches!(delta, Delta::ToolReady(_)),
                    "Incomplete arguments cannot execute"
                );
                if let Delta::Summary(summary) = delta {
                    deltas.send(summary).unwrap();
                }
                Ok(())
            },
        )
        .await
    });
    connected.await.unwrap();
    chunks.send("HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\ndata: {\"response\":{\"candidates\":[{\"content\":{\"parts\":[{\"thought\":true,\"text\":\"Preparing HTML\"}]}}]}}\n\n".into()).unwrap();
    assert_eq!(observed.recv().await.as_deref(), Some("Preparing HTML"));
    chunks.send("data: {\"response\":{\"candidates\":[{\"content\":{\"parts\":[{\"functionCall\":{\"name\":\"write\",\"args\":{\"path\":\"index.html\",\"content\":\"".into()).unwrap();
    tokio::time::sleep(Duration::from_millis(10)).await;
    for _ in 0..6 {
        tokio::time::pause();
        tokio::time::advance(Duration::from_secs(100)).await;
        chunks.send("html ".into()).unwrap();
        tokio::time::resume();
        tokio::time::sleep(Duration::from_millis(10)).await;
        assert!(
            !task.is_finished(),
            "Active argument transmission must not be restarted"
        );
        assert!(observed.try_recv().is_err());
    }
    chunks
        .send("\"}}}]},\"finishReason\":\"STOP\"}]}}\n\n".into())
        .unwrap();
    let result = tokio::time::timeout(Duration::from_secs(2), task)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(result.calls.len(), 1);
    assert_eq!(result.calls[0].args["content"], "html ".repeat(6));
    assert!(!server.is_finished());
    drop(chunks);
    server.await.unwrap();
}

#[tokio::test]
async fn comments_cannot_keep_incomplete_arguments_alive() {
    let super::super::tests::ControlledSseServer {
        request,
        connected,
        chunks,
        task: server,
    } = super::super::tests::controlled_sse_server_with_client(
        super::super::http_client_for(&credential()).unwrap(),
    );
    let (_cancel, signal) = watch::channel(false);
    let (deltas, mut observed) = tokio::sync::mpsc::unbounded_channel();
    let task = tokio::spawn(async move {
        receive(
            request.send().await.unwrap(),
            "gemini-3.8-flash",
            signal,
            None,
            &mut |delta| {
                if let Delta::Summary(summary) = delta {
                    deltas.send(summary).unwrap();
                }
                Ok(())
            },
        )
        .await
    });
    connected.await.unwrap();
    chunks.send("HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nx-request-id: incomplete-test\r\nConnection: close\r\n\r\ndata: {\"response\":{\"candidates\":[{\"content\":{\"parts\":[{\"thought\":true,\"text\":\"Thinking\"}]}}]}}\n\n".into()).unwrap();
    assert_eq!(observed.recv().await.as_deref(), Some("Thinking"));
    chunks.send("data: {\"response\":\n".into()).unwrap();
    tokio::time::sleep(Duration::from_millis(10)).await;
    for _ in 0..2 {
        tokio::time::pause();
        tokio::time::advance(Duration::from_secs(100)).await;
        chunks.send(": keepalive\n".into()).unwrap();
        tokio::time::resume();
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    tokio::time::pause();
    tokio::time::advance(Duration::from_secs(101)).await;
    let error = task.await.unwrap().unwrap_err();
    assert_eq!(error.code, "provider_timeout");
    assert!(error.message.contains("evento incompleto"));
    assert_eq!(
        error.provider_metadata.unwrap().request_id.as_deref(),
        Some("incomplete-test")
    );
    drop(chunks);
    server.await.unwrap();
    tokio::time::resume();
}

#[test]
fn fast_startup_is_scoped_to_gemini_flash_and_retries_use_only_known_endpoints() {
    for model in ["gemini-3.8-flash", "gemini-3.8-flash-thinking"] {
        assert_eq!(first_response_timeout(model), Duration::from_secs(60));
    }
    for model in ["gemini-3.8-pro", "claude-sonnet", "gemini"] {
        assert_eq!(first_response_timeout(model), STREAM_IDLE_TIMEOUT);
    }
    let mut auth = credential();
    for preferred in [
        Some(ENDPOINTS[0]),
        Some(ENDPOINTS[1]),
        None,
        Some("https://untrusted.invalid"),
    ] {
        auth.antigravity_endpoint = preferred.map(str::to_owned);
        let selected = retry_endpoint(&auth, 0);
        let alternate = retry_endpoint(&auth, 1);
        assert!(ENDPOINTS.contains(&selected.as_str()));
        assert!(ENDPOINTS.contains(&alternate.as_str()));
        assert_ne!(selected, alternate);
        assert_eq!(retry_endpoint(&auth, 2), selected);
    }
}

#[test]
fn empty_events_are_not_inference_progress() {
    let mut state = Output::default();
    for event in [
        json!({}),
        json!({"response":{}}),
        json!({"response":{"candidates":[{"content":{"parts":[{"text":""}]}}]}}),
    ] {
        assert!(!state.event(&event, &mut |_| Ok(())).unwrap());
    }
    assert!(state.event(&json!({"response":{"candidates":[{"content":{"parts":[{"thought":true,"text":"Thinking"}]}}]}}), &mut |_| Ok(())).unwrap());
}

#[tokio::test]
async fn cancellation_during_buffered_tool_generation_is_immediate() {
    let super::super::tests::ControlledSseServer {
        request,
        connected,
        chunks,
        task: server,
    } = super::super::tests::controlled_sse_server_with_client(
        super::super::http_client_for(&credential()).unwrap(),
    );
    let (cancel, signal) = watch::channel(false);
    let (deltas, mut observed) = tokio::sync::mpsc::unbounded_channel();
    let task = tokio::spawn(async move {
        receive(
            request.send().await.unwrap(),
            "gemini-3.8-flash",
            signal,
            None,
            &mut |delta| {
                if let Delta::Summary(summary) = delta {
                    deltas.send(summary).unwrap();
                }
                Ok(())
            },
        )
        .await
    });
    connected.await.unwrap();
    chunks.send("HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\ndata: {\"response\":{\"candidates\":[{\"content\":{\"parts\":[{\"thought\":true,\"text\":\"Thinking\"}]}}]}}\n\n".into()).unwrap();
    assert_eq!(observed.recv().await.as_deref(), Some("Thinking"));
    chunks.send("data: {\"response\":{\"candidates\":[{\"content\":{\"parts\":[{\"functionCall\":{\"name\":\"write\",\"args\":{\"content\":\"unfinished".into()).unwrap();
    tokio::time::pause();
    let started = tokio::time::Instant::now();
    cancel.send(true).unwrap();
    assert_eq!(task.await.unwrap().unwrap_err().code, "cancelled");
    assert_eq!(started.elapsed(), Duration::ZERO);
    drop(chunks);
    server.await.unwrap();
}

#[test]
fn completed_thought_only_stop_preserves_signatures_and_usage_without_transport_retry() {
    let model = "gemini-3.8-flash";
    let part =
        json!({"thought":true,"text":"The result is ready","thoughtSignature":"signed-thought"});
    let response = fixture_response(&[
        json!({"response":{"candidates":[{"content":{"parts":[part.clone()]},"finishReason":"STOP"}],
            "usageMetadata":{"promptTokenCount":100,"cachedContentTokenCount":80,"candidatesTokenCount":0,"thoughtsTokenCount":7}}}),
    ], model).unwrap();
    assert!(response.text.is_empty());
    assert!(response.tool_calls().is_empty());
    assert_eq!(response.summary, "The result is ready");
    assert_eq!(response.output[0]["_antigravity_part"], part);
    assert_eq!(response.output[0]["_antigravity_model"], model);
    let usage = response.usage.unwrap();
    assert_eq!(usage.input_tokens, 100);
    assert_eq!(usage.output_tokens, 7);
    assert_eq!(usage.cache_read_tokens, Some(80));

    let truncated = json!({"response":{"candidates":[{"content":{"parts":[part]}}]}});
    assert_eq!(
        fixture_response(&[truncated], model).unwrap_err().code,
        "provider_protocol"
    );
    for parts in [json!([]), json!([{"thought":true,"text":""}])] {
        let empty_stop =
            json!({"response":{"candidates":[{"content":{"parts":parts},"finishReason":"STOP"}]}});
        assert_eq!(
            fixture_response(&[empty_stop], model).unwrap_err().code,
            "provider_protocol"
        );
    }
}

#[tokio::test]
async fn thought_only_continuation_replays_signed_context_and_can_deliver_the_direct_answer() {
    let fixture = crate::agent::tests::Fixture::new();
    let session = crate::agent::tests::session(&fixture);
    let selected = options("gemini-3.8-flash");
    session
        .reserve("Explain the saved result".into(), selected.clone())
        .unwrap();
    let thought = fixture_response(&[json!({"response":{"candidates":[{
        "content":{"parts":[{"thought":true,"text":"I can explain it","thoughtSignature":"signed-thought"}]},
        "finishReason":"STOP",
    }]}})], &selected.model).unwrap();
    let call = json!({"type":"function_call","call_id":"confirmed","name":"write","arguments":"{\"path\":\"result.txt\",\"content\":\"Done\"}"});
    let receipt = json!({"type":"function_call_output","call_id":"confirmed","output":"Saved"});
    session
        .update_async(|data| {
            data.turns
                .last_mut()
                .unwrap()
                .wire
                .extend([call.clone(), receipt.clone()]);
            data.turns
                .last_mut()
                .unwrap()
                .wire
                .extend(thought.output.clone());
        })
        .await
        .unwrap();
    session.flush_async().await.unwrap();
    let mut reminded = false;
    crate::agent::remind_antigravity_final_output(&session, &mut reminded)
        .await
        .unwrap();
    let input = session
        .data
        .lock()
        .unwrap()
        .turns
        .last()
        .unwrap()
        .wire
        .clone();
    assert_eq!(
        input
            .iter()
            .filter(|item| item["call_id"] == "confirmed")
            .count(),
        2
    );
    assert_eq!(input.last().unwrap()["role"], "user");
    assert_eq!(input.last().unwrap()["_jarvis_runtime"], true);
    let body = fixture_request(&credential(), &selected, &input, &[]).unwrap();
    let contents = body["request"]["contents"].as_array().unwrap();
    assert!(contents
        .iter()
        .flat_map(|item| item["parts"].as_array().unwrap())
        .any(|part| part["thoughtSignature"] == "signed-thought"));
    assert_eq!(contents.last().unwrap()["role"], "user");
    assert!(contents.last().unwrap()["parts"][0]["text"]
        .as_str()
        .unwrap()
        .contains("Deliver the requested final result"));
    let final_response = fixture_response(
        &[json!({"response":{"candidates":[{
            "content":{"parts":[{"text":"O resultado foi salvo."}]},"finishReason":"STOP",
        }]}})],
        &selected.model,
    )
    .unwrap();
    assert_eq!(final_response.text, "O resultado foi salvo.");
    assert!(final_response.tool_calls().is_empty());
}
