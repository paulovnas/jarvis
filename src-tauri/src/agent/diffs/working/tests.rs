use super::*;

fn run(root: &Path, args: &[&str]) {
    let output = crate::background::command("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
fn init(root: &Path) {
    run(root, &["init", "-q"]);
    run(root, &["config", "user.name", "Jarvis Test"]);
    run(root, &["config", "user.email", "test@example.invalid"]);
    run(root, &["config", "commit.gpgsign", "false"]);
}
fn commit(root: &Path) {
    run(
        root,
        &["-c", "core.hooksPath=/dev/null", "commit", "-qm", "test"],
    );
}
fn revision(before: &str, after: &str) -> FileRevision {
    FileRevision::new(
        "a.txt".into(),
        Some(before.into()),
        Some(after.into()),
        "conversation",
    )
}

#[test]
fn intersects_session_lines_with_partial_commits_and_other_dirty_changes() {
    let file = revision("external dirty\none\ntwo\n", "external dirty\nONE\nTWO\n");
    let live = "external dirty\nONE\nTWO\nother change\n";
    let filtered = pending(&file, Some("clean\nONE\ntwo\n"), Some(live));
    assert_eq!((filtered.additions, filtered.deletions), (Some(1), Some(1)));
    let rows = filtered.detail().rows;
    assert!(rows
        .iter()
        .any(|row| row.kind == "added" && row.text == "TWO"));
    assert!(rows
        .iter()
        .any(|row| row.kind == "removed" && row.text == "two"));
    assert!(!rows.iter().any(|row| row.kind != "context"
        && (row.text == "ONE"
            || row.text.contains("external")
            || row.text.contains("other change"))));
    assert!(!pending(&file, Some(live), Some(live)).changed());
    assert!(!pending(
        &file,
        Some("external dirty\nONE\nTWO\n"),
        Some("external dirty\nONE\nTWO\nunrelated\n")
    )
    .changed());
    assert!(!pending(
        &revision("original\n", "session\n"),
        Some("original\n"),
        Some("overwritten externally\n")
    )
    .changed());
}

#[test]
fn empty_file_creation_and_deletion_keep_zero_line_changes_until_committed() {
    let created = FileRevision::new("empty".into(), None, Some(String::new()), "conversation");
    let filtered = pending(&created, None, Some(""));
    assert!(filtered.changed());
    assert_eq!((filtered.additions, filtered.deletions), (Some(0), Some(0)));
    assert!(!pending(&created, Some(""), Some("")).changed());
    let deleted = FileRevision::new("empty".into(), Some(String::new()), None, "conversation");
    assert!(pending(&deleted, Some(""), None).changed());
    assert!(!pending(&deleted, None, None).changed());
}

#[tokio::test]
async fn live_git_excludes_other_files_staged_commits_and_keeps_post_commit_edits() {
    let fixture = crate::agent::tests::Fixture::new();
    let session = crate::agent::tests::session(&fixture);
    init(&fixture.root);
    std::fs::write(fixture.root.join("a.txt"), "one\ntwo\n").unwrap();
    run(&fixture.root, &["add", "a.txt"]);
    commit(&fixture.root);
    std::fs::write(fixture.root.join("a.txt"), "ONE\ntwo\n").unwrap();
    record(&session, revision("one\ntwo\n", "ONE\ntwo\n"))
        .await
        .unwrap();
    std::fs::write(fixture.root.join("unrelated.txt"), "dirty").unwrap();
    run(&fixture.root, &["add", "a.txt"]);
    assert_eq!(
        files(&session, None).await.unwrap().len(),
        1,
        "staging is not committing"
    );
    commit(&fixture.root);
    assert!(files(&session, None).await.unwrap().is_empty());
    // A new session edit may restore the original content after a commit.
    std::fs::write(fixture.root.join("a.txt"), "one\ntwo\n").unwrap();
    record(&session, revision("ONE\ntwo\n", "one\ntwo\n"))
        .await
        .unwrap();
    let pending = files(&session, None).await.unwrap();
    assert_eq!(
        (pending[0].additions, pending[0].deletions),
        (Some(1), Some(1))
    );
    let (_, extras) = journal::load_all(&session.journal).unwrap();
    session.data.lock().unwrap().extras = extras;
    assert_eq!(files(&session, None).await.unwrap().len(), 1);
    run(&fixture.root, &["add", "a.txt"]);
    commit(&fixture.root);
    std::fs::write(fixture.root.join("a.txt"), "one\ntwo\nexternal\n").unwrap();
    assert!(files(&session, None).await.unwrap().is_empty());
}

#[tokio::test]
async fn supports_unborn_repositories_deleted_files_and_literal_paths() {
    let fixture = crate::agent::tests::Fixture::new();
    let session = crate::agent::tests::session(&fixture);
    init(&fixture.root);
    // Brackets exercise git's --literal-pathspecs (it would glob [name]
    // otherwise) on every platform. An embedded newline additionally exercises
    // the NUL-delimited ls-tree parsing, but NTFS forbids it, so keep it Unix-only.
    let path = if cfg!(windows) {
        "strange [name].txt"
    } else {
        "strange [name]\n.txt"
    };
    std::fs::write(fixture.root.join(path), "new\n").unwrap();
    record(
        &session,
        FileRevision::new(path.into(), None, Some("new\n".into()), "conversation"),
    )
    .await
    .unwrap();
    assert_eq!(files(&session, None).await.unwrap()[0].additions, Some(1));
    run(&fixture.root, &["add", "--", path]);
    commit(&fixture.root);
    assert!(files(&session, None).await.unwrap().is_empty());
    std::fs::remove_file(fixture.root.join(path)).unwrap();
    record(
        &session,
        FileRevision::new(path.into(), Some("new\n".into()), None, "conversation"),
    )
    .await
    .unwrap();
    assert_eq!(files(&session, None).await.unwrap()[0].deletions, Some(1));
    run(&fixture.root, &["add", "--", path]);
    commit(&fixture.root);
    assert!(files(&session, None).await.unwrap().is_empty());
}

#[tokio::test]
async fn non_git_projects_use_session_ownership_and_reject_symlink_escapes() {
    let fixture = crate::agent::tests::Fixture::new();
    let session = crate::agent::tests::session(&fixture);
    std::fs::write(fixture.root.join("a.txt"), "new\n").unwrap();
    record(&session, revision("old\n", "new\n")).await.unwrap();
    assert_eq!(files(&session, None).await.unwrap().len(), 1);
    std::fs::write(fixture.root.join("a.txt"), "old\n").unwrap();
    assert!(files(&session, None).await.unwrap().is_empty());
    assert!(current(&fixture.root, "../outside").await.is_err());
    #[cfg(unix)]
    {
        let external = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink(external.path(), fixture.root.join("link")).unwrap();
        assert!(current(&fixture.root, "link/secret").await.is_err());
    }
}

#[tokio::test]
async fn legacy_unknown_edits_never_claim_live_disk_changes_as_session_work() {
    let fixture = crate::agent::tests::Fixture::new();
    let session = crate::agent::tests::session(&fixture);
    std::fs::write(fixture.root.join("a.txt"), "external\n").unwrap();
    session.data.lock().unwrap().extras.files.insert(
        "a.txt".into(),
        FileRevision::new("a.txt".into(), None, None, "unknown"),
    );
    assert!(files(&session, None).await.unwrap().is_empty());
    std::fs::write(fixture.root.join("a.txt"), "session\n").unwrap();
    record(&session, revision("external\n", "session\n"))
        .await
        .unwrap();
    let changes = files(&session, None).await.unwrap();
    assert_eq!(
        (changes[0].additions, changes[0].deletions),
        (Some(1), Some(1))
    );
}
