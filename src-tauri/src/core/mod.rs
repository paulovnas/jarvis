//! Jarvis-owned packages. Installation readiness is independent of provider setup.
pub mod beads;
pub mod context;
pub mod context7;
pub mod design;
pub mod health;
pub mod hooks;
mod install;
pub mod lsp;
pub mod ponytail;

use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    path::{Component, Path, PathBuf},
    sync::{Arc, Mutex},
};
use tauri::{Emitter, Manager};

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "kebab-case")]
pub enum ComponentId {
    ContextMode,
    Ponytail,
    Beads,
    OpenDesign,
    Context7,
    Lsp,
}
impl ComponentId {
    pub const ALL: [Self; 6] = [
        Self::ContextMode,
        Self::Ponytail,
        Self::Beads,
        Self::OpenDesign,
        Self::Context7,
        Self::Lsp,
    ];
    pub fn key(self) -> &'static str {
        match self {
            Self::ContextMode => "context-mode",
            Self::Ponytail => "ponytail",
            Self::Beads => "beads",
            Self::OpenDesign => "open-design",
            Self::Context7 => "context7",
            Self::Lsp => "lsp",
        }
    }
    fn name(self) -> &'static str {
        match self {
            Self::ContextMode => "Context-mode",
            Self::Ponytail => "Ponytail",
            Self::Beads => "Beads",
            Self::OpenDesign => "Open Design",
            Self::Context7 => "Context7",
            Self::Lsp => "Servidores LSP",
        }
    }
    fn repository(self) -> &'static str {
        match self {
            Self::ContextMode => "mksglu/context-mode",
            Self::Ponytail => "DietrichGebert/ponytail",
            Self::Beads => "gastownhall/beads",
            Self::OpenDesign => "nexu-io/open-design",
            Self::Context7 => "upstash/context7",
            Self::Lsp => "typescript-language-server/typescript-language-server",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct CoreError {
    pub code: &'static str,
    pub message: String,
}
pub fn error(message: impl Into<String>) -> CoreError {
    CoreError {
        code: "core_error",
        message: message.into(),
    }
}
pub fn cancelled_error() -> CoreError {
    CoreError {
        code: "cancelled",
        message: "Execução do Core interrompida.".into(),
    }
}
impl From<std::io::Error> for CoreError {
    fn from(_: std::io::Error) -> Self {
        error("Não foi possível acessar ~/.jarvis/core. Confira o espaço e as permissões.")
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Installation {
    pub version: String,
    pub directory: String,
    pub files: Vec<String>,
}
#[derive(Default, Deserialize, Serialize)]
pub struct Manifest {
    pub installations: BTreeMap<ComponentId, Installation>,
}
pub fn root(home: &Path) -> PathBuf {
    home.join(".jarvis/core")
}
fn read_manifest(home: &Path) -> Result<Manifest, CoreError> {
    match fs::read(root(home).join("manifest.json")) {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(|_| {
            error("O registro do Core está inválido. Restaure ~/.jarvis/core/manifest.json.")
        }),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Manifest::default()),
        Err(e) => Err(e.into()),
    }
}
fn save_manifest(home: &Path, manifest: &Manifest) -> Result<(), CoreError> {
    let parent = root(home);
    fs::create_dir_all(&parent)?;
    let mut tmp = tempfile::NamedTempFile::new_in(&parent)?;
    tmp.write_all(
        &serde_json::to_vec_pretty(manifest).map_err(|_| error("Registro do Core inválido."))?,
    )?;
    tmp.as_file().sync_all()?;
    tmp.persist(parent.join("manifest.json"))
        .map_err(|_| error("Não foi possível salvar o registro do Core."))?;
    #[cfg(unix)]
    fs::File::open(parent)?.sync_all()?;
    Ok(())
}
fn relative(value: &str) -> bool {
    !value.is_empty()
        && Path::new(value)
            .components()
            .all(|part| matches!(part, Component::Normal(_)))
}
impl Installation {
    fn validate(&self, home: &Path, id: ComponentId) -> Result<PathBuf, CoreError> {
        let path = self.path(home)?;
        if id == ComponentId::Ponytail {
            ponytail::Ponytail::at(&path, &self.version)?;
        }
        if id == ComponentId::OpenDesign {
            design::Pack::at(&path, &self.version)?;
        }
        Ok(path)
    }
    pub fn path(&self, home: &Path) -> Result<PathBuf, CoreError> {
        if !relative(&self.directory)
            || self.files.is_empty()
            || self.files.iter().any(|f| !relative(f))
        {
            return Err(error("Caminho inválido no registro do Core."));
        }
        let plain = root(home).join(&self.directory);
        let path = fs::canonicalize(&plain)?;
        let base = fs::canonicalize(root(home))?;
        if !path.starts_with(&base) {
            return Err(error("O Core precisa estar dentro de ~/.jarvis."));
        }
        for file in &self.files {
            if !plain.join(file).is_file()
                || !fs::canonicalize(plain.join(file))?.starts_with(&path)
            {
                return Err(error("Instalação incompleta. Reinstale o componente."));
            }
        }
        // Windows canonicalize() returns a \\?\ verbatim path: node cannot resolve
        // module paths under it and CreateProcessW rejects its mixed separators.
        // Containment was proven above, so callers receive the plain path.
        Ok(plain)
    }
}
pub fn installed(home: &Path, id: ComponentId) -> Result<Installation, CoreError> {
    let item = read_manifest(home)?
        .installations
        .remove(&id)
        .ok_or_else(|| {
            error(format!(
                "Instale {} em Configurações → Ferramentas → Core.",
                id.name()
            ))
        })?;
    item.validate(home, id)?;
    Ok(item)
}
pub fn require_ready(home: &Path) -> Result<(), CoreError> {
    for id in ComponentId::ALL {
        installed(home, id)?;
    }
    if !context7::configured(home) {
        return Err(error(
            "Configure a chave do Context7 em Ferramentas → Core.",
        ));
    }
    Ok(())
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadProgress {
    received_bytes: u64,
    total_bytes: Option<u64>,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CoreDownloadEvent {
    id: ComponentId,
    download: DownloadProgress,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CoreItem {
    id: ComponentId,
    name: String,
    repository: String,
    installed_version: Option<String>,
    latest_version: Option<String>,
    update_available: bool,
    installed: bool,
    configured: bool,
    stage: Option<String>,
    download: Option<DownloadProgress>,
    error: Option<String>,
    health_error: Option<String>,
    diagnostics: Vec<health::Check>,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub ready: bool,
    pub items: Vec<CoreItem>,
    pub checking: bool,
}
#[derive(Default)]
struct StateData {
    latest: BTreeMap<ComponentId, String>,
    errors: BTreeMap<ComponentId, String>,
    stages: BTreeMap<ComponentId, String>,
    downloads: BTreeMap<ComponentId, DownloadProgress>,
    checking: bool,
    diagnostics: BTreeMap<ComponentId, Vec<health::Check>>,
}
#[derive(Clone, Default)]
pub struct CoreState {
    data: Arc<Mutex<StateData>>,
    install_lock: Arc<tokio::sync::Mutex<()>>,
}
impl CoreState {
    pub(crate) fn busy_for_update(&self) -> bool {
        self.install_lock.try_lock().is_err()
    }
    pub fn require_ready(&self, home: &Path) -> Result<(), CoreError> {
        require_ready(home)?;
        if self.snapshot(home)?.ready {
            Ok(())
        } else {
            Err(error(
                "O Core precisa de atenção. Abra Diagnóstico e Reparo.",
            ))
        }
    }
    pub fn snapshot(&self, home: &Path) -> Result<Snapshot, CoreError> {
        let manifest_result = read_manifest(home);
        let manifest_error = manifest_result.as_ref().err().map(|e| e.message.clone());
        let manifest = manifest_result.unwrap_or_default();
        let data = self.data.lock().map_err(|_| error("Core indisponível."))?;
        let items: Vec<_> = ComponentId::ALL
            .into_iter()
            .map(|id| {
                let record = manifest.installations.get(&id);
                let validation = record.map(|r| r.validate(home, id));
                let valid = validation.as_ref().is_some_and(|result| result.is_ok());
                let latest = data.latest.get(&id).cloned();
                let diagnostics = data.diagnostics.get(&id).cloned().unwrap_or_default();
                let health_error = manifest_error.clone().or_else(|| {
                    diagnostics
                        .iter()
                        .find(|c| !c.passed)
                        .map(|c| c.message.clone())
                });
                CoreItem {
                    id,
                    name: id.name().into(),
                    repository: format!("https://github.com/{}", id.repository()),
                    installed_version: record.map(|r| r.version.clone()),
                    latest_version: latest.clone(),
                    installed: valid,
                    configured: valid
                        && (id != ComponentId::Context7 || context7::configured(home)),
                    update_available: valid
                        && record
                            .zip(latest.as_ref())
                            .is_some_and(|(r, v)| newer(v, &r.version)),
                    stage: data.stages.get(&id).cloned(),
                    download: data.downloads.get(&id).cloned(),
                    health_error,
                    diagnostics,
                    error: data.errors.get(&id).cloned().or_else(|| {
                        validation
                            .as_ref()
                            .and_then(|result| result.as_ref().err())
                            .map(|cause| cause.message.clone())
                    }),
                }
            })
            .collect();
        Ok(Snapshot {
            ready: items
                .iter()
                .all(|i| i.installed && i.configured && i.health_error.is_none()),
            items,
            checking: data.checking,
        })
    }
    fn stage(&self, app: &tauri::AppHandle, home: &Path, id: ComponentId, stage: &str) {
        if let Ok(mut data) = self.data.lock() {
            data.stages.insert(id, stage.into());
            data.downloads.remove(&id);
        }
        self.emit(app, home);
    }
    fn download(&self, app: &tauri::AppHandle, id: ComponentId, download: DownloadProgress) {
        if let Ok(mut data) = self.data.lock() {
            data.downloads.insert(id, download.clone());
        }
        // Byte updates must not revalidate every installed resource pack on disk.
        let _ = app.emit("core:download", CoreDownloadEvent { id, download });
    }
    fn emit(&self, app: &tauri::AppHandle, home: &Path) {
        if let Ok(snapshot) = self.snapshot(home) {
            let _ = app.emit("core:changed", snapshot);
        }
    }
}
fn newer(candidate: &str, current: &str) -> bool {
    match (
        semver::Version::parse(candidate),
        semver::Version::parse(current),
    ) {
        (Ok(a), Ok(b)) => a > b,
        _ => false,
    }
}
#[tauri::command]
pub async fn get_core_status(
    app: tauri::AppHandle,
    core: tauri::State<'_, CoreState>,
) -> Result<Snapshot, CoreError> {
    let home = app
        .path()
        .home_dir()
        .map_err(|_| error("Pasta pessoal indisponível."))?;
    core.snapshot(&home)
}
#[tauri::command]
pub async fn check_core_updates(
    app: tauri::AppHandle,
    core: tauri::State<'_, CoreState>,
) -> Result<Snapshot, CoreError> {
    let home = app
        .path()
        .home_dir()
        .map_err(|_| error("Pasta pessoal indisponível."))?;
    {
        let mut data = core.data.lock().map_err(|_| error("Core indisponível."))?;
        if data.checking {
            drop(data);
            return core.snapshot(&home);
        }
        data.checking = true;
    }
    core.emit(&app, &home);
    for id in ComponentId::ALL {
        let result = install::component_release(id).await;
        if let Ok(mut data) = core.data.lock() {
            match result {
                Ok(release) => {
                    data.latest.insert(id, release.version());
                    data.errors.remove(&id);
                }
                Err(cause) => {
                    data.errors.insert(id, cause.message);
                }
            }
        }
    }
    if let Ok(mut data) = core.data.lock() {
        data.checking = false;
    }
    core.emit(&app, &home);
    core.snapshot(&home)
}
#[tauri::command]
pub async fn install_core_component(
    app: tauri::AppHandle,
    core: tauri::State<'_, CoreState>,
    id: ComponentId,
) -> Result<Snapshot, CoreError> {
    let _activity = crate::updater::begin_activity(&app).map_err(|message| error(&message))?;
    let _lock = core
        .install_lock
        .try_lock()
        .map_err(|_| error("Aguarde a instalação atual terminar."))?;
    let home = app
        .path()
        .home_dir()
        .map_err(|_| error("Pasta pessoal indisponível."))?;
    fs::create_dir_all(root(&home))?;
    let lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(root(&home).join("install.lock"))?;
    fs2::FileExt::try_lock_exclusive(&lock)
        .map_err(|_| error("Outro Jarvis está instalando o Core."))?;
    if let Ok(mut data) = core.data.lock() {
        data.errors.remove(&id);
    }
    core.stage(&app, &home, id, "Consultando release");
    let result = install::install(
        &home,
        id,
        |stage| core.stage(&app, &home, id, stage),
        |download| core.download(&app, id, download),
    )
    .await;
    if let Ok(mut data) = core.data.lock() {
        data.stages.remove(&id);
        data.downloads.remove(&id);
        match &result {
            Ok(version) => {
                data.latest.insert(id, version.clone());
                data.diagnostics.remove(&id);
            }
            Err(cause) => {
                data.errors.insert(id, cause.message.clone());
            }
        }
    }
    core.emit(&app, &home);
    result?;
    core.snapshot(&home)
}

#[cfg(test)]
mod tests;
