use super::*;
use library::{
    cleanup::{self, Candidate},
    LibraryError,
};
use std::{collections::HashSet, path::Path};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Preview {
    days: u32,
    conversations: Vec<Candidate>,
    bytes: u64,
    protected: usize,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Selection {
    id: String,
    activity: i64,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Cleaned {
    deleted: usize,
    skipped: usize,
    failed: usize,
    bytes: u64,
}

fn failure() -> LibraryError {
    LibraryError::new(
        "cleanup_failed",
        "Não foi possível conferir os históricos para limpeza.",
    )
}
fn busy(data: &SessionData) -> bool {
    data.active.is_some()
        || data.compacting
        || data.manual_compaction
        || !data.extras.queue.is_empty()
}
fn seconds() -> i64 {
    (now() / 1000) as i64
}

impl AgentState {
    fn cleanup_preview(
        &self,
        state: &AppState,
        home: &Path,
        days: u32,
    ) -> Result<Preview, LibraryError> {
        let sessions = self.sessions.lock().map_err(|_| failure())?;
        state.with_connection(home, |connection| {
            let mut preview = Preview {
                days,
                conversations: vec![],
                bytes: 0,
                protected: 0,
            };
            for mut item in cleanup::candidates(connection, days, seconds())? {
                let active = sessions
                    .get(&item.id)
                    .is_some_and(|session| session.data.lock().map_or(true, |data| busy(&data)));
                let path = cleanup::journal_path(home, &item);
                let queued = path
                    .as_ref()
                    .ok()
                    .is_none_or(|path| self.histories.has_queue(path).unwrap_or(true));
                if active || queued {
                    preview.protected += 1;
                    continue;
                }
                match cleanup::size(home, &item) {
                    Ok(bytes) => {
                        item.bytes = bytes;
                        preview.bytes += bytes;
                        preview.conversations.push(item);
                    }
                    Err(_) => preview.protected += 1,
                }
            }
            Ok(preview)
        })
    }

    fn cleanup_confirmed(
        &self,
        state: &AppState,
        home: &Path,
        days: u32,
        selection: Vec<Selection>,
        confirmed: bool,
    ) -> Result<Cleaned, LibraryError> {
        if !confirmed {
            return Err(LibraryError::new(
                "confirmation_required",
                "Confirme a exclusão definitiva antes de continuar.",
            ));
        }
        let ids: HashSet<_> = selection.iter().map(|item| item.id.as_str()).collect();
        if selection.len() > 500 || ids.len() != selection.len() {
            return Err(failure());
        }
        let mut sessions = self.sessions.lock().map_err(|_| failure())?;
        let targets: Vec<_> = selection
            .iter()
            .filter_map(|item| sessions.get(&item.id).cloned())
            .collect();
        let mut locked = targets
            .iter()
            .map(|session| session.data.lock().map_err(|_| failure()))
            .collect::<Result<Vec<_>, _>>()?;
        state.with_connection(home, |connection| {
            let mut result = Cleaned {
                deleted: 0,
                skipped: 0,
                failed: 0,
                bytes: 0,
            };
            for selected in selection {
                // Never extend confirmation to another ID, a newly active session,
                // or a session that has become its project's last remaining one.
                let item = cleanup::candidates(connection, days, seconds())?
                    .into_iter()
                    .find(|item| item.id == selected.id && item.activity == selected.activity);
                let Some(item) = item else {
                    result.skipped += 1;
                    continue;
                };
                let cached = targets.iter().position(|session| session.id == item.id);
                if cached.is_some_and(|index| busy(&locked[index])) {
                    result.skipped += 1;
                    continue;
                }
                let path = match cleanup::journal_path(home, &item) {
                    Ok(path) => path,
                    Err(_) => {
                        result.failed += 1;
                        continue;
                    }
                };
                if self.histories.has_queue(&path).unwrap_or(true) {
                    result.skipped += 1;
                    continue;
                }
                let bytes = match cleanup::size(home, &item) {
                    Ok(bytes) => bytes,
                    Err(_) => {
                        result.failed += 1;
                        continue;
                    }
                };
                let deletion = library::deletion::delete(
                    connection,
                    home,
                    &library::deletion::DeleteTarget::Conversation(item.id.clone()),
                );
                let exists: bool = connection.query_row(
                    "SELECT EXISTS(SELECT 1 FROM conversations WHERE id = ?1)",
                    [&item.id],
                    |row| row.get(0),
                )?;
                if !exists {
                    self.processes.stop_conversation(&item.id);
                    if let Some(index) = cached {
                        locked[index].storage_failed = true;
                    }
                    sessions.remove(&item.id);
                    self.histories.forget(&path);
                    result.deleted += 1;
                    if deletion.is_ok() {
                        result.bytes += bytes;
                    }
                }
                if deletion.is_err() {
                    result.failed += 1;
                }
            }
            Ok(result)
        })
    }
}

#[tauri::command]
pub async fn preview_chat_cleanup(
    app: tauri::AppHandle,
    persistence: tauri::State<'_, AppState>,
    agent: tauri::State<'_, AgentState>,
    days: u32,
) -> Result<Preview, LibraryError> {
    let home = app.path().home_dir().map_err(|_| failure())?;
    let state = persistence.inner().clone();
    let agent = agent.inner().clone();
    tauri::async_runtime::spawn_blocking(move || agent.cleanup_preview(&state, &home, days))
        .await
        .map_err(|_| failure())?
}

#[tauri::command]
pub async fn cleanup_old_chats(
    app: tauri::AppHandle,
    persistence: tauri::State<'_, AppState>,
    agent: tauri::State<'_, AgentState>,
    days: u32,
    selection: Vec<Selection>,
    confirmed: bool,
) -> Result<Cleaned, LibraryError> {
    let home = app.path().home_dir().map_err(|_| failure())?;
    let state = persistence.inner().clone();
    let agent = agent.inner().clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        agent.cleanup_confirmed(&state, &home, days, selection, confirmed)
    })
    .await
    .map_err(|_| failure())?;
    let _ = app.emit("library:changed", ());
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::tests::Fixture;
    use std::fs;

    fn setup(fixture: &Fixture) -> (AppState, String, Vec<String>) {
        let state = AppState::default();
        let project = library::new_id().unwrap();
        let ids: Vec<_> = (0..4).map(|_| library::new_id().unwrap()).collect();
        state.with_connection(&fixture.root, |connection| {
            connection.execute("INSERT INTO workspaces (id, name) VALUES ('workspace', 'Test')", [])?;
            connection.execute("INSERT INTO projects (id, workspace_id, name, path) VALUES (?1, 'workspace', 'Project', ?2)", rusqlite::params![project, fixture.root.to_string_lossy()])?;
            for id in &ids { connection.execute("INSERT INTO conversations (id, project_id, title, created_at, last_activity_at) VALUES (?1, ?2, 'Old', ?3, ?3)", rusqlite::params![id, project, seconds() - 20 * 86_400])?; }
            Ok::<_, LibraryError>(())
        }).unwrap();
        let root = fixture.root.join(".jarvis/sessions").join(&project);
        fs::create_dir_all(&root).unwrap();
        for id in &ids {
            fs::write(root.join(format!("{id}.jsonl")), "{}\n").unwrap();
        }
        (state, project, ids)
    }
    fn selection(preview: &Preview) -> Vec<Selection> {
        preview
            .conversations
            .iter()
            .map(|item| Selection {
                id: item.id.clone(),
                activity: item.activity,
            })
            .collect()
    }

    #[test]
    fn preview_is_read_only_and_confirmation_retains_latest_in_every_project() {
        let fixture = Fixture::new();
        let (state, project, ids) = setup(&fixture);
        let agent = AgentState::default();
        let other = library::new_id().unwrap();
        let only = library::new_id().unwrap();
        state.with_connection(&fixture.root, |connection| {
            connection.execute("INSERT INTO projects (id, workspace_id, name, path) VALUES (?1, 'workspace', 'Other', ?2)", rusqlite::params![other, fixture.root.join("other").to_string_lossy()])?;
            connection.execute("INSERT INTO conversations (id, project_id, title, created_at) VALUES (?1, ?2, 'Only', 1)", rusqlite::params![only, other])?;
            Ok::<_, LibraryError>(())
        }).unwrap();
        let source = fixture.root.join("source.txt");
        fs::write(&source, "keep source").unwrap();
        let context = crate::core::context::storage(&fixture.root, &ids[0]);
        fs::create_dir_all(&context).unwrap();
        fs::write(context.join("memory.db"), "private memory").unwrap();
        let preview = agent.cleanup_preview(&state, &fixture.root, 7).unwrap();
        assert_eq!(preview.conversations.len(), 3);
        assert!(!preview
            .conversations
            .iter()
            .any(|item| item.id == ids[3] || item.id == only));
        assert!(context.exists());
        assert!(agent
            .cleanup_confirmed(&state, &fixture.root, 7, selection(&preview), false)
            .is_err());
        let cleaned = agent
            .cleanup_confirmed(&state, &fixture.root, 7, selection(&preview), true)
            .unwrap();
        assert_eq!(
            (cleaned.deleted, cleaned.failed, cleaned.skipped),
            (3, 0, 0)
        );
        assert_eq!(cleaned.bytes, preview.bytes);
        assert_eq!(fs::read_to_string(source).unwrap(), "keep source");
        assert!(!context.exists());
        assert!(fixture
            .root
            .join(".jarvis/sessions")
            .join(project)
            .join(format!("{}.jsonl", ids[3]))
            .exists());
        assert!(agent
            .cleanup_preview(&state, &fixture.root, 7)
            .unwrap()
            .conversations
            .is_empty());
    }

    #[test]
    fn confirmation_rechecks_activity_queue_and_active_runtime_and_never_extends_preview() {
        let fixture = Fixture::new();
        let (state, project, ids) = setup(&fixture);
        let agent = AgentState::default();
        let preview = agent.cleanup_preview(&state, &fixture.root, 7).unwrap();
        library::dashboard::touch_activity(&state, &fixture.root, &ids[0]).unwrap();
        let queued = queue::QueuedMessage {
            id: "queued".into(),
            content: "later".into(),
            options: TurnOptions {
                account: "test".into(),
                model: "test".into(),
                reasoning: None,
                mode: Mode::Build,
                workflow: None,
                approval_mode: ApprovalMode::Manual,
            },
            parts: vec![],
        };
        let path = fixture
            .root
            .join(".jarvis/sessions")
            .join(&project)
            .join(format!("{}.jsonl", ids[1]));
        journal::append_event(&path, "queue_checkpoint", &vec![queued]).unwrap();
        let mut session = crate::agent::tests::session(&fixture);
        Arc::get_mut(&mut session).unwrap().id = ids[2].clone();
        session.data.lock().unwrap().manual_compaction = true;
        agent
            .sessions
            .lock()
            .unwrap()
            .insert(ids[2].clone(), session);
        let cleaned = agent
            .cleanup_confirmed(&state, &fixture.root, 7, selection(&preview), true)
            .unwrap();
        assert_eq!((cleaned.deleted, cleaned.skipped), (0, 3));
        let next = agent.cleanup_preview(&state, &fixture.root, 7).unwrap();
        assert_eq!(next.protected, 2);
        assert_eq!(next.conversations.len(), 1);
        assert_eq!(next.conversations[0].id, ids[3]);
    }
}
