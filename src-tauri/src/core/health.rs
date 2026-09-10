//! Local health checks and scoped recovery. Network availability is not Core health.
use super::*;
use std::time::{Duration, Instant};

#[derive(Clone, Serialize)]
pub struct Check {
    label: String,
    pub(super) passed: bool,
    pub(super) message: String,
}
impl Check {
    fn result(label: &str, result: Result<(), CoreError>, success: &str) -> Self {
        Self {
            label: label.into(),
            passed: result.is_ok(),
            message: result.err().map_or_else(|| success.into(), |e| e.message),
        }
    }
}

const RECEIPT: &str = "jarvis-installation.json";

pub(super) fn save_receipt(
    home: &Path,
    id: ComponentId,
    record: &Installation,
) -> Result<(), CoreError> {
    let path = package_path(home, id, record)?;
    let mut file = tempfile::NamedTempFile::new_in(&path)?;
    file.write_all(&serde_json::to_vec(record).map_err(|_| error("Registro inválido."))?)?;
    file.as_file().sync_all()?;
    file.persist(path.join(RECEIPT))
        .map_err(|_| error("Não foi possível salvar o registro de recuperação."))?;
    Ok(())
}

// Only generation directories belonging to this component may be repaired or removed.
fn package_path(home: &Path, id: ComponentId, record: &Installation) -> Result<PathBuf, CoreError> {
    let relative_path = Path::new(&record.directory);
    if !relative(&record.directory)
        || relative_path.components().count() != 2
        || relative_path
            .components()
            .next()
            .and_then(|p| p.as_os_str().to_str())
            != Some(id.key())
    {
        return Err(error("Pasta do componente fora do local esperado."));
    }
    let base = root(home);
    let component = base.join(id.key());
    let path = base.join(relative_path);
    for directory in [&base, &component, &path] {
        if fs::symlink_metadata(directory)?.file_type().is_symlink() {
            return Err(error("O reparo não aceita atalhos nas pastas do Core."));
        }
    }
    let canonical = fs::canonicalize(&path)?;
    if !canonical.starts_with(fs::canonicalize(base)?) {
        return Err(error("Pasta do Core inválida."));
    }
    Ok(canonical)
}

fn recover_manifest(home: &Path) -> Result<(), CoreError> {
    let path = root(home).join("manifest.json");
    if path.exists() && read_manifest(home).is_ok() {
        return Ok(());
    }
    let bytes = match fs::read(&path) {
        Ok(bytes) => bytes,
        Err(cause) if cause.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(cause) => return Err(cause.into()),
    };
    let mut recovered = Manifest::default();
    // Preserve individually parseable entries when one entry broke the whole manifest.
    if let Ok(value) = serde_json::from_slice::<serde_json::Value>(&bytes) {
        for id in ComponentId::ALL {
            if let Ok(record) =
                serde_json::from_value::<Installation>(value["installations"][id.key()].clone())
            {
                recovered.installations.insert(id, record);
            }
        }
    }
    for id in ComponentId::ALL {
        if recovered.installations.contains_key(&id) {
            continue;
        }
        let Ok(entries) = fs::read_dir(root(home).join(id.key())) else {
            continue;
        };
        let mut candidates = Vec::new();
        for entry in entries.flatten() {
            let Ok(bytes) = fs::read(entry.path().join(RECEIPT)) else {
                continue;
            };
            let Ok(record) = serde_json::from_slice::<Installation>(&bytes) else {
                continue;
            };
            if package_path(home, id, &record).ok().as_ref()
                != fs::canonicalize(entry.path()).ok().as_ref()
                || record.validate(home, id).is_err()
            {
                continue;
            }
            candidates.push(record);
        }
        candidates.sort_by(|a, b| {
            semver::Version::parse(&a.version)
                .ok()
                .cmp(&semver::Version::parse(&b.version).ok())
                .then_with(|| a.directory.cmp(&b.directory))
        });
        if let Some(record) = candidates.pop() {
            recovered.installations.insert(id, record);
        }
    }
    // Keep the damaged bytes and every package on disk, even if no receipt is recoverable.
    if path.exists() {
        let mut backup = tempfile::Builder::new()
            .prefix("manifest-damaged-")
            .suffix(".json")
            .tempfile_in(root(home))?;
        backup.write_all(&bytes)?;
        backup.as_file().sync_all()?;
        backup
            .keep()
            .map_err(|_| error("Não foi possível preservar o registro original."))?;
    }
    save_manifest(home, &recovered)
}

fn retire_previous(home: &Path, id: ComponentId, old: &Installation) -> Result<(), CoreError> {
    let current = installed(home, id)?;
    if current.directory == old.directory {
        return Err(error("A instalação ativa deve ser preservada."));
    }
    // A damaged or redirected old path is left untouched, never followed outside this package.
    if let Ok(path) = package_path(home, id, old) {
        fs::remove_dir_all(path)?;
    }
    Ok(())
}

fn repair_local(home: &Path, id: ComponentId) -> Result<(), CoreError> {
    recover_manifest(home)?;
    let manifest = read_manifest(home)?;
    let record = manifest
        .installations
        .get(&id)
        .ok_or_else(|| error("Pacote ausente. Instale o componente."))?;
    let path = package_path(home, id, record)?;
    #[cfg(unix)]
    for executable in match id {
        ComponentId::ContextMode => vec![
            install::node_path(&path),
            install::node_path(&path).with_file_name("bun"),
        ],
        ComponentId::Context7 => vec![install::node_path(&path)],
        ComponentId::Lsp => vec![install::node_path(&path)],
        ComponentId::Beads => vec![path.join("bd"), path.join("dolt/bin/dolt")],
        _ => vec![],
    } {
        use std::os::unix::fs::PermissionsExt;
        let target = fs::canonicalize(&executable)?;
        if !target.starts_with(&path) || !target.is_file() {
            return Err(error("Executável fora do pacote."));
        }
        let mut permissions = fs::metadata(&target)?.permissions();
        permissions.set_mode(permissions.mode() | 0o100);
        fs::set_permissions(target, permissions)?;
    }
    if id == ComponentId::ContextMode {
        let mut file = tempfile::NamedTempFile::new_in(&path)?;
        file.write_all(include_bytes!("context-hook.mjs"))?;
        file.persist(path.join("jarvis-hook.mjs"))
            .map_err(|_| error("Não foi possível restaurar os hooks."))?;
    }
    record.validate(home, id)?;
    save_receipt(home, id, record)
}

async fn runtime(home: &Path, id: ComponentId, record: &Installation) -> Result<(), CoreError> {
    let path = record.validate(home, id)?;
    match id {
        ComponentId::ContextMode => context::verify(&path).await,
        ComponentId::Context7 => context7::verify(&path).await,
        ComponentId::Lsp => lsp::verify(&path).await,
        ComponentId::Beads => {
            for executable in [
                path.join(if cfg!(windows) { "bd.exe" } else { "bd" }),
                path.join(if cfg!(windows) {
                    "dolt/bin/dolt.exe"
                } else {
                    "dolt/bin/dolt"
                }),
            ] {
                let mut command = tokio::process::Command::new(executable);
                command.arg("version").current_dir(&path);
                install::command(command, 20).await?;
            }
            Ok(())
        }
        _ => Ok(()), // validate() parses the full Ponytail/Open Design resource contracts.
    }
}

async fn inspect(home: &Path, id: ComponentId) -> Vec<Check> {
    let record = installed(home, id);
    let mut checks = vec![Check::result(
        "Arquivos e recursos",
        record.as_ref().map(|_| ()).map_err(Clone::clone),
        "Integridade verificada",
    )];
    if let Ok(record) = record {
        let result = tokio::time::timeout(Duration::from_secs(90), runtime(home, id, &record))
            .await
            .unwrap_or_else(|_| {
                Err(error(
                    "O componente não respondeu ao diagnóstico. Tente reparar.",
                ))
            });
        checks.push(Check::result(
            "Execução local",
            result,
            "Verificação concluída",
        ));
        // A recovery receipt is best effort; failure to write it must not block a working Core.
        let _ = save_receipt(home, id, &record);
    }
    if id == ComponentId::Context7 {
        checks.push(Check::result(
            "Chave de API",
            context7::verify_credentials(home),
            "Chave disponível no Keychain",
        ));
    }
    checks
}

fn enforce(app: &tauri::AppHandle, core: &CoreState, home: &Path) {
    if !core.snapshot(home).is_ok_and(|s| s.ready) {
        app.state::<crate::agent::AgentState>()
            .stop_for_core_failure();
    }
    core.emit(app, home);
}

async fn diagnose(app: &tauri::AppHandle, core: &CoreState, home: &Path) -> Result<(), CoreError> {
    let _activity = crate::updater::begin_activity(app).map_err(error)?;
    let _lock = core
        .install_lock
        .try_lock()
        .map_err(|_| error("Aguarde a operação atual do Core."))?;
    for id in ComponentId::ALL {
        core.stage(app, home, id, "Analisando componente");
        let checks = inspect(home, id).await;
        {
            let mut data = core.data.lock().map_err(|_| error("Core indisponível."))?;
            data.diagnostics.insert(id, checks);
            data.stages.remove(&id);
        }
        enforce(app, core, home);
    }
    Ok(())
}

#[tauri::command]
pub async fn diagnose_core(
    app: tauri::AppHandle,
    core: tauri::State<'_, CoreState>,
) -> Result<Snapshot, CoreError> {
    let home = app
        .path()
        .home_dir()
        .map_err(|_| error("Pasta pessoal indisponível."))?;
    diagnose(&app, &core, &home).await?;
    core.snapshot(&home)
}

#[tauri::command]
pub async fn repair_core_component(
    app: tauri::AppHandle,
    core: tauri::State<'_, CoreState>,
    id: ComponentId,
    reinstall: bool,
) -> Result<Snapshot, CoreError> {
    let _activity = crate::updater::begin_activity(&app).map_err(error)?;
    let _lock = core
        .install_lock
        .try_lock()
        .map_err(|_| error("Aguarde a operação atual do Core."))?;
    let home = app
        .path()
        .home_dir()
        .map_err(|_| error("Pasta pessoal indisponível."))?;
    let agents = app.state::<crate::agent::AgentState>();
    agents.stop_for_core_failure();
    if agents.busy_for_update() {
        return Err(error(
            "Aguarde as execuções interrompidas encerrarem e tente novamente.",
        ));
    }
    fs::create_dir_all(root(&home))?;
    let lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(root(&home).join("install.lock"))?;
    fs2::FileExt::try_lock_exclusive(&lock)
        .map_err(|_| error("Outro Jarvis está alterando o Core."))?;
    core.stage(
        &app,
        &home,
        id,
        if reinstall {
            "Preparando reinstalação"
        } else {
            "Reparando componente"
        },
    );
    let result = async {
        recover_manifest(&home)?;
        if reinstall {
            let old = read_manifest(&home)?.installations.remove(&id);
            // Verify and publish a fresh package before deleting the previous generation.
            install::install(
                &home,
                id,
                |stage| core.stage(&app, &home, id, stage),
                |progress| core.download(&app, id, progress),
            )
            .await?;
            if let Some(old) = old {
                retire_previous(&home, id, &old)?;
            }
        } else {
            repair_local(&home, id)?;
        }
        Ok::<_, CoreError>(())
    }
    .await;
    let checks = inspect(&home, id).await;
    {
        let mut data = core.data.lock().map_err(|_| error("Core indisponível."))?;
        data.stages.remove(&id);
        data.downloads.remove(&id);
        data.diagnostics.insert(id, checks);
        if let Err(cause) = &result {
            data.errors.insert(id, cause.message.clone());
        } else {
            data.errors.remove(&id);
        }
    }
    enforce(&app, &core, &home);
    result?;
    core.snapshot(&home)
}

pub fn start_monitor(app: &tauri::AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let Ok(home) = app.path().home_dir() else {
            return;
        };
        let core = app.state::<CoreState>();
        let mut checked = None::<Instant>;
        loop {
            if !core.busy_for_update() {
                enforce(&app, &core, &home);
                if checked.is_none_or(|at| at.elapsed() >= Duration::from_secs(300))
                    && diagnose(&app, &core, &home).await.is_ok()
                {
                    checked = Some(Instant::now());
                }
            }
            tokio::time::sleep(Duration::from_secs(15)).await;
        }
    });
}

#[cfg(test)]
mod tests;
