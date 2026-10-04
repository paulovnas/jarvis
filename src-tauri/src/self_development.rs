//! Explicit, local consent for developing Jarvis with sanitized incident data.
//! Neither project files nor model instructions can grant this permission.

use crate::{agent::AgentState, persistence::AppState};
use regex::Regex;
use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    process::Stdio,
    sync::{Mutex, OnceLock},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tauri::{Emitter, Manager};

const SCHEMA: u8 = 1;
const MAX_BINDINGS: usize = 64;
const MAX_INCIDENTS: usize = 20;
const MAX_EVENTS: usize = 256;
const MAX_PAGE: usize = 50;
const MAX_FILE_BYTES: u64 = 512 * 1024;
const RETENTION_MS: u64 = 30 * 24 * 60 * 60 * 1_000;
static STORAGE_LOCK: Mutex<()> = Mutex::new(());

#[derive(Debug, Clone, Serialize)]
pub(crate) struct SelfDevelopmentError {
    pub(crate) code: &'static str,
    pub(crate) message: &'static str,
}

fn denied() -> SelfDevelopmentError {
    SelfDevelopmentError {
        code: "self_development_denied",
        message: "O autodesenvolvimento não está ativo neste projeto Jarvis.",
    }
}

fn storage() -> SelfDevelopmentError {
    SelfDevelopmentError {
        code: "self_development_storage",
        message: "Não foi possível acessar os incidentes locais de autodesenvolvimento.",
    }
}

impl From<crate::persistence::PersistenceError> for SelfDevelopmentError {
    fn from(_: crate::persistence::PersistenceError) -> Self {
        storage()
    }
}
impl From<rusqlite::Error> for SelfDevelopmentError {
    fn from(_: rusqlite::Error) -> Self {
        storage()
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SelfDevelopmentStatus {
    project_id: String,
    eligible: bool,
    enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IncidentSource {
    id: String,
    project_id: String,
    project_name: String,
    title: String,
    status: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct IncidentSummary {
    id: String,
    captured_at: u64,
    conversation_title: String,
    source_project_name: String,
    source_status: String,
    event_count: usize,
    truncated: bool,
    reference: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RepositoryIdentity {
    root: PathBuf,
    common_dir: PathBuf,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Authorization {
    schema_version: u8,
    pub(crate) project_id: String,
    identity: RepositoryIdentity,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum EventKind {
    Turn,
    Tool,
    Compaction,
    Subagent,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum EventStatus {
    Running,
    Completed,
    Cancelled,
    Error,
    Interrupted,
    Pending,
    Queued,
    Waiting,
    Unknown,
}

impl EventStatus {
    fn parse(value: &str) -> Self {
        match value {
            "running" | "in_progress" => Self::Running,
            "completed" | "succeeded" => Self::Completed,
            "cancelled" => Self::Cancelled,
            "error" | "failed" | "blocked" => Self::Error,
            "interrupted" => Self::Interrupted,
            "pending" => Self::Pending,
            "queued" => Self::Queued,
            "waiting" => Self::Waiting,
            _ => Self::Unknown,
        }
    }
    fn label(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Cancelled => "cancelled",
            Self::Error => "error",
            Self::Interrupted => "interrupted",
            Self::Pending => "pending",
            Self::Queued => "queued",
            Self::Waiting => "waiting",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum ShapeKind {
    Null,
    Boolean,
    Number,
    String,
    Array,
    Object,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Shape {
    kind: ShapeKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    size: Option<usize>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    children: Vec<Shape>,
}

fn shape(value: &Value, depth: usize) -> Shape {
    let kind = match value {
        Value::Null => ShapeKind::Null,
        Value::Bool(_) => ShapeKind::Boolean,
        Value::Number(_) => ShapeKind::Number,
        Value::String(_) => ShapeKind::String,
        Value::Array(_) => ShapeKind::Array,
        Value::Object(_) => ShapeKind::Object,
    };
    let size = match value {
        Value::String(value) => Some(value.len()),
        Value::Array(value) => Some(value.len()),
        Value::Object(value) => Some(value.len()),
        _ => None,
    };
    let children = if depth >= 2 {
        vec![]
    } else {
        match value {
            Value::Array(values) => values.iter().take(4).map(|v| shape(v, depth + 1)).collect(),
            Value::Object(values) => values
                .values()
                .take(4)
                .map(|v| shape(v, depth + 1))
                .collect(),
            _ => vec![],
        }
    };
    Shape {
        kind,
        size,
        children,
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct IncidentEvent {
    kind: EventKind,
    correlation_id: String,
    status: EventStatus,
    duration_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    model_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    executor: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    provider_kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    parent_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reasoning: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    arguments: Option<Shape>,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Shape>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Incident {
    schema_version: u8,
    authorization: Authorization,
    summary: IncidentSummary,
    app_version: String,
    os: String,
    profile: String,
    description: Option<String>,
    events: Vec<IncidentEvent>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct IncidentPage {
    summary: IncidentSummary,
    app_version: String,
    os: String,
    profile: String,
    description: Option<String>,
    start: usize,
    total: usize,
    next_start: Option<usize>,
    events: Vec<IncidentEvent>,
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

fn valid_id(value: &str) -> bool {
    value.len() == 32
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

fn hash(value: &[u8]) -> String {
    Sha256::digest(value)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn profile_label() -> &'static str {
    match crate::data_dir::profile() {
        crate::data_dir::Profile::Production => "production",
        crate::data_dir::Profile::Development => "development",
    }
}

/// Use a bounded local Git read; never authenticate, fetch or invoke a shell.
fn git(root: &Path, args: &[&str]) -> Option<String> {
    let mut child = crate::background::command("git")
        .args(["--no-optional-locks", "-C"])
        .arg(root)
        .args(args)
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_COMMON_DIR")
        .env_remove("GIT_CONFIG")
        .env_remove("GIT_CONFIG_COUNT")
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let started = Instant::now();
    loop {
        match child.try_wait().ok()? {
            Some(status) => {
                if !status.success() {
                    return None;
                }
                let mut output = String::new();
                child
                    .stdout
                    .take()?
                    .take(16 * 1024)
                    .read_to_string(&mut output)
                    .ok()?;
                return Some(output.trim().to_owned());
            }
            None if started.elapsed() < Duration::from_secs(2) => {
                std::thread::sleep(Duration::from_millis(5))
            }
            None => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
}

fn official_origin(origin: &str) -> bool {
    let origin = origin.trim().trim_end_matches('/');
    let Some(path) = origin
        .strip_prefix("git@github.com:")
        .or_else(|| origin.strip_prefix("https://github.com/"))
        .or_else(|| origin.strip_prefix("ssh://git@github.com/"))
    else {
        return false;
    };
    matches!(path, "paulovnas/jarvis" | "paulovnas/jarvis.git")
}

fn manifest(path: &Path) -> Option<Vec<u8>> {
    let metadata = fs::symlink_metadata(path).ok()?;
    if !metadata.is_file()
        || metadata.is_symlink()
        || metadata.len() > 128 * 1024
        || fs::canonicalize(path).ok()?.as_path() != path
    {
        return None;
    }
    fs::read(path).ok()
}

fn repository_identity(requested: &Path) -> Option<RepositoryIdentity> {
    let root = fs::canonicalize(requested).ok()?;
    let git_root = fs::canonicalize(git(&root, &["rev-parse", "--show-toplevel"])?).ok()?;
    if root != git_root {
        return None;
    }
    let common_dir = fs::canonicalize(git(
        &root,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    )?)
    .ok()?;
    for origins in [
        git(
            &root,
            &["config", "--local", "--get-all", "remote.origin.url"],
        ),
        git(&root, &["remote", "get-url", "--all", "origin"]),
    ] {
        let origins = origins?;
        if origins.is_empty() || !origins.lines().all(official_origin) {
            return None;
        }
    }
    let package: Value = serde_json::from_slice(&manifest(&root.join("package.json"))?).ok()?;
    let native: Value =
        serde_json::from_slice(&manifest(&root.join("src-tauri/tauri.conf.json"))?).ok()?;
    if package["name"] != "jarvis" || native["identifier"] != crate::data_dir::PRODUCTION_IDENTIFIER
    {
        return None;
    }
    // Cargo package identity is stable across version/display-title changes.
    let cargo = String::from_utf8(manifest(&root.join("src-tauri/Cargo.toml"))?).ok()?;
    let mut in_package = false;
    let mut cargo_name = None;
    for line in cargo.lines() {
        let line = line.split('#').next()?.trim();
        if line.starts_with('[') {
            in_package = line == "[package]";
            continue;
        }
        if in_package {
            if let Some((key, value)) = line.split_once('=') {
                if key.trim() == "name" {
                    cargo_name = Some(value.trim().trim_matches(['\"', '\'']));
                }
            }
        }
    }
    if cargo_name != Some("jarvis") {
        return None;
    }
    Some(RepositoryIdentity { root, common_dir })
}

fn directory(home: &Path, create: bool) -> Result<PathBuf, SelfDevelopmentError> {
    let root = crate::data_dir::root(home);
    let directory = root.join("self-development");
    for path in [&root, &directory] {
        if create && !path.exists() {
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt;
                fs::DirBuilder::new()
                    .mode(0o700)
                    .create(path)
                    .map_err(|_| storage())?;
            }
            #[cfg(not(unix))]
            fs::create_dir(path).map_err(|_| storage())?;
        }
        match fs::symlink_metadata(path) {
            Ok(metadata) if metadata.is_dir() && !metadata.is_symlink() => {
                let canonical_home = fs::canonicalize(home).map_err(|_| storage())?;
                let expected = crate::data_dir::root(&canonical_home);
                let expected = if *path == directory {
                    expected.join("self-development")
                } else {
                    expected
                };
                if fs::canonicalize(path).map_err(|_| storage())? != expected {
                    return Err(storage());
                }
            }
            Err(error) if !create && error.kind() == std::io::ErrorKind::NotFound => {}
            _ => return Err(storage()),
        }
    }
    Ok(directory)
}

fn binding_key(authorization: &Authorization) -> Result<String, SelfDevelopmentError> {
    serde_json::to_vec(&(authorization.project_id.as_str(), &authorization.identity))
        .map(|bytes| hash(&bytes))
        .map_err(|_| storage())
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, SelfDevelopmentError> {
    let metadata = fs::symlink_metadata(path).map_err(|_| storage())?;
    if !metadata.is_file() || metadata.is_symlink() || metadata.len() > MAX_FILE_BYTES {
        return Err(storage());
    }
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.custom_flags(windows_sys::Win32::Storage::FileSystem::FILE_FLAG_OPEN_REPARSE_POINT);
    }
    let file = options.open(path).map_err(|_| storage())?;
    let opened = file.metadata().map_err(|_| storage())?;
    if !opened.is_file() || opened.len() > MAX_FILE_BYTES {
        return Err(storage());
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if opened.file_attributes()
            & windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT
            != 0
        {
            return Err(storage());
        }
    }
    let mut bytes = vec![];
    file.take(MAX_FILE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| storage())?;
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err(storage());
    }
    serde_json::from_slice(&bytes).map_err(|_| storage())
}

fn save_json<T: Serialize>(path: &Path, value: &T) -> Result<(), SelfDevelopmentError> {
    let bytes = serde_json::to_vec(value).map_err(|_| storage())?;
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err(storage());
    }
    let directory = path.parent().ok_or_else(storage)?;
    let mut temporary = tempfile::NamedTempFile::new_in(directory).map_err(|_| storage())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        temporary
            .as_file()
            .set_permissions(fs::Permissions::from_mode(0o600))
            .map_err(|_| storage())?;
    }
    temporary
        .write_all(&bytes)
        .and_then(|()| temporary.as_file().sync_all())
        .map_err(|_| storage())?;
    temporary.persist(path).map_err(|_| storage())?;
    #[cfg(unix)]
    fs::File::open(directory)
        .and_then(|file| file.sync_all())
        .map_err(|_| storage())?;
    Ok(())
}

fn authorizations(home: &Path) -> Result<Vec<Authorization>, SelfDevelopmentError> {
    let directory = directory(home, false)?;
    if !directory.exists() {
        return Ok(vec![]);
    }
    let mut records = vec![];
    let entries = fs::read_dir(directory).map_err(|_| storage())?;
    for entry in entries.take(MAX_BINDINGS * (MAX_INCIDENTS + 2) + 1) {
        let path = entry.map_err(|_| storage())?.path();
        let Some(name) = path.file_name().and_then(|v| v.to_str()) else {
            continue;
        };
        if !name.ends_with(".authorization.json") {
            continue;
        }
        let authorization: Authorization = read_json(&path)?;
        if authorization.schema_version != SCHEMA
            || !valid_id(&authorization.project_id)
            || name != format!("{}.authorization.json", binding_key(&authorization)?)
        {
            return Err(storage());
        }
        records.push(authorization);
        if records.len() > MAX_BINDINGS {
            return Err(storage());
        }
    }
    Ok(records)
}

fn authorization_for_project(
    state: &AppState,
    home: &Path,
    project_id: &str,
) -> Result<Authorization, SelfDevelopmentError> {
    if !valid_id(project_id) {
        return Err(denied());
    }
    let root = crate::library::project_directory(state, home, project_id).map_err(|_| denied())?;
    let identity = repository_identity(&root).ok_or_else(denied)?;
    Ok(Authorization {
        schema_version: SCHEMA,
        project_id: project_id.to_owned(),
        identity,
    })
}

fn authorized(home: &Path, expected: &Authorization) -> Result<(), SelfDevelopmentError> {
    let key = binding_key(expected)?;
    let stored: Authorization =
        read_json(&directory(home, false)?.join(format!("{key}.authorization.json")))
            .map_err(|_| denied())?;
    if stored.schema_version != SCHEMA
        || stored.project_id != expected.project_id
        || stored.identity != expected.identity
    {
        return Err(denied());
    }
    Ok(())
}

pub(crate) fn require_for_root(
    home: &Path,
    root: &Path,
    project_id: &str,
) -> Result<Authorization, SelfDevelopmentError> {
    if !valid_id(project_id) {
        return Err(denied());
    }
    // Normal projects/default OFF return before starting a Git process.
    let candidate = authorizations(home)?
        .into_iter()
        .find(|item| item.project_id == project_id)
        .ok_or_else(denied)?;
    let canonical_root = fs::canonicalize(root).map_err(|_| denied())?;
    if candidate.identity.root != canonical_root {
        return Err(denied());
    }
    let identity = repository_identity(&canonical_root).ok_or_else(denied)?;
    if candidate.identity != identity {
        return Err(denied());
    }
    Ok(candidate)
}

pub(crate) fn enabled_for_root(home: &Path, root: &Path, project_id: &str) -> bool {
    require_for_root(home, root, project_id).is_ok()
}

fn status(
    state: &AppState,
    home: &Path,
    project_id: &str,
) -> Result<SelfDevelopmentStatus, SelfDevelopmentError> {
    let authorization = authorization_for_project(state, home, project_id);
    let (eligible, enabled) = match authorization {
        Ok(authorization) => (true, authorized(home, &authorization).is_ok()),
        Err(_) => (false, false),
    };
    Ok(SelfDevelopmentStatus {
        project_id: project_id.to_owned(),
        eligible,
        enabled,
        reason: (!eligible).then(|| {
            "Disponível somente no repositório oficial do Jarvis, na raiz do projeto.".to_owned()
        }),
    })
}

fn set_enabled(
    state: &AppState,
    home: &Path,
    project_id: &str,
    enabled: bool,
) -> Result<SelfDevelopmentStatus, SelfDevelopmentError> {
    let _lock = STORAGE_LOCK.lock().map_err(|_| storage())?;
    if enabled {
        let authorization = authorization_for_project(state, home, project_id)?;
        let records = authorizations(home)?;
        if records.len() >= MAX_BINDINGS
            && !records.iter().any(|item| item.project_id == project_id)
        {
            return Err(storage());
        }
        let key = binding_key(&authorization)?;
        // A registration can move to another checkout. Its old consent and
        // incident snapshots must not survive or shadow the newly selected root.
        for previous in records
            .into_iter()
            .filter(|item| item.project_id == project_id)
        {
            let previous_key = binding_key(&previous)?;
            if previous_key == key {
                continue;
            }
            fs::remove_file(
                directory(home, false)?.join(format!("{previous_key}.authorization.json")),
            )
            .map_err(|_| storage())?;
            for path in incident_paths(home, &previous)? {
                fs::remove_file(path).map_err(|_| storage())?;
            }
        }
        save_json(
            &directory(home, true)?.join(format!("{key}.authorization.json")),
            &authorization,
        )?;
    } else {
        // Revocation remains possible after a checkout disappears or changes origin.
        for authorization in authorizations(home)?
            .into_iter()
            .filter(|item| item.project_id == project_id)
        {
            let key = binding_key(&authorization)?;
            let directory = directory(home, false)?;
            fs::remove_file(directory.join(format!("{key}.authorization.json")))
                .map_err(|_| storage())?;
            for path in incident_paths(home, &authorization)? {
                fs::remove_file(path).map_err(|_| storage())?;
            }
        }
    }
    status(state, home, project_id)
}

fn sanitize_text(value: &str, limit: usize) -> String {
    static REDACTIONS: OnceLock<Vec<Regex>> = OnceLock::new();
    let redactions = REDACTIONS.get_or_init(|| [
        r"(?i)(?:bearer\s+\S+|(?:api[_-]?key|token|secret|password|senha|authorization)\s*[:=]\s*\S+)",
        r"\b(?:sk-[A-Za-z0-9_-]+|gh[pousr]_[A-Za-z0-9_]+|github_pat_[A-Za-z0-9_]+|AKIA[A-Z0-9]{16})\b",
        r"(?i)\b(?:https?|ssh|postgres|postgresql|mysql)://\S+",
        r"[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}",
        r"\b[A-Za-z0-9_+/=-]{32,}\b",
    ].into_iter().map(|pattern| Regex::new(pattern).expect("static incident redaction regex")).collect());
    let mut sanitized: String = value
        .chars()
        .take(4_000)
        .filter(|c| !c.is_control() || c.is_whitespace())
        .collect();
    for regex in redactions {
        sanitized = regex.replace_all(&sanitized, "[redigido]").into_owned();
    }
    sanitized
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(limit)
        .collect()
}

fn sources(
    state: &AppState,
    agent: &AgentState,
    home: &Path,
    project_id: &str,
) -> Result<Vec<IncidentSource>, SelfDevelopmentError> {
    let authorization = authorization_for_project(state, home, project_id)?;
    authorized(home, &authorization)?;
    state.with_connection(home, |connection| {
        let mut query = connection.prepare("SELECT c.id, c.project_id, p.name, COALESCE(c.display_title,c.title) FROM conversations c JOIN projects p ON p.id=c.project_id ORDER BY COALESCE(c.last_activity_at,c.created_at) DESC LIMIT 200")?;
        let rows = query.query_map([], |row| Ok(IncidentSource {
            id: row.get(0)?, project_id: row.get(1)?, project_name: row.get(2)?, title: row.get(3)?, status: "saved".into(),
        }))?.collect::<Result<Vec<_>, _>>()?;
        Ok(rows.into_iter().map(|mut source| {
            source.title = sanitize_text(&source.title, 120);
            source.project_name = sanitize_text(&source.project_name, 120);
            if let Some(status) = agent.self_development_source_status(&source.id) { source.status = EventStatus::parse(&status).label().into(); }
            source
        }).collect())
    })
}

fn source(state: &AppState, home: &Path, id: &str) -> Result<IncidentSource, SelfDevelopmentError> {
    if !valid_id(id) {
        return Err(denied());
    }
    state.with_connection(home, |connection| {
        connection.query_row("SELECT c.id,c.project_id,p.name,COALESCE(c.display_title,c.title) FROM conversations c JOIN projects p ON p.id=c.project_id WHERE c.id=?1", [id], |row| Ok(IncidentSource {
            id: row.get(0)?, project_id: row.get(1)?, project_name: row.get(2)?, title: row.get(3)?, status: "unknown".into(),
        })).optional()?.ok_or_else(denied)
    })
}

fn limited_choice(value: &Value, choices: &[&str]) -> Option<String> {
    value
        .as_str()
        .filter(|value| choices.contains(value))
        .map(str::to_owned)
}

const PUBLIC_ERROR_CODES: &[&str] = &[
    "interrupted",
    "cancelled",
    "provider_error",
    "storage",
    "workflow_error",
    "tool_execution",
    "tool_arguments",
    "execution_denied",
    "provider_auth",
    "provider_limit",
    "provider_output_limit",
    "context_overflow",
    "provider_network",
    "provider_transport_interrupted",
    "provider_timeout",
    "provider_unavailable",
    "provider_failed",
    "provider_protocol",
    "provider_incomplete",
    "provider_retry_exhausted",
    "provider_request",
    "invalid_message",
    "context_invalid",
    "context_limit",
    "invalid_tool_arguments",
    "session_storage",
    "denied",
    "approval_denied",
    "policy_denied",
    "provider_tools_unsupported",
    "provider_images_unsupported",
    "provider_reasoning_unsupported",
    "unknown",
];
const PROVIDERS: &[&str] = &[
    "openai-codex",
    "antigravity",
    "custom",
    "claude-code",
    "opencode-go",
    "unavailable",
];
const NATIVE_TOOLS: &[&str] = &[
    "read",
    "glob",
    "grep",
    "write",
    "apply_patch",
    "bash",
    "run_shell",
    "exec_command",
    "write_stdin",
    "browser",
    "web_search",
    "spawn_agent",
    "send_message",
    "wait",
    "update_tasks",
    "request_user_input",
    "external",
];

fn events(snapshot: &Value) -> (Vec<IncidentEvent>, bool, String) {
    let mut events = vec![];
    let mut omitted = false;
    let latest_status = snapshot["turns"]
        .as_array()
        .and_then(|turns| turns.last())
        .map_or("unknown", |turn| {
            EventStatus::parse(turn["status"].as_str().unwrap_or("")).label()
        })
        .to_owned();
    let duration = |value: &Value| value.as_u64().unwrap_or(0).min(24 * 60 * 60 * 1_000);
    for turn in snapshot["turns"].as_array().into_iter().flatten() {
        events.push(IncidentEvent {
            kind: EventKind::Turn,
            correlation_id: crate::agent::telemetry::context_id(turn["id"].as_str().unwrap_or("")),
            status: EventStatus::parse(turn["status"].as_str().unwrap_or("")),
            duration_ms: duration(&turn["durationMs"]),
            model_id: turn["options"]["model"]
                .as_str()
                .map(crate::agent::telemetry::model_id),
            executor: Some(
                limited_choice(
                    &turn["options"]["executor"],
                    &["jarvis", "claude", "unavailable"],
                )
                .unwrap_or_else(|| "jarvis".into()),
            ),
            provider_kind: limited_choice(&turn["options"]["providerKind"], PROVIDERS),
            parent_id: None,
            reasoning: limited_choice(
                &turn["options"]["reasoning"],
                &[
                    "none", "minimal", "low", "medium", "high", "xhigh", "max", "ultra",
                ],
            ),
            error_code: turn["error"]["code"].as_str().map(|code| {
                if PUBLIC_ERROR_CODES.contains(&code) {
                    code.into()
                } else {
                    "unknown".into()
                }
            }),
            tool_name: None,
            tool_id: None,
            arguments: None,
            result: None,
        });
        for tool in turn["steps"]
            .as_array()
            .into_iter()
            .flatten()
            .flat_map(|step| step["tools"].as_array().into_iter().flatten())
        {
            let name = tool["name"].as_str().unwrap_or("");
            let tool_name = if NATIVE_TOOLS.contains(&name) {
                name
            } else {
                "external"
            };
            let output = tool["output"].as_str().unwrap_or("");
            let result = if output.len() <= 128 * 1024 {
                serde_json::from_str::<Value>(output)
                    .ok()
                    .map(|v| shape(&v, 0))
            } else {
                None
            }
            .unwrap_or(Shape {
                kind: ShapeKind::String,
                size: Some(output.len()),
                children: vec![],
            });
            events.push(IncidentEvent {
                kind: EventKind::Tool,
                correlation_id: crate::agent::telemetry::context_id(
                    tool["id"].as_str().unwrap_or(""),
                ),
                status: EventStatus::parse(tool["status"].as_str().unwrap_or("")),
                duration_ms: duration(&tool["durationMs"]),
                model_id: None,
                executor: None,
                provider_kind: None,
                parent_id: None,
                reasoning: None,
                error_code: None,
                tool_name: Some(tool_name.into()),
                tool_id: Some(crate::agent::telemetry::tool_id(name)),
                arguments: Some(shape(&tool["args"], 0)),
                result: Some(result),
            });
            if events.len() >= MAX_EVENTS {
                omitted = true;
                break;
            }
        }
        if events.len() >= MAX_EVENTS {
            omitted = true;
            break;
        }
    }
    let compaction_count = snapshot["compactions"].as_array().map_or(0, Vec::len);
    omitted |= compaction_count > MAX_EVENTS.saturating_sub(events.len());
    for compaction in snapshot["compactions"]
        .as_array()
        .into_iter()
        .flatten()
        .take(MAX_EVENTS.saturating_sub(events.len()))
    {
        events.push(IncidentEvent {
            kind: EventKind::Compaction,
            correlation_id: crate::agent::telemetry::context_id(
                compaction["turnId"].as_str().unwrap_or(""),
            ),
            status: EventStatus::Completed,
            duration_ms: duration(&compaction["durationMs"]),
            model_id: None,
            executor: None,
            provider_kind: None,
            parent_id: None,
            reasoning: None,
            error_code: None,
            tool_name: None,
            tool_id: None,
            arguments: None,
            result: None,
        });
    }
    let subagent_count = snapshot["subagents"].as_array().map_or(0, Vec::len);
    omitted |= subagent_count > MAX_EVENTS.saturating_sub(events.len());
    for subagent in snapshot["subagents"]
        .as_array()
        .into_iter()
        .flatten()
        .take(MAX_EVENTS.saturating_sub(events.len()))
    {
        events.push(IncidentEvent {
            kind: EventKind::Subagent,
            correlation_id: subagent["id"].as_str().unwrap_or("").into(),
            parent_id: subagent["parentId"].as_str().map(str::to_owned),
            status: EventStatus::parse(subagent["status"].as_str().unwrap_or("")),
            duration_ms: duration(&subagent["durationMs"]),
            model_id: subagent["options"]["model"]
                .as_str()
                .map(crate::agent::telemetry::model_id),
            executor: limited_choice(
                &subagent["options"]["executor"],
                &["jarvis", "claude", "unavailable"],
            ),
            provider_kind: None,
            reasoning: limited_choice(
                &subagent["options"]["reasoning"],
                &[
                    "none", "minimal", "low", "medium", "high", "xhigh", "max", "ultra",
                ],
            ),
            error_code: None,
            tool_name: None,
            tool_id: None,
            arguments: None,
            result: None,
        });
    }
    let total = snapshot["history"]["total"].as_u64().unwrap_or(0);
    let shown = snapshot["turns"].as_array().map_or(0, |turns| turns.len()) as u64;
    (events, omitted || total > shown, latest_status)
}

fn incident_paths(
    home: &Path,
    authorization: &Authorization,
) -> Result<Vec<PathBuf>, SelfDevelopmentError> {
    let directory = directory(home, false)?;
    if !directory.exists() {
        return Ok(vec![]);
    }
    let prefix = format!("{}.", binding_key(authorization)?);
    let mut paths = vec![];
    for entry in fs::read_dir(directory)
        .map_err(|_| storage())?
        .take(MAX_BINDINGS * (MAX_INCIDENTS + 2) + 1)
    {
        let path = entry.map_err(|_| storage())?.path();
        let Some(name) = path.file_name().and_then(|v| v.to_str()) else {
            continue;
        };
        if name.starts_with(&prefix) && name.ends_with(".incident.json") {
            paths.push(path);
        }
    }
    Ok(paths)
}

fn incident_path(
    home: &Path,
    authorization: &Authorization,
    id: &str,
) -> Result<PathBuf, SelfDevelopmentError> {
    if !valid_id(id) {
        return Err(denied());
    }
    Ok(directory(home, false)?.join(format!(
        "{}.{id}.incident.json",
        binding_key(authorization)?
    )))
}

fn read_incident(
    home: &Path,
    authorization: &Authorization,
    path: &Path,
) -> Result<Incident, SelfDevelopmentError> {
    let mut incident: Incident = read_json(path).map_err(|_| denied())?;
    if incident.schema_version != SCHEMA
        || incident.authorization.schema_version != SCHEMA
        || incident.authorization.project_id != authorization.project_id
        || incident.authorization.identity != authorization.identity
        || !valid_id(&incident.summary.id)
        || path != incident_path(home, authorization, &incident.summary.id)?
        || now().saturating_sub(incident.summary.captured_at) > RETENTION_MS
        || incident.events.len() > MAX_EVENTS
        || incident.summary.event_count != incident.events.len()
    {
        return Err(denied());
    }
    // Reapply redaction at the trust boundary, even to privately persisted text.
    incident.summary.conversation_title = sanitize_text(&incident.summary.conversation_title, 120);
    incident.summary.source_project_name =
        sanitize_text(&incident.summary.source_project_name, 120);
    incident.description = incident.description.map(|text| sanitize_text(&text, 800));
    incident.summary.reference = reference(&incident.summary.id);
    incident.summary.source_status = EventStatus::parse(&incident.summary.source_status)
        .label()
        .into();
    for event in &mut incident.events {
        if !valid_correlation(&event.correlation_id)
            || event
                .model_id
                .as_ref()
                .is_some_and(|id| !valid_correlation(id))
            || event
                .tool_id
                .as_ref()
                .is_some_and(|id| !valid_correlation(id))
            || event
                .parent_id
                .as_ref()
                .is_some_and(|id| !valid_correlation(id))
        {
            return Err(denied());
        }
        event.tool_name = event.tool_name.take().map(|name| {
            if NATIVE_TOOLS.contains(&name.as_str()) {
                name
            } else {
                "external".into()
            }
        });
        event.error_code = event.error_code.take().map(|code| {
            if PUBLIC_ERROR_CODES.contains(&code.as_str()) {
                code
            } else {
                "unknown".into()
            }
        });
        event.executor =
            limited_choice(&json!(event.executor), &["jarvis", "claude", "unavailable"]);
        event.provider_kind = limited_choice(&json!(event.provider_kind), PROVIDERS);
        event.reasoning = limited_choice(
            &json!(event.reasoning),
            &[
                "none", "minimal", "low", "medium", "high", "xhigh", "max", "ultra",
            ],
        );
    }
    if semver::Version::parse(&incident.app_version).is_err()
        || !["linux", "macos", "windows", "android", "ios"].contains(&incident.os.as_str())
        || !["production", "development"].contains(&incident.profile.as_str())
    {
        return Err(denied());
    }
    Ok(incident)
}

fn valid_correlation(value: &str) -> bool {
    value.strip_prefix("sha256:").is_some_and(|digest| {
        digest.len() == 24
            && digest
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    })
}

fn reference(id: &str) -> String {
    format!("Investigue o incidente de autodesenvolvimento {id} com as ferramentas de diagnóstico do Jarvis. Preserve a intenção atual e os resultados confirmados; confira o estado real antes de repetir qualquer ação.")
}

fn capture(
    state: &AppState,
    agent: &AgentState,
    home: &Path,
    project_id: &str,
    conversation_id: &str,
    description: Option<&str>,
) -> Result<IncidentSummary, SelfDevelopmentError> {
    let _lock = STORAGE_LOCK.lock().map_err(|_| storage())?;
    let authorization = authorization_for_project(state, home, project_id)?;
    authorized(home, &authorization)?;
    let source = source(state, home, conversation_id)?;
    let mut snapshot = agent
        .self_development_snapshot(state, home, conversation_id)
        .map_err(|_| storage())?;
    state.with_connection(home, |connection| {
        for turn in snapshot["turns"].as_array_mut().into_iter().flatten() {
            let provider = if turn["options"]["executor"] == "claude" {
                "claude-code".to_owned()
            } else {
                connection
                    .query_row(
                        "SELECT provider_kind FROM provider_accounts WHERE alias=?1",
                        [turn["options"]["account"].as_str().unwrap_or("")],
                        |row| row.get::<_, String>(0),
                    )
                    .optional()?
                    .filter(|provider| PROVIDERS.contains(&provider.as_str()))
                    .unwrap_or_else(|| "unavailable".into())
            };
            turn["options"]["providerKind"] = json!(provider);
        }
        Ok::<_, SelfDevelopmentError>(())
    })?;
    let (events, truncated, source_status) = events(&snapshot);
    let id = crate::library::new_id().map_err(|_| storage())?;
    let summary = IncidentSummary {
        reference: reference(&id),
        id,
        captured_at: now(),
        conversation_title: sanitize_text(&source.title, 120),
        source_project_name: sanitize_text(&source.project_name, 120),
        source_status,
        event_count: events.len(),
        truncated,
    };
    let incident = Incident {
        schema_version: SCHEMA,
        authorization: authorization.clone(),
        summary: summary.clone(),
        app_version: env!("CARGO_PKG_VERSION").into(),
        os: std::env::consts::OS.into(),
        profile: profile_label().into(),
        description: description
            .map(|text| sanitize_text(text, 800))
            .filter(|text| !text.is_empty()),
        events,
    };
    // Revalidate after the read, before committing the user-selected snapshot.
    if repository_identity(&authorization.identity.root).as_ref() != Some(&authorization.identity) {
        return Err(denied());
    }
    authorized(home, &authorization)?;
    save_json(
        &incident_path(home, &authorization, &summary.id)?,
        &incident,
    )?;
    let mut retained = vec![];
    for path in incident_paths(home, &authorization)? {
        match read_incident(home, &authorization, &path) {
            Ok(incident) => retained.push((incident.summary.captured_at, path)),
            Err(_) => {
                fs::remove_file(path).map_err(|_| storage())?;
            }
        }
    }
    retained.sort_by_key(|entry| std::cmp::Reverse(entry.0));
    for (_, path) in retained.into_iter().skip(MAX_INCIDENTS) {
        fs::remove_file(path).map_err(|_| storage())?;
    }
    Ok(summary)
}

fn list_incidents(
    home: &Path,
    authorization: &Authorization,
) -> Result<Vec<IncidentSummary>, SelfDevelopmentError> {
    authorized(home, authorization)?;
    let mut summaries: Vec<_> = incident_paths(home, authorization)?
        .into_iter()
        .filter_map(|path| {
            read_incident(home, authorization, &path)
                .ok()
                .map(|incident| incident.summary)
        })
        .collect();
    summaries.sort_by_key(|entry| std::cmp::Reverse(entry.captured_at));
    summaries.truncate(MAX_INCIDENTS);
    Ok(summaries)
}

pub(crate) fn list_approved_incidents(
    home: &Path,
    root: &Path,
    project_id: &str,
) -> Result<Vec<IncidentSummary>, SelfDevelopmentError> {
    let _lock = STORAGE_LOCK.lock().map_err(|_| storage())?;
    let authorization = require_for_root(home, root, project_id)?;
    list_incidents(home, &authorization)
}

pub(crate) fn read_approved_incident(
    home: &Path,
    root: &Path,
    project_id: &str,
    incident_id: &str,
    start: usize,
    limit: usize,
) -> Result<IncidentPage, SelfDevelopmentError> {
    let _lock = STORAGE_LOCK.lock().map_err(|_| storage())?;
    let authorization = require_for_root(home, root, project_id)?;
    let incident = read_incident(
        home,
        &authorization,
        &incident_path(home, &authorization, incident_id)?,
    )?;
    if !(1..=MAX_PAGE).contains(&limit) || start > incident.events.len() {
        return Err(denied());
    }
    let total = incident.events.len();
    let end = start.saturating_add(limit).min(total);
    Ok(IncidentPage {
        summary: incident.summary,
        app_version: incident.app_version,
        os: incident.os,
        profile: incident.profile,
        description: incident.description,
        start,
        total,
        next_start: (end < total).then_some(end),
        events: incident.events[start..end].to_vec(),
    })
}

pub(crate) async fn read_summary_diagnostics(
    home: &Path,
    root: &Path,
    project_id: &str,
) -> Result<Value, SelfDevelopmentError> {
    require_for_root(home, root, project_id)?;
    let mut result = json!({ "schemaVersion": SCHEMA, "profile": profile_label() });
    if let Ok(summary) = crate::diagnostics::active_summary() {
        let summary = serde_json::to_value(summary).map_err(|_| storage())?;
        for field in [
            "appVersion",
            "os",
            "arch",
            "logFiles",
            "logBytes",
            "eventCount",
        ] {
            if let Some(value) = summary.get(field) {
                result[field] = value.clone();
            }
        }
    }
    if let Ok(report) = crate::agent::telemetry::get_harness_report().await {
        result["harness"] = serde_json::to_value(report).map_err(|_| storage())?;
    }
    require_for_root(home, root, project_id)?;
    Ok(result)
}

fn changed(app: &tauri::AppHandle, project_id: &str) {
    let _ = app.emit("self-development-changed", json!({"projectId": project_id}));
}

#[tauri::command]
pub async fn get_self_development_status(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    project_id: String,
) -> Result<SelfDevelopmentStatus, SelfDevelopmentError> {
    let home = app.path().home_dir().map_err(|_| storage())?;
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || status(&state, &home, &project_id))
        .await
        .map_err(|_| storage())?
}

#[tauri::command]
pub async fn set_self_development_enabled(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    project_id: String,
    enabled: bool,
) -> Result<SelfDevelopmentStatus, SelfDevelopmentError> {
    let home = app.path().home_dir().map_err(|_| storage())?;
    let state = state.inner().clone();
    let changed_project = project_id.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        set_enabled(&state, &home, &project_id, enabled)
    })
    .await
    .map_err(|_| storage())??;
    changed(&app, &changed_project);
    Ok(result)
}

#[tauri::command]
pub async fn list_self_development_sources(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    agent: tauri::State<'_, AgentState>,
    project_id: String,
) -> Result<Vec<IncidentSource>, SelfDevelopmentError> {
    let home = app.path().home_dir().map_err(|_| storage())?;
    let state = state.inner().clone();
    let agent = agent.inner().clone();
    tauri::async_runtime::spawn_blocking(move || sources(&state, &agent, &home, &project_id))
        .await
        .map_err(|_| storage())?
}

#[tauri::command]
pub async fn capture_self_development_incident(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    agent: tauri::State<'_, AgentState>,
    project_id: String,
    conversation_id: String,
    description: Option<String>,
) -> Result<IncidentSummary, SelfDevelopmentError> {
    let home = app.path().home_dir().map_err(|_| storage())?;
    let state = state.inner().clone();
    let agent = agent.inner().clone();
    let changed_project = project_id.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        capture(
            &state,
            &agent,
            &home,
            &project_id,
            &conversation_id,
            description.as_deref(),
        )
    })
    .await
    .map_err(|_| storage())??;
    changed(&app, &changed_project);
    Ok(result)
}

#[tauri::command]
pub async fn list_self_development_incidents(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    project_id: String,
) -> Result<Vec<IncidentSummary>, SelfDevelopmentError> {
    let home = app.path().home_dir().map_err(|_| storage())?;
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let authorization = authorization_for_project(&state, &home, &project_id)?;
        list_incidents(&home, &authorization)
    })
    .await
    .map_err(|_| storage())?
}

#[tauri::command]
pub async fn delete_self_development_incident(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    project_id: String,
    incident_id: String,
) -> Result<(), SelfDevelopmentError> {
    let home = app.path().home_dir().map_err(|_| storage())?;
    let state = state.inner().clone();
    let changed_project = project_id.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _lock = STORAGE_LOCK.lock().map_err(|_| storage())?;
        let authorization = authorization_for_project(&state, &home, &project_id)?;
        authorized(&home, &authorization)?;
        let path = incident_path(&home, &authorization, &incident_id)?;
        read_incident(&home, &authorization, &path)?;
        fs::remove_file(path).map_err(|_| storage())
    })
    .await
    .map_err(|_| storage())??;
    changed(&app, &changed_project);
    Ok(())
}

#[cfg(test)]
mod tests;
