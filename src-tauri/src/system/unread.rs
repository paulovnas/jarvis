//! One durable unread cursor per conversation. Reading an old cursor cannot
//! acknowledge a newer event, and a repeated event cannot resurrect read state.
use crate::persistence::{AppState, PersistenceError};
use rusqlite::{params, Connection};
use serde::Serialize;
use std::sync::atomic::{AtomicU64, Ordering};
use tauri::{Emitter, Listener, Manager};

#[derive(Default)]
pub(super) struct UnreadState {
    edit: tokio::sync::Mutex<()>,
    revision: AtomicU64,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    conversation_id: String,
    event_key: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    revision: u64,
    conversations: Vec<Entry>,
}

fn entries(db: &Connection) -> Result<Vec<Entry>, PersistenceError> {
    Ok(db.prepare("SELECT conversation_id, event_key FROM conversation_unread WHERE unread = 1 ORDER BY conversation_id")?
        .query_map([], |row| Ok(Entry { conversation_id: row.get(0)?, event_key: row.get(1)? }))?
        .collect::<Result<_, _>>()?)
}

fn record(db: &Connection, id: &str, key: &str) -> Result<bool, PersistenceError> {
    Ok(db.execute(
        "INSERT INTO conversation_unread(conversation_id, event_key, unread)
        SELECT id, ?2, 1 FROM conversations WHERE id = ?1
        ON CONFLICT(conversation_id) DO UPDATE SET event_key = excluded.event_key, unread = 1
        WHERE conversation_unread.event_key <> excluded.event_key",
        params![id, key],
    )? > 0)
}

fn acknowledge(db: &Connection, id: &str, key: &str) -> Result<(), PersistenceError> {
    db.execute(
        "UPDATE conversation_unread SET unread = 0 WHERE conversation_id = ?1 AND event_key = ?2
        AND conversation_id = (SELECT conversation_id FROM navigation_selection WHERE id = 1)",
        params![id, key],
    )?;
    Ok(())
}

fn focused(app: &tauri::AppHandle) -> bool {
    app.get_window("main").is_some_and(|window| {
        window.is_focused().unwrap_or(false)
            && window.is_visible().unwrap_or(false)
            && !window.is_minimized().unwrap_or(true)
    })
}

async fn update(
    app: &tauri::AppHandle,
    operation: impl FnOnce(&Connection) -> Result<(), PersistenceError> + Send + 'static,
) -> Result<Snapshot, String> {
    let state = app.state::<super::SystemState>();
    let _edit = state.unread.edit.lock().await;
    let home = app
        .path()
        .home_dir()
        .map_err(|_| "Não foi possível localizar as conversas.")?;
    let db = app.state::<AppState>().inner().clone();
    let conversations = tauri::async_runtime::spawn_blocking(move || {
        db.with_connection(&home, |db| {
            operation(db)?;
            entries(db)
        })
    })
    .await
    .map_err(|_| "Não foi possível consultar as mensagens não lidas.")?
    .map_err(|_| "Não foi possível salvar o estado de leitura das conversas.")?;
    let snapshot = Snapshot {
        revision: state.unread.revision.fetch_add(1, Ordering::SeqCst) + 1,
        conversations,
    };
    // OS notification preferences affect the Dock, not the in-app unread marks.
    let count = if state.preferences().is_ok_and(|p| p.notifications) {
        snapshot.conversations.len() as i64
    } else {
        0
    };
    if let Some(window) = app.get_window("main") {
        #[cfg(not(target_os = "windows"))]
        if let Err(error) = window.set_badge_count((count > 0).then_some(count)) {
            eprintln!("Unable to update Jarvis badge: {error}");
        }
        #[cfg(target_os = "windows")]
        let _ = (window, count); // Windows needs a taskbar overlay, not a badge count.
    }
    let _ = app.emit("unread:changed", &snapshot);
    Ok(snapshot)
}

pub(super) async fn notify(app: &tauri::AppHandle, id: String, key: String) -> Result<(), String> {
    update(app, move |db| {
        record(db, &id, &key)?;
        Ok(())
    })
    .await
    .map(|_| ())
}

pub(crate) async fn refresh(app: &tauri::AppHandle) -> Result<Snapshot, String> {
    update(app, |_| Ok(())).await
}

pub(super) fn setup(app: &tauri::AppHandle) {
    let handle = app.clone();
    app.listen("library:changed", move |_| {
        let app = handle.clone();
        tauri::async_runtime::spawn(async move {
            let _ = refresh(&app).await;
        });
    });
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let _ = refresh(&app).await;
    });
}

#[tauri::command]
pub async fn get_unread_conversations(app: tauri::AppHandle) -> Result<Snapshot, String> {
    refresh(&app).await
}

#[tauri::command]
pub async fn mark_conversation_read(
    app: tauri::AppHandle,
    conversation_id: String,
    event_key: String,
) -> Result<Snapshot, String> {
    // Query the native window before taking the database lock: a synchronous
    // window query must never wait on the UI thread while holding SQLite.
    let visible = focused(&app);
    update(&app, move |db| {
        if visible {
            acknowledge(db, &conversation_id, &event_key)?;
        }
        Ok(())
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seed(db: &mut Connection) {
        crate::persistence::initialize_database(db).unwrap();
        db.execute_batch("INSERT INTO workspaces(id, name) VALUES ('workspace', 'Pessoal');
            INSERT INTO projects(id, workspace_id, name, path) VALUES ('p1', 'workspace', 'One', '/one'), ('p2', 'workspace', 'Two', '/two');
            INSERT INTO conversations(id, project_id, title) VALUES ('a', 'p1', 'First'), ('b', 'p2', 'Second');
            INSERT INTO navigation_selection(id, workspace_id, project_id, conversation_id) VALUES (1, 'workspace', 'p1', 'a')
            ON CONFLICT(id) DO UPDATE SET workspace_id='workspace', project_id='p1', conversation_id='a';").unwrap();
    }

    #[test]
    fn duplicate_notices_count_once_and_do_not_resurrect_a_read_conversation() {
        let mut db = Connection::open_in_memory().unwrap();
        seed(&mut db);
        assert!(record(&db, "a", "completed").unwrap());
        assert!(!record(&db, "a", "completed").unwrap());
        assert_eq!(entries(&db).unwrap().len(), 1);
        acknowledge(&db, "a", "completed").unwrap();
        assert!(entries(&db).unwrap().is_empty());
        assert!(!record(&db, "a", "completed").unwrap());
        assert!(entries(&db).unwrap().is_empty());
        assert!(record(&db, "a", "question").unwrap());
        assert_eq!(entries(&db).unwrap().len(), 1);
    }

    #[test]
    fn stale_reads_cannot_clear_newer_notices_or_another_selected_conversation() {
        let mut db = Connection::open_in_memory().unwrap();
        seed(&mut db);
        record(&db, "a", "old").unwrap();
        record(&db, "b", "other").unwrap();
        record(&db, "a", "new").unwrap();
        acknowledge(&db, "a", "old").unwrap();
        acknowledge(&db, "b", "other").unwrap();
        assert_eq!(entries(&db).unwrap().len(), 2);
        acknowledge(&db, "a", "new").unwrap();
        assert_eq!(
            entries(&db).unwrap(),
            vec![Entry {
                conversation_id: "b".into(),
                event_key: "other".into()
            }]
        );
        db.execute(
            "UPDATE navigation_selection SET project_id = 'p2', conversation_id = 'b' WHERE id = 1",
            [],
        )
        .unwrap();
        acknowledge(&db, "b", "other").unwrap();
        assert!(entries(&db).unwrap().is_empty());
    }

    #[test]
    fn unread_and_read_cursors_survive_restart_and_deletion_cascades() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("db.sqlite");
        {
            let mut db = Connection::open(&path).unwrap();
            seed(&mut db);
            record(&db, "a", "completed").unwrap();
            record(&db, "b", "failed").unwrap();
            acknowledge(&db, "a", "completed").unwrap();
        }
        let mut db = Connection::open(path).unwrap();
        crate::persistence::initialize_database(&mut db).unwrap();
        assert!(!record(&db, "a", "completed").unwrap());
        assert_eq!(entries(&db).unwrap().len(), 1);
        db.execute_batch("DELETE FROM conversations WHERE project_id = 'p2'; DELETE FROM projects WHERE id = 'p2';").unwrap();
        assert!(entries(&db).unwrap().is_empty());
        assert!(!record(&db, "b", "late-delivery").unwrap());
        assert!(!record(&db, "unknown", "notice").unwrap());
    }

    #[test]
    fn keeps_only_one_cursor_per_conversation_as_history_grows() {
        let mut db = Connection::open_in_memory().unwrap();
        seed(&mut db);
        for index in 0..100 {
            record(&db, "a", &format!("turn-{index}")).unwrap();
        }
        assert_eq!(
            entries(&db).unwrap(),
            vec![Entry {
                conversation_id: "a".into(),
                event_key: "turn-99".into()
            }]
        );
        assert_eq!(
            db.query_row("SELECT count(*) FROM conversation_unread", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            1
        );
    }
}
