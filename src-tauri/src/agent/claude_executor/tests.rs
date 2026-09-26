use super::*;
use crate::agent::tests::{options, session, Fixture};

fn reserve(fixture: &Fixture) -> (Arc<Session>, watch::Receiver<bool>) {
    let session = session(fixture);
    let mut selected = options(ApprovalMode::Yolo);
    selected.executor = crate::claude::Executor::Claude;
    selected.account.clear();
    selected.model = "default".into();
    let signal = session
        .reserve("Corrija o comportamento do chat.".into(), selected)
        .unwrap();
    (session, signal)
}

fn assistant(id: &str, uuid: &str, content: Value) -> Value {
    json!({"type":"assistant","uuid":uuid,"message":{"id":id,"content":content,"usage":{"input_tokens":20,"output_tokens":7,"cache_read_input_tokens":80,"cache_creation_input_tokens":5}}})
}

#[test]
fn native_user_acknowledgements_never_replace_the_visible_user_message() {
    let fixture = Fixture::new();
    let (session, _) = reserve(&fixture);
    let before = session.snapshot().unwrap();
    let mut projection = projection::Projection::default();
    for event in [
        json!({"type":"user","isReplay":true,"message":{"role":"user","content":"ACK"}}),
        json!({"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"tool-1","content":"done"}]}}),
    ] {
        projection.apply(&session, &event).unwrap();
    }
    let after = session.snapshot().unwrap();
    assert_eq!(after.turns[0].user, before.turns[0].user);
    assert_eq!(after.active_turn_id, before.active_turn_id);
    assert!(after.turns[0].steps.is_empty());
    assert_eq!(
        session.data.lock().unwrap().turns[0].wire[0]["content"],
        "Corrija o comportamento do chat."
    );
}

#[test]
fn streaming_and_split_assistant_envelopes_keep_one_message_and_all_tool_calls() {
    let fixture = Fixture::new();
    let (session, _) = reserve(&fixture);
    let mut projection = projection::Projection::default();
    for event in [
        json!({"type":"stream_event","event":{"type":"message_start","message":{"id":"message-1"}}}),
        json!({"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":"Verificando."}}}),
        json!({"type":"stream_event","event":{"type":"content_block_delta","index":1,"delta":{"type":"text_delta","text":"Vou conferir o arquivo."}}}),
        assistant(
            "message-1",
            "text-envelope",
            json!([{"type":"thinking","thinking":"Verificando."},{"type":"text","text":"Vou conferir o arquivo."}]),
        ),
        assistant(
            "message-1",
            "read-envelope",
            json!([{"type":"tool_use","id":"read-1","name":"mcp__jarvis__read","input":{"path":"src/App.tsx"}}]),
        ),
        assistant(
            "message-1",
            "tasks-envelope",
            json!([{"type":"tool_use","id":"tasks-1","name":"mcp__jarvis__update_tasks","input":{"tasks":[]}}]),
        ),
        assistant(
            "message-1",
            "tasks-envelope",
            json!([{"type":"tool_use","id":"tasks-1","name":"mcp__jarvis__update_tasks","input":{"tasks":[]}}]),
        ),
    ] {
        projection.apply(&session, &event).unwrap();
    }
    let snapshot = session.snapshot().unwrap();
    assert_eq!(snapshot.turns[0].steps.len(), 1);
    let step = &snapshot.turns[0].steps[0];
    assert_eq!(step.text, "Vou conferir o arquivo.");
    assert_eq!(step.summary, "Verificando.");
    assert_eq!(
        step.tools
            .iter()
            .map(|tool| tool.name.as_str())
            .collect::<Vec<_>>(),
        ["read", "update_tasks"]
    );
    assert_eq!(step.usage.as_ref().unwrap().input_tokens, 105);
    assert_eq!(
        session.data.lock().unwrap().turns[0]
            .wire
            .iter()
            .filter(|item| item["_jarvis_claude_message"] == "message-1")
            .count(),
        1
    );
}

#[test]
fn child_messages_and_results_do_not_pollute_or_complete_the_parent_chat() {
    let fixture = Fixture::new();
    let (session, _) = reserve(&fixture);
    let mut projection = projection::Projection::default();
    let mut child = assistant(
        "child-message",
        "child-envelope",
        json!([{"type":"text","text":"Internal child output"}]),
    );
    child["parent_tool_use_id"] = json!("child-1");
    projection.apply(&session, &child).unwrap();
    let child_result = json!({"type":"result","parent_tool_use_id":"child-1","subtype":"error_during_execution","is_error":true});
    projection.apply(&session, &child_result).unwrap();
    assert!(projection::final_result(&child_result).is_none());
    assert!(session.snapshot().unwrap().turns[0].steps.is_empty());
    assert!(session.snapshot().unwrap().active_turn_id.is_some());
}

#[test]
fn completed_blocks_restore_missing_stream_suffixes_without_hiding_later_text() {
    let fixture = Fixture::new();
    let (session, _) = reserve(&fixture);
    let mut projection = projection::Projection::default();
    for event in [
        json!({"type":"stream_event","event":{"type":"message_start","message":{"id":"message"}}}),
        json!({"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"text":"Vou"}}}),
        assistant(
            "message",
            "first-text",
            json!([{"type":"text","text":"Vou conferir os arquivos."}]),
        ),
        assistant(
            "message",
            "tool",
            json!([{"type":"tool_use","id":"read","name":"mcp__jarvis__read","input":{"path":"README.md"}}]),
        ),
        json!({"type":"stream_event","event":{"type":"content_block_delta","index":2,"delta":{"text":"Confira"}}}),
        assistant(
            "message",
            "second-text",
            json!([{"type":"text","text":"Confira o resultado completo."}]),
        ),
    ] {
        projection.apply(&session, &event).unwrap();
    }
    let snapshot = session.snapshot().unwrap();
    assert_eq!(
        snapshot.turns[0].steps[0].text,
        "Vou conferir os arquivos.\nConfira o resultado completo."
    );
    assert_eq!(snapshot.turns[0].steps[0].tools.len(), 1);
}

#[tokio::test]
async fn mapped_tool_calls_and_task_updates_are_visible_before_the_turn_finishes() {
    let fixture = Fixture::new();
    let (session, _) = reserve(&fixture);
    let args = json!({"tasks":[{"id":"fix","title":"Corrigir chat","status":"in_progress"},{"id":"verify","title":"Validar comportamento","status":"pending"}]});
    let mut projection = projection::Projection::default();
    projection.apply(&session, &assistant("message", "envelope", json!([{"type":"tool_use","id":"native-tasks","name":"mcp__jarvis__update_tasks","input":args}]))).unwrap();
    let tool = projection
        .take_tool("update_tasks", args.clone(), None)
        .unwrap();
    assert_eq!(tool.id, "native-tasks");
    projection::start_tool(&session, &tool).await.unwrap();
    let output = tasks::execute(&session, &args).unwrap();
    core_runtime::checkpoint_tool(&session, &tool, &output, "completed", 1, None)
        .await
        .unwrap();
    let snapshot = session.snapshot().unwrap();
    assert!(snapshot.active_turn_id.is_some());
    assert_eq!(snapshot.turns[0].tasks.len(), 2);
    assert_eq!(snapshot.turns[0].tasks[0].status, tasks::Status::InProgress);
    assert_eq!(snapshot.turns[0].steps[0].tools[0].status, "completed");
    let data = session.data.lock().unwrap();
    assert!(data.turns[0]
        .wire
        .iter()
        .any(|item| item["type"] == "function_call_output" && item["call_id"] == "native-tasks"));
}

async fn start_question(
    session: &Arc<Session>,
    signal: watch::Receiver<bool>,
) -> tokio::task::JoinHandle<Result<String, AgentError>> {
    let tool = ToolCall {
        id: "question".into(),
        name: "ask_user".into(),
        args: json!({"questions":[{"id":"target","question":"Qual destino?","options":[{"label":"Homologação"},{"label":"Produção"}]}]}),
        status: "pending".into(),
        output: String::new(),
        duration_ms: 0,
    };
    projection::start_tool(session, &tool).await.unwrap();
    let running = session.clone();
    let task = tokio::spawn(async move { questions::execute(&running, &tool, signal, 30).await });
    tokio::time::timeout(Duration::from_secs(2), async {
        while session.snapshot().unwrap().pending_question.is_none() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    task
}

#[tokio::test]
async fn native_questions_use_the_existing_drawer_and_persist_the_answer_once() {
    let fixture = Fixture::new();
    let (session, signal) = reserve(&fixture);
    let task = start_question(&session, signal).await;
    let pending = session.snapshot().unwrap().pending_question.unwrap();
    let response = serde_json::from_value(json!({"cancelled":false,"answers":[{"id":"target","value":"Homologação","selectedLabel":"Homologação"}]})).unwrap();
    questions::answer(&session, &pending.turn_id, &pending.tool_id, response).unwrap();
    let output: Value = serde_json::from_str(&task.await.unwrap().unwrap()).unwrap();
    assert_eq!(output["answers"][0]["value"], "Homologação");
    assert!(session.snapshot().unwrap().pending_question.is_none());
    let data = session.data.lock().unwrap();
    assert_eq!(
        data.turns[0]
            .wire
            .iter()
            .filter(
                |item| item["type"] == "function_call_output" && item["call_id"] == pending.tool_id
            )
            .count(),
        1
    );
}

#[tokio::test]
async fn cancelling_a_native_question_preserves_history_and_clears_the_drawer() {
    let fixture = Fixture::new();
    let (session, signal) = reserve(&fixture);
    let task = start_question(&session, signal).await;
    session
        .data
        .lock()
        .unwrap()
        .active
        .as_ref()
        .unwrap()
        .cancel
        .send(true)
        .unwrap();
    let error = tokio::time::timeout(Duration::from_secs(2), task)
        .await
        .unwrap()
        .unwrap()
        .unwrap_err();
    assert_eq!(error.code, "cancelled");
    finish(&session, Err(error));
    let snapshot = session.snapshot().unwrap();
    assert!(snapshot.pending_question.is_none());
    assert!(snapshot.active_turn_id.is_none());
    assert_eq!(snapshot.turns[0].user, "Corrija o comportamento do chat.");
}

#[tokio::test]
async fn cli_control_abort_is_retryable_interruption_unless_the_user_stopped() {
    for user_stopped in [false, true] {
        let fixture = Fixture::new();
        let (session, signal) = reserve(&fixture);
        if user_stopped {
            session
                .data
                .lock()
                .unwrap()
                .active
                .as_mut()
                .unwrap()
                .cancel();
        }
        let error = control_cancellation(&signal);
        assert!(!finish_run(&session, async { Err(error) }).await);
        let snapshot = session.snapshot().unwrap();
        assert_eq!(
            snapshot.turns[0].status,
            if user_stopped {
                TurnStatus::Cancelled
            } else {
                TurnStatus::Interrupted
            }
        );
        assert_eq!(*signal.borrow(), user_stopped);
        assert_eq!(snapshot.turns[0].user, "Corrija o comportamento do chat.");
        if !user_stopped {
            let id = &snapshot.turns[0].id;
            let (resumed, workflow) = session.retry_failed_turn(id).unwrap();
            assert!(!*resumed.borrow());
            assert!(workflow.is_none());
            let retried = session.snapshot().unwrap();
            assert_eq!(retried.active_turn_id.as_ref(), Some(id));
            assert_eq!(retried.turns[0].status, TurnStatus::Running);
            assert_eq!(retried.history.total, 1);
        }
    }
}

#[test]
fn auxiliary_delivery_uses_only_new_user_input_without_replaying_tool_results() {
    let fixture = Fixture::new();
    let (session, _) = reserve(&fixture);
    let mut cursor = session.data.lock().unwrap().turns[0].wire.len();
    session
        .update(true, |data| {
            let wire = &mut data.turns[0].wire;
            wire.push(
                json!({"type":"function_call_output","call_id":"done","output":"Already applied"}),
            );
            wire.push(json!({"role":"assistant","content":"Working"}));
            wire.push(json!({"role":"user","content":"Inclua também o teste."}));
        })
        .unwrap();
    assert_eq!(
        pending_input(&session, &mut cursor).unwrap(),
        ["Inclua também o teste."]
    );
    assert!(pending_input(&session, &mut cursor).unwrap().is_empty());
}

#[tokio::test]
async fn resumed_callbacks_reuse_receipts_but_new_tool_ids_with_the_same_args_can_execute() {
    let fixture = Fixture::new();
    let (session, _) = reserve(&fixture);
    let args = json!({"path":"state.txt","content":"saved"});
    let envelope = assistant(
        "write-message",
        "write-envelope",
        json!([{
            "type":"tool_use","id":"native-write","name":"mcp__jarvis__write","input":args
        }]),
    );
    let mut original = projection::Projection::default();
    original.apply(&session, &envelope).unwrap();
    let tool = original.take_tool("write", args.clone(), None).unwrap();
    assert!(replay_tool(&session, &tool.id).unwrap().is_none());
    projection::start_tool(&session, &tool).await.unwrap();
    assert!(replay_tool(&session, &tool.id)
        .unwrap()
        .unwrap()
        .to_string()
        .contains("uncertain"));
    core_runtime::checkpoint_tool(&session, &tool, "Written once", "completed", 1, None)
        .await
        .unwrap();
    let mut resumed = projection::Projection::default();
    resumed.apply(&session, &envelope).unwrap();
    let replay = resumed.take_tool("write", args.clone(), None).unwrap();
    assert_eq!(replay.id, tool.id);
    let output = replay_tool(&session, &replay.id).unwrap().unwrap();
    assert_eq!(output["content"][0]["text"], "Written once");
    assert_eq!(output["isError"], false);
    resumed.apply(&session, &assistant("new-write-message", "new-envelope", json!([{
        "type":"tool_use","id":"new-native-write","name":"mcp__jarvis__write","input":args
    }]))).unwrap();
    let new = resumed.take_tool("write", args.clone(), None).unwrap();
    assert_eq!(new.id, "new-native-write");
    assert!(replay_tool(&session, &new.id).unwrap().is_none());
    assert_eq!(
        session.data.lock().unwrap().turns[0]
            .wire
            .iter()
            .filter(|item| item["type"] == "function_call")
            .count(),
        1
    );
    let early = resumed
        .take_tool("write", args.clone(), Some("early-native-write"))
        .unwrap();
    projection::start_tool(&session, &early).await.unwrap();
    core_runtime::checkpoint_tool(&session, &early, "Early result", "completed", 1, None)
        .await
        .unwrap();
    resumed.apply(&session, &assistant("early-message", "early-envelope", json!([{
        "type":"tool_use","id":"early-native-write","name":"mcp__jarvis__write","input":args
    }]))).unwrap();
    let snapshot = session.snapshot().unwrap();
    let early_tools: Vec<_> = snapshot.turns[0]
        .steps
        .iter()
        .flat_map(|step| &step.tools)
        .filter(|tool| tool.id == early.id)
        .collect();
    assert_eq!(early_tools.len(), 1);
    assert_eq!(early_tools[0].status, "completed");
    assert!(resumed.take_tool("write", args, None).is_none());
}
