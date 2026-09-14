//! Compact, typed protocol between the Rust harness and the renderer.
use super::{
    authoring, compaction, diffs, history,
    protocol::chat::{ChatSnapshot, Step, ToolCall, Turn, Usage},
    questions, queue, tasks,
};
use serde::Serialize;
use std::{collections::VecDeque, sync::Mutex};
use tauri::Emitter;

pub(super) const EVENT_NAME: &str = "agent:event";
const MAX_BUFFERED_BATCHES: usize = 256;
const MAX_BUFFERED_BYTES: usize = 4 * 1024 * 1024;

#[derive(Clone, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(rename = "AgentEventBatch"))]
#[serde(rename_all = "camelCase")]
pub(super) struct EventBatch {
    protocol_version: u32,
    conversation_id: String,
    #[cfg_attr(test, ts(type = "number | null"))]
    base_revision: Option<u64>,
    #[cfg_attr(test, ts(type = "number"))]
    revision: u64,
    events: Vec<Event>,
}

#[derive(Clone, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct ChatSubscription {
    protocol_version: u32,
    reset: bool,
    snapshot: Option<ChatSnapshot>,
    batches: Vec<EventBatch>,
}

impl ChatSubscription {
    pub(super) fn from_snapshot(snapshot: ChatSnapshot, reset: bool) -> Self {
        Self {
            protocol_version: super::protocol::VERSION,
            reset,
            snapshot: Some(snapshot),
            batches: Vec::new(),
        }
    }
}

#[derive(Clone, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(rename = "AgentEvent"))]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub(super) enum Event {
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
        #[cfg_attr(test, ts(optional))]
        text_replace: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        #[cfg_attr(test, ts(optional))]
        summary_replace: Option<String>,
        #[cfg_attr(test, ts(type = "number"))]
        duration_ms: u64,
        retry: Option<super::provider::retry::Status>,
        usage: Option<Usage>,
    },
    ItemCompleted {
        step_index: usize,
        tool: ToolCall,
    },
    TasksUpdated {
        tasks: Vec<tasks::Task>,
    },
    ApprovalRequested {
        approval: Option<super::PendingApproval>,
    },
    StateChanged {
        state: Box<SnapshotState>,
    },
    TurnCompleted {
        turn: Turn,
    },
}

#[derive(Clone, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub(super) enum StartedItem {
    Step { step_index: usize, step: Step },
    Tool { step_index: usize, tool: ToolCall },
}

#[derive(Clone, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub(super) struct SnapshotState {
    compacting: bool,
    active_turn_id: Option<String>,
    pending_approval: Option<super::PendingApproval>,
    #[cfg_attr(test, ts(type = "unknown | null"))]
    pending_question: Option<questions::PendingQuestion>,
    #[cfg_attr(test, ts(type = "unknown | null"))]
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
    state: Mutex<RevisionBuffer>,
}

#[derive(Clone)]
struct BufferedBatch {
    batch: EventBatch,
    bytes: usize,
}

#[derive(Default)]
struct RevisionBuffer {
    previous: Option<ChatSnapshot>,
    batches: VecDeque<BufferedBatch>,
    bytes: usize,
}

impl RevisionBuffer {
    fn seed(&mut self, snapshot: ChatSnapshot) {
        self.previous = Some(snapshot);
        self.batches.clear();
        self.bytes = 0;
    }

    fn record(&mut self, snapshot: &ChatSnapshot) -> EventBatch {
        let batch = batch(self.previous.as_ref(), snapshot);
        self.previous = Some(snapshot.clone());
        let bytes = serde_json::to_vec(&batch).map_or(MAX_BUFFERED_BYTES + 1, |value| value.len());
        if bytes > MAX_BUFFERED_BYTES {
            self.batches.clear();
            self.bytes = 0;
            return batch;
        }
        self.bytes = self.bytes.saturating_add(bytes);
        self.batches.push_back(BufferedBatch {
            batch: batch.clone(),
            bytes,
        });
        while self.batches.len() > MAX_BUFFERED_BATCHES || self.bytes > MAX_BUFFERED_BYTES {
            if let Some(removed) = self.batches.pop_front() {
                self.bytes = self.bytes.saturating_sub(removed.bytes);
            } else {
                break;
            }
        }
        batch
    }

    fn replay(&self, cursor: u64) -> Option<Vec<EventBatch>> {
        let latest = self.previous.as_ref()?.revision;
        if cursor == latest {
            return Some(Vec::new());
        }
        let start = self
            .batches
            .iter()
            .position(|entry| entry.batch.base_revision == Some(cursor))?;
        let mut expected = cursor;
        let mut replay = Vec::new();
        for entry in self.batches.iter().skip(start) {
            if entry.batch.base_revision != Some(expected) {
                return None;
            }
            expected = entry.batch.revision;
            replay.push(entry.batch.clone());
            if expected == latest {
                return Some(replay);
            }
        }
        None
    }

    fn subscribe(&self, cursor: Option<u64>, fallback: ChatSnapshot) -> ChatSubscription {
        if let Some(cursor) = cursor {
            if let Some(batches) = self.replay(cursor) {
                return ChatSubscription {
                    protocol_version: super::protocol::VERSION,
                    reset: false,
                    snapshot: None,
                    batches,
                };
            }
        }
        let batches = self.replay(fallback.revision).unwrap_or_default();
        ChatSubscription {
            protocol_version: super::protocol::VERSION,
            reset: cursor.is_some(),
            snapshot: Some(fallback),
            batches,
        }
    }
}

impl ProtocolEmitter {
    pub(super) fn new(app: tauri::AppHandle) -> Self {
        Self {
            app,
            state: Mutex::new(RevisionBuffer::default()),
        }
    }

    pub(super) fn emit(&self, snapshot: &ChatSnapshot) {
        let Ok(mut state) = self.state.lock() else {
            return;
        };
        let batch = state.record(snapshot);
        drop(state);
        let _ = self.app.emit(EVENT_NAME, batch);
    }

    pub(super) fn seed(&self, snapshot: ChatSnapshot) {
        if let Ok(mut state) = self.state.lock() {
            state.seed(snapshot);
        }
    }

    pub(super) fn subscribe(
        &self,
        cursor: Option<u64>,
        fallback: ChatSnapshot,
    ) -> Result<ChatSubscription, super::AgentError> {
        self.state
            .lock()
            .map(|state| state.subscribe(cursor, fallback))
            .map_err(|_| super::AgentError::internal())
    }
}

fn batch(previous: Option<&ChatSnapshot>, current: &ChatSnapshot) -> EventBatch {
    EventBatch {
        protocol_version: super::protocol::VERSION,
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
            if before.tasks != after.tasks {
                events.push(Event::TasksUpdated {
                    tasks: after.tasks.clone(),
                });
            }
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
        .map(|approval| &approval.tool.id)
        != current
            .pending_approval
            .as_ref()
            .map(|approval| &approval.tool.id)
    {
        events.push(Event::ApprovalRequested {
            approval: current.pending_approval.clone(),
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
    fn task_updates_are_emitted_while_the_turn_is_active() {
        let fixture = Fixture::new();
        let session = session(&fixture);
        session
            .reserve("hello".into(), options(ApprovalMode::Yolo))
            .unwrap();
        let before = session.snapshot().unwrap();
        let tasks = vec![tasks::Task {
            id: "inspect".into(),
            title: "Inspecionar o projeto".into(),
            status: tasks::Status::InProgress,
        }];

        session.replace_tasks(tasks.clone()).unwrap();

        let after = session.snapshot().unwrap();
        assert_eq!(after.active_turn_id, before.active_turn_id);
        assert!(matches!(
            differences(Some(&before), &after).as_slice(),
            [Event::TasksUpdated { tasks: updated }] if updated == &tasks
        ));
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
        let serialized = serde_json::to_value(&event).unwrap();
        assert_eq!(
            serialized["protocolVersion"],
            super::super::protocol::VERSION
        );
        assert_eq!(event.base_revision, Some(before.revision));
        assert_eq!(event.revision, after.revision);
    }

    #[test]
    fn stream_event_fields_follow_the_renderer_contract() {
        let event = Event::ItemDelta {
            step_index: 2,
            text_append: "texto".into(),
            summary_append: "resumo".into(),
            text_replace: None,
            summary_replace: None,
            duration_ms: 15,
            retry: None,
            usage: None,
        };
        let serialized = serde_json::to_value(event).unwrap();
        assert_eq!(serialized["type"], "itemDelta");
        assert_eq!(serialized["stepIndex"], 2);
        assert_eq!(serialized["textAppend"], "texto");
        assert!(serialized.get("step_index").is_none());
        assert!(serialized.get("textReplace").is_none());
    }

    #[test]
    fn revision_buffer_replays_a_contiguous_cursor_without_a_snapshot() {
        let fixture = Fixture::new();
        let session = session(&fixture);
        let initial = session.snapshot().unwrap();
        let mut buffer = RevisionBuffer::default();
        buffer.seed(initial.clone());

        session
            .reserve("hello".into(), options(ApprovalMode::Yolo))
            .unwrap();
        let started = session.snapshot().unwrap();
        buffer.record(&started);
        session
            .update(false, |data| {
                data.turns.last_mut().unwrap().turn.steps.push(Step {
                    text: "streaming".into(),
                    ..Step::default()
                });
            })
            .unwrap();
        let streaming = session.snapshot().unwrap();
        buffer.record(&streaming);

        let subscription = buffer.subscribe(Some(initial.revision), streaming);
        assert!(!subscription.reset);
        assert!(subscription.snapshot.is_none());
        assert_eq!(subscription.batches.len(), 2);
        assert_eq!(
            subscription.batches[0].base_revision,
            Some(initial.revision)
        );
        assert_eq!(
            subscription.batches[1].base_revision,
            Some(started.revision)
        );
    }

    #[test]
    fn revision_buffer_requests_a_reset_when_the_cursor_is_not_retained() {
        let fixture = Fixture::new();
        let session = session(&fixture);
        let snapshot = session.snapshot().unwrap();
        let mut buffer = RevisionBuffer::default();
        buffer.seed(snapshot.clone());

        let subscription = buffer.subscribe(Some(snapshot.revision.saturating_add(99)), snapshot);
        assert!(subscription.reset);
        assert!(subscription.snapshot.is_some());
        assert!(subscription.batches.is_empty());
    }
}
