use super::transport::{command_for, ClaudeProcess, RunOptions};
use process_wrap::tokio::{CommandWrap, KillOnDrop};
use serde::Serialize;
use serde_json::{json, Value};
use std::{
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};
use tokio::{io::AsyncReadExt, sync::Mutex};

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub(crate) struct Model {
    pub id: String,
    pub name: String,
    pub description: String,
    pub reasoning_levels: Vec<String>,
    pub default_reasoning: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub(crate) struct RuntimeStatus {
    pub installed: bool,
    pub authenticated: bool,
    pub version: Option<String>,
    pub models: Vec<Model>,
    pub error: Option<String>,
    pub auth_method: Option<String>,
    pub email: Option<String>,
    pub subscription_type: Option<String>,
}

#[derive(Default)]
pub(crate) struct ClaudeState {
    // One metadata probe per app at a time; refresh explicitly re-reads native state.
    cached: Mutex<Option<RuntimeStatus>>,
}

pub(super) async fn cached(state: &ClaudeState, refresh: bool) -> RuntimeStatus {
    let mut cached = state.cached.lock().await;
    if !refresh {
        if let Some(status) = &*cached {
            return status.clone();
        }
    }
    let status = discover().await;
    *cached = Some(status.clone());
    status
}

pub(super) fn executable() -> Option<PathBuf> {
    let name = if cfg!(windows) {
        "claude.exe"
    } else {
        "claude"
    };
    let mut directories: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|path| {
            std::env::split_paths(&path)
                .filter(|path| path.is_absolute())
                .collect()
        })
        .unwrap_or_default();
    if let Some(home) = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }) {
        let home = PathBuf::from(home);
        directories.extend([home.join(".local/bin"), home.join(".claude/local")]);
    }
    #[cfg(unix)]
    directories.extend([
        PathBuf::from("/usr/local/bin"),
        PathBuf::from("/opt/homebrew/bin"),
    ]);
    directories
        .into_iter()
        .map(|directory| directory.join(name))
        .find(|path| {
            std::fs::metadata(path).is_ok_and(|metadata| {
                if !metadata.is_file() {
                    return false;
                }
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    metadata.permissions().mode() & 0o111 != 0
                }
                #[cfg(not(unix))]
                {
                    true
                }
            })
        })
}

fn fallback_models() -> Vec<Model> {
    [
        ("default", "Padrão do Claude Code"),
        ("sonnet", "Sonnet (alias)"),
        ("opus", "Opus (alias)"),
        ("haiku", "Haiku (alias)"),
    ]
    .into_iter()
    .map(|(id, name)| Model {
        id: id.into(),
        name: name.into(),
        description: "Alias do CLI; disponibilidade e limites dependem da conexão Claude.".into(),
        reasoning_levels: vec![],
        default_reasoning: None,
    })
    .collect()
}

pub(super) fn models_from_initialize(value: &Value) -> Vec<Model> {
    let mut models = vec![fallback_models().remove(0)];
    let mut seen = std::collections::HashSet::new();
    for entry in value["models"].as_array().into_iter().flatten().take(256) {
        let Some(id) = text(entry, "value").or_else(|| text(entry, "id")) else {
            continue;
        };
        if super::validate_selection(&id, None).is_err() || !seen.insert(id.clone()) {
            continue;
        }
        let reasoning_levels: Vec<_> = entry["supportedEffortLevels"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .filter(|level| super::validate_selection(&id, Some(level)).is_ok())
            .map(str::to_owned)
            .collect();
        let default_reasoning =
            text(entry, "defaultEffortLevel").filter(|level| reasoning_levels.contains(level));
        let model = Model {
            name: text(entry, "displayName")
                .or_else(|| text(entry, "name"))
                .unwrap_or_else(|| id.clone()),
            description: text(entry, "description").unwrap_or_default(),
            id,
            reasoning_levels,
            default_reasoning,
        };
        if model.id == "default" {
            models[0] = model;
        } else {
            models.push(model);
        }
    }
    models
}

fn text(value: &Value, key: &str) -> Option<String> {
    value[key]
        .as_str()
        .filter(|text| !text.trim().is_empty())
        .map(|text| text.chars().take(1000).collect())
}

pub(super) fn account_from_status(status: &mut RuntimeStatus, value: &Value) {
    status.authenticated = value["loggedIn"]
        .as_bool()
        .or_else(|| value["authenticated"].as_bool())
        .unwrap_or(false);
    status.auth_method = text(value, "authMethod");
    status.email = text(value, "email");
    status.subscription_type = text(value, "subscriptionType");
}

async fn discover() -> RuntimeStatus {
    let mut status = RuntimeStatus {
        installed: false,
        authenticated: false,
        version: None,
        models: vec![],
        error: None,
        auth_method: None,
        email: None,
        subscription_type: None,
    };
    let Some(executable) = executable() else {
        status.error = Some("Claude Code não encontrado. Instale o CLI oficial e execute claude auth login no terminal.".into());
        return status;
    };
    status.installed = true;
    status.models = fallback_models();
    let directory = match tempfile::tempdir() {
        Ok(directory) => directory,
        Err(error) => {
            status.error = Some(error.to_string());
            return status;
        }
    };
    let (version, account) = tokio::join!(
        probe(&executable, directory.path(), &["--version"]),
        probe(&executable, directory.path(), &["auth", "status"]),
    );
    let mut errors = Vec::new();
    match version {
        Ok((true, value)) => status.version = Some(value.trim().chars().take(120).collect()),
        Ok(_) => errors.push("Não foi possível identificar a versão do Claude Code.".into()),
        Err(error) => errors.push(error),
    }
    match account {
        Ok((_, value)) => match serde_json::from_str(&value) {
            Ok(value) => account_from_status(&mut status, &value),
            Err(_) => errors
                .push("Claude Code não retornou um estado de autenticação reconhecido.".into()),
        },
        Err(error) => errors.push(error),
    }
    let options = RunOptions {
        cwd: directory.path().to_owned(),
        session_id: String::new(),
        resume: false,
        model: "default".into(),
        effort: None,
        append_system_prompt: String::new(),
        mcp_servers: json!({}),
    };
    let runtime = command_for(&executable, &options, true)
        .and_then(|(command, files)| ClaudeProcess::spawn_command(command, files));
    match runtime {
        Ok(mut process) => {
            let control = process.control();
            let catalog = tokio::time::timeout(Duration::from_secs(20), async {
                let initialize = control.initialize(json!({}));
                tokio::pin!(initialize);
                loop {
                    tokio::select! {
                        result = &mut initialize => return result,
                        event = process.next_event() => match event? {
                            Some(event) if event["type"] == "control_request" => {
                                let id = event["request_id"].as_str().ok_or("Solicitação de metadados Claude inválida.")?;
                                control.respond_control(id, Err("A descoberta de modelos não executa ferramentas.".into())).await?;
                            }
                            Some(_) => {},
                            None => return Err("Claude encerrou antes de informar os modelos.".into()),
                        }
                    }
                }
            }).await;
            match catalog {
                Ok(Ok(value)) => {
                    let models = models_from_initialize(&value);
                    let reported_default = value["models"].as_array().is_some_and(|entries| {
                        entries
                            .iter()
                            .any(|entry| entry["value"] == "default" || entry["id"] == "default")
                    });
                    if models.len() > 1 || reported_default {
                        status.models = models;
                    } else {
                        errors.push("Catálogo indisponível; mostrando aliases do CLI.".into());
                    }
                }
                Ok(Err(error)) => errors.push(error),
                Err(_) => errors.push(
                    "A consulta dos modelos Claude demorou demais; mostrando aliases do CLI."
                        .into(),
                ),
            }
            if let Err(error) = process.cancel().await {
                errors.push(error);
            }
        }
        Err(error) => errors.push(error),
    }
    if !errors.is_empty() {
        status.error = Some(errors.join(" "));
    }
    status
}

async fn probe(
    executable: &Path,
    cwd: &Path,
    arguments: &[&str],
) -> Result<(bool, String), String> {
    let mut command = crate::background::tokio_command(executable);
    command
        .args(arguments)
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let mut wrapped = CommandWrap::from(command);
    #[cfg(unix)]
    wrapped.wrap(process_wrap::tokio::ProcessGroup::leader());
    #[cfg(windows)]
    crate::background::windows_job(&mut wrapped);
    wrapped.wrap(KillOnDrop);
    let mut child = wrapped.spawn().map_err(|error| error.to_string())?;
    let stdout = child
        .stdout()
        .take()
        .ok_or("Saída de metadados Claude indisponível.")?;
    let result = tokio::time::timeout(Duration::from_secs(10), async {
        let mut output = Vec::new();
        stdout
            .take(65_537)
            .read_to_end(&mut output)
            .await
            .map_err(|error| error.to_string())?;
        if output.len() > 65_536 {
            return Err("Resposta de metadados Claude excessiva.".into());
        }
        let status = child.wait().await.map_err(|error| error.to_string())?;
        Ok((
            status.success(),
            String::from_utf8_lossy(&output).into_owned(),
        ))
    })
    .await;
    // Also release descendants of metadata probes, even if their parent already exited.
    let _ = child.start_kill();
    result.map_err(|_| "A consulta de metadados Claude demorou demais.".to_string())?
}
