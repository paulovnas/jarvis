//! Read-only, project-scoped Explorer access. Never recursively enumerate a project.
use super::LibraryError;
use crate::persistence::AppState;
use serde::Serialize;
use std::{
    fs,
    io::Read,
    path::{Component, Path, PathBuf},
};
use tauri::{AppHandle, Manager, State, WebviewWindow};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_opener::OpenerExt;

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

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoPreview {
    path: String,
    absolute_path: String,
    size: u64,
    mime: &'static str,
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

fn video(root: &Path, relative: &str) -> Result<VideoPreview, LibraryError> {
    let relative = relative_path(relative, false)?;
    let path = resolve(root, &relative)?;
    let metadata = fs::metadata(&path).map_err(|_| unavailable())?;
    if !metadata.is_file() || metadata.len() == 0 {
        return Err(unavailable());
    }
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let mime = match extension.as_str() {
        "mp4" | "m4v" => "video/mp4",
        "webm" => "video/webm",
        "mov" => "video/quicktime",
        "ogv" => "video/ogg",
        "wav" => "audio/wav",
        "mp3" => "audio/mpeg",
        "ogg" => "audio/ogg",
        _ => return Err(LibraryError::new(
            "video_format",
            "Este formato de mídia não é suportado. Use MP4, WebM, MOV, M4V, OGV, WAV, MP3 ou OGG.",
        )),
    };
    Ok(VideoPreview {
        path: relative,
        absolute_path: path.to_str().ok_or_else(unavailable)?.to_owned(),
        size: metadata.len(),
        mime,
    })
}

fn save_video(source: &Path, destination: &Path) -> Result<(), LibraryError> {
    let failed = || LibraryError::new("save_video", "Não foi possível salvar o arquivo de mídia.");
    if destination.canonicalize().ok().as_deref() == Some(source) {
        return Ok(());
    }
    let parent = destination.parent().ok_or_else(failed)?;
    let temporary = tempfile::NamedTempFile::new_in(parent).map_err(|_| failed())?;
    fs::copy(source, temporary.path()).map_err(|_| failed())?;
    temporary.as_file().sync_all().map_err(|_| failed())?;
    temporary.persist(destination).map_err(|_| failed())?;
    Ok(())
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

#[tauri::command]
pub async fn get_project_video(
    app: AppHandle,
    window: WebviewWindow,
    state: State<'_, AppState>,
    project_id: String,
    path: String,
) -> Result<VideoPreview, LibraryError> {
    if window.label() != "main" {
        return Err(unavailable());
    }
    let root = root(app.clone(), state.inner().clone(), project_id).await?;
    tauri::async_runtime::spawn_blocking(move || {
        let preview = video(&root, &path)?;
        app.asset_protocol_scope()
            .allow_file(&preview.absolute_path)
            .map_err(|_| unavailable())?;
        Ok(preview)
    })
    .await
    .map_err(|_| unavailable())?
}

#[tauri::command]
pub async fn save_project_video(
    app: AppHandle,
    window: WebviewWindow,
    state: State<'_, AppState>,
    project_id: String,
    path: String,
) -> Result<bool, LibraryError> {
    if window.label() != "main" {
        return Err(unavailable());
    }
    let root = root(app.clone(), state.inner().clone(), project_id).await?;
    tauri::async_runtime::spawn_blocking(move || {
        let preview = video(&root, &path)?;
        let source = Path::new(&preview.absolute_path);
        let Some(destination) = app
            .dialog()
            .file()
            .set_title(if preview.mime.starts_with("audio/") {
                "Salvar áudio"
            } else {
                "Salvar vídeo"
            })
            .set_file_name(
                source
                    .file_name()
                    .and_then(|name| name.to_str())
                    .ok_or_else(unavailable)?,
            )
            .blocking_save_file()
        else {
            return Ok(false);
        };
        let destination = destination.into_path().map_err(|_| unavailable())?;
        save_video(source, &destination)?;
        Ok(true)
    })
    .await
    .map_err(|_| unavailable())?
}

#[tauri::command]
pub async fn open_project_video(
    app: AppHandle,
    window: WebviewWindow,
    state: State<'_, AppState>,
    project_id: String,
    path: String,
) -> Result<(), LibraryError> {
    if window.label() != "main" {
        return Err(unavailable());
    }
    let root = root(app.clone(), state.inner().clone(), project_id).await?;
    tauri::async_runtime::spawn_blocking(move || {
        let preview = video(&root, &path)?;
        app.opener()
            .open_path(preview.absolute_path, None::<&str>)
            .map_err(|_| {
                LibraryError::new(
                    "open_video",
                    "Não foi possível abrir o arquivo de mídia no aplicativo padrão.",
                )
            })
    })
    .await
    .map_err(|_| unavailable())?
}

#[cfg(test)]
mod tests;
