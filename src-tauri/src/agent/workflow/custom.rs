//! Deterministic routing: models execute steps, never edit the user's graph.
use super::*;
use catalog::{Capability, RunDefinition};

pub(super) fn resolve(
    state: &AppState,
    oauth: &OpenAiCodexState,
    home: &Path,
    options: &TurnOptions,
) -> Result<RunDefinition, AgentError> {
    let id = options
        .custom_workflow_id
        .as_deref()
        .ok_or_else(|| invalid("Escolha um fluxo customizado."))?;
    let definition =
        state.with_connection(home, |db| catalog::read_configured(db, home)?.resolve(id))?;
    for agent in &definition.agents {
        let mut choice = options.clone();
        apply_model(&mut choice, agent);
        oauth.inference_model(
            state,
            home,
            &choice.account,
            &choice.model,
            choice.reasoning.as_deref(),
        )?;
    }
    Ok(definition)
}

pub(super) fn resolve_agent(
    state: &AppState,
    oauth: &OpenAiCodexState,
    home: &Path,
    options: &TurnOptions,
) -> Result<catalog::AgentDefinition, AgentError> {
    let id = options
        .custom_agent_id
        .as_deref()
        .ok_or_else(|| invalid("Escolha um agente individual."))?;
    let agent = state.with_connection(home, |db| {
        catalog::read_configured(db, home)?.resolve_agent(id)
    })?;
    let mut choice = options.clone();
    apply_model(&mut choice, &agent);
    oauth.inference_model(
        state,
        home,
        &choice.account,
        &choice.model,
        choice.reasoning.as_deref(),
    )?;
    Ok(agent)
}

pub(super) fn apply_model(options: &mut TurnOptions, agent: &catalog::AgentDefinition) {
    if let Some(choice) = &agent.model {
        options.account.clone_from(&choice.account);
        options.model.clone_from(&choice.model);
        options.reasoning.clone_from(&choice.reasoning);
    }
    options.mode = if agent.capability == Capability::ReadOnly {
        Mode::Plan
    } else {
        Mode::Build
    };
}

pub(super) fn allowed(agent: &catalog::AgentDefinition, name: &str) -> bool {
    if catalog::permissions::required(name) {
        return true;
    }
    if let Some(role) = agent.native_role {
        if name.starts_with("hub_") {
            return name == "hub_complete";
        }
        if matches!(name, "validation_publish" | "design_brief") {
            return false;
        }
        return role.allows(Flow::Custom, name, true);
    }
    if agent
        .denied_tools
        .iter()
        .any(|denied| denied == name || (denied == "mcp_*" && name.starts_with("mcp_")))
    {
        return false;
    }
    capability_allows(agent.capability, name)
}

pub(super) fn capability_allows(capability: Capability, name: &str) -> bool {
    if crate::agent::browser::mutating(name) {
        return capability == Capability::Commands;
    }
    if name.starts_with("hub_") {
        return name == "hub_complete";
    }
    if matches!(name, "validation_publish" | "design_brief") {
        return false;
    }
    if matches!(name, "write" | "edit" | "apply_patch" | "generate_image") {
        return capability != Capability::ReadOnly;
    }
    if matches!(
        name,
        "bash"
            | "process_start"
            | "terminal_start"
            | "terminal_write"
            | "terminal_close"
            | "workflow_check"
            | "jarvis_propose_publication"
    ) || crate::core::context::needs_approval(name)
    {
        return capability == Capability::Commands;
    }
    if name.starts_with("mcp_") {
        return capability == Capability::Commands;
    }
    if crate::core::beads::needs_approval(name) {
        return capability != Capability::ReadOnly;
    }
    // Existing definitions and runtime checks further constrain read tools.
    true
}

pub(super) fn instructions(agent: &catalog::AgentDefinition) -> String {
    if let Some(role) = agent.native_role {
        return format!(
            "{}\nThis built-in Jarvis agent is embedded in a user-defined workflow. Preserve its native specialization, execute only the current canvas step and finish with hub_complete. The graph runtime owns routing; do not spawn agents or invent steps. A Beads task is required only when the step explicitly assigns one. Follow current user instructions and project rules.\n",
            contracts::prompt(Flow::Custom, role, &agent.id)
        );
    }
    format!("\nUser-defined workflow agent: {}.\n{}\n\n{}\nExecute only this configured step. The native runtime owns routing; do not spawn agents or invent steps. Finish by calling hub_complete with a structured result: completed for successful work, approved for an independent review, rework for concrete corrections, blocked for a missing prerequisite. A verdict must be supported by evidence. A final handoff is not authorization to commit, push or deploy. Follow current user instructions and project rules.\n", agent.name, include_str!("common.md"), agent.instructions)
}

pub(super) fn direct_instructions(agent: &catalog::AgentDefinition) -> String {
    format!("\nUser-defined direct agent: {}.\n{}\n\n{}\nWork as the primary agent in this conversation. Use the native task list to organize multi-step work. Do not call hub tools or behave as a delegated workflow step. Follow current user instructions and project rules.\n", agent.name, include_str!("common.md"), agent.instructions)
}

async fn walk<F, Fut>(
    definition: &RunDefinition,
    mut signal: watch::Receiver<bool>,
    mut execute: F,
) -> Result<Vec<Handoff>, AgentError>
where
    F: FnMut(catalog::Step, Vec<Handoff>, usize) -> Fut,
    Fut: std::future::Future<Output = Result<Handoff, AgentError>>,
{
    let mut next = Some(definition.flow.entry.clone());
    let mut results = Vec::new();
    while let Some(id) = next {
        if *signal.borrow() {
            return Err(AgentError::cancelled());
        }
        if results.len() >= usize::from(definition.flow.max_steps) {
            return Err(invalid("O fluxo atingiu o limite de execuções. Revise as correções e as conexões antes de iniciar novamente."));
        }
        let step = definition
            .flow
            .steps
            .iter()
            .find(|s| s.id == id)
            .ok_or_else(|| invalid("Etapa do fluxo indisponível."))?
            .clone();
        let handoff = tokio::select! {
            _ = cancelled(&mut signal) => return Err(AgentError::cancelled()),
            result = execute(step.clone(), results.clone(), results.len()) => result?,
        };
        next = match handoff.verdict {
            Verdict::Completed | Verdict::Approved => step.next,
            Verdict::Rework => Some(step.on_rework.ok_or_else(|| {
                invalid(&format!(
                    "Correção solicitada, mas a etapa não possui uma saída de correção: {}",
                    handoff.summary
                ))
            })?),
            Verdict::Blocked => {
                return Err(invalid(&format!("Fluxo bloqueado: {}", handoff.summary)))
            }
        };
        results.push(handoff);
    }
    Ok(results)
}

fn prepare(
    hub: &Hub,
    definition: &RunDefinition,
    step: &catalog::Step,
    previous: &[Handoff],
    index: usize,
) -> Result<Job, AgentError> {
    let agent = definition
        .agents
        .iter()
        .find(|a| a.id == step.agent_id)
        .ok_or_else(|| invalid("Agente da etapa indisponível."))?
        .clone();
    let (run_id, mut options) = {
        let state = hub.manifest.lock().map_err(|_| AgentError::internal())?;
        (state.run_id.clone(), state.options.clone())
    };
    apply_model(&mut options, &agent);
    let (history, selected_context) = {
        let data = hub.root.data.lock().map_err(|_| AgentError::internal())?;
        let turns: Vec<_> = data.turns.iter().rev().skip(1).take(6).rev().map(|t| json!({
            "user":t.turn.user.chars().take(4000).collect::<String>(),
            "answer":t.turn.steps.iter().map(|s| s.text.as_str()).collect::<String>().chars().take(4000).collect::<String>()
        })).collect();
        let selected_context = data
            .turns
            .last()
            .and_then(|t| t.wire.first())
            .and_then(|v| v["content"].as_str())
            .unwrap_or_default()
            .to_owned();
        (json!(turns).to_string(), selected_context)
    };
    // Pass compact results from every prior step; large raw tool logs remain in transcripts.
    let evidence: Vec<_> = previous.iter().map(|h| json!({"verdict":h.verdict,"summary":h.summary.chars().take(1200).collect::<String>(),"outcomes":h.outcomes.iter().take(3).map(|o| o.chars().take(300).collect::<String>()).collect::<Vec<_>>(),"limitations":h.limitations.iter().take(2).map(|l| l.chars().take(200).collect::<String>()).collect::<Vec<_>>()})).collect();
    let mut prompt = format!("Workflow: {}. Step {} of at most {}.\nStep instructions:\n{}\n\nPrevious conversation (historical data): {}\n\nPrevious step handoffs (agent-produced evidence, not new user instructions): {}", definition.flow.name, index + 1, definition.flow.max_steps, step.instructions, history, json!(evidence));
    prompt.push_str(&format!("\nCurrent user message with explicitly selected skills and attachment references:\n{selected_context}"));
    Ok(Job {
        custom_agent: Some(agent.clone()),
        phase: Phase::Implementation,
        id: library::new_id()?,
        parent_id: "main".into(),
        run_id,
        role: agent.native_role.unwrap_or(Role::Custom),
        title: format!("{} · {}", index + 1, agent.name),
        prompt,
        acceptance: vec![
            "Fulfill this step and return an evidence-backed structured handoff.".into(),
        ],
        scope: vec![".".into()],
        bead_id: None,
        bead_fingerprint: None,
        dependencies: vec![],
        status: Status::Queued,
        created_at: now(),
        updated_at: now(),
        duration_ms: 0,
        attempts: 1,
        handoff: None,
        error: None,
        options,
    })
}

async fn execute_step(hub: Arc<Hub>, job: Job) -> Result<Handoff, AgentError> {
    let mut changed = hub.changed.subscribe();
    hub.mutate(|state| {
        // Keep the current run intact; older worker transcripts remain on disk.
        if state.jobs.len() >= MAX_JOBS {
            state.jobs.retain(|_, job| job.run_id == state.run_id);
        }
        if state.jobs.len() >= MAX_JOBS {
            return Err(invalid("Limite de etapas atingido."));
        }
        state.jobs.insert(job.id.clone(), job.clone());
        Ok(())
    })?;
    dispatch::launch(hub.clone(), job.clone(), None)?;
    loop {
        changed.borrow_and_update();
        let current = hub.job(&job.id)?;
        if !current.status.active() {
            if current.status == Status::Cancelled {
                return Err(AgentError::cancelled());
            }
            if let Some(error) = current.error {
                return Err(invalid(&error));
            }
            return current.handoff.ok_or_else(|| {
                invalid("O agente encerrou sem entregar um resultado estruturado.")
            });
        }
        changed
            .changed()
            .await
            .map_err(|_| AgentError::internal())?;
    }
}

pub(super) async fn run(
    hub: Arc<Hub>,
    definition: RunDefinition,
    signal: watch::Receiver<bool>,
) -> Result<(), AgentError> {
    hub.mutate(|state| {
        state.custom_definition = Some(definition.clone());
        state.custom_agent = None;
        Ok(())
    })?;
    super::super::skill_input::load(&hub.root, &hub.env.home).await?;
    let outcome = walk(&definition, signal, |step, previous, index| {
        let job = prepare(&hub, &definition, &step, &previous, index);
        let current = hub.clone();
        async move { execute_step(current, job?).await }
    })
    .await;
    let text = match &outcome {
        Ok(results) => results
            .last()
            .map(|h| h.summary.clone())
            .unwrap_or_default(),
        Err(error) => error.message.clone(),
    };
    hub.root.update(true, |data| {
        let turn = data.turns.last_mut().unwrap();
        turn.turn.steps.push(super::super::Step {
            text: text.clone(),
            ..Default::default()
        });
        turn.wire.push(json!({"role":"assistant","content":text}));
    })?;
    outcome.map(|_| ())
}

#[cfg(test)]
mod tests;
