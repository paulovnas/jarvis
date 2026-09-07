use std::{
    fmt, fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use tauri::{AppHandle, Manager, State};

const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        sql: include_str!("../../drizzle/0000_heavy_tomas.sql"),
    },
    Migration {
        version: 2,
        sql: include_str!("../../drizzle/0001_nervous_nighthawk.sql"),
    },
    Migration {
        version: 3,
        sql: include_str!("../../drizzle/0002_silky_meltdown.sql"),
    },
    Migration {
        version: 4,
        sql: include_str!("../../drizzle/0003_amazing_sasquatch.sql"),
    },
    Migration {
        version: 5,
        sql: include_str!("../../drizzle/0004_web_search.sql"),
    },
    Migration {
        version: 6,
        sql: include_str!("../../drizzle/0005_nostalgic_shockwave.sql"),
    },
    Migration {
        version: 7,
        sql: include_str!("../../drizzle/0006_mcp_discovery.sql"),
    },
    Migration {
        version: 8,
        sql: include_str!("../../drizzle/0007_antigravity_provider.sql"),
    },
    Migration {
        version: 9,
        sql: include_str!("../../drizzle/0008_slim_sabra.sql"),
    },
    Migration {
        version: 10,
        sql: include_str!("../../drizzle/0009_provider_usage.sql"),
    },
    Migration {
        version: 11,
        sql: include_str!("../../drizzle/0010_tool_models.sql"),
    },
    Migration {
        version: 12,
        sql: include_str!("../../drizzle/0011_tricky_spiral.sql"),
    },
    Migration {
        version: 13,
        sql: include_str!("../../drizzle/0012_sudden_nehzno.sql"),
    },
    Migration { version: 14, sql: include_str!("../../drizzle/0013_context7_core.sql") },
    Migration { version: 15, sql: include_str!("../../drizzle/0014_image_generation.sql") },
    Migration { version: 16, sql: include_str!("../../drizzle/0015_unread_conversations.sql") },
];

#[test]
fn custom_migration_preserves_oauth_accounts_and_both_tool_selections() {
    let mut db = Connection::open_in_memory().unwrap();
    for migration in MIGRATIONS.iter().take(12) {
        db.execute_batch(migration.sql).unwrap();
    }
    db.execute_batch("INSERT INTO provider_accounts(alias,provider_kind,account_id,enabled,show_usage) VALUES ('openai-codex-old','openai-codex','old',0,0); INSERT INTO web_search_config(id,account_alias,model,inherit_chat) VALUES (1,'openai-codex-old','chosen',0); INSERT INTO vision_config(id,account_alias,model,inherit_chat) VALUES (1,'openai-codex-old','vision',0); PRAGMA user_version=12;").unwrap();
    initialize_database(&mut db).unwrap();
    for table in ["web_search_config", "vision_config"] {
        assert_eq!(
            db.query_row(&format!("SELECT account_alias FROM {table}"), [], |r| {
                r.get::<_, String>(0)
            })
            .unwrap(),
            "openai-codex-old"
        );
    }
    assert_eq!(
        db.query_row(
            "SELECT enabled + show_usage FROM provider_accounts",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
    assert_eq!(
        db.query_row("SELECT count(*) FROM pragma_foreign_key_check", [], |r| r
            .get::<_, i64>(
            0
        ))
        .unwrap(),
        0
    );
}

#[derive(Debug)]
struct Migration {
    version: i64,
    sql: &'static str,
}

#[derive(Clone, Default)]
pub struct AppState {
    connection: Arc<Mutex<Option<Connection>>>,
}

#[derive(Debug, Serialize, Clone, PartialEq, Eq)]
pub struct AppConfig {
    #[serde(rename = "onboardingCompleted")]
    pub onboarding_completed: bool,
}

#[derive(Debug, Serialize)]
pub struct PersistenceError {
    pub message: String,
}

impl PersistenceError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for PersistenceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.message.fmt(formatter)
    }
}

impl std::error::Error for PersistenceError {}

impl From<rusqlite::Error> for PersistenceError {
    fn from(error: rusqlite::Error) -> Self {
        Self::new(format!("SQLite error: {error}"))
    }
}

impl From<std::io::Error> for PersistenceError {
    fn from(error: std::io::Error) -> Self {
        Self::new(format!("Database filesystem error: {error}"))
    }
}

pub(crate) fn database_path(home_dir: &Path) -> PathBuf {
    home_dir.join(".jarvis").join("jarvis.db")
}

fn open_database(path: &Path) -> Result<Connection, PersistenceError> {
    let parent = path
        .parent()
        .ok_or_else(|| PersistenceError::new("Database path has no parent directory"))?;
    fs::create_dir_all(parent)?;
    let mut connection = Connection::open(path)?;
    initialize_database(&mut connection)?;
    Ok(connection)
}

pub(crate) fn initialize_database(connection: &mut Connection) -> Result<(), PersistenceError> {
    connection.pragma_update(None, "foreign_keys", true)?;
    let mut current_version =
        connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
    let latest_version = MIGRATIONS.last().map_or(0, |migration| migration.version);
    if current_version > latest_version {
        return Err(PersistenceError::new(format!(
            "Database schema version {current_version} is newer than the supported version {latest_version}"
        )));
    }

    for migration in MIGRATIONS {
        if migration.version <= current_version {
            continue;
        }
        if migration.version != current_version + 1 {
            return Err(PersistenceError::new(format!(
                "Missing database migration for version {}",
                current_version + 1
            )));
        }

        let transaction = connection.transaction()?;
        transaction.execute_batch(migration.sql)?;
        transaction.pragma_update(None, "user_version", migration.version)?;
        ensure_app_config_row(&transaction)?;
        transaction.commit()?;
        current_version = migration.version;
    }

    if current_version == latest_version {
        let transaction = connection.transaction()?;
        ensure_app_config_row(&transaction)?;
        transaction.commit()?;
    }

    Ok(())
}

fn ensure_app_config_row(connection: &Connection) -> Result<(), PersistenceError> {
    connection.execute(
        "INSERT INTO app_config (id) VALUES (?1) ON CONFLICT(id) DO NOTHING",
        params![1_i64],
    )?;
    Ok(())
}

fn read_app_config(connection: &Connection) -> Result<AppConfig, PersistenceError> {
    connection
        .query_row(
            "SELECT onboarding_completed FROM app_config WHERE id = ?1",
            params![1_i64],
            |row| {
                Ok(AppConfig {
                    onboarding_completed: row.get(0)?,
                })
            },
        )
        .map_err(Into::into)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProviderAccountRecord {
    pub(crate) alias: String,
    pub(crate) provider_kind: String,
    pub(crate) account_id: String,
    pub(crate) created_at: i64,
    pub(crate) enabled: bool,
    pub(crate) show_usage: bool,
    pub(crate) show_third_party_usage: bool,
}

fn provider_account_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ProviderAccountRecord> {
    Ok(ProviderAccountRecord {
        alias: row.get(0)?,
        provider_kind: row.get(1)?,
        account_id: row.get(2)?,
        created_at: row.get(3)?,
        enabled: row.get(4)?,
        show_usage: row.get(5)?,
        show_third_party_usage: row.get(6)?,
    })
}

pub(crate) fn list_provider_accounts(
    connection: &Connection,
) -> Result<Vec<ProviderAccountRecord>, PersistenceError> {
    let mut statement = connection.prepare(
        "SELECT alias, provider_kind, account_id, created_at, enabled, show_usage, show_third_party_usage
         FROM provider_accounts
         ORDER BY created_at, alias",
    )?;
    let rows = statement.query_map([], provider_account_from_row)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

pub(crate) fn provider_account_exists(
    connection: &Connection,
    alias: &str,
    account_id: &str,
) -> Result<bool, PersistenceError> {
    connection
        .query_row(
            "SELECT EXISTS(
                 SELECT 1
                 FROM provider_accounts
                 WHERE alias = ?1 OR account_id = ?2
             )",
            params![alias, account_id],
            |row| row.get(0),
        )
        .map_err(Into::into)
}

pub(crate) fn insert_provider_account(
    connection: &Connection,
    alias: &str,
    account_id: &str,
) -> Result<ProviderAccountRecord, PersistenceError> {
    connection.execute(
        "INSERT INTO provider_accounts (alias, provider_kind, account_id)
         VALUES (?1, ?3, ?2)",
        params![alias, account_id, if alias.starts_with("antigravity-") { "antigravity" } else { "openai-codex" }],
    )?;
    connection
        .query_row(
            "SELECT alias, provider_kind, account_id, created_at, enabled, show_usage, show_third_party_usage
             FROM provider_accounts
             WHERE alias = ?1",
            params![alias],
            provider_account_from_row,
        )
        .map_err(Into::into)
}

pub(crate) fn delete_provider_account(
    connection: &Connection,
    alias: &str,
) -> Result<(), PersistenceError> {
    connection.execute(
        "DELETE FROM provider_accounts WHERE alias = ?1",
        params![alias],
    )?;
    Ok(())
}

pub(crate) fn require_enabled_account(
    state: &AppState,
    home: &Path,
    alias: &str,
) -> Result<(), crate::openai_codex::ProviderError> {
    if state
        .list_provider_accounts(home)
        .map_err(|_| {
            crate::openai_codex::ProviderError::new(
                "database_error",
                "Não foi possível verificar a conta.",
            )
        })?
        .iter()
        .any(|record| record.alias == alias && record.enabled)
    {
        Ok(())
    } else {
        Err(crate::openai_codex::ProviderError::new(
            "account_disabled",
            "A conta foi desativada ou desconectada. Ative uma conta nas configurações.",
        ))
    }
}

fn complete_app_config(connection: &mut Connection, workspace_name: &str) -> Result<AppConfig, PersistenceError> {
    let transaction = connection.transaction()?;
    let existing = read_app_config(&transaction)?;
    if existing.onboarding_completed { return Ok(existing); }
    let name = workspace_name.trim();
    let name = if name.is_empty() { "Pessoal" } else { name };
    if name.chars().count() > 120 || name.chars().any(char::is_control) { return Err(PersistenceError::new("Informe um nome de até 120 caracteres, sem quebras de linha.")); }
    if !transaction.query_row("SELECT EXISTS(SELECT 1 FROM provider_accounts WHERE enabled = 1)", [], |row| row.get::<_, bool>(0))? {
        return Err(PersistenceError::new("Conecte um provedor antes de começar."));
    }
    let workspace: Option<String> = transaction.query_row("SELECT id FROM workspaces WHERE name = ?1", [name], |row| row.get(0)).optional()?;
    let workspace = match workspace {
        Some(id) => id,
        None => {
            let id = crate::library::new_id().map_err(|_| PersistenceError::new("Não foi possível criar o workspace."))?;
            transaction.execute("INSERT INTO workspaces(id, name) VALUES (?1, ?2)", params![id, name])?;
            id
        }
    };
    transaction.execute("INSERT INTO navigation_selection(id, workspace_id, project_id, conversation_id) VALUES (1, ?1, NULL, NULL) ON CONFLICT(id) DO UPDATE SET workspace_id = excluded.workspace_id, project_id = NULL, conversation_id = NULL", [&workspace])?;
    let updated = transaction.execute(
        "UPDATE app_config SET onboarding_completed = 1 WHERE id = ?1",
        params![1_i64],
    )?;
    if updated != 1 {
        return Err(PersistenceError::new(
            "The singleton app configuration row is missing",
        ));
    }
    let config = read_app_config(&transaction)?;
    transaction.commit()?;
    Ok(config)
}

impl AppState {
    pub(crate) fn with_connection<T, E>(
        &self,
        home_dir: &Path,
        operation: impl FnOnce(&mut Connection) -> Result<T, E>,
    ) -> Result<T, E>
    where
        E: From<PersistenceError>,
    {
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| PersistenceError::new("Database state lock is poisoned"))?;
        if connection.is_none() {
            *connection = Some(open_database(&database_path(home_dir))?);
        }
        let connection = connection
            .as_mut()
            .ok_or_else(|| PersistenceError::new("Database connection was not initialized"))?;
        operation(connection)
    }

    fn get_app_config(&self, home_dir: &Path) -> Result<AppConfig, PersistenceError> {
        self.with_connection(home_dir, |connection| read_app_config(connection))
    }

    fn complete_onboarding(&self, home_dir: &Path, workspace_name: &str) -> Result<AppConfig, PersistenceError> {
        crate::core::require_ready(home_dir).map_err(|cause| PersistenceError::new(cause.message))?;
        self.with_connection(home_dir, |connection| complete_app_config(connection, workspace_name))
    }

    pub(crate) fn list_provider_accounts(
        &self,
        home_dir: &Path,
    ) -> Result<Vec<ProviderAccountRecord>, PersistenceError> {
        self.with_connection(home_dir, |connection| list_provider_accounts(connection))
    }
}

#[tauri::command]
pub async fn get_app_config(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<AppConfig, PersistenceError> {
    let home_dir = app.path().home_dir().map_err(|error| {
        PersistenceError::new(format!("Unable to resolve home directory: {error}"))
    })?;
    let state = state.inner().clone();

    tauri::async_runtime::spawn_blocking(move || state.get_app_config(&home_dir))
        .await
        .map_err(|error| PersistenceError::new(format!("Database task failed: {error}")))?
}

#[tauri::command]
pub async fn complete_onboarding(
    app: AppHandle,
    state: State<'_, AppState>,
    workspace_name: String,
) -> Result<AppConfig, PersistenceError> {
    let home_dir = app.path().home_dir().map_err(|error| {
        PersistenceError::new(format!("Unable to resolve home directory: {error}"))
    })?;
    let state = state.inner().clone();
    if state.get_app_config(&home_dir)?.onboarding_completed { return state.get_app_config(&home_dir); }
    let accounts = crate::openai_codex::list_provider_accounts(app.clone(), app.state(), app.state()).await
        .map_err(|_| PersistenceError::new("Não foi possível verificar os provedores. Tente novamente."))?;
    if !accounts.iter().any(|account| account.enabled && account.models_available && !account.models.is_empty()) {
        return Err(PersistenceError::new("Conecte um provedor com modelos disponíveis antes de começar."));
    }
    tauri::async_runtime::spawn_blocking(move || state.complete_onboarding(&home_dir, &workspace_name))
        .await
        .map_err(|error| PersistenceError::new(format!("Database task failed: {error}")))?
}

#[cfg(test)]
mod tests {
#[test]
fn tool_migration_preserves_explicit_models_and_inherits_unconfigured_tools() {
    let mut db = Connection::open_in_memory().unwrap();
    for migration in super::MIGRATIONS.iter().take(11) { db.execute_batch(migration.sql).unwrap(); }
    db.pragma_update(None, "user_version", 11).unwrap();
    db.execute("INSERT INTO web_search_config (id, account_alias) VALUES (1, NULL)", []).unwrap();
    db.execute("INSERT INTO provider_accounts (alias, provider_kind, account_id) VALUES ('antigravity-personal', 'antigravity', 'account')", []).unwrap();
    db.execute("INSERT INTO vision_config (id, account_alias, model) VALUES (1, 'antigravity-personal', 'gemini-3.8-flash')", []).unwrap();
    crate::persistence::initialize_database(&mut db).unwrap();
    assert!(db.query_row("SELECT inherit_chat FROM web_search_config WHERE id = 1", [], |row| row.get::<_, bool>(0)).unwrap());
    let preserved: (bool, String) = db.query_row("SELECT inherit_chat, model FROM vision_config", [], |row| Ok((row.get(0)?, row.get(1)?))).unwrap();
    assert_eq!(preserved, (false, "gemini-3.8-flash".into()));
}

    #[test]
    fn antigravity_migration_preserves_codex_accounts_and_search_selection() {
        let mut connection = rusqlite::Connection::open_in_memory().unwrap();
        connection.pragma_update(None, "foreign_keys", true).unwrap();
        for migration in super::MIGRATIONS.iter().take(7) { connection.execute_batch(migration.sql).unwrap(); }
        connection.pragma_update(None, "user_version", 7).unwrap();
        connection.execute("INSERT INTO provider_accounts(alias, provider_kind, account_id) VALUES ('openai-codex-old','openai-codex','account-old')", []).unwrap();
        connection.execute("UPDATE provider_accounts SET enabled=0", []).unwrap();
        connection.execute("INSERT INTO web_search_config (id,account_alias) VALUES (1,'openai-codex-old')", []).unwrap();
        super::initialize_database(&mut connection).unwrap();
        let accounts=super::list_provider_accounts(&connection).unwrap();
        assert_eq!(accounts.len(),1); assert_eq!(accounts[0].alias,"openai-codex-old"); assert!(!accounts[0].enabled);
        assert_eq!(connection.query_row("SELECT account_alias FROM web_search_config", [], |row|row.get::<_,String>(0)).unwrap(),"openai-codex-old");
        super::insert_provider_account(&connection,"antigravity-new","google:123").unwrap();
        assert_eq!(super::list_provider_accounts(&connection).unwrap().len(),2);
        assert_eq!(connection.query_row("SELECT count(*) FROM pragma_foreign_key_check", [], |row|row.get::<_,i64>(0)).unwrap(),0);
        super::delete_provider_account(&connection,"openai-codex-old").unwrap();
        assert!(connection.query_row("SELECT account_alias FROM web_search_config", [], |row|row.get::<_,Option<String>>(0)).unwrap().is_none());
    }
    use super::*;

    fn in_memory_database() -> Connection {
        let mut connection = Connection::open_in_memory().expect("in-memory SQLite");
        initialize_database(&mut connection).expect("initial migration");
        connection
    }

    #[test]
    fn resolves_the_exact_native_database_path() {
        assert_eq!(
            database_path(Path::new("/Users/example")),
            PathBuf::from("/Users/example/.jarvis/jarvis.db")
        );
    }

    #[test]
    fn migration_creates_one_default_singleton_row() {
        let connection = in_memory_database();

        let version: i64 = connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .expect("schema version");
        let count: i64 = connection
            .query_row("SELECT COUNT(*) FROM app_config", [], |row| row.get(0))
            .expect("singleton count");

        assert_eq!(version, 16);
        assert_eq!(count, 1);
        assert_eq!(
            read_app_config(&connection).expect("default config"),
            AppConfig {
                onboarding_completed: false,
            }
        );
    }

    #[test]
    fn singleton_check_rejects_an_extra_row() {
        let connection = in_memory_database();

        let result = connection.execute("INSERT INTO app_config (id) VALUES (?1)", params![2_i64]);

        assert!(result.is_err());
    }

    #[test]
    fn completion_is_idempotent_and_reinitialization_preserves_true() {
        let mut connection = in_memory_database();
        connection.execute("INSERT INTO provider_accounts(alias,provider_kind,account_id) VALUES ('test','openai-codex','test')", []).unwrap();

        assert_eq!(
            complete_app_config(&mut connection, "  Meu espaço  ").expect("first completion"),
            AppConfig {
                onboarding_completed: true,
            }
        );
        initialize_database(&mut connection).expect("idempotent migration check");
        assert_eq!(
            complete_app_config(&mut connection, "Different name").expect("second completion"),
            AppConfig {
                onboarding_completed: true,
            }
        );
        assert_eq!(
            read_app_config(&connection).expect("completed config"),
            AppConfig {
                onboarding_completed: true,
            }
        );
        assert_eq!(connection.query_row("SELECT count(*) FROM workspaces", [], |row| row.get::<_, i64>(0)).unwrap(), 1);
        assert_eq!(connection.query_row("SELECT name FROM workspaces JOIN navigation_selection ON workspaces.id = navigation_selection.workspace_id", [], |row| row.get::<_, String>(0)).unwrap(), "Meu espaço");
    }

    #[test]
    fn onboarding_requires_provider_and_invalid_names_leave_no_partial_workspace() {
        let mut connection = in_memory_database();
        assert!(complete_app_config(&mut connection, "").is_err());
        connection.execute("INSERT INTO provider_accounts(alias,provider_kind,account_id) VALUES ('test','openai-codex','test')", []).unwrap();
        assert!(complete_app_config(&mut connection, "bad\nname").is_err());
        assert!(!read_app_config(&connection).unwrap().onboarding_completed);
        assert_eq!(connection.query_row("SELECT count(*) FROM workspaces", [], |row| row.get::<_, i64>(0)).unwrap(), 0);
        complete_app_config(&mut connection, " ").unwrap();
        assert_eq!(connection.query_row("SELECT name FROM workspaces", [], |row| row.get::<_, String>(0)).unwrap(), "Pessoal");
    }

    #[test]
    fn context7_migration_removes_only_unconfigured_seed() {
        for configured in [false, true] {
            let mut db = Connection::open_in_memory().unwrap();
            for migration in MIGRATIONS.iter().take(13) { db.execute_batch(migration.sql).unwrap(); }
            db.pragma_update(None, "user_version", 13).unwrap();
            db.execute("UPDATE mcp_servers SET configured = ?1 WHERE id = 'builtin-context7'", [configured]).unwrap();
            initialize_database(&mut db).unwrap();
            let count = db.query_row("SELECT count(*) FROM mcp_servers WHERE id = 'builtin-context7'", [], |row| row.get::<_, i64>(0)).unwrap();
            assert_eq!(count, i64::from(configured));
        }
    }

    #[test]
    fn latest_schema_preserves_existing_app_config() {
        let mut connection = Connection::open_in_memory().expect("in-memory SQLite");
        connection
            .execute_batch(include_str!("../../drizzle/0000_heavy_tomas.sql"))
            .expect("historical migration");
        connection
            .pragma_update(None, "user_version", 1_i64)
            .expect("schema version");
        connection
            .execute(
                "INSERT INTO app_config (id, onboarding_completed) VALUES (?1, ?2)",
                params![1_i64, 1_i64],
            )
            .expect("existing app config");

        initialize_database(&mut connection).expect("latest schema");

        let version: i64 = connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .expect("schema version");
        assert_eq!(version, 16);
        assert_eq!(
            read_app_config(&connection).expect("preserved app config"),
            AppConfig {
                onboarding_completed: true,
            }
        );
        assert_eq!(
            list_provider_accounts(&connection)
                .expect("provider account list")
                .len(),
            0
        );
    }

    #[test]
    fn provider_accounts_support_two_aliases_and_reject_duplicate_keys() {
        let connection = in_memory_database();
        insert_provider_account(&connection, "openai-codex-one", "account-one")
            .expect("first account");
        insert_provider_account(&connection, "openai-codex-two", "account-two")
            .expect("second account");

        assert_eq!(
            list_provider_accounts(&connection)
                .expect("account list")
                .len(),
            2
        );
        assert!(insert_provider_account(&connection, "openai-codex-one", "account-three").is_err());
        assert!(insert_provider_account(&connection, "openai-codex-three", "account-two").is_err());
    }

    #[test]
    fn provider_account_delete_is_parameterized_and_schema_has_no_token_columns() {
        let connection = in_memory_database();
        insert_provider_account(&connection, "openai-codex-one", "account-one")
            .expect("first account");
        insert_provider_account(&connection, "openai-codex-two", "account-two")
            .expect("second account");

        delete_provider_account(
            &connection,
            "openai-codex-one'; DELETE FROM provider_accounts; --",
        )
        .expect("parameterized delete");
        assert_eq!(
            list_provider_accounts(&connection)
                .expect("account list")
                .len(),
            2
        );

        delete_provider_account(&connection, "openai-codex-one").expect("delete account");
        let mut statement = connection
            .prepare("PRAGMA table_info(provider_accounts)")
            .expect("provider schema");
        let columns = statement
            .query_map([], |row| row.get::<_, String>(1))
            .expect("provider columns")
            .collect::<Result<Vec<_>, _>>()
            .expect("provider column names");
        assert_eq!(
            columns,
            vec![
                "alias",
                "provider_kind",
                "account_id",
                "created_at",
                "enabled",
                "show_usage",
                "show_third_party_usage"
            ]
        );
        assert_eq!(
            list_provider_accounts(&connection)
                .expect("account list")
                .len(),
            1
        );
    }
}
