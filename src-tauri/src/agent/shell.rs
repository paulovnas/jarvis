//! Platform shell resolution and tree-killable process spawning.
//!
//! Unix runs bash in its own process group; Windows runs PowerShell inside a
//! job object. Both wrappers make `start_kill` terminate the whole process
//! tree, so cancellation and timeouts never orphan descendants.
use portable_pty::CommandBuilder;
use process_wrap::tokio::{ChildWrapper, CommandWrap, KillOnDrop};
use std::{
    collections::HashSet,
    ffi::{OsStr, OsString},
    path::{Path, PathBuf},
    process::Stdio,
    sync::LazyLock,
};

#[cfg(unix)]
use std::{ffi::CStr, os::unix::fs::PermissionsExt};

use crate::system::TerminalPreferences;

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

fn executable(path: &Path) -> bool {
    let Ok(metadata) = path.metadata() else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }
    #[cfg(unix)]
    return metadata.permissions().mode() & 0o111 != 0;
    #[cfg(not(unix))]
    true
}

fn executable_from_path(value: &OsStr) -> Option<PathBuf> {
    let path = Path::new(value);
    if path.is_absolute() {
        return executable(path).then(|| path.to_path_buf());
    }
    if path.components().count() != 1 {
        return None;
    }
    std::env::split_paths(&crate::mcp::executable::configured_path()).find_map(|directory| {
        let direct = directory.join(path);
        if executable(&direct) {
            return Some(direct);
        }
        #[cfg(windows)]
        {
            let exe = direct.with_extension("exe");
            if executable(&exe) {
                return Some(exe);
            }
        }
        None
    })
}

#[cfg(unix)]
fn account_login_shell() -> Option<PathBuf> {
    // GUI applications do not necessarily inherit the user's interactive
    // environment. Read the account database instead of trusting `$SHELL`.
    unsafe {
        let requested = libc::sysconf(libc::_SC_GETPW_R_SIZE_MAX);
        let capacity = if requested <= 0 {
            16 * 1024
        } else {
            usize::try_from(requested)
                .unwrap_or(16 * 1024)
                .min(1024 * 1024)
        };
        let mut record = std::mem::MaybeUninit::<libc::passwd>::uninit();
        let mut result = std::ptr::null_mut();
        let mut buffer = vec![0_u8; capacity];
        if libc::getpwuid_r(
            libc::geteuid(),
            record.as_mut_ptr(),
            buffer.as_mut_ptr().cast(),
            buffer.len(),
            &mut result,
        ) != 0
            || result.is_null()
        {
            return None;
        }
        let record = record.assume_init();
        if record.pw_shell.is_null() {
            return None;
        }
        let value = OsString::from(
            String::from_utf8_lossy(CStr::from_ptr(record.pw_shell).to_bytes()).into_owned(),
        );
        executable_from_path(&value)
    }
}

#[cfg(not(unix))]
fn account_login_shell() -> Option<PathBuf> {
    None
}

#[cfg(windows)]
fn automatic_interactive_shell(_account: Option<PathBuf>, _inherited: Option<OsString>) -> PathBuf {
    SHELL.program.clone()
}

#[cfg(unix)]
fn automatic_interactive_shell(account: Option<PathBuf>, inherited: Option<OsString>) -> PathBuf {
    if let Some(shell) = account.filter(|path| executable(path)) {
        return shell;
    }
    if let Some(shell) = inherited.as_deref().and_then(executable_from_path) {
        return shell;
    }
    #[cfg(target_os = "macos")]
    {
        PathBuf::from("/bin/zsh")
    }
    #[cfg(not(target_os = "macos"))]
    {
        ["/bin/bash", "/bin/sh"]
            .into_iter()
            .map(PathBuf::from)
            .find(|path| executable(path))
            .unwrap_or_else(|| PathBuf::from("/bin/sh"))
    }
}

pub(crate) fn interactive_shell(preferences: &TerminalPreferences) -> Result<PathBuf, String> {
    if let Some(configured) = preferences.shell.as_deref() {
        return executable_from_path(OsStr::new(configured.trim())).ok_or_else(|| {
            format!(
                "O shell configurado não foi encontrado ou não pode ser executado: {}",
                configured.trim()
            )
        });
    }
    Ok(automatic_interactive_shell(
        account_login_shell(),
        std::env::var_os("SHELL"),
    ))
}

pub(crate) fn interactive_shells() -> Vec<String> {
    let mut candidates = Vec::<PathBuf>::new();
    if let Some(shell) = account_login_shell() {
        candidates.push(shell);
    }
    if let Some(shell) = std::env::var_os("SHELL").and_then(|value| executable_from_path(&value)) {
        candidates.push(shell);
    }
    #[cfg(unix)]
    {
        if let Ok(contents) = std::fs::read_to_string("/etc/shells") {
            candidates.extend(
                contents
                    .lines()
                    .map(str::trim)
                    .filter(|line| !line.is_empty() && !line.starts_with('#'))
                    .map(PathBuf::from)
                    .filter(|path| executable(path)),
            );
        }
        candidates.extend(["/bin/zsh", "/bin/bash", "/bin/sh"].map(PathBuf::from));
    }
    #[cfg(windows)]
    {
        candidates.extend(
            ["pwsh", "powershell"]
                .into_iter()
                .filter_map(|name| executable_from_path(OsStr::new(name))),
        );
        if let Some(command) =
            std::env::var_os("ComSpec").and_then(|value| executable_from_path(&value))
        {
            candidates.push(command);
        }
    }
    let mut seen = HashSet::new();
    candidates
        .into_iter()
        .filter(|path| executable(path) && seen.insert(path.clone()))
        .map(|path| path.to_string_lossy().into_owned())
        .collect()
}

fn default_interactive_arguments(program: &Path) -> Vec<String> {
    let name = program
        .file_stem()
        .and_then(OsStr::to_str)
        .unwrap_or_default()
        .to_ascii_lowercase();
    match name.as_str() {
        "zsh" | "bash" | "ksh" => vec!["-l".into(), "-i".into()],
        "fish" => vec!["-l".into()],
        "pwsh" | "powershell" => vec![
            "-NoLogo".into(),
            "-NoExit".into(),
            "-Command".into(),
            "[Console]::OutputEncoding=[Text.UTF8Encoding]::new()".into(),
        ],
        _ => Vec::new(),
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

/// Service commands use a PTY with stdin enabled, but exit with the command.
pub(crate) fn terminal_service_command(root: &Path, script: &str) -> CommandBuilder {
    let mut command = CommandBuilder::new(&SHELL.program);
    command.args(
        arguments(script)
            .into_iter()
            .filter(|arg| arg != "-NonInteractive"),
    );
    configure_terminal(&mut command, root);
    command
}

/// Builds the interactive shell launched inside a PTY.
pub(crate) fn terminal_command(
    root: &Path,
    preferences: &TerminalPreferences,
) -> Result<CommandBuilder, String> {
    let program = interactive_shell(preferences)?;
    let mut command = CommandBuilder::new(&program);
    let arguments = if preferences.arguments.is_empty() {
        default_interactive_arguments(&program)
    } else {
        preferences.arguments.clone()
    };
    command.args(arguments);
    #[cfg(unix)]
    command.env("SHELL", &program);
    configure_terminal(&mut command, root);
    Ok(command)
}

fn configure_terminal(command: &mut CommandBuilder, root: &Path) {
    command.env("TERM", "xterm-256color");
    command.env("COLORTERM", "truecolor");
    command.env("TERM_PROGRAM", "Jarvis");
    command.env("TERM_PROGRAM_VERSION", env!("CARGO_PKG_VERSION"));
    // Keep canonical paths for backend checks, but give PowerShell a regular
    // Win32/UNC path so its prompt and native child processes resolve the cwd.
    #[cfg(windows)]
    command.cwd(crate::library::strip_verbatim(&root.to_string_lossy()).as_ref());
    #[cfg(not(windows))]
    command.cwd(root);
    command.env("PATH", crate::mcp::executable::configured_path());
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
        let command = terminal_command(&canonical, &TerminalPreferences::default()).unwrap();
        let cwd = command.get_cwd().unwrap();
        assert_eq!(std::fs::canonicalize(cwd).unwrap(), canonical);
        if cfg!(windows) {
            assert!(!cwd.to_string_lossy().starts_with(r"\\?\"));
        } else {
            assert_eq!(Path::new(cwd), canonical);
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn automatic_terminal_prefers_the_account_login_shell_without_an_environment() {
        let account = PathBuf::from("/bin/zsh");
        assert_eq!(
            automatic_interactive_shell(Some(account.clone()), None),
            account
        );
    }

    #[test]
    fn custom_terminal_arguments_remain_separate_argv_entries() {
        let root = tempfile::tempdir().unwrap();
        let preferences = TerminalPreferences {
            shell: Some(SHELL.program.to_string_lossy().into_owned()),
            arguments: vec!["--first".into(), "two words".into()],
            ..TerminalPreferences::default()
        };
        let command = terminal_command(root.path(), &preferences).unwrap();
        assert_eq!(
            command.get_argv(),
            &[
                SHELL.program.as_os_str().to_owned(),
                OsString::from("--first"),
                OsString::from("two words")
            ]
        );
    }

    #[test]
    fn missing_custom_terminal_shell_is_rejected() {
        let preferences = TerminalPreferences {
            shell: Some("/jarvis/missing/shell".into()),
            ..TerminalPreferences::default()
        };
        assert!(interactive_shell(&preferences)
            .unwrap_err()
            .contains("não foi encontrado"));
    }

    #[test]
    fn terminal_announces_truecolor_and_jarvis_without_changing_agent_shell() {
        let root = tempfile::tempdir().unwrap();
        let command = terminal_command(root.path(), &TerminalPreferences::default()).unwrap();
        assert_eq!(command.get_env("TERM"), Some(OsStr::new("xterm-256color")));
        assert_eq!(command.get_env("COLORTERM"), Some(OsStr::new("truecolor")));
        assert_eq!(command.get_env("TERM_PROGRAM"), Some(OsStr::new("Jarvis")));
        assert!(prompt().contains(if cfg!(windows) { "PowerShell" } else { "bash" }));
    }

    #[cfg(windows)]
    #[test]
    fn terminal_preserves_unc_network_roots() {
        let command = terminal_command(
            Path::new(r"\\?\UNC\server\share\projeto ação"),
            &TerminalPreferences::default(),
        )
        .unwrap();
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
