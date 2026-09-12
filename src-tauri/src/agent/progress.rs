use super::{AgentError, ToolCall};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::{HashSet, VecDeque};

pub(super) const TOOL_NAME: &str = "progress_checkpoint";

const MAX_ACTIONS_WITHOUT_MATERIAL_PROGRESS: usize = 64;
const MAX_UNPRODUCTIVE_STREAK: usize = 8;
const ERROR_WINDOW: usize = 12;
const MAX_ERRORS_IN_WINDOW: usize = 8;
const MAX_CHECKPOINT_VIOLATIONS: usize = 3;

const CHECKPOINT_REQUIRED: &str = "Jarvis progress watchdog checkpoint (runtime instruction, not a new user request). Work has continued without observable material progress. Before any other tool, call progress_checkpoint with the current objective, concrete evidence gathered so far, and one bounded next action. Do not claim progress from plans, repeated polling, or an unconfirmed mutation.";
const CHECKPOINT_ACCEPTED: &str = "Checkpoint registrado. Execute a próxima ação delimitada. O Jarvis continuará enquanto houver fatos novos ou progresso material confirmado.";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Observation {
    MaterialProgress,
    NewEvidence,
    Unproductive,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Action {
    RequireCheckpoint(String),
    Pause(String),
}

#[derive(Clone, Debug)]
enum Phase {
    Monitoring {
        checkpointed: bool,
    },
    CheckpointRequired {
        reason: &'static str,
        announced: bool,
        violations: usize,
    },
    Paused {
        reason: &'static str,
        announced: bool,
    },
}

impl Default for Phase {
    fn default() -> Self {
        Self::Monitoring {
            checkpointed: false,
        }
    }
}

#[derive(Default)]
pub(super) struct Watchdog {
    phase: Phase,
    actions_since_progress: usize,
    unproductive_streak: usize,
    recent_errors: VecDeque<bool>,
    evidence: HashSet<u64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Checkpoint {
    objective: String,
    evidence: Vec<String>,
    next_action: String,
}

impl Watchdog {
    pub(super) fn checkpoint_required(&self) -> bool {
        matches!(self.phase, Phase::CheckpointRequired { .. })
    }

    pub(super) fn definition(&self) -> Option<Value> {
        matches!(self.phase, Phase::CheckpointRequired { .. }).then(|| {
            json!({
                "type": "function",
                "name": TOOL_NAME,
                "description": "Required by the Jarvis runtime after progress stalls. Restate the active objective, cite concrete evidence already obtained, and commit to one bounded next action. This checkpoint does not itself complete work.",
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
            })
        })
    }

    pub(super) fn preflight(&self, tool: &ToolCall) -> Result<(), AgentError> {
        match (&self.phase, tool.name.as_str()) {
            (Phase::CheckpointRequired { .. }, TOOL_NAME) => Ok(()),
            (Phase::CheckpointRequired { .. }, _) => Err(AgentError::new(
                "progress_checkpoint_required",
                "O watchdog exige um checkpoint de objetivo e evidências antes de executar outra ferramenta.",
            )),
            (_, TOOL_NAME) => Err(AgentError::new(
                "tool_unavailable",
                "Nenhum checkpoint de progresso está pendente.",
            )),
            _ => Ok(()),
        }
    }

    pub(super) fn checkpoint(&mut self, args: &Value) -> Result<String, AgentError> {
        if !matches!(self.phase, Phase::CheckpointRequired { .. }) {
            return Err(AgentError::new(
                "tool_unavailable",
                "Nenhum checkpoint de progresso está pendente.",
            ));
        }
        let checkpoint: Checkpoint = serde_json::from_value(args.clone()).map_err(|_| {
            AgentError::new(
                "progress_checkpoint_invalid",
                "Informe objective, evidence e nextAction no formato solicitado.",
            )
        })?;
        let valid = bounded(&checkpoint.objective)
            && bounded(&checkpoint.next_action)
            && (1..=12).contains(&checkpoint.evidence.len())
            && checkpoint.evidence.iter().all(|item| bounded(item));
        if !valid {
            return Err(AgentError::new(
                "progress_checkpoint_invalid",
                "O checkpoint precisa conter um objetivo, de 1 a 12 evidências concretas e uma próxima ação delimitada.",
            ));
        }
        self.phase = Phase::Monitoring { checkpointed: true };
        self.reset_counters();
        Ok(CHECKPOINT_ACCEPTED.into())
    }

    pub(super) fn observe(
        &mut self,
        tool: &ToolCall,
        failed: bool,
        output: &str,
        confirmed_mutation: bool,
    ) -> Observation {
        if tool.name == TOOL_NAME {
            if failed {
                self.checkpoint_violation();
            }
            return Observation::Unproductive;
        }
        if matches!(self.phase, Phase::CheckpointRequired { .. }) {
            self.checkpoint_violation();
            return Observation::Unproductive;
        }
        if failed {
            self.actions_since_progress += 1;
            self.unproductive_streak += 1;
            self.record_error(true);
            self.detect_stagnation();
            return Observation::Unproductive;
        }
        if confirmed_mutation || material_tool(&tool.name) {
            self.phase = Phase::Monitoring {
                checkpointed: false,
            };
            self.reset_counters();
            return Observation::MaterialProgress;
        }

        self.actions_since_progress += 1;
        self.record_error(false);
        let observation = if polling_tool(&tool.name) || output.trim().is_empty() {
            self.unproductive_streak += 1;
            Observation::Unproductive
        } else if self.evidence.insert(fingerprint(tool, output)) {
            self.unproductive_streak = 0;
            Observation::NewEvidence
        } else {
            self.unproductive_streak += 1;
            Observation::Unproductive
        };
        self.detect_stagnation();
        observation
    }

    pub(super) fn missed_checkpoint(&mut self) {
        self.checkpoint_violation();
    }

    pub(super) fn take_action(&mut self) -> Option<Action> {
        match &mut self.phase {
            Phase::CheckpointRequired {
                reason, announced, ..
            } if !*announced => {
                *announced = true;
                Some(Action::RequireCheckpoint(format!(
                    "{CHECKPOINT_REQUIRED}\nTrigger: {reason}"
                )))
            }
            Phase::Paused { reason, announced } if !*announced => {
                *announced = true;
                Some(Action::Pause(format!(
                    "A execução foi pausada de forma segura após uma segunda estagnação ({reason}). O histórico e os resultados de ferramentas foram preservados.{}",
                    " Em fluxos diretos, envie uma nova mensagem para continuar; em fluxos Planejado ou Completo, use Retomar fluxo."
                )))
            }
            _ => None,
        }
    }

    fn detect_stagnation(&mut self) {
        let reason = if self.unproductive_streak >= MAX_UNPRODUCTIVE_STREAK {
            Some("ações consecutivas sem fato novo ou mudança confirmada")
        } else if self.recent_errors.len() >= MAX_ERRORS_IN_WINDOW
            && self.recent_errors.iter().filter(|failed| **failed).count() >= MAX_ERRORS_IN_WINDOW
        {
            Some("taxa de erro elevada nas ações recentes")
        } else if self.actions_since_progress >= MAX_ACTIONS_WITHOUT_MATERIAL_PROGRESS {
            Some("muitas ações exploratórias sem progresso material")
        } else {
            None
        };
        let Some(reason) = reason else {
            return;
        };
        self.phase = match self.phase {
            Phase::Monitoring {
                checkpointed: false,
            } => Phase::CheckpointRequired {
                reason,
                announced: false,
                violations: 0,
            },
            Phase::Monitoring { checkpointed: true } => Phase::Paused {
                reason,
                announced: false,
            },
            ref phase => phase.clone(),
        };
    }

    fn checkpoint_violation(&mut self) {
        if let Phase::CheckpointRequired {
            reason, violations, ..
        } = &mut self.phase
        {
            *violations += 1;
            if *violations >= MAX_CHECKPOINT_VIOLATIONS {
                self.phase = Phase::Paused {
                    reason,
                    announced: false,
                };
            }
        }
    }

    fn record_error(&mut self, failed: bool) {
        if self.recent_errors.len() == ERROR_WINDOW {
            self.recent_errors.pop_front();
        }
        self.recent_errors.push_back(failed);
    }

    fn reset_counters(&mut self) {
        self.actions_since_progress = 0;
        self.unproductive_streak = 0;
        self.recent_errors.clear();
        self.evidence.clear();
    }
}

fn bounded(value: &str) -> bool {
    let value = value.trim();
    !value.is_empty() && value.len() <= 2_000
}

fn material_tool(name: &str) -> bool {
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
            | "jarvis_propose_agent"
            | "jarvis_propose_flow"
    )
}

fn polling_tool(name: &str) -> bool {
    matches!(
        name,
        "process_list"
            | "process_output"
            | "terminal_list"
            | "terminal_output"
            | "hub_list"
            | "hub_wait"
            | "browser_snapshot"
            | "browser_console"
    )
}

fn fingerprint(tool: &ToolCall, output: &str) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325;
    hash_bytes(&mut hash, tool.name.as_bytes());
    hash_value(&mut hash, &tool.args);
    hash_bytes(&mut hash, output.as_bytes());
    hash
}

fn hash_value(hash: &mut u64, value: &Value) {
    match value {
        Value::Object(object) => {
            let mut fields: Vec<_> = object.iter().collect();
            fields.sort_by_key(|(name, _)| *name);
            for (name, value) in fields {
                hash_bytes(hash, name.as_bytes());
                hash_value(hash, value);
            }
        }
        Value::Array(values) => {
            for value in values {
                hash_value(hash, value);
            }
        }
        _ => hash_bytes(hash, value.to_string().as_bytes()),
    }
}

fn hash_bytes(hash: &mut u64, bytes: &[u8]) {
    for byte in bytes {
        *hash = (*hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3);
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

    fn checkpoint(watchdog: &mut Watchdog) {
        watchdog
            .checkpoint(&json!({
                "objective":"Implementar a correção",
                "evidence":["O arquivo relevante foi localizado"],
                "nextAction":"Editar o arquivo identificado"
            }))
            .unwrap();
    }

    #[test]
    fn harness_evaluation_long_work_with_material_progress_never_hits_a_step_limit() {
        let mut watchdog = Watchdog::default();
        let mut evidence = 0;
        let mut progress = 0;
        for cycle in 0..5 {
            for index in 0..63 {
                let call = tool("read", cycle * 100 + index);
                assert_eq!(
                    watchdog.observe(&call, false, &format!("fact-{cycle}-{index}"), false),
                    Observation::NewEvidence
                );
                evidence += 1;
                assert!(watchdog.take_action().is_none());
            }
            let write = tool("write", cycle);
            assert_eq!(
                watchdog.observe(&write, false, "Arquivo salvo", true),
                Observation::MaterialProgress
            );
            progress += 1;
            assert!(watchdog.take_action().is_none());
        }
        crate::agent::evaluation::assert_runtime_report(
            "progress-long-run",
            crate::agent::evaluation::RuntimeReport::new(
                "completed",
                [
                    ("checkpoints", 0),
                    ("evidenceEvents", evidence),
                    ("progressEvents", progress),
                    ("toolCalls", evidence + progress),
                ],
            ),
        );
    }

    #[test]
    fn harness_evaluation_first_stagnation_requires_a_structured_checkpoint() {
        let mut watchdog = Watchdog::default();
        for index in 0..MAX_ACTIONS_WITHOUT_MATERIAL_PROGRESS {
            let call = tool("read", index);
            watchdog.observe(&call, false, &format!("fact-{index}"), false);
        }
        assert!(matches!(
            watchdog.take_action(),
            Some(Action::RequireCheckpoint(_))
        ));
        assert!(watchdog.definition().is_some());
        assert_eq!(
            watchdog.preflight(&tool("write", 1)).unwrap_err().code,
            "progress_checkpoint_required"
        );
        checkpoint(&mut watchdog);
        assert!(watchdog.definition().is_none());
        crate::agent::evaluation::assert_runtime_report(
            "progress-first-checkpoint",
            crate::agent::evaluation::RuntimeReport::new(
                "checkpointed",
                [
                    ("checkpoints", 1),
                    ("evidenceEvents", 64),
                    ("steps", 65),
                    ("toolCalls", 65),
                ],
            ),
        );
    }

    #[test]
    fn harness_evaluation_second_stagnation_pauses_without_replaying_tools() {
        let mut watchdog = Watchdog::default();
        for index in 0..MAX_ACTIONS_WITHOUT_MATERIAL_PROGRESS {
            let call = tool("read", index);
            watchdog.observe(&call, false, &format!("first-{index}"), false);
        }
        assert!(matches!(
            watchdog.take_action(),
            Some(Action::RequireCheckpoint(_))
        ));
        checkpoint(&mut watchdog);
        for index in 0..MAX_ACTIONS_WITHOUT_MATERIAL_PROGRESS {
            let call = tool("search", index + 1000);
            watchdog.observe(&call, false, &format!("second-{index}"), false);
        }
        assert!(matches!(watchdog.take_action(), Some(Action::Pause(_))));
        assert!(watchdog.take_action().is_none());
        crate::agent::evaluation::assert_runtime_report(
            "progress-second-stagnation",
            crate::agent::evaluation::RuntimeReport::new(
                "paused",
                [
                    ("checkpoints", 1),
                    ("evidenceEvents", 128),
                    ("pausedCalls", 0),
                    ("steps", 129),
                    ("toolCalls", 129),
                ],
            ),
        );
    }

    #[test]
    fn high_error_rate_triggers_before_unique_failures_can_run_forever() {
        let mut watchdog = Watchdog::default();
        for index in 0..MAX_ERRORS_IN_WINDOW {
            watchdog.observe(&tool("mcp_query", index), true, "failed", false);
        }
        assert!(matches!(
            watchdog.take_action(),
            Some(Action::RequireCheckpoint(message)) if message.contains("taxa de erro") || message.contains("ações consecutivas")
        ));
    }

    #[test]
    fn evidence_identity_ignores_json_object_key_order() {
        let mut watchdog = Watchdog::default();
        let mut first = tool("search", 1);
        first.args = json!({"query":"needle","path":"src"});
        let mut reordered = tool("search", 2);
        reordered.args = json!({"path":"src","query":"needle"});
        assert_eq!(
            watchdog.observe(&first, false, "same result", false),
            Observation::NewEvidence
        );
        assert_eq!(
            watchdog.observe(&reordered, false, "same result", false),
            Observation::Unproductive
        );
    }

    #[test]
    fn three_ignored_checkpoint_requests_pause_the_execution() {
        let mut watchdog = Watchdog::default();
        for index in 0..MAX_UNPRODUCTIVE_STREAK {
            watchdog.observe(&tool("process_output", index), false, "same", false);
        }
        assert!(matches!(
            watchdog.take_action(),
            Some(Action::RequireCheckpoint(_))
        ));
        for _ in 0..MAX_CHECKPOINT_VIOLATIONS {
            watchdog.missed_checkpoint();
        }
        assert!(matches!(watchdog.take_action(), Some(Action::Pause(_))));
    }
}
