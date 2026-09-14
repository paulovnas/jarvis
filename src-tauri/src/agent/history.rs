//! A disposable, bounded offset cache. Journals remain the only source of truth.
use super::*;
use std::{
    collections::{BTreeMap, HashSet, VecDeque},
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::Path,
    time::SystemTime,
};

pub(super) const PAGE_SIZE: usize = 20;
const PAGE_BYTES: usize = 1024 * 1024;
const RAIL_SIZE: usize = 48;
const INDEX_CACHE_ENTRIES: usize = 16;
const INDEX_CACHE_BYTES: usize = 32 * 1024 * 1024;
const MAX_SINGLE_INDEX_BYTES: usize = 16 * 1024 * 1024;
const MAX_SIDECAR_BYTES: usize = 32 * 1024 * 1024;
const SIDECAR_VERSION: u8 = 3;
const MAX_CACHED_PREVIEW_BYTES: usize = 256 * 1024;
const DEFERRED_DETAIL_KEY: &str = "_jarvisHistoryDetailsDeferred";

fn preview_value(value: &Value) -> Option<Value> {
    match value {
        Value::String(text) => Some(Value::String(text.chars().take(240).collect())),
        Value::Number(_) | Value::Bool(_) | Value::Null => Some(value.clone()),
        _ => None,
    }
}

pub(super) fn history_preview(mut turn: Turn) -> Turn {
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
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(rename = "HistoryWindow"))]
#[serde(rename_all = "camelCase")]
pub(super) struct Window {
    pub start: usize,
    pub total: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(rename = "HistoryExcerpt"))]
#[serde(rename_all = "camelCase")]
pub(super) struct Excerpt {
    pub id: String,
    pub index: usize,
    #[cfg_attr(test, ts(type = "number"))]
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
    mcp_intent: Option<crate::mcp::McpIntent>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PersistedEntry {
    offset: u64,
    length: usize,
    excerpt: Excerpt,
    status: TurnStatus,
    resumable: bool,
    tokens: Vec<u64>,
    limit: Option<u64>,
    edited_paths: Vec<String>,
    mcp_intent: Option<crate::mcp::McpIntent>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PersistedIndex {
    version: u8,
    fingerprint: journal::PrefixFingerprint,
    entries: Vec<PersistedEntry>,
    queue: Vec<queue::QueuedMessage>,
    context: Option<compaction::Checkpoint>,
    compactions: Vec<compaction::CompactionEvent>,
    files: BTreeMap<String, (u64, usize, diffs::FileSummary)>,
    tail: Option<StoredTurn>,
    damaged_turn: Option<String>,
    replay: ReplayCursor,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ReplayCursor {
    through: usize,
    start_entry: usize,
    wire_base: usize,
    total_items: usize,
}

pub(super) struct ReplayLoad {
    pub turns: Vec<StoredTurn>,
    pub extras: journal::Extras,
    pub wire_base: usize,
    pub turn_base: usize,
    pub total_turns: usize,
    pub inherited_mcp_intent: crate::mcp::McpIntent,
    pub file_checkpoints: Vec<diffs::FileRevision>,
    pub replayed_bytes: u64,
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
    replay: ReplayCursor,
}

impl PersistedEntry {
    fn from_entry(entry: &Entry) -> Self {
        Self {
            offset: entry.offset,
            length: entry.length,
            excerpt: entry.excerpt.clone(),
            status: entry.status.clone(),
            resumable: entry.resumable,
            tokens: entry.tokens.clone(),
            limit: entry.limit,
            edited_paths: entry.edited_paths.clone(),
            mcp_intent: entry.mcp_intent.clone(),
        }
    }

    fn into_entry(self) -> Entry {
        Entry {
            offset: self.offset,
            length: self.length,
            excerpt: self.excerpt,
            preview: None,
            preview_size: 0,
            status: self.status,
            resumable: self.resumable,
            tokens: self.tokens,
            limit: self.limit,
            edited_paths: self.edited_paths,
            mcp_intent: self.mcp_intent,
        }
    }
}

impl PersistedIndex {
    fn from_index(index: &Index, fingerprint: journal::PrefixFingerprint) -> Self {
        Self {
            version: SIDECAR_VERSION,
            fingerprint,
            entries: index
                .entries
                .iter()
                .map(PersistedEntry::from_entry)
                .collect(),
            queue: index.queue.clone(),
            context: index.context.clone(),
            compactions: index.compactions.clone(),
            files: index.files.clone(),
            tail: index.tail.clone(),
            damaged_turn: index.damaged_turn.clone(),
            replay: index.replay.clone(),
        }
    }

    fn into_index(self) -> Option<Index> {
        if self.version != SIDECAR_VERSION || self.fingerprint.length == 0 {
            return None;
        }
        let end = self.fingerprint.length;
        let mut ids = HashSet::with_capacity(self.entries.len());
        let mut entries = Vec::with_capacity(self.entries.len());
        for (index, entry) in self.entries.into_iter().enumerate() {
            if entry.length == 0
                || entry.offset == 0
                || entry.excerpt.id.is_empty()
                || entry.excerpt.index != index
                || entry
                    .offset
                    .checked_add(entry.length as u64)
                    .is_none_or(|record_end| record_end > end)
                || !ids.insert(entry.excerpt.id.clone())
            {
                return None;
            }
            entries.push(entry.into_entry());
        }
        if self.files.values().any(|(offset, length, _)| {
            *length == 0
                || *offset == 0
                || offset
                    .checked_add(*length as u64)
                    .is_none_or(|record_end| record_end > end)
        }) {
            return None;
        }
        let running_tail = entries
            .last()
            .is_some_and(|entry| entry.status == TurnStatus::Running);
        if running_tail != self.tail.is_some()
            || self.tail.as_ref().is_some_and(|tail| {
                tail.turn.status != TurnStatus::Running
                    || entries
                        .last()
                        .is_none_or(|entry| entry.excerpt.id != tail.turn.id)
            })
        {
            return None;
        }
        if self
            .damaged_turn
            .as_ref()
            .is_some_and(|id| entries.last().is_none_or(|entry| entry.excerpt.id != *id))
        {
            return None;
        }
        let index = Index {
            end,
            length: end,
            modified: None,
            entries,
            ids,
            queue: self.queue,
            context: self.context,
            compactions: self.compactions,
            files: self.files,
            tail: self.tail,
            damaged_turn: self.damaged_turn,
            replay: self.replay,
        };
        (index.valid_context() && index.replay == index.derive_replay_cursor()).then_some(index)
    }
}

fn sidecar_path(path: &Path) -> PathBuf {
    path.with_extension("jarvis-index.json")
}

fn load_sidecar(
    path: &Path,
    metadata: &fs::Metadata,
) -> Option<(Index, journal::PrefixFingerprint)> {
    let sidecar = sidecar_path(path);
    let sidecar_metadata = fs::symlink_metadata(&sidecar).ok()?;
    if !sidecar_metadata.is_file()
        || sidecar_metadata.is_symlink()
        || sidecar_metadata.len() > MAX_SIDECAR_BYTES as u64
    {
        return None;
    }
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW);
    }
    let mut file = options.open(sidecar).ok()?;
    let mut bytes = Vec::with_capacity(sidecar_metadata.len() as usize);
    Read::take(&mut file, MAX_SIDECAR_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() > MAX_SIDECAR_BYTES {
        return None;
    }
    let persisted: PersistedIndex = serde_json::from_slice(&bytes).ok()?;
    if metadata.len() < persisted.fingerprint.length {
        return None;
    }
    let fingerprint = persisted.fingerprint.clone();
    let mut index = persisted.into_index()?;
    if metadata.len() == index.end {
        index.length = metadata.len();
        index.modified = metadata.modified().ok();
    }
    Some((index, fingerprint))
}

fn persist_sidecar(
    path: &Path,
    index: &Index,
    fingerprint: journal::PrefixFingerprint,
) -> Result<(), AgentError> {
    let destination = sidecar_path(path);
    if fs::symlink_metadata(&destination)
        .is_ok_and(|metadata| !metadata.is_file() || metadata.is_symlink())
    {
        return Err(AgentError::storage());
    }
    let bytes = serde_json::to_vec(&PersistedIndex::from_index(index, fingerprint))
        .map_err(|_| AgentError::storage())?;
    if bytes.len() > MAX_SIDECAR_BYTES {
        return Ok(());
    }
    let parent = destination.parent().ok_or_else(AgentError::storage)?;
    let mut temporary =
        tempfile::NamedTempFile::new_in(parent).map_err(|_| AgentError::storage())?;
    temporary
        .write_all(&bytes)
        .and_then(|()| temporary.as_file_mut().sync_all())
        .map_err(|_| AgentError::storage())?;
    temporary
        .persist(destination)
        .map_err(|_| AgentError::storage())?;
    Ok(())
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
        mcp_intent: turn.mcp_intent.clone(),
    })
}

impl Index {
    fn derive_replay_cursor(&self) -> ReplayCursor {
        let through = self.context.as_ref().map_or(0, |context| context.through);
        let total_items = self.entries.iter().map(|entry| entry.tokens.len()).sum();
        let mut start_entry = 0;
        let mut wire_base = 0usize;
        while let Some(entry) = self.entries.get(start_entry) {
            let end = wire_base.saturating_add(entry.tokens.len());
            if end > through {
                break;
            }
            wire_base = end;
            start_entry += 1;
        }
        ReplayCursor {
            through,
            start_entry,
            wire_base,
            total_items,
        }
    }

    fn stored_turn(&self, path: &Path, entry_index: usize) -> Result<StoredTurn, AgentError> {
        let entry = self
            .entries
            .get(entry_index)
            .ok_or_else(AgentError::storage)?;
        if entry_index + 1 == self.entries.len() {
            if let Some(tail) = self
                .tail
                .as_ref()
                .filter(|tail| tail.turn.id == entry.excerpt.id)
            {
                return Ok(tail.clone());
            }
        }
        serde_json::from_value(journal::record_at(path, entry.offset, entry.length)?.data)
            .map_err(|_| AgentError::storage())
    }

    fn load_replay(&self, path: &Path, root: &Path) -> Result<ReplayLoad, AgentError> {
        if self.damaged_turn.is_some() {
            return Err(AgentError::storage());
        }
        let mut replayed_records = HashSet::new();
        let turns = (self.replay.start_entry..self.entries.len())
            .map(|entry_index| {
                let entry = &self.entries[entry_index];
                replayed_records.insert((entry.offset, entry.length));
                self.stored_turn(path, entry_index)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let mut extras = journal::Extras {
            queue: self.queue.clone(),
            context: self.context.clone(),
            compactions: self.compactions.clone(),
            files: BTreeMap::new(),
        };
        for (name, (offset, length, _)) in &self.files {
            replayed_records.insert((*offset, *length));
            extras.files.insert(
                name.clone(),
                serde_json::from_value(journal::record_at(path, *offset, *length)?.data)
                    .map_err(|_| AgentError::storage())?,
            );
        }
        let persisted_files: HashSet<_> = extras.files.keys().cloned().collect();
        let mut legacy = vec![];
        for (entry_index, entry) in self.entries.iter().enumerate().filter(|(_, entry)| {
            entry.edited_paths.iter().any(|name| {
                let relative = Path::new(name)
                    .strip_prefix(root)
                    .unwrap_or_else(|_| Path::new(name));
                !self.files.contains_key(relative.to_string_lossy().as_ref())
            })
        }) {
            replayed_records.insert((entry.offset, entry.length));
            legacy.push(self.stored_turn(path, entry_index)?);
        }
        diffs::load_legacy(root, &legacy, &mut extras.files);
        let file_checkpoints = extras
            .files
            .values()
            .filter(|file| !persisted_files.contains(&file.path))
            .cloned()
            .collect();
        let inherited_mcp_intent = self.entries[..self.replay.start_entry]
            .iter()
            .rev()
            .find_map(|entry| entry.mcp_intent.clone())
            .unwrap_or_default();
        let replayed_bytes = replayed_records
            .into_iter()
            .map(|(_, length)| u64::try_from(length).unwrap_or(u64::MAX))
            .sum();
        Ok(ReplayLoad {
            turns,
            extras,
            wire_base: self.replay.wire_base,
            turn_base: self.replay.start_entry,
            total_turns: self.entries.len(),
            inherited_mcp_intent,
            file_checkpoints,
            replayed_bytes,
        })
    }

    fn valid_context(&self) -> bool {
        let count: usize = self.entries.iter().map(|entry| entry.tokens.len()).sum();
        !self.context.as_ref().is_some_and(|context| {
            context.through > count
                || (context.through > 0 && context.summary.is_empty())
                || context
                    .measured
                    .as_ref()
                    .is_some_and(|usage| usage.wire_end > count || usage.wire_end < context.through)
        })
    }

    fn apply_record(
        &mut self,
        offset: u64,
        length: usize,
        record: journal::Record,
    ) -> Result<(), AgentError> {
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
                self.context =
                    Some(serde_json::from_value(record.data).map_err(|_| AgentError::storage())?)
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
    }

    fn scan_from(
        &mut self,
        path: &Path,
        expected: Option<&journal::PrefixFingerprint>,
    ) -> Result<Option<journal::ScanSnapshot>, AgentError> {
        let start = self.end;
        if let Some(expected) = expected {
            journal::scan_snapshot_after_verified_prefix(
                path,
                start,
                expected,
                |offset, length, record| self.apply_record(offset, length, record),
            )
        } else {
            journal::scan_snapshot(path, start, |offset, length, record| {
                self.apply_record(offset, length, record)
            })
            .map(Some)
        }
    }

    fn refresh(&mut self, path: &Path) -> Result<(), AgentError> {
        let metadata = std::fs::symlink_metadata(path).map_err(|_| AgentError::storage())?;
        if !metadata.is_file() || metadata.is_symlink() {
            return Err(AgentError::storage());
        }
        let modified = metadata.modified().ok();
        let mut verified_prefix = None;
        if self.end == 0 {
            if let Some((index, fingerprint)) = load_sidecar(path, &metadata) {
                *self = index;
                verified_prefix = Some(fingerprint);
            }
        }
        if verified_prefix.is_none()
            && self.length == metadata.len()
            && self.modified == modified
            && self.end > 0
        {
            return Ok(());
        }
        if metadata.len() < self.end
            || metadata.len() < self.length
            || (metadata.len() == self.length && self.modified != modified)
        {
            *self = Self::default();
            verified_prefix = None;
        }
        let snapshot = match self.scan_from(path, verified_prefix.as_ref())? {
            Some(snapshot) => snapshot,
            None => {
                *self = Self::default();
                self.scan_from(path, None)?
                    .ok_or_else(AgentError::storage)?
            }
        };
        let sidecar_is_current = verified_prefix
            .as_ref()
            .is_some_and(|fingerprint| fingerprint == &snapshot.fingerprint);
        self.end = snapshot.end;
        self.queue.retain(|message| !self.ids.contains(&message.id));
        if !self.valid_context() {
            return Err(AgentError::storage());
        }
        self.replay = self.derive_replay_cursor();
        self.length = snapshot.file_length;
        self.modified = snapshot.modified;
        if !sidecar_is_current {
            let _ = persist_sidecar(path, self, snapshot.fingerprint);
        }
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
        let prefix = measured
            .map(|value| value.tokens)
            .unwrap_or_else(|| context.map_or(0, compaction::prefix_tokens));
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
                std::mem::size_of::<Entry>()
                    + entry.preview_size
                    + entry.excerpt.id.len()
                    + entry.excerpt.user.len()
                    + entry.excerpt.assistant.len()
                    + entry.tokens.len() * 8
                    + entry
                        .edited_paths
                        .iter()
                        .map(|path| path.len() + 24)
                        .sum::<usize>()
                    + entry.mcp_intent.as_ref().map_or(0, |intent| {
                        intent
                            .servers
                            .iter()
                            .chain(&intent.excluded_servers)
                            .map(|server| server.id.len() + server.name.len() + 32)
                            .sum::<usize>()
                            + 32
                    })
            })
            .sum::<usize>()
            + self
                .queue
                .iter()
                .map(|message| message.id.len() + message.content.len() + 256)
                .sum::<usize>()
            + self
                .context
                .as_ref()
                .map_or(0, |context| context.summary.len())
            + self.files.len() * 512
            + self.compactions.len() * 256
            + self.damaged_turn.as_ref().map_or(0, String::len)
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

struct CachedIndex {
    path: PathBuf,
    index: Arc<Mutex<Index>>,
    weight: usize,
}

#[derive(Default)]
struct HistoryCache {
    entries: VecDeque<CachedIndex>,
}

impl HistoryCache {
    fn trim(&mut self, releasing: Option<&Arc<Mutex<Index>>>) {
        while self.entries.len() > INDEX_CACHE_ENTRIES
            || self.entries.iter().map(|entry| entry.weight).sum::<usize>() > INDEX_CACHE_BYTES
        {
            let Some(position) = self.entries.iter().position(|entry| {
                Arc::strong_count(&entry.index) == 1
                    || releasing.is_some_and(|current| {
                        Arc::ptr_eq(current, &entry.index) && Arc::strong_count(&entry.index) == 2
                    })
            }) else {
                break;
            };
            self.entries.remove(position);
        }
    }
}

#[derive(Clone, Default)]
pub(super) struct HistoryState(Arc<Mutex<HistoryCache>>);
impl HistoryState {
    pub(super) fn load_replay(&self, path: &Path, root: &Path) -> Result<ReplayLoad, AgentError> {
        let (valid_end, file_length) = self.with(path, |index| Ok((index.end, index.length)))?;
        if valid_end < file_length {
            journal::repair_incomplete_tail(path, valid_end)?;
            self.forget(path);
        }
        self.with(path, |index| index.load_replay(path, root))
    }

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
                protocol_version: protocol::VERSION,
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
            cache.entries.retain(|entry| entry.path != path);
        }
    }

    fn cached_index(&self, path: &Path) -> Result<Arc<Mutex<Index>>, AgentError> {
        let mut cache = self.0.lock().map_err(|_| AgentError::internal())?;
        if let Some(position) = cache.entries.iter().position(|entry| entry.path == path) {
            let entry = cache
                .entries
                .remove(position)
                .ok_or_else(AgentError::internal)?;
            let index = entry.index.clone();
            cache.entries.push_back(entry);
            return Ok(index);
        }
        let index = Arc::new(Mutex::new(Index::default()));
        cache.entries.push_back(CachedIndex {
            path: path.into(),
            index: index.clone(),
            weight: 0,
        });
        cache.trim(None);
        Ok(index)
    }

    fn finish(&self, path: &Path, index: &Arc<Mutex<Index>>, weight: usize, succeeded: bool) {
        let Ok(mut cache) = self.0.lock() else {
            return;
        };
        let Some(position) = cache
            .entries
            .iter()
            .position(|entry| entry.path == path && Arc::ptr_eq(&entry.index, index))
        else {
            return;
        };
        let Some(mut entry) = cache.entries.remove(position) else {
            return;
        };
        if succeeded && weight <= MAX_SINGLE_INDEX_BYTES {
            entry.weight = weight;
            cache.entries.push_back(entry);
            cache.trim(Some(index));
        }
    }

    fn with<T>(
        &self,
        path: &Path,
        action: impl FnOnce(&Index) -> Result<T, AgentError>,
    ) -> Result<T, AgentError> {
        let cached = self.cached_index(path)?;
        let Ok(mut index) = cached.lock() else {
            self.finish(path, &cached, 0, false);
            return Err(AgentError::internal());
        };
        let (result, weight) = {
            let result = index.refresh(path).and_then(|()| action(&index));
            (result, index.weight())
        };
        drop(index);
        self.finish(path, &cached, weight, result.is_ok());
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
        let gate = self.session_gate(id)?;
        let _gate = gate.lock().map_err(|_| AgentError::internal())?;
        {
            let mut sessions = self.sessions.lock().map_err(|_| AgentError::internal())?;
            Self::prune_idle(&mut sessions);
            if let Some(session) = sessions.get(id) {
                return Ok(session.clone());
            }
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
        let writer = session_writer::SessionWriter::start(path.clone(), id.into(), None)?;
        let file_session = Arc::new(Session {
            id: id.into(),
            journal: path,
            root,
            journal_maintenance: Default::default(),
            writer,
            emit: Arc::new(|_| {}),
            data: Mutex::new(SessionData {
                turns: vec![],
                turn_base: 0,
                wire_base: 0,
                inherited_mcp_intent: crate::mcp::McpIntent::default(),
                active: None,
                recovery: None,
                revision: 0,
                storage_failed: false,
                last_emit: std::time::Instant::now(),
                extras,
                compacting: false,
                manual_compaction: false,
            }),
        });
        Ok(self
            .sessions
            .lock()
            .map_err(|_| AgentError::internal())?
            .get(id)
            .cloned()
            .unwrap_or(file_session))
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
        let (path, _) = library::agent_location(state, home, id)?;
        let mut page = self
            .histories
            .with(&path, |index| index.page(&path, id, before, after, around))?;
        // The last durable checkpoint can lag behind the active streamed response.
        let session = self
            .sessions
            .lock()
            .map_err(|_| AgentError::internal())?
            .get(id)
            .cloned();
        if let Some(session) = session {
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
        let session = self
            .sessions
            .lock()
            .map_err(|_| AgentError::internal())?
            .get(id)
            .cloned();
        if let Some(session) = session {
            if let Some(tool) = session.data.lock().ok().and_then(|data| {
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
                            .find(|tool| {
                                tool.id == tool_id && tool.args[DEFERRED_DETAIL_KEY] != true
                            })
                    })
                    .cloned()
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
        // Serialize disk snapshots with runtime session creation. Otherwise a
        // read that started while the session was loading could receive a newer
        // global revision while still carrying the older on-disk transcript,
        // temporarily hiding the submitted turn in the renderer.
        let gate = self.session_gate(id)?;
        let _gate = gate.lock().map_err(|_| AgentError::internal())?;
        let session = {
            let mut sessions = self.sessions.lock().map_err(|_| AgentError::internal())?;
            Self::prune_idle(&mut sessions);
            sessions.get(id).cloned()
        };
        if let Some(session) = session {
            let data = session.data.lock().map_err(|_| AgentError::internal())?;
            let mut snapshot = session.snapshot_data(&data);
            if data.turns.is_empty() {
                drop(data);
                let page = self.histories.with(&session.journal, |index| {
                    index.page(&session.journal, id, None, None, None)
                })?;
                snapshot.turns = page.turns;
                snapshot.history = page.history;
                snapshot.navigation = Some(page.navigation);
                snapshot.compactions = page.compactions;
                return Ok(snapshot);
            }
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
            snapshot.history.start = data.turn_base.saturating_add(start);
            snapshot.compactions = data
                .extras
                .compactions
                .iter()
                .filter(|event| snapshot.turns.iter().any(|turn| turn.id == event.turn_id))
                .cloned()
                .collect();
            snapshot.navigation = Some(
                self.histories
                    .with(&session.journal, |index| Ok(index.navigation()))?,
            );
            return Ok(snapshot);
        }
        let (path, _) = library::agent_location(state, home, id)?;
        self.histories.with(&path, |index| {
            let page = index.page(&path, id, None, None, None)?;
            Ok(ChatSnapshot {
                protocol_version: protocol::VERSION,
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
    use std::{
        fs,
        sync::mpsc,
        thread,
        time::{Duration, Instant},
    };
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
                    manual_validation: false,
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

    fn checkpoint(through: usize) -> compaction::Checkpoint {
        compaction::Checkpoint {
            through,
            summary: format!("Resumo ate {through}"),
            count: 1,
            ..Default::default()
        }
    }

    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct ReplayMemoryFixture {
        schema_version: u64,
        id: String,
        description: String,
        history_turns: Vec<usize>,
        items_per_turn: usize,
        checkpoint_retained_turns: Vec<usize>,
        restart_count: usize,
    }

    #[test]
    fn harness_evaluation_bounds_replay_after_multiple_compactions_and_restarts() {
        let specification: ReplayMemoryFixture = serde_json::from_str(include_str!(
            "fixtures/evaluations/bounded-replay-memory.json"
        ))
        .unwrap();
        assert_eq!(specification.schema_version, 1);
        assert_eq!(specification.id, "bounded-replay-memory");
        assert!(!specification.description.trim().is_empty());
        assert_eq!(specification.items_per_turn, 2);
        let retained_turns = *specification.checkpoint_retained_turns.last().unwrap();
        let fixture = Fixture::new();
        let mut observed = vec![];

        for total_turns in specification.history_turns {
            let path = fixture.root.join(format!("bounded-{total_turns}.jsonl"));
            fs::write(&path, "{}\n").unwrap();
            for turn_index in 0..total_turns {
                let mut turn = stored(turn_index);
                let payload = format!("turn-{turn_index}-{}", "x".repeat(512));
                turn.turn.steps[0].text = payload.clone();
                turn.wire[1]["content"] = json!(payload);
                journal::append(&path, &turn).unwrap();
            }
            for (index, retained) in specification.checkpoint_retained_turns.iter().enumerate() {
                let through = total_turns.saturating_sub(*retained) * specification.items_per_turn;
                let context = compaction::Checkpoint {
                    through,
                    summary: format!("Compaction checkpoint {}", index + 1),
                    preserved_users: vec![json!({
                        "role": "user",
                        "content": "Preserve the current objective."
                    })],
                    tool_receipts: vec![json!({
                        "callId": format!("receipt-{index}"),
                        "tool": "read",
                        "output": "completed"
                    })],
                    count: u64::try_from(index + 1).unwrap(),
                    ..Default::default()
                };
                journal::append_event(
                    &path,
                    "compaction_completed",
                    &compaction::CompletedCompaction {
                        context,
                        event: compaction::CompactionEvent {
                            id: format!("compaction-{index}"),
                            created_at: u64::try_from(index + 1).unwrap(),
                            turn_id: format!("t{}", total_turns - *retained - 1),
                            after_turn: true,
                            automatic: true,
                            tokens_before: 10_000,
                            tokens_after: 1_000,
                        },
                    },
                )
                .unwrap();
            }

            let (full_turns, full_extras) = journal::read_only(&path).unwrap();
            let session = crate::agent::tests::session(&fixture);
            let expected = {
                let mut data = session.data.lock().unwrap();
                data.turns = full_turns;
                data.extras = full_extras;
                compaction::input(&data)
            };
            let journal_bytes = fs::metadata(&path).unwrap().len();
            for _ in 0..specification.restart_count {
                let replay = HistoryState::default()
                    .load_replay(&path, &fixture.root)
                    .unwrap();
                let replayed_items = replay
                    .turns
                    .iter()
                    .map(|turn| turn.wire.len())
                    .sum::<usize>();
                assert_eq!(replay.total_turns, total_turns);
                assert_eq!(replay.turns.len(), retained_turns);
                assert_eq!(
                    replayed_items,
                    retained_turns * specification.items_per_turn
                );
                assert!(replay.replayed_bytes < journal_bytes / 4);
                let mut data = session.data.lock().unwrap();
                data.turns = replay.turns;
                data.turn_base = replay.turn_base;
                data.wire_base = replay.wire_base;
                data.inherited_mcp_intent = replay.inherited_mcp_intent;
                data.extras = replay.extras;
                assert_eq!(compaction::input(&data), expected);
                observed.push((total_turns, data.turns.len(), replayed_items));
            }
        }
        assert!(observed.iter().all(|(_, turns, items)| {
            *turns == retained_turns && *items == retained_turns * specification.items_per_turn
        }));
        println!(
            "HARNESS_EVAL case={} current={observed:?}",
            specification.id
        );
    }

    #[test]
    fn replay_cursor_keeps_the_complete_turn_that_crosses_the_checkpoint() {
        let fixture = Fixture::new();
        let path = fixture.root.join("replay-boundaries.jsonl");
        fs::write(&path, "{}\n").unwrap();
        for index in 0..3 {
            journal::append(&path, &stored(index)).unwrap();
        }

        journal::append_event(&path, "context_checkpoint", &checkpoint(3)).unwrap();
        let mut index = Index::default();
        index.refresh(&path).unwrap();
        assert_eq!(
            index.replay,
            ReplayCursor {
                through: 3,
                start_entry: 1,
                wire_base: 2,
                total_items: 6,
            }
        );

        journal::append_event(&path, "context_checkpoint", &checkpoint(4)).unwrap();
        index.refresh(&path).unwrap();
        assert_eq!(index.replay.start_entry, 2);
        assert_eq!(index.replay.wire_base, 4);

        journal::append_event(&path, "context_checkpoint", &checkpoint(6)).unwrap();
        index.refresh(&path).unwrap();
        assert_eq!(index.replay.start_entry, 3);
        assert_eq!(index.replay.wire_base, 6);
        assert_eq!(index.replay.total_items, 6);
    }

    #[test]
    fn replay_cursor_rebuilds_legacy_sidecars_without_compaction() {
        let fixture = Fixture::new();
        let path = fixture.root.join("legacy-replay.jsonl");
        fs::write(&path, "{}\n").unwrap();
        for index in 0..3 {
            journal::append(&path, &stored(index)).unwrap();
        }
        Index::default().refresh(&path).unwrap();
        let sidecar = sidecar_path(&path);
        let mut persisted: serde_json::Value =
            serde_json::from_slice(&fs::read(&sidecar).unwrap()).unwrap();
        persisted["version"] = json!(1);
        persisted.as_object_mut().unwrap().remove("replay");
        fs::write(&sidecar, serde_json::to_vec(&persisted).unwrap()).unwrap();

        let mut rebuilt = Index::default();
        rebuilt.refresh(&path).unwrap();

        assert_eq!(
            rebuilt.replay,
            ReplayCursor {
                through: 0,
                start_entry: 0,
                wire_base: 0,
                total_items: 6,
            }
        );
        assert!(rebuilt.entries.iter().all(|entry| entry.preview.is_some()));
        let persisted: PersistedIndex =
            serde_json::from_slice(&fs::read(sidecar).unwrap()).unwrap();
        assert_eq!(persisted.version, SIDECAR_VERSION);
        assert_eq!(persisted.replay, rebuilt.replay);
    }

    #[test]
    fn replay_cursor_ignores_an_incomplete_final_record() {
        let fixture = Fixture::new();
        let path = fixture.root.join("incomplete-replay.jsonl");
        fs::write(&path, "{}\n").unwrap();
        for index in 0..2 {
            journal::append(&path, &stored(index)).unwrap();
        }
        journal::append_event(&path, "context_checkpoint", &checkpoint(2)).unwrap();
        OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap()
            .write_all(b"{incomplete")
            .unwrap();

        let mut index = Index::default();
        index.refresh(&path).unwrap();

        assert_eq!(index.entries.len(), 2);
        assert_eq!(
            index.replay,
            ReplayCursor {
                through: 2,
                start_entry: 1,
                wire_base: 2,
                total_items: 4,
            }
        );

        let replay = HistoryState::default()
            .load_replay(&path, &fixture.root)
            .unwrap();
        assert_eq!(replay.turns.len(), 1);
        assert_eq!(replay.wire_base, 2);
        assert!(fs::read(&path).unwrap().ends_with(b"\n"));
        assert!(fixture.root.read_dir().unwrap().any(|entry| entry
            .unwrap()
            .path()
            .to_string_lossy()
            .contains("recovery-")));
    }

    #[test]
    fn bounded_replay_matches_full_model_input_and_inherits_mcp_intent() {
        let fixture = Fixture::new();
        let path = fixture.root.join("bounded-replay.jsonl");
        fs::write(&path, "{}\n").unwrap();
        for turn_index in 0..5 {
            let mut turn = stored(turn_index);
            if turn_index == 1 {
                turn.mcp_intent = Some(crate::mcp::McpIntent {
                    mode: crate::mcp::McpIntentMode::Explicit,
                    servers: vec![crate::mcp::McpIntentServer {
                        id: "notebook".into(),
                        name: "Gemini Notebook".into(),
                    }],
                    excluded_servers: vec![],
                });
            }
            journal::append(&path, &turn).unwrap();
        }
        let mut context = checkpoint(5);
        context.preserved_users = vec![json!({
            "role": "user",
            "content": "Use somente o notebook solicitado."
        })];
        context.tool_receipts = vec![json!({
            "callId": "write-1",
            "tool": "write",
            "output": "completed"
        })];
        journal::append_event(&path, "context_checkpoint", &context).unwrap();
        let (all_turns, all_extras) = journal::read_only(&path).unwrap();
        let replay = HistoryState::default()
            .load_replay(&path, &fixture.root)
            .unwrap();

        assert_eq!(replay.total_turns, 5);
        assert_eq!(replay.turn_base, 2);
        assert_eq!(replay.wire_base, 4);
        assert_eq!(replay.turns.len(), 3);
        assert_eq!(
            replay.inherited_mcp_intent.mode,
            crate::mcp::McpIntentMode::Explicit
        );
        assert_eq!(replay.inherited_mcp_intent.servers[0].id, "notebook");

        let session = crate::agent::tests::session(&fixture);
        let mut data = session.data.lock().unwrap();
        data.turns = all_turns;
        data.extras = all_extras;
        let full_input = compaction::input(&data);
        data.turns = replay.turns;
        data.turn_base = replay.turn_base;
        data.wire_base = replay.wire_base;
        data.inherited_mcp_intent = replay.inherited_mcp_intent;
        data.extras = replay.extras;
        assert_eq!(compaction::input(&data), full_input);
        assert_eq!(data.total_turns(), 5);
        assert_eq!(session.snapshot_data(&data).history.total, 5);
    }

    #[test]
    fn bounded_replay_materializes_the_latest_running_delta() {
        let fixture = Fixture::new();
        let path = fixture.root.join("running-replay.jsonl");
        fs::write(&path, "{}\n").unwrap();
        for turn_index in 0..2 {
            journal::append(&path, &stored(turn_index)).unwrap();
        }
        journal::append_event(&path, "context_checkpoint", &checkpoint(4)).unwrap();
        let mut initial = stored(2);
        initial.turn.status = TurnStatus::Running;
        initial.turn.steps.clear();
        initial.wire.truncate(1);
        journal::append(&path, &initial).unwrap();
        let mut updated = initial.clone();
        updated.turn.duration_ms = 42;
        updated.turn.steps.push(Step {
            text: "Resposta parcial".into(),
            ..Step::default()
        });
        updated
            .wire
            .push(json!({"role":"assistant", "content":"Resposta parcial"}));
        journal::append_update(&path, &initial, &updated).unwrap();

        let replay = HistoryState::default()
            .load_replay(&path, &fixture.root)
            .unwrap();

        assert_eq!(replay.turn_base, 2);
        assert_eq!(replay.wire_base, 4);
        assert_eq!(replay.turns.len(), 1);
        assert_eq!(replay.turns[0].turn.id, updated.turn.id);
        assert_eq!(replay.turns[0].turn.duration_ms, 42);
        assert_eq!(replay.turns[0].wire, updated.wire);
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
        assert!(restarted
            .entries
            .iter()
            .all(|entry| entry.preview.is_none()));
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
        assert_eq!(cache.0.lock().unwrap().entries.len(), INDEX_CACHE_ENTRIES);
    }

    #[test]
    fn appended_journal_reuses_verified_sidecar_and_indexes_only_the_suffix() {
        let fixture = Fixture::new();
        let path = fixture.root.join("appended.jsonl");
        fs::write(&path, "{}\n").unwrap();
        for index in 0..40 {
            journal::append(&path, &stored(index)).unwrap();
        }
        Index::default().refresh(&path).unwrap();
        journal::append(&path, &stored(40)).unwrap();

        let mut restarted = Index::default();
        restarted.refresh(&path).unwrap();

        assert_eq!(restarted.entries.len(), 41);
        assert!(restarted.entries[..40]
            .iter()
            .all(|entry| entry.preview.is_none()));
        assert!(restarted.entries[40].preview.is_some());
    }

    #[test]
    fn invalid_sidecar_is_rebuilt_from_the_preserved_journal() {
        let fixture = Fixture::new();
        let path = fixture.root.join("invalid-sidecar.jsonl");
        fs::write(&path, "{}\n").unwrap();
        for index in 0..25 {
            journal::append(&path, &stored(index)).unwrap();
        }
        Index::default().refresh(&path).unwrap();
        fs::write(sidecar_path(&path), b"not an index").unwrap();

        let mut rebuilt = Index::default();
        rebuilt.refresh(&path).unwrap();

        assert_eq!(rebuilt.entries.len(), 25);
        assert!(rebuilt.entries.iter().all(|entry| entry.preview.is_some()));
        let persisted: PersistedIndex =
            serde_json::from_slice(&fs::read(sidecar_path(&path)).unwrap()).unwrap();
        assert_eq!(persisted.version, SIDECAR_VERSION);
    }

    #[test]
    fn sidecar_with_a_stale_journal_fingerprint_is_rebuilt() {
        let fixture = Fixture::new();
        let path = fixture.root.join("stale-fingerprint.jsonl");
        let replacement = fixture.root.join("replacement.jsonl");
        for candidate in [&path, &replacement] {
            fs::write(candidate, "{}\n").unwrap();
        }
        for index in 0..25 {
            journal::append(&path, &stored(index)).unwrap();
            let mut turn = stored(index);
            if index == 24 {
                turn.turn.steps[0].text = "Respostb 24".into();
            }
            journal::append(&replacement, &turn).unwrap();
        }
        Index::default().refresh(&path).unwrap();
        assert_eq!(
            fs::metadata(&path).unwrap().len(),
            fs::metadata(&replacement).unwrap().len()
        );
        fs::copy(&replacement, &path).unwrap();

        let mut rebuilt = Index::default();
        rebuilt.refresh(&path).unwrap();

        assert!(rebuilt.entries.iter().all(|entry| entry.preview.is_some()));
        assert_eq!(
            rebuilt
                .page(&path, "conversation", None, None, None)
                .unwrap()
                .turns
                .last()
                .unwrap()
                .steps[0]
                .text,
            "Respostb 24"
        );
    }

    #[test]
    fn independent_conversations_do_not_hold_the_global_history_cache_lock() {
        let fixture = Fixture::new();
        let first_path = fixture.root.join("first.jsonl");
        let second_path = fixture.root.join("second.jsonl");
        for path in [&first_path, &second_path] {
            fs::write(path, "{}\n").unwrap();
            journal::append(path, &stored(0)).unwrap();
        }
        let cache = HistoryState::default();
        cache.with(&first_path, |_| Ok(())).unwrap();
        cache.with(&second_path, |_| Ok(())).unwrap();

        let (started_tx, started_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let first_cache = cache.clone();
        let first = thread::spawn(move || {
            first_cache
                .with(&first_path, |_| {
                    started_tx.send(()).unwrap();
                    release_rx.recv().unwrap();
                    Ok(())
                })
                .unwrap();
        });
        started_rx.recv_timeout(Duration::from_secs(1)).unwrap();

        let (finished_tx, finished_rx) = mpsc::channel();
        let second_cache = cache.clone();
        let second = thread::spawn(move || {
            second_cache.with(&second_path, |_| Ok(())).unwrap();
            finished_tx.send(()).unwrap();
        });
        let independent = finished_rx.recv_timeout(Duration::from_secs(1)).is_ok();
        release_tx.send(()).unwrap();
        first.join().unwrap();
        second.join().unwrap();

        assert!(
            independent,
            "another conversation waited for the global cache"
        );
    }

    #[test]
    #[ignore = "performance characterization for large local journals"]
    fn benchmark_large_journal_sidecar_and_concurrent_history_reads() {
        let fixture = Fixture::new();
        let paths = [
            fixture.root.join("benchmark-a.jsonl"),
            fixture.root.join("benchmark-b.jsonl"),
        ];
        for path in &paths {
            fs::write(path, "{}\n").unwrap();
            for index in 0..600 {
                let mut turn = stored(index);
                turn.turn.steps[0].text.push_str(&"x".repeat(4096));
                journal::append(path, &turn).unwrap();
            }
        }

        let cold_started = Instant::now();
        for path in &paths {
            Index::default().refresh(path).unwrap();
        }
        let cold = cold_started.elapsed();
        let warm_started = Instant::now();
        for path in &paths {
            Index::default().refresh(path).unwrap();
        }
        let warm = warm_started.elapsed();

        let cache = HistoryState::default();
        let parallel_started = Instant::now();
        let workers: Vec<_> = paths
            .into_iter()
            .map(|path| {
                let cache = cache.clone();
                thread::spawn(move || {
                    cache
                        .with(&path, |index| {
                            index.page(&path, "benchmark", None, None, None)
                        })
                        .unwrap()
                })
            })
            .collect();
        for worker in workers {
            assert_eq!(worker.join().unwrap().turns.len(), PAGE_SIZE);
        }
        eprintln!(
            "large journal benchmark: cold={cold:?}, verified_sidecar={warm:?}, concurrent_pages={:?}",
            parallel_started.elapsed()
        );
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
        // A snapshot must not race a runtime session load and receive an
        // apparently newer revision for older disk state.
        let gate = agent.session_gate(&id).unwrap();
        let guard = gate.lock().unwrap();
        let reader = agent.clone();
        let reader_state = state.clone();
        let reader_home = fixture.root.clone();
        let reader_id = id.clone();
        let (started_tx, started_rx) = mpsc::channel();
        let (snapshot_tx, snapshot_rx) = mpsc::channel();
        let reader_thread = thread::spawn(move || {
            started_tx.send(()).unwrap();
            snapshot_tx
                .send(reader.read_chat(&reader_state, &reader_home, &reader_id))
                .unwrap();
        });
        started_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        assert!(snapshot_rx.recv_timeout(Duration::from_millis(50)).is_err());
        drop(guard);
        let chat = snapshot_rx
            .recv_timeout(Duration::from_secs(2))
            .unwrap()
            .unwrap();
        reader_thread.join().unwrap();
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
        runtime.writer =
            session_writer::SessionWriter::start(path.clone(), id.clone(), Some(stored(79)))
                .unwrap();
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
