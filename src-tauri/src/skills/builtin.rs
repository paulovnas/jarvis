use super::{error, store, SkillError};
use std::{
    fs,
    path::{Path, PathBuf},
};

const AUTHORING: &str = include_str!("builtin/jarvis-authoring.md");

pub(super) fn root(home: &Path) -> PathBuf {
    home.join(".jarvis/builtin-skills")
}

pub(super) fn sync(home: &Path) -> Result<(), SkillError> {
    let root = root(home);
    let directory = root.join("jarvis-authoring");
    fs::create_dir_all(&directory)?;
    for path in [&root, &directory] {
        let metadata = fs::symlink_metadata(path)?;
        if !metadata.is_dir() || metadata.is_symlink() {
            return Err(error("A pasta das skills nativas do Jarvis não é segura."));
        }
    }
    let path = directory.join("SKILL.md");
    let current = fs::read(&path).ok();
    if current.as_deref() != Some(AUTHORING.as_bytes()) {
        store::atomic_file(&path, AUTHORING.as_bytes())?;
    }
    let metadata = fs::symlink_metadata(&path)?;
    if !metadata.is_file() || metadata.is_symlink() {
        return Err(error(
            "A skill nativa de autoria do Jarvis está indisponível.",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn materializes_and_repairs_the_managed_authoring_skill() {
        let home = tempfile::tempdir().unwrap();
        sync(home.path()).unwrap();
        let file = root(home.path()).join("jarvis-authoring/SKILL.md");
        assert!(fs::read_to_string(&file)
            .unwrap()
            .contains("jarvis_propose_agent"));
        fs::write(&file, "stale").unwrap();
        sync(home.path()).unwrap();
        assert_eq!(fs::read_to_string(file).unwrap(), AUTHORING);
    }

    #[cfg(unix)]
    #[test]
    fn refuses_a_linked_builtin_package_without_writing_through_it() {
        use std::os::unix::fs::symlink;

        let home = tempfile::tempdir().unwrap();
        let external = tempfile::tempdir().unwrap();
        let root = root(home.path());
        fs::create_dir_all(&root).unwrap();
        symlink(external.path(), root.join("jarvis-authoring")).unwrap();

        assert!(sync(home.path()).is_err());
        assert!(!external.path().join("SKILL.md").exists());
    }
}
