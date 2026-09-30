//! Private versioned snapshots. Runtime handles and old PIDs are never retained.
use super::{tracking, AgentError, AppState, Entry, Output, ProjectTerminal, TerminalState};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

const VERSION: u32 = 1;
const MAX_FILE: u64 = 16 * 1024 * 1024;

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Saved {
    pub info: ProjectTerminal,
    pub root: PathBuf,
    pub cwd: PathBuf,
    pub call_id: Option<String>,
    pub owner_id: Option<String>,
    pub output: String,
    pub revision: u64,
    pub truncated: bool,
    pub live: bool,
    pub execution: tracking::Execution,
    pub sandbox: Option<super::super::execution_sandbox::SandboxPlan>,
    pub shell_program: Option<PathBuf>,
    pub shell_restorable: bool,
    pub service_port: Option<u16>,
}

impl Saved {
    pub(super) fn capture(entry: &Entry) -> Result<Self, AgentError> {
        let output = entry.output.lock().map_err(|_| AgentError::internal())?;
        let mut info = entry.info.clone();
        info.pid = 0;
        let live = entry.info.running()
            && entry.runtime.as_ref().is_some_and(|runtime| {
                runtime.alive.load(super::Ordering::SeqCst)
                    && !runtime.closing.load(super::Ordering::SeqCst)
            });
        Ok(Self {
            info,
            root: entry.project_root.clone(),
            cwd: PathBuf::from(&entry.info.cwd),
            call_id: entry.call_id.clone(),
            owner_id: entry.owner_id.clone(),
            output: output.text.clone(),
            revision: output.revision,
            truncated: output.truncated,
            live,
            execution: if entry
                .runtime
                .as_ref()
                .is_some_and(|runtime| runtime.interrupted.load(super::Ordering::SeqCst))
            {
                tracking::Execution::Idle
            } else {
                entry.execution.clone()
            },
            sandbox: entry.sandbox.clone(),
            shell_program: entry.shell_program.clone(),
            shell_restorable: entry.shell_restorable,
            service_port: entry.service_port,
        })
    }

    pub(super) fn output(&self) -> Output {
        Output {
            text: self.output.clone(),
            revision: self.revision,
            truncated: self.truncated,
        }
    }

    fn valid(&self) -> bool {
        let bounded = |value: &str| {
            !value.is_empty() && value.len() <= 128 && !value.chars().any(char::is_control)
        };
        bounded(&self.info.id)
            && bounded(&self.info.project_id)
            && self.info.conversation_id.as_deref().is_none_or(bounded)
            && self.call_id.as_deref().is_none_or(bounded)
            && self.owner_id.as_deref().is_none_or(bounded)
            && super::terminal_title(Some(&self.info.title), 0).is_ok()
            && self.output.len() <= super::OUTPUT_LIMIT
            && self.root.is_absolute()
            && self.cwd.is_absolute()
            && self.service_port.is_none_or(|port| port > 0)
            && self
                .shell_program
                .as_ref()
                .is_none_or(|program| program.is_absolute())
            && self
                .info
                .command
                .as_ref()
                .is_none_or(|command| command.len() <= 16_000 && !command.contains('\0'))
            && match &self.execution {
                tracking::Execution::Running { command, cwd } => {
                    command.len() <= 16_000 && !command.contains('\0') && cwd.is_absolute()
                }
                _ => true,
            }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Manifest {
    version: u32,
    terminals: Vec<Saved>,
}

#[derive(Default)]
pub(super) struct Store {
    pub home: Option<PathBuf>,
    pub app_state: Option<AppState>,
    pub directory: Option<PathBuf>,
    pub pending: Vec<Saved>,
    pub restored: bool,
    pub restoring: bool,
    pub frozen: bool,
    pub stopped: bool,
    last_write: Option<Instant>,
}

fn regular_directory(path: &Path) -> Result<(), AgentError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() && !metadata.is_symlink() => {}
        Ok(_) => return Err(AgentError::storage()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir(path).map_err(|_| AgentError::storage())?;
        }
        Err(_) => return Err(AgentError::storage()),
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))
            .map_err(|_| AgentError::storage())?;
    }
    Ok(())
}

impl Store {
    pub(super) fn configure(&mut self, home: &Path, app_state: AppState) -> Result<(), AgentError> {
        if self.directory.is_some() {
            return Ok(());
        }
        let root = crate::data_dir::root(home);
        regular_directory(&root)?;
        let directory = root.join("terminals");
        regular_directory(&directory)?;
        let path = directory.join("state.json");
        let pending = match fs::symlink_metadata(&path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(_) => return Err(AgentError::storage()),
            Ok(metadata) => {
                if metadata.is_symlink() || !metadata.is_file() || metadata.len() > MAX_FILE {
                    return Err(AgentError::storage());
                }
                let bytes = fs::read(&path).map_err(|_| AgentError::storage())?;
                let manifest: Manifest = match serde_json::from_slice(&bytes) {
                    Ok(manifest) => manifest,
                    Err(_) => {
                        preserve_invalid(&directory, &path)?;
                        Manifest {
                            version: VERSION,
                            terminals: Vec::new(),
                        }
                    }
                };
                let mut ids = HashSet::new();
                let mut counts = std::collections::HashMap::<&str, usize>::new();
                if manifest.version != VERSION || manifest.terminals.len() > super::MAX_TERMINALS {
                    preserve_invalid(&directory, &path)?;
                    self.home = Some(home.to_path_buf());
                    self.app_state = Some(app_state);
                    self.directory = Some(directory);
                    return Ok(());
                }
                for saved in &manifest.terminals {
                    let count = counts.entry(&saved.info.project_id).or_default();
                    *count += 1;
                    if !saved.valid()
                        || !ids.insert(&saved.info.id)
                        || *count > super::MAX_PROJECT_TERMINALS
                    {
                        preserve_invalid(&directory, &path)?;
                        self.home = Some(home.to_path_buf());
                        self.app_state = Some(app_state);
                        self.directory = Some(directory);
                        return Ok(());
                    }
                }
                manifest.terminals
            }
        };
        self.home = Some(home.to_path_buf());
        self.app_state = Some(app_state);
        self.directory = Some(directory);
        self.pending = pending;
        Ok(())
    }

    pub(super) fn save(
        &mut self,
        mut terminals: Vec<Saved>,
        force: bool,
    ) -> Result<(), AgentError> {
        if self.frozen
            || (!force
                && self
                    .last_write
                    .is_some_and(|time| time.elapsed() < Duration::from_secs(2)))
        {
            return Ok(());
        }
        let Some(directory) = &self.directory else {
            return Ok(());
        };
        let ids = terminals
            .iter()
            .map(|saved| saved.info.id.clone())
            .collect::<HashSet<_>>();
        terminals.extend(
            self.pending
                .iter()
                .filter(|saved| !ids.contains(&saved.info.id))
                .cloned(),
        );
        regular_directory(directory)?;
        let bytes = serde_json::to_vec(&Manifest {
            version: VERSION,
            terminals,
        })
        .map_err(|_| AgentError::storage())?;
        if bytes.len() as u64 > MAX_FILE {
            return Err(AgentError::storage());
        }
        let mut file =
            tempfile::NamedTempFile::new_in(directory).map_err(|_| AgentError::storage())?;
        use std::io::Write;
        file.write_all(&bytes)
            .and_then(|()| file.as_file().sync_all())
            .map_err(|_| AgentError::storage())?;
        file.persist(directory.join("state.json"))
            .map_err(|_| AgentError::storage())?;
        #[cfg(unix)]
        fs::File::open(directory)
            .and_then(|file| file.sync_all())
            .map_err(|_| AgentError::storage())?;
        self.last_write = Some(Instant::now());
        Ok(())
    }

    pub(super) fn forget(&mut self, projects: &HashSet<String>) {
        self.pending
            .retain(|saved| !projects.contains(&saved.info.project_id));
    }
}

fn preserve_invalid(directory: &Path, path: &Path) -> Result<(), AgentError> {
    let destination = directory.join(format!("state.recovery.{}.json", crate::library::new_id()?));
    // Reserve a private exclusive pathname before the atomic rename. Existing
    // recovery files are never overwritten, and invalid state remains inspectable.
    let file = tempfile::NamedTempFile::new_in(directory).map_err(|_| AgentError::storage())?;
    file.persist_noclobber(&destination)
        .map_err(|_| AgentError::storage())?;
    fs::rename(path, &destination).map_err(|_| AgentError::storage())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(destination, fs::Permissions::from_mode(0o600))
            .map_err(|_| AgentError::storage())?;
    }
    Ok(())
}

pub(crate) struct TerminalShutdownGuard(TerminalState);
impl Drop for TerminalShutdownGuard {
    fn drop(&mut self) {
        let resume = self.0 .3.lock().ok().is_some_and(|mut store| {
            // A successful Windows installer exits the process. A returning
            // installer failed: resume the original app, without replaying any
            // command whose old process was already stopped by its callback.
            store.frozen = false;
            store.stopped = false;
            true
        });
        if resume {
            let _ = self.0.checkpoint();
        }
    }
}

impl TerminalState {
    pub(crate) fn configure(&self, home: &Path, app_state: AppState) -> Result<(), AgentError> {
        self.3
            .lock()
            .map_err(|_| AgentError::internal())?
            .configure(home, app_state)
    }

    pub(crate) fn checkpoint(&self) -> Result<(), AgentError> {
        self.save_checkpoint(true)
    }

    pub(super) fn save_checkpoint(&self, force: bool) -> Result<(), AgentError> {
        // Always entries -> output -> store. Output readers release their lock
        // before acquiring entries; DB locks never overlap with restore spawning.
        let entries = self.0.lock().map_err(|_| AgentError::internal())?;
        let mut store = self.3.lock().map_err(|_| AgentError::internal())?;
        if store.frozen || store.restoring || store.directory.is_none() {
            return Ok(());
        }
        store.forget(&*self.2.lock().map_err(|_| AgentError::internal())?);
        let saved = entries
            .values()
            .map(Saved::capture)
            .collect::<Result<Vec<_>, _>>()?;
        store.save(saved, force)
    }

    pub(crate) fn shutdown(&self) -> Result<(), AgentError> {
        let runtimes = {
            let entries = self.0.lock().map_err(|_| AgentError::internal())?;
            let mut store = self.3.lock().map_err(|_| AgentError::internal())?;
            if store.stopped {
                return Ok(());
            }
            if !store.frozen {
                ensure_writes_finished(&entries)?;
                store.forget(&*self.2.lock().map_err(|_| AgentError::internal())?);
                let saved = entries
                    .values()
                    .map(Saved::capture)
                    .collect::<Result<Vec<_>, _>>()?;
                store.save(saved, true)?;
            }
            // Freeze before killing. Exit/output watchers cannot replace active
            // restart recipes with the exit generated by our own shutdown.
            store.frozen = true;
            store.stopped = true;
            entries
                .values()
                .filter_map(|entry| entry.runtime.clone())
                .collect::<Vec<_>>()
        };
        for runtime in runtimes {
            runtime.close();
        }
        Ok(())
    }

    pub(crate) fn prepare_update_shutdown(&self) -> Result<TerminalShutdownGuard, AgentError> {
        let entries = self.0.lock().map_err(|_| AgentError::internal())?;
        let mut store = self.3.lock().map_err(|_| AgentError::internal())?;
        if store.frozen {
            return Err(super::invalid("O Jarvis já está encerrando os terminais."));
        }
        ensure_writes_finished(&entries)?;
        store.forget(&*self.2.lock().map_err(|_| AgentError::internal())?);
        let saved = entries
            .values()
            .map(Saved::capture)
            .collect::<Result<Vec<_>, _>>()?;
        store.save(saved, true)?;
        store.frozen = true;
        Ok(TerminalShutdownGuard(self.clone()))
    }
}

fn ensure_writes_finished(
    entries: &std::collections::HashMap<String, Entry>,
) -> Result<(), AgentError> {
    if entries
        .values()
        .filter_map(|entry| entry.runtime.as_ref())
        .any(|runtime| runtime.pending_writes.load(super::Ordering::SeqCst) > 0)
    {
        return Err(super::invalid(
            "Aguarde o envio de dados ao terminal terminar antes de encerrar.",
        ));
    }
    Ok(())
}
