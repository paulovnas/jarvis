//! A disposable, bounded offset cache. Journals remain the only source of truth.
use super::*;
use std::{
    collections::{BTreeMap, HashSet, VecDeque},
    path::Path,
    time::SystemTime,
};

pub(super) const PAGE_SIZE: usize = 20;
const PAGE_BYTES: usize = 1024 * 1024;
const RAIL_SIZE: usize = 48;
const INDEX_CACHE_ENTRIES: usize = 16;
const INDEX_CACHE_BYTES: usize = 32 * 1024 * 1024;
const MAX_CACHED_PREVIEW_BYTES: usize = 256 * 1024;
const DEFERRED_DETAIL_KEY: &str = "_jarvisHistoryDetailsDeferred";

fn preview_value(value: &Value) -> Option<Value> {
    match value {
        Value::String(text) => Some(Value::String(text.chars().take(240).collect())),
        Value::Number(_) | Value::Bool(_) | Value::Null => Some(value.clone()),
        _ => None,
    }
}

fn history_preview(mut turn: Turn) -> Turn {
    const SUMMARY_KEYS: [&str; 12] = [
        "title",
        "path",
        "command",
        "query",
        "question",
        "url",
        "name",
        "libraryId",
        "library_id",
        "target",
        "port",
        "id",
    ];
    for tool in turn.steps.iter_mut().flat_map(|step| &mut step.tools) {
        if matches!(tool.status.as_str(), "pending" | "running")
            || matches!(
                tool.name.as_str(),
                "ask_user" | "generate_image" | "browser_screenshot"
            )
        {
            continue;
        }
        let mut args = serde_json::Map::new();
        for key in SUMMARY_KEYS {
            if let Some(value) = tool.args.get(key).and_then(preview_value) {
                args.insert(key.into(), value);
            }
        }
        args.insert(DEFERRED_DETAIL_KEY.into(), Value::Bool(true));
        tool.args = Value::Object(args);
        tool.output = if tool.name == "read_skill" {
            tool.output
                .lines()
                .next()
                .unwrap_or_default()
                .chars()
                .take(240)
                .collect()
        } else {
            String::new()
        };
    }
    turn
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Window {
    pub start: usize,
    pub total: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Excerpt {
    pub id: String,
    pub index: usize,
    pub created_at: u64,
    pub user: String,
    pub assistant: String,
}

fn snippet(text: &str) -> String {
    text.chars()
        .take(240)
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(160)
        .collect()
}

pub(super) fn excerpt(turn: &Turn, index: usize) -> Excerpt {
    Excerpt {
        id: turn.id.clone(),
        index,
        created_at: turn.created_at,
        user: snippet(&turn.user),
        assistant: snippet(turn.steps.last().map_or("", |step| &step.text)),
    }
}

struct Entry {
    offset: u64,
    length: usize,
    excerpt: Excerpt,
    preview: Option<Turn>,
    preview_size: usize,
    status: TurnStatus,
    resumable: bool,
    tokens: Vec<u64>,
    limit: Option<u64>,
    edited_paths: Vec<String>,
}
#[derive(Default)]
struct Index {
    end: u64,
    length: u64,
    modified: Option<SystemTime>,
    entries: Vec<Entry>,
    ids: HashSet<String>,
    queue: Vec<queue::QueuedMessage>,
    context: Option<compaction::Checkpoint>,
    compactions: Vec<compaction::CompactionEvent>,
    files: BTreeMap<String, (u64, usize, diffs::FileSummary)>,
    tail: Option<StoredTurn>,
    damaged_turn: Option<String>,
}

fn indexed_entry(
    turn: &StoredTurn,
    index: usize,
    offset: u64,
    length: usize,
) -> Result<Entry, AgentError> {
    let edited_paths = turn
        .turn
        .steps
        .iter()
        .flat_map(|step| &step.tools)
        .filter(|tool| {
            tool.status == "completed"
                && matches!(tool.name.as_str(), "write" | "edit" | "apply_patch")
        })
        .filter_map(|tool| tool.args["path"].as_str().map(str::to_owned))
        .collect();
    let preview = history_preview(turn.turn.clone());
    let serialized_preview = serde_json::to_vec(&preview)
        .map_err(|_| AgentError::storage())?
        .len();
    let (preview, preview_size) = if serialized_preview <= MAX_CACHED_PREVIEW_BYTES {
        (Some(preview), serialized_preview)
    } else {
        (None, 0)
    };
    Ok(Entry {
        offset,
        length,
        excerpt: excerpt(&turn.turn, index),
        preview,
        preview_size,
        status: turn.turn.status.clone(),
        resumable: resumable_direct_turn(turn),
        tokens: turn.wire.iter().map(compaction::estimate).collect(),
        limit: turn.turn.context_window,
        edited_paths,
    })
}

impl Index {
    fn refresh(&mut self, path: &Path) -> Result<(), AgentError> {
        let metadata = std::fs::symlink_metadata(path).map_err(|_| AgentError::storage())?;
        if !metadata.is_file() || metadata.is_symlink() {
            return Err(AgentError::storage());
        }
        let modified = metadata.modified().ok();
        if self.length == metadata.len() && self.modified == modified && self.end > 0 {
            return Ok(());
        }
        if metadata.len() < self.length
            || (metadata.len() == self.length && self.modified != modified)
        {
            *self = Self::default();
        }
        self.end = journal::scan(path, self.end, |offset, length, record| {
            match record.r#type.as_str() {
                "turn_checkpoint" => {
                    let turn: StoredTurn =
                        serde_json::from_value(record.data).map_err(|_| AgentError::storage())?;
                    if let Some(damaged) = self.damaged_turn.as_deref() {
                        if turn.turn.id != damaged
                            || !self
                                .entries
                                .last()
                                .is_some_and(|last| last.excerpt.id == damaged)
                        {
                            return Err(AgentError::storage());
                        }
                        self.damaged_turn = None;
                    }
                    let replacing = self
                        .entries
                        .last()
                        .is_some_and(|last| last.excerpt.id == turn.turn.id);
                    if replacing {
                        self.entries.pop();
                    } else if !self.ids.insert(turn.turn.id.clone()) {
                        return Err(AgentError::storage());
                    }
                    let entry = indexed_entry(&turn, self.entries.len(), offset, length)?;
                    self.tail = (turn.turn.status == TurnStatus::Running).then_some(turn);
                    self.entries.push(entry);
                }
                "turn_delta" => {
                    let current_id = self
                        .tail
                        .as_ref()
                        .map(|turn| turn.turn.id.clone())
                        .ok_or_else(AgentError::storage)?;
                    let delta = serde_json::from_value::<journal::TurnDelta>(record.data);
                    if let Some(damaged) = &self.damaged_turn {
                        if delta.as_ref().is_ok_and(|delta| delta.turn_id != *damaged) {
                            return Err(AgentError::storage());
                        }
                        return Ok(());
                    }
                    let Ok(delta) = delta else {
                        self.damaged_turn = Some(current_id);
                        return Ok(());
                    };
                    let mut candidate = self.tail.clone().ok_or_else(AgentError::storage)?;
                    if journal::apply_delta(&mut candidate, delta).is_err() {
                        self.damaged_turn = Some(current_id);
                        return Ok(());
                    }
                    let entry = self.entries.last().ok_or_else(AgentError::storage)?;
                    let replacement = indexed_entry(
                        &candidate,
                        self.entries.len().saturating_sub(1),
                        entry.offset,
                        entry.length,
                    )?;
                    *self.entries.last_mut().ok_or_else(AgentError::storage)? = replacement;
                    self.tail = Some(candidate);
                }
                "queue_checkpoint" => {
                    self.queue =
                        serde_json::from_value(record.data).map_err(|_| AgentError::storage())?
                }
                "context_checkpoint" => {
                    self.context = Some(
                        serde_json::from_value(record.data).map_err(|_| AgentError::storage())?,
                    )
                }
                "compaction_completed" => {
                    let completed: compaction::CompletedCompaction =
                        serde_json::from_value(record.data).map_err(|_| AgentError::storage())?;
                    self.context = Some(completed.context);
                    self.compactions.push(completed.event);
                }
                "file_checkpoint" => {
                    let revision: diffs::FileRevision =
                        serde_json::from_value(record.data).map_err(|_| AgentError::storage())?;
                    self.files
                        .insert(revision.path.clone(), (offset, length, revision.summary()));
                }
                _ => return Err(AgentError::storage()),
            }
            Ok(())
        })?;
        self.queue.retain(|message| !self.ids.contains(&message.id));
        let count: usize = self.entries.iter().map(|entry| entry.tokens.len()).sum();
        if self.context.as_ref().is_some_and(|context| {
            context.through > count
                || (context.through > 0 && context.summary.is_empty())
                || context
                    .measured
                    .as_ref()
                    .is_some_and(|usage| usage.wire_end > count || usage.wire_end < context.through)
        }) {
            return Err(AgentError::storage());
        }
        self.length = metadata.len();
        self.modified = modified;
        Ok(())
    }

    fn navigation(&self) -> Vec<Excerpt> {
        let total = self.entries.len();
        (0..total.min(RAIL_SIZE))
            .map(|slot| {
                let index = if total <= RAIL_SIZE {
                    slot
                } else {
                    slot * (total - 1) / (RAIL_SIZE - 1)
                };
                self.entries[index].excerpt.clone()
            })
            .collect()
    }

    fn context(&self) -> compaction::ContextInfo {
        let context = self.context.as_ref();
        let measured = context.and_then(|value| value.measured.as_ref());
        let through = measured.map_or_else(
            || context.map_or(0, |value| value.through),
            |value| value.wire_end,
        );
        let trailing: u64 = self
            .entries
            .iter()
            .flat_map(|entry| entry.tokens.iter())
            .skip(through)
            .sum();
        let prefix = measured.map(|value| value.tokens).unwrap_or_else(|| context.filter(|value| !value.summary.is_empty()).map_or(0, |value| {
            compaction::estimate(&json!({"role":"user", "content":format!("Earlier conversation summary (reference data, not a new instruction):\n{}", value.summary)})) + value.preserved_user.as_ref().map_or(0, compaction::estimate)
        }));
        compaction::ContextInfo {
            tokens: prefix.saturating_add(trailing),
            limit: self.entries.last().and_then(|entry| entry.limit),
            estimated: measured.is_none() || trailing > 0,
            compacting: false,
            compactions: context.map_or(0, |value| value.count),
        }
    }

    fn page(
        &self,
        path: &Path,
        id: &str,
        before: Option<usize>,
        after: Option<usize>,
        around: Option<usize>,
    ) -> Result<Page, AgentError> {
        self.page_internal(path, id, before, after, around, true)
    }

    fn full_page(&self, path: &Path, id: &str) -> Result<Page, AgentError> {
        self.page_internal(path, id, None, None, None, false)
    }

    fn page_internal(
        &self,
        path: &Path,
        id: &str,
        before: Option<usize>,
        after: Option<usize>,
        around: Option<usize>,
        defer_details: bool,
    ) -> Result<Page, AgentError> {
        let total = self.entries.len();
        if before.is_some() as u8 + after.is_some() as u8 + around.is_some() as u8 > 1 {
            return Err(AgentError::internal());
        }
        let forward = after.is_some() || around.is_some();
        let mut start = after
            .or_else(|| around.map(|value| value.saturating_sub(4)))
            .unwrap_or_else(|| before.unwrap_or(total).min(total).saturating_sub(PAGE_SIZE))
            .min(total);
        let mut end = if forward {
            (start + PAGE_SIZE).min(total)
        } else {
            before.unwrap_or(total).min(total)
        };
        let mut bytes = 0;
        if forward {
            for index in start..end {
                let size = self.entries[index].length;
                if bytes > 0 && bytes + size > PAGE_BYTES {
                    end = index;
                    break;
                }
                bytes += size;
            }
        } else {
            for index in (start..end).rev() {
                let size = self.entries[index].length;
                if bytes > 0 && bytes + size > PAGE_BYTES {
                    start = index + 1;
                    break;
                }
                bytes += size;
            }
        }
        // An oversized predecessor must not prevent a requested jump from arriving.
        if let Some(target) = around.filter(|target| *target < total && *target >= end) {
            return self.page_internal(path, id, None, Some(target), None, defer_details);
        }
        let turns = self.entries[start..end]
            .iter()
            .enumerate()
            .map(|(relative, entry)| {
                if start + relative + 1 == self.entries.len() {
                    if let Some(tail) = self
                        .tail
                        .as_ref()
                        .filter(|turn| turn.turn.id == entry.excerpt.id)
                    {
                        let mut stored = tail.clone();
                        if stored.turn.status == TurnStatus::Running {
                            journal::interrupt_tools(&mut stored);
                            stored.turn.status = TurnStatus::Interrupted;
                            stored.turn.error = Some(AgentError::new(
                                "interrupted",
                                "Execução interrompida. Revise os arquivos antes de continuar.",
                            ));
                        }
                        return Ok(if defer_details {
                            history_preview(stored.turn)
                        } else {
                            stored.turn
                        });
                    }
                }
                if defer_details && entry.status != TurnStatus::Running {
                    if let Some(preview) = &entry.preview {
                        return Ok(preview.clone());
                    }
                }
                let mut stored: StoredTurn = serde_json::from_value(
                    journal::record_at(path, entry.offset, entry.length)?.data,
                )
                .map_err(|_| AgentError::storage())?;
                if stored.turn.status == TurnStatus::Running {
                    journal::interrupt_tools(&mut stored);
                    stored.turn.status = TurnStatus::Interrupted;
                    stored.turn.error = Some(AgentError::new(
                        "interrupted",
                        "Execução interrompida. Revise os arquivos antes de continuar.",
                    ));
                }
                Ok(if defer_details {
                    history_preview(stored.turn)
                } else {
                    stored.turn
                })
            })
            .collect::<Result<Vec<_>, AgentError>>()?;
        let ids: HashSet<_> = turns.iter().map(|turn| turn.id.as_str()).collect();
        let compactions = self
            .compactions
            .iter()
            .filter(|event| ids.contains(event.turn_id.as_str()))
            .cloned()
            .collect();
        Ok(Page {
            conversation_id: id.into(),
            turns,
            compactions,
            history: Window { start, total },
            navigation: self.navigation(),
        })
    }

    fn weight(&self) -> usize {
        self.entries
            .iter()
            .map(|entry| {
                entry.preview_size
                    + entry.excerpt.user.len()
                    + entry.excerpt.assistant.len()
                    + entry.tokens.len() * 8
                    + entry
                        .edited_paths
                        .iter()
                        .map(|path| path.len() + 24)
                        .sum::<usize>()
            })
            .sum::<usize>()
            + self.files.len() * 512
            + self.compactions.len() * 256
    }

    fn tool_call(&self, path: &Path, turn_id: &str, tool_id: &str) -> Result<ToolCall, AgentError> {
        if let Some(tool) = self.tail.as_ref().and_then(|turn| {
            (turn.turn.id == turn_id).then_some(turn).and_then(|turn| {
                turn.turn
                    .steps
                    .iter()
                    .flat_map(|step| &step.tools)
                    .find(|tool| tool.id == tool_id)
            })
        }) {
            return Ok(tool.clone());
        }
        let entry = self
            .entries
            .iter()
            .find(|entry| entry.excerpt.id == turn_id)
            .ok_or_else(|| {
                AgentError::new(
                    "history_detail_not_found",
                    "A interação não está mais disponível no histórico.",
                )
            })?;
        let stored: StoredTurn =
            serde_json::from_value(journal::record_at(path, entry.offset, entry.length)?.data)
                .map_err(|_| AgentError::storage())?;
        stored
            .turn
            .steps
            .into_iter()
            .flat_map(|step| step.tools)
            .find(|tool| tool.id == tool_id)
            .ok_or_else(|| {
                AgentError::new(
                    "history_detail_not_found",
                    "Os detalhes desta ação não estão mais disponíveis.",
                )
            })
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Page {
    conversation_id: String,
    turns: Vec<Turn>,
    compactions: Vec<compaction::CompactionEvent>,
    history: Window,
    navigation: Vec<Excerpt>,
}

#[derive(Clone, Default)]
pub(super) struct HistoryState(Arc<Mutex<VecDeque<(PathBuf, Index)>>>);
impl HistoryState {
    pub(super) fn has_recovery_tail(&self, path: &Path) -> Result<bool, AgentError> {
        self.with(path, |index| {
            Ok(index
                .entries
                .last()
                .is_some_and(|entry| entry.status == TurnStatus::Running || entry.resumable))
        })
    }
    pub(super) fn has_turn(&self, path: &Path, id: &str) -> Result<bool, AgentError> {
        self.with(path, |index| Ok(index.ids.contains(id)))
    }
    pub(super) fn worker_snapshot(
        &self,
        path: &Path,
        id: &str,
    ) -> Result<ChatSnapshot, AgentError> {
        self.with(path, |index| {
            let page = index.full_page(path, id)?;
            Ok(ChatSnapshot {
                conversation_id: id.into(),
                compacting: false,
                revision: next_revision(),
                turns: page.turns,
                history: page.history,
                navigation: Some(page.navigation),
                active_turn_id: None,
                pending_approval: None,
                pending_question: None,
                pending_authoring: None,
                queued_messages: vec![],
                context: index.context(),
                compactions: page.compactions,
                file_changes: vec![],
            })
        })
    }
    pub(super) fn has_queue(&self, path: &Path) -> Result<bool, AgentError> {
        self.with(path, |index| Ok(!index.queue.is_empty()))
    }
    pub(super) fn forget(&self, path: &Path) {
        if let Ok(mut cache) = self.0.lock() {
            cache.retain(|(key, _)| key != path);
        }
    }
    fn with<T>(
        &self,
        path: &Path,
        action: impl FnOnce(&Index) -> Result<T, AgentError>,
    ) -> Result<T, AgentError> {
        let mut cache = self.0.lock().map_err(|_| AgentError::internal())?;
        let mut index = cache
            .iter()
            .position(|(key, _)| key == path)
            .and_then(|position| cache.remove(position))
            .map(|(_, value)| value)
            .unwrap_or_default();
        index.refresh(path)?;
        let result = action(&index);
        if index.weight() <= 16 * 1024 * 1024 {
            cache.push_back((path.into(), index));
        }
        while cache.len() > INDEX_CACHE_ENTRIES
            || cache.iter().map(|(_, value)| value.weight()).sum::<usize>() > INDEX_CACHE_BYTES
        {
            cache.pop_front();
        }
        result
    }
}

impl AgentState {
    pub(super) fn file_session(
        &self,
        state: &AppState,
        home: &Path,
        id: &str,
    ) -> Result<Arc<Session>, AgentError> {
        let mut sessions = self.sessions.lock().map_err(|_| AgentError::internal())?;
        Self::prune_idle(&mut sessions);
        if let Some(session) = sessions.get(id) {
            return Ok(session.clone());
        }
        let (path, root) = library::agent_location(state, home, id)?;
        let extras = self.histories.with(&path, |index| {
            let mut extras = journal::Extras::default();
            for (name, (offset, length, _)) in &index.files {
                extras.files.insert(
                    name.clone(),
                    serde_json::from_value(journal::record_at(&path, *offset, *length)?.data)
                        .map_err(|_| AgentError::storage())?,
                );
            }
            let mut legacy = vec![];
            for entry in index.entries.iter().filter(|entry| {
                entry.edited_paths.iter().any(|name| {
                    let relative = Path::new(name)
                        .strip_prefix(&root)
                        .unwrap_or_else(|_| Path::new(name));
                    !index
                        .files
                        .contains_key(relative.to_string_lossy().as_ref())
                })
            }) {
                legacy.push(
                    serde_json::from_value(
                        journal::record_at(&path, entry.offset, entry.length)?.data,
                    )
                    .map_err(|_| AgentError::storage())?,
                );
            }
            diffs::load_legacy(&root, &legacy, &mut extras.files);
            Ok(extras)
        })?;
        Ok(Arc::new(Session {
            id: id.into(),
            journal: path,
            root,
            journal_maintenance: Default::default(),
            emit: Arc::new(|_| {}),
            data: Mutex::new(SessionData {
                turns: vec![],
                durable_turn: None,
                active: None,
                recovery: None,
                revision: 0,
                storage_failed: false,
                last_emit: std::time::Instant::now(),
                extras,
                compacting: false,
                manual_compaction: false,
            }),
        }))
    }

    fn history_page(
        &self,
        state: &AppState,
        home: &Path,
        id: &str,
        before: Option<usize>,
        after: Option<usize>,
        around: Option<usize>,
    ) -> Result<Page, AgentError> {
        let sessions = self.sessions.lock().map_err(|_| AgentError::internal())?;
        let (path, _) = library::agent_location(state, home, id)?;
        let mut page = self
            .histories
            .with(&path, |index| index.page(&path, id, before, after, around))?;
        // The last durable checkpoint can lag behind the active streamed response.
        if let Some(session) = sessions.get(id) {
            let data = session.data.lock().map_err(|_| AgentError::internal())?;
            for turn in &mut page.turns {
                if let Some(live) = data.turns.last().filter(|last| last.turn.id == turn.id) {
                    *turn = live.turn.clone();
                }
            }
        }
        Ok(page)
    }

    fn chat_tool_call(
        &self,
        state: &AppState,
        home: &Path,
        id: &str,
        turn_id: &str,
        tool_id: &str,
    ) -> Result<ToolCall, AgentError> {
        {
            let sessions = self.sessions.lock().map_err(|_| AgentError::internal())?;
            if let Some(tool) = sessions.get(id).and_then(|session| {
                session.data.lock().ok().and_then(|data| {
                    data.turns
                        .iter()
                        .rev()
                        .find(|stored| stored.turn.id == turn_id)
                        .and_then(|stored| {
                            stored
                                .turn
                                .steps
                                .iter()
                                .flat_map(|step| &step.tools)
                                .find(|tool| tool.id == tool_id)
                        })
                        .cloned()
                })
            }) {
                return Ok(tool);
            }
        }
        let (path, _) = library::agent_location(state, home, id)?;
        self.histories
            .with(&path, |index| index.tool_call(&path, turn_id, tool_id))
    }

    pub(super) fn read_chat(
        &self,
        state: &AppState,
        home: &Path,
        id: &str,
    ) -> Result<ChatSnapshot, AgentError> {
        let mut sessions = self.sessions.lock().map_err(|_| AgentError::internal())?;
        Self::prune_idle(&mut sessions);
        if let Some(session) = sessions.get(id) {
            let data = session.data.lock().map_err(|_| AgentError::internal())?;
            let mut snapshot = session.snapshot_data(&data);
            let mut start = data.turns.len().saturating_sub(PAGE_SIZE);
            let mut bytes = 0;
            for index in (start..data.turns.len()).rev() {
                let size = serde_json::to_vec(&data.turns[index].turn)
                    .map_err(|_| AgentError::storage())?
                    .len();
                if bytes > 0 && bytes + size > PAGE_BYTES {
                    start = index + 1;
                    break;
                }
                bytes += size;
            }
            snapshot.turns = data.turns[start..]
                .iter()
                .map(|turn| history_preview(turn.turn.clone()))
                .collect();
            snapshot.history.start = start;
            snapshot.compactions = data
                .extras
                .compactions
                .iter()
                .filter(|event| snapshot.turns.iter().any(|turn| turn.id == event.turn_id))
                .cloned()
                .collect();
            snapshot.navigation = Some(
                (0..data.turns.len().min(RAIL_SIZE))
                    .map(|slot| {
                        let index = if data.turns.len() <= RAIL_SIZE {
                            slot
                        } else {
                            slot * (data.turns.len() - 1) / (RAIL_SIZE - 1)
                        };
                        excerpt(&data.turns[index].turn, index)
                    })
                    .collect(),
            );
            return Ok(snapshot);
        }
        let (path, _) = library::agent_location(state, home, id)?;
        self.histories.with(&path, |index| {
            let page = index.page(&path, id, None, None, None)?;
            Ok(ChatSnapshot {
                conversation_id: id.into(),
                compacting: false,
                revision: next_revision(),
                turns: page.turns,
                history: page.history,
                navigation: Some(page.navigation),
                active_turn_id: None,
                pending_approval: None,
                pending_question: None,
                pending_authoring: None,
                queued_messages: index.queue.clone(),
                context: index.context(),
                compactions: page.compactions,
                file_changes: index
                    .files
                    .values()
                    .map(|(_, _, summary)| summary.clone())
                    .collect(),
            })
        })
    }
}

#[tauri::command]
pub async fn get_chat_history(
    app: tauri::AppHandle,
    persistence: tauri::State<'_, AppState>,
    agent: tauri::State<'_, AgentState>,
    conversation_id: String,
    before: Option<usize>,
    after: Option<usize>,
    around: Option<usize>,
) -> Result<Page, AgentError> {
    let state = persistence.inner().clone();
    let agent = agent.inner().clone();
    let home = app.path().home_dir().map_err(|_| AgentError::storage())?;
    tauri::async_runtime::spawn_blocking(move || {
        agent.history_page(&state, &home, &conversation_id, before, after, around)
    })
    .await
    .map_err(|_| AgentError::internal())?
}

#[tauri::command]
pub async fn get_chat_tool_call(
    app: tauri::AppHandle,
    persistence: tauri::State<'_, AppState>,
    agent: tauri::State<'_, AgentState>,
    conversation_id: String,
    turn_id: String,
    tool_id: String,
) -> Result<ToolCall, AgentError> {
    let state = persistence.inner().clone();
    let agent = agent.inner().clone();
    let home = app.path().home_dir().map_err(|_| AgentError::storage())?;
    tauri::async_runtime::spawn_blocking(move || {
        agent.chat_tool_call(&state, &home, &conversation_id, &turn_id, &tool_id)
    })
    .await
    .map_err(|_| AgentError::internal())?
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::tests::Fixture;
    use std::fs;
    fn stored(index: usize) -> StoredTurn {
        StoredTurn {
            mcp_intent: None,
            turn: Turn {
                id: format!("t{index}"),
                created_at: index as u64,
                duration_ms: 0,
                user: format!("Pedido {index}"),
                parts: vec![],
                options: TurnOptions {
                    account: "test".into(),
                    model: "test".into(),
                    reasoning: None,
                    mode: Mode::Build,
                    workflow: None,
                    custom_workflow_id: None,
                    custom_agent_id: None,
                    approval_mode: ApprovalMode::Manual,
                },
                context_window: Some(128000),
                status: TurnStatus::Completed,
                tasks: vec![],
                steps: vec![Step {
                    text: format!("Resposta {index}"),
                    ..Step::default()
                }],
                error: None,
            },
            wire: vec![
                json!({"role":"user", "content":format!("Pedido {index}")}),
                json!({"role":"assistant", "content":format!("Resposta {index}")}),
            ],
        }
    }
    #[test]
    fn indexes_latest_checkpoints_and_seeks_bounded_pages_with_compaction_markers() {
        let fixture = Fixture::new();
        let path = fixture.root.join("history.jsonl");
        fs::write(&path, "{}\n").unwrap();
        for index in 0..120 {
            journal::append(&path, &stored(index)).unwrap();
        }
        let mut updated = stored(119);
        updated.turn.steps[0].text = "Resposta final".into();
        journal::append(&path, &updated).unwrap();
        let event = compaction::CompactionEvent {
            id: "compact".into(),
            created_at: 30,
            turn_id: "t30".into(),
            after_turn: true,
            automatic: true,
            tokens_before: 10000,
            tokens_after: 1000,
        };
        let checkpoint = compaction::Checkpoint {
            through: 60,
            summary: "Resumo".into(),
            count: 1,
            ..Default::default()
        };
        journal::append_event(
            &path,
            "compaction_completed",
            &compaction::CompletedCompaction {
                context: checkpoint,
                event: event.clone(),
            },
        )
        .unwrap();
        let mut index = Index::default();
        index.refresh(&path).unwrap();
        let tail = index.page(&path, "c", None, None, None).unwrap();
        assert_eq!(tail.turns.len(), 20);
        assert_eq!(tail.history.start, 100);
        assert_eq!(tail.history.total, 120);
        assert_eq!(tail.turns[19].steps[0].text, "Resposta final");
        assert_eq!(tail.navigation.len(), RAIL_SIZE);
        assert_eq!(tail.navigation.first().unwrap().index, 0);
        assert_eq!(tail.navigation.last().unwrap().index, 119);
        let jump = index.page(&path, "c", None, None, Some(30)).unwrap();
        assert!(jump.turns.iter().any(|turn| turn.id == "t30"));
        assert_eq!(jump.compactions, vec![event]);
        assert_eq!(
            index
                .page(&path, "c", Some(100), None, None)
                .unwrap()
                .history
                .start,
            80
        );
        let (turns, extras) = journal::read_only(&path).unwrap();
        let session = crate::agent::tests::session(&fixture);
        let mut data = session.data.lock().unwrap();
        data.turns = turns;
        data.extras = extras;
        assert_eq!(index.context().tokens, compaction::info(&data).tokens);
        let snapshot = session.snapshot_data(&data);
        assert_eq!(snapshot.turns.len(), 1);
        assert_eq!(snapshot.history.total, 120);
        drop(data);
        journal::append(&path, &stored(120)).unwrap();
        index.refresh(&path).unwrap();
        assert_eq!(index.entries.len(), 121);
        let mut restarted = Index::default();
        restarted.refresh(&path).unwrap();
        assert_eq!(restarted.context().tokens, index.context().tokens);
    }

    #[test]
    fn live_index_waits_for_a_checkpoint_that_supersedes_an_invalid_delta() {
        let fixture = Fixture::new();
        let path = fixture.root.join("recoverable-delta.jsonl");
        fs::write(&path, "{}\n").unwrap();
        let mut initial = stored(0);
        initial.turn.status = TurnStatus::Running;
        initial.turn.steps.clear();
        initial.wire.truncate(1);
        journal::append(&path, &initial).unwrap();
        journal::append_event(
            &path,
            "turn_delta",
            &journal::TurnDelta {
                turn_id: initial.turn.id.clone(),
                operations: vec![journal::DeltaOperation::Set {
                    path: vec![
                        journal::DeltaPathPart::Key("turn".into()),
                        journal::DeltaPathPart::Key("steps".into()),
                        journal::DeltaPathPart::Index(0),
                        journal::DeltaPathPart::Key("durationMs".into()),
                    ],
                    value: json!(42),
                }],
            },
        )
        .unwrap();

        let mut index = Index::default();
        index.refresh(&path).unwrap();
        assert_eq!(index.damaged_turn.as_deref(), Some("t0"));

        journal::append(&path, &stored(0)).unwrap();
        index.refresh(&path).unwrap();
        assert!(index.damaged_turn.is_none());
        let page = index.page(&path, "conversation", None, None, None).unwrap();
        assert_eq!(page.turns[0].steps[0].text, "Resposta 0");
    }

    #[test]
    fn safe_direct_interruption_is_loaded_for_automatic_recovery() {
        let fixture = Fixture::new();
        let path = fixture.root.join("history.jsonl");
        fs::write(&path, "{}\n").unwrap();
        let mut turn = stored(0);
        turn.turn.options.workflow = Some(workflow::Flow::Designer);
        turn.turn.status = TurnStatus::Interrupted;
        turn.turn.error = Some(AgentError::new(
            "interrupted",
            "O Jarvis foi encerrado durante esta execução.",
        ));
        turn.turn.steps[0].tools.push(ToolCall {
            id: "read-1".into(),
            name: "read".into(),
            args: json!({"path":"README.md"}),
            status: "completed".into(),
            output: "# Jarvis".into(),
            duration_ms: 1,
        });
        turn.wire.extend([
            json!({"type":"function_call","call_id":"read-1","name":"read","arguments":"{\"path\":\"README.md\"}"}),
            json!({"type":"function_call_output","call_id":"read-1","output":"# Jarvis"}),
        ]);
        journal::append(&path, &turn).unwrap();

        assert!(HistoryState::default().has_recovery_tail(&path).unwrap());
    }

    #[test]
    fn history_larger_than_sixty_four_megabytes_is_read_in_bounded_records() {
        let fixture = Fixture::new();
        let path = fixture.root.join("large.jsonl");
        fs::write(&path, "{}\n").unwrap();
        let mut turn = stored(0);
        turn.turn.steps[0].text = "x".repeat(1024 * 1024);
        for _ in 0..65 {
            journal::append(&path, &turn).unwrap();
        }
        assert!(fs::metadata(&path).unwrap().len() > 64 * 1024 * 1024);
        let mut index = Index::default();
        index.refresh(&path).unwrap();
        assert_eq!(index.entries.len(), 1);
        assert!(index.weight() < 4096);
        assert_eq!(
            index
                .page(&path, "c", None, None, None)
                .unwrap()
                .turns
                .len(),
            1
        );
        assert_eq!(journal::read_only(&path).unwrap().0.len(), 1);
    }

    #[test]
    fn oversized_turn_does_not_hide_jump_target_and_cached_sessions_are_bounded() {
        let fixture = Fixture::new();
        let path = fixture.root.join("bytes.jsonl");
        fs::write(&path, "{}\n").unwrap();
        for i in 0..8 {
            let mut turn = stored(i);
            turn.turn.user = "x".repeat(PAGE_BYTES);
            journal::append(&path, &turn).unwrap();
        }
        let mut index = Index::default();
        index.refresh(&path).unwrap();
        let page = index.page(&path, "c", None, None, Some(6)).unwrap();
        assert_eq!(page.turns[0].id, "t6");
        assert_eq!(page.turns.len(), 1);
        let cache = HistoryState::default();
        for i in 0..(INDEX_CACHE_ENTRIES + 3) {
            let path = fixture.root.join(format!("cache-{i}.jsonl"));
            fs::write(&path, "{}\n").unwrap();
            cache.with(&path, |_| Ok(())).unwrap();
        }
        assert_eq!(cache.0.lock().unwrap().len(), INDEX_CACHE_ENTRIES);
    }

    #[test]
    fn history_defers_large_tool_details_until_the_action_is_requested() {
        let fixture = Fixture::new();
        let path = fixture.root.join("deferred-details.jsonl");
        fs::write(&path, "{}\n").unwrap();
        let mut turn = stored(0);
        let content = "x".repeat(512 * 1024);
        let output = "y".repeat(512 * 1024);
        turn.turn.steps[0].tools.push(ToolCall {
            id: "large-read".into(),
            name: "read".into(),
            args: json!({"path":"src/large.ts","content":content}),
            status: "completed".into(),
            output,
            duration_ms: 8,
        });
        journal::append(&path, &turn).unwrap();
        let mut index = Index::default();
        index.refresh(&path).unwrap();
        assert!(index.entries[0].preview.is_some());

        let page = index.page(&path, "chat", None, None, None).unwrap();
        let preview = &page.turns[0].steps[0].tools[0];
        assert_eq!(preview.args[DEFERRED_DETAIL_KEY], true);
        assert_eq!(preview.args["path"], "src/large.ts");
        assert!(preview.args.get("content").is_none());
        assert!(preview.output.is_empty());
        assert!(serde_json::to_vec(&page).unwrap().len() < 16 * 1024);

        let detail = index.tool_call(&path, "t0", "large-read").unwrap();
        assert_eq!(detail.args["content"].as_str().unwrap().len(), 512 * 1024);
        assert_eq!(detail.output.len(), 512 * 1024);
    }

    #[test]
    fn opening_a_chat_does_not_materialize_or_cache_runtime_replay() {
        let fixture = Fixture::new();
        let state = AppState::default();
        let agent = AgentState::default();
        let project = library::new_id().unwrap();
        let id = library::new_id().unwrap();
        state.with_connection(&fixture.root, |connection| {
            connection.execute("INSERT INTO workspaces(id,name) VALUES ('workspace','Test')", [])?;
            connection.execute("INSERT INTO projects(id,workspace_id,name,path) VALUES (?1,'workspace','Project',?2)", rusqlite::params![project, fixture.root.to_string_lossy()])?;
            connection.execute("INSERT INTO conversations(id,project_id,title,created_at) VALUES (?1,?2,'Test',1)", rusqlite::params![id, project])?;
            Ok::<_, library::LibraryError>(())
        }).unwrap();
        let path = crate::data_dir::root(&fixture.root)
            .join("sessions")
            .join(&project)
            .join(format!("{id}.jsonl"));
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, format!("{}\n", json!({"type":"session","version":1,"id":id,"projectId":project,"title":"Test","createdAt":1,"cwd":fixture.root}))).unwrap();
        for index in 0..80 {
            journal::append(&path, &stored(index)).unwrap();
        }
        let chat = agent.read_chat(&state, &fixture.root, &id).unwrap();
        assert_eq!(chat.history.start, 60);
        assert_eq!(chat.turns.len(), 20);
        assert!(agent.sessions.lock().unwrap().is_empty());
        let files = agent.file_session(&state, &fixture.root, &id).unwrap();
        assert!(files.data.lock().unwrap().turns.is_empty());
        assert!(agent.sessions.lock().unwrap().is_empty());
        let page = agent
            .history_page(&state, &fixture.root, &id, None, None, Some(3))
            .unwrap();
        assert_eq!(page.history.start, 0);
        assert_eq!(page.turns[3].id, "t3");

        // A completed runtime is evicted. Its durable history must supersede the
        // last running snapshot, even if the final event never reached the UI.
        let mut runtime = Arc::try_unwrap(crate::agent::tests::session(&fixture))
            .ok()
            .unwrap();
        runtime.id = id.clone();
        runtime.journal = path.clone();
        let session = Arc::new(runtime);
        agent
            .sessions
            .lock()
            .unwrap()
            .insert(id.clone(), session.clone());
        let _signal = session
            .reserve("Oi".into(), stored(0).turn.options)
            .unwrap();
        let running = agent.read_chat(&state, &fixture.root, &id).unwrap();
        session
            .update(true, |data| {
                data.turns.last_mut().unwrap().turn.steps.push(Step {
                    text: "Olá!".into(),
                    ..Step::default()
                })
            })
            .unwrap();
        finish(&session, Ok(()));
        agent.release_idle(&session);
        assert!(agent.sessions.lock().unwrap().is_empty());
        let restored = agent.read_chat(&state, &fixture.root, &id).unwrap();
        assert!(
            restored.revision > running.revision,
            "durable completion must supersede live state"
        );
        assert!(restored.active_turn_id.is_none());
        assert_eq!(restored.turns.last().unwrap().status, TurnStatus::Completed);
        assert_eq!(
            restored.turns.last().unwrap().steps.last().unwrap().text,
            "Olá!"
        );
    }
}
