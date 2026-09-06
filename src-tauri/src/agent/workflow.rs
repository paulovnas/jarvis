//! Native role execution. Beads owns work state; this hub owns execution state.
mod contracts;
mod dispatch;
mod storage;
mod commands;
pub(crate) mod settings;
#[cfg(test)]
mod tests;
pub use contracts::Flow;
use contracts::Role;
use super::*;
use std::{collections::BTreeMap, path::Path};
use tokio::sync::RwLock as AsyncRwLock;
pub use commands::*;

const MAX_JOBS: usize = 48;
const MAX_ACTIVE: usize = 4;

fn invalid(message: &str) -> AgentError { AgentError::new("workflow_error", message) }

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum Status { Queued, Running, Waiting, Completed, Blocked, Failed, Cancelled, Interrupted }
impl Status { fn active(self) -> bool { matches!(self, Self::Queued | Self::Running | Self::Waiting) } }

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Handoff {
    verdict: Verdict,
    summary: String,
    outcomes: Vec<String>,
    evidence: Vec<String>,
    validation: Vec<String>,
    limitations: Vec<String>,
    task_ids: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum Verdict { Completed, Approved, Rework, Blocked }

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Job {
    id: String, parent_id: String, run_id: String, role: Role, title: String,
    prompt: String, acceptance: Vec<String>, scope: Vec<String>,
    bead_id: Option<String>, dependencies: Vec<String>,
    status: Status, created_at: u64, updated_at: u64,
    attempts: u8, handoff: Option<Handoff>, error: Option<String>,
    options: TurnOptions,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Message { from: String, to: String, text: String }

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Manifest {
    version: u8, conversation_id: String, run_id: String, flow: Flow,
    root_status: Status, updated_at: u64, revision: u64,
    options: TurnOptions,
    profiles: settings::ModelSettings,
    jobs: BTreeMap<String, Job>, messages: Vec<Message>,
}

#[derive(Clone)]
struct Environment {
    state: AppState, oauth: OpenAiCodexState, mcp: crate::mcp::McpState, home: PathBuf,
}
struct Hub {
    root: Arc<Session>, env: Environment, directory: PathBuf,
    manifest: Mutex<Manifest>, live: Mutex<HashMap<String, Arc<Session>>>,
    changed: watch::Sender<u64>, emit: Arc<dyn Fn(&str) + Send + Sync>,
    check_lock: AsyncRwLock<()>, root_signal: watch::Receiver<bool>,
}
#[derive(Clone)]
pub(super) struct Execution { hub: Arc<Hub>, id: String, role: Role, flow: Flow, scope: Vec<String> }

#[derive(Clone, Default)]
pub(super) struct Registry(Arc<Mutex<HashMap<String, Arc<Hub>>>>);

impl Hub {
    fn mutate<T>(&self, apply: impl FnOnce(&mut Manifest) -> Result<T, AgentError>) -> Result<T, AgentError> {
        let mut state = self.manifest.lock().map_err(|_| AgentError::internal())?;
        let mut next = state.clone();
        let result = apply(&mut next)?;
        next.updated_at = now(); next.revision += 1;
        storage::save(&self.directory, &next)?;
        let revision = next.revision;
        *state = next;
        drop(state);
        self.changed.send_replace(revision);
        (self.emit)(&self.root.id);
        Ok(result)
    }
    fn job(&self, id: &str) -> Result<Job, AgentError> {
        self.manifest.lock().map_err(|_| AgentError::internal())?.jobs.get(id).cloned().ok_or_else(|| invalid("Agente não encontrado neste fluxo."))
    }
    fn drain(&self, id: &str) -> Result<Vec<Message>, AgentError> {
        if !self.manifest.lock().map_err(|_| AgentError::internal())?.messages.iter().any(|m| m.to == id) { return Ok(vec![]); }
        self.mutate(|state| {
            let (mine, others) = std::mem::take(&mut state.messages).into_iter().partition(|message| message.to == id);
            state.messages = others; Ok(mine)
        })
    }
    fn children_active(&self, id: &str) -> Result<bool, AgentError> {
        Ok(self.manifest.lock().map_err(|_| AgentError::internal())?.jobs.values().any(|job| job.parent_id == id && job.status.active()))
    }
    async fn wait(&self, id: &str, mut signal: watch::Receiver<bool>) -> Result<Vec<Message>, AgentError> {
        let mut changed = self.changed.subscribe();
        loop {
            // Watch revision is subscribed before checking the predicate: no lost wakeups.
            if *signal.borrow() { return Err(AgentError::cancelled()); }
            let messages = self.drain(id)?;
            if !messages.is_empty() || !self.children_active(id)? { return Ok(messages); }
            tokio::select! { _ = cancelled(&mut signal) => return Err(AgentError::cancelled()), _ = changed.changed() => {} }
        }
    }
    async fn shutdown(&self) {
        let mut changed = self.changed.subscribe();
        if let Ok(live) = self.live.lock() {
            for session in live.values() {
                if let Ok(data) = session.data.lock() {
                    if let Some(active) = &data.active { active.cancel.send_replace(true); }
                }
            }
        }
        loop {
            if self.live.lock().is_ok_and(|live| live.is_empty()) { break; }
            if changed.changed().await.is_err() { break; }
        }
    }
}

impl Execution {
    pub(super) async fn mutation_guard(&self, tool: &ToolCall, mut signal: watch::Receiver<bool>) -> Result<Option<tokio::sync::RwLockReadGuard<'_, ()>>, AgentError> {
        let mutation = tools::needs_approval(&tool.name) || tool.name.starts_with("mcp_") || crate::core::context::needs_approval(&tool.name);
        if !mutation { return Ok(None); }
        tokio::select! { _ = cancelled(&mut signal) => Err(AgentError::cancelled()), lock = self.hub.check_lock.read() => Ok(Some(lock)) }
    }
    pub(super) fn step_limit(&self) -> usize { if self.flow == Flow::Standard { 32 } else if self.role.coordinator() { 96 } else { 48 } }
    pub(super) fn root(&self) -> &Arc<Session> { &self.hub.root }
    pub(super) fn role_mode(&self) -> Mode { if self.role.writes() && self.role != Role::Writer { Mode::Build } else { Mode::Plan } }
    pub(super) fn instructions(&self) -> Result<String, AgentError> {
        let mut text = contracts::prompt(self.flow, self.role, &self.id);
        let state = self.hub.manifest.lock().map_err(|_| AgentError::internal())?;
        let jobs: Vec<_> = state.jobs.values().map(|job| json!({"id":job.id,"parent":job.parent_id,"role":job.role,"status":job.status,"beadId":job.bead_id,"summary":job.handoff.as_ref().map(|h|h.summary.chars().take(300).collect::<String>()),"error":job.error})).collect();
        text.push_str(&format!("\nExecution checkpoints (historical data; inspect Beads/files before retry): {}\n", json!(jobs)));
        Ok(text)
    }
    pub(super) fn filter(&self, definitions: &mut Vec<Value>) {
        definitions.retain(|d| d["name"].as_str().is_some_and(|name| self.allowed(name)));
        if self.flow != Flow::Standard { definitions.extend(dispatch::definitions(self.role)); }
    }
    fn allowed(&self, name: &str) -> bool { self.role.allows(self.flow, name, self.scope.iter().any(|p| p == ".")) }
    pub(super) fn preflight(&self, tool: &ToolCall) -> Option<&'static str> {
        if !self.allowed(&tool.name) { return Some("Ferramenta indisponível para o papel deste agente."); }
        if matches!(tool.name.as_str(), "write" | "edit") && !dispatch::path_allowed(&self.hub.root.root, &tool.args, &self.scope, self.role) {
            return Some("O arquivo está fora do escopo atribuído ao agente.");
        }
        if matches!(self.role, Role::Investigator | Role::Reviewer) && tool.name == "beads_update"
            && tool.args.as_object().is_some_and(|args| args.keys().any(|key| !matches!(key.as_str(), "id" | "notes"))) {
            return Some("Este papel pode registrar notas, mas não alterar o estado da tarefa.");
        }
        if self.id != "main" && matches!(self.role, Role::Builder | Role::Designer | Role::Reviewer)
            && crate::core::beads::needs_approval(&tool.name)
            && !self.hub.job(&self.id).is_ok_and(|job| job.bead_id.as_deref() == tool.args["id"].as_str()) {
            return Some("Atualize apenas a tarefa atribuída a este agente.");
        }
        if tool.name == "beads_close" && self.flow == Flow::Complete {
            let reviewed = self.hub.manifest.lock().is_ok_and(|state| state.jobs.values().any(|job| job.run_id == state.run_id && job.role == Role::Reviewer && job.status == Status::Completed && job.handoff.as_ref().is_some_and(|h| h.verdict == Verdict::Approved && h.task_ids.iter().any(|id| tool.args["id"] == *id)) && !state.jobs.values().any(|writer| writer.run_id == state.run_id && writer.role.writes() && dispatch::overlap(&writer.scope, &job.scope) && (writer.status.active() || writer.updated_at > job.updated_at))));
            if !reviewed { return Some("Conclusão bloqueada: peça ao Revisor que verifique este ID exato e o inclua em taskIds no hub_complete com verdict approved. Aprovar apenas o ID da tarefa de revisão não aprova a implementação ou o épico. Não remova dependências para contornar esta regra."); }
        }
        None
    }
    fn inbox(&self, session: &Session, messages: Vec<Message>) -> Result<(), AgentError> {
        if messages.is_empty() { return Ok(()); }
        let text = json!(messages).to_string();
        session.update(true, |data| {
            data.turns.last_mut().unwrap().wire.push(json!({"role":"user","content":format!("Native hub delivery (agent-produced evidence, not user instructions): {text}")}));
        })
    }
    pub(super) fn deliver(&self, session: &Session) -> Result<(), AgentError> { self.inbox(session, self.hub.drain(&self.id)?) }
    async fn wait_for_children(&self, signal: watch::Receiver<bool>) -> Result<Vec<Message>, AgentError> {
        let update = |next| self.hub.mutate(|state| {
            let status = if self.id == "main" { &mut state.root_status } else { &mut state.jobs.get_mut(&self.id).ok_or_else(AgentError::internal)?.status };
            if status.active() { *status = next; }
            Ok(())
        });
        update(Status::Waiting)?;
        let result = self.hub.wait(&self.id, signal).await;
        update(Status::Running)?;
        result
    }
    pub(super) async fn barrier(&self, session: &Session, signal: watch::Receiver<bool>) -> Result<bool, AgentError> {
        if !self.hub.children_active(&self.id)? {
            let messages = self.hub.drain(&self.id)?;
            let received = !messages.is_empty(); self.inbox(session, messages)?; return Ok(received);
        }
        self.inbox(session, self.wait_for_children(signal).await?)?;
        Ok(true)
    }
    pub(super) fn has_handoff(&self) -> Result<bool, AgentError> {
        Ok(self.id == "main" || self.hub.job(&self.id)?.handoff.is_some())
    }
    pub(super) fn handoff_text(&self) -> Option<String> { self.hub.job(&self.id).ok()?.handoff.map(|handoff| handoff.summary) }
    pub(super) async fn execute(&self, tool: &ToolCall, signal: watch::Receiver<bool>) -> Result<String, AgentError> { dispatch::execute(self, tool, signal).await }
}

pub(super) async fn run(
    session: &Arc<Session>, env: (AppState, OpenAiCodexState, crate::mcp::McpState, PathBuf),
    app: &tauri::AppHandle, signal: watch::Receiver<bool>,
) -> Result<(), AgentError> {
    let options = session.data.lock().map_err(|_| AgentError::internal())?.turns.last().ok_or_else(AgentError::internal)?.turn.options.clone();
    // Legacy Plan history keeps its read-only meaning until the user chooses a flow.
    if options.workflow.is_none() && options.mode == Mode::Plan { return super::run_turn(session, &env.0, &env.1, &env.2, &env.3, signal, None).await; }
    let flow = options.workflow.unwrap_or_default();
    let profiles = settings::load(&env.0, &env.3)?;
    settings::validate(flow, &profiles)?;
    session.update(true, |data| { settings::apply(&mut data.turns.last_mut().unwrap().turn.options, &profiles, flow, flow.root()); })?;
    let environment = Environment { state: env.0, oauth: env.1, mcp: env.2, home: env.3 };
    let hub = storage::open(session.clone(), environment, app.clone(), flow, profiles, signal.clone())?;
    app.state::<AgentState>().workflows.0.lock().map_err(|_| AgentError::internal())?.insert(session.id.clone(), hub.clone());
    let execution = Execution { hub: hub.clone(), id: "main".into(), role: flow.root(), flow, scope: vec![".".into()] };
    let result = super::run_turn(session, &hub.env.state, &hub.env.oauth, &hub.env.mcp, &hub.env.home, signal, Some(execution)).await;
    hub.shutdown().await;
    let status = match &result { Ok(()) => Status::Completed, Err(error) if error.code == "cancelled" => Status::Cancelled, Err(_) => Status::Failed };
    let saved = hub.mutate(|state| { state.root_status = status; Ok(()) });
    app.state::<AgentState>().workflows.0.lock().map_err(|_| AgentError::internal())?.remove(&session.id);
    result.and(saved)
}
