use super::*;

fn prepare(hub: &Hub) -> Result<Job, AgentError> {
    let (run_id, options) = {
        let state = hub.manifest.lock().map_err(|_| AgentError::internal())?;
        (state.run_id.clone(), state.options.clone())
    };
    let prompt = hub
        .root
        .data
        .lock()
        .map_err(|_| AgentError::internal())?
        .turns
        .last()
        .ok_or_else(AgentError::internal)?
        .turn
        .user
        .clone();
    Ok(Job {
        custom_agent: None,
        phase: Phase::Implementation,
        id: library::new_id()?,
        parent_id: "main".into(),
        run_id,
        role: Role::Github,
        title: "Publicar alterações".into(),
        prompt,
        acceptance: vec![
            "Inspect every changed Git repository inside the project root.".into(),
            "Run the relevant checks and present one supervised publication proposal.".into(),
            "Apply only the actions approved by the user and verify the resulting state.".into(),
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
        recovery: None,
        options,
    })
}

async fn execute(
    hub: Arc<Hub>,
    job: Job,
    mut signal: watch::Receiver<bool>,
) -> Result<Handoff, AgentError> {
    let mut changed = hub.changed.subscribe();
    hub.mutate(|state| {
        if state.jobs.len() >= MAX_JOBS {
            state
                .jobs
                .retain(|_, existing| existing.run_id == state.run_id);
        }
        if state.jobs.len() >= MAX_JOBS {
            return Err(invalid("Limite de agentes desta conversa atingido."));
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
            if current.status != Status::Completed {
                let message = current
                    .handoff
                    .as_ref()
                    .map(|handoff| handoff.summary.as_str())
                    .unwrap_or("O agente GitHub não concluiu a publicação.");
                return Err(invalid(message));
            }
            return current.handoff.ok_or_else(|| {
                invalid("O agente GitHub encerrou sem entregar um resultado estruturado.")
            });
        }
        tokio::select! {
            _ = cancelled(&mut signal) => return Err(AgentError::cancelled()),
            result = changed.changed() => result.map_err(|_| AgentError::internal())?,
        }
    }
}

pub(super) async fn run(hub: Arc<Hub>, signal: watch::Receiver<bool>) -> Result<(), AgentError> {
    super::super::skill_input::load(&hub.root, &hub.env.home).await?;
    let outcome = execute(hub.clone(), prepare(&hub)?, signal).await;
    let text = outcome
        .as_ref()
        .map(|handoff| handoff.summary.clone())
        .unwrap_or_else(|error| error.message.clone());
    if hub
        .root
        .data
        .lock()
        .map_err(|_| AgentError::internal())?
        .turns
        .is_empty()
    {
        return Err(AgentError::internal());
    }
    hub.root.update(true, |data| {
        let turn = data
            .turns
            .last_mut()
            .expect("publication flow requires an active root turn");
        turn.turn.steps.push(super::super::Step {
            text: text.clone(),
            ..Default::default()
        });
        turn.wire.push(json!({"role":"assistant","content":text}));
    })?;
    outcome.map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn publication_worker_is_isolated_and_uses_the_dedicated_profile() {
        let (_fixture, hub) = super::super::tests::hub();
        {
            let mut state = hub.manifest.lock().unwrap();
            state.flow = Flow::Publication;
            state.options.workflow = Some(Flow::Publication);
            state.options.account = "github-account".into();
            state.options.model = "economical-model".into();
            state.options.reasoning = Some("medium".into());
        }

        let job = prepare(&hub).unwrap();

        assert_eq!(job.role, Role::Github);
        assert_eq!(job.parent_id, "main");
        assert_eq!(job.scope, vec!["."]);
        assert!(job.bead_id.is_none());
        assert_eq!(job.options.account, "github-account");
        assert_eq!(job.options.model, "economical-model");
        assert_eq!(job.options.workflow, Some(Flow::Publication));
    }
}
