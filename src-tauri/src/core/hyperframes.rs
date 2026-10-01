//! Private runtime used by Hyperframes commands and local health checks.
use super::{error, install, installed, relative, ComponentId, CoreError};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

pub(super) const ENTRY: &str = "node_modules/hyperframes/bin/hyperframes.mjs";
pub(super) const PACKAGE: &str = "node_modules/hyperframes/package.json";
pub(super) const PATHS: &str = "jarvis-hyperframes.json";
pub(crate) const GSAP: &str = "node_modules/gsap/dist/gsap.min.js";
pub(super) const GSAP_VERSION: &str = "3.14.2";
const DOCS_RECEIPT: &str = "jarvis-hyperframes-docs.json";
const DOCS_LICENSE: &str = "jarvis-hyperframes-docs-LICENSE";
// The npm CLI ships only three skills. Preserve the official split contracts
// and their Markdown references from the CLI release's immutable git revision.
const DOCUMENTS: &[(&str, &str)] = &[
    (
        "hyperframes-core/SKILL.md",
        include_str!("hyperframes-docs/hyperframes-core/SKILL.md"),
    ),
    (
        "hyperframes-core/references/composition-patterns.md",
        include_str!("hyperframes-docs/hyperframes-core/references/composition-patterns.md"),
    ),
    (
        "hyperframes-core/references/creator-editing-recipes.md",
        include_str!("hyperframes-docs/hyperframes-core/references/creator-editing-recipes.md"),
    ),
    (
        "hyperframes-core/references/data-attributes.md",
        include_str!("hyperframes-docs/hyperframes-core/references/data-attributes.md"),
    ),
    (
        "hyperframes-core/references/determinism-rules.md",
        include_str!("hyperframes-docs/hyperframes-core/references/determinism-rules.md"),
    ),
    (
        "hyperframes-core/references/full-screen-motion.md",
        include_str!("hyperframes-docs/hyperframes-core/references/full-screen-motion.md"),
    ),
    (
        "hyperframes-core/references/minimal-composition.md",
        include_str!("hyperframes-docs/hyperframes-core/references/minimal-composition.md"),
    ),
    (
        "hyperframes-core/references/sub-compositions.md",
        include_str!("hyperframes-docs/hyperframes-core/references/sub-compositions.md"),
    ),
    (
        "hyperframes-core/references/tailwind.md",
        include_str!("hyperframes-docs/hyperframes-core/references/tailwind.md"),
    ),
    (
        "hyperframes-core/references/tracks-and-clips.md",
        include_str!("hyperframes-docs/hyperframes-core/references/tracks-and-clips.md"),
    ),
    (
        "hyperframes-core/references/variables-and-media.md",
        include_str!("hyperframes-docs/hyperframes-core/references/variables-and-media.md"),
    ),
    (
        "hyperframes-audio/SKILL.md",
        include_str!("hyperframes-docs/hyperframes-audio/SKILL.md"),
    ),
    (
        "hyperframes-audio/references/attributes.md",
        include_str!("hyperframes-docs/hyperframes-audio/references/attributes.md"),
    ),
    (
        "hyperframes-audio/references/diagnosis.md",
        include_str!("hyperframes-docs/hyperframes-audio/references/diagnosis.md"),
    ),
    (
        "hyperframes-audio/references/fx-registry.md",
        include_str!("hyperframes-docs/hyperframes-audio/references/fx-registry.md"),
    ),
    (
        "hyperframes-audio/references/presets.md",
        include_str!("hyperframes-docs/hyperframes-audio/references/presets.md"),
    ),
];

pub(super) fn install_documentation(package: &Path) -> Result<(), CoreError> {
    for &(relative, content) in DOCUMENTS {
        let path = package
            .join("node_modules/hyperframes/dist/skills")
            .join(relative);
        fs::create_dir_all(
            path.parent()
                .ok_or_else(|| error("Documentação inválida."))?,
        )?;
        fs::write(path, content)?;
    }
    fs::write(
        package.join(DOCS_RECEIPT),
        include_str!("hyperframes-docs/provenance.json"),
    )?;
    fs::write(
        package.join(DOCS_LICENSE),
        include_str!("hyperframes-docs/LICENSE"),
    )?;
    Ok(())
}
pub(super) const SKILLS: [&str; 5] = [
    "node_modules/hyperframes/dist/skills/hyperframes/SKILL.md",
    "node_modules/hyperframes/dist/skills/hyperframes-cli/SKILL.md",
    "node_modules/hyperframes/dist/skills/media-use/SKILL.md",
    "node_modules/hyperframes/dist/skills/hyperframes-core/SKILL.md",
    "node_modules/hyperframes/dist/skills/hyperframes-audio/SKILL.md",
];

#[derive(Deserialize, Serialize)]
pub(super) struct Binaries {
    pub browser: String,
    pub ffmpeg: String,
    pub ffprobe: String,
}
impl Binaries {
    pub(super) fn read(package: &Path) -> Result<Self, CoreError> {
        serde_json::from_slice(&fs::read(package.join(PATHS))?).map_err(|_| {
            error("Os caminhos do Hyperframes estão inválidos. Reinstale o componente.")
        })
    }

    pub(super) fn files(&self) -> [&str; 3] {
        [&self.browser, &self.ffmpeg, &self.ffprobe]
    }
}

pub(crate) struct Runtime {
    pub node: PathBuf,
    pub entry: PathBuf,
    pub package: PathBuf,
    pub environment: BTreeMap<String, String>,
}

pub(crate) fn runtime(home: &Path) -> Result<Runtime, CoreError> {
    Runtime::at(&installed(home, ComponentId::Hyperframes)?.path(home)?)
}

impl Runtime {
    pub(super) fn at(package: &Path) -> Result<Self, CoreError> {
        let package = package.to_path_buf();
        let binaries = Binaries::read(&package)?;
        let base = fs::canonicalize(&package)?;
        for file in binaries.files() {
            if !relative(file)
                || !package.join(file).is_file()
                || !fs::canonicalize(package.join(file))?.starts_with(&base)
            {
                return Err(error(
                    "Os binários do Hyperframes devem permanecer na instalação privada.",
                ));
            }
        }
        let node = install::node_path(&package);
        let mut paths = vec![node
            .parent()
            .ok_or_else(|| error("Runtime Node inválido."))?
            .to_path_buf()];
        paths.extend(std::env::split_paths(
            &std::env::var_os("PATH").unwrap_or_default(),
        ));
        let path = std::env::join_paths(paths)
            .map_err(|_| error("Caminho do runtime Hyperframes inválido."))?;
        let browser = package
            .join(&binaries.browser)
            .to_string_lossy()
            .into_owned();
        let environment = BTreeMap::from([
            ("PATH".into(), path.to_string_lossy().into_owned()),
            ("NODE_OPTIONS".into(), String::new()),
            ("HYPERFRAMES_BROWSER_PATH".into(), browser.clone()),
            ("PRODUCER_HEADLESS_SHELL_PATH".into(), browser),
            (
                "HYPERFRAMES_FFMPEG_PATH".into(),
                package
                    .join(&binaries.ffmpeg)
                    .to_string_lossy()
                    .into_owned(),
            ),
            (
                "HYPERFRAMES_FFPROBE_PATH".into(),
                package
                    .join(&binaries.ffprobe)
                    .to_string_lossy()
                    .into_owned(),
            ),
            ("CI".into(), "true".into()),
            ("HYPERFRAMES_SKIP_SKILLS".into(), "1".into()),
            ("HYPERFRAMES_NO_UPDATE_CHECK".into(), "1".into()),
            ("HYPERFRAMES_NO_TELEMETRY".into(), "1".into()),
            ("NO_COLOR".into(), "1".into()),
        ]);
        Ok(Self {
            node,
            entry: package.join(ENTRY),
            package,
            environment,
        })
    }

    pub(super) fn command(&self) -> tokio::process::Command {
        let mut command = tokio::process::Command::new(&self.node);
        command
            .arg(&self.entry)
            .envs(&self.environment)
            .current_dir(&self.package);
        command
    }
}

pub(super) fn required_files(package: &Path) -> Result<Vec<String>, CoreError> {
    let mut files = vec![
        ENTRY.into(),
        PACKAGE.into(),
        PATHS.into(),
        "node_modules/hyperframes/dist/cli.js".into(),
        "node_modules/hyperframes/dist/templates/blank/index.html".into(),
        "node_modules/hyperframes/dist/hyperframes-player.global.js".into(),
        GSAP.into(),
        "node_modules/gsap/package.json".into(),
        DOCS_RECEIPT.into(),
        DOCS_LICENSE.into(),
    ];
    files.extend(SKILLS.map(String::from));
    files.extend(
        DOCUMENTS
            .iter()
            .map(|(relative, _)| format!("node_modules/hyperframes/dist/skills/{relative}")),
    );
    files.sort();
    files.dedup();
    files.extend(Binaries::read(package)?.files().map(String::from));
    files.push(
        install::node_path(package)
            .strip_prefix(package)
            .map_err(|_| error("Runtime Node inválido."))?
            .to_string_lossy()
            .into_owned(),
    );
    Ok(files)
}

#[cfg(unix)]
pub(super) fn executable_paths(package: &Path) -> Result<Vec<PathBuf>, CoreError> {
    let binaries = Binaries::read(package)?;
    let mut paths = vec![install::node_path(package)];
    paths.extend(binaries.files().map(|file| package.join(file)));
    Ok(paths)
}

pub(super) fn validate(package: &Path, version: &str) -> Result<(), CoreError> {
    let metadata: serde_json::Value = serde_json::from_slice(&fs::read(package.join(PACKAGE))?)
        .map_err(|_| error("Pacote Hyperframes inválido."))?;
    if metadata["name"] != "hyperframes" || metadata["version"] != version {
        return Err(error(
            "A versão instalada do Hyperframes diverge do registro.",
        ));
    }
    Runtime::at(package)?;
    for file in required_files(package)? {
        if !package.join(&file).is_file() {
            return Err(error(format!(
                "O pacote Hyperframes está incompleto: falta {file}. Reinstale o componente no Core."
            )));
        }
    }
    let gsap: serde_json::Value =
        serde_json::from_slice(&fs::read(package.join("node_modules/gsap/package.json"))?)
            .map_err(|_| error("Pacote GSAP inválido. Reinstale Hyperframes no Core."))?;
    if gsap["name"] != "gsap" || gsap["version"] != GSAP_VERSION {
        return Err(error(
            "A animação local do Hyperframes está desatualizada. Reinstale o componente no Core.",
        ));
    }
    if fs::read_to_string(package.join(DOCS_RECEIPT))?
        != include_str!("hyperframes-docs/provenance.json")
    {
        return Err(error("A documentação nativa do Hyperframes está desatualizada. Reinstale o componente no Core."));
    }
    Ok(())
}

fn verify_report(report: &str) -> Result<(), CoreError> {
    let report: serde_json::Value = serde_json::from_str(report)
        .map_err(|_| error("O diagnóstico do Hyperframes retornou uma resposta inválida."))?;
    let checks = report["checks"]
        .as_array()
        .ok_or_else(|| error("Diagnóstico do Hyperframes incompleto."))?;
    // Optional voice/music packages, Docker and update availability do not determine local rendering health.
    for name in ["Node.js", "FFmpeg", "FFprobe", "Chrome"] {
        if !checks
            .iter()
            .any(|check| check["name"] == name && check["ok"] == true)
        {
            return Err(error(format!(
                "O Hyperframes não conseguiu validar {name}. Reinstale o componente."
            )));
        }
    }
    Ok(())
}

pub(super) async fn verify(package: &Path, version: &str) -> Result<(), CoreError> {
    validate(package, version)?;
    let runtime = Runtime::at(package)?;
    let mut command = runtime.command();
    command.arg("--version");
    if install::command(command, 20).await?.trim() != version {
        return Err(error(
            "A versão do runtime Hyperframes diverge do registro.",
        ));
    }
    let mut command = runtime.command();
    command.args(["doctor", "--json"]);
    verify_report(&install::command(command, 60).await?)?;
    // Upstream doctor currently reports FFprobe as healthy even if its version probe fails.
    for name in ["FFMPEG", "FFPROBE"] {
        let mut command =
            tokio::process::Command::new(&runtime.environment[&format!("HYPERFRAMES_{name}_PATH")]);
        command.arg("-version");
        install::command(command, 10).await?;
    }
    let mut command =
        tokio::process::Command::new(&runtime.environment["HYPERFRAMES_BROWSER_PATH"]);
    command.arg("--version");
    install::command(command, 10).await?;
    let mut command = tokio::process::Command::new(&runtime.environment["HYPERFRAMES_FFMPEG_PATH"]);
    command.args(["-hide_banner", "-encoders"]);
    let encoders = install::command(command, 10).await?;
    if !encoders
        .split_whitespace()
        .any(|word| matches!(word, "libx264" | "h264_videotoolbox"))
    {
        return Err(error(
            "O FFmpeg gerenciado não oferece codificação H.264. Reinstale o componente.",
        ));
    }
    let mut command = tokio::process::Command::new(&runtime.environment["HYPERFRAMES_FFMPEG_PATH"]);
    command.args(["-hide_banner", "-h", "full"]);
    if !install::command(command, 10)
        .await?
        .split_whitespace()
        .any(|word| word == "-fps_mode")
    {
        return Err(error("O FFmpeg gerenciado não suporta a extração de vídeos do Hyperframes. Reinstale o componente."));
    }
    Ok(())
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;

    pub(in crate::core) fn fixture(package: &Path, version: &str) {
        let binaries = Binaries {
            browser: "browser/chrome".into(),
            ffmpeg: "bin/ffmpeg".into(),
            ffprobe: "bin/ffprobe".into(),
        };
        fs::write(package.join(PATHS), serde_json::to_vec(&binaries).unwrap()).unwrap();
        for file in required_files(package)
            .unwrap()
            .into_iter()
            .filter(|file| file != PATHS)
        {
            fs::create_dir_all(package.join(&file).parent().unwrap()).unwrap();
            fs::write(package.join(file), b"fixture").unwrap();
        }
        fs::write(
            package.join(PACKAGE),
            serde_json::json!({"name":"hyperframes","version":version}).to_string(),
        )
        .unwrap();
        fs::write(
            package.join("node_modules/gsap/package.json"),
            serde_json::json!({"name":"gsap","version":GSAP_VERSION}).to_string(),
        )
        .unwrap();
        install_documentation(package).unwrap();
    }

    #[test]
    fn split_official_contracts_and_references_are_installed_without_scripts() {
        let package = tempfile::tempdir().unwrap();
        fixture(package.path(), "0.8.105");
        let base = package.path().join("node_modules/hyperframes/dist/skills");
        let audio = fs::read_to_string(base.join("hyperframes-audio/SKILL.md")).unwrap();
        assert!(audio.contains("data-automation"));
        assert!(base
            .join("hyperframes-audio/references/fx-registry.md")
            .is_file());
        assert!(base
            .join("hyperframes-core/references/data-attributes.md")
            .is_file());
        assert!(!base.join("hyperframes-audio/scripts/carve.mjs").exists());
        let provenance: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(package.path().join(DOCS_RECEIPT)).unwrap())
                .unwrap();
        assert_eq!(
            provenance["revision"],
            "bc57e282fdde4afdccec1d1a2bacd94a9c3c5383"
        );
        assert_eq!(provenance["license"], "Apache-2.0");
        validate(package.path(), "0.8.105").unwrap();
        fs::remove_file(base.join("hyperframes-audio/references/fx-registry.md")).unwrap();
        assert!(validate(package.path(), "0.8.105")
            .unwrap_err()
            .message
            .contains("hyperframes-audio/references/fx-registry.md"));
    }

    #[test]
    fn rendering_requires_pinned_local_animation_instead_of_a_cdn() {
        let package = tempfile::tempdir().unwrap();
        fixture(package.path(), "0.8.104");
        validate(package.path(), "0.8.104").unwrap();
        fs::write(
            package.path().join("node_modules/gsap/package.json"),
            serde_json::json!({"name":"gsap","version":"3.13.0"}).to_string(),
        )
        .unwrap();
        assert!(validate(package.path(), "0.8.104").is_err());
        fixture(package.path(), "0.8.104");
        fs::remove_file(package.path().join(GSAP)).unwrap();
        assert!(validate(package.path(), "0.8.104").is_err());
    }

    #[test]
    fn rendering_health_ignores_optional_services_but_requires_every_local_binary() {
        let mut report = serde_json::json!({"ok":false,"checks":[
            {"name":"Node.js","ok":true},{"name":"FFmpeg","ok":true},
            {"name":"FFprobe","ok":true},{"name":"Chrome","ok":true},
            {"name":"Docker","ok":false},{"name":"TTS (Kokoro)","ok":false}
        ]});
        assert!(verify_report(&report.to_string()).is_ok());
        report["checks"][3]["ok"] = false.into();
        assert!(verify_report(&report.to_string())
            .unwrap_err()
            .message
            .contains("Chrome"));
        report["checks"].as_array_mut().unwrap().remove(3);
        assert!(verify_report(&report.to_string()).is_err());
    }

    #[test]
    fn managed_runtime_keeps_binaries_private_and_relocates_with_the_generation() {
        let root = tempfile::tempdir().unwrap();
        let first = root.path().join("staging");
        fs::create_dir_all(&first).unwrap();
        fixture(&first, "0.8.99");
        validate(&first, "0.8.99").unwrap();
        let final_path = root.path().join("published");
        fs::rename(&first, &final_path).unwrap();
        let runtime = Runtime::at(&final_path).unwrap();
        assert_eq!(
            runtime.environment["HYPERFRAMES_BROWSER_PATH"],
            final_path.join("browser/chrome").to_string_lossy()
        );
        assert_eq!(
            runtime.environment["PRODUCER_HEADLESS_SHELL_PATH"],
            runtime.environment["HYPERFRAMES_BROWSER_PATH"]
        );
        assert_eq!(runtime.node, install::node_path(&final_path));
        let mut binaries = Binaries::read(&final_path).unwrap();
        binaries.browser = "../external".into();
        fs::write(
            final_path.join(PATHS),
            serde_json::to_vec(&binaries).unwrap(),
        )
        .unwrap();
        assert!(Runtime::at(&final_path).is_err());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn health_probes_execution_and_source_video_support_despite_a_successful_doctor_report() {
        use std::os::unix::fs::PermissionsExt;
        let package = tempfile::tempdir().unwrap();
        fixture(package.path(), "0.8.99");
        let stub = |path: PathBuf, body: &str| {
            fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
            fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
        };
        stub(install::node_path(package.path()), "if [ \"$2\" = --version ]; then echo 0.8.99; else echo '{\"checks\":[{\"name\":\"Node.js\",\"ok\":true},{\"name\":\"Chrome\",\"ok\":true},{\"name\":\"FFmpeg\",\"ok\":true},{\"name\":\"FFprobe\",\"ok\":true}]}'; fi");
        stub(package.path().join("bin/ffmpeg"), "echo libx264");
        stub(package.path().join("browser/chrome"), "echo 'Chrome 152'");
        stub(package.path().join("bin/ffprobe"), "exit 1");
        assert!(verify(package.path(), "0.8.99").await.is_err());
        stub(
            package.path().join("bin/ffprobe"),
            "echo 'ffprobe version 6.1.1'",
        );
        assert!(verify(package.path(), "0.8.99")
            .await
            .unwrap_err()
            .message
            .contains("extração"));
        stub(
            package.path().join("bin/ffmpeg"),
            "echo 'libx264 -fps_mode'",
        );
        assert!(verify(package.path(), "0.8.99").await.is_ok());
        stub(package.path().join("browser/chrome"), "exit 1");
        assert!(verify(package.path(), "0.8.99").await.is_err());
    }
}
