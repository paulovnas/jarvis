//! Native OpenRouter identity with manually selected models and read-only USD probes.
use super::{
    custom, CodexCredential, OpenAiCodexState, ProviderAccount, ProviderError, SecretStore,
};
use crate::persistence::{AppState, ProviderAccountRecord};
use serde::Serialize;
use serde_json::Value;
use std::{io::Read, path::Path};

pub(crate) const BASE_URL: &str = "https://openrouter.ai/api/v1";

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Balance {
    pub account_balance: Option<f64>,
    pub key_limit_remaining: Option<f64>,
    pub note: Option<String>,
}
impl Balance {
    pub(super) fn available(&self) -> Option<f64> {
        self.account_balance.map(|balance| {
            self.key_limit_remaining
                .map_or(balance, |limit| balance.min(limit))
        })
    }
}

fn error(message: &'static str) -> ProviderError {
    ProviderError::new("openrouter", message)
}

pub(super) fn validate_config(config: &custom::Config) -> Result<(), ProviderError> {
    config.validate()?;
    if config.base_url.trim_end_matches('/') != BASE_URL
        || config.protocol != custom::Protocol::OpenaiCompletions
        || config.auth_mode != custom::AuthMode::Bearer
    {
        return Err(error(
            "O OpenRouter nativo utiliza o endpoint oficial com autenticação Bearer.",
        ));
    }
    Ok(())
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
        config
            .as_ref()
            .map_or_else(Vec::new, custom::Config::catalog),
        config.is_some(),
    );
    account.custom = config;
    Ok(account)
}

pub(super) fn credential(
    state: &AppState,
    home: &Path,
    store: &dyn SecretStore,
    alias: &str,
) -> Result<CodexCredential, ProviderError> {
    let record = state
        .list_provider_accounts(home)
        .map_err(|_| ProviderError::database())?
        .into_iter()
        .find(|record| {
            record.alias == alias && record.provider_kind == "openrouter" && record.enabled
        })
        .ok_or_else(|| error("Ative uma conta OpenRouter nas configurações."))?;
    let mut credential = store
        .load(alias)
        .map_err(|_| error("Edite o OpenRouter e informe sua chave de API."))?;
    if credential.account_id != record.account_id || credential.project_id.is_some() {
        return Err(error("Revise a chave da conta OpenRouter selecionada."));
    }
    let config = custom::load(state, home, alias)?;
    validate_config(&config)?;
    credential.custom = Some(config);
    Ok(credential)
}

fn get(
    client: &reqwest::blocking::Client,
    base: &str,
    path: &str,
    key: &str,
) -> Result<Option<Value>, ProviderError> {
    let response = client
        .get(format!("{base}/{path}"))
        .bearer_auth(key)
        .send()
        .map_err(|_| error("Não foi possível consultar o OpenRouter."))?;
    if path == "credits" && matches!(response.status().as_u16(), 401 | 403) {
        return Ok(None);
    }
    if !response.status().is_success() {
        return Err(error(
            "O OpenRouter recusou a consulta. Confira a chave e tente novamente.",
        ));
    }
    let mut bytes = Vec::new();
    response
        .take(1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| error("Não foi possível ler o saldo OpenRouter."))?;
    if bytes.len() > 1024 * 1024 {
        return Err(error("A resposta OpenRouter excedeu o tamanho permitido."));
    }
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|_| error("O OpenRouter retornou dados inválidos."))
}

fn balance(key: &Value, credits: Option<&Value>) -> Result<Balance, ProviderError> {
    let data = key
        .get("data")
        .filter(|data| data.is_object())
        .ok_or_else(|| error("O OpenRouter não informou os dados da chave."))?;
    let mut key_limit_remaining = match &data["limit_remaining"] {
        Value::Null => None,
        value => Some(
            value
                .as_f64()
                .filter(|v| v.is_finite())
                .ok_or_else(|| error("O limite OpenRouter é inválido."))?,
        ),
    };
    if let Some(budget) = data["effective_budget"]["remaining_usd"].as_f64() {
        if !budget.is_finite() {
            return Err(error("O orçamento OpenRouter é inválido."));
        }
        key_limit_remaining = Some(key_limit_remaining.map_or(budget, |limit| limit.min(budget)));
    }
    let account_balance = credits
        .map(|credits| {
            let purchased = credits["data"]["total_credits"]
                .as_f64()
                .filter(|v| v.is_finite() && *v >= 0.0);
            let used = credits["data"]["total_usage"]
                .as_f64()
                .filter(|v| v.is_finite() && *v >= 0.0);
            purchased
                .zip(used)
                .map(|(purchased, used)| purchased - used)
                .filter(|v| v.is_finite())
                .ok_or_else(|| error("O OpenRouter não informou um saldo válido."))
        })
        .transpose()?;
    Ok(Balance { account_balance, key_limit_remaining, note: account_balance.is_none().then(|| "O saldo total exige uma chave de gerenciamento da mesma conta. Você pode adicioná-la ao editar este provedor.".into()) })
}

pub(super) fn fetch(
    client: &reqwest::blocking::Client,
    base: &str,
    credential: &CodexCredential,
) -> Result<Balance, ProviderError> {
    let key = get(client, base, "key", &credential.access)?
        .ok_or_else(|| error("A chave OpenRouter foi recusada."))?;
    let credits = get(
        client,
        base,
        "credits",
        credential
            .openrouter_balance_key
            .as_deref()
            .unwrap_or(&credential.access),
    )?;
    balance(&key, credits.as_ref())
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn save_openrouter_provider(
    app: tauri::AppHandle,
    persistence_state: tauri::State<'_, AppState>,
    oauth_state: tauri::State<'_, OpenAiCodexState>,
    alias: String,
    config: custom::Config,
    api_key: Option<String>,
    balance_key: Option<String>,
    remove_balance_key: bool,
    editing: bool,
    display_name: Option<String>,
) -> Result<ProviderAccount, ProviderError> {
    validate_config(&config)?;
    for key in [api_key.as_deref(), balance_key.as_deref()]
        .into_iter()
        .flatten()
        .filter(|key| !key.is_empty())
    {
        if key.len() > 8192 || !key.bytes().all(|byte| byte.is_ascii_graphic()) {
            return Err(error("A chave contém caracteres inválidos."));
        }
    }
    let home = super::home_dir(&app)?;
    let state = persistence_state.inner().clone();
    let manager = oauth_state.manager.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = manager
            .credentials_guard
            .lock()
            .map_err(|_| ProviderError::internal())?;
        state.with_connection(&home, |db| {
            custom::save_provider_with_balance(
                db,
                manager.secret_store.as_ref(),
                &alias,
                &config,
                api_key.as_deref(),
                editing,
                display_name.as_deref(),
                "openrouter",
                if remove_balance_key {
                    Some(None)
                } else {
                    balance_key
                        .as_deref()
                        .filter(|key| !key.is_empty())
                        .map(Some)
                },
            )
        })?;
        manager.usage_cache.invalidate(&alias);
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
#[path = "openrouter/tests.rs"]
mod credential_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn usd_balance_respects_key_cap_and_never_invents_unreported_credit() {
        let credits = json!({"data":{"total_credits":100.5,"total_usage":25.75}});
        let report = balance(&json!({"data":{"limit_remaining":20.0}}), Some(&credits)).unwrap();
        assert_eq!(report.account_balance, Some(74.75));
        assert_eq!(report.available(), Some(20.0));
        assert_eq!(
            balance(
                &json!({"data":{"limit_remaining":20.0,"effective_budget":{"remaining_usd":8.0}}}),
                Some(&credits)
            )
            .unwrap()
            .available(),
            Some(8.0)
        );
        let unrestricted =
            balance(&json!({"data":{"limit_remaining":null}}), Some(&credits)).unwrap();
        assert_eq!(unrestricted.available(), Some(74.75));
        let limited = balance(&json!({"data":{"limit_remaining":15.0}}), None).unwrap();
        assert_eq!(limited.key_limit_remaining, Some(15.0));
        assert!(limited.available().is_none());
        assert!(limited.note.is_some());
        assert!(balance(&json!({"data":{}}), Some(&json!({"data":{}}))).is_err());
        assert!(balance(&json!({"data":{"limit_remaining":"wrong"}}), None).is_err());
    }
}
