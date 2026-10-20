//! Authenticated catalog reads for user-configured compatible endpoints.
use super::{AuthMode, Config, Protocol, TokenField};
use crate::openai_codex::{OpenAiCodexState, ProviderError, SecretStore};
use crate::persistence::AppState;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::HashSet, path::Path, time::Duration};

const MAX_BYTES: usize = 16 * 1024 * 1024;
const MAX_MODELS: usize = 10_000;

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Target {
    base_url: String,
    protocol: Protocol,
    auth_mode: AuthMode,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CatalogModel {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) context_window: Option<f64>,
    pub(crate) max_output_tokens: Option<f64>,
    pub(crate) supports_tools: Option<bool>,
    pub(crate) supports_images: Option<bool>,
    pub(crate) reasoning_levels: Vec<CatalogReasoningLevel>,
    pub(crate) default_reasoning_level: Option<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct CatalogReasoningLevel {
    pub(crate) value: String,
    display: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct Catalog {
    models: Vec<CatalogModel>,
}

fn error(message: &'static str) -> ProviderError {
    ProviderError::new("model_discovery", message)
}

impl Target {
    fn url(&self) -> Result<reqwest::Url, ProviderError> {
        if self.protocol == Protocol::Ollama {
            return Err(error(
                "No Ollama, adicione o ID do modelo para consultar seus recursos.",
            ));
        }
        if self.protocol != Protocol::AnthropicMessages && self.auth_mode != AuthMode::Bearer {
            return Err(error("Endpoints OpenAI utilizam autenticação Bearer."));
        }
        let config = Config {
            base_url: self.base_url.clone(),
            protocol: self.protocol,
            auth_mode: self.auth_mode,
            token_field: TokenField::MaxTokens,
            replay_unsigned_thinking: false,
            models: vec![],
        };
        let mut url = config.endpoint()?;
        let suffix = match self.protocol {
            Protocol::OpenaiCompletions => "/chat/completions",
            Protocol::OpenaiResponses => "/responses",
            Protocol::AnthropicMessages => "/messages",
            Protocol::Ollama => unreachable!("native catalog lookup was handled above"),
        };
        let path = url
            .path()
            .strip_suffix(suffix)
            .expect("validated inference endpoint");
        let path = if path.is_empty() { "/v1" } else { path };
        url.set_path(&format!("{path}/models"));
        Ok(url)
    }
}

fn credential(
    target: &Target,
    api_key: Option<String>,
    alias: Option<String>,
    state: &AppState,
    home: &Path,
    store: &dyn SecretStore,
) -> Result<String, ProviderError> {
    target.url()?;
    if let Some(key) = api_key.filter(|key| !key.is_empty()) {
        if key.len() > 8192 || !key.bytes().all(|byte| byte.is_ascii_graphic()) {
            return Err(error("A chave contém caracteres inválidos."));
        }
        return Ok(key);
    }
    let alias = alias.ok_or_else(|| error("Informe a chave de API antes de descobrir modelos."))?;
    super::validate_alias(&alias)?;
    let saved = super::load(state, home, &alias)?;
    let previous = Target {
        base_url: saved.base_url,
        protocol: saved.protocol,
        auth_mode: saved.auth_mode,
    };
    if previous.url()? != target.url()? {
        return Err(error("A URL mudou. Informe a chave para consultar este endereço; a chave salva não será enviada a outro endpoint."));
    }
    store
        .load(&alias)
        .map(|secret| secret.access)
        .map_err(|error| ProviderError::from_account_error(error.into()))
}

pub(crate) fn parse(value: &Value) -> Result<Vec<CatalogModel>, ProviderError> {
    let rows = value["data"].as_array().ok_or_else(|| {
        error("O catálogo retornou um formato inesperado. Esperado: data com uma lista de modelos.")
    })?;
    if rows.len() > MAX_MODELS {
        return Err(error("O catálogo excedeu o limite de 10.000 modelos."));
    }
    let mut models = vec![];
    let mut ids = HashSet::new();
    for row in rows {
        let id = row["id"]
            .as_str()
            .filter(|id| super::valid_model_id(id))
            .ok_or_else(|| {
                error("O catálogo contém um ID vazio, longo demais ou com caracteres de controle.")
            })?;
        if !ids.insert(id) {
            continue;
        }
        let name = row["name"]
            .as_str()
            .or_else(|| row["display_name"].as_str())
            .filter(|name| !name.trim().is_empty() && !name.chars().any(char::is_control))
            .unwrap_or(id)
            .chars()
            .scan(0, |length, character| {
                *length += character.len_utf16();
                (*length <= 120).then_some(character)
            })
            .collect();
        let context = [
            row["context_length"].as_f64(),
            row["context_window"].as_f64(),
            row["top_provider"]["context_length"].as_f64(),
        ]
        .into_iter()
        .flatten()
        .reduce(f64::min);
        let output = row["max_output_tokens"]
            .as_f64()
            .or_else(|| row["top_provider"]["max_completion_tokens"].as_f64());
        let parameters = row["supported_parameters"].as_array();
        let modalities = row["architecture"]["input_modalities"].as_array();
        let mut efforts = HashSet::new();
        let reasoning_levels: Vec<CatalogReasoningLevel> = row["reasoning"]["effort_levels"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|level| {
                let value = level["value"].as_str().or_else(|| level.as_str())?;
                if value.is_empty()
                    || value.len() > 32
                    || !value
                        .bytes()
                        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || b"_-".contains(&c))
                    || !efforts.insert(value)
                {
                    return None;
                }
                let display = level["display"]
                    .as_str()
                    .filter(|name| {
                        !name.trim().is_empty()
                            && name.encode_utf16().count() <= 120
                            && !name.chars().any(char::is_control)
                    })
                    .unwrap_or(value);
                Some(CatalogReasoningLevel {
                    value: value.into(),
                    display: display.into(),
                })
            })
            .collect();
        let default_reasoning_level = row["reasoning"]["default_effort_level"]
            .as_str()
            .filter(|level| efforts.contains(level))
            .map(str::to_owned);
        models.push(CatalogModel {
            id: id.into(),
            name,
            context_window: context,
            max_output_tokens: output,
            supports_tools: row["supports_tools"]
                .as_bool()
                .or_else(|| parameters.map(|items| items.iter().any(|v| v == "tools"))),
            supports_images: row["supports_images"]
                .as_bool()
                .or_else(|| row["capabilities"]["vision"].as_bool())
                .or_else(|| modalities.map(|items| items.iter().any(|v| v == "image"))),
            reasoning_levels,
            default_reasoning_level,
        });
    }
    Ok(models)
}

async fn fetch(target: &Target, key: &str) -> Result<Catalog, ProviderError> {
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(20))
        .connect_timeout(Duration::from_secs(10))
        .user_agent(concat!("Jarvis/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|_| ProviderError::internal())?;
    let mut models = vec![];
    let mut ids = HashSet::new();
    let mut cursors = HashSet::new();
    let mut url = target.url()?;
    let mut bytes_read = 0;
    for _ in 0..20 {
        let mut request = client.get(url.clone()).header("accept", "application/json");
        request = match target.auth_mode {
            AuthMode::Bearer => request.bearer_auth(key),
            AuthMode::XApiKey => request.header("x-api-key", key),
        };
        if target.protocol == Protocol::AnthropicMessages {
            request = request.header("anthropic-version", "2023-06-01");
        }
        let mut response = request.send().await.map_err(|_| {
            error("Não foi possível consultar o catálogo. Verifique a conexão e a URL.")
        })?;
        if !response.status().is_success() {
            return Err(error(match response.status().as_u16() {
                401 | 403 => "O catálogo recusou a autenticação (401/403). Confira a chave e as permissões.",
                404 => "O endpoint de modelos não foi encontrado (404). Confira a URL base ou configure os modelos manualmente.",
                429 => "O provedor limitou as consultas (429). Tente novamente mais tarde.",
                _ => "O catálogo está indisponível. Confira a URL base e tente novamente.",
            }));
        }
        let mut bytes = vec![];
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| error("O catálogo foi interrompido. Tente novamente."))?
        {
            bytes_read += chunk.len();
            if bytes_read > MAX_BYTES {
                return Err(error("O catálogo excedeu o limite de leitura."));
            }
            bytes.extend_from_slice(&chunk);
        }
        let value: Value = serde_json::from_slice(&bytes)
            .map_err(|_| error("O catálogo retornou dados inválidos."))?;
        for model in parse(&value)? {
            if ids.insert(model.id.clone()) {
                models.push(model);
            }
            if models.len() > MAX_MODELS {
                return Err(error("O catálogo excedeu o limite de 10.000 modelos."));
            }
        }
        if value["has_more"] != true {
            return Ok(Catalog { models });
        }
        let cursor = value["last_id"]
            .as_str()
            .filter(|id| super::valid_model_id(id))
            .ok_or_else(|| error("O catálogo retornou uma página incompleta sem cursor válido."))?;
        if !cursors.insert(cursor.to_owned()) {
            return Err(error("O catálogo repetiu uma página. Tente novamente."));
        }
        url = target.url()?;
        url.query_pairs_mut().append_pair("after_id", cursor);
    }
    Err(error("O catálogo excedeu o limite de páginas da consulta."))
}

#[tauri::command]
pub async fn discover_custom_models(
    app: tauri::AppHandle,
    persistence_state: tauri::State<'_, AppState>,
    oauth_state: tauri::State<'_, OpenAiCodexState>,
    target: Target,
    api_key: Option<String>,
    alias: Option<String>,
) -> Result<Catalog, ProviderError> {
    let home = super::super::home_dir(&app)?;
    let state = persistence_state.inner().clone();
    let manager = oauth_state.manager.clone();
    let lookup_target = target.clone();
    let key = tauri::async_runtime::spawn_blocking(move || {
        let _guard = manager
            .credentials_guard
            .lock()
            .map_err(|_| ProviderError::internal())?;
        credential(
            &lookup_target,
            api_key,
            alias,
            &state,
            &home,
            manager.secret_store.as_ref(),
        )
    })
    .await
    .map_err(|_| ProviderError::internal())??;
    tokio::time::timeout(Duration::from_secs(25), fetch(&target, &key))
        .await
        .map_err(|_| error("A consulta demorou demais. Verifique a conexão e tente novamente."))?
}

#[cfg(test)]
mod tests;
