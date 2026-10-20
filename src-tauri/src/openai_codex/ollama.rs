//! Native local/LAN Ollama connections. The saved catalog contains only user-selected models.
use super::{
    custom::{self, AuthMode, Config, Model, Protocol, Reasoning, TokenField},
    OpenAiCodexState, ProviderAccount, ProviderError,
};
use crate::persistence::{AppState, ProviderAccountRecord};
use serde_json::{json, Value};
use std::{net::IpAddr, path::Path, time::Duration};

fn error(message: &'static str) -> ProviderError {
    ProviderError::new("ollama_config", message)
}

pub(super) fn endpoint(base: &str, operation: &str) -> Result<reqwest::Url, ProviderError> {
    let mut url =
        reqwest::Url::parse(base).map_err(|_| error("Informe o endereço do servidor Ollama."))?;
    let host = url.host_str().unwrap_or_default();
    let local = host == "localhost"
        || host
            .trim_matches(['[', ']'])
            .parse::<IpAddr>()
            .is_ok_and(|ip| match ip {
                IpAddr::V4(ip) => ip.is_loopback() || ip.is_private(),
                IpAddr::V6(ip) => {
                    ip.is_loopback() || ip.is_unique_local() || ip.is_unicast_link_local()
                }
            });
    if base.len() > 2048
        || host.is_empty()
        || !(url.scheme() == "https" || url.scheme() == "http" && local)
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(error("Use HTTPS ou HTTP em localhost/rede privada, sem credenciais ou parâmetros na URL do Ollama."));
    }
    let path = url.path().trim_end_matches('/');
    let path = ["/api/chat", "/api/show", "/v1", "/api"]
        .iter()
        .find_map(|suffix| path.strip_suffix(suffix))
        .unwrap_or(path);
    url.set_path(&format!("{path}/api/{operation}"));
    Ok(url)
}

pub(super) fn validate_config(config: &Config) -> Result<(), ProviderError> {
    if config.protocol != Protocol::Ollama || config.auth_mode != AuthMode::Bearer {
        return Err(error("Revise a conexão nativa do Ollama."));
    }
    config.validate()
}

pub(super) fn account(
    state: &AppState,
    home: &Path,
    record: ProviderAccountRecord,
) -> Result<ProviderAccount, ProviderError> {
    let config = custom::load(state, home, &record.alias)
        .ok()
        .filter(|config| validate_config(config).is_ok());
    let mut account = ProviderAccount::from_record(
        record,
        None,
        config.as_ref().map_or_else(Vec::new, Config::catalog),
        config.is_some(),
    );
    account.custom = config;
    account.show_usage = false;
    account.show_third_party_usage = false;
    Ok(account)
}

pub(crate) fn model_details(value: &Value, id: &str) -> Result<Model, ProviderError> {
    let capabilities = value["capabilities"].as_array().ok_or_else(|| error("O Ollama não informou os recursos do modelo. Atualize o servidor ou configure o modelo manualmente."))?;
    let supports = |feature: &str| capabilities.iter().any(|item| item == feature);
    if !supports("completion") {
        return Err(error(
            "Este modelo do Ollama não oferece chat. Adicione um modelo de texto ou multimodal.",
        ));
    }
    let context = value["model_info"]
        .as_object()
        .and_then(|info| {
            info.iter().find_map(|(key, value)| {
                key.ends_with(".context_length")
                    .then(|| value.as_u64())
                    .flatten()
            })
        })
        .filter(|context| *context >= 4096)
        .ok_or_else(|| {
            error("O Ollama não informou um contexto compatível. Preencha os limites manualmente.")
        })?;
    // Allocate a modest context explicitly; trained maxima can require much more RAM/VRAM.
    let context_window = context.min(32768);
    let mut levels = vec![];
    if let Some(values) = value["thinking"]["values"].as_array() {
        for value in values {
            let level =
                match value {
                    Value::Bool(true) => "on",
                    Value::Bool(false) => "off",
                    Value::String(level) => level,
                    _ => return Err(error(
                        "O Ollama retornou níveis de raciocínio inválidos. Configure manualmente.",
                    )),
                };
            if !levels.iter().any(|item| item == level) {
                levels.push(level.to_owned());
            }
        }
    }
    if !levels.iter().any(|level| level != "off") {
        levels.clear();
    }
    let reasoning = if levels.is_empty() {
        Reasoning::None
    } else if levels
        .iter()
        .all(|level| matches!(level.as_str(), "on" | "off"))
    {
        Reasoning::Toggle
    } else {
        Reasoning::Effort
    };
    let default = match &value["thinking"]["default"] {
        Value::Bool(true) => Some("on"),
        Value::Bool(false) => Some("off"),
        Value::String(level) => Some(level.as_str()),
        _ => None,
    }
    .filter(|default| levels.iter().any(|level| level == default))
    .map(str::to_owned)
    .or_else(|| levels.first().cloned());
    Ok(Model {
        id: id.to_owned(),
        name: id.to_owned(),
        context_window,
        max_output_tokens: 4096.min(context_window / 2),
        supports_chat: true,
        supports_image_generation: false,
        supports_images: supports("vision"),
        supports_tools: supports("tools"),
        reasoning,
        reasoning_levels: levels,
        default_reasoning_level: default,
        thinking_budget: None,
    })
}

#[tauri::command]
pub async fn lookup_ollama_model(
    base_url: String,
    model_id: String,
) -> Result<custom::discovery::DiscoveredModel, ProviderError> {
    if model_id.trim().is_empty()
        || model_id.encode_utf16().count() > 200
        || model_id.chars().any(char::is_control)
    {
        return Err(error("Informe o ID exato do modelo instalado no Ollama."));
    }
    let url = endpoint(&base_url, "show")?;
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(25))
        .build()
        .map_err(|_| ProviderError::internal())?;
    let mut response = client.post(url.clone()).json(&json!({"model":model_id})).send().await
        .map_err(|_| error("Não foi possível conectar ao Ollama. Confira o endereço e se o servidor aceita conexões pela rede."))?;
    if !response.status().is_success() {
        return Err(error(if response.status().as_u16() == 404 {
            "Modelo não encontrado no Ollama. Confira o nome e a tag de um modelo já instalado no servidor."
        } else {
            "O Ollama recusou a consulta do modelo. Confira o servidor e tente novamente."
        }));
    }
    let mut bytes = vec![];
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| error("A consulta ao Ollama foi interrompida. Tente novamente."))?
    {
        if bytes.len() + chunk.len() > 8 * 1024 * 1024 {
            return Err(error("Os dados do modelo excederam o limite de leitura."));
        }
        bytes.extend_from_slice(&chunk);
    }
    let value =
        serde_json::from_slice(&bytes).map_err(|_| error("O Ollama retornou dados inválidos."))?;
    let model = model_details(&value, &model_id)?;
    let config = Config {
        base_url,
        protocol: Protocol::Ollama,
        auth_mode: AuthMode::Bearer,
        token_field: TokenField::MaxTokens,
        replay_unsigned_thinking: false,
        models: vec![model.clone()],
    };
    validate_config(&config)?;
    Ok(custom::discovery::DiscoveredModel {
        model,
        source_url: url.into(),
        token_field: TokenField::MaxTokens,
    })
}

#[tauri::command]
pub async fn save_ollama_provider(
    app: tauri::AppHandle,
    persistence_state: tauri::State<'_, AppState>,
    oauth_state: tauri::State<'_, OpenAiCodexState>,
    alias: String,
    config: Config,
    editing: bool,
    display_name: Option<String>,
) -> Result<ProviderAccount, ProviderError> {
    validate_config(&config)?;
    let home = super::home_dir(&app)?;
    let state = persistence_state.inner().clone();
    let manager = oauth_state.manager.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = manager
            .credentials_guard
            .lock()
            .map_err(|_| ProviderError::internal())?;
        state.with_connection(&home, |db| {
            custom::save_provider(
                db,
                manager.secret_store.as_ref(),
                &alias,
                &config,
                None,
                editing,
                display_name.as_deref(),
                "ollama",
            )
        })?;
        let record = state
            .list_provider_accounts(&home)
            .map_err(|_| ProviderError::database())?
            .into_iter()
            .find(|record| record.alias == alias)
            .ok_or_else(ProviderError::database)?;
        super::attach_model_exclusions(&state, &home, account(&state, &home, record)?)
    })
    .await
    .map_err(|_| ProviderError::internal())?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ollama_can_edit_remove_models_and_resolve_credentials_without_a_secret() {
        use super::super::{InMemorySecretStore, OAuthManager, SecretStore};
        use std::sync::Arc;
        let home = tempfile::tempdir().unwrap();
        let state = AppState::default();
        let store = Arc::new(InMemorySecretStore::default());
        let oauth = OpenAiCodexState {
            manager: Arc::new(OAuthManager::production(store.clone())),
        };
        let model = model_details(&json!({"capabilities":["completion","tools","vision"],"model_info":{"any.context_length":8192}}), "any-model:tag").unwrap();
        let mut config = Config {
            base_url: "http://localhost:11434".into(),
            protocol: Protocol::Ollama,
            auth_mode: AuthMode::Bearer,
            token_field: TokenField::MaxTokens,
            replay_unsigned_thinking: false,
            models: vec![model],
        };
        state
            .with_connection(home.path(), |db| {
                custom::save_provider(
                    db,
                    store.as_ref(),
                    "ollama-test",
                    &config,
                    None,
                    false,
                    None,
                    "ollama",
                )
            })
            .unwrap();
        let (credential, selected) = oauth
            .inference_model(&state, home.path(), "ollama-test", "any-model:tag", None)
            .unwrap();
        assert!(credential.access.is_empty());
        assert_eq!(selected.id, "any-model:tag");
        config.models[0].id = "a-new-model:latest".into();
        state
            .with_connection(home.path(), |db| {
                custom::save_provider(
                    db,
                    store.as_ref(),
                    "ollama-test",
                    &config,
                    None,
                    true,
                    None,
                    "ollama",
                )
            })
            .unwrap();
        assert!(oauth
            .inference_model(&state, home.path(), "ollama-test", "any-model:tag", None)
            .is_err());
        oauth
            .inference_model(
                &state,
                home.path(),
                "ollama-test",
                "a-new-model:latest",
                None,
            )
            .unwrap();
        assert!(store.load("ollama-test").is_err());
        oauth
            .remove_with_updates(&state, home.path(), "ollama-test", |_| Ok(()))
            .unwrap();
        assert!(state
            .list_provider_accounts(home.path())
            .unwrap()
            .is_empty());
    }

    #[test]
    fn ollama_metadata_uses_advertised_capabilities_without_implying_image_generation() {
        for vision in [false, true] {
            let mut value = json!({"capabilities":["completion","tools","thinking"],"thinking":{"values":[false,true],"default":true},"model_info":{"model.context_length":262144}});
            if vision {
                value["capabilities"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!("vision"));
            }
            let model = model_details(&value, "literal:tag").unwrap();
            assert_eq!(model.id, "literal:tag");
            assert_eq!(model.supports_images, vision);
            assert!(model.supports_tools);
            assert!(!model.supports_image_generation);
            assert_eq!(model.context_window, 32768);
            assert_eq!(model.max_output_tokens, 4096);
            assert_eq!(model.reasoning, Reasoning::Toggle);
            assert_eq!(model.default_reasoning_level.as_deref(), Some("on"));
        }
        let model = model_details(&json!({"capabilities":["completion","thinking"],"thinking":{"values":["low","medium","high"],"default":"medium"},"model_info":{"gptoss.context_length":131072}}), "any-new-model").unwrap();
        assert_eq!(model.reasoning, Reasoning::Effort);
        assert_eq!(model.default_reasoning_level.as_deref(), Some("medium"));
        assert!(model_details(&json!({"capabilities":["embedding"]}), "embedding").is_err());
    }
}
