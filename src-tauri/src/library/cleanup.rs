use super::*;

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Candidate {
    pub id: String,
    pub project_id: String,
    pub title: String,
    pub project_name: String,
    pub activity: i64,
    pub bytes: u64,
}

pub(crate) fn candidates(
    connection: &Connection,
    days: u32,
    now: i64,
) -> Result<Vec<Candidate>, LibraryError> {
    if ![7, 14, 30, 90].contains(&days) {
        return Err(LibraryError::new(
            "invalid_period",
            "Selecione um período de limpeza válido.",
        ));
    }
    let cutoff = now - i64::from(days) * 86_400;
    let sql = "WITH ranked AS (
        SELECT c.id, c.project_id, COALESCE(c.display_title, c.title) AS title,
        COALESCE(c.last_activity_at, c.created_at) AS activity,
        ROW_NUMBER() OVER (PARTITION BY c.project_id ORDER BY COALESCE(c.last_activity_at, c.created_at) DESC, c.rowid DESC) AS position
        FROM conversations c)
        SELECT r.id, r.project_id, r.title, p.name, r.activity FROM ranked r JOIN projects p ON p.id = r.project_id
        WHERE r.position > 1 AND r.activity < ?1 ORDER BY r.activity, r.id LIMIT 500";
    Ok(connection
        .prepare(sql)?
        .query_map([cutoff], |row| {
            Ok(Candidate {
                id: row.get(0)?,
                project_id: row.get(1)?,
                title: row.get(2)?,
                project_name: row.get(3)?,
                activity: row.get(4)?,
                bytes: 0,
            })
        })?
        .collect::<Result<_, _>>()?)
}

fn directory_bytes(path: &Path) -> Result<u64, LibraryError> {
    let mut total = 0;
    let mut pending = vec![path.to_owned()];
    while let Some(path) = pending.pop() {
        let meta = match fs::symlink_metadata(&path) {
            Ok(meta) => meta,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(_) => return Err(LibraryError::storage()),
        };
        if meta.is_symlink() {
            continue;
        }
        if meta.is_file() {
            total += meta.len();
        } else if meta.is_dir() {
            for entry in fs::read_dir(path).map_err(|_| LibraryError::storage())? {
                pending.push(entry.map_err(|_| LibraryError::storage())?.path());
            }
        }
    }
    Ok(total)
}

pub(crate) fn journal_path(home: &Path, item: &Candidate) -> Result<PathBuf, LibraryError> {
    session_path(home, &item.project_id, &item.id, false)
}

pub(crate) fn size(home: &Path, item: &Candidate) -> Result<u64, LibraryError> {
    let files = deletion::files_to_delete(home, &item.project_id, Some(&item.id))?;
    let mut bytes = related_size(home, &item.id)?;
    for file in files {
        bytes += fs::symlink_metadata(file)
            .map_err(|_| LibraryError::storage())?
            .len();
    }
    Ok(bytes)
}

pub(super) fn related_size(home: &Path, id: &str) -> Result<u64, LibraryError> {
    let attachment_root = home.join(".jarvis/attachments");
    if fs::symlink_metadata(&attachment_root).is_ok_and(|meta| meta.is_symlink()) {
        return Err(LibraryError::storage());
    }
    let mut bytes = directory_bytes(&attachment_root.join(id))?;
    let context_root = home.join(".jarvis/context-mode");
    if fs::symlink_metadata(&context_root).is_ok_and(|meta| meta.is_symlink()) {
        return Err(LibraryError::storage());
    }
    let workflow = home.join(".jarvis/workflows").join(id);
    if fs::symlink_metadata(home.join(".jarvis/workflows")).is_ok_and(|meta| meta.is_symlink()) {
        return Err(LibraryError::storage());
    }
    bytes += directory_bytes(&workflow)?;
    if workflow.is_dir()
        && !fs::symlink_metadata(&workflow)
            .map_err(|_| LibraryError::storage())?
            .is_symlink()
    {
        for entry in fs::read_dir(&workflow).map_err(|_| LibraryError::storage())? {
            let entry = entry.map_err(|_| LibraryError::storage())?;
            if let Some(id) = entry
                .file_name()
                .to_str()
                .and_then(|name| name.strip_suffix(".jsonl"))
                .filter(|id| valid_id(id))
            {
                bytes += directory_bytes(&crate::core::context::storage(home, id))?;
            }
        }
    }
    Ok(bytes + directory_bytes(&crate::core::context::storage(home, id))?)
}
