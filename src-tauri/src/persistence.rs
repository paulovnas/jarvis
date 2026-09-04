use std::{
    fmt,
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use rusqlite::{params, Connection};
use serde::Serialize;
use tauri::{AppHandle, Manager, State};

const MIGRATIONS: &[Migration] = &[Migration {
    version: 1,
    sql: include_str!("../../drizzle/0000_heavy_tomas.sql"),
}];

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

fn initialize_database(connection: &mut Connection) -> Result<(), PersistenceError> {
    let mut current_version = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
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

fn complete_app_config(connection: &mut Connection) -> Result<AppConfig, PersistenceError> {
    let transaction = connection.transaction()?;
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
    fn with_connection<T>(
        &self,
        home_dir: &Path,
        operation: impl FnOnce(&mut Connection) -> Result<T, PersistenceError>,
    ) -> Result<T, PersistenceError> {
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

    fn complete_onboarding(&self, home_dir: &Path) -> Result<AppConfig, PersistenceError> {
        self.with_connection(home_dir, complete_app_config)
    }
}

#[tauri::command]
pub async fn get_app_config(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<AppConfig, PersistenceError> {
    let home_dir = app
        .path()
        .home_dir()
        .map_err(|error| PersistenceError::new(format!("Unable to resolve home directory: {error}")))?;
    let state = state.inner().clone();

    tauri::async_runtime::spawn_blocking(move || state.get_app_config(&home_dir))
        .await
        .map_err(|error| PersistenceError::new(format!("Database task failed: {error}")))?
}

#[tauri::command]
pub async fn complete_onboarding(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<AppConfig, PersistenceError> {
    let home_dir = app
        .path()
        .home_dir()
        .map_err(|error| PersistenceError::new(format!("Unable to resolve home directory: {error}")))?;
    let state = state.inner().clone();

    tauri::async_runtime::spawn_blocking(move || state.complete_onboarding(&home_dir))
        .await
        .map_err(|error| PersistenceError::new(format!("Database task failed: {error}")))?
}

#[cfg(test)]
mod tests {
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

        assert_eq!(version, 1);
        assert_eq!(count, 1);
        assert_eq!(read_app_config(&connection).expect("default config"), AppConfig {
            onboarding_completed: false,
        });
    }

    #[test]
    fn singleton_check_rejects_an_extra_row() {
        let connection = in_memory_database();

        let result = connection.execute(
            "INSERT INTO app_config (id) VALUES (?1)",
            params![2_i64],
        );

        assert!(result.is_err());
    }

    #[test]
    fn completion_is_idempotent_and_reinitialization_preserves_true() {
        let mut connection = in_memory_database();

        assert_eq!(
            complete_app_config(&mut connection).expect("first completion"),
            AppConfig {
                onboarding_completed: true,
            }
        );
        initialize_database(&mut connection).expect("idempotent migration check");
        assert_eq!(
            complete_app_config(&mut connection).expect("second completion"),
            AppConfig {
                onboarding_completed: true,
            }
        );
        assert_eq!(read_app_config(&connection).expect("completed config"), AppConfig {
            onboarding_completed: true,
        });
    }
}
