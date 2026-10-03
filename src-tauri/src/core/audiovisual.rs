//! Private, versioned audio inference. Models are downloaded during installation,
//! never implicitly by an agent invocation or local health check.
use super::{error, install, installed, relative, ComponentId, CoreError, DownloadProgress};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

pub(super) const VERSION: &str = "1.0.0";
pub(super) const PYTHON_VERSION: &str = "3.11.16";
pub(super) const PYTHON_RELEASE: &str = "20260929";
const ENTRY: &str = "jarvis-audio.py";
const RECEIPT: &str = "jarvis-audiovisual.json";

#[derive(Deserialize)]
struct Assets {
    revision: String,
    models: Vec<ModelAsset>,
}
#[derive(Deserialize)]
struct ModelAsset {
    path: String,
    url: String,
    sha256: String,
    size: u64,
}
fn assets() -> Result<Assets, CoreError> {
    let assets: Assets = serde_json::from_str(include_str!("audiovisual/assets.json"))
        .map_err(|_| error("Contrato dos modelos de áudio inválido."))?;
    if assets.models.is_empty()
        || assets.models.iter().any(|asset| {
            !relative(&asset.path)
                || asset.size == 0
                || !asset.url.starts_with("https://")
                || asset.sha256.len() != 64
                || !asset.sha256.bytes().all(|b| b.is_ascii_hexdigit())
        })
    {
        return Err(error("Manifesto de downloads de áudio inválido."));
    }
    Ok(assets)
}

#[derive(Serialize, Deserialize)]
struct Metadata {
    version: String,
    python: String,
    revision: String,
    music_license: String,
}

pub(crate) struct Runtime {
    pub python: PathBuf,
    pub entry: PathBuf,
    pub package: PathBuf,
    pub models: PathBuf,
    pub lock: PathBuf,
}
pub(crate) fn runtime(home: &Path) -> Result<Runtime, CoreError> {
    let record = installed(home, ComponentId::Audiovisual)?;
    let package = record.path(home)?;
    refresh_runner(&package)?;
    Ok(Runtime::at(&package))
}

fn refresh_runner(package: &Path) -> Result<(), CoreError> {
    use std::io::Write;
    let entry = contained_file(package, ENTRY)?;
    let source = include_bytes!("audiovisual/runner.py");
    if fs::read(&entry)? == source {
        return Ok(());
    }
    // The runtime/dependency contract is unchanged. Publish just the bundled
    // runner atomically so existing installations receive narration fixes.
    let mut staged = tempfile::NamedTempFile::new_in(package)?;
    staged.write_all(source)?;
    staged.as_file().sync_all()?;
    staged
        .persist(entry)
        .map_err(|_| error("Não foi possível atualizar o executor de áudio do Core."))?;
    #[cfg(unix)]
    fs::File::open(package)?.sync_all()?;
    Ok(())
}
pub(super) fn python_path(package: &Path) -> PathBuf {
    package.join(if cfg!(windows) {
        "python/python.exe"
    } else {
        "python/bin/python3.11"
    })
}
impl Runtime {
    fn at(package: &Path) -> Self {
        Self {
            python: python_path(package),
            entry: package.join(ENTRY),
            models: package.join("models"),
            lock: package.parent().unwrap_or(package).join(".inference.lock"),
            package: package.into(),
        }
    }
    pub(crate) fn environment(&self, action: &str) -> Result<BTreeMap<String, String>, CoreError> {
        let packages = match action {
            "narrate" => "packages_tts",
            "music" => "packages_music",
            _ => return Err(error("Selecione narração ou música.")),
        };
        let mut paths = vec![self
            .python
            .parent()
            .ok_or_else(|| error("Python privado inválido."))?
            .to_path_buf()];
        paths.extend(std::env::split_paths(
            &std::env::var_os("PATH").unwrap_or_default(),
        ));
        let path = std::env::join_paths(paths).map_err(|_| error("Ambiente de áudio inválido."))?;
        Ok(BTreeMap::from([
            ("PATH".into(), path.to_string_lossy().into_owned()),
            (
                "PYTHONPATH".into(),
                self.package.join(packages).to_string_lossy().into_owned(),
            ),
            ("PYTHONHOME".into(), String::new()),
            ("PYTHONNOUSERSITE".into(), "1".into()),
            ("PYTHONUNBUFFERED".into(), "1".into()),
            ("PYTHONDONTWRITEBYTECODE".into(), "1".into()),
            (
                "HF_HOME".into(),
                self.package
                    .join("offline-cache")
                    .to_string_lossy()
                    .into_owned(),
            ),
            ("HF_HUB_OFFLINE".into(), "1".into()),
            ("TRANSFORMERS_OFFLINE".into(), "1".into()),
            ("HF_HUB_DISABLE_TELEMETRY".into(), "1".into()),
            ("TOKENIZERS_PARALLELISM".into(), "false".into()),
            // Inference is already serialized by the host lock. Avoid joblib's
            // unused multiprocessing semaphore probe inside the OS sandbox.
            ("JOBLIB_MULTIPROCESSING".into(), "0".into()),
            ("OMP_NUM_THREADS".into(), "2".into()),
            ("MKL_NUM_THREADS".into(), "2".into()),
            (
                "JARVIS_AUDIO_LOCK_PATH".into(),
                self.lock.to_string_lossy().into_owned(),
            ),
        ]))
    }
}

fn contained_file(package: &Path, file: &str) -> Result<PathBuf, CoreError> {
    let path = package.join(file);
    if !relative(file)
        || !path.is_file()
        || !fs::canonicalize(&path)?.starts_with(fs::canonicalize(package)?)
    {
        return Err(error(
            "O runtime de áudio deve permanecer na instalação privada do Jarvis.",
        ));
    }
    Ok(path)
}
pub(super) fn validate(package: &Path, version: &str) -> Result<(), CoreError> {
    let metadata: Metadata = serde_json::from_slice(&fs::read(package.join(RECEIPT))?)
        .map_err(|_| error("Registro do runtime de áudio inválido."))?;
    let assets = assets()?;
    if version != VERSION
        || metadata.version != version
        || metadata.python != PYTHON_VERSION
        || metadata.revision != assets.revision
        || metadata.music_license != "CC-BY-NC-4.0"
    {
        return Err(error(
            "O contrato do áudio mudou. Atualize o componente Audiovisual no Core.",
        ));
    }
    for file in required_files(package)? {
        contained_file(package, &file)?;
    }
    for model in assets.models {
        if fs::metadata(package.join("models").join(model.path))?.len() != model.size {
            return Err(error(
                "Modelo de áudio incompleto. Reinstale Audiovisual no Core.",
            ));
        }
    }
    Ok(())
}
pub(super) fn required_files(package: &Path) -> Result<Vec<String>, CoreError> {
    let mut files = vec![
        ENTRY.into(),
        RECEIPT.into(),
        python_path(package)
            .strip_prefix(package)
            .map_err(|_| error("Python privado inválido."))?
            .to_string_lossy()
            .into_owned(),
        "packages_tts/kokoro_onnx/__init__.py".into(),
        "packages_tts/onnxruntime/__init__.py".into(),
        "packages_music/torch/__init__.py".into(),
        "packages_music/transformers/__init__.py".into(),
    ];
    files.extend(
        assets()?
            .models
            .into_iter()
            .map(|asset| format!("models/{}", asset.path)),
    );
    Ok(files)
}
pub(super) async fn verify(package: &Path, version: &str) -> Result<(), CoreError> {
    validate(package, version)?;
    let runtime = Runtime::at(package);
    for (action, script) in [
        ("narrate", "import sys,importlib.metadata as m;import kokoro_onnx,onnxruntime;assert sys.version.split()[0]=='3.11.16';assert m.version('kokoro-onnx')=='0.6.1';print('Kokoro ONNX pronto')"),
        ("music", "import sys,torch,transformers,numpy;assert sys.version.split()[0]=='3.11.16';assert torch.__version__.split('+')[0]=='2.2.2';assert transformers.__version__=='4.46.3';assert numpy.__version__=='1.26.4';print('MusicGen pronto')"),
    ] {
        let mut command = tokio::process::Command::new(&runtime.python);
        command.args(["-c", script]).envs(runtime.environment(action)?).current_dir(package);
        install::command(command, 40).await?;
    }
    Ok(())
}

pub(super) fn python_asset() -> Result<(&'static str, &'static str), CoreError> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => Ok(("aarch64-apple-darwin", "ed30a8c85d48b8d51dd11cc9acb63ef407786fac3ff77f211dd7768a926ae73f")),
        ("macos", "x86_64") => Ok(("x86_64-apple-darwin", "1c1b5da6c23a10cdbedcae9de1b00a340187d39605ea945390e921e2086e2e11")),
        ("windows", "x86_64") => Ok(("x86_64-pc-windows-msvc", "fe8c767e9d9b19f6780b8a843fe68f6c52c4e9771b1d4db3b4af4d8883d49530")),
        ("linux", "x86_64") => Ok(("x86_64-unknown-linux-gnu", "fbbfd0f2253996486455f44a28c09ac7e2df534b8a4835408d17c99d185b80ec")),
        _ => Err(error("O ambiente audiovisual gerenciado requer macOS Intel/Apple Silicon, Windows x64 ou Linux x64.")),
    }
}

#[cfg(any(target_os = "macos", test))]
fn check_macos_version(version: &str) -> Result<(), CoreError> {
    let mut numbers = version.trim().split('.').map(str::parse::<u32>);
    let major = numbers.next().and_then(Result::ok);
    if major.is_none() || numbers.any(|number| number.is_err()) {
        return Err(error(
            "Não foi possível identificar a versão do macOS. Audiovisual requer macOS 13 ou posterior.",
        ));
    }
    if major.is_some_and(|major| major < 13) {
        return Err(error(
            "Audiovisual requer macOS 13 ou posterior para gerar voz e música localmente. Nenhum modelo foi baixado.",
        ));
    }
    Ok(())
}

pub(super) async fn check_platform() -> Result<(), CoreError> {
    #[cfg(target_os = "macos")]
    {
        let mut command = tokio::process::Command::new("/usr/bin/sw_vers");
        command.arg("-productVersion");
        check_macos_version(&install::command(command, 5).await?)?;
    }
    Ok(())
}

/// Large model transfers never collect their body in RAM or impose a total
/// duration limit. The client still detects stalled connections/read periods.
pub(super) async fn download_checked(
    client: &reqwest::Client,
    url: &str,
    digest: &str,
    expected: Option<u64>,
    limit: u64,
    destination: &Path,
    progress: &(impl Fn(DownloadProgress) + Sync),
) -> Result<(), CoreError> {
    let mut response = client.get(url).send().await.map_err(|_| {
        error("Falha ao conectar o download audiovisual. Verifique a internet e tente novamente.")
    })?;
    if !response.status().is_success() {
        return Err(error(format!(
            "Download audiovisual indisponível (HTTP {}).",
            response.status().as_u16()
        )));
    }
    let total = expected.or(response.content_length());
    if response.content_length().is_some_and(|size| size > limit)
        || expected
            .zip(response.content_length())
            .is_some_and(|(a, b)| a != b)
    {
        return Err(error(
            "O tamanho do modelo não corresponde à versão registrada.",
        ));
    }
    let parent = destination
        .parent()
        .ok_or_else(|| error("Destino de download inválido."))?;
    fs::create_dir_all(parent)?;
    let temporary = tempfile::NamedTempFile::new_in(parent)?;
    let mut file = tokio::fs::File::from_std(temporary.reopen()?);
    let mut hasher = Sha256::new();
    let mut received = 0_u64;
    let mut updated = Instant::now();
    progress(DownloadProgress {
        received_bytes: 0,
        total_bytes: total,
    });
    while let Some(chunk) = response.chunk().await.map_err(|_| {
        error("Download audiovisual interrompido. Verifique a internet e tente novamente.")
    })? {
        received = received
            .checked_add(chunk.len() as u64)
            .filter(|size| *size <= limit)
            .ok_or_else(|| error("O download excedeu o tamanho registrado."))?;
        hasher.update(&chunk);
        file.write_all(&chunk).await?;
        if updated.elapsed() >= Duration::from_millis(250) {
            progress(DownloadProgress {
                received_bytes: received,
                total_bytes: total,
            });
            updated = Instant::now();
        }
    }
    if expected.is_some_and(|size| size != received) || format!("{:x}", hasher.finalize()) != digest
    {
        return Err(error(
            "Checksum do modelo audiovisual divergente. O download incompleto foi descartado.",
        ));
    }
    file.sync_all().await?;
    drop(file);
    temporary
        .persist(destination)
        .map_err(|_| error("Não foi possível salvar o modelo audiovisual."))?;
    progress(DownloadProgress {
        received_bytes: received,
        total_bytes: total,
    });
    Ok(())
}
pub(super) async fn matches_file(path: &Path, size: u64, digest: &str) -> Result<bool, CoreError> {
    if !tokio::fs::symlink_metadata(path)
        .await
        .is_ok_and(|meta| meta.is_file() && meta.len() == size)
    {
        return Ok(false);
    }
    let mut file = tokio::fs::File::open(path).await?;
    // This buffer survives an await and is embedded in every core's installer
    // future. Heap storage leaves room for Tauri's command frames on Windows.
    let mut buffer = vec![0u8; 64 * 1024];
    let mut hash = Sha256::new();
    loop {
        let size = file.read(&mut buffer).await?;
        if size == 0 {
            break;
        }
        hash.update(&buffer[..size]);
    }
    Ok(format!("{:x}", hash.finalize()) == digest)
}

pub(super) async fn install(
    home: &Path,
    destination: &Path,
    stage: &(impl Fn(&str) + Sync),
    progress: &(impl Fn(DownloadProgress) + Sync),
) -> Result<Vec<String>, CoreError> {
    check_platform().await?;
    let client = reqwest::Client::builder()
        .user_agent("Jarvis-Audiovisual/1.0")
        .connect_timeout(Duration::from_secs(8))
        .read_timeout(Duration::from_secs(30))
        .build()
        .map_err(|_| error("Não foi possível iniciar o download audiovisual."))?;
    let (platform, digest) = python_asset()?;
    stage("Baixando Python privado");
    let archive = destination.join("python.tar.gz");
    let name = format!("cpython-{PYTHON_VERSION}+{PYTHON_RELEASE}-{platform}-install_only.tar.gz");
    download_checked(&client, &format!("https://github.com/astral-sh/python-build-standalone/releases/download/{PYTHON_RELEASE}/{name}"), digest, None, 160 * 1024 * 1024, &archive, progress).await?;
    let bytes = fs::read(&archive)?;
    let python = destination.join("python");
    tokio::task::spawn_blocking(move || install::unpack(bytes, &python, false, true))
        .await
        .map_err(|_| error("Não foi possível extrair o Python privado."))??;
    fs::remove_file(archive)?;
    let cache = super::root(home).join("cache/audiovisual");
    fs::create_dir_all(&cache)?;
    let runtime = Runtime::at(destination);
    for (action, requirements, target) in [
        (
            "narrate",
            include_str!("audiovisual/requirements_tts.txt"),
            "packages_tts",
        ),
        (
            "music",
            include_str!("audiovisual/requirements_music.txt"),
            "packages_music",
        ),
    ] {
        let requirement_path = destination.join(format!("{target}.txt"));
        fs::write(&requirement_path, requirements)?;
        stage(if action == "narrate" {
            "Preparando voz local"
        } else {
            "Preparando música local"
        });
        let mut command = pip(&runtime, home, target)?;
        command.args(["--requirement"]).arg(&requirement_path);
        if action == "music" {
            // The local version suffix selects the CPU wheel rather than
            // PyPI's Linux CUDA dependency tree; macOS wheels include MPS.
            command.args([
                if cfg!(target_os = "macos") {
                    "torch==2.2.2"
                } else {
                    "torch==2.2.2+cpu"
                },
                "--extra-index-url",
                "https://download.pytorch.org/whl/cpu",
            ]);
        }
        install::command_unbounded(command).await?;
    }
    let assets = assets()?;
    for (index, asset) in assets.models.iter().enumerate() {
        stage(&format!(
            "Baixando modelos de áudio ({}/{})",
            index + 1,
            assets.models.len()
        ));
        let cached = cache.join(&asset.sha256);
        if !matches_file(&cached, asset.size, &asset.sha256).await? {
            #[cfg(windows)]
            if let Ok(metadata) = fs::symlink_metadata(&cached) {
                let mut permissions = metadata.permissions();
                permissions.set_readonly(false);
                fs::set_permissions(&cached, permissions)?;
            }
            download_checked(
                &client,
                &asset.url,
                &asset.sha256,
                Some(asset.size),
                asset.size,
                &cached,
                progress,
            )
            .await?;
        }
        let target = destination.join("models").join(&asset.path);
        fs::create_dir_all(
            target
                .parent()
                .ok_or_else(|| error("Destino de modelo inválido."))?,
        )?;
        // Immutable content-addressed blobs can be shared without duplicating
        // gigabytes. Replacing a cache entry never changes a previous inode.
        let mut permissions = fs::metadata(&cached)?.permissions();
        permissions.set_readonly(true);
        fs::set_permissions(&cached, permissions)?;
        tokio::task::spawn_blocking(move || {
            if fs::hard_link(&cached, &target).is_err() {
                fs::copy(cached, target)?;
            }
            Ok::<_, std::io::Error>(())
        })
        .await
        .map_err(|_| error("Não foi possível preparar os modelos locais."))??;
    }
    fs::write(
        destination.join(ENTRY),
        include_str!("audiovisual/runner.py"),
    )?;
    fs::write(
        destination.join(RECEIPT),
        serde_json::to_vec(&Metadata {
            version: VERSION.into(),
            python: PYTHON_VERSION.into(),
            revision: assets.revision,
            music_license: "CC-BY-NC-4.0".into(),
        })
        .map_err(|_| error("Registro audiovisual inválido."))?,
    )?;
    stage("Validando voz e música offline");
    verify(destination, VERSION).await?;
    required_files(destination)
}
fn pip(runtime: &Runtime, home: &Path, target: &str) -> Result<tokio::process::Command, CoreError> {
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
            "--target",
        ])
        .arg(runtime.package.join(target))
        .arg("--cache-dir")
        .arg(super::root(home).join("cache/pip"))
        .current_dir(&runtime.package)
        .env("PYTHONNOUSERSITE", "1")
        .env_remove("PYTHONHOME")
        .env_remove("PYTHONPATH");
    Ok(command)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    pub(crate) async fn install_smoke_component(home: &Path, id: ComponentId) {
        install::install(home, id, |stage| eprintln!("{}: {stage}", id.key()), |_| {})
            .await
            .unwrap();
    }

    #[test]
    fn unsupported_macos_reports_minimum_before_model_downloads() {
        for supported in ["13", "13.0", "14.6.1", "26.0\n"] {
            assert!(check_macos_version(supported).is_ok(), "{supported}");
        }
        for unsupported in ["10.15.7", "11.7.10", "12.7.6"] {
            let failure = check_macos_version(unsupported).unwrap_err();
            assert!(failure.message.contains("macOS 13"));
            assert!(failure.message.contains("Nenhum modelo foi baixado"));
        }
        for malformed in ["", "unknown", "13.invalid", "13."] {
            assert!(check_macos_version(malformed).is_err(), "{malformed}");
        }
    }

    pub(crate) fn fixture(package: &Path) {
        fs::create_dir_all(package).unwrap();
        fs::write(
            package.join(RECEIPT),
            serde_json::to_vec(&Metadata {
                version: VERSION.into(),
                python: PYTHON_VERSION.into(),
                revision: assets().unwrap().revision,
                music_license: "CC-BY-NC-4.0".into(),
            })
            .unwrap(),
        )
        .unwrap();
        for relative in required_files(package).unwrap() {
            let path = package.join(&relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            if relative != RECEIPT {
                fs::write(path, "fixture").unwrap();
            }
        }
        for model in assets().unwrap().models {
            fs::File::create(package.join("models").join(model.path))
                .unwrap()
                .set_len(model.size)
                .unwrap();
        }
    }

    #[test]
    fn runtime_requires_complete_pinned_private_assets_and_isolates_environments() {
        let package = tempfile::tempdir().unwrap();
        fixture(package.path());
        assert!(validate(package.path(), VERSION).is_ok());
        let runtime = Runtime::at(package.path());
        let narration = runtime.environment("narrate").unwrap();
        let music = runtime.environment("music").unwrap();
        assert!(narration["PYTHONPATH"].ends_with("packages_tts"));
        assert!(music["PYTHONPATH"].ends_with("packages_music"));
        assert_eq!(music["HF_HUB_OFFLINE"], "1");
        assert_eq!(narration["PYTHONNOUSERSITE"], "1");
        assert_eq!(narration["JOBLIB_MULTIPROCESSING"], "0");
        assert!(runtime.environment("unknown").is_err());
        fs::write(
            package.path().join("models/kokoro/voices-v1.0.bin"),
            "partial",
        )
        .unwrap();
        assert!(validate(package.path(), VERSION).is_err());
    }

    #[test]
    fn refreshes_only_old_runner_without_reinstalling_or_rewriting_current_runtime() {
        let package = tempfile::tempdir().unwrap();
        fixture(package.path());
        let entry = package.path().join(ENTRY);
        let dependency = package.path().join("packages_tts/kokoro_onnx/__init__.py");
        let model = package.path().join("models/kokoro/kokoro-v1.0.onnx");
        let dependency_before = fs::read(&dependency).unwrap();
        let model_before = fs::metadata(&model).unwrap().modified().unwrap();
        refresh_runner(package.path()).unwrap();
        assert_eq!(
            fs::read(&entry).unwrap(),
            include_bytes!("audiovisual/runner.py")
        );
        assert_eq!(fs::read(&dependency).unwrap(), dependency_before);
        assert_eq!(
            fs::metadata(&model).unwrap().modified().unwrap(),
            model_before
        );
        assert!(validate(package.path(), VERSION).is_ok());
        let old_time = std::time::UNIX_EPOCH + Duration::from_secs(1);
        fs::OpenOptions::new()
            .write(true)
            .open(&entry)
            .unwrap()
            .set_times(fs::FileTimes::new().set_modified(old_time))
            .unwrap();
        refresh_runner(package.path()).unwrap();
        assert_eq!(fs::metadata(&entry).unwrap().modified().unwrap(), old_time);
    }

    #[cfg(unix)]
    #[test]
    fn runner_refresh_never_follows_a_symlink_outside_the_private_runtime() {
        let package = tempfile::tempdir().unwrap();
        fixture(package.path());
        let outside = tempfile::NamedTempFile::new().unwrap();
        fs::write(outside.path(), "external script").unwrap();
        let entry = package.path().join(ENTRY);
        fs::remove_file(&entry).unwrap();
        std::os::unix::fs::symlink(outside.path(), &entry).unwrap();
        assert!(refresh_runner(package.path()).is_err());
        assert_eq!(
            fs::read_to_string(outside.path()).unwrap(),
            "external script"
        );
        assert!(entry.is_symlink());
    }

    #[cfg(unix)]
    #[test]
    fn runtime_rejects_model_symlinks_outside_its_private_generation() {
        let package = tempfile::tempdir().unwrap();
        let outside = tempfile::NamedTempFile::new().unwrap();
        fixture(package.path());
        let path = package.path().join("models/musicgen/config.json");
        fs::remove_file(&path).unwrap();
        std::os::unix::fs::symlink(outside.path(), path).unwrap();
        assert!(validate(package.path(), VERSION).is_err());
    }

    async fn served(bytes: &'static [u8]) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut connection, _) = listener.accept().await.unwrap();
            let mut request = [0u8; 1024];
            assert!(connection.read(&mut request).await.unwrap() > 0);
            connection
                .write_all(
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        bytes.len()
                    )
                    .as_bytes(),
                )
                .await
                .unwrap();
            connection.write_all(bytes).await.unwrap();
        });
        format!("http://{address}/model")
    }

    #[tokio::test]
    async fn streamed_model_download_publishes_only_matching_length_and_checksum() {
        let directory = tempfile::tempdir().unwrap();
        let target = directory.path().join("model");
        let bytes = b"validated model";
        let hash = format!("{:x}", Sha256::digest(bytes));
        download_checked(
            &reqwest::Client::new(),
            &served(bytes).await,
            &hash,
            Some(bytes.len() as u64),
            100,
            &target,
            &|_| {},
        )
        .await
        .unwrap();
        assert_eq!(fs::read(&target).unwrap(), bytes);
        assert!(matches_file(&target, bytes.len() as u64, &hash)
            .await
            .unwrap());
        assert!(download_checked(
            &reqwest::Client::new(),
            &served(b"wrong model").await,
            &hash,
            None,
            100,
            &target,
            &|_| {}
        )
        .await
        .is_err());
        assert_eq!(fs::read(&target).unwrap(), bytes);
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
        assert!(download_checked(
            &reqwest::Client::new(),
            &served(bytes).await,
            &hash,
            Some(1),
            100,
            &target,
            &|_| {}
        )
        .await
        .is_err());
    }

    #[tokio::test]
    async fn cancelled_installation_never_starts_network_or_changes_existing_manifest() {
        let home = tempfile::tempdir().unwrap();
        super::super::save_manifest(home.path(), &super::super::Manifest::default()).unwrap();
        let path = super::super::root(home.path()).join("manifest.json");
        let previous = fs::read(&path).unwrap();
        let (_sender, signal) = tokio::sync::watch::channel(true);
        let failure = install::install_cancellable(
            home.path(),
            ComponentId::Audiovisual,
            |_| panic!("A cancelled installation must not start"),
            |_| {},
            signal,
        )
        .await
        .unwrap_err();
        assert_eq!(failure.code, "cancelled");
        assert_eq!(fs::read(path).unwrap(), previous);
    }

    #[tokio::test]
    #[ignore = "Downloads managed Python, private inference wheels and pinned audio models"]
    async fn managed_installation_smoke() {
        let home = std::env::var_os("JARVIS_TEST_AUDIOVISUAL_HOME")
            .expect("Set JARVIS_TEST_AUDIOVISUAL_HOME to an isolated test home");
        let home = Path::new(&home);
        let version = install::install(
            home,
            ComponentId::Audiovisual,
            |stage| eprintln!("Audiovisual: {stage}"),
            |_| {},
        )
        .await
        .unwrap();
        let runtime = runtime(home).unwrap();
        assert_eq!(version, VERSION);
        verify(&runtime.package, VERSION).await.unwrap();
        eprintln!("AUDIOVISUAL_RUNTIME={}", runtime.package.display());
    }
}
