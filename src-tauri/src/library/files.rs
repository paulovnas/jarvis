//! Read-only, project-scoped Explorer access. Never recursively enumerate a project.
use super::LibraryError;
use crate::persistence::AppState;
use serde::Serialize;
use std::{
    fs,
    io::Read,
    path::{Component, Path, PathBuf},
};
use tauri::{AppHandle, State};

const MAX_ENTRIES: usize = 4_000;
const MAX_FILE_BYTES: u64 = 2 * 1024 * 1024;

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum EntryKind {
    Directory,
    File,
    Link,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileEntry {
    name: String,
    path: String,
    kind: EntryKind,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DirectoryListing {
    path: String,
    entries: Vec<FileEntry>,
    truncated: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FilePreview {
    path: String,
    content: String,
    size: u64,
    encoding: &'static str,
}

fn unavailable() -> LibraryError {
    LibraryError::new(
        "project_file",
        "O arquivo ou a pasta não está disponível dentro deste projeto.",
    )
}

fn relative_path(value: &str, root_allowed: bool) -> Result<String, LibraryError> {
    let normalized = value.replace('\\', "/");
    if root_allowed && normalized.is_empty() {
        return Ok(normalized);
    }
    if normalized.is_empty()
        || normalized.contains([':', '\0'])
        || !Path::new(&normalized)
            .components()
            .all(|part| matches!(part, Component::Normal(_)))
    {
        return Err(unavailable());
    }
    Ok(normalized)
}

fn resolve(root: &Path, relative: &str) -> Result<PathBuf, LibraryError> {
    let path = root
        .join(relative)
        .canonicalize()
        .map_err(|_| unavailable())?;
    if !path.starts_with(root) {
        return Err(unavailable());
    }
    Ok(path)
}

fn directory(root: &Path, relative: &str) -> Result<DirectoryListing, LibraryError> {
    let relative = relative_path(relative, true)?;
    let path = resolve(root, &relative)?;
    let items = fs::read_dir(path).map_err(|_| unavailable())?;
    let mut entries = Vec::new();
    let mut truncated = false;
    for entry in items {
        let entry = entry.map_err(|_| unavailable())?;
        if entries.len() == MAX_ENTRIES {
            truncated = true;
            break;
        }
        // Non-Unicode filenames cannot round-trip through the JSON/Monaco boundary.
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        let file_type = entry.file_type().map_err(|_| unavailable())?;
        let kind = if file_type.is_symlink() {
            EntryKind::Link
        } else if file_type.is_dir() {
            EntryKind::Directory
        } else if file_type.is_file() {
            EntryKind::File
        } else {
            continue;
        };
        let path = if relative.is_empty() {
            name.clone()
        } else {
            format!("{relative}/{name}")
        };
        entries.push(FileEntry { name, path, kind });
    }
    entries.sort_by(|a, b| {
        (a.kind != EntryKind::Directory)
            .cmp(&(b.kind != EntryKind::Directory))
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            .then_with(|| a.name.cmp(&b.name))
    });
    Ok(DirectoryListing {
        path: relative,
        entries,
        truncated,
    })
}

fn decode(bytes: &[u8]) -> Result<(String, &'static str), LibraryError> {
    let unsupported = || {
        LibraryError::new("file_encoding", "Este arquivo é binário ou usa uma codificação não suportada. A visualização aceita UTF-8 e UTF-16.")
    };
    let (content, encoding) =
        if bytes.starts_with(&[0xff, 0xfe]) || bytes.starts_with(&[0xfe, 0xff]) {
            if !bytes.len().is_multiple_of(2) {
                return Err(unsupported());
            }
            let little = bytes[0] == 0xff;
            let words: Vec<u16> = bytes[2..]
                .as_chunks::<2>()
                .0
                .iter()
                .map(|pair| {
                    if little {
                        u16::from_le_bytes(*pair)
                    } else {
                        u16::from_be_bytes(*pair)
                    }
                })
                .collect();
            (
                String::from_utf16(&words).map_err(|_| unsupported())?,
                if little { "UTF-16 LE" } else { "UTF-16 BE" },
            )
        } else {
            let bytes = bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(bytes);
            (
                std::str::from_utf8(bytes)
                    .map_err(|_| unsupported())?
                    .to_owned(),
                "UTF-8",
            )
        };
    if content.contains('\0') {
        return Err(unsupported());
    }
    Ok((content, encoding))
}

fn preview(root: &Path, relative: &str) -> Result<FilePreview, LibraryError> {
    let relative = relative_path(relative, false)?;
    let path = resolve(root, &relative)?;
    let file = fs::File::open(&path).map_err(|_| unavailable())?;
    let metadata = file.metadata().map_err(|_| unavailable())?;
    if !metadata.is_file() {
        return Err(unavailable());
    }
    let too_large = || {
        LibraryError::new(
            "file_too_large",
            "Este arquivo ultrapassa o limite de 2 MB para visualização.",
        )
    };
    if metadata.len() > MAX_FILE_BYTES {
        return Err(too_large());
    }
    let mut bytes = Vec::new();
    file.take(MAX_FILE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| unavailable())?;
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err(too_large());
    }
    let (content, encoding) = decode(&bytes)?;
    Ok(FilePreview {
        path: relative,
        content,
        size: bytes.len() as u64,
        encoding,
    })
}

async fn root(
    app: AppHandle,
    state: AppState,
    project_id: String,
) -> Result<PathBuf, LibraryError> {
    super::run(app, state, move |connection, _| {
        let root = super::project_opener_path(connection, &project_id)?;
        super::canonical_directory(Path::new(&root))
    })
    .await
}

#[tauri::command]
pub async fn list_project_directory(
    app: AppHandle,
    state: State<'_, AppState>,
    project_id: String,
    path: String,
) -> Result<DirectoryListing, LibraryError> {
    let root = root(app, state.inner().clone(), project_id).await?;
    // Filesystem work runs outside the database lock and outside the GUI thread.
    tauri::async_runtime::spawn_blocking(move || directory(&root, &path))
        .await
        .map_err(|_| unavailable())?
}

#[tauri::command]
pub async fn read_project_file(
    app: AppHandle,
    state: State<'_, AppState>,
    project_id: String,
    path: String,
) -> Result<FilePreview, LibraryError> {
    let root = root(app, state.inner().clone(), project_id).await?;
    tauri::async_runtime::spawn_blocking(move || preview(&root, &path))
        .await
        .map_err(|_| unavailable())?
}

#[cfg(test)]
mod tests;
