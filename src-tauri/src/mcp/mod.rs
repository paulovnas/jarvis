pub mod config;
pub(crate) mod executable;
pub(crate) mod oauth;
pub mod runtime;
mod stdio;
#[cfg(any(target_os = "windows", target_os = "linux"))]
mod vault_secrets;

use crate::persistence::{AppState, PersistenceError};
use config::Config;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::{
    path::Path,
    sync::{Arc, Mutex},
};
use tauri::Manager as _;

type PluginConfigs = std::collections::HashMap<String, (Server, Config)>;
type PluginOwners = std::collections::HashMap<String, String>;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct McpValidationIssue {
    pub path: String,
    pub keyword: String,
    pub message: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct McpIntent {
    #[serde(default)]
    pub mode: McpIntentMode,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub servers: Vec<McpIntentServer>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub excluded_servers: Vec<McpIntentServer>,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum McpIntentMode {
    #[default]
    OnDemand,
    Explicit,
    Disabled,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct McpIntentServer {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpErrorMetadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool: Option<String>,
    pub retryable: bool,
    pub outcome_uncertain: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub connection_recovered: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recovery_error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_error_code: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_error_data: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub validation_errors: Vec<McpValidationIssue>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpError {
    pub code: &'static str,
    pub message: String,
    #[serde(flatten)]
    pub metadata: Box<McpErrorMetadata>,
}
impl McpError {
    pub(crate) fn tool_result(&self) -> String {
        serde_json::to_string(&serde_json::json!({
            "ok": false,
            "error": self,
        }))
        .unwrap_or_else(|_| {
            format!(
                r#"{{"ok":false,"error":{{"code":"{}","message":"{}"}}}}"#,
                self.code, "Falha MCP sem detalhes serializáveis."
            )
        })
    }
}
pub fn error(message: &str) -> McpError {
    coded_error("mcp_error", message)
}
pub(crate) fn coded_error(code: &'static str, message: &str) -> McpError {
    McpError {
        code,
        message: message.into(),
        metadata: Box::default(),
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
    fn load_optional(&self, key: &str) -> Result<Option<String>, McpError> {
        self.load(key).map(Some)
    }
    fn store(&self, key: &str, value: &str) -> Result<(), McpError>;
    fn delete(&self, key: &str) -> Result<(), McpError>;
}
pub(crate) struct Keychain;
#[cfg(target_os = "macos")]
fn keychain_service() -> &'static str {
    crate::data_dir::keychain_service("com.foxtag.jarvis.mcp", "com.foxtag.jarvis.dev.mcp")
}
#[cfg(target_os = "macos")]
impl Secrets for Keychain {
    fn load_optional(&self, key: &str) -> Result<Option<String>, McpError> {
        match security_framework::passwords::get_generic_password(keychain_service(), key) {
            Ok(bytes) => String::from_utf8(bytes)
                .map(Some)
                .map_err(|_| storage_error()),
            Err(error) if error.code() == -25300 => Ok(None),
            Err(_) => Err(storage_error()),
        }
    }
    fn load(&self, key: &str) -> Result<String, McpError> {
        let bytes = security_framework::passwords::get_generic_password(keychain_service(), key)
            .map_err(|_| error("Não foi possível ler a configuração no Keychain."))?;
        String::from_utf8(bytes).map_err(|_| storage_error())
    }
    fn store(&self, key: &str, value: &str) -> Result<(), McpError> {
        security_framework::passwords::set_generic_password(
            keychain_service(),
            key,
            value.as_bytes(),
        )
        .map_err(|_| error("Não foi possível salvar a configuração no Keychain."))
    }
    fn delete(&self, key: &str) -> Result<(), McpError> {
        match security_framework::passwords::delete_generic_password(keychain_service(), key) {
            Ok(()) => Ok(()),
            Err(err) if err.code() == -25300 => Ok(()),
            Err(_) => Err(error(
                "Não foi possível remover a configuração do Keychain.",
            )),
        }
    }
}
#[cfg(any(target_os = "windows", target_os = "linux"))]
impl Secrets for Keychain {
    fn load_optional(&self, key: &str) -> Result<Option<String>, McpError> {
        vault_secrets::load_optional(key)
    }
    fn load(&self, key: &str) -> Result<String, McpError> {
        vault_secrets::load(key)
    }
    fn store(&self, key: &str, value: &str) -> Result<(), McpError> {
        vault_secrets::store(key, value)
    }
    fn delete(&self, key: &str) -> Result<(), McpError> {
        vault_secrets::delete(key)
    }
}
#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
impl Secrets for Keychain {
    fn load(&self, _: &str) -> Result<String, McpError> {
        Err(error(
            "O armazenamento seguro de MCPs não está disponível neste sistema.",
        ))
    }
    fn store(&self, _: &str, _: &str) -> Result<(), McpError> {
        Err(error(
            "O armazenamento seguro de MCPs não está disponível neste sistema.",
        ))
    }
    fn delete(&self, _: &str) -> Result<(), McpError> {
        Err(error(
            "O armazenamento seguro de MCPs não está disponível neste sistema.",
        ))
    }
}

struct Manager {
    guard: Mutex<()>,
    secrets: Arc<dyn Secrets>,
    apps_context: Mutex<Option<crate::plugins::apps::Context>>,
}
#[derive(Clone)]
pub struct McpState(Arc<Manager>);
impl Default for McpState {
    fn default() -> Self {
        Self(Arc::new(Manager {
            guard: Mutex::new(()),
            secrets: Arc::new(Keychain),
            apps_context: Mutex::new(None),
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
    pub(crate) fn configure_apps(&self, context: crate::plugins::apps::Context) {
        if let Ok(mut stored) = self.0.apps_context.lock() {
            *stored = Some(context);
        }
    }
    pub(crate) fn apps_context(&self) -> Option<crate::plugins::apps::Context> {
        self.0
            .apps_context
            .lock()
            .ok()
            .and_then(|value| value.clone())
    }

    pub fn list(&self, state: &AppState, home: &Path) -> Result<Vec<Server>, McpError> {
        self.list_for_project(state, home, None)
    }
    pub(crate) fn list_for_project(
        &self,
        state: &AppState,
        home: &Path,
        project: Option<&Path>,
    ) -> Result<Vec<Server>, McpError> {
        let mut servers = state.with_connection(home, |connection| rows(connection))?;
        let overlay =
            crate::plugins::load_active_for_project(home, project).map_err(|_| storage_error())?;
        for contribution in overlay.mcp_servers {
            let configured = self
                .plugin_config(&contribution)
                .is_ok_and(|value| value.configured());
            servers.push(plugin_server(&contribution, configured));
        }
        servers.extend(
            crate::plugins::apps::servers(home, project)?
                .into_iter()
                .map(|(server, _)| server),
        );
        Ok(servers)
    }
    #[cfg(test)]
    pub(crate) fn plugin_configs(
        &self,
        home: &Path,
        project: &Path,
    ) -> Result<std::collections::HashMap<String, (Server, Config)>, McpError> {
        self.plugin_configs_with_owners(home, project)
            .map(|(configs, _)| configs)
    }
    pub(crate) fn plugin_configs_with_owners(
        &self,
        home: &Path,
        project: &Path,
    ) -> Result<(PluginConfigs, PluginOwners), McpError> {
        let overlay = crate::plugins::load_active_for_project(home, Some(project))
            .map_err(|_| storage_error())?;
        let mut owners: std::collections::HashMap<_, _> = overlay
            .mcp_servers
            .iter()
            .map(|source| {
                (
                    plugin_server_id(&source.plugin_id, &source.component_id),
                    source.plugin_id.clone(),
                )
            })
            .collect();
        owners.extend(overlay.apps.iter().map(|source| {
            (
                crate::plugins::apps::server_id(&source.plugin_id),
                source.plugin_id.clone(),
            )
        }));
        let mut configs: std::collections::HashMap<_, _> = overlay
            .mcp_servers
            .into_iter()
            .filter_map(|source| {
                self.plugin_config(&source).ok().map(|config| {
                    let server = plugin_server(&source, config.configured());
                    (server.id.clone(), (server, config))
                })
            })
            .collect();
        configs.extend(
            crate::plugins::apps::servers(home, Some(project))?
                .into_iter()
                .map(|(server, config)| (server.id.clone(), (server, config))),
        );
        Ok((configs, owners))
    }

    fn plugin_config(&self, source: &crate::plugins::McpContribution) -> Result<Config, McpError> {
        let key = plugin_values_key(source);
        let values = match self.0.secrets.load_optional(&key)? {
            Some(raw) if raw.len() <= 64 * 1024 => {
                serde_json::from_str(&raw).map_err(|_| storage_error())?
            }
            Some(_) => return Err(storage_error()),
            None => std::collections::BTreeMap::new(),
        };
        config::from_plugin_with_values(source, &values)
    }
    fn config(&self, home: &Path, server: &Server) -> Result<Config, McpError> {
        if server.id.starts_with("plugin-app:") {
            return crate::plugins::apps::servers(home, None)?
                .into_iter()
                .find(|(entry, _)| entry.id == server.id)
                .map(|(_, config)| config)
                .ok_or_else(|| {
                    error("O aplicativo deste plugin está desativado ou foi removido.")
                });
        }
        if server.id.starts_with("plugin-mcp:") {
            let source = crate::plugins::load_active(home)
                .map_err(|_| storage_error())?
                .mcp_servers
                .into_iter()
                .find(|source| {
                    plugin_server_id(&source.plugin_id, &source.component_id) == server.id
                })
                .ok_or_else(|| error("O plugin deste MCP está desativado ou foi removido."))?;
            return self.plugin_config(&source);
        }
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
        if id.starts_with("plugin-") {
            return Err(coded_error(
                "plugin_owned_mcp",
                "Gerencie este MCP na seção Plugins.",
            ));
        }
        let _guard = self.0.guard.lock().map_err(|_| storage_error())?;
        let server = state.with_connection(home, |connection| find(connection, id))?;
        Ok(self.config(home, &server)?.named(&server.name))
    }
    pub(crate) fn add(
        &self,
        state: &AppState,
        home: &Path,
        name: &str,
        config: &Config,
    ) -> Result<Server, McpError> {
        self.save(state, home, None, &config.named(name))?
            .into_iter()
            .find(|server| server.name == name)
            .ok_or_else(storage_error)
    }
    fn save(
        &self,
        state: &AppState,
        home: &Path,
        id: Option<&str>,
        raw: &str,
    ) -> Result<Vec<Server>, McpError> {
        if id.is_some_and(|id| id.starts_with("plugin-")) {
            return Err(coded_error(
                "plugin_owned_mcp",
                "Gerencie este MCP na seção Plugins.",
            ));
        }
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
        if id.starts_with("plugin-") {
            return Err(coded_error(
                "plugin_owned_mcp",
                "Gerencie este MCP na seção Plugins.",
            ));
        }
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
        if id.starts_with("plugin-") {
            return Err(coded_error(
                "plugin_owned_mcp",
                "Gerencie este MCP na seção Plugins.",
            ));
        }
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
    #[cfg(test)]
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
            match self.config(home, &server) {
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

    pub(crate) fn active_config(
        &self,
        state: &AppState,
        home: &Path,
        expected: &Server,
    ) -> Result<Option<(Server, Config)>, McpError> {
        let _guard = self.0.guard.lock().map_err(|_| storage_error())?;
        let current = self
            .list(state, home)?
            .into_iter()
            .find(|server| server.id == expected.id);
        let Some(server) = current.filter(|server| {
            server.enabled && server.configured && server.revision == expected.revision
        }) else {
            return Ok(None);
        };
        let config = self.config(home, &server)?;
        Ok(Some((server, config)))
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
            .map(|server| {
                self.config(home, server)
                    .map(|config| config.named(&server.name))
            })
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

    pub(crate) fn current_for_project(
        &self,
        state: &AppState,
        home: &Path,
        project: &Path,
        server: &Server,
    ) -> bool {
        if server.id.starts_with("plugin-") {
            return self
                .list_for_project(state, home, Some(project))
                .is_ok_and(|servers| {
                    servers
                        .iter()
                        .any(|item| item.id == server.id && item.enabled && item.configured)
                });
        }
        self.current(state, home, server)
    }
    pub fn current(&self, state: &AppState, home: &Path, server: &Server) -> bool {
        if server.id.starts_with("plugin-") {
            return self.list(state, home).is_ok_and(|servers| {
                servers
                    .iter()
                    .any(|item| item.id == server.id && item.enabled && item.configured)
            });
        }
        state
            .with_connection(home, |connection| find(connection, &server.id))
            .is_ok_and(|current| {
                current.enabled && current.configured && current.revision == server.revision
            })
    }
    pub(crate) fn frozen_config_current(
        &self,
        state: &AppState,
        home: &Path,
        project: &Path,
        server: &Server,
        config: &Config,
    ) -> bool {
        if !self.current_for_project(state, home, project, server) {
            return false;
        }
        if !server.id.starts_with("plugin-mcp:") {
            return true;
        }
        let Config::Local { environment, .. } = config else {
            return true;
        };
        let Some(root) = environment.get("CODEX_PLUGIN_ROOT").map(Path::new) else {
            return false;
        };
        let Some(hash) = root
            .file_name()
            .and_then(|name| name.to_str())
            .and_then(|name| name.get(..64))
        else {
            return false;
        };
        let Some((plugin, name)) = server.name.split_once(": ") else {
            return false;
        };
        let component = format!("mcp:{name}");
        plugin_server_id(plugin, &component) == server.id
            && crate::plugins::frozen_component_authorized(
                home,
                Some(project),
                plugin,
                &component,
                hash,
                root,
            )
    }
    pub fn record_check(&self, state: &AppState, home: &Path, server: &Server, check: Check) {
        if server.id.starts_with("plugin-") {
            return;
        }
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

/// Stable logical IDs; changing a version changes the revision, not user selection.
pub(crate) fn plugin_server_id(plugin: &str, component: &str) -> String {
    use sha2::{Digest, Sha256};
    format!(
        "plugin-mcp:{:x}",
        Sha256::digest(format!("{plugin}\0{component}").as_bytes())
    )
}
fn plugin_server(source: &crate::plugins::McpContribution, configured: bool) -> Server {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(source.plugin_hash.as_bytes());
    let revision = i64::from_le_bytes(digest[..8].try_into().unwrap_or_default()) & i64::MAX;
    Server {
        id: plugin_server_id(&source.plugin_id, &source.component_id),
        name: format!("{}: {}", source.plugin_id, source.name),
        kind: if source.definition.get("url").is_some() {
            "remote"
        } else {
            "local"
        }
        .into(),
        enabled: true,
        configured,
        revision,
        last_check: None,
    }
}

fn plugin_values_key(source: &crate::plugins::McpContribution) -> String {
    format!(
        "plugin-values:{}:{}",
        plugin_server_id(&source.plugin_id, &source.component_id),
        source.plugin_hash
    )
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PluginMcpRequirements {
    fields: Vec<String>,
    configured: bool,
}
fn contribution(home: &Path, id: &str) -> Result<crate::plugins::McpContribution, McpError> {
    crate::plugins::load_active(home)
        .map_err(|_| storage_error())?
        .mcp_servers
        .into_iter()
        .find(|source| plugin_server_id(&source.plugin_id, &source.component_id) == id)
        .ok_or_else(|| {
            coded_error(
                "plugin_mcp_unavailable",
                "Ative o componente MCP do plugin antes de configurá-lo.",
            )
        })
}
#[tauri::command]
pub(crate) async fn plugin_mcp_requirements(
    app: tauri::AppHandle,
    mcp: tauri::State<'_, McpState>,
    id: String,
) -> Result<PluginMcpRequirements, McpError> {
    let home = app.path().home_dir().map_err(|_| storage_error())?;
    let mcp = mcp.inner().clone();
    tokio::task::spawn_blocking(move || {
        let source = contribution(&home, &id)?;
        Ok(PluginMcpRequirements {
            fields: config::plugin_fields(&source),
            configured: mcp.plugin_config(&source).is_ok(),
        })
    })
    .await
    .map_err(|_| storage_error())?
}
#[tauri::command]
pub(crate) async fn configure_plugin_mcp(
    app: tauri::AppHandle,
    mcp: tauri::State<'_, McpState>,
    id: String,
    values: std::collections::BTreeMap<String, String>,
) -> Result<PluginMcpRequirements, McpError> {
    let home = app.path().home_dir().map_err(|_| storage_error())?;
    let mcp = mcp.inner().clone();
    tokio::task::spawn_blocking(move || {
        let _guard = mcp.0.guard.lock().map_err(|_| storage_error())?;
        let source = contribution(&home, &id)?;
        let fields = config::plugin_fields(&source);
        if values.len() > 64
            || values.keys().any(|name| !fields.contains(name))
            || values
                .values()
                .any(|value| value.len() > 16 * 1024 || value.contains('\0'))
        {
            return Err(coded_error(
                "plugin_mcp_configuration",
                "As variáveis privadas são inválidas ou não pertencem a este componente.",
            ));
        }
        let _ = config::from_plugin_with_values(&source, &values)?;
        let raw = serde_json::to_string(&values).map_err(|_| storage_error())?;
        if raw.len() > 64 * 1024 {
            return Err(coded_error(
                "plugin_mcp_configuration",
                "A configuração privada excedeu 64 KiB.",
            ));
        }
        mcp.0.secrets.store(&plugin_values_key(&source), &raw)?;
        Ok(PluginMcpRequirements {
            fields,
            configured: true,
        })
    })
    .await
    .map_err(|_| storage_error())?
}
