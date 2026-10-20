//! Search-only accounts use the existing secure credential store, never the model catalog.
use super::{CodexCredential, OpenAiCodexState, ProviderAccount, ProviderError, SecretStore};
use crate::persistence::{AppState, ProviderAccountRecord};
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;

fn error(message: &'static str) -> ProviderError {
    ProviderError::new("brave_search_account", message)
}

fn validate_key(key: &str) -> Result<(), ProviderError> {
    if key.is_empty() || key.len() > 8192 || !key.bytes().all(|byte| byte.is_ascii_graphic()) {
        return Err(error("Informe uma chave de API válida do Brave Search."));
    }
    Ok(())
}

pub(super) fn account(record: ProviderAccountRecord) -> ProviderAccount {
    ProviderAccount::from_record(record, None, vec![], true)
}

pub(super) fn credential(
    state: &AppState,
    home: &Path,
    store: &dyn SecretStore,
    record: &ProviderAccountRecord,
) -> Result<CodexCredential, ProviderError> {
    crate::persistence::require_enabled_account(state, home, &record.alias)?;
    let credential = store
        .load(&record.alias)
        .map_err(|e| ProviderError::from_account_error(e.into()))?;
    if record.provider_kind != "brave-search" || credential.account_id != record.account_id {
        return Err(error(
            "A conta Brave Search mudou. Conecte sua chave novamente.",
        ));
    }
    validate_key(&credential.access)?;
    Ok(credential)
}

fn save(
    db: &mut Connection,
    store: &dyn SecretStore,
    alias: &str,
    key: Option<&str>,
    editing: bool,
    display_name: Option<&str>,
) -> Result<(), ProviderError> {
    let suffix = alias
        .strip_prefix("brave-search-")
        .ok_or_else(ProviderError::invalid_alias)?;
    super::validate_alias_suffix(suffix).map_err(|_| ProviderError::invalid_alias())?;
    let name = display_name
        .map(super::display_name::validate)
        .transpose()?;
    if let Some(key) = key {
        validate_key(key)?;
    } else if !editing {
        return Err(error("Informe a chave de API do Brave Search."));
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
            .is_none_or(|(kind, _)| kind != "brave-search")
        || !editing && previous.is_some()
    {
        return Err(error(
            "O alias já existe ou o provedor Brave Search foi desconectado.",
        ));
    }
    let old = if editing && key.is_some() {
        Some(
            store
                .load(alias)
                .map_err(|e| ProviderError::from_account_error(e.into()))?,
        )
    } else {
        None
    };
    let id = match previous {
        Some((_, id)) => id,
        None => format!(
            "brave-search:{}",
            crate::library::new_id().map_err(|_| ProviderError::internal())?
        ),
    };
    let tx = db.transaction().map_err(|_| ProviderError::database())?;
    if !editing {
        tx.execute("INSERT INTO provider_accounts(alias,provider_kind,account_id,show_usage) VALUES (?1,'brave-search',?2,0)", params![alias,id])
            .map_err(|_| ProviderError::database())?;
    }
    if let Some(name) = name {
        super::display_name::update(&tx, alias, name)?;
    }
    if let Some(key) = key {
        store
            .store(
                alias,
                &CodexCredential::new(key, "", i64::MAX, &id, None, None),
            )
            .map_err(|e| ProviderError::from_account_error(e.into()))?;
    }
    if tx.commit().is_err() {
        if key.is_some()
            && old
                .as_ref()
                .map_or_else(|| store.remove(alias), |old| store.store(alias, old))
                .is_err()
        {
            return Err(error(
                "A gravação falhou. Revise a chave Brave Search antes de usá-la.",
            ));
        }
        return Err(ProviderError::database());
    }
    Ok(())
}

#[tauri::command]
pub async fn save_brave_search_provider(
    app: tauri::AppHandle,
    persistence_state: tauri::State<'_, AppState>,
    oauth_state: tauri::State<'_, OpenAiCodexState>,
    alias: String,
    api_key: Option<String>,
    editing: bool,
    display_name: Option<String>,
) -> Result<ProviderAccount, ProviderError> {
    let home = super::home_dir(&app)?;
    let state = persistence_state.inner().clone();
    let manager = oauth_state.manager.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = manager
            .credentials_guard
            .lock()
            .map_err(|_| ProviderError::internal())?;
        state.with_connection(&home, |db| {
            save(
                db,
                manager.secret_store.as_ref(),
                &alias,
                api_key.as_deref().filter(|key| !key.is_empty()),
                editing,
                display_name.as_deref(),
            )
        })?;
        let record = state
            .list_provider_accounts(&home)
            .map_err(|_| ProviderError::database())?
            .into_iter()
            .find(|record| record.alias == alias && record.provider_kind == "brave-search")
            .ok_or_else(|| error("O provedor Brave Search foi desconectado."))?;
        Ok(account(record))
    })
    .await
    .map_err(|_| ProviderError::internal())?
}

#[cfg(test)]
mod tests;
