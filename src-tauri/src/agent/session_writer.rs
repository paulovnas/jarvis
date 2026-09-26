//! Ordered, recoverable journal writes for one conversation.
//!
//! The agent loop only enqueues immutable operations. A dedicated writer owns
//! disk ordering and the last successfully persisted turn. Flush barriers are
//! used before effects are exposed to a later model step and during shutdown.
use super::{journal, AgentError, StoredTurn};
use serde::Serialize;
use serde_json::Value;
use std::{
    collections::VecDeque,
    path::{Path, PathBuf},
    sync::{mpsc, Mutex},
    thread::JoinHandle,
    time::Duration,
};

const FLUSH_TIMEOUT: Duration = Duration::from_secs(5);
const RETRY_DELAYS: [Duration; 3] = [
    Duration::from_millis(20),
    Duration::from_millis(75),
    Duration::from_millis(200),
];

#[derive(Clone)]
enum Operation {
    Turn(Box<StoredTurn>),
    Event { kind: String, value: Value },
}

enum Command {
    Write(Operation),
    Flush(mpsc::Sender<Result<(), ()>>),
    AsyncFlush(tokio::sync::oneshot::Sender<Result<(), ()>>),
    Shutdown,
    #[cfg(test)]
    Pause(mpsc::Receiver<()>),
}

pub(super) struct SessionWriter {
    conversation_id: String,
    sender: mpsc::Sender<Command>,
    worker: Mutex<Option<JoinHandle<()>>>,
}

impl SessionWriter {
    #[cfg(test)]
    pub(super) fn pause(&self) -> mpsc::Sender<()> {
        let (release, gate) = mpsc::channel();
        self.sender.send(Command::Pause(gate)).unwrap();
        release
    }
    pub(super) fn start(
        path: PathBuf,
        conversation_id: String,
        durable_turn: Option<StoredTurn>,
    ) -> Result<Self, AgentError> {
        let (sender, receiver) = mpsc::channel();
        let worker = std::thread::Builder::new()
            .name(format!("jarvis-journal-{conversation_id}"))
            .spawn(move || run(path, durable_turn, receiver))
            .map_err(|_| AgentError::storage())?;
        Ok(Self {
            conversation_id,
            sender,
            worker: Mutex::new(Some(worker)),
        })
    }

    pub(super) fn append_turn(&self, turn: StoredTurn) -> Result<(), AgentError> {
        self.enqueue(Operation::Turn(Box::new(turn)))
    }

    pub(super) fn append_event(
        &self,
        kind: &str,
        value: &impl Serialize,
    ) -> Result<(), AgentError> {
        let value = serde_json::to_value(value).map_err(|_| AgentError::storage())?;
        self.enqueue(Operation::Event {
            kind: kind.to_owned(),
            value,
        })
    }

    fn enqueue(&self, operation: Operation) -> Result<(), AgentError> {
        self.sender
            .send(Command::Write(operation))
            .map_err(|_| self.failure())
    }

    pub(super) fn flush(&self) -> Result<(), AgentError> {
        let (reply, received) = mpsc::channel();
        self.sender
            .send(Command::Flush(reply))
            .map_err(|_| self.failure())?;
        match received.recv_timeout(FLUSH_TIMEOUT) {
            Ok(Ok(())) => Ok(()),
            Ok(Err(())) | Err(_) => Err(self.failure()),
        }
    }

    /// Dropping the waiter never cancels the already queued writes or barrier.
    pub(super) async fn flush_async(&self) -> Result<(), AgentError> {
        let (reply, received) = tokio::sync::oneshot::channel();
        self.sender
            .send(Command::AsyncFlush(reply))
            .map_err(|_| self.failure())?;
        match tokio::time::timeout(FLUSH_TIMEOUT, received).await {
            Ok(Ok(Ok(()))) => Ok(()),
            _ => Err(self.failure()),
        }
    }

    fn failure(&self) -> AgentError {
        crate::diagnostics::record_storage_failure(
            "session_journal_writer",
            Some(&self.conversation_id),
        );
        AgentError::storage()
    }
}

impl Drop for SessionWriter {
    fn drop(&mut self) {
        // Explicit barriers own durability. Drop only queues a final drain and
        // detaches the OS worker: executor threads must not join a slow disk.
        let _ = self.sender.send(Command::Shutdown);
        if let Ok(worker) = self.worker.get_mut() {
            let _ = worker.take();
        }
    }
}

fn run(path: PathBuf, mut durable_turn: Option<StoredTurn>, receiver: mpsc::Receiver<Command>) {
    let mut pending = VecDeque::new();
    while let Ok(command) = receiver.recv() {
        match command {
            #[cfg(test)]
            Command::Pause(gate) => {
                let _ = gate.recv();
            }
            Command::Write(operation) => {
                pending.push_back(operation);
                let _ = drain_once(&path, &mut durable_turn, &mut pending);
            }
            Command::Flush(reply) => {
                let _ = reply.send(drain_with_retry(&path, &mut durable_turn, &mut pending));
            }
            Command::AsyncFlush(reply) => {
                let _ = reply.send(drain_with_retry(&path, &mut durable_turn, &mut pending));
            }
            Command::Shutdown => {
                let _ = drain_with_retry(&path, &mut durable_turn, &mut pending);
                break;
            }
        }
    }
}

fn drain_with_retry(
    path: &Path,
    durable_turn: &mut Option<StoredTurn>,
    pending: &mut VecDeque<Operation>,
) -> Result<(), ()> {
    drain_with_retry_using(
        path,
        durable_turn,
        pending,
        &RETRY_DELAYS,
        &mut write_operation,
    )
}

fn drain_with_retry_using(
    path: &Path,
    durable_turn: &mut Option<StoredTurn>,
    pending: &mut VecDeque<Operation>,
    retry_delays: &[Duration],
    write: &mut impl FnMut(&Path, &mut Option<StoredTurn>, &Operation) -> Result<(), AgentError>,
) -> Result<(), ()> {
    if drain_once_using(path, durable_turn, pending, write).is_ok() {
        return Ok(());
    }
    for delay in retry_delays {
        std::thread::sleep(*delay);
        if drain_once_using(path, durable_turn, pending, write).is_ok() {
            return Ok(());
        }
    }
    Err(())
}

fn drain_once(
    path: &Path,
    durable_turn: &mut Option<StoredTurn>,
    pending: &mut VecDeque<Operation>,
) -> Result<(), ()> {
    drain_once_using(path, durable_turn, pending, &mut write_operation)
}

fn drain_once_using(
    path: &Path,
    durable_turn: &mut Option<StoredTurn>,
    pending: &mut VecDeque<Operation>,
    write: &mut impl FnMut(&Path, &mut Option<StoredTurn>, &Operation) -> Result<(), AgentError>,
) -> Result<(), ()> {
    while let Some(operation) = pending.front() {
        write(path, durable_turn, operation).map_err(|_| ())?;
        pending.pop_front();
    }
    Ok(())
}

fn write_operation(
    path: &Path,
    durable_turn: &mut Option<StoredTurn>,
    operation: &Operation,
) -> Result<(), AgentError> {
    match operation {
        Operation::Turn(candidate) => {
            match durable_turn.as_ref() {
                Some(durable) if durable.turn.id == candidate.turn.id => {
                    journal::append_update(path, durable, candidate)?;
                }
                _ => journal::append(path, candidate)?,
            }
            *durable_turn = Some((**candidate).clone());
        }
        Operation::Event { kind, value } => journal::append_event(path, kind, value)?,
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(flavor = "current_thread")]
    async fn disk_wait_yields_and_cancelled_waiter_preserves_queued_events() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("session.jsonl");
        std::fs::write(&path, "{}\n").unwrap();
        let writer = SessionWriter::start(path.clone(), "async".into(), None).unwrap();
        let release = writer.pause();
        writer
            .append_event("queue_checkpoint", &serde_json::json!([]))
            .unwrap();
        assert!(
            tokio::time::timeout(Duration::from_millis(20), writer.flush_async())
                .await
                .is_err()
        );
        release.send(()).unwrap();
        writer.flush_async().await.unwrap();
        assert!(std::fs::read_to_string(path)
            .unwrap()
            .contains("queue_checkpoint"));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn async_update_releases_state_lock_while_disk_is_slow() {
        use crate::agent::tests::{options, session, Fixture};
        let fixture = Fixture::new();
        let session = session(&fixture);
        session
            .reserve("hello".into(), options(crate::agent::ApprovalMode::Yolo))
            .unwrap();
        let release = session.writer.pause();
        let update = session.update_async(|data| {
            data.turns
                .last_mut()
                .unwrap()
                .turn
                .steps
                .push(crate::agent::Step::default())
        });
        tokio::pin!(update);
        tokio::select! {
            _ = &mut update => panic!("must wait for disk"),
            _ = tokio::time::sleep(Duration::from_millis(20)) => {},
        }
        assert!(session.data.try_lock().is_ok());
        assert_eq!(session.snapshot().unwrap().turns[0].steps.len(), 1);
        release.send(()).unwrap();
        update.await.unwrap();
        assert_eq!(
            journal::load_all(&session.journal).unwrap().0[0]
                .turn
                .steps
                .len(),
            1
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn acknowledgement_emits_only_its_own_revision_during_concurrent_updates() {
        use crate::agent::tests::{options, session, Fixture};
        use std::sync::Arc;

        let fixture = Fixture::new();
        let mut session = session(&fixture);
        let emitted = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&emitted);
        Arc::get_mut(&mut session).unwrap().emit = Arc::new(move |snapshot| {
            sink.lock().unwrap().push(snapshot);
        });
        session
            .reserve("hello".into(), options(crate::agent::ApprovalMode::Yolo))
            .unwrap();
        emitted.lock().unwrap().clear();
        let release_first = session.writer.pause();
        let first = session.update_async(|data| {
            data.turns
                .last_mut()
                .unwrap()
                .turn
                .steps
                .push(crate::agent::Step::default());
        });
        tokio::pin!(first);
        tokio::select! {
            biased;
            _ = &mut first => panic!("first acknowledgement is paused"),
            _ = tokio::task::yield_now() => {},
        }
        let release_second = session.writer.pause();
        let second = session.update_async(|data| {
            data.turns
                .last_mut()
                .unwrap()
                .turn
                .steps
                .push(crate::agent::Step::default());
        });
        tokio::pin!(second);
        tokio::select! {
            biased;
            _ = &mut second => panic!("second acknowledgement is paused"),
            _ = tokio::task::yield_now() => {},
        }
        assert!(emitted.lock().unwrap().is_empty());
        release_first.send(()).unwrap();
        tokio::time::timeout(Duration::from_secs(2), &mut first)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(emitted.lock().unwrap()[0].turns[0].steps.len(), 1);
        assert_eq!(
            journal::load_all(&session.journal).unwrap().0[0]
                .turn
                .steps
                .len(),
            1
        );
        release_second.send(()).unwrap();
        second.await.unwrap();
        assert_eq!(emitted.lock().unwrap()[1].turns[0].steps.len(), 2);
        assert_eq!(
            journal::load_all(&session.journal).unwrap().0[0]
                .turn
                .steps
                .len(),
            2
        );
    }

    #[test]
    fn transient_failure_retries_the_same_operation_and_preserves_order() {
        let mut pending = VecDeque::from([
            Operation::Event {
                kind: "first".into(),
                value: Value::Null,
            },
            Operation::Event {
                kind: "second".into(),
                value: Value::Null,
            },
            Operation::Event {
                kind: "third".into(),
                value: Value::Null,
            },
        ]);
        let mut durable = None;
        let mut attempted = Vec::<String>::new();
        let mut fail_once = true;
        let mut write = |_: &Path,
                         _: &mut Option<StoredTurn>,
                         operation: &Operation|
         -> Result<(), AgentError> {
            let Operation::Event { kind, .. } = operation else {
                unreachable!()
            };
            attempted.push(kind.clone());
            if kind == "second" && fail_once {
                fail_once = false;
                return Err(AgentError::storage());
            }
            Ok(())
        };
        assert!(drain_with_retry_using(
            Path::new("unused"),
            &mut durable,
            &mut pending,
            &[Duration::ZERO],
            &mut write,
        )
        .is_ok());
        assert_eq!(attempted, ["first", "second", "second", "third"]);
        assert!(pending.is_empty());
    }

    #[test]
    fn flush_and_shutdown_persist_queued_turns() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("session.jsonl");
        // The first JSONL line is the immutable conversation header and is
        // deliberately skipped by the journal replay code.
        std::fs::write(&path, "{}\n").unwrap();
        let writer = SessionWriter::start(path.clone(), "conversation".into(), None).unwrap();
        let turn = StoredTurn {
            turn: super::super::Turn {
                id: "turn".into(),
                created_at: 1,
                duration_ms: 0,
                user: "hello".into(),
                parts: vec![],
                options: super::super::TurnOptions {
                    executor: crate::claude::Executor::Jarvis,
                    account: "account".into(),
                    model: "model".into(),
                    reasoning: None,
                    mode: super::super::Mode::Build,
                    workflow: None,
                    custom_workflow_id: None,
                    custom_agent_id: None,
                    approval_mode: super::super::ApprovalMode::Yolo,
                    manual_validation: false,
                },
                context_window: None,
                status: super::super::TurnStatus::Running,
                tasks: vec![],
                steps: vec![],
                error: None,
            },
            wire: vec![serde_json::json!({"role":"user","content":"hello"})],
            mcp_intent: None,
        };
        writer.append_turn(turn).unwrap();
        writer.flush().unwrap();
        assert_eq!(journal::load_all(&path).unwrap().0.len(), 1);
        drop(writer);
        assert_eq!(journal::load_all(&path).unwrap().0[0].turn.id, "turn");
    }
}
