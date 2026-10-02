//! Internal persistence scope and silent project handoffs for Jarvito.
use super::*;

pub(crate) const GLOBAL_WORKSPACE_ID: &str = "00000000000000000000000000000001";
pub(crate) const GLOBAL_PROJECT_ID: &str = "00000000000000000000000000000002";
pub(crate) const GLOBAL_CONVERSATION_ID: &str = "00000000000000000000000000000003";

pub(crate) fn ensure_global(connection: &mut Connection, home: &Path) -> Result<(), LibraryError> {
    let root = crate::data_dir::root(home).join("companion").join("global");
    fs::create_dir_all(&root).map_err(|_| LibraryError::storage())?;
    let metadata = fs::symlink_metadata(&root).map_err(|_| LibraryError::storage())?;
    if metadata.is_symlink() || !metadata.is_dir() {
        return Err(LibraryError::invalid_session());
    }
    let root = canonical_directory(&root)?;
    let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    tx.execute(
        "INSERT OR IGNORE INTO workspaces(id,name) VALUES(?1,?2)",
        params![
            GLOBAL_WORKSPACE_ID,
            "Jarvito · internal-00000000000000000000000000000001"
        ],
    )?;
    tx.execute(
        "INSERT OR IGNORE INTO projects(id,workspace_id,name,path,icon,color) VALUES(?1,?2,'Jarvito',?3,'bot','cyan')",
        params![GLOBAL_PROJECT_ID, GLOBAL_WORKSPACE_ID, root.to_string_lossy()],
    )?;
    let stored = project(&tx, GLOBAL_PROJECT_ID)?;
    if stored.workspace_id != GLOBAL_WORKSPACE_ID || Path::new(&stored.path) != root {
        return Err(LibraryError::invalid_session());
    }
    let mut created_journal = None;
    let exists = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM conversations WHERE id=?1)",
        [GLOBAL_CONVERSATION_ID],
        |row| row.get::<_, bool>(0),
    )?;
    if exists {
        let details = read_conversation(&tx, home, GLOBAL_CONVERSATION_ID)?;
        if details.project.id != GLOBAL_PROJECT_ID {
            return Err(LibraryError::invalid_session());
        }
    } else {
        tx.execute(
            "INSERT INTO conversations(id,project_id,title,title_source) VALUES(?1,?2,'Jarvito','manual')",
            params![GLOBAL_CONVERSATION_ID, GLOBAL_PROJECT_ID],
        )?;
        let record = conversation(&tx, GLOBAL_CONVERSATION_ID)?;
        let path = session_path(home, GLOBAL_PROJECT_ID, GLOBAL_CONVERSATION_ID, true)?;
        let header = SessionHeader {
            kind: "session".into(),
            version: 1,
            id: GLOBAL_CONVERSATION_ID.into(),
            project_id: GLOBAL_PROJECT_ID.into(),
            cwd: stored.path,
            title: "Jarvito".into(),
            created_at: record.created_at,
        };
        match fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.is_file() && !metadata.is_symlink() => {
                // A process exit can leave the durable header before SQLite commits.
                // Reattach only the exact reserved journal, never overwrite its history.
                let file = File::open(&path).map_err(|_| LibraryError::storage())?;
                let mut line = String::new();
                BufReader::new(file.take(65_537))
                    .read_line(&mut line)
                    .map_err(|_| LibraryError::invalid_session())?;
                if line.len() > 65_536 || !line.ends_with('\n') {
                    return Err(LibraryError::invalid_session());
                }
                let existing: SessionHeader =
                    serde_json::from_str(&line).map_err(|_| LibraryError::invalid_session())?;
                if existing.kind != header.kind
                    || existing.version != header.version
                    || existing.id != header.id
                    || existing.project_id != header.project_id
                    || existing.title != header.title
                    || existing.cwd != header.cwd
                {
                    return Err(LibraryError::invalid_session());
                }
                tx.execute(
                    "UPDATE conversations SET created_at=?1 WHERE id=?2",
                    params![existing.created_at, GLOBAL_CONVERSATION_ID],
                )?;
                read_conversation(&tx, home, GLOBAL_CONVERSATION_ID)?;
            }
            Err(cause) if cause.kind() == std::io::ErrorKind::NotFound => {
                persist_header(&path, &header)?;
                created_journal = Some(path);
            }
            _ => return Err(LibraryError::invalid_session()),
        }
    }
    if tx.commit().is_err() {
        if let Some(path) = created_journal {
            let _ = fs::remove_file(path);
        }
        return Err(LibraryError::storage());
    }
    Ok(())
}

pub(crate) fn create_conversation_silently(
    connection: &mut Connection,
    home: &Path,
    project_id: &str,
) -> Result<String, LibraryError> {
    if project_id == GLOBAL_PROJECT_ID {
        return Err(LibraryError::missing());
    }
    let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let project = project(&tx, project_id)?;
    let root = project_location(&tx, project_id)?;
    if let Some(existing) = latest_empty_conversation(&tx, home, project_id, "Nova Conversa")? {
        tx.commit()?;
        return Ok(existing.id);
    }
    let id = new_id()?;
    tx.execute("INSERT INTO conversations(id,project_id,title,title_source) VALUES(?1,?2,'Nova Conversa','default')", params![id,project_id])?;
    let record = conversation(&tx, &id)?;
    let path = session_path(home, project_id, &id, true)?;
    persist_header(
        &path,
        &SessionHeader {
            kind: "session".into(),
            version: 1,
            id: id.clone(),
            project_id: project.id,
            cwd: root.to_string_lossy().into_owned(),
            title: "Nova Conversa".into(),
            created_at: record.created_at,
        },
    )?;
    if tx.commit().is_err() {
        let _ = fs::remove_file(path);
        return Err(LibraryError::storage());
    }
    Ok(id)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Scope {
    pub project_id: String,
    pub project_name: String,
    pub workspace_name: String,
}

pub(crate) fn scope(connection: &Connection, id: &str) -> Result<Scope, LibraryError> {
    let project = project(connection, id)?;
    if project.workspace_id == GLOBAL_WORKSPACE_ID {
        return Err(LibraryError::missing());
    }
    let workspace = workspace(connection, &project.workspace_id)?;
    Ok(Scope {
        project_id: project.id,
        project_name: project.name,
        workspace_name: workspace.name,
    })
}

pub(crate) fn conversation_scope(connection: &Connection, id: &str) -> Result<Scope, LibraryError> {
    scope(connection, &conversation(connection, id)?.project_id)
}

pub(crate) fn validate_conversation_scope(
    connection: &Connection,
    home: &Path,
    id: &str,
    project_id: &str,
) -> Result<(), LibraryError> {
    let details = read_conversation(connection, home, id)?;
    if details.project.id != project_id || project_id == GLOBAL_PROJECT_ID {
        return Err(LibraryError::missing());
    }
    project_location(connection, project_id)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn global_scope_is_idempotent_hidden_and_does_not_change_main_selection() {
        let home = tempfile::tempdir().unwrap();
        let mut db = Connection::open_in_memory().unwrap();
        crate::persistence::initialize_database(&mut db).unwrap();
        let selected = insert_workspace(&mut db, "Meu workspace")
            .unwrap()
            .selection;
        ensure_global(&mut db, home.path()).unwrap();
        ensure_global(&mut db, home.path()).unwrap();
        let visible = snapshot(&db).unwrap();
        assert_eq!(visible.selection, selected);
        assert_eq!(visible.workspaces.len(), 1);
        assert!(visible.projects.is_empty());
        assert!(visible.conversations.is_empty());
        assert_eq!(
            read_conversation(&db, home.path(), GLOBAL_CONVERSATION_ID)
                .unwrap()
                .project
                .id,
            GLOBAL_PROJECT_ID
        );
        assert!(scope(&db, GLOBAL_PROJECT_ID).is_err());
    }

    #[test]
    fn historical_hidden_selections_recover_to_the_first_visible_workspace_without_changing_history(
    ) {
        let home = tempfile::tempdir().unwrap();
        let mut db = Connection::open_in_memory().unwrap();
        crate::persistence::initialize_database(&mut db).unwrap();
        ensure_global(&mut db, home.path()).unwrap();
        let first = insert_workspace(&mut db, "Primeiro workspace")
            .unwrap()
            .selection;
        insert_workspace(&mut db, "Segundo workspace").unwrap();
        let history = session_path(
            home.path(),
            GLOBAL_PROJECT_ID,
            GLOBAL_CONVERSATION_ID,
            false,
        )
        .unwrap();
        let original = fs::read(&history).unwrap();
        for hidden in [
            Selection {
                workspace_id: Some(GLOBAL_WORKSPACE_ID.into()),
                ..Selection::default()
            },
            Selection {
                workspace_id: Some(GLOBAL_WORKSPACE_ID.into()),
                project_id: Some(GLOBAL_PROJECT_ID.into()),
                ..Selection::default()
            },
            Selection {
                workspace_id: Some(GLOBAL_WORKSPACE_ID.into()),
                project_id: Some(GLOBAL_PROJECT_ID.into()),
                conversation_id: Some(GLOBAL_CONVERSATION_ID.into()),
            },
        ] {
            save_selection(&db, &hidden).unwrap();
            let visible = snapshot(&db).unwrap();
            assert_eq!(visible.selection, first);
            assert_eq!(visible.workspaces.len(), 2);
            assert!(visible.projects.is_empty());
            assert!(visible.conversations.is_empty());
            let stored: (Option<String>, Option<String>, Option<String>) = db
                .query_row(
                    "SELECT workspace_id, project_id, conversation_id FROM navigation_selection WHERE id=1",
                    [],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
                .unwrap();
            assert_eq!(stored, (first.workspace_id.clone(), None, None));
        }
        assert_eq!(fs::read(&history).unwrap(), original);
        assert_eq!(
            read_conversation(&db, home.path(), GLOBAL_CONVERSATION_ID)
                .unwrap()
                .project
                .id,
            GLOBAL_PROJECT_ID
        );
    }

    #[test]
    fn hidden_selection_recovers_to_an_empty_selection_when_no_workspace_is_visible() {
        let home = tempfile::tempdir().unwrap();
        let mut db = Connection::open_in_memory().unwrap();
        crate::persistence::initialize_database(&mut db).unwrap();
        ensure_global(&mut db, home.path()).unwrap();
        save_selection(
            &db,
            &Selection {
                workspace_id: Some(GLOBAL_WORKSPACE_ID.into()),
                project_id: Some(GLOBAL_PROJECT_ID.into()),
                conversation_id: Some(GLOBAL_CONVERSATION_ID.into()),
            },
        )
        .unwrap();
        assert_eq!(
            snapshot(&db).unwrap(),
            LibrarySnapshot {
                workspaces: vec![],
                projects: vec![],
                conversations: vec![],
                selection: Selection::default(),
            }
        );
        let stored: (Option<String>, Option<String>, Option<String>) = db
            .query_row(
                "SELECT workspace_id, project_id, conversation_id FROM navigation_selection WHERE id=1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(stored, (None, None, None));
    }

    #[test]
    fn hidden_targets_cannot_replace_a_visible_conversation_selection() {
        let home = tempfile::tempdir().unwrap();
        let root = tempfile::tempdir().unwrap();
        let mut db = Connection::open_in_memory().unwrap();
        crate::persistence::initialize_database(&mut db).unwrap();
        let workspace = insert_workspace(&mut db, "Workspace").unwrap();
        let library = insert_project(&mut db, &workspace.workspaces[0].id, root.path()).unwrap();
        let expected =
            insert_conversation(&mut db, home.path(), &library.projects[0].id, "Trabalho").unwrap();
        ensure_global(&mut db, home.path()).unwrap();
        assert_eq!(snapshot(&db).unwrap(), expected);
        for target in [
            LibraryTarget::Workspace(GLOBAL_WORKSPACE_ID.into()),
            LibraryTarget::Project(GLOBAL_PROJECT_ID.into()),
            LibraryTarget::Conversation(GLOBAL_CONVERSATION_ID.into()),
        ] {
            assert_eq!(
                select_item(&mut db, home.path(), target).unwrap_err().code,
                "not_found"
            );
            assert_eq!(snapshot(&db).unwrap(), expected);
        }
    }

    #[test]
    fn interrupted_global_indexing_recovers_the_same_journal_without_overwriting() {
        let home = tempfile::tempdir().unwrap();
        let mut db = Connection::open_in_memory().unwrap();
        crate::persistence::initialize_database(&mut db).unwrap();
        ensure_global(&mut db, home.path()).unwrap();
        let path = session_path(
            home.path(),
            GLOBAL_PROJECT_ID,
            GLOBAL_CONVERSATION_ID,
            false,
        )
        .unwrap();
        let original = fs::read(&path).unwrap();
        db.execute(
            "DELETE FROM conversations WHERE id=?1",
            [GLOBAL_CONVERSATION_ID],
        )
        .unwrap();
        ensure_global(&mut db, home.path()).unwrap();
        assert_eq!(fs::read(&path).unwrap(), original);
        assert_eq!(
            read_conversation(&db, home.path(), GLOBAL_CONVERSATION_ID)
                .unwrap()
                .conversation
                .id,
            GLOBAL_CONVERSATION_ID
        );
    }

    #[test]
    fn project_handoff_creates_a_real_journal_without_navigating_main() {
        let home = tempfile::tempdir().unwrap();
        let root = tempfile::tempdir().unwrap();
        let mut db = Connection::open_in_memory().unwrap();
        crate::persistence::initialize_database(&mut db).unwrap();
        let workspace = insert_workspace(&mut db, "Workspace").unwrap();
        let library = insert_project(&mut db, &workspace.workspaces[0].id, root.path()).unwrap();
        let selected = library.selection;
        let project_id = &library.projects[0].id;
        let id = create_conversation_silently(&mut db, home.path(), project_id).unwrap();
        assert_eq!(snapshot(&db).unwrap().selection, selected);
        validate_conversation_scope(&db, home.path(), &id, project_id).unwrap();
        assert_eq!(
            create_conversation_silently(&mut db, home.path(), project_id).unwrap(),
            id
        );
        assert!(validate_conversation_scope(&db, home.path(), &id, GLOBAL_PROJECT_ID).is_err());
    }
}
