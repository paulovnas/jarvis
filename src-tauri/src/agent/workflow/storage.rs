use super::*;
use std::{fs, io::Write};

fn directory(path: &Path) -> Result<(), AgentError> {
    if !path.exists() {
        #[cfg(unix)]
        let mut builder = fs::DirBuilder::new();
        #[cfg(not(unix))]
        let builder = fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        match builder.create(path) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(_) => return Err(AgentError::storage()),
        }
    }
    let meta = fs::symlink_metadata(path).map_err(|_| AgentError::storage())?;
    if !meta.is_dir() || meta.is_symlink() {
        return Err(AgentError::storage());
    }
    Ok(())
}
pub(super) fn path(home: &Path, id: &str) -> Result<PathBuf, AgentError> {
    if !valid_id(id) {
        return Err(invalid("Identidade da conversa inválida."));
    }
    Ok(crate::data_dir::root(home).join("workflows").join(id))
}
pub(super) fn valid_id(id: &str) -> bool {
    id.len() == 32 && id.bytes().all(|c| c.is_ascii_hexdigit())
}
pub(super) fn load(directory: &Path, id: &str) -> Result<Option<Manifest>, AgentError> {
    let path = directory.join("state.json");
    let metadata = match fs::symlink_metadata(&path) {
        Ok(meta) => meta,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(AgentError::storage()),
    };
    if metadata.is_symlink() || !metadata.is_file() || metadata.len() > 8 * 1024 * 1024 {
        return Err(AgentError::storage());
    }
    if fs::symlink_metadata(directory)
        .map_err(|_| AgentError::storage())?
        .is_symlink()
    {
        return Err(AgentError::storage());
    }
    let mut state: Manifest =
        serde_json::from_slice(&fs::read(path).map_err(|_| AgentError::storage())?)
            .map_err(|_| AgentError::storage())?;
    if state.version != 1
        || state.conversation_id != id
        || state.jobs.len() > MAX_JOBS
        || state
            .jobs
            .iter()
            .any(|(id, job)| !valid_id(id) || id != &job.id)
    {
        return Err(AgentError::storage());
    }
    if state.root_status.active() {
        state.root_status = Status::Interrupted;
    }
    for job in state.jobs.values_mut().filter(|job| job.status.active()) {
        job.status = Status::Interrupted;
        job.error = Some("Execução interrompida. Confira o checkpoint antes de retomar.".into());
    }
    Ok(Some(state))
}
pub(super) fn save(directory: &Path, state: &Manifest) -> Result<(), AgentError> {
    let mut file = tempfile::NamedTempFile::new_in(directory).map_err(|_| AgentError::storage())?;
    serde_json::to_writer(file.as_file_mut(), state).map_err(|_| AgentError::storage())?;
    file.as_file_mut()
        .sync_all()
        .map_err(|_| AgentError::storage())?;
    file.persist(directory.join("state.json"))
        .map_err(|_| AgentError::storage())?;
    #[cfg(unix)]
    fs::File::open(directory)
        .and_then(|file| file.sync_all())
        .map_err(|_| AgentError::storage())?;
    Ok(())
}
pub(super) fn open(
    root: Arc<Session>,
    env: Environment,
    app: tauri::AppHandle,
    flow: Flow,
    profiles: settings::ModelSettings,
    signal: watch::Receiver<bool>,
) -> Result<Arc<Hub>, AgentError> {
    let directory_path = path(&env.home, &root.id)?;
    directory(directory_path.parent().ok_or_else(AgentError::storage)?)?;
    directory(&directory_path)?;
    let (run_id, options, mcp_intent) = {
        let data = root.data.lock().map_err(|_| AgentError::internal())?;
        let run_id = data
            .active
            .as_ref()
            .ok_or_else(AgentError::cancelled)?
            .id
            .clone();
        let current = data.turns.last().ok_or_else(AgentError::internal)?;
        (
            run_id,
            current.turn.options.clone(),
            current.mcp_intent.clone().unwrap_or_default(),
        )
    };
    let mut manifest = load(&directory_path, &root.id)?.unwrap_or_else(|| Manifest {
        custom_definition: None,
        custom_agent: None,
        validation: None,
        root_recovery: None,
        version: 1,
        conversation_id: root.id.clone(),
        run_id: run_id.clone(),
        flow,
        mcp_intent: mcp_intent.clone(),
        root_status: Status::Running,
        updated_at: now(),
        revision: now(),
        options: options.clone(),
        profiles: profiles.clone(),
        jobs: BTreeMap::new(),
        messages: vec![],
        design_briefs: BTreeMap::new(),
        guidance: BTreeMap::new(),
    });
    // Keep recent recovery context without allowing unbounded metadata growth.
    if let Some(batch) = &mut manifest.validation {
        if run_id == batch.id {
            batch.submitted = true;
        } else {
            batch.stale = true;
        }
    }
    if manifest.jobs.len() > MAX_JOBS / 2 {
        let mut ids: Vec<_> = manifest
            .jobs
            .values()
            .filter(|job| {
                job.status == Status::Completed
                    && !manifest
                        .validation
                        .as_ref()
                        .is_some_and(|batch| !batch.stale && batch.run_id == job.run_id)
            })
            .map(|job| (job.updated_at, job.id.clone()))
            .collect();
        ids.sort();
        for (_, id) in ids {
            if manifest.jobs.len() <= MAX_JOBS / 2 {
                break;
            }
            manifest.jobs.remove(&id);
        }
    }
    manifest.run_id = run_id;
    manifest.flow = flow;
    manifest.mcp_intent = mcp_intent;
    manifest.custom_definition = None;
    manifest.custom_agent = None;
    manifest.root_recovery = None;
    manifest.root_status = Status::Running;
    manifest.options = options;
    manifest.profiles = profiles;
    manifest.guidance.clear();
    manifest
        .design_briefs
        .retain(|id, _| id == "main" || manifest.jobs.contains_key(id));
    manifest.messages.clear();
    manifest.updated_at = now();
    manifest.revision += 1;
    save(&directory_path, &manifest)?;
    let (changed, _) = watch::channel(manifest.revision);
    let notification_app = app.clone();
    let conversation = root.id.clone();
    let attention = Arc::new(move |snapshot: &ChatSnapshot| {
        super::super::desktop_events::attention(&notification_app, &conversation, snapshot);
    });
    let hub = Arc::new(Hub {
        root,
        env,
        directory: directory_path,
        manifest: Mutex::new(manifest),
        live: Mutex::new(HashMap::new()),
        changed,
        emit: Arc::new(move |id| {
            let _ = app.emit("workflow:changed", json!({"conversationId":id}));
        }),
        attention,
        check_lock: AsyncRwLock::new(()),
        root_signal: signal,
    });
    (hub.emit)(&hub.root.id);
    Ok(hub)
}

pub(super) fn recover(
    root: Arc<Session>,
    env: Environment,
    app: tauri::AppHandle,
    flow: Flow,
    signal: watch::Receiver<bool>,
    root_uncertain: Vec<String>,
) -> Result<(Arc<Hub>, Vec<Job>), AgentError> {
    if !matches!(flow, Flow::Planned | Flow::Complete) {
        return Err(invalid(
            "A retomada está disponível apenas para fluxos Planejado e Completo.",
        ));
    }
    let directory_path = path(&env.home, &root.id)?;
    directory(directory_path.parent().ok_or_else(AgentError::storage)?)?;
    directory(&directory_path)?;
    let run_id = root
        .data
        .lock()
        .map_err(|_| AgentError::internal())?
        .active
        .as_ref()
        .ok_or_else(AgentError::cancelled)?
        .id
        .clone();
    let manifest = load(&directory_path, &root.id)?
        .ok_or_else(|| invalid("Checkpoint do fluxo não encontrado."))?;
    let (manifest, resumed) =
        prepare_recovery(&directory_path, manifest, flow, &run_id, root_uncertain)?;
    save(&directory_path, &manifest)?;
    let (changed, _) = watch::channel(manifest.revision);
    let notification_app = app.clone();
    let conversation = root.id.clone();
    let attention = Arc::new(move |snapshot: &ChatSnapshot| {
        super::super::desktop_events::attention(&notification_app, &conversation, snapshot);
    });
    let hub = Arc::new(Hub {
        root,
        env,
        directory: directory_path,
        manifest: Mutex::new(manifest),
        live: Mutex::new(HashMap::new()),
        changed,
        emit: Arc::new(move |id| {
            let _ = app.emit("workflow:changed", json!({"conversationId":id}));
        }),
        attention,
        check_lock: AsyncRwLock::new(()),
        root_signal: signal,
    });
    (hub.emit)(&hub.root.id);
    Ok((hub, resumed))
}

pub(super) fn prepare_recovery(
    directory: &Path,
    mut manifest: Manifest,
    flow: Flow,
    run_id: &str,
    root_uncertain: Vec<String>,
) -> Result<(Manifest, Vec<Job>), AgentError> {
    if manifest.flow != flow
        || manifest.run_id != run_id
        || manifest.root_status != Status::Interrupted
    {
        return Err(invalid(
            "O checkpoint não corresponde ao fluxo interrompido desta conversa.",
        ));
    }
    manifest.root_status = Status::Running;
    manifest.root_recovery = Some(RecoveryCheckpoint::new(root_uncertain));
    let mut resumed = Vec::new();
    for job in manifest
        .jobs
        .values_mut()
        .filter(|job| job.run_id == run_id && job.status == Status::Interrupted)
    {
        let journal_path = directory.join(format!("{}.jsonl", job.id));
        let tail = if journal_path.exists() {
            journal::read_only(&journal_path)?.0.pop()
        } else {
            None
        };
        match tail {
            Some(turn) if super::super::resumable_workflow_turn(&turn) => {
                job.status = Status::Queued;
                job.error = None;
                job.recovery = Some(RecoveryCheckpoint::new(journal::uncertain_tool_names(
                    &turn,
                )));
                job.updated_at = now();
                resumed.push(job.clone());
            }
            Some(turn) if turn.turn.status == TurnStatus::Completed => {
                job.status = if job.handoff.as_ref().is_some_and(|handoff| {
                    matches!(handoff.verdict, Verdict::Completed | Verdict::Approved)
                }) {
                    Status::Completed
                } else {
                    Status::Blocked
                };
                job.error = None;
                job.recovery = None;
                job.duration_ms = turn.turn.duration_ms;
                job.updated_at = now();
            }
            Some(turn) if turn.turn.status == TurnStatus::Cancelled => {
                job.status = Status::Cancelled;
                job.error = turn.turn.error.map(|error| error.message);
                job.recovery = None;
                job.duration_ms = turn.turn.duration_ms;
                job.updated_at = now();
            }
            Some(turn) if turn.turn.status == TurnStatus::Error => {
                job.status = Status::Failed;
                job.error =
                    turn.turn.error.map(|error| error.message).or_else(|| {
                        Some("O agente terminou com erro antes da interrupção.".into())
                    });
                job.recovery = None;
                job.duration_ms = turn.turn.duration_ms;
                job.updated_at = now();
            }
            Some(_) => {
                return Err(invalid(
                    "O histórico de um agente não corresponde ao checkpoint interrompido.",
                ));
            }
            None => {
                job.status = Status::Queued;
                job.error = None;
                job.recovery = Some(RecoveryCheckpoint::new(vec![]));
                job.updated_at = now();
                resumed.push(job.clone());
            }
        }
    }
    let active: std::collections::HashSet<_> = manifest
        .jobs
        .values()
        .filter(|job| job.status.active())
        .map(|job| job.id.clone())
        .collect();
    manifest
        .guidance
        .retain(|_, request| request.run_id == run_id && active.contains(&request.from));
    manifest.updated_at = now();
    manifest.revision += 1;
    Ok((manifest, resumed))
}

fn worker_session(
    hub: &Arc<Hub>,
    job: &Job,
    path: PathBuf,
    turns: Vec<StoredTurn>,
    extras: journal::Extras,
) -> Arc<Session> {
    let durable_turn = turns.last().cloned();
    let weak = Arc::downgrade(hub);
    Arc::new(Session {
        id: job.id.clone(),
        journal: path,
        root: hub.root.root.clone(),
        journal_maintenance: hub.root.journal_maintenance.clone(),
        data: Mutex::new(SessionData {
            turns,
            durable_turn,
            extras,
            active: None,
            recovery: None,
            revision: next_revision(),
            storage_failed: false,
            last_emit: std::time::Instant::now(),
            compacting: false,
            manual_compaction: false,
        }),
        emit: Arc::new(move |snapshot| {
            if let Some(hub) = weak.upgrade() {
                (hub.attention)(&snapshot);
                (hub.emit)(&hub.root.id);
            }
        }),
    })
}

pub(super) fn worker(
    hub: &Arc<Hub>,
    job: &Job,
    resume: Option<String>,
) -> Result<(Arc<Session>, watch::Receiver<bool>), AgentError> {
    let path = hub.directory.join(format!("{}.jsonl", job.id));
    let (turns, extras) = if path.exists() {
        journal::load_all(&path)?
    } else {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|_| AgentError::storage())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            file.set_permissions(fs::Permissions::from_mode(0o600))
                .map_err(|_| AgentError::storage())?;
        }
        writeln!(
            file,
            "{}",
            json!({"type":"agent", "version":1,"id":job.id,"conversationId":hub.root.id})
        )
        .map_err(|_| AgentError::storage())?;
        file.sync_all().map_err(|_| AgentError::storage())?;
        (vec![], journal::Extras::default())
    };
    let session = worker_session(hub, job, path, turns, extras);
    let content = resume.unwrap_or_else(|| job.prompt.clone());
    let original = hub
        .root
        .data
        .lock()
        .map_err(|_| AgentError::internal())?
        .turns
        .last()
        .ok_or_else(AgentError::internal)?
        .turn
        .user
        .clone();
    let mcp_intent = hub
        .manifest
        .lock()
        .map_err(|_| AgentError::internal())?
        .mcp_intent
        .clone();
    let wire = format!("Original user request (preserve exact paths, constraints and acceptance; a coordinator cannot silently replace these):\n{original}\n\nNative dispatch from {} (assigned subset of the original request):\n{content}\nBeads: {}\nScope: {}\nAcceptance criteria: {}\nIf the dispatch conflicts with the original request, return the discrepancy to your coordinator before implementing. Return a structured hub_complete handoff when finished.", job.parent_id, job.bead_id.as_deref().unwrap_or("research/planning"), json!(job.scope), json!(job.acceptance));
    let signal = {
        let mut data = session.data.lock().map_err(|_| AgentError::internal())?;
        session.reserve_locked(&mut data, content, job.options.clone(), None, vec![])?
    };
    session.update(true, |data| {
        let current = data.turns.last_mut().unwrap();
        current.wire = vec![json!({"role":"user","content":wire})];
        current.mcp_intent = Some(mcp_intent);
    })?;
    Ok((session, signal))
}

pub(super) fn resume_worker(
    hub: &Arc<Hub>,
    job: &Job,
) -> Result<(Arc<Session>, watch::Receiver<bool>), AgentError> {
    let path = hub.directory.join(format!("{}.jsonl", job.id));
    if !path.exists() {
        return worker(
            hub,
            job,
            Some("The previous runtime stopped before this worker created a durable turn. Inspect current Beads and project state, then continue the assigned work without assuming that no external state changed.".into()),
        );
    }
    let (turns, extras) = journal::load_for_recovery(&path)?;
    if turns.is_empty() {
        return worker(
            hub,
            job,
            Some("The previous runtime stopped before this worker created a durable turn. Inspect current Beads and project state, then continue the assigned work without assuming that no external state changed.".into()),
        );
    }
    let session = worker_session(hub, job, path, turns, extras);
    let (signal, _) = session.resume_interrupted_workflow_turn()?;
    Ok((session, signal))
}
