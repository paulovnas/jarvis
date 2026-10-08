//! Reviewed Brag creative package, embedded and cached inside managed HyperFrames.
use super::{error, CoreError};
use flate2::read::GzDecoder;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{Cursor, Read, Write},
    path::{Component, Path, PathBuf},
    sync::{Mutex, OnceLock},
};

pub(crate) const VERSION: &str = "0.4.0";
const MANIFEST: &[u8] = include_bytes!("brag-docs/manifest.json");
const ARCHIVE: &[u8] = include_bytes!("brag-docs/package.tar.gz");
const CACHE: &str = "jarvis-brag";
const MAX_PACKAGE_BYTES: u64 = 32 * 1024 * 1024;
static PROVISION: Mutex<()> = Mutex::new(());
static EMBEDDED_MANIFEST: OnceLock<Result<Manifest, CoreError>> = OnceLock::new();

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Manifest {
    schema_version: u8,
    version: String,
    archive_sha256: String,
    files: Vec<Resource>,
}

#[derive(Deserialize)]
struct Resource {
    path: String,
    bytes: u64,
    sha256: String,
    license: String,
}

fn digest(data: &[u8]) -> String {
    format!("{:x}", Sha256::digest(data))
}

fn controlled_path(value: &str) -> bool {
    !value.is_empty()
        && !value.contains(['\\', ':', '\0'])
        && value
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..")
        && Path::new(value)
            .components()
            .all(|part| matches!(part, Component::Normal(_)))
}

fn manifest() -> Result<&'static Manifest, CoreError> {
    // Embedded bytes cannot change within this executable. Verify them once;
    // resources on disk still receive fresh integrity checks on every access.
    EMBEDDED_MANIFEST
        .get_or_init(verify_embedded_manifest)
        .as_ref()
        .map_err(Clone::clone)
}

fn verify_embedded_manifest() -> Result<Manifest, CoreError> {
    let manifest: Manifest = serde_json::from_slice(MANIFEST)
        .map_err(|_| error("O pacote Brag incluído no Jarvis está inválido."))?;
    let mut names = BTreeSet::new();
    let mut bytes = 0_u64;
    for file in &manifest.files {
        bytes = bytes
            .checked_add(file.bytes)
            .ok_or_else(|| error("Pacote Brag excedeu o limite."))?;
        if !controlled_path(&file.path)
            || !names.insert(&file.path)
            || file.bytes == 0
            || file.sha256.len() != 64
            || !file.sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
            || !matches!(
                file.license.as_str(),
                "MIT" | "CC0-1.0" | "CC-BY-4.0" | "Apache-2.0"
            )
        {
            return Err(error("Um recurso do pacote Brag está inválido."));
        }
    }
    if manifest.schema_version != 1
        || manifest.version != VERSION
        || manifest.files.is_empty()
        || bytes > MAX_PACKAGE_BYTES
        || digest(ARCHIVE) != manifest.archive_sha256
    {
        return Err(error(
            "A integridade do pacote Brag não pôde ser confirmada.",
        ));
    }
    Ok(manifest)
}

fn require_plain_directory(path: &Path) -> Result<(), CoreError> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(error(
            "O pacote Brag deve permanecer na instalação privada do HyperFrames.",
        ));
    }
    Ok(())
}

fn hash_file(path: &Path) -> Result<String, CoreError> {
    let mut file = fs::File::open(path)?;
    let mut hash = Sha256::new();
    let mut buffer = [0_u8; 8192];
    loop {
        let length = file.read(&mut buffer)?;
        if length == 0 {
            break;
        }
        hash.update(&buffer[..length]);
    }
    Ok(format!("{:x}", hash.finalize()))
}

fn validate_tree(
    directory: &Path,
    root: &Path,
    expected: &BTreeSet<&str>,
) -> Result<(), CoreError> {
    let mut pending = vec![directory.to_path_buf()];
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(directory)? {
            let path = entry?.path();
            let metadata = fs::symlink_metadata(&path)?;
            if metadata.file_type().is_symlink() {
                return Err(error("O pacote Brag não permite links simbólicos."));
            }
            if metadata.is_dir() {
                pending.push(path);
            } else {
                let name = path
                    .strip_prefix(root)
                    .map_err(|_| error("Recurso Brag fora do pacote."))?
                    .to_string_lossy()
                    .replace('\\', "/");
                if !metadata.is_file()
                    || (name != "manifest.json" && !expected.contains(name.as_str()))
                {
                    return Err(error("O pacote Brag contém um recurso não reconhecido."));
                }
            }
        }
    }
    Ok(())
}

fn validate(directory: &Path, manifest: &Manifest) -> Result<(), CoreError> {
    require_plain_directory(directory)?;
    let expected = manifest
        .files
        .iter()
        .map(|file| file.path.as_str())
        .collect();
    validate_tree(directory, directory, &expected)?;
    if fs::read(directory.join("manifest.json"))? != MANIFEST {
        return Err(error("O registro do pacote Brag foi alterado."));
    }
    for file in &manifest.files {
        let path = directory.join(&file.path);
        let metadata = fs::symlink_metadata(&path)?;
        if !metadata.is_file() || metadata.len() != file.bytes || hash_file(&path)? != file.sha256 {
            return Err(error(
                "Um recurso do pacote Brag está ausente ou foi alterado.",
            ));
        }
    }
    Ok(())
}

fn extract(archive: &[u8], directory: &Path, manifest: &Manifest) -> Result<(), CoreError> {
    let mut expected: BTreeMap<&str, &Resource> = manifest
        .files
        .iter()
        .map(|file| (file.path.as_str(), file))
        .collect();
    let decoder = GzDecoder::new(Cursor::new(archive));
    let mut archive = tar::Archive::new(decoder);
    for entry in archive.entries()? {
        let entry = entry?;
        let path = entry.path()?.to_string_lossy().into_owned();
        if !controlled_path(&path) || !entry.header().entry_type().is_file() {
            return Err(error(
                "O pacote Brag contém um caminho ou tipo de arquivo inválido.",
            ));
        }
        let resource = expected
            .remove(path.as_str())
            .ok_or_else(|| error("O pacote Brag contém um recurso inesperado ou duplicado."))?;
        if entry.size() != resource.bytes {
            return Err(error("O tamanho de um recurso Brag diverge do registro."));
        }
        let destination = directory.join(&path);
        fs::create_dir_all(
            destination
                .parent()
                .ok_or_else(|| error("Caminho Brag inválido."))?,
        )?;
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&destination)?;
        std::io::copy(&mut entry.take(resource.bytes + 1), &mut file)?;
        file.flush()?;
        file.sync_all()?;
        if fs::metadata(&destination)?.len() != resource.bytes
            || hash_file(&destination)? != resource.sha256
        {
            return Err(error(
                "A integridade de um recurso Brag não pôde ser confirmada.",
            ));
        }
    }
    if !expected.is_empty() {
        return Err(error("O pacote Brag está incompleto."));
    }
    fs::write(directory.join("manifest.json"), MANIFEST)?;
    validate(directory, manifest)
}

pub(super) fn install(package: &Path) -> Result<PathBuf, CoreError> {
    let _guard = PROVISION
        .lock()
        .map_err(|_| error("Não foi possível preparar o pacote Brag."))?;
    require_plain_directory(package)?;
    let manifest = manifest()?;
    let cache = package.join(CACHE);
    fs::create_dir_all(&cache)?;
    require_plain_directory(&cache)?;
    if !fs::canonicalize(&cache)?.starts_with(fs::canonicalize(package)?) {
        return Err(error("O pacote Brag precisa permanecer no Core privado."));
    }
    let lock_path = cache.join("provision.lock");
    if let Ok(metadata) = fs::symlink_metadata(&lock_path) {
        if !metadata.is_file() || metadata.file_type().is_symlink() {
            return Err(error(
                "O controle do pacote Brag precisa permanecer no Core privado.",
            ));
        }
    }
    let lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(lock_path)?;
    // The process mutex handles threads; the existing Core file-lock dependency
    // serializes publication and repair across concurrently running app builds.
    fs2::FileExt::lock_exclusive(&lock)?;
    let destination = cache.join(digest(MANIFEST));
    if destination.exists() {
        require_plain_directory(&destination)?;
        if validate(&destination, manifest).is_ok() {
            return Ok(destination);
        }
    }
    let staging = tempfile::Builder::new()
        .prefix(".brag-staging-")
        .tempdir_in(&cache)?;
    extract(ARCHIVE, staging.path(), manifest)?;
    // Never expose a partly extracted package. Damaged cache generations are
    // quarantined within the same private root and removed by the temp guard.
    let quarantine = tempfile::Builder::new()
        .prefix(".brag-replaced-")
        .tempdir_in(&cache)?;
    if destination.exists() {
        if validate(&destination, manifest).is_ok() {
            return Ok(destination);
        }
        match fs::rename(&destination, quarantine.path().join("damaged")) {
            Ok(()) => (),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
            Err(error) => return Err(error.into()),
        }
    }
    match fs::rename(staging.path(), &destination) {
        Ok(()) => (),
        // A different Jarvis process may have published the same immutable
        // generation while this one was extracting. Accept only verified data.
        Err(error) => {
            validate(&destination, manifest).map_err(|_| CoreError::from(error))?;
        }
    }
    validate(&destination, manifest)?;
    Ok(destination)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::core::{installed, ComponentId};

    fn cache_generation_count(package: &Path) -> usize {
        fs::read_dir(package.join(CACHE))
            .unwrap()
            .filter(|entry| entry.as_ref().unwrap().file_type().unwrap().is_dir())
            .count()
    }

    #[test]
    fn reviewed_package_contains_full_skills_licensed_effects_and_music() {
        let package = tempfile::tempdir().unwrap();
        let directory = install(package.path()).unwrap();
        let catalog: serde_json::Value =
            serde_json::from_slice(&fs::read(directory.join("asset-catalog.json")).unwrap())
                .unwrap();
        let assets = catalog["assets"].as_array().unwrap();
        assert_eq!(assets.len(), 265);
        assert_eq!(
            assets
                .iter()
                .filter(|asset| asset["license"] == "CC0-1.0")
                .count(),
            260
        );
        assert_eq!(
            assets
                .iter()
                .filter(|asset| asset["license"] == "CC-BY-4.0" && asset["attribution"].is_string())
                .count(),
            5
        );
        let skill = fs::read_to_string(directory.join("SKILL.md")).unwrap();
        assert!(skill.contains("Jarvis managed workflow (takes precedence)"));
        assert!(!skill.contains("switch to brag-slim"));
        assert!(directory.join("references/step-1-inspect.md").is_file());
        for domain in [
            "hyperframes-animation",
            "hyperframes-creative",
            "hyperframes-keyframes",
        ] {
            assert!(directory
                .join("hyperframes")
                .join(domain)
                .join("SKILL.md")
                .is_file());
        }
        assert!(directory
            .join("hyperframes/hyperframes-animation/rules-index.md")
            .is_file());
        assert!(directory
            .join("hyperframes/hyperframes-keyframes/references/keyframe-patterns.md")
            .is_file());
        let supplemental: serde_json::Value = serde_json::from_slice(
            &fs::read(directory.join("hyperframes/provenance.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(
            supplemental["revision"],
            "bc57e282fdde4afdccec1d1a2bacd94a9c3c5383"
        );
        assert_eq!(supplemental["license"], "Apache-2.0");
        assert!(fs::read_to_string(directory.join("hyperframes/LICENSE"))
            .unwrap()
            .contains("Apache License"));
        assert!(directory
            .join(
                "assets/music/cues/happy-beats-business-moves-vol-1-by-ende-dot-app.music-cues.md"
            )
            .is_file());
        assert!(!directory.join("slim.md").exists());
        assert!(!directory.join("scripts").exists());
        assert!(fs::read_to_string(directory.join("MUSIC_NOTICE.md"))
            .unwrap()
            .contains("CC BY 4.0"));
    }

    #[test]
    fn missing_brag_lazily_provisions_without_invalidating_legacy_hyperframes() {
        let home = tempfile::tempdir().unwrap();
        let package = super::super::root(home.path()).join("packages/hyperframes/legacy");
        fs::create_dir_all(&package).unwrap();
        super::super::hyperframes::tests::fixture(&package, "0.8.105");
        let installation = super::super::Installation {
            version: "0.8.105".into(),
            directory: "packages/hyperframes/legacy".into(),
            files: super::super::hyperframes::required_files(&package).unwrap(),
        };
        super::super::save_manifest(
            home.path(),
            &super::super::Manifest {
                installations: BTreeMap::from([(ComponentId::Hyperframes, installation)]),
            },
        )
        .unwrap();
        let original = fs::read(super::super::root(home.path()).join("manifest.json")).unwrap();
        assert!(!package.join(CACHE).exists());
        installed(home.path(), ComponentId::Hyperframes).unwrap();
        let directory = install(&package).unwrap();
        assert!(directory.starts_with(&package));
        assert_eq!(
            original,
            fs::read(super::super::root(home.path()).join("manifest.json")).unwrap()
        );
        fs::remove_dir_all(package.join(CACHE)).unwrap();
        installed(home.path(), ComponentId::Hyperframes).unwrap();
    }

    #[test]
    fn repeated_and_concurrent_provisioning_reuses_one_complete_generation() {
        let package = tempfile::tempdir().unwrap();
        let results: Vec<PathBuf> = std::thread::scope(|scope| {
            let handles: Vec<_> = (0..4)
                .map(|_| scope.spawn(|| install(package.path()).unwrap()))
                .collect();
            handles
                .into_iter()
                .map(|handle| handle.join().unwrap())
                .collect()
        });
        assert!(results.iter().all(|path| path == &results[0]));
        assert_eq!(cache_generation_count(package.path()), 1);
        validate(&results[0], manifest().unwrap()).unwrap();
    }

    #[test]
    fn changed_or_missing_resources_are_repaired_atomically() {
        let package = tempfile::tempdir().unwrap();
        let directory = install(package.path()).unwrap();
        let audio_path = directory.join("assets/sfx/keyboard/keypress-001.wav");
        let original_audio = fs::read(&audio_path).unwrap();
        let mut changed_audio = original_audio.clone();
        changed_audio[0] ^= 1;
        fs::write(&audio_path, changed_audio).unwrap();
        fs::remove_file(directory.join("references/tones.md")).unwrap();
        let repaired = install(package.path()).unwrap();
        assert_eq!(directory, repaired);
        assert_eq!(fs::read(audio_path).unwrap(), original_audio);
        validate(&repaired, manifest().unwrap()).unwrap();
        assert_eq!(cache_generation_count(package.path()), 1);
    }

    #[test]
    fn extraction_rejects_unknown_duplicate_link_and_path_escape_entries() {
        assert!(controlled_path("references/tones.md"));
        for path in [
            "../escape",
            "/escape",
            "C:/escape",
            "assets\\escape",
            "a/../b",
            "a//b",
            "./a",
        ] {
            assert!(!controlled_path(path), "{path}");
        }
        let package = tempfile::tempdir().unwrap();
        for kind in [tar::EntryType::Regular, tar::EntryType::Symlink] {
            let encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
            let mut archive = tar::Builder::new(encoder);
            let mut header = tar::Header::new_ustar();
            header.set_size(1);
            header.set_mode(0o644);
            header.set_entry_type(kind);
            header.set_cksum();
            archive
                .append_data(&mut header, "unexpected", Cursor::new(b"x"))
                .unwrap();
            let bytes = archive.into_inner().unwrap().finish().unwrap();
            assert!(extract(&bytes, package.path(), manifest().unwrap()).is_err());
        }
        assert!(!package.path().join("unexpected").exists());
    }

    #[test]
    fn extraction_rejects_duplicates_and_wrong_sizes_before_publication() {
        for (duplicate, size_delta) in [(true, 0), (false, 1)] {
            let staging = tempfile::tempdir().unwrap();
            let encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
            let mut archive = tar::Builder::new(encoder);
            let license = include_bytes!("brag-docs/LICENSE");
            for _ in 0..if duplicate { 2 } else { 1 } {
                let mut header = tar::Header::new_ustar();
                let mut data = license.to_vec();
                data.extend(std::iter::repeat_n(b' ', size_delta));
                header.set_size(data.len() as u64);
                header.set_mode(0o644);
                header.set_cksum();
                archive
                    .append_data(&mut header, "LICENSE", Cursor::new(data))
                    .unwrap();
            }
            let bytes = archive.into_inner().unwrap().finish().unwrap();
            let error = extract(&bytes, staging.path(), manifest().unwrap()).unwrap_err();
            assert!(error
                .message
                .contains(if duplicate { "duplicado" } else { "tamanho" }));
            assert!(!staging.path().join("manifest.json").exists());
        }
    }

    #[test]
    fn extraction_rejects_parent_escape_from_raw_archive_headers() {
        let staging = tempfile::tempdir().unwrap();
        let encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        let mut archive = tar::Builder::new(encoder);
        let mut header = tar::Header::new_ustar();
        header.as_mut_bytes()[..9].copy_from_slice(b"../escape");
        header.set_size(1);
        header.set_mode(0o644);
        header.set_cksum();
        archive.append(&header, Cursor::new(b"x")).unwrap();
        let bytes = archive.into_inner().unwrap().finish().unwrap();
        assert!(extract(&bytes, staging.path(), manifest().unwrap()).is_err());
        assert!(!staging.path().join("manifest.json").exists());
    }

    #[cfg(unix)]
    #[test]
    fn private_cache_never_follows_an_external_symlink() {
        let package = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        fs::write(outside.path().join("marker"), "preserved").unwrap();
        std::os::unix::fs::symlink(outside.path(), package.path().join(CACHE)).unwrap();
        assert!(install(package.path()).is_err());
        assert_eq!(
            fs::read_to_string(outside.path().join("marker")).unwrap(),
            "preserved"
        );
        assert_eq!(fs::read_dir(outside.path()).unwrap().count(), 1);
    }
}
