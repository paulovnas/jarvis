use super::{catalog, error, root, Detail, Skill, SkillError};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::Stdio,
    sync::Mutex,
    time::{Duration, Instant},
};

const META: &str = ".jarvis-source.json";
const MAX_PACKAGE: u64 = 32 * 1024 * 1024;
static REPOS: Mutex<BTreeMap<String, CachedRepo>> = Mutex::new(BTreeMap::new());
struct CachedRepo {
    _directory: tempfile::TempDir,
    path: PathBuf,
    checked: Instant,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Metadata {
    pub source: String,
    pub skill_id: String,
    pub subpath: PathBuf,
    pub digest: String,
    #[serde(default)]
    pub update_available: bool,
    #[serde(default)]
    pub update_error: Option<String>,
}
pub(super) fn validate(source: &str, skill_id: &str) -> Result<(), SkillError> {
    let parts: Vec<_> = source.split('/').collect();
    let valid = |s: &str| {
        !s.is_empty()
            && s.len() <= 100
            && !s.starts_with('.')
            && s != ".."
            && s.bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
    };
    if parts.len() != 2 || !parts.iter().all(|p| valid(p)) || !valid(skill_id) {
        return Err(error("Origem da skill inválida."));
    }
    Ok(())
}
pub(super) fn metadata(dir: &Path) -> Result<Option<Metadata>, SkillError> {
    match fs::read(dir.join(META)) {
        Ok(bytes) => {
            if bytes.len() > 16000 {
                return Err(error("Registro de instalação inválido."));
            }
            let meta: Metadata = serde_json::from_slice(&bytes)
                .map_err(|_| error("Registro de instalação inválido."))?;
            validate(&meta.source, &meta.skill_id)?;
            if meta.subpath.is_absolute()
                || meta.subpath.components().any(|c| {
                    !matches!(
                        c,
                        std::path::Component::Normal(_) | std::path::Component::CurDir
                    )
                })
            {
                return Err(error("Caminho da origem inválido."));
            }
            Ok(Some(meta))
        }
        Err(cause) if cause.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(cause) => Err(cause.into()),
    }
}
pub(super) fn atomic_file(path: &Path, bytes: &[u8]) -> Result<(), SkillError> {
    let parent = path.parent().ok_or_else(|| error("Caminho inválido."))?;
    fs::create_dir_all(parent)?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    temporary.write_all(bytes)?;
    temporary.as_file().sync_all()?;
    temporary
        .persist(path)
        .map_err(|_| error("Não foi possível salvar a configuração da skill."))?;
    sync_dir(parent)?;
    Ok(())
}
fn sync_dir(path: &Path) -> Result<(), SkillError> {
    #[cfg(unix)]
    fs::File::open(path)?.sync_all()?;
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}
fn save_metadata(dir: &Path, meta: &Metadata) -> Result<(), SkillError> {
    atomic_file(
        &dir.join(META),
        &serde_json::to_vec_pretty(meta).map_err(|_| error("Registro inválido."))?,
    )
}
fn repository(home: &Path, source: &str, force: bool) -> Result<PathBuf, SkillError> {
    validate(source, "skill")?;
    let key = format!("{}:{source}", home.display());
    {
        let mut cache = REPOS
            .lock()
            .map_err(|_| error("Cache de skills indisponível."))?;
        if !force {
            if let Some(repo) = cache.get(&key) {
                if repo.checked.elapsed() < Duration::from_secs(120) {
                    return Ok(repo.path.clone());
                }
            }
        }
        cache.retain(|_, repo| repo.checked.elapsed() < Duration::from_secs(120));
    }
    let cache_path = root(home).join("cache/skills");
    fs::create_dir_all(&cache_path)?;
    let temporary = tempfile::Builder::new()
        .prefix("repo-")
        .tempdir_in(&cache_path)?;
    let path = temporary.path().join("checkout");
    let url = format!("https://github.com/{source}.git");
    let mut child = crate::background::command("git")
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
            "--no-recurse-submodules",
            "--",
            &url,
        ])
        .arg(&path)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_LFS_SKIP_SMUDGE", "1")
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_CONFIG_COUNT")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| error("Instale o Git para baixar skills do Marketplace."))?;
    let started = Instant::now();
    loop {
        if let Some(status) = child.try_wait()? {
            if !status.success() {
                return Err(error(
                    "Não foi possível baixar o repositório público da skill.",
                ));
            }
            break;
        }
        if started.elapsed() > Duration::from_secs(90) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(error("O download da skill excedeu o tempo limite."));
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    // Git and the network must never hold the shared cache mutex. A detail
    // request closed by the user may still finish in the background, and it
    // must not freeze every later Marketplace request while cloning.
    let mut cache = REPOS
        .lock()
        .map_err(|_| error("Cache de skills indisponível."))?;
    if !force {
        if let Some(repo) = cache.get(&key) {
            if repo.checked.elapsed() < Duration::from_secs(120) {
                return Ok(repo.path.clone());
            }
        }
    }
    if cache.len() >= 8 {
        if let Some(oldest) = cache
            .iter()
            .min_by_key(|(_, repo)| repo.checked)
            .map(|(key, _)| key.clone())
        {
            cache.remove(&oldest);
        }
    }
    cache.insert(
        key,
        CachedRepo {
            _directory: temporary,
            path: path.clone(),
            checked: Instant::now(),
        },
    );
    Ok(path)
}
pub(super) fn resolve(repo: &Path, skill_id: &str) -> Result<PathBuf, SkillError> {
    fn matches(dir: &Path, skill_id: &str) -> bool {
        let Some(file) = catalog::skill_file(dir) else {
            return false;
        };
        catalog::parse(&file).is_ok_and(|(name, _, _)| {
            dir.file_name().is_some_and(|segment| segment == skill_id) || name == skill_id
        })
    }

    // Match the deterministic roots used by skill managers before recursive
    // discovery. Repositories may publish generated plugin/provider copies of
    // the same skill elsewhere; the authoring root remains the stable source.
    for dir in [
        repo.join(skill_id),
        repo.join("skills").join(skill_id),
        repo.join(".agents").join("skills").join(skill_id),
        repo.join(".claude").join("skills").join(skill_id),
        repo.join(".codex").join("skills").join(skill_id),
        repo.join(".cursor").join("skills").join(skill_id),
        repo.join(".gemini").join("skills").join(skill_id),
        repo.join(".github").join("skills").join(skill_id),
    ] {
        if matches(&dir, skill_id) {
            return Ok(dir);
        }
    }

    fn walk(
        dir: &Path,
        skill_id: &str,
        found: &mut Vec<PathBuf>,
        remaining: &mut usize,
        depth: usize,
    ) -> Result<(), SkillError> {
        if *remaining == 0 || depth > 9 {
            return Ok(());
        }
        *remaining -= 1;
        if catalog::skill_file(dir).is_some() {
            // Repositories commonly expose compatibility aliases such as
            // `.gemini/skills/<name>/SKILL.md` through symlinks. Parsing uses
            // O_NOFOLLOW and therefore accepts only an actual package manifest;
            // otherwise a single skill appears twice and becomes ambiguous.
            if matches(dir, skill_id) {
                found.push(dir.to_path_buf());
            }
            return Ok(());
        }
        let mut entries: Vec<_> = fs::read_dir(dir)?.filter_map(Result::ok).collect();
        entries.sort_by_key(|e| e.file_name());
        for entry in entries {
            let name = entry.file_name();
            if name == ".git" || name == "node_modules" || name == "target" {
                continue;
            }
            if entry.file_type()?.is_dir() {
                walk(&entry.path(), skill_id, found, remaining, depth + 1)?;
            }
        }
        Ok(())
    }
    let mut found = Vec::new();
    walk(repo, skill_id, &mut found, &mut 12000, 0)?;
    if found.len() != 1 {
        return Err(error(if found.is_empty() {
            "SKILL.md não encontrado no repositório."
        } else {
            "O repositório contém mais de uma skill com este nome."
        }));
    }
    Ok(found.remove(0))
}
fn package_files(dir: &Path) -> Result<Vec<(PathBuf, Vec<u8>, fs::Permissions)>, SkillError> {
    fn walk(
        base: &Path,
        dir: &Path,
        files: &mut Vec<(PathBuf, Vec<u8>, fs::Permissions)>,
        bytes: &mut u64,
        depth: usize,
    ) -> Result<(), SkillError> {
        if depth > 16 {
            return Err(error("A skill possui pastas demais."));
        }
        let mut entries: Vec<_> = fs::read_dir(dir)?.collect::<Result<_, _>>()?;
        entries.sort_by_key(|e| e.file_name());
        for entry in entries {
            if entry.file_name() == ".git" || entry.file_name() == META {
                continue;
            }
            let meta = entry.path().symlink_metadata()?;
            if meta.is_symlink() || (!meta.is_file() && !meta.is_dir()) {
                return Err(error(
                    "O pacote contém um link ou arquivo especial não suportado.",
                ));
            }
            if meta.is_dir() {
                walk(base, &entry.path(), files, bytes, depth + 1)?;
            } else {
                *bytes = bytes.saturating_add(meta.len());
                if *bytes > MAX_PACKAGE || files.len() >= 2000 {
                    return Err(error(
                        "A skill excede o limite de 32 MiB ou 2.000 arquivos.",
                    ));
                }
                let data = fs::read(entry.path())?;
                if data.len() as u64 > meta.len() {
                    return Err(error("O pacote mudou durante a leitura."));
                }
                files.push((
                    entry
                        .path()
                        .strip_prefix(base)
                        .map_err(|_| error("Caminho inválido."))?
                        .to_path_buf(),
                    data,
                    meta.permissions(),
                ));
            }
        }
        Ok(())
    }
    let mut files = Vec::new();
    walk(dir, dir, &mut files, &mut 0, 0)?;
    Ok(files)
}
pub(super) fn digest(dir: &Path) -> Result<String, SkillError> {
    let mut hash = Sha256::new();
    for (path, bytes, permissions) in package_files(dir)? {
        let path = path.to_string_lossy();
        hash.update((path.len() as u64).to_le_bytes());
        hash.update(path.as_bytes());
        hash.update((bytes.len() as u64).to_le_bytes());
        hash.update(bytes);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            hash.update((permissions.mode() & 0o111).to_le_bytes());
        }
        #[cfg(not(unix))]
        let _ = permissions;
    }
    Ok(format!("{:x}", hash.finalize()))
}
pub(super) fn target(home: &Path, source: &str, skill_id: &str) -> PathBuf {
    root(home).join("skills").join(format!(
        "{}-{}",
        skill_id,
        &catalog::id(Path::new(source))[..12]
    ))
}
pub(super) fn recover(home: &Path) -> Result<(), SkillError> {
    let transactions = root(home).join("skill-transactions");
    if !transactions.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(transactions)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let marker = entry.path().join("target.json");
        if !marker.exists() {
            continue;
        }
        let name: String = serde_json::from_slice(&fs::read(marker)?)
            .map_err(|_| error("Instalação de skill interrompida com registro inválido."))?;
        if name.is_empty()
            || name.starts_with('.')
            || name.len() > 150
            || !name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
        {
            return Err(error("Registro de recuperação inválido."));
        }
        let destination = root(home).join("skills").join(name);
        let backup = entry.path().join("previous");
        if !destination.exists() && backup.exists() {
            fs::rename(&backup, &destination)?;
            sync_dir(destination.parent().unwrap())?;
        }
        fs::remove_dir_all(entry.path())?;
    }
    Ok(())
}
pub(super) fn replace(
    home: &Path,
    source_dir: &Path,
    destination: &Path,
    meta: &Metadata,
) -> Result<(), SkillError> {
    recover(home)?;
    if fs::symlink_metadata(destination).is_ok_and(|m| m.is_symlink()) {
        return Err(error("O destino da skill não pode ser um link."));
    }
    let parent = root(home).join("skill-transactions");
    fs::create_dir_all(&parent)?;
    fs::create_dir_all(root(home).join("skills"))?;
    let staging = tempfile::Builder::new()
        .prefix("install-")
        .tempdir_in(&parent)?;
    let next = staging.path().join("next");
    fs::create_dir(&next)?;
    for (relative, bytes, permissions) in package_files(source_dir)? {
        let path = next.join(relative);
        fs::create_dir_all(path.parent().unwrap())?;
        let mut file = fs::File::create(&path)?;
        file.write_all(&bytes)?;
        file.set_permissions(permissions)?;
        file.sync_all()?;
    }
    catalog::parse(&catalog::skill_file(&next).ok_or_else(|| error("SKILL.md ausente."))?)?;
    save_metadata(&next, meta)?;
    let target_name = destination
        .file_name()
        .ok_or_else(|| error("Destino inválido."))?
        .to_string_lossy();
    atomic_file(
        &staging.path().join("target.json"),
        &serde_json::to_vec(&target_name).map_err(|_| error("Destino inválido."))?,
    )?;
    // Keep the recovery journal until both directory renames are durable.
    let staging = staging.keep();
    let result = (|| {
        if destination.exists() {
            fs::rename(destination, staging.join("previous"))?;
            sync_dir(destination.parent().unwrap())?;
            sync_dir(&staging)?;
        }
        fs::rename(&next, destination)?;
        sync_dir(destination.parent().unwrap())?;
        Ok::<_, SkillError>(())
    })();
    if result.is_err() {
        recover(home)?;
        return result;
    }
    fs::remove_dir_all(staging)?;
    Ok(())
}
pub(super) fn preview(home: &Path, source: &str, skill_id: &str) -> Result<Detail, SkillError> {
    validate(source, skill_id)?;
    let repo = repository(home, source, false)?;
    let dir = resolve(&repo, skill_id)?;
    let file = catalog::skill_file(&dir).ok_or_else(|| error("SKILL.md ausente."))?;
    let (name, description, _) = catalog::parse(&file)?;
    Ok(Detail {
        name,
        description,
        content: catalog::text(&file)?,
        source: Some(source.into()),
        path: None,
        files: catalog::files(&dir)?,
    })
}
pub(super) fn install(home: &Path, source: &str, skill_id: &str) -> Result<(), SkillError> {
    validate(source, skill_id)?;
    let destination = target(home, source, skill_id);
    if destination.exists() {
        return Err(error("Esta skill já está instalada."));
    }
    let repo = repository(home, source, false)?;
    let dir = resolve(&repo, skill_id)?;
    let meta = Metadata {
        source: source.into(),
        skill_id: skill_id.into(),
        subpath: dir
            .strip_prefix(&repo)
            .map_err(|_| error("Origem inválida."))?
            .to_path_buf(),
        digest: digest(&dir)?,
        update_available: false,
        update_error: None,
    };
    replace(home, &dir, &destination, &meta)
}
pub(super) fn check(home: &Path) -> Result<(), SkillError> {
    let config = super::read_config(home)?;
    let (skills, _) = catalog::discover(home, None, &config)?;
    let mut repos: BTreeMap<String, Result<PathBuf, SkillError>> = BTreeMap::new();
    for skill in skills
        .into_iter()
        .filter(|s| s.origin == "jarvis" && s.source.is_some())
    {
        let Some(mut meta) = metadata(&skill.path)? else {
            continue;
        };
        let result = repos
            .entry(meta.source.clone())
            .or_insert_with(|| repository(home, &meta.source, true))
            .clone()
            .and_then(|repo| resolve(&repo, &meta.skill_id))
            .and_then(|dir| digest(&dir));
        match result {
            Ok(remote) => {
                meta.update_available = remote != meta.digest;
                meta.update_error = None;
            }
            Err(cause) => meta.update_error = Some(cause.message),
        }
        save_metadata(&skill.path, &meta)?;
    }
    Ok(())
}
pub(super) fn update(home: &Path, skill: &Skill) -> Result<(), SkillError> {
    let Some(mut meta) = metadata(&skill.path)? else {
        return Err(error("Esta skill não foi instalada pelo Marketplace."));
    };
    if skill.origin != "jarvis"
        || Some(&skill.path)
            != target(home, &meta.source, &meta.skill_id)
                .canonicalize()
                .ok()
                .as_ref()
    {
        return Err(error("Esta skill não é gerenciada pelo Jarvis."));
    }
    if digest(&skill.path)? != meta.digest {
        return Err(error(format!(
            "{} tem alterações locais. Preserve-as antes de atualizar.",
            skill.name
        )));
    }
    let repo = repository(home, &meta.source, false)?;
    let dir = resolve(&repo, &meta.skill_id)?;
    meta.digest = digest(&dir)?;
    meta.subpath = dir
        .strip_prefix(&repo)
        .map_err(|_| error("Origem inválida."))?
        .to_path_buf();
    meta.update_available = false;
    meta.update_error = None;
    replace(home, &dir, &skill.path, &meta)
}
