//! Read-only, cached projection of durable journals. No prompt or provider wire
//! leaves the backend, and opening a Dashboard never repairs session files.
use super::*;
use std::collections::BTreeMap;
use std::path::Path;
use std::time::SystemTime;

#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Metrics {
    pub efficiency: Efficiency,
    pub turns: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub measured_steps: u64,
    pub tool_calls: u64,
    pub tool_errors: u64,
    pub duration_ms: u64,
    pub compactions: u64,
    pub changed_files: u64,
    pub models: BTreeMap<String, u64>,
    pub tools: BTreeMap<String, u64>,
    pub days: BTreeMap<u64, u64>,
}
#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Efficiency {
    pub context_searches: u64,
    pub loop_steers: u64,
    pub loop_avoided_calls: u64,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
    pub cache_read_input_tokens: u64,
    pub cache_read_requests: u64,
    pub cache_write_requests: u64,
    pub auxiliary_requests: u64,
    pub auxiliary_input_tokens: u64,
    pub auxiliary_output_tokens: u64,
    pub indexed_outputs: u64,
    pub original_bytes: u64,
    pub retained_bytes: u64,
}
impl Metrics {
    fn usage(&mut self, usage: &Usage, auxiliary: bool) {
        self.input_tokens += usage.input_tokens;
        self.output_tokens += usage.output_tokens;
        let e = &mut self.efficiency;
        if auxiliary {
            e.auxiliary_requests += 1;
            e.auxiliary_input_tokens += usage.input_tokens;
            e.auxiliary_output_tokens += usage.output_tokens;
        } else {
            self.measured_steps += 1;
        }
        if let Some(tokens) = usage.cache_read_tokens.filter(|n| *n <= usage.input_tokens) {
            e.cache_read_tokens += tokens;
            e.cache_read_input_tokens += usage.input_tokens;
            e.cache_read_requests += 1;
        }
        if let Some(tokens) = usage
            .cache_write_tokens
            .filter(|n| *n <= usage.input_tokens)
        {
            e.cache_write_tokens += tokens;
            e.cache_write_requests += 1;
        }
    }
}
#[derive(Clone)]
struct Cached {
    modified: SystemTime,
    length: u64,
    metrics: Metrics,
}
#[derive(Clone, Default)]
pub struct DashboardState(Arc<Mutex<BTreeMap<PathBuf, Cached>>>);

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentSession {
    id: String,
    title: String,
    activity: i64,
    turns: u64,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DashboardMetrics {
    project_id: String,
    sessions: usize,
    unavailable_sessions: usize,
    metrics: Metrics,
    recent: Vec<RecentSession>,
}

fn summarize(turns: &[StoredTurn], extras: &journal::Extras) -> Metrics {
    let mut metrics = Metrics {
        turns: turns.len() as u64,
        compactions: extras.context.as_ref().map_or(0, |value| value.count),
        changed_files: extras.files.len() as u64,
        ..Metrics::default()
    };
    for stored in turns {
        let turn = &stored.turn;
        metrics.duration_ms += turn.duration_ms;
        *metrics
            .models
            .entry(format!("{}/{}", turn.options.account, turn.options.model))
            .or_default() += 1;
        *metrics
            .days
            .entry(turn.created_at / 86_400_000)
            .or_default() += 1;
        for step in &turn.steps {
            metrics.efficiency.context_searches += step.context_searches;
            metrics.efficiency.loop_steers += step.loop_steers;
            metrics.efficiency.loop_avoided_calls += step.loop_avoided_calls;
            if let Some(usage) = &step.usage {
                metrics.usage(usage, false);
            }
            for reduction in &step.context_reductions {
                metrics.efficiency.indexed_outputs += 1;
                metrics.efficiency.original_bytes += reduction.original_bytes;
                metrics.efficiency.retained_bytes += reduction.retained_bytes;
            }
            for tool in &step.tools {
                metrics.tool_calls += 1;
                metrics.tool_errors += u64::from(tool.status == "error");
                *metrics.tools.entry(tool.name.clone()).or_default() += 1;
                // These native tools make separate inference requests. Do not accept
                // arbitrary MCP/page JSON as accounting data or count provider calls twice.
                if tool.status == "completed"
                    && matches!(tool.name.as_str(), "vision" | "web_search")
                {
                    if let Ok(value) = serde_json::from_str::<Value>(&tool.output) {
                        if let Some(usage) = value.get("usage").filter(|u| u.is_object()) {
                            if let Ok(usage) = serde_json::from_value::<Usage>(usage.clone()) {
                                metrics.usage(&usage, true);
                            }
                        }
                    }
                }
            }
        }
    }
    metrics
}

impl DashboardState {
    fn read(&self, path: &Path) -> Result<Metrics, AgentError> {
        let meta = std::fs::symlink_metadata(path).map_err(|_| AgentError::storage())?;
        if !meta.is_file() || meta.is_symlink() {
            return Err(AgentError::storage());
        }
        let modified = meta.modified().map_err(|_| AgentError::storage())?;
        let mut cache = self.0.lock().map_err(|_| AgentError::storage())?;
        if let Some(entry) = cache
            .get(path)
            .filter(|entry| entry.modified == modified && entry.length == meta.len())
        {
            return Ok(entry.metrics.clone());
        }
        let (turns, extras) = journal::read_only(path)?;
        let metrics = summarize(&turns, &extras);
        if cache.len() >= 256 {
            cache.clear();
        }
        cache.insert(
            path.into(),
            Cached {
                modified,
                length: meta.len(),
                metrics: metrics.clone(),
            },
        );
        Ok(metrics)
    }
}

fn merge(total: &mut Metrics, metrics: &Metrics) {
    let target = &mut total.efficiency;
    let source = &metrics.efficiency;
    target.context_searches += source.context_searches;
    target.loop_steers += source.loop_steers;
    target.loop_avoided_calls += source.loop_avoided_calls;
    target.cache_read_tokens += source.cache_read_tokens;
    target.cache_write_tokens += source.cache_write_tokens;
    target.cache_read_input_tokens += source.cache_read_input_tokens;
    target.cache_read_requests += source.cache_read_requests;
    target.cache_write_requests += source.cache_write_requests;
    target.auxiliary_requests += source.auxiliary_requests;
    target.auxiliary_input_tokens += source.auxiliary_input_tokens;
    target.auxiliary_output_tokens += source.auxiliary_output_tokens;
    target.indexed_outputs += source.indexed_outputs;
    target.original_bytes += source.original_bytes;
    target.retained_bytes += source.retained_bytes;
    total.turns += metrics.turns;
    total.input_tokens += metrics.input_tokens;
    total.output_tokens += metrics.output_tokens;
    total.measured_steps += metrics.measured_steps;
    total.tool_calls += metrics.tool_calls;
    total.tool_errors += metrics.tool_errors;
    total.duration_ms += metrics.duration_ms;
    total.compactions += metrics.compactions;
    total.changed_files += metrics.changed_files;
    for (key, value) in &metrics.models {
        *total.models.entry(key.clone()).or_default() += value;
    }
    for (key, value) in &metrics.tools {
        *total.tools.entry(key.clone()).or_default() += value;
    }
    for (key, value) in &metrics.days {
        *total.days.entry(*key).or_default() += value;
    }
}

#[tauri::command]
pub async fn get_project_metrics(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    dashboard: tauri::State<'_, DashboardState>,
    project_id: String,
) -> Result<DashboardMetrics, AgentError> {
    let home = app.path().home_dir().map_err(|_| AgentError::storage())?;
    let state = state.inner().clone();
    let dashboard = dashboard.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let sources = library::dashboard::sources(&state, &home, &project_id)?;
        let mut result = DashboardMetrics {
            project_id,
            sessions: sources.len(),
            unavailable_sessions: 0,
            metrics: Metrics::default(),
            recent: vec![],
        };
        for source in sources {
            let metrics = source
                .journal
                .as_ref()
                .ok_or_else(AgentError::storage)
                .and_then(|path| dashboard.read(path));
            if metrics.is_err() {
                result.unavailable_sessions += 1;
            }
            let metrics = metrics.unwrap_or_default();
            merge(&mut result.metrics, &metrics);
            if result.recent.len() < 5 {
                result.recent.push(RecentSession {
                    id: source.id,
                    title: source.title,
                    activity: source.activity,
                    turns: metrics.turns,
                });
            }
        }
        Ok(result)
    })
    .await
    .map_err(|_| AgentError::storage())?
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn read_only_metrics_deduplicate_checkpoints_and_preserve_running_and_crash_tail() {
        let home = tempfile::tempdir().unwrap();
        let path = home.path().join("session.jsonl");
        std::fs::write(&path, "{}\n").unwrap();
        let mut stored: StoredTurn = serde_json::from_value(json!({"turn": {
            "id":"turn-1","createdAt":86_400_000,"durationMs":1200,"user":"private prompt",
            "options":{"account":"Conta","model":"modelo","reasoning":null,"mode":"build","approvalMode":"yolo"},
            "status":"running","steps":[{"text":"private answer","summary":"","tools":[{"id":"tool","name":"ctx_search","args":{},"status":"completed","output":"private output","durationMs":100}],"usage":{"inputTokens":500,"outputTokens":50}}],"error":null
        },"wire":[{"secret":"provider"}]})).unwrap();
        journal::append(&path, &stored).unwrap();
        stored.turn.steps[0].usage.as_mut().unwrap().output_tokens = 75;
        journal::append(&path, &stored).unwrap();
        journal::append_event(
            &path,
            "context_checkpoint",
            &compaction::Checkpoint {
                count: 3,
                ..Default::default()
            },
        )
        .unwrap();
        use std::io::Write;
        std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap()
            .write_all(b"{truncated")
            .unwrap();
        let before = std::fs::read(&path).unwrap();
        let state = DashboardState::default();
        let metrics = state.read(&path).unwrap();
        assert_eq!(
            (
                metrics.turns,
                metrics.input_tokens,
                metrics.output_tokens,
                metrics.tool_calls,
                metrics.compactions
            ),
            (1, 500, 75, 1, 3)
        );
        assert_eq!(metrics.days.get(&1), Some(&1));
        assert_eq!(metrics.efficiency.cache_read_requests, 0);
        assert_eq!(state.read(&path).unwrap().turns, 1);
        assert_eq!(std::fs::read(&path).unwrap(), before);
        assert_eq!(std::fs::read_dir(home.path()).unwrap().count(), 1);
        assert!(!serde_json::to_string(&metrics).unwrap().contains("private"));
        assert_eq!(
            journal::read_only(&path).unwrap().0[0].turn.status,
            TurnStatus::Running
        );
    }
    #[test]
    fn cache_auxiliary_and_indexing_metrics_survive_reload_without_double_counting() {
        let home = tempfile::tempdir().unwrap();
        let path = home.path().join("metrics.jsonl");
        std::fs::write(&path, "{}\n").unwrap();
        let stored: StoredTurn = serde_json::from_value(json!({"turn": {
            "id":"turn","createdAt":0,"durationMs":10,"user":"request",
            "options":{"account":"test","model":"test","reasoning":null,"mode":"build","approvalMode":"yolo"},
            "status":"completed","steps":[{
                "text":"done","summary":"","contextSearches":1,"usage":{"inputTokens":1000,"outputTokens":100,"cacheReadTokens":600,"cacheWriteTokens":200},
                "contextReductions":[{"callId":"snapshot","originalBytes":10000,"retainedBytes":1000}],
                "tools":[{"id":"vision","name":"vision","args":{},"status":"completed","durationMs":1,
                    "output":json!({"usage":{"inputTokens":500,"outputTokens":50,"cacheReadTokens":0}}).to_string()}]
            }],"error":null
        },"wire":[]})).unwrap();
        journal::append(&path, &stored).unwrap();
        journal::append(&path, &stored).unwrap();
        let metrics = DashboardState::default().read(&path).unwrap();
        assert_eq!(
            (
                metrics.input_tokens,
                metrics.output_tokens,
                metrics.measured_steps
            ),
            (1500, 150, 1)
        );
        let e = &metrics.efficiency;
        assert_eq!(e.context_searches, 1);
        assert_eq!(
            (
                e.cache_read_tokens,
                e.cache_read_input_tokens,
                e.cache_read_requests
            ),
            (600, 1500, 2)
        );
        assert_eq!((e.cache_write_tokens, e.cache_write_requests), (200, 1));
        assert_eq!((e.auxiliary_requests, e.auxiliary_input_tokens), (1, 500));
        assert_eq!(
            (e.indexed_outputs, e.original_bytes, e.retained_bytes),
            (1, 10000, 1000)
        );
        let mut project = Metrics::default();
        merge(&mut project, &metrics);
        merge(&mut project, &metrics);
        assert_eq!(project.efficiency.context_searches, 2);
        assert_eq!(
            (
                project.input_tokens,
                project.efficiency.cache_read_tokens,
                project.efficiency.indexed_outputs
            ),
            (3000, 1200, 2)
        );
    }
}
