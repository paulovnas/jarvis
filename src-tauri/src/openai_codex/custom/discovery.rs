//! Public metadata only: never access credentials or send an inference request.
use super::{AuthMode, Config, Model, Protocol, Reasoning, TokenField};
use crate::openai_codex::ProviderError;
use serde::Serialize;
use serde_json::Value;
use std::{
    sync::{Arc, OnceLock},
    time::{Duration, Instant},
};
use tokio::sync::Mutex;

const CATALOG_URL: &str = "https://openrouter.ai/api/v1/models";
const MAX_CATALOG: usize = 16 * 1024 * 1024;
type Cache = Mutex<Option<(Instant, Arc<Value>)>>;
static CACHE: OnceLock<Cache> = OnceLock::new();

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DiscoveredModel {
    pub model: Model,
    pub source_url: String,
    pub token_field: TokenField,
}
fn error(message: &'static str) -> ProviderError {
    ProviderError::new("model_discovery", message)
}
fn validate_target(base_url: &str, model_id: &str) -> Result<(), ProviderError> {
    let url = reqwest::Url::parse(base_url)
        .map_err(|_| error("Informe a URL base antes de buscar o modelo."))?;
    let path = url.path().trim_end_matches('/');
    if url.scheme() != "https"
        || url.host_str() != Some("openrouter.ai")
        || url.port_or_known_default() != Some(443)
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || !matches!(
            path,
            "/api/v1" | "/api/v1/chat/completions" | "/api/v1/responses" | "/api/v1/messages"
        )
    {
        return Err(error("A busca automática está disponível para o OpenRouter. Para este provedor, use Configurar manualmente."));
    }
    if model_id.is_empty()
        || model_id.len() > 200
        || model_id
            .chars()
            .any(|c| c.is_whitespace() || c.is_control())
    {
        return Err(error("Copie o ID exato da página do modelo no OpenRouter."));
    }
    Ok(())
}
fn positive(value: &Value) -> Option<u64> {
    value.as_u64().filter(|n| *n > 0)
}
fn parse(catalog: &Value, id: &str, protocol: Protocol) -> Result<DiscoveredModel, ProviderError> {
    let data = catalog["data"].as_array().ok_or_else(|| {
        error("O catálogo retornou um formato inesperado. Configure manualmente.")
    })?;
    let raw = data.iter().find(|m| m["id"] == id).ok_or_else(|| error("Modelo não encontrado. Copie o ID completo da página do modelo ou configure manualmente."))?;
    let context_window = match (positive(&raw["context_length"]), positive(&raw["top_provider"]["context_length"])) {
        (Some(a), Some(b)) => a.min(b), (Some(a), None) | (None, Some(a)) => a,
        _ => return Err(error("O catálogo não informou a janela de contexto. Consulte a documentação e preencha manualmente.")),
    };
    let max_output_tokens = positive(&raw["top_provider"]["max_completion_tokens"]).ok_or_else(|| error("O catálogo não informou a saída máxima. Consulte a documentação e preencha manualmente."))?;
    let parameters = raw["supported_parameters"].as_array();
    let supports = |name| parameters.is_some_and(|list| list.iter().any(|p| p == name));
    let info = &raw["reasoning"];
    let mut levels: Vec<String> =
        info["supported_efforts"]
            .as_array()
            .map_or_else(Vec::new, |levels| {
                levels
                    .iter()
                    .filter_map(|level| level.as_str().map(str::to_owned))
                    .collect()
            });
    // OpenRouter Messages accepts adaptive thinking with output_config.effort.
    // Intersect its wire vocabulary with the catalog rather than inventing levels
    // or token budgets: /docs/api/api-reference/anthropic-messages/create-a-message
    if protocol == Protocol::AnthropicMessages {
        levels
            .retain(|level| matches!(level.as_str(), "low" | "medium" | "high" | "xhigh" | "max"));
    }
    // A list of supported parameters is not a list of supported effort values.
    let reasoning = if !levels.is_empty() && (supports("reasoning") || supports("reasoning_effort"))
    {
        if info["mandatory"] == false && !levels.iter().any(|l| l == "off") {
            levels.insert(0, "off".into());
        }
        match protocol {
            Protocol::OpenaiCompletions => Reasoning::Openrouter,
            Protocol::OpenaiResponses => Reasoning::Effort,
            Protocol::AnthropicMessages => Reasoning::Adaptive,
        }
    } else {
        levels.clear();
        Reasoning::None
    };
    let default = if info["default_enabled"] == false && levels.iter().any(|l| l == "off") {
        Some("off".into())
    } else {
        info["default_effort"]
            .as_str()
            .filter(|level| levels.iter().any(|l| l == level))
            .map(str::to_owned)
            .or_else(|| levels.iter().find(|l| l.as_str() != "off").cloned())
    };
    let model = Model {
        id: id.into(),
        name: raw["name"]
            .as_str()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or(id)
            .into(),
        context_window,
        max_output_tokens,
        supports_tools: supports("tools"),
        supports_images: raw["architecture"]["input_modalities"]
            .as_array()
            .is_some_and(|values| values.iter().any(|v| v == "image")),
        reasoning,
        reasoning_levels: levels,
        default_reasoning_level: default,
        thinking_budget: None,
    };
    let token_field = if supports("max_tokens") {
        TokenField::MaxTokens
    } else if supports("max_completion_tokens") {
        TokenField::MaxCompletionTokens
    } else {
        TokenField::MaxTokens
    };
    Config { base_url:"https://openrouter.ai/api/v1".into(), protocol, auth_mode:AuthMode::Bearer, token_field, replay_unsigned_thinking:protocol == Protocol::AnthropicMessages, models:vec![model.clone()] }.validate().map_err(|_| error("O catálogo não retornou limites ou capacidades válidos. Configure este modelo manualmente."))?;
    let mut source =
        reqwest::Url::parse("https://openrouter.ai/").map_err(|_| ProviderError::internal())?;
    source.set_path(id);
    Ok(DiscoveredModel {
        model,
        source_url: source.to_string(),
        token_field,
    })
}
async fn catalog() -> Result<Arc<Value>, ProviderError> {
    let mut cache = CACHE.get_or_init(|| Mutex::new(None)).lock().await;
    if let Some((created, value)) = &*cache {
        if created.elapsed() < Duration::from_secs(300) {
            return Ok(value.clone());
        }
    }
    let fetch = async {
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(20))
            .connect_timeout(Duration::from_secs(10))
            .build()
            .map_err(|_| ProviderError::internal())?;
        let mut response = client.get(CATALOG_URL).send().await.map_err(|_| error("Não foi possível consultar o OpenRouter. Tente novamente ou configure manualmente."))?;
        if !response.status().is_success() {
            return Err(error(
                "O catálogo está indisponível. Tente novamente ou configure manualmente.",
            ));
        }
        let mut bytes = vec![];
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| error("O catálogo foi interrompido. Tente novamente."))?
        {
            if bytes.len() + chunk.len() > MAX_CATALOG {
                return Err(error(
                    "O catálogo excedeu o limite de leitura. Configure manualmente.",
                ));
            }
            bytes.extend_from_slice(&chunk);
        }
        let value: Value = serde_json::from_slice(&bytes)
            .map_err(|_| error("O catálogo retornou dados inválidos."))?;
        if !value["data"].is_array() {
            return Err(error("O catálogo retornou dados inválidos."));
        }
        Ok(Arc::new(value))
    };
    let value = tokio::time::timeout(Duration::from_secs(25), fetch)
        .await
        .map_err(|_| {
            error("A consulta demorou demais. Tente novamente ou configure manualmente.")
        })??;
    *cache = Some((Instant::now(), value.clone()));
    Ok(value)
}
#[tauri::command]
pub async fn lookup_custom_model(
    base_url: String,
    model_id: String,
    protocol: Protocol,
) -> Result<DiscoveredModel, ProviderError> {
    validate_target(&base_url, &model_id)?;
    let catalog = catalog().await?;
    parse(catalog.as_ref(), &model_id, protocol)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[tokio::test]
    #[ignore = "Reads only the public OpenRouter catalog for an explicitly selected model"]
    async fn live_openrouter_metadata() {
        let id = std::env::var("JARVIS_CUSTOM_MODEL_ID").expect("Select an exact catalog ID");
        let result = lookup_custom_model(
            "https://openrouter.ai/api/v1".into(),
            id.clone(),
            Protocol::OpenaiCompletions,
        )
        .await
        .unwrap();
        assert_eq!(result.model.id, id);
        assert!(result.model.context_window > result.model.max_output_tokens);
        assert!(result.model.supports_tools);
        println!("Public catalog lookup passed; no credentials or inference used.");
    }
    fn fixture() -> Value {
        json!({"data":[{"id":"vendor/model","name":"Model","context_length":1310720,"top_provider":{"context_length":1048576,"max_completion_tokens":131072},"supported_parameters":["tools","reasoning","max_tokens"],"architecture":{"input_modalities":["text"]},"reasoning":{"mandatory":false,"default_enabled":true,"supported_efforts":["max","high","low"],"default_effort":"high"}}]})
    }
    #[test]
    fn imports_reported_limits_modalities_efforts_and_default_without_model_name_guesses() {
        let result = parse(&fixture(), "vendor/model", Protocol::OpenaiCompletions).unwrap();
        assert_eq!(result.model.context_window, 1048576);
        assert_eq!(result.model.max_output_tokens, 131072);
        assert_eq!(result.model.reasoning, Reasoning::Openrouter);
        assert_eq!(
            result.model.reasoning_levels,
            vec!["off", "max", "high", "low"]
        );
        assert_eq!(
            result.model.default_reasoning_level.as_deref(),
            Some("high")
        );
        assert!(result.model.supports_tools);
        assert!(!result.model.supports_images);
        assert_eq!(result.source_url, "https://openrouter.ai/vendor/model");
    }
    #[test]
    fn missing_metadata_never_invents_limits_or_effort_levels() {
        let mut catalog = fixture();
        catalog["data"][0]["top_provider"]["max_completion_tokens"] = Value::Null;
        assert!(parse(&catalog, "vendor/model", Protocol::OpenaiCompletions).is_err());
        assert!(parse(&fixture(), "missing", Protocol::OpenaiCompletions).is_err());
        let mut catalog = fixture();
        catalog["data"][0]["reasoning"] = Value::Null;
        assert_eq!(
            parse(&catalog, "vendor/model", Protocol::OpenaiResponses)
                .unwrap()
                .model
                .reasoning,
            Reasoning::None
        );
        assert_eq!(
            parse(&catalog, "vendor/model", Protocol::AnthropicMessages)
                .unwrap()
                .model
                .reasoning,
            Reasoning::None
        );
    }
    #[test]
    fn messages_imports_catalog_efforts_with_adaptive_thinking_and_respects_mandatory_reasoning() {
        let mut catalog = fixture();
        catalog["data"][0]["reasoning"]["mandatory"] = Value::Bool(true);
        catalog["data"][0]["reasoning"]["default_effort"] = Value::String("max".into());
        for protocol in [
            Protocol::OpenaiCompletions,
            Protocol::OpenaiResponses,
            Protocol::AnthropicMessages,
        ] {
            let model = parse(&catalog, "vendor/model", protocol).unwrap().model;
            assert_eq!(model.reasoning_levels, vec!["max", "high", "low"]);
            assert_eq!(model.default_reasoning_level.as_deref(), Some("max"));
            assert_eq!(model.thinking_budget, None);
            if protocol == Protocol::AnthropicMessages {
                assert_eq!(model.reasoning, Reasoning::Adaptive);
            }
        }
    }
    #[test]
    fn messages_filters_unsupported_wire_efforts_and_preserves_explicit_disabled_default() {
        let mut catalog = fixture();
        catalog["data"][0]["reasoning"]["supported_efforts"] =
            json!(["minimal", "low", "medium", "high", "xhigh", "max", "ultra"]);
        catalog["data"][0]["reasoning"]["default_enabled"] = Value::Bool(false);
        let model = parse(&catalog, "vendor/model", Protocol::AnthropicMessages)
            .unwrap()
            .model;
        assert_eq!(
            model.reasoning_levels,
            vec!["off", "low", "medium", "high", "xhigh", "max"]
        );
        assert_eq!(model.default_reasoning_level.as_deref(), Some("off"));
        catalog["data"][0]["reasoning"]["supported_efforts"] = json!(["minimal", "ultra"]);
        let model = parse(&catalog, "vendor/model", Protocol::AnthropicMessages)
            .unwrap()
            .model;
        assert_eq!(model.reasoning, Reasoning::None);
        assert!(model.reasoning_levels.is_empty());
        assert_eq!(model.default_reasoning_level, None);
    }
    #[test]
    fn lookup_accepts_only_known_public_catalog_target_without_credentials() {
        for url in [
            "https://openrouter.ai/api/v1",
            "https://openrouter.ai/api/v1/messages/",
        ] {
            assert!(validate_target(url, "vendor/model").is_ok());
        }
        for url in [
            "https://openrouter.ai.evil.example/api/v1",
            "https://secret@openrouter.ai/api/v1",
            "https://openrouter.ai/api/v1?key=secret",
            "http://openrouter.ai/api/v1",
            "https://elsewhere.example/v1",
        ] {
            assert!(validate_target(url, "vendor/model").is_err());
        }
    }
}
