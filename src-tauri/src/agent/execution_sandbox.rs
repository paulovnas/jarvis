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

    /// Native execution with material effects needs informed approval in manual
    /// mode. YOLO preauthorizes it; matching grants are handled by the caller.
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
    if policy.outcome.native_working_directory.is_some() {
        return Some(unavailable(&policy.outcome.reason, &policy.outcome.effects));
    }
    let plan = prepare_for(
        current_platform(),
        detect_adapters(),
        &policy.working_directory,
        &policy.outcome.effects,
    );
    #[cfg(target_os = "linux")]
    let plan = verify_linux_sandbox(
        plan,
        &policy.outcome.effects,
        std::time::Duration::from_secs(2),
    );
    Some(plan)
}

#[cfg(target_os = "linux")]
fn verify_linux_sandbox(
    plan: SandboxPlan,
    effects: &ExecutionEffects,
    timeout: std::time::Duration,
) -> SandboxPlan {
    use std::process::{Command, Stdio};
    if plan.report.backend != SandboxBackend::LinuxBubblewrap {
        return plan;
    }
    // Probe the selected profile with no user command or side effects. A real
    // command is never retried here: existing approval/recovery rules still apply.
    let (program, arguments) = plan.wrap(Path::new("/bin/true"), std::iter::empty());
    let usable = Command::new(program)
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .is_ok_and(|mut child| {
            let deadline = std::time::Instant::now() + timeout;
            loop {
                match child.try_wait() {
                    Ok(Some(status)) => return status.success(),
                    Ok(None) if std::time::Instant::now() < deadline => {
                        std::thread::sleep(std::time::Duration::from_millis(10))
                    }
                    _ => {
                        let _ = child.kill();
                        let _ = child.wait();
                        return false;
                    }
                }
            }
        });
    if usable {
        plan
    } else {
        unavailable("O Bubblewrap está instalado, mas não conseguiu iniciar o isolamento neste sistema. A execução nativa seguirá o modo de aprovação ativo.", effects)
    }
}

/// Preserve partial output and describe recovery under the active approval mode.
/// A denial can happen after side effects, so recovery never reruns it blindly.
pub(super) fn command_failure(
    plan: Option<&SandboxPlan>,
    args: &serde_json::Value,
    exit_code: Option<i32>,
    output: &str,
) -> super::AgentError {
    let message = format!(
        "Código de saída: {}\n{output}",
        exit_code.map_or("sinal".into(), |code| code.to_string())
    );
    let lower = output.to_lowercase();
    let denied = plan.is_some_and(|plan| plan.report.filesystem_isolated)
        && [
            "operation not permitted",
            "permission denied",
            "eperm",
            "eacces",
            "sandbox-exec:",
        ]
        .iter()
        .any(|text| lower.contains(text));
    if !denied {
        return super::AgentError::new("tool_error", &message);
    }
    let mut retry = args.clone();
    retry["sandboxPermissions"] = "require_escalated".into();
    let guidance = "O ambiente negou um acesso. Verifique os efeitos já produzidos antes de repetir. Se ainda necessário, execute com sandboxPermissions=require_escalated e justification. No modo YOLO, a execução já está autorizada e não exige confirmação; no modo manual, o Jarvis solicitará aprovação quando não houver uma autorização compatível.";
    let mut error = super::AgentError::new("sandbox_denied", &format!("{message}\n{guidance}"));
    error.tool_result = Some(serde_json::json!({"error":{"code":"sandbox_denied","message":message},"recovery":{"arguments":retry,"approvalPolicy":"according_to_turn","sideEffects":"unknown","instructions":guidance}}).to_string());
    error
}

pub(super) fn add_permission_parameters(definition: &mut serde_json::Value) {
    if !matches!(
        definition["name"].as_str(),
        Some("bash" | "process_start" | "terminal_start")
    ) {
        return;
    }
    definition["parameters"]["properties"]["sandboxPermissions"] = serde_json::json!({"type":"string","enum":["use_default","require_escalated"],"description":"Use require_escalated only for a necessary operation blocked by filesystem/network isolation. YOLO preauthorizes this execution; manual mode requests informed approval unless a matching grant exists. Never blindly retry an action whose side effects are uncertain."});
    definition["parameters"]["properties"]["justification"] = serde_json::json!({"type":"string","minLength":1,"maxLength":1000,"description":"Explain the additional access needed when requesting require_escalated."});
}

pub(super) fn command_arguments(args: &serde_json::Value) -> serde_json::Value {
    let mut args = args.clone();
    if let Some(object) = args.as_object_mut() {
        object.remove("sandboxPermissions");
        object.remove("justification");
    }
    args
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
            "O sandbox-exec do macOS não está disponível; a execução nativa seguirá o modo de aprovação ativo.",
            effects,
        ),
        (Platform::Linux, _, None) => unavailable(
            "O Bubblewrap (bwrap) não está disponível; a execução nativa seguirá o modo de aprovação ativo.",
            effects,
        ),
        (Platform::Other, _, _) => unavailable(
            "Este sistema não possui um adaptador de sandbox do Jarvis; a execução nativa seguirá o modo de aprovação ativo.",
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
    // macOS normally places TMPDIR under /var/folders, not /tmp. Resolve
    // symlinks because Seatbelt evaluates the canonical filesystem path.
    let temporary = std::env::temp_dir();
    let temporary = std::fs::canonicalize(&temporary).unwrap_or(temporary);
    let temporary = seatbelt_string(&temporary);
    let network_rule = if network { "(allow network*)\n" } else { "" };
    // Git and shells open /dev/null with O_RDWR even for read-only commands.
    // Permit data I/O on that device without granting writes to the /dev tree.
    format!(
        "(version 1)\n(deny default)\n(allow process-exec)\n(allow process-fork)\n(allow signal (target same-sandbox))\n(allow process-info* (target same-sandbox))\n(allow sysctl-read)\n(allow mach-lookup)\n(allow file-read*)\n(allow file-write-data (require-all (literal \"/dev/null\") (vnode-type CHARACTER-DEVICE)))\n(allow file-write* (subpath \"/tmp\"))\n(allow file-write* (subpath \"/private/tmp\"))\n(allow file-write* (subpath \"/var/tmp\"))\n(allow file-write* (subpath \"/private/var/tmp\"))\n(allow file-write* (subpath \"{temporary}\"))\n(allow file-write* (subpath \"{root}\"))\n{network_rule}"
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

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_unusable_or_stalled_sandbox_requires_informed_approval() {
        use std::os::unix::fs::PermissionsExt;
        let root = tempfile::tempdir().unwrap();
        let adapter = root.path().join("bwrap");
        for (script, expected) in [
            ("#!/bin/sh\nexit 0\n", true),
            ("#!/bin/sh\nexit 1\n", false),
            ("#!/bin/sh\nexec sleep 30\n", false),
        ] {
            std::fs::write(&adapter, script).unwrap();
            std::fs::set_permissions(&adapter, std::fs::Permissions::from_mode(0o700)).unwrap();
            let effects = effects(true, false);
            let plan = prepare_for(
                Platform::Linux,
                AdapterAvailability {
                    seatbelt: None,
                    bubblewrap: Some(adapter.clone()),
                },
                root.path(),
                &effects,
            );
            // Match production: parallel tests can delay even a successful
            // process start beyond 100 ms. The stalled case still times out.
            let plan = verify_linux_sandbox(plan, &effects, std::time::Duration::from_secs(2));
            assert_eq!(
                plan.report.availability == SandboxAvailability::Full,
                expected
            );
            assert_eq!(plan.requires_informed_approval(&effects), !expected);
            if !expected {
                assert!(plan.report.reason.unwrap().contains("está instalado"));
            }
        }
        let absent = prepare_for(
            Platform::Linux,
            AdapterAvailability::default(),
            root.path(),
            &effects(true, false),
        );
        assert!(absent
            .report
            .reason
            .unwrap()
            .contains("não está disponível"));
    }

    fn effects(write: bool, network: bool) -> ExecutionEffects {
        ExecutionEffects {
            writes_filesystem: write,
            uses_network: network,
            ..ExecutionEffects::default()
        }
    }

    fn command_policy(name: &str, command: &str, root: &Path) -> ToolPolicy {
        use crate::agent::{
            execution_policy::inspect_tool,
            tool_contract::{ApprovalPolicy, Capabilities, Effect},
            ToolCall,
        };
        let root = std::fs::canonicalize(root).unwrap();
        inspect_tool(
            &root,
            &ToolCall {
                id: "sandbox-probe".into(),
                name: name.into(),
                args: serde_json::json!({"command": command}),
                status: "pending".into(),
                output: String::new(),
                duration_ms: 0,
            },
            Capabilities {
                effect: Effect::Stateful,
                approval: ApprovalPolicy::AccordingToTurn,
                parallel_safe: false,
            },
        )
        .unwrap()
        .unwrap()
    }

    #[test]
    fn script_and_terminal_network_admission_reaches_both_sandbox_adapters() {
        let root = tempfile::tempdir().unwrap();
        for (tool, command) in [
            ("bash", "npm test"),
            ("bash", "npm run check"),
            ("bash", "node scripts/check.js"),
            ("terminal_start", "git status"),
            ("process_start", "bun run dev"),
        ] {
            let policy = command_policy(tool, command, root.path());
            assert_eq!(
                policy.outcome.decision,
                super::super::execution_policy::ExecutionDecision::Ask
            );
            for platform in [Platform::Macos, Platform::Linux] {
                let plan = prepare_for(
                    platform,
                    AdapterAvailability {
                        seatbelt: Some("/usr/bin/sandbox-exec".into()),
                        bubblewrap: Some("/usr/bin/bwrap".into()),
                    },
                    root.path(),
                    &policy.outcome.effects,
                );
                assert_eq!(
                    plan.report.network,
                    SandboxNetwork::Allowed,
                    "{tool}: {command}"
                );
                let (_, arguments) = plan.wrap(Path::new("/bin/sh"), std::iter::empty());
                match platform {
                    Platform::Macos => {
                        assert!(arguments[1].to_string_lossy().contains("(allow network*)"))
                    }
                    Platform::Linux => {
                        assert!(!arguments.contains(&OsString::from("--unshare-net")))
                    }
                    _ => unreachable!(),
                }
            }
        }
    }

    // Re-enter only this test in a sandboxed copy of the test executable. This
    // exercises real OS enforcement without requiring Node/Python/PostgreSQL.
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    #[test]
    fn sandbox_probe_child() {
        let Ok(operation) = std::env::var("JARVIS_SANDBOX_PROBE_OPERATION") else {
            return;
        };
        let target = std::env::var("JARVIS_SANDBOX_PROBE_TARGET").unwrap();
        match operation.as_str() {
            "connect" => {
                std::net::TcpStream::connect_timeout(
                    &target.parse().unwrap(),
                    std::time::Duration::from_secs(2),
                )
                .unwrap();
            }
            "bind" => {
                std::net::TcpListener::bind(&target).unwrap();
            }
            "tempfile" => {
                use std::io::Write;
                let mut file = tempfile::NamedTempFile::new_in(&target).unwrap();
                file.write_all(b"jarvis-sandbox-probe").unwrap();
            }
            _ => panic!("Unknown sandbox probe"),
        }
        println!("jarvis-sandbox-probe-ok");
    }

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    fn run_probe(
        root: &Path,
        network: bool,
        operation: &str,
        target: &str,
    ) -> std::process::Output {
        let plan = prepare_for(
            current_platform(),
            detect_adapters(),
            root,
            &effects(true, network),
        );
        assert!(matches!(
            plan.report.backend,
            SandboxBackend::MacosSeatbelt | SandboxBackend::LinuxBubblewrap
        ));
        let (program, arguments) = plan.wrap(
            &std::env::current_exe().unwrap(),
            [
                "--exact".into(),
                "agent::execution_sandbox::tests::sandbox_probe_child".into(),
                "--nocapture".into(),
            ],
        );
        std::process::Command::new(program)
            .args(arguments)
            .current_dir(root)
            .env("JARVIS_SANDBOX_PROBE_OPERATION", operation)
            .env("JARVIS_SANDBOX_PROBE_TARGET", target)
            .output()
            .unwrap()
    }

    #[cfg(target_os = "linux")]
    #[test]
    #[ignore = "requires usable Bubblewrap namespaces on a Linux host"]
    fn linux_sandbox_enforces_filesystem_and_network_boundaries() {
        let workspace = tempfile::tempdir_in(std::env::current_dir().unwrap()).unwrap();
        let root = workspace.path().join("project");
        let outside = workspace.path().join("outside");
        std::fs::create_dir(&root).unwrap();
        std::fs::create_dir(&outside).unwrap();
        assert!(run_probe(&root, false, "tempfile", root.to_str().unwrap())
            .status
            .success());
        assert!(
            !run_probe(&root, false, "tempfile", outside.to_str().unwrap())
                .status
                .success()
        );
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap().to_string();
        assert!(run_probe(&root, true, "connect", &address).status.success());
        assert!(!run_probe(&root, false, "connect", &address)
            .status
            .success());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_admitted_script_connects_and_binds_ipv4_and_ipv6_loopback() {
        let root = tempfile::tempdir().unwrap();
        let policy = command_policy("bash", "npm test", root.path());
        for address in ["127.0.0.1:0", "[::1]:0"] {
            let listener = std::net::TcpListener::bind(address).unwrap();
            for (operation, target) in [
                ("connect", listener.local_addr().unwrap().to_string()),
                ("bind", address.to_owned()),
            ] {
                let output = run_probe(
                    root.path(),
                    policy.outcome.effects.uses_network,
                    operation,
                    &target,
                );
                assert!(
                    output.status.success(),
                    "{operation} {target}: {}\n{}",
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr)
                );
                assert!(String::from_utf8_lossy(&output.stdout).contains("jarvis-sandbox-probe-ok"));
            }
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_offline_profile_still_rejects_loopback_connections() {
        let root = tempfile::tempdir().unwrap();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let policy = command_policy("bash", "git status", root.path());
        let output = run_probe(
            root.path(),
            policy.outcome.effects.uses_network,
            "connect",
            &listener.local_addr().unwrap().to_string(),
        );
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("PermissionDenied"));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_profile_allows_native_temp_files_but_not_unrelated_project_files() {
        // Keep the project and canary outside TMPDIR to exercise both roots.
        let workspace = tempfile::tempdir_in(std::env::current_dir().unwrap()).unwrap();
        let root = workspace.path().join("project");
        let outside = workspace.path().join("outside");
        std::fs::create_dir(&root).unwrap();
        std::fs::create_dir(&outside).unwrap();
        for target in [&root, &std::env::temp_dir()] {
            let output = run_probe(&root, false, "tempfile", &target.to_string_lossy());
            assert!(
                output.status.success(),
                "{}: {}\n{}",
                target.display(),
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            assert!(String::from_utf8_lossy(&output.stdout).contains("jarvis-sandbox-probe-ok"));
        }
        let output = run_probe(&root, false, "tempfile", &outside.to_string_lossy());
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("PermissionDenied"));
    }

    #[cfg(target_os = "macos")]
    fn sandboxed_shell(root: &Path, script: &str) -> std::process::Output {
        let policy = command_policy("bash", script, root);
        let plan = prepare(&policy).unwrap();
        assert_eq!(plan.report.backend, SandboxBackend::MacosSeatbelt);
        let (program, arguments) = plan.wrap(
            Path::new("/bin/bash"),
            ["--noprofile", "--norc", "-c", script].map(OsString::from),
        );
        std::process::Command::new(program)
            .args(arguments)
            .current_dir(root)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .output()
            .unwrap()
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_null_device_supports_shell_standard_streams() {
        let root = tempfile::tempdir().unwrap();
        for script in [
            "printf discarded > /dev/null; printf discarded >> /dev/null",
            "printf discarded 2> /dev/null >&2",
            "exec 3<> /dev/null; printf discarded >&3; cat < /dev/null",
        ] {
            let output = sandboxed_shell(root.path(), &format!("set -e; {script}"));
            assert!(
                output.status.success(),
                "{script}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert!(output.stdout.is_empty(), "{script}");
            assert!(output.stderr.is_empty(), "{script}");
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_git_inspects_diverged_history_without_null_device_errors() {
        let root = tempfile::tempdir().unwrap();
        let git = |args: &[&str]| {
            let output = std::process::Command::new("git")
                .args([
                    "-c",
                    "user.name=Jarvis Test",
                    "-c",
                    "user.email=jarvis@example.test",
                    "-c",
                    "commit.gpgSign=false",
                    "-c",
                    "core.hooksPath=/dev/null",
                ])
                .args(args)
                .current_dir(root.path())
                .env("GIT_CONFIG_NOSYSTEM", "1")
                .env("GIT_CONFIG_GLOBAL", "/dev/null")
                .output()
                .unwrap();
            assert!(output.status.success(), "{args:?}: {output:?}");
            String::from_utf8(output.stdout).unwrap()
        };
        git(&["init", "--initial-branch=main"]);
        std::fs::write(root.path().join("history.txt"), "base\n").unwrap();
        git(&["add", "history.txt"]);
        git(&["commit", "-m", "base"]);
        git(&["checkout", "-b", "peer"]);
        std::fs::write(root.path().join("history.txt"), "remote change\n").unwrap();
        git(&["commit", "-am", "remote change"]);
        git(&["update-ref", "refs/remotes/origin/main", "HEAD"]);
        git(&["checkout", "main"]);
        std::fs::write(root.path().join("history.txt"), "local change\n").unwrap();
        git(&["commit", "-am", "local change"]);
        let before = git(&["show-ref"]);

        for (script, expected) in [
            (
                "git log --left-right --oneline main...origin/main",
                "remote change",
            ),
            ("git show --stat HEAD", "history.txt"),
            (
                "git diff origin/main...main -- history.txt",
                "+local change",
            ),
        ] {
            let output = sandboxed_shell(root.path(), script);
            assert!(output.status.success(), "{script}: {output:?}");
            let stdout = String::from_utf8(output.stdout).unwrap();
            assert!(stdout.contains(expected), "{script}: {stdout}");
            if script.starts_with("git log") {
                assert!(stdout
                    .lines()
                    .any(|line| line.starts_with("< ") && line.ends_with("local change")));
                assert!(stdout
                    .lines()
                    .any(|line| line.starts_with("> ") && line.ends_with("remote change")));
            }
        }
        assert_eq!(git(&["show-ref"]), before);
        assert!(git(&["status", "--porcelain"]).trim().is_empty());
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
    fn denied_execution_preserves_output_and_recovers_under_the_active_approval_mode() {
        let directory = tempfile::tempdir().unwrap();
        let policy = command_policy("bash", "npm test", directory.path());
        let plan = prepare_for(
            Platform::Macos,
            AdapterAvailability {
                seatbelt: Some("/usr/bin/sandbox-exec".into()),
                bubblewrap: None,
            },
            Path::new("/project"),
            &policy.outcome.effects,
        );
        let error = command_failure(
            Some(&plan),
            &serde_json::json!({"command":"npm test"}),
            Some(1),
            "migration completed\nconnect EPERM 127.0.0.1",
        );
        let result: serde_json::Value =
            serde_json::from_str(error.tool_result.as_deref().unwrap()).unwrap();
        assert_eq!(result["recovery"]["sideEffects"], "unknown");
        assert_eq!(result["recovery"]["approvalPolicy"], "according_to_turn");
        assert!(result["recovery"].get("requiresApproval").is_none());
        assert_eq!(
            result["recovery"]["arguments"]["sandboxPermissions"],
            "require_escalated"
        );
        assert!(error.message.contains("migration completed"));
        assert!(
            command_failure(None, &serde_json::json!({}), Some(1), "EPERM")
                .tool_result
                .is_none()
        );
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
