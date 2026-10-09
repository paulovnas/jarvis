use super::*;
use crate::agent::tests::{options, session, Fixture};

#[test]
fn learned_context_reaches_claude_initial_input_and_live_tool_responses_once() {
    let fixture = Fixture::new();
    let session = session(&fixture);
    session
        .reserve("Ajustar Select".into(), options(ApprovalMode::Yolo))
        .unwrap();
    session.update(true, |data| {
        data.turns[0].wire.push(json!({"role":"user","_jarvis_runtime":true,"_jarvis_learning":"lesson:1","content":"Learned: display the Select label."}));
    }).unwrap();
    let mut cursor = {
        let data = session.data.lock().unwrap();
        assert!(initial_input(&data, false)
            .unwrap()
            .contains("display the Select label"));
        data.turns[0].wire.len()
    };
    session.update(true, |data| {
        data.turns[0].wire.push(json!({"role":"user","_jarvis_runtime":true,"_jarvis_learning":"lesson:2","content":"Learned: preserve existing labels after loading."}));
    }).unwrap();
    assert_eq!(
        pending_input(&session, &mut cursor).unwrap(),
        vec!["Learned: preserve existing labels after loading."]
    );
    assert!(pending_input(&session, &mut cursor).unwrap().is_empty());
}

#[test]
fn secondary_claude_starts_fresh_with_active_turn_receipts_on_first_turn() {
    let fixture = Fixture::new();
    let session = session(&fixture);
    let mut choice = options(ApprovalMode::Yolo);
    choice.executor = crate::claude::Executor::Claude;
    session
        .reserve("Implemente sem publicar".into(), choice)
        .unwrap();
    let mut data = session.data.lock().unwrap();
    let turn = data.turns.last_mut().unwrap();
    turn.wire
        .push(json!({"_jarvis_claude_session":"primary-session"}));
    turn.turn.steps.push(Step {
        text: "Arquivo criado".into(),
        tools: vec![ToolCall {
            id: "confirmed-write".into(),
            name: "write".into(),
            args: json!({"path":"saved.txt"}),
            status: "completed".into(),
            output: "Conteúdo salvo".into(),
            duration_ms: 1,
        }],
        ..Step::default()
    });
    turn.wire
        .push(json!({"role":"user","content":"Preserve o nome atual"}));
    turn.wire
        .push(json!({"_jarvis_model_fallback":{"to":"secondary"}}));
    turn.turn.steps.push(Step {
        text: "Continuando com o modelo secundário".into(),
        context_id: Some("model-fallback".into()),
        ..Step::default()
    });
    assert!(session_reference(&data).is_none());
    let input = initial_input(&data, false).unwrap();
    for expected in [
        "sem publicar",
        "Preserve o nome atual",
        "confirmed-write",
        "Conteúdo salvo",
        "completed",
        "historyIsPartial",
        "Arquivo criado",
    ] {
        assert!(input.contains(expected), "missing {expected}");
    }
    data.turns
        .last_mut()
        .unwrap()
        .wire
        .push(json!({"_jarvis_claude_session":"secondary-session"}));
    assert_eq!(
        session_reference(&data).as_deref(),
        Some("secondary-session")
    );
    assert!(initial_input(&data, true)
        .unwrap()
        .contains("interrupted Jarvis turn"));
}

#[test]
fn manual_model_switch_starts_fresh_with_confirmed_receipts_and_current_images() {
    let fixture = Fixture::new();
    let mut session = Arc::try_unwrap(session(&fixture)).ok().unwrap();
    session.id = "a".repeat(32);
    let session = Arc::new(session);
    let mut choice = options(ApprovalMode::Yolo);
    choice.executor = crate::claude::Executor::Claude;
    session
        .reserve("Inspect the image; preserve the saved file".into(), choice)
        .unwrap();
    let mut image = std::io::Cursor::new(Vec::new());
    image::DynamicImage::new_rgb8(4, 4)
        .write_to(&mut image, image::ImageFormat::Png)
        .unwrap();
    let attachment =
        attachments::store(&fixture.root, &session.id, "screen.png", image.get_ref()).unwrap();
    let mut data = session.data.lock().unwrap();
    let turn = &mut data.turns[0];
    turn.turn
        .parts
        .push(skill_input::MessagePart::Attachment { attachment });
    turn.wire
        .push(json!({"_jarvis_claude_session":"old-native-session"}));
    turn.turn.steps.push(Step {
        tools: vec![ToolCall {
            id: "confirmed-write".into(),
            name: "write".into(),
            args: json!({"path":"saved.txt"}),
            status: "completed".into(),
            output: "Saved once".into(),
            duration_ms: 1,
        }],
        ..Step::default()
    });
    turn.wire.extend([
        json!({"role":"user","content":"Keep the current filename"}),
        json!({"_jarvis_retry_model":{"from":"old","to":"new"}}),
    ]);
    let original = serde_json::to_value(&data.turns).unwrap();
    assert!(session_reference(&data).is_none());
    let text = initial_input(&data, false).unwrap();
    for expected in [
        "preserve the saved file",
        "Keep the current filename",
        "confirmed-write",
        "Saved once",
        "completed",
        "historyIsPartial",
    ] {
        assert!(text.contains(expected), "missing {expected}");
    }
    let content =
        handoff::content(&fixture.root, &session.id, text, &data.turns[0].turn.parts).unwrap();
    assert_eq!(content[2]["type"], "image");
    assert_eq!(content[2]["source"]["media_type"], "image/png");
    assert_eq!(serde_json::to_value(&data.turns).unwrap(), original);
    data.turns[0]
        .wire
        .push(json!({"_jarvis_claude_session":"new-native-session"}));
    assert_eq!(
        session_reference(&data).as_deref(),
        Some("new-native-session")
    );
    assert!(initial_input(&data, true)
        .unwrap()
        .contains("interrupted Jarvis turn"));
}

#[test]
fn executor_switch_keeps_intent_and_receipts_without_replaying_large_payloads() {
    let fixture = Fixture::new();
    let session = session(&fixture);
    session
        .reserve(
            "Implementar cadastro; não publique.".into(),
            options(ApprovalMode::Yolo),
        )
        .unwrap();
    session.update(true, |data| {
        let turn = data.turns.last_mut().unwrap();
        turn.wire.push(json!({"type":"function_call","call_id":"large","name":"read","arguments":"{\"path\":\"large.txt\"}"}));
        turn.wire.push(json!({"type":"function_call_output","call_id":"large","output":"HUGE_TOOL_PAYLOAD".repeat(100_000)}));
        turn.wire.push(json!({"type":"function_call","call_id":"write","name":"write","arguments":"{\"path\":\"file.ts\",\"content\":\"saved\"}"}));
        turn.wire.push(json!({"type":"function_call_output","call_id":"write","output":"Arquivo salvo"}));
        turn.turn.steps.push(Step { text: "Cadastro implementado e validado.".into(), tools: vec![ToolCall { id:"write".into(),name:"write".into(),args:json!({}),status:"completed".into(),output:"Arquivo salvo".into(),duration_ms:1 }], ..Step::default() });
    }).unwrap();
    finish(&session, Ok(()));
    session
        .reserve(
            "Adicione o filtro por data".into(),
            options(ApprovalMode::Yolo),
        )
        .unwrap();
    session
        .update(true, |data| {
            data.turns
                .last_mut()
                .unwrap()
                .wire
                .push(json!({"role":"user","content":"Use UTC, não horário local"}));
        })
        .unwrap();
    finish(&session, Ok(()));
    let mut choice = options(ApprovalMode::Yolo);
    choice.executor = crate::claude::Executor::Claude;
    session
        .reserve("Continue, mantendo essas restrições".into(), choice)
        .unwrap();
    session.update(true, |data| { data.turns.last_mut().unwrap().wire.push(json!({"role":"user","_jarvis_runtime":true,"_jarvis_core_design":true,"content":"Prepared Impeccable references"})); }).unwrap();
    let data = session.data.lock().unwrap();
    let text = initial_input(&data, false).unwrap();
    assert!(text.len() < 12_000);
    assert!(!text.contains("HUGE_TOOL_PAYLOAD"));
    for expected in [
        "não publique",
        "Use UTC",
        "Continue, mantendo",
        "Prepared Impeccable references",
        "Arquivo salvo",
        "completed",
        "historyIsPartial",
    ] {
        assert!(text.contains(expected), "missing {expected}");
    }
}

#[test]
fn pruned_history_still_hands_off_its_persisted_summary() {
    let fixture = Fixture::new();
    let session = session(&fixture);
    session
        .reserve("Continue".into(), options(ApprovalMode::Yolo))
        .unwrap();
    let mut data = session.data.lock().unwrap();
    data.turn_base = 8;
    data.extras.context = Some(compaction::Checkpoint {
        summary: "Migration applied; never apply it again without inspection".into(),
        preserved_users: vec![json!({"role":"user","content":"Only work on the backend"})],
        tool_receipts: vec![json!({"tool":"bash","output":"Migration 14 already committed"})],
        ..compaction::Checkpoint::default()
    });
    let input = initial_input(&data, false).unwrap();
    assert!(input.contains("Migration applied"));
    assert!(input.contains("Only work on the backend"));
    assert!(input.contains("Migration 14 already committed"));
    assert!(input.contains("Continue"));
}

#[tokio::test]
async fn resumed_claude_session_keeps_new_messages_after_initial_input_snapshot() {
    let fixture = Fixture::new();
    let session = session(&fixture);
    let mut choice = options(ApprovalMode::Yolo);
    choice.executor = crate::claude::Executor::Claude;
    session.reserve("Continue".into(), choice).unwrap();
    let mut cursor = session
        .data
        .lock()
        .unwrap()
        .turns
        .last()
        .unwrap()
        .wire
        .len();
    record_native_session(
        &session,
        "native-session",
        &json!({"type":"system","subtype":"init","session_id":"native-session"}),
    )
    .await
    .unwrap();
    record_native_session(
        &session,
        "native-session",
        &json!({"type":"system","subtype":"init","session_id":"native-session"}),
    )
    .await
    .unwrap();
    session
        .update(true, |data| {
            data.turns
                .last_mut()
                .unwrap()
                .wire
                .push(json!({"role":"user","content":"New guidance during startup"}));
        })
        .unwrap();
    assert_eq!(
        pending_input(&session, &mut cursor).unwrap(),
        vec!["New guidance during startup"]
    );
    assert!(pending_input(&session, &mut cursor).unwrap().is_empty());
    let data = session.data.lock().unwrap();
    assert_eq!(session_reference(&data).as_deref(), Some("native-session"));
    assert_eq!(
        data.turns
            .last()
            .unwrap()
            .wire
            .iter()
            .filter(|item| item["_jarvis_claude_session"].is_string())
            .count(),
        1
    );
    assert!(initial_input(&data, true)
        .unwrap()
        .contains("interrupted Jarvis turn"));
}

#[tokio::test]
async fn duplicate_callbacks_keep_structured_errors_and_uncertain_actions_are_not_repeated() {
    let fixture = Fixture::new();
    let session = session(&fixture);
    session
        .reserve("Write".into(), options(ApprovalMode::Yolo))
        .unwrap();
    let tool = ToolCall {
        id: "call-id".into(),
        name: "write".into(),
        args: json!({}),
        status: "running".into(),
        output: String::new(),
        duration_ms: 0,
    };
    projection::start_tool(&session, &tool).await.unwrap();
    session
        .update(true, |data| {
            data.turns.last_mut().unwrap().wire.last_mut().unwrap()["_jarvis_claude_request"] =
                json!("request-id");
        })
        .unwrap();
    let uncertain = replay_request(&session, "request-id").unwrap().unwrap();
    assert_eq!(uncertain["isError"], true);
    assert!(uncertain.to_string().contains("uncertain"));
    assert!(replay_request(&session, "different-request")
        .unwrap()
        .is_none());
    let structured = json!({"ok":false,"error":{"code":"invalid_arguments","validationErrors":[{"path":"$.path"}]}}).to_string();
    core_runtime::checkpoint_tool(
        &session,
        &tool,
        "Invalid path",
        "error",
        1,
        Some(&structured),
    )
    .await
    .unwrap();
    let replayed = replay_request(&session, "request-id").unwrap().unwrap();
    assert_eq!(replayed["isError"], true);
    assert_eq!(replayed["content"][0]["text"], structured);
    session
        .update(true, |data| {
            data.turns
                .last_mut()
                .unwrap()
                .turn
                .steps
                .last_mut()
                .unwrap()
                .tools[0]
                .status = "completed".into();
        })
        .unwrap();
    assert_eq!(
        replay_request(&session, "request-id").unwrap().unwrap()["isError"],
        false
    );
}

#[test]
fn current_images_are_native_blocks_and_conversation_scope_is_enforced() {
    let home = tempfile::tempdir().unwrap();
    let conversation = "a".repeat(32);
    let mut image = std::io::Cursor::new(Vec::new());
    image::DynamicImage::new_rgb8(4, 4)
        .write_to(&mut image, image::ImageFormat::Png)
        .unwrap();
    let attachment =
        attachments::store(home.path(), &conversation, "screen.png", image.get_ref()).unwrap();
    let parts = vec![skill_input::MessagePart::Attachment { attachment }];
    let content =
        handoff::content(home.path(), &conversation, "Inspect this".into(), &parts).unwrap();
    assert_eq!(content[0]["text"], "Inspect this");
    assert_eq!(content[2]["type"], "image");
    assert_eq!(content[2]["source"]["media_type"], "image/png");
    assert!(content[2]["source"]["data"]
        .as_str()
        .unwrap()
        .starts_with("iVBOR"));
    assert!(handoff::content(home.path(), &"b".repeat(32), "Inspect".into(), &parts).is_err());
    assert!(
        handoff::content(home.path(), &conversation, "x".repeat(8 * 1024 * 1024), &[]).is_err()
    );
}
