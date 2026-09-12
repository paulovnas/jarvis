use super::*;
use crate::library::{
    deletion::DeleteTarget,
    workspaces::{move_project, usage},
};

#[test]
fn moving_a_project_keeps_session_identity_source_and_selected_conversation() {
    let home = TestHome::new();
    let mut db = database();
    let project = setup_project(&mut db, &home);
    let chat = insert_conversation(&mut db, &home.0, &project.id, "Chat")
        .unwrap()
        .conversations[0]
        .clone();
    let path = session_path(&home.0, &project.id, &chat.id, false).unwrap();
    let journal = fs::read(&path).unwrap();
    let workspace = insert_workspace(&mut db, "Trabalho")
        .unwrap()
        .selection
        .workspace_id
        .unwrap();
    select_item(
        &mut db,
        &home.0,
        LibraryTarget::Conversation(chat.id.clone()),
    )
    .unwrap();
    let moved = move_project(&mut db, &project.id, &workspace).unwrap();
    assert_eq!(
        moved.selection.workspace_id.as_deref(),
        Some(workspace.as_str())
    );
    assert_eq!(
        moved.selection.conversation_id.as_deref(),
        Some(chat.id.as_str())
    );
    assert_eq!(
        read_conversation(&db, &home.0, &chat.id)
            .unwrap()
            .project
            .path,
        project.path
    );
    assert_eq!(fs::read(path).unwrap(), journal);
    assert!(move_project(&mut db, &project.id, &new_id().unwrap()).is_err());
    assert_eq!(snapshot(&db).unwrap(), moved);
}

#[test]
fn workspace_storage_counts_owned_history_and_attachments_not_project_source() {
    let home = TestHome::new();
    let mut db = database();
    let project = setup_project(&mut db, &home);
    let chat = insert_conversation(&mut db, &home.0, &project.id, "Chat")
        .unwrap()
        .conversations[0]
        .clone();
    let path = session_path(&home.0, &project.id, &chat.id, false).unwrap();
    let journal = fs::metadata(path).unwrap().len();
    let attachments = crate::data_dir::root(&home.0)
        .join("attachments")
        .join(&chat.id);
    fs::create_dir_all(&attachments).unwrap();
    fs::write(attachments.join("file.txt"), "0123456789").unwrap();
    let memory = crate::core::context::storage(&home.0, &chat.id);
    fs::create_dir_all(&memory).unwrap();
    fs::write(memory.join("memory.db"), "12345").unwrap();
    fs::write(
        Path::new(&project.path).join("source.txt"),
        vec![0; 100_000],
    )
    .unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(&project.path, attachments.join("source-link")).unwrap();
    let value = serde_json::to_value(usage(&db, &home.0).unwrap()).unwrap();
    assert_eq!(value[0]["bytes"], journal + 15);
    assert_eq!(value[0]["conversations"], 1);
    assert_eq!(value[0]["projects"][0]["bytes"], journal + 15);
}

#[test]
fn workspace_deletion_removes_all_histories_and_keeps_other_workspaces_and_source() {
    let home = TestHome::new();
    let mut db = database();
    let project = setup_project(&mut db, &home);
    let second = insert_project(&mut db, &project.workspace_id, &home.project("second"))
        .unwrap()
        .selection
        .project_id
        .unwrap();
    let first_chat = insert_conversation(&mut db, &home.0, &project.id, "A")
        .unwrap()
        .selection
        .conversation_id
        .unwrap();
    insert_conversation(&mut db, &home.0, &second, "B").unwrap();
    let source = Path::new(&project.path).join("source.txt");
    fs::write(&source, "preserve").unwrap();
    let other = insert_workspace(&mut db, "Other")
        .unwrap()
        .selection
        .workspace_id
        .unwrap();
    let third = insert_project(&mut db, &other, &home.project("third"))
        .unwrap()
        .selection
        .project_id
        .unwrap();
    let surviving = insert_conversation(&mut db, &home.0, &third, "C")
        .unwrap()
        .selection
        .conversation_id
        .unwrap();
    select_item(
        &mut db,
        &home.0,
        LibraryTarget::Conversation(first_chat.clone()),
    )
    .unwrap();
    let target = DeleteTarget::Workspace(project.workspace_id);
    assert_eq!(
        crate::library::deletion::conversation_ids(&db, &target)
            .unwrap()
            .len(),
        2
    );
    let deleted = crate::library::deletion::delete(&mut db, &home.0, &target).unwrap();
    assert_eq!(deleted.projects.len(), 1);
    assert_eq!(deleted.conversations.len(), 1);
    assert_eq!(deleted.conversations[0].id, surviving);
    assert_eq!(
        deleted.selection.workspace_id.as_deref(),
        Some(other.as_str())
    );
    assert!(deleted.selection.conversation_id.is_none());
    assert_eq!(fs::read_to_string(source).unwrap(), "preserve");
    assert!(!crate::data_dir::root(&home.0)
        .join("sessions")
        .join(&project.id)
        .join(format!("{first_chat}.jsonl"))
        .exists());
    assert!(read_conversation(&db, &home.0, &surviving).is_ok());
    assert_eq!(
        crate::library::deletion::delete(&mut db, &home.0, &target).unwrap(),
        deleted
    );
}

#[test]
#[cfg(unix)]
fn workspace_deletion_rolls_back_all_projects_if_a_later_journal_cannot_be_staged() {
    let home = TestHome::new();
    let mut db = database();
    let first = setup_project(&mut db, &home);
    let a = insert_conversation(&mut db, &home.0, &first.id, "A")
        .unwrap()
        .selection
        .conversation_id
        .unwrap();
    let second = insert_project(&mut db, &first.workspace_id, &home.project("second"))
        .unwrap()
        .selection
        .project_id
        .unwrap();
    let b = insert_conversation(&mut db, &home.0, &second, "B")
        .unwrap()
        .selection
        .conversation_id
        .unwrap();
    let path = session_path(&home.0, &second, &b, false).unwrap();
    let original = fs::read(&path).unwrap();
    // Non-journal files are left alone during recovery; a symlink journal
    // prevents staging before metadata can be removed.
    #[cfg(unix)]
    {
        fs::remove_file(&path).unwrap();
        std::os::unix::fs::symlink(home.0.join("missing"), &path).unwrap();
        assert!(crate::library::deletion::delete(
            &mut db,
            &home.0,
            &DeleteTarget::Workspace(first.workspace_id)
        )
        .is_err());
        assert_eq!(snapshot(&db).unwrap().conversations.len(), 2);
        assert!(read_conversation(&db, &home.0, &a).is_ok());
        fs::remove_file(&path).unwrap();
    }
    fs::write(path, original).unwrap();
}

#[test]
fn workspace_deletion_restores_every_staged_journal_after_metadata_failure() {
    let home = TestHome::new();
    let mut db = database();
    let project = setup_project(&mut db, &home);
    let a = insert_conversation(&mut db, &home.0, &project.id, "A")
        .unwrap()
        .selection
        .conversation_id
        .unwrap();
    let second = insert_project(&mut db, &project.workspace_id, &home.project("second"))
        .unwrap()
        .selection
        .project_id
        .unwrap();
    let b = insert_conversation(&mut db, &home.0, &second, "B")
        .unwrap()
        .selection
        .conversation_id
        .unwrap();
    let before = snapshot(&db).unwrap();
    db.execute_batch("CREATE TRIGGER prevent_workspace_delete BEFORE DELETE ON workspaces BEGIN SELECT RAISE(FAIL, 'synthetic failure'); END;").unwrap();
    assert!(crate::library::deletion::delete(
        &mut db,
        &home.0,
        &DeleteTarget::Workspace(project.workspace_id)
    )
    .is_err());
    assert_eq!(snapshot(&db).unwrap(), before);
    assert!(read_conversation(&db, &home.0, &a).is_ok());
    assert!(read_conversation(&db, &home.0, &b).is_ok());
}
