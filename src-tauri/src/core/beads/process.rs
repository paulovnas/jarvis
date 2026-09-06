use super::*;
use process_wrap::tokio::{CommandWrap, KillOnDrop};
use std::process::Stdio;
use tokio::io::{AsyncRead, AsyncReadExt};

pub(super) fn command(package: &Path, workspace: &Path, session: &str) -> tokio::process::Command {
    let mut command =
        tokio::process::Command::new(package.join(super::super::install::executable("bd")));
    // Embedded Dolt needs no listener or daemon. Never inherit another host's
    // Beads routing, credentials, Git configuration or global instruction setup.
    command
        .env_clear()
        .current_dir(workspace)
        .env("HOME", workspace.join("host"))
        .env("USERPROFILE", workspace.join("host"))
        .env("XDG_CONFIG_HOME", workspace.join("host/config"))
        .env("APPDATA", workspace.join("host/config"))
        .env("DOLT_ROOT_PATH", workspace.join("host"))
        .env("BEADS_DIR", workspace.join(".beads"))
        .env("BD_DOLT_MODE", "embedded")
        .env("BEADS_DOLT_AUTO_START", "0")
        .env("BD_NON_INTERACTIVE", "1")
        .env("BD_DISABLE_METRICS", "1")
        .env("GIT_CEILING_DIRECTORIES", workspace)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", workspace.join("host/gitconfig"))
        .env("GIT_CONFIG_COUNT", "3")
        .env("GIT_CONFIG_KEY_0", "beads.role")
        .env("GIT_CONFIG_VALUE_0", "maintainer")
        .env("GIT_CONFIG_KEY_1", "user.name")
        .env("GIT_CONFIG_VALUE_1", "Jarvis")
        .env("GIT_CONFIG_KEY_2", "user.email")
        .env("GIT_CONFIG_VALUE_2", "jarvis@localhost")
        .env("BEADS_ACTOR", format!("jarvis-{session}"));
    let mut paths = vec![package.to_path_buf(), package.join("dolt/bin")];
    #[cfg(unix)]
    paths.extend([PathBuf::from("/usr/bin"), PathBuf::from("/bin")]);
    #[cfg(windows)]
    if let Some(system) = std::env::var_os("SystemRoot") {
        command.env("SystemRoot", &system);
        paths.push(PathBuf::from(system).join("System32"));
    }
    command.env("PATH", std::env::join_paths(paths).unwrap_or_default());
    command
}

async fn drain(mut stream: impl AsyncRead + Unpin) -> std::io::Result<(Vec<u8>, bool)> {
    let mut result = Vec::new();
    let mut buffer = [0; 8192];
    let mut overflow = false;
    loop {
        let count = stream.read(&mut buffer).await?;
        if count == 0 {
            break;
        }
        let keep = count.min((2 * 1024 * 1024_usize).saturating_sub(result.len()));
        result.extend_from_slice(&buffer[..keep]);
        overflow |= keep < count;
    }
    Ok((result, overflow))
}

pub(super) async fn run(
    mut command: tokio::process::Command,
    mut signal: watch::Receiver<bool>,
) -> Result<String, CoreError> {
    if *signal.borrow() {
        return Err(super::super::cancelled_error());
    }
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let mut wrapped = CommandWrap::from(command);
    #[cfg(unix)]
    wrapped.wrap(process_wrap::tokio::ProcessGroup::leader());
    wrapped.wrap(KillOnDrop);
    let mut child = wrapped
        .spawn()
        .map_err(|_| failure("Não foi possível iniciar o Beads."))?;
    let stdout = child
        .stdout()
        .take()
        .ok_or_else(|| failure("Beads sem saída."))?;
    let stderr = child
        .stderr()
        .take()
        .ok_or_else(|| failure("Beads sem diagnóstico."))?;
    let result = tokio::select! {
        _ = super::super::context::cancelled(&mut signal) => Err(super::super::cancelled_error()),
        _ = tokio::time::sleep(std::time::Duration::from_secs(90)) => Err(failure("O Beads excedeu o tempo limite. Consulte a tarefa antes de repetir uma alteração.")),
        result = async { tokio::try_join!(child.wait(), drain(stdout), drain(stderr)) } => (|| {
            let (status, (output, overflow), (stderr, _)) = result.map_err(|_| failure("Não foi possível ler a resposta do Beads."))?;
            if !status.success() {
                let detail = String::from_utf8_lossy(if output.is_empty() { &stderr } else { &output });
                Err(failure(format!("Beads: {}", detail.chars().take(4000).collect::<String>())))
            } else if overflow {
                Err(failure("A resposta do Beads excedeu o limite. Reduza a consulta; antes de repetir uma alteração, consulte a tarefa."))
            } else {
                String::from_utf8(output).map_err(|_| failure("Resposta inválida do Beads."))
            }
        })()
    };
    if result.is_err() {
        // Keep the project lock until the embedded engine has actually exited.
        let _ = Box::into_pin(child.kill()).await;
        let _ = child.wait().await;
    }
    result
}
