//! Advisory recovery based on observed results, never on the number of useful actions.
use super::{AgentError, ToolCall};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::{HashSet, VecDeque};

pub(super) const TOOL_NAME: &str = "progress_checkpoint";
const UNPRODUCTIVE_STREAK: usize = 8;
const EVIDENCE_CAPACITY: usize = 512;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Observation {
    MaterialProgress,
    NewEvidence,
    Unproductive,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Action {
    SuggestCheckpoint(String),
}

#[derive(Default)]
pub(super) struct Watchdog {
    checkpoint_pending: bool,
    announce: bool,
    unproductive_streak: usize,
    evidence: HashSet<u64>,
    evidence_order: VecDeque<u64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Checkpoint {
    objective: String,
    evidence: Vec<String>,
    next_action: String,
}

impl Watchdog {
    pub(super) fn definition(&self) -> Option<Value> {
        self.checkpoint_pending.then(|| json!({
            "type": "function",
            "name": TOOL_NAME,
            "description": "Recovery aid after repeated unchanged results or errors. Summarize the active objective, reuse concrete evidence, and choose one bounded next action. You may also proceed directly with a different productive action or finish with an accurate result. A checkpoint is not a prerequisite for continuing.",
            "parameters": {
                "type": "object",
                "properties": {
                    "objective": {"type":"string","minLength":1,"maxLength":2000},
                    "evidence": {"type":"array","items":{"type":"string","minLength":1,"maxLength":2000},"minItems":1,"maxItems":12},
                    "nextAction": {"type":"string","minLength":1,"maxLength":2000}
                },
                "required": ["objective", "evidence", "nextAction"],
                "additionalProperties": false
            }
        }))
    }

    pub(super) fn preflight(&self, tool: &ToolCall) -> Result<(), AgentError> {
        if tool.name == TOOL_NAME && !self.checkpoint_pending {
            return Err(AgentError::new("tool_unavailable", "Nenhum checkpoint de progresso está pendente. Continue com a próxima ação necessária."));
        }
        Ok(())
    }

    pub(super) fn checkpoint(&mut self, args: &Value) -> Result<String, AgentError> {
        let checkpoint: Checkpoint = serde_json::from_value(args.clone()).map_err(|error| {
            AgentError::new(
                "progress_checkpoint_invalid",
                &format!("Informe objective, evidence e nextAction: {error}"),
            )
        })?;
        if !bounded(&checkpoint.objective)
            || !bounded(&checkpoint.next_action)
            || !(1..=12).contains(&checkpoint.evidence.len())
            || !checkpoint.evidence.iter().all(|item| bounded(item))
        {
            return Err(AgentError::new("progress_checkpoint_invalid", "Informe um objetivo, de 1 a 12 evidências e uma próxima ação, cada texto com 1 a 2000 bytes."));
        }
        self.reset_guidance();
        // Keep evidence identities: a checkpoint must not make an old result new again.
        Ok("Checkpoint registrado. Execute a próxima ação delimitada usando as evidências já obtidas.".into())
    }

    pub(super) fn observe(
        &mut self,
        tool: &ToolCall,
        failed: bool,
        output: &str,
        confirmed_mutation: bool,
    ) -> Observation {
        if tool.name == TOOL_NAME {
            return Observation::Unproductive;
        }
        let new_evidence =
            !failed && !output.trim().is_empty() && self.remember(fingerprint(tool, output));
        if !failed && (confirmed_mutation || (new_evidence && material_tool(&tool.name, output))) {
            self.reset_guidance();
            return Observation::MaterialProgress;
        }
        if new_evidence {
            self.reset_guidance();
            return Observation::NewEvidence;
        }
        self.unproductive_streak += 1;
        if self.unproductive_streak >= UNPRODUCTIVE_STREAK {
            self.checkpoint_pending = true;
            self.announce = true;
            self.unproductive_streak = 0;
        }
        Observation::Unproductive
    }

    pub(super) fn take_action(&mut self) -> Option<Action> {
        if !std::mem::take(&mut self.announce) {
            return None;
        }
        Some(Action::SuggestCheckpoint("Jarvis recovery guidance (runtime instruction, not a new user request): recent actions returned errors or unchanged evidence. Reuse confirmed results and choose a different bounded action toward the user's current objective. You can use progress_checkpoint to organize recovery. Do not repeat an uncertain mutation; inspect its outcome first. If an external dependency truly prevents completion, report that specific blocker or use ask_user. This guidance does not suspend the task or require another user message.".into()))
    }

    fn reset_guidance(&mut self) {
        self.checkpoint_pending = false;
        self.announce = false;
        self.unproductive_streak = 0;
    }

    fn remember(&mut self, fingerprint: u64) -> bool {
        if !self.evidence.insert(fingerprint) {
            return false;
        }
        self.evidence_order.push_back(fingerprint);
        if self.evidence_order.len() > EVIDENCE_CAPACITY {
            if let Some(oldest) = self.evidence_order.pop_front() {
                self.evidence.remove(&oldest);
            }
        }
        true
    }
}

fn bounded(value: &str) -> bool {
    let value = value.trim();
    !value.is_empty() && value.len() <= 2_000
}

fn material_tool(name: &str, output: &str) -> bool {
    if name.starts_with("jarvis_propose_") {
        // A declined proposal or partial application is not a confirmed mutation.
        return serde_json::from_str::<Value>(output).is_ok_and(|v| {
            v["approved"] == true && matches!(v["status"].as_str(), Some("applied" | "published"))
        });
    }
    matches!(
        name,
        "update_tasks"
            | "beads_create"
            | "beads_update"
            | "beads_claim"
            | "beads_close"
            | "beads_dependency"
            | "hub_spawn"
            | "hub_retry"
            | "hub_cancel"
            | "hub_send"
            | "hub_complete"
            | "hub_request_guidance"
            | "hub_respond_guidance"
            | "workflow_check"
            | "validation_publish"
            | "design_brief"
            | "ask_user"
            | "ctx_index"
            | "generate_image"
            | "process_start"
            | "process_stop"
            | "process_remove"
            | "terminal_start"
            | "terminal_write"
            | "terminal_close"
    )
}

fn fingerprint(tool: &ToolCall, output: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    tool.name.hash(&mut hash);
    hash_value(&tool.args, &mut hash);
    output.hash(&mut hash);
    hash.finish()
}

fn hash_value(value: &Value, hash: &mut impl std::hash::Hasher) {
    use std::hash::Hash;
    match value {
        Value::Object(object) => {
            let mut fields: Vec<_> = object.iter().collect();
            fields.sort_by_key(|(key, _)| *key);
            for (key, value) in fields {
                key.hash(hash);
                hash_value(value, hash);
            }
        }
        Value::Array(items) => {
            for item in items {
                hash_value(item, hash);
            }
        }
        _ => value.to_string().hash(hash),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tool(name: &str, index: usize) -> ToolCall {
        ToolCall {
            id: format!("call-{index}"),
            name: name.into(),
            args: json!({"path":format!("src/{index}.ts")}),
            status: "completed".into(),
            output: String::new(),
            duration_ms: 0,
        }
    }

    #[test]
    fn arbitrarily_long_analysis_with_new_evidence_never_pauses_or_requires_writes() {
        let mut watchdog = Watchdog::default();
        for index in 0..2_000 {
            assert_eq!(
                watchdog.observe(&tool("read", index), false, &format!("fact-{index}"), false),
                Observation::NewEvidence
            );
            assert!(watchdog.take_action().is_none());
        }
        assert!(watchdog.evidence.len() <= EVIDENCE_CAPACITY);
        super::super::evaluation::assert_runtime_report(
            "progress-long-run",
            super::super::evaluation::RuntimeReport::new(
                "completed",
                [
                    ("checkpoints", 0),
                    ("evidenceEvents", 2000),
                    ("toolCalls", 2000),
                ],
            ),
        );
    }

    #[test]
    fn repeated_results_offer_recovery_without_blocking_next_action() {
        let mut watchdog = Watchdog::default();
        let call = tool("read", 0);
        for _ in 0..=UNPRODUCTIVE_STREAK {
            watchdog.observe(&call, false, "same", false);
        }
        assert!(matches!(
            watchdog.take_action(),
            Some(Action::SuggestCheckpoint(_))
        ));
        assert!(watchdog.definition().is_some());
        assert!(watchdog.preflight(&tool("bash", 1)).is_ok());
        assert_eq!(
            watchdog.observe(&tool("bash", 1), false, "checks passed", false),
            Observation::NewEvidence
        );
        assert!(watchdog.definition().is_none());
    }

    #[test]
    fn checkpoint_keeps_evidence_and_invalid_checkpoints_do_not_block_work() {
        let mut watchdog = Watchdog::default();
        let call = tool("read", 0);
        for _ in 0..=UNPRODUCTIVE_STREAK {
            watchdog.observe(&call, false, "same", false);
        }
        for _ in 0..5 {
            assert!(watchdog.checkpoint(&json!({})).is_err());
        }
        assert!(watchdog.preflight(&tool("bash", 1)).is_ok());
        watchdog.checkpoint(&json!({"objective":"Publish the requested diff", "evidence":["Diff inspected"], "nextAction":"Run relevant validation"})).unwrap();
        assert_eq!(
            watchdog.observe(&call, false, "same", false),
            Observation::Unproductive
        );
    }

    #[test]
    fn changed_polling_is_evidence_but_identical_task_updates_are_not_progress() {
        let mut watchdog = Watchdog::default();
        let call = tool("terminal_output", 0);
        for index in 0..20 {
            assert_eq!(
                watchdog.observe(&call, false, &format!("output {index}"), false),
                Observation::NewEvidence
            );
        }
        let task = tool("update_tasks", 1);
        assert_eq!(
            watchdog.observe(&task, false, "in progress", false),
            Observation::MaterialProgress
        );
        assert_eq!(
            watchdog.observe(&task, false, "in progress", false),
            Observation::Unproductive
        );
        assert!(!material_tool(
            "jarvis_propose_publication",
            r#"{"approved":false,"status":"rejected"}"#
        ));
        assert!(material_tool(
            "jarvis_propose_publication",
            r#"{"approved":true,"status":"applied"}"#
        ));
    }

    #[test]
    fn error_streaks_can_recover_more_than_once_without_aborting_the_turn() {
        let mut watchdog = Watchdog::default();
        for cycle in 0..3 {
            for index in 0..UNPRODUCTIVE_STREAK {
                watchdog.observe(&tool("read", index), true, "invalid arguments", false);
            }
            assert!(matches!(
                watchdog.take_action(),
                Some(Action::SuggestCheckpoint(_))
            ));
            assert_eq!(
                watchdog.observe(
                    &tool("search", cycle),
                    false,
                    &format!("recovered-{cycle}"),
                    false
                ),
                Observation::NewEvidence
            );
        }
    }
}
