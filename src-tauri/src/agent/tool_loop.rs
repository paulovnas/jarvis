use super::{AgentError, ToolCall};
use serde_json::Value;

const STEER_AFTER: usize = 5;

#[derive(Clone, Debug, PartialEq, Eq)]
struct CallKey {
    name: String,
    arguments: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Observation {
    call: CallKey,
    result: ResultClass,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ResultClass {
    Error,
    Success(u64),
}

#[derive(Default)]
pub(super) struct Guard {
    last: Option<Observation>,
    consecutive: usize,
    steered: Option<CallKey>,
}

impl Guard {
    pub(super) fn before_call(&self, tool: &ToolCall) -> Result<(), AgentError> {
        if exempt(&tool.name) {
            return Ok(());
        }
        let call = call_key(tool);
        if self.steered.as_ref() == Some(&call) {
            return Err(AgentError::new(
                "repeated_tool_loop",
                "O agente repetiu a mesma chamada após receber uma orientação para mudar de estratégia. A execução foi interrompida antes de executar a ação novamente.",
            ));
        }
        Ok(())
    }

    pub(super) fn observe(
        &mut self,
        tool: &ToolCall,
        failed: bool,
        output: &str,
    ) -> Option<String> {
        if exempt(&tool.name) {
            self.reset();
            return None;
        }
        let observation = Observation {
            call: call_key(tool),
            result: if failed {
                ResultClass::Error
            } else {
                ResultClass::Success(stable_hash(output.as_bytes()))
            },
        };
        if self.last.as_ref() == Some(&observation) {
            self.consecutive += 1;
        } else {
            self.last = Some(observation.clone());
            self.consecutive = 1;
            self.steered = None;
        }
        if self.consecutive != STEER_AFTER {
            return None;
        }
        self.steered = Some(observation.call);
        Some(format!(
            "Jarvis detected {STEER_AFTER} consecutive identical calls to `{}` with the same result class. Do not call it again with the same arguments. Explain what is blocking progress and choose a different source, query, file, tool, or approach. If user input is required, use ask_user.",
            tool.name
        ))
    }

    fn reset(&mut self) {
        self.last = None;
        self.consecutive = 0;
        self.steered = None;
    }
}

fn call_key(tool: &ToolCall) -> CallKey {
    CallKey {
        name: tool.name.clone(),
        arguments: canonical_json(&tool.args),
    }
}

fn canonical_json(value: &Value) -> String {
    match value {
        Value::Object(object) => {
            let mut entries: Vec<_> = object.iter().collect();
            entries.sort_by_key(|(left, _)| *left);
            let fields = entries
                .into_iter()
                .map(|(key, value)| {
                    format!(
                        "{}:{}",
                        serde_json::to_string(key).unwrap_or_default(),
                        canonical_json(value)
                    )
                })
                .collect::<Vec<_>>()
                .join(",");
            format!("{{{fields}}}")
        }
        Value::Array(array) => format!(
            "[{}]",
            array
                .iter()
                .map(canonical_json)
                .collect::<Vec<_>>()
                .join(",")
        ),
        _ => serde_json::to_string(value).unwrap_or_default(),
    }
}

fn stable_hash(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

fn exempt(name: &str) -> bool {
    matches!(
        name,
        "ask_user"
            | "process_list"
            | "process_output"
            | "process_check_port"
            | "terminal_output"
            | "browser_snapshot"
            | "browser_console"
            | "hub_wait"
            | "hub_list"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn tool(name: &str, args: Value) -> ToolCall {
        ToolCall {
            id: "call".into(),
            name: name.into(),
            args,
            status: "completed".into(),
            output: String::new(),
            duration_ms: 0,
        }
    }

    #[test]
    fn fifth_identical_result_steers_and_next_identical_call_is_stopped() {
        let mut guard = Guard::default();
        let call = tool("read", json!({"path":"src/app.ts"}));
        for _ in 0..4 {
            assert!(guard.observe(&call, false, "same").is_none());
            assert!(guard.before_call(&call).is_ok());
        }
        assert!(guard
            .observe(&call, false, "same")
            .is_some_and(|message| message.contains("5 consecutive identical calls")));
        assert_eq!(
            guard.before_call(&call).unwrap_err().code,
            "repeated_tool_loop"
        );
    }

    #[test]
    fn canonical_arguments_ignore_object_key_order_and_changed_results_reset_sequence() {
        let mut guard = Guard::default();
        let first = tool("search", json!({"query":"needle","path":"src"}));
        let reordered = tool("search", json!({"path":"src","query":"needle"}));
        assert!(guard.observe(&first, false, "one").is_none());
        assert!(guard.observe(&reordered, false, "one").is_none());
        assert_eq!(guard.consecutive, 2);
        assert!(guard.observe(&reordered, false, "two").is_none());
        assert_eq!(guard.consecutive, 1);
    }

    #[test]
    fn polling_tools_never_accumulate_repetition() {
        let mut guard = Guard::default();
        let call = tool("process_output", json!({"id":"server"}));
        for _ in 0..20 {
            assert!(guard.observe(&call, false, "unchanged").is_none());
            assert!(guard.before_call(&call).is_ok());
        }
        assert_eq!(guard.consecutive, 0);
    }

    #[test]
    fn repeated_errors_share_a_result_class_even_when_messages_change() {
        let mut guard = Guard::default();
        let call = tool("web_search", json!({"query":"status"}));
        for index in 0..4 {
            assert!(guard
                .observe(&call, true, &format!("request {index} failed"))
                .is_none());
        }
        assert!(guard.observe(&call, true, "request 5 failed").is_some());
    }
}
