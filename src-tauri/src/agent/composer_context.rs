//! Public conversation text for auxiliary composer requests; never provider wire or reasoning.
use super::{AgentError, AgentState, ChatSnapshot, Turn, TurnStatus};
use crate::persistence::AppState;
use serde_json::{json, Value};
use std::path::Path;

pub(super) fn final_text(turn: &Turn) -> Option<String> {
    if turn.status != TurnStatus::Completed {
        return None;
    }
    turn.lsp_final_response()
        .or_else(|| turn.steps.last().map(|step| step.text.clone()))
        .filter(|text| !text.trim().is_empty())
}

pub(super) fn messages(turns: &[Turn]) -> Vec<Value> {
    let mut visible = Vec::new();
    for turn in turns {
        if !turn.user.trim().is_empty() {
            visible.push(json!({"role":"user", "content":turn.user}));
        }
        for message in &turn.auxiliary_messages {
            if !message.content.trim().is_empty() {
                visible.push(json!({"role":"user", "content":message.content}));
            }
        }
        if let Some(text) = final_text(turn) {
            visible.push(json!({"role":"assistant", "content":text}));
        }
    }
    let mut users = 0;
    let mut assistants = 0;
    let mut recent: Vec<_> = visible
        .into_iter()
        .rev()
        .filter(|message| {
            let count = if message["role"] == "user" {
                &mut users
            } else {
                &mut assistants
            };
            *count += 1;
            *count <= 6
        })
        .collect();
    recent.reverse();
    recent
}

pub(super) fn read(
    agent: &AgentState,
    state: &AppState,
    home: &Path,
    id: &str,
) -> Result<(ChatSnapshot, Vec<Value>), AgentError> {
    let snapshot = agent.read_chat(state, home, id)?;
    let mut turns = snapshot.turns.clone();
    let mut before = snapshot.history.start;
    loop {
        let recent = messages(&turns);
        if before == 0
            || (recent
                .iter()
                .filter(|message| message["role"] == "user")
                .count()
                == 6
                && recent
                    .iter()
                    .filter(|message| message["role"] == "assistant")
                    .count()
                    == 6)
        {
            return Ok((snapshot, recent));
        }
        let mut page = agent.history_page(state, home, id, Some(before), None, None)?;
        if page.history.start >= before {
            return Ok((snapshot, recent));
        }
        before = page.history.start;
        page.turns.append(&mut turns);
        turns = page.turns;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::Step;

    fn turn(index: usize) -> Turn {
        serde_json::from_value(json!({
            "id":format!("turn-{index}"), "createdAt":index, "durationMs":1,
            "user":format!("user-{index}"), "status":"completed", "error":null,
            "options":crate::agent::tests::options(crate::agent::ApprovalMode::Manual),
            "steps":[{"text":"internal commentary", "summary":"private reasoning", "tools":[], "usage":null},
                {"text":format!("final-{index}"), "summary":"more private reasoning", "tools":[], "usage":null}]
        })).unwrap()
    }

    #[test]
    fn context_keeps_only_six_users_and_six_completed_visible_final_responses() {
        let mut turns: Vec<_> = (0..9).map(turn).collect();
        turns[8].status = TurnStatus::Running;
        let context = messages(&turns);
        assert_eq!(
            context.iter().filter(|item| item["role"] == "user").count(),
            6
        );
        assert_eq!(
            context
                .iter()
                .filter(|item| item["role"] == "assistant")
                .count(),
            6
        );
        let serialized = serde_json::to_string(&context).unwrap();
        assert!(!serialized.contains("internal commentary"));
        assert!(!serialized.contains("private reasoning"));
        assert!(!serialized.contains("final-8"));
        assert!(!serialized.contains("final-1"));
        assert!(serialized.contains("final-2"));
        assert_eq!(context.last().unwrap()["content"], "user-8");
    }

    #[test]
    fn context_includes_real_mid_turn_user_messages_and_preserves_small_histories() {
        let mut turn = turn(0);
        turn.auxiliary_messages
            .push(crate::agent::queue::QueuedMessage {
                id: "steer".into(),
                content: "Preserve accessibility".into(),
                options: turn.options.clone(),
                parts: vec![],
                auxiliary_for: None,
                sent_at: Some(2),
                after_step: None,
            });
        assert_eq!(
            messages(&[turn]),
            vec![
                json!({"role":"user", "content":"user-0"}),
                json!({"role":"user", "content":"Preserve accessibility"}),
                json!({"role":"assistant", "content":"final-0"}),
            ]
        );
        assert!(messages(&[]).is_empty());
    }

    #[test]
    fn context_preserves_the_delivery_before_an_automatic_lsp_acknowledgement() {
        let mut turn = turn(0);
        turn.steps.remove(0);
        let activity = serde_json::from_value(json!({"component":"lsp", "action":"file_diagnostics",
            "status":"applied", "summary":"not context", "sources":[], "fingerprint":null, "durationMs":0})).unwrap();
        turn.steps.push(Step {
            text: "Diagnostics clean.".into(),
            core_activities: vec![activity],
            ..Default::default()
        });
        assert_eq!(
            final_text(&turn).as_deref(),
            Some("final-0\n\nDiagnostics clean.")
        );
    }
}
