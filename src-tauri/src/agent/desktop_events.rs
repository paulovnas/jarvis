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
        .or_else(|| snapshot.pending_authoring.as_ref().map(|p| &p.tool_id))
        .or_else(|| snapshot.pending_approval.as_ref().map(|a| &a.id))?;
    Some(format!("{}/{turn}/{tool}", snapshot.conversation_id))
}

fn terminal_notice(
    status: TurnStatus,
    error_code: Option<&str>,
    active: bool,
    queued: bool,
    validation: bool,
) -> Option<Notice> {
    if active {
        return None;
    }
    match status {
        TurnStatus::Error => Some(Notice::Failed),
        TurnStatus::Interrupted if error_code == Some("progress_paused") => Some(Notice::Paused),
        TurnStatus::Completed if !queued => Some(if validation {
            Notice::Validation
        } else {
            Notice::Completed
        }),
        _ => None,
    }
}

pub(super) fn finished(app: &tauri::AppHandle, session: &Session, home: &std::path::Path) {
    let Some((id, status, error_code, active, queued)) =
        session.data.lock().ok().and_then(|data| {
            data.turns.last().map(|last| {
                (
                    last.turn.id.clone(),
                    last.turn.status.clone(),
                    last.turn.error.as_ref().map(|error| error.code.clone()),
                    data.active.is_some(),
                    !data.extras.queue.is_empty(),
                )
            })
        })
    else {
        return;
    };
    let validation = workflow::awaiting_validation(home, &session.id, &id);
    if let Some(notice) = terminal_notice(status, error_code.as_deref(), active, queued, validation)
    {
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
            deadline_at: 1,
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
        snapshot.pending_authoring = Some(authoring::PendingProposal {
            turn_id: "turn".into(),
            tool_id: "proposal".into(),
            action: authoring::Action::Create,
            summary: "Criar agente".into(),
            catalog_revision: Some(1),
            target: authoring::Target::Agent {
                before: None,
                after: workflow::catalog::tests::example().agents[0].clone(),
            },
            agent_references: vec![],
        });
        assert_eq!(
            attention_key(&snapshot).as_deref(),
            Some("child/turn/proposal")
        );
        snapshot.pending_authoring = None;
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
            custom_workflow_id: None,
            custom_agent_id: None,
            approval_mode: ApprovalMode::Yolo,
            manual_validation: false,
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
    fn reconnections_keep_the_turn_active_until_final_failure_or_recovery() {
        for succeeds in [false, true] {
            let fixture = Fixture::new();
            let session = session(&fixture);
            let options = TurnOptions {
                account: "synthetic".into(),
                model: "model".into(),
                reasoning: None,
                mode: Mode::Build,
                workflow: None,
                custom_workflow_id: None,
                custom_agent_id: None,
                approval_mode: ApprovalMode::Yolo,
                manual_validation: false,
            };
            let _signal = session.reserve("Continue".into(), options).unwrap();
            session
                .update(true, |data| {
                    data.turns
                        .last_mut()
                        .unwrap()
                        .turn
                        .steps
                        .push(Step::default());
                })
                .unwrap();
            for attempt in 1..=5 {
                session
                    .update(true, |data| {
                        data.turns
                            .last_mut()
                            .unwrap()
                            .turn
                            .steps
                            .last_mut()
                            .unwrap()
                            .retry = Some(provider::retry::Status {
                            attempt,
                            max_attempts: 5,
                            retry_at: now(),
                            message: "HTTP 502".into(),
                        });
                    })
                    .unwrap();
                let snapshot = session.snapshot().unwrap();
                assert!(snapshot.active_turn_id.is_some());
                assert!(snapshot.turns[0].error.is_none());
                assert_eq!(
                    terminal_notice(snapshot.turns[0].status.clone(), None, true, false, false),
                    None
                );
            }
            finish(
                &session,
                if succeeds {
                    Ok(())
                } else {
                    Err(AgentError::new(
                        "provider_retry_exhausted",
                        "Cinco reconexões falharam.",
                    ))
                },
            );
            let snapshot = session.snapshot().unwrap();
            assert!(snapshot.active_turn_id.is_none());
            assert!(snapshot.turns[0].steps[0].retry.is_none());
            assert_eq!(
                terminal_notice(snapshot.turns[0].status.clone(), None, false, false, false),
                Some(if succeeds {
                    Notice::Completed
                } else {
                    Notice::Failed
                })
            );
            let (history, _) = journal::load_all(&session.journal).unwrap();
            assert!(history[0].turn.steps[0].retry.is_none());
        }
    }

    #[test]
    fn only_idle_root_completion_or_failure_notifies() {
        assert_eq!(
            terminal_notice(TurnStatus::Completed, None, false, false, false),
            Some(Notice::Completed)
        );
        assert_eq!(
            terminal_notice(TurnStatus::Completed, None, false, false, true),
            Some(Notice::Validation)
        );
        assert_eq!(
            terminal_notice(TurnStatus::Completed, None, false, true, false),
            None
        );
        assert_eq!(
            terminal_notice(TurnStatus::Completed, None, true, false, false),
            None
        );
        assert_eq!(
            terminal_notice(TurnStatus::Error, None, false, true, false),
            Some(Notice::Failed)
        );
        assert_eq!(
            terminal_notice(
                TurnStatus::Interrupted,
                Some("progress_paused"),
                false,
                false,
                false,
            ),
            Some(Notice::Paused)
        );
        for status in [
            TurnStatus::Running,
            TurnStatus::Cancelled,
            TurnStatus::Interrupted,
        ] {
            assert_eq!(terminal_notice(status, None, false, false, false), None);
        }
    }
}
