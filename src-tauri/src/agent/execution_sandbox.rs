//! Platform sandbox adapters for commands started by the agent.
//!
//! Admission remains the source of truth for what a command may do. This
//! module turns that decision into an OS-specific process wrapper when one is
//! available and describes the exact fallback when it is not.

use super::execution_policy::{ExecutionEffects, ToolPolicy};
use serde::{Deserialize, Serialize};
use std::{
    ffi::OsString,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub(super) enum SandboxBackend {
    MacosSeatbelt,
    LinuxBubblewrap,
    WindowsJobObject,
    Native,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub(super) enum SandboxAvailability {
    Full,
    Partial,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub(super) enum SandboxNetwork {
    Isolated,
    Allowed,
    Native,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub(super) struct SandboxReport {
    pub backend: SandboxBackend,
    pub availability: SandboxAvailability,
    pub filesystem_isolated: bool,
    pub network: SandboxNetwork,
    pub process_tree_isolated: bool,
    pub reason: Option<String>,
}

#[derive(Debug, Clone)]
enum Launcher {
    Seatbelt {
        executable: PathBuf,
        profile: String,
    },
    Bubblewrap {
        executable: PathBuf,
        arguments: Vec<OsString>,
    },
    Native,
}

#[derive(Debug, Clone)]
pub(super) struct SandboxPlan {
    report: SandboxReport,
    launcher: Launcher,
}

impl SandboxPlan {
    pub(super) fn report(&self) -> &SandboxReport {
        &self.report
    }

    /// A native fallback that can mutate state must bypass YOLO and request an
    /// informed decision. A previously created matching grant already records
    /// that decision and is handled by the caller.
    pub(super) fn requires_informed_approval(&self, effects: &ExecutionEffects) -> bool {
        self.report.availability != SandboxAvailability::Full && material_effects(effects)
    }

    pub(super) fn wrap(
        &self,
        program: &Path,
        arguments: impl IntoIterator<Item = OsString>,
    ) -> (PathBuf, Vec<OsString>) {
        let child_arguments = arguments.into_iter().collect::<Vec<_>>();
        match &self.launcher {
            Launcher::Seatbelt {
                executable,
                profile,
            } => {
                let mut wrapped = vec![
                    OsString::from("-p"),
                    OsString::from(profile),
                    program.as_os_str().to_owned(),
                ];
                wrapped.extend(child_arguments);
                (executable.clone(), wrapped)
            }
            Launcher::Bubblewrap {
                executable,
                arguments,
            } => {
                let mut wrapped = arguments.clone();
                wrapped.push(OsString::from("--"));
                wrapped.push(program.as_os_str().to_owned());
                wrapped.extend(child_arguments);
                (executable.clone(), wrapped)
            }
            Launcher::Native => (program.to_path_buf(), child_arguments),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
enum Platform {
    Macos,
    Linux,
    Windows,
    Other,
}

#[derive(Debug, Clone, Default)]
struct AdapterAvailability {
    seatbelt: Option<PathBuf>,
    bubblewrap: Option<PathBuf>,
}

pub(super) fn prepare(policy: &ToolPolicy) -> Option<SandboxPlan> {
    policy.outcome.command.as_ref()?;
    Some(prepare_for(
        current_platform(),
        detect_adapters(),
        &policy.working_directory,
        &policy.outcome.effects,
    ))
}

fn prepare_for(
    platform: Platform,
    adapters: AdapterAvailability,
    working_directory: &Path,
    effects: &ExecutionEffects,
) -> SandboxPlan {
    match (platform, adapters.seatbelt, adapters.bubblewrap) {
        (Platform::Macos, Some(executable), _) => SandboxPlan {
            report: SandboxReport {
                backend: SandboxBackend::MacosSeatbelt,
                availability: SandboxAvailability::Full,
                filesystem_isolated: true,
                network: network_mode(effects),
                process_tree_isolated: true,
                reason: None,
            },
            launcher: Launcher::Seatbelt {
                executable,
                profile: seatbelt_profile(working_directory, effects.uses_network),
            },
        },
        (Platform::Linux, _, Some(executable)) => SandboxPlan {
            report: SandboxReport {
                backend: SandboxBackend::LinuxBubblewrap,
                availability: SandboxAvailability::Full,
                filesystem_isolated: true,
                network: network_mode(effects),
                process_tree_isolated: true,
                reason: None,
            },
            launcher: Launcher::Bubblewrap {
                executable,
                arguments: bubblewrap_arguments(working_directory, effects.uses_network),
            },
        },
        (Platform::Windows, _, _) => SandboxPlan {
            report: SandboxReport {
                backend: SandboxBackend::WindowsJobObject,
                availability: SandboxAvailability::Partial,
                filesystem_isolated: false,
                network: SandboxNetwork::Native,
                process_tree_isolated: true,
                reason: Some("O Windows Job Object encerra a árvore de processos, mas não restringe arquivos ou rede nesta instalação.".into()),
            },
            launcher: Launcher::Native,
        },
        (Platform::Macos, None, _) => unavailable(
            "O sandbox-exec do macOS não está disponível; o comando será executado nativamente somente após autorização explícita.",
            effects,
        ),
        (Platform::Linux, _, None) => unavailable(
            "O Bubblewrap (bwrap) não está disponível; o comando será executado nativamente somente após autorização explícita.",
            effects,
        ),
        (Platform::Other, _, _) => unavailable(
            "Este sistema não possui um adaptador de sandbox do Jarvis; efeitos externos exigem autorização explícita.",
            effects,
        ),
    }
}

fn unavailable(reason: &str, effects: &ExecutionEffects) -> SandboxPlan {
    SandboxPlan {
        report: SandboxReport {
            backend: SandboxBackend::Native,
            availability: SandboxAvailability::Unavailable,
            filesystem_isolated: false,
            network: if effects.uses_network {
                SandboxNetwork::Allowed
            } else {
                SandboxNetwork::Native
            },
            process_tree_isolated: true,
            reason: Some(reason.into()),
        },
        launcher: Launcher::Native,
    }
}

fn material_effects(effects: &ExecutionEffects) -> bool {
    effects.writes_filesystem
        || effects.uses_network
        || effects.controls_processes
        || effects.destructive
        || effects.dynamic
        || effects.unknown
}

fn network_mode(effects: &ExecutionEffects) -> SandboxNetwork {
    if effects.uses_network {
        SandboxNetwork::Allowed
    } else {
        SandboxNetwork::Isolated
    }
}

fn current_platform() -> Platform {
    #[cfg(target_os = "macos")]
    return Platform::Macos;
    #[cfg(target_os = "linux")]
    return Platform::Linux;
    #[cfg(windows)]
    return Platform::Windows;
    #[allow(unreachable_code)]
    Platform::Other
}

fn detect_adapters() -> AdapterAvailability {
    AdapterAvailability {
        seatbelt: executable(Path::new("/usr/bin/sandbox-exec"))
            .then(|| PathBuf::from("/usr/bin/sandbox-exec")),
        bubblewrap: find_in_path("bwrap"),
    }
}

fn executable(path: &Path) -> bool {
    path.metadata().is_ok_and(|metadata| metadata.is_file())
}

fn find_in_path(name: &str) -> Option<PathBuf> {
    [
        PathBuf::from("/usr/bin").join(name),
        PathBuf::from("/bin").join(name),
    ]
    .into_iter()
    .chain(
        std::env::split_paths(&crate::mcp::executable::configured_path())
            .map(|path| path.join(name)),
    )
    .find(|path| executable(path))
}

fn seatbelt_profile(writable_root: &Path, network: bool) -> String {
    let root = seatbelt_string(writable_root);
    let network_rule = if network { "(allow network*)\n" } else { "" };
    format!(
        "(version 1)\n(deny default)\n(allow process-exec)\n(allow process-fork)\n(allow signal (target same-sandbox))\n(allow process-info* (target same-sandbox))\n(allow sysctl-read)\n(allow mach-lookup)\n(allow file-read*)\n(allow file-write* (subpath \"/tmp\"))\n(allow file-write* (subpath \"/private/tmp\"))\n(allow file-write* (subpath \"/var/tmp\"))\n(allow file-write* (subpath \"{root}\"))\n{network_rule}"
    )
}

fn seatbelt_string(path: &Path) -> String {
    path.to_string_lossy()
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
}

fn bubblewrap_arguments(writable_root: &Path, network: bool) -> Vec<OsString> {
    let root = writable_root.as_os_str().to_owned();
    let mut arguments = vec![
        "--die-with-parent".into(),
        "--new-session".into(),
        "--unshare-user".into(),
        "--unshare-pid".into(),
        "--unshare-ipc".into(),
        "--ro-bind".into(),
        "/".into(),
        "/".into(),
        "--dev".into(),
        "/dev".into(),
        "--proc".into(),
        "/proc".into(),
        "--tmpfs".into(),
        "/tmp".into(),
        "--bind".into(),
        root.clone(),
        root.clone(),
        "--chdir".into(),
        root,
        "--cap-drop".into(),
        "ALL".into(),
    ];
    if !network {
        arguments.push("--unshare-net".into());
    }
    arguments
}

#[cfg(test)]
mod tests {
    use super::*;

    fn effects(write: bool, network: bool) -> ExecutionEffects {
        ExecutionEffects {
            writes_filesystem: write,
            uses_network: network,
            ..ExecutionEffects::default()
        }
    }

    #[test]
    fn macos_uses_fixed_seatbelt_binary_and_escapes_the_writable_root() {
        let plan = prepare_for(
            Platform::Macos,
            AdapterAvailability {
                seatbelt: Some("/usr/bin/sandbox-exec".into()),
                bubblewrap: None,
            },
            Path::new("/tmp/project \"quoted\""),
            &effects(true, false),
        );
        assert_eq!(plan.report.backend, SandboxBackend::MacosSeatbelt);
        assert_eq!(plan.report.network, SandboxNetwork::Isolated);
        let (program, arguments) = plan.wrap(
            Path::new("/bin/bash"),
            [OsString::from("-c"), OsString::from("echo ok")],
        );
        assert_eq!(program, Path::new("/usr/bin/sandbox-exec"));
        assert!(arguments[1]
            .to_string_lossy()
            .contains("project \\\"quoted\\\""));
        assert!(arguments[1]
            .to_string_lossy()
            .contains("(allow signal (target same-sandbox))"));
        assert!(!arguments[1].to_string_lossy().contains("signal*"));
        assert_eq!(arguments[2], OsString::from("/bin/bash"));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn generated_macos_profile_runs_a_child_and_signals_it() {
        let directory = tempfile::tempdir().unwrap();
        let plan = prepare_for(
            Platform::Macos,
            AdapterAvailability {
                seatbelt: Some("/usr/bin/sandbox-exec".into()),
                bubblewrap: None,
            },
            directory.path(),
            &effects(true, false),
        );
        let script = "sleep 5 & child=$!; kill -TERM \"$child\"; wait \"$child\"; code=$?; [ \"$code\" -eq 143 ]";
        let (program, arguments) = plan.wrap(
            Path::new("/bin/sh"),
            [OsString::from("-c"), OsString::from(script)],
        );
        let output = std::process::Command::new(program)
            .args(arguments)
            .current_dir(directory.path())
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "sandbox-exec failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[test]
    fn linux_layers_one_writable_root_and_isolates_unrequested_network() {
        let plan = prepare_for(
            Platform::Linux,
            AdapterAvailability {
                seatbelt: None,
                bubblewrap: Some("/usr/bin/bwrap".into()),
            },
            Path::new("/workspace/project"),
            &effects(true, false),
        );
        let (_, arguments) = plan.wrap(
            Path::new("/bin/bash"),
            [OsString::from("-c"), OsString::from("echo ok")],
        );
        assert!(arguments
            .windows(3)
            .any(|items| items == ["--bind", "/workspace/project", "/workspace/project"]));
        assert!(arguments.contains(&OsString::from("--unshare-net")));
        assert_eq!(arguments.iter().filter(|item| *item == "--").count(), 1);
    }

    #[test]
    fn approved_network_is_visible_and_keeps_the_host_namespace() {
        let plan = prepare_for(
            Platform::Linux,
            AdapterAvailability {
                seatbelt: None,
                bubblewrap: Some("/usr/bin/bwrap".into()),
            },
            Path::new("/workspace/project"),
            &effects(false, true),
        );
        let (_, arguments) = plan.wrap(Path::new("/bin/bash"), std::iter::empty());
        assert_eq!(plan.report.network, SandboxNetwork::Allowed);
        assert!(!arguments.contains(&OsString::from("--unshare-net")));
    }

    #[test]
    fn missing_adapter_forces_informed_approval_only_for_material_effects() {
        let mutating = prepare_for(
            Platform::Linux,
            AdapterAvailability::default(),
            Path::new("/workspace/project"),
            &effects(true, false),
        );
        assert!(mutating.requires_informed_approval(&effects(true, false)));
        assert_eq!(
            mutating.report.availability,
            SandboxAvailability::Unavailable
        );

        let read_only = effects(false, false);
        assert!(!prepare_for(
            Platform::Other,
            AdapterAvailability::default(),
            Path::new("/workspace/project"),
            &read_only
        )
        .requires_informed_approval(&read_only));
    }

    #[test]
    fn windows_reports_process_containment_without_claiming_file_or_network_isolation() {
        let plan = prepare_for(
            Platform::Windows,
            AdapterAvailability::default(),
            Path::new(r"C:\\project"),
            &effects(true, false),
        );
        assert_eq!(plan.report.backend, SandboxBackend::WindowsJobObject);
        assert_eq!(plan.report.availability, SandboxAvailability::Partial);
        assert!(!plan.report.filesystem_isolated);
        assert!(plan.report.process_tree_isolated);
        assert!(plan.requires_informed_approval(&effects(true, false)));
    }
}
