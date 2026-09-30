use process_wrap::tokio::{CommandWrap, KillOnDrop};
use serde::Serialize;
use std::{
    path::{Path, PathBuf},
    process::Stdio,
    time::{Duration, Instant},
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
    pub preferences: super::ProviderPreferences,
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
pub(crate) struct AgyState {
    cached: Mutex<Option<(Instant, RuntimeStatus)>>,
    pub(super) usage: Mutex<super::usage::Cache>,
}

pub(super) async fn cached(state: &AgyState, refresh: bool) -> RuntimeStatus {
    let mut cached = state.cached.lock().await;
    if !refresh {
        if let Some((at, status)) = &*cached {
            if at.elapsed() < Duration::from_secs(60) {
                return status.clone();
            }
        }
    }
    let status = discover(state).await;
    *cached = Some((Instant::now(), status.clone()));
    status
}

pub(super) fn executable() -> Option<PathBuf> {
    let name = if cfg!(windows) { "agy.exe" } else { "agy" };
    let mut directories: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|path| {
            std::env::split_paths(&path)
                .filter(|path| path.is_absolute())
                .collect()
        })
        .unwrap_or_default();
    if let Some(home) = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }) {
        directories.push(PathBuf::from(home).join(".local/bin"));
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

pub(super) fn local_status() -> RuntimeStatus {
    let installed = executable().is_some();
    RuntimeStatus {
        preferences: super::ProviderPreferences::default(), installed, authenticated: false,
        version: None, models: vec![], error: (!installed).then(|| "Antigravity CLI não encontrado. Instale o CLI oficial e execute agy no terminal para entrar na sua conta.".into()),
        auth_method: None, email: None, subscription_type: None,
    }
}

async fn discover(state: &AgyState) -> RuntimeStatus {
    let mut status = local_status();
    let Some(executable) = executable() else {
        return status;
    };
    let directory = match tempfile::tempdir() {
        Ok(directory) => directory,
        Err(_) => {
            status.error = Some("Não foi possível preparar a consulta do Antigravity CLI.".into());
            return status;
        }
    };
    let (version, models, quota) = tokio::join!(
        probe(&executable, directory.path(), &["--version"]),
        probe(&executable, directory.path(), &["models"]),
        super::usage::cached(state),
    );
    let mut errors = Vec::new();
    match version {
        Ok(value) => status.version = Some(value.trim().chars().take(120).collect()),
        Err(error) => errors.push(error),
    }
    match models {
        Ok(value) => {
            status.models = parse_models(&value);
            if status.models.is_empty() {
                errors.push("O Antigravity CLI não retornou modelos disponíveis.".into());
            }
        }
        Err(error) => errors.push(error),
    }
    match quota.error {
        None => {
            status.authenticated = true;
            status.auth_method = Some("agy".into());
            status.email = quota.email;
            status.subscription_type = quota.plan;
        }
        Some(error) => errors.push(error),
    }
    status.error = (!errors.is_empty()).then(|| errors.join(" "));
    status
}

pub(super) fn parse_models(value: &str) -> Vec<Model> {
    let mut seen = std::collections::HashSet::new();
    let mut models: Vec<Model> = Vec::new();
    for line in value.lines() {
        let Some((id, name)) = line.split_once('\t') else {
            continue;
        };
        let id = id.trim();
        let name = name.trim();
        if super::validate_selection(id, None).is_err()
            || name.is_empty()
            || !seen.insert(id.to_owned())
        {
            continue;
        }
        let (base, effort) = super::model_selection(id, None);
        let index = if let Some(index) = models.iter().position(|model| model.id == base) {
            index
        } else {
            if models.len() == 256 {
                continue;
            }
            let name = if let Some(effort) = effort {
                let suffix = format!(
                    " ({})",
                    match effort {
                        "low" => "Low",
                        "medium" => "Medium",
                        "high" => "High",
                        _ => "Max",
                    }
                );
                name.strip_suffix(&suffix).unwrap_or(name)
            } else {
                name
            };
            models.push(Model {
                id: base.into(), name: name.chars().take(256).collect(),
                description: "Modelo informado pelo Antigravity CLI; disponibilidade e cotas dependem da sua conta.".into(),
                reasoning_levels: Vec::new(), default_reasoning: effort.map(str::to_owned),
            });
            models.len() - 1
        };
        if let Some(effort) = effort {
            models[index]
                .default_reasoning
                .get_or_insert_with(|| effort.into());
            if !models[index]
                .reasoning_levels
                .iter()
                .any(|level| level == effort)
            {
                models[index].reasoning_levels.push(effort.into());
            }
        }
    }
    for model in &mut models {
        model
            .reasoning_levels
            .sort_by_key(|effort| match effort.as_str() {
                "low" => 0,
                "medium" => 1,
                "high" => 2,
                _ => 3,
            });
    }
    models
}

pub(super) async fn probe(executable: &Path, cwd: &Path, args: &[&str]) -> Result<String, String> {
    let mut command = crate::background::tokio_command(executable);
    command
        .args(args)
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
    let mut child = wrapped
        .spawn()
        .map_err(|_| "Não foi possível consultar o Antigravity CLI.".to_owned())?;
    let stdout = child
        .stdout()
        .take()
        .ok_or("Saída de metadados AGY indisponível.")?;
    let result = tokio::time::timeout(Duration::from_secs(30), async {
        let mut output = Vec::new();
        stdout.take(262_145).read_to_end(&mut output).await.map_err(|_| "Falha ao ler os metadados AGY.")?;
        if output.len() > 262_144 { return Err("Resposta de metadados AGY excessiva.".to_owned()); }
        if !child.wait().await.map_err(|_| "O Antigravity CLI não confirmou a consulta.")?.success() {
            return Err("O Antigravity CLI não concluiu a consulta. Verifique a conexão e execute agy no terminal para confirmar o login.".into());
        }
        String::from_utf8(output).map_err(|_| "Metadados AGY inválidos.".into())
    }).await.map_err(|_| "A consulta do Antigravity CLI demorou demais. Tente atualizar novamente.".to_owned());
    let _ = child.start_kill();
    result?
}
