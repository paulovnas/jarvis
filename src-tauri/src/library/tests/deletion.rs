use super::*;
use crate::library::deletion::{delete, recover, DeleteTarget};

#[test]
fn beads_survives_conversation_deletion_and_project_removal_preserves_external_tracker() {
    let home = TestHome::new();
    let mut db = database();
    let project = setup_project(&mut db, &home);
    let conversation = insert_conversation(&mut db, &home.0, &project.id, "Synthetic")
        .unwrap()
        .conversations[0]
        .clone();
    let private = crate::core::beads::storage(&home.0, &project.id);
    fs::create_dir_all(&private).unwrap();
    fs::write(private.join("tasks.db"), "private tasks").unwrap();
    let external = Path::new(&project.path).join(".beads");
    fs::create_dir_all(&external).unwrap();
    fs::write(external.join("tasks.db"), "external tasks").unwrap();
    delete(
        &mut db,
        &home.0,
        &DeleteTarget::Conversation(conversation.id),
    )
    .unwrap();
    assert_eq!(
        fs::read_to_string(private.join("tasks.db")).unwrap(),
        "private tasks"
    );
    delete(&mut db, &home.0, &DeleteTarget::Project(project.id)).unwrap();
    assert!(!private.exists());
    assert_eq!(
        fs::read_to_string(external.join("tasks.db")).unwrap(),
        "external tasks"
    );
}

#[test]
fn beads_busy_cleanup_recovers_after_committed_project_deletion() {
    use fs2::FileExt;
    let home = TestHome::new();
    let mut db = database();
    let project = setup_project(&mut db, &home);
    let private = crate::core::beads::storage(&home.0, &project.id);
    fs::create_dir_all(&private).unwrap();
    fs::write(private.join("tasks.db"), "private tasks").unwrap();
    let locks = home.0.join(".jarvis/beads/locks");
    fs::create_dir_all(&locks).unwrap();
    let lock = fs::File::create(locks.join(format!("{}.lock", project.id))).unwrap();
    FileExt::lock_exclusive(&lock).unwrap();
    assert!(delete(&mut db, &home.0, &DeleteTarget::Project(project.id.clone())).is_err());
    assert!(snapshot(&db).unwrap().projects.is_empty());
    assert!(private.exists());
    drop(lock);
    recover(&db, &home.0).unwrap();
    assert!(!private.exists());
    assert!(Path::new(&project.path).is_dir());
}

#[test]
fn removes_only_selected_conversation_and_its_recovery_copies() {
    let home = TestHome::new();
    let mut db = database();
    let project = setup_project(&mut db, &home);
    let first = insert_conversation(&mut db, &home.0, &project.id, "First")
        .unwrap()
        .conversations[0]
        .clone();
    let second = insert_conversation(&mut db, &home.0, &project.id, "Second")
        .unwrap()
        .conversations[0]
        .clone();
    let path = session_path(&home.0, &project.id, &second.id, false).unwrap();
    let backup = path.with_extension(format!("recovery-{}.jsonl", new_id().unwrap()));
    fs::write(&backup, "private recovery").unwrap();
    let context_memory = crate::core::context::storage(&home.0, &second.id);
    let retained_memory = crate::core::context::storage(&home.0, &first.id);
    for path in [&context_memory, &retained_memory] {
        fs::create_dir_all(path).unwrap();
        fs::write(path.join("private.db"), "indexed private history").unwrap();
    }
    fs::write(Path::new(&project.path).join("source.txt"), "keep source").unwrap();
    let result = delete(
        &mut db,
        &home.0,
        &DeleteTarget::Conversation(second.id.clone()),
    )
    .unwrap();
    assert_eq!(result.conversations, vec![first.clone()]);
    assert_eq!(
        result.selection.project_id.as_deref(),
        Some(project.id.as_str())
    );
    assert!(result.selection.conversation_id.is_none());
    assert!(!path.exists());
    assert!(!backup.exists());
    assert!(!context_memory.exists());
    assert!(retained_memory.join("private.db").exists());
    assert!(read_conversation(&db, &home.0, &first.id).is_ok());
    assert_eq!(
        fs::read_to_string(Path::new(&project.path).join("source.txt")).unwrap(),
        "keep source"
    );
    assert_eq!(
        delete(&mut db, &home.0, &DeleteTarget::Conversation(second.id)).unwrap(),
        result
    );
}

#[test]
fn removes_project_histories_but_preserves_source_workspace_unrelated_selection_and_unknown_files()
{
    let home = TestHome::new();
    let mut db = database();
    let first = setup_project(&mut db, &home);
    let a = insert_conversation(&mut db, &home.0, &first.id, "A")
        .unwrap()
        .conversations[0]
        .clone();
    insert_conversation(&mut db, &home.0, &first.id, "B").unwrap();
    let history = session_path(&home.0, &first.id, &a.id, false)
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();
    let orphan = history.join(format!("{}.jsonl", new_id().unwrap()));
    fs::write(&orphan, "orphaned session").unwrap();
    fs::write(history.join("keep.txt"), "unknown user file").unwrap();
    let source = Path::new(&first.path).join("code.rs");
    fs::write(&source, "source content").unwrap();
    let added = insert_project(&mut db, &first.workspace_id, &home.project("other")).unwrap();
    let other = added
        .projects
        .iter()
        .find(|item| item.id != first.id)
        .unwrap();
    let before = insert_conversation(&mut db, &home.0, &other.id, "Other").unwrap();
    let result = delete(&mut db, &home.0, &DeleteTarget::Project(first.id)).unwrap();
    assert_eq!(result.selection, before.selection);
    assert_eq!(result.workspaces, before.workspaces);
    assert_eq!(result.projects.len(), 1);
    assert_eq!(result.conversations.len(), 1);
    assert_eq!(fs::read_to_string(source).unwrap(), "source content");
    assert!(!orphan.exists());
    assert_eq!(fs::read_dir(history).unwrap().count(), 1);
    assert!(read_conversation(&db, &home.0, &result.conversations[0].id).is_ok());
}

#[test]
fn selected_project_deletion_keeps_workspace_and_works_when_source_is_unavailable() {
    let home = TestHome::new();
    let mut db = database();
    let project = setup_project(&mut db, &home);
    insert_conversation(&mut db, &home.0, &project.id, "A").unwrap();
    fs::remove_dir(&project.path).unwrap();
    let result = delete(&mut db, &home.0, &DeleteTarget::Project(project.id.clone())).unwrap();
    assert_eq!(result.selection.workspace_id, Some(project.workspace_id));
    assert!(result.selection.project_id.is_none());
    assert!(result.selection.conversation_id.is_none());
    assert!(!home.0.join(".jarvis/sessions").join(project.id).exists());
}

#[test]
fn database_failure_restores_staged_files_and_navigation() {
    let home = TestHome::new();
    let mut db = database();
    let project = setup_project(&mut db, &home);
    let before = insert_conversation(&mut db, &home.0, &project.id, "A").unwrap();
    let path = session_path(&home.0, &project.id, &before.conversations[0].id, false).unwrap();
    let bytes = fs::read(&path).unwrap();
    db.execute_batch("CREATE TRIGGER prevent_delete BEFORE DELETE ON conversations BEGIN SELECT RAISE(ABORT, 'test failure'); END;").unwrap();
    assert!(delete(&mut db, &home.0, &DeleteTarget::Project(project.id)).is_err());
    assert_eq!(snapshot(&db).unwrap(), before);
    assert_eq!(fs::read(&path).unwrap(), bytes);
    assert_eq!(fs::read_dir(path.parent().unwrap()).unwrap().count(), 1);
}

#[test]
fn interrupted_deletions_restore_before_commit_and_finish_after_commit() {
    let home = TestHome::new();
    let mut db = database();
    let project = setup_project(&mut db, &home);
    let before = insert_conversation(&mut db, &home.0, &project.id, "A").unwrap();
    let id = &before.conversations[0].id;
    let path = session_path(&home.0, &project.id, id, false).unwrap();
    let pending = path.with_extension("jsonl.deleting");
    fs::rename(&path, &pending).unwrap();
    recover(&db, &home.0).unwrap();
    assert!(path.exists());
    assert!(!pending.exists());
    fs::rename(&path, &pending).unwrap();
    db.execute("UPDATE navigation_selection SET conversation_id = NULL", [])
        .unwrap();
    db.execute("DELETE FROM conversations WHERE id = ?1", [id])
        .unwrap();
    recover(&db, &home.0).unwrap();
    assert!(!path.exists());
    assert!(!pending.exists());
}

#[test]
fn project_recovery_preserves_unindexed_history_until_project_commit() {
    let home = TestHome::new();
    let mut db = database();
    let project = setup_project(&mut db, &home);
    let path = session_path(&home.0, &project.id, &new_id().unwrap(), true).unwrap();
    fs::write(&path, "unindexed history").unwrap();
    let pending = path.with_extension("jsonl.project-deleting");
    fs::rename(&path, &pending).unwrap();
    recover(&db, &home.0).unwrap();
    assert_eq!(fs::read_to_string(path).unwrap(), "unindexed history");
}

#[test]
fn missing_history_can_be_deleted_but_invalid_ids_and_recovery_collisions_preserve_files() {
    let home = TestHome::new();
    let mut db = database();
    let project = setup_project(&mut db, &home);
    let before = insert_conversation(&mut db, &home.0, &project.id, "A").unwrap();
    let id = before.conversations[0].id.clone();
    let path = session_path(&home.0, &project.id, &id, false).unwrap();
    let pending = path.with_extension("jsonl.deleting");
    fs::write(&pending, "preserve pending").unwrap();
    assert!(recover(&db, &home.0).is_err());
    assert!(path.exists());
    assert_eq!(fs::read_to_string(&pending).unwrap(), "preserve pending");
    assert!(delete(&mut db, &home.0, &DeleteTarget::Project("../source".into())).is_err());
    fs::remove_file(pending).unwrap();
    fs::remove_file(path).unwrap();
    assert!(delete(&mut db, &home.0, &DeleteTarget::Conversation(id))
        .unwrap()
        .conversations
        .is_empty());
}

#[cfg(unix)]
#[test]
fn symlinked_journal_never_deletes_the_linked_project_file() {
    let home = TestHome::new();
    let mut db = database();
    let project = setup_project(&mut db, &home);
    let before = insert_conversation(&mut db, &home.0, &project.id, "A").unwrap();
    let path = session_path(&home.0, &project.id, &before.conversations[0].id, false).unwrap();
    let source = Path::new(&project.path).join("source.txt");
    fs::write(&source, "never delete").unwrap();
    fs::remove_file(&path).unwrap();
    std::os::unix::fs::symlink(&source, path).unwrap();
    assert!(delete(&mut db, &home.0, &DeleteTarget::Project(project.id)).is_err());
    assert_eq!(snapshot(&db).unwrap(), before);
    assert_eq!(fs::read_to_string(source).unwrap(), "never delete");
}
