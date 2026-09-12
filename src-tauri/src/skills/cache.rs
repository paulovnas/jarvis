use super::{error, root, store, SkillCacheCleanup, SkillCacheStatus, SkillError};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::Stdio,
    sync::{Arc, Mutex, RwLock, Weak},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const CACHE_META: &str = ".jarvis-repository.json";
const CACHE_LOCK: &str = ".cache.lock";
const CACHE_TTL: Duration = Duration::from_secs(120);
const CACHE_RETENTION: Duration = Duration::from_secs(7 * 24 * 60 * 60);
const ORPHAN_GRACE: Duration = Duration::from_secs(10 * 60);
const MAX_REPOSITORIES: usize = 12;
static CACHE_ACCESS: Mutex<BTreeMap<String, Weak<RwLock<()>>>> = Mutex::new(BTreeMap::new());
static REPOSITORY_ACCESS: Mutex<BTreeMap<String, Weak<Mutex<()>>>> = Mutex::new(BTreeMap::new());

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RepositoryMetadata {
    source: String,
    checked_at: u64,
}

fn cache_root(home: &Path) -> PathBuf {
    root(home).join("cache/skills")
}

fn repositories_root(home: &Path) -> PathBuf {
    cache_root(home).join("repositories")
}

fn ensure_real_directory(path: &Path) -> Result<(), SkillError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => Ok(()),
        Ok(_) => Err(error("O cache de skills possui um caminho inválido.")),
        Err(cause) if cause.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir_all(path)?;
            let metadata = fs::symlink_metadata(path)?;
            if metadata.is_dir() && !metadata.file_type().is_symlink() {
                Ok(())
            } else {
                Err(error("O cache de skills possui um caminho inválido."))
            }
        }
        Err(cause) => Err(cause.into()),
    }
}

fn prepare(home: &Path) -> Result<PathBuf, SkillError> {
    let jarvis = root(home);
    ensure_real_directory(&jarvis)?;
    let cache = cache_root(home);
    ensure_real_directory(&cache)?;
    if !cache.canonicalize()?.starts_with(jarvis.canonicalize()?) {
        return Err(error("O cache de skills está fora da pasta do Jarvis."));
    }
    Ok(cache)
}

fn weak_lock<T: Default>(
    registry: &Mutex<BTreeMap<String, Weak<T>>>,
    key: String,
) -> Result<Arc<T>, SkillError> {
    let mut registry = registry
        .lock()
        .map_err(|_| error("Cache de skills indisponível."))?;
    registry.retain(|_, lock| lock.strong_count() > 0);
    if let Some(lock) = registry.get(&key).and_then(Weak::upgrade) {
        return Ok(lock);
    }
    let lock = Arc::new(T::default());
    registry.insert(key, Arc::downgrade(&lock));
    Ok(lock)
}

fn access_lock(home: &Path) -> Result<Arc<RwLock<()>>, SkillError> {
    weak_lock(&CACHE_ACCESS, home.to_string_lossy().into_owned())
}

fn repository_lock(home: &Path, source: &str) -> Result<Arc<Mutex<()>>, SkillError> {
    weak_lock(
        &REPOSITORY_ACCESS,
        format!("{}:{source}", home.to_string_lossy()),
    )
}

fn file_lock(home: &Path) -> Result<fs::File, SkillError> {
    fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(prepare(home)?.join(CACHE_LOCK))
        .map_err(Into::into)
}

fn repository_id(source: &str) -> String {
    format!("{:x}", Sha256::digest(source.as_bytes()))
}

fn repository_entry(home: &Path, source: &str) -> PathBuf {
    repositories_root(home).join(repository_id(source))
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn read_metadata(path: &Path) -> Option<RepositoryMetadata> {
    let bytes = fs::read(path.join(CACHE_META)).ok()?;
    if bytes.len() > 4096 {
        return None;
    }
    serde_json::from_slice(&bytes).ok()
}

fn write_metadata_at(path: &Path, source: &str, checked_at: u64) -> Result<(), SkillError> {
    store::atomic_file(
        &path.join(CACHE_META),
        &serde_json::to_vec(&RepositoryMetadata {
            source: source.into(),
            checked_at,
        })
        .map_err(|_| error("Registro do cache de skills inválido."))?,
    )
}

fn write_metadata(path: &Path, source: &str) -> Result<(), SkillError> {
    write_metadata_at(path, source, now())
}

fn repository_is_valid(entry: &Path, source: &str) -> bool {
    fs::symlink_metadata(entry)
        .is_ok_and(|metadata| metadata.is_dir() && !metadata.file_type().is_symlink())
        && fs::symlink_metadata(entry.join("checkout/.git"))
            .is_ok_and(|metadata| metadata.is_dir() && !metadata.file_type().is_symlink())
        && read_metadata(entry).is_some_and(|metadata| metadata.source == source)
}

fn repository_is_fresh(entry: &Path, source: &str) -> bool {
    repository_is_valid(entry, source)
        && read_metadata(entry)
            .is_some_and(|metadata| now().saturating_sub(metadata.checked_at) < CACHE_TTL.as_secs())
}

fn git_command() -> std::process::Command {
    let mut command = crate::background::command("git");
    command
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_LFS_SKIP_SMUDGE", "1")
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_CONFIG_COUNT")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    command
}

fn run_git(command: &mut std::process::Command, failure: &'static str) -> Result<(), SkillError> {
    let mut child = command
        .spawn()
        .map_err(|_| error("Instale o Git para baixar skills do Marketplace."))?;
    let started = Instant::now();
    loop {
        if let Some(status) = child.try_wait()? {
            return if status.success() {
                Ok(())
            } else {
                Err(error(failure))
            };
        }
        if started.elapsed() > Duration::from_secs(90) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(error("O download da skill excedeu o tempo limite."));
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn remove_node(path: &Path) -> Result<(), SkillError> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(cause) if cause.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(cause) => return Err(cause.into()),
    };
    if metadata.is_dir() && !metadata.file_type().is_symlink() {
        fs::remove_dir_all(path)?;
    } else {
        fs::remove_file(path)?;
    }
    Ok(())
}

fn clone_repository(home: &Path, source: &str, entry: &Path) -> Result<PathBuf, SkillError> {
    let repositories = repositories_root(home);
    ensure_real_directory(&repositories)?;
    let staging = tempfile::Builder::new()
        .prefix("refresh-")
        .tempdir_in(&repositories)?;
    let checkout = staging.path().join("checkout");
    let url = format!("https://github.com/{source}.git");
    let mut command = git_command();
    command
        .args([
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "credential.helper=",
            "-c",
            "protocol.file.allow=never",
            "-c",
            "protocol.ext.allow=never",
            "clone",
            "--quiet",
            "--depth",
            "1",
            "--filter=blob:none",
            "--single-branch",
            "--no-tags",
            "--no-recurse-submodules",
            "--",
            &url,
        ])
        .arg(&checkout);
    run_git(
        &mut command,
        "Não foi possível baixar o repositório público da skill.",
    )?;
    write_metadata(staging.path(), source)?;
    if fs::symlink_metadata(entry).is_ok() {
        remove_node(entry)?;
    }
    let staging = staging.keep();
    if let Err(cause) = fs::rename(&staging, entry) {
        let _ = fs::remove_dir_all(&staging);
        return Err(cause.into());
    }
    Ok(entry.join("checkout"))
}

fn repository_command(checkout: &Path, arguments: &[&str]) -> Result<(), SkillError> {
    let mut command = git_command();
    command
        .args([
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "credential.helper=",
            "-c",
            "protocol.file.allow=never",
            "-c",
            "protocol.ext.allow=never",
            "-C",
        ])
        .arg(checkout)
        .args(arguments);
    run_git(
        &mut command,
        "Não foi possível atualizar o repositório público da skill.",
    )
}

fn refresh_repository(source: &str, entry: &Path) -> Result<PathBuf, SkillError> {
    let checkout = entry.join("checkout");
    let url = format!("https://github.com/{source}.git");
    repository_command(&checkout, &["remote", "set-url", "origin", &url])?;
    repository_command(
        &checkout,
        &[
            "fetch",
            "--quiet",
            "--depth",
            "1",
            "--no-tags",
            "origin",
            "HEAD",
        ],
    )?;
    repository_command(&checkout, &["reset", "--hard", "--quiet", "FETCH_HEAD"])?;
    repository_command(&checkout, &["clean", "-ffdx"])?;
    write_metadata(entry, source)?;
    Ok(checkout)
}

fn ensure_repository(
    home: &Path,
    source: &str,
    force: bool,
) -> Result<(PathBuf, bool), SkillError> {
    let entry = repository_entry(home, source);
    if !force && repository_is_fresh(&entry, source) {
        return Ok((entry.join("checkout"), false));
    }
    if repository_is_valid(&entry, source) {
        return refresh_repository(source, &entry).map(|path| (path, true));
    }
    clone_repository(home, source, &entry).map(|path| (path, true))
}

pub(super) fn with_repository<T>(
    home: &Path,
    source: &str,
    force: bool,
    operation: impl FnOnce(&Path) -> Result<T, SkillError>,
) -> Result<T, SkillError> {
    store::validate(source, "skill")?;
    let access = access_lock(home)?;
    let access_guard = access
        .read()
        .map_err(|_| error("Cache de skills indisponível."))?;
    let cache_file = file_lock(home)?;
    FileExt::lock_shared(&cache_file)
        .map_err(|_| error("O cache de skills está sendo limpo por outro Jarvis."))?;
    let repository_access = repository_lock(home, source)?;
    let repository_guard = repository_access
        .lock()
        .map_err(|_| error("Repositório de skills indisponível."))?;
    let (repository, changed) = ensure_repository(home, source, force)?;
    let result = operation(&repository);
    drop(repository_guard);
    drop(cache_file);
    drop(access_guard);
    if changed {
        let _ = maintain(home, Some(&repository_entry(home, source)));
    }
    result
}

fn directory_bytes(path: &Path) -> Result<u64, SkillError> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(cause) if cause.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(cause) => return Err(cause.into()),
    };
    if metadata.file_type().is_symlink() || metadata.is_file() {
        return Ok(metadata.len());
    }
    if !metadata.is_dir() {
        return Ok(0);
    }
    let mut bytes = metadata.len();
    let entries = match fs::read_dir(path) {
        Ok(entries) => entries,
        Err(cause) if cause.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(cause) => return Err(cause.into()),
    };
    for entry in entries {
        match entry {
            Ok(entry) => bytes = bytes.saturating_add(directory_bytes(&entry.path())?),
            Err(cause) if cause.kind() == std::io::ErrorKind::NotFound => {}
            Err(cause) => return Err(cause.into()),
        }
    }
    Ok(bytes)
}

fn status_locked(home: &Path) -> Result<SkillCacheStatus, SkillError> {
    let cache = prepare(home)?;
    let mut repositories = 0;
    let mut residues = 0;
    for entry in fs::read_dir(&cache)? {
        let entry = entry?;
        if entry.file_name().to_string_lossy().starts_with("repo-") {
            residues += 1;
        }
    }
    let repositories_path = repositories_root(home);
    match fs::symlink_metadata(&repositories_path) {
        Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {
            for entry in fs::read_dir(&repositories_path)? {
                let entry = entry?;
                if entry.file_name().to_string_lossy().starts_with("refresh-") {
                    residues += 1;
                } else {
                    repositories += 1;
                }
            }
        }
        Ok(_) => return Err(error("O cache de skills possui um caminho inválido.")),
        Err(cause) if cause.kind() == std::io::ErrorKind::NotFound => {}
        Err(cause) => return Err(cause.into()),
    }
    Ok(SkillCacheStatus {
        bytes: directory_bytes(&cache)?,
        repositories,
        residues,
    })
}

pub(super) fn status(home: &Path) -> Result<SkillCacheStatus, SkillError> {
    let access = access_lock(home)?;
    let _access_guard = access
        .read()
        .map_err(|_| error("Cache de skills indisponível."))?;
    let cache_file = file_lock(home)?;
    FileExt::lock_shared(&cache_file)
        .map_err(|_| error("O cache de skills está sendo limpo por outro Jarvis."))?;
    status_locked(home)
}

fn expired(path: &Path, grace: Duration) -> bool {
    fs::symlink_metadata(path)
        .and_then(|metadata| metadata.modified())
        .ok()
        .and_then(|modified| modified.elapsed().ok())
        .is_some_and(|age| age >= grace)
}

fn cleanup_residues(home: &Path, grace: Duration) -> Result<(), SkillError> {
    let cache = prepare(home)?;
    for entry in fs::read_dir(&cache)? {
        let entry = entry?;
        if entry.file_name().to_string_lossy().starts_with("repo-") && expired(&entry.path(), grace)
        {
            remove_node(&entry.path())?;
        }
    }
    let repositories = repositories_root(home);
    match fs::symlink_metadata(&repositories) {
        Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {
            for entry in fs::read_dir(&repositories)? {
                let entry = entry?;
                if entry.file_name().to_string_lossy().starts_with("refresh-")
                    && expired(&entry.path(), grace)
                {
                    remove_node(&entry.path())?;
                }
            }
        }
        Ok(_) => return Err(error("O cache de skills possui um caminho inválido.")),
        Err(cause) if cause.kind() == std::io::ErrorKind::NotFound => {}
        Err(cause) => return Err(cause.into()),
    }
    Ok(())
}

fn prune_repositories(home: &Path, keep: Option<&Path>) -> Result<(), SkillError> {
    let repositories = repositories_root(home);
    match fs::symlink_metadata(&repositories) {
        Err(cause) if cause.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(cause) => return Err(cause.into()),
        Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {}
        Ok(_) => return Err(error("O cache de skills possui um caminho inválido.")),
    }
    let current = now();
    let mut entries = Vec::new();
    for entry in fs::read_dir(&repositories)? {
        let entry = entry?;
        let path = entry.path();
        if entry.file_name().to_string_lossy().starts_with("refresh-") {
            continue;
        }
        let metadata = read_metadata(&path);
        let checked_at = metadata.as_ref().map_or(0, |metadata| metadata.checked_at);
        let valid = metadata.is_some()
            && fs::symlink_metadata(&path)
                .is_ok_and(|metadata| metadata.is_dir() && !metadata.file_type().is_symlink());
        entries.push((path, checked_at, valid));
    }
    entries.sort_by(|left, right| {
        let left_keep = keep.is_some_and(|keep| keep == left.0);
        let right_keep = keep.is_some_and(|keep| keep == right.0);
        right_keep
            .cmp(&left_keep)
            .then_with(|| right.1.cmp(&left.1))
    });
    for (index, (path, checked_at, valid)) in entries.into_iter().enumerate() {
        if keep.is_some_and(|keep| keep == path) {
            continue;
        }
        let expired = current.saturating_sub(checked_at) >= CACHE_RETENTION.as_secs();
        if !valid || expired || index >= MAX_REPOSITORIES {
            remove_node(&path)?;
        }
    }
    Ok(())
}

fn maintain(home: &Path, keep: Option<&Path>) -> Result<(), SkillError> {
    let access = access_lock(home)?;
    let _access_guard = match access.try_write() {
        Ok(guard) => guard,
        Err(_) => return Ok(()),
    };
    let cache_file = file_lock(home)?;
    if FileExt::try_lock_exclusive(&cache_file).is_err() {
        return Ok(());
    }
    cleanup_residues(home, ORPHAN_GRACE)?;
    prune_repositories(home, keep)
}

pub(super) fn start_maintenance(home: &Path) {
    let home = home.to_path_buf();
    let _ = std::thread::Builder::new()
        .name("jarvis-skill-cache".into())
        .spawn(move || {
            let _ = maintain(&home, None);
        });
}

pub(super) fn clear(home: &Path) -> Result<SkillCacheCleanup, SkillError> {
    let access = access_lock(home)?;
    let _access_guard = access
        .try_write()
        .map_err(|_| error("O cache de skills está em uso. Tente novamente em instantes."))?;
    let cache_file = file_lock(home)?;
    FileExt::try_lock_exclusive(&cache_file).map_err(|_| {
        error("O cache de skills está sendo usado por outro Jarvis. Tente novamente em instantes.")
    })?;
    let before = status_locked(home)?;
    let cache = cache_root(home);
    for entry in fs::read_dir(&cache)? {
        let entry = entry?;
        if entry.file_name() != CACHE_LOCK {
            remove_node(&entry.path())?;
        }
    }
    let status = status_locked(home)?;
    Ok(SkillCacheCleanup {
        freed_bytes: before.bytes.saturating_sub(status.bytes),
        removed_repositories: before.repositories,
        removed_residues: before.residues,
        status,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cached_repository(home: &Path, source: &str, body: &[u8]) -> PathBuf {
        let entry = repository_entry(home, source);
        fs::create_dir_all(entry.join("checkout/.git")).unwrap();
        fs::write(entry.join("checkout/content.bin"), body).unwrap();
        write_metadata(&entry, source).unwrap();
        entry
    }

    #[test]
    fn repository_cache_reuses_a_stable_source_path_within_the_ttl() {
        let temporary = tempfile::tempdir().unwrap();
        let home = temporary.path();
        let entry = cached_repository(home, "owner/repository", b"cached");

        let (checkout, changed) = ensure_repository(home, "owner/repository", false).unwrap();

        assert_eq!(checkout, entry.join("checkout"));
        assert!(!changed);
        assert_ne!(
            repository_entry(home, "owner/repository"),
            repository_entry(home, "another/repository")
        );
    }

    #[test]
    fn startup_maintenance_removes_expired_residues_and_keeps_repository_cache() {
        let temporary = tempfile::tempdir().unwrap();
        let home = temporary.path();
        let cache = prepare(home).unwrap();
        let repository = cached_repository(home, "owner/repository", b"cached");
        let legacy = cache.join("repo-abandoned");
        let staging = repositories_root(home).join("refresh-abandoned");
        fs::create_dir_all(&legacy).unwrap();
        fs::create_dir_all(&staging).unwrap();
        fs::write(legacy.join("data"), b"legacy").unwrap();
        fs::write(staging.join("data"), b"staging").unwrap();

        cleanup_residues(home, Duration::ZERO).unwrap();

        assert!(!legacy.exists());
        assert!(!staging.exists());
        assert!(repository.join("checkout/content.bin").is_file());
    }

    #[test]
    fn repository_cache_is_bounded_and_preserves_the_current_entry() {
        let temporary = tempfile::tempdir().unwrap();
        let home = temporary.path();
        let mut keep = PathBuf::new();
        for index in 0..(MAX_REPOSITORIES + 2) {
            let source = format!("owner/repository-{index}");
            let entry = cached_repository(home, &source, b"cached");
            write_metadata_at(&entry, &source, now().saturating_sub(index as u64)).unwrap();
            if index == MAX_REPOSITORIES + 1 {
                keep = entry;
            }
        }

        prune_repositories(home, Some(&keep)).unwrap();

        assert_eq!(status_locked(home).unwrap().repositories, MAX_REPOSITORIES);
        assert!(keep.exists());
    }

    #[test]
    fn manual_cleanup_refuses_to_remove_a_cache_held_by_an_active_reader() {
        let temporary = tempfile::tempdir().unwrap();
        let home = temporary.path();
        cached_repository(home, "owner/repository", b"cached");
        let access = access_lock(home).unwrap();
        let _reader = access.read().unwrap();

        let failure = clear(home).unwrap_err();

        assert!(failure.message.contains("está em uso"));
        assert!(repository_entry(home, "owner/repository").exists());
    }

    #[test]
    fn manual_cleanup_reports_reclaimed_bytes_and_preserves_installed_skills() {
        let temporary = tempfile::tempdir().unwrap();
        let home = temporary.path();
        cached_repository(home, "owner/repository", &[b'x'; 4096]);
        let installed = root(home).join("skills/example/SKILL.md");
        fs::create_dir_all(installed.parent().unwrap()).unwrap();
        fs::write(&installed, "---\nname: example\ndescription: Test\n---\n").unwrap();

        let result = clear(home).unwrap();

        assert!(result.freed_bytes >= 4096);
        assert_eq!(result.removed_repositories, 1);
        assert_eq!(result.status.repositories, 0);
        assert!(installed.is_file());
    }

    #[cfg(unix)]
    #[test]
    fn cleanup_refuses_to_follow_a_linked_cache_root() {
        use std::os::unix::fs::symlink;

        let temporary = tempfile::tempdir().unwrap();
        let home = temporary.path();
        let external = home.join("external");
        fs::create_dir_all(root(home).join("cache")).unwrap();
        fs::create_dir_all(&external).unwrap();
        fs::write(external.join("keep"), b"keep").unwrap();
        symlink(&external, cache_root(home)).unwrap();

        assert!(clear(home).is_err());
        assert!(external.join("keep").is_file());
    }
}
