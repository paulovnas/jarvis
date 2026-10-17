//! Native Hyper accounts share the compatible transport and secure API-key storage.
use super::{
    custom::{self, AuthMode, Config, Model, Protocol, Reasoning, TokenField},
    CodexCredential, OpenAiCodexState, ProviderAccount, ProviderError, SecretStore,
};
use crate::persistence::{AppState, ProviderAccountRecord};
use serde_json::Value;
use std::{io::Read, path::Path, time::Duration};

pub(crate) const BASE_URL: &str = "https://hyper.charm.land/v1";
const KIND: &str = "charm-hyper";

pub(crate) mod panel;
pub(super) mod quotas;

fn error(message: &'static str) -> ProviderError {
    ProviderError::new("charm_hyper", message)
}

fn validate_alias(alias: &str) -> Result<(), ProviderError> {
    if alias
        .strip_prefix("charm-hyper-")
        .is_none_or(|suffix| super::validate_alias_suffix(suffix).is_err())
    {
        return Err(error(
            "Alias Charm Hyper: informe um nome após charm-hyper-.",
        ));
    }
    Ok(())
}

fn client() -> Result<reqwest::blocking::Client, ProviderError> {
    reqwest::blocking::Client::builder()
        .user_agent(concat!("jarvis/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(8))
        .timeout(Duration::from_secs(20))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| error("Não foi possível preparar a conexão com Charm Hyper."))
}

fn json(
    client: &reqwest::blocking::Client,
    base: &str,
    path: &str,
    key: Option<&str>,
) -> Result<Value, ProviderError> {
    let mut request = client.get(format!("{base}/{path}"));
    if let Some(key) = key {
        request = request.bearer_auth(key);
    }
    let response = request
        .send()
        .map_err(|_| error("Não foi possível consultar Charm Hyper. Tente novamente."))?;
    if matches!(response.status().as_u16(), 401 | 403) {
        return Err(ProviderError::new(
            "unauthorized",
            "A chave Charm Hyper foi recusada. Edite o provedor e confira sua chave.",
        ));
    }
    if !response.status().is_success() {
        return Err(error(
            "Charm Hyper não concluiu a consulta. Tente novamente.",
        ));
    }
    let mut bytes = Vec::new();
    response
        .take(16 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| error("Não foi possível ler a resposta Charm Hyper."))?;
    if bytes.len() > 16 * 1024 * 1024 {
        return Err(error("A resposta Charm Hyper excedeu o tamanho permitido."));
    }
    serde_json::from_slice(&bytes).map_err(|_| error("Charm Hyper retornou dados inválidos."))
}

fn catalog(data: &Value) -> Result<Config, ProviderError> {
    let integer = |value: Option<f64>, max: u64| {
        value.filter(|value| value.is_finite() && *value > 0.0 && value.fract() == 0.0 && *value <= max as f64)
            .map(|value| value as u64)
            .ok_or_else(|| error("O catálogo Charm Hyper contém limites inválidos. Atualize os modelos e tente novamente."))
    };
    let models = custom::catalog::parse(data)?
        .into_iter()
        .map(|model| {
            let levels: Vec<_> = model
                .reasoning_levels
                .into_iter()
                .map(|level| level.value)
                .collect();
            if !levels.is_empty() && model.default_reasoning_level.is_none() {
                return Err(error(
                    "O catálogo Charm Hyper não informou o raciocínio padrão deste modelo.",
                ));
            }
            Ok(Model {
                id: model.id,
                name: model.name,
                context_window: integer(model.context_window, 100_000_000)?,
                max_output_tokens: integer(model.max_output_tokens, 10_000_000)?,
                supports_images: model.supports_images.unwrap_or(false),
                // Hyper documents OpenAI tool calling; OMP exposes it for this catalog.
                supports_tools: model.supports_tools.unwrap_or(true),
                reasoning: if levels.is_empty() {
                    Reasoning::None
                } else {
                    Reasoning::Effort
                },
                reasoning_levels: levels,
                default_reasoning_level: model.default_reasoning_level,
                thinking_budget: None,
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let config = Config {
        base_url: BASE_URL.into(),
        protocol: Protocol::OpenaiCompletions,
        auth_mode: AuthMode::Bearer,
        token_field: TokenField::MaxTokens,
        replay_unsigned_thinking: false,
        models,
    };
    config.validate()?;
    Ok(config)
}

pub(super) fn discover() -> Result<Config, ProviderError> {
    catalog(&json(&client()?, BASE_URL, "models", None)?)
}

pub(crate) fn load(state: &AppState, home: &Path, alias: &str) -> Result<Config, ProviderError> {
    validate_alias(alias)?;
    let config = custom::load(state, home, alias)?;
    if config.base_url != BASE_URL
        || config.protocol != Protocol::OpenaiCompletions
        || config.auth_mode != AuthMode::Bearer
    {
        return Err(error(
            "A configuração Charm Hyper não corresponde ao endpoint oficial. Edite o provedor.",
        ));
    }
    Ok(config)
}

pub(super) fn remember(
    state: &AppState,
    home: &Path,
    alias: &str,
    config: &Config,
) -> Result<(), ProviderError> {
    config.validate()?;
    state.with_connection(home, |db| {
        db.execute("UPDATE custom_provider_configs SET config = ?1 WHERE alias = ?2 AND EXISTS(SELECT 1 FROM provider_accounts WHERE alias = ?2 AND provider_kind = 'charm-hyper')",
            rusqlite::params![serde_json::to_string(config).map_err(|_| ProviderError::internal())?, alias])
            .map_err(|_| ProviderError::database())?;
        Ok(())
    })
}

pub(super) fn account(
    state: &AppState,
    home: &Path,
    record: ProviderAccountRecord,
) -> Result<ProviderAccount, ProviderError> {
    let config = load(state, home, &record.alias).ok();
    let mut account = ProviderAccount::from_record(
        record,
        None,
        config.as_ref().map_or_else(Vec::new, Config::catalog),
        config.is_some(),
    );
    account.vision_models = config.map_or_else(Vec::new, |config| {
        config
            .models
            .into_iter()
            .filter(|model| model.supports_images)
            .map(|model| model.id)
            .collect()
    });
    Ok(account)
}

pub(super) fn credential(
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
        .find(|record| record.alias == alias && record.provider_kind == KIND && record.enabled)
        .ok_or_else(|| error("Ative uma conta Charm Hyper nas configurações."))?;
    let mut credential = store
        .load(alias)
        .map_err(|cause| ProviderError::from_account_error(cause.into()))?;
    if credential.account_id != record.account_id
        || !credential.account_id.starts_with("charm-hyper:")
        || credential.project_id.is_some()
    {
        return Err(error("Revise a chave da conta Charm Hyper selecionada."));
    }
    credential.custom = Some(load(state, home, alias)?);
    Ok(credential)
}

pub(super) fn credits(
    client: &reqwest::blocking::Client,
    base: &str,
    key: &str,
) -> Result<f64, ProviderError> {
    credit_balance(&json(client, base, "credits", Some(key))?)
}

fn credit_balance(data: &Value) -> Result<f64, ProviderError> {
    data["balance"].as_f64().filter(|balance| balance.is_finite() && *balance >= 0.0)
        .ok_or_else(|| error("Charm Hyper não informou um saldo em Hypercredits. Consulte os limites da assinatura no painel Hyper."))
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn save_charm_hyper_provider(
    app: tauri::AppHandle,
    persistence_state: tauri::State<'_, AppState>,
    oauth_state: tauri::State<'_, OpenAiCodexState>,
    alias: String,
    api_key: Option<String>,
    editing: bool,
    display_name: Option<String>,
) -> Result<ProviderAccount, ProviderError> {
    validate_alias(&alias)?;
    if let Some(name) = &display_name {
        super::display_name::validate(name)?;
    }
    let key = api_key.filter(|key| !key.is_empty());
    if key
        .as_ref()
        .is_some_and(|key| key.len() > 8192 || !key.bytes().all(|byte| byte.is_ascii_graphic()))
    {
        return Err(error("A chave contém caracteres inválidos."));
    }
    if !editing && key.is_none() {
        return Err(error("Informe a chave de API Charm Hyper."));
    }
    let home = super::home_dir(&app)?;
    let state = persistence_state.inner().clone();
    let manager = oauth_state.manager.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let config = if editing && key.is_none() {
            load(&state, &home, &alias)?
        } else {
            json(
                &client()?,
                BASE_URL,
                "credits",
                Some(key.as_deref().ok_or_else(ProviderError::internal)?),
            )?;
            discover()?
        };
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
                key.as_deref(),
                editing,
                display_name.as_deref(),
                KIND,
            )
        })?;
        manager.usage_cache.invalidate(&alias);
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
