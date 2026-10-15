//! Projectless conversations reuse the private journal scope, not a user project.
use super::*;

pub(crate) fn is_global(connection: &Connection, id: &str) -> Result<bool, LibraryError> {
    Ok(conversation(connection, id)?.project_id == companion::GLOBAL_PROJECT_ID)
}

pub(crate) fn project_tool(name: &str) -> bool {
    matches!(
        name,
        "impeccable"
            | "jarvis_propose_publication"
            | "jarvis_inspect_publication"
            | "learn_project"
    ) || name.starts_with("beads_")
        || name.starts_with("project_beads_")
        || matches!(name, "video_run" | "video_tools")
        || name.starts_with("http_")
        || matches!(name, "terminal_start" | "process_start")
}

pub(crate) fn filter_tools(definitions: &mut Vec<serde_json::Value>) {
    definitions.retain(|tool| {
        tool["name"]
            .as_str()
            .or_else(|| tool["function"]["name"].as_str())
            .is_none_or(|name| !project_tool(name))
    });
}

pub(super) fn directory(connection: &Connection, home: &Path) -> Result<PathBuf, LibraryError> {
    let configured: Option<String> = connection
        .query_row(
            "SELECT directory FROM global_preferences WHERE id=1",
            [],
            |row| row.get(0),
        )
        .optional()?;
    if let Some(configured) = configured {
        return Ok(PathBuf::from(configured));
    }
    let root = home.join("Jarvis");
    match fs::symlink_metadata(&root) {
        Ok(metadata) if metadata.is_dir() && !metadata.is_symlink() => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir(&root).map_err(|_| LibraryError::storage())?;
        }
        _ => return Err(LibraryError::invalid_session()),
    }
    canonical_directory(&root)
}

pub(super) fn selection(
    connection: &Connection,
    project: &Project,
    id: String,
) -> Result<Selection, LibraryError> {
    if project.id != companion::GLOBAL_PROJECT_ID {
        return Ok(Selection {
            workspace_id: Some(project.workspace_id.clone()),
            project_id: Some(project.id.clone()),
            conversation_id: Some(id),
        });
    }
    let workspace_id = connection.query_row(
        "SELECT id FROM workspaces WHERE id<>?1 ORDER BY CASE WHEN id=(SELECT workspace_id FROM navigation_selection WHERE id=1) THEN 0 ELSE 1 END, created_at, rowid LIMIT 1",
        [companion::GLOBAL_WORKSPACE_ID], |row| row.get(0),
    ).optional()?;
    Ok(Selection {
        workspace_id,
        project_id: None,
        conversation_id: Some(id),
    })
}

pub(crate) fn move_conversation(
    connection: &mut Connection,
    home: &Path,
    id: &str,
    project_id: &str,
) -> Result<LibrarySnapshot, LibraryError> {
    let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let chat = read_conversation(&tx, home, id)?;
    if chat.conversation.project_id != companion::GLOBAL_PROJECT_ID
        || project_id == companion::GLOBAL_PROJECT_ID
    {
        return Err(LibraryError::new(
            "invalid_migration",
            "Escolha uma conversa global e um projeto de destino.",
        ));
    }
    let target = project(&tx, project_id)?;
    project_location(&tx, project_id)?;
    // Journal identity and attachments stay unchanged; only future turns use the project.
    tx.execute("UPDATE conversations SET journal_project_id=COALESCE(journal_project_id,project_id), project_id=?2 WHERE id=?1", params![id, project_id])?;
    let selected: Option<String> = tx
        .query_row(
            "SELECT conversation_id FROM navigation_selection WHERE id=1",
            [],
            |row| row.get(0),
        )
        .optional()?
        .flatten();
    if selected.as_deref() == Some(id) {
        save_selection(&tx, &selection(&tx, &target, id.to_owned())?)?;
    }
    let result = snapshot(&tx)?;
    tx.commit()?;
    Ok(result)
}

pub(crate) fn change_directory(
    connection: &mut Connection,
    directory: &Path,
) -> Result<LibrarySnapshot, LibraryError> {
    let root = canonical_directory(directory)?;
    let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    tx.execute("INSERT INTO global_preferences(id,directory) VALUES(1,?1) ON CONFLICT(id) DO UPDATE SET directory=excluded.directory", [root.to_string_lossy().as_ref()])?;
    tx.execute(
        "UPDATE projects SET path=?1 WHERE id=?2",
        params![root.to_string_lossy(), companion::GLOBAL_PROJECT_ID],
    )?;
    let result = snapshot(&tx)?;
    tx.commit()?;
    Ok(result)
}

#[tauri::command]
pub async fn change_global_directory(
    app: AppHandle,
    state: State<'_, AppState>,
    agent: State<'_, crate::agent::AgentState>,
) -> Result<Option<LibrarySnapshot>, LibraryError> {
    let picker = app.clone();
    let directory = tauri::async_runtime::spawn_blocking(move || {
        picker
            .dialog()
            .file()
            .set_title("Selecionar pasta de trabalho global")
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
    let directory = directory.into_path().map_err(|_| LibraryError::storage())?;
    let home = app.path().home_dir().map_err(|_| LibraryError::storage())?;
    let state = state.inner().clone();
    let agent = agent.inner().clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        agent.change_global_directory(&state, &home, &directory)
    })
    .await
    .map_err(|_| LibraryError::storage())??;
    let _ = app.emit("library:changed", ());
    Ok(Some(result))
}

#[tauri::command]
pub async fn create_global_conversation(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<LibrarySnapshot, LibraryError> {
    let result = run(app.clone(), state.inner().clone(), |db, home| {
        companion::ensure_global(db, home)?;
        insert_conversation(db, home, companion::GLOBAL_PROJECT_ID, "Nova conversa")
    })
    .await?;
    let _ = app.emit("library:changed", ());
    Ok(result)
}

#[tauri::command]
pub async fn move_global_conversation(
    app: AppHandle,
    state: State<'_, AppState>,
    agent: State<'_, crate::agent::AgentState>,
    id: String,
    project_id: String,
) -> Result<LibrarySnapshot, LibraryError> {
    let home = app.path().home_dir().map_err(|_| LibraryError::storage())?;
    let state = state.inner().clone();
    let agent = agent.inner().clone();
    let moved_id = id.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        agent.move_global_conversation(&state, &home, &id, &project_id)
    })
    .await
    .map_err(|_| LibraryError::storage())??;
    let _ = app.emit("library:changed", ());
    let _ = app.emit_to(
        "companion",
        "companion:chat_changed",
        serde_json::json!({"conversationId":moved_id}),
    );
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn global_scope_does_not_reserve_a_folder_against_visible_projects() {
        let home = tempfile::tempdir().unwrap();
        let mut db = Connection::open_in_memory().unwrap();
        crate::persistence::initialize_database(&mut db).unwrap();
        let root = directory(&db, home.path()).unwrap();
        let workspace = insert_workspace(&mut db, "Pessoal").unwrap();
        let project = insert_project(&mut db, &workspace.workspaces[0].id, &root)
            .unwrap()
            .projects
            .remove(0);
        companion::ensure_global(&mut db, home.path()).unwrap();
        assert_eq!(snapshot(&db).unwrap().projects[0].id, project.id);
        change_directory(&mut db, &root).unwrap();
        assert!(insert_project(&mut db, &workspace.workspaces[0].id, &root).is_err());
    }

    #[test]
    fn global_catalog_keeps_portable_tools_and_blocks_project_integrations() {
        let mut catalog = vec![
            "read_file",
            "generate_image",
            "browser_open",
            "design_read",
            "video_docs",
            "video_wait",
            "video_cancel",
            "impeccable",
            "learn_project",
            "jarvis_propose_publication",
            "jarvis_inspect_publication",
            "beads_create",
            "video_run",
            "video_tools",
            "http_send",
            "terminal_start",
            "process_start",
        ]
        .into_iter()
        .map(|name| serde_json::json!({"name":name}))
        .collect();
        filter_tools(&mut catalog);
        assert_eq!(
            catalog,
            vec![
                "read_file",
                "generate_image",
                "browser_open",
                "design_read",
                "video_docs",
                "video_wait",
                "video_cancel"
            ]
            .into_iter()
            .map(|name| serde_json::json!({"name":name}))
            .collect::<Vec<_>>()
        );
    }

    #[test]
    fn global_directory_is_in_the_user_jarvis_folder_and_is_idempotent() {
        let home = tempfile::tempdir().unwrap();
        let mut db = Connection::open_in_memory().unwrap();
        crate::persistence::initialize_database(&mut db).unwrap();
        let expected = fs::canonicalize(home.path()).unwrap().join("Jarvis");
        assert_eq!(directory(&db, home.path()).unwrap(), expected);
        fs::write(expected.join("keep.txt"), "preserved").unwrap();
        assert_eq!(directory(&db, home.path()).unwrap(), expected);
        assert_eq!(
            fs::read_to_string(expected.join("keep.txt")).unwrap(),
            "preserved"
        );
        companion::ensure_global(&mut db, home.path()).unwrap();
        let other = tempfile::tempdir().unwrap();
        change_directory(&mut db, other.path()).unwrap();
        companion::ensure_global(&mut db, home.path()).unwrap();
        assert_eq!(
            directory(&db, home.path()).unwrap(),
            fs::canonicalize(other.path()).unwrap()
        );
        assert!(read_conversation(&db, home.path(), companion::GLOBAL_CONVERSATION_ID).is_ok());
        assert_eq!(
            fs::read_to_string(expected.join("keep.txt")).unwrap(),
            "preserved"
        );
    }

    #[test]
    fn moving_a_global_chat_preserves_identity_history_and_can_delete_it_from_the_project() {
        let home = tempfile::tempdir().unwrap();
        let project_root = tempfile::tempdir().unwrap();
        let mut db = Connection::open_in_memory().unwrap();
        crate::persistence::initialize_database(&mut db).unwrap();
        let workspace = insert_workspace(&mut db, "Pessoal").unwrap();
        let target = insert_project(&mut db, &workspace.workspaces[0].id, project_root.path())
            .unwrap()
            .projects
            .remove(0);
        companion::ensure_global(&mut db, home.path()).unwrap();
        let before = insert_conversation(
            &mut db,
            home.path(),
            companion::GLOBAL_PROJECT_ID,
            "Global test",
        )
        .unwrap();
        let id = before.selection.conversation_id.unwrap();
        let journal = conversation_journal_path(&db, home.path(), &id).unwrap();
        let mut file = OpenOptions::new().append(true).open(&journal).unwrap();
        writeln!(file, "{{\"kind\":\"fixture\",\"content\":\"preserved\"}}").unwrap();
        drop(file);
        let bytes = fs::read(&journal).unwrap();
        let moved = move_conversation(&mut db, home.path(), &id, &target.id).unwrap();
        assert_eq!(
            moved.selection.project_id.as_deref(),
            Some(target.id.as_str())
        );
        assert_eq!(
            moved.selection.conversation_id.as_deref(),
            Some(id.as_str())
        );
        assert_eq!(
            read_conversation(&db, home.path(), &id).unwrap().project.id,
            target.id
        );
        assert_eq!(
            conversation_journal_path(&db, home.path(), &id).unwrap(),
            journal
        );
        assert_eq!(fs::read(&journal).unwrap(), bytes);
        let serialized = serde_json::to_value(&moved).unwrap();
        assert!(serialized["conversations"]
            .as_array()
            .unwrap()
            .iter()
            .any(|chat| chat["id"] == companion::GLOBAL_CONVERSATION_ID
                && chat["projectId"].is_null()));
        deletion::delete(
            &mut db,
            home.path(),
            &deletion::DeleteTarget::Project(target.id),
        )
        .unwrap();
        assert!(!journal.exists());
        assert!(read_conversation(&db, home.path(), companion::GLOBAL_CONVERSATION_ID).is_ok());
    }
}
