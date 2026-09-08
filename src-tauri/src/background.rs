//! Internal commands must not create external console windows in a Windows GUI build.
//! Interactive terminals use their own PTY path instead of these constructors.
use std::{ffi::OsStr, process::Command};

pub(crate) fn command(program: impl AsRef<OsStr>) -> Command {
    let command = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let mut command = command;
        command.creation_flags(windows_sys::Win32::System::Threading::CREATE_NO_WINDOW);
        command
    }
    #[cfg(not(windows))]
    command
}

pub(crate) fn tokio_command(program: impl AsRef<OsStr>) -> tokio::process::Command {
    tokio::process::Command::from(command(program))
}

pub(crate) fn prepare_node(command: &mut tokio::process::Command) -> std::io::Result<()> {
    #[cfg(windows)]
    {
        use sha2::{Digest, Sha256};
        use std::{fs, io::Write, path::Path};
        let name = Path::new(command.as_std().get_program())
            .file_name()
            .and_then(OsStr::to_str)
            .unwrap_or_default();
        if !name.eq_ignore_ascii_case("node") && !name.eq_ignore_ascii_case("node.exe") {
            return Ok(());
        }
        const SCRIPT: &[u8] = include_bytes!("background-node.cjs");
        // This managed preload lives outside verified third-party Core packages.
        let directory = std::env::temp_dir().join("jarvis-node");
        let path = directory.join(format!("background-{:x}.cjs", Sha256::digest(SCRIPT)));
        if !fs::read(&path).is_ok_and(|content| content == SCRIPT) {
            fs::create_dir_all(&directory)?;
            let mut file = tempfile::NamedTempFile::new_in(&directory)?;
            file.write_all(SCRIPT)?;
            file.persist(&path).map_err(|error| error.error)?;
        }
        let existing = command
            .as_std()
            .get_envs()
            .find(|(key, _)| key.to_string_lossy().eq_ignore_ascii_case("NODE_OPTIONS"))
            .map(|(_, value)| value.unwrap_or_default().to_owned())
            .unwrap_or_else(|| std::env::var_os("NODE_OPTIONS").unwrap_or_default());
        let preload = format!(
            "--require \"{}\"",
            path.to_string_lossy().replace('\\', "/")
        );
        let options = if existing.is_empty() {
            preload
        } else {
            format!("{} {preload}", existing.to_string_lossy())
        };
        command.env("NODE_OPTIONS", options);
    }
    #[cfg(not(windows))]
    let _ = command;
    Ok(())
}

#[cfg(windows)]
pub(crate) fn windows_job(command: &mut process_wrap::tokio::CommandWrap) {
    // JobObject replaces flags on the underlying command. Register CreationFlags so
    // CREATE_NO_WINDOW survives temporary suspension and process-tree supervision.
    let mut flags = process_wrap::tokio::CreationFlags(Default::default());
    flags.0 .0 = windows_sys::Win32::System::Threading::CREATE_NO_WINDOW;
    command.wrap(flags).wrap(process_wrap::tokio::JobObject);
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use std::process::{Output, Stdio};

    fn probe(command: &mut Command) {
        command
            .args([
                "--exact",
                "background::tests::console_probe_child",
                "--ignored",
                "--nocapture",
            ])
            .env("JARVIS_CONSOLE_PROBE", "1")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
    }

    fn assert_output(output: Output) {
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stdout)
        );
        assert!(String::from_utf8_lossy(&output.stdout).contains("background stdout"));
        assert!(String::from_utf8_lossy(&output.stderr).contains("background stderr"));
    }

    #[test]
    #[ignore = "Child fixture spawned by native background-process tests"]
    fn console_probe_child() {
        assert_eq!(std::env::var("JARVIS_CONSOLE_PROBE").unwrap(), "1");
        assert!(unsafe { windows_sys::Win32::System::Console::GetConsoleWindow() }.is_null());
        println!("background stdout");
        eprintln!("background stderr");
    }

    #[test]
    fn sync_command_has_no_console_and_preserves_output() {
        let mut command = command(std::env::current_exe().unwrap());
        probe(&mut command);
        assert_output(command.output().unwrap());
    }

    #[tokio::test]
    async fn async_command_and_job_have_no_console_and_preserve_output() {
        for supervised in [false, true] {
            let mut command = tokio_command(std::env::current_exe().unwrap());
            probe(command.as_std_mut());
            if supervised {
                let mut wrapped = process_wrap::tokio::CommandWrap::from(command);
                windows_job(&mut wrapped);
                wrapped.wrap(process_wrap::tokio::KillOnDrop);
                assert_output(
                    Box::into_pin(wrapped.spawn().unwrap().wait_with_output())
                        .await
                        .unwrap(),
                );
            } else {
                assert_output(command.output().await.unwrap());
            }
        }
    }

    #[tokio::test]
    async fn node_sync_probes_keep_descendants_hidden_and_preserve_esm_exports() {
        let mut command = tokio_command("node");
        command.args(["--input-type=module", "-e", r#"
            import { execFileSync, execSync, spawnSync } from 'node:child_process';
            const exe = process.env.JARVIS_PROBE_EXECUTABLE;
            const args = ['--exact', 'background::tests::console_probe_child', '--ignored', '--nocapture'];
            const options = { encoding: 'utf8', windowsHide: false };
            if (!execFileSync(exe, args, options).includes('background stdout')) throw Error('execFileSync output');
            const result = spawnSync(exe, args, options);
            if (result.status !== 0 || !result.stderr.includes('background stderr')) throw Error(result.stdout + result.stderr);
            if (!execSync(`"${exe}" ${args.join(' ')}`, options).includes('background stdout')) throw Error('execSync output');
        "#])
            .env("JARVIS_PROBE_EXECUTABLE", std::env::current_exe().unwrap())
            .env("JARVIS_CONSOLE_PROBE", "1")
            .env("NODE_OPTIONS", "--no-warnings");
        prepare_node(&mut command).unwrap();
        let output = command.output().await.unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
