use super::*;

pub(super) fn backfill_activity(connection: &Connection, home: &Path) -> Result<(), LibraryError> {
    let rows = connection
        .prepare(
            "SELECT id, project_id, created_at FROM conversations WHERE last_activity_at IS NULL",
        )?
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    for (id, project, created) in rows {
        // Existing sessions predate activity tracking; use their durable journal's
        // last write once, without parsing or repairing history during startup.
        let activity = session_path(home, &project, &id, false)
            .ok()
            .and_then(|path| fs::symlink_metadata(path).ok())
            .filter(|meta| meta.is_file() && !meta.is_symlink())
            .and_then(|meta| meta.modified().ok())
            .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
            .map_or(created, |time| (time.as_secs() as i64).max(created));
        connection.execute("UPDATE conversations SET last_activity_at = ?2 WHERE id = ?1 AND last_activity_at IS NULL", params![id, activity])?;
    }
    Ok(())
}

pub(crate) fn touch_activity(state: &AppState, home: &Path, id: &str) -> Result<(), LibraryError> {
    state.with_connection(home, |connection| {
        connection.execute("UPDATE conversations SET last_activity_at = MAX(COALESCE(last_activity_at, created_at), unixepoch()) WHERE id = ?1", [id])?;
        Ok(())
    })
}

pub(crate) fn check_project(state: &AppState, home: &Path, id: &str) -> Result<(), LibraryError> {
    state.with_connection(home, |connection| {
        project(connection, id)?;
        Ok(())
    })
}

pub(crate) struct SessionSource {
    pub id: String,
    pub title: String,
    pub activity: i64,
    pub journal: Option<PathBuf>,
}

pub(crate) fn sources(
    state: &AppState,
    home: &Path,
    project_id: &str,
) -> Result<Vec<SessionSource>, LibraryError> {
    state.with_connection(home, |connection| {
        project(connection, project_id)?;
        let mut query = connection.prepare("SELECT id, project_id, COALESCE(display_title, title), created_at, title, COALESCE(last_activity_at, created_at) FROM conversations WHERE project_id = ?1 ORDER BY COALESCE(last_activity_at, created_at) DESC, rowid DESC")?;
        let items = query.query_map([project_id], conversation_row)?.collect::<Result<Vec<_>, _>>()?;
        Ok(items.into_iter().map(|item| {
            // Verify identity and header, without requiring the source checkout to
            // be mounted: a Dashboard remains useful for an offline project.
            let journal = read_conversation(connection, home, &item.id).ok()
                .and_then(|_| session_path(home, project_id, &item.id, false).ok());
            SessionSource { id: item.id, title: item.title, activity: item.last_activity_at, journal }
        }).collect())
    })
}
