//! Account-specific transport preferences; provider secrets are never exported.
use super::AgentError;
use crate::persistence::AppState;
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use std::path::Path;
use tauri::Manager;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransportSettings {
    supported: bool,
    enabled: bool,
}

fn read(connection: &Connection, alias: &str) -> Result<TransportSettings, AgentError> {
    let account: Option<(String, Option<String>, bool)> = connection.query_row(
        "SELECT a.provider_kind, c.config, COALESCE(t.incremental_responses, 0) FROM provider_accounts a LEFT JOIN custom_provider_configs c ON c.alias = a.alias LEFT JOIN provider_transport_preferences t ON t.account_alias = a.alias WHERE a.alias = ?1",
        [alias], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    ).optional().map_err(|_| AgentError::storage())?;
    let (kind, config, enabled) = account.ok_or_else(|| {
        AgentError::new(
            "provider_account",
            "A conta do provedor não está disponível.",
        )
    })?;
    let supported = kind == "openai-codex"
        || (kind == "custom"
            && config
                .as_deref()
                .and_then(|value| serde_json::from_str::<serde_json::Value>(value).ok())
                .is_some_and(|config| config["protocol"] == "openai-responses"));
    Ok(TransportSettings {
        supported,
        enabled: enabled && supported,
    })
}

fn save(
    connection: &Connection,
    alias: &str,
    enabled: bool,
) -> Result<TransportSettings, AgentError> {
    let settings = read(connection, alias)?;
    if enabled && !settings.supported {
        return Err(AgentError::new("provider_transport", "A conexão incremental está disponível apenas para OpenAI Codex e endpoints OpenAI Responses."));
    }
    connection.execute("INSERT INTO provider_transport_preferences(account_alias, incremental_responses) VALUES (?1, ?2) ON CONFLICT(account_alias) DO UPDATE SET incremental_responses = excluded.incremental_responses", params![alias, enabled]).map_err(|_| AgentError::storage())?;
    read(connection, alias)
}

pub(super) fn enabled(state: &AppState, home: &Path, alias: &str) -> bool {
    state
        .with_connection(home, |db| read(db, alias))
        .is_ok_and(|settings| settings.enabled)
}

#[tauri::command]
pub async fn get_provider_transport(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    alias: String,
) -> Result<TransportSettings, AgentError> {
    let home = app.path().home_dir().map_err(|_| AgentError::storage())?;
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        state.with_connection(&home, |db| read(db, &alias))
    })
    .await
    .map_err(|_| AgentError::internal())?
}

#[tauri::command]
pub async fn set_provider_transport(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    alias: String,
    enabled: bool,
) -> Result<TransportSettings, AgentError> {
    let home = app.path().home_dir().map_err(|_| AgentError::storage())?;
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        state.with_connection(&home, |db| save(db, &alias, enabled))
    })
    .await
    .map_err(|_| AgentError::internal())?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transport_is_opt_in_account_scoped_and_removed_with_the_account() {
        let mut db = Connection::open_in_memory().unwrap();
        crate::persistence::initialize_database(&mut db).unwrap();
        db.execute_batch("INSERT INTO provider_accounts(alias,provider_kind,account_id) VALUES ('one','openai-codex','a'),('two','openai-codex','b'),('google','antigravity','c');").unwrap();
        assert!(!read(&db, "one").unwrap().enabled);
        assert!(save(&db, "one", true).unwrap().enabled);
        assert!(!read(&db, "two").unwrap().enabled);
        assert!(save(&db, "google", true).is_err());
        db.execute("DELETE FROM provider_accounts WHERE alias = 'one'", [])
            .unwrap();
        assert_eq!(
            db.query_row(
                "SELECT COUNT(*) FROM provider_transport_preferences",
                [],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
            0
        );
    }
}
