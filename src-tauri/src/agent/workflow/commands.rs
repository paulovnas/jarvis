use super::*;

#[derive(Serialize)]
struct HandoffSummary {
    verdict: Verdict,
    summary: String,
}

#[derive(Serialize)]
struct AgentIdentity {
    name: String,
    appearance: Option<catalog::Appearance>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RecoveryEffect {
    agent_id: String,
    agent_title: String,
    tool: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RecoverySummary {
    run_id: String,
    affected_agents: usize,
    uncertain_actions: Vec<RecoveryEffect>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentCard {
    id: String,
    parent_id: Option<String>,
    role: Role,
    title: String,
    status: Status,
    updated_at: u64,
    created_at: u64,
    started_at: u64,
    duration_ms: u64,
    current_thought: Option<String>,
    options: TurnOptions,
    bead_id: Option<String>,
    handoff: Option<HandoffSummary>,
    error: Option<String>,
    attempts: u8,
    pending_approval: Option<ToolCall>,
    pending_question: Option<questions::PendingQuestion>,
    pending_authoring: Option<authoring::PendingProposal>,
    active_turn_id: Option<String>,
    identity: Option<AgentIdentity>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    conversation_id: String,
    revision: u64,
    flow: Flow,
    agents: Vec<AgentCard>,
    validation: Option<validation::Batch>,
    #[serde(skip_serializing_if = "Option::is_none")]
    recovery: Option<RecoverySummary>,
}

fn live_telemetry(data: &SessionData) -> (u64, u64, Option<String>) {
    let Some(turn) = data.turns.last() else {
        return (0, 0, None);
    };
    let running = data
        .active
        .as_ref()
        .is_some_and(|active| active.id == turn.turn.id);
    let duration = if running {
        now().saturating_sub(turn.turn.created_at)
    } else {
        turn.turn.duration_ms
    };
    let thought = running
        .then(|| {
            turn.turn
                .steps
                .iter()
                .rev()
                .find_map(|step| (!step.summary.trim().is_empty()).then_some(step.summary.trim()))
                .unwrap_or_default()
                .chars()
                .take(2_000)
                .collect::<String>()
        })
        .filter(|thought| !thought.is_empty());
    (turn.turn.created_at, duration, thought)
}

fn snapshot(state: &Manifest, hub: Option<&Hub>) -> Result<Snapshot, AgentError> {
    let mut agents = vec![AgentCard {
        id: "main".into(),
        parent_id: None,
        role: state.flow.root(),
        identity: state.custom_agent.as_ref().map_or_else(
            || {
                state
                    .custom_definition
                    .as_ref()
                    .filter(|_| state.flow == Flow::Custom)
                    .map(|definition| AgentIdentity {
                        name: definition.flow.name.clone(),
                        appearance: definition.flow.appearance,
                    })
            },
            |agent| {
                Some(AgentIdentity {
                    name: agent.name.clone(),
                    appearance: agent.appearance,
                })
            },
        ),
        title: state.custom_agent.as_ref().map_or_else(
            || {
                state
                    .custom_definition
                    .as_ref()
                    .filter(|_| state.flow == Flow::Custom)
                    .map_or_else(|| state.flow.root().label().into(), |d| d.flow.name.clone())
            },
            |agent| agent.name.clone(),
        ),
        status: state.root_status,
        created_at: state.updated_at,
        updated_at: state.updated_at,
        started_at: state.updated_at,
        duration_ms: 0,
        current_thought: None,
        options: state.options.clone(),
        bead_id: None,
        handoff: None,
        error: None,
        attempts: 1,
        pending_approval: None,
        pending_question: None,
        pending_authoring: None,
        active_turn_id: None,
    }];
    if let Some(hub) = hub {
        let data = hub.root.data.lock().map_err(|_| AgentError::internal())?;
        let (started_at, duration_ms, current_thought) = live_telemetry(&data);
        if started_at > 0 {
            agents[0].started_at = started_at;
        }
        agents[0].duration_ms = duration_ms;
        agents[0].current_thought = current_thought;
        if data.active.as_ref().is_some_and(|active| {
            active.question.is_some() || active.approval.is_some() || active.authoring.is_some()
        }) {
            agents[0].status = Status::Waiting;
        }
    }
    let live = hub
        .map(|hub| hub.live.lock().map_err(|_| AgentError::internal()))
        .transpose()?;
    let mut current_jobs: Vec<&Job> = vec![];
    for job in state.jobs.values().filter(|job| job.run_id == state.run_id) {
        if let Some(index) = current_jobs.iter().position(|current| {
            state.flow != Flow::Custom && current.role == job.role && job.role != Role::Custom
        }) {
            let current = current_jobs[index];
            if (job.created_at, job.updated_at, job.id.as_str())
                > (current.created_at, current.updated_at, current.id.as_str())
            {
                current_jobs[index] = job;
            }
        } else {
            current_jobs.push(job);
        }
    }
    for job in current_jobs {
        let mut card = AgentCard {
            id: job.id.clone(),
            parent_id: Some(job.parent_id.clone()),
            role: job.role,
            identity: job.custom_agent.as_ref().map(|agent| AgentIdentity {
                name: agent.name.clone(),
                appearance: agent.appearance,
            }),
            title: job.title.clone(),
            status: job.status,
            created_at: job.created_at,
            updated_at: job.updated_at,
            started_at: job.updated_at.saturating_sub(job.duration_ms),
            duration_ms: job.duration_ms,
            current_thought: None,
            options: job.options.clone(),
            bead_id: job.bead_id.clone(),
            handoff: job.handoff.as_ref().map(|handoff| HandoffSummary {
                verdict: handoff.verdict.clone(),
                summary: handoff.summary.chars().take(300).collect(),
            }),
            error: job.error.clone(),
            attempts: job.attempts,
            pending_approval: None,
            pending_question: None,
            pending_authoring: None,
            active_turn_id: None,
        };
        if let Some(session) = live.as_ref().and_then(|live| live.get(&job.id)) {
            let data = session.data.lock().map_err(|_| AgentError::internal())?;
            let (started_at, duration_ms, current_thought) = live_telemetry(&data);
            if started_at > 0 {
                card.started_at = started_at;
            }
            card.duration_ms = duration_ms;
            card.current_thought = current_thought;
            if let Some(active) = &data.active {
                card.pending_approval = active
                    .approval
                    .as_ref()
                    .map(|approval| approval.tool.clone());
                card.pending_question = active
                    .question
                    .as_ref()
                    .map(|pending| pending.request.clone());
                card.pending_authoring = active
                    .authoring
                    .as_ref()
                    .map(|pending| pending.request.clone());
                card.active_turn_id = Some(active.id.clone());
                if card.pending_approval.is_some()
                    || card.pending_question.is_some()
                    || card.pending_authoring.is_some()
                {
                    card.status = Status::Waiting;
                }
            }
        }
        agents.push(card);
    }
    agents[1..].sort_by(|left, right| {
        (left.created_at, left.updated_at, left.id.as_str()).cmp(&(
            right.created_at,
            right.updated_at,
            right.id.as_str(),
        ))
    });
    Ok(Snapshot {
        conversation_id: state.conversation_id.clone(),
        revision: state.revision,
        flow: state.flow,
        agents,
        validation: state
            .validation
            .clone()
            .filter(|batch| !state.flow.direct() && batch.flow == state.flow),
        recovery: None,
    })
}

fn recovery_summary(
    state: &Manifest,
    root_journal: &Path,
    directory: &Path,
) -> Result<Option<RecoverySummary>, AgentError> {
    if state.root_status != Status::Interrupted
        || !matches!(state.flow, Flow::Planned | Flow::Complete)
    {
        return Ok(None);
    }
    let Some(root) = journal::read_only(root_journal)?.0.pop() else {
        return Ok(None);
    };
    if !super::super::resumable_workflow_turn(&root) || root.turn.id != state.run_id {
        return Ok(None);
    }
    let mut affected_agents = 1;
    let mut uncertain_actions = journal::uncertain_tool_names(&root)
        .into_iter()
        .take(32)
        .map(|tool| RecoveryEffect {
            agent_id: "main".into(),
            agent_title: state.flow.root().label().into(),
            tool,
        })
        .collect::<Vec<_>>();
    for job in state
        .jobs
        .values()
        .filter(|job| job.run_id == state.run_id && job.status == Status::Interrupted)
    {
        let path = directory.join(format!("{}.jsonl", job.id));
        if !path.exists() {
            affected_agents += 1;
            continue;
        }
        let Some(turn) = journal::read_only(&path)?.0.pop() else {
            affected_agents += 1;
            continue;
        };
        if !super::super::resumable_workflow_turn(&turn) {
            continue;
        }
        affected_agents += 1;
        let remaining = 32usize.saturating_sub(uncertain_actions.len());
        uncertain_actions.extend(
            journal::uncertain_tool_names(&turn)
                .into_iter()
                .take(remaining)
                .map(|tool| RecoveryEffect {
                    agent_id: job.id.clone(),
                    agent_title: job.title.clone(),
                    tool,
                }),
        );
    }
    Ok(Some(RecoverySummary {
        run_id: state.run_id.clone(),
        affected_agents,
        uncertain_actions,
    }))
}
fn active_hub(agent: &AgentState, id: &str) -> Result<Arc<Hub>, AgentError> {
    agent
        .workflows
        .0
        .lock()
        .map_err(|_| AgentError::internal())?
        .get(id)
        .cloned()
        .ok_or_else(AgentError::cancelled)
}
fn active_worker(
    agent: &AgentState,
    conversation: &str,
    id: &str,
) -> Result<Arc<Session>, AgentError> {
    active_hub(agent, conversation)?
        .live
        .lock()
        .map_err(|_| AgentError::internal())?
        .get(id)
        .cloned()
        .ok_or_else(AgentError::cancelled)
}
#[tauri::command]
pub async fn get_workflow(
    app: tauri::AppHandle,
    persistence: tauri::State<'_, AppState>,
    agent: tauri::State<'_, AgentState>,
    conversation_id: String,
) -> Result<Option<Snapshot>, AgentError> {
    let home = app.path().home_dir().map_err(|_| AgentError::storage())?;
    let state = persistence.inner().clone();
    let agent = agent.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let (journal, _) = library::agent_location(&state, &home, &conversation_id)?;
        if let Ok(hub) = active_hub(&agent, &conversation_id) {
            let state = hub
                .manifest
                .lock()
                .map_err(|_| AgentError::internal())?
                .clone();
            return snapshot(&state, Some(&hub)).map(Some);
        }
        let directory = storage::path(&home, &conversation_id)?;
        storage::load(&directory, &conversation_id)?
            .map(|mut state| {
                if let Some(batch) = &mut state.validation {
                    batch.submitted |= agent.histories.has_turn(&journal, &batch.id)?;
                }
                let mut view = snapshot(&state, None)?;
                view.recovery = recovery_summary(&state, &journal, &directory)?;
                Ok(view)
            })
            .transpose()
    })
    .await
    .map_err(|_| AgentError::internal())?
}
#[tauri::command]
pub async fn get_workflow_transcript(
    app: tauri::AppHandle,
    persistence: tauri::State<'_, AppState>,
    agent: tauri::State<'_, AgentState>,
    conversation_id: String,
    agent_id: String,
) -> Result<ChatSnapshot, AgentError> {
    let home = app.path().home_dir().map_err(|_| AgentError::storage())?;
    let state = persistence.inner().clone();
    let agent = agent.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        library::agent_location(&state, &home, &conversation_id)?;
        if agent_id == "main" {
            return agent.read_chat(&state, &home, &conversation_id);
        }
        if !storage::valid_id(&agent_id) {
            return Err(invalid("Agente inválido."));
        }
        if let Ok(session) = active_worker(&agent, &conversation_id, &agent_id) {
            let mut snapshot = agent
                .histories
                .worker_snapshot(&session.journal, &agent_id)?;
            let live = session.snapshot()?;
            if let Some(turn) = live.turns.last() {
                if let Some(item) = snapshot.turns.iter_mut().find(|item| item.id == turn.id) {
                    *item = turn.clone();
                }
            }
            snapshot.revision = live.revision;
            snapshot.active_turn_id = live.active_turn_id;
            snapshot.context = live.context;
            return Ok(snapshot);
        }
        let directory = storage::path(&home, &conversation_id)?;
        let manifest = storage::load(&directory, &conversation_id)?
            .ok_or_else(|| invalid("Fluxo não encontrado."))?;
        if !manifest.jobs.contains_key(&agent_id) {
            return Err(invalid("Agente não pertence a esta conversa."));
        }
        agent
            .histories
            .worker_snapshot(&directory.join(format!("{agent_id}.jsonl")), &agent_id)
    })
    .await
    .map_err(|_| AgentError::internal())?
}
#[tauri::command]
pub fn approve_workflow_tool(
    agent: tauri::State<'_, AgentState>,
    conversation_id: String,
    agent_id: String,
    turn_id: String,
    tool_id: String,
    approved: bool,
) -> Result<(), AgentError> {
    let session = active_worker(&agent, &conversation_id, &agent_id)?;
    answer_approval(&session, &turn_id, &tool_id, approved)
}
#[tauri::command]
pub fn answer_workflow_question(
    agent: tauri::State<'_, AgentState>,
    conversation_id: String,
    agent_id: String,
    turn_id: String,
    tool_id: String,
    response: questions::Response,
) -> Result<(), AgentError> {
    let session = active_worker(&agent, &conversation_id, &agent_id)?;
    questions::answer(&session, &turn_id, &tool_id, response).map(|_| ())
}

#[tauri::command]
pub async fn answer_workflow_authoring(
    app: tauri::AppHandle,
    persistence: tauri::State<'_, AppState>,
    agent: tauri::State<'_, AgentState>,
    conversation_id: String,
    agent_id: String,
    decision: authoring::Decision,
) -> Result<(), AgentError> {
    use tauri::Manager;
    let home = app.path().home_dir().map_err(|_| AgentError::storage())?;
    let persistence = persistence.inner().clone();
    let agent = agent.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let session = active_worker(&agent, &conversation_id, &agent_id)?;
        authoring::answer(&app, &persistence, &home, &session, decision).map(|_| ())
    })
    .await
    .map_err(|_| AgentError::internal())?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recovery_summary_counts_a_worker_with_only_a_journal_header() {
        let (_fixture, hub) = super::super::tests::hub();
        let mut worker = super::super::tests::job(&hub, Role::Builder, "src");
        worker.status = Status::Interrupted;
        let worker_path = hub.directory.join(format!("{}.jsonl", worker.id));
        std::fs::write(
            worker_path,
            format!(
                "{}\n",
                json!({"type":"agent", "version":1,"id":worker.id,"conversationId":hub.root.id})
            ),
        )
        .unwrap();
        let run_id = hub
            .root
            .data
            .lock()
            .unwrap()
            .turns
            .last()
            .unwrap()
            .turn
            .id
            .clone();
        hub.root
            .update(true, |data| {
                let turn = &mut data.turns.last_mut().unwrap().turn;
                turn.status = TurnStatus::Interrupted;
                turn.error = Some(AgentError::new(
                    "interrupted",
                    "O Jarvis foi encerrado durante esta execução.",
                ));
            })
            .unwrap();
        let mut state = hub.manifest.lock().unwrap();
        state.run_id = run_id.clone();
        state.root_status = Status::Interrupted;
        worker.run_id = run_id;
        state.jobs.insert(worker.id.clone(), worker);

        let summary = recovery_summary(&state, &hub.root.journal, &hub.directory)
            .unwrap()
            .unwrap();

        assert_eq!(summary.affected_agents, 2);
    }

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
    fn inspector_snapshots_keep_only_the_latest_card_for_each_role() {
        let (_fixture, hub) = super::super::tests::hub();
        let mut first = super::super::tests::job(&hub, Role::Designer, ".");
        first.created_at = 10;
        first.updated_at = 20;
        let mut latest = super::super::tests::job(&hub, Role::Designer, ".");
        latest.created_at = 30;
        latest.updated_at = 30;
        let first_id = first.id.clone();
        let latest_id = latest.id.clone();
        let mut state = hub.manifest.lock().unwrap();
        state.jobs.insert(first_id.clone(), first);
        state.jobs.insert(latest_id.clone(), latest);
        let cards = snapshot(&state, None).unwrap().agents;
        assert_eq!(cards.len(), 2);
        assert_eq!(cards[1].id, latest_id);
        assert!(state.jobs.contains_key(&first_id));
    }

    #[test]
    fn live_worker_snapshots_report_elapsed_time_and_current_thought() {
        let (_fixture, hub) = super::super::tests::hub();
        let mut worker = super::super::tests::job(&hub, Role::Builder, ".");
        worker.status = Status::Running;
        let worker_id = worker.id.clone();
        let started_at = now().saturating_sub(2_000);
        {
            let mut data = hub.root.data.lock().unwrap();
            let turn = data.turns.last_mut().unwrap();
            turn.turn.created_at = started_at;
            turn.turn.steps.push(Step {
                summary: "Conferindo o contrato antes de editar".into(),
                ..Step::default()
            });
        }
        hub.live
            .lock()
            .unwrap()
            .insert(worker_id.clone(), hub.root.clone());
        let mut state = hub.manifest.lock().unwrap();
        state.jobs.insert(worker_id, worker);

        let value = serde_json::to_value(snapshot(&state, Some(&hub)).unwrap()).unwrap();
        let card = &value["agents"][1];
        assert_eq!(card["startedAt"], started_at);
        assert!(card["durationMs"].as_u64().unwrap() >= 2_000);
        assert_eq!(
            card["currentThought"],
            "Conferindo o contrato antes de editar"
        );
    }
    #[test]
    fn inspector_snapshots_omit_full_handoff_evidence_until_transcript_is_opened() {
        let (_fixture, hub) = super::super::tests::hub();
        let mut job = super::super::tests::job(&hub, Role::Reviewer, ".");
        job.handoff = Some(Handoff {
            verdict: Verdict::Approved,
            summary: "á".repeat(1000),
            outcomes: vec!["Outcome".into()],
            evidence: vec!["large private evidence".repeat(1000)],
            validation: vec![],
            limitations: vec![],
            task_ids: vec![],
        });
        let mut state = hub.manifest.lock().unwrap();
        state.jobs.insert(job.id.clone(), job);
        let value = serde_json::to_value(snapshot(&state, None).unwrap()).unwrap();
        assert_eq!(
            value["agents"][1]["handoff"]["summary"]
                .as_str()
                .unwrap()
                .chars()
                .count(),
            300
        );
        assert!(value["agents"][1]["handoff"].get("evidence").is_none());
        assert!(value.to_string().len() < 2500);
    }
    #[test]
    fn custom_steps_are_not_collapsed_by_role_in_the_native_snapshot() {
        let (_fixture, hub) = super::super::tests::hub();
        for role in [Role::Custom, Role::Designer] {
            let first = super::super::tests::job(&hub, role, ".");
            let second = super::super::tests::job(&hub, role, ".");
            let mut state = hub.manifest.lock().unwrap();
            state.flow = Flow::Custom;
            state.jobs.clear();
            state.jobs.insert(first.id.clone(), first);
            state.jobs.insert(second.id.clone(), second);
            let result = snapshot(&state, None).unwrap();
            assert_eq!(result.agents.len(), 3, "{role:?}");
        }
    }

    #[test]
    fn custom_snapshot_uses_frozen_flow_and_agent_identity() {
        let (_fixture, hub) = super::super::tests::hub();
        let mut catalog = catalog::tests::example();
        let appearance =
            serde_json::from_value(serde_json::json!({ "icon": "brain", "color": "cyan" }))
                .unwrap();
        catalog.agents[0].appearance = Some(appearance);
        catalog.flows[0].appearance = Some(appearance);
        let mut job = super::super::tests::job(&hub, Role::Custom, ".");
        job.custom_agent = Some(catalog.agents[0].clone());
        let mut state = hub.manifest.lock().unwrap();
        state.flow = Flow::Custom;
        state.custom_definition = Some(catalog.resolve(&catalog.flows[0].id).unwrap());
        state.jobs.insert(job.id.clone(), job);
        let value = serde_json::to_value(snapshot(&state, None).unwrap()).unwrap();
        assert_eq!(
            value["agents"][0]["identity"]["name"],
            catalog.flows[0].name
        );
        assert_eq!(
            value["agents"][1]["identity"]["name"],
            catalog.agents[0].name
        );
        assert_eq!(
            value["agents"][1]["identity"]["appearance"]["icon"],
            "brain"
        );
        assert_eq!(
            value["agents"][1]["identity"]["appearance"]["color"],
            "cyan"
        );
        assert!(value["agents"][1]["identity"].get("instructions").is_none());
    }

    #[test]
    fn direct_custom_agent_is_the_root_identity_without_a_duplicate_worker() {
        let (_fixture, hub) = super::super::tests::hub();
        let mut agent = catalog::tests::example().agents.remove(0);
        let appearance =
            serde_json::from_value(serde_json::json!({ "icon": "search", "color": "yellow" }))
                .unwrap();
        agent.name = "Support analyst".into();
        agent.appearance = Some(appearance);
        let mut state = hub.manifest.lock().unwrap();
        state.flow = Flow::Custom;
        state.options.workflow = Some(Flow::Custom);
        state.options.custom_workflow_id = None;
        state.options.custom_agent_id = Some(agent.id.clone());
        state.custom_definition = None;
        state.custom_agent = Some(agent.clone());
        let value = serde_json::to_value(snapshot(&state, None).unwrap()).unwrap();
        assert_eq!(value["agents"].as_array().unwrap().len(), 1);
        assert_eq!(value["agents"][0]["title"], agent.name);
        assert_eq!(value["agents"][0]["identity"]["name"], agent.name);
        assert_eq!(
            value["agents"][0]["identity"]["appearance"]["icon"],
            "search"
        );
        assert_eq!(value["agents"][0]["options"]["customAgentId"], agent.id);
        assert!(value["agents"][0]["identity"].get("instructions").is_none());
    }
}
