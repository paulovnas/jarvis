use super::{
    compaction::{Checkpoint, CompactionEvent, CompletedCompaction},
    diffs::FileRevision,
    queue::QueuedMessage,
    AgentError, StoredTurn, TurnStatus,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    fs::{self, File, OpenOptions},
    io::{BufRead, BufReader, Read, Seek, SeekFrom, Write},
    path::Path,
};

const MAX_RECORD: usize = 10 * 1024 * 1024;
#[cfg(not(test))]
const VACUUM_MIN_BYTES: u64 = 16 * 1024 * 1024;
#[cfg(test)]
const VACUUM_MIN_BYTES: u64 = 64 * 1024;
const UNKNOWN_TOOL_OUTPUT: &str =
    "Execução interrompida; resultado desconhecido. Verifique o estado atual antes de repetir a operação.";
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Record {
    pub r#type: String,
    pub version: u8,
    pub data: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub(super) enum DeltaPathPart {
    Key(String),
    Index(usize),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub(super) enum DeltaOperation {
    Set {
        path: Vec<DeltaPathPart>,
        value: Value,
    },
    Remove {
        path: Vec<DeltaPathPart>,
    },
    Append {
        path: Vec<DeltaPathPart>,
        values: Vec<Value>,
    },
    Truncate {
        path: Vec<DeltaPathPart>,
        len: usize,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct TurnDelta {
    pub turn_id: String,
    pub operations: Vec<DeltaOperation>,
}

#[derive(Default)]
pub(super) struct Extras {
    pub queue: Vec<QueuedMessage>,
    pub context: Option<Checkpoint>,
    pub compactions: Vec<CompactionEvent>,
    pub files: std::collections::BTreeMap<String, FileRevision>,
}

fn open(path: &Path, write: bool) -> Result<File, AgentError> {
    recover_swap(path)?;
    let mut options = OpenOptions::new();
    options.read(true).write(write).append(write);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW);
    }
    let file = options.open(path).map_err(|_| AgentError::storage())?;
    let meta = file.metadata().map_err(|_| AgentError::storage())?;
    if !meta.is_file() {
        return Err(AgentError::storage());
    }
    Ok(file)
}

#[cfg(not(windows))]
fn recover_swap(_path: &Path) -> Result<(), AgentError> {
    Ok(())
}

#[cfg(windows)]
fn recover_swap(path: &Path) -> Result<(), AgentError> {
    let backup = path.with_extension("vacuum-backup");
    match (path.exists(), backup.exists()) {
        (false, true) => fs::rename(&backup, path).map_err(|_| AgentError::storage()),
        (true, true) => fs::remove_file(backup).map_err(|_| AgentError::storage()),
        _ => Ok(()),
    }
}

pub(super) fn append(path: &Path, turn: &StoredTurn) -> Result<(), AgentError> {
    append_event(path, "turn_checkpoint", turn)
}

pub(super) fn append_update(
    path: &Path,
    before: &StoredTurn,
    after: &StoredTurn,
) -> Result<(), AgentError> {
    let Some((kind, value)) = update_event(before, after)? else {
        return Ok(());
    };
    append_event(path, kind, &value)
}

fn update_event(
    before: &StoredTurn,
    after: &StoredTurn,
) -> Result<Option<(&'static str, Value)>, AgentError> {
    if before.turn.id != after.turn.id {
        return Err(AgentError::storage());
    }
    // A final full checkpoint keeps completed turns directly addressable by
    // the paged history index. Only the growing, in-flight state uses deltas.
    if after.turn.status != TurnStatus::Running {
        return serde_json::to_value(after)
            .map(|value| Some(("turn_checkpoint", value)))
            .map_err(|_| AgentError::storage());
    }
    let before_value = serde_json::to_value(before).map_err(|_| AgentError::storage())?;
    let after_value = serde_json::to_value(after).map_err(|_| AgentError::storage())?;
    let mut operations = Vec::new();
    diff_value(
        &before_value,
        &after_value,
        &mut Vec::new(),
        &mut operations,
    );
    if operations.is_empty() {
        return Ok(None);
    }
    let delta = TurnDelta {
        turn_id: after.turn.id.clone(),
        operations,
    };
    let delta_size = serde_json::to_vec(&delta)
        .map_err(|_| AgentError::storage())?
        .len();
    let full_size = serde_json::to_vec(after)
        .map_err(|_| AgentError::storage())?
        .len();
    if delta_size >= full_size {
        Ok(Some(("turn_checkpoint", after_value)))
    } else {
        serde_json::to_value(delta)
            .map(|value| Some(("turn_delta", value)))
            .map_err(|_| AgentError::storage())
    }
}

fn diff_value(
    before: &Value,
    after: &Value,
    path: &mut Vec<DeltaPathPart>,
    operations: &mut Vec<DeltaOperation>,
) {
    if before == after {
        return;
    }
    match (before, after) {
        (Value::Object(before), Value::Object(after)) => {
            for key in before.keys().filter(|key| !after.contains_key(*key)) {
                path.push(DeltaPathPart::Key(key.clone()));
                operations.push(DeltaOperation::Remove { path: path.clone() });
                path.pop();
            }
            for (key, value) in after {
                path.push(DeltaPathPart::Key(key.clone()));
                if let Some(previous) = before.get(key) {
                    diff_value(previous, value, path, operations);
                } else {
                    operations.push(DeltaOperation::Set {
                        path: path.clone(),
                        value: value.clone(),
                    });
                }
                path.pop();
            }
        }
        (Value::Array(before), Value::Array(after)) => {
            for index in 0..before.len().min(after.len()) {
                path.push(DeltaPathPart::Index(index));
                diff_value(&before[index], &after[index], path, operations);
                path.pop();
            }
            if after.len() > before.len() {
                operations.push(DeltaOperation::Append {
                    path: path.clone(),
                    values: after[before.len()..].to_vec(),
                });
            } else if after.len() < before.len() {
                operations.push(DeltaOperation::Truncate {
                    path: path.clone(),
                    len: after.len(),
                });
            }
        }
        _ => operations.push(DeltaOperation::Set {
            path: path.clone(),
            value: after.clone(),
        }),
    }
}

pub(super) fn apply_delta(turn: &mut StoredTurn, delta: TurnDelta) -> Result<(), AgentError> {
    if turn.turn.id != delta.turn_id {
        return Err(AgentError::storage());
    }
    let mut value = serde_json::to_value(&*turn).map_err(|_| AgentError::storage())?;
    for operation in delta.operations {
        apply_operation(&mut value, operation)?;
    }
    *turn = serde_json::from_value(value).map_err(|_| AgentError::storage())?;
    Ok(())
}

fn apply_operation(root: &mut Value, operation: DeltaOperation) -> Result<(), AgentError> {
    match operation {
        DeltaOperation::Set { path, value } => {
            if path.is_empty() {
                *root = value;
                return Ok(());
            }
            let (parent, last) = parent_mut(root, &path)?;
            match (parent, last) {
                (Value::Object(object), DeltaPathPart::Key(key)) => {
                    object.insert(key.clone(), value);
                }
                (Value::Array(array), DeltaPathPart::Index(index)) if *index < array.len() => {
                    array[*index] = value;
                }
                _ => return Err(AgentError::storage()),
            }
        }
        DeltaOperation::Remove { path } => {
            let (parent, last) = parent_mut(root, &path)?;
            match (parent, last) {
                (Value::Object(object), DeltaPathPart::Key(key)) => {
                    if object.remove(key).is_none() {
                        return Err(AgentError::storage());
                    }
                }
                _ => return Err(AgentError::storage()),
            }
        }
        DeltaOperation::Append { path, values } => {
            let Value::Array(array) = value_mut(root, &path)? else {
                return Err(AgentError::storage());
            };
            array.extend(values);
        }
        DeltaOperation::Truncate { path, len } => {
            let Value::Array(array) = value_mut(root, &path)? else {
                return Err(AgentError::storage());
            };
            if len > array.len() {
                return Err(AgentError::storage());
            }
            array.truncate(len);
        }
    }
    Ok(())
}

fn parent_mut<'a>(
    root: &'a mut Value,
    path: &'a [DeltaPathPart],
) -> Result<(&'a mut Value, &'a DeltaPathPart), AgentError> {
    let (last, parents) = path.split_last().ok_or_else(AgentError::storage)?;
    Ok((value_mut(root, parents)?, last))
}

fn value_mut<'a>(
    mut value: &'a mut Value,
    path: &[DeltaPathPart],
) -> Result<&'a mut Value, AgentError> {
    for part in path {
        value = match (value, part) {
            (Value::Object(object), DeltaPathPart::Key(key)) => {
                object.get_mut(key).ok_or_else(AgentError::storage)?
            }
            (Value::Array(array), DeltaPathPart::Index(index)) => {
                array.get_mut(*index).ok_or_else(AgentError::storage)?
            }
            _ => return Err(AgentError::storage()),
        };
    }
    Ok(value)
}

pub(super) fn append_event(
    path: &Path,
    kind: &str,
    value: &impl Serialize,
) -> Result<(), AgentError> {
    let bytes = event_bytes(kind, value)?;
    let mut file = open(path, true)?;
    file.write_all(&bytes)
        .and_then(|()| file.sync_all())
        .map_err(|_| AgentError::storage())
}

fn event_bytes(kind: &str, value: &impl Serialize) -> Result<Vec<u8>, AgentError> {
    let mut bytes = serde_json::to_vec(&Record {
        r#type: kind.into(),
        version: 1,
        data: serde_json::to_value(value).map_err(|_| AgentError::storage())?,
    })
    .map_err(|_| AgentError::storage())?;
    bytes.push(b'\n');
    if bytes.len() > MAX_RECORD {
        return Err(AgentError::new(
            "history_limit",
            "Esta interação atingiu o limite de histórico local.",
        ));
    }
    Ok(bytes)
}

// Scan one bounded record at a time. The journal itself can grow beyond memory.
pub(super) fn scan(
    path: &Path,
    start: u64,
    mut visit: impl FnMut(u64, usize, Record) -> Result<(), AgentError>,
) -> Result<u64, AgentError> {
    let mut file = open(path, false)?;
    file.seek(SeekFrom::Start(start))
        .map_err(|_| AgentError::storage())?;
    let mut reader = BufReader::new(file);
    let mut offset = start;
    loop {
        let mut line = Vec::new();
        (&mut reader)
            .take(MAX_RECORD as u64 + 1)
            .read_until(b'\n', &mut line)
            .map_err(|_| AgentError::storage())?;
        if line.len() > MAX_RECORD {
            return Err(AgentError::storage());
        }
        if !line.ends_with(b"\n") {
            break;
        }
        if offset > 0 {
            let record: Record = serde_json::from_slice(&line).map_err(|_| {
                AgentError::new(
                    "invalid_history",
                    "O histórico contém um registro inválido. O arquivo original foi preservado.",
                )
            })?;
            if record.version != 1 {
                return Err(AgentError::storage());
            }
            visit(offset, line.len(), record)?;
        }
        offset += line.len() as u64;
    }
    Ok(offset)
}

pub(super) fn record_at(path: &Path, offset: u64, length: usize) -> Result<Record, AgentError> {
    if length > MAX_RECORD {
        return Err(AgentError::storage());
    }
    let mut file = open(path, false)?;
    file.seek(SeekFrom::Start(offset))
        .map_err(|_| AgentError::storage())?;
    let mut bytes = vec![0; length];
    file.read_exact(&mut bytes)
        .map_err(|_| AgentError::storage())?;
    serde_json::from_slice(&bytes).map_err(|_| AgentError::storage())
}

pub(super) fn load_all(path: &Path) -> Result<(Vec<StoredTurn>, Extras), AgentError> {
    read(path, true, true)
}

pub(super) fn read_only(path: &Path) -> Result<(Vec<StoredTurn>, Extras), AgentError> {
    read(path, false, false)
}

// Root conversations can inspect a repaired journal before deciding whether a
// direct turn has enough durable state to continue. Worker journals and all
// ordinary history readers keep the conservative interrupted repair below.
pub(super) fn load_for_recovery(path: &Path) -> Result<(Vec<StoredTurn>, Extras), AgentError> {
    read(path, true, false)
}

fn read(
    path: &Path,
    repair: bool,
    interrupt_running: bool,
) -> Result<(Vec<StoredTurn>, Extras), AgentError> {
    let mut turns: Vec<StoredTurn> = vec![];
    let mut extras = Extras::default();
    let mut ids = std::collections::HashSet::new();
    let mut turn_checkpoints = 0usize;
    let mut damaged_turn: Option<String> = None;
    let valid_end = scan(path, 0, |_, _, record| {
        match record.r#type.as_str() {
            "compaction_completed" => {
                let completed: CompletedCompaction =
                    serde_json::from_value(record.data).map_err(|_| AgentError::storage())?;
                extras.context = Some(completed.context);
                extras.compactions.push(completed.event);
                return Ok(());
            }
            "queue_checkpoint" => {
                extras.queue =
                    serde_json::from_value(record.data).map_err(|_| AgentError::storage())?;
                return Ok(());
            }
            "context_checkpoint" => {
                extras.context =
                    Some(serde_json::from_value(record.data).map_err(|_| AgentError::storage())?);
                return Ok(());
            }
            "file_checkpoint" => {
                let file: FileRevision =
                    serde_json::from_value(record.data).map_err(|_| AgentError::storage())?;
                extras.files.insert(file.path.clone(), file);
                return Ok(());
            }
            "turn_delta" => {
                let current_id = turns
                    .last()
                    .map(|turn| turn.turn.id.clone())
                    .ok_or_else(AgentError::storage)?;
                let delta = serde_json::from_value::<TurnDelta>(record.data);
                if let Some(damaged) = &damaged_turn {
                    if delta.as_ref().is_ok_and(|delta| delta.turn_id != *damaged) {
                        return Err(AgentError::storage());
                    }
                    return Ok(());
                }
                let Ok(delta) = delta else {
                    damaged_turn = Some(current_id);
                    return Ok(());
                };
                let mut candidate = turns.last().cloned().ok_or_else(AgentError::storage)?;
                if apply_delta(&mut candidate, delta).is_err() {
                    damaged_turn = Some(current_id);
                    return Ok(());
                }
                *turns.last_mut().ok_or_else(AgentError::storage)? = candidate;
                return Ok(());
            }
            "turn_checkpoint" => turn_checkpoints += 1,
            _ => return Err(AgentError::storage()),
        }
        let turn: StoredTurn =
            serde_json::from_value(record.data).map_err(|_| AgentError::storage())?;
        if let Some(damaged) = damaged_turn.take() {
            if turn.turn.id != damaged
                || !turns
                    .last()
                    .is_some_and(|current| current.turn.id == damaged)
            {
                return Err(AgentError::storage());
            }
            *turns.last_mut().ok_or_else(AgentError::storage)? = turn;
            return Ok(());
        }
        if turns
            .last()
            .is_some_and(|last| last.turn.id == turn.turn.id)
        {
            *turns.last_mut().unwrap() = turn;
        } else {
            if !ids.insert(turn.turn.id.clone()) {
                return Err(AgentError::storage());
            }
            turns.push(turn);
        }
        Ok(())
    })?;
    if damaged_turn.is_some() {
        return Err(AgentError::storage());
    }
    if repair
        && valid_end
            < open(path, false)?
                .metadata()
                .map_err(|_| AgentError::storage())?
                .len()
    {
        // Preserve crash debris before repairing only an incomplete final line.
        let backup = path.with_extension(format!("recovery-{}.jsonl", crate::library::new_id()?));
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut copy = options.open(&backup).map_err(|_| AgentError::storage())?;
        std::io::copy(&mut open(path, false)?, &mut copy)
            .and_then(|_| copy.sync_all())
            .map_err(|_| AgentError::storage())?;
        #[cfg(unix)]
        File::open(path.parent().ok_or_else(AgentError::storage)?)
            .and_then(|directory| directory.sync_all())
            .map_err(|_| AgentError::storage())?;
        // Truncation needs a write handle that is NOT in append mode: on Windows
        // SetEndOfFile is denied (ACCESS_DENIED) on an append-only handle.
        let mut options = OpenOptions::new();
        options.read(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.custom_flags(libc::O_NOFOLLOW);
        }
        let file = options.open(path).map_err(|_| AgentError::storage())?;
        file.set_len(valid_end)
            .and_then(|()| file.sync_all())
            .map_err(|_| AgentError::storage())?;
    }
    for turn in &mut turns {
        if repair && interrupt_running && turn.turn.status == TurnStatus::Running {
            mark_interrupted(turn);
            append(path, turn)?;
        }
    }
    // A queued message and its turn share an ID. A crash between the durable
    // turn reservation and the queue update must never replay the same message.
    extras
        .queue
        .retain(|message| !turns.iter().any(|turn| turn.turn.id == message.id));
    if let Some(context) = &extras.context {
        context.validate(&turns)?;
    }
    if repair && valid_end >= VACUUM_MIN_BYTES && turn_checkpoints > turns.len().saturating_mul(3) {
        // Vacuum is best effort: the validated original remains authoritative
        // if the replacement cannot be completed on this filesystem.
        let _ = vacuum(path, &turns, &extras);
    }
    Ok((turns, extras))
}

fn vacuum(path: &Path, turns: &[StoredTurn], extras: &Extras) -> Result<(), AgentError> {
    let metadata = fs::symlink_metadata(path).map_err(|_| AgentError::storage())?;
    if !metadata.is_file() || metadata.is_symlink() {
        return Err(AgentError::storage());
    }
    let mut source = BufReader::new(open(path, false)?);
    let mut header = Vec::new();
    source
        .read_until(b'\n', &mut header)
        .map_err(|_| AgentError::storage())?;
    if header.is_empty() || !header.ends_with(b"\n") || header.len() > MAX_RECORD {
        return Err(AgentError::storage());
    }
    let temp = path.with_extension(format!("vacuum-{}.tmp", crate::library::new_id()?));
    let result = (|| {
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut replacement = options.open(&temp).map_err(|_| AgentError::storage())?;
        replacement
            .write_all(&header)
            .map_err(|_| AgentError::storage())?;
        for turn in turns {
            replacement
                .write_all(&event_bytes("turn_checkpoint", turn)?)
                .map_err(|_| AgentError::storage())?;
        }
        for event in &extras.compactions {
            if let Some(context) = &extras.context {
                replacement
                    .write_all(&event_bytes(
                        "compaction_completed",
                        &CompletedCompaction {
                            context: context.clone(),
                            event: event.clone(),
                        },
                    )?)
                    .map_err(|_| AgentError::storage())?;
            }
        }
        replacement
            .write_all(&event_bytes("queue_checkpoint", &extras.queue)?)
            .map_err(|_| AgentError::storage())?;
        if let Some(context) = &extras.context {
            replacement
                .write_all(&event_bytes("context_checkpoint", context)?)
                .map_err(|_| AgentError::storage())?;
        }
        for revision in extras.files.values() {
            replacement
                .write_all(&event_bytes("file_checkpoint", revision)?)
                .map_err(|_| AgentError::storage())?;
        }
        replacement.sync_all().map_err(|_| AgentError::storage())?;
        fs::set_permissions(&temp, metadata.permissions()).map_err(|_| AgentError::storage())?;
        let (verified_turns, verified_extras) = read_only(&temp)?;
        if serde_json::to_value(&verified_turns).map_err(|_| AgentError::storage())?
            != serde_json::to_value(turns).map_err(|_| AgentError::storage())?
            || serde_json::to_value(&verified_extras.queue).map_err(|_| AgentError::storage())?
                != serde_json::to_value(&extras.queue).map_err(|_| AgentError::storage())?
            || serde_json::to_value(&verified_extras.context).map_err(|_| AgentError::storage())?
                != serde_json::to_value(&extras.context).map_err(|_| AgentError::storage())?
            || serde_json::to_value(&verified_extras.compactions)
                .map_err(|_| AgentError::storage())?
                != serde_json::to_value(&extras.compactions).map_err(|_| AgentError::storage())?
            || serde_json::to_value(&verified_extras.files).map_err(|_| AgentError::storage())?
                != serde_json::to_value(&extras.files).map_err(|_| AgentError::storage())?
        {
            return Err(AgentError::storage());
        }
        replace_file(path, &temp)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

#[cfg(not(windows))]
fn replace_file(path: &Path, replacement: &Path) -> Result<(), AgentError> {
    fs::rename(replacement, path).map_err(|_| AgentError::storage())?;
    File::open(path.parent().ok_or_else(AgentError::storage)?)
        .and_then(|directory| directory.sync_all())
        .map_err(|_| AgentError::storage())
}

#[cfg(windows)]
fn replace_file(path: &Path, replacement: &Path) -> Result<(), AgentError> {
    let backup = path.with_extension("vacuum-backup");
    if backup.exists() {
        fs::remove_file(&backup).map_err(|_| AgentError::storage())?;
    }
    fs::rename(path, &backup).map_err(|_| AgentError::storage())?;
    if fs::rename(replacement, path).is_err() {
        let _ = fs::rename(&backup, path);
        return Err(AgentError::storage());
    }
    let _ = fs::remove_file(backup);
    Ok(())
}

#[cfg(test)]
fn load(path: &Path) -> Result<Vec<StoredTurn>, AgentError> {
    load_all(path).map(|(turns, _)| turns)
}

pub(super) fn interrupt_tools(turn: &mut StoredTurn) {
    for step in &mut turn.turn.steps {
        step.retry = None;
        for tool in &mut step.tools {
            if !turn
                .wire
                .iter()
                .any(|item| item["type"] == "function_call_output" && item["call_id"] == tool.id)
            {
                tool.status = "error".into();
                tool.output = UNKNOWN_TOOL_OUTPUT.into();
                if tool.name == "ask_user" {
                    tool.output = super::questions::cancelled_output();
                } else if matches!(
                    tool.name.as_str(),
                    "jarvis_propose_agent" | "jarvis_propose_flow"
                ) {
                    tool.output = super::authoring::cancelled_output();
                }
                turn.wire.push(
                    json!({"type":"function_call_output", "call_id":tool.id, "output":tool.output}),
                );
            }
        }
    }
}

pub(super) fn all_tool_results_durable(turn: &StoredTurn) -> bool {
    let mut calls = std::collections::HashSet::new();
    let mut outputs = std::collections::HashSet::new();
    for item in &turn.wire {
        match item["type"].as_str() {
            Some("function_call") => {
                let Some(id) = item["call_id"].as_str() else {
                    return false;
                };
                calls.insert(id);
            }
            Some("function_call_output") => {
                let Some(id) = item["call_id"].as_str() else {
                    return false;
                };
                outputs.insert(id);
            }
            _ => {}
        }
    }
    calls.iter().all(|id| outputs.contains(id))
        && turn
            .turn
            .steps
            .iter()
            .flat_map(|step| &step.tools)
            .all(|tool| outputs.contains(tool.id.as_str()))
}

pub(super) fn safe_to_resume(turn: &StoredTurn) -> bool {
    all_tool_results_durable(turn)
        && !turn
            .turn
            .steps
            .iter()
            .flat_map(|step| &step.tools)
            .any(|tool| {
                tool.output == UNKNOWN_TOOL_OUTPUT
                    || (tool.name == "ask_user"
                        && tool.output == super::questions::cancelled_output())
                    || (matches!(
                        tool.name.as_str(),
                        "jarvis_propose_agent" | "jarvis_propose_flow"
                    ) && tool.output == super::authoring::cancelled_output())
            })
        && !turn.wire.iter().any(|item| {
            item["type"].as_str() == Some("function_call_output")
                && item["output"].as_str() == Some(UNKNOWN_TOOL_OUTPUT)
        })
}

pub(super) fn mark_interrupted(turn: &mut StoredTurn) {
    interrupt_tools(turn);
    turn.turn.status = TurnStatus::Interrupted;
    turn.turn.error = Some(AgentError::new(
        "interrupted",
        "O Jarvis foi encerrado durante esta execução. Revise os arquivos antes de continuar; ferramentas não foram repetidas.",
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{tests::Fixture, ApprovalMode, Mode, Step, ToolCall, Turn, TurnOptions};
    use std::fs;
    fn turn() -> StoredTurn {
        StoredTurn {
            turn: Turn {
                id: "turn-1".into(),
                created_at: 1,
                duration_ms: 0,
                user: "Read".into(),
                parts: vec![],
                context_window: Some(128_000),
                options: TurnOptions {
                    account: "test".into(),
                    model: "model".into(),
                    reasoning: None,
                    mode: Mode::Build,
                    workflow: None,
                    custom_workflow_id: None,
                    custom_agent_id: None,
                    approval_mode: ApprovalMode::Manual,
                },
                status: TurnStatus::Running,
                tasks: vec![],
                steps: vec![],
                error: None,
            },
            wire: vec![json!({"role":"user", "content":"Read"})],
        }
    }
    #[test]
    fn restart_preserves_messages_and_repairs_missing_tool_result_without_execution() {
        let fixture = Fixture::new();
        let path = fixture.root.join("session.jsonl");
        fs::write(&path, "{\"type\":\"session\"}\n").unwrap();
        let mut item = turn();
        item.turn.steps.push(Step {
            text: "Checking".into(),
            tools: vec![ToolCall {
                id: "call-1".into(),
                name: "write".into(),
                args: json!({}),
                status: "running".into(),
                output: String::new(),
                duration_ms: 0,
            }],
            ..Step::default()
        });
        item.wire.push(
            json!({"type":"function_call", "call_id":"call-1", "name":"write", "arguments":"{}"}),
        );
        append(&path, &item).unwrap();
        let loaded = load(&path).unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].turn.status, TurnStatus::Interrupted);
        assert_eq!(loaded[0].turn.steps[0].text, "Checking");
        assert_eq!(loaded[0].turn.context_window, Some(128_000));
        assert_eq!(loaded[0].wire[2]["call_id"], "call-1");
        let twice = load(&path).unwrap();
        assert_eq!(twice[0].wire.len(), 3);
    }

    #[test]
    fn recovery_loader_keeps_a_running_turn_for_the_runtime_to_assess() {
        let fixture = Fixture::new();
        let path = fixture.root.join("session.jsonl");
        fs::write(&path, "{\"type\":\"session\"}\n").unwrap();
        let mut item = turn();
        item.turn.steps.push(Step {
            tools: vec![ToolCall {
                id: "call-1".into(),
                name: "read".into(),
                args: json!({}),
                status: "completed".into(),
                output: "README".into(),
                duration_ms: 1,
            }],
            ..Step::default()
        });
        item.wire.extend([
            json!({"type":"function_call", "call_id":"call-1", "name":"read", "arguments":"{}"}),
            json!({"type":"function_call_output", "call_id":"call-1", "output":"README"}),
        ]);
        append(&path, &item).unwrap();

        let (recovery, _) = load_for_recovery(&path).unwrap();
        assert_eq!(recovery[0].turn.status, TurnStatus::Running);
        assert!(all_tool_results_durable(&recovery[0]));

        assert_eq!(load(&path).unwrap()[0].turn.status, TurnStatus::Interrupted);
    }
    #[test]
    fn legacy_turn_without_context_window_remains_readable() {
        let mut value = serde_json::to_value(turn()).unwrap();
        let turn = value["turn"].as_object_mut().unwrap();
        turn.remove("contextWindow");
        turn.remove("tasks");
        let stored: StoredTurn = serde_json::from_value(value).unwrap();
        assert_eq!(stored.turn.context_window, None);
        assert!(stored.turn.tasks.is_empty());
    }

    #[test]
    fn incremental_turn_events_grow_with_new_work_instead_of_prior_snapshots() {
        let mut current = turn();
        let initial = serde_json::to_vec(&current).unwrap().len();
        let mut journal_bytes = initial;
        for index in 0..500 {
            let previous = current.clone();
            let call_id = format!("call-{index}");
            current.turn.steps.push(Step {
                text: format!("Step {index}"),
                tools: vec![ToolCall {
                    id: call_id.clone(),
                    name: "read".into(),
                    args: json!({"path":format!("src/{index}.ts")}),
                    status: "completed".into(),
                    output: "x".repeat(256),
                    duration_ms: 1,
                }],
                ..Step::default()
            });
            current.wire.extend([
                json!({"type":"function_call","call_id":call_id,"name":"read","arguments":"{}"}),
                json!({"type":"function_call_output","call_id":call_id,"output":"x".repeat(256)}),
            ]);
            let (kind, value) = update_event(&previous, &current).unwrap().unwrap();
            assert_eq!(kind, "turn_delta");
            journal_bytes += serde_json::to_vec(&Record {
                r#type: kind.into(),
                version: 1,
                data: value,
            })
            .unwrap()
            .len();
        }
        let final_size = serde_json::to_vec(&current).unwrap().len();
        assert!(journal_bytes < final_size * 2);
    }

    #[test]
    fn recovery_replays_every_incremental_event_boundary_and_final_checkpoint() {
        let fixture = Fixture::new();
        let path = fixture.root.join("incremental.jsonl");
        fs::write(&path, "{\"type\":\"session\"}\n").unwrap();
        let mut current = turn();
        append(&path, &current).unwrap();
        for index in 0..5 {
            let previous = current.clone();
            current.turn.steps.push(Step {
                text: format!("Durable {index}"),
                ..Step::default()
            });
            current
                .wire
                .push(json!({"role":"assistant","content":format!("Durable {index}")}));
            append_update(&path, &previous, &current).unwrap();
            let (recovered, _) = load_for_recovery(&path).unwrap();
            assert_eq!(
                serde_json::to_value(&recovered[0]).unwrap(),
                serde_json::to_value(&current).unwrap()
            );
        }
        let previous = current.clone();
        current.turn.status = TurnStatus::Completed;
        append_update(&path, &previous, &current).unwrap();
        let (loaded, _) = read_only(&path).unwrap();
        assert_eq!(
            serde_json::to_value(&loaded[0]).unwrap(),
            serde_json::to_value(&current).unwrap()
        );
    }

    #[test]
    fn later_checkpoint_recovers_a_turn_from_a_superseded_invalid_delta() {
        let fixture = Fixture::new();
        let path = fixture.root.join("superseded-delta.jsonl");
        fs::write(&path, "{\"type\":\"session\"}\n").unwrap();
        let initial = turn();
        append(&path, &initial).unwrap();
        append_event(
            &path,
            "turn_delta",
            &TurnDelta {
                turn_id: initial.turn.id.clone(),
                operations: vec![DeltaOperation::Set {
                    path: vec![
                        DeltaPathPart::Key("turn".into()),
                        DeltaPathPart::Key("steps".into()),
                        DeltaPathPart::Index(0),
                        DeltaPathPart::Key("durationMs".into()),
                    ],
                    value: json!(42),
                }],
            },
        )
        .unwrap();
        assert!(read_only(&path).is_err());

        let mut completed = initial;
        completed.turn.steps.push(Step {
            text: "Resposta preservada".into(),
            duration_ms: 42,
            ..Step::default()
        });
        completed.turn.status = TurnStatus::Completed;
        append(&path, &completed).unwrap();

        let (loaded, _) = read_only(&path).unwrap();
        assert_eq!(loaded[0].turn.status, TurnStatus::Completed);
        assert_eq!(loaded[0].turn.steps[0].text, "Resposta preservada");
        assert_eq!(loaded[0].turn.steps[0].duration_ms, 42);
    }

    #[test]
    fn legacy_cumulative_journal_is_vacuumed_after_validation() {
        let fixture = Fixture::new();
        let path = fixture.root.join("legacy-large.jsonl");
        let header = "{\"type\":\"session\",\"title\":\"Preservado\"}\n";
        fs::write(&path, header).unwrap();
        let mut current = turn();
        current.turn.status = TurnStatus::Completed;
        current.turn.steps.push(Step {
            text: "x".repeat(8 * 1024),
            ..Step::default()
        });
        for index in 0..12 {
            current.turn.duration_ms = index;
            append(&path, &current).unwrap();
        }
        let cumulative_size = fs::metadata(&path).unwrap().len();
        let (loaded, _) = load_all(&path).unwrap();
        let compacted_size = fs::metadata(&path).unwrap().len();
        assert!(compacted_size * 3 < cumulative_size);
        assert_eq!(
            fs::read_to_string(&path).unwrap().lines().next(),
            Some(header.trim())
        );
        assert_eq!(loaded[0].turn.duration_ms, 11);
        assert_eq!(read_only(&path).unwrap().0.len(), 1);
    }

    #[test]
    fn incomplete_tail_is_backed_up_but_corrupt_complete_record_is_not_rewritten() {
        let fixture = Fixture::new();
        let path = fixture.root.join("session.jsonl");
        fs::write(&path, "{}\n").unwrap();
        let mut item = turn();
        item.turn.status = TurnStatus::Completed;
        append(&path, &item).unwrap();
        OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap()
            .write_all(b"{broken")
            .unwrap();
        let original = fs::read(&path).unwrap();
        assert_eq!(load(&path).unwrap().len(), 1);
        let backup = fs::read_dir(&fixture.root)
            .unwrap()
            .map(|item| item.unwrap().path())
            .find(|item| item.to_string_lossy().contains("recovery-"))
            .unwrap();
        assert_eq!(fs::read(backup).unwrap(), original);
        OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap()
            .write_all(b"{broken}\n")
            .unwrap();
        let corrupt = fs::read(&path).unwrap();
        assert!(load(&path).is_err());
        assert_eq!(fs::read(&path).unwrap(), corrupt);
    }
}
