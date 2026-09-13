//! Typed context boundaries and immutable per-step settings.
use super::{compaction, AgentError, Session, SessionData, TurnOptions};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::sync::Arc;

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
    authorization: UserAuthorization,
}

impl StepContext {
    pub(super) fn capture(
        session: &Session,
        options: &TurnOptions,
        instructions: &str,
        tools: &[Value],
    ) -> Result<Self, AgentError> {
        let data = session.data.lock().map_err(|_| AgentError::internal())?;
        Self::from_data(&data, options, instructions, tools)
    }

    fn from_data(
        data: &SessionData,
        options: &TurnOptions,
        instructions: &str,
        tools: &[Value],
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
            digest.update(serde_json::to_vec(&item.value).map_err(|_| AgentError::internal())?);
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
            authorization: UserAuthorization {
                values: Arc::from(authorized),
            },
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

    pub(super) fn input(&self) -> Vec<Value> {
        self.input.to_vec()
    }

    pub(super) fn authorization(&self) -> &UserAuthorization {
        &self.authorization
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

    #[test]
    fn captures_immutable_settings_and_keeps_original_user_authorization() {
        let fixture = Fixture::new();
        let session = session(&fixture);
        let mut options = options(ApprovalMode::Yolo);
        session
            .reserve("original request".into(), options.clone())
            .unwrap();
        let mut tools = vec![json!({"name":"read","parameters":{"type":"object"}})];
        let step = StepContext::capture(&session, &options, "stable", &tools).unwrap();
        options.model = "changed".into();
        tools.clear();
        assert_eq!(step.options().model, "model");
        assert_eq!(step.tools().len(), 1);
        assert_eq!(step.authorization().values.len(), 1);
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
        let before = StepContext::capture(&session, &options, "stable", &[]).unwrap();
        session
            .update(true, |data| {
                data.turns.last_mut().unwrap().wire.push(json!({
                    "role":"user",
                    "_jarvis_runtime":true,
                    "content":"new runtime evidence"
                }));
            })
            .unwrap();
        let after = StepContext::capture(&session, &options, "stable", &[]).unwrap();
        assert_ne!(before.id(), after.id());
    }
}
