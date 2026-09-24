//! Typed context boundaries and immutable per-step settings.
use super::{compaction, AgentError, Session, SessionData, TurnOptions};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::sync::Arc;

#[cfg(test)]
mod allocation_probe;

const MAX_PROVIDER_CONTEXT_BYTES: usize = 8 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ItemKind {
    User,
    Runtime,
    Assistant,
    Reasoning,
    ToolCall,
    ToolResult,
    Unknown,
}

#[derive(Debug, Clone)]
pub(super) struct ContextItem {
    pub kind: ItemKind,
    value: Value,
}

impl ContextItem {
    fn classify(value: Value) -> Self {
        let kind = match value["type"].as_str() {
            Some("function_call") => ItemKind::ToolCall,
            Some("function_call_output") => ItemKind::ToolResult,
            Some("reasoning") => ItemKind::Reasoning,
            _ if value["role"] == "user" && value["_jarvis_runtime"] == true => ItemKind::Runtime,
            _ if value["role"] == "user" => ItemKind::User,
            _ if value["role"] == "assistant" || value["type"] == "message" => ItemKind::Assistant,
            _ => ItemKind::Unknown,
        };
        Self { kind, value }
    }

    fn into_provider_value(mut self) -> Value {
        if let Some(map) = self.value.as_object_mut() {
            map.retain(|key, _| !key.starts_with("_jarvis_"));
        }
        self.value
    }
}

#[derive(Debug, Clone)]
pub(super) struct UserAuthorization {
    pub values: Arc<[Value]>,
}

#[derive(Clone)]
pub(super) struct StepContext {
    id: String,
    options: Arc<TurnOptions>,
    instructions: Arc<str>,
    tools: Arc<[Value]>,
    input: Arc<[Value]>,
    input_bytes: u64,
    authorization: UserAuthorization,
    capabilities: Arc<super::provider::ModelCapabilities>,
}

impl StepContext {
    pub(super) fn capture(
        session: &Session,
        options: &TurnOptions,
        instructions: &str,
        tools: &[Value],
        capabilities: &Arc<super::provider::ModelCapabilities>,
    ) -> Result<Self, AgentError> {
        let data = session.data.lock().map_err(|_| AgentError::internal())?;
        Self::from_data(&data, options, instructions, tools, capabilities)
    }

    fn from_data(
        data: &SessionData,
        options: &TurnOptions,
        instructions: &str,
        tools: &[Value],
        capabilities: &Arc<super::provider::ModelCapabilities>,
    ) -> Result<Self, AgentError> {
        let authorized: Vec<Value> = data
            .turns
            .iter()
            .flat_map(|turn| turn.wire.iter())
            .filter(|value| value["role"] == "user" && value["_jarvis_runtime"] != true)
            .cloned()
            .collect();
        let typed: Vec<ContextItem> = compaction::input(data)
            .into_iter()
            .map(ContextItem::classify)
            .collect();
        let mut digest = Sha256::new();
        for item in &typed {
            digest.update(item.kind.label().as_bytes());
        }
        let input: Vec<Value> = typed
            .into_iter()
            .map(ContextItem::into_provider_value)
            .collect();
        if input.is_empty() || authorized.is_empty() {
            return Err(AgentError::new(
                "context_invalid",
                "O contexto perdeu a solicitação original do usuário. A execução foi interrompida antes de consultar o provedor.",
            ));
        }
        let encoded_input = serde_json::to_vec(&input).map_err(|_| AgentError::internal())?;
        if encoded_input.len() > MAX_PROVIDER_CONTEXT_BYTES {
            return Err(AgentError::new(
                "context_limit",
                "Esta conversa atingiu o limite de contexto local. Inicie uma nova conversa para continuar.",
            ));
        }
        digest.update(&encoded_input);
        digest.update(serde_json::to_vec(&authorized).map_err(|_| AgentError::internal())?);
        digest.update(instructions.as_bytes());
        digest.update(serde_json::to_vec(tools).map_err(|_| AgentError::internal())?);
        digest.update(serde_json::to_vec(options).map_err(|_| AgentError::internal())?);
        digest.update(serde_json::to_vec(capabilities).map_err(|_| AgentError::internal())?);
        let id = digest
            .finalize()
            .iter()
            .take(12)
            .map(|byte| format!("{byte:02x}"))
            .collect();
        Ok(Self {
            id,
            options: Arc::new(options.clone()),
            instructions: Arc::from(instructions),
            tools: Arc::from(tools.to_vec()),
            input: Arc::from(input),
            input_bytes: encoded_input.len() as u64,
            authorization: UserAuthorization {
                values: Arc::from(authorized),
            },
            capabilities: Arc::clone(capabilities),
        })
    }

    pub(super) fn id(&self) -> &str {
        &self.id
    }

    pub(super) fn options(&self) -> &TurnOptions {
        &self.options
    }

    pub(super) fn instructions(&self) -> &str {
        &self.instructions
    }

    pub(super) fn tools(&self) -> &[Value] {
        &self.tools
    }

    pub(super) fn input(&self) -> &[Value] {
        &self.input
    }

    pub(super) fn input_bytes(&self) -> u64 {
        self.input_bytes
    }

    pub(super) fn authorization(&self) -> &UserAuthorization {
        &self.authorization
    }

    pub(super) fn capabilities(&self) -> &super::provider::ModelCapabilities {
        &self.capabilities
    }
}

impl ItemKind {
    fn label(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Runtime => "runtime",
            Self::Assistant => "assistant",
            Self::Reasoning => "reasoning",
            Self::ToolCall => "tool_call",
            Self::ToolResult => "tool_result",
            Self::Unknown => "unknown",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::tests::{options, session, Fixture};
    use crate::agent::ApprovalMode;
    use serde_json::json;
    use std::sync::Arc;

    fn capabilities() -> Arc<super::super::provider::ModelCapabilities> {
        let credential = crate::openai_codex::CodexCredential::new(
            "access", "refresh", 1, "account", None, None,
        );
        let model = crate::openai_codex::ProviderModel {
            id: "model".into(),
            name: "Model".into(),
            reasoning_levels: vec!["medium".into()],
            default_reasoning_level: Some("medium".into()),
            context_window: Some(128_000),
        };
        Arc::new(super::super::provider::ModelCapabilities::resolve(
            &credential,
            &model,
        ))
    }

    #[test]
    fn captures_immutable_settings_and_keeps_original_user_authorization() {
        let fixture = Fixture::new();
        let session = session(&fixture);
        let mut options = options(ApprovalMode::Yolo);
        session
            .reserve("original request".into(), options.clone())
            .unwrap();
        let mut tools = vec![json!({"name":"read","parameters":{"type":"object"}})];
        let capabilities = capabilities();
        let step =
            StepContext::capture(&session, &options, "stable", &tools, &capabilities).unwrap();
        options.model = "changed".into();
        tools.clear();
        assert_eq!(step.options().model, "model");
        assert_eq!(step.tools().len(), 1);
        assert_eq!(step.authorization().values.len(), 1);
        assert_eq!(step.capabilities().context_window, Some(128_000));
        assert_eq!(
            step.authorization().values[0]["content"],
            "original request"
        );
        assert!(step
            .input()
            .iter()
            .all(|item| item.get("_jarvis_runtime").is_none()));
    }

    #[test]
    fn context_identity_changes_when_model_visible_input_changes() {
        let fixture = Fixture::new();
        let session = session(&fixture);
        let options = options(ApprovalMode::Yolo);
        session
            .reserve("original request".into(), options.clone())
            .unwrap();
        let capabilities = capabilities();
        let before =
            StepContext::capture(&session, &options, "stable", &[], &capabilities).unwrap();
        session
            .update(true, |data| {
                data.turns.last_mut().unwrap().wire.push(json!({
                    "role":"user",
                    "_jarvis_runtime":true,
                    "content":"new runtime evidence"
                }));
            })
            .unwrap();
        let after = StepContext::capture(&session, &options, "stable", &[], &capabilities).unwrap();
        assert_ne!(before.id(), after.id());
    }

    #[test]
    fn harness_evaluation_profiles_preparation_and_inspection_for_128_actions() {
        use std::{hint::black_box, time::Instant};

        let fixture = Fixture::new();
        let session = session(&fixture);
        let options = options(ApprovalMode::Yolo);
        session
            .reserve("Inspect the project".into(), options.clone())
            .unwrap();
        session.update(true, |data| {
            let wire = &mut data.turns.last_mut().unwrap().wire;
            for index in 0..128 {
                wire.push(json!({"type":"function_call", "call_id":format!("read-{index}"), "name":"read", "arguments":"{\"path\":\"fixture.txt\"}"}));
                wire.push(json!({"type":"function_call_output", "call_id":format!("read-{index}"), "output":"synthetic content\n".repeat(128)}));
            }
        }).unwrap();
        let capabilities = capabilities();
        let mut timings = [Vec::new(), Vec::new(), Vec::new()];
        let mut allocations = [Vec::new(), Vec::new(), Vec::new()];
        let mut bytes = [Vec::new(), Vec::new(), Vec::new()];
        for _ in 0..25 {
            let started = Instant::now();
            let (step, preparation) = allocation_probe::measure(|| {
                StepContext::capture(&session, &options, "stable", &[], &capabilities).unwrap()
            });
            timings[0].push(started.elapsed().as_nanos() as u64);
            // Reproduce the old telemetry inspection's two input clones and
            // serialization, not an invented baseline of the entire runtime.
            let started = Instant::now();
            let (legacy, copies) = allocation_probe::measure(|| {
                let items = black_box(step.input().to_vec()).len();
                let size = serde_json::to_vec(&black_box(step.input().to_vec()))
                    .unwrap()
                    .len() as u64;
                black_box((items, size))
            });
            timings[1].push(started.elapsed().as_nanos() as u64);
            let started = Instant::now();
            let (current, borrowed) =
                allocation_probe::measure(|| black_box((step.input().len(), step.input_bytes())));
            timings[2].push(started.elapsed().as_nanos() as u64);
            assert_eq!(legacy, current);
            assert!(current.0 > 256);
            assert!(copies.allocations > 128);
            assert_eq!(borrowed.allocations, 0);
            for (index, counts) in [preparation, copies, borrowed].into_iter().enumerate() {
                allocations[index].push(counts.allocations);
                bytes[index].push(counts.bytes);
            }
            let replay = journal_input(&session);
            assert_eq!(step.input(), replay.as_slice());
        }
        let stats = |samples: &mut Vec<u64>| {
            samples.sort_unstable();
            json!({"samples":samples.len(), "p50":samples[12], "p95":samples[23]})
        };
        for (index, phase) in [
            "current_preparation",
            "legacy_inspection",
            "borrowed_inspection",
        ]
        .iter()
        .enumerate()
        {
            println!(
                "{}",
                json!({"phase":phase, "actions":128, "elapsedNs":stats(&mut timings[index]), "allocations":stats(&mut allocations[index]), "allocatedBytes":stats(&mut bytes[index])})
            );
        }
    }

    fn journal_input(session: &Session) -> Vec<Value> {
        let (turns, _) = super::super::journal::load_all(&session.journal).unwrap();
        turns.into_iter().flat_map(|turn| turn.wire).collect()
    }
}
