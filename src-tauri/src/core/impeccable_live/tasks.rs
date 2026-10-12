//! Durable host-owned Live requests; upstream journals remain read-only.
use super::{error, CoreError};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

const MAX_JOURNAL: u64 = 2 * 1024 * 1024;
const MAX_ENTRY: usize = 256 * 1024;
const MAX_SCAN: u64 = 32 * 1024 * 1024;
const MAX_FILES: usize = 1024;
const MAX_QUEUE: u64 = 8 * 1024 * 1024;
const MAX_TASKS: usize = 1024;
const MAX_CONTENT: usize = 12_000;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) enum Status {
    Pending,
    Paused,
    Running,
    Completed,
    Cancelled,
    Blocked,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Task {
    pub request_id: String,
    pub action_id: String,
    pub revision: u64,
    pub content: String,
    #[serde(skip)]
    pub event: Value,
    pub status: Status,
    pub bead_id: Option<String>,
    pub turn_id: Option<String>,
    pub created_at: u64,
}

/// Keep this cutoff fixed for one activation. Only an exact owned receipt may
/// restore a request from an earlier activation automatically.
#[derive(Clone, Copy, Debug)]
pub(super) struct ScanPolicy<'a> {
    pub activation_cutoff_ms: u64,
    pub recover_action_id: Option<&'a str>,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct StoredTask {
    #[serde(flatten)]
    task: Task,
    private_event: Value,
    overlay: Option<String>,
    journal_sequence: u64,
    #[serde(default)]
    explicit_resume: bool,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct Queue {
    version: u32,
    project_root: PathBuf,
    activation_cutoff_ms: u64,
    tasks: Vec<StoredTask>,
}

struct Request {
    sequence: u64,
    created_at: u64,
    event: Value,
}

struct Canonical {
    action_id: String,
    request: Request,
    pending: Option<Value>,
    pending_at: u64,
    final_status: Option<Status>,
    journal_sequence: u64,
    choice_sequence: Option<u64>,
    phase_sequence: u64,
}

pub(super) fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| {
            u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
        })
}

fn invalid() -> CoreError {
    error("O histórico Live não pôde ser lido com segurança. Os pedidos foram preservados; confira o diagnóstico antes de retomar.")
}

fn conflict() -> CoreError {
    error("Este pedido Live mudou. Atualize a lista antes de editar ou remover.")
}

fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

fn checked_path(root: &Path, path: &Path) -> Result<(), CoreError> {
    let relative = path.strip_prefix(root).map_err(|_| invalid())?;
    let mut current = root.to_path_buf();
    for component in relative.components() {
        let Component::Normal(part) = component else {
            return Err(invalid());
        };
        current.push(part);
        match fs::symlink_metadata(&current) {
            Ok(metadata) => {
                if metadata.is_symlink() || !current.canonicalize()?.starts_with(root) {
                    return Err(invalid());
                }
            }
            Err(cause) if cause.kind() == std::io::ErrorKind::NotFound => break,
            Err(cause) => return Err(cause.into()),
        }
    }
    Ok(())
}

fn regular(root: &Path, path: &Path, limit: u64) -> Result<Option<Vec<u8>>, CoreError> {
    checked_path(root, path)?;
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(cause) if cause.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(cause) => return Err(cause.into()),
    };
    if !metadata.is_file() || metadata.is_symlink() || metadata.len() > limit {
        return Err(invalid());
    }
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW);
    }
    let file = options.open(path)?;
    if !file.metadata()?.is_file() || file.metadata()?.len() > limit {
        return Err(invalid());
    }
    let mut bytes = Vec::new();
    file.take(limit + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err(invalid());
    }
    Ok(Some(bytes))
}

fn validate_source(root: &Path, event: &Value) -> Result<(), CoreError> {
    for pointer in [
        "/file",
        "/sourceFile",
        "/previewFile",
        "/componentDir",
        "/sourceLocation/file",
        "/sourceLocation/path",
        "/scaffold/file",
        "/scaffold/sourceFile",
        "/scaffold/previewFile",
    ] {
        let Some(value) = event.pointer(pointer).filter(|value| !value.is_null()) else {
            continue;
        };
        let value = value.as_str().ok_or_else(invalid)?;
        if value.is_empty() {
            continue;
        }
        if value.contains('\0') || value.starts_with('~') || value.len() > 4096 {
            return Err(invalid());
        }
        let path: PathBuf = if value.starts_with("file:") {
            url::Url::parse(value)
                .ok()
                .and_then(|url| url.to_file_path().ok())
                .ok_or_else(invalid)?
        } else {
            value.into()
        };
        if path.components().any(|part| part == Component::ParentDir) {
            return Err(invalid());
        }
        let path = if path.is_absolute() {
            path
        } else {
            root.join(path)
        };
        checked_path(root, &path)?;
    }
    Ok(())
}

fn stamp(value: &Value) -> Option<u64> {
    value.as_u64().or_else(|| {
        value.as_str().and_then(|value| {
            time::OffsetDateTime::parse(value, &time::format_description::well_known::Rfc3339)
                .ok()
                .and_then(|time| u64::try_from(time.unix_timestamp_nanos() / 1_000_000).ok())
        })
    })
}

fn submitted_at(record: &Value, event: &Value) -> u64 {
    match (stamp(&record["ts"]), stamp(&event["clientSentAt"])) {
        (Some(journal), Some(client)) => journal.min(client),
        (Some(time), None) | (None, Some(time)) => time,
        (None, None) => 0,
    }
}

fn human(event: &Value) -> Result<String, CoreError> {
    for key in ["freeformPrompt", "message", "prompt"] {
        if let Some(value) = event.get(key).filter(|value| !value.is_null()) {
            let value = value.as_str().ok_or_else(invalid)?;
            if value.chars().count() > MAX_CONTENT || value.contains('\0') {
                return Err(invalid());
            }
            if !value.trim().is_empty() {
                return Ok(value.to_owned());
            }
        }
    }
    Ok(match event["type"].as_str() {
        Some("generate") => "Aplicar o ajuste de design ao elemento selecionado",
        Some("steer") => "Ajustar a página selecionada",
        Some("accept") => "Aplicar a variante selecionada",
        Some("discard") => "Descartar as variantes desta alteração",
        Some("manual_edit_apply") => "Aplicar a edição visual",
        Some("carbonize_cleanup") => "Concluir a alteração visual aprovada",
        Some("variant_mount_failed") => "Recuperar a visualização da variante",
        _ => return Err(invalid()),
    }
    .to_owned())
}

fn same_submission(left: &Value, right: &Value) -> bool {
    [
        "type",
        "id",
        "action",
        "freeformPrompt",
        "prompt",
        "message",
        "clientSentAt",
        "pageUrl",
        "element",
        "count",
    ]
    .iter()
    .all(|key| left.get(key) == right.get(key))
}

fn redact_tokens(value: &mut Value) {
    match value {
        Value::Object(values) => {
            values.remove("token");
            for value in values.values_mut() {
                redact_tokens(value);
            }
        }
        Value::Array(values) => values.iter_mut().for_each(redact_tokens),
        _ => {}
    }
}

fn canonical(root: &Path, id: &str, bytes: &[u8]) -> Result<Option<Canonical>, CoreError> {
    let content = std::str::from_utf8(bytes).map_err(|_| invalid())?;
    let mut previous = 0_u64;
    let mut request: Option<Request> = None;
    let mut pending = None;
    let mut pending_at = 0;
    let mut final_status = None;
    let mut choice_sequence = None;
    let mut phase_sequence = 0;
    for line in content.lines().filter(|line| !line.trim().is_empty()) {
        if line.len() > MAX_ENTRY {
            return Err(invalid());
        }
        let record: Value = serde_json::from_str(line).map_err(|_| invalid())?;
        let sequence = record["seq"]
            .as_u64()
            .filter(|sequence| *sequence > previous)
            .ok_or_else(invalid)?;
        let mut event = record
            .get("event")
            .filter(|event| event.is_object())
            .cloned()
            .ok_or_else(invalid)?;
        let kind = event["type"].as_str().ok_or_else(invalid)?.to_owned();
        if record["id"] != id || event["id"] != id || record["type"] != kind {
            return Err(invalid());
        }
        previous = sequence;
        validate_source(root, &event)?;
        redact_tokens(&mut event);
        match kind.as_str() {
            "generate" | "steer" | "manual_edit_apply" | "variant_mount_failed" => {
                human(&event)?;
                let at = submitted_at(&record, &event);
                if !request
                    .as_ref()
                    .is_some_and(|request| same_submission(&request.event, &event))
                    || final_status.is_some()
                {
                    request = Some(Request {
                        sequence,
                        created_at: at,
                        event: event.clone(),
                    });
                    choice_sequence = None;
                }
                pending_at = request.as_ref().map_or(at, |request| request.created_at);
                event["_jarvisPendingEventSeq"] = Value::from(sequence);
                pending = Some(event);
                phase_sequence = sequence;
                final_status = None;
            }
            "accept" | "discard" | "carbonize_cleanup" => {
                if kind != "carbonize_cleanup" {
                    choice_sequence = Some(sequence);
                }
                if request.is_none() {
                    request = Some(Request {
                        sequence,
                        created_at: submitted_at(&record, &event),
                        event: event.clone(),
                    });
                }
                pending_at = submitted_at(&record, &event);
                event["_jarvisPendingEventSeq"] = Value::from(sequence);
                pending = Some(event);
                phase_sequence = sequence;
                final_status = None;
            }
            "agent_done" | "steer_done" | "variants_ready" | "complete" => {
                pending = None;
                final_status = Some(Status::Completed);
            }
            "discarded" => {
                pending = None;
                final_status = Some(Status::Cancelled);
            }
            "agent_error" => {
                pending = None;
                final_status = Some(Status::Blocked);
            }
            _ => {}
        }
    }
    Ok(request.map(|request| Canonical {
        action_id: id.into(),
        request,
        pending,
        pending_at,
        final_status,
        journal_sequence: previous,
        choice_sequence,
        phase_sequence,
    }))
}

fn journals(root: &Path) -> Result<Vec<Canonical>, CoreError> {
    let mut paths = HashMap::new();
    for relative in [".impeccable-live/sessions", ".impeccable/live/sessions"] {
        let directory = root.join(relative);
        checked_path(root, &directory)?;
        match fs::symlink_metadata(&directory) {
            Err(cause) if cause.kind() == std::io::ErrorKind::NotFound => continue,
            Err(cause) => return Err(cause.into()),
            Ok(metadata) if !metadata.is_dir() || metadata.is_symlink() => return Err(invalid()),
            Ok(_) => {}
        }
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            let filename = entry.file_name();
            let Some(id) = filename
                .to_str()
                .and_then(|name| name.strip_suffix(".jsonl"))
            else {
                continue;
            };
            if !valid_id(id) || paths.len() >= MAX_FILES && !paths.contains_key(id) {
                return Err(invalid());
            }
            paths.insert(id.to_owned(), entry.path());
        }
    }
    let mut bytes_read = 0_u64;
    let mut snapshots = Vec::new();
    for (id, path) in paths {
        let bytes = regular(root, &path, MAX_JOURNAL)?.ok_or_else(invalid)?;
        bytes_read += bytes.len() as u64;
        if bytes_read > MAX_SCAN {
            return Err(invalid());
        }
        if let Some(snapshot) = canonical(root, &id, &bytes)? {
            snapshots.push(snapshot);
        }
    }
    Ok(snapshots)
}

fn directory(home: &Path) -> Result<PathBuf, CoreError> {
    let home = home.canonicalize()?;
    let target = super::super::root(&home).join("live/tasks");
    let mut directory = home.clone();
    for part in target
        .strip_prefix(&home)
        .map_err(|_| invalid())?
        .components()
    {
        let Component::Normal(part) = part else {
            return Err(invalid());
        };
        directory.push(part);
        checked_path(&home, &directory)?;
        match fs::create_dir(&directory) {
            Ok(()) => {}
            Err(cause) if cause.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(cause) => return Err(cause.into()),
        }
        if !fs::symlink_metadata(&directory)?.is_dir() {
            return Err(invalid());
        }
    }
    Ok(directory)
}

fn path(root: &Path, directory: &Path) -> PathBuf {
    directory.join(format!(
        "{:x}.json",
        Sha256::digest(root.as_os_str().as_encoded_bytes())
    ))
}

fn lock(directory: &Path, path: &Path) -> Result<File, CoreError> {
    let path = path.with_extension("lock");
    checked_path(directory, &path)?;
    let mut options = OpenOptions::new();
    options.create(true).truncate(false).read(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
    }
    let file = options.open(path)?;
    if !file.metadata()?.is_file() {
        return Err(invalid());
    }
    fs2::FileExt::lock_exclusive(&file)?;
    Ok(file)
}

fn load(root: &Path, directory: &Path, path: &Path) -> Result<Queue, CoreError> {
    let Some(bytes) = regular(directory, path, MAX_QUEUE)? else {
        return Ok(Queue {
            version: 1,
            project_root: root.into(),
            activation_cutoff_ms: 0,
            tasks: Vec::new(),
        });
    };
    let queue: Queue = serde_json::from_slice(&bytes).map_err(|_| invalid())?;
    if queue.version != 1 || queue.project_root != root || queue.tasks.len() > MAX_TASKS {
        return Err(invalid());
    }
    let mut ids = std::collections::HashSet::new();
    for record in &queue.tasks {
        if !valid_id(&record.task.action_id)
            || record.task.revision == 0
            || !ids.insert(&record.task.request_id)
            || !record
                .task
                .request_id
                .starts_with(&format!("{}:", record.task.action_id))
            || record.private_event["id"] != record.task.action_id
            || record.task.content.chars().count() > MAX_CONTENT
        {
            return Err(invalid());
        }
        validate_source(root, &record.private_event)?;
    }
    Ok(queue)
}

fn save(directory: &Path, path: &Path, queue: &Queue) -> Result<(), CoreError> {
    if queue.tasks.len() > MAX_TASKS {
        return Err(invalid());
    }
    let bytes = serde_json::to_vec(queue).map_err(|_| invalid())?;
    if bytes.len() as u64 > MAX_QUEUE {
        return Err(invalid());
    }
    if regular(directory, path, MAX_QUEUE)?.as_deref() == Some(bytes.as_slice()) {
        return Ok(());
    }
    checked_path(directory, path)?;
    let mut file = tempfile::NamedTempFile::new_in(directory)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.as_file()
            .set_permissions(fs::Permissions::from_mode(0o600))?;
    }
    file.write_all(&bytes)?;
    file.as_file().sync_all()?;
    file.persist(path).map_err(|cause| cause.error)?;
    #[cfg(unix)]
    File::open(directory)?.sync_all()?;
    Ok(())
}

fn bump(task: &mut Task) -> Result<(), CoreError> {
    task.revision = task.revision.checked_add(1).ok_or_else(invalid)?;
    Ok(())
}

fn visible(record: &StoredTask) -> Task {
    let mut task = record.task.clone();
    task.event = record.private_event.clone();
    if let Some(content) = &record.overlay {
        let key = if task.event["type"] == "steer" {
            "message"
        } else {
            "freeformPrompt"
        };
        task.event[key] = Value::String(content.clone());
        task.event["_jarvisEdited"] = Value::Bool(true);
    }
    task
}

/// Read an atomic persisted snapshot without creating directories, lock files,
/// or queue records. Mutations still compare revisions while holding the lock.
pub(super) fn read(root: &Path, home: &Path) -> Result<Vec<Task>, CoreError> {
    let root = root.canonicalize()?;
    let home = home.canonicalize()?;
    let directory = super::super::root(&home).join("live/tasks");
    checked_path(&home, &directory)?;
    let path = path(&root, &directory);
    Ok(load(&root, &directory, &path)?
        .tasks
        .iter()
        .map(visible)
        .collect())
}

/// Fence a leased event against the latest human request before merging its
/// editable overlay or admitting a turn. An action ID may span many requests.
fn current_snapshot(root: &Path, task: &Task) -> Result<Option<Canonical>, CoreError> {
    if !valid_id(&task.action_id) {
        return Err(invalid());
    }
    let root = root.canonicalize()?;
    for relative in [".impeccable/live/sessions", ".impeccable-live/sessions"] {
        let path = root
            .join(relative)
            .join(format!("{}.jsonl", task.action_id));
        if let Some(bytes) = regular(&root, &path, MAX_JOURNAL)? {
            return canonical(&root, &task.action_id, &bytes);
        }
    }
    Ok(None)
}

pub(super) fn is_current(root: &Path, task: &Task) -> Result<bool, CoreError> {
    Ok(current_snapshot(root, task)?.is_some_and(|snapshot| {
        format!("{}:{}", snapshot.action_id, snapshot.request.sequence) == task.request_id
    }))
}

fn current_event(snapshot: &Canonical, task: &Task) -> bool {
    if format!("{}:{}", snapshot.action_id, snapshot.request.sequence) != task.request_id {
        return false;
    }
    let Some(sequence) = task.event["_jarvisPendingEventSeq"]
        .as_u64()
        .filter(|seq| *seq > 0)
    else {
        return false;
    };
    match task.event["type"].as_str() {
        Some("accept" | "discard") => snapshot.choice_sequence == Some(sequence),
        Some("generate" | "steer") => snapshot.choice_sequence.is_none_or(|seq| seq <= sequence),
        _ => {
            snapshot.phase_sequence == sequence
                && snapshot
                    .pending
                    .as_ref()
                    .is_none_or(|pending| pending["type"] == task.event["type"])
        }
    }
}

/// A choice owns its cleanup, but never a later browser decision in the same request.
/// Terminal outcomes remain readable after a successful completion or discard.
pub(super) fn is_current_event(root: &Path, task: &Task) -> Result<bool, CoreError> {
    Ok(current_snapshot(root, task)?.is_some_and(|snapshot| current_event(&snapshot, task)))
}

/// A terminal failure or discard cannot authorize another source mutation or ACK.
/// A completed choice retains its lineage for the helper's idempotent completion.
pub(super) fn can_mutate_current_event(root: &Path, task: &Task) -> Result<bool, CoreError> {
    Ok(current_snapshot(root, task)?.is_some_and(|snapshot| {
        current_event(&snapshot, task)
            && !matches!(
                snapshot.final_status,
                Some(Status::Blocked | Status::Cancelled)
            )
    }))
}

/// A model may fail after a successful ACK. Reconciliation must prove that the
/// same human request is confirmed and that no later lifecycle event is pending.
pub(super) fn confirmed_outcome(root: &Path, task: &Task) -> Result<Option<Status>, CoreError> {
    Ok(current_snapshot(root, task)?.and_then(|snapshot| {
        if format!("{}:{}", snapshot.action_id, snapshot.request.sequence) == task.request_id
            && snapshot.pending.is_none()
        {
            snapshot.final_status.filter(|status| {
                matches!(
                    status,
                    Status::Completed | Status::Cancelled | Status::Blocked
                )
            })
        } else {
            None
        }
    }))
}

/// This precheck avoids repeated/no-op cleanup after restart. The helper still
/// performs the atomic compare; a journal read is never the ACK authorization.
pub(super) fn has_current_pending(root: &Path, task: &Task) -> Result<bool, CoreError> {
    let Some(sequence) = task.event["_jarvisPendingEventSeq"].as_u64() else {
        return Ok(false);
    };
    Ok(current_snapshot(root, task)?.is_some_and(|snapshot| {
        format!("{}:{}", snapshot.action_id, snapshot.request.sequence) == task.request_id
            && snapshot.pending.is_some_and(|event| {
                event["_jarvisPendingEventSeq"].as_u64() == Some(sequence)
                    && event["type"] == task.event["type"]
            })
    }))
}

/// Refresh the host queue from bounded canonical journals without modifying the
/// upstream session. Scaffold/pickup enrichment keeps the original request ID.
pub(super) fn scan(
    root: &Path,
    home: &Path,
    policy: ScanPolicy<'_>,
) -> Result<Vec<Task>, CoreError> {
    let root = root.canonicalize()?;
    let directory = directory(home)?;
    let path = path(&root, &directory);
    let _lock = lock(&directory, &path)?;
    let mut queue = load(&root, &directory, &path)?;
    let activation_changed = queue.activation_cutoff_ms != policy.activation_cutoff_ms;
    for snapshot in journals(&root)? {
        let request_id = format!("{}:{}", snapshot.action_id, snapshot.request.sequence);
        // The upstream action now points at a newer human request. Preserve old
        // requests for review, but never merge them into or poll for the new one.
        for record in queue.tasks.iter_mut().filter(|record| {
            record.task.action_id == snapshot.action_id
                && record.task.request_id != request_id
                && record.task.status == Status::Pending
        }) {
            record.task.status = Status::Paused;
            record.explicit_resume = false;
            bump(&mut record.task)?;
        }
        let existing = queue
            .tasks
            .iter()
            .position(|record| record.task.request_id == request_id);
        let Some(pending) = snapshot.pending else {
            if let Some(record) = existing.map(|index| &mut queue.tasks[index]) {
                if !matches!(record.task.status, Status::Cancelled | Status::Running)
                    && snapshot.journal_sequence > record.journal_sequence
                {
                    if let Some(status) = snapshot.final_status {
                        if record.task.status != status {
                            record.task.status = status;
                            record.explicit_resume = false;
                            bump(&mut record.task)?;
                        }
                        if status == Status::Blocked {
                            record.private_event["_jarvisRecovery"] = Value::Bool(true);
                        }
                    }
                }
                record.journal_sequence = snapshot.journal_sequence;
            }
            continue;
        };
        let status = if snapshot.pending_at >= policy.activation_cutoff_ms
            || policy.recover_action_id == Some(snapshot.action_id.as_str())
        {
            Status::Pending
        } else {
            Status::Paused
        };
        let lifecycle = matches!(
            pending["type"].as_str(),
            Some("accept" | "discard" | "carbonize_cleanup")
        );
        if let Some(record) = existing.map(|index| &mut queue.tasks[index]) {
            // A preview pause ends only for a newer browser decision. Repeated
            // scans must not resume an already observed acceptance.
            let new_lifecycle = lifecycle
                && pending["_jarvisPendingEventSeq"]
                    .as_u64()
                    .is_some_and(|sequence| {
                        sequence
                            > record.private_event["_jarvisPendingEventSeq"]
                                .as_u64()
                                .unwrap_or(0)
                    });
            let new_choice =
                new_lifecycle && matches!(pending["type"].as_str(), Some("accept" | "discard"));
            // A fresh browser choice supersedes cancellation of an earlier phase,
            // while an offline choice stays paused instead of being replayed.
            if record.task.status == Status::Cancelled && !new_choice {
                continue;
            }
            record.private_event = pending;
            record.journal_sequence = snapshot.journal_sequence;
            let next = match record.task.status {
                Status::Cancelled if new_choice => Some(status),
                Status::Running if new_choice => Some(status),
                Status::Completed if lifecycle => Some(status),
                Status::Paused | Status::Blocked if new_lifecycle => Some(status),
                Status::Pending if activation_changed => Some(if record.explicit_resume {
                    Status::Pending
                } else {
                    status
                }),
                Status::Running if activation_changed => Some(status),
                Status::Paused if policy.recover_action_id == Some(snapshot.action_id.as_str()) => {
                    Some(Status::Pending)
                }
                _ => None,
            };
            if let Some(next) = next.filter(|next| *next != record.task.status) {
                record.task.status = next;
                bump(&mut record.task)?;
            }
        } else {
            let content = human(&snapshot.request.event)?;
            queue.tasks.push(StoredTask {
                task: Task {
                    request_id,
                    action_id: snapshot.action_id,
                    revision: 1,
                    content,
                    event: Value::Null,
                    status,
                    bead_id: None,
                    turn_id: None,
                    created_at: snapshot.request.created_at,
                },
                private_event: pending,
                overlay: None,
                journal_sequence: snapshot.journal_sequence,
                explicit_resume: false,
            });
        }
    }
    queue.activation_cutoff_ms = policy.activation_cutoff_ms;
    queue.tasks.sort_by(|left, right| {
        (left.task.created_at, &left.task.request_id)
            .cmp(&(right.task.created_at, &right.task.request_id))
    });
    save(&directory, &path, &queue)?;
    Ok(queue.tasks.iter().map(visible).collect())
}

fn mutate(
    root: &Path,
    home: &Path,
    request_id: &str,
    expected_revision: Option<u64>,
    update: impl FnOnce(&mut StoredTask) -> Result<(), CoreError>,
) -> Result<Task, CoreError> {
    let root = root.canonicalize()?;
    let directory = directory(home)?;
    let path = path(&root, &directory);
    let _lock = lock(&directory, &path)?;
    let mut queue = load(&root, &directory, &path)?;
    let record = queue
        .tasks
        .iter_mut()
        .find(|record| record.task.request_id == request_id)
        .ok_or_else(|| error("Este pedido Live não foi encontrado. Atualize a lista."))?;
    if expected_revision.is_some_and(|revision| record.task.revision != revision) {
        return Err(conflict());
    }
    update(record)?;
    bump(&mut record.task)?;
    let task = visible(record);
    save(&directory, &path, &queue)?;
    Ok(task)
}

pub(super) fn edit(
    root: &Path,
    home: &Path,
    request_id: &str,
    expected_revision: u64,
    content: &str,
) -> Result<Task, CoreError> {
    if content.trim().is_empty() || content.chars().count() > MAX_CONTENT || content.contains('\0')
    {
        return Err(error("Escreva um pedido Live válido antes de salvar."));
    }
    mutate(root, home, request_id, Some(expected_revision), |record| {
        if !matches!(record.task.status, Status::Pending | Status::Paused) {
            return Err(error(
                "Só é possível editar pedidos Live que ainda não estão executando.",
            ));
        }
        record.task.content = content.into();
        record.overlay = Some(content.into());
        Ok(())
    })
}

pub(super) fn remove(
    root: &Path,
    home: &Path,
    request_id: &str,
    expected_revision: u64,
) -> Result<Task, CoreError> {
    mutate(root, home, request_id, Some(expected_revision), |record| {
        if !matches!(
            record.task.status,
            Status::Pending | Status::Paused | Status::Blocked
        ) {
            return Err(error(
                "Interrompa a execução Live antes de remover este pedido.",
            ));
        }
        record.task.status = Status::Cancelled;
        record.explicit_resume = false;
        Ok(())
    })
}

pub(super) fn resume(
    root: &Path,
    home: &Path,
    request_id: &str,
    expected_revision: u64,
) -> Result<Task, CoreError> {
    mutate(root, home, request_id, Some(expected_revision), |record| {
        if !matches!(record.task.status, Status::Paused | Status::Blocked) {
            return Err(error("Este pedido Live não está aguardando retomada."));
        }
        record.task.status = Status::Pending;
        record.task.turn_id = None;
        record.explicit_resume = true;
        Ok(())
    })
}

pub(super) fn transition(
    root: &Path,
    home: &Path,
    request_id: &str,
    status: Status,
    bead_id: Option<&str>,
    turn_id: Option<&str>,
) -> Result<Task, CoreError> {
    mutate(root, home, request_id, None, |record| {
        let allowed = record.task.status == status
            || matches!(
                (record.task.status, status),
                (
                    Status::Pending,
                    Status::Running
                        | Status::Paused
                        | Status::Blocked
                        | Status::Completed
                        | Status::Cancelled
                ) | (
                    Status::Running,
                    Status::Completed | Status::Blocked | Status::Paused | Status::Cancelled
                ) | (
                    Status::Paused | Status::Blocked,
                    Status::Pending | Status::Cancelled | Status::Paused | Status::Blocked
                )
            );
        if !allowed {
            return Err(error(
                "Este pedido Live já terminou ou foi substituído. Atualize a lista.",
            ));
        }
        if status != Status::Pending {
            record.explicit_resume = false;
        } else if matches!(record.task.status, Status::Paused | Status::Blocked) {
            record.explicit_resume = true;
        }
        record.task.status = status;
        if let Some(id) = bead_id {
            record.task.bead_id = Some(id.into());
        }
        if let Some(id) = turn_id {
            record.task.turn_id = Some(id.into());
        }
        Ok(())
    })
}

/// A turn admission may finish after a browser choice updates the same request.
/// Keep that newer phase pending instead of marking it as the old running turn.
pub(super) fn admit(
    root: &Path,
    home: &Path,
    task: &Task,
    bead_id: &str,
    turn_id: Option<&str>,
) -> Result<Task, CoreError> {
    mutate(root, home, &task.request_id, None, |record| {
        if matches!(record.task.status, Status::Pending | Status::Running)
            && record.private_event["type"] == task.event["type"]
            && record.private_event["_jarvisPendingEventSeq"]
                == task.event["_jarvisPendingEventSeq"]
        {
            record.task.status = Status::Running;
            record.explicit_resume = false;
            record.task.bead_id = Some(bead_id.into());
            if let Some(id) = turn_id {
                record.task.turn_id = Some(id.into());
            }
        }
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    struct Fixture {
        _directory: tempfile::TempDir,
        root: PathBuf,
        home: PathBuf,
    }

    impl Fixture {
        fn new() -> Self {
            let directory = tempfile::tempdir().unwrap();
            let root = directory.path().join("project");
            let home = directory.path().join("home");
            fs::create_dir_all(root.join(".impeccable/live/sessions")).unwrap();
            fs::create_dir_all(&home).unwrap();
            Self {
                _directory: directory,
                root,
                home,
            }
        }

        fn journal(&self, id: &str, events: &[(u64, Value)]) -> PathBuf {
            let path = self
                .root
                .join(".impeccable/live/sessions")
                .join(format!("{id}.jsonl"));
            let content = events
                .iter()
                .enumerate()
                .map(|(index, (time, event))| {
                    json!({"seq":index + 1,"id":id,"type":event["type"],"ts":time,"event":event})
                        .to_string()
                })
                .collect::<Vec<_>>()
                .join("\n");
            fs::write(&path, content).unwrap();
            path
        }

        fn scan(&self, cutoff: u64) -> Vec<Task> {
            scan(
                &self.root,
                &self.home,
                ScanPolicy {
                    activation_cutoff_ms: cutoff,
                    recover_action_id: None,
                },
            )
            .unwrap()
        }
    }

    fn generate(id: &str, content: &str, sent: u64) -> Value {
        json!({"type":"generate","id":id,"action":"impeccable","freeformPrompt":content,"clientSentAt":sent,"element":{"tagName":"div"},"count":3,"token":"private-helper-token"})
    }

    #[test]
    fn reading_an_absent_queue_does_not_create_files() {
        let fixture = Fixture::new();
        assert!(read(&fixture.root, &fixture.home).unwrap().is_empty());
        assert!(!fixture.home.join("core").exists());
        assert!(!crate::data_dir::root(&fixture.home).exists());
    }

    #[test]
    fn host_queue_uses_only_the_active_runtime_profile() {
        let fixture = Fixture::new();
        fixture.journal(
            "profile",
            &[(110, generate("profile", "Change icons", 110))],
        );
        fixture.scan(100);
        let selected = super::super::super::root(&fixture.home).join("live/tasks");
        assert!(selected.is_dir());
        assert!(!fixture.home.join("core").exists());
        let alternate = match crate::data_dir::profile() {
            crate::data_dir::Profile::Production => crate::data_dir::Profile::Development,
            crate::data_dir::Profile::Development => crate::data_dir::Profile::Production,
        };
        assert!(!crate::data_dir::root_for(&fixture.home, alternate).exists());
    }

    #[test]
    fn repeated_scan_keeps_the_atomic_snapshot_unchanged() {
        let fixture = Fixture::new();
        fixture.journal("stable", &[(110, generate("stable", "Change icons", 110))]);
        let first = fixture.scan(100).remove(0);
        let directory = directory(&fixture.home).unwrap();
        let path = path(&fixture.root.canonicalize().unwrap(), &directory);
        let before = fs::metadata(&path).unwrap();
        let bytes = fs::read(&path).unwrap();
        let again = fixture.scan(100).remove(0);
        let after = fs::metadata(&path).unwrap();
        assert_eq!(again.revision, first.revision);
        assert_eq!(fs::read(&path).unwrap(), bytes);
        assert_eq!(before.modified().unwrap(), after.modified().unwrap());
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            assert_eq!(before.ino(), after.ino());
        }
        let persisted = read(&fixture.root, &fixture.home).unwrap().remove(0);
        assert_eq!(persisted.request_id, first.request_id);
        assert_eq!(persisted.event["freeformPrompt"], "Change icons");
    }

    #[test]
    fn canonical_ack_does_not_finish_the_host_turn_before_its_cleanup() {
        let fixture = Fixture::new();
        let event = generate("running", "Change icons", 110);
        fixture.journal("running", &[(110, event.clone())]);
        let task = fixture.scan(100).remove(0);
        transition(
            &fixture.root,
            &fixture.home,
            &task.request_id,
            Status::Running,
            None,
            Some("current-turn"),
        )
        .unwrap();
        fixture.journal(
            "running",
            &[
                (110, event),
                (120, json!({"type":"agent_done","id":"running"})),
            ],
        );
        let current = fixture.scan(100).remove(0);
        assert_eq!(current.status, Status::Running);
        assert_eq!(current.turn_id.as_deref(), Some("current-turn"));
        assert!(is_current(&fixture.root, &current).unwrap());
    }

    #[test]
    fn legacy_requests_pause_and_new_requests_keep_human_fifo() {
        let fixture = Fixture::new();
        fixture.journal("old", &[(10, generate("old", "Move the badge", 10))]);
        fixture.journal(
            "z-first",
            &[(110, generate("z-first", "Change icons", 110))],
        );
        fixture.journal(
            "a-last",
            &[(
                130,
                json!({"type":"steer","id":"a-last","message":"Adjust spacing"}),
            )],
        );
        let tasks = fixture.scan(100);
        assert_eq!(
            tasks
                .iter()
                .map(|task| (task.action_id.as_str(), task.status))
                .collect::<Vec<_>>(),
            [
                ("old", Status::Paused),
                ("z-first", Status::Pending),
                ("a-last", Status::Pending)
            ]
        );
        assert_eq!(tasks[1].content, "Change icons");
        assert_eq!(tasks[1].event["id"], "z-first");
        assert!(tasks[1].event.get("token").is_none());
        let serialized = serde_json::to_string(&tasks).unwrap();
        assert!(!serialized.contains("private-helper-token"));
        assert!(!serialized.contains("element"));
    }

    #[test]
    fn enrichment_does_not_duplicate_or_make_legacy_request_fresh() {
        let fixture = Fixture::new();
        let original = generate("old", "Move the badge", 10);
        fixture.journal("old", &[(10, original.clone())]);
        let first = fixture.scan(100).remove(0);
        let mut prepared = original.clone();
        prepared["generationReadyAt"] = json!(150);
        prepared["scaffold"] = json!({"file":"index.html","sourceWritten":false});
        fixture.journal(
            "old",
            &[
                (10, original),
                (
                    140,
                    json!({"type":"agent_phase","id":"old","phase":"picked_up"}),
                ),
                (150, prepared),
            ],
        );
        let tasks = fixture.scan(100);
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].request_id, first.request_id);
        assert_eq!(tasks[0].status, Status::Paused);
        assert_eq!(tasks[0].created_at, 10);
        assert_eq!(tasks[0].event["generationReadyAt"], 150);
        assert!(is_current(&fixture.root, &tasks[0]).unwrap());
    }

    #[test]
    fn newer_request_on_the_same_action_fences_the_previously_selected_task() {
        let fixture = Fixture::new();
        let original = generate("reused", "Move the badge", 110);
        let journal = fixture.journal("reused", &[(110, original.clone())]);
        let first = fixture.scan(100).remove(0);
        assert!(is_current(&fixture.root, &first).unwrap());
        fixture.journal(
            "reused",
            &[
                (110, original),
                (
                    120,
                    json!({"type":"steer","id":"reused","message":"Change icons"}),
                ),
            ],
        );
        let bytes = fs::read(&journal).unwrap();
        assert!(!is_current(&fixture.root, &first).unwrap());
        assert_eq!(fs::read(&journal).unwrap(), bytes);
        let tasks = fixture.scan(100);
        assert_eq!(tasks.len(), 2);
        assert_eq!(tasks[0].status, Status::Paused);
        assert_eq!(tasks[0].content, "Move the badge");
        assert_eq!(tasks[1].status, Status::Pending);
        assert_eq!(tasks[1].content, "Change icons");
        assert!(is_current(&fixture.root, &tasks[1]).unwrap());
    }

    #[test]
    fn a_new_human_request_does_not_pause_an_already_admitted_turn() {
        let fixture = Fixture::new();
        let original = generate("active", "Move the badge", 110);
        fixture.journal("active", &[(110, original.clone())]);
        let first = fixture.scan(100).remove(0);
        transition(
            &fixture.root,
            &fixture.home,
            &first.request_id,
            Status::Running,
            None,
            Some("turn-1"),
        )
        .unwrap();
        fixture.journal(
            "active",
            &[
                (110, original),
                (
                    120,
                    json!({"type":"steer","id":"active","message":"Change icons"}),
                ),
            ],
        );
        let tasks = fixture.scan(100);
        assert_eq!(tasks[0].status, Status::Running);
        assert_eq!(tasks[0].turn_id.as_deref(), Some("turn-1"));
        assert_eq!(tasks[1].status, Status::Pending);
    }

    #[test]
    fn exact_owned_recovery_does_not_resume_other_legacy_requests() {
        let fixture = Fixture::new();
        fixture.journal("owned", &[(10, generate("owned", "Continue this", 10))]);
        fixture.journal("other", &[(11, generate("other", "Keep this paused", 11))]);
        let tasks = scan(
            &fixture.root,
            &fixture.home,
            ScanPolicy {
                activation_cutoff_ms: 100,
                recover_action_id: Some("owned"),
            },
        )
        .unwrap();
        assert_eq!(
            tasks
                .iter()
                .map(|task| (task.action_id.as_str(), task.status))
                .collect::<Vec<_>>(),
            [("owned", Status::Pending), ("other", Status::Paused)]
        );
    }

    #[test]
    fn editing_is_revisioned_durable_and_leaves_upstream_journal_unchanged() {
        let fixture = Fixture::new();
        let journal = fixture.journal("edit", &[(110, generate("edit", "Change icons", 110))]);
        let original = fs::read(&journal).unwrap();
        let task = fixture.scan(100).remove(0);
        let updated = edit(
            &fixture.root,
            &fixture.home,
            &task.request_id,
            task.revision,
            "Use subtle cyan icons",
        )
        .unwrap();
        assert_eq!(updated.revision, 2);
        assert!(edit(
            &fixture.root,
            &fixture.home,
            &task.request_id,
            1,
            "Stale edit"
        )
        .is_err());
        let again = fixture.scan(100).remove(0);
        assert_eq!(again.content, "Use subtle cyan icons");
        assert_eq!(again.event["freeformPrompt"], "Use subtle cyan icons");
        assert_eq!(fs::read(journal).unwrap(), original);
    }

    #[test]
    fn cancelled_generation_stays_cancelled_and_later_offline_choice_waits_for_resume() {
        let fixture = Fixture::new();
        let original = generate("cancel", "Change icons", 110);
        fixture.journal("cancel", &[(110, original.clone())]);
        let task = fixture.scan(100).remove(0);
        remove(
            &fixture.root,
            &fixture.home,
            &task.request_id,
            task.revision,
        )
        .unwrap();
        assert_eq!(fixture.scan(200)[0].status, Status::Cancelled);
        fixture.journal(
            "cancel",
            &[
                (110, original),
                (120, json!({"type":"accept","id":"cancel","variantId":0})),
            ],
        );
        let paused = fixture.scan(200).remove(0);
        assert_eq!(paused.status, Status::Paused);
        assert_eq!(paused.event["type"], "accept");
        assert_eq!(fixture.scan(200)[0].status, Status::Paused);
    }

    #[test]
    fn a_new_browser_choice_after_cancellation_or_failure_runs_only_that_choice_once() {
        for previous in [Status::Cancelled, Status::Blocked] {
            for kind in ["accept", "discard"] {
                let fixture = Fixture::new();
                let original = generate("retry-choice", "Change icons", 110);
                let mut events = vec![(110, original)];
                fixture.journal("retry-choice", &events);
                let task = fixture.scan(100).remove(0);
                let stopped = transition(
                    &fixture.root,
                    &fixture.home,
                    &task.request_id,
                    previous,
                    Some("existing-bead"),
                    Some("previous-turn"),
                )
                .unwrap();
                assert_eq!(fixture.scan(200)[0].status, previous);
                events.push((
                    210,
                    json!({"type":kind,"id":"retry-choice","variantId":"2","clientSentAt":210}),
                ));
                fixture.journal("retry-choice", &events);
                let tasks = fixture.scan(200);
                assert_eq!(tasks.len(), 1);
                let choice = &tasks[0];
                assert_eq!(choice.status, Status::Pending);
                assert_eq!(choice.request_id, stopped.request_id);
                assert_eq!(choice.bead_id, stopped.bead_id);
                assert_eq!(choice.event["type"], kind);
                assert_eq!(choice.event["variantId"], "2");
                assert_eq!(choice.revision, stopped.revision + 1);
                assert_eq!(fixture.scan(200)[0].revision, choice.revision);
                let cancelled = remove(
                    &fixture.root,
                    &fixture.home,
                    &choice.request_id,
                    choice.revision,
                )
                .unwrap();
                let again = fixture.scan(200).remove(0);
                assert_eq!(again.status, Status::Cancelled);
                assert_eq!(again.revision, cancelled.revision);
                assert_eq!(fixture.scan(300)[0].status, Status::Cancelled);
            }
        }
    }

    #[test]
    fn accept_and_cleanup_reuse_the_completed_task_and_bead() {
        let fixture = Fixture::new();
        let original = generate("accept", "Change icons", 110);
        let done = json!({"type":"agent_done","id":"accept","sourceEventType":"generate","file":"index.html"});
        fixture.journal("accept", &[(110, original.clone())]);
        let task = fixture.scan(100).remove(0);
        transition(
            &fixture.root,
            &fixture.home,
            &task.request_id,
            Status::Running,
            Some("jarvis-bead"),
            Some("turn-1"),
        )
        .unwrap();
        fixture.journal("accept", &[(110, original.clone()), (120, done.clone())]);
        assert_eq!(fixture.scan(100)[0].status, Status::Running);
        transition(
            &fixture.root,
            &fixture.home,
            &task.request_id,
            Status::Completed,
            None,
            None,
        )
        .unwrap();
        assert_eq!(fixture.scan(100)[0].status, Status::Completed);
        fixture.journal(
            "accept",
            &[
                (110, original),
                (120, done),
                (130, json!({"type":"accept","id":"accept","variantId":1})),
                (140, json!({"type":"carbonize_cleanup","id":"accept"})),
            ],
        );
        let tasks = fixture.scan(100);
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].request_id, task.request_id);
        assert_eq!(tasks[0].status, Status::Pending);
        assert_eq!(tasks[0].bead_id.as_deref(), Some("jarvis-bead"));
        assert_eq!(tasks[0].event["type"], "carbonize_cleanup");
    }

    fn paused_preview(fixture: &Fixture, id: &str) -> (Task, Vec<(u64, Value)>) {
        let mut events = vec![(110, generate(id, "Change icons", 110))];
        fixture.journal(id, &events);
        let task = fixture.scan(100).remove(0);
        let task = edit(
            &fixture.root,
            &fixture.home,
            &task.request_id,
            task.revision,
            "Use subtle cyan icons",
        )
        .unwrap();
        transition(
            &fixture.root,
            &fixture.home,
            &task.request_id,
            Status::Running,
            Some("jarvis-bead"),
            Some("preview-turn"),
        )
        .unwrap();
        events.push((120, json!({"type":"variants_ready","id":id})));
        fixture.journal(id, &events);
        assert_eq!(fixture.scan(100)[0].status, Status::Running);
        let task = transition(
            &fixture.root,
            &fixture.home,
            &task.request_id,
            Status::Paused,
            None,
            None,
        )
        .unwrap();
        (task, events)
    }

    #[test]
    fn choice_completion_cannot_acknowledge_a_later_choice_in_the_same_human_request() {
        let fixture = Fixture::new();
        let mut events = vec![(110, generate("choice", "Move the badge", 110))];
        fixture.journal("choice", &events);
        let generation = fixture.scan(100).remove(0);
        assert!(is_current_event(&fixture.root, &generation).unwrap());
        events.push((120, json!({"id":"choice","type":"variants_ready"})));
        events.push((130, json!({"id":"choice","type":"accept","variantId":"2"})));
        fixture.journal("choice", &events);
        let first = fixture.scan(100).remove(0);
        assert!(is_current_event(&fixture.root, &first).unwrap());
        assert!(!is_current_event(&fixture.root, &generation).unwrap());
        events.push((140, json!({"id":"choice","type":"carbonize_cleanup"})));
        fixture.journal("choice", &events);
        let cleanup = fixture.scan(100).remove(0);
        assert!(is_current_event(&fixture.root, &first).unwrap());
        assert!(is_current_event(&fixture.root, &cleanup).unwrap());
        events.push((150, json!({"id":"choice","type":"accept","variantId":"1"})));
        fixture.journal("choice", &events);
        let latest = fixture.scan(100).remove(0);
        assert_eq!(latest.request_id, first.request_id);
        assert!(!is_current_event(&fixture.root, &first).unwrap());
        assert!(!is_current_event(&fixture.root, &cleanup).unwrap());
        assert!(is_current_event(&fixture.root, &latest).unwrap());
        events.push((160, json!({"id":"choice","type":"carbonize_cleanup"})));
        events.push((170, json!({"id":"choice","type":"complete"})));
        fixture.journal("choice", &events);
        assert!(!is_current_event(&fixture.root, &first).unwrap());
        assert!(is_current_event(&fixture.root, &latest).unwrap());
    }

    #[test]
    fn blocked_or_discarded_choice_cannot_mutate_even_while_its_host_turn_is_running() {
        for terminal in ["agent_error", "discarded"] {
            let fixture = Fixture::new();
            let mut events = vec![
                (110, generate("stale-choice", "Move the badge", 110)),
                (120, json!({"id":"stale-choice","type":"variants_ready"})),
                (
                    130,
                    json!({"id":"stale-choice","type":"accept","variantId":"2"}),
                ),
            ];
            let journal = fixture.journal("stale-choice", &events);
            let task = fixture.scan(100).remove(0);
            let task = transition(
                &fixture.root,
                &fixture.home,
                &task.request_id,
                Status::Running,
                Some("same-bead"),
                Some("accept-turn"),
            )
            .unwrap();
            assert!(can_mutate_current_event(&fixture.root, &task).unwrap());
            events.push((140, json!({"id":"stale-choice","type":terminal})));
            fixture.journal("stale-choice", &events);
            let before = fs::read(&journal).unwrap();
            // Queue reconciliation preserves the active turn until it settles;
            // admission must inspect the canonical outcome, not this host status.
            assert_eq!(fixture.scan(100)[0].status, Status::Running);
            assert!(is_current_event(&fixture.root, &task).unwrap());
            assert!(!can_mutate_current_event(&fixture.root, &task).unwrap());
            assert_eq!(fs::read(&journal).unwrap(), before);
        }
    }

    #[test]
    fn confirmed_discard_is_readable_without_authorizing_another_mutation() {
        let fixture = Fixture::new();
        let mut events = vec![
            (110, generate("discard-result", "Move the badge", 110)),
            (120, json!({"id":"discard-result","type":"variants_ready"})),
            (130, json!({"id":"discard-result","type":"discard"})),
        ];
        fixture.journal("discard-result", &events);
        let task = fixture.scan(100).remove(0);
        assert!(can_mutate_current_event(&fixture.root, &task).unwrap());
        events.push((140, json!({"id":"discard-result","type":"discarded"})));
        fixture.journal("discard-result", &events);
        assert!(is_current_event(&fixture.root, &task).unwrap());
        assert!(!can_mutate_current_event(&fixture.root, &task).unwrap());
        assert_eq!(
            confirmed_outcome(&fixture.root, &task).unwrap(),
            Some(Status::Cancelled)
        );
    }

    #[test]
    fn choice_retains_cleanup_and_idempotent_completion_only_for_its_own_lineage() {
        let fixture = Fixture::new();
        let mut events = vec![
            (110, generate("choice-lineage", "Move the badge", 110)),
            (120, json!({"id":"choice-lineage","type":"variants_ready"})),
            (
                130,
                json!({"id":"choice-lineage","type":"accept","variantId":"2"}),
            ),
        ];
        fixture.journal("choice-lineage", &events);
        let task = fixture.scan(100).remove(0);
        events.push((
            140,
            json!({"id":"choice-lineage","type":"carbonize_cleanup"}),
        ));
        fixture.journal("choice-lineage", &events);
        assert!(can_mutate_current_event(&fixture.root, &task).unwrap());
        events.push((150, json!({"id":"choice-lineage","type":"complete"})));
        fixture.journal("choice-lineage", &events);
        assert!(can_mutate_current_event(&fixture.root, &task).unwrap());
        assert_eq!(
            confirmed_outcome(&fixture.root, &task).unwrap(),
            Some(Status::Completed)
        );
        events.push((
            160,
            json!({"id":"choice-lineage","type":"accept","variantId":"1"}),
        ));
        fixture.journal("choice-lineage", &events);
        assert!(!can_mutate_current_event(&fixture.root, &task).unwrap());
        assert!(!is_current_event(&fixture.root, &task).unwrap());
    }

    #[test]
    fn browser_choice_supersedes_running_generation_once_without_replacing_the_request() {
        for kind in ["accept", "discard"] {
            let fixture = Fixture::new();
            let mut events = vec![(110, generate(kind, "Move the badge", 110))];
            fixture.journal(kind, &events);
            let task = fixture.scan(100).remove(0);
            transition(
                &fixture.root,
                &fixture.home,
                &task.request_id,
                Status::Running,
                Some("same-bead"),
                Some("generation-turn"),
            )
            .unwrap();
            events.push((120, json!({"type":"variants_ready","id":kind})));
            fixture.journal(kind, &events);
            assert_eq!(fixture.scan(100)[0].status, Status::Running);
            events.push((
                130,
                json!({"type":kind,"id":kind,"variantId":"2","clientSentAt":130}),
            ));
            let journal = fixture.journal(kind, &events);
            let original = fs::read(&journal).unwrap();
            let choice = fixture.scan(100).remove(0);
            assert_eq!(choice.status, Status::Pending);
            assert_eq!(choice.request_id, task.request_id);
            assert_eq!(choice.bead_id.as_deref(), Some("same-bead"));
            assert_eq!(choice.turn_id.as_deref(), Some("generation-turn"));
            assert_eq!(choice.event["type"], kind);
            assert_eq!(choice.event["variantId"], "2");
            let late_admission = admit(
                &fixture.root,
                &fixture.home,
                &task,
                "same-bead",
                Some("late-generation-turn"),
            )
            .unwrap();
            assert_eq!(late_admission.status, Status::Pending);
            assert_eq!(late_admission.event["variantId"], "2");
            assert_eq!(late_admission.turn_id.as_deref(), Some("generation-turn"));
            let running = transition(
                &fixture.root,
                &fixture.home,
                &choice.request_id,
                Status::Running,
                None,
                Some("choice-turn"),
            )
            .unwrap();
            let again = fixture.scan(100).remove(0);
            assert_eq!(again.status, Status::Running);
            assert_eq!(again.revision, running.revision);
            assert_eq!(fs::read(journal).unwrap(), original);
        }
    }

    #[test]
    fn new_accept_resumes_the_paused_preview_once_with_the_same_request_and_bead() {
        let fixture = Fixture::new();
        let (paused, mut events) = paused_preview(&fixture, "accept-paused");
        events.push((
            130,
            json!({"type":"accept","id":"accept-paused","variantId":"2","clientSentAt":130}),
        ));
        let journal = fixture.journal("accept-paused", &events);
        let original = fs::read(&journal).unwrap();
        let tasks = fixture.scan(100);
        assert_eq!(tasks.len(), 1);
        let accepted = &tasks[0];
        assert_eq!(accepted.status, Status::Pending);
        assert_eq!(accepted.request_id, paused.request_id);
        assert_eq!(accepted.action_id, paused.action_id);
        assert_eq!(accepted.bead_id, paused.bead_id);
        assert_eq!(accepted.turn_id, paused.turn_id);
        assert_eq!(accepted.content, paused.content);
        assert_eq!(accepted.created_at, paused.created_at);
        assert_eq!(accepted.event["variantId"], "2");
        assert_eq!(accepted.event["_jarvisPendingEventSeq"], 3);
        assert_eq!(accepted.event["_jarvisEdited"], true);
        assert_eq!(accepted.revision, paused.revision + 1);
        let again = fixture.scan(100).remove(0);
        assert_eq!(again.status, Status::Pending);
        assert_eq!(again.revision, accepted.revision);
        assert_eq!(fs::read(journal).unwrap(), original);
        // Pausing this exact acceptance must not admit it again on every scan.
        let paused = transition(
            &fixture.root,
            &fixture.home,
            &accepted.request_id,
            Status::Paused,
            None,
            None,
        )
        .unwrap();
        let again = fixture.scan(100).remove(0);
        assert_eq!(again.status, Status::Paused);
        assert_eq!(again.revision, paused.revision);
    }

    #[test]
    fn new_discard_or_carbonize_cleanup_resumes_only_the_existing_paused_request() {
        for kind in ["discard", "carbonize_cleanup"] {
            let fixture = Fixture::new();
            let (paused, mut events) = paused_preview(&fixture, kind);
            events.push((130, json!({"type":kind,"id":kind,"clientSentAt":130})));
            fixture.journal(kind, &events);
            let tasks = fixture.scan(100);
            assert_eq!(tasks.len(), 1);
            assert_eq!(tasks[0].status, Status::Pending);
            assert_eq!(tasks[0].request_id, paused.request_id);
            assert_eq!(tasks[0].bead_id, paused.bead_id);
            assert_eq!(tasks[0].content, paused.content);
            assert_eq!(tasks[0].event["type"], kind);
            assert_eq!(tasks[0].revision, paused.revision + 1);
            assert_eq!(fixture.scan(100)[0].revision, tasks[0].revision);
        }
    }

    #[test]
    fn an_old_accept_stays_paused_until_a_new_event_or_explicit_resume() {
        for explicitly_resume in [false, true] {
            let fixture = Fixture::new();
            let (paused, mut events) = paused_preview(&fixture, "old-accept");
            events.push((
                130,
                json!({"type":"accept","id":"old-accept","variantId":"1","clientSentAt":130}),
            ));
            fixture.journal("old-accept", &events);
            let old = fixture.scan(200).remove(0);
            assert_eq!(old.status, Status::Paused);
            assert_eq!(old.revision, paused.revision);
            assert_eq!(fixture.scan(200)[0].status, Status::Paused);
            assert_eq!(fixture.scan(200)[0].revision, old.revision);
            if explicitly_resume {
                resume(&fixture.root, &fixture.home, &old.request_id, old.revision).unwrap();
            } else {
                events.push((
                    210,
                    json!({"type":"accept","id":"old-accept","variantId":"2","clientSentAt":210}),
                ));
                fixture.journal("old-accept", &events);
            }
            let current = fixture.scan(200).remove(0);
            assert_eq!(current.status, Status::Pending);
            assert_eq!(current.request_id, old.request_id);
            assert_eq!(current.revision, old.revision + 1);
            assert_eq!(fixture.scan(200)[0].revision, current.revision);
        }
    }

    #[test]
    fn paused_request_resumes_explicitly_and_running_request_cannot_be_edited() {
        let fixture = Fixture::new();
        fixture.journal("resume", &[(10, generate("resume", "Change icons", 10))]);
        let task = fixture.scan(100).remove(0);
        let task = resume(
            &fixture.root,
            &fixture.home,
            &task.request_id,
            task.revision,
        )
        .unwrap();
        assert_eq!(fixture.scan(100)[0].status, Status::Pending);
        assert_eq!(fixture.scan(200)[0].status, Status::Pending);
        let task = transition(
            &fixture.root,
            &fixture.home,
            &task.request_id,
            Status::Running,
            None,
            Some("turn-1"),
        )
        .unwrap();
        assert_eq!(fixture.scan(200)[0].status, Status::Running);
        assert!(edit(
            &fixture.root,
            &fixture.home,
            &task.request_id,
            task.revision,
            "Different request"
        )
        .is_err());
        assert_eq!(fixture.scan(300)[0].status, Status::Paused);
    }

    #[test]
    fn journal_iso_timestamp_is_the_human_submission_time() {
        let fixture = Fixture::new();
        let event = json!({"type":"steer","id":"iso","message":"Change icons"});
        let record = json!({"seq":1,"id":"iso","type":"steer","ts":"2026-10-09T18:00:00.125Z","event":event});
        fs::write(
            fixture.root.join(".impeccable/live/sessions/iso.jsonl"),
            record.to_string(),
        )
        .unwrap();
        let expected = 1_791_568_800_125;
        let task = fixture.scan(expected - 1).remove(0);
        assert_eq!(task.created_at, expected);
        assert_eq!(task.status, Status::Pending);
    }

    #[test]
    fn failed_ack_retains_original_request_for_explicit_retry() {
        let fixture = Fixture::new();
        let original = generate("error", "Change icons", 110);
        fixture.journal("error", &[(110, original.clone())]);
        let task = fixture.scan(100).remove(0);
        transition(
            &fixture.root,
            &fixture.home,
            &task.request_id,
            Status::Running,
            None,
            Some("turn-1"),
        )
        .unwrap();
        fixture.journal("error",&[(110,original),(120,json!({"type":"agent_error","id":"error","sourceEventType":"generate","message":"Provider failed"}))]);
        transition(
            &fixture.root,
            &fixture.home,
            &task.request_id,
            Status::Blocked,
            None,
            None,
        )
        .unwrap();
        let task = fixture.scan(100).remove(0);
        assert_eq!(task.status, Status::Blocked);
        assert_eq!(task.event["freeformPrompt"], "Change icons");
        assert_eq!(task.event["_jarvisRecovery"], true);
        let task = resume(
            &fixture.root,
            &fixture.home,
            &task.request_id,
            task.revision,
        )
        .unwrap();
        assert_eq!(task.status, Status::Pending);
        assert!(task.turn_id.is_none());
        assert_eq!(fixture.scan(100)[0].status, Status::Pending);
    }

    #[test]
    fn corrupted_journal_preserves_the_private_queue() {
        let fixture = Fixture::new();
        let journal = fixture.journal("broken", &[(110, generate("broken", "Change icons", 110))]);
        let task = fixture.scan(100).remove(0);
        let directory = directory(&fixture.home).unwrap();
        let path = path(&fixture.root.canonicalize().unwrap(), &directory);
        let original = fs::read(&path).unwrap();
        fs::write(journal, "{invalid json").unwrap();
        assert!(scan(
            &fixture.root,
            &fixture.home,
            ScanPolicy {
                activation_cutoff_ms: 100,
                recover_action_id: None
            }
        )
        .is_err());
        assert_eq!(fs::read(path).unwrap(), original);
        assert_eq!(task.status, Status::Pending);
    }

    #[test]
    fn outside_source_paths_fail_without_writing_the_queue() {
        let fixture = Fixture::new();
        let mut event = generate("outside", "Change icons", 110);
        event["scaffold"] = json!({"file":"../private.html"});
        fixture.journal("outside", &[(110, event)]);
        assert!(scan(
            &fixture.root,
            &fixture.home,
            ScanPolicy {
                activation_cutoff_ms: 100,
                recover_action_id: None
            }
        )
        .is_err());
    }

    #[cfg(unix)]
    #[test]
    fn journal_symlinks_are_rejected_and_queue_permissions_are_private() {
        use std::os::unix::fs::{symlink, PermissionsExt};
        let fixture = Fixture::new();
        let journal = fixture.journal(
            "private",
            &[(110, generate("private", "Change icons", 110))],
        );
        fixture.scan(100);
        let directory = directory(&fixture.home).unwrap();
        let path = path(&fixture.root.canonicalize().unwrap(), &directory);
        assert_eq!(
            fs::metadata(path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        let outside = fixture.home.join("outside.jsonl");
        fs::copy(&journal, &outside).unwrap();
        fs::remove_file(&journal).unwrap();
        symlink(outside, journal).unwrap();
        assert!(scan(
            &fixture.root,
            &fixture.home,
            ScanPolicy {
                activation_cutoff_ms: 100,
                recover_action_id: None
            }
        )
        .is_err());
    }
    #[test]
    fn cancelled_offline_request_cleans_only_its_exact_pending_after_restart() {
        let fixture = Fixture::new();
        let event = generate("cancelled-off", "Move badge", 110);
        fixture.journal("cancelled-off", &[(110, event.clone())]);
        let task = fixture.scan(100).remove(0);
        let cancelled = remove(
            &fixture.root,
            &fixture.home,
            &task.request_id,
            task.revision,
        )
        .unwrap();
        assert!(has_current_pending(&fixture.root, &cancelled).unwrap());
        assert_eq!(fixture.scan(200).remove(0).status, Status::Cancelled);
        fixture.journal(
            "cancelled-off",
            &[
                (110, event.clone()),
                (
                    120,
                    json!({"id":"cancelled-off","type":"agent_error","message":"Removed"}),
                ),
            ],
        );
        assert!(!has_current_pending(&fixture.root, &cancelled).unwrap());
        // Same action reused for a fresh request must survive the old cleanup.
        fixture.journal(
            "cancelled-off",
            &[
                (110, event),
                (120, generate("cancelled-off", "Adjust icons", 120)),
            ],
        );
        assert!(!has_current_pending(&fixture.root, &cancelled).unwrap());
    }

    #[test]
    fn cancelled_generation_cannot_clear_newer_acceptance_phase_of_same_request() {
        let fixture = Fixture::new();
        let event = generate("phase-change", "Move badge", 110);
        fixture.journal("phase-change", &[(110, event.clone())]);
        let task = fixture.scan(100).remove(0);
        let cancelled = remove(
            &fixture.root,
            &fixture.home,
            &task.request_id,
            task.revision,
        )
        .unwrap();
        fixture.journal(
            "phase-change",
            &[
                (110, event),
                (
                    120,
                    json!({"id":"phase-change","type":"accept","variantId":"variant-2"}),
                ),
            ],
        );
        assert!(is_current(&fixture.root, &cancelled).unwrap());
        assert!(!has_current_pending(&fixture.root, &cancelled).unwrap());
    }
}
