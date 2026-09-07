//! A disposable, bounded offset cache. Journals remain the only source of truth.
use super::*;
use std::{collections::{BTreeMap, HashSet, VecDeque}, path::Path, time::SystemTime};

pub(super) const PAGE_SIZE: usize = 20;
const PAGE_BYTES: usize = 1024 * 1024;
const RAIL_SIZE: usize = 48;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Window { pub start: usize, pub total: usize }

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Excerpt {
    pub id: String, pub index: usize, pub created_at: u64, pub user: String, pub assistant: String,
}

fn snippet(text: &str) -> String { text.chars().take(240).collect::<String>().split_whitespace().collect::<Vec<_>>().join(" ").chars().take(160).collect() }

pub(super) fn excerpt(turn: &Turn, index: usize) -> Excerpt {
    Excerpt { id: turn.id.clone(), index, created_at: turn.created_at, user: snippet(&turn.user), assistant: snippet(turn.steps.last().map_or("", |step| &step.text)) }
}

struct Entry { offset: u64, length: usize, excerpt: Excerpt, tokens: Vec<u64>, limit: Option<u64>, edited_paths: Vec<String> }
#[derive(Default)]
struct Index {
    end: u64, length: u64, modified: Option<SystemTime>,
    entries: Vec<Entry>, ids: HashSet<String>, queue: Vec<queue::QueuedMessage>,
    context: Option<compaction::Checkpoint>, compactions: Vec<compaction::CompactionEvent>,
    files: BTreeMap<String, (u64, usize, diffs::FileSummary)>,
}

impl Index {
    fn refresh(&mut self, path: &Path) -> Result<(), AgentError> {
        let metadata = std::fs::symlink_metadata(path).map_err(|_| AgentError::storage())?;
        if !metadata.is_file() || metadata.is_symlink() { return Err(AgentError::storage()); }
        let modified = metadata.modified().ok();
        if self.length == metadata.len() && self.modified == modified && self.end > 0 { return Ok(()); }
        if metadata.len() < self.length || (metadata.len() == self.length && self.modified != modified) { *self = Self::default(); }
        self.end = journal::scan(path, self.end, |offset, length, record| {
            match record.r#type.as_str() {
                "turn_checkpoint" => {
                    let turn: StoredTurn = serde_json::from_value(record.data).map_err(|_| AgentError::storage())?;
                    let replacing = self.entries.last().is_some_and(|last| last.excerpt.id == turn.turn.id);
                    if replacing { self.entries.pop(); }
                    else if !self.ids.insert(turn.turn.id.clone()) { return Err(AgentError::storage()); }
                    let edited_paths = turn.turn.steps.iter().flat_map(|step| &step.tools).filter(|tool| tool.status == "completed" && matches!(tool.name.as_str(), "write" | "edit")).filter_map(|tool| tool.args["path"].as_str().map(str::to_owned)).collect();
                    let entry = Entry { offset, length, excerpt: excerpt(&turn.turn, self.entries.len()), tokens: turn.wire.iter().map(compaction::estimate).collect(), limit: turn.turn.context_window, edited_paths };
                    self.entries.push(entry);
                }
                "queue_checkpoint" => self.queue = serde_json::from_value(record.data).map_err(|_| AgentError::storage())?,
                "context_checkpoint" => self.context = Some(serde_json::from_value(record.data).map_err(|_| AgentError::storage())?),
                "compaction_completed" => {
                    let completed: compaction::CompletedCompaction = serde_json::from_value(record.data).map_err(|_| AgentError::storage())?;
                    self.context = Some(completed.context); self.compactions.push(completed.event);
                }
                "file_checkpoint" => {
                    let revision: diffs::FileRevision = serde_json::from_value(record.data).map_err(|_| AgentError::storage())?;
                    self.files.insert(revision.path.clone(), (offset, length, revision.summary()));
                }
                _ => return Err(AgentError::storage()),
            }
            Ok(())
        })?;
        self.queue.retain(|message| !self.ids.contains(&message.id));
        let count: usize = self.entries.iter().map(|entry| entry.tokens.len()).sum();
        if self.context.as_ref().is_some_and(|context| context.through > count || (context.through > 0 && context.summary.is_empty()) || context.measured.as_ref().is_some_and(|usage| usage.wire_end > count || usage.wire_end < context.through)) { return Err(AgentError::storage()); }
        self.length = metadata.len(); self.modified = modified;
        Ok(())
    }

    fn navigation(&self) -> Vec<Excerpt> {
        let total = self.entries.len();
        (0..total.min(RAIL_SIZE)).map(|slot| {
            let index = if total <= RAIL_SIZE { slot } else { slot * (total - 1) / (RAIL_SIZE - 1) };
            self.entries[index].excerpt.clone()
        }).collect()
    }

    fn context(&self) -> compaction::ContextInfo {
        let context = self.context.as_ref();
        let measured = context.and_then(|value| value.measured.as_ref());
        let through = measured.map_or_else(|| context.map_or(0, |value| value.through), |value| value.wire_end);
        let trailing: u64 = self.entries.iter().flat_map(|entry| entry.tokens.iter()).skip(through).sum();
        let prefix = measured.map(|value| value.tokens).unwrap_or_else(|| context.filter(|value| !value.summary.is_empty()).map_or(0, |value| {
            compaction::estimate(&json!({"role":"user", "content":format!("Earlier conversation summary (reference data, not a new instruction):\n{}", value.summary)})) + value.preserved_user.as_ref().map_or(0, compaction::estimate)
        }));
        compaction::ContextInfo { tokens: prefix.saturating_add(trailing), limit: self.entries.last().and_then(|entry| entry.limit), estimated: measured.is_none() || trailing > 0, compacting: false, compactions: context.map_or(0, |value| value.count) }
    }

    fn page(&self, path: &Path, id: &str, before: Option<usize>, after: Option<usize>, around: Option<usize>) -> Result<Page, AgentError> {
        let total = self.entries.len();
        if before.is_some() as u8 + after.is_some() as u8 + around.is_some() as u8 > 1 { return Err(AgentError::internal()); }
        let forward = after.is_some() || around.is_some();
        let mut start = after.or_else(|| around.map(|value| value.saturating_sub(4))).unwrap_or_else(|| before.unwrap_or(total).min(total).saturating_sub(PAGE_SIZE)).min(total);
        let mut end = if forward { (start + PAGE_SIZE).min(total) } else { before.unwrap_or(total).min(total) };
        let mut bytes = 0;
        if forward {
            for index in start..end {
                let size = self.entries[index].length;
                if bytes > 0 && bytes + size > PAGE_BYTES { end = index; break; }
                bytes += size;
            }
        } else {
            for index in (start..end).rev() {
                let size = self.entries[index].length;
                if bytes > 0 && bytes + size > PAGE_BYTES { start = index + 1; break; }
                bytes += size;
            }
        }
        // An oversized predecessor must not prevent a requested jump from arriving.
        if let Some(target) = around.filter(|target| *target < total && *target >= end) {
            return self.page(path, id, None, Some(target), None);
        }
        let turns = self.entries[start..end].iter().map(|entry| {
            let mut stored: StoredTurn = serde_json::from_value(journal::record_at(path, entry.offset, entry.length)?.data).map_err(|_| AgentError::storage())?;
            if stored.turn.status == TurnStatus::Running {
                journal::interrupt_tools(&mut stored);
                stored.turn.status = TurnStatus::Interrupted;
                stored.turn.error = Some(AgentError::new("interrupted", "Execução interrompida. Revise os arquivos antes de continuar."));
            }
            Ok(stored.turn)
        }).collect::<Result<Vec<_>, AgentError>>()?;
        let ids: HashSet<_> = turns.iter().map(|turn| turn.id.as_str()).collect();
        let compactions = self.compactions.iter().filter(|event| ids.contains(event.turn_id.as_str())).cloned().collect();
        Ok(Page { conversation_id: id.into(), turns, compactions, history: Window { start, total }, navigation: self.navigation() })
    }

    fn weight(&self) -> usize {
        self.entries.iter().map(|entry| 256 + entry.excerpt.user.len() + entry.excerpt.assistant.len() + entry.tokens.len() * 8 + entry.edited_paths.iter().map(|path| path.len() + 24).sum::<usize>()).sum::<usize>() + self.files.len() * 512 + self.compactions.len() * 256
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Page {
    conversation_id: String, turns: Vec<Turn>, compactions: Vec<compaction::CompactionEvent>,
    history: Window, navigation: Vec<Excerpt>,
}

#[derive(Clone, Default)]
pub(super) struct HistoryState(Arc<Mutex<VecDeque<(PathBuf, Index)>>>);
impl HistoryState {
    pub(super) fn has_turn(&self, path: &Path, id: &str) -> Result<bool, AgentError> { self.with(path, |index| Ok(index.ids.contains(id))) }
    pub(super) fn worker_snapshot(&self, path: &Path, id: &str) -> Result<ChatSnapshot, AgentError> {
        self.with(path, |index| {
            let page = index.page(path, id, None, None, None)?;
            Ok(ChatSnapshot { conversation_id: id.into(), compacting: false, revision: next_revision(), turns: page.turns, history: page.history,
                navigation: Some(page.navigation), active_turn_id: None, pending_approval: None, pending_question: None,
                queued_messages: vec![], context: index.context(), compactions: page.compactions, file_changes: vec![],
            })
        })
    }
    pub(super) fn has_queue(&self, path: &Path) -> Result<bool, AgentError> { self.with(path, |index| Ok(!index.queue.is_empty())) }
    pub(super) fn forget(&self, path: &Path) { if let Ok(mut cache) = self.0.lock() { cache.retain(|(key, _)| key != path); } }
    fn with<T>(&self, path: &Path, action: impl FnOnce(&Index) -> Result<T, AgentError>) -> Result<T, AgentError> {
        let mut cache = self.0.lock().map_err(|_| AgentError::internal())?;
        let mut index = cache.iter().position(|(key, _)| key == path).and_then(|position| cache.remove(position)).map(|(_, value)| value).unwrap_or_default();
        index.refresh(path)?;
        let result = action(&index);
        if index.weight() <= 16 * 1024 * 1024 { cache.push_back((path.into(), index)); }
        while cache.len() > 4 || cache.iter().map(|(_, value)| value.weight()).sum::<usize>() > 16 * 1024 * 1024 { cache.pop_front(); }
        result
    }
}

impl AgentState {
    pub(super) fn file_session(&self, state: &AppState, home: &Path, id: &str) -> Result<Arc<Session>, AgentError> {
        let mut sessions = self.sessions.lock().map_err(|_| AgentError::internal())?;
        Self::prune_idle(&mut sessions);
        if let Some(session) = sessions.get(id) { return Ok(session.clone()); }
        let (path, root) = library::agent_location(state, home, id)?;
        let extras = self.histories.with(&path, |index| {
            let mut extras = journal::Extras::default();
            for (name, (offset, length, _)) in &index.files {
                extras.files.insert(name.clone(), serde_json::from_value(journal::record_at(&path, *offset, *length)?.data).map_err(|_| AgentError::storage())?);
            }
            let mut legacy = vec![];
            for entry in index.entries.iter().filter(|entry| entry.edited_paths.iter().any(|name| {
                let relative = Path::new(name).strip_prefix(&root).unwrap_or_else(|_| Path::new(name));
                !index.files.contains_key(relative.to_string_lossy().as_ref())
            })) {
                legacy.push(serde_json::from_value(journal::record_at(&path, entry.offset, entry.length)?.data).map_err(|_| AgentError::storage())?);
            }
            diffs::load_legacy(&root, &legacy, &mut extras.files);
            Ok(extras)
        })?;
        Ok(Arc::new(Session { id: id.into(), journal: path, root, emit: Arc::new(|_| {}), data: Mutex::new(SessionData { turns: vec![], active: None, revision: 0, storage_failed: false, last_emit: std::time::Instant::now(), extras, compacting: false, manual_compaction: false }) }))
    }

    fn history_page(&self, state: &AppState, home: &Path, id: &str, before: Option<usize>, after: Option<usize>, around: Option<usize>) -> Result<Page, AgentError> {
        let sessions = self.sessions.lock().map_err(|_| AgentError::internal())?;
        let (path, _) = library::agent_location(state, home, id)?;
        let mut page = self.histories.with(&path, |index| index.page(&path, id, before, after, around))?;
        // The last durable checkpoint can lag behind the active streamed response.
        if let Some(session) = sessions.get(id) {
            let data = session.data.lock().map_err(|_| AgentError::internal())?;
            for turn in &mut page.turns {
                if let Some(live) = data.turns.last().filter(|last| last.turn.id == turn.id) { *turn = live.turn.clone(); }
            }
        }
        Ok(page)
    }

    pub(super) fn read_chat(&self, state: &AppState, home: &Path, id: &str) -> Result<ChatSnapshot, AgentError> {
        let mut sessions = self.sessions.lock().map_err(|_| AgentError::internal())?;
        Self::prune_idle(&mut sessions);
        if let Some(session) = sessions.get(id) {
            let data = session.data.lock().map_err(|_| AgentError::internal())?;
            let mut snapshot = session.snapshot_data(&data);
            let mut start = data.turns.len().saturating_sub(PAGE_SIZE);
            let mut bytes = 0;
            for index in (start..data.turns.len()).rev() {
                let size = serde_json::to_vec(&data.turns[index].turn).map_err(|_| AgentError::storage())?.len();
                if bytes > 0 && bytes + size > PAGE_BYTES { start = index + 1; break; }
                bytes += size;
            }
            snapshot.turns = data.turns[start..].iter().map(|turn| turn.turn.clone()).collect();
            snapshot.history.start = start;
            snapshot.compactions = data.extras.compactions.iter().filter(|event| snapshot.turns.iter().any(|turn| turn.id == event.turn_id)).cloned().collect();
            snapshot.navigation = Some((0..data.turns.len().min(RAIL_SIZE)).map(|slot| {
                let index = if data.turns.len() <= RAIL_SIZE { slot } else { slot * (data.turns.len() - 1) / (RAIL_SIZE - 1) };
                excerpt(&data.turns[index].turn, index)
            }).collect());
            return Ok(snapshot);
        }
        let (path, _) = library::agent_location(state, home, id)?;
        self.histories.with(&path, |index| {
            let page = index.page(&path, id, None, None, None)?;
            Ok(ChatSnapshot { conversation_id: id.into(), compacting: false, revision: next_revision(),
                turns: page.turns, history: page.history, navigation: Some(page.navigation),
                active_turn_id: None, pending_approval: None, pending_question: None,
                queued_messages: index.queue.clone(), context: index.context(), compactions: page.compactions,
                file_changes: index.files.values().map(|(_, _, summary)| summary.clone()).collect(),
            })
        })
    }
}

#[tauri::command]
pub async fn get_chat_history(app: tauri::AppHandle, persistence: tauri::State<'_, AppState>, agent: tauri::State<'_, AgentState>, conversation_id: String, before: Option<usize>, after: Option<usize>, around: Option<usize>) -> Result<Page, AgentError> {
    let state = persistence.inner().clone(); let agent = agent.inner().clone();
    let home = app.path().home_dir().map_err(|_| AgentError::storage())?;
    tauri::async_runtime::spawn_blocking(move || agent.history_page(&state, &home, &conversation_id, before, after, around)).await.map_err(|_| AgentError::internal())?
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::tests::Fixture;
    use std::fs;
    fn stored(index: usize) -> StoredTurn {
        StoredTurn { turn: Turn { id: format!("t{index}"), created_at: index as u64, duration_ms: 0, user: format!("Pedido {index}"), parts: vec![], options: TurnOptions { account: "test".into(), model: "test".into(), reasoning: None, mode: Mode::Build, workflow: None, approval_mode: ApprovalMode::Manual }, context_window: Some(128000), status: TurnStatus::Completed, steps: vec![Step { text: format!("Resposta {index}"), ..Step::default() }], error: None }, wire: vec![json!({"role":"user", "content":format!("Pedido {index}")}), json!({"role":"assistant", "content":format!("Resposta {index}")})] }
    }
    #[test]
    fn indexes_latest_checkpoints_and_seeks_bounded_pages_with_compaction_markers() {
        let fixture = Fixture::new(); let path = fixture.root.join("history.jsonl"); fs::write(&path, "{}\n").unwrap();
        for index in 0..120 { journal::append(&path, &stored(index)).unwrap(); }
        let mut updated = stored(119); updated.turn.steps[0].text = "Resposta final".into(); journal::append(&path, &updated).unwrap();
        let event = compaction::CompactionEvent { id: "compact".into(), created_at: 30, turn_id: "t30".into(), after_turn: true, automatic: true, tokens_before: 10000, tokens_after: 1000 };
        let checkpoint = compaction::Checkpoint { through: 60, summary: "Resumo".into(), count: 1, ..Default::default() };
        journal::append_event(&path, "compaction_completed", &compaction::CompletedCompaction { context: checkpoint, event: event.clone() }).unwrap();
        let mut index = Index::default(); index.refresh(&path).unwrap();
        let tail = index.page(&path, "c", None, None, None).unwrap();
        assert_eq!(tail.turns.len(), 20); assert_eq!(tail.history.start, 100); assert_eq!(tail.history.total, 120);
        assert_eq!(tail.turns[19].steps[0].text, "Resposta final");
        assert_eq!(tail.navigation.len(), RAIL_SIZE); assert_eq!(tail.navigation.first().unwrap().index, 0); assert_eq!(tail.navigation.last().unwrap().index, 119);
        let jump = index.page(&path, "c", None, None, Some(30)).unwrap();
        assert!(jump.turns.iter().any(|turn| turn.id == "t30")); assert_eq!(jump.compactions, vec![event]);
        assert_eq!(index.page(&path, "c", Some(100), None, None).unwrap().history.start, 80);
        let (turns, extras) = journal::read_only(&path).unwrap();
        let session = crate::agent::tests::session(&fixture); let mut data = session.data.lock().unwrap(); data.turns = turns; data.extras = extras;
        assert_eq!(index.context().tokens, compaction::info(&data).tokens);
        let snapshot = session.snapshot_data(&data); assert_eq!(snapshot.turns.len(), 1); assert_eq!(snapshot.history.total, 120);
        drop(data);
        journal::append(&path, &stored(120)).unwrap(); index.refresh(&path).unwrap();
        assert_eq!(index.entries.len(), 121);
        let mut restarted = Index::default(); restarted.refresh(&path).unwrap();
        assert_eq!(restarted.context().tokens, index.context().tokens);
    }

    #[test]
    fn history_larger_than_sixty_four_megabytes_is_read_in_bounded_records() {
        let fixture = Fixture::new(); let path = fixture.root.join("large.jsonl"); fs::write(&path, "{}\n").unwrap();
        let mut turn = stored(0); turn.turn.steps[0].text = "x".repeat(1024 * 1024);
        for _ in 0..65 { journal::append(&path, &turn).unwrap(); }
        assert!(fs::metadata(&path).unwrap().len() > 64 * 1024 * 1024);
        let mut index = Index::default(); index.refresh(&path).unwrap();
        assert_eq!(index.entries.len(), 1); assert!(index.weight() < 4096);
        assert_eq!(index.page(&path, "c", None, None, None).unwrap().turns.len(), 1);
        assert_eq!(journal::read_only(&path).unwrap().0.len(), 1);
    }

    #[test]
    fn oversized_turn_does_not_hide_jump_target_and_cached_sessions_are_bounded() {
        let fixture = Fixture::new(); let path = fixture.root.join("bytes.jsonl"); fs::write(&path, "{}\n").unwrap();
        for i in 0..8 { let mut turn = stored(i); turn.turn.user = "x".repeat(PAGE_BYTES); journal::append(&path, &turn).unwrap(); }
        let mut index = Index::default(); index.refresh(&path).unwrap();
        let page = index.page(&path, "c", None, None, Some(6)).unwrap(); assert_eq!(page.turns[0].id, "t6"); assert_eq!(page.turns.len(), 1);
        let cache = HistoryState::default();
        for i in 0..7 { let path = fixture.root.join(format!("cache-{i}.jsonl")); fs::write(&path, "{}\n").unwrap(); cache.with(&path, |_| Ok(())).unwrap(); }
        assert_eq!(cache.0.lock().unwrap().len(), 4);
    }

    #[test]
    fn opening_a_chat_does_not_materialize_or_cache_runtime_replay() {
        let fixture = Fixture::new(); let state = AppState::default(); let agent = AgentState::default();
        let project = library::new_id().unwrap(); let id = library::new_id().unwrap();
        state.with_connection(&fixture.root, |connection| {
            connection.execute("INSERT INTO workspaces(id,name) VALUES ('workspace','Test')", [])?;
            connection.execute("INSERT INTO projects(id,workspace_id,name,path) VALUES (?1,'workspace','Project',?2)", rusqlite::params![project, fixture.root.to_string_lossy()])?;
            connection.execute("INSERT INTO conversations(id,project_id,title,created_at) VALUES (?1,?2,'Test',1)", rusqlite::params![id, project])?;
            Ok::<_, library::LibraryError>(())
        }).unwrap();
        let path = fixture.root.join(".jarvis/sessions").join(&project).join(format!("{id}.jsonl"));
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, format!("{}\n", json!({"type":"session","version":1,"id":id,"projectId":project,"title":"Test","createdAt":1,"cwd":fixture.root}))).unwrap();
        for index in 0..80 { journal::append(&path, &stored(index)).unwrap(); }
        let chat = agent.read_chat(&state, &fixture.root, &id).unwrap();
        assert_eq!(chat.history.start, 60); assert_eq!(chat.turns.len(), 20);
        assert!(agent.sessions.lock().unwrap().is_empty());
        let files = agent.file_session(&state, &fixture.root, &id).unwrap();
        assert!(files.data.lock().unwrap().turns.is_empty());
        assert!(agent.sessions.lock().unwrap().is_empty());
        let page = agent.history_page(&state, &fixture.root, &id, None, None, Some(3)).unwrap();
        assert_eq!(page.history.start, 0); assert_eq!(page.turns[3].id, "t3");

        // A completed runtime is evicted. Its durable history must supersede the
        // last running snapshot, even if the final event never reached the UI.
        let mut runtime = Arc::try_unwrap(crate::agent::tests::session(&fixture)).ok().unwrap();
        runtime.id = id.clone(); runtime.journal = path.clone();
        let session = Arc::new(runtime);
        agent.sessions.lock().unwrap().insert(id.clone(), session.clone());
        let _signal = session.reserve("Oi".into(), stored(0).turn.options).unwrap();
        let running = agent.read_chat(&state, &fixture.root, &id).unwrap();
        session.update(true, |data| data.turns.last_mut().unwrap().turn.steps.push(Step { text: "Olá!".into(), ..Step::default() })).unwrap();
        finish(&session, Ok(()));
        agent.release_idle(&session);
        assert!(agent.sessions.lock().unwrap().is_empty());
        let restored = agent.read_chat(&state, &fixture.root, &id).unwrap();
        assert!(restored.revision > running.revision, "durable completion must supersede live state");
        assert!(restored.active_turn_id.is_none());
        assert_eq!(restored.turns.last().unwrap().status, TurnStatus::Completed);
        assert_eq!(restored.turns.last().unwrap().steps.last().unwrap().text, "Olá!");
    }
}
