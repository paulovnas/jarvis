use super::*;
use crate::agent::{authorize, finish, tests::{session, Fixture}, tools, ApprovalMode, Mode, Step, TurnOptions, TurnStatus};
use std::sync::Arc;

fn request() -> Value {
    json!({"questions":[
        {"id":"place","question":"Onde prefere ficar?","options":[{"label":"Praia","description":"Perto do mar"},{"label":"Montanha"}]},
        {"id":"night","question":"O que prefere fazer à noite?"}
    ]})
}
fn response() -> Response {
    serde_json::from_value(json!({"cancelled":false,"answers":[
        {"id":"place","value":"Praia","selectedLabel":"Praia"},
        {"id":"night","value":"Ficar em casa e jogar"}
    ]})).unwrap()
}
fn prepare(fixture: &Fixture) -> (Arc<Session>, ToolCall, watch::Receiver<bool>) {
    let session = session(fixture);
    let signal = session.reserve("Ajude a escolher".into(), TurnOptions {
        account:"account".into(), model:"model".into(), reasoning:None, mode:Mode::Build, approval_mode:ApprovalMode::Manual,
    }).unwrap();
    let tool = ToolCall { id:"ask-1".into(), name:"ask_user".into(), args:request(), status:"running".into(), output:String::new(), duration_ms:0 };
    session.update(true, |data| {
        let current = data.turns.last_mut().unwrap();
        current.turn.steps.push(Step { tools:vec![tool.clone()], ..Step::default() });
        current.wire.push(json!({"type":"function_call","call_id":tool.id,"name":tool.name,"arguments":tool.args.to_string()}));
    }).unwrap();
    (session, tool, signal)
}
async fn start(session: &Arc<Session>, tool: ToolCall, signal: watch::Receiver<bool>) -> tokio::task::JoinHandle<Result<String, AgentError>> {
    let running = session.clone();
    let task = tokio::spawn(async move { execute(&running, &tool, signal).await });
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        while session.snapshot().unwrap().pending_question.is_none() { tokio::task::yield_now().await; }
    }).await.unwrap();
    task
}

#[tokio::test]
async fn answers_wake_tool_only_after_durable_history_and_survive_restart() {
    let fixture = Fixture::new();
    let (session, tool, signal) = prepare(&fixture);
    let task = start(&session, tool, signal).await;
    assert!(!task.is_finished());
    let pending = session.snapshot().unwrap().pending_question.unwrap();
    let snapshot = answer(&session, &pending.turn_id, &pending.tool_id, response()).unwrap();
    assert!(snapshot.pending_question.is_none());
    assert!(snapshot.active_turn_id.is_some());
    let output = task.await.unwrap().unwrap();
    let parsed: Value = serde_json::from_str(&output).unwrap();
    assert_eq!(parsed["answers"][0]["selectedLabel"], "Praia");
    assert_eq!(parsed["answers"][1]["value"], "Ficar em casa e jogar");
    let (reloaded, _) = journal::load_all(&session.journal).unwrap();
    assert_eq!(reloaded[0].turn.status, TurnStatus::Interrupted);
    assert_eq!(reloaded[0].turn.steps[0].tools[0].output, output);
    assert_eq!(reloaded[0].turn.steps[0].tools[0].status, "completed");
    assert_eq!(reloaded[0].wire.iter().filter(|item| item["type"] == "function_call_output").count(), 1);
    assert_eq!(reloaded[0].wire.last().unwrap()["output"], output);
    assert_eq!(answer(&session, &pending.turn_id, &pending.tool_id, response()).unwrap_err().code, "stale_question");
}

#[tokio::test]
async fn rejects_wrong_turn_tool_conversation_and_invalid_answers_without_consuming_request() {
    let fixture = Fixture::new();
    let (session, tool, signal) = prepare(&fixture);
    let task = start(&session, tool, signal).await;
    let pending = session.snapshot().unwrap().pending_question.unwrap();
    assert!(answer(&session, "other-turn", &pending.tool_id, response()).is_err());
    assert!(answer(&session, &pending.turn_id, "other-tool", response()).is_err());
    let other = Fixture::new();
    let (other_session, _, _) = prepare(&other);
    assert!(answer(&other_session, &pending.turn_id, &pending.tool_id, response()).is_err());
    let invalid_answers = [
        json!({"cancelled":false,"answers":[]}),
        json!({"cancelled":true,"answers":[{"id":"place","value":"Praia"}]}),
        json!({"cancelled":false,"answers":[{"id":"place","value":"Praia"},{"id":"place","value":"Praia"}]}),
        json!({"cancelled":false,"answers":[{"id":"unknown","value":"Praia"},{"id":"night","value":"Jogar"}]}),
        json!({"cancelled":false,"answers":[{"id":"place","value":"Praia","selectedLabel":"Cidade"},{"id":"night","value":"Jogar"}]}),
        json!({"cancelled":false,"answers":[{"id":"place","value":"Cidade","selectedLabel":"Praia"},{"id":"night","value":"Jogar"}]}),
        json!({"cancelled":false,"answers":[{"id":"place","value":"Praia"},{"id":"night","value":"  "}]}),
    ];
    for value in invalid_answers {
        assert!(answer(&session, &pending.turn_id, &pending.tool_id, serde_json::from_value(value).unwrap()).is_err());
        assert!(session.snapshot().unwrap().pending_question.is_some());
        assert!(!task.is_finished());
    }
    answer(&session, &pending.turn_id, &pending.tool_id, response()).unwrap();
    task.await.unwrap().unwrap();
}

#[tokio::test]
async fn dismiss_cancels_only_questions_and_keeps_the_agent_turn_active() {
    let fixture = Fixture::new();
    let (session, tool, signal) = prepare(&fixture);
    let task = start(&session, tool, signal).await;
    let pending = session.snapshot().unwrap().pending_question.unwrap();
    answer(&session, &pending.turn_id, &pending.tool_id, Response { cancelled:true, answers:vec![] }).unwrap();
    assert_eq!(serde_json::from_str::<Value>(&task.await.unwrap().unwrap()).unwrap(), json!({"cancelled":true,"answers":[]}));
    assert!(session.snapshot().unwrap().active_turn_id.is_some());
    assert!(!*session.data.lock().unwrap().active.as_ref().unwrap().cancel.borrow());
}

#[tokio::test]
async fn stop_and_restart_never_invent_an_answer_or_leave_a_live_request() {
    let fixture = Fixture::new();
    let (session, tool, signal) = prepare(&fixture);
    let task = start(&session, tool, signal).await;
    let pending = session.snapshot().unwrap().pending_question.unwrap();
    let (reloaded, _) = journal::load_all(&session.journal).unwrap();
    assert_eq!(reloaded[0].turn.steps[0].tools[0].output, cancelled_output());
    session.data.lock().unwrap().active.as_ref().unwrap().cancel.send(true).unwrap();
    assert_eq!(answer(&session, &pending.turn_id, &pending.tool_id, response()).unwrap_err().code, "stale_question");
    let error = task.await.unwrap().unwrap_err();
    assert_eq!(error.code, "cancelled");
    finish(&session, Err(error));
    assert!(session.snapshot().unwrap().pending_question.is_none());
    assert!(session.snapshot().unwrap().active_turn_id.is_none());
    assert_eq!(session.snapshot().unwrap().turns[0].steps[0].tools[0].output, cancelled_output());
}

#[tokio::test]
async fn storage_failure_never_acknowledges_or_delivers_an_answer() {
    let fixture = Fixture::new();
    let (session, tool, signal) = prepare(&fixture);
    let task = start(&session, tool, signal).await;
    let pending = session.snapshot().unwrap().pending_question.unwrap();
    std::fs::remove_file(&session.journal).unwrap();
    assert_eq!(answer(&session, &pending.turn_id, &pending.tool_id, response()).unwrap_err().code, "session_storage");
    assert!(task.await.unwrap().is_err());
    assert!(session.data.lock().unwrap().turns[0].wire.iter().all(|item| item["type"] != "function_call_output"));
}

#[tokio::test]
async fn available_in_plan_build_manual_yolo_without_an_approval_prompt() {
    let fixture = Fixture::new();
    let (session, tool, signal) = prepare(&fixture);
    for mode in [Mode::Plan, Mode::Build] {
        assert!(tools::definitions(mode).iter().any(|definition| definition["name"] == "ask_user"));
        for approval_mode in [ApprovalMode::Manual, ApprovalMode::Yolo] {
            let options = TurnOptions { account:"account".into(), model:"model".into(), reasoning:None, mode, approval_mode };
            assert!(authorize(&session, &tool, &options, signal.clone()).await.unwrap());
            assert!(session.snapshot().unwrap().pending_approval.is_none());
        }
    }
}

#[test]
fn validates_bounded_question_schema_before_opening_ui() {
    assert!(parse_request(&request()).is_ok());
    for args in [json!({"questions":[]}), json!({"questions":[{"id":"x","question":" "}]}),
        json!({"questions":[{"id":"x","question":"One?"},{"id":"x","question":"Two?"}]}),
        json!({"questions":[{"id":"x","question":"Pick?","options":[{"label":"A"},{"label":" A "}]}]}),
        json!({"questions":[{"id":"x","question":"Pick?","options": [{"label":"A","description":"x".repeat(501)}]}]}),
        json!({"questions":[{"id":"x","question":"Pick?","options":"bad"}]})] {
        assert!(parse_request(&args).is_err());
    }
}
