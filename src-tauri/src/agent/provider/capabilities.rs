//! Model behavior resolved once at the provider boundary and frozen per step.
//!
//! Provider protocols do not share one request contract. Keeping their known
//! behavior in a single value prevents request builders and the agent loop from
//! independently guessing support from a model name.

use crate::openai_codex::{
    custom::{Protocol, Reasoning},
    CodexCredential, ProviderModel,
};
use serde::Serialize;
use serde_json::Value;

const EFFECTIVE_CONTEXT_PERCENT: u64 = 80;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ProviderFamily {
    OpenAiCodex,
    Antigravity,
    Custom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum WireProtocol {
    OpenAiResponses,
    OpenAiCompletions,
    AnthropicMessages,
    CloudCodeAssist,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub(crate) struct Modalities {
    pub(crate) text: bool,
    pub(crate) image: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub(crate) struct ReasoningCapabilities {
    pub(crate) supported: bool,
    pub(crate) summaries: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub(crate) struct CompactionCapabilities {
    pub(crate) local: bool,
    pub(crate) remote: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub(crate) struct ReplayRequirements {
    /// The provider can return opaque state that must stay scoped to the same
    /// provider, model and protocol when replayed.
    pub(crate) opaque_state: bool,
    /// Thinking blocks require a provider signature unless the custom gateway
    /// was explicitly configured to accept unsigned blocks.
    pub(crate) signed_thinking: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct ModelCapabilities {
    pub(crate) family: ProviderFamily,
    pub(crate) protocol: WireProtocol,
    pub(crate) input: Modalities,
    pub(crate) output: Modalities,
    pub(crate) tools: bool,
    pub(crate) parallel_tool_calls: bool,
    pub(crate) reasoning: ReasoningCapabilities,
    pub(crate) context_window: Option<u64>,
    pub(crate) effective_context_window: Option<u64>,
    pub(crate) compaction: CompactionCapabilities,
    pub(crate) replay: ReplayRequirements,
}

impl ModelCapabilities {
    /// Resolve provider facts once. Unknown/custom behavior stays conservative:
    /// optional request fields are disabled until configuration proves support.
    pub(crate) fn resolve(credential: &CodexCredential, model: &ProviderModel) -> Self {
        let context_window = model.context_window;
        let effective_context_window = context_window.map(|window| {
            (window.saturating_mul(EFFECTIVE_CONTEXT_PERCENT) / 100)
                .max(1)
                .min(window)
        });
        let compaction = CompactionCapabilities {
            local: true,
            remote: false,
        };

        if let Some(config) = &credential.custom {
            let configured = config.models.iter().find(|item| item.id == model.id);
            let supports_tools = configured.is_some_and(|item| item.supports_tools);
            let supports_images = configured.is_some_and(|item| item.supports_images);
            let reasoning = configured.map_or(Reasoning::None, |item| item.reasoning);
            let protocol = match config.protocol {
                Protocol::OpenaiResponses => WireProtocol::OpenAiResponses,
                Protocol::OpenaiCompletions => WireProtocol::OpenAiCompletions,
                Protocol::AnthropicMessages => WireProtocol::AnthropicMessages,
            };
            return Self {
                family: ProviderFamily::Custom,
                protocol,
                input: Modalities {
                    text: true,
                    image: supports_images,
                },
                output: Modalities {
                    text: true,
                    image: false,
                },
                tools: supports_tools,
                // The custom-provider schema does not currently let the user
                // assert this capability, so do not send the optional flag.
                parallel_tool_calls: false,
                reasoning: ReasoningCapabilities {
                    supported: reasoning != Reasoning::None,
                    summaries: reasoning != Reasoning::None,
                },
                context_window,
                effective_context_window,
                compaction,
                replay: ReplayRequirements {
                    opaque_state: reasoning != Reasoning::None,
                    signed_thinking: config.protocol == Protocol::AnthropicMessages
                        && !config.replay_unsigned_thinking,
                },
            };
        }

        if credential.project_id.is_some() {
            let metadata = credential
                .antigravity_models
                .get(&model.id)
                .cloned()
                .unwrap_or_default();
            let reasoning =
                metadata["supportsThinking"] == true || !model.reasoning_levels.is_empty();
            return Self {
                family: ProviderFamily::Antigravity,
                protocol: WireProtocol::CloudCodeAssist,
                input: Modalities {
                    text: true,
                    image: true,
                },
                output: Modalities {
                    text: true,
                    image: false,
                },
                tools: true,
                parallel_tool_calls: true,
                reasoning: ReasoningCapabilities {
                    supported: reasoning,
                    summaries: reasoning,
                },
                context_window,
                effective_context_window,
                compaction,
                replay: ReplayRequirements {
                    opaque_state: reasoning,
                    signed_thinking: reasoning,
                },
            };
        }

        Self {
            family: ProviderFamily::OpenAiCodex,
            protocol: WireProtocol::OpenAiResponses,
            input: Modalities {
                text: true,
                image: true,
            },
            output: Modalities {
                text: true,
                image: false,
            },
            tools: true,
            parallel_tool_calls: true,
            reasoning: ReasoningCapabilities {
                supported: !model.reasoning_levels.is_empty(),
                summaries: !model.reasoning_levels.is_empty(),
            },
            context_window,
            effective_context_window,
            compaction,
            replay: ReplayRequirements {
                opaque_state: true,
                signed_thinking: false,
            },
        }
    }

    pub(super) fn resolve_for_options(
        credential: &CodexCredential,
        options: &super::super::TurnOptions,
    ) -> Self {
        let model = if let Some(config) = &credential.custom {
            config
                .models
                .iter()
                .find(|item| item.id == options.model)
                .map_or(
                    ProviderModel {
                        id: options.model.clone(),
                        name: options.model.clone(),
                        reasoning_levels: Vec::new(),
                        default_reasoning_level: None,
                        context_window: None,
                    },
                    |item| ProviderModel {
                        id: item.id.clone(),
                        name: item.name.clone(),
                        reasoning_levels: item.reasoning_levels.clone(),
                        default_reasoning_level: item.default_reasoning_level.clone(),
                        context_window: Some(item.context_window),
                    },
                )
        } else if credential.project_id.is_some() {
            let metadata = credential
                .antigravity_models
                .get(&options.model)
                .unwrap_or(&Value::Null);
            ProviderModel {
                id: options.model.clone(),
                name: options.model.clone(),
                reasoning_levels: if metadata["supportsThinking"] == true {
                    vec!["low".into(), "medium".into(), "high".into()]
                } else {
                    Vec::new()
                },
                default_reasoning_level: None,
                context_window: metadata["maxTokens"].as_u64().filter(|value| *value > 0),
            }
        } else {
            ProviderModel {
                id: options.model.clone(),
                name: options.model.clone(),
                reasoning_levels: options.reasoning.iter().cloned().collect(),
                default_reasoning_level: None,
                context_window: None,
            }
        };
        Self::resolve(credential, &model)
    }

    pub(super) fn accepts_input(&self, input: &[Value]) -> bool {
        self.input.image
            || !input.iter().any(|item| {
                item["content"]
                    .as_array()
                    .is_some_and(|parts| parts.iter().any(|part| part["type"] == "input_image"))
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::openai_codex::custom::{AuthMode, Config, Model, TokenField};
    use serde_json::json;

    fn model() -> ProviderModel {
        ProviderModel {
            id: "model".into(),
            name: "Model".into(),
            reasoning_levels: vec!["low".into(), "high".into()],
            default_reasoning_level: Some("high".into()),
            context_window: Some(100_000),
        }
    }

    fn credential() -> CodexCredential {
        CodexCredential::new("access", "refresh", 1, "account", None, None)
    }

    #[test]
    fn official_codex_freezes_known_responses_capabilities() {
        let capabilities = ModelCapabilities::resolve(&credential(), &model());
        assert_eq!(capabilities.family, ProviderFamily::OpenAiCodex);
        assert_eq!(capabilities.protocol, WireProtocol::OpenAiResponses);
        assert!(capabilities.tools);
        assert!(capabilities.parallel_tool_calls);
        assert!(capabilities.reasoning.summaries);
        assert_eq!(capabilities.effective_context_window, Some(80_000));
        assert!(capabilities.replay.opaque_state);
    }

    #[test]
    fn antigravity_uses_catalog_thinking_and_signed_replay() {
        let mut credential = credential();
        credential.project_id = Some("project".into());
        credential
            .antigravity_models
            .insert("model".into(), json!({"supportsThinking":true}));
        let capabilities = ModelCapabilities::resolve(&credential, &model());
        assert_eq!(capabilities.family, ProviderFamily::Antigravity);
        assert_eq!(capabilities.protocol, WireProtocol::CloudCodeAssist);
        assert!(capabilities.input.image);
        assert!(capabilities.reasoning.supported);
        assert!(capabilities.replay.signed_thinking);
    }

    #[test]
    fn custom_defaults_are_conservative_and_follow_explicit_model_flags() {
        let mut credential = credential();
        credential.custom = Some(Config {
            base_url: "https://example.com/v1".into(),
            protocol: Protocol::OpenaiCompletions,
            auth_mode: AuthMode::Bearer,
            token_field: TokenField::MaxCompletionTokens,
            replay_unsigned_thinking: false,
            models: vec![Model {
                id: "model".into(),
                name: "Model".into(),
                context_window: 100_000,
                max_output_tokens: 8_000,
                supports_images: false,
                supports_tools: true,
                reasoning: Reasoning::None,
                reasoning_levels: vec![],
                default_reasoning_level: None,
                thinking_budget: None,
            }],
        });
        let capabilities = ModelCapabilities::resolve(&credential, &model());
        assert_eq!(capabilities.family, ProviderFamily::Custom);
        assert_eq!(capabilities.protocol, WireProtocol::OpenAiCompletions);
        assert!(capabilities.tools);
        assert!(!capabilities.parallel_tool_calls);
        assert!(!capabilities.input.image);
        assert!(!capabilities.reasoning.supported);
        assert!(!capabilities.replay.opaque_state);
    }

    #[test]
    fn missing_custom_model_disables_optional_capabilities() {
        let mut credential = credential();
        credential.custom = Some(Config {
            base_url: "https://example.com/v1".into(),
            protocol: Protocol::AnthropicMessages,
            auth_mode: AuthMode::XApiKey,
            token_field: TokenField::MaxTokens,
            replay_unsigned_thinking: false,
            models: vec![],
        });
        let capabilities = ModelCapabilities::resolve(&credential, &model());
        assert!(!capabilities.tools);
        assert!(!capabilities.parallel_tool_calls);
        assert!(!capabilities.input.image);
        assert!(!capabilities.reasoning.supported);
    }
}
