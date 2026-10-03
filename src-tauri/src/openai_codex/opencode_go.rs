//! Subscription accounts reuse the existing transports; keys never cross IPC.
pub(crate) mod free_models;

use super::{
    custom::{AuthMode, Config, Model, Protocol, Reasoning, TokenField},
    CodexCredential, OpenAiCodexState, ProviderAccount, ProviderAccountType, ProviderError,
    SecretStore,
};
use crate::persistence::{AppState, ProviderAccountRecord};
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::Value;
use std::{collections::HashSet, io::Read, path::Path, time::Duration};

pub(crate) const BASE_URL: &str = "https://opencode.ai/zen/go/v1";
const METADATA_URL: &str = "https://models.dev/api.json";
const MAX_CATALOG: u64 = 24 * 1024 * 1024;
// Match OpenCode's operational output budget, independently of the model's
// advertised maximum. Gateway routes may enforce a smaller upstream cap.
const MAX_OUTPUT_TOKENS: u64 = 32_000;

pub(crate) fn user_agent() -> String {
    format!("jarvis/{}", env!("CARGO_PKG_VERSION"))
}
fn error(message: &'static str) -> ProviderError {
    ProviderError::new("opencode_go", message)
}
fn validate_alias(alias: &str) -> Result<(), ProviderError> {
    if alias
        .strip_prefix("opencode-go-")
        .is_none_or(|suffix| super::validate_alias_suffix(suffix).is_err())
    {
        return Err(error(
            "Alias OpenCode Go: informe um nome após opencode-go-.",
        ));
    }
    Ok(())
}
pub(crate) fn client() -> Result<reqwest::blocking::Client, ProviderError> {
    reqwest::blocking::Client::builder()
        .user_agent(user_agent())
        .connect_timeout(Duration::from_secs(8))
        .timeout(Duration::from_secs(15))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| error("Não foi possível preparar a conexão com OpenCode Go."))
}
fn public_json(client: &reqwest::blocking::Client, url: &str) -> Result<Value, ProviderError> {
    let response = client.get(url).send().map_err(|_| {
        error("Não foi possível atualizar os modelos OpenCode Go. Tente novamente.")
    })?;
    if !response.status().is_success() {
        return Err(error(
            "O catálogo OpenCode Go está indisponível. Tente novamente.",
        ));
    }
    let mut bytes = vec![];
    response
        .take(MAX_CATALOG + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| error("Não foi possível ler o catálogo OpenCode Go."))?;
    if bytes.len() as u64 > MAX_CATALOG {
        return Err(error("O catálogo OpenCode Go excedeu o tamanho esperado."));
    }
    serde_json::from_slice(&bytes)
        .map_err(|_| error("O catálogo OpenCode Go retornou dados inválidos."))
}

fn protocol(id: &str, metadata: &Value) -> Option<Protocol> {
    // The official Go endpoint table supersedes stale SDK entries for Qwen.
    if id.starts_with("qwen") || id.starts_with("minimax-") {
        return Some(Protocol::AnthropicMessages);
    }
    if id.starts_with("gpt-") || id.starts_with("grok-") || id.starts_with("muse-") {
        return Some(Protocol::OpenaiResponses);
    }
    match metadata["provider"]["npm"].as_str() {
        None | Some("@ai-sdk/openai-compatible") => Some(Protocol::OpenaiCompletions),
        Some("@ai-sdk/openai") => Some(Protocol::OpenaiResponses),
        Some("@ai-sdk/anthropic") => Some(Protocol::AnthropicMessages),
        _ => None,
    }
}

fn model_config_from_metadata(id: &str, raw: &Value) -> Option<Config> {
    let protocol = protocol(id, raw)?;
    let context_window = raw["limit"]["context"].as_u64()?;
    // Some catalog entries report the same total context and maximum output.
    // Keep the transport's output strictly below its total context envelope.
    let max_output_tokens = raw["limit"]["output"]
        .as_u64()?
        .min(context_window.checked_sub(1)?);
    let mut levels = raw["reasoning_options"]
        .as_array()
        .and_then(|options| options.iter().find(|o| o["type"] == "effort"))
        .and_then(|option| option["values"].as_array())
        .map_or_else(Vec::new, |values| {
            values
                .iter()
                .filter_map(|v| v.as_str())
                .filter(|s| {
                    matches!(
                        *s,
                        "none" | "minimal" | "low" | "medium" | "high" | "xhigh" | "max"
                    )
                })
                .map(str::to_owned)
                .collect::<Vec<_>>()
        });
    let toggle = raw["reasoning_options"]
        .as_array()
        .is_some_and(|options| options.iter().any(|o| o["type"] == "toggle"));
    let mut budget = None;
    let reasoning = if protocol == Protocol::AnthropicMessages {
        // Qwen documents enabled thinking with a token budget, not Claude's
        // adaptive thinking dialect. MiniMax's fixed reasoning needs no effort.
        if id.starts_with("qwen") && !levels.is_empty() {
            if toggle {
                levels.insert(0, "off".into());
            }
            Reasoning::EnabledEffort
        } else if toggle {
            levels = vec!["off".into(), "on".into()];
            if id.starts_with("qwen") {
                budget = Some(8192.min(max_output_tokens / 2));
                Reasoning::Budget
            } else {
                Reasoning::Toggle
            }
        } else {
            levels.clear();
            Reasoning::None
        }
    } else if !levels.is_empty() {
        if protocol == Protocol::OpenaiCompletions && id.starts_with("deepseek-") {
            Reasoning::Deepseek
        } else {
            Reasoning::Effort
        }
    } else if toggle && protocol == Protocol::OpenaiCompletions {
        levels = vec!["off".into(), "on".into()];
        Reasoning::Toggle
    } else {
        Reasoning::None
    };
    let default = levels
        .iter()
        .find(|l| matches!(l.as_str(), "high" | "on"))
        .or_else(|| levels.last())
        .cloned();
    let mut config = Config {
        base_url: BASE_URL.into(),
        protocol,
        auth_mode: if protocol == Protocol::AnthropicMessages {
            AuthMode::XApiKey
        } else {
            AuthMode::Bearer
        },
        token_field: TokenField::MaxTokens,
        replay_unsigned_thinking: protocol == Protocol::AnthropicMessages,
        models: vec![Model {
            id: id.into(),
            name: raw["name"].as_str()?.into(),
            context_window,
            max_output_tokens,
            supports_images: raw["modalities"]["input"]
                .as_array()
                .is_some_and(|m| m.iter().any(|v| v == "image")),
            supports_tools: raw["tool_call"].as_bool()?,
            reasoning,
            reasoning_levels: levels,
            default_reasoning_level: default,
            thinking_budget: budget,
        }],
    };
    normalize_contract(&mut config);
    config.validate().ok()?;
    Some(config)
}

fn normalize_contract(config: &mut Config) {
    for model in &mut config.models {
        model.max_output_tokens = model.max_output_tokens.min(MAX_OUTPUT_TOKENS);
        // Go's DeepSeek routes use the OpenAI effort dialect, not the native
        // DeepSeek thinking toggle. OMP pins the original Flash to Responses;
        // V4.1 and the vision experiment use Completions with a different cap field.
        if !model.id.starts_with("deepseek-") {
            continue;
        }
        match model.id.as_str() {
            "deepseek-v4-flash" => config.protocol = Protocol::OpenaiResponses,
            "deepseek-v4.1-flash" | "deepseek-v4-flash-vision-exp" => {
                config.protocol = Protocol::OpenaiCompletions;
                config.token_field = TokenField::MaxCompletionTokens;
            }
            _ => {}
        }
        config.auth_mode = AuthMode::Bearer;
        if model.reasoning != Reasoning::None {
            model.reasoning = Reasoning::Effort;
        }
    }
}
fn parse_catalog(ids: &Value, metadata: &Value) -> Result<Vec<Config>, ProviderError> {
    let ids = ids["data"]
        .as_array()
        .ok_or_else(|| error("O catálogo OpenCode Go retornou dados inválidos."))?;
    let mut seen = HashSet::new();
    let catalog: Vec<_> = ids
        .iter()
        .filter_map(|item| {
            let id = item["id"].as_str()?;
            if !seen.insert(id) {
                return None;
            }
            let raw = metadata["opencode-go"]["models"]
                .get(id)
                .or_else(|| metadata["opencode"]["models"].get(id))?;
            model_config_from_metadata(id, raw)
        })
        .take(100)
        .collect();
    if catalog.is_empty() {
        return Err(error("Não foi possível confirmar as capacidades dos modelos OpenCode Go. Tente atualizar o catálogo novamente."));
    }
    Ok(catalog)
}
pub(super) fn discover() -> Result<Vec<Config>, ProviderError> {
    let client = client()?;
    parse_catalog(
        &public_json(&client, &format!("{BASE_URL}/models"))?,
        &public_json(&client, METADATA_URL)?,
    )
}
fn load(state: &AppState, home: &Path, alias: &str) -> Result<Vec<Config>, ProviderError> {
    state.with_connection(home, |db| {
        let raw: String = db
            .query_row(
                "SELECT catalog FROM opencode_go_catalogs WHERE alias=?1",
                [alias],
                |row| row.get(0),
            )
            .map_err(|_| error("Atualize o catálogo OpenCode Go nas configurações."))?;
        let mut configs: Vec<Config> = serde_json::from_str(&raw)
            .map_err(|_| error("Atualize o catálogo OpenCode Go nas configurações."))?;
        if configs.is_empty()
            || configs.len() > 100
            || configs
                .iter()
                .any(|c| c.base_url != BASE_URL || c.models.len() != 1 || c.validate().is_err())
        {
            return Err(error("Atualize o catálogo OpenCode Go nas configurações."));
        }
        // Apply corrected contracts to already-connected accounts without
        // rewriting their stored catalog, identity, key or selected effort.
        for config in &mut configs {
            normalize_contract(config);
            config
                .validate()
                .map_err(|_| error("Atualize o catálogo OpenCode Go nas configurações."))?;
        }
        Ok(configs)
    })
}
pub(super) fn remember(
    state: &AppState,
    home: &Path,
    alias: &str,
    catalog: &[Config],
) -> Result<(), ProviderError> {
    state.with_connection(home, |db| {
        db.execute("INSERT INTO opencode_go_catalogs(alias,catalog) VALUES (?1,?2) ON CONFLICT(alias) DO UPDATE SET catalog=excluded.catalog", params![alias, serde_json::to_string(catalog).map_err(|_| ProviderError::internal())?]).map_err(|_| ProviderError::database())?;
        Ok(())
    })
}
pub(super) fn account(
    state: &AppState,
    home: &Path,
    record: ProviderAccountRecord,
) -> Result<ProviderAccount, ProviderError> {
    let catalog = load(state, home, &record.alias).ok();
    let models = catalog.as_ref().map_or_else(Vec::new, |configs| {
        configs.iter().flat_map(Config::catalog).collect()
    });
    let mut account = ProviderAccount::from_record(record, None, models, catalog.is_some());
    account.vision_models = catalog.as_ref().map_or_else(Vec::new, |configs| {
        configs
            .iter()
            .flat_map(|c| c.models.iter())
            .filter(|model| model.supports_images)
            .map(|model| model.id.clone())
            .collect()
    });
    account.account_type = ProviderAccountType::Personal;
    Ok(account)
}
pub(crate) fn model_config(
    state: &AppState,
    home: &Path,
    alias: &str,
    model: &str,
) -> Result<Config, ProviderError> {
    load(state, home, alias)?
        .into_iter()
        .find(|config| config.models[0].id == model)
        .ok_or_else(|| error("O modelo selecionado não está disponível no OpenCode Go."))
}

pub(crate) fn credential(
    state: &AppState,
    home: &Path,
    store: &dyn SecretStore,
    alias: &str,
) -> Result<CodexCredential, ProviderError> {
    validate_alias(alias)?;
    let record = state
        .list_provider_accounts(home)
        .map_err(|_| ProviderError::database())?
        .into_iter()
        .find(|r| r.alias == alias && r.provider_kind == "opencode-go")
        .ok_or_else(|| error("O provedor OpenCode Go foi desconectado."))?;
    if !record.enabled {
        return Err(error("Ative o provedor OpenCode Go nas configurações."));
    }
    let credential = store
        .load(alias)
        .map_err(|_| error("Edite o provedor OpenCode Go e informe sua chave de API."))?;
    if credential.account_id != record.account_id
        || !credential.account_id.starts_with("opencode-go:")
        || credential.project_id.is_some()
    {
        return Err(error(
            "A chave do OpenCode Go não corresponde à conta configurada. Reconecte o provedor.",
        ));
    }
    Ok(credential)
}

fn save(
    db: &mut Connection,
    store: &dyn SecretStore,
    alias: &str,
    key: &str,
    editing: bool,
    catalog: &[Config],
) -> Result<(), ProviderError> {
    validate_alias(alias)?;
    if key.is_empty() || key.len() > 8192 || !key.bytes().all(|b| b.is_ascii_graphic()) {
        return Err(error("Informe uma chave de API válida do OpenCode Go."));
    }
    let previous: Option<(String, String)> = db
        .query_row(
            "SELECT provider_kind,account_id FROM provider_accounts WHERE alias=?1",
            [alias],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(|_| ProviderError::database())?;
    if editing
        && previous
            .as_ref()
            .is_none_or(|(kind, _)| kind != "opencode-go")
        || !editing && previous.is_some()
    {
        return Err(error(
            "O alias já existe ou o provedor OpenCode Go foi desconectado.",
        ));
    }
    let old = if editing {
        Some(
            store
                .load(alias)
                .map_err(|e| ProviderError::from_account_error(e.into()))?,
        )
    } else {
        None
    };
    let id = previous.map(|(_, id)| id).unwrap_or(format!(
        "opencode-go:{}",
        crate::library::new_id().map_err(|_| ProviderError::internal())?
    ));
    let next = CodexCredential::new(key, "", i64::MAX, &id, None, None);
    let tx = db.transaction().map_err(|_| ProviderError::database())?;
    if !editing {
        tx.execute("INSERT INTO provider_accounts(alias,provider_kind,account_id) VALUES (?1,'opencode-go',?2)", params![alias,id]).map_err(|_| ProviderError::database())?;
    }
    tx.execute("INSERT INTO opencode_go_catalogs(alias,catalog) VALUES (?1,?2) ON CONFLICT(alias) DO UPDATE SET catalog=excluded.catalog", params![alias,serde_json::to_string(catalog).map_err(|_| ProviderError::internal())?]).map_err(|_| ProviderError::database())?;
    store
        .store(alias, &next)
        .map_err(|e| ProviderError::from_account_error(e.into()))?;
    if tx.commit().is_err() {
        let rollback = old
            .as_ref()
            .map_or_else(|| store.remove(alias), |old| store.store(alias, old));
        if rollback.is_err() {
            return Err(error(
                "A gravação falhou. Revise a chave OpenCode Go antes de usá-la.",
            ));
        }
        return Err(ProviderError::database());
    }
    Ok(())
}

#[tauri::command]
pub async fn save_opencode_go_provider(
    app: tauri::AppHandle,
    persistence_state: tauri::State<'_, AppState>,
    oauth_state: tauri::State<'_, OpenAiCodexState>,
    alias: String,
    api_key: Option<String>,
    editing: bool,
) -> Result<ProviderAccount, ProviderError> {
    let home = super::home_dir(&app)?;
    let state = persistence_state.inner().clone();
    let manager = oauth_state.manager.clone();
    tauri::async_runtime::spawn_blocking(move || {
        validate_alias(&alias)?;
        if let Some(key) = api_key.filter(|s| !s.is_empty()) {
            if key.len() > 8192 || !key.bytes().all(|b| b.is_ascii_graphic()) {
                return Err(error("Informe uma chave de API válida do OpenCode Go."));
            }
            let session = format!("jarvis-go-{}", alias);
            super::usage::go::fetch_windows(&client()?, BASE_URL, &key, &session)?;
            let catalog = discover()?;
            // Network probes must not hold the shared credential lock: other
            // chats can keep sending while this account is being connected.
            let _guard = manager
                .credentials_guard
                .lock()
                .map_err(|_| ProviderError::internal())?;
            state.with_connection(&home, |db| {
                save(
                    db,
                    manager.secret_store.as_ref(),
                    &alias,
                    &key,
                    editing,
                    &catalog,
                )
            })?;
            manager.usage_cache.invalidate(&alias);
        } else if !editing {
            return Err(error("Informe a chave de API do OpenCode Go."));
        }
        let record = state
            .list_provider_accounts(&home)
            .map_err(|_| ProviderError::database())?
            .into_iter()
            .find(|r| r.alias == alias && r.provider_kind == "opencode-go")
            .ok_or_else(|| error("O provedor OpenCode Go foi desconectado."))?;
        super::attach_model_exclusions(&state, &home, account(&state, &home, record)?)
    })
    .await
    .map_err(|_| ProviderError::internal())?
}

#[cfg(test)]
mod tests;
