//! Local, bounded diagnostics with an allowlisted schema.
//!
//! Diagnostics never contain prompts, model responses, request bodies, URLs or
//! credentials. Export reparses every JSONL line before adding it to the ZIP so
//! manually altered and future incompatible records are excluded by default.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    fs::{self, File, OpenOptions},
    io::{self, BufRead, BufReader, Write},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, Once, OnceLock,
    },
    time::{SystemTime, UNIX_EPOCH},
};
use zip::write::SimpleFileOptions;

use crate::persistence::{AppState, DatabaseIntegrityResult};

const SCHEMA_VERSION: u8 = 1;
const DIRECTORY: &str = "diagnostics";
const CURRENT_LOG: &str = "events.jsonl";
const MARKER: &str = "active-run.json";
const MAX_LOG_BYTES: u64 = 10 * 1024 * 1024;
const MAX_LOG_FILES: usize = 5;
const MAX_LINE_BYTES: usize = 4 * 1024;
const MAX_EXPORT_INPUT_BYTES: u64 = MAX_LOG_BYTES * MAX_LOG_FILES as u64;
const MAX_RECENT_EVENTS: usize = 20;
const FORMAT: &str = "jarvis-diagnostics";
static ACTIVE: OnceLock<DiagnosticsState> = OnceLock::new();
static PANIC_HOOK: Once = Once::new();

#[derive(Clone)]
pub struct DiagnosticsState {
    inner: Arc<Inner>,
}

struct Inner {
    root: PathBuf,
    run_id: String,
    app_version: String,
    started_at: u64,
    write_lock: Mutex<()>,
    storage_events: Mutex<BTreeSet<String>>,
    begun: AtomicBool,
    finished: AtomicBool,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum Level {
    Info,
    Warning,
    Error,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum EventKind {
    Startup,
    CleanShutdown,
    AbruptShutdown,
    Panic,
    SingleInstanceConflict,
    StorageFailure,
    ProviderRefusal,
    ProviderFailure,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ShutdownReason {
    UserExit,
    Update,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Record {
    schema_version: u8,
    timestamp: u64,
    level: Level,
    event: EventKind,
    run_id: String,
    app_version: String,
    os: String,
    arch: String,
    pid: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    previous_run_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    shutdown_reason: Option<ShutdownReason>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    operation: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    correlation_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    provider: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    category: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    http_status: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    upstream_code: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    request_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RunMarker {
    schema_version: u8,
    run_id: String,
    started_at: u64,
    clean_shutdown: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    shutdown_reason: Option<ShutdownReason>,
}

#[derive(Clone, Debug)]
pub(crate) struct ProviderMetadata {
    pub(crate) http_status: Option<u16>,
    pub(crate) upstream_code: Option<String>,
    pub(crate) request_id: Option<String>,
}

impl ProviderMetadata {
    pub(crate) fn new(
        http_status: Option<u16>,
        upstream_code: Option<&str>,
        request_id: Option<&str>,
    ) -> Self {
        Self {
            http_status: http_status.filter(|status| (100..=599).contains(status)),
            upstream_code: upstream_code.and_then(|value| safe_token(value, 96)),
            request_id: request_id.and_then(|value| safe_token(value, 160)),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticEvent {
    timestamp: u64,
    level: Level,
    event: EventKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    shutdown_reason: Option<ShutdownReason>,
    #[serde(skip_serializing_if = "Option::is_none")]
    operation: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    correlation_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    provider: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    category: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    http_status: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    upstream_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    request_id: Option<String>,
}

impl From<Record> for DiagnosticEvent {
    fn from(record: Record) -> Self {
        Self {
            timestamp: record.timestamp,
            level: record.level,
            event: record.event,
            shutdown_reason: record.shutdown_reason,
            operation: record.operation,
            correlation_id: record.correlation_id,
            provider: record.provider,
            category: record.category,
            http_status: record.http_status,
            upstream_code: record.upstream_code,
            request_id: record.request_id,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticSummary {
    run_id: String,
    app_version: String,
    os: String,
    arch: String,
    started_at: u64,
    log_files: usize,
    log_bytes: u64,
    event_count: usize,
    recent_events: Vec<DiagnosticEvent>,
    copyable: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticExportResult {
    path: String,
    bytes: u64,
    events: usize,
}

#[derive(Clone, Debug, Serialize)]
pub struct DiagnosticError {
    code: &'static str,
    message: String,
}

fn error(message: impl Into<String>) -> DiagnosticError {
    DiagnosticError {
        code: "diagnostic_error",
        message: message.into(),
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BundleManifest<'a> {
    format: &'static str,
    version: u8,
    created_at: u64,
    app_version: &'a str,
    os: &'a str,
    arch: &'a str,
    run_id: &'a str,
    events: usize,
    files: usize,
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

fn new_run_id() -> String {
    let mut bytes = [0_u8; 16];
    if getrandom::fill(&mut bytes).is_err() {
        bytes.copy_from_slice(&Sha256::digest(format!("{}:{}", now(), std::process::id()))[..16]);
    }
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn safe_token(value: &str, max: usize) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()
        && value.len() <= max
        && value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-' | b':' | b'/' | b'=')
        }))
    .then(|| value.to_owned())
}

fn correlation(value: &str) -> Option<String> {
    if value.is_empty() {
        return None;
    }
    let digest = Sha256::digest(value.as_bytes());
    Some(format!(
        "sha256:{}",
        digest[..12]
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    ))
}

fn redirected(metadata: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes()
            & windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT
            != 0
    }
    #[cfg(not(windows))]
    {
        metadata.file_type().is_symlink()
    }
}

fn ensure_directory(path: &Path) -> io::Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() && !redirected(&metadata) => return Ok(()),
        Ok(_) => return Err(io::Error::other("invalid diagnostics directory")),
        Err(cause) if cause.kind() == io::ErrorKind::NotFound => {}
        Err(cause) => return Err(cause),
    }
    fs::create_dir(path)?;
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_dir() || redirected(&metadata) {
        return Err(io::Error::other("invalid diagnostics directory"));
    }
    Ok(())
}

fn regular_file(path: &Path) -> io::Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_file() && !redirected(&metadata) => Ok(true),
        Ok(_) => Err(io::Error::other("invalid diagnostics file")),
        Err(cause) if cause.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(cause) => Err(cause),
    }
}

fn append_file(path: &Path) -> io::Result<File> {
    let _ = regular_file(path)?;
    let mut options = OpenOptions::new();
    options.create(true).append(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.custom_flags(windows_sys::Win32::Storage::FileSystem::FILE_FLAG_OPEN_REPARSE_POINT);
    }
    let file = options.open(path)?;
    if !file.metadata()?.is_file() || !regular_file(path)? {
        return Err(io::Error::other("invalid diagnostics file"));
    }
    Ok(file)
}

fn log_path(root: &Path, index: usize) -> PathBuf {
    if index == 0 {
        root.join(CURRENT_LOG)
    } else {
        root.join(format!("events.{index}.jsonl"))
    }
}

fn rotate(root: &Path, files: usize) -> io::Result<()> {
    for index in (1..files).rev() {
        let source = log_path(root, index - 1);
        let destination = log_path(root, index);
        if regular_file(&destination)? {
            fs::remove_file(&destination)?;
        }
        if regular_file(&source)? {
            fs::rename(source, destination)?;
        }
    }
    Ok(())
}

fn write_record(root: &Path, record: &Record, max_bytes: u64, files: usize) -> io::Result<()> {
    ensure_directory(root)?;
    let mut bytes = serde_json::to_vec(record).map_err(io::Error::other)?;
    bytes.push(b'\n');
    if bytes.len() > MAX_LINE_BYTES {
        return Err(io::Error::other("diagnostics record is too large"));
    }
    let current = log_path(root, 0);
    let size = if regular_file(&current)? {
        fs::metadata(&current)?.len()
    } else {
        0
    };
    if size.saturating_add(bytes.len() as u64) > max_bytes {
        rotate(root, files)?;
    }
    let mut file = append_file(&current)?;
    file.write_all(&bytes)?;
    file.flush()
}

fn read_marker(root: &Path) -> Option<RunMarker> {
    let path = root.join(MARKER);
    let metadata = fs::symlink_metadata(&path).ok()?;
    if !metadata.is_file() || redirected(&metadata) || metadata.len() > MAX_LINE_BYTES as u64 {
        return None;
    }
    let marker: RunMarker = serde_json::from_slice(&fs::read(path).ok()?).ok()?;
    (marker.schema_version == SCHEMA_VERSION
        && safe_token(&marker.run_id, 64).as_deref() == Some(marker.run_id.as_str()))
    .then_some(marker)
}

fn write_marker(root: &Path, marker: &RunMarker) -> io::Result<()> {
    ensure_directory(root)?;
    let destination = root.join(MARKER);
    if fs::symlink_metadata(&destination)
        .is_ok_and(|metadata| !metadata.is_file() || redirected(&metadata))
    {
        return Err(io::Error::other("invalid diagnostics marker"));
    }
    let mut temporary = tempfile::NamedTempFile::new_in(root)?;
    temporary.write_all(&serde_json::to_vec(marker).map_err(io::Error::other)?)?;
    temporary.as_file().sync_all()?;
    temporary
        .persist(destination)
        .map_err(|cause| cause.error)?;
    Ok(())
}

impl Record {
    fn new(state: &DiagnosticsState, event: EventKind, level: Level) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            timestamp: now(),
            level,
            event,
            run_id: state.inner.run_id.clone(),
            app_version: state.inner.app_version.clone(),
            os: std::env::consts::OS.into(),
            arch: std::env::consts::ARCH.into(),
            pid: std::process::id(),
            previous_run_id: None,
            shutdown_reason: None,
            operation: None,
            correlation_id: None,
            provider: None,
            category: None,
            http_status: None,
            upstream_code: None,
            request_id: None,
        }
    }

    fn valid(&self) -> bool {
        self.schema_version == SCHEMA_VERSION
            && safe_token(&self.run_id, 64).as_deref() == Some(self.run_id.as_str())
            && safe_token(&self.app_version, 64).as_deref() == Some(self.app_version.as_str())
            && safe_token(&self.os, 32).as_deref() == Some(self.os.as_str())
            && safe_token(&self.arch, 32).as_deref() == Some(self.arch.as_str())
            && self
                .previous_run_id
                .as_deref()
                .is_none_or(|value| safe_token(value, 64).as_deref() == Some(value))
            && self
                .operation
                .as_deref()
                .is_none_or(|value| safe_token(value, 64).as_deref() == Some(value))
            && self
                .correlation_id
                .as_deref()
                .is_none_or(|value| safe_token(value, 96).as_deref() == Some(value))
            && self
                .provider
                .as_deref()
                .is_none_or(|value| matches!(value, "openai_codex" | "antigravity" | "custom"))
            && self
                .category
                .as_deref()
                .is_none_or(|value| safe_token(value, 96).as_deref() == Some(value))
            && self
                .http_status
                .is_none_or(|status| (100..=599).contains(&status))
            && self
                .upstream_code
                .as_deref()
                .is_none_or(|value| safe_token(value, 96).as_deref() == Some(value))
            && self
                .request_id
                .as_deref()
                .is_none_or(|value| safe_token(value, 160).as_deref() == Some(value))
    }
}

impl DiagnosticsState {
    fn new(root: PathBuf, app_version: String, run_id: String) -> Self {
        Self {
            inner: Arc::new(Inner {
                root: root.join(DIRECTORY),
                run_id,
                app_version,
                started_at: now(),
                write_lock: Mutex::new(()),
                storage_events: Mutex::new(BTreeSet::new()),
                begun: AtomicBool::new(false),
                finished: AtomicBool::new(false),
            }),
        }
    }

    fn begin(&self) {
        if self.inner.begun.swap(true, Ordering::AcqRel) {
            return;
        }
        let Ok(_guard) = self.inner.write_lock.lock() else {
            return;
        };
        if let Some(previous) =
            read_marker(&self.inner.root).filter(|marker| !marker.clean_shutdown)
        {
            let mut record = Record::new(self, EventKind::AbruptShutdown, Level::Error);
            record.previous_run_id = Some(previous.run_id);
            let _ = write_record(&self.inner.root, &record, MAX_LOG_BYTES, MAX_LOG_FILES);
        }
        let marker = RunMarker {
            schema_version: SCHEMA_VERSION,
            run_id: self.inner.run_id.clone(),
            started_at: self.inner.started_at,
            clean_shutdown: false,
            shutdown_reason: None,
        };
        let _ = write_marker(&self.inner.root, &marker);
        let _ = write_record(
            &self.inner.root,
            &Record::new(self, EventKind::Startup, Level::Info),
            MAX_LOG_BYTES,
            MAX_LOG_FILES,
        );
    }

    fn record(&self, record: Record) {
        if !record.valid() {
            return;
        }
        let Ok(_guard) = self.inner.write_lock.lock() else {
            return;
        };
        let _ = write_record(&self.inner.root, &record, MAX_LOG_BYTES, MAX_LOG_FILES);
    }

    fn try_record(&self, record: Record) {
        if !record.valid() {
            return;
        }
        let Ok(_guard) = self.inner.write_lock.try_lock() else {
            return;
        };
        let _ = write_record(&self.inner.root, &record, MAX_LOG_BYTES, MAX_LOG_FILES);
    }

    fn finish(&self, reason: ShutdownReason) {
        if self.inner.finished.swap(true, Ordering::AcqRel) {
            return;
        }
        let Ok(_guard) = self.inner.write_lock.lock() else {
            return;
        };
        let marker = RunMarker {
            schema_version: SCHEMA_VERSION,
            run_id: self.inner.run_id.clone(),
            started_at: self.inner.started_at,
            clean_shutdown: true,
            shutdown_reason: Some(reason),
        };
        let _ = write_marker(&self.inner.root, &marker);
        let mut record = Record::new(self, EventKind::CleanShutdown, Level::Info);
        record.shutdown_reason = Some(reason);
        let _ = write_record(&self.inner.root, &record, MAX_LOG_BYTES, MAX_LOG_FILES);
    }

    fn records(&self) -> io::Result<Vec<(usize, Vec<Record>)>> {
        let mut total = 0_u64;
        let mut result = Vec::new();
        for index in (0..MAX_LOG_FILES).rev() {
            let path = log_path(&self.inner.root, index);
            if !regular_file(&path)? {
                continue;
            }
            let size = fs::metadata(&path)?.len();
            total = total.saturating_add(size);
            if size > MAX_LOG_BYTES || total > MAX_EXPORT_INPUT_BYTES {
                continue;
            }
            let file = File::open(path)?;
            let mut records = Vec::new();
            for line in BufReader::new(file).split(b'\n') {
                let Ok(line) = line else { continue };
                if line.is_empty() || line.len() > MAX_LINE_BYTES {
                    continue;
                }
                let Ok(record) = serde_json::from_slice::<Record>(&line) else {
                    continue;
                };
                if record.valid() {
                    records.push(record);
                }
            }
            result.push((index, records));
        }
        Ok(result)
    }

    fn summary(&self) -> Result<DiagnosticSummary, DiagnosticError> {
        let grouped = self
            .records()
            .map_err(|_| error("Não foi possível ler os registros locais."))?;
        let log_files = grouped.len();
        let log_bytes = (0..MAX_LOG_FILES)
            .filter_map(|index| fs::metadata(log_path(&self.inner.root, index)).ok())
            .map(|metadata| metadata.len())
            .sum();
        let records: Vec<Record> = grouped
            .into_iter()
            .flat_map(|(_, records)| records)
            .collect();
        let event_count = records.len();
        let recent: Vec<Record> = records
            .into_iter()
            .rev()
            .take(MAX_RECENT_EVENTS)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        let copyable = copyable_summary(self, log_files, log_bytes, event_count, &recent);
        Ok(DiagnosticSummary {
            run_id: self.inner.run_id.clone(),
            app_version: self.inner.app_version.clone(),
            os: std::env::consts::OS.into(),
            arch: std::env::consts::ARCH.into(),
            started_at: self.inner.started_at,
            log_files,
            log_bytes,
            event_count,
            recent_events: recent.into_iter().map(Into::into).collect(),
            copyable,
        })
    }

    fn export(&self, destination: &Path) -> Result<DiagnosticExportResult, DiagnosticError> {
        let parent = destination
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
            .ok_or_else(|| error("Escolha uma pasta válida para o diagnóstico."))?;
        fs::create_dir_all(parent)
            .map_err(|_| error("Não foi possível preparar a pasta escolhida."))?;
        if destination
            .extension()
            .and_then(|extension| extension.to_str())
            .is_none_or(|extension| !extension.eq_ignore_ascii_case("zip"))
        {
            return Err(error("O pacote de diagnóstico deve usar a extensão .zip."));
        }
        if fs::symlink_metadata(destination).is_ok_and(|metadata| metadata.file_type().is_symlink())
        {
            return Err(error("O destino do diagnóstico não pode ser um atalho."));
        }
        let grouped = self
            .records()
            .map_err(|_| error("Não foi possível preparar os registros locais."))?;
        let events = grouped.iter().map(|(_, records)| records.len()).sum();
        let summary = self.summary()?;
        let manifest = BundleManifest {
            format: FORMAT,
            version: SCHEMA_VERSION,
            created_at: now(),
            app_version: &self.inner.app_version,
            os: std::env::consts::OS,
            arch: std::env::consts::ARCH,
            run_id: &self.inner.run_id,
            events,
            files: grouped.len(),
        };
        let options = SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated)
            .unix_permissions(0o600);
        let mut temporary = tempfile::NamedTempFile::new_in(parent)
            .map_err(|_| error("Não foi possível criar o pacote de diagnóstico."))?;
        {
            let mut archive = zip::ZipWriter::new(temporary.as_file_mut());
            archive
                .start_file("manifest.json", options)
                .map_err(|_| error("Não foi possível gravar o manifesto do diagnóstico."))?;
            archive
                .write_all(
                    &serde_json::to_vec_pretty(&manifest).map_err(|_| {
                        error("Não foi possível preparar o manifesto do diagnóstico.")
                    })?,
                )
                .map_err(|_| error("Não foi possível gravar o manifesto do diagnóstico."))?;
            archive
                .start_file("summary.txt", options)
                .map_err(|_| error("Não foi possível gravar o resumo do diagnóstico."))?;
            archive
                .write_all(summary.copyable.as_bytes())
                .map_err(|_| error("Não foi possível gravar o resumo do diagnóstico."))?;
            for (index, records) in grouped {
                archive
                    .start_file(
                        format!("logs/{}", log_path(Path::new(""), index).display()),
                        options,
                    )
                    .map_err(|_| error("Não foi possível adicionar os registros ao pacote."))?;
                for record in records {
                    serde_json::to_writer(&mut archive, &record)
                        .map_err(|_| error("Não foi possível sanitizar os registros."))?;
                    archive
                        .write_all(b"\n")
                        .map_err(|_| error("Não foi possível adicionar os registros ao pacote."))?;
                }
            }
            archive
                .finish()
                .map_err(|_| error("Não foi possível finalizar o pacote de diagnóstico."))?;
        }
        temporary
            .as_file()
            .sync_all()
            .map_err(|_| error("Não foi possível finalizar o pacote de diagnóstico."))?;
        let bytes = temporary
            .as_file()
            .metadata()
            .map_err(|_| error("Não foi possível conferir o pacote de diagnóstico."))?
            .len();
        temporary
            .persist(destination)
            .map_err(|_| error("Não foi possível salvar o pacote de diagnóstico."))?;
        Ok(DiagnosticExportResult {
            path: destination.to_string_lossy().into_owned(),
            bytes,
            events,
        })
    }
}

fn copyable_summary(
    state: &DiagnosticsState,
    log_files: usize,
    log_bytes: u64,
    event_count: usize,
    records: &[Record],
) -> String {
    let mut lines = vec![
        "Diagnóstico do Jarvis".to_owned(),
        format!("Versão: {}", state.inner.app_version),
        format!(
            "Sistema: {} {}",
            std::env::consts::OS,
            std::env::consts::ARCH
        ),
        format!("Execução: {}", state.inner.run_id),
        format!("Início: {}", state.inner.started_at),
        format!("Registros: {event_count} em {log_files} arquivo(s), {log_bytes} bytes"),
    ];
    if !records.is_empty() {
        lines.push("Eventos recentes:".into());
    }
    for record in records {
        let mut detail = vec![format!("{} {:?}", record.timestamp, record.event)];
        if let Some(reason) = record.shutdown_reason {
            detail.push(format!("reason={reason:?}"));
        }
        if let Some(operation) = &record.operation {
            detail.push(format!("operation={operation}"));
        }
        if let Some(provider) = &record.provider {
            detail.push(format!("provider={provider}"));
        }
        if let Some(category) = &record.category {
            detail.push(format!("category={category}"));
        }
        if let Some(status) = record.http_status {
            detail.push(format!("http={status}"));
        }
        if let Some(code) = &record.upstream_code {
            detail.push(format!("upstream={code}"));
        }
        if let Some(request) = &record.request_id {
            detail.push(format!("request={request}"));
        }
        if let Some(id) = &record.correlation_id {
            detail.push(format!("correlation={id}"));
        }
        lines.push(format!("- {}", detail.join(" · ")));
    }
    lines.join("\n")
}

pub(crate) fn initialize(data_root: &Path, app_version: &str) -> DiagnosticsState {
    let candidate = DiagnosticsState::new(
        data_root.to_path_buf(),
        app_version.to_owned(),
        new_run_id(),
    );
    let state = ACTIVE.get_or_init(|| candidate).clone();
    state.begin();
    PANIC_HOOK.call_once(|| {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |information| {
            if let Some(state) = ACTIVE.get() {
                state.try_record(Record::new(state, EventKind::Panic, Level::Error));
            }
            previous(information);
        }));
    });
    state
}

pub(crate) fn finish(reason: ShutdownReason) {
    if let Some(state) = ACTIVE.get() {
        state.finish(reason);
    }
}

pub(crate) fn record_single_instance_conflict() {
    if let Some(state) = ACTIVE.get() {
        state.record(Record::new(
            state,
            EventKind::SingleInstanceConflict,
            Level::Warning,
        ));
    }
}

pub(crate) fn record_storage_failure(operation: &str, correlation_value: Option<&str>) {
    let Some(state) = ACTIVE.get() else { return };
    let operation = safe_token(operation, 64).unwrap_or_else(|| "storage".into());
    let correlation_id = correlation_value.and_then(correlation);
    let key = format!(
        "{operation}:{}",
        correlation_id.as_deref().unwrap_or("global")
    );
    let Ok(mut seen) = state.inner.storage_events.lock() else {
        return;
    };
    if seen.len() >= 64 || !seen.insert(key) {
        return;
    }
    drop(seen);
    let mut record = Record::new(state, EventKind::StorageFailure, Level::Error);
    record.operation = Some(operation);
    record.correlation_id = correlation_id;
    state.record(record);
}

pub(crate) fn record_provider_failure(
    provider: &str,
    correlation_value: &str,
    category: &str,
    metadata: Option<&ProviderMetadata>,
) {
    let Some(state) = ACTIVE.get() else { return };
    let Some(provider) = safe_token(provider, 32) else {
        return;
    };
    if !matches!(provider.as_str(), "openai_codex" | "antigravity" | "custom") {
        return;
    }
    let Some(category) = safe_token(category, 96) else {
        return;
    };
    let refusal = metadata.is_some_and(|metadata| metadata.http_status.is_some())
        || matches!(
            category.as_str(),
            "provider_auth"
                | "provider_request"
                | "provider_blocked"
                | "provider_model_unsupported"
        );
    let mut record = Record::new(
        state,
        if refusal {
            EventKind::ProviderRefusal
        } else {
            EventKind::ProviderFailure
        },
        Level::Error,
    );
    record.provider = Some(provider);
    record.category = Some(category);
    record.correlation_id = correlation(correlation_value);
    if let Some(metadata) = metadata {
        record.http_status = metadata.http_status;
        record.upstream_code.clone_from(&metadata.upstream_code);
        record.request_id.clone_from(&metadata.request_id);
    }
    state.record(record);
}

#[tauri::command]
pub async fn get_diagnostic_summary(
    state: tauri::State<'_, DiagnosticsState>,
) -> Result<DiagnosticSummary, DiagnosticError> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || state.summary())
        .await
        .map_err(|_| error("A leitura do diagnóstico foi interrompida."))?
}

#[tauri::command]
pub async fn check_database_integrity(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<DatabaseIntegrityResult, DiagnosticError> {
    use tauri::Manager as _;

    let home = app
        .path()
        .home_dir()
        .map_err(|_| error("Não foi possível localizar o banco de dados do Jarvis."))?;
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        state.check_database_integrity(&home).map_err(|cause| {
            error(format!(
                "Não foi possível verificar o banco de dados: {cause}"
            ))
        })
    })
    .await
    .map_err(|_| error("A verificação do banco de dados foi interrompida."))?
}

#[tauri::command]
pub async fn export_diagnostic_bundle(
    state: tauri::State<'_, DiagnosticsState>,
    path: String,
) -> Result<DiagnosticExportResult, DiagnosticError> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || state.export(Path::new(&path)))
        .await
        .map_err(|_| error("A exportação do diagnóstico foi interrompida."))?
}

#[cfg(test)]
mod tests;
