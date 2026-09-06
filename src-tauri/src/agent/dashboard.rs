//! Read-only, cached projection of durable journals. No prompt or provider wire
//! leaves the backend, and opening a Dashboard never repairs session files.
use super::*;
use std::collections::BTreeMap;
use std::time::SystemTime;
use std::path::Path;

#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Metrics {
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
        *metrics.models.entry(format!("{}/{}", turn.options.account, turn.options.model)).or_default() += 1;
        *metrics.days.entry(turn.created_at / 86_400_000).or_default() += 1;
        for step in &turn.steps {
            if let Some(usage) = &step.usage {
                metrics.input_tokens += usage.input_tokens;
                metrics.output_tokens += usage.output_tokens;
                metrics.measured_steps += 1;
            }
            for tool in &step.tools {
                metrics.tool_calls += 1;
                metrics.tool_errors += u64::from(tool.status == "error");
                *metrics.tools.entry(tool.name.clone()).or_default() += 1;
            }
        }
    }
    metrics
}

impl DashboardState {
    fn read(&self, path: &Path) -> Result<Metrics, AgentError> {
        let meta = std::fs::symlink_metadata(path).map_err(|_| AgentError::storage())?;
        if !meta.is_file() || meta.is_symlink() { return Err(AgentError::storage()); }
        let modified = meta.modified().map_err(|_| AgentError::storage())?;
        let mut cache = self.0.lock().map_err(|_| AgentError::storage())?;
        if let Some(entry) = cache.get(path).filter(|entry| entry.modified == modified && entry.length == meta.len()) {
            return Ok(entry.metrics.clone());
        }
        let (turns, extras) = journal::read_only(path)?;
        let metrics = summarize(&turns, &extras);
        if cache.len() >= 256 { cache.clear(); }
        cache.insert(path.into(), Cached { modified, length: meta.len(), metrics: metrics.clone() });
        Ok(metrics)
    }
}

fn merge(total: &mut Metrics, metrics: &Metrics) {
    total.turns += metrics.turns;
    total.input_tokens += metrics.input_tokens;
    total.output_tokens += metrics.output_tokens;
    total.measured_steps += metrics.measured_steps;
    total.tool_calls += metrics.tool_calls;
    total.tool_errors += metrics.tool_errors;
    total.duration_ms += metrics.duration_ms;
    total.compactions += metrics.compactions;
    total.changed_files += metrics.changed_files;
    for (key, value) in &metrics.models { *total.models.entry(key.clone()).or_default() += value; }
    for (key, value) in &metrics.tools { *total.tools.entry(key.clone()).or_default() += value; }
    for (key, value) in &metrics.days { *total.days.entry(*key).or_default() += value; }
}

#[tauri::command]
pub async fn get_project_metrics(app: tauri::AppHandle, state: tauri::State<'_, AppState>, dashboard: tauri::State<'_, DashboardState>, project_id: String) -> Result<DashboardMetrics, AgentError> {
    let home = app.path().home_dir().map_err(|_| AgentError::storage())?;
    let state = state.inner().clone();
    let dashboard = dashboard.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let sources = library::dashboard::sources(&state, &home, &project_id)?;
        let mut result = DashboardMetrics { project_id, sessions: sources.len(), unavailable_sessions: 0, metrics: Metrics::default(), recent: vec![] };
        for source in sources {
            let metrics = source.journal.as_ref().ok_or_else(AgentError::storage).and_then(|path| dashboard.read(path));
            if metrics.is_err() { result.unavailable_sessions += 1; }
            let metrics = metrics.unwrap_or_default();
            merge(&mut result.metrics, &metrics);
            if result.recent.len() < 5 {
                result.recent.push(RecentSession { id: source.id, title: source.title, activity: source.activity, turns: metrics.turns });
            }
        }
        Ok(result)
    }).await.map_err(|_| AgentError::storage())?
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
        journal::append_event(&path, "context_checkpoint", &compaction::Checkpoint { count: 3, ..Default::default() }).unwrap();
        use std::io::Write;
        std::fs::OpenOptions::new().append(true).open(&path).unwrap().write_all(b"{truncated").unwrap();
        let before = std::fs::read(&path).unwrap();
        let state = DashboardState::default();
        let metrics = state.read(&path).unwrap();
        assert_eq!((metrics.turns, metrics.input_tokens, metrics.output_tokens, metrics.tool_calls, metrics.compactions), (1,500,75,1,3));
        assert_eq!(metrics.days.get(&1), Some(&1));
        assert_eq!(state.read(&path).unwrap().turns, 1);
        assert_eq!(std::fs::read(&path).unwrap(), before);
        assert_eq!(std::fs::read_dir(home.path()).unwrap().count(), 1);
        assert!(!serde_json::to_string(&metrics).unwrap().contains("private"));
        assert_eq!(journal::read_only(&path).unwrap().0[0].turn.status, TurnStatus::Running);
    }
}
