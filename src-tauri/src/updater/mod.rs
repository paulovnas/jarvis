//! App updates stay native. The renderer cannot provide download URLs or bypass signatures.
pub(crate) mod relaunch;

use serde::{Deserialize, Serialize};
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};
use tauri::{ipc::Channel, Manager};
use tauri_plugin_updater::{Update, UpdaterExt};

const REPOSITORY: &str = "paulovnas/jarvis";
const RELEASES_API: &str = "https://api.github.com/repos/paulovnas/jarvis/releases?per_page=100";
const MAX_RELEASES_BYTES: usize = 4 * 1024 * 1024;

#[derive(Default)]
pub struct UpdateState {
    operation: tokio::sync::Mutex<()>,
    activity: Arc<tokio::sync::RwLock<()>>,
    candidate: Mutex<Option<Update>>,
    installed: Mutex<Option<String>>,
}

pub(crate) type ActivityLease = tokio::sync::OwnedRwLockReadGuard<()>;

pub(crate) fn begin_activity(app: &tauri::AppHandle) -> Result<ActivityLease, String> {
    app.state::<UpdateState>()
        .activity
        .clone()
        .try_read_owned()
        .map_err(|_| "Aguarde a atualização do Jarvis terminar.".into())
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AvailableUpdate {
    version: String,
    notes: String,
    published_at: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckResult {
    current_version: String,
    available: Option<AvailableUpdate>,
    installable: bool,
}

#[derive(Clone, Serialize)]
#[serde(tag = "stage", rename_all = "camelCase")]
pub enum Progress {
    Downloading { downloaded: u64, total: Option<u64> },
    Verifying,
    Installing,
    Restarting,
}

#[derive(Deserialize)]
struct ReleaseAsset {
    name: String,
    browser_download_url: String,
}
#[derive(Deserialize)]
struct Release {
    tag_name: String,
    draft: bool,
    prerelease: bool,
    #[serde(default)]
    assets: Vec<ReleaseAsset>,
}

fn platform_key() -> String {
    let os = if cfg!(target_os = "macos") {
        "darwin"
    } else {
        std::env::consts::OS
    };
    format!("{os}-{}", std::env::consts::ARCH)
}

fn release_url(url: &str, tag: &str) -> bool {
    let Ok(url) = url::Url::parse(url) else {
        return false;
    };
    url.scheme() == "https"
        && url.host_str() == Some("github.com")
        && url.port_or_known_default() == Some(443)
        && url.username().is_empty()
        && url.password().is_none()
        && url.query().is_none()
        && url.fragment().is_none()
        && url
            .path()
            .starts_with(&format!("/{REPOSITORY}/releases/download/{tag}/"))
}

fn select_release(
    releases: &[Release],
    current: &semver::Version,
    platform: &str,
) -> Option<(semver::Version, url::Url)> {
    let name = format!("latest-{platform}.json");
    releases
        .iter()
        .filter_map(|release| {
            let version = semver::Version::parse(release.tag_name.strip_prefix('v')?).ok()?;
            if release.draft
                || version <= *current
                || (current.pre.is_empty() && (release.prerelease || !version.pre.is_empty()))
            {
                return None;
            }
            let asset = release.assets.iter().find(|asset| {
                asset.name == name && release_url(&asset.browser_download_url, &release.tag_name)
            })?;
            Some((version, url::Url::parse(&asset.browser_download_url).ok()?))
        })
        .max_by(|a, b| a.0.cmp(&b.0))
}

fn installable(app: &tauri::AppHandle) -> bool {
    let Ok(binary) = tauri::process::current_binary(&app.env()) else {
        return false;
    };
    #[cfg(target_os = "macos")]
    return binary
        .parent()
        .and_then(|p| p.parent())
        .and_then(|p| p.parent())
        .is_some_and(|p| p.extension().is_some_and(|e| e == "app") && !p.starts_with("/Volumes"));
    #[cfg(not(target_os = "macos"))]
    {
        !cfg!(debug_assertions) && binary.is_file()
    }
}

fn require_idle(app: &tauri::AppHandle) -> Result<(), String> {
    if app.state::<crate::agent::AgentState>().busy_for_update()
        || app.state::<crate::core::CoreState>().busy_for_update()
    {
        return Err(
            "Aguarde as execuções e encerre os processos ativos antes de atualizar.".into(),
        );
    }
    Ok(())
}

#[tauri::command]
pub async fn check_app_update(
    app: tauri::AppHandle,
    state: tauri::State<'_, UpdateState>,
) -> Result<CheckResult, String> {
    let _operation = state
        .operation
        .try_lock()
        .map_err(|_| "Uma atualização já está em andamento.")?;
    let current = app.package_info().version.clone();
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .connect_timeout(Duration::from_secs(10))
        .user_agent(format!("Jarvis/{current}"))
        .build()
        .map_err(|_| "Não foi possível iniciar a consulta.")?;
    let mut response = client
        .get(RELEASES_API)
        .header("accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|_| "Não foi possível consultar as atualizações. Tente novamente.")?;
    if !response.status().is_success() {
        return Err("O GitHub não respondeu à consulta. Tente novamente mais tarde.".into());
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| "A consulta foi interrompida.")?
    {
        if bytes.len() + chunk.len() > MAX_RELEASES_BYTES {
            return Err("A lista de versões excedeu o limite de leitura.".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    let releases: Vec<Release> = serde_json::from_slice(&bytes)
        .map_err(|_| "O GitHub retornou uma lista de versões inválida.")?;
    let mut candidate = None;
    if let Some((version, endpoint)) = select_release(&releases, &current, &platform_key()) {
        let exit_app = app.clone();
        let updater = app
            .updater_builder()
            .on_before_exit(move || crate::prepare_exit_for_update(&exit_app))
            .endpoints(vec![endpoint])
            .map_err(|_| "Endereço de atualização inválido.")?
            .timeout(Duration::from_secs(20))
            .build()
            .map_err(|_| "Atualizador indisponível.")?;
        candidate = updater
            .check()
            .await
            .map_err(|_| "Não foi possível ler os detalhes desta atualização.")?;
        if let Some(update) = &mut candidate {
            if update.version != version.to_string()
                || !release_url(update.download_url.as_str(), &format!("v{version}"))
            {
                return Err("A atualização não corresponde ao release publicado.".into());
            }
            // The manifest request is short; a complete app download may take longer.
            update.timeout = Some(Duration::from_secs(20 * 60));
        }
    }
    let available = candidate.as_ref().map(|update| AvailableUpdate {
        version: update.version.clone(),
        notes: update.body.clone().unwrap_or_default(),
        published_at: update.date.map(|date| date.to_string()),
    });
    *state
        .candidate
        .lock()
        .map_err(|_| "Atualizador indisponível.")? = candidate;
    Ok(CheckResult {
        current_version: current.to_string(),
        available,
        installable: installable(&app),
    })
}

#[tauri::command]
pub async fn install_app_update(
    app: tauri::AppHandle,
    state: tauri::State<'_, UpdateState>,
    on_progress: Channel<Progress>,
) -> Result<(), String> {
    let _operation = state
        .operation
        .try_lock()
        .map_err(|_| "Uma atualização já está em andamento.")?;
    // Exclude admission of new runs throughout download, installation and relaunch.
    let _activity = state
        .activity
        .try_write()
        .map_err(|_| "Aguarde as execuções e instalações ativas antes de atualizar.")?;
    if !installable(&app) {
        return Err("Abra o Jarvis instalado no computador para atualizar.".into());
    }
    require_idle(&app)?;
    let already_installed = state
        .installed
        .lock()
        .map_err(|_| "Atualizador indisponível.")?
        .clone();
    let version = if let Some(version) = already_installed {
        version
    } else {
        let mut update = state
            .candidate
            .lock()
            .map_err(|_| "Atualizador indisponível.")?
            .clone()
            .ok_or("Verifique as atualizações antes de instalar.")?;
        update = update.restart_after_install(true);
        let mut downloaded = 0u64;
        let mut last_progress = std::time::Instant::now();
        let _ = on_progress.send(Progress::Downloading {
            downloaded: 0,
            total: None,
        });
        let bytes = update.download(|count, total| {
            downloaded = downloaded.saturating_add(count as u64);
            if last_progress.elapsed() >= Duration::from_millis(100) || total == Some(downloaded) {
                let _ = on_progress.send(Progress::Downloading { downloaded, total });
                last_progress = std::time::Instant::now();
            }
        }, || { let _ = on_progress.send(Progress::Verifying); }).await
            .map_err(|_| "O download ou a assinatura não pôde ser verificado. Nada foi instalado; tente novamente.")?;
        require_idle(&app)?;
        let _ = on_progress.send(Progress::Installing);
        crate::desktop::flush(&app);
        let version = update.version.clone();
        tauri::async_runtime::spawn_blocking(move || update.install(bytes)).await
            .map_err(|_| "Não foi possível instalar a atualização.")?
            .map_err(|_| "Não foi possível substituir o aplicativo. Verifique a permissão de escrita da instalação.")?;
        // A successful Windows install exits the process and lets NSIS restart it.
        // Never start a competing successor while the installer owns replacement.
        if cfg!(windows) {
            return Err(
                "O instalador não confirmou o encerramento do Jarvis. Tente atualizar novamente."
                    .into(),
            );
        }
        *state
            .installed
            .lock()
            .map_err(|_| "Atualizador indisponível.")? = Some(version.clone());
        version
    };
    let _ = on_progress.send(Progress::Restarting);
    crate::desktop::flush(&app);
    relaunch::launch_updated(&app, &version).await?;
    // The successor has shown its window. Never exit merely because spawning was attempted.
    crate::prepare_exit_for_update(&app);
    app.exit(0);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn release(version: &str, draft: bool, prerelease: bool, platform: &str) -> Release {
        Release { tag_name: format!("v{version}"), draft, prerelease, assets: vec![ReleaseAsset { name: format!("latest-{platform}.json"), browser_download_url: format!("https://github.com/{REPOSITORY}/releases/download/v{version}/latest-{platform}.json") }] }
    }
    #[test]
    fn beta_finds_prereleases_by_semver_while_stable_skips_them_and_incomplete_releases() {
        let releases = vec![
            release("0.8.0-beta.2", false, true, "darwin-aarch64"),
            release("0.8.0-beta.10", false, true, "darwin-aarch64"),
            release("0.9.0", true, false, "darwin-aarch64"),
            release("0.8.0", false, false, "darwin-x86_64"),
        ];
        assert_eq!(
            select_release(
                &releases,
                &semver::Version::parse("0.8.0-beta.1").unwrap(),
                "darwin-aarch64"
            )
            .unwrap()
            .0
            .to_string(),
            "0.8.0-beta.10"
        );
        assert!(select_release(
            &releases,
            &semver::Version::parse("0.7.0").unwrap(),
            "darwin-aarch64"
        )
        .is_none());
        assert!(select_release(
            &releases,
            &semver::Version::parse("0.8.0-beta.10").unwrap(),
            "darwin-aarch64"
        )
        .is_none());
        assert_eq!(
            select_release(
                &[release("0.8.0", false, false, "darwin-aarch64")],
                &semver::Version::parse("0.8.0-beta.10").unwrap(),
                "darwin-aarch64"
            )
            .unwrap()
            .0
            .to_string(),
            "0.8.0"
        );
    }
    #[test]
    fn release_assets_must_belong_to_the_expected_repository_and_tag() {
        for url in [
            "https://evil.example/paulovnas/jarvis/releases/download/v0.8.0/x",
            "https://github.com/other/jarvis/releases/download/v0.8.0/x",
            "http://github.com/paulovnas/jarvis/releases/download/v0.8.0/x",
            "https://github.com/paulovnas/jarvis/releases/download/v0.8.1/x",
        ] {
            assert!(!release_url(url, "v0.8.0"));
        }
    }
    #[test]
    fn windows_updates_require_the_windows_manifest_even_when_macos_is_newer() {
        let releases = vec![
            release("0.8.6-beta", false, true, "windows-x86_64"),
            release("0.8.7-beta", false, true, "darwin-aarch64"),
        ];
        let (version, endpoint) = select_release(
            &releases,
            &semver::Version::parse("0.8.5-beta").unwrap(),
            "windows-x86_64",
        )
        .unwrap();
        assert_eq!(version.to_string(), "0.8.6-beta");
        assert!(endpoint.path().ends_with("/latest-windows-x86_64.json"));
    }
    #[test]
    fn updates_and_active_work_are_mutually_exclusive_and_failures_release_the_gate() {
        let state = UpdateState::default();
        let work = state.activity.clone().try_read_owned().unwrap();
        assert!(state.activity.try_write().is_err());
        drop(work);
        let update = state.activity.try_write().unwrap();
        assert!(state.activity.clone().try_read_owned().is_err());
        drop(update);
        assert!(state.activity.clone().try_read_owned().is_ok());
    }
}
