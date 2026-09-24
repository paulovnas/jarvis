//! Execute only the independent native-read prefix of a streaming response.
//! Writes, approvals, unknown tools and dependencies end admission for the step.
use super::{core_runtime, telemetry, tools, AgentError, Mode, Session, ToolCall};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    sync::Arc,
    time::Instant,
};
use tokio::{sync::watch, task::JoinSet};

pub(super) struct Reads {
    session: Arc<Session>,
    signal: watch::Receiver<bool>,
    mode: Mode,
    trace: telemetry::TraceContext,
    contract: super::tool_contract::Orchestrator,
    calls: HashMap<String, ToolCall>,
    fingerprints: HashSet<String>,
    stopped: bool,
    wire_start: usize,
    jobs: JoinSet<Result<(String, tools::ParallelExecution), AgentError>>,
}

impl Reads {
    pub(super) fn new(
        session: Arc<Session>,
        signal: watch::Receiver<bool>,
        mode: Mode,
        trace: telemetry::TraceContext,
    ) -> Self {
        let wire_start = session
            .data
            .lock()
            .ok()
            .and_then(|data| data.turns.last().map(|turn| turn.wire.len()))
            .unwrap_or(usize::MAX);
        Self {
            session,
            signal,
            mode,
            trace,
            contract: super::tool_contract::Orchestrator::new(&tools::definitions(mode)),
            calls: HashMap::new(),
            fingerprints: HashSet::new(),
            stopped: false,
            wire_start,
            jobs: JoinSet::new(),
        }
    }

    pub(super) fn is_empty(&self) -> bool {
        self.calls.is_empty()
    }

    pub(super) fn contains(&self, call: &ToolCall) -> bool {
        self.calls
            .get(&call.id)
            .is_some_and(|old| old.name == call.name && old.args == call.args)
    }

    pub(super) fn admit(
        &mut self,
        call: ToolCall,
        envelope: Vec<serde_json::Value>,
        eligible: bool,
    ) {
        if self.contains(&call) {
            return;
        }
        if self.stopped
            || !eligible
            || self.contract.preflight(&call).is_err()
            || self.calls.len() >= 4
            || *self.signal.borrow()
        {
            self.stopped = true;
            return;
        }
        if !matches!(call.name.as_str(), "read" | "list" | "search")
            || self.calls.contains_key(&call.id)
            || !self
                .fingerprints
                .insert(format!("{}:{}", call.name, call.args))
        {
            self.stopped = true;
            return;
        }
        self.calls.insert(call.id.clone(), call.clone());
        let session = Arc::clone(&self.session);
        let signal = self.signal.clone();
        let mode = self.mode;
        let trace = self.trace.clone();
        let wire_start = self.wire_start;
        let queued = telemetry::phase(&trace, telemetry::Phase::ToolQueue);
        self.jobs.spawn(async move {
            drop(queued);
            session
                .update_async(|data| {
                    let current = data.turns.last_mut().unwrap();
                    for item in envelope {
                        if !current
                            .wire
                            .iter()
                            .skip(wire_start)
                            .any(|existing| existing == &item)
                        {
                            current.wire.push(item);
                        }
                    }
                    let mut running = call.clone();
                    running.status = "running".into();
                    current.turn.steps.last_mut().unwrap().tools.push(running);
                })
                .await?;
            let started = Instant::now();
            let handler = telemetry::phase(&trace, telemetry::Phase::ToolHandler);
            let result = tools::execute_with_revision(&session.root, &call, mode, signal).await;
            drop(handler);
            let duration_ms = started.elapsed().as_millis() as u64;
            let (output, status) = match &result {
                Ok(value) => (value.output.clone(), "completed"),
                Err(error) => (error.message.clone(), "error"),
            };
            telemetry::record(
                &trace,
                telemetry::Event::ToolFinished {
                    tool: telemetry::tool_kind(&call.name),
                    tool_id: telemetry::tool_id(&call.id),
                    outcome: telemetry::outcome(result.as_ref().err(), false),
                    duration_ms,
                    input_bytes: telemetry::serialized_bytes(&call.args),
                    output_bytes: output.len() as u64,
                    failure: result.as_ref().err().map(telemetry::failure_class),
                },
            );
            // Preserve a confirmed read even if the provider disconnects later.
            core_runtime::checkpoint_tool(&session, &call, &output, status, duration_ms, None)
                .await?;
            Ok((
                call.id,
                tools::ParallelExecution {
                    result,
                    duration_ms,
                },
            ))
        });
    }

    pub(super) async fn drain(
        &mut self,
    ) -> Result<BTreeMap<String, tools::ParallelExecution>, AgentError> {
        let mut results = BTreeMap::new();
        let mut failure = None;
        while let Some(result) = self.jobs.join_next().await {
            match result {
                Ok(Ok((id, result))) => {
                    results.insert(id, result);
                }
                Ok(Err(error)) => {
                    failure = Some(error);
                }
                Err(_) => {
                    failure = Some(AgentError::internal());
                }
            }
        }
        match failure {
            Some(error) => Err(error),
            None => Ok(results),
        }
    }

    /// Replace provisional envelopes with the exact completed provider output,
    /// keeping matching results after their calls. Orphaned retry results stay
    /// in history and are never silently discarded or executed again here.
    pub(super) fn reconcile(
        &self,
        current: &mut super::StoredTurn,
        calls: &[ToolCall],
        output: Vec<serde_json::Value>,
    ) {
        let ids: HashSet<_> = calls
            .iter()
            .filter(|call| self.contains(call))
            .map(|call| call.id.as_str())
            .collect();
        let mut results = HashMap::new();
        let mut index = 0;
        current.wire.retain(|item| {
            let before_step = index < self.wire_start;
            index += 1;
            if before_step {
                return true;
            }
            let matched = item["call_id"].as_str().is_some_and(|id| ids.contains(id));
            if matched && item["type"] == "function_call_output" {
                results.insert(item["call_id"].as_str().unwrap().to_owned(), item.clone());
            }
            !matched && !output.contains(item)
        });
        current.wire.extend(output);
        current
            .wire
            .extend(calls.iter().filter_map(|call| results.remove(&call.id)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{
        journal,
        tests::{options, session, Fixture},
        ApprovalMode, Step,
    };
    use serde_json::json;

    #[tokio::test(flavor = "current_thread")]
    async fn reads_finish_before_stream_end_and_replay_once_after_disconnect() {
        let fixture = Fixture::new();
        let session = session(&fixture);
        std::fs::write(session.root.join("data.txt"), "confirmed content").unwrap();
        let signal = session
            .reserve("Read data".into(), options(ApprovalMode::Yolo))
            .unwrap();
        session
            .update(true, |data| {
                data.turns
                    .last_mut()
                    .unwrap()
                    .turn
                    .steps
                    .push(Step::default())
            })
            .unwrap();
        let mut reads = Reads::new(
            Arc::clone(&session),
            signal,
            Mode::Build,
            telemetry::trace(&session.id, "turn"),
        );
        let call = ToolCall {
            id: "read-during-stream".into(),
            name: "read".into(),
            args: json!({"path":"data.txt"}),
            status: "pending".into(),
            output: String::new(),
            duration_ms: 0,
        };
        let envelope = vec![
            json!({"type":"reasoning", "encrypted_content":"private-replay-signature", "summary":[]}),
            json!({"type":"function_call", "call_id":call.id, "name":call.name, "arguments":call.args.to_string()}),
        ];
        reads.admit(call.clone(), envelope.clone(), true);
        reads.admit(call.clone(), envelope.clone(), true);
        // The simulated provider is still waiting for its terminal frame.
        let (_end, unfinished_stream) = tokio::sync::oneshot::channel::<()>();
        tokio::select! {
            _ = unfinished_stream => panic!("provider has not completed"),
            results = reads.drain() => {
                let results = results.unwrap();
                assert_eq!(results.len(), 1);
                assert!(results[&call.id].result.as_ref().unwrap().output.contains("confirmed content"));
            }
        }
        let (turns, _) = journal::load_all(&session.journal).unwrap();
        assert!(turns[0]
            .wire
            .iter()
            .any(|item| item["encrypted_content"] == "private-replay-signature"));
        let mut replay = turns[0].clone();
        reads.reconcile(&mut replay, std::slice::from_ref(&call), envelope);
        assert_eq!(
            replay
                .wire
                .iter()
                .filter(|item| item["type"] == "reasoning")
                .count(),
            1
        );
        assert_eq!(
            replay
                .wire
                .iter()
                .filter(|item| item["type"] == "function_call")
                .count(),
            1
        );
        assert_eq!(replay.wire.last().unwrap()["type"], "function_call_output");
        assert_eq!(
            turns[0]
                .wire
                .iter()
                .filter(|item| item["type"] == "function_call_output")
                .count(),
            1
        );
        // Concurrent reads can finish out of order; final replay follows the
        // provider's call order while retaining older identical messages.
        let mut second = call.clone();
        second.id = "second-read".into();
        reads.calls.insert(second.id.clone(), second.clone());
        let mut ordered = turns[0].clone();
        let earlier = ordered.wire[0].clone();
        let mut completed = vec![earlier.clone()];
        for item in [&call, &second] {
            completed.push(json!({"type":"function_call", "call_id":item.id, "name":item.name, "arguments":item.args.to_string()}));
        }
        ordered.wire.insert(
            reads.wire_start,
            json!({"type":"function_call_output", "call_id":second.id, "output":"second result"}),
        );
        reads.reconcile(&mut ordered, &[call.clone(), second], completed);
        assert_eq!(
            ordered.wire.iter().filter(|item| **item == earlier).count(),
            2
        );
        let results = ordered
            .wire
            .iter()
            .filter(|item| item["type"] == "function_call_output")
            .map(|item| item["call_id"].as_str().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(results, [call.id.as_str(), "second-read"]);
        let mut changed = call;
        changed.id = "not-eligible".into();
        reads.admit(changed.clone(), vec![], false);
        changed.id = "after-barrier".into();
        reads.admit(changed, vec![], true);
        assert!(reads.drain().await.unwrap().is_empty());
    }
}
