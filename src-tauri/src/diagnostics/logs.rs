//! Detailed evidence shares the existing rotating JSONL store, not chat journals.
use super::*;
use regex::Regex;

const MAX_MESSAGE_BYTES: usize = 8 * 1024;
const MAX_STACK_BYTES: usize = 128 * 1024;
const MAX_PAGE_BYTES: usize = 256 * 1024;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Details {
    message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    stack: Option<String>,
    truncated: bool,
}

fn redact(value: &str, max_bytes: usize) -> (String, bool) {
    static PATTERNS: OnceLock<Vec<(Regex, &'static str)>> = OnceLock::new();
    let patterns = PATTERNS.get_or_init(|| {
        [
            (r"(?is)-----BEGIN [^-]*PRIVATE KEY-----.*?(?:-----END [^-]*PRIVATE KEY-----|$)", "[redigido]"),
            (r#"(?i)\b(?:bearer|basic)\s+[^\s\"',;]+"#, "[redigido]"),
            (r#"(?i)[\"']?(?:api[_-]?key|access[_-]?token|refresh[_-]?token|token|secret|password|senha|authorization|cookie|set-cookie)[\"']?\s*[:=]\s*(?:\"[^\"]*\"|'[^']*'|[^\s,;]+)"#, "[redigido]"),
            (r"\b(?:sk-[A-Za-z0-9_-]+|gh[pousr]_[A-Za-z0-9_]+|github_pat_[A-Za-z0-9_]+|AKIA[A-Z0-9]{16})\b", "[redigido]"),
            (r"\beyJ[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+\b", "[redigido]"),
            (r"(?i)(\b(?:https?|ssh|postgres(?:ql)?|mysql)://)[^\s/@]+:[^\s/@]+@", "${1}[redigido]@"),
            (r"(?i)(\b(?:https?|ssh|postgres(?:ql)?|mysql)://[^\s?#]+)[?#][^\s]+", "${1}?[redigido]"),
            (r"[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}", "[redigido]"),
        ].into_iter().map(|(pattern, replacement)| {
            (Regex::new(pattern).expect("static diagnostic redaction regex"), replacement)
        }).collect()
    });
    // Bound work before applying regexes, without cutting a UTF-8 character.
    let mut end = value.len().min(max_bytes);
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    let mut text: String = value[..end]
        .chars()
        .filter(|c| !c.is_control() || matches!(c, '\n' | '\r' | '\t'))
        .collect();
    for (pattern, replacement) in patterns {
        text = pattern.replace_all(&text, *replacement).into_owned();
    }
    let truncated = value.len() > max_bytes || text.len() > max_bytes;
    let mut end = text.len().min(max_bytes);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text.truncate(end);
    (text, truncated)
}

impl Details {
    pub(super) fn new(message: &str, stack: Option<&str>) -> Self {
        let (message, message_truncated) = redact(message, MAX_MESSAGE_BYTES);
        let (stack, stack_truncated) = stack
            .map(|stack| redact(stack, MAX_STACK_BYTES))
            .map_or((None, false), |(stack, truncated)| (Some(stack), truncated));
        Self {
            message,
            stack,
            truncated: message_truncated || stack_truncated,
        }
    }

    pub(super) fn valid(&self) -> bool {
        self.message == redact(&self.message, MAX_MESSAGE_BYTES).0
            && self
                .stack
                .as_ref()
                .is_none_or(|stack| *stack == redact(stack, MAX_STACK_BYTES).0)
    }
}

pub(crate) fn record_runtime(
    level: Level,
    operation: &str,
    code: Option<&str>,
    message: &str,
    stack: Option<&str>,
    correlation_value: Option<&str>,
) {
    let Some(state) = ACTIVE.get() else { return };
    let mut record = Record::new(state, EventKind::Runtime, level);
    record.operation = safe_token(operation, 64);
    record.category = code.and_then(|code| safe_token(code, 96));
    record.correlation_id = correlation_value.and_then(correlation);
    record.details = Some(Details::new(message, stack));
    state.record(record);
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Query {
    level: Option<Level>,
    query: Option<String>,
    since: Option<u64>,
    before: Option<u64>,
    #[serde(default)]
    offset: usize,
    limit: Option<usize>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Page {
    events: Vec<Record>,
    total: usize,
    offset: usize,
    next_offset: Option<usize>,
    before: u64,
    dropped_records_current_run: u64,
}

impl DiagnosticsState {
    fn log_page(&self, query: Query) -> Result<Page, DiagnosticError> {
        let limit = query.limit.unwrap_or(20);
        if !(1..=50).contains(&limit)
            || query.offset > 10_000
            || query.query.as_ref().is_some_and(|q| q.len() > 200)
        {
            return Err(error("Filtros de logs inválidos."));
        }
        let before = query.before.unwrap_or_else(now);
        let search = query.query.as_deref().map(str::to_lowercase);
        let records: Vec<_> = self
            .records()
            .map_err(|_| error("Não foi possível ler os logs locais."))?
            .into_iter()
            .flat_map(|(_, records)| records)
            .rev()
            .filter(|record| {
                record.timestamp <= before
                    && query.since.is_none_or(|since| record.timestamp >= since)
            })
            .filter(|record| query.level.is_none_or(|level| record.level == level))
            .filter(|record| {
                search.as_ref().is_none_or(|search| {
                    serde_json::to_string(record)
                        .is_ok_and(|text| text.to_lowercase().contains(search))
                })
            })
            .collect();
        let total = records.len();
        let mut events = Vec::new();
        let mut bytes = 0;
        for record in records.into_iter().skip(query.offset).take(limit) {
            let size = serde_json::to_vec(&record)
                .map_err(|_| error("Log inválido."))?
                .len();
            if !events.is_empty() && bytes + size > MAX_PAGE_BYTES {
                break;
            }
            bytes += size;
            events.push(record);
        }
        let next = query.offset + events.len();
        Ok(Page {
            events,
            total,
            offset: query.offset,
            next_offset: (next < total).then_some(next),
            before,
            dropped_records_current_run: self.inner.dropped_records.load(Ordering::Relaxed),
        })
    }

    fn log_storage(&self, clear: bool) -> Result<(usize, u64), DiagnosticError> {
        let _guard = self
            .inner
            .write_lock
            .lock()
            .map_err(|_| error("Os logs estão ocupados."))?;
        ensure_directory(&self.inner.root).map_err(|_| error("Armazenamento de logs inválido."))?;
        let mut paths = Vec::new();
        for index in 0..MAX_LOG_FILES {
            let path = log_path(&self.inner.root, index);
            if regular_file(&path).map_err(|_| error("Armazenamento de logs inválido."))? {
                let bytes = fs::metadata(&path)
                    .map_err(|_| error("Não foi possível analisar os logs."))?
                    .len();
                paths.push((path, bytes));
            }
        }
        let bytes = paths.iter().map(|(_, bytes)| bytes).sum();
        if clear {
            for (path, _) in &paths {
                fs::remove_file(path).map_err(|_| {
                    error(
                        "A limpeza dos logs pode estar parcial. Analise novamente antes de tentar.",
                    )
                })?;
            }
            self.inner
                .storage_events
                .lock()
                .map_err(|_| error("Os logs estão ocupados."))?
                .clear();
        }
        Ok((paths.len(), bytes))
    }
}

pub(crate) async fn read(query: Query) -> Result<Page, DiagnosticError> {
    let state = ACTIVE
        .get()
        .cloned()
        .ok_or_else(|| error("Os logs ainda não foram inicializados."))?;
    tauri::async_runtime::spawn_blocking(move || state.log_page(query))
        .await
        .map_err(|_| error("Não foi possível consultar os logs."))?
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StorageStatus {
    files: usize,
    bytes: u64,
    max_files: usize,
    max_bytes: u64,
    dropped_records_current_run: u64,
}

fn storage_status(state: &DiagnosticsState) -> Result<StorageStatus, DiagnosticError> {
    let (files, bytes) = state.log_storage(false)?;
    let (harness_files, harness_bytes) = crate::agent::telemetry::log_storage(false)
        .map_err(|_| error("Não foi possível analisar os logs do harness."))?;
    Ok(StorageStatus {
        files: files + harness_files,
        bytes: bytes + harness_bytes,
        max_files: MAX_LOG_FILES + crate::agent::telemetry::MAX_LOG_FILES,
        max_bytes: MAX_EXPORT_INPUT_BYTES
            + crate::agent::telemetry::MAX_LOG_BYTES
                * crate::agent::telemetry::MAX_LOG_FILES as u64,
        dropped_records_current_run: state.inner.dropped_records.load(Ordering::Relaxed),
    })
}

#[tauri::command]
pub(crate) async fn get_diagnostic_log_status(
    state: tauri::State<'_, DiagnosticsState>,
) -> Result<StorageStatus, DiagnosticError> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || storage_status(&state))
        .await
        .map_err(|_| error("Não foi possível analisar os logs."))?
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CleanupResult {
    freed_bytes: u64,
    status: StorageStatus,
}

fn clear(state: &DiagnosticsState, confirmed: bool) -> Result<CleanupResult, DiagnosticError> {
    if !confirmed {
        return Err(error("Confirme a limpeza dos logs."));
    }
    // Preflight both fixed stores; never remove markers, incidents or journals.
    storage_status(state)?;
    let (_, harness_bytes) = crate::agent::telemetry::log_storage(true).map_err(|_| {
        error("A limpeza dos logs pode estar parcial. Analise novamente antes de tentar.")
    })?;
    let (_, bytes) = state.log_storage(true)?;
    Ok(CleanupResult {
        freed_bytes: bytes + harness_bytes,
        status: storage_status(state)?,
    })
}

#[tauri::command]
pub(crate) async fn clear_diagnostic_logs(
    state: tauri::State<'_, DiagnosticsState>,
    confirmed: bool,
) -> Result<CleanupResult, DiagnosticError> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || clear(&state, confirmed))
        .await
        .map_err(|_| error("Não foi possível limpar os logs. Analise novamente antes de tentar."))?
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct FrontendEvent {
    level: Level,
    source: String,
    message: String,
    stack: Option<String>,
}

#[tauri::command]
pub(crate) async fn record_frontend_diagnostic(
    event: FrontendEvent,
) -> Result<(), DiagnosticError> {
    if !matches!(
        event.source.as_str(),
        "window_error"
            | "unhandled_rejection"
            | "console_error"
            | "console_warning"
            | "react_error"
            | "react_recovery"
    ) {
        return Err(error("Origem do diagnóstico inválida."));
    }
    tauri::async_runtime::spawn_blocking(move || {
        record_runtime(
            event.level,
            &event.source,
            None,
            &event.message,
            event.stack.as_deref(),
            None,
        );
    })
    .await
    .map_err(|_| error("Não foi possível registrar o diagnóstico."))
}

#[cfg(test)]
#[path = "logs_tests.rs"]
mod tests;
