use super::*;
mod working;
use similar::{ChangeTag, TextDiff};
use std::{io::Read, path::Path, process::Stdio};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct FileRevision {
    pub path: String,
    pub before: Option<String>,
    pub after: Option<String>,
    pub base: String,
    pub additions: Option<u64>,
    pub deletions: Option<u64>,
    #[serde(default)]
    pub revision: u64,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileSummary {
    pub path: String,
    pub additions: Option<u64>,
    pub deletions: Option<u64>,
    pub base: String,
    pub revision: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffRow {
    kind: &'static str,
    old_line: Option<usize>,
    new_line: Option<usize>,
    text: String,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileDiff {
    path: String,
    base: String,
    rows: Vec<DiffRow>,
    truncated: bool,
}

impl FileRevision {
    pub fn new(path: String, before: Option<String>, after: Option<String>, base: &str) -> Self {
        let diff = TextDiff::configure()
            .timeout(Duration::from_millis(300))
            .diff_lines(
                before.as_deref().unwrap_or(""),
                after.as_deref().unwrap_or(""),
            );
        let mut additions = 0;
        let mut deletions = 0;
        for change in diff.iter_all_changes() {
            match change.tag() {
                ChangeTag::Insert => additions += 1,
                ChangeTag::Delete => deletions += 1,
                _ => {}
            }
        }
        Self {
            path,
            before,
            after,
            base: base.into(),
            additions: (base != "unknown").then_some(additions),
            deletions: (base != "unknown").then_some(deletions),
            revision: 0,
        }
    }
    fn changed(&self) -> bool {
        self.base == "unknown" || self.before != self.after
    }
    pub(super) fn summary(&self) -> FileSummary {
        FileSummary {
            path: self.path.clone(),
            additions: self.additions,
            deletions: self.deletions,
            base: self.base.clone(),
            revision: self.revision,
        }
    }
    fn detail(&self) -> FileDiff {
        let mut rows = vec![];
        let mut truncated = false;
        if self.base != "unknown" {
            let diff = TextDiff::configure()
                .timeout(Duration::from_millis(500))
                .diff_lines(
                    self.before.as_deref().unwrap_or(""),
                    self.after.as_deref().unwrap_or(""),
                );
            let mut previous_end = 0;
            let mut bytes = 0;
            'groups: for group in diff.grouped_ops(3) {
                if let Some(first) = group.first() {
                    let skipped = first.old_range().start.saturating_sub(previous_end);
                    if skipped > 0 {
                        rows.push(DiffRow {
                            kind: "gap",
                            old_line: None,
                            new_line: None,
                            text: format!("{skipped} linhas sem alteração"),
                        });
                    }
                }
                for op in group {
                    previous_end = op.old_range().end;
                    for change in diff.iter_changes(&op) {
                        bytes += change.value().len();
                        if rows.len() >= 4000 || bytes > 512 * 1024 {
                            truncated = true;
                            break 'groups;
                        }
                        rows.push(DiffRow {
                            kind: match change.tag() {
                                ChangeTag::Insert => "added",
                                ChangeTag::Delete => "removed",
                                ChangeTag::Equal => "context",
                            },
                            old_line: change.old_index().map(|value| value + 1),
                            new_line: change.new_index().map(|value| value + 1),
                            text: change.value().trim_end_matches('\n').to_owned(),
                        });
                    }
                }
            }
        }
        FileDiff {
            path: self.path.clone(),
            base: self.base.clone(),
            rows,
            truncated,
        }
    }
}

pub(super) fn summaries(data: &SessionData) -> Vec<FileSummary> {
    data.extras
        .files
        .values()
        .filter(|file| file.changed())
        .map(FileRevision::summary)
        .collect()
}

pub(super) async fn record(session: &Session, revision: FileRevision) -> Result<(), AgentError> {
    let previous = session
        .data
        .lock()
        .map_err(|_| AgentError::internal())?
        .extras
        .files
        .get(&revision.path)
        .cloned();
    let mut baseline = revision.before.clone();
    if let Some(previous) = previous.as_ref().filter(|file| file.base != "unknown") {
        // Rebase ownership after full/partial commits before recording another
        // edit. Preserve the journal baseline if Git is temporarily unavailable.
        baseline = previous.before.clone();
        if let Ok(repository) = working::Repository::open(&session.root).await {
            let head = if let Some(repo) = repository {
                repo.contents(&session.root, &revision.path).await
            } else {
                Ok(previous.before.clone())
            };
            if let Ok(head) = head {
                baseline =
                    working::pending(previous, head.as_deref(), revision.before.as_deref()).before;
            }
        }
    }
    let mut data = session.data.lock().map_err(|_| AgentError::internal())?;
    let version = data
        .extras
        .files
        .get(&revision.path)
        .map_or(1, |file| file.revision + 1);
    let mut revision = FileRevision::new(revision.path, baseline, revision.after, "conversation");
    revision.revision = version;
    session.checkpoint(&mut data, "file_checkpoint", &revision)?;
    data.extras.files.insert(revision.path.clone(), revision);
    Ok(())
}

// Older journals lack pre-write snapshots. When possible, explicitly compare
// their final recorded contents with HEAD, rather than inventing a before-state.
pub(super) fn load_legacy(
    root: &Path,
    turns: &[StoredTurn],
    files: &mut std::collections::BTreeMap<String, FileRevision>,
) {
    let mut contents: std::collections::BTreeMap<String, Option<String>> =
        std::collections::BTreeMap::new();
    for turn in turns {
        for step in &turn.turn.steps {
            for tool in &step.tools {
                if tool.status != "completed" || !matches!(tool.name.as_str(), "write" | "edit") {
                    continue;
                }
                let Some(path) = tool.args["path"].as_str() else {
                    continue;
                };
                let path = if Path::new(path).is_absolute() {
                    Path::new(path).to_path_buf()
                } else {
                    root.join(path)
                };
                let Ok(path) = path.canonicalize() else {
                    continue;
                };
                let Ok(relative) = path.strip_prefix(root) else {
                    continue;
                };
                let relative = relative.to_string_lossy().to_string();
                if files.contains_key(&relative) {
                    continue;
                }
                if tool.name == "write" {
                    contents.insert(relative, tool.args["content"].as_str().map(str::to_owned));
                } else if let Some(Some(content)) = contents.get_mut(&relative) {
                    if let (Some(old), Some(new)) =
                        (tool.args["oldText"].as_str(), tool.args["newText"].as_str())
                    {
                        if !old.is_empty() && content.match_indices(old).count() == 1 {
                            *content = content.replacen(old, new, 1);
                        }
                    }
                } else {
                    contents.insert(relative, None);
                }
            }
        }
    }
    if contents.is_empty() {
        return;
    }
    let git_root = crate::background::command("git")
        .arg("-C")
        .arg(root)
        .args(["rev-parse", "--show-toplevel"])
        .stderr(Stdio::null())
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|value| PathBuf::from(value.trim()));
    for (path, after) in contents {
        // Live disk content cannot establish ownership for an old edit whose
        // resulting contents were never recorded in the journal.
        let mut base = "unknown";
        let mut before = None;
        if let Some(git_root) = &git_root {
            if let Ok(relative) = root.join(&path).strip_prefix(git_root) {
                if let Ok(mut child) = crate::background::command("git")
                    .arg("--no-pager")
                    .arg("-C")
                    .arg(git_root)
                    .arg("show")
                    .arg(format!("HEAD:{}", relative.to_string_lossy()))
                    .stdout(Stdio::piped())
                    .stderr(Stdio::null())
                    .spawn()
                {
                    let mut bytes = vec![];
                    let read = child
                        .stdout
                        .take()
                        .map(|pipe| pipe.take(1024 * 1024 + 1).read_to_end(&mut bytes));
                    if bytes.len() > 1024 * 1024 {
                        let _ = child.kill();
                    }
                    let status = child.wait();
                    if matches!(read, Some(Ok(_))) && bytes.len() <= 1024 * 1024 {
                        if status.is_ok_and(|status| status.success()) {
                            if let Ok(text) = String::from_utf8(bytes) {
                                before = Some(text);
                                base = "git";
                            }
                        } else if crate::background::command("git")
                            .arg("-C")
                            .arg(git_root)
                            .args(["ls-tree", "-z", "--full-tree", "HEAD", "--"])
                            .arg(relative)
                            .stderr(Stdio::null())
                            .output()
                            .is_ok_and(|output| output.status.success() && output.stdout.is_empty())
                        {
                            base = "git";
                        }
                    }
                }
            }
        }
        if after.is_none() {
            base = "unknown";
        }
        files.insert(path.clone(), FileRevision::new(path, before, after, base));
    }
}

#[tauri::command]
pub async fn get_agent_file_changes(
    app: tauri::AppHandle,
    persistence: tauri::State<'_, AppState>,
    agent: tauri::State<'_, AgentState>,
    conversation_id: String,
) -> Result<Vec<FileSummary>, AgentError> {
    let home = app.path().home_dir().map_err(|_| AgentError::storage())?;
    let state = persistence.inner().clone();
    let agent = agent.inner().clone();
    let session = tauri::async_runtime::spawn_blocking(move || {
        agent.file_session(&state, &home, &conversation_id)
    })
    .await
    .map_err(|_| AgentError::internal())??;
    Ok(working::files(&session, None)
        .await?
        .iter()
        .map(FileRevision::summary)
        .collect())
}

#[tauri::command]
pub async fn get_agent_file_diff(
    app: tauri::AppHandle,
    persistence: tauri::State<'_, AppState>,
    agent: tauri::State<'_, AgentState>,
    conversation_id: String,
    path: String,
) -> Result<FileDiff, AgentError> {
    let home = app.path().home_dir().map_err(|_| AgentError::storage())?;
    let state = persistence.inner().clone();
    let agent = agent.inner().clone();
    let session = tauri::async_runtime::spawn_blocking(move || {
        agent.file_session(&state, &home, &conversation_id)
    })
    .await
    .map_err(|_| AgentError::internal())??;
    let file = working::files(&session, Some(&path))
        .await?
        .pop()
        .ok_or_else(|| {
            AgentError::new(
                "diff_missing",
                "A alteração selecionada não está disponível nesta conversa.",
            )
        })?;
    tauri::async_runtime::spawn_blocking(move || file.detail())
        .await
        .map_err(|_| AgentError::internal())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn tool_writes_capture_net_diff_and_keep_original_baseline_after_restart() {
        let fixture = crate::agent::tests::Fixture::new();
        let session = crate::agent::tests::session(&fixture);
        std::fs::write(fixture.root.join("a.txt"), "one\ntwo\n").unwrap();
        let tool = ToolCall {
            id: "write1".into(),
            name: "write".into(),
            args: json!({"path":"a.txt","content":"one\nthree\nfour\n"}),
            status: "running".into(),
            output: String::new(),
            duration_ms: 0,
        };
        let (_send, signal) = watch::channel(false);
        let (_, revision) =
            tools::execute_with_revision(&fixture.root, &tool, Mode::Build, signal.clone())
                .await
                .unwrap();
        record(&session, revision.unwrap()).await.unwrap();
        let summary = session.snapshot().unwrap().file_changes;
        assert_eq!(
            (summary[0].additions, summary[0].deletions),
            (Some(2), Some(1))
        );
        let (_, extras) = journal::load_all(&session.journal).unwrap();
        session.data.lock().unwrap().extras = extras;
        let restored = ToolCall {
            args: json!({"path":"a.txt","content":"one\ntwo\n"}),
            ..tool
        };
        let (_, revision) =
            tools::execute_with_revision(&fixture.root, &restored, Mode::Build, signal.clone())
                .await
                .unwrap();
        record(&session, revision.unwrap()).await.unwrap();
        assert!(session.snapshot().unwrap().file_changes.is_empty());
        assert!(
            tools::execute_with_revision(&fixture.root, &restored, Mode::Plan, signal)
                .await
                .is_err()
        );
    }

    #[test]
    fn huge_diffs_are_bounded_without_losing_total_counts() {
        let file = FileRevision::new(
            "large.txt".into(),
            None,
            Some("new line\n".repeat(6000)),
            "conversation",
        );
        assert_eq!(file.additions, Some(6000));
        let detail = file.detail();
        assert!(detail.truncated);
        assert_eq!(detail.rows.len(), 4000);
    }
    #[test]
    fn new_overwritten_and_reverted_files_have_real_line_counts() {
        let created =
            FileRevision::new("a".into(), None, Some("one\ntwo\n".into()), "conversation");
        assert_eq!((created.additions, created.deletions), (Some(2), Some(0)));
        let overwritten = FileRevision::new(
            "a".into(),
            Some("one\ntwo\n".into()),
            Some("one\nthree\n".into()),
            "conversation",
        );
        assert_eq!(
            (overwritten.additions, overwritten.deletions),
            (Some(1), Some(1))
        );
        assert!(overwritten
            .detail()
            .rows
            .iter()
            .any(|row| row.kind == "removed" && row.old_line == Some(2)));
        assert!(!FileRevision::new(
            "a".into(),
            Some("same".into()),
            Some("same".into()),
            "conversation"
        )
        .changed());
    }
}
