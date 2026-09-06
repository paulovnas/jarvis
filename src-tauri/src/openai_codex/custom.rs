//! Offline configuration and secure credentials for compatible, user-owned endpoints.
use super::{
    CodexCredential, OpenAiCodexState, ProviderAccount, ProviderError, ProviderModel, SecretStore,
};
use crate::persistence::{AppState, ProviderAccountRecord};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::{collections::HashSet, path::Path};
pub(crate) mod discovery;

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum Protocol {
    OpenaiCompletions,
    OpenaiResponses,
    AnthropicMessages,
}
#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum AuthMode {
    Bearer,
    XApiKey,
}
#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum TokenField {
    MaxTokens,
    MaxCompletionTokens,
}
#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum Reasoning {
    None,
    Effort,
    Openrouter,
    Deepseek,
    Budget,
    Adaptive,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Model {
    pub id: String,
    pub name: String,
    pub context_window: u64,
    pub max_output_tokens: u64,
    pub supports_images: bool,
    pub supports_tools: bool,
    pub reasoning: Reasoning,
    pub reasoning_levels: Vec<String>,
    pub default_reasoning_level: Option<String>,
    pub thinking_budget: Option<u64>,
}
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Config {
    pub base_url: String,
    pub protocol: Protocol,
    pub auth_mode: AuthMode,
    pub token_field: TokenField,
    #[serde(default)]
    pub replay_unsigned_thinking: bool,
    pub models: Vec<Model>,
}

fn invalid(message: &'static str) -> ProviderError {
    ProviderError::new("custom_config", message)
}
pub(crate) fn validate_alias(alias: &str) -> Result<(), ProviderError> {
    if alias.is_empty()
        || alias.len() > 64
        || !alias
            .bytes()
            .next()
            .is_some_and(|c| c.is_ascii_alphanumeric())
        || !alias
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"-_.".contains(&c))
    {
        return Err(invalid("Alias: use até 64 letras, números, pontos, hífens ou sublinhados, começando por letra ou número."));
    }
    Ok(())
}
impl Config {
    pub(crate) fn endpoint(&self) -> Result<reqwest::Url, ProviderError> {
        let mut url = reqwest::Url::parse(&self.base_url)
            .map_err(|_| invalid("Informe uma URL base válida."))?;
        let local = matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"));
        if !(url.scheme() == "https" || url.scheme() == "http" && local)
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
            || self.base_url.len() > 2048
        {
            return Err(invalid(
                "Use HTTPS (HTTP apenas local), sem credenciais, parâmetros ou fragmentos na URL.",
            ));
        }
        let suffix = match self.protocol {
            Protocol::OpenaiCompletions => "/chat/completions",
            Protocol::OpenaiResponses => "/responses",
            Protocol::AnthropicMessages => "/messages",
        };
        let path = url.path().trim_end_matches('/').to_owned();
        if ["/chat/completions", "/responses", "/messages"]
            .iter()
            .any(|other| path.ends_with(other) && *other != suffix)
        {
            return Err(invalid(
                "A URL termina em um endpoint diferente do selecionado.",
            ));
        }
        if !path.ends_with(suffix) {
            url.set_path(&format!("{path}{suffix}"));
        } else {
            url.set_path(&path);
        }
        Ok(url)
    }
    pub(crate) fn validate(&self) -> Result<(), ProviderError> {
        self.endpoint()?;
        if self.protocol != Protocol::AnthropicMessages && self.auth_mode != AuthMode::Bearer {
            return Err(invalid("Endpoints OpenAI utilizam autenticação Bearer."));
        }
        if self.models.is_empty() || self.models.len() > 100 {
            return Err(invalid("Cadastre entre 1 e 100 modelos."));
        }
        let mut ids = HashSet::new();
        for model in &self.models {
            if model.id.is_empty()
                || model.id.len() > 200
                || model
                    .id
                    .chars()
                    .any(|c| c.is_control() || c.is_whitespace())
                || model.name.trim().is_empty()
                || model.name.len() > 120
                || model.name.chars().any(char::is_control)
                || !ids.insert(&model.id)
            {
                return Err(invalid(
                    "Cada modelo precisa de um ID único (sem espaços) e um nome de exibição.",
                ));
            }
            if !(4096..=100_000_000).contains(&model.context_window)
                || model.max_output_tokens == 0
                || model.max_output_tokens >= model.context_window
                || model.max_output_tokens > 10_000_000
            {
                return Err(invalid("Informe contexto de 4.096 a 100.000.000 tokens e saída positiva menor que o contexto (até 10.000.000)."));
            }
            let compatible = match self.protocol {
                Protocol::OpenaiCompletions => matches!(
                    model.reasoning,
                    Reasoning::None
                        | Reasoning::Effort
                        | Reasoning::Openrouter
                        | Reasoning::Deepseek
                ),
                Protocol::OpenaiResponses => {
                    matches!(model.reasoning, Reasoning::None | Reasoning::Effort)
                }
                Protocol::AnthropicMessages => matches!(
                    model.reasoning,
                    Reasoning::None | Reasoning::Budget | Reasoning::Adaptive
                ),
            };
            if !compatible {
                return Err(invalid(
                    "O formato de raciocínio não é compatível com o endpoint.",
                ));
            }
            let levels = &model.reasoning_levels;
            if levels.len() > 12
                || levels.iter().any(|s| {
                    s.is_empty()
                        || s.len() > 32
                        || !s.bytes().all(|c| {
                            c.is_ascii_lowercase() || c.is_ascii_digit() || b"_-".contains(&c)
                        })
                })
                || levels.iter().collect::<HashSet<_>>().len() != levels.len()
                || model
                    .default_reasoning_level
                    .as_ref()
                    .is_some_and(|level| !levels.contains(level))
            {
                return Err(invalid(
                    "Informe níveis de raciocínio únicos e um padrão presente na lista.",
                ));
            }
            if model.reasoning == Reasoning::None {
                if !levels.is_empty()
                    || model.default_reasoning_level.is_some()
                    || model.thinking_budget.is_some()
                {
                    return Err(invalid(
                        "Remova as opções de raciocínio deste modelo ou selecione um formato.",
                    ));
                }
            } else if levels.is_empty() || model.default_reasoning_level.is_none() {
                return Err(invalid(
                    "Informe os níveis aceitos pelo modelo e selecione o padrão.",
                ));
            }
            if model.reasoning == Reasoning::Budget {
                if !levels.iter().all(|l| matches!(l.as_str(), "off" | "on"))
                    || !levels.iter().any(|l| l == "on")
                    || !model
                        .thinking_budget
                        .is_some_and(|n| n >= 1024 && n < model.max_output_tokens)
                {
                    return Err(invalid("Thinking por orçamento: use níveis off/on e orçamento de pelo menos 1.024 tokens, menor que a saída."));
                }
            } else if model.thinking_budget.is_some() {
                return Err(invalid(
                    "Orçamento de thinking só se aplica ao formato por orçamento.",
                ));
            }
        }
        Ok(())
    }
    pub(crate) fn catalog(&self) -> Vec<ProviderModel> {
        self.models
            .iter()
            .map(|m| ProviderModel {
                id: m.id.clone(),
                name: m.name.clone(),
                reasoning_levels: m.reasoning_levels.clone(),
                default_reasoning_level: m.default_reasoning_level.clone(),
                context_window: Some(m.context_window),
            })
            .collect()
    }
}
pub(crate) fn load(state: &AppState, home: &Path, alias: &str) -> Result<Config, ProviderError> {
    state.with_connection(home, |connection| {
        let raw: String = connection
            .query_row(
                "SELECT config FROM custom_provider_configs WHERE alias = ?1",
                [alias],
                |row| row.get(0),
            )
            .map_err(|_| invalid("A configuração Custom não está disponível. Edite o provedor."))?;
        let config: Config =
            serde_json::from_str(&raw).map_err(|_| invalid("Revise a configuração Custom."))?;
        config.validate()?;
        Ok(config)
    })
}
pub(super) fn account(
    state: &AppState,
    home: &Path,
    record: ProviderAccountRecord,
) -> Result<ProviderAccount, ProviderError> {
    // Reading the catalog is offline and does not require opening the Keychain.
    let config = load(state, home, &record.alias).ok();
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
fn save(
    connection: &mut Connection,
    store: &dyn SecretStore,
    alias: &str,
    config: &Config,
    api_key: Option<&str>,
    editing: bool,
) -> Result<(), ProviderError> {
    validate_alias(alias)?;
    config.validate()?;
    let previous: Option<(String, String)> = connection
        .query_row(
            "SELECT provider_kind, account_id FROM provider_accounts WHERE alias = ?1",
            [alias],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(|_| ProviderError::database())?;
    if (editing && previous.as_ref().is_none_or(|(kind, _)| kind != "custom"))
        || (!editing && previous.is_some())
    {
        return Err(invalid(
            "O alias já existe ou o provedor Custom não está mais disponível.",
        ));
    }
    let key = api_key.filter(|key| !key.is_empty());
    if key.is_some_and(|key| key.len() > 8192 || !key.bytes().all(|byte| byte.is_ascii_graphic())) {
        return Err(invalid("A chave contém caracteres inválidos."));
    }
    let old_credential = if editing {
        store.load(alias).ok()
    } else {
        None
    };
    if key.is_none() && old_credential.is_none() {
        return Err(invalid("Informe a chave de API."));
    }
    let account_id = previous
        .as_ref()
        .map(|(_, id)| id.clone())
        .unwrap_or(format!(
            "custom:{}",
            crate::library::new_id().map_err(|_| ProviderError::internal())?
        ));
    let next_credential =
        key.map(|key| CodexCredential::new(key, "", i64::MAX, &account_id, None, None));
    let transaction = connection
        .transaction()
        .map_err(|_| ProviderError::database())?;
    if !editing {
        transaction.execute("INSERT INTO provider_accounts(alias, provider_kind, account_id, show_usage) VALUES (?1, 'custom', ?2, 0)", params![alias, account_id]).map_err(|_| ProviderError::database())?;
    }
    transaction.execute("INSERT INTO custom_provider_configs(alias, config) VALUES (?1, ?2) ON CONFLICT(alias) DO UPDATE SET config = excluded.config", params![alias, serde_json::to_string(config).map_err(|_| ProviderError::internal())?]).map_err(|_| ProviderError::database())?;
    if let Some(credential) = &next_credential {
        store
            .store(alias, credential)
            .map_err(|_| invalid("Não foi possível salvar a chave no armazenamento seguro."))?;
    }
    if transaction.commit().is_err() {
        if next_credential.is_some() {
            let rollback = match old_credential {
                Some(old) => store.store(alias, &old),
                None => store.remove(alias),
            };
            if rollback.is_err() {
                return Err(invalid(
                    "A gravação falhou. Revise a chave deste provedor antes de usá-lo.",
                ));
            }
        }
        return Err(ProviderError::database());
    }
    Ok(())
}

#[tauri::command]
pub async fn save_custom_provider(
    app: tauri::AppHandle,
    persistence_state: tauri::State<'_, AppState>,
    oauth_state: tauri::State<'_, OpenAiCodexState>,
    alias: String,
    config: Config,
    api_key: Option<String>,
    editing: bool,
) -> Result<ProviderAccount, ProviderError> {
    let home = super::home_dir(&app)?;
    let state = persistence_state.inner().clone();
    let manager = oauth_state.manager.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = manager
            .credentials_guard
            .lock()
            .map_err(|_| ProviderError::internal())?;
        state.with_connection(&home, |connection| {
            save(
                connection,
                manager.secret_store.as_ref(),
                &alias,
                &config,
                api_key.as_deref(),
                editing,
            )
        })?;
        let record = state
            .list_provider_accounts(&home)
            .map_err(|_| ProviderError::database())?
            .into_iter()
            .find(|record| record.alias == alias)
            .ok_or_else(ProviderError::database)?;
        account(&state, &home, record)
    })
    .await
    .map_err(|_| ProviderError::internal())?
}

#[cfg(test)]
mod tests;
