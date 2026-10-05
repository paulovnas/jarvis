//! Managed, offline, CPU-only ComfyUI graphs. Each operation has one cancellable
//! private process and a host-owned staging directory, never an HTTP server.
use super::{audiovisual, error, install, installed, ComponentId, CoreError, DownloadProgress};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

pub(super) const VERSION: &str = "1.0.0";
const COMFY_VERSION: &str = "0.3.8";
const REVISION: &str = "9f4b181ab38b246961c5a51994a8357e62634de1";
const SOURCE_SHA256: &str = "a5ccb341db71f1af757a18468b7bab1f823689cbc1fd242c211e22f07557c1a2";
const MODEL_SHA256: &str = "309c8469258dda742793dce0ebea8e6dd393174f89934733ecc8b14c76f4ddd8";
const MODEL_SIZE: u64 = 4_574_861;
const ENTRY: &str = "jarvis-image.py";
const RECEIPT: &str = "jarvis-comfyui.json";
const MODEL: &str = "models/u2netp.onnx";

#[derive(Serialize, Deserialize)]
struct Metadata {
    version: String,
    python: String,
    comfyui: String,
    revision: String,
    model: String,
}
fn metadata() -> Metadata {
    Metadata {
        version: VERSION.into(),
        python: audiovisual::PYTHON_VERSION.into(),
        comfyui: COMFY_VERSION.into(),
        revision: REVISION.into(),
        model: MODEL_SHA256.into(),
    }
}

pub(crate) struct Runtime {
    pub python: PathBuf,
    pub entry: PathBuf,
    pub package: PathBuf,
}
pub(crate) fn runtime(home: &Path) -> Result<Runtime, CoreError> {
    let record = installed(home, ComponentId::Comfyui)?;
    Ok(Runtime::at(&record.path(home)?))
}
pub(super) fn python_path(package: &Path) -> PathBuf {
    // Only the platform path/install helpers are shared with audio, never its
    // installation record, inference environment, packages or generation path.
    audiovisual::python_path(package)
}
impl Runtime {
    fn at(package: &Path) -> Self {
        Self {
            python: python_path(package),
            entry: package.join(ENTRY),
            package: package.into(),
        }
    }
    pub(crate) fn environment(&self, output: &Path) -> Result<BTreeMap<String, String>, CoreError> {
        if !output.is_absolute() || !output.is_dir() {
            return Err(error("A pasta temporária das imagens está indisponível."));
        }
        let mut paths = vec![self
            .python
            .parent()
            .ok_or_else(|| error("Python privado inválido."))?
            .to_path_buf()];
        paths.extend(std::env::split_paths(
            &std::env::var_os("PATH").unwrap_or_default(),
        ));
        let path =
            std::env::join_paths(paths).map_err(|_| error("Ambiente de imagem inválido."))?;
        let output = output.to_string_lossy().into_owned();
        let mut environment = BTreeMap::from([
            ("PATH".into(), path.to_string_lossy().into_owned()),
            (
                "PYTHONPATH".into(),
                self.package.join("packages").to_string_lossy().into_owned(),
            ),
            ("PYTHONHOME".into(), String::new()),
            ("PYTHONNOUSERSITE".into(), "1".into()),
            ("PYTHONUNBUFFERED".into(), "1".into()),
            ("PYTHONDONTWRITEBYTECODE".into(), "1".into()),
            ("HF_HUB_OFFLINE".into(), "1".into()),
            ("TRANSFORMERS_OFFLINE".into(), "1".into()),
            ("HF_HUB_DISABLE_TELEMETRY".into(), "1".into()),
            ("TOKENIZERS_PARALLELISM".into(), "false".into()),
            ("JOBLIB_MULTIPROCESSING".into(), "0".into()),
            // Keep optional compiler caches disabled: this fixed CPU graph
            // requires neither JIT compilation nor alpha-matting libraries.
            ("NUMBA_DISABLE_JIT".into(), "1".into()),
            ("OMP_NUM_THREADS".into(), "2".into()),
            ("MKL_NUM_THREADS".into(), "2".into()),
            (
                "U2NET_HOME".into(),
                self.package.join("models").to_string_lossy().into_owned(),
            ),
        ]);
        for key in [
            "HF_HOME",
            "NUMBA_CACHE_DIR",
            "XDG_CACHE_HOME",
            "TMPDIR",
            "TEMP",
            "TMP",
        ] {
            environment.insert(key.into(), output.clone());
        }
        Ok(environment)
    }
}
pub(super) fn required_files(package: &Path) -> Result<Vec<String>, CoreError> {
    Ok(vec![
        ENTRY.into(),
        RECEIPT.into(),
        MODEL.into(),
        "PROVENANCE.md".into(),
        "models/U2NET-LICENSE".into(),
        "source/LICENSE".into(),
        "source/nodes.py".into(),
        "source/execution.py".into(),
        "packages/torch/__init__.py".into(),
        "packages/numpy/__init__.py".into(),
        "packages/onnxruntime/__init__.py".into(),
        "packages/PIL/__init__.py".into(),
        python_path(package)
            .strip_prefix(package)
            .map_err(|_| error("Python privado inválido."))?
            .to_string_lossy()
            .into_owned(),
    ])
}
pub(super) fn validate(package: &Path, version: &str) -> Result<(), CoreError> {
    let record: Metadata = serde_json::from_slice(&fs::read(package.join(RECEIPT))?)
        .map_err(|_| error("Registro ComfyUI inválido."))?;
    if version != VERSION
        || record.version != VERSION
        || record.python != audiovisual::PYTHON_VERSION
        || record.comfyui != COMFY_VERSION
        || record.revision != REVISION
        || record.model != MODEL_SHA256
    {
        return Err(error(
            "O contrato de imagem mudou. Atualize ComfyUI no Core.",
        ));
    }
    let base = fs::canonicalize(package)?;
    for file in required_files(package)? {
        let path = package.join(file);
        if !path.is_file() || !fs::canonicalize(path)?.starts_with(&base) {
            return Err(error(
                "ComfyUI precisa permanecer na instalação privada do Jarvis.",
            ));
        }
    }
    if fs::metadata(package.join(MODEL))?.len() != MODEL_SIZE
        || fs::read(package.join(ENTRY))? != include_bytes!("comfyui/runner.py")
    {
        return Err(error(
            "ComfyUI está incompleto. Repare o componente no Core.",
        ));
    }
    Ok(())
}
pub(super) async fn verify(package: &Path, version: &str) -> Result<(), CoreError> {
    validate(package, version)?;
    if !audiovisual::matches_file(&package.join(MODEL), MODEL_SIZE, MODEL_SHA256).await? {
        return Err(error(
            "Checksum do modelo de imagem divergente. Repare ComfyUI no Core.",
        ));
    }
    let runtime = Runtime::at(package);
    let output = tempfile::tempdir()?;
    let mut command = tokio::process::Command::new(&runtime.python);
    command
        .arg(&runtime.entry)
        .arg("--health")
        .envs(runtime.environment(output.path())?)
        .current_dir(output.path());
    install::command(command, 60).await?;
    Ok(())
}

async fn cached_download(
    client: &reqwest::Client,
    cache: &Path,
    url: &str,
    digest: &str,
    size: u64,
    progress: &(impl Fn(DownloadProgress) + Sync),
) -> Result<PathBuf, CoreError> {
    let target = cache.join(digest);
    if !audiovisual::matches_file(&target, size, digest).await? {
        audiovisual::download_checked(client, url, digest, Some(size), size, &target, progress)
            .await?;
    }
    Ok(target)
}

pub(super) async fn install(
    home: &Path,
    destination: &Path,
    stage: &(impl Fn(&str) + Sync),
    progress: &(impl Fn(DownloadProgress) + Sync),
) -> Result<Vec<String>, CoreError> {
    audiovisual::check_platform().await?;
    let client = reqwest::Client::builder()
        .user_agent("Jarvis-ComfyUI/1.0")
        .connect_timeout(Duration::from_secs(8))
        .read_timeout(Duration::from_secs(30))
        .build()
        .map_err(|_| error("Não foi possível iniciar o download de imagem."))?;
    let cache = super::root(home).join("cache/comfyui");
    fs::create_dir_all(&cache)?;
    let (platform, digest) = audiovisual::python_asset().map_err(|_| {
        error("ComfyUI gerenciado requer macOS Intel/Apple Silicon, Windows x64 ou Linux x64.")
    })?;
    let archive = destination.join("python.tar.gz");
    let name = format!(
        "cpython-{}+{}-{platform}-install_only.tar.gz",
        audiovisual::PYTHON_VERSION,
        audiovisual::PYTHON_RELEASE
    );
    stage("Preparando Python privado de imagem");
    audiovisual::download_checked(
        &client,
        &format!(
            "https://github.com/astral-sh/python-build-standalone/releases/download/{}/{name}",
            audiovisual::PYTHON_RELEASE
        ),
        digest,
        None,
        160 * 1024 * 1024,
        &archive,
        progress,
    )
    .await?;
    let bytes = fs::read(&archive)?;
    let python = destination.join("python");
    tokio::task::spawn_blocking(move || install::unpack(bytes, &python, false, true))
        .await
        .map_err(|_| error("Não foi possível extrair Python privado."))??;
    fs::remove_file(archive)?;
    stage("Baixando ComfyUI oficial");
    let source = cached_download(
        &client,
        &cache,
        &format!("https://codeload.github.com/Comfy-Org/ComfyUI/tar.gz/{REVISION}"),
        SOURCE_SHA256,
        9_442_162,
        progress,
    )
    .await?;
    let bytes = fs::read(source)?;
    let source = destination.join("source");
    tokio::task::spawn_blocking(move || install::unpack(bytes, &source, false, true))
        .await
        .map_err(|_| error("Não foi possível extrair ComfyUI."))??;
    let runtime = Runtime::at(destination);
    let requirements = destination.join("requirements.txt");
    fs::write(&requirements, include_str!("comfyui/requirements.txt"))?;
    stage("Instalando fluxo e remoção de fundo CPU");
    let mut command = tokio::process::Command::new(&runtime.python);
    command
        .args([
            "-m",
            "pip",
            "--isolated",
            "install",
            "--disable-pip-version-check",
            "--no-input",
            "--only-binary=:all:",
            "--no-compile",
            "--no-deps",
            "--target",
        ])
        .arg(destination.join("packages"))
        .arg("--cache-dir")
        .arg(super::root(home).join("cache/pip"))
        .arg("--requirement")
        .arg(requirements)
        .args(if cfg!(target_os = "macos") {
            ["torch==2.2.2", "torchvision==0.17.2"]
        } else {
            ["torch==2.2.2+cpu", "torchvision==0.17.2+cpu"]
        })
        .args(["--extra-index-url", "https://download.pytorch.org/whl/cpu"])
        .current_dir(destination)
        .env("PYTHONNOUSERSITE", "1")
        .env_remove("PYTHONHOME")
        .env_remove("PYTHONPATH");
    install::command_unbounded(command).await?;
    stage("Preparando modelo local de remoção de fundo");
    let model = cached_download(
        &client,
        &cache,
        "https://github.com/danielgatis/rembg/releases/download/v0.0.0/u2netp.onnx",
        MODEL_SHA256,
        MODEL_SIZE,
        progress,
    )
    .await?;
    fs::create_dir_all(destination.join("models"))?;
    if fs::hard_link(&model, destination.join(MODEL)).is_err() {
        fs::copy(model, destination.join(MODEL))?;
    }
    fs::write(destination.join(ENTRY), include_bytes!("comfyui/runner.py"))?;
    fs::write(
        destination.join("PROVENANCE.md"),
        include_bytes!("comfyui/PROVENANCE.md"),
    )?;
    fs::write(
        destination.join("models/U2NET-LICENSE"),
        include_bytes!("comfyui/U2NET-LICENSE"),
    )?;
    fs::write(
        destination.join(RECEIPT),
        serde_json::to_vec(&metadata()).map_err(|_| error("Registro ComfyUI inválido."))?,
    )?;
    stage("Validando ComfyUI offline");
    verify(destination, VERSION).await?;
    required_files(destination)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    pub(crate) fn fixture(package: &Path) {
        fs::create_dir_all(package).unwrap();
        for file in required_files(package).unwrap() {
            let path = package.join(file);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, "fixture").unwrap();
        }
        fs::write(package.join(ENTRY), include_bytes!("comfyui/runner.py")).unwrap();
        fs::write(
            package.join(RECEIPT),
            serde_json::to_vec(&metadata()).unwrap(),
        )
        .unwrap();
        fs::File::create(package.join(MODEL))
            .unwrap()
            .set_len(MODEL_SIZE)
            .unwrap();
    }
    #[test]
    fn runtime_is_complete_independent_and_writes_only_to_staging() {
        let package = tempfile::tempdir().unwrap();
        let output = tempfile::tempdir().unwrap();
        fixture(package.path());
        assert!(validate(package.path(), VERSION).is_ok());
        assert!(ComponentId::Comfyui.required());
        let runtime = Runtime::at(package.path());
        let env = runtime.environment(output.path()).unwrap();
        assert_eq!(
            env["PYTHONPATH"],
            package.path().join("packages").to_string_lossy()
        );
        assert_eq!(env["HF_HOME"], output.path().to_string_lossy());
        assert_eq!(env["JOBLIB_MULTIPROCESSING"], "0");
        assert_eq!(env["NUMBA_DISABLE_JIT"], "1");
        assert_eq!(env["HF_HUB_OFFLINE"], "1");
        assert_eq!(env["PYTHONNOUSERSITE"], "1");
        assert!(runtime.environment(Path::new("relative")).is_err());
        fs::write(package.path().join(ENTRY), "tampered").unwrap();
        assert!(validate(package.path(), VERSION).is_err());
    }
    #[cfg(unix)]
    #[test]
    fn installed_models_cannot_escape_the_private_generation() {
        let package = tempfile::tempdir().unwrap();
        let outside = tempfile::NamedTempFile::new().unwrap();
        fixture(package.path());
        fs::remove_file(package.path().join(MODEL)).unwrap();
        std::os::unix::fs::symlink(outside.path(), package.path().join(MODEL)).unwrap();
        assert!(validate(package.path(), VERSION).is_err());
    }
    #[tokio::test]
    #[ignore = "Downloads the private CPU runtime and checks official ComfyUI imports"]
    async fn official_comfyui_install() {
        let home = tempfile::tempdir().unwrap();
        install::install(
            home.path(),
            ComponentId::Comfyui,
            |phase| eprintln!("{phase}"),
            |_| {},
        )
        .await
        .unwrap();
        runtime(home.path()).unwrap();
    }
}
