//! Foreground loopback Backlot service scoped to one authorized production.
use super::{error, installed, ComponentId, CoreError, Runtime};
use serde::Serialize;
use std::{
    fs,
    net::TcpListener,
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};
use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BoardLocation {
    pub url: String,
    pub production_path: String,
}
struct BoardProcess {
    production: PathBuf,
    location: BoardLocation,
    child: Box<dyn process_wrap::tokio::ChildWrapper>,
}
#[derive(Default)]
pub struct BacklotState(tokio::sync::Mutex<Option<BoardProcess>>);

fn production_directory(root: &Path, production_path: &str) -> Result<PathBuf, CoreError> {
    let root = fs::canonicalize(root)?;
    let production = fs::canonicalize(root.join(production_path))?;
    if production == root
        || !production.is_dir()
        || !production.starts_with(&root)
        || !production.join("project.json").is_file()
        || !fs::canonicalize(production.join("project.json"))?.starts_with(&production)
        || production
            .parent()
            .is_none_or(|parent| !parent.starts_with(&root))
    {
        return Err(error(
            "Abra um projeto de produção do OpenMontage dentro do projeto atual.",
        ));
    }
    Ok(production)
}

pub(crate) async fn open_board(
    home: &Path,
    root: &Path,
    production_path: &str,
    state: &BacklotState,
) -> Result<BoardLocation, CoreError> {
    let production = production_directory(root, production_path)?;
    let mut active = state.0.lock().await;
    if let Some(board) = active.as_mut() {
        let running = board
            .child
            .try_wait()
            .map_err(|_| error("Painel de produção indisponível."))?
            .is_none();
        if board.production == production && running {
            return Ok(board.location.clone());
        }
        if running {
            Box::into_pin(board.child.kill())
                .await
                .map_err(|_| error("Não foi possível fechar o painel anterior."))?;
        }
        *active = None;
    }
    let generation = installed(home, ComponentId::Openmontage)?.path(home)?;
    let runtime = Runtime::at(&generation)?;
    // Board only reads local artifacts; it never receives provider credentials.
    let environment = runtime.environment;
    let listener =
        TcpListener::bind(("127.0.0.1", 0)).map_err(|_| error("Porta local indisponível."))?;
    let port = listener
        .local_addr()
        .map_err(|_| error("Porta local indisponível."))?
        .port();
    drop(listener);
    let mut command = tokio::process::Command::new(&runtime.python);
    command
        .arg(runtime.package.join("jarvis_board.py"))
        .arg(&production)
        .arg(port.to_string())
        .env_clear()
        .envs(environment)
        .current_dir(&runtime.package)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    crate::background::prepare_node(&mut command)?;
    let mut wrapped = process_wrap::tokio::CommandWrap::from(command);
    #[cfg(unix)]
    wrapped.wrap(process_wrap::tokio::ProcessGroup::leader());
    #[cfg(windows)]
    crate::background::windows_job(&mut wrapped);
    wrapped.wrap(process_wrap::tokio::KillOnDrop);
    let mut child = wrapped
        .spawn()
        .map_err(|_| error("Não foi possível iniciar o painel de produção."))?;
    let base = format!("http://127.0.0.1:{port}");
    let client = reqwest::Client::builder()
        .no_proxy()
        .connect_timeout(Duration::from_millis(250))
        .timeout(Duration::from_millis(500))
        .build()
        .map_err(|_| error("Painel de produção indisponível."))?;
    let wait = async {
        loop {
            if child
                .try_wait()
                .map_err(|_| error("Painel indisponível."))?
                .is_some()
            {
                return Err(error("O painel de produção não iniciou. Execute Diagnóstico e Reparo do OpenMontage."));
            }
            if let Ok(response) = client.get(format!("{base}/api/health")).send().await {
                if response.status().is_success() {
                    if let Ok(value) = response.json::<serde_json::Value>().await {
                        if value["app"] == "backlot" && value["ok"] == true {
                            return Ok(());
                        }
                    }
                }
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    };
    tokio::time::timeout(Duration::from_secs(15), wait)
        .await
        .map_err(|_| error("O painel demorou para iniciar. Tente novamente."))??;
    let name = production
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| error("Nome de produção inválido."))?;
    let mut url = reqwest::Url::parse(&base).map_err(|_| error("Endereço local inválido."))?;
    url.set_path(&format!("/p/{name}"));
    let location = BoardLocation {
        url: url.into(),
        production_path: production.to_string_lossy().into_owned(),
    };
    *active = Some(BoardProcess {
        production,
        location: location.clone(),
        child,
    });
    Ok(location)
}

pub(crate) fn show_board(
    app: &tauri::AppHandle,
    location: &BoardLocation,
) -> Result<(), CoreError> {
    let url = reqwest::Url::parse(&location.url).map_err(|_| error("Endereço local inválido."))?;
    if url.scheme() != "http"
        || url.host_str() != Some("127.0.0.1")
        || url.port().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || !url.path().starts_with("/p/")
    {
        return Err(error(
            "O painel de produção aceita somente seu servidor local.",
        ));
    }
    if let Some(window) = app.get_webview_window("openmontage-board") {
        window
            .navigate(url)
            .map_err(|_| error("Não foi possível atualizar o painel de produção."))?;
        window
            .show()
            .and_then(|()| window.set_focus())
            .map_err(|_| error("Não foi possível abrir o painel de produção."))?;
        return Ok(());
    }
    WebviewWindowBuilder::new(app, "openmontage-board", WebviewUrl::External(url))
        .title("Produção de vídeo · Jarvis")
        .inner_size(1240., 800.)
        .min_inner_size(760., 520.)
        .build()
        .map_err(|_| error("Não foi possível abrir a janela de produção."))?;
    Ok(())
}

#[tauri::command]
pub async fn open_openmontage_board(
    app: tauri::AppHandle,
    state: tauri::State<'_, BacklotState>,
    library: tauri::State<'_, crate::persistence::AppState>,
    project_id: String,
    production_path: String,
) -> Result<BoardLocation, CoreError> {
    let home = app
        .path()
        .home_dir()
        .map_err(|_| error("Pasta pessoal indisponível."))?;
    let root = crate::library::project_directory(&library, &home, &project_id)
        .map_err(|_| error("Projeto indisponível."))?;
    let location = open_board(&home, &root, &production_path, &state).await?;
    show_board(&app, &location)?;
    Ok(location)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    #[test]
    fn board_is_scoped_to_a_real_production_and_rejects_parent_projects() {
        let workspace = tempfile::tempdir().unwrap();
        let root = workspace.path().join("project");
        let production = root.join("videos/launch");
        fs::create_dir_all(&production).unwrap();
        fs::write(production.join("project.json"), "{}").unwrap();
        assert_eq!(
            production_directory(&root, "videos/launch").unwrap(),
            fs::canonicalize(&production).unwrap()
        );
        assert!(production_directory(&root, ".").is_err());
        assert!(production_directory(&root, "..").is_err());
        let outside = workspace.path().join("outside");
        fs::create_dir(&outside).unwrap();
        fs::write(outside.join("project.json"), "{}").unwrap();
        assert!(production_directory(&root, outside.to_str().unwrap()).is_err());
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&outside, root.join("link")).unwrap();
            assert!(production_directory(&root, "link").is_err());
        }
    }

    pub(crate) async fn smoke(home: &Path) {
        let project = tempfile::tempdir_in(home).unwrap();
        let production = project.path().join("production");
        fs::create_dir(&production).unwrap();
        fs::write(production.join("project.json"), "{}").unwrap();
        let state = BacklotState::default();
        let first = open_board(home, project.path(), "production", &state)
            .await
            .unwrap();
        assert_eq!(
            open_board(home, project.path(), "production", &state)
                .await
                .unwrap()
                .url,
            first.url
        );
        let origin = reqwest::Url::parse(&first.url).unwrap();
        let base = format!("http://127.0.0.1:{}", origin.port().unwrap());
        let client = reqwest::Client::builder().no_proxy().build().unwrap();
        let projects: serde_json::Value = client
            .get(format!("{base}/api/projects"))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(projects.as_array().unwrap().len(), 1);
        assert_eq!(projects[0]["project_id"], "production");
        assert_eq!(
            client
                .get(format!("{base}/api/project/other-project/state"))
                .send()
                .await
                .unwrap()
                .status(),
            reqwest::StatusCode::NOT_FOUND
        );
        {
            let mut active = state.0.lock().await;
            Box::into_pin(active.as_mut().unwrap().child.kill())
                .await
                .unwrap();
        }
        let reopened = open_board(home, project.path(), "production", &state)
            .await
            .unwrap();
        assert!(client
            .get(
                reqwest::Url::parse(&reopened.url)
                    .unwrap()
                    .join("/api/health")
                    .unwrap()
            )
            .send()
            .await
            .unwrap()
            .status()
            .is_success());
        let mut active = state.0.lock().await;
        Box::into_pin(active.as_mut().unwrap().child.kill())
            .await
            .unwrap();
        *active = None;
    }
}
