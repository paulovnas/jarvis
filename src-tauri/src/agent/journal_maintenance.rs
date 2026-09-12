use super::*;
use std::{
    collections::HashSet,
    fs,
    io::{BufRead, BufReader, Read},
    sync::atomic::Ordering,
};
use tauri::ipc::Channel;

const HEADER_LIMIT: u64 = 65_537;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum JournalKind {
    Conversation,
    Worker,
}

#[derive(Debug, Clone)]
struct Candidate {
    path: PathBuf,
    conversation_id: String,
    kind: JournalKind,
}

#[derive(Default)]
struct Discovery {
    candidates: Vec<Candidate>,
    invalid_files: usize,
}

#[derive(Debug, Clone, Serialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    files: usize,
    conversation_journals: usize,
    worker_journals: usize,
    protected_files: usize,
    invalid_files: usize,
    candidates: usize,
    current_bytes: u64,
    live_bytes: u64,
    recoverable_bytes: u64,
    obsolete_records: usize,
    max_amplification_bps: u64,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum Phase {
    Analyzing,
    Compacting,
    Completed,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    phase: Phase,
    processed_files: usize,
    total_files: usize,
    recovered_bytes: u64,
    current_kind: Option<JournalKind>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MaintenanceResult {
    optimized_files: usize,
    failed_files: usize,
    recovered_bytes: u64,
    status: Summary,
}

struct MaintenanceLease(Arc<std::sync::atomic::AtomicBool>);

impl Drop for MaintenanceLease {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

pub(super) fn maintenance_error() -> AgentError {
    AgentError::new(
        "journal_maintenance",
        "A manutenção dos históricos está em andamento. Tente novamente em instantes.",
    )
}

fn valid_id(value: &str) -> bool {
    value.len() == 32 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn directory(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|metadata| metadata.is_dir() && !metadata.is_symlink())
}

fn journal_id(path: &Path) -> Option<String> {
    let name = path.file_name()?.to_str()?;
    if name.contains(".recovery-") {
        return None;
    }
    let id = name.strip_suffix(".jsonl")?;
    valid_id(id).then(|| id.to_owned())
}

fn header_matches(path: &Path, kind: JournalKind, conversation: &str, id: &str) -> bool {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_file() && !metadata.is_symlink() => metadata,
        _ => return false,
    };
    if metadata.len() == 0 {
        return false;
    }
    let file = match fs::File::open(path) {
        Ok(file) => file,
        Err(_) => return false,
    };
    let mut line = Vec::new();
    if BufReader::new(file)
        .take(HEADER_LIMIT)
        .read_until(b'\n', &mut line)
        .is_err()
        || line.len() as u64 >= HEADER_LIMIT
        || !line.ends_with(b"\n")
    {
        return false;
    }
    let Ok(header) = serde_json::from_slice::<Value>(&line) else {
        return false;
    };
    let common = header["version"].as_u64() == Some(1) && header["id"].as_str() == Some(id);
    match kind {
        JournalKind::Conversation => {
            common
                && header["type"].as_str() == Some("session")
                && header["projectId"].as_str() == Some(conversation)
        }
        JournalKind::Worker => {
            common
                && header["type"].as_str() == Some("agent")
                && header["conversationId"].as_str() == Some(conversation)
        }
    }
}

fn discover_tree(root: &Path, kind: JournalKind, result: &mut Discovery) {
    if !root.exists() {
        return;
    }
    if !directory(root) {
        result.invalid_files += 1;
        return;
    }
    let Ok(directories) = fs::read_dir(root) else {
        result.invalid_files += 1;
        return;
    };
    for directory_entry in directories {
        let Ok(directory_entry) = directory_entry else {
            result.invalid_files += 1;
            continue;
        };
        let directory_path = directory_entry.path();
        let Some(conversation_id) = directory_path.file_name().and_then(|name| name.to_str())
        else {
            continue;
        };
        if !valid_id(conversation_id) || !directory(&directory_path) {
            continue;
        }
        let Ok(files) = fs::read_dir(&directory_path) else {
            result.invalid_files += 1;
            continue;
        };
        for file in files {
            let Ok(file) = file else {
                result.invalid_files += 1;
                continue;
            };
            let path = file.path();
            let Some(id) = journal_id(&path) else {
                continue;
            };
            if !header_matches(&path, kind, conversation_id, &id) {
                result.invalid_files += 1;
                continue;
            }
            let protected_id = match kind {
                JournalKind::Conversation => id.clone(),
                JournalKind::Worker => conversation_id.to_owned(),
            };
            result.candidates.push(Candidate {
                path,
                conversation_id: protected_id,
                kind,
            });
        }
    }
}

fn discover(home: &Path) -> Discovery {
    let mut result = Discovery::default();
    let data_root = crate::data_dir::root(home);
    discover_tree(
        &data_root.join("sessions"),
        JournalKind::Conversation,
        &mut result,
    );
    discover_tree(
        &data_root.join("workflows"),
        JournalKind::Worker,
        &mut result,
    );
    result
}

fn inspect(
    discovery: &Discovery,
    protected: &HashSet<String>,
    progress: &mut dyn FnMut(Progress),
) -> (Summary, Vec<Candidate>) {
    let mut summary = Summary {
        files: discovery.candidates.len(),
        invalid_files: discovery.invalid_files,
        max_amplification_bps: 100,
        ..Summary::default()
    };
    let mut compactable = Vec::new();
    progress(Progress {
        phase: Phase::Analyzing,
        processed_files: 0,
        total_files: discovery.candidates.len(),
        recovered_bytes: 0,
        current_kind: None,
    });
    for (index, candidate) in discovery.candidates.iter().enumerate() {
        match candidate.kind {
            JournalKind::Conversation => summary.conversation_journals += 1,
            JournalKind::Worker => summary.worker_journals += 1,
        }
        let bytes = fs::symlink_metadata(&candidate.path)
            .map(|metadata| metadata.len())
            .unwrap_or(0);
        summary.current_bytes = summary.current_bytes.saturating_add(bytes);
        if protected.contains(&candidate.conversation_id) {
            summary.protected_files += 1;
            summary.live_bytes = summary.live_bytes.saturating_add(bytes);
        } else {
            match journal::analyze(&candidate.path) {
                Ok(analysis) => {
                    summary.live_bytes = summary.live_bytes.saturating_add(analysis.live_bytes);
                    summary.obsolete_records = summary
                        .obsolete_records
                        .saturating_add(analysis.obsolete_records);
                    summary.max_amplification_bps = summary
                        .max_amplification_bps
                        .max(analysis.amplification_bps);
                    if analysis.compactable {
                        summary.candidates += 1;
                        summary.recoverable_bytes = summary
                            .recoverable_bytes
                            .saturating_add(analysis.recoverable_bytes);
                        compactable.push(candidate.clone());
                    }
                }
                Err(_) => {
                    summary.invalid_files += 1;
                    summary.live_bytes = summary.live_bytes.saturating_add(bytes);
                }
            }
        }
        progress(Progress {
            phase: Phase::Analyzing,
            processed_files: index + 1,
            total_files: discovery.candidates.len(),
            recovered_bytes: 0,
            current_kind: Some(candidate.kind),
        });
    }
    (summary, compactable)
}

impl AgentState {
    fn begin_journal_maintenance(&self) -> Result<MaintenanceLease, AgentError> {
        self.journal_maintenance
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| maintenance_error())?;
        Ok(MaintenanceLease(self.journal_maintenance.clone()))
    }

    fn protected_journals(&self) -> Result<HashSet<String>, AgentError> {
        let sessions = self.sessions.lock().map_err(|_| AgentError::internal())?;
        let mut protected = HashSet::new();
        for (id, session) in sessions.iter() {
            let data = session.data.lock().map_err(|_| AgentError::internal())?;
            if data.active.is_some()
                || data.compacting
                || data.manual_compaction
                || !data.extras.queue.is_empty()
            {
                protected.insert(id.clone());
            }
        }
        drop(sessions);
        protected.extend(
            self.loading_sessions
                .lock()
                .map_err(|_| AgentError::internal())?
                .iter()
                .cloned(),
        );
        protected.extend(self.workflows.active_ids()?);
        Ok(protected)
    }

    fn journal_status(&self, home: &Path) -> Result<Summary, AgentError> {
        let protected = self.protected_journals()?;
        let discovery = discover(home);
        Ok(inspect(&discovery, &protected, &mut |_| {}).0)
    }

    fn optimize_journals(
        &self,
        home: &Path,
        progress: &mut dyn FnMut(Progress),
    ) -> Result<MaintenanceResult, AgentError> {
        let _lease = self.begin_journal_maintenance()?;
        let protected = self.protected_journals()?;
        let discovery = discover(home);
        let (_, candidates) = inspect(&discovery, &protected, progress);
        progress(Progress {
            phase: Phase::Compacting,
            processed_files: 0,
            total_files: candidates.len(),
            recovered_bytes: 0,
            current_kind: None,
        });
        let mut optimized_files = 0usize;
        let mut failed_files = 0usize;
        let mut recovered_bytes = 0u64;
        for (index, candidate) in candidates.iter().enumerate() {
            match journal::compact(&candidate.path) {
                Ok(recovered) => {
                    if recovered > 0 {
                        optimized_files += 1;
                        recovered_bytes = recovered_bytes.saturating_add(recovered);
                    }
                }
                Err(_) => failed_files += 1,
            }
            progress(Progress {
                phase: Phase::Compacting,
                processed_files: index + 1,
                total_files: candidates.len(),
                recovered_bytes,
                current_kind: Some(candidate.kind),
            });
        }
        progress(Progress {
            phase: Phase::Completed,
            processed_files: candidates.len(),
            total_files: candidates.len(),
            recovered_bytes,
            current_kind: None,
        });
        let status = inspect(&discover(home), &protected, &mut |_| {}).0;
        Ok(MaintenanceResult {
            optimized_files,
            failed_files,
            recovered_bytes,
            status,
        })
    }
}

#[tauri::command]
pub async fn get_journal_maintenance_status(
    app: tauri::AppHandle,
    agent: tauri::State<'_, AgentState>,
) -> Result<Summary, AgentError> {
    let home = app.path().home_dir().map_err(|_| AgentError::storage())?;
    let agent = agent.inner().clone();
    tauri::async_runtime::spawn_blocking(move || agent.journal_status(&home))
        .await
        .map_err(|_| AgentError::internal())?
}

#[tauri::command]
pub async fn optimize_journals(
    app: tauri::AppHandle,
    agent: tauri::State<'_, AgentState>,
    on_progress: Channel<Progress>,
) -> Result<MaintenanceResult, AgentError> {
    let home = app.path().home_dir().map_err(|_| AgentError::storage())?;
    let agent = agent.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        agent.optimize_journals(&home, &mut |progress| {
            let _ = on_progress.send(progress);
        })
    })
    .await
    .map_err(|_| AgentError::internal())?
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::tests::Fixture;
    use std::io::Write;

    fn id(value: char) -> String {
        std::iter::repeat_n(value, 32).collect()
    }

    fn options() -> TurnOptions {
        TurnOptions {
            account: "account".into(),
            model: "model".into(),
            reasoning: None,
            mode: Mode::Build,
            workflow: None,
            custom_workflow_id: None,
            custom_agent_id: None,
            approval_mode: ApprovalMode::Yolo,
            manual_validation: false,
        }
    }

    fn turn() -> StoredTurn {
        StoredTurn {
            wire: vec![],
            mcp_intent: None,
            turn: Turn {
                id: "turn".into(),
                created_at: 1,
                duration_ms: 0,
                user: "Teste".into(),
                parts: vec![],
                options: options(),
                context_window: None,
                status: TurnStatus::Completed,
                tasks: vec![],
                steps: vec![Step {
                    text: "x".repeat(8 * 1024),
                    ..Step::default()
                }],
                error: None,
            },
        }
    }

    fn amplified(path: &Path, header: Value) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, format!("{}\n", header)).unwrap();
        let mut current = turn();
        for revision in 0..12 {
            current.turn.duration_ms = revision;
            journal::append(path, &current).unwrap();
        }
    }

    fn journals(fixture: &Fixture) -> (String, PathBuf, PathBuf) {
        let project = id('a');
        let conversation = id('b');
        let worker = id('c');
        let root = crate::data_dir::root(&fixture.root)
            .join("sessions")
            .join(&project)
            .join(format!("{conversation}.jsonl"));
        let worker_path = crate::data_dir::root(&fixture.root)
            .join("workflows")
            .join(&conversation)
            .join(format!("{worker}.jsonl"));
        amplified(
            &root,
            json!({"type":"session","version":1,"id":conversation,"projectId":project}),
        );
        amplified(
            &worker_path,
            json!({"type":"agent","version":1,"id":worker,"conversationId":conversation}),
        );
        (conversation, root, worker_path)
    }

    #[test]
    fn root_and_worker_journals_are_measured_compacted_and_report_progress() {
        let fixture = Fixture::new();
        let (_, root, worker) = journals(&fixture);
        let agent = AgentState::default();
        let before_root = fs::metadata(&root).unwrap().len();
        let before_worker = fs::metadata(&worker).unwrap().len();
        let status = agent.journal_status(&fixture.root).unwrap();
        assert_eq!(status.files, 2);
        assert_eq!(status.conversation_journals, 1);
        assert_eq!(status.worker_journals, 1);
        assert_eq!(status.candidates, 2);
        assert!(status.recoverable_bytes > 0);
        assert!(status.max_amplification_bps > 300);

        let mut progress = Vec::new();
        let result = agent
            .optimize_journals(&fixture.root, &mut |event| progress.push(event))
            .unwrap();
        assert_eq!(result.optimized_files, 2);
        assert_eq!(result.failed_files, 0);
        assert!(result.recovered_bytes > 0);
        assert_eq!(result.status.candidates, 0);
        assert!(fs::metadata(root).unwrap().len() < before_root / 3);
        assert!(fs::metadata(worker).unwrap().len() < before_worker / 3);
        assert_eq!(progress.first().unwrap().phase, Phase::Analyzing);
        assert_eq!(progress.last().unwrap().phase, Phase::Completed);
        assert!(progress
            .iter()
            .any(|event| event.phase == Phase::Compacting && event.total_files == 2));
    }

    #[test]
    fn active_conversation_protects_its_root_and_worker_journals() {
        let fixture = Fixture::new();
        let (conversation, root, worker) = journals(&fixture);
        let agent = AgentState::default();
        let mut session = crate::agent::tests::session(&fixture);
        let editable = Arc::get_mut(&mut session).unwrap();
        editable.id = conversation.clone();
        editable.journal = root.clone();
        editable.journal_maintenance = agent.journal_maintenance.clone();
        session.reserve("Em andamento".into(), options()).unwrap();
        agent
            .sessions
            .lock()
            .unwrap()
            .insert(conversation, session.clone());
        let root_bytes = fs::metadata(&root).unwrap().len();
        let worker_bytes = fs::metadata(&worker).unwrap().len();

        let result = agent.optimize_journals(&fixture.root, &mut |_| {}).unwrap();
        assert_eq!(result.optimized_files, 0);
        assert_eq!(result.status.protected_files, 2);
        assert_eq!(fs::metadata(root).unwrap().len(), root_bytes);
        assert_eq!(fs::metadata(worker).unwrap().len(), worker_bytes);
        assert!(!agent.journal_maintenance.load(Ordering::Acquire));
    }

    #[test]
    fn maintenance_blocks_new_messages_until_the_lease_is_released() {
        let fixture = Fixture::new();
        let agent = AgentState::default();
        let mut session = crate::agent::tests::session(&fixture);
        Arc::get_mut(&mut session).unwrap().journal_maintenance = agent.journal_maintenance.clone();

        let lease = agent.begin_journal_maintenance().unwrap();
        let error = session
            .submit("Não deve iniciar".into(), options())
            .unwrap_err();
        assert_eq!(error.code, "journal_maintenance");
        assert!(session.snapshot().unwrap().active_turn_id.is_none());

        drop(lease);
        assert!(session
            .submit("Agora pode iniciar".into(), options())
            .unwrap()
            .is_some());
    }

    #[test]
    fn loading_conversation_is_protected_and_failed_load_registration_is_released() {
        let agent = AgentState::default();
        let loading = agent.begin_session_load("conversation-loading").unwrap();
        assert!(agent
            .protected_journals()
            .unwrap()
            .contains("conversation-loading"));
        drop(loading);
        assert!(!agent
            .protected_journals()
            .unwrap()
            .contains("conversation-loading"));

        let maintenance = agent.begin_journal_maintenance().unwrap();
        let error = agent
            .begin_session_load("conversation-blocked")
            .unwrap_err();
        assert_eq!(error.code, "journal_maintenance");
        assert!(!agent
            .loading_sessions
            .lock()
            .unwrap()
            .contains("conversation-blocked"));
        drop(maintenance);
    }

    #[test]
    fn malformed_journal_is_reported_without_replacement() {
        let fixture = Fixture::new();
        let (conversation, root, _) = journals(&fixture);
        fs::OpenOptions::new()
            .append(true)
            .open(&root)
            .unwrap()
            .write_all(b"{broken}\n")
            .unwrap();
        let original = fs::read(&root).unwrap();
        let result = AgentState::default()
            .optimize_journals(&fixture.root, &mut |_| {})
            .unwrap();
        assert!(result.status.invalid_files >= 1);
        assert_eq!(fs::read(root).unwrap(), original);
        assert!(valid_id(&conversation));
    }
}
