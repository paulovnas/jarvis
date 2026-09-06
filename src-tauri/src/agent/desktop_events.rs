//! Only root completion is announced; child snapshots may announce user requests.
use super::*;
use crate::system::{self, Notice};

pub(super) fn attention(app: &tauri::AppHandle, conversation: &str, snapshot: &ChatSnapshot) {
    if let Some(key) = attention_key(snapshot) {
        system::notify(app, conversation, &key, Notice::Question);
    }
}

fn attention_key(snapshot: &ChatSnapshot) -> Option<String> {
    let turn = snapshot.active_turn_id.as_ref()?;
    let tool = snapshot
        .pending_question
        .as_ref()
        .map(|q| &q.tool_id)
        .or_else(|| snapshot.pending_approval.as_ref().map(|a| &a.id))?;
    Some(format!("{}/{turn}/{tool}", snapshot.conversation_id))
}

fn terminal_notice(
    status: TurnStatus,
    active: bool,
    queued: bool,
    validation: bool,
) -> Option<Notice> {
    if active {
        return None;
    }
    match status {
        TurnStatus::Error => Some(Notice::Failed),
        TurnStatus::Completed if !queued => Some(if validation {
            Notice::Validation
        } else {
            Notice::Completed
        }),
        _ => None,
    }
}

pub(super) fn finished(app: &tauri::AppHandle, session: &Session, home: &std::path::Path) {
    let Some((id, status, active, queued)) = session.data.lock().ok().and_then(|data| {
        data.turns.last().map(|last| {
            (
                last.turn.id.clone(),
                last.turn.status.clone(),
                data.active.is_some(),
                !data.extras.queue.is_empty(),
            )
        })
    }) else {
        return;
    };
    let validation = workflow::awaiting_validation(home, &session.id, &id);
    if let Some(notice) = terminal_notice(status, active, queued, validation) {
        system::notify(app, &session.id, &id, notice);
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::{session, Fixture};
    use super::*;

    #[test]
    fn only_pending_user_requests_produce_attention_keys_for_root_or_child() {
        let fixture = Fixture::new();
        let session = session(&fixture);
        let mut snapshot = session.snapshot().unwrap();
        assert_eq!(attention_key(&snapshot), None);
        snapshot.active_turn_id = Some("turn".into());
        assert_eq!(attention_key(&snapshot), None);
        snapshot.pending_question = Some(questions::PendingQuestion {
            turn_id: "turn".into(),
            tool_id: "question".into(),
            questions: vec![],
        });
        assert_eq!(
            attention_key(&snapshot).as_deref(),
            Some("conversation/turn/question")
        );
        snapshot.conversation_id = "child".into();
        assert_eq!(
            attention_key(&snapshot).as_deref(),
            Some("child/turn/question")
        );
        snapshot.pending_question = None;
        assert_eq!(attention_key(&snapshot), None);
        snapshot.active_turn_id = None;
        assert_eq!(attention_key(&snapshot), None);
    }

    #[test]
    fn activity_covers_all_conversations_compaction_and_waiting_until_the_last_one_finishes() {
        let fixture_a = Fixture::new();
        let fixture_b = Fixture::new();
        let a = session(&fixture_a);
        let b = session(&fixture_b);
        let agent = AgentState::default();
        agent
            .sessions
            .lock()
            .unwrap()
            .extend([("a".into(), a.clone()), ("b".into(), b.clone())]);
        assert!(!agent.has_active_chats());
        let options = TurnOptions {
            account: "a".into(),
            model: "m".into(),
            reasoning: None,
            mode: Mode::Build,
            workflow: None,
            approval_mode: ApprovalMode::Yolo,
        };
        a.submit_message("hello".into(), options, vec![]).unwrap();
        b.data.lock().unwrap().manual_compaction = true;
        assert!(agent.has_active_chats());
        finish(&a, Ok(()));
        assert!(agent.has_active_chats());
        b.data.lock().unwrap().manual_compaction = false;
        assert!(!agent.has_active_chats());
    }

    #[test]
    fn only_idle_root_completion_or_failure_notifies() {
        assert_eq!(
            terminal_notice(TurnStatus::Completed, false, false, false),
            Some(Notice::Completed)
        );
        assert_eq!(
            terminal_notice(TurnStatus::Completed, false, false, true),
            Some(Notice::Validation)
        );
        assert_eq!(
            terminal_notice(TurnStatus::Completed, false, true, false),
            None
        );
        assert_eq!(
            terminal_notice(TurnStatus::Completed, true, false, false),
            None
        );
        assert_eq!(
            terminal_notice(TurnStatus::Error, false, true, false),
            Some(Notice::Failed)
        );
        for status in [
            TurnStatus::Running,
            TurnStatus::Cancelled,
            TurnStatus::Interrupted,
        ] {
            assert_eq!(terminal_notice(status, false, false, false), None);
        }
    }
}
