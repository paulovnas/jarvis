use std::path::{Path, PathBuf};

use tauri::Manager;

const KINDS: [&str; 4] = ["completed", "failed", "question", "approval"];

/// Clip IDs are a closed catalogue, never paths provided by a webview.
pub(super) fn valid_id(id: &str) -> bool {
    id.rsplit_once('-').is_some_and(|(kind, variant)| {
        KINDS.contains(&kind) && matches!(variant, "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8")
    })
}

fn resolve_in(root: &Path, id: &str) -> Option<PathBuf> {
    if !valid_id(id) {
        return None;
    }
    let root = root.canonicalize().ok()?;
    let clip = root.join(format!("{id}.wav")).canonicalize().ok()?;
    (clip.starts_with(&root) && clip.is_file()).then_some(clip)
}

fn available_in(root: &Path) -> Vec<String> {
    KINDS
        .iter()
        .flat_map(|kind| (1..=8).map(move |variant| format!("{kind}-{variant}")))
        .filter(|id| resolve_in(root, id).is_some())
        .collect()
}

fn resource_root(app: &tauri::AppHandle) -> Option<PathBuf> {
    let bundled = app
        .path()
        .resource_dir()
        .ok()?
        .join("jarvito/announcements");
    if bundled.is_dir() {
        return Some(bundled);
    }
    #[cfg(debug_assertions)]
    {
        Some(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/jarvito/announcements"))
    }
    #[cfg(not(debug_assertions))]
    {
        None
    }
}

pub(super) fn available(app: &tauri::AppHandle) -> Vec<String> {
    resource_root(app).map_or_else(Vec::new, |root| available_in(&root))
}

pub(super) fn resolve(app: &tauri::AppHandle, id: &str) -> Option<PathBuf> {
    resolve_in(&resource_root(app)?, id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_known_notice_variants_can_be_resolved() {
        for kind in KINDS {
            for variant in 1..=8 {
                assert!(valid_id(&format!("{kind}-{variant}")));
            }
        }
        for id in [
            "completed-0",
            "failed-9",
            "call-1",
            "question-01",
            "../approval-1",
        ] {
            assert!(!valid_id(id), "{id}");
        }
    }

    #[test]
    fn catalogue_includes_only_present_wav_files() {
        let root = tempfile::tempdir().unwrap();
        assert!(available_in(root.path()).is_empty());
        for name in [
            "completed-2.wav",
            "completed-7.wav",
            "completed-8.wav",
            "failed-1.wav",
            "call-1.wav",
            "question-1.mp3",
        ] {
            std::fs::write(root.path().join(name), b"fixture").unwrap();
        }
        std::fs::create_dir(root.path().join("approval-1.wav")).unwrap();
        assert_eq!(
            available_in(root.path()),
            ["completed-2", "completed-7", "completed-8", "failed-1"]
        );
        let canonical_root = root.path().canonicalize().unwrap();
        for id in ["completed-7", "completed-8"] {
            assert_eq!(
                resolve_in(root.path(), id),
                Some(canonical_root.join(format!("{id}.wav")))
            );
        }
        assert!(resolve_in(root.path(), "completed-1").is_none());
        assert!(resolve_in(root.path(), "../completed-2").is_none());
    }

    #[cfg(unix)]
    #[test]
    fn clips_cannot_escape_the_resource_folder_through_a_symlink() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::NamedTempFile::new().unwrap();
        std::os::unix::fs::symlink(outside.path(), root.path().join("completed-1.wav")).unwrap();
        assert!(resolve_in(root.path(), "completed-1").is_none());
        assert!(available_in(root.path()).is_empty());
    }
}
