//! One managed production package: upstream tools, direction, audio and rendering.
//! Cloud keys and optional model downloads remain explicit user configuration.
mod animation;
pub mod board;
pub mod configuration;
pub(crate) use board::{open_board, show_board, BacklotState};

use super::{
    audiovisual, error, hyperframes, install, installed, relative, ComponentId, CoreError,
    DownloadProgress,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

pub(super) const VERSION: &str = "1.0.0";
pub(super) const REVISION: &str = "9327439db69021ab4b0e2776729bf3b58fdb5a87";
const SOURCE_SHA256: &str = "bb560fa9b7a1ef3e349412507f60241ed96ada0d6bfa4b9dab17201484391ab9";
pub(super) const HYPERFRAMES_VERSION: &str = "0.8.140";
const RECEIPT: &str = "jarvis-openmontage.json";
const REPOSITORY: &str = "OpenMontage";
const BRIDGE: &str = "jarvis_bridge.py";
const SOURCE_FILES: &[&str] = &[
    "AGENT_GUIDE.md",
    "LICENSE",
    "config.yaml",
    "requirements.txt",
    "requirements-gpu.txt",
    "pipeline_defs/screen-demo.yaml",
    "pipeline_defs/documentary-montage.yaml",
    "tools/tool_registry.py",
    "tools/base_tool.py",
    "lib/checkpoint.py",
    "lib/config_model.py",
    "skills/meta/creative-intake.md",
    "skills/meta/reviewer.md",
    "remotion-composer/package.json",
    "remotion-composer/src/index.tsx",
];

#[derive(Deserialize, Serialize)]
struct Receipt {
    version: String,
    revision: String,
    source_sha256: String,
    python: String,
    renderer: String,
    sources: BTreeMap<String, String>,
}

pub(crate) struct Runtime {
    pub python: PathBuf,
    pub package: PathBuf,
    pub bridge: PathBuf,
    pub environment: BTreeMap<String, String>,
}

pub(crate) fn runtime(home: &Path) -> Result<Runtime, CoreError> {
    let record = installed(home, ComponentId::Openmontage)?;
    let generation = record.path(home)?;
    let mut runtime = Runtime::at(&generation)?;
    configuration::apply_environment(home, &mut runtime.environment)?;
    let configured = super::root(home).join("openmontage/config.yaml");
    let config = if configured.is_file() {
        configured
    } else {
        runtime.package.join("config.yaml")
    };
    runtime.environment.insert(
        "OPENMONTAGE_CONFIG".into(),
        config.to_string_lossy().into_owned(),
    );
    runtime.environment.insert(
        "HF_HOME".into(),
        super::root(home)
            .join("cache/openmontage/huggingface")
            .to_string_lossy()
            .into_owned(),
    );
    Ok(runtime)
}

pub(super) fn python_path(generation: &Path) -> PathBuf {
    generation.join(if cfg!(windows) {
        "venv/Scripts/python.exe"
    } else {
        "venv/bin/python"
    })
}

fn base_python(generation: &Path) -> PathBuf {
    audiovisual::python_path(generation)
}

impl Runtime {
    fn at(generation: &Path) -> Result<Self, CoreError> {
        let renderer = hyperframes::Runtime::at(generation)?;
        let python = python_path(generation);
        let package = generation.join(REPOSITORY);
        let mut environment = renderer.environment;
        let node_directory = install::node_path(generation)
            .parent()
            .ok_or_else(|| error("Runtime Node inválido."))?
            .to_path_buf();
        let mut paths = vec![
            python
                .parent()
                .ok_or_else(|| error("Python privado inválido."))?
                .to_path_buf(),
            generation.join("bin"),
            node_directory,
            generation.join("node_modules/.bin"),
            package.join("remotion-composer/node_modules/.bin"),
        ];
        if let Some(animation) = animation::runtime(generation).ok().flatten() {
            // Keep native Cairo/Pango outside the production package's pip environment.
            paths.insert(1, animation.bin);
            for (key, path) in [
                ("JARVIS_OPENMONTAGE_MANIM_MANAGER", animation.manager),
                ("JARVIS_OPENMONTAGE_MANIM_PREFIX", animation.prefix),
                ("JARVIS_OPENMONTAGE_MANIM_PYTHON", animation.python),
            ] {
                environment.insert(key.into(), path.to_string_lossy().into_owned());
            }
        }
        paths.extend(std::env::split_paths(
            &std::env::var_os("PATH").unwrap_or_default(),
        ));
        environment.insert(
            "PATH".into(),
            std::env::join_paths(paths)
                .map_err(|_| error("Caminho do OpenMontage inválido."))?
                .to_string_lossy()
                .into_owned(),
        );
        for (key, value) in [
            ("PYTHONPATH", package.to_string_lossy().into_owned()),
            ("PYTHONHOME", String::new()),
            ("PYTHONNOUSERSITE", "1".into()),
            ("PYTHONUTF8", "1".into()),
            ("PYTHONIOENCODING", "utf-8".into()),
            (
                "VIRTUAL_ENV",
                generation.join("venv").to_string_lossy().into_owned(),
            ),
            ("npm_config_offline", "true".into()),
            (
                "npm_config_cache",
                generation.join("npm-cache").to_string_lossy().into_owned(),
            ),
            ("HYPERFRAMES_MANAGED_VERSION", HYPERFRAMES_VERSION.into()),
            (
                "JARVIS_OPENMONTAGE_NODE",
                install::node_path(generation)
                    .to_string_lossy()
                    .into_owned(),
            ),
            (
                "JARVIS_OPENMONTAGE_NPX",
                generation
                    .join("bin/jarvis-npx.cjs")
                    .to_string_lossy()
                    .into_owned(),
            ),
            (
                "JARVIS_OPENMONTAGE_NPM",
                install::npm_path(generation).to_string_lossy().into_owned(),
            ),
            (
                "REMOTION_BROWSER_EXECUTABLE",
                environment
                    .get("HYPERFRAMES_BROWSER_PATH")
                    .cloned()
                    .unwrap_or_default(),
            ),
            ("JARVIS_OPENMONTAGE_ALLOW_PAID", "0".into()),
            ("JARVIS_OPENMONTAGE_ALLOW_MODEL_DOWNLOADS", "0".into()),
            ("HF_HUB_OFFLINE", "1".into()),
            ("TRANSFORMERS_OFFLINE", "1".into()),
        ] {
            environment.insert(key.into(), value);
        }
        for key in configuration::CREDENTIAL_KEYS.split_whitespace() {
            environment.insert(key.into(), String::new());
        }
        for key in [
            "HOME",
            "USERPROFILE",
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
                environment.insert(key.into(), value.to_string_lossy().into_owned());
            }
        }
        Ok(Self {
            python,
            bridge: package.join(BRIDGE),
            package,
            environment,
        })
    }
}

fn contained(generation: &Path, file: &str) -> Result<(), CoreError> {
    if !relative(file)
        || !generation.join(file).is_file()
        || !fs::canonicalize(generation.join(file))?.starts_with(fs::canonicalize(generation)?)
    {
        return Err(error(
            "Os arquivos do OpenMontage precisam permanecer no Core privado.",
        ));
    }
    Ok(())
}

pub(super) fn required_files(generation: &Path) -> Result<Vec<String>, CoreError> {
    let mut files = hyperframes::required_files(generation)?;
    files.extend(
        SOURCE_FILES
            .iter()
            .map(|file| format!("{REPOSITORY}/{file}")),
    );
    files.extend([
        RECEIPT.into(),
        "python-requirements.lock.txt".into(),
        "venv/pyvenv.cfg".into(),
        format!("{REPOSITORY}/{BRIDGE}"),
        format!("{REPOSITORY}/jarvis_board.py"),
        format!("{REPOSITORY}/remotion-composer/package-lock.json"),
        format!("{REPOSITORY}/remotion-composer/node_modules/@remotion/cli/package.json"),
        "bin/jarvis-npx.cjs".into(),
    ]);
    files.extend(if cfg!(windows) {
        ["bin/npm.cmd".into(), "bin/npx.cmd".into()]
    } else {
        ["bin/npm".into(), "bin/npx".into()]
    });
    for path in [python_path(generation), base_python(generation)] {
        files.push(
            path.strip_prefix(generation)
                .map_err(|_| error("Runtime fora do Core."))?
                .to_string_lossy()
                .into_owned(),
        );
    }
    Ok(files)
}

pub(super) fn validate(generation: &Path, version: &str) -> Result<(), CoreError> {
    let receipt: Receipt = serde_json::from_slice(&fs::read(generation.join(RECEIPT))?)
        .map_err(|_| error("Registro do OpenMontage inválido."))?;
    if version != VERSION
        || receipt.version != version
        || receipt.revision != REVISION
        || receipt.source_sha256 != SOURCE_SHA256
        || receipt.python != audiovisual::PYTHON_VERSION
        || receipt.renderer != HYPERFRAMES_VERSION
        || SOURCE_FILES
            .iter()
            .any(|file| !receipt.sources.contains_key(*file))
    {
        return Err(error(
            "O contrato do OpenMontage mudou. Atualize o componente no Core.",
        ));
    }
    hyperframes::validate(generation, HYPERFRAMES_VERSION)?;
    for file in required_files(generation)? {
        contained(generation, &file)?;
    }
    for file in receipt.sources.keys() {
        contained(generation, &format!("{REPOSITORY}/{file}"))?;
    }
    for file in SOURCE_FILES {
        let digest = format!(
            "{:x}",
            Sha256::digest(fs::read(generation.join(REPOSITORY).join(file))?)
        );
        if receipt.sources.get(*file) != Some(&digest) {
            return Err(error(
                "Recursos do OpenMontage foram alterados. Reinstale o componente.",
            ));
        }
    }
    if fs::read(generation.join(REPOSITORY).join(BRIDGE))?
        != include_bytes!("openmontage_bridge.py")
    {
        return Err(error(
            "A integração do OpenMontage mudou. Repare o componente no Core.",
        ));
    }
    if fs::read(generation.join(REPOSITORY).join("jarvis_board.py"))?
        != include_bytes!("openmontage/board-runner.py")
        || fs::read(generation.join("bin/jarvis-npx.cjs"))?
            != include_bytes!("openmontage/node-tools.cjs")
    {
        return Err(error(
            "A integração visual do OpenMontage mudou. Repare o componente no Core.",
        ));
    }
    Ok(())
}

pub(super) async fn verify(generation: &Path, version: &str) -> Result<(), CoreError> {
    validate(generation, version)?;
    let receipt: Receipt = serde_json::from_slice(&fs::read(generation.join(RECEIPT))?)
        .map_err(|_| error("Registro do OpenMontage inválido."))?;
    for (file, expected) in receipt.sources {
        let digest = format!(
            "{:x}",
            Sha256::digest(fs::read(generation.join(REPOSITORY).join(file))?)
        );
        if digest != expected {
            return Err(error(
                "A integridade dos recursos do OpenMontage diverge. Reinstale o componente.",
            ));
        }
    }
    hyperframes::verify(generation, HYPERFRAMES_VERSION).await?;
    let runtime = Runtime::at(generation)?;
    let mut command = tokio::process::Command::new(&runtime.python);
    command.args(["-c", "import sys,yaml,pydantic,jsonschema,PIL,numpy,requests,google.auth,google.genai,openai,fastapi,uvicorn,watchfiles;from tools.tool_registry import ToolRegistry;assert sys.version.split()[0]=='3.11.16';print('OpenMontage pronto')"])
        .env_clear().envs(&runtime.environment).current_dir(&runtime.package);
    install::command(command, 40).await?;
    Ok(())
}

#[cfg(unix)]
pub(super) fn executable_paths(generation: &Path) -> Result<Vec<PathBuf>, CoreError> {
    let mut paths = hyperframes::executable_paths(generation)?;
    paths.extend([
        base_python(generation),
        python_path(generation),
        generation.join("bin/npm"),
        generation.join("bin/npx"),
    ]);
    Ok(paths)
}

pub(super) fn repair_bridge(generation: &Path) -> Result<(), CoreError> {
    fs::write(
        generation.join(REPOSITORY).join(BRIDGE),
        include_bytes!("openmontage_bridge.py"),
    )?;
    fs::write(
        generation.join(REPOSITORY).join("jarvis_board.py"),
        include_bytes!("openmontage/board-runner.py"),
    )?;
    aliases(generation)?;
    Ok(())
}

pub(super) fn relocate_venv(from: &Path, to: &Path) -> Result<(), CoreError> {
    let config = from.join("venv/pyvenv.cfg");
    let text = fs::read_to_string(&config)?.replace(
        from.to_string_lossy().as_ref(),
        to.to_string_lossy().as_ref(),
    );
    fs::write(config, text)?;
    #[cfg(unix)]
    for entry in fs::read_dir(from.join("venv/bin"))? {
        let path = entry?.path();
        if path.is_file() {
            if let Ok(text) = fs::read_to_string(&path) {
                if text.starts_with("#!") && text.contains(from.to_string_lossy().as_ref()) {
                    fs::write(
                        path,
                        text.replace(
                            from.to_string_lossy().as_ref(),
                            to.to_string_lossy().as_ref(),
                        ),
                    )?;
                }
            }
        }
    }
    Ok(())
}

fn aliases(generation: &Path) -> Result<(), CoreError> {
    fs::create_dir_all(generation.join("bin"))?;
    fs::write(
        generation.join("bin/jarvis-npx.cjs"),
        include_bytes!("openmontage/node-tools.cjs"),
    )?;
    #[cfg(unix)]
    for (name, entry) in [
        ("npm", "runtime/lib/node_modules/npm/bin/npm-cli.js"),
        ("npx", "bin/jarvis-npx.cjs"),
    ] {
        use std::os::unix::fs::PermissionsExt;
        let path = generation.join("bin").join(name);
        fs::write(&path, format!("#!/bin/sh\njarvis_runtime=$(CDPATH= cd -- \"$(dirname -- \"$0\")/..\" && pwd)\nexec \"$jarvis_runtime/runtime/bin/node\" \"$jarvis_runtime/{entry}\" \"$@\"\n"))?;
        fs::set_permissions(path, fs::Permissions::from_mode(0o755))?;
    }
    #[cfg(windows)]
    {
        fs::write(
            generation.join("bin/npx.cmd"),
            "@\"%~dp0..\\runtime\\node.exe\" \"%~dp0jarvis-npx.cjs\" %*\r\n",
        )?;
        fs::write(generation.join("bin/npm.cmd"), "@\"%~dp0..\\runtime\\node.exe\" \"%~dp0..\\runtime\\node_modules\\npm\\bin\\npm-cli.js\" %*\r\n")?;
    }
    Ok(())
}

pub(super) async fn install(
    home: &Path,
    generation: &Path,
    stage: &(impl Fn(&str) + Sync),
    progress: &(impl Fn(DownloadProgress) + Sync),
) -> Result<Vec<String>, CoreError> {
    stage("Baixando pacote completo OpenMontage");
    let client = reqwest::Client::builder()
        .user_agent("Jarvis-OpenMontage/1.0")
        .connect_timeout(Duration::from_secs(8))
        .read_timeout(Duration::from_secs(30))
        .build()
        .map_err(|_| error("Não foi possível iniciar o download de vídeo."))?;
    let archive = generation.join("openmontage.tar.gz");
    audiovisual::download_checked(
        &client,
        &format!("https://codeload.github.com/calesthio/OpenMontage/tar.gz/{REVISION}"),
        SOURCE_SHA256,
        None,
        512 * 1024 * 1024,
        &archive,
        progress,
    )
    .await?;
    let bytes = fs::read(&archive)?;
    let repository = generation.join(REPOSITORY);
    let destination = repository.clone();
    tokio::task::spawn_blocking(move || install::unpack(bytes, &destination, false, true))
        .await
        .map_err(|_| error("Falha ao extrair o OpenMontage."))??;
    fs::remove_file(archive)?;
    stage("Preparando Python privado do OpenMontage");
    let (platform, digest) = audiovisual::python_asset()?;
    let archive = generation.join("python.tar.gz");
    let filename = format!(
        "cpython-{}+{}-{platform}-install_only.tar.gz",
        audiovisual::PYTHON_VERSION,
        audiovisual::PYTHON_RELEASE
    );
    audiovisual::download_checked(
        &client,
        &format!(
            "https://github.com/astral-sh/python-build-standalone/releases/download/{}/{filename}",
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
    let destination = generation.join("python");
    tokio::task::spawn_blocking(move || install::unpack(bytes, &destination, false, true))
        .await
        .map_err(|_| error("Falha ao extrair o Python do OpenMontage."))??;
    fs::remove_file(archive)?;
    let mut command = tokio::process::Command::new(base_python(generation));
    command
        .args(["-m", "venv", "--copies"])
        .arg(generation.join("venv"))
        .env_remove("PYTHONHOME")
        .env_remove("PYTHONPATH");
    install::command(command, 90).await?;
    stage("Instalando todas as dependências base do OpenMontage");
    let mut command = pip(generation, home);
    command
        .arg("--requirement")
        .arg(repository.join("requirements.txt"));
    install::command_unbounded(command).await?;
    let mut command = tokio::process::Command::new(python_path(generation));
    command
        .args(["-m", "pip", "--isolated", "freeze"])
        .env_remove("PYTHONHOME")
        .env_remove("PYTHONPATH");
    fs::write(
        generation.join("python-requirements.lock.txt"),
        install::command(command, 30).await?,
    )?;
    stage("Preparando os renderizadores do OpenMontage");
    install::install_video_runtime(
        home,
        generation,
        HYPERFRAMES_VERSION,
        false,
        stage,
        progress,
    )
    .await?;
    aliases(generation)?;
    stage("Instalando compositor Remotion");
    let composer = repository.join("remotion-composer");
    let mut command = tokio::process::Command::new(install::node_path(generation));
    command
        .arg(install::npm_path(generation))
        .args([
            "install",
            "--ignore-scripts",
            "--no-audit",
            "--no-fund",
            "--package-lock=true",
            "--global=false",
            "--workspaces=false",
            "--registry=https://registry.npmjs.org",
        ])
        .arg("--prefix")
        .arg(&composer)
        .current_dir(&composer)
        .env("NODE_OPTIONS", "")
        .env("npm_config_cache", super::root(home).join("cache/npm"))
        .env("npm_config_userconfig", generation.join("empty.npmrc"));
    install::command_unbounded(command).await?;
    repair_bridge(generation)?;
    let mut sources = BTreeMap::new();
    for directory in [
        "tools",
        "lib",
        "skills",
        "pipeline_defs",
        "schemas",
        ".agents/skills",
        "assets",
        "ink-theater",
        "styles",
        "library",
        "backlot",
        "remotion-composer/src",
    ] {
        collect_sources(&repository, &repository.join(directory), &mut sources)?;
    }
    for file in SOURCE_FILES {
        sources.insert(
            (*file).into(),
            format!("{:x}", Sha256::digest(fs::read(repository.join(file))?)),
        );
    }
    fs::write(
        generation.join(RECEIPT),
        serde_json::to_vec(&Receipt {
            version: VERSION.into(),
            revision: REVISION.into(),
            source_sha256: SOURCE_SHA256.into(),
            python: audiovisual::PYTHON_VERSION.into(),
            renderer: HYPERFRAMES_VERSION.into(),
            sources,
        })
        .map_err(|_| error("Registro do OpenMontage inválido."))?,
    )?;
    stage("Validando catálogo, direção e renderização");
    verify(generation, VERSION).await?;
    required_files(generation)
}

fn collect_sources(
    repository: &Path,
    directory: &Path,
    files: &mut BTreeMap<String, String>,
) -> Result<(), CoreError> {
    if !directory.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();
        if entry.file_type()?.is_dir() {
            if entry.file_name() != "__pycache__" {
                collect_sources(repository, &path, files)?;
            }
        } else if entry.file_type()?.is_file() {
            let relative = path
                .strip_prefix(repository)
                .map_err(|_| error("Recurso fora do OpenMontage."))?
                .to_string_lossy()
                .replace('\\', "/");
            files.insert(relative, format!("{:x}", Sha256::digest(fs::read(path)?)));
        }
    }
    Ok(())
}

pub(super) fn pip(generation: &Path, home: &Path) -> tokio::process::Command {
    let mut command = tokio::process::Command::new(python_path(generation));
    command
        .args([
            "-m",
            "pip",
            "--isolated",
            "install",
            "--disable-pip-version-check",
            "--no-input",
            "--no-compile",
            "--index-url",
            "https://pypi.org/simple",
            "--cache-dir",
        ])
        .arg(super::root(home).join("cache/pip"))
        .current_dir(generation)
        .env_remove("PYTHONHOME")
        .env_remove("PYTHONPATH")
        .env("PYTHONNOUSERSITE", "1");
    command
}

#[cfg(test)]
pub(crate) mod tests;
