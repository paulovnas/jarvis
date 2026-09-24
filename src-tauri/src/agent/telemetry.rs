//! Local, bounded and content-free telemetry for the agent harness.
//!
//! Records contain only allowlisted counters, timings, classifications and
//! hashed correlation identifiers. Prompts, responses, file paths, command
//! arguments, URLs, credentials and raw provider errors are never accepted by
//! this schema.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex, OnceLock,
    },
    time::{Instant, SystemTime, UNIX_EPOCH},
};

const SCHEMA_VERSION: u8 = 1;
const DIRECTORY: &str = "harness";
const CURRENT_LOG: &str = "traces.jsonl";
const MAX_LOG_BYTES: u64 = 2 * 1024 * 1024;
const MAX_LOG_FILES: usize = 3;
const MAX_LINE_BYTES: usize = 2 * 1024;
const MAX_DURATION_MS: u64 = 24 * 60 * 60 * 1_000;
const MAX_BYTES: u64 = 512 * 1024 * 1024;
const MAX_COUNT: u64 = 10_000_000;
const MAX_TOKENS: u64 = 10_000_000_000;
const EXPORT_FORMAT: &str = "jarvis-harness-trace";
static ACTIVE: OnceLock<TelemetryState> = OnceLock::new();

#[derive(Clone)]
pub(crate) struct TelemetryState {
    inner: Arc<Inner>,
}

struct Inner {
    root: PathBuf,
    run_id: String,
    app_version: String,
    write_lock: Mutex<()>,
    dropped_records: AtomicU64,
}

/// Phase durations may overlap. Their sum is work, never turn wall time.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Phase {
    Preparation,
    Journal,
    ToolQueue,
    ToolHandler,
    CorePostprocessing,
    HumanWait,
}

pub(crate) struct PhaseSpan {
    context: TraceContext,
    phase: Phase,
    timestamp: u64,
    started: Instant,
}

pub(crate) fn phase(context: &TraceContext, phase: Phase) -> PhaseSpan {
    PhaseSpan {
        context: context.clone(),
        phase,
        timestamp: now(),
        started: Instant::now(),
    }
}

impl Drop for PhaseSpan {
    fn drop(&mut self) {
        record(
            &self.context,
            Event::PhaseFinished {
                phase: self.phase,
                started_at: self.timestamp,
                duration_ms: self
                    .started
                    .elapsed()
                    .as_millis()
                    .try_into()
                    .unwrap_or(u64::MAX),
            },
        );
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
struct Distribution {
    samples: u64,
    p50_ms: u64,
    p95_ms: u64,
}

fn distribution(mut values: Vec<u64>) -> Distribution {
    values.sort_unstable();
    let percentile = |percent: usize| {
        values
            .get((values.len() * percent).div_ceil(100).saturating_sub(1))
            .copied()
            .unwrap_or(0)
    };
    Distribution {
        samples: values.len() as u64,
        p50_ms: percentile(50),
        p95_ms: percentile(95),
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
struct PhaseReport {
    phase: Phase,
    distribution: Distribution,
    work_ms: u64,
    /// Union within each turn, not the sum of concurrent spans.
    occupied_ms: u64,
}

fn occupied(intervals: &mut [(u64, u64)]) -> u64 {
    intervals.sort_unstable();
    let mut end = 0;
    let mut total = 0_u64;
    for &(start, next_end) in intervals.iter() {
        total = total.saturating_add(next_end.saturating_sub(start.max(end)));
        end = end.max(next_end);
    }
    total
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TraceContext {
    trace_id: String,
    conversation_id: String,
    turn_id: String,
}

impl TraceContext {
    pub(crate) fn new(conversation_id: &str, turn_id: &str) -> Self {
        Self {
            trace_id: correlation(&format!("{conversation_id}\0{turn_id}")),
            conversation_id: correlation(conversation_id),
            turn_id: correlation(turn_id),
        }
    }

    #[cfg(test)]
    fn fixture(seed: &str) -> Self {
        Self {
            trace_id: seed.repeat(32),
            conversation_id: correlation("private-conversation"),
            turn_id: correlation("private-turn"),
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ProviderKind {
    #[serde(rename = "openai_codex")]
    OpenAiCodex,
    Antigravity,
    Custom,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Outcome {
    Succeeded,
    Failed,
    Cancelled,
    Retried,
    Denied,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum FailureClass {
    Authentication,
    RateLimit,
    Network,
    Timeout,
    Unavailable,
    Protocol,
    InvalidRequest,
    ToolArguments,
    ToolExecution,
    Storage,
    Policy,
    Unknown,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ToolKind {
    Read,
    Search,
    Write,
    Patch,
    Shell,
    Terminal,
    Process,
    Browser,
    Web,
    Mcp,
    Workflow,
    Question,
    Image,
    Other,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PolicyDecision {
    Allow,
    Ask,
    Deny,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PolicyReason {
    ReadOnly,
    Write,
    Network,
    Process,
    Destructive,
    Dynamic,
    Unknown,
    Scope,
    Capability,
    Privilege,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum CompactionReason {
    AutomaticThreshold,
    Manual,
    ProviderLimit,
    Recovery,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub(crate) enum Event {
    PhaseFinished {
        phase: Phase,
        started_at: u64,
        duration_ms: u64,
    },
    TurnStarted {
        context_items: u64,
        context_bytes: u64,
        advertised_tools: u64,
    },
    ContextPrepared {
        context_id: String,
        input_items: u64,
        input_bytes: u64,
        instructions_bytes: u64,
        advertised_tools: u64,
    },
    TurnFinished {
        outcome: Outcome,
        duration_ms: u64,
        provider_requests: u64,
        tool_calls: u64,
        compacted: bool,
    },
    ProviderRequest {
        provider: ProviderKind,
        model_id: String,
        attempt: u8,
        input_items: u64,
        input_bytes: u64,
        advertised_tools: u64,
    },
    ProviderResponse {
        provider: ProviderKind,
        model_id: String,
        attempt: u8,
        outcome: Outcome,
        duration_ms: u64,
        first_event_ms: Option<u64>,
        input_tokens: Option<u64>,
        output_tokens: Option<u64>,
        cache_read_tokens: Option<u64>,
        cache_write_tokens: Option<u64>,
        failure: Option<FailureClass>,
    },
    ToolFinished {
        tool: ToolKind,
        tool_id: String,
        outcome: Outcome,
        duration_ms: u64,
        input_bytes: u64,
        output_bytes: u64,
        failure: Option<FailureClass>,
    },
    PolicyEvaluated {
        tool: ToolKind,
        tool_id: String,
        decision: PolicyDecision,
        reason: PolicyReason,
        grant_used: bool,
    },
    Compaction {
        reason: CompactionReason,
        outcome: Outcome,
        source_items: u64,
        retained_items: u64,
        source_bytes: u64,
        retained_bytes: u64,
        duration_ms: u64,
        failure: Option<FailureClass>,
    },
    Recovery {
        outcome: Outcome,
        journal_turns: u64,
        replayed_turns: u64,
        replayed_items: u64,
        replayed_bytes: u64,
        duration_ms: u64,
        failure: Option<FailureClass>,
    },
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Record {
    schema_version: u8,
    timestamp: u64,
    run_id: String,
    app_version: String,
    trace_id: String,
    conversation_id: String,
    turn_id: String,
    event: Event,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SanitizedExport<'a> {
    format: &'static str,
    schema_version: u8,
    created_at: u64,
    records: &'a [Record],
    dropped_records_current_run: u64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct HarnessTraceExportResult {
    path: String,
    bytes: u64,
    records: usize,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct HarnessTraceError {
    code: &'static str,
    message: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct HarnessReport {
    schema_version: u8,
    records: u64,
    dropped_records_current_run: u64,
    turn_duration: Distribution,
    phases: Vec<PhaseReport>,
    turns_started: u64,
    turns_finished: u64,
    successful_turns: u64,
    failed_turns: u64,
    provider_requests: u64,
    provider_retries: u64,
    provider_failures: u64,
    tool_calls: u64,
    tool_failures: u64,
    policy_decisions: u64,
    policy_asks: u64,
    policy_denials: u64,
    policy_grants_used: u64,
    compactions: u64,
    compaction_failures: u64,
    recoveries: u64,
    recovery_failures: u64,
    input_tokens: u64,
    output_tokens: u64,
    cache_read_tokens: u64,
    cache_write_tokens: u64,
    provider_duration_ms: u64,
    tool_duration_ms: u64,
    average_first_event_ms: Option<u64>,
    providers: Vec<ProviderReport>,
    summary: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProviderReport {
    provider: String,
    requests: u64,
    retries: u64,
    failures: u64,
    input_tokens: u64,
    output_tokens: u64,
    cache_read_tokens: u64,
    cache_write_tokens: u64,
    duration_ms: u64,
}

fn export_error(message: impl Into<String>) -> HarnessTraceError {
    HarnessTraceError {
        code: "harness_trace_error",
        message: message.into(),
    }
}

impl Record {
    fn new(state: &TelemetryState, context: &TraceContext, event: Event) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            timestamp: now(),
            run_id: state.inner.run_id.clone(),
            app_version: state.inner.app_version.clone(),
            trace_id: context.trace_id.clone(),
            conversation_id: context.conversation_id.clone(),
            turn_id: context.turn_id.clone(),
            event,
        }
    }

    fn valid(&self) -> bool {
        self.schema_version == SCHEMA_VERSION
            && safe_token(&self.run_id, 64)
            && safe_token(&self.app_version, 64)
            && safe_token(&self.trace_id, 64)
            && valid_correlation(&self.conversation_id)
            && valid_correlation(&self.turn_id)
            && self.event.valid()
    }
}

impl Event {
    fn valid(&self) -> bool {
        match self {
            Self::PhaseFinished {
                started_at,
                duration_ms,
                ..
            } => *started_at > 0 && duration(*duration_ms),
            Self::TurnStarted {
                context_items,
                context_bytes,
                advertised_tools,
            } => count(*context_items) && bytes(*context_bytes) && count(*advertised_tools),
            Self::ContextPrepared {
                context_id,
                input_items,
                input_bytes,
                instructions_bytes,
                advertised_tools,
            } => {
                valid_correlation(context_id)
                    && count(*input_items)
                    && bytes(*input_bytes)
                    && bytes(*instructions_bytes)
                    && count(*advertised_tools)
            }
            Self::TurnFinished {
                duration_ms,
                provider_requests,
                tool_calls,
                ..
            } => duration(*duration_ms) && count(*provider_requests) && count(*tool_calls),
            Self::ProviderRequest {
                model_id,
                attempt,
                input_items,
                input_bytes,
                advertised_tools,
                ..
            } => {
                valid_correlation(model_id)
                    && (1..=16).contains(attempt)
                    && count(*input_items)
                    && bytes(*input_bytes)
                    && count(*advertised_tools)
            }
            Self::ProviderResponse {
                model_id,
                attempt,
                duration_ms,
                first_event_ms,
                input_tokens,
                output_tokens,
                cache_read_tokens,
                cache_write_tokens,
                ..
            } => {
                valid_correlation(model_id)
                    && (1..=16).contains(attempt)
                    && duration(*duration_ms)
                    && first_event_ms.is_none_or(|value| value <= *duration_ms)
                    && [
                        input_tokens,
                        output_tokens,
                        cache_read_tokens,
                        cache_write_tokens,
                    ]
                    .into_iter()
                    .all(|value| value.is_none_or(|value| value <= MAX_TOKENS))
            }
            Self::ToolFinished {
                tool_id,
                duration_ms,
                input_bytes,
                output_bytes,
                ..
            } => {
                valid_correlation(tool_id)
                    && duration(*duration_ms)
                    && bytes(*input_bytes)
                    && bytes(*output_bytes)
            }
            Self::PolicyEvaluated { tool_id, .. } => valid_correlation(tool_id),
            Self::Compaction {
                source_items,
                retained_items,
                source_bytes,
                retained_bytes,
                duration_ms,
                ..
            } => {
                count(*source_items)
                    && count(*retained_items)
                    && retained_items <= source_items
                    && bytes(*source_bytes)
                    && bytes(*retained_bytes)
                    && retained_bytes <= source_bytes
                    && duration(*duration_ms)
            }
            Self::Recovery {
                journal_turns,
                replayed_turns,
                replayed_items,
                replayed_bytes,
                duration_ms,
                ..
            } => {
                count(*journal_turns)
                    && count(*replayed_turns)
                    && replayed_turns <= journal_turns
                    && count(*replayed_items)
                    && bytes(*replayed_bytes)
                    && duration(*duration_ms)
            }
        }
    }
}

impl TelemetryState {
    fn new(data_root: &Path, app_version: String, run_id: String) -> Self {
        Self {
            inner: Arc::new(Inner {
                root: data_root.join(DIRECTORY),
                run_id,
                app_version,
                write_lock: Mutex::new(()),
                dropped_records: AtomicU64::new(0),
            }),
        }
    }

    fn record(&self, context: &TraceContext, event: Event) -> bool {
        let record = Record::new(self, context, event);
        if !record.valid() {
            self.inner.dropped_records.fetch_add(1, Ordering::Relaxed);
            return false;
        }
        let Ok(_guard) = self.inner.write_lock.try_lock() else {
            self.inner.dropped_records.fetch_add(1, Ordering::Relaxed);
            return false;
        };
        let written = write_record(&self.inner.root, &record).is_ok();
        if !written {
            self.inner.dropped_records.fetch_add(1, Ordering::Relaxed);
        }
        written
    }

    fn records(&self) -> io::Result<Vec<Record>> {
        use std::io::BufRead as _;

        let mut records = Vec::new();
        let mut total = 0_u64;
        for index in (0..MAX_LOG_FILES).rev() {
            let path = log_path(&self.inner.root, index);
            if !regular_file(&path)? {
                continue;
            }
            let size = fs::metadata(&path)?.len();
            total = total.saturating_add(size);
            if size > MAX_LOG_BYTES || total > MAX_LOG_BYTES * MAX_LOG_FILES as u64 {
                continue;
            }
            for line in std::io::BufReader::new(File::open(path)?).split(b'\n') {
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
        }
        Ok(records)
    }

    fn export(&self, destination: &Path) -> Result<HarnessTraceExportResult, HarnessTraceError> {
        let parent = destination
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
            .ok_or_else(|| export_error("Escolha uma pasta válida para o trace."))?;
        if destination
            .extension()
            .and_then(|extension| extension.to_str())
            .is_none_or(|extension| !extension.eq_ignore_ascii_case("json"))
        {
            return Err(export_error("O trace deve usar a extensão .json."));
        }
        fs::create_dir_all(parent)
            .map_err(|_| export_error("Não foi possível preparar a pasta escolhida."))?;
        if fs::symlink_metadata(destination).is_ok_and(|metadata| redirected(&metadata)) {
            return Err(export_error("O destino do trace não pode ser um atalho."));
        }
        let records = self
            .records()
            .map_err(|_| export_error("Não foi possível ler a telemetria local."))?;
        let export = SanitizedExport {
            format: EXPORT_FORMAT,
            schema_version: SCHEMA_VERSION,
            created_at: now(),
            records: &records,
            dropped_records_current_run: self.inner.dropped_records.load(Ordering::Relaxed),
        };
        let mut temporary = tempfile::NamedTempFile::new_in(parent)
            .map_err(|_| export_error("Não foi possível preparar o arquivo do trace."))?;
        serde_json::to_writer_pretty(&mut temporary, &export)
            .and_then(|()| temporary.write_all(b"\n").map_err(serde_json::Error::io))
            .map_err(|_| export_error("Não foi possível montar o trace sanitizado."))?;
        temporary
            .as_file()
            .sync_all()
            .map_err(|_| export_error("Não foi possível finalizar o trace sanitizado."))?;
        let bytes = temporary
            .as_file()
            .metadata()
            .map_err(|_| export_error("Não foi possível conferir o trace sanitizado."))?
            .len();
        temporary
            .persist(destination)
            .map_err(|_| export_error("Não foi possível salvar o trace sanitizado."))?;
        Ok(HarnessTraceExportResult {
            path: destination.to_string_lossy().into_owned(),
            bytes,
            records: records.len(),
        })
    }

    fn report(&self) -> Result<HarnessReport, HarnessTraceError> {
        let records = self
            .records()
            .map_err(|_| export_error("Não foi possível ler a telemetria local."))?;
        let mut report = HarnessReport {
            schema_version: SCHEMA_VERSION,
            records: u64::try_from(records.len()).unwrap_or(u64::MAX),
            dropped_records_current_run: self.inner.dropped_records.load(Ordering::Relaxed),
            ..HarnessReport::default()
        };
        let mut providers: BTreeMap<ProviderKind, ProviderReport> = BTreeMap::new();
        let mut first_event_total = 0_u64;
        let mut first_event_samples = 0_u64;
        let mut turn_durations = Vec::new();
        let mut phases: BTreeMap<Phase, Vec<u64>> = BTreeMap::new();
        let mut intervals: BTreeMap<(Phase, String, String), Vec<(u64, u64)>> = BTreeMap::new();
        for record in records {
            match record.event {
                Event::PhaseFinished {
                    phase,
                    started_at,
                    duration_ms,
                } => {
                    phases.entry(phase).or_default().push(duration_ms);
                    intervals
                        .entry((phase, record.run_id, record.trace_id))
                        .or_default()
                        .push((started_at, started_at.saturating_add(duration_ms)));
                }
                Event::TurnStarted { .. } => report.turns_started += 1,
                Event::TurnFinished {
                    outcome,
                    duration_ms,
                    ..
                } => {
                    turn_durations.push(duration_ms);
                    report.turns_finished += 1;
                    if outcome == Outcome::Succeeded {
                        report.successful_turns += 1;
                    } else {
                        report.failed_turns += 1;
                    }
                }
                Event::ProviderRequest { provider, .. } => {
                    report.provider_requests += 1;
                    providers
                        .entry(provider)
                        .or_insert_with(|| ProviderReport {
                            provider: provider_label(provider).into(),
                            ..ProviderReport::default()
                        })
                        .requests += 1;
                }
                Event::ProviderResponse {
                    provider,
                    outcome,
                    duration_ms,
                    first_event_ms,
                    input_tokens,
                    output_tokens,
                    cache_read_tokens,
                    cache_write_tokens,
                    ..
                } => {
                    let provider = providers.entry(provider).or_insert_with(|| ProviderReport {
                        provider: provider_label(provider).into(),
                        ..ProviderReport::default()
                    });
                    if outcome == Outcome::Retried {
                        report.provider_retries += 1;
                        provider.retries += 1;
                    } else if outcome != Outcome::Succeeded {
                        report.provider_failures += 1;
                        provider.failures += 1;
                    }
                    let input = input_tokens.unwrap_or_default();
                    let output = output_tokens.unwrap_or_default();
                    let cache_read = cache_read_tokens.unwrap_or_default();
                    let cache_write = cache_write_tokens.unwrap_or_default();
                    report.input_tokens = report.input_tokens.saturating_add(input);
                    report.output_tokens = report.output_tokens.saturating_add(output);
                    report.cache_read_tokens = report.cache_read_tokens.saturating_add(cache_read);
                    report.cache_write_tokens =
                        report.cache_write_tokens.saturating_add(cache_write);
                    report.provider_duration_ms =
                        report.provider_duration_ms.saturating_add(duration_ms);
                    provider.input_tokens = provider.input_tokens.saturating_add(input);
                    provider.output_tokens = provider.output_tokens.saturating_add(output);
                    provider.cache_read_tokens =
                        provider.cache_read_tokens.saturating_add(cache_read);
                    provider.cache_write_tokens =
                        provider.cache_write_tokens.saturating_add(cache_write);
                    provider.duration_ms = provider.duration_ms.saturating_add(duration_ms);
                    if let Some(first_event) = first_event_ms {
                        first_event_total = first_event_total.saturating_add(first_event);
                        first_event_samples += 1;
                    }
                }
                Event::ToolFinished {
                    outcome,
                    duration_ms,
                    ..
                } => {
                    report.tool_calls += 1;
                    report.tool_duration_ms = report.tool_duration_ms.saturating_add(duration_ms);
                    if outcome != Outcome::Succeeded {
                        report.tool_failures += 1;
                    }
                }
                Event::PolicyEvaluated {
                    decision,
                    grant_used,
                    ..
                } => {
                    report.policy_decisions += 1;
                    if decision == PolicyDecision::Ask {
                        report.policy_asks += 1;
                    } else if decision == PolicyDecision::Deny {
                        report.policy_denials += 1;
                    }
                    if grant_used {
                        report.policy_grants_used += 1;
                    }
                }
                Event::Compaction { outcome, .. } => {
                    report.compactions += 1;
                    if outcome != Outcome::Succeeded {
                        report.compaction_failures += 1;
                    }
                }
                Event::Recovery { outcome, .. } => {
                    report.recoveries += 1;
                    if outcome != Outcome::Succeeded {
                        report.recovery_failures += 1;
                    }
                }
                Event::ContextPrepared { .. } => {}
            }
        }
        report.average_first_event_ms =
            (first_event_samples > 0).then(|| first_event_total / first_event_samples);
        report.providers = providers.into_values().collect();
        report.turn_duration = distribution(turn_durations);
        report.phases = phases
            .into_iter()
            .map(|(phase, durations)| {
                let work_ms = durations.iter().copied().fold(0_u64, u64::saturating_add);
                let occupied_ms = intervals
                    .iter_mut()
                    .filter(|((p, _, _), _)| *p == phase)
                    .map(|(_, spans)| occupied(spans))
                    .fold(0_u64, u64::saturating_add);
                PhaseReport {
                    phase,
                    distribution: distribution(durations),
                    work_ms,
                    occupied_ms,
                }
            })
            .collect();
        let cache_basis_points = if report.input_tokens == 0 {
            0
        } else {
            ((u128::from(report.cache_read_tokens) * 10_000 + u128::from(report.input_tokens) / 2)
                / u128::from(report.input_tokens)) as u64
        };
        report.summary = format!(
            "{} turnos concluídos · {} solicitações ao provedor · {} retries · {} ferramentas · cache hit {:.2}%",
            report.turns_finished,
            report.provider_requests,
            report.provider_retries,
            report.tool_calls,
            cache_basis_points as f64 / 100.0,
        );
        Ok(report)
    }
}

pub(crate) fn initialize(data_root: &Path, app_version: &str) {
    let candidate = TelemetryState::new(data_root, app_version.to_owned(), random_id());
    let _ = ACTIVE.get_or_init(|| candidate);
}

pub(crate) fn trace(conversation_id: &str, turn_id: &str) -> TraceContext {
    TraceContext::new(conversation_id, turn_id)
}

pub(crate) fn model_id(value: &str) -> String {
    correlation(value)
}

pub(crate) fn tool_id(value: &str) -> String {
    correlation(value)
}

pub(super) fn policy_decision(value: super::execution_policy::ExecutionDecision) -> PolicyDecision {
    match value {
        super::execution_policy::ExecutionDecision::Allow => PolicyDecision::Allow,
        super::execution_policy::ExecutionDecision::Ask => PolicyDecision::Ask,
        super::execution_policy::ExecutionDecision::Deny => PolicyDecision::Deny,
    }
}

pub(super) fn policy_reason(code: &str) -> PolicyReason {
    match code {
        "read_only_within_scope" => PolicyReason::ReadOnly,
        "write_approval_required" => PolicyReason::Write,
        "network_approval_required" | "network_scope_denied" => PolicyReason::Network,
        "process_approval_required" => PolicyReason::Process,
        "destructive_command_approval_required" => PolicyReason::Destructive,
        "dynamic_command_approval_required" => PolicyReason::Dynamic,
        "read_scope_escape" | "write_scope_escape" => PolicyReason::Scope,
        "tool_capability_mismatch" => PolicyReason::Capability,
        "privilege_escalation_denied" => PolicyReason::Privilege,
        _ => PolicyReason::Unknown,
    }
}

pub(crate) fn context_id(value: &str) -> String {
    correlation(value)
}

pub(crate) fn serialized_bytes<T: Serialize>(value: &T) -> u64 {
    serde_json::to_vec(value)
        .map(|value| {
            u64::try_from(value.len())
                .unwrap_or(u64::MAX)
                .min(MAX_BYTES)
        })
        .unwrap_or_default()
}

pub(crate) fn provider_kind(credential: &crate::openai_codex::CodexCredential) -> ProviderKind {
    if credential.custom.is_some() {
        ProviderKind::Custom
    } else if credential.project_id.is_some() {
        ProviderKind::Antigravity
    } else {
        ProviderKind::OpenAiCodex
    }
}

pub(crate) fn failure_class(error: &super::AgentError) -> FailureClass {
    match error.code.as_str() {
        "provider_auth" => FailureClass::Authentication,
        "provider_limit" | "provider_output_limit" | "context_overflow" => FailureClass::RateLimit,
        "provider_network" => FailureClass::Network,
        "provider_timeout" => FailureClass::Timeout,
        "provider_unavailable" | "provider_failed" => FailureClass::Unavailable,
        "provider_protocol" | "provider_incomplete" | "provider_retry_exhausted" => {
            FailureClass::Protocol
        }
        "provider_request" | "invalid_message" | "context_invalid" | "context_limit" => {
            FailureClass::InvalidRequest
        }
        "invalid_tool_arguments" | "tool_arguments" => FailureClass::ToolArguments,
        "session_storage" => FailureClass::Storage,
        "denied" | "approval_denied" | "policy_denied" => FailureClass::Policy,
        code if code.starts_with("tool_") || code.ends_with("_error") => {
            FailureClass::ToolExecution
        }
        _ => FailureClass::Unknown,
    }
}

pub(crate) fn outcome(error: Option<&super::AgentError>, retried: bool) -> Outcome {
    if retried {
        Outcome::Retried
    } else {
        match error.map(|error| error.code.as_str()) {
            None => Outcome::Succeeded,
            Some("cancelled") => Outcome::Cancelled,
            Some("denied" | "approval_denied" | "policy_denied") => Outcome::Denied,
            Some(_) => Outcome::Failed,
        }
    }
}

pub(crate) fn tool_kind(name: &str) -> ToolKind {
    match name {
        "read" | "read_attachment" | "read_skill" => ToolKind::Read,
        "search" | "list" | "search_skills" => ToolKind::Search,
        "write" | "edit" => ToolKind::Write,
        "apply_patch" => ToolKind::Patch,
        "bash" => ToolKind::Shell,
        name if name.starts_with("terminal_") => ToolKind::Terminal,
        name if name.starts_with("process_") || name == "check_port" => ToolKind::Process,
        name if name.starts_with("browser_") => ToolKind::Browser,
        "web_search" => ToolKind::Web,
        name if name.starts_with("mcp_") => ToolKind::Mcp,
        name if name.starts_with("hub_") || name.starts_with("beads_") => ToolKind::Workflow,
        "ask_user" => ToolKind::Question,
        "generate_image" | "inspect_image" => ToolKind::Image,
        _ => ToolKind::Other,
    }
}

pub(crate) fn record(context: &TraceContext, event: Event) -> bool {
    ACTIVE
        .get()
        .is_some_and(|state| state.record(context, event))
}

#[tauri::command]
pub(crate) async fn export_harness_trace(
    destination: String,
) -> Result<HarnessTraceExportResult, HarnessTraceError> {
    let state = ACTIVE
        .get()
        .ok_or_else(|| export_error("A telemetria local ainda não foi inicializada."))?;
    state.export(Path::new(&destination))
}

#[tauri::command]
pub(crate) async fn get_harness_report() -> Result<HarnessReport, HarnessTraceError> {
    let state = ACTIVE
        .get()
        .ok_or_else(|| export_error("A telemetria local ainda não foi inicializada."))?;
    state.report()
}

fn provider_label(provider: ProviderKind) -> &'static str {
    match provider {
        ProviderKind::OpenAiCodex => "openai_codex",
        ProviderKind::Antigravity => "antigravity",
        ProviderKind::Custom => "custom",
    }
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

fn random_id() -> String {
    let mut value = [0_u8; 16];
    if getrandom::fill(&mut value).is_err() {
        value.copy_from_slice(&Sha256::digest(format!("{}:{}", now(), std::process::id()))[..16]);
    }
    value.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn correlation(value: &str) -> String {
    let digest = Sha256::digest(value.as_bytes());
    format!(
        "sha256:{}",
        digest[..12]
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    )
}

fn valid_correlation(value: &str) -> bool {
    value.len() == 31
        && value.starts_with("sha256:")
        && value[7..].bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn safe_token(value: &str, max: usize) -> bool {
    !value.is_empty()
        && value.len() <= max
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-' | b'+'))
}

fn count(value: u64) -> bool {
    value <= MAX_COUNT
}

fn bytes(value: u64) -> bool {
    value <= MAX_BYTES
}

fn duration(value: u64) -> bool {
    value <= MAX_DURATION_MS
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
        Ok(_) => return Err(io::Error::other("invalid harness telemetry directory")),
        Err(cause) if cause.kind() == io::ErrorKind::NotFound => {}
        Err(cause) => return Err(cause),
    }
    fs::create_dir(path)?;
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_dir() || redirected(&metadata) {
        return Err(io::Error::other("invalid harness telemetry directory"));
    }
    Ok(())
}

fn regular_file(path: &Path) -> io::Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_file() && !redirected(&metadata) => Ok(true),
        Ok(_) => Err(io::Error::other("invalid harness telemetry file")),
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
        return Err(io::Error::other("invalid harness telemetry file"));
    }
    Ok(file)
}

fn log_path(root: &Path, index: usize) -> PathBuf {
    if index == 0 {
        root.join(CURRENT_LOG)
    } else {
        root.join(format!("traces.{index}.jsonl"))
    }
}

fn rotate(root: &Path) -> io::Result<()> {
    for index in (1..MAX_LOG_FILES).rev() {
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

fn write_record(root: &Path, record: &Record) -> io::Result<()> {
    ensure_directory(root)?;
    let mut serialized = serde_json::to_vec(record).map_err(io::Error::other)?;
    serialized.push(b'\n');
    if serialized.len() > MAX_LINE_BYTES {
        return Err(io::Error::other("harness telemetry record is too large"));
    }
    let current = log_path(root, 0);
    let size = if regular_file(&current)? {
        fs::metadata(&current)?.len()
    } else {
        0
    };
    if size.saturating_add(serialized.len() as u64) > MAX_LOG_BYTES {
        rotate(root)?;
    }
    let mut file = append_file(&current)?;
    file.write_all(&serialized)?;
    file.flush()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_overlap_percentiles_and_event_loss_without_content() {
        let directory = tempfile::tempdir().unwrap();
        let state = state(directory.path());
        let context = TraceContext::fixture("a");
        {
            let _held = state.inner.write_lock.lock().unwrap();
            assert!(!state.record(
                &context,
                Event::PhaseFinished {
                    phase: Phase::Journal,
                    started_at: 1000,
                    duration_ms: 10,
                }
            ));
        }
        for (start, duration_ms) in [(1000, 100), (1050, 200), (1400, 50)] {
            assert!(state.record(
                &context,
                Event::PhaseFinished {
                    phase: Phase::ToolHandler,
                    started_at: start,
                    duration_ms,
                }
            ));
        }
        let report = state.report().unwrap();
        assert_eq!(report.dropped_records_current_run, 1);
        let phase = &report.phases[0];
        assert_eq!(
            phase.distribution,
            Distribution {
                samples: 3,
                p50_ms: 100,
                p95_ms: 200
            }
        );
        assert_eq!(phase.work_ms, 350);
        assert_eq!(phase.occupied_ms, 300);
    }

    fn state(root: &Path) -> TelemetryState {
        TelemetryState::new(root, "1.1.2".into(), "a".repeat(32))
    }

    #[test]
    fn schema_serializes_only_allowlisted_metadata() {
        let directory = tempfile::tempdir().unwrap();
        let state = state(directory.path());
        let context = TraceContext::fixture("b");
        let event = Event::ProviderResponse {
            provider: ProviderKind::OpenAiCodex,
            model_id: model_id("private-model-name"),
            attempt: 2,
            outcome: Outcome::Retried,
            duration_ms: 1_200,
            first_event_ms: Some(310),
            input_tokens: Some(2_000),
            output_tokens: Some(400),
            cache_read_tokens: Some(1_500),
            cache_write_tokens: None,
            failure: Some(FailureClass::Network),
        };
        assert!(state.record(&context, event));
        let serialized = fs::read_to_string(log_path(&state.inner.root, 0)).unwrap();
        assert!(serialized.contains("provider_response"));
        assert!(serialized.contains("cacheReadTokens"));
        for private in [
            "private-model-name",
            "private-conversation",
            "private-turn",
            "prompt",
            "credential",
        ] {
            assert!(!serialized.contains(private));
        }
        assert_eq!(state.records().unwrap().len(), 1);
    }

    #[test]
    fn schema_rejects_unknown_fields_and_unbounded_values() {
        let directory = tempfile::tempdir().unwrap();
        let state = state(directory.path());
        let context = TraceContext::fixture("b");
        assert!(!state.record(
            &context,
            Event::TurnFinished {
                outcome: Outcome::Succeeded,
                duration_ms: MAX_DURATION_MS + 1,
                provider_requests: 1,
                tool_calls: 0,
                compacted: false,
            },
        ));
        let encoded = serde_json::to_value(Record::new(
            &state,
            &context,
            Event::TurnStarted {
                context_items: 1,
                context_bytes: 2,
                advertised_tools: 3,
            },
        ))
        .unwrap();
        let mut object = encoded.as_object().unwrap().clone();
        object.insert("prompt".into(), serde_json::json!("secret"));
        assert!(serde_json::from_value::<Record>(object.into()).is_err());
    }

    #[test]
    fn retention_rotates_a_bounded_set_of_regular_files() {
        let directory = tempfile::tempdir().unwrap();
        let state = state(directory.path());
        let context = TraceContext::fixture("b");
        let record = Record::new(
            &state,
            &context,
            Event::TurnStarted {
                context_items: 1,
                context_bytes: 2,
                advertised_tools: 3,
            },
        );
        ensure_directory(&state.inner.root).unwrap();
        for _ in 0..24 {
            let mut serialized = serde_json::to_vec(&record).unwrap();
            serialized.push(b'\n');
            let current = log_path(&state.inner.root, 0);
            if regular_file(&current).unwrap()
                && fs::metadata(&current).unwrap().len() + serialized.len() as u64 > 512
            {
                rotate(&state.inner.root).unwrap();
            }
            append_file(&current)
                .unwrap()
                .write_all(&serialized)
                .unwrap();
        }
        assert!(log_path(&state.inner.root, 0).is_file());
        assert!(log_path(&state.inner.root, 1).is_file());
        assert!(log_path(&state.inner.root, 2).is_file());
        assert!(!log_path(&state.inner.root, 3).exists());
    }

    #[test]
    fn a_busy_or_invalid_sink_never_blocks_the_agent_loop() {
        let directory = tempfile::tempdir().unwrap();
        let state = state(directory.path());
        let context = TraceContext::fixture("b");
        let _guard = state.inner.write_lock.lock().unwrap();
        assert!(!state.record(
            &context,
            Event::TurnStarted {
                context_items: 1,
                context_bytes: 2,
                advertised_tools: 3,
            },
        ));
        assert!(!state.inner.root.exists());
    }

    #[test]
    fn sanitized_export_reparses_records_and_drops_injected_content() {
        let directory = tempfile::tempdir().unwrap();
        let state = state(directory.path());
        let context = TraceContext::fixture("b");
        assert!(state.record(
            &context,
            Event::TurnStarted {
                context_items: 1,
                context_bytes: 2,
                advertised_tools: 3,
            },
        ));
        let log = log_path(&state.inner.root, 0);
        writeln!(
            OpenOptions::new().append(true).open(log).unwrap(),
            "{{\"prompt\":\"TOP SECRET\",\"response\":\"private\"}}"
        )
        .unwrap();
        let destination = directory.path().join("trace.json");
        let result = state.export(&destination).unwrap();
        assert_eq!(result.records, 1);
        let exported = fs::read_to_string(destination).unwrap();
        assert!(exported.contains(EXPORT_FORMAT));
        assert!(exported.contains("turn_started"));
        assert!(!exported.contains("TOP SECRET"));
        assert!(!exported.contains("private"));
        assert!(!exported.contains("prompt"));
        assert!(!exported.contains("response"));
    }

    #[test]
    fn local_report_aggregates_efficiency_and_recovery_metrics() {
        let directory = tempfile::tempdir().unwrap();
        let state = state(directory.path());
        let context = TraceContext::fixture("b");
        for event in [
            Event::TurnStarted {
                context_items: 4,
                context_bytes: 800,
                advertised_tools: 3,
            },
            Event::ProviderRequest {
                provider: ProviderKind::Antigravity,
                model_id: model_id("model"),
                attempt: 1,
                input_items: 4,
                input_bytes: 800,
                advertised_tools: 3,
            },
            Event::ProviderResponse {
                provider: ProviderKind::Antigravity,
                model_id: model_id("model"),
                attempt: 1,
                outcome: Outcome::Retried,
                duration_ms: 400,
                first_event_ms: None,
                input_tokens: None,
                output_tokens: None,
                cache_read_tokens: None,
                cache_write_tokens: None,
                failure: Some(FailureClass::Network),
            },
            Event::ProviderRequest {
                provider: ProviderKind::Antigravity,
                model_id: model_id("model"),
                attempt: 2,
                input_items: 4,
                input_bytes: 800,
                advertised_tools: 3,
            },
            Event::ProviderResponse {
                provider: ProviderKind::Antigravity,
                model_id: model_id("model"),
                attempt: 2,
                outcome: Outcome::Succeeded,
                duration_ms: 600,
                first_event_ms: Some(150),
                input_tokens: Some(100),
                output_tokens: Some(20),
                cache_read_tokens: Some(75),
                cache_write_tokens: Some(5),
                failure: None,
            },
            Event::ToolFinished {
                tool: ToolKind::Read,
                tool_id: tool_id("call"),
                outcome: Outcome::Succeeded,
                duration_ms: 30,
                input_bytes: 12,
                output_bytes: 50,
                failure: None,
            },
            Event::Compaction {
                reason: CompactionReason::AutomaticThreshold,
                outcome: Outcome::Succeeded,
                source_items: 20,
                retained_items: 8,
                source_bytes: 4_000,
                retained_bytes: 1_000,
                duration_ms: 90,
                failure: None,
            },
            Event::Recovery {
                outcome: Outcome::Succeeded,
                journal_turns: 2,
                replayed_turns: 1,
                replayed_items: 8,
                replayed_bytes: 1_000,
                duration_ms: 10,
                failure: None,
            },
            Event::TurnFinished {
                outcome: Outcome::Succeeded,
                duration_ms: 1_200,
                provider_requests: 2,
                tool_calls: 1,
                compacted: true,
            },
        ] {
            assert!(state.record(&context, event));
        }
        let report = state.report().unwrap();
        assert_eq!(report.turns_finished, 1);
        assert_eq!(report.provider_requests, 2);
        assert_eq!(report.provider_retries, 1);
        assert_eq!(report.tool_calls, 1);
        assert_eq!(report.compactions, 1);
        assert_eq!(report.recoveries, 1);
        assert_eq!(report.input_tokens, 100);
        assert_eq!(report.cache_read_tokens, 75);
        assert_eq!(report.average_first_event_ms, Some(150));
        assert_eq!(report.providers[0].provider, "antigravity");
        assert!(report.summary.contains("cache hit 75.00%"));
    }
}
