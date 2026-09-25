use super::*;
use crate::agent::tests::{options, session, Fixture};

fn worker_history() -> (Fixture, Arc<Session>) {
    let fixture = Fixture::new();
    let session = session(&fixture);
    let options = options(ApprovalMode::Yolo);
    session
        .reserve(
            "Implement the dialog; preserve the API and do not publish".into(),
            options.clone(),
        )
        .unwrap();
    session.update(true, |data| {
        let turn = data.turns.last_mut().unwrap();
        let args = json!({"path":"src/dialog.tsx","content":"historical source\n".repeat(12_000)});
        turn.wire.extend([
            json!({"role":"user","_jarvis_runtime":true,"_jarvis_bead_checkpoint":"task","content":"Old specification snapshot"}),
            json!({"type":"reasoning","encrypted_content":"old-signature","summary":[]}),
            json!({"type":"function_call","call_id":"write-old","name":"write","arguments":args.to_string()}),
            json!({"type":"function_call_output","call_id":"write-old","output":"File updated"}),
            json!({"type":"function_call","call_id":"handoff","name":"hub_complete","arguments":"{\"verdict\":\"completed\",\"evidence\":[\"190 tests passed\"]}"}),
            json!({"type":"function_call_output","call_id":"handoff","output":"Registered"}),
        ]);
        turn.turn.steps.push(Step { tools: vec![
            ToolCall { id:"write-old".into(), name:"write".into(), args, status:"completed".into(), output:"File updated".into(), duration_ms:1 },
            ToolCall { id:"handoff".into(), name:"hub_complete".into(), args:json!({}), status:"completed".into(), output:"Registered".into(), duration_ms:1 },
        ], ..Step::default() });
    }).unwrap();
    crate::agent::finish(&session, Ok(()));
    session
        .reserve(
            "Fix the finding without redoing approved work".into(),
            options,
        )
        .unwrap();
    session.update(true, |data| {
        let turn = data.turns.last_mut().unwrap();
        turn.wire[0]["_jarvis_worker_dispatch"] = json!(true);
        turn.wire.extend([
            json!({"role":"user","_jarvis_runtime":true,"_jarvis_bead_checkpoint":"task","content":"Latest comment: handle unknown codes"}),
            json!({"type":"reasoning","encrypted_content":"current-signature","summary":[]}),
            json!({"type":"function_call","call_id":"current-read","name":"read","arguments":"{\"path\":\"src/dialog.tsx\"}"}),
            json!({"type":"function_call_output","call_id":"current-read","output":"Current file content"}),
        ]);
    }).unwrap();
    (fixture, session)
}

#[test]
fn reused_worker_replay_keeps_intent_latest_state_and_receipts_with_less_input() {
    let (_fixture, session) = worker_history();
    let data = session.data.lock().unwrap();
    let before = raw(&data);
    let replay = input(&data);
    let serialized = serde_json::to_string(&replay).unwrap();
    assert!(serialized.len() < serde_json::to_string(&before).unwrap().len() / 10);
    for expected in [
        "preserve the API",
        "do not publish",
        "without redoing",
        "src/dialog.tsx",
        "File updated",
        "190 tests passed",
        "Latest comment",
        "current-signature",
        "Current file content",
    ] {
        assert!(serialized.contains(expected), "lost {expected}");
    }
    for obsolete in [
        "historical source",
        "Old specification snapshot",
        "old-signature",
    ] {
        assert!(!serialized.contains(obsolete), "replayed {obsolete}");
    }
    let current = &data.turns.last().unwrap().wire;
    assert!(
        replay.ends_with(current),
        "current signed tool envelopes must remain byte-for-byte unchanged"
    );
    assert_eq!(
        raw(&data),
        before,
        "projection must not edit durable history"
    );
    assert!(info(&data).estimated);
    assert!(info(&data).tokens < before.iter().map(estimate).sum::<u64>() / 10);
}

#[test]
fn worker_followup_ignores_prior_usage_until_the_current_request_is_measured() {
    let (_fixture, session) = worker_history();
    let mut data = session.data.lock().unwrap();
    let previous_end = data.absolute_wire_end() - data.turns.last().unwrap().wire.len();
    data.extras.context = Some(Checkpoint {
        measured: Some(Measurement {
            tokens: 300_000,
            wire_end: previous_end,
        }),
        ..Checkpoint::default()
    });
    data.turns
        .last_mut()
        .unwrap()
        .turn
        .steps
        .push(Step::default());
    assert!(info(&data).estimated);
    assert!(info(&data).tokens < 10_000);
    let current_end = data.absolute_wire_end();
    data.extras.context.as_mut().unwrap().measured = Some(Measurement {
        tokens: 1_234,
        wire_end: current_end,
    });
    assert_eq!(info(&data).tokens, 1_234);
    assert!(!info(&data).estimated);
}

#[test]
fn correction_rounds_reduce_replayed_bytes_without_losing_findings_or_pairing() {
    let (_fixture, session) = worker_history();
    let mut data = session.data.lock().unwrap();
    let past = data.turns[0].clone();
    let finding: Value = serde_json::from_str(include_str!(
        "../../fixtures/evaluations/movarte-resumed-review.json"
    ))
    .unwrap();
    for round in 2..=4 {
        let mut corrected = past.clone();
        corrected.turn.id = format!("repair-{round}");
        corrected.wire[0]["content"] = json!(format!("Correction {round}: {}", finding["handoff"]));
        let at = data.turns.len() - 1;
        data.turns.insert(at, corrected);
        let original = raw(&data);
        let replay = input(&data);
        let before_bytes = serde_json::to_vec(&original).unwrap().len();
        let after_bytes = serde_json::to_vec(&replay).unwrap().len();
        let before_tokens: u64 = original.iter().map(estimate).sum();
        let after_tokens: u64 = replay.iter().map(estimate).sum();
        eprintln!("synthetic worker round {round}: bytes {before_bytes} -> {after_bytes}; estimated tokens {before_tokens} -> {after_tokens}");
        assert!(after_bytes < before_bytes / 10);
        assert!(after_tokens < before_tokens / 10);
        for message in original
            .iter()
            .filter(|item| item["role"] == "user" && item["_jarvis_runtime"] != true)
        {
            assert!(
                replay.contains(message),
                "lost original request or complete repair contract"
            );
        }
        let calls: Vec<_> = replay
            .iter()
            .filter(|item| item["type"] == "function_call")
            .map(|item| &item["call_id"])
            .collect();
        let results: Vec<_> = replay
            .iter()
            .filter(|item| item["type"] == "function_call_output")
            .map(|item| &item["call_id"])
            .collect();
        assert_eq!(calls, results);
        assert_eq!(raw(&data), original);
    }
}

#[test]
fn small_envelopes_are_not_expanded_and_structured_results_are_preserved() {
    let (_fixture, session) = worker_history();
    let mut turn = session.data.lock().unwrap().turns[0].clone();
    let structured = json!({"path":"src/dialog.tsx","revision":"confirmed-revision"});
    turn.wire
        .iter_mut()
        .find(|item| item["call_id"] == "write-old" && item["type"] == "function_call_output")
        .unwrap()["output"] = structured.clone();
    let projected = project(&turn, 0);
    assert!(projected.iter().any(|item| item["content"]
        .as_str()
        .is_some_and(|text| text.contains(&structured.to_string()))));
    turn.wire.retain(|item| {
        matches!(
            item["type"].as_str(),
            Some("function_call" | "function_call_output")
        ) && item["call_id"] == "write-old"
    });
    turn.wire[0]["arguments"] = json!("{\"path\":\"src/dialog.tsx\",\"content\":\"short\"}");
    assert_eq!(project(&turn, 0), turn.wire);
}

#[test]
fn uncertain_side_effects_and_missing_receipts_keep_the_original_envelope() {
    let (_fixture, session) = worker_history();
    let mut turn = session.data.lock().unwrap().turns[0].clone();
    turn.turn.steps[0].tools[0].status = "error".into();
    assert_eq!(project(&turn, 0), turn.wire);
    turn.turn.steps[0].tools[0].status = "completed".into();
    turn.wire
        .retain(|item| !(item["type"] == "function_call_output" && item["call_id"] == "write-old"));
    assert_eq!(project(&turn, 0), turn.wire);
}

#[test]
fn direct_chat_history_is_not_treated_as_a_worker_followup() {
    let (_fixture, session) = worker_history();
    let mut data = session.data.lock().unwrap();
    data.turns.last_mut().unwrap().wire[0]
        .as_object_mut()
        .unwrap()
        .remove("_jarvis_worker_dispatch");
    assert!(input(&data)
        .iter()
        .any(|item| item["call_id"] == "write-old"));
}

#[test]
#[ignore = "Read-only local diagnostic: set JARVIS_REPLAY_JOURNAL to an explicitly selected worker journal; no provider calls"]
fn measure_worker_replay_from_journal_snapshot() {
    let source = std::env::var_os("JARVIS_REPLAY_JOURNAL").expect("Select a worker journal");
    let (fixture, session) = worker_history();
    // Load a copy: journal recovery must never touch the user's live file.
    let snapshot = fixture.root.join("replay-snapshot.jsonl");
    std::fs::write(&snapshot, std::fs::read(source).unwrap()).unwrap();
    let (turns, extras) = journal::read_only(&snapshot).unwrap();
    let mut data = session.data.lock().unwrap();
    let mut next = data.turns.pop().unwrap();
    next.wire.truncate(1);
    data.turns = turns;
    data.extras = extras;
    data.turns.push(next);
    let mut baseline = data.extras.context.as_ref().map(prefix).unwrap_or_default();
    let through = data
        .extras
        .context
        .as_ref()
        .map_or(0, |context| data.local_wire_offset(context.through));
    baseline.extend(raw(&data).into_iter().skip(through));
    let replay = input(&data);
    eprintln!(
        "worker snapshot: {} settled turns; bytes {} -> {}; estimated tokens {} -> {}",
        data.turns.len() - 1,
        serde_json::to_vec(&baseline).unwrap().len(),
        serde_json::to_vec(&replay).unwrap().len(),
        baseline.iter().map(estimate).sum::<u64>(),
        replay.iter().map(estimate).sum::<u64>()
    );
}
