use super::*;
use crate::agent::{ApprovalMode, Mode};
use crate::openai_codex::custom::Protocol;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread::JoinHandle;

fn options() -> TurnOptions {
    TurnOptions {
        account: "synthetic".into(),
        model: "test-model".into(),
        reasoning: None,
        mode: Mode::Build,
        workflow: None,
        custom_workflow_id: None,
        approval_mode: ApprovalMode::Yolo,
    }
}

fn credential(url: String, protocol: Protocol) -> CodexCredential {
    let mut credential = CodexCredential::new("synthetic-key", "", i64::MAX, "", None, None);
    credential.custom = Some(serde_json::from_value(json!({
        "baseUrl":url,"protocol":protocol,"authMode":"bearer","tokenField":"max_tokens",
        "models":[{"id":"test-model","name":"Test","contextWindow":128000,"maxOutputTokens":4096,
            "supportsImages":false,"supportsTools":true,"reasoning":"none","reasoningLevels":[],
            "defaultReasoningLevel":null,"thinkingBudget":null}]
    })).unwrap());
    credential
}

fn sse(events: &[Value]) -> String {
    events
        .iter()
        .map(|event| format!("data: {event}\n\n"))
        .collect()
}

fn complete(protocol: Protocol) -> String {
    match protocol {
        Protocol::OpenaiCompletions => {
            sse(&[
                json!({"choices":[{"index":0,"delta":{"content":"Concluído"},"finish_reason":null}]}),
                json!({"choices":[{"index":0,"delta":{},"finish_reason":"stop"}]}),
            ]) + "data: [DONE]\n\n"
        }
        Protocol::OpenaiResponses => sse(&[
            json!({"type":"response.output_text.delta","delta":"Concluído"}),
            json!({"type":"response.completed","response":{"status":"completed","output":[
                {"type":"message","role":"assistant","content":[{"type":"output_text","text":"Concluído"}]}
            ]}}),
        ]),
        Protocol::AnthropicMessages => sse(&[
            json!({"type":"message_start","message":{"usage":{"input_tokens":10}}}),
            json!({"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}),
            json!({"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Concluído"}}),
            json!({"type":"content_block_stop","index":0}),
            json!({"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":4}}),
            json!({"type":"message_stop"}),
        ]),
    }
}

// Each scripted response consumes one complete request. Bounded reads and accepts
// ensure a broken retry implementation fails the test instead of hanging it.
fn server(replies: Vec<(u16, String)>) -> (String, JoinHandle<Vec<Value>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let worker = std::thread::spawn(move || {
        let mut requests = vec![];
        for (status, body) in replies {
            let deadline = std::time::Instant::now() + Duration::from_secs(10);
            let mut stream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        assert!(
                            std::time::Instant::now() < deadline,
                            "Expected another inference request"
                        );
                        std::thread::sleep(Duration::from_millis(2));
                    }
                    Err(error) => panic!("{error}"),
                }
            };
            stream.set_nonblocking(false).unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut bytes = vec![];
            let (offset, length) = loop {
                let mut chunk = [0; 4096];
                let count = stream.read(&mut chunk).unwrap();
                assert!(count > 0);
                bytes.extend_from_slice(&chunk[..count]);
                if let Some(end) = bytes.windows(4).position(|part| part == b"\r\n\r\n") {
                    let headers = String::from_utf8_lossy(&bytes[..end]).to_lowercase();
                    let length = headers
                        .lines()
                        .find_map(|line| line.strip_prefix("content-length:"))
                        .unwrap()
                        .trim()
                        .parse::<usize>()
                        .unwrap();
                    break (end + 4, length);
                }
            };
            while bytes.len() < offset + length {
                let mut chunk = [0; 4096];
                let count = stream.read(&mut chunk).unwrap();
                assert!(count > 0);
                bytes.extend_from_slice(&chunk[..count]);
            }
            requests.push(serde_json::from_slice(&bytes[offset..offset + length]).unwrap());
            if status != 0 {
                write!(stream, "HTTP/1.1 {status} Test\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
            }
        }
        requests
    });
    (url, worker)
}

fn request<'a>(credential: &'a CodexCredential, options: &'a TurnOptions) -> Request<'a> {
    Request {
        credential,
        options,
        session_id: "session",
        instructions: "Synthetic test",
        input: vec![json!({"role":"user","content":"Continue"})],
        tools: vec![],
    }
}

#[tokio::test]
async fn retries_gateway_and_connection_failures_across_custom_protocols() {
    for protocol in [
        Protocol::OpenaiCompletions,
        Protocol::OpenaiResponses,
        Protocol::AnthropicMessages,
    ] {
        let (url, server) = server(vec![
            (502, "private upstream body".into()),
            (0, String::new()),
            (200, complete(protocol)),
        ]);
        let auth = credential(url, protocol);
        let options = options();
        let (_stop, signal) = watch::channel(false);
        let mut attempts = vec![];
        let result = request(&auth, &options)
            .run(
                signal,
                |delta| {
                    if let Delta::Retry(status) = delta {
                        if let Some(status) = &status {
                            assert!(!status.message.contains("private"));
                        }
                        attempts.push(status.map(|s| s.attempt));
                    }
                    Ok(())
                },
                Duration::ZERO,
            )
            .await
            .unwrap();
        assert_eq!(result.text, "Concluído");
        assert_eq!(attempts, vec![Some(1), Some(2), None]);
        let requests = server.join().unwrap();
        assert_eq!(requests.len(), 3);
        assert!(requests.windows(2).all(|pair| pair[0] == pair[1]));
    }
}

#[tokio::test]
async fn partial_output_does_not_reset_the_consecutive_failure_budget() {
    let partial = sse(&[
        json!({"choices":[{"index":0,"delta":{"content":"Texto parcial"},"finish_reason":null}]}),
    ]);
    let (url, server) = server(vec![(200, partial); 6]);
    let auth = credential(url, Protocol::OpenaiCompletions);
    let options = options();
    let (_stop, signal) = watch::channel(false);
    let mut attempts = vec![];
    let error = request(&auth, &options)
        .run(
            signal,
            |delta| {
                if let Delta::Retry(Some(status)) = delta {
                    attempts.push(status.attempt);
                }
                Ok(())
            },
            Duration::ZERO,
        )
        .await
        .err()
        .unwrap();
    assert_eq!(error.code, "provider_retry_exhausted");
    assert_eq!(attempts, vec![1, 2, 3, 4, 5]);
    assert_eq!(server.join().unwrap().len(), 6);
}

#[tokio::test]
async fn streamed_overload_retries_but_streamed_authentication_failure_does_not() {
    for (kind, expected) in [
        ("overloaded_error", None),
        ("authentication_error", Some("provider_auth")),
    ] {
        let protocol = Protocol::AnthropicMessages;
        let mut replies = vec![(
            200,
            sse(&[json!({"type":"error","error":{"type":kind,"message":"private details"}})]),
        )];
        if expected.is_none() {
            replies.push((200, complete(protocol)));
        }
        let (url, server) = server(replies);
        let auth = credential(url, protocol);
        let options = options();
        let (_stop, signal) = watch::channel(false);
        let result = request(&auth, &options)
            .run(signal, |_| Ok(()), Duration::ZERO)
            .await;
        if let Some(code) = expected {
            let error = result.err().unwrap();
            assert_eq!(error.code, code);
            assert!(!error.message.contains("private"));
            assert_eq!(server.join().unwrap().len(), 1);
        } else {
            assert_eq!(result.unwrap().text, "Concluído");
            assert_eq!(server.join().unwrap().len(), 2);
        }
    }
}

#[tokio::test]
async fn stops_after_five_reconnections_and_preserves_a_safe_terminal_error() {
    let (url, server) = server(vec![(503, "private body".into()); 6]);
    let auth = credential(url, Protocol::OpenaiCompletions);
    let options = options();
    let (_stop, signal) = watch::channel(false);
    let mut attempts = vec![];
    let error = request(&auth, &options)
        .run(
            signal,
            |delta| {
                if let Delta::Retry(Some(status)) = delta {
                    attempts.push(status.attempt);
                }
                Ok(())
            },
            Duration::ZERO,
        )
        .await
        .err()
        .unwrap();
    assert_eq!(error.code, "provider_retry_exhausted");
    assert!(error.message.contains("5 tentativas consecutivas"));
    assert!(!error.message.contains("private"));
    assert_eq!(attempts, vec![1, 2, 3, 4, 5]);
    assert_eq!(server.join().unwrap().len(), 6);
}

#[tokio::test]
async fn cancellation_interrupts_backoff_without_another_request() {
    let (url, server) = server(vec![(502, String::new())]);
    let auth = credential(url, Protocol::OpenaiCompletions);
    let options = options();
    let (stop, signal) = watch::channel(false);
    let result = tokio::time::timeout(
        Duration::from_secs(2),
        request(&auth, &options).run(
            signal,
            |delta| {
                if let Delta::Retry(Some(_)) = delta {
                    stop.send(true).unwrap();
                }
                Ok(())
            },
            Duration::from_secs(30),
        ),
    )
    .await
    .unwrap();
    assert_eq!(result.err().unwrap().code, "cancelled");
    assert_eq!(server.join().unwrap().len(), 1);
}

#[tokio::test]
async fn authentication_bad_requests_and_context_overflow_do_not_retry() {
    for (status, body, expected) in [
        (401, "private".into(), "provider_auth"),
        (400, "private".into(), "provider_request"),
        (
            400,
            json!({"error":{"code":"context_length_exceeded"}}).to_string(),
            "context_overflow",
        ),
    ] {
        let (url, server) = server(vec![(status, body)]);
        let auth = credential(url, Protocol::OpenaiCompletions);
        let options = options();
        let (_stop, signal) = watch::channel(false);
        let error = request(&auth, &options)
            .run(
                signal,
                |delta| {
                    assert!(!matches!(delta, Delta::Retry(_)));
                    Ok(())
                },
                Duration::ZERO,
            )
            .await
            .err()
            .unwrap();
        assert_eq!(error.code, expected);
        assert_eq!(server.join().unwrap().len(), 1);
    }
}

#[tokio::test]
async fn partial_stream_is_replaced_without_replaying_completed_tool_results() {
    let partial = sse(&[
        json!({"choices":[{"index":0,"delta":{"content":"Texto parcial"},"finish_reason":null}]}),
    ]);
    let protocol = Protocol::OpenaiCompletions;
    let (url, server) = server(vec![(200, partial), (200, complete(protocol))]);
    let auth = credential(url, protocol);
    let options = options();
    let (_stop, signal) = watch::channel(false);
    let mut request = request(&auth, &options);
    request.tools = vec![
        json!({"type":"function","name":"write","description":"Write","parameters":{"type":"object","properties":{}}}),
    ];
    request.input.extend([
        json!({"type":"function_call","call_id":"already-done","name":"write","arguments":"{}"}),
        json!({"type":"function_call_output","call_id":"already-done","output":"Arquivo salvo"}),
    ]);
    let mut visible = String::new();
    let mut resets = 0;
    let result = request
        .run(
            signal,
            |delta| {
                match delta {
                    Delta::Text(text) => visible.push_str(&text),
                    Delta::Reset => {
                        assert_eq!(visible, "Texto parcial");
                        visible.clear();
                        resets += 1;
                    }
                    _ => {}
                }
                Ok(())
            },
            Duration::ZERO,
        )
        .await
        .unwrap();
    assert_eq!(resets, 1);
    assert_eq!(visible, "Concluído");
    assert!(tool_calls(&result.output).unwrap().is_empty());
    let requests = server.join().unwrap();
    assert_eq!(requests[0], requests[1]);
    let tools: Vec<_> = requests[1]["messages"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|item| item["role"] == "tool")
        .collect();
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0]["content"], "Arquivo salvo");
}

#[tokio::test]
async fn storage_failure_in_stream_callback_is_not_retried() {
    let protocol = Protocol::OpenaiCompletions;
    let (url, server) = server(vec![(200, complete(protocol))]);
    let auth = credential(url, protocol);
    let options = options();
    let (_stop, signal) = watch::channel(false);
    let result = request(&auth, &options)
        .run(signal, |_| Err(AgentError::storage()), Duration::ZERO)
        .await;
    assert_eq!(result.err().unwrap().code, "session_storage");
    assert_eq!(server.join().unwrap().len(), 1);
}

#[tokio::test]
async fn a_successful_inference_resets_the_consecutive_failure_budget() {
    let protocol = Protocol::OpenaiCompletions;
    let mut replies = vec![(502, String::new()); 5];
    replies.push((200, complete(protocol)));
    replies.extend(replies.clone());
    let (url, server) = server(replies);
    let auth = credential(url, protocol);
    let options = options();
    let (_stop, signal) = watch::channel(false);
    for _ in 0..2 {
        let mut attempts = vec![];
        request(&auth, &options)
            .run(
                signal.clone(),
                |delta| {
                    if let Delta::Retry(Some(status)) = delta {
                        attempts.push(status.attempt);
                    }
                    Ok(())
                },
                Duration::ZERO,
            )
            .await
            .unwrap();
        assert_eq!(attempts, vec![1, 2, 3, 4, 5]);
    }
    assert_eq!(server.join().unwrap().len(), 12);
}

#[test]
fn backoff_is_exponential_and_respects_provider_retry_after() {
    let delays: Vec<_> = (1..=5)
        .map(|attempt| backoff(attempt, Duration::from_secs(2), None).as_secs())
        .collect();
    assert_eq!(delays, vec![2, 4, 8, 16, 30]);
    assert_eq!(
        backoff(2, Duration::from_secs(2), Some(Duration::from_secs(60))).as_secs(),
        60
    );
    for code in [
        "cancelled",
        "session_storage",
        "history_limit",
        "context_overflow",
        "provider_auth",
        "provider_request",
        "provider_model_unsupported",
    ] {
        assert!(!retryable(&AgentError::new(code, "test")));
    }
}
