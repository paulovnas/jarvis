use super::*;
use crate::persistence::initialize_database;

mod deletion;

struct TestHome(PathBuf);

impl TestHome {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("jarvis-library-{}", new_id().unwrap()));
        fs::create_dir(&path).unwrap();
        Self(path)
    }

    fn project(&self, name: &str) -> PathBuf {
        let path = self.0.join(name);
        fs::create_dir(&path).unwrap();
        path
    }
}

impl Drop for TestHome {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn database() -> Connection {
    let mut connection = Connection::open_in_memory().unwrap();
    initialize_database(&mut connection).unwrap();
    connection
}

#[test]
fn automatic_titles_only_replace_defaults_and_preserve_manual_overrides_and_header() {
    let home = TestHome::new();
    let app_state = AppState::default();
    let id = app_state
        .with_connection(&home.0, |connection| {
            let project = setup_project(connection, &home);
            let snapshot = insert_conversation(connection, &home.0, &project.id, "Nova Conversa")?;
            Ok::<_, LibraryError>(snapshot.conversations[0].id.clone())
        })
        .unwrap();
    assert!(needs_generated_title(&app_state, &home.0, &id).unwrap());
    assert!(
        save_generated_title(&app_state, &home.0, &id, "Resumo da primeira interação").unwrap()
    );
    assert!(!needs_generated_title(&app_state, &home.0, &id).unwrap());
    assert!(!save_generated_title(&app_state, &home.0, &id, "Resposta atrasada").unwrap());
    app_state
        .with_connection(&home.0, |connection| {
            let details = read_conversation(connection, &home.0, &id)?;
            assert_eq!(details.conversation.title, "Resumo da primeira interação");
            assert_eq!(details.conversation.initial_title, "Nova Conversa");
            rename_conversation_record(connection, &home.0, &id, "Meu título")?;
            Ok::<_, LibraryError>(())
        })
        .unwrap();
    assert!(
        !save_generated_title(&app_state, &home.0, &id, "Título gerado depois da edição").unwrap()
    );
    app_state
        .with_connection(&home.0, |connection| {
            assert_eq!(
                read_conversation(connection, &home.0, &id)?
                    .conversation
                    .title,
                "Meu título"
            );
            Ok::<_, LibraryError>(())
        })
        .unwrap();
}

fn setup_project(connection: &mut Connection, home: &TestHome) -> Project {
    let state = insert_workspace(connection, "Pessoal").unwrap();
    let state =
        insert_project(connection, &state.workspaces[0].id, &home.project("source")).unwrap();
    state.projects[0].clone()
}

#[test]
fn fresh_library_is_empty_and_workspaces_are_only_named_groups() {
    let mut connection = database();
    assert_eq!(
        snapshot(&connection).unwrap(),
        LibrarySnapshot {
            workspaces: vec![],
            projects: vec![],
            conversations: vec![],
            selection: Selection::default(),
        }
    );
    let created = insert_workspace(&mut connection, "  Pessoal  ").unwrap();
    assert_eq!(created.workspaces[0].name, "Pessoal");
    assert!(valid_id(&created.workspaces[0].id));
    assert_eq!(
        created.selection.workspace_id.as_ref(),
        Some(&created.workspaces[0].id)
    );
    assert!(created.projects.is_empty());
    for invalid in ["", " \t ", "Bad\nName", &"x".repeat(121)] {
        assert_eq!(
            insert_workspace(&mut connection, invalid).unwrap_err().code,
            "invalid_name"
        );
    }
    assert_eq!(
        insert_workspace(&mut connection, "Pessoal")
            .unwrap_err()
            .code,
        "duplicate_workspace"
    );
    assert_eq!(snapshot(&connection).unwrap(), created);
}

#[test]
fn version_two_upgrade_preserves_onboarding_and_provider_accounts_without_seeds() {
    let mut connection = Connection::open_in_memory().unwrap();
    connection
        .execute_batch(include_str!("../../../drizzle/0000_heavy_tomas.sql"))
        .unwrap();
    connection
        .execute_batch(include_str!("../../../drizzle/0001_nervous_nighthawk.sql"))
        .unwrap();
    connection
        .execute(
            "INSERT INTO app_config (id, onboarding_completed) VALUES (1, 1)",
            [],
        )
        .unwrap();
    connection.execute("INSERT INTO provider_accounts (alias, provider_kind, account_id) VALUES ('test-account', 'openai-codex', 'account')", []).unwrap();
    connection.pragma_update(None, "user_version", 2).unwrap();
    initialize_database(&mut connection).unwrap();
    initialize_database(&mut connection).unwrap();
    assert!(connection
        .query_row("SELECT onboarding_completed FROM app_config", [], |row| row
            .get::<_, bool>(0))
        .unwrap());
    assert_eq!(
        connection
            .query_row("SELECT account_id FROM provider_accounts", [], |row| row
                .get::<_, String>(
                0
            ))
            .unwrap(),
        "account"
    );
    assert!(snapshot(&connection).unwrap().workspaces.is_empty());
    assert_eq!(
        connection
            .pragma_query_value(None, "user_version", |row| row.get::<_, i64>(0))
            .unwrap(),
        7
    );
}

#[test]
fn projects_validate_folders_and_parent_without_touching_sources() {
    let home = TestHome::new();
    let mut connection = database();
    let project = setup_project(&mut connection, &home);
    assert_eq!(project.name, "source");
    assert_eq!(
        Path::new(&project.path),
        fs::canonicalize(home.0.join("source")).unwrap()
    );
    let before = snapshot(&connection).unwrap();
    assert_eq!(
        insert_project(
            &mut connection,
            &project.workspace_id,
            Path::new(&project.path)
        )
        .unwrap_err()
        .code,
        "duplicate_project"
    );
    let other = home.project("other");
    assert_eq!(
        insert_project(&mut connection, "missing", &other)
            .unwrap_err()
            .code,
        "not_found"
    );
    assert_eq!(
        insert_project(
            &mut connection,
            &project.workspace_id,
            &home.0.join("absent")
        )
        .unwrap_err()
        .code,
        "project_directory"
    );
    let file = home.0.join("regular-file");
    fs::write(&file, "contents").unwrap();
    assert_eq!(
        insert_project(&mut connection, &project.workspace_id, &file)
            .unwrap_err()
            .code,
        "project_directory"
    );
    assert_eq!(snapshot(&connection).unwrap(), before);
    assert_eq!(fs::read_dir(&project.path).unwrap().count(), 0);
}

#[cfg(unix)]
#[test]
fn symlink_aliases_cannot_register_the_same_project_in_another_workspace() {
    let home = TestHome::new();
    let mut connection = database();
    let project = setup_project(&mut connection, &home);
    let other = insert_workspace(&mut connection, "Trabalho")
        .unwrap()
        .selection
        .workspace_id
        .unwrap();
    let alias = home.0.join("alias");
    std::os::unix::fs::symlink(&project.path, &alias).unwrap();
    assert_eq!(
        insert_project(&mut connection, &other, &alias)
            .unwrap_err()
            .code,
        "duplicate_project"
    );
    assert_eq!(snapshot(&connection).unwrap().projects.len(), 1);
}

#[test]
fn conversation_header_and_selection_survive_database_reopening() {
    let home = TestHome::new();
    let db = home.0.join("index.db");
    let mut connection = Connection::open(&db).unwrap();
    initialize_database(&mut connection).unwrap();
    let project = setup_project(&mut connection, &home);
    let created =
        insert_conversation(&mut connection, &home.0, &project.id, "  Planejamento  ").unwrap();
    let id = created.selection.conversation_id.as_ref().unwrap();
    let path = session_path(&home.0, &project.id, id, false).unwrap();
    let bytes = fs::read_to_string(&path).unwrap();
    assert_eq!(bytes.lines().count(), 1);
    assert!(bytes.ends_with('\n'));
    let header: SessionHeader = serde_json::from_str(&bytes).unwrap();
    assert_eq!(header.kind, "session");
    assert_eq!(header.version, 1);
    assert_eq!(header.id, *id);
    assert_eq!(header.project_id, project.id);
    assert_eq!(header.cwd, project.path);
    assert_eq!(header.title, "Planejamento");
    assert_eq!(fs::read_dir(&project.path).unwrap().count(), 0);
    drop(connection);
    let mut reopened = Connection::open(db).unwrap();
    initialize_database(&mut reopened).unwrap();
    assert_eq!(snapshot(&reopened).unwrap(), created);
    let details = read_conversation(&reopened, &home.0, id).unwrap();
    assert_eq!(details.project, project);
    assert_eq!(details.conversation.title, "Planejamento");
}

#[test]
fn selection_resolves_ancestors_and_clears_descendants() {
    let home = TestHome::new();
    let mut connection = database();
    let project = setup_project(&mut connection, &home);
    let saved = insert_conversation(&mut connection, &home.0, &project.id, "First").unwrap();
    let other_workspace = insert_workspace(&mut connection, "Trabalho")
        .unwrap()
        .selection
        .workspace_id
        .unwrap();
    let other_project = insert_project(&mut connection, &other_workspace, &home.project("work"))
        .unwrap()
        .selection
        .project_id
        .unwrap();
    let other = insert_conversation(&mut connection, &home.0, &other_project, "Second").unwrap();
    assert_eq!(
        other
            .conversations
            .iter()
            .filter(|item| item.project_id == other_project)
            .count(),
        1
    );
    let restored = select_item(
        &mut connection,
        &home.0,
        LibraryTarget::Conversation(saved.selection.conversation_id.clone().unwrap()),
    )
    .unwrap();
    assert_eq!(restored.selection, saved.selection);
    let selected_project = select_item(
        &mut connection,
        &home.0,
        LibraryTarget::Project(other_project.clone()),
    )
    .unwrap();
    assert_eq!(
        selected_project.selection,
        Selection {
            workspace_id: Some(other_workspace.clone()),
            project_id: Some(other_project),
            conversation_id: None
        }
    );
    let selected_workspace = select_item(
        &mut connection,
        &home.0,
        LibraryTarget::Workspace(project.workspace_id.clone()),
    )
    .unwrap();
    assert_eq!(
        selected_workspace.selection,
        Selection {
            workspace_id: Some(project.workspace_id),
            ..Selection::default()
        }
    );
    assert_eq!(
        select_item(
            &mut connection,
            &home.0,
            LibraryTarget::Conversation("missing".into())
        )
        .unwrap_err()
        .code,
        "not_found"
    );
    assert_eq!(snapshot(&connection).unwrap(), selected_workspace);
}

#[test]
fn unavailable_project_cannot_create_a_conversation() {
    let home = TestHome::new();
    let mut connection = database();
    let project = setup_project(&mut connection, &home);
    fs::remove_dir(&project.path).unwrap();
    let before = snapshot(&connection).unwrap();
    assert_eq!(
        insert_conversation(&mut connection, &home.0, &project.id, "Test")
            .unwrap_err()
            .code,
        "project_directory"
    );
    assert_eq!(snapshot(&connection).unwrap(), before);
}

#[test]
fn missing_corrupt_oversized_or_mismatched_history_is_preserved_and_rejected() {
    let home = TestHome::new();
    let mut connection = database();
    let project = setup_project(&mut connection, &home);
    let created = insert_conversation(&mut connection, &home.0, &project.id, "Test").unwrap();
    let id = created.selection.conversation_id.unwrap();
    let path = session_path(&home.0, &project.id, &id, false).unwrap();
    let original = fs::read_to_string(&path).unwrap();
    select_item(
        &mut connection,
        &home.0,
        LibraryTarget::Project(project.id.clone()),
    )
    .unwrap();
    let before = snapshot(&connection).unwrap();
    for bytes in [
        "bad json\n".to_owned(),
        "".into(),
        original.trim().to_owned(),
        format!("{}\n", "x".repeat(65_537)),
        original.replace("Test", "Changed"),
    ] {
        fs::write(&path, &bytes).unwrap();
        assert_eq!(
            select_item(
                &mut connection,
                &home.0,
                LibraryTarget::Conversation(id.clone())
            )
            .unwrap_err()
            .code,
            "invalid_session"
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), bytes);
        assert_eq!(snapshot(&connection).unwrap(), before);
    }
    fs::remove_file(&path).unwrap();
    assert_eq!(
        read_conversation(&connection, &home.0, &id)
            .unwrap_err()
            .code,
        "invalid_session"
    );
    assert!(!path.exists());
}

#[test]
fn filesystem_failure_rolls_back_index_and_database_failure_removes_new_file() {
    let home = TestHome::new();
    let mut connection = database();
    let project = setup_project(&mut connection, &home);
    let before = snapshot(&connection).unwrap();
    fs::write(home.0.join(".jarvis"), "occupied").unwrap();
    assert!(insert_conversation(&mut connection, &home.0, &project.id, "Test").is_err());
    assert_eq!(snapshot(&connection).unwrap(), before);
    fs::remove_file(home.0.join(".jarvis")).unwrap();
    connection.execute_batch("CREATE TRIGGER fail_selection BEFORE UPDATE ON navigation_selection BEGIN SELECT RAISE(ABORT, 'injected failure'); END;").unwrap();
    assert_eq!(
        insert_conversation(&mut connection, &home.0, &project.id, "Test")
            .unwrap_err()
            .code,
        "database"
    );
    assert_eq!(snapshot(&connection).unwrap(), before);
    assert_eq!(
        fs::read_dir(home.0.join(".jarvis/sessions").join(&project.id))
            .unwrap()
            .count(),
        0
    );
}

#[test]
fn existing_session_file_is_never_overwritten_and_ids_cannot_escape_storage() {
    let home = TestHome::new();
    let path = home.0.join("existing.jsonl");
    fs::write(&path, "preserve me").unwrap();
    let header = SessionHeader {
        kind: "session".into(),
        version: 1,
        id: new_id().unwrap(),
        project_id: new_id().unwrap(),
        cwd: "/tmp".into(),
        title: "Test".into(),
        created_at: 1,
    };
    assert!(persist_header(&path, &header).is_err());
    assert_eq!(fs::read_to_string(path).unwrap(), "preserve me");
    assert!(session_path(&home.0, "../outside", &header.id, true).is_err());
    assert!(session_path(&home.0, &header.project_id, "../outside", true).is_err());
}

#[cfg(unix)]
#[test]
fn session_files_are_private_and_symlinks_are_not_read_or_written() {
    use std::os::unix::fs::{symlink, PermissionsExt};
    let home = TestHome::new();
    let mut connection = database();
    let project = setup_project(&mut connection, &home);
    let created = insert_conversation(&mut connection, &home.0, &project.id, "Test").unwrap();
    let id = created.selection.conversation_id.unwrap();
    let path = session_path(&home.0, &project.id, &id, false).unwrap();
    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert_eq!(
        fs::metadata(path.parent().unwrap())
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
    let real = home.0.join("real-history");
    fs::rename(&path, &real).unwrap();
    symlink(&real, &path).unwrap();
    assert_eq!(
        read_conversation(&connection, &home.0, &id)
            .unwrap_err()
            .code,
        "invalid_session"
    );
    fs::remove_file(&path).unwrap();
    let sessions = home.0.join(".jarvis/sessions");
    let relocated = home.0.join("relocated");
    fs::rename(&sessions, &relocated).unwrap();
    symlink(&relocated, &sessions).unwrap();
    assert_eq!(
        insert_conversation(&mut connection, &home.0, &project.id, "Next")
            .unwrap_err()
            .code,
        "invalid_session"
    );
    assert_eq!(snapshot(&connection).unwrap().conversations.len(), 1);
}

#[test]
fn concurrent_creation_through_app_state_keeps_distinct_sessions() {
    let home = TestHome::new();
    let state = AppState::default();
    let project = state
        .with_connection(&home.0, |connection| -> Result<Project, LibraryError> {
            Ok(setup_project(connection, &home))
        })
        .unwrap();
    let workers: Vec<_> = (0..4)
        .map(|index| {
            let state = state.clone();
            let home = home.0.clone();
            let project_id = project.id.clone();
            std::thread::spawn(move || {
                state
                    .with_connection(&home, |connection| {
                        insert_conversation(
                            connection,
                            &home,
                            &project_id,
                            &format!("Session {index}"),
                        )
                    })
                    .unwrap()
                    .selection
                    .conversation_id
                    .unwrap()
            })
        })
        .collect();
    let ids: std::collections::HashSet<_> = workers
        .into_iter()
        .map(|worker| worker.join().unwrap())
        .collect();
    assert_eq!(ids.len(), 4);
    drop(state);
    let restarted = AppState::default();
    let stored = restarted
        .with_connection(&home.0, |connection| snapshot(connection))
        .unwrap();
    assert_eq!(stored.conversations.len(), 4);
    for id in ids {
        restarted
            .with_connection(&home.0, |connection| {
                read_conversation(connection, &home.0, &id)
            })
            .unwrap();
    }
}

#[test]
fn renaming_preserves_paths_ids_history_and_selection_after_restart() {
    let home = TestHome::new();
    let state = AppState::default();
    let project = state
        .with_connection(&home.0, |connection| -> Result<Project, LibraryError> {
            Ok(setup_project(connection, &home))
        })
        .unwrap();
    let created = state
        .with_connection(&home.0, |connection| {
            insert_conversation(connection, &home.0, &project.id, "Nova Conversa")
        })
        .unwrap();
    let id = created.selection.conversation_id.clone().unwrap();
    let path = session_path(&home.0, &project.id, &id, false).unwrap();
    let bytes = fs::read(&path).unwrap();
    state
        .with_connection(&home.0, |connection| {
            rename_project_record(connection, &project.id, "  Meu projeto  ")
        })
        .unwrap();
    let renamed = state
        .with_connection(&home.0, |connection| {
            rename_conversation_record(connection, &home.0, &id, "Planejar autenticação")
        })
        .unwrap();
    assert_eq!(renamed.selection, created.selection);
    assert_eq!(renamed.projects[0].path, project.path);
    assert_eq!(renamed.projects[0].name, "Meu projeto");
    assert_eq!(renamed.conversations[0].title, "Planejar autenticação");
    assert_eq!(renamed.conversations[0].initial_title, "Nova Conversa");
    assert_eq!(fs::read(&path).unwrap(), bytes);
    assert!(Path::new(&project.path).is_dir());
    assert_eq!(fs::read_dir(&project.path).unwrap().count(), 0);
    drop(state);
    let restarted = AppState::default();
    let restored = restarted
        .with_connection(&home.0, |connection| {
            read_conversation(connection, &home.0, &id)
        })
        .unwrap();
    assert_eq!(restored.conversation.title, "Planejar autenticação");
    assert_eq!(restored.project.name, "Meu projeto");
    assert_eq!(
        restarted
            .with_connection(&home.0, |connection| snapshot(connection))
            .unwrap(),
        renamed
    );
}

#[test]
fn manual_default_title_is_distinguished_from_an_untitled_conversation() {
    let home = TestHome::new();
    let mut connection = database();
    let project = setup_project(&mut connection, &home);
    let saved =
        insert_conversation(&mut connection, &home.0, &project.id, "Nova Conversa").unwrap();
    let id = saved.selection.conversation_id.unwrap();
    let source = |connection: &Connection| {
        connection
            .query_row(
                "SELECT title_source FROM conversations WHERE id = ?1",
                [&id],
                |row| row.get::<_, String>(0),
            )
            .unwrap()
    };
    assert_eq!(source(&connection), "default");
    rename_conversation_record(&mut connection, &home.0, &id, "Nova Conversa").unwrap();
    assert_eq!(source(&connection), "manual");
    assert!(connection
        .execute(
            "UPDATE conversations SET title_source = 'unknown' WHERE id = ?1",
            [&id]
        )
        .is_err());
}

#[test]
fn invalid_or_failed_renames_preserve_existing_metadata_and_history() {
    let home = TestHome::new();
    let mut connection = database();
    let project = setup_project(&mut connection, &home);
    let before = insert_conversation(&mut connection, &home.0, &project.id, "Original").unwrap();
    let id = before.selection.conversation_id.as_ref().unwrap();
    let path = session_path(&home.0, &project.id, id, false).unwrap();
    let bytes = fs::read(&path).unwrap();
    for invalid in ["", " \t ", "Title\nline", &"x".repeat(121)] {
        assert_eq!(
            rename_project_record(&mut connection, &project.id, invalid)
                .unwrap_err()
                .code,
            "invalid_name"
        );
        assert_eq!(
            rename_conversation_record(&mut connection, &home.0, id, invalid)
                .unwrap_err()
                .code,
            "invalid_name"
        );
    }
    assert_eq!(
        rename_project_record(&mut connection, "missing", "Title")
            .unwrap_err()
            .code,
        "not_found"
    );
    assert_eq!(
        rename_conversation_record(&mut connection, &home.0, "missing", "Title")
            .unwrap_err()
            .code,
        "not_found"
    );
    connection.execute_batch("CREATE TRIGGER fail_rename BEFORE UPDATE ON conversations BEGIN SELECT RAISE(ABORT, 'injected failure'); END;").unwrap();
    assert_eq!(
        rename_conversation_record(&mut connection, &home.0, id, "Other")
            .unwrap_err()
            .code,
        "database"
    );
    assert_eq!(snapshot(&connection).unwrap(), before);
    assert_eq!(fs::read(&path).unwrap(), bytes);
    fs::write(&path, "corrupted\n").unwrap();
    assert_eq!(
        rename_conversation_record(&mut connection, &home.0, id, "Other")
            .unwrap_err()
            .code,
        "invalid_session"
    );
    assert_eq!(snapshot(&connection).unwrap(), before);
    assert_eq!(fs::read_to_string(&path).unwrap(), "corrupted\n");
}

#[test]
fn version_three_migration_keeps_existing_conversation_names_and_navigation() {
    let mut connection = Connection::open_in_memory().unwrap();
    connection
        .execute_batch(include_str!("../../../drizzle/0000_heavy_tomas.sql"))
        .unwrap();
    connection
        .execute_batch(include_str!("../../../drizzle/0001_nervous_nighthawk.sql"))
        .unwrap();
    connection
        .execute_batch(include_str!("../../../drizzle/0002_silky_meltdown.sql"))
        .unwrap();
    connection.execute_batch("INSERT INTO app_config VALUES (1, 1);
        INSERT INTO workspaces (id, name) VALUES ('w', 'Personal');
        INSERT INTO projects (id, workspace_id, name, path) VALUES ('p', 'w', 'Project', '/project');
        INSERT INTO conversations (id, project_id, title) VALUES ('c', 'p', 'Original title');
        INSERT INTO navigation_selection VALUES (1, 'w', 'p', 'c');
        PRAGMA user_version = 3;").unwrap();
    initialize_database(&mut connection).unwrap();
    initialize_database(&mut connection).unwrap();
    let restored = snapshot(&connection).unwrap();
    assert_eq!(restored.conversations[0].title, "Original title");
    assert_eq!(restored.selection.conversation_id.as_deref(), Some("c"));
    assert_eq!(restored.selection.project_id.as_deref(), Some("p"));
    assert_eq!(
        connection
            .query_row("SELECT title_source FROM conversations", [], |row| row
                .get::<_, String>(
                0
            ))
            .unwrap(),
        "manual"
    );
    assert!(!connection
        .prepare("PRAGMA foreign_key_check")
        .unwrap()
        .exists([])
        .unwrap());
}
