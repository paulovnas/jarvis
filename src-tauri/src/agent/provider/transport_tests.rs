use super::*;
use crate::agent::{context_manager::StepContext, tests, ApprovalMode};
use futures_util::{SinkExt, StreamExt};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};
use tokio_tungstenite::{accept_async, tungstenite::Message};

fn credential(port: u16) -> (CodexCredential, ProviderModel) {
    let mut credential = CodexCredential::new("fixture", "", i64::MAX, "account", None, None);
    credential.custom = Some(serde_json::from_value(json!({
        "baseUrl":format!("http://127.0.0.1:{port}"),"protocol":"openai-responses","authMode":"bearer","tokenField":"max_tokens",
        "models":[{"id":"model","name":"Model","contextWindow":64000,"maxOutputTokens":4000,"supportsImages":false,"supportsTools":true,"reasoning":"none","reasoningLevels":[],"defaultReasoningLevel":null,"thinkingBudget":null}]
    })).unwrap());
    let model = ProviderModel {
        supports_fast: false,
        id: "model".into(),
        name: "Model".into(),
        reasoning_levels: vec![],
        default_reasoning_level: None,
        context_window: Some(64000),
        multi_agent_reasoning_effort: None,
    };
    (credential, model)
}

fn options(model: &ProviderModel) -> TurnOptions {
    let mut options = tests::options(ApprovalMode::Yolo);
    options.model = model.id.clone();
    options.reasoning = None;
    options
}

fn turn(
    session: &super::super::Session,
    credential: CodexCredential,
    model: &ProviderModel,
) -> TurnSession {
    TurnSession::new(
        credential,
        model,
        session.id.clone(),
        super::super::telemetry::TraceContext::new(&session.id, "transport-test"),
        &session.transports,
    )
    .unwrap()
}

fn step(session: &super::super::Session, turn: &TurnSession, options: &TurnOptions) -> StepContext {
    StepContext::capture(session, options, "Be concise", &[], turn.capabilities()).unwrap()
}

fn done(id: &str) -> Value {
    json!({"type":"response.completed","response":{"id":id,"status":"completed","output":[{"type":"message","role":"assistant","content":[{"type":"output_text","text":"OK"}]}]}})
}

async fn response(turn: &TurnSession, step: &StepContext) -> Response {
    let (_cancel, signal) = watch::channel(false);
    tokio::time::timeout(
        Duration::from_secs(3),
        turn.stream(step, signal, |_| Ok(())),
    )
    .await
    .unwrap()
    .unwrap()
}

fn commit_response(session: &super::super::Session, response: Response) {
    session
        .update(true, |data| {
            data.turns.last_mut().unwrap().wire.extend(response.output)
        })
        .unwrap();
    super::super::finish(session, Ok(()));
}

#[tokio::test]
async fn separate_turns_reuse_socket_and_send_only_the_new_user_input() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = tokio::spawn(async move {
        let mut socket = accept_async(listener.accept().await.unwrap().0)
            .await
            .unwrap();
        let mut requests = Vec::new();
        for id in ["response-1", "response-2"] {
            let request: Value =
                serde_json::from_str(&socket.next().await.unwrap().unwrap().into_text().unwrap())
                    .unwrap();
            requests.push(request);
            socket
                .send(Message::Text(done(id).to_string().into()))
                .await
                .unwrap();
        }
        requests
    });
    let fixture = tests::Fixture::new();
    let session = tests::session(&fixture);
    let (credential, model) = credential(port);
    let options = options(&model);
    session
        .reserve("First instruction".into(), options.clone())
        .unwrap();
    session.update(true, |data| data.turns.last_mut().unwrap().wire.extend([
        json!({"type":"function_call","call_id":"committed-write","name":"write","arguments":"{}"}),
        json!({"type":"function_call_output","call_id":"committed-write","output":"already completed"}),
    ])).unwrap();
    let first = turn(&session, credential.clone(), &model);
    let first_response = response(&first, &step(&session, &first, &options)).await;
    drop(first);
    commit_response(&session, first_response);
    session
        .reserve("Continue from the completed result".into(), options.clone())
        .unwrap();
    let second = turn(&session, credential, &model);
    let second_response = response(&second, &step(&session, &second, &options)).await;
    drop(second);
    commit_response(&session, second_response);
    let requests = server.await.unwrap();
    assert!(requests[0].get("previous_response_id").is_none());
    assert_eq!(requests[1]["previous_response_id"], "response-1");
    assert_eq!(requests[1]["input"].as_array().unwrap().len(), 1);
    assert_eq!(
        requests[1]["input"][0]["content"],
        "Continue from the completed result"
    );
    assert_eq!(requests[1]["store"], false);
    assert_eq!(
        requests[0]["input"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|item| item["type"] == "function_call_output"
                && item["call_id"] == "committed-write")
            .count(),
        1
    );
}

#[tokio::test]
async fn changed_identity_model_contract_or_history_opens_a_fresh_full_request() {
    for change in [
        "credential",
        "account",
        "backend",
        "contract",
        "model",
        "history",
        "conversation",
    ] {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = tokio::spawn(async move {
            let mut sockets = Vec::new();
            let mut requests = Vec::new();
            for id in ["first", "second"] {
                let mut socket = accept_async(listener.accept().await.unwrap().0)
                    .await
                    .unwrap();
                requests.push(
                    serde_json::from_str::<Value>(
                        &socket.next().await.unwrap().unwrap().into_text().unwrap(),
                    )
                    .unwrap(),
                );
                socket
                    .send(Message::Text(done(id).to_string().into()))
                    .await
                    .unwrap();
                // Keep the previous peer open so this proves invalidation rather than EOF recovery.
                sockets.push(socket);
            }
            requests
        });
        let fixture = tests::Fixture::new();
        let session = tests::session(&fixture);
        let (mut credential, mut model) = credential(port);
        let first_options = options(&model);
        session
            .reserve("First".into(), first_options.clone())
            .unwrap();
        let first = turn(&session, credential.clone(), &model);
        let first_response = response(&first, &step(&session, &first, &first_options)).await;
        drop(first);
        commit_response(&session, first_response);
        match change {
            "credential" => credential.access = "rotated-fixture".into(),
            "account" => credential.account_id = "other-account".into(),
            "backend" => credential.custom.as_mut().unwrap().base_url.push_str("/v1"),
            "contract" => credential.custom.as_mut().unwrap().replay_unsigned_thinking = true,
            "model" => {
                model.id = "other-model".into();
                credential.custom.as_mut().unwrap().models[0].id = model.id.clone();
            }
            "history" => session
                .update(true, |data| {
                    data.turns[0].wire[0]["content"] = json!("Corrected history")
                })
                .unwrap(),
            "conversation" => {}
            _ => unreachable!(),
        }
        let second_options = options(&model);
        session
            .reserve("Current intent".into(), second_options.clone())
            .unwrap();
        let second = if change == "conversation" {
            TurnSession::new(
                credential,
                &model,
                "other-conversation".into(),
                super::super::telemetry::TraceContext::new("other", "second"),
                &session.transports,
            )
            .unwrap()
        } else {
            turn(&session, credential, &model)
        };
        let second_response = response(&second, &step(&session, &second, &second_options)).await;
        drop(second);
        commit_response(&session, second_response);
        let requests = server.await.unwrap();
        assert!(
            requests[1].get("previous_response_id").is_none(),
            "{change}"
        );
        assert_eq!(
            requests[1]["input"].as_array().unwrap().len(),
            3,
            "{change}"
        );
        assert_eq!(
            requests[1]["input"][2]["content"], "Current intent",
            "{change}"
        );
        assert_eq!(requests[1]["store"], false, "{change}");
    }
}

#[tokio::test]
async fn cancellation_failure_and_dropped_stream_do_not_return_a_socket_to_the_pool() {
    for interruption in ["cancelled", "failure", "dropped"] {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let (accepted, ready) = tokio::sync::oneshot::channel();
        let server = tokio::spawn(async move {
            let mut first = accept_async(listener.accept().await.unwrap().0)
                .await
                .unwrap();
            let _ = first.next().await.unwrap().unwrap();
            first
                .send(Message::Text(
                    json!({"type":"response.created","response":{"id":"incomplete"}})
                        .to_string()
                        .into(),
                ))
                .await
                .unwrap();
            if interruption == "failure" {
                first.send(Message::Text(json!({"type":"response.failed","response":{"error":{"code":"invalid_request_error","message":"fixture"}}}).to_string().into())).await.unwrap();
            }
            accepted.send(()).unwrap();
            let mut second = accept_async(listener.accept().await.unwrap().0)
                .await
                .unwrap();
            let request: Value =
                serde_json::from_str(&second.next().await.unwrap().unwrap().into_text().unwrap())
                    .unwrap();
            second
                .send(Message::Text(done("recovered").to_string().into()))
                .await
                .unwrap();
            request
        });
        let fixture = tests::Fixture::new();
        let session = tests::session(&fixture);
        let (credential, model) = credential(port);
        let options = options(&model);
        session.reserve("First".into(), options.clone()).unwrap();
        let first = turn(&session, credential.clone(), &model);
        let first_step = step(&session, &first, &options);
        let (cancel, signal) = watch::channel(false);
        let result = {
            let inference = first.stream(&first_step, signal, |_| Ok(()));
            tokio::pin!(inference);
            if interruption == "dropped" {
                assert!(
                    tokio::time::timeout(Duration::from_millis(100), &mut inference)
                        .await
                        .is_err()
                );
                ready.await.unwrap();
                Err(AgentError::cancelled())
            } else {
                tokio::time::timeout(Duration::from_secs(3), async {
                    tokio::join!(&mut inference, async {
                        ready.await.unwrap();
                        if interruption == "cancelled" {
                            cancel.send(true).unwrap();
                        }
                    })
                    .0
                })
                .await
                .unwrap()
            }
        };
        assert!(result.is_err());
        drop(first_step);
        drop(first);
        assert!(
            session.transports.0.lock().unwrap().is_empty(),
            "{interruption}"
        );
        super::super::finish(&session, result.map(|_| ()));
        session
            .reserve("Recover the current intent".into(), options.clone())
            .unwrap();
        let second = turn(&session, credential, &model);
        let second_response = response(&second, &step(&session, &second, &options)).await;
        drop(second);
        commit_response(&session, second_response);
        let request = server.await.unwrap();
        assert!(
            request.get("previous_response_id").is_none(),
            "{interruption}"
        );
        assert_eq!(
            request["input"].as_array().unwrap().last().unwrap()["content"],
            "Recover the current intent"
        );
    }
}

#[test]
fn pool_bounds_idle_entries_bytes_and_age_and_never_resurrects_a_disposed_lease() {
    let pool = TransportPool::default();
    let (credential, model) = credential(1);
    for index in 0..MAX_IDLE_TRANSPORTS + 4 {
        let turn = TurnSession::new(
            credential.clone(),
            &model,
            index.to_string(),
            super::super::telemetry::TraceContext::new("fixture", "turn"),
            &pool,
        )
        .unwrap();
        turn.completed.store(true, Ordering::Relaxed);
        drop(turn);
    }
    let mut slots = pool.0.lock().unwrap();
    assert_eq!(slots.len(), MAX_IDLE_TRANSPORTS);
    assert!(!slots.contains_key("0"));
    slots
        .get_mut("4")
        .unwrap()
        .cached
        .as_mut()
        .unwrap()
        .idle_since = Instant::now() - TRANSPORT_IDLE_TTL;
    slots.get_mut("5").unwrap().cached.as_mut().unwrap().bytes = MAX_IDLE_TRANSPORT_BYTES + 1;
    TransportPool::prune(&mut slots);
    assert!(!slots.contains_key("4"));
    assert!(!slots.contains_key("5"));
    drop(slots);
    let older = TurnSession::new(
        credential.clone(),
        &model,
        "disposed".into(),
        super::super::telemetry::TraceContext::new("fixture", "turn"),
        &pool,
    )
    .unwrap();
    older.completed.store(true, Ordering::Relaxed);
    pool.invalidate("disposed");
    let newer = TurnSession::new(
        credential,
        &model,
        "disposed".into(),
        super::super::telemetry::TraceContext::new("fixture", "turn"),
        &pool,
    )
    .unwrap();
    drop(older);
    assert!(pool.0.lock().unwrap()["disposed"].cached.is_none());
    pool.invalidate("disposed");
    newer.completed.store(true, Ordering::Relaxed);
    drop(newer);
    assert!(!pool.0.lock().unwrap().contains_key("disposed"));
}

#[test]
fn turn_failure_or_failed_journal_checkpoint_invalidates_completed_cached_transport() {
    for storage_failure in [false, true] {
        let fixture = tests::Fixture::new();
        let session = tests::session(&fixture);
        let (credential, model) = credential(1);
        session.reserve("First".into(), options(&model)).unwrap();
        let turn = turn(&session, credential, &model);
        turn.completed.store(true, Ordering::Relaxed);
        drop(turn);
        assert!(session.transports.0.lock().unwrap()[&session.id]
            .cached
            .is_some());
        let result = if storage_failure {
            session.data.lock().unwrap().storage_failed = true;
            Ok(())
        } else {
            Err(AgentError::cancelled())
        };
        super::super::finish(&session, result);
        assert!(session.transports.0.lock().unwrap().is_empty());
    }
}

#[test]
fn deleting_an_unloaded_conversation_discards_its_transport_without_restoring_a_late_lease() {
    use crate::{agent::AgentState, library, persistence::AppState};
    let fixture = tests::Fixture::new();
    let state = AppState::default();
    let agent = AgentState::default();
    let project = library::new_id().unwrap();
    let id = library::new_id().unwrap();
    state.with_connection(&fixture.root, |connection| {
        connection.execute("INSERT INTO workspaces (id, name) VALUES ('workspace', 'Test')", [])?;
        connection.execute("INSERT INTO projects (id, workspace_id, name, path) VALUES (?1, 'workspace', 'Project', ?2)", rusqlite::params![project, fixture.root.to_string_lossy()])?;
        connection.execute("INSERT INTO conversations (id, project_id, title) VALUES (?1, ?2, 'Test')", rusqlite::params![id, project])?;
        Ok::<_, library::LibraryError>(())
    }).unwrap();
    let (credential, model) = credential(1);
    let old = TurnSession::new(
        credential,
        &model,
        id.clone(),
        super::super::telemetry::TraceContext::new("fixture", "turn"),
        &agent.transports,
    )
    .unwrap();
    old.completed.store(true, Ordering::Relaxed);
    agent
        .delete_library_item(
            &state,
            &fixture.root,
            &library::deletion::DeleteTarget::Conversation(id),
        )
        .unwrap();
    drop(old);
    assert!(agent.transports.0.lock().unwrap().is_empty());
}

async fn http_request(socket: &mut TcpStream) -> Value {
    let mut bytes = Vec::new();
    let mut buffer = [0; 2048];
    loop {
        let read = tokio::time::timeout(Duration::from_secs(3), socket.read(&mut buffer))
            .await
            .unwrap()
            .unwrap();
        assert!(read > 0);
        bytes.extend_from_slice(&buffer[..read]);
        let Some(boundary) = bytes.windows(4).position(|value| value == b"\r\n\r\n") else {
            continue;
        };
        let headers = String::from_utf8(bytes[..boundary].to_vec()).unwrap();
        let size: usize = headers
            .lines()
            .find_map(|line| {
                line.to_lowercase()
                    .strip_prefix("content-length:")
                    .map(str::trim)
                    .map(str::to_owned)
            })
            .unwrap()
            .parse()
            .unwrap();
        if bytes.len() >= boundary + 4 + size {
            return serde_json::from_slice(&bytes[boundary + 4..boundary + 4 + size]).unwrap();
        }
    }
}

#[tokio::test]
async fn websocket_fallback_reuses_http_across_turns_with_authoritative_full_input() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = tokio::spawn(async move {
        let (mut upgrade, _) = listener.accept().await.unwrap();
        let mut bytes = [0; 4096];
        assert!(upgrade.read(&mut bytes).await.unwrap() > 0);
        upgrade
            .write_all(
                b"HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            )
            .await
            .unwrap();
        drop(upgrade);
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut requests = Vec::new();
        for id in ["first-http", "second-http"] {
            requests.push(http_request(&mut socket).await);
            let sse = format!("data: {}\n\n", done(id));
            socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: keep-alive\r\n\r\n{sse}", sse.len()).as_bytes()).await.unwrap();
        }
        requests
    });
    let fixture = tests::Fixture::new();
    let session = tests::session(&fixture);
    let (credential, model) = credential(port);
    let options = options(&model);
    for user in ["First", "Next"] {
        session.reserve(user.into(), options.clone()).unwrap();
        let turn = turn(&session, credential.clone(), &model);
        let response = response(&turn, &step(&session, &turn, &options)).await;
        drop(turn);
        commit_response(&session, response);
    }
    let requests = server.await.unwrap();
    assert_eq!(requests[0]["input"].as_array().unwrap().len(), 1);
    assert_eq!(requests[1]["input"].as_array().unwrap().len(), 3);
    assert!(requests[1].get("previous_response_id").is_none());
    assert_eq!(requests[1]["store"], false);
}
