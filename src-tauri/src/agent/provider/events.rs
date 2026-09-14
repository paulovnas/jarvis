//! Provider-neutral output events produced before the agent loop can dispatch tools.

use super::{protocol_error, AgentError, ToolCall, Usage};
use serde_json::Value;
use std::collections::HashSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReplayKind {
    None,
    CodexEncrypted,
    Antigravity,
    Custom,
}

/// A typed semantic event. The original wire value remains private Rust data so
/// opaque signatures can be replayed without being flattened or exposed over
/// IPC.
#[derive(Debug, Clone)]
enum ProviderOutputEvent {
    Message {
        wire: Value,
        text: String,
    },
    Reasoning {
        wire: Value,
        summary: String,
        replay: ReplayKind,
    },
    ToolCall {
        wire: Value,
        call: ToolCall,
    },
    WebSearch {
        wire: Value,
    },
}

impl ProviderOutputEvent {
    fn parse(wire: Value) -> Result<Self, AgentError> {
        match wire["type"].as_str() {
            Some("message") => {
                if wire["role"]
                    .as_str()
                    .is_some_and(|role| role != "assistant")
                {
                    return Err(protocol_error());
                }
                let content = wire["content"].as_array().ok_or_else(protocol_error)?;
                let mut text = String::new();
                for part in content {
                    match part["type"].as_str() {
                        Some("output_text" | "text") => {
                            text.push_str(part["text"].as_str().ok_or_else(protocol_error)?);
                        }
                        Some("refusal") => {
                            text.push_str(part["refusal"].as_str().ok_or_else(protocol_error)?);
                        }
                        _ => return Err(protocol_error()),
                    }
                }
                Ok(Self::Message { wire, text })
            }
            Some("reasoning") => {
                let summary = wire["summary"]
                    .as_array()
                    .map(|parts| {
                        parts
                            .iter()
                            .filter_map(|part| part["text"].as_str())
                            .collect::<Vec<_>>()
                            .join("\n\n")
                    })
                    .unwrap_or_default();
                let has_codex = wire["encrypted_content"].is_string();
                let has_antigravity_model = wire["_antigravity_model"].is_string();
                let has_antigravity_part = wire["_antigravity_part"].is_object();
                if has_antigravity_model != has_antigravity_part {
                    return Err(protocol_error());
                }
                if wire.get("_custom").is_some_and(|value| !value.is_object()) {
                    return Err(protocol_error());
                }
                let replay = if has_codex {
                    ReplayKind::CodexEncrypted
                } else if has_antigravity_model {
                    ReplayKind::Antigravity
                } else if wire["_custom"].is_object() {
                    ReplayKind::Custom
                } else {
                    ReplayKind::None
                };
                Ok(Self::Reasoning {
                    wire,
                    summary,
                    replay,
                })
            }
            Some("function_call") => {
                let call = parse_tool_call(&wire)?;
                Ok(Self::ToolCall { wire, call })
            }
            Some("web_search_call") if wire.is_object() => Ok(Self::WebSearch { wire }),
            _ => Err(protocol_error()),
        }
    }

    fn wire(&self) -> &Value {
        match self {
            Self::Message { wire, .. }
            | Self::Reasoning { wire, .. }
            | Self::ToolCall { wire, .. }
            | Self::WebSearch { wire } => wire,
        }
    }
}

#[derive(Debug)]
pub(super) struct NormalizedOutput {
    events: Vec<ProviderOutputEvent>,
    pub text: String,
    pub summary: String,
    pub calls: Vec<ToolCall>,
    pub usage: Option<Usage>,
}

impl NormalizedOutput {
    pub(super) fn parse(output: Vec<Value>, usage: Option<Usage>) -> Result<Self, AgentError> {
        let mut events = Vec::with_capacity(output.len());
        let mut text = String::new();
        let mut summaries = Vec::new();
        let mut calls = Vec::new();
        let mut ids = HashSet::new();
        for wire in output {
            let event = ProviderOutputEvent::parse(wire)?;
            match &event {
                ProviderOutputEvent::Message { text: delta, .. } => text.push_str(delta),
                ProviderOutputEvent::Reasoning {
                    summary, replay, ..
                } => {
                    // Reading the replay tag here makes the provider boundary
                    // explicit while the opaque payload stays untouched.
                    let _provider_scoped = *replay != ReplayKind::None;
                    if !summary.is_empty() {
                        summaries.push(summary.clone());
                    }
                }
                ProviderOutputEvent::ToolCall { call, .. } => {
                    if calls.len() >= 16 || !ids.insert(call.id.clone()) {
                        return Err(protocol_error());
                    }
                    calls.push(call.clone());
                }
                ProviderOutputEvent::WebSearch { .. } => {}
            }
            events.push(event);
        }
        if text.is_empty() && calls.is_empty() {
            return Err(protocol_error());
        }
        Ok(Self {
            events,
            text,
            summary: summaries.join("\n\n"),
            calls,
            usage,
        })
    }

    pub(super) fn wire_output(&self) -> Vec<Value> {
        self.events
            .iter()
            .map(ProviderOutputEvent::wire)
            .cloned()
            .collect()
    }
}

pub(super) fn parse_tool_call(item: &Value) -> Result<ToolCall, AgentError> {
    let id = item["call_id"]
        .as_str()
        .filter(|id| !id.is_empty() && id.len() <= 200)
        .ok_or_else(protocol_error)?;
    let name = item["name"]
        .as_str()
        .filter(|name| !name.is_empty() && name.len() <= 200)
        .ok_or_else(protocol_error)?;
    let parsed =
        serde_json::from_str::<Value>(item["arguments"].as_str().ok_or_else(protocol_error)?);
    let (args, argument_error) = match parsed {
        Ok(args) if args.is_object() => (args, None),
        Ok(_) => (
            serde_json::json!({}),
            Some("Os argumentos devem ser um objeto JSON.".to_owned()),
        ),
        Err(error) => (
            serde_json::json!({}),
            Some(format!(
                "JSON inválido na linha {}, coluna {}. Corrija a sintaxe dos argumentos.",
                error.line(),
                error.column()
            )),
        ),
    };
    Ok(ToolCall {
        id: id.into(),
        name: name.into(),
        args,
        status: if argument_error.is_some() {
            "error"
        } else {
            "pending"
        }
        .into(),
        output: argument_error.unwrap_or_default(),
        duration_ms: 0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn preserves_order_ids_usage_and_opaque_replay() {
        let usage = Usage {
            input_tokens: 10,
            output_tokens: 4,
            cache_read_tokens: Some(7),
            cache_write_tokens: None,
        };
        let wire = vec![
            json!({"type":"reasoning","summary":[{"text":"Inspect"}],"encrypted_content":"opaque"}),
            json!({"type":"message","role":"assistant","content":[{"type":"output_text","text":"Done"}]}),
            json!({"type":"function_call","call_id":"call-1","name":"read","arguments":"{\"path\":\"README.md\"}"}),
        ];
        let normalized = NormalizedOutput::parse(wire.clone(), Some(usage)).unwrap();
        assert_eq!(normalized.wire_output(), wire);
        assert_eq!(normalized.text, "Done");
        assert_eq!(normalized.summary, "Inspect");
        assert_eq!(normalized.calls[0].id, "call-1");
        assert_eq!(normalized.calls[0].args["path"], "README.md");
        assert_eq!(normalized.usage.unwrap().cache_read_tokens, Some(7));
    }

    #[test]
    fn rejects_duplicate_calls_and_partial_provider_replay_envelopes() {
        let duplicate =
            json!({"type":"function_call","call_id":"same","name":"read","arguments":"{}"});
        assert!(NormalizedOutput::parse(vec![duplicate.clone(), duplicate], None).is_err());
        assert!(NormalizedOutput::parse(
            vec![
                json!({
                    "type":"reasoning",
                    "summary":[],
                    "_antigravity_model":"gemini"
                }),
                json!({"type":"message","content":[{"type":"output_text","text":"Done"}]})
            ],
            None
        )
        .is_err());
    }
}
