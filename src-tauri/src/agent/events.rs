//! Compact, typed protocol between the Rust harness and the renderer.
use super::{
    authoring, compaction, diffs, history, questions, queue, ChatSnapshot, Step, ToolCall, Turn,
    Usage,
};
use serde::Serialize;
use std::sync::Mutex;
use tauri::Emitter;

pub(super) const EVENT_NAME: &str = "agent:event";

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct EventBatch {
    conversation_id: String,
    base_revision: Option<u64>,
    revision: u64,
    events: Vec<Event>,
}

#[derive(Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
enum Event {
    TurnStarted {
        turn: Turn,
    },
    ItemStarted {
        item: StartedItem,
    },
    ItemDelta {
        step_index: usize,
        text_append: String,
        summary_append: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        text_replace: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        summary_replace: Option<String>,
        duration_ms: u64,
        retry: Option<super::provider::retry::Status>,
        usage: Option<Usage>,
    },
    ItemCompleted {
        step_index: usize,
        tool: ToolCall,
    },
    ApprovalRequested {
        tool: Option<ToolCall>,
    },
    StateChanged {
        state: Box<SnapshotState>,
    },
    TurnCompleted {
        turn: Turn,
    },
}

#[derive(Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
enum StartedItem {
    Step { step_index: usize, step: Step },
    Tool { step_index: usize, tool: ToolCall },
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct SnapshotState {
    compacting: bool,
    active_turn_id: Option<String>,
    pending_approval: Option<ToolCall>,
    pending_question: Option<questions::PendingQuestion>,
    pending_authoring: Option<authoring::PendingProposal>,
    queued_messages: Vec<queue::QueuedMessage>,
    context: compaction::ContextInfo,
    compactions: Vec<compaction::CompactionEvent>,
    file_changes: Vec<diffs::FileSummary>,
    history: history::Window,
}

impl SnapshotState {
    fn from_snapshot(snapshot: &ChatSnapshot) -> Self {
        Self {
            compacting: snapshot.compacting,
            active_turn_id: snapshot.active_turn_id.clone(),
            pending_approval: snapshot.pending_approval.clone(),
            pending_question: snapshot.pending_question.clone(),
            pending_authoring: snapshot.pending_authoring.clone(),
            queued_messages: snapshot.queued_messages.clone(),
            context: snapshot.context.clone(),
            compactions: snapshot.compactions.clone(),
            file_changes: snapshot.file_changes.clone(),
            history: snapshot.history.clone(),
        }
    }
}

pub(super) struct ProtocolEmitter {
    app: tauri::AppHandle,
    previous: Mutex<Option<ChatSnapshot>>,
}

impl ProtocolEmitter {
    pub(super) fn new(app: tauri::AppHandle) -> Self {
        Self {
            app,
            previous: Mutex::new(None),
        }
    }

    pub(super) fn emit(&self, snapshot: &ChatSnapshot) {
        let Ok(mut previous) = self.previous.lock() else {
            return;
        };
        let batch = batch(previous.as_ref(), snapshot);
        *previous = Some(snapshot.clone());
        drop(previous);
        let _ = self.app.emit(EVENT_NAME, batch);
    }

    pub(super) fn seed(&self, snapshot: ChatSnapshot) {
        if let Ok(mut previous) = self.previous.lock() {
            *previous = Some(snapshot);
        }
    }
}

fn batch(previous: Option<&ChatSnapshot>, current: &ChatSnapshot) -> EventBatch {
    EventBatch {
        conversation_id: current.conversation_id.clone(),
        base_revision: previous.map(|snapshot| snapshot.revision),
        revision: current.revision,
        events: differences(previous, current),
    }
}

fn differences(previous: Option<&ChatSnapshot>, current: &ChatSnapshot) -> Vec<Event> {
    let mut events = Vec::new();
    let before_turn = previous.and_then(|snapshot| snapshot.turns.last());
    let after_turn = current.turns.last();
    if let Some(after) = after_turn {
        if before_turn.is_none_or(|before| before.id != after.id) {
            events.push(Event::TurnStarted {
                turn: after.clone(),
            });
        } else if let Some(before) = before_turn {
            append_step_events(&mut events, before, after);
            if before.status != after.status || !same(&before.error, &after.error) {
                events.push(Event::TurnCompleted {
                    turn: after.clone(),
                });
            }
        }
    }
    if previous.is_none_or(|snapshot| !same_state(snapshot, current)) {
        events.push(Event::StateChanged {
            state: Box::new(SnapshotState::from_snapshot(current)),
        });
    }
    if previous
        .and_then(|snapshot| snapshot.pending_approval.as_ref())
        .map(|tool| &tool.id)
        != current.pending_approval.as_ref().map(|tool| &tool.id)
    {
        events.push(Event::ApprovalRequested {
            tool: current.pending_approval.clone(),
        });
    }
    events
}

fn append_step_events(events: &mut Vec<Event>, before: &Turn, after: &Turn) {
    for (index, step) in after.steps.iter().enumerate().skip(before.steps.len()) {
        events.push(Event::ItemStarted {
            item: StartedItem::Step {
                step_index: index,
                step: step.clone(),
            },
        });
    }
    for index in 0..before.steps.len().min(after.steps.len()) {
        let old = &before.steps[index];
        let new = &after.steps[index];
        let text_append = new
            .text
            .strip_prefix(&old.text)
            .unwrap_or_default()
            .to_owned();
        let summary_append = new
            .summary
            .strip_prefix(&old.summary)
            .unwrap_or_default()
            .to_owned();
        let text_replace = (!new.text.starts_with(&old.text)).then(|| new.text.clone());
        let summary_replace = (!new.summary.starts_with(&old.summary)).then(|| new.summary.clone());
        if !text_append.is_empty()
            || !summary_append.is_empty()
            || text_replace.is_some()
            || summary_replace.is_some()
            || old.duration_ms != new.duration_ms
            || !same(&old.retry, &new.retry)
            || !same(&old.usage, &new.usage)
        {
            events.push(Event::ItemDelta {
                step_index: index,
                text_append,
                summary_append,
                text_replace,
                summary_replace,
                duration_ms: new.duration_ms,
                retry: new.retry.clone(),
                usage: new.usage.clone(),
            });
        }
        for tool in new.tools.iter().skip(old.tools.len()) {
            events.push(Event::ItemStarted {
                item: StartedItem::Tool {
                    step_index: index,
                    tool: tool.clone(),
                },
            });
        }
        for tool in &new.tools[..old.tools.len().min(new.tools.len())] {
            let Some(previous_tool) = old.tools.iter().find(|candidate| candidate.id == tool.id)
            else {
                continue;
            };
            if !same(previous_tool, tool) {
                if matches!(tool.status.as_str(), "completed" | "error") {
                    events.push(Event::ItemCompleted {
                        step_index: index,
                        tool: tool.clone(),
                    });
                } else {
                    events.push(Event::ItemStarted {
                        item: StartedItem::Tool {
                            step_index: index,
                            tool: tool.clone(),
                        },
                    });
                }
            }
        }
    }
}

fn same_state(left: &ChatSnapshot, right: &ChatSnapshot) -> bool {
    same(
        &SnapshotState::from_snapshot(left),
        &SnapshotState::from_snapshot(right),
    )
}

fn same(left: &impl Serialize, right: &impl Serialize) -> bool {
    serde_json::to_value(left).ok() == serde_json::to_value(right).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::tests::{options, session, Fixture};
    use crate::agent::ApprovalMode;

    #[test]
    fn streaming_updates_are_deltas_and_completion_is_explicit() {
        let fixture = Fixture::new();
        let session = session(&fixture);
        session
            .reserve("hello".into(), options(ApprovalMode::Yolo))
            .unwrap();
        session
            .update(false, |data| {
                data.turns
                    .last_mut()
                    .unwrap()
                    .turn
                    .steps
                    .push(Step::default())
            })
            .unwrap();
        let before = session.snapshot().unwrap();
        session
            .update(false, |data| {
                data.turns.last_mut().unwrap().turn.steps[0]
                    .text
                    .push_str("answer")
            })
            .unwrap();
        let after = session.snapshot().unwrap();
        let events = differences(Some(&before), &after);
        assert!(
            matches!(events.first(), Some(Event::ItemDelta { text_append, .. }) if text_append == "answer")
        );
        assert!(!serde_json::to_string(&events).unwrap().contains("\"turn\""));
    }

    #[test]
    fn event_batch_identifies_the_snapshot_revision_it_extends() {
        let fixture = Fixture::new();
        let session = session(&fixture);
        let before = session.snapshot().unwrap();
        session
            .reserve("hello".into(), options(ApprovalMode::Yolo))
            .unwrap();
        let after = session.snapshot().unwrap();
        let event = batch(Some(&before), &after);
        assert_eq!(event.base_revision, Some(before.revision));
        assert_eq!(event.revision, after.revision);
    }
}
