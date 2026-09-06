use super::*;

#[derive(Serialize)]
struct HandoffSummary { verdict: Verdict, summary: String }

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentCard {
    id: String, parent_id: Option<String>, role: Role, title: String, status: Status,
    updated_at: u64, created_at: u64, options: TurnOptions, bead_id: Option<String>,
    handoff: Option<HandoffSummary>, error: Option<String>, attempts: u8,
    pending_approval: Option<ToolCall>, pending_question: Option<questions::PendingQuestion>,
    active_turn_id: Option<String>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot { conversation_id: String, revision: u64, flow: Flow, agents: Vec<AgentCard> }

fn snapshot(state: &Manifest, hub: Option<&Hub>) -> Result<Snapshot, AgentError> {
    let mut agents = vec![AgentCard {
        id: "main".into(), parent_id: None, role: state.flow.root(), title: state.flow.root().label().into(), status: state.root_status,
        created_at: state.updated_at, updated_at: state.updated_at, options: state.options.clone(), bead_id: None,
        handoff: None, error: None, attempts: 1, pending_approval: None, pending_question: None, active_turn_id: None,
    }];
    if let Some(hub) = hub {
        let data = hub.root.data.lock().map_err(|_| AgentError::internal())?;
        if data.active.as_ref().is_some_and(|active| active.question.is_some() || active.approval.is_some()) {
            agents[0].status = Status::Waiting;
        }
    }
    let live = hub.map(|hub| hub.live.lock().map_err(|_| AgentError::internal())).transpose()?;
    for job in state.jobs.values().filter(|job| job.run_id == state.run_id) {
        let mut card = AgentCard { id: job.id.clone(), parent_id: Some(job.parent_id.clone()), role: job.role, title: job.title.clone(), status: job.status,
            created_at: job.created_at, updated_at: job.updated_at, options: job.options.clone(), bead_id: job.bead_id.clone(), handoff: job.handoff.as_ref().map(|handoff| HandoffSummary { verdict: handoff.verdict.clone(), summary: handoff.summary.chars().take(300).collect() }), error: job.error.clone(), attempts: job.attempts,
            pending_approval: None, pending_question: None, active_turn_id: None,
        };
        if let Some(session) = live.as_ref().and_then(|live| live.get(&job.id)) {
            let data = session.data.lock().map_err(|_| AgentError::internal())?;
            if let Some(active) = &data.active {
                card.pending_approval = active.approval.as_ref().map(|approval| approval.tool.clone());
                card.pending_question = active.question.as_ref().map(|pending| pending.request.clone());
                card.active_turn_id = Some(active.id.clone());
                if card.pending_approval.is_some() || card.pending_question.is_some() { card.status = Status::Waiting; }
            }
        }
        agents.push(card);
    }
    agents[1..].sort_by_key(|card| card.created_at);
    Ok(Snapshot { conversation_id: state.conversation_id.clone(), revision: state.revision, flow: state.flow, agents })
}
fn active_hub(agent: &AgentState, id: &str) -> Result<Arc<Hub>, AgentError> {
    agent.workflows.0.lock().map_err(|_| AgentError::internal())?.get(id).cloned().ok_or_else(AgentError::cancelled)
}
fn active_worker(agent: &AgentState, conversation: &str, id: &str) -> Result<Arc<Session>, AgentError> {
    active_hub(agent, conversation)?.live.lock().map_err(|_| AgentError::internal())?.get(id).cloned().ok_or_else(AgentError::cancelled)
}
#[tauri::command]
pub async fn get_workflow(app: tauri::AppHandle, persistence: tauri::State<'_, AppState>, agent: tauri::State<'_, AgentState>, conversation_id: String) -> Result<Option<Snapshot>, AgentError> {
    let home = app.path().home_dir().map_err(|_| AgentError::storage())?;
    let state = persistence.inner().clone(); let agent = agent.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        library::agent_location(&state, &home, &conversation_id)?;
        if let Ok(hub) = active_hub(&agent, &conversation_id) {
            let state = hub.manifest.lock().map_err(|_| AgentError::internal())?.clone();
            return snapshot(&state, Some(&hub)).map(Some);
        }
        let directory = storage::path(&home, &conversation_id)?;
        storage::load(&directory, &conversation_id)?.map(|state| snapshot(&state, None)).transpose()
    }).await.map_err(|_| AgentError::internal())?
}
#[tauri::command]
pub async fn get_workflow_transcript(app: tauri::AppHandle, persistence: tauri::State<'_, AppState>, agent: tauri::State<'_, AgentState>, conversation_id: String, agent_id: String) -> Result<ChatSnapshot, AgentError> {
    let home = app.path().home_dir().map_err(|_| AgentError::storage())?;
    let state = persistence.inner().clone(); let agent = agent.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        library::agent_location(&state, &home, &conversation_id)?;
        if agent_id == "main" { return agent.read_chat(&state, &home, &conversation_id); }
        if !storage::valid_id(&agent_id) { return Err(invalid("Agente inválido.")); }
        if let Ok(session) = active_worker(&agent, &conversation_id, &agent_id) {
            let mut snapshot = agent.histories.worker_snapshot(&session.journal, &agent_id)?;
            let live = session.snapshot()?;
            if let Some(turn) = live.turns.last() { if let Some(item) = snapshot.turns.iter_mut().find(|item| item.id == turn.id) { *item = turn.clone(); } }
            snapshot.revision = live.revision; snapshot.active_turn_id = live.active_turn_id; snapshot.context = live.context;
            return Ok(snapshot);
        }
        let directory = storage::path(&home, &conversation_id)?;
        let manifest = storage::load(&directory, &conversation_id)?.ok_or_else(|| invalid("Fluxo não encontrado."))?;
        if !manifest.jobs.contains_key(&agent_id) { return Err(invalid("Agente não pertence a esta conversa.")); }
        agent.histories.worker_snapshot(&directory.join(format!("{agent_id}.jsonl")), &agent_id)
    }).await.map_err(|_| AgentError::internal())?
}
#[tauri::command]
pub fn approve_workflow_tool(agent: tauri::State<'_, AgentState>, conversation_id: String, agent_id: String, turn_id: String, tool_id: String, approved: bool) -> Result<(), AgentError> {
    let session = active_worker(&agent, &conversation_id, &agent_id)?;
    answer_approval(&session, &turn_id, &tool_id, approved)
}
#[tauri::command]
pub fn answer_workflow_question(agent: tauri::State<'_, AgentState>, conversation_id: String, agent_id: String, turn_id: String, tool_id: String, response: questions::Response) -> Result<(), AgentError> {
    let session = active_worker(&agent, &conversation_id, &agent_id)?;
    questions::answer(&session, &turn_id, &tool_id, response).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_new_run_hides_prior_agents_without_deleting_their_history() {
        let (_fixture, hub) = super::super::tests::hub();
        let mut old = super::super::tests::job(&hub, Role::Writer, ".");
        old.status = Status::Completed;
        let current = super::super::tests::job(&hub, Role::Investigator, ".");
        let mut state = hub.manifest.lock().unwrap();
        old.run_id = "previous".into();
        state.jobs.insert(old.id.clone(), old.clone());
        state.jobs.insert(current.id.clone(), current.clone());
        let cards = snapshot(&state, None).unwrap().agents;
        assert_eq!(cards.len(), 2);
        assert_eq!(cards[0].id, "main");
        assert_eq!(cards[1].id, current.id);
        assert!(state.jobs.contains_key(&old.id));
        state.run_id = "next".into();
        assert_eq!(snapshot(&state, None).unwrap().agents.len(), 1);
        assert_eq!(state.jobs.len(), 2);
    }
    #[test]
    fn inspector_snapshots_omit_full_handoff_evidence_until_transcript_is_opened() {
        let (_fixture, hub) = super::super::tests::hub();
        let mut job = super::super::tests::job(&hub, Role::Reviewer, ".");
        job.handoff = Some(Handoff { verdict: Verdict::Approved, summary: "á".repeat(1000), outcomes: vec!["Outcome".into()], evidence: vec!["large private evidence".repeat(1000)], validation: vec![], limitations: vec![], task_ids: vec![] });
        let mut state = hub.manifest.lock().unwrap(); state.jobs.insert(job.id.clone(), job);
        let value = serde_json::to_value(snapshot(&state, None).unwrap()).unwrap();
        assert_eq!(value["agents"][1]["handoff"]["summary"].as_str().unwrap().chars().count(), 300);
        assert!(value["agents"][1]["handoff"].get("evidence").is_none());
        assert!(value.to_string().len() < 2500);
    }
}
