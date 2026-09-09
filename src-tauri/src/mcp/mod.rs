pub mod config;
pub(crate) mod executable;
pub mod runtime;
mod stdio;
#[cfg(target_os = "windows")]
mod windows_secrets;

use crate::persistence::{AppState, PersistenceError};
use config::Config;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::{
    path::Path,
    sync::{Arc, Mutex},
};
use tauri::Manager as _;

#[derive(Debug, Clone, Serialize)]
pub struct McpError {
    pub code: &'static str,
    pub message: String,
}
pub fn error(message: &str) -> McpError {
    McpError {
        code: "mcp_error",
        message: message.into(),
    }
}
impl From<PersistenceError> for McpError {
    fn from(_: PersistenceError) -> Self {
        storage_error()
    }
}
impl From<rusqlite::Error> for McpError {
    fn from(_: rusqlite::Error) -> Self {
        storage_error()
    }
}
fn storage_error() -> McpError {
    error("Não foi possível acessar as configurações dos MCPs.")
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Server {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub enabled: bool,
    pub configured: bool,
    pub revision: i64,
    pub last_check: Option<Check>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Check {
    pub tool_count: usize,
    pub tools: Vec<String>,
    pub error: Option<String>,
}

pub(crate) trait Secrets: Send + Sync {
    fn load(&self, key: &str) -> Result<String, McpError>;
    fn store(&self, key: &str, value: &str) -> Result<(), McpError>;
    fn delete(&self, key: &str) -> Result<(), McpError>;
}
pub(crate) struct Keychain;
#[cfg(target_os = "macos")]
impl Secrets for Keychain {
    fn load(&self, key: &str) -> Result<String, McpError> {
        let bytes =
            security_framework::passwords::get_generic_password("com.foxtag.jarvis.mcp", key)
                .map_err(|_| error("Não foi possível ler a configuração no Keychain."))?;
        String::from_utf8(bytes).map_err(|_| storage_error())
    }
    fn store(&self, key: &str, value: &str) -> Result<(), McpError> {
        security_framework::passwords::set_generic_password(
            "com.foxtag.jarvis.mcp",
            key,
            value.as_bytes(),
        )
        .map_err(|_| error("Não foi possível salvar a configuração no Keychain."))
    }
    fn delete(&self, key: &str) -> Result<(), McpError> {
        match security_framework::passwords::delete_generic_password("com.foxtag.jarvis.mcp", key) {
            Ok(()) => Ok(()),
            Err(err) if err.code() == -25300 => Ok(()),
            Err(_) => Err(error(
                "Não foi possível remover a configuração do Keychain.",
            )),
        }
    }
}
#[cfg(target_os = "windows")]
impl Secrets for Keychain {
    fn load(&self, key: &str) -> Result<String, McpError> {
        windows_secrets::load(key)
    }
    fn store(&self, key: &str, value: &str) -> Result<(), McpError> {
        windows_secrets::store(key, value)
    }
    fn delete(&self, key: &str) -> Result<(), McpError> {
        windows_secrets::delete(key)
    }
}
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
impl Secrets for Keychain {
    fn load(&self, _: &str) -> Result<String, McpError> {
        Err(error(
            "O armazenamento seguro de MCPs está disponível no macOS.",
        ))
    }
    fn store(&self, _: &str, _: &str) -> Result<(), McpError> {
        Err(error(
            "O armazenamento seguro de MCPs está disponível no macOS.",
        ))
    }
    fn delete(&self, _: &str) -> Result<(), McpError> {
        Err(error(
            "O armazenamento seguro de MCPs está disponível no macOS.",
        ))
    }
}

struct Manager {
    guard: Mutex<()>,
    secrets: Arc<dyn Secrets>,
}
#[derive(Clone)]
pub struct McpState(Arc<Manager>);
impl Default for McpState {
    fn default() -> Self {
        Self(Arc::new(Manager {
            guard: Mutex::new(()),
            secrets: Arc::new(Keychain),
        }))
    }
}
fn rows(connection: &Connection) -> Result<Vec<Server>, McpError> {
    let mut statement = connection.prepare("SELECT id, name, kind, enabled, configured, revision, last_check FROM mcp_servers ORDER BY created_at, name")?;
    let rows = statement.query_map([], |row| {
        Ok(Server {
            id: row.get(0)?,
            name: row.get(1)?,
            kind: row.get(2)?,
            enabled: row.get(3)?,
            configured: row.get(4)?,
            revision: row.get(5)?,
            last_check: row
                .get::<_, Option<String>>(6)?
                .and_then(|raw| serde_json::from_str(&raw).ok()),
        })
    })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}
fn key(server: &Server) -> String {
    format!("{}:{}", server.id, server.revision)
}
fn find(connection: &Connection, id: &str) -> Result<Server, McpError> {
    rows(connection)?
        .into_iter()
        .find(|server| server.id == id)
        .ok_or_else(|| error("Este MCP não está mais cadastrado."))
}
impl McpState {
    pub fn list(&self, state: &AppState, home: &Path) -> Result<Vec<Server>, McpError> {
        state.with_connection(home, |connection| rows(connection))
    }
    fn config(&self, server: &Server) -> Result<Config, McpError> {
        let raw = if server.id == "builtin-context7" && server.revision == 0 {
            config::TEMPLATE.into()
        } else {
            self.0.secrets.load(&key(server))?
        };
        let (_, mut config) = config::parse(&raw)?;
        config.set_enabled(server.enabled);
        Ok(config)
    }
    fn edit(&self, state: &AppState, home: &Path, id: &str) -> Result<String, McpError> {
        let _guard = self.0.guard.lock().map_err(|_| storage_error())?;
        let server = state.with_connection(home, |connection| find(connection, id))?;
        Ok(self.config(&server)?.named(&server.name))
    }
    fn save(
        &self,
        state: &AppState,
        home: &Path,
        id: Option<&str>,
        raw: &str,
    ) -> Result<Vec<Server>, McpError> {
        let (name, config) = config::parse(raw)?;
        let _guard = self.0.guard.lock().map_err(|_| storage_error())?;
        state.with_connection(home, |connection| {
            let transaction = connection.transaction()?;
            let previous = id.map(|id| find(&transaction, id)).transpose()?;
            if previous.is_none() && rows(&transaction)?.len() >= 32 { return Err(error("Você pode cadastrar até 32 MCPs.")); }
            let duplicate: Option<String> = transaction.query_row("SELECT id FROM mcp_servers WHERE name = ?1", [&name], |row| row.get(0)).optional()?;
            if duplicate.as_deref().is_some_and(|existing| Some(existing) != id) { return Err(error("Já existe um MCP com esse nome.")); }
            let mut bytes = [0_u8; 16];
            getrandom::fill(&mut bytes).map_err(|_| storage_error())?;
            let server = Server {
                id: previous.as_ref().map(|s| s.id.clone()).unwrap_or_else(|| bytes.iter().map(|b| format!("{b:02x}")).collect()),
                name, kind: config.kind().into(), enabled: config.enabled(), configured: config.configured(),
                revision: previous.as_ref().map_or(1, |s| s.revision + 1), last_check: None,
            };
            // Versioned secrets make the committed DB revision the only active config.
            self.0.secrets.store(&key(&server), &config.named(&server.name))?;
            let update = (|| -> Result<(), McpError> {
                transaction.execute("INSERT INTO mcp_servers (id, name, kind, enabled, configured, revision) VALUES (?1,?2,?3,?4,?5,?6) ON CONFLICT(id) DO UPDATE SET name=excluded.name, kind=excluded.kind, enabled=excluded.enabled, configured=excluded.configured, revision=excluded.revision, last_check=NULL", params![server.id, server.name, server.kind, server.enabled, server.configured, server.revision])?;
                transaction.commit()?;
                Ok(())
            })();
            if update.is_err() { let _ = self.0.secrets.delete(&key(&server)); }
            update?;
            if let Some(previous) = previous.filter(|s| s.revision > 0) { let _ = self.0.secrets.delete(&key(&previous)); }
            Ok::<_, McpError>(())
        })?;
        self.list(state, home)
    }
    fn set_enabled(
        &self,
        state: &AppState,
        home: &Path,
        id: &str,
        enabled: bool,
    ) -> Result<Vec<Server>, McpError> {
        let _guard = self.0.guard.lock().map_err(|_| storage_error())?;
        state.with_connection(home, |connection| {
            if connection.execute(
                "UPDATE mcp_servers SET enabled = ?2 WHERE id = ?1",
                params![id, enabled],
            )? != 1
            {
                return Err(error("Este MCP não está mais cadastrado."));
            }
            Ok::<_, McpError>(())
        })?;
        self.list(state, home)
    }
    fn remove(&self, state: &AppState, home: &Path, id: &str) -> Result<Vec<Server>, McpError> {
        let _guard = self.0.guard.lock().map_err(|_| storage_error())?;
        let server = state.with_connection(home, |connection| {
            let transaction = connection.transaction()?;
            let server = find(&transaction, id)?;
            transaction.execute("DELETE FROM mcp_servers WHERE id = ?1", [id])?;
            transaction.commit()?;
            Ok::<_, McpError>(server)
        })?;
        // SQLite owns the MCP registration. A stale credential cannot be used
        // after its record is gone, so Keychain cleanup must not block removal.
        // This also lets users remove legacy Context7 MCP registrations whose
        // old Keychain item is no longer accessible to the current build.
        if server.revision > 0 {
            let _ = self.0.secrets.delete(&key(&server));
        }
        self.list(state, home)
    }
    pub fn active_configs(
        &self,
        state: &AppState,
        home: &Path,
    ) -> Result<Vec<(Server, Config)>, McpError> {
        let _guard = self.0.guard.lock().map_err(|_| storage_error())?;
        let mut active = Vec::new();
        for server in self
            .list(state, home)?
            .into_iter()
            .filter(|server| server.enabled && server.configured)
        {
            match self.config(&server) {
                Ok(config) => active.push((server, config)),
                Err(err) => self.record_check(
                    state,
                    home,
                    &server,
                    Check {
                        tool_count: 0,
                        tools: vec![],
                        error: Some(err.message),
                    },
                ),
            }
        }
        Ok(active)
    }

    pub(crate) fn backup_configs(
        &self,
        state: &AppState,
        home: &Path,
    ) -> Result<Vec<String>, McpError> {
        let _guard = self.0.guard.lock().map_err(|_| storage_error())?;
        let servers = state.with_connection(home, |connection| rows(connection))?;
        servers
            .iter()
            .map(|server| self.config(server).map(|config| config.named(&server.name)))
            .collect()
    }

    pub(crate) fn replace_from_backup(
        &self,
        state: &AppState,
        home: &Path,
        raw_configs: &[String],
        model_targets: &[String],
    ) -> Result<Vec<Server>, McpError> {
        if raw_configs.len() > 32 {
            return Err(error("O backup contém mais de 32 MCPs."));
        }
        let mut names = std::collections::BTreeSet::new();
        let mut imported = Vec::with_capacity(raw_configs.len());
        for raw in raw_configs {
            let (name, config) = config::parse(raw)?;
            if !names.insert(name.clone()) {
                return Err(error("O backup contém MCPs com nomes repetidos."));
            }
            let mut bytes = [0_u8; 16];
            getrandom::fill(&mut bytes).map_err(|_| storage_error())?;
            imported.push((
                Server {
                    id: bytes.iter().map(|byte| format!("{byte:02x}")).collect(),
                    name,
                    kind: config.kind().into(),
                    enabled: config.enabled(),
                    configured: config.configured(),
                    revision: 1,
                    last_check: None,
                },
                config,
            ));
        }

        let _guard = self.0.guard.lock().map_err(|_| storage_error())?;
        let previous = state.with_connection(home, |connection| rows(connection))?;
        let mut stored: Vec<String> = Vec::new();
        for (server, config) in &imported {
            if let Err(cause) = self
                .0
                .secrets
                .store(&key(server), &config.named(&server.name))
            {
                for key in &stored {
                    let _ = self.0.secrets.delete(key);
                }
                return Err(cause);
            }
            stored.push(key(server));
        }

        let update = state.with_connection(home, |connection| {
            let transaction = connection.transaction()?;
            transaction.execute("DELETE FROM mcp_servers", [])?;
            for (server, _) in &imported {
                transaction.execute(
                    "INSERT INTO mcp_servers (id, name, kind, enabled, configured, revision) VALUES (?1,?2,?3,?4,?5,?6)",
                    params![
                        server.id,
                        server.name,
                        server.kind,
                        server.enabled,
                        server.configured,
                        server.revision
                    ],
                )?;
            }
            for target in model_targets {
                transaction.execute(
                    "DELETE FROM provider_model_bindings WHERE item_key = ?1",
                    [target],
                )?;
            }
            if !model_targets.is_empty() {
                transaction.execute(
                    "UPDATE provider_bindings_revision SET revision = revision + 1 WHERE id = 1",
                    [],
                )?;
            }
            transaction.commit()?;
            Ok::<_, McpError>(())
        });
        if let Err(cause) = update {
            for key in &stored {
                let _ = self.0.secrets.delete(key);
            }
            return Err(cause);
        }
        for server in previous.into_iter().filter(|server| server.revision > 0) {
            let _ = self.0.secrets.delete(&key(&server));
        }
        self.list(state, home)
    }

    pub fn current(&self, state: &AppState, home: &Path, server: &Server) -> bool {
        state
            .with_connection(home, |connection| find(connection, &server.id))
            .is_ok_and(|current| {
                current.enabled && current.configured && current.revision == server.revision
            })
    }
    pub fn record_check(&self, state: &AppState, home: &Path, server: &Server, check: Check) {
        let Ok(raw) = serde_json::to_string(&check) else {
            return;
        };
        let _ = state.with_connection(home, |connection| {
            connection.execute(
                "UPDATE mcp_servers SET last_check = ?3 WHERE id = ?1 AND revision = ?2",
                params![server.id, server.revision, raw],
            )?;
            Ok::<_, McpError>(())
        });
    }
}

#[tauri::command]
pub async fn list_mcp_servers(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    mcp: tauri::State<'_, McpState>,
) -> Result<Vec<Server>, McpError> {
    let home = app.path().home_dir().map_err(|_| storage_error())?;
    let state = state.inner().clone();
    let mcp = mcp.inner().clone();
    tauri::async_runtime::spawn_blocking(move || mcp.list(&state, &home))
        .await
        .map_err(|_| storage_error())?
}
#[tauri::command]
pub async fn get_mcp_config(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    mcp: tauri::State<'_, McpState>,
    id: String,
) -> Result<String, McpError> {
    let home = app.path().home_dir().map_err(|_| storage_error())?;
    let state = state.inner().clone();
    let mcp = mcp.inner().clone();
    tauri::async_runtime::spawn_blocking(move || mcp.edit(&state, &home, &id))
        .await
        .map_err(|_| storage_error())?
}
#[tauri::command]
pub async fn save_mcp_server(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    mcp: tauri::State<'_, McpState>,
    id: Option<String>,
    config: String,
) -> Result<Vec<Server>, McpError> {
    let home = app.path().home_dir().map_err(|_| storage_error())?;
    let state = state.inner().clone();
    let mcp = mcp.inner().clone();
    tauri::async_runtime::spawn_blocking(move || mcp.save(&state, &home, id.as_deref(), &config))
        .await
        .map_err(|_| storage_error())?
}
#[tauri::command]
pub async fn set_mcp_enabled(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    mcp: tauri::State<'_, McpState>,
    id: String,
    enabled: bool,
) -> Result<Vec<Server>, McpError> {
    let home = app.path().home_dir().map_err(|_| storage_error())?;
    let state = state.inner().clone();
    let mcp = mcp.inner().clone();
    tauri::async_runtime::spawn_blocking(move || mcp.set_enabled(&state, &home, &id, enabled))
        .await
        .map_err(|_| storage_error())?
}
#[tauri::command]
pub async fn delete_mcp_server(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    mcp: tauri::State<'_, McpState>,
    id: String,
) -> Result<Vec<Server>, McpError> {
    let home = app.path().home_dir().map_err(|_| storage_error())?;
    let state = state.inner().clone();
    let mcp = mcp.inner().clone();
    tauri::async_runtime::spawn_blocking(move || mcp.remove(&state, &home, &id))
        .await
        .map_err(|_| storage_error())?
}

#[cfg(test)]
mod tests;
