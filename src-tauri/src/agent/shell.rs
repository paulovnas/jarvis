//! Platform shell resolution and tree-killable process spawning.
//!
//! Unix runs bash in its own process group; Windows runs PowerShell inside a
//! job object. Both wrappers make `start_kill` terminate the whole process
//! tree, so cancellation and timeouts never orphan descendants.
use portable_pty::CommandBuilder;
use process_wrap::tokio::{ChildWrapper, CommandWrap, KillOnDrop};
use std::{
    path::{Path, PathBuf},
    process::Stdio,
    sync::LazyLock,
};

pub(crate) struct Shell {
    program: PathBuf,
    powershell: bool,
    version: String,
}

static SHELL: LazyLock<Shell> = LazyLock::new(resolve);

#[cfg(windows)]
fn which(name: &str) -> Option<PathBuf> {
    let executable = format!("{name}.exe");
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths)
            .map(|directory| directory.join(&executable))
            .find(|path| path.is_file())
    })
}

#[cfg(windows)]
fn probe_version(program: &Path, fallback: &str) -> String {
    let output = crate::background::command(program)
        .args([
            "-NoProfile",
            "-Command",
            "$PSVersionTable.PSVersion.ToString()",
        ])
        .output();
    match output {
        Ok(output) if output.status.success() => {
            let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if text
                .chars()
                .next()
                .is_some_and(|first| first.is_ascii_digit())
            {
                return text;
            }
            fallback.to_string()
        }
        _ => fallback.to_string(),
    }
}

fn resolve() -> Shell {
    #[cfg(windows)]
    {
        // PowerShell 7 is opt-in; Windows only ships Windows PowerShell 5.1.
        if let Some(program) = which("pwsh") {
            let version = probe_version(&program, "7");
            return Shell {
                program,
                powershell: true,
                version,
            };
        }
        let system = std::env::var_os("SystemRoot")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("C:\\Windows"));
        let program = system.join("System32\\WindowsPowerShell\\v1.0\\powershell.exe");
        let version = probe_version(&program, "5.1");
        Shell {
            program,
            powershell: true,
            version,
        }
    }
    #[cfg(not(windows))]
    {
        Shell {
            program: PathBuf::from("/bin/bash"),
            powershell: false,
            version: "POSIX".into(),
        }
    }
}

/// Sentence telling the model which shell and syntax it must emit.
pub(crate) fn prompt() -> String {
    let shell = &*SHELL;
    if !shell.powershell {
        return "OS: Unix. Shell: bash. Use POSIX shell syntax for shell calls; native tools receive separate arguments.".into();
    }
    if shell.version.starts_with("5") {
        format!(
            "OS: Windows. Shell: Windows PowerShell {}. Use PowerShell syntax for shell calls: '&&' and '||' are not statement operators in 5.1, so chain with ';' and inspect $?, or use if/else. Native executables receive separate arguments.",
            shell.version
        )
    } else {
        format!(
            "OS: Windows. Shell: PowerShell {} (pwsh). Use PowerShell syntax for shell calls; native executables receive separate arguments.",
            shell.version
        )
    }
}

fn arguments(script: &str) -> Vec<String> {
    if SHELL.powershell {
        // Report the native exit code, a cmdlet failure as 1, success as 0,
        // and keep stdout UTF-8 regardless of the system code page.
        vec![
            "-NoProfile".into(),
            "-NonInteractive".into(),
            "-Command".into(),
            format!("[Console]::OutputEncoding=[Text.UTF8Encoding]::new(); {script}; exit $(if ($?) {{ [int]$LASTEXITCODE }} elseif ($LASTEXITCODE) {{ $LASTEXITCODE }} else {{ 1 }})"),
        ]
    } else {
        vec![
            "--noprofile".into(),
            "--norc".into(),
            "-c".into(),
            script.into(),
        ]
    }
}

/// Builds the interactive shell launched inside a PTY.
pub(crate) fn terminal_command(root: &Path) -> CommandBuilder {
    let mut command = CommandBuilder::new(&SHELL.program);
    if SHELL.powershell {
        command.args([
            "-NoLogo",
            "-NoProfile",
            "-NoExit",
            "-Command",
            "[Console]::OutputEncoding=[Text.UTF8Encoding]::new()",
        ]);
    } else {
        command.args(["--noprofile", "--norc", "-i"]);
        command.env("TERM", "xterm-256color");
    }
    // Keep canonical paths for backend checks, but give PowerShell a regular
    // Win32/UNC path so its prompt and native child processes resolve the cwd.
    #[cfg(windows)]
    command.cwd(crate::library::strip_verbatim(&root.to_string_lossy()).as_ref());
    #[cfg(not(windows))]
    command.cwd(root);
    command.env("PATH", crate::mcp::executable::configured_path());
    command
}

/// Spawns `command` in `root` through the platform shell. The returned child
/// kills its whole process tree on `start_kill` and on drop.
pub(crate) fn spawn(command: &str, root: &Path) -> std::io::Result<Box<dyn ChildWrapper>> {
    let mut process = crate::background::tokio_command(&SHELL.program);
    crate::mcp::executable::configure(&mut process, false);
    process
        .args(arguments(command))
        .current_dir(root)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut wrapped = CommandWrap::from(process);
    #[cfg(unix)]
    wrapped.wrap(process_wrap::tokio::ProcessGroup::leader());
    #[cfg(windows)]
    crate::background::windows_job(&mut wrapped);
    wrapped.wrap(KillOnDrop);
    wrapped.spawn()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_starts_in_the_native_project_directory() {
        let root = tempfile::tempdir().unwrap();
        let project = root.path().join("projeto ação [teste]");
        std::fs::create_dir(&project).unwrap();
        let canonical = std::fs::canonicalize(&project).unwrap();
        let command = terminal_command(&canonical);
        let cwd = command.get_cwd().unwrap();
        assert_eq!(std::fs::canonicalize(cwd).unwrap(), canonical);
        if cfg!(windows) {
            assert!(!cwd.to_string_lossy().starts_with(r"\\?\"));
        } else {
            assert_eq!(Path::new(cwd), canonical);
        }
    }

    #[cfg(windows)]
    #[test]
    fn terminal_preserves_unc_network_roots() {
        let command = terminal_command(Path::new(r"\\?\UNC\server\share\projeto ação"));
        assert_eq!(command.get_cwd().unwrap(), r"\\server\share\projeto ação");
    }

    #[test]
    fn resolved_shell_exists_and_arguments_match_the_platform() {
        assert!(
            SHELL.program.is_file() || !SHELL.powershell,
            "resolved shell missing"
        );
        let arguments = arguments("echo hi");
        if cfg!(windows) {
            assert!(SHELL.powershell);
            assert_eq!(arguments[0], "-NoProfile");
            assert!(arguments[3].contains("echo hi"));
            assert!(arguments[3].contains("$LASTEXITCODE"));
        } else {
            assert!(!SHELL.powershell);
            assert_eq!(arguments[2], "-c");
            assert_eq!(arguments[3], "echo hi");
        }
        assert!(prompt().contains("Shell:"));
    }
    #[tokio::test]
    async fn spawned_command_reports_output_and_exit_code() {
        let root = tempfile::tempdir().unwrap();
        let mut child = spawn("exit 7", root.path()).unwrap();
        let status = child.wait().await.unwrap();
        assert_eq!(status.code(), Some(7));
    }
}
