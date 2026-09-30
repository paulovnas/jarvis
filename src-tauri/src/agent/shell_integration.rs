//! Private startup wrappers preserve user profiles and add explicit shell events.
use super::TerminalIntegration;
use std::{
    ffi::OsString,
    io::Write,
    path::{Path, PathBuf},
};

fn quote(value: &Path) -> String {
    format!("'{}'", value.to_string_lossy().replace('\'', "'\\''"))
}

fn write(directory: &Path, name: &str, content: &str) -> Result<PathBuf, String> {
    let path = directory.join(name);
    let mut file = tempfile::NamedTempFile::new_in(directory)
        .map_err(|_| "Não foi possível preparar os hooks privados do terminal.".to_owned())?;
    file.write_all(content.as_bytes())
        .and_then(|()| file.flush())
        .map_err(|_| "Não foi possível salvar os hooks privados do terminal.".to_owned())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.as_file()
            .set_permissions(std::fs::Permissions::from_mode(0o600))
            .map_err(|_| "Não foi possível proteger os hooks do terminal.".to_owned())?;
    }
    file.persist(&path)
        .map_err(|_| "Não foi possível confirmar os hooks do terminal.".to_owned())?;
    Ok(path)
}

pub(super) fn prepare(
    program: &Path,
    arguments: &mut Vec<OsString>,
    integration: &TerminalIntegration,
) -> Result<Option<PathBuf>, String> {
    let name = program
        .file_stem()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let flags = arguments
        .iter()
        .map(|arg| arg.to_string_lossy())
        .collect::<Vec<_>>();
    let shell_flags = flags.iter().all(|arg| {
        matches!(
            arg.as_ref(),
            "-l" | "-i" | "-li" | "-il" | "--login" | "--interactive" | "--noprofile"
        )
    });
    let powershell_default = flags
        == [
            "-NoLogo",
            "-NoExit",
            "-Command",
            "[Console]::OutputEncoding=[Text.UTF8Encoding]::new()",
        ];
    // Custom execution arguments must never be rewritten or replayed as commands.
    if !((matches!(name.as_str(), "bash" | "zsh") && shell_flags)
        || (matches!(name.as_str(), "pwsh" | "powershell") && powershell_default))
    {
        return Ok(None);
    }
    if !integration.directory.is_absolute()
        || !(16..=128).contains(&integration.token.len())
        || !integration
            .token
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    {
        return Err("Configuração dos hooks do terminal inválida.".into());
    }
    if let Ok(metadata) = std::fs::symlink_metadata(&integration.directory) {
        if !metadata.is_dir() || metadata.is_symlink() {
            return Err(
                "A configuração privada do terminal não pode ser um link ou arquivo.".into(),
            );
        }
    }
    std::fs::create_dir_all(&integration.directory)
        .map_err(|_| "Não foi possível criar a configuração privada do terminal.".to_owned())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(
            &integration.directory,
            std::fs::Permissions::from_mode(0o700),
        )
        .map_err(|_| "Não foi possível proteger a configuração do terminal.".to_owned())?;
    }
    if matches!(name.as_str(), "pwsh" | "powershell") {
        let script = include_str!("shell-integration/powershell.ps1")
            .replace("__JARVIS_TOKEN__", &integration.token);
        let path = write(&integration.directory, "integration.ps1", &script)?;
        let path = crate::library::strip_verbatim(&path.to_string_lossy()).replace('\'', "''");
        arguments[3] =
            format!("[Console]::OutputEncoding=[Text.UTF8Encoding]::new(); . '{path}'").into();
        return Ok(None);
    }
    let common = write(
        &integration.directory,
        "common.sh",
        &include_str!("shell-integration/common.sh")
            .replace("__JARVIS_TOKEN__", &integration.token),
    )?;
    let script = if name == "zsh" {
        include_str!("shell-integration/zsh.sh")
    } else {
        include_str!("shell-integration/bash.sh")
    };
    let hook = write(
        &integration.directory,
        "integration.sh",
        &format!(". {}\n{script}", quote(&common)),
    )?;
    let login = flags
        .iter()
        .any(|flag| matches!(flag.as_ref(), "-l" | "-li" | "-il" | "--login"));
    if name == "zsh" {
        let original = std::env::var_os("ZDOTDIR")
            .or_else(|| std::env::var_os("HOME"))
            .map(PathBuf::from)
            .ok_or("Não foi possível localizar a configuração do shell.")?;
        let private = quote(&integration.directory);
        write(&integration.directory, ".zshenv", &format!("typeset -g __jarvis_user_zdotdir={}\nZDOTDIR=$__jarvis_user_zdotdir\n[[ -r \"$ZDOTDIR/.zshenv\" ]] && builtin source \"$ZDOTDIR/.zshenv\"\n__jarvis_user_zdotdir=${{ZDOTDIR:-$HOME}}\nZDOTDIR={private}\n", quote(&original)))?;
        for profile in [".zprofile", ".zshrc", ".zlogin"] {
            let restore = if profile == ".zlogin" || (profile == ".zshrc" && !login) {
                "ZDOTDIR=$__jarvis_user_zdotdir".into()
            } else {
                format!("ZDOTDIR={private}")
            };
            let install = if (profile == ".zshrc" && !login) || (profile == ".zlogin" && login) {
                format!("builtin source {}\n", quote(&hook))
            } else {
                String::new()
            };
            write(&integration.directory, profile, &format!("ZDOTDIR=$__jarvis_user_zdotdir\n[[ -r \"$ZDOTDIR/{profile}\" ]] && builtin source \"$ZDOTDIR/{profile}\"\n__jarvis_user_zdotdir=${{ZDOTDIR:-$HOME}}\n{install}{restore}\n"))?;
        }
        return Ok(Some(integration.directory.clone()));
    }
    let profile = if login && !flags.iter().any(|flag| flag == "--noprofile") {
        ". /etc/profile\nfor __jarvis_profile in \"$HOME/.bash_profile\" \"$HOME/.bash_login\" \"$HOME/.profile\"; do\n if [[ -r $__jarvis_profile ]]; then . \"$__jarvis_profile\"; break; fi\ndone\n"
    } else if !login {
        "[[ -r $HOME/.bashrc ]] && . \"$HOME/.bashrc\"\n"
    } else {
        ""
    };
    let rc = write(
        &integration.directory,
        "bashrc",
        &format!("{profile}. {}\n", quote(&hook)),
    )?;
    *arguments = vec![
        "--noprofile".into(),
        "--rcfile".into(),
        rc.into_os_string(),
        "-i".into(),
    ];
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn integration(root: &Path) -> TerminalIntegration {
        TerminalIntegration {
            directory: root.join("hooks"),
            token: "terminal-test-0123456789".into(),
        }
    }

    #[test]
    fn custom_execution_arguments_are_preserved_without_telemetry() {
        let directory = tempfile::tempdir().unwrap();
        let integration = integration(directory.path());
        let mut arguments = vec!["-c".into(), "npm run dev".into()];
        let original = arguments.clone();
        assert!(
            prepare(Path::new("/bin/bash"), &mut arguments, &integration)
                .unwrap()
                .is_none()
        );
        assert_eq!(arguments, original);
        assert!(!integration.directory.exists());
    }

    #[test]
    fn invalid_metadata_tokens_cannot_be_written_into_scripts() {
        let directory = tempfile::tempdir().unwrap();
        let mut integration = integration(directory.path());
        integration.token = "invalid'; printf injected".into();
        let mut arguments = vec!["-l".into(), "-i".into()];
        assert!(prepare(Path::new("/bin/bash"), &mut arguments, &integration).is_err());
        assert!(!integration.directory.exists());
    }

    #[cfg(unix)]
    #[test]
    fn startup_hooks_are_private_and_load_profiles_before_instrumentation() {
        use std::os::unix::fs::PermissionsExt;
        let directory = tempfile::tempdir().unwrap();
        let integration = integration(directory.path());
        let mut arguments = vec!["-l".into(), "-i".into()];
        prepare(Path::new("/bin/bash"), &mut arguments, &integration).unwrap();
        let rc = std::fs::read_to_string(integration.directory.join("bashrc")).unwrap();
        assert!(rc.find(". /etc/profile").unwrap() < rc.find("integration.sh").unwrap());
        assert_eq!(
            std::fs::metadata(&integration.directory)
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        assert_eq!(
            std::fs::metadata(integration.directory.join("common.sh"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }

    #[test]
    fn powershell_keeps_profiles_and_installs_hooks_after_utf8_setup() {
        let directory = tempfile::tempdir().unwrap();
        let integration = integration(directory.path());
        let mut arguments =
            super::super::default_interactive_arguments(Path::new("powershell.exe"))
                .into_iter()
                .map(OsString::from)
                .collect();
        prepare(Path::new("powershell.exe"), &mut arguments, &integration).unwrap();
        assert!(!arguments.iter().any(|argument| argument == "-NoProfile"));
        assert!(arguments[3]
            .to_string_lossy()
            .contains("UTF8Encoding]::new(); . '"));
    }

    #[cfg(unix)]
    #[test]
    fn unix_hooks_report_idle_real_commands_exit_codes_and_current_directory() {
        use base64::{engine::general_purpose::STANDARD, Engine};
        use portable_pty::{native_pty_system, CommandBuilder, PtySize};
        use std::{
            io::Read,
            sync::mpsc,
            time::{Duration, Instant},
        };

        for program in ["/bin/bash", "/bin/zsh"]
            .into_iter()
            .filter(|program| Path::new(program).is_file())
        {
            let directory = tempfile::tempdir().unwrap();
            let cwd = directory.path().join("diretório com espaços");
            std::fs::create_dir(&cwd).unwrap();
            let integration = integration(directory.path());
            let mut args = vec!["-i".into()];
            prepare(Path::new(program), &mut args, &integration).unwrap();
            let hook = integration.directory.join("integration.sh");
            let pair = native_pty_system()
                .openpty(PtySize {
                    rows: 24,
                    cols: 100,
                    pixel_width: 0,
                    pixel_height: 0,
                })
                .unwrap();
            let mut command = CommandBuilder::new(program);
            command.args(if program.ends_with("bash") {
                vec!["--noprofile", "--norc", "-i"]
            } else {
                vec!["-f", "-i"]
            });
            command.cwd(directory.path());
            let mut child = pair.slave.spawn_command(command).unwrap();
            drop(pair.slave);
            let mut reader = pair.master.try_clone_reader().unwrap();
            let mut writer = pair.master.take_writer().unwrap();
            let (send, recv) = mpsc::channel();
            std::thread::spawn(move || {
                let mut buffer = [0; 4096];
                while let Ok(size) = reader.read(&mut buffer) {
                    if size == 0
                        || send
                            .send(String::from_utf8_lossy(&buffer[..size]).into_owned())
                            .is_err()
                    {
                        break;
                    }
                }
            });
            let mut output = String::new();
            let mut wait_for = |needle: &str| {
                let deadline = Instant::now() + Duration::from_secs(5);
                while !output.contains(needle) {
                    let remaining = deadline.saturating_duration_since(Instant::now());
                    let chunk = recv.recv_timeout(remaining).unwrap_or_else(|error| {
                        panic!("{program}: expected {needle:?}: {error}; output={output:?}")
                    });
                    output.push_str(&chunk);
                }
                let consumed = output.find(needle).unwrap() + needle.len();
                output.drain(..consumed);
            };
            if program.ends_with("zsh") {
                writeln!(writer, "precmd() {{ local user_exit=$?; print -r -- USER_PRECMD:$user_exit; }}; preexec() {{ print -r -- USER_PREEXEC; }}; user_array_hook() {{ print -r -- USER_ARRAY; }}; precmd_functions=(user_array_hook)").unwrap();
                writer.flush().unwrap();
                wait_for("USER_PRECMD:0");
            } else {
                writeln!(writer, "HISTCONTROL=; HISTIGNORE=; HISTSIZE=500; set -o history; PROMPT_COMMAND='printf \"USER_BASH_PROMPT\\n\"'").unwrap();
                writer.flush().unwrap();
                wait_for("USER_BASH_PROMPT\r\n");
            }
            writeln!(writer, ". {}", quote(&hook)).unwrap();
            writer.flush().unwrap();
            wait_for(&format!(";{};idle;", integration.token));
            writeln!(writer, "cd -- {}", quote(&cwd)).unwrap();
            writer.flush().unwrap();
            wait_for(&format!(
                ";{};end;0;{}\x07",
                integration.token,
                STANDARD.encode(cwd.to_string_lossy().as_bytes())
            ));
            writeln!(writer, "false").unwrap();
            writer.flush().unwrap();
            wait_for(&format!(
                ";{};start;{};{};1\x07",
                integration.token,
                STANDARD.encode(cwd.to_string_lossy().as_bytes()),
                STANDARD.encode("false")
            ));
            if program.ends_with("zsh") {
                wait_for("USER_PREEXEC");
            }
            wait_for(&format!(
                ";{};end;1;{}\x07",
                integration.token,
                STANDARD.encode(cwd.to_string_lossy().as_bytes())
            ));
            if program.ends_with("zsh") {
                wait_for("USER_PRECMD:1");
                wait_for("USER_ARRAY");
            } else {
                wait_for("USER_BASH_PROMPT\r\n");
            }
            writeln!(
                writer,
                "sh -c 'test -z \"$__jarvis_token\" && printf \"\\nJARVIS_TOKEN_PRIVATE\\n\"'"
            )
            .unwrap();
            writer.flush().unwrap();
            wait_for("\r\nJARVIS_TOKEN_PRIVATE\r\n");
            writeln!(writer, "sleep 20 &").unwrap();
            writer.flush().unwrap();
            wait_for(&format!(
                ";{};idle;{};1\x07",
                integration.token,
                STANDARD.encode(cwd.to_string_lossy().as_bytes())
            ));
            writeln!(writer, "kill %1; wait %1 2>/dev/null; true").unwrap();
            writer.flush().unwrap();
            wait_for(&format!(
                ";{};idle;{};0\x07",
                integration.token,
                STANDARD.encode(cwd.to_string_lossy().as_bytes())
            ));
            if program.ends_with("bash") {
                use std::os::unix::fs::PermissionsExt;
                let bin = directory.path().join("bin");
                std::fs::create_dir(&bin).unwrap();
                for (name, body) in [
                    ("npm", "exit 0"),
                    ("migration", "IFS= read -r migration_input"),
                ] {
                    let script = bin.join(name);
                    std::fs::write(&script, format!("#!/bin/sh\n{body}\n")).unwrap();
                    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700))
                        .unwrap();
                }
                writeln!(writer, "PATH={}:$PATH; HISTCONTROL=ignoreboth", quote(&bin)).unwrap();
                writer.flush().unwrap();
                wait_for(&format!(";{};idle;", integration.token));
                writeln!(writer, "npm run dev").unwrap();
                writer.flush().unwrap();
                wait_for(&format!(
                    ";{};start;{};{};1\x07",
                    integration.token,
                    STANDARD.encode(cwd.to_string_lossy().as_bytes()),
                    STANDARD.encode("npm run dev")
                ));
                wait_for(&format!(";{};end;0;", integration.token));
                writeln!(writer, "npm run dev; migration").unwrap();
                writer.flush().unwrap();
                for command in ["npm run dev", "migration"] {
                    wait_for(&format!(
                        ";{};start;{};{};0\x07",
                        integration.token,
                        STANDARD.encode(cwd.to_string_lossy().as_bytes()),
                        STANDARD.encode(command)
                    ));
                }
                // The fake migration is still blocked here; the completed npm
                // command must already have been replaced and be nonrestartable.
                writeln!(writer, "migration-complete").unwrap();
                writer.flush().unwrap();
                wait_for(&format!(
                    ";{};end;0;{}\x07",
                    integration.token,
                    STANDARD.encode(cwd.to_string_lossy().as_bytes())
                ));
                let nested = cwd.join("sub");
                std::fs::create_dir(&nested).unwrap();
                writeln!(writer, "cd sub && npm run dev").unwrap();
                writer.flush().unwrap();
                wait_for(&format!(
                    ";{};start;{};{};0\x07",
                    integration.token,
                    STANDARD.encode(nested.to_string_lossy().as_bytes()),
                    STANDARD.encode("npm run dev")
                ));
                wait_for(&format!(
                    ";{};end;0;{}\x07",
                    integration.token,
                    STANDARD.encode(nested.to_string_lossy().as_bytes())
                ));
                writeln!(writer, "HISTCONTROL=ignorespace").unwrap();
                writer.flush().unwrap();
                wait_for(&format!(";{};end;0;", integration.token));
                writeln!(writer, " npm run dev").unwrap();
                writer.flush().unwrap();
                wait_for(&format!(
                    ";{};start;{};{};0\x07",
                    integration.token,
                    STANDARD.encode(nested.to_string_lossy().as_bytes()),
                    STANDARD.encode("npm run dev")
                ));
                wait_for(&format!(";{};end;0;", integration.token));
            }
            child.kill().unwrap();
            child.wait().unwrap();
        }
    }
}
