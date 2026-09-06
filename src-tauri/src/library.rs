use std::{
    fs::{self, File, OpenOptions},
    io::{BufRead, BufReader, Read, Write},
    path::{Path, PathBuf},
};

use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_dialog::DialogExt;

#[cfg(unix)]
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};

use crate::persistence::{AppState, PersistenceError};

pub(crate) mod deletion;
pub(crate) mod cleanup;
pub(crate) mod dashboard;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Workspace {
    id: String,
    name: String,
    created_at: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    id: String,
    workspace_id: String,
    name: String,
    path: String,
    created_at: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Conversation {
    id: String,
    project_id: String,
    title: String,
    created_at: i64,
    last_activity_at: i64,
    // The JSONL header remains immutable when the display title changes.
    #[serde(skip)]
    initial_title: String,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Selection {
    workspace_id: Option<String>,
    project_id: Option<String>,
    conversation_id: Option<String>,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct LibrarySnapshot {
    workspaces: Vec<Workspace>,
    projects: Vec<Project>,
    conversations: Vec<Conversation>,
    selection: Selection,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", content = "id", rename_all = "snake_case")]
pub enum LibraryTarget {
    Workspace(String),
    Project(String),
    Conversation(String),
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationDetails {
    conversation: Conversation,
    project: Project,
    workspace: Workspace,
}

#[derive(Debug, Serialize)]
pub struct LibraryError {
    code: &'static str,
    message: &'static str,
}

impl LibraryError {
    pub(crate) fn new(code: &'static str, message: &'static str) -> Self {
        Self { code, message }
    }

    fn missing() -> Self {
        Self::new(
            "not_found",
            "O item selecionado não existe mais. Atualize a lista.",
        )
    }

    fn storage() -> Self {
        Self::new("session_storage", "Não foi possível acessar o histórico da conversa. Verifique as permissões e tente novamente.")
    }

    fn invalid_session() -> Self {
        Self::new("invalid_session", "O histórico desta conversa está ausente ou inválido. Os arquivos existentes foram preservados.")
    }
}

impl From<PersistenceError> for LibraryError {
    fn from(_: PersistenceError) -> Self {
        Self::new(
            "database",
            "Não foi possível acessar os dados do Jarvis. Tente novamente.",
        )
    }
}

impl From<rusqlite::Error> for LibraryError {
    fn from(_: rusqlite::Error) -> Self {
        Self::new(
            "database",
            "Não foi possível salvar ou consultar os dados do Jarvis. Tente novamente.",
        )
    }
}

pub(crate) fn new_id() -> Result<String, LibraryError> {
    let mut bytes = [0_u8; 16];
    getrandom::fill(&mut bytes).map_err(|_| {
        LibraryError::new(
            "identity",
            "Não foi possível criar um identificador. Tente novamente.",
        )
    })?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

fn valid_id(id: &str) -> bool {
    id.len() == 32
        && id
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn name(value: &str) -> Result<&str, LibraryError> {
    let value = value.trim();
    if value.is_empty() || value.chars().count() > 120 || value.chars().any(char::is_control) {
        return Err(LibraryError::new(
            "invalid_name",
            "Informe um nome de até 120 caracteres, sem quebras de linha.",
        ));
    }
    Ok(value)
}

fn workspace_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Workspace> {
    Ok(Workspace {
        id: row.get(0)?,
        name: row.get(1)?,
        created_at: row.get(2)?,
    })
}

fn project_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Project> {
    Ok(Project {
        id: row.get(0)?,
        workspace_id: row.get(1)?,
        name: row.get(2)?,
        path: row.get(3)?,
        created_at: row.get(4)?,
    })
}

fn conversation_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Conversation> {
    Ok(Conversation {
        id: row.get(0)?,
        project_id: row.get(1)?,
        title: row.get(2)?,
        created_at: row.get(3)?,
        initial_title: row.get(4)?,
        last_activity_at: row.get(5)?,
    })
}

fn workspace(connection: &Connection, id: &str) -> Result<Workspace, LibraryError> {
    connection
        .query_row(
            "SELECT id, name, created_at FROM workspaces WHERE id = ?1",
            [id],
            workspace_row,
        )
        .optional()?
        .ok_or_else(LibraryError::missing)
}

fn project(connection: &Connection, id: &str) -> Result<Project, LibraryError> {
    connection
        .query_row(
            "SELECT id, workspace_id, name, path, created_at FROM projects WHERE id = ?1",
            [id],
            project_row,
        )
        .optional()?
        .ok_or_else(LibraryError::missing)
}

fn conversation(connection: &Connection, id: &str) -> Result<Conversation, LibraryError> {
    connection
        .query_row(
            "SELECT id, project_id, COALESCE(display_title, title), created_at, title, COALESCE(last_activity_at, created_at) FROM conversations WHERE id = ?1",
            [id],
            conversation_row,
        )
        .optional()?
        .ok_or_else(LibraryError::missing)
}

fn snapshot(connection: &Connection) -> Result<LibrarySnapshot, LibraryError> {
    let workspaces = connection
        .prepare("SELECT id, name, created_at FROM workspaces ORDER BY created_at, rowid")?
        .query_map([], workspace_row)?
        .collect::<Result<Vec<_>, _>>()?;
    let projects = connection.prepare("SELECT id, workspace_id, name, path, created_at FROM projects ORDER BY created_at, rowid")?
        .query_map([], project_row)?.collect::<Result<Vec<_>, _>>()?;
    let conversations = connection.prepare("SELECT id, project_id, COALESCE(display_title, title), created_at, title, COALESCE(last_activity_at, created_at) FROM conversations ORDER BY COALESCE(last_activity_at, created_at) DESC, rowid DESC")?
        .query_map([], conversation_row)?.collect::<Result<Vec<_>, _>>()?;
    let selection = connection.query_row(
        "SELECT workspace_id, project_id, conversation_id FROM navigation_selection WHERE id = 1", [],
        |row| Ok(Selection { workspace_id: row.get(0)?, project_id: row.get(1)?, conversation_id: row.get(2)? }),
    ).optional()?.unwrap_or_default();
    Ok(LibrarySnapshot {
        workspaces,
        projects,
        conversations,
        selection,
    })
}

fn save_selection(connection: &Connection, selection: &Selection) -> Result<(), LibraryError> {
    connection.execute(
        "INSERT INTO navigation_selection (id, workspace_id, project_id, conversation_id) VALUES (1, ?1, ?2, ?3)
         ON CONFLICT(id) DO UPDATE SET workspace_id = excluded.workspace_id, project_id = excluded.project_id, conversation_id = excluded.conversation_id",
        params![selection.workspace_id, selection.project_id, selection.conversation_id],
    )?;
    Ok(())
}

fn insert_workspace(
    connection: &mut Connection,
    value: &str,
) -> Result<LibrarySnapshot, LibraryError> {
    let name = name(value)?;
    let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    if tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM workspaces WHERE name = ?1)",
        [name],
        |row| row.get::<_, bool>(0),
    )? {
        return Err(LibraryError::new(
            "duplicate_workspace",
            "Já existe um workspace com esse nome.",
        ));
    }
    let id = new_id()?;
    tx.execute(
        "INSERT INTO workspaces (id, name) VALUES (?1, ?2)",
        params![id, name],
    )?;
    save_selection(
        &tx,
        &Selection {
            workspace_id: Some(id),
            ..Selection::default()
        },
    )?;
    let result = snapshot(&tx)?;
    tx.commit()?;
    Ok(result)
}

fn canonical_directory(path: &Path) -> Result<PathBuf, LibraryError> {
    let path = fs::canonicalize(path).map_err(|_| {
        LibraryError::new(
            "project_directory",
            "A pasta do projeto não está disponível. Verifique o caminho e as permissões.",
        )
    })?;
    if !path.is_dir() {
        return Err(LibraryError::new(
            "project_directory",
            "Selecione uma pasta válida para o projeto.",
        ));
    }
    Ok(path)
}

fn insert_project(
    connection: &mut Connection,
    workspace_id: &str,
    directory: &Path,
) -> Result<LibrarySnapshot, LibraryError> {
    let directory = canonical_directory(directory)?;
    let path = directory.to_str().ok_or_else(|| {
        LibraryError::new(
            "project_directory",
            "O caminho da pasta contém caracteres não compatíveis.",
        )
    })?;
    let title = directory
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("Projeto");
    let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    workspace(&tx, workspace_id)?;
    if tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM projects WHERE path = ?1)",
        [path],
        |row| row.get::<_, bool>(0),
    )? {
        return Err(LibraryError::new(
            "duplicate_project",
            "Esta pasta já foi adicionada. Selecione o projeto existente.",
        ));
    }
    let id = new_id()?;
    tx.execute(
        "INSERT INTO projects (id, workspace_id, name, path) VALUES (?1, ?2, ?3, ?4)",
        params![id, workspace_id, title, path],
    )?;
    save_selection(
        &tx,
        &Selection {
            workspace_id: Some(workspace_id.to_owned()),
            project_id: Some(id),
            conversation_id: None,
        },
    )?;
    let result = snapshot(&tx)?;
    tx.commit()?;
    Ok(result)
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct SessionHeader {
    #[serde(rename = "type")]
    kind: String,
    version: u32,
    id: String,
    project_id: String,
    cwd: String,
    title: String,
    created_at: i64,
}

fn session_path(
    home: &Path,
    project_id: &str,
    conversation_id: &str,
    create: bool,
) -> Result<PathBuf, LibraryError> {
    if !valid_id(project_id) || !valid_id(conversation_id) {
        return Err(LibraryError::invalid_session());
    }
    let mut directory = home.to_path_buf();
    for component in [".jarvis", "sessions", project_id] {
        directory.push(component);
        if create {
            let mut builder = fs::DirBuilder::new();
            #[cfg(unix)]
            builder.mode(0o700);
            match builder.create(&directory) {
                Ok(()) => {
                    #[cfg(unix)]
                    File::open(directory.parent().ok_or_else(LibraryError::storage)?)
                        .and_then(|parent| parent.sync_all())
                        .map_err(|_| LibraryError::storage())?;
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(_) => return Err(LibraryError::storage()),
            }
        }
        let metadata =
            fs::symlink_metadata(&directory).map_err(|_| LibraryError::invalid_session())?;
        if metadata.is_symlink() || !metadata.is_dir() {
            return Err(LibraryError::invalid_session());
        }
    }
    Ok(directory.join(format!("{conversation_id}.jsonl")))
}

fn persist_header(path: &Path, header: &SessionHeader) -> Result<(), LibraryError> {
    let bytes = serde_json::to_vec(header).map_err(|_| LibraryError::storage())?;
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    options.mode(0o600);
    let mut file = options.open(path).map_err(|_| LibraryError::storage())?;
    let result = file
        .write_all(&bytes)
        .and_then(|()| file.write_all(b"\n"))
        .and_then(|()| file.sync_all());
    if result.is_err() {
        drop(file);
        let _ = fs::remove_file(path);
        return Err(LibraryError::storage());
    }
    // Sync the directory entry as well as file contents before indexing the session.
    #[cfg(unix)]
    if path
        .parent()
        .and_then(|parent| File::open(parent).ok())
        .and_then(|parent| parent.sync_all().ok())
        .is_none()
    {
        drop(file);
        let _ = fs::remove_file(path);
        return Err(LibraryError::storage());
    }
    Ok(())
}

fn is_empty_conversation(home: &Path, conversation: &Conversation) -> Result<bool, LibraryError> {
    let path = session_path(home, &conversation.project_id, &conversation.id, false)?;
    let metadata = fs::symlink_metadata(&path).map_err(|_| LibraryError::invalid_session())?;
    if metadata.is_symlink() || !metadata.is_file() {
        return Err(LibraryError::invalid_session());
    }
    let file = File::open(path).map_err(|_| LibraryError::storage())?;
    let mut reader = BufReader::new(file);
    let mut header = Vec::new();
    {
        let mut bounded = reader.by_ref().take(65_537);
        bounded.read_until(b'\n', &mut header).map_err(|_| LibraryError::storage())?;
    }
    if header.len() > 65_536 || !header.ends_with(b"\n") {
        return Err(LibraryError::invalid_session());
    }
    reader.fill_buf().map(|remaining| remaining.is_empty()).map_err(|_| LibraryError::storage())
}

fn latest_empty_conversation(
    connection: &Connection,
    home: &Path,
    project_id: &str,
    title: &str,
) -> Result<Option<Conversation>, LibraryError> {
    let latest = connection.query_row(
        "SELECT id, project_id, COALESCE(display_title, title), created_at, title, COALESCE(last_activity_at, created_at) FROM conversations WHERE project_id = ?1 ORDER BY COALESCE(last_activity_at, created_at) DESC, rowid DESC LIMIT 1",
        [project_id],
        conversation_row,
    ).optional()?;
    let Some(latest) = latest else {
        return Ok(None);
    };
    if latest.initial_title != title {
        return Ok(None);
    }
    read_conversation(connection, home, &latest.id)?;
    if is_empty_conversation(home, &latest)? { Ok(Some(latest)) } else { Ok(None) }
}

fn insert_conversation(
    connection: &mut Connection,
    home: &Path,
    project_id: &str,
    title: &str,
) -> Result<LibrarySnapshot, LibraryError> {
    let title = name(title)?;
    let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let project = project(&tx, project_id)?;
    if canonical_directory(Path::new(&project.path))? != Path::new(&project.path) {
        return Err(LibraryError::new(
            "project_directory",
            "A pasta do projeto mudou. Verifique o caminho antes de criar a conversa.",
        ));
    }
    if let Some(existing) = latest_empty_conversation(&tx, home, project_id, title)? {
        let result = (|| {
            save_selection(&tx, &Selection {
                workspace_id: Some(project.workspace_id),
                project_id: Some(project_id.to_owned()),
                conversation_id: Some(existing.id),
            })?;
            let snapshot = snapshot(&tx)?;
            tx.commit()?;
            Ok(snapshot)
        })();
        return result;
    }
    let id = new_id()?;
    tx.execute(
        "INSERT INTO conversations (id, project_id, title, title_source) VALUES (?1, ?2, ?3, 'default')",
        params![id, project_id, title],
    )?;
    let record = conversation(&tx, &id)?;
    let path = session_path(home, project_id, &id, true)?;
    persist_header(
        &path,
        &SessionHeader {
            kind: "session".to_owned(),
            version: 1,
            id: id.clone(),
            project_id: project_id.to_owned(),
            cwd: project.path,
            title: title.to_owned(),
            created_at: record.created_at,
        },
    )?;
    let result = (|| {
        save_selection(
            &tx,
            &Selection {
                workspace_id: Some(project.workspace_id),
                project_id: Some(project_id.to_owned()),
                conversation_id: Some(id),
            },
        )?;
        let snapshot = snapshot(&tx)?;
        tx.commit()?;
        Ok(snapshot)
    })();
    if result.is_err() {
        let _ = fs::remove_file(path);
    }
    result
}

fn read_conversation(
    connection: &Connection,
    home: &Path,
    id: &str,
) -> Result<ConversationDetails, LibraryError> {
    let conversation = conversation(connection, id)?;
    let project = project(connection, &conversation.project_id)?;
    let workspace = workspace(connection, &project.workspace_id)?;
    let path = session_path(home, &project.id, id, false)?;
    let metadata = fs::symlink_metadata(&path).map_err(|_| LibraryError::invalid_session())?;
    if metadata.is_symlink() || !metadata.is_file() {
        return Err(LibraryError::invalid_session());
    }
    let file = File::open(path).map_err(|_| LibraryError::storage())?;
    let mut first_line = String::new();
    BufReader::new(file.take(65_537))
        .read_line(&mut first_line)
        .map_err(|_| LibraryError::invalid_session())?;
    if first_line.len() > 65_536 || !first_line.ends_with('\n') {
        return Err(LibraryError::invalid_session());
    }
    let header: SessionHeader =
        serde_json::from_str(&first_line).map_err(|_| LibraryError::invalid_session())?;
    if header.kind != "session"
        || header.version != 1
        || header.id != id
        || header.project_id != project.id
        || header.cwd != project.path
        || header.title != conversation.initial_title
        || header.created_at != conversation.created_at
    {
        return Err(LibraryError::invalid_session());
    }
    Ok(ConversationDetails {
        conversation,
        project,
        workspace,
    })
}

pub(crate) fn agent_location(
    state: &AppState,
    home: &Path,
    id: &str,
) -> Result<(PathBuf, PathBuf), LibraryError> {
    state.with_connection(home, |connection| {
        let details = read_conversation(connection, home, id)?;
        let journal = session_path(home, &details.project.id, id, false)?;
        let root = PathBuf::from(details.project.path);
        if !root.is_dir() || fs::canonicalize(&root).ok().as_ref() != Some(&root) {
            return Err(LibraryError::new(
                "project_unavailable",
                "A pasta do projeto não está disponível no caminho original.",
            ));
        }
        Ok((journal, root))
    })
}

pub(crate) fn notification_names(state: &AppState, home: &Path, id: &str) -> Result<(String, String), LibraryError> {
    state.with_connection(home, |connection| {
        Ok(connection.query_row(
            "SELECT p.name, COALESCE(c.display_title, c.title) FROM conversations c JOIN projects p ON p.id = c.project_id WHERE c.id = ?1",
            [id], |row| Ok((row.get(0)?, row.get(1)?)),
        )?)
    })
}

pub(crate) fn needs_generated_title(
    state: &AppState,
    home: &Path,
    id: &str,
) -> Result<bool, LibraryError> {
    state.with_connection(home, |connection| {
        Ok(connection.query_row(
            "SELECT title_source = 'default' FROM conversations WHERE id = ?1",
            [id],
            |row| row.get(0),
        )?)
    })
}

pub(crate) fn save_generated_title(
    state: &AppState,
    home: &Path,
    id: &str,
    title: &str,
) -> Result<bool, LibraryError> {
    let title = name(title)?;
    state.with_connection(home, |connection| {
        Ok(connection.execute("UPDATE conversations SET display_title = ?1, title_source = 'generated' WHERE id = ?2 AND title_source = 'default'", params![title, id])? == 1)
    })
}

fn rename_project_record(
    connection: &mut Connection,
    id: &str,
    value: &str,
) -> Result<LibrarySnapshot, LibraryError> {
    let value = name(value)?;
    let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    project(&tx, id)?;
    tx.execute(
        "UPDATE projects SET name = ?1 WHERE id = ?2",
        params![value, id],
    )?;
    let result = snapshot(&tx)?;
    tx.commit()?;
    Ok(result)
}

fn rename_conversation_record(
    connection: &mut Connection,
    home: &Path,
    id: &str,
    value: &str,
) -> Result<LibrarySnapshot, LibraryError> {
    let value = name(value)?;
    let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    read_conversation(&tx, home, id)?;
    // Sidebar metadata is owned by SQLite; editing it never rewrites session history.
    tx.execute(
        "UPDATE conversations SET display_title = ?1, title_source = 'manual' WHERE id = ?2",
        params![value, id],
    )?;
    let result = snapshot(&tx)?;
    tx.commit()?;
    Ok(result)
}

fn select_item(
    connection: &mut Connection,
    home: &Path,
    target: LibraryTarget,
) -> Result<LibrarySnapshot, LibraryError> {
    let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let selection = match target {
        LibraryTarget::Workspace(id) => {
            workspace(&tx, &id)?;
            Selection {
                workspace_id: Some(id),
                ..Selection::default()
            }
        }
        LibraryTarget::Project(id) => {
            let project = project(&tx, &id)?;
            Selection {
                workspace_id: Some(project.workspace_id),
                project_id: Some(id),
                conversation_id: None,
            }
        }
        LibraryTarget::Conversation(id) => {
            let details = read_conversation(&tx, home, &id)?;
            Selection {
                workspace_id: Some(details.workspace.id),
                project_id: Some(details.project.id),
                conversation_id: Some(id),
            }
        }
    };
    save_selection(&tx, &selection)?;
    let result = snapshot(&tx)?;
    tx.commit()?;
    Ok(result)
}

async fn run<T: Send + 'static>(
    app: AppHandle,
    state: AppState,
    operation: impl FnOnce(&mut Connection, &Path) -> Result<T, LibraryError> + Send + 'static,
) -> Result<T, LibraryError> {
    let home = app.path().home_dir().map_err(|_| LibraryError::storage())?;
    tauri::async_runtime::spawn_blocking(move || {
        state.with_connection(&home, |connection| operation(connection, &home))
    })
    .await
    .map_err(|_| {
        LibraryError::new(
            "internal",
            "Não foi possível concluir a operação. Tente novamente.",
        )
    })?
}

#[tauri::command]
pub async fn get_library_snapshot(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<LibrarySnapshot, LibraryError> {
    run(app, state.inner().clone(), |connection, home| {
        deletion::recover(connection, home)?;
        dashboard::backfill_activity(connection, home)?;
        let tx = connection.transaction()?;
        let result = snapshot(&tx)?;
        tx.commit()?;
        Ok(result)
    })
    .await
}

#[tauri::command]
pub async fn create_workspace(
    app: AppHandle,
    state: State<'_, AppState>,
    name: String,
) -> Result<LibrarySnapshot, LibraryError> {
    run(app, state.inner().clone(), move |connection, _| {
        insert_workspace(connection, &name)
    })
    .await
}

#[tauri::command]
pub async fn add_project(
    app: AppHandle,
    state: State<'_, AppState>,
    workspace_id: String,
) -> Result<Option<LibrarySnapshot>, LibraryError> {
    let workspace_check = workspace_id.clone();
    run(app.clone(), state.inner().clone(), move |connection, _| {
        workspace(connection, &workspace_check)
    })
    .await?;
    let picker_app = app.clone();
    let directory = tauri::async_runtime::spawn_blocking(move || {
        picker_app
            .dialog()
            .file()
            .set_title("Selecionar pasta do projeto")
            .blocking_pick_folder()
    })
    .await
    .map_err(|_| {
        LibraryError::new(
            "directory_picker",
            "Não foi possível abrir o seletor de pastas.",
        )
    })?;
    let Some(directory) = directory else {
        return Ok(None);
    };
    let directory = directory.into_path().map_err(|_| {
        LibraryError::new(
            "project_directory",
            "Selecione uma pasta local para o projeto.",
        )
    })?;
    run(app, state.inner().clone(), move |connection, _| {
        insert_project(connection, &workspace_id, &directory)
    })
    .await
    .map(Some)
}

#[tauri::command]
pub async fn create_conversation(
    app: AppHandle,
    state: State<'_, AppState>,
    project_id: String,
) -> Result<LibrarySnapshot, LibraryError> {
    run(app, state.inner().clone(), move |connection, home| {
        insert_conversation(connection, home, &project_id, "Nova Conversa")
    })
    .await
}

#[tauri::command]
pub async fn rename_project(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    name: String,
) -> Result<LibrarySnapshot, LibraryError> {
    run(app, state.inner().clone(), move |connection, _| {
        rename_project_record(connection, &id, &name)
    })
    .await
}

#[tauri::command]
pub async fn delete_library_item(
    app: AppHandle,
    state: State<'_, AppState>,
    agent: State<'_, crate::agent::AgentState>,
    target: deletion::DeleteTarget,
    confirmed: bool,
) -> Result<LibrarySnapshot, LibraryError> {
    if !confirmed {
        return Err(LibraryError::new(
            "confirmation_required",
            "Confirme a exclusão definitiva antes de continuar.",
        ));
    }
    let home = app.path().home_dir().map_err(|_| LibraryError::storage())?;
    let state = state.inner().clone();
    let agent = agent.inner().clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        agent.delete_library_item(&state, &home, &target)
    })
    .await
    .map_err(|_| LibraryError::storage())?;
    // Refresh other views even if metadata committed but a cleanup needs retrying.
    let _ = app.emit("library:changed", ());
    result
}

#[tauri::command]
pub async fn rename_conversation(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    title: String,
) -> Result<LibrarySnapshot, LibraryError> {
    run(app, state.inner().clone(), move |connection, home| {
        rename_conversation_record(connection, home, &id, &title)
    })
    .await
}

#[tauri::command]
pub async fn select_library_item(
    app: AppHandle,
    state: State<'_, AppState>,
    target: LibraryTarget,
) -> Result<LibrarySnapshot, LibraryError> {
    run(app, state.inner().clone(), move |connection, home| {
        select_item(connection, home, target)
    })
    .await
}

#[tauri::command]
pub async fn get_conversation(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> Result<ConversationDetails, LibraryError> {
    run(app, state.inner().clone(), move |connection, home| {
        read_conversation(connection, home, &id)
    })
    .await
}

#[cfg(test)]
mod tests;
