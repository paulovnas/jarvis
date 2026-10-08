//! Manim's Cairo/Pango dependencies live in a separate private environment.
use super::{audiovisual, error, install as core_install, CoreError};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    time::Duration,
};

const MANAGER_VERSION: &str = "2.9.0-0";
const MANIM_VERSION: &str = "0.20.1";
const PYTHON_VERSION: &str = "3.12";
const RECEIPT: &str = "runtime.json";

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Receipt {
    schema: u32,
    platform: String,
    manager_version: String,
    manim_version: String,
    python_version: String,
    prefix: String,
}

pub(super) struct Runtime {
    pub(super) manager: PathBuf,
    pub(super) prefix: PathBuf,
    pub(super) python: PathBuf,
    pub(super) bin: PathBuf,
}

fn asset() -> Result<(&'static str, &'static str, u64), CoreError> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => Ok((
            "osx-arm64",
            "ec2a072f028e1a7cf20f3e2e74d5a8127cf5a5f27636375b5359811565f4e5be",
            14_596_096,
        )),
        ("macos", "x86_64") => Ok((
            "osx-64",
            "1e71054bb3ac9a076e21f7ec48acfef536f9b3f1408f371a942784bf5ef83d8a",
            16_317_928,
        )),
        ("linux", "x86_64") => Ok((
            "linux-64",
            "366cd9cd8be14df1ab8ed50352a82111082a36686b2d389fdb79a92c3fafb3e3",
            18_292_808,
        )),
        ("linux", "aarch64") => Ok((
            "linux-aarch64",
            "9f93b974adcb4d166996af969b6cd371287d1a3e52733704727884d9b74cb7a7",
            22_020_296,
        )),
        ("windows", "x86_64") => Ok((
            "win-64",
            "a6d804394b2418991c4e29562853eaace2f2ce9d9da661a98e74e02e8dbb44b0",
            11_454_464,
        )),
        _ => Err(error(
            "O runtime Manim não está disponível para este sistema e arquitetura.",
        )),
    }
}

fn manager_path(directory: &Path) -> PathBuf {
    directory.join(if cfg!(windows) {
        "micromamba-2.9.0-0.exe"
    } else {
        "micromamba-2.9.0-0"
    })
}

fn contained(directory: &Path, path: &Path) -> Result<(), CoreError> {
    if !fs::canonicalize(path)?.starts_with(fs::canonicalize(directory)?) {
        return Err(error("O runtime Manim precisa permanecer no Core privado."));
    }
    Ok(())
}

fn from_receipt(directory: &Path, receipt: &Receipt) -> Result<Runtime, CoreError> {
    if receipt.schema != 1
        || receipt.platform != asset()?.0
        || receipt.manager_version != MANAGER_VERSION
        || receipt.manim_version != MANIM_VERSION
        || receipt.python_version != PYTHON_VERSION
        || !receipt.prefix.starts_with("env-")
        || receipt.prefix.len() <= 4
        || receipt.prefix.len() > 100
        || !receipt
            .prefix
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
    {
        return Err(error(
            "O registro do runtime Manim está inválido. Reinstale Animações Manim.",
        ));
    }
    let prefix = directory.join(&receipt.prefix);
    let bin = prefix.join(if cfg!(windows) { "Scripts" } else { "bin" });
    let python = prefix.join(if cfg!(windows) {
        "python.exe"
    } else {
        "bin/python"
    });
    let manager = manager_path(directory);
    contained(directory, &prefix)?;
    contained(&prefix, &bin)?;
    for path in [
        &manager,
        &python,
        &bin.join(if cfg!(windows) { "manim.exe" } else { "manim" }),
    ] {
        if !path.is_file() {
            return Err(error(
                "O runtime Manim está incompleto. Reinstale Animações Manim.",
            ));
        }
        contained(if path == &manager { directory } else { &prefix }, path)?;
    }
    Ok(Runtime {
        manager,
        prefix,
        python,
        bin,
    })
}

pub(super) fn runtime(generation: &Path) -> Result<Option<Runtime>, CoreError> {
    let directory = generation.join("animation");
    let path = directory.join(RECEIPT);
    let bytes = match fs::read(&path) {
        Ok(bytes) => bytes,
        Err(cause) if cause.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(cause) => return Err(cause.into()),
    };
    contained(generation, &directory)?;
    contained(&directory, &path)?;
    let receipt = serde_json::from_slice(&bytes).map_err(|_| {
        error("O registro do runtime Manim está inválido. Reinstale Animações Manim.")
    })?;
    from_receipt(&directory, &receipt).map(Some)
}

fn command(manager: &Path, cache: &Path) -> tokio::process::Command {
    let mut command = tokio::process::Command::new(manager);
    command.env_clear();
    for key in [
        "PATH",
        "SystemRoot",
        "SYSTEMROOT",
        "WINDIR",
        "TMPDIR",
        "TEMP",
        "TMP",
        "LOCALAPPDATA",
        "APPDATA",
        "PROGRAMFILES",
    ] {
        if let Some(value) = std::env::var_os(key) {
            command.env(key, value);
        }
    }
    command
        // Micromamba also registers environments and fallback caches in its
        // user's home, independently of root-prefix. Isolate only this child.
        .env("HOME", cache.join("user"))
        .env("USERPROFILE", cache.join("user"))
        .env("XDG_CACHE_HOME", cache)
        .env("XDG_CONFIG_HOME", cache.join("config"))
        .env("XDG_DATA_HOME", cache.join("data"))
        .env("PYTHONNOUSERSITE", "1")
        .env("PYTHONUTF8", "1")
        .env("PYTHONIOENCODING", "utf-8")
        .args(["--no-rc", "--no-env", "--root-prefix"])
        .arg(cache);
    command
}

const VERIFY: &str = r#"
import sys
from pathlib import Path
import cairo, manimpango, manim
from manim import Circle, Scene, Text, tempconfig
assert sys.version_info[:2] == (3, 12)
assert manim.__version__ == '0.20.1'
class Check(Scene):
    def construct(self):
        self.add(Circle(), Text('Jarvis'))
        self.wait(0.2)
with tempconfig({'renderer': 'cairo', 'pixel_width': 320, 'pixel_height': 180,
                 'frame_rate': 10, 'media_dir': sys.argv[1], 'disable_caching': True,
                 'preview': False, 'write_to_movie': True}):
    scene = Check()
    scene.render()
    movie = Path(scene.renderer.file_writer.movie_file_path)
    assert movie.is_file() and movie.stat().st_size > 0
"#;

pub(super) async fn install(generation: &Path, home: &Path) -> Result<(), CoreError> {
    let (platform, digest, size) = asset()?;
    let directory = generation.join("animation");
    fs::create_dir_all(&directory)?;
    contained(generation, &directory)?;
    let manager = manager_path(&directory);
    if manager.exists() {
        contained(&directory, &manager)?;
    }
    if !audiovisual::matches_file(&manager, size, digest).await? {
        let client = reqwest::Client::builder()
            .user_agent("Jarvis-Manim/1.0")
            .connect_timeout(Duration::from_secs(8))
            .read_timeout(Duration::from_secs(30))
            .build()
            .map_err(|_| error("Não foi possível iniciar o download do runtime Manim."))?;
        let suffix = if cfg!(windows) { ".exe" } else { "" };
        let url = format!("https://github.com/mamba-org/micromamba-releases/releases/download/{MANAGER_VERSION}/micromamba-{platform}{suffix}");
        audiovisual::download_checked(&client, &url, digest, Some(size), size, &manager, &|_| {})
            .await?;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&manager, fs::Permissions::from_mode(0o755))?;
    }
    let cache = super::super::root(home).join("cache/openmontage/animation");
    fs::create_dir_all(cache.join("user"))?;
    // Conda embeds the installation prefix. Keep this unique directory in place;
    // TempDir removes it if verification fails or installation is cancelled.
    let pending = tempfile::Builder::new()
        .prefix("env-")
        .tempdir_in(&directory)?;
    let prefix = pending.path();
    let mut create = command(&manager, &cache);
    create
        .args(["create", "--yes", "--prefix"])
        .arg(prefix)
        .args([
            "--override-channels",
            "--channel",
            "conda-forge",
            "python=3.12",
            "manim=0.20.1",
        ])
        .current_dir(&directory);
    core_install::command_unbounded(create).await?;
    let receipt = Receipt {
        schema: 1,
        platform: platform.into(),
        manager_version: MANAGER_VERSION.into(),
        manim_version: MANIM_VERSION.into(),
        python_version: PYTHON_VERSION.into(),
        prefix: prefix
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| error("Caminho do runtime Manim inválido."))?
            .into(),
    };
    let runtime = from_receipt(&directory, &receipt)?;
    let check = tempfile::Builder::new()
        .prefix("check-")
        .tempdir_in(&directory)?;
    let mut verify = command(&manager, &cache);
    verify
        .args(["run", "--prefix"])
        .arg(prefix)
        .arg(&runtime.python)
        .args(["-I", "-B", "-c", VERIFY])
        .arg(check.path())
        .current_dir(check.path());
    core_install::command(verify, 300).await?;
    let mut published = tempfile::NamedTempFile::new_in(&directory)?;
    published.write_all(
        &serde_json::to_vec(&receipt)
            .map_err(|_| error("Não foi possível registrar o runtime Manim."))?,
    )?;
    published.as_file().sync_all()?;
    published
        .persist(directory.join(RECEIPT))
        .map_err(|_| error("Não foi possível salvar o runtime Manim."))?;
    let _ = pending.keep();
    Ok(())
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;

    pub(in crate::core::openmontage) fn fixture(generation: &Path) {
        let directory = generation.join("animation");
        let prefix = directory.join("env-test");
        let bin = prefix.join(if cfg!(windows) { "Scripts" } else { "bin" });
        fs::create_dir_all(&bin).unwrap();
        fs::write(manager_path(&directory), "manager").unwrap();
        fs::write(
            prefix.join(if cfg!(windows) {
                "python.exe"
            } else {
                "bin/python"
            }),
            "python",
        )
        .unwrap();
        fs::write(
            bin.join(if cfg!(windows) { "manim.exe" } else { "manim" }),
            "manim",
        )
        .unwrap();
        let receipt = Receipt {
            schema: 1,
            platform: asset().unwrap().0.into(),
            manager_version: MANAGER_VERSION.into(),
            manim_version: MANIM_VERSION.into(),
            python_version: PYTHON_VERSION.into(),
            prefix: "env-test".into(),
        };
        fs::write(
            directory.join(RECEIPT),
            serde_json::to_vec(&receipt).unwrap(),
        )
        .unwrap();
    }

    #[test]
    fn missing_optional_runtime_is_not_an_error() {
        let generation = tempfile::tempdir().unwrap();
        assert!(runtime(generation.path()).unwrap().is_none());
    }

    #[test]
    fn receipt_resolves_only_the_verified_private_runtime() {
        let generation = tempfile::tempdir().unwrap();
        fixture(generation.path());
        let runtime = runtime(generation.path()).unwrap().unwrap();
        assert_eq!(runtime.prefix, generation.path().join("animation/env-test"));
        assert!(runtime.manager.is_file() && runtime.python.is_file() && runtime.bin.is_dir());
        fs::remove_file(&runtime.python).unwrap();
        assert!(super::runtime(generation.path()).is_err());
    }

    #[test]
    fn untrusted_receipt_cannot_select_other_paths_or_versions() {
        let generation = tempfile::tempdir().unwrap();
        fixture(generation.path());
        let mut receipt: Receipt = serde_json::from_slice(
            &fs::read(generation.path().join("animation/runtime.json")).unwrap(),
        )
        .unwrap();
        for prefix in ["../env-test", "env-test/../outside", "/env-test", "env-"] {
            receipt.prefix = prefix.into();
            fs::write(
                generation.path().join("animation/runtime.json"),
                serde_json::to_vec(&receipt).unwrap(),
            )
            .unwrap();
            assert!(runtime(generation.path()).is_err());
        }
        receipt.prefix = "env-test".into();
        receipt.manim_version = "0.1.0".into();
        fs::write(
            generation.path().join("animation/runtime.json"),
            serde_json::to_vec(&receipt).unwrap(),
        )
        .unwrap();
        assert!(runtime(generation.path()).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_interpreter_cannot_escape_the_private_prefix() {
        let generation = tempfile::tempdir().unwrap();
        fixture(generation.path());
        let python = generation.path().join("animation/env-test/bin/python");
        fs::remove_file(&python).unwrap();
        let outside = tempfile::NamedTempFile::new().unwrap();
        std::os::unix::fs::symlink(outside.path(), python).unwrap();
        assert!(runtime(generation.path()).is_err());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn installation_rejects_external_manager_before_download_or_execution() {
        let generation = tempfile::tempdir().unwrap();
        let home = tempfile::tempdir().unwrap();
        let directory = generation.path().join("animation");
        fs::create_dir_all(&directory).unwrap();
        let outside = tempfile::NamedTempFile::new().unwrap();
        std::os::unix::fs::symlink(outside.path(), manager_path(&directory)).unwrap();
        let failure = install(generation.path(), home.path()).await.unwrap_err();
        assert!(failure.message.contains("Core privado"));
        assert!(!directory.join(RECEIPT).exists());
    }

    #[tokio::test]
    #[ignore = "downloads and renders the real managed Manim environment"]
    async fn native_install_renders_cairo_and_pango_without_system_dependencies() {
        let home = tempfile::tempdir().unwrap();
        let generation = tempfile::tempdir().unwrap();
        #[cfg(unix)]
        if let Some(existing) = std::env::var_os("JARVIS_MANIM_TEST_CACHE") {
            let cache = crate::core::root(home.path()).join("cache/openmontage/animation");
            fs::create_dir_all(cache.parent().unwrap()).unwrap();
            std::os::unix::fs::symlink(existing, cache).unwrap();
        }
        install(generation.path(), home.path()).await.unwrap();
        assert!(runtime(generation.path()).unwrap().is_some());
    }
}
