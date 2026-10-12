//! The interactive browser ships with the app, never with downloadable Core packages.
use super::{error, health::Check};
use serde::Serialize;
use std::path::{Path, PathBuf};

// Matches the CEF dependency of Jarvis's pinned Tauri runtime.
const CEF_VERSION: &str = "151.8.1";

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    version: &'static str,
    state: State,
    diagnostics: Vec<Check>,
}

#[derive(Clone, Serialize, PartialEq, Debug)]
#[serde(rename_all = "kebab-case")]
enum State {
    Included,
    Development,
    Unavailable,
}

fn check_files(label: &str, paths: &[PathBuf], executable: bool) -> Check {
    let complete = paths.iter().all(|path| {
        std::fs::metadata(path).is_ok_and(|metadata| {
            let valid = metadata.is_file() && metadata.len() > 0;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                valid && (!executable || metadata.permissions().mode() & 0o111 != 0)
            }
            #[cfg(not(unix))]
            {
                let _ = executable;
                valid
            }
        })
    });
    Check::result(
        label,
        if complete {
            Ok(())
        } else {
            Err(error(format!(
                "{label} ausente ou incompleto no aplicativo."
            )))
        },
        "Incluído no aplicativo",
    )
}

fn inspect(executable: &Path, platform: &str) -> Status {
    let mut diagnostics = Vec::new();
    if let Some(directory) = executable.parent() {
        if platform == "macos" {
            // Cargo test/check executables are not app bundles and must not report damage.
            if let Some(contents) = directory.parent().filter(|path| {
                directory.file_name().is_some_and(|name| name == "MacOS")
                    && path.file_name().is_some_and(|name| name == "Contents")
                    && path
                        .parent()
                        .is_some_and(|app| app.extension().is_some_and(|ext| ext == "app"))
            }) {
                let frameworks = contents.join("Frameworks");
                diagnostics.push(check_files(
                    "Motor Chromium",
                    &[frameworks
                        .join("Chromium Embedded Framework.framework/Chromium Embedded Framework")],
                    false,
                ));
                let name = executable.file_name().unwrap_or_default().to_string_lossy();
                // These suffixes match the pinned official macOS CEF bundler.
                let helpers =
                    ["", " (GPU)", " (Renderer)", " (Plugin)", " (Alerts)"].map(|suffix| {
                        let helper = format!("{name} Helper{suffix}");
                        frameworks.join(format!("{helper}.app/Contents/MacOS/{helper}"))
                    });
                diagnostics.push(check_files(
                    "Processos auxiliares do Chromium",
                    &helpers,
                    true,
                ));
            }
        } else {
            let library = directory.join(if platform == "windows" {
                "libcef.dll"
            } else {
                "libcef.so"
            });
            if library.exists() {
                diagnostics.push(check_files("Motor Chromium", &[library], false));
                diagnostics.push(check_files(
                    "Recursos do Chromium",
                    &[
                        directory.join("icudtl.dat"),
                        directory.join("v8_context_snapshot.bin"),
                        directory.join("resources.pak"),
                    ],
                    false,
                ));
            }
        }
    }
    let state = if diagnostics.is_empty() {
        State::Development
    } else if diagnostics.iter().all(|check| check.passed) {
        State::Included
    } else {
        State::Unavailable
    };
    Status {
        version: CEF_VERSION,
        state,
        diagnostics,
    }
}

pub(super) fn snapshot() -> Status {
    match std::env::current_exe() {
        Ok(executable) => inspect(&executable, std::env::consts::OS),
        Err(_) => Status {
            version: CEF_VERSION,
            state: State::Unavailable,
            diagnostics: vec![Check::result(
                "Aplicativo",
                Err(error(
                    "Não foi possível verificar o navegador incluído no Jarvis.",
                )),
                "",
            )],
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(path: &Path) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, b"runtime fixture").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
    }

    #[test]
    fn development_executable_does_not_report_missing_bundled_runtime() {
        let status = inspect(Path::new("target/debug/deps/jarvis-test"), "macos");
        assert_eq!(status.state, State::Development);
        assert!(status.diagnostics.is_empty());
    }

    #[test]
    fn app_bundle_requires_framework_and_every_official_helper() {
        let root = tempfile::tempdir().unwrap();
        let contents = root.path().join("Jarvis.app/Contents");
        let executable = contents.join("MacOS/jarvis");
        file(&executable);
        let framework = contents.join("Frameworks");
        file(&framework.join("Chromium Embedded Framework.framework/Chromium Embedded Framework"));
        for suffix in ["", " (GPU)", " (Renderer)", " (Plugin)", " (Alerts)"] {
            let helper = format!("jarvis Helper{suffix}");
            file(&framework.join(format!("{helper}.app/Contents/MacOS/{helper}")));
        }
        let status = inspect(&executable, "macos");
        assert_eq!(status.state, State::Included);
        assert!(status.diagnostics.iter().all(|check| check.passed));
        let json = serde_json::to_value(status).unwrap();
        assert_eq!(json["version"], "151.8.1");
        assert_eq!(json["state"], "included");

        std::fs::remove_file(
            framework.join("jarvis Helper (Renderer).app/Contents/MacOS/jarvis Helper (Renderer)"),
        )
        .unwrap();
        let status = inspect(&executable, "macos");
        assert_eq!(status.state, State::Unavailable);
        assert!(!status.diagnostics[1].passed);
    }

    #[test]
    fn empty_framework_is_damaged_and_video_browser_does_not_substitute_it() {
        let root = tempfile::tempdir().unwrap();
        let executable = root.path().join("Jarvis.app/Contents/MacOS/jarvis");
        file(&executable);
        let framework = root.path().join("Jarvis.app/Contents/Frameworks/Chromium Embedded Framework.framework/Chromium Embedded Framework");
        file(&framework);
        std::fs::write(framework, []).unwrap();
        file(&root.path().join("core/openmontage/chrome-headless-shell"));
        let status = inspect(&executable, "macos");
        assert_eq!(status.state, State::Unavailable);
        assert!(status.diagnostics.iter().all(|check| !check.passed));
    }

    #[test]
    fn windows_and_linux_distributions_validate_included_resources() {
        for (platform, library) in [("windows", "libcef.dll"), ("linux", "libcef.so")] {
            let root = tempfile::tempdir().unwrap();
            let executable = root.path().join("jarvis");
            for name in [
                library,
                "icudtl.dat",
                "v8_context_snapshot.bin",
                "resources.pak",
            ] {
                file(&root.path().join(name));
            }
            assert_eq!(inspect(&executable, platform).state, State::Included);
            std::fs::write(root.path().join("resources.pak"), []).unwrap();
            assert_eq!(inspect(&executable, platform).state, State::Unavailable);
        }
    }
}
