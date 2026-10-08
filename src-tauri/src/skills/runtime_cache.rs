//! Immutable runtime snapshots. read_skill always rechecks current permissions.
use super::{catalog, error, read_config, root, store, Config, Skill, SkillError};
use sha2::{Digest, Sha256};
use std::{
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::SystemTime,
};

static CACHE: Mutex<Vec<Entry>> = Mutex::new(Vec::new());
const MAX_PROJECTS: usize = 8;
type MetadataStamp = (u64, Option<SystemTime>, Option<SystemTime>, u64, i64, i64);

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Stamp {
    path: PathBuf,
    link: Option<PathBuf>,
    metadata: Option<MetadataStamp>,
    content: Option<[u8; 32]>,
    entries: Option<Vec<OsString>>,
}

impl Stamp {
    pub(super) fn read(path: &Path) -> Self {
        let file = fs::metadata(path).ok();
        let metadata = file.as_ref().map(|m| {
            #[cfg(unix)]
            let identity = {
                use std::os::unix::fs::MetadataExt;
                (m.ino(), m.ctime(), m.ctime_nsec())
            };
            #[cfg(not(unix))]
            let identity = (0, 0, 0);
            (
                m.len(),
                m.modified().ok(),
                m.created().ok(),
                identity.0,
                identity.1,
                identity.2,
            )
        });
        // Metadata alone can miss rapid same-size edits on coarse timestamp filesystems.
        // ponytail: hash bounded skill files; use filesystem notifications if polling becomes costly.
        let content = file
            .as_ref()
            .filter(|m| m.is_file())
            .and_then(|_| catalog::text(path).ok())
            .map(|text| Sha256::digest(text.as_bytes()).into());
        let mut entries = file
            .as_ref()
            .filter(|m| m.is_dir())
            .and_then(|_| fs::read_dir(path).ok())
            .and_then(|entries| {
                entries
                    .map(|entry| entry.map(|e| e.file_name()))
                    .collect::<std::io::Result<Vec<_>>>()
                    .ok()
            });
        if let Some(names) = &mut entries {
            names.sort();
        }
        Self {
            path: path.to_path_buf(),
            link: fs::read_link(path).ok(),
            metadata,
            content,
            entries,
        }
    }
    fn valid(&self) -> bool {
        *self == Self::read(&self.path)
    }
}

struct Entry {
    home: PathBuf,
    project: PathBuf,
    config: Config,
    plugin_revision: u64,
    watched: Vec<Stamp>,
    skills: Arc<[Skill]>,
}

pub(super) fn active(home: &Path, project: &Path) -> Result<Arc<[Skill]>, SkillError> {
    let config = read_config(home)?;
    let plugin_revision = crate::plugins::load_active_for_project(home, Some(project))
        .map_err(|cause| error(cause.message))?
        .revision;
    let mut cache = CACHE
        .lock()
        .map_err(|_| error("Catálogo de skills ocupado."))?;
    if let Some(index) = cache
        .iter()
        .position(|e| e.home == home && e.project == project)
    {
        let entry = cache.remove(index);
        if entry.config == config
            && entry.plugin_revision == plugin_revision
            && entry.watched.iter().all(Stamp::valid)
        {
            let result = Arc::clone(&entry.skills);
            cache.push(entry);
            return Ok(result);
        }
    }
    store::recover(home)?;
    let (skills, _, mut watched) = catalog::discover_watched(home, Some(project), &config)?;
    watched.push(Stamp::read(&root(home)));
    let skills: Arc<[Skill]> = skills.into_iter().filter(|s| s.enabled).collect();
    // A racing external edit prevents reuse; execution uses authoritative reads.
    if watched.iter().all(Stamp::valid) {
        if cache.len() >= MAX_PROJECTS {
            cache.remove(0);
        }
        cache.push(Entry {
            home: home.into(),
            project: project.into(),
            config,
            plugin_revision,
            watched,
            skills: Arc::clone(&skills),
        });
    }
    Ok(skills)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn suppress_metadata_change(home: &Path, path: &Path) {
        // Reproduce filesystems that report identical metadata for rapid edits.
        let metadata = Stamp::read(path).metadata;
        for entry in CACHE.lock().unwrap().iter_mut().filter(|e| e.home == home) {
            for stamp in entry.watched.iter_mut().filter(|s| s.path == path) {
                stamp.metadata = metadata;
            }
        }
    }

    #[test]
    fn catalog_refreshes_on_edit_add_disable_remove_and_project_switch() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path();
        let dir = root(home).join("skills/one");
        fs::create_dir_all(&dir).unwrap();
        let file = dir.join("SKILL.md");
        let content = "---\nname: one\ndescription: First description\n---\nBody";
        fs::write(&file, content).unwrap();
        let first = active(home, home).unwrap();
        assert!(Arc::ptr_eq(&first, &active(home, home).unwrap()));
        fs::write(&file, content.replace("First", "Other")).unwrap();
        suppress_metadata_change(home, &file);
        let edited = active(home, home).unwrap();
        assert!(edited[0].description.contains("Other"));
        assert!(first[0].description.contains("First"));
        let new = root(home).join("skills/two");
        fs::create_dir_all(&new).unwrap();
        fs::write(new.join("SKILL.md"), content.replace("one", "two")).unwrap();
        suppress_metadata_change(home, new.parent().unwrap());
        assert_eq!(active(home, home).unwrap().len(), 2);
        super::super::update_config(home, |c| {
            c.disabled.insert(first[0].id.clone());
        })
        .unwrap();
        assert_eq!(active(home, home).unwrap().len(), 1);
        assert!(!Arc::ptr_eq(
            &edited,
            &active(home, &home.join("another-project")).unwrap()
        ));
        fs::remove_dir_all(&new).unwrap();
        suppress_metadata_change(home, new.parent().unwrap());
        assert!(active(home, home).unwrap().is_empty());
    }
    #[tokio::test]
    async fn plugin_skill_cache_refreshes_and_reads_authorized_frozen_version() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path();
        let create = |body: &str| {
            crate::plugins::Operation::Create { draft: serde_json::from_value(serde_json::json!({"name":"design-fixture","description":"Plugin skills fixture","skills":[{"name":"design","content":format!("---\nname: design\ndescription: Design a product\n---\n{body}")}],"files":[],"mcpServers":{},"apps":{}})).unwrap() }
        };
        let prepared = crate::plugins::preview(home, 0, create("Original guidance"))
            .await
            .unwrap();
        let catalog = crate::plugins::apply(home, &prepared).unwrap();
        let frozen = active(home, home).unwrap();
        let skill = frozen
            .iter()
            .find(|skill| skill.origin == "plugin")
            .unwrap();
        assert!(skill.managed && skill.enabled);
        assert!(skill.name.ends_with(":design"));
        let id = skill.id.clone();
        let prepared = crate::plugins::preview(home, catalog.revision, create("Updated guidance"))
            .await
            .unwrap();
        let catalog = crate::plugins::apply(home, &prepared).unwrap();
        let fresh = active(home, home).unwrap();
        assert!(fresh.iter().any(|skill| skill.id == id));
        assert!(!Arc::ptr_eq(&frozen, &fresh));
        let args = serde_json::json!({"id":id});
        let old = super::super::read_from_snapshot(home, home, &args, &frozen)
            .await
            .unwrap();
        assert!(old.contains("Original guidance") && !old.contains("Updated guidance"));
        assert!(super::super::read(home, home, &args)
            .await
            .unwrap()
            .contains("Updated guidance"));
        let prepared = crate::plugins::preview(
            home,
            catalog.revision,
            crate::plugins::Operation::SetEnabled {
                plugin_id: catalog.installed[0].id.clone(),
                enabled: false,
                project_path: None,
            },
        )
        .await
        .unwrap();
        crate::plugins::apply(home, &prepared).unwrap();
        assert!(super::super::read_from_snapshot(home, home, &args, &frozen)
            .await
            .is_err());
    }
}
