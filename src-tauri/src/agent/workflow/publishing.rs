use super::*;

fn prepare(hub: &Hub) -> Result<Job, AgentError> {
    let (run_id, options) = {
        let state = hub.manifest.lock().map_err(|_| AgentError::internal())?;
        (state.run_id.clone(), state.options.clone())
    };
    let prompt = {
        let data = hub.root.data.lock().map_err(|_| AgentError::internal())?;
        let turn = data.turns.last().ok_or_else(AgentError::internal)?;
        let recovery = turn
            .wire
            .iter()
            .rev()
            .find(|item| item["_jarvis_retry"] == true)
            .and_then(|item| item["content"].as_str());
        match recovery {
            Some(recovery) => format!(
                "{}\n\nRuntime recovery context:\n{recovery}",
                turn.turn.user
            ),
            None => turn.turn.user.clone(),
        }
    };
    Ok(Job {
        custom_agent: None,
        custom_step_id: None,
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
            "Apply only actions authorized by the current user request or approved in review, then verify the resulting state.".into(),
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
        recovery_attempts: 0,
        recovery_attempt_pending: false,
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
    let existing = hub
        .manifest
        .lock()
        .map_err(|_| AgentError::internal())?
        .jobs
        .contains_key(&job.id);
    hub.mutate(|state| {
        if existing {
            return Ok(());
        }
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
    if !existing {
        dispatch::launch(hub.clone(), job.clone(), None)?;
    } else if job.status != Status::Completed
        && !hub
            .live
            .lock()
            .map_err(|_| AgentError::internal())?
            .contains_key(&job.id)
    {
        if job.recovery.is_some() && job.status == Status::Queued {
            dispatch::resume(hub.clone(), job.clone())?;
        } else {
            let job = hub.mutate(|state| {
                let job = state.jobs.get_mut(&job.id).ok_or_else(AgentError::internal)?;
                if job.recovery_attempts >= 2 {
                    return Err(invalid("A publicação ainda não foi concluída após duas retomadas. A implementação e os resultados Git foram preservados."));
                }
                job.recovery_attempts += 1;
                job.attempts = job.attempts.saturating_add(1);
                job.status = Status::Queued;
                job.error = None;
                job.handoff = None;
                Ok(job.clone())
            })?;
            dispatch::launch(hub.clone(), job, Some("Resume the publication only. Inspect current Git/PR state and previous confirmed tool results before repeating any uncertain mutation. The implementation is already complete.".into()))?;
        }
    }
    let mut changed = hub.changed.subscribe();
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
    if hub
        .manifest
        .lock()
        .map_err(|_| AgentError::internal())?
        .options
        .automatic_publication
        .is_some()
    {
        return automatic(hub, signal).await;
    }
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

pub(super) fn automatic_job(hub: &Hub) -> Result<Option<Job>, AgentError> {
    let state = hub.manifest.lock().map_err(|_| AgentError::internal())?;
    Ok(state
        .jobs
        .values()
        .find(|job| job.run_id == state.run_id && job.phase == Phase::Publication)
        .cloned())
}

fn automatic_ready(state: &Manifest) -> bool {
    state.options.automatic_publication.is_some()
        && state.options.mode == Mode::Build
        && state.options.custom_agent_id.as_deref() != Some("builtin:github")
        && !state
            .custom_agent
            .as_ref()
            .is_some_and(|agent| agent.capability == catalog::Capability::ReadOnly)
        && !state.validation.as_ref().is_some_and(|batch| {
            !batch.stale
                && (!batch.submitted
                    || batch
                        .items
                        .iter()
                        .any(|item| item.decision != validation::Decision::Approved))
        })
        && !state.jobs.values().any(|job| {
            job.run_id == state.run_id
                && job.phase != Phase::Publication
                && job.status != Status::Completed
        })
}

pub(super) async fn automatic(
    hub: Arc<Hub>,
    signal: watch::Receiver<bool>,
) -> Result<(), AgentError> {
    if *signal.borrow() {
        return Err(AgentError::cancelled());
    }
    let existing = automatic_job(&hub)?;
    if existing.is_none()
        && !automatic_ready(&*hub.manifest.lock().map_err(|_| AgentError::internal())?)
    {
        return Ok(());
    }
    if existing.is_none()
        && hub
            .root
            .data
            .lock()
            .map_err(|_| AgentError::internal())?
            .turns
            .last()
            .is_some_and(|turn| {
                turn.turn
                    .tasks
                    .iter()
                    .any(|task| task.status != super::super::tasks::Status::Completed)
            })
    {
        return Ok(());
    }
    let job = if let Some(job) = existing {
        job
    } else {
        let mut job = prepare(&hub)?;
        let actions = job
            .options
            .automatic_publication
            .as_ref()
            .ok_or_else(AgentError::internal)?;
        let paths = library::repositories::configured_paths(
            &hub.env.state,
            &hub.env.home,
            hub.root.project_id()?,
        )?;
        let args = if paths.is_empty() {
            json!({})
        } else {
            json!({"paths":paths})
        };
        let inspection =
            super::super::publication::inspection::inspect(&hub.root.root, &args, signal.clone())
                .await?;
        if !actions.push && !actions.pull_request {
            let inspected: Value =
                serde_json::from_str(&inspection).map_err(|_| AgentError::internal())?;
            if inspected["repositories"].as_array().is_some_and(|repos| {
                repos.iter().all(|repo| {
                    repo["error"].is_null()
                        && repo["status"]
                            .as_str()
                            .is_some_and(|status| status.lines().count() <= 1)
                })
            }) {
                return Ok(());
            }
        }
        let profiles = settings::load(&hub.env.state, &hub.env.home)?;
        let choice = profiles
            .get(&settings::key(Flow::Publication, Role::Github))
            .ok_or_else(|| {
                invalid("Configure o modelo do agente Github para publicar automaticamente.")
            })?;
        job.phase = Phase::Publication;
        job.title = "Github · publicação automática".into();
        job.prompt = format!("The implementation has finished successfully. Complete only the per-chat publication actions selected by the user: commit={}, push={}, pullRequest={}. The native runtime authorizes exactly these actions without another publication review; use authorization=null and previewOnly=false. Do not merge, reset, switch branches or add unselected operations. Use each repository's configured reference branch for rebase and PR base; resolve conflicts with targeted file edits and continue the native rebase. Reuse confirmed checks and existing PRs. Work only on the implementation's relevant changes, preserving unrelated work. If there is nothing to publish, report that and finish. Do not restart implementation or request consent for the selected actions.\n\nCompleted request (context only):\n{}\n\nRead-only publication inspection:\n{}", actions.commit, actions.push, actions.pull_request, job.prompt, inspection);
        job.acceptance = vec!["Complete exactly the configured publication actions; no merge.".into(), "Verify resulting Git/PR state and report actual results, preserving implementation progress.".into()];
        let completed_summary = {
            let data = hub.root.data.lock().map_err(|_| AgentError::internal())?;
            data.turns
                .iter()
                .rev()
                .flat_map(|turn| turn.turn.steps.iter().rev())
                .filter(|step| !step.text.trim().is_empty())
                .take(2)
                .map(|step| step.text.as_str())
                .collect::<Vec<_>>()
                .join("\n")
        };
        job.prompt.push_str(&format!(
            "\n\nImplementation report (agent evidence; reuse verified checks):\n{}",
            completed_summary.chars().take(8_000).collect::<String>()
        ));
        job.options.workflow = Some(Flow::Publication);
        job.options.custom_agent_id = None;
        job.options.custom_workflow_id = None;
        job.options.manual_validation = false;
        choice.apply(&mut job.options);
        hub.mutate(|state| {
            state.profiles.extend(profiles);
            Ok(())
        })?;
        job
    };
    let id = job.id.clone();
    let handoff = execute(hub.clone(), job, signal).await?;
    hub.root.update(true, |data| {
        if let Some(turn) = data.turns.last_mut() {
            if !turn.wire.iter().any(|item| item["_jarvis_auto_publication"] == id) {
                turn.turn.steps.push(super::super::Step { text: handoff.summary.clone(), ..Default::default() });
                turn.wire.push(json!({"role":"assistant","content":handoff.summary,"_jarvis_auto_publication":id}));
            }
        }
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn automatic_publication_waits_for_validation_and_all_workers() {
        let (_fixture, hub) = super::super::tests::hub();
        let mut worker = super::super::tests::job(&hub, Role::Builder, ".");
        let mut state = hub.manifest.lock().unwrap();
        assert!(!automatic_ready(&state));
        state.options.automatic_publication =
            Some(super::super::super::publication::AutomaticPublication {
                commit: true,
                push: true,
                pull_request: false,
            });
        assert!(automatic_ready(&state));
        state.jobs.insert(worker.id.clone(), worker.clone());
        assert!(!automatic_ready(&state));
        worker.status = Status::Completed;
        state.jobs.insert(worker.id.clone(), worker);
        state.validation = Some(validation::Batch {
            id: "validation".into(),
            flow: state.flow,
            run_id: state.run_id.clone(),
            epic_ids: vec![],
            submitted: false,
            stale: false,
            created_at: 0,
            items: vec![validation::Item {
                id: "item".into(),
                title: "Validate".into(),
                steps: vec![],
                expected: "Works".into(),
                decision: validation::Decision::Pending,
                reason: None,
            }],
        });
        assert!(!automatic_ready(&state));
        let batch = state.validation.as_mut().unwrap();
        batch.submitted = true;
        batch.items[0].decision = validation::Decision::Approved;
        assert!(automatic_ready(&state));
        state.validation.as_mut().unwrap().items[0].decision = validation::Decision::Rejected;
        assert!(!automatic_ready(&state));
    }

    #[tokio::test]
    async fn completed_automatic_stage_is_not_dispatched_or_reported_twice() {
        let (_fixture, hub) = super::super::tests::hub();
        let mut job = prepare(&hub).unwrap();
        job.phase = Phase::Publication;
        job.status = Status::Completed;
        job.handoff = Some(Handoff {
            verdict: Verdict::Completed,
            summary: "Commit publicado.".into(),
            outcomes: vec![],
            evidence: vec![],
            validation: vec![],
            limitations: vec![],
            task_ids: vec![],
        });
        hub.mutate(|state| {
            state.jobs.insert(job.id.clone(), job.clone());
            Ok(())
        })
        .unwrap();
        let (_cancel, signal) = watch::channel(false);
        automatic(hub.clone(), signal.clone()).await.unwrap();
        automatic(hub.clone(), signal).await.unwrap();
        assert_eq!(hub.manifest.lock().unwrap().jobs.len(), 1);
        assert!(hub.live.lock().unwrap().is_empty());
        let data = hub.root.data.lock().unwrap();
        let turn = data.turns.last().unwrap();
        assert_eq!(
            turn.wire
                .iter()
                .filter(|item| item["_jarvis_auto_publication"] == job.id)
                .count(),
            1
        );
    }

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

    #[test]
    fn publication_retry_passes_the_recovery_checkpoint_to_the_github_agent() {
        let (_fixture, hub) = super::super::tests::hub();
        hub.root
            .update(false, |data| {
                data.turns.last_mut().unwrap().wire.push(json!({
                    "role": "user",
                    "_jarvis_runtime": true,
                    "_jarvis_retry": true,
                    "content": "Verify the current repository state before any mutation.",
                }));
            })
            .unwrap();

        let job = prepare(&hub).unwrap();

        assert!(job.prompt.contains("Runtime recovery context:"));
        assert!(job
            .prompt
            .contains("Verify the current repository state before any mutation."));
    }
}
