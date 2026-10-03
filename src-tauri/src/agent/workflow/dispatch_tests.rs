use super::super::tests::{hub, job};
use super::*;

#[test]
fn image_requests_create_durable_jobs_without_beads_and_reuse_only_identical_active_requests() {
    let (_fixture, hub) = hub();
    let execution = Execution {
        hub: hub.clone(),
        id: "main".into(),
        role: Role::Builder,
        flow: Flow::Standard,
        scope: vec![".".into()],
    };
    let request =
        json!({"prompt":"Create a friendly robot","variants":2,"processing":{"format":"png"}});
    let (first, created) = image_job(&execution, &request).unwrap();
    assert!(created);
    assert_eq!(first.role, Role::ImageGenerator);
    assert_eq!(first.options.workflow, Some(Flow::ImageGenerator));
    assert_eq!(first.parent_id, "main");
    assert!(first.bead_id.is_none());
    assert!(first.dependencies.is_empty());
    assert_eq!(first.options.account, "root-account");
    assert_eq!(first.options.model, "root-model");
    let persisted = storage::load(&hub.directory, &hub.root.id)
        .unwrap()
        .unwrap();
    assert_eq!(persisted.jobs[&first.id].prompt, first.prompt);
    let (duplicate, created) = image_job(&execution, &request).unwrap();
    assert!(!created);
    assert_eq!(duplicate.id, first.id);
    let (different, created) =
        image_job(&execution, &json!({"prompt":"Create a landscape"})).unwrap();
    assert!(created);
    assert_ne!(different.id, first.id);
    let specialist = Execution {
        hub: hub.clone(),
        id: first.id,
        role: Role::ImageGenerator,
        flow: Flow::ImageGenerator,
        scope: first.scope,
    };
    assert!(image_job(&specialist, &request).is_err());
}

#[test]
fn image_job_conversational_primary_and_fallback_do_not_replace_the_independent_image_account() {
    let (_fixture, hub) = hub();
    let specialist: settings::ModelChoice = serde_json::from_value(json!({
        "executor":"claude","account":"","model":"sonnet","reasoning":"high",
        "fallback":{"executor":"claude","account":"","model":"opus","reasoning":"max"}
    }))
    .unwrap();
    let profiles = BTreeMap::from([(
        settings::key(Flow::ImageGenerator, Role::ImageGenerator),
        specialist.clone(),
    )]);
    let directory = crate::data_dir::root(&hub.env.home);
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(
        directory.join("agents.json"),
        serde_json::to_vec(&profiles).unwrap(),
    )
    .unwrap();
    hub.env.state.with_connection(&hub.env.home,|db| {
        db.execute("INSERT INTO image_generation_config(id,account_alias) VALUES(1,'independent-image-account')",[]).map_err(|_|AgentError::storage())?;
        Ok::<_,AgentError>(())
    }).unwrap();
    let execution = Execution {
        hub: hub.clone(),
        id: "main".into(),
        role: Role::Planner,
        flow: Flow::Complete,
        scope: vec![".".into()],
    };
    let (child, _) = image_job(&execution, &json!({"prompt":"Create a robot"})).unwrap();
    assert_eq!(child.options.executor, crate::claude::Executor::Claude);
    assert_eq!(child.options.model, "sonnet");
    assert!(child.options.account.is_empty());
    assert_eq!(
        hub.manifest.lock().unwrap().profiles["image_generator/image_generator"],
        specialist
    );
    hub.env
        .state
        .with_connection(&hub.env.home, |db| {
            let alias: String = db
                .query_row(
                    "SELECT account_alias FROM image_generation_config WHERE id=1",
                    [],
                    |row| row.get(0),
                )
                .map_err(|_| AgentError::storage())?;
            assert_eq!(alias, "independent-image-account");
            Ok::<_, AgentError>(())
        })
        .unwrap();
}

#[test]
fn image_admission_releases_waiting_parent_slots_without_bypassing_independent_writer_capacity() {
    let (_fixture, hub) = hub();
    let mut state = hub.manifest.lock().unwrap().clone();
    let mut target = job(&hub, Role::ImageGenerator, ".jarvis-image-attachments");
    for index in 0..MAX_ACTIVE {
        let mut parent = job(&hub, Role::Builder, &format!("src/area-{index}"));
        parent.status = Status::Waiting;
        let mut child = job(&hub, Role::ImageGenerator, ".jarvis-image-attachments");
        child.parent_id = parent.id.clone();
        if index == 0 {
            target = child.clone();
        }
        state.jobs.insert(parent.id.clone(), parent);
        state.jobs.insert(child.id.clone(), child);
    }
    assert!(admitted(&state, &target).unwrap());
    for index in 0..MAX_ACTIVE {
        let mut worker = job(&hub, Role::Builder, &format!("other-{index}"));
        worker.status = Status::Running;
        state.jobs.insert(worker.id.clone(), worker);
    }
    assert!(!admitted(&state, &target).unwrap());
}

#[test]
fn attachment_only_images_run_beside_sibling_writers_but_real_image_exports_remain_serialized() {
    let (_fixture, hub) = hub();
    let mut state = hub.manifest.lock().unwrap().clone();
    let mut builder = job(&hub, Role::Builder, ".");
    builder.status = Status::Running;
    let image = job(&hub, Role::ImageGenerator, ".jarvis-image-attachments");
    state.jobs.insert(builder.id.clone(), builder);
    assert!(admitted(&state, &image).unwrap());
    let mut sibling = job(&hub, Role::ImageGenerator, ".jarvis-image-attachments");
    sibling.status = Status::Running;
    state.jobs.insert(sibling.id.clone(), sibling);
    assert!(admitted(&state, &image).unwrap());
    let export = job(&hub, Role::ImageGenerator, "assets/images");
    assert!(!admitted(&state, &export).unwrap());
    let execution = Execution {
        hub: hub.clone(),
        id: "main".into(),
        role: Role::Builder,
        flow: Flow::Standard,
        scope: vec![".".into()],
    };
    let (explicit_reserved_export,_) = image_job(&execution,&json!({"prompt":"Create a robot","processing":{"output_directory":".jarvis-image-attachments"}})).unwrap();
    assert!(!attachment_only_image(&explicit_reserved_export));
    assert!(!admitted(&state, &explicit_reserved_export).unwrap());
    state.jobs.clear();
    let mut same_export = job(&hub, Role::ImageGenerator, "assets/images");
    same_export.status = Status::Running;
    state.jobs.insert(same_export.id.clone(), same_export);
    assert!(!admitted(&state, &export).unwrap());
}

#[test]
fn image_exports_cannot_escape_requesting_worker_scope_or_global_conversation() {
    let (_fixture, mut hub) = hub();
    let execution = Execution {
        hub: hub.clone(),
        id: "main".into(),
        role: Role::Builder,
        flow: Flow::Standard,
        scope: vec!["src/assets".into()],
    };
    assert!(image_job(
        &execution,
        &json!({"prompt":"Create a robot","processing":{"output_directory":"other"}})
    )
    .is_err());
    assert!(image_job(
        &execution,
        &json!({"prompt":"Create a robot","processing":{"output_directory":"src/assets"}})
    )
    .is_ok());
    drop(execution);
    let hub_mut = Arc::get_mut(&mut hub).unwrap();
    Arc::get_mut(&mut hub_mut.root).unwrap().id =
        crate::agent::companion_chat::GLOBAL_CONVERSATION_ID.into();
    let execution = Execution {
        hub: hub.clone(),
        id: "main".into(),
        role: Role::Builder,
        flow: Flow::Standard,
        scope: vec![".".into()],
    };
    assert!(image_job(
        &execution,
        &json!({"prompt":"Create a robot","processing":{"output_directory":"images"}})
    )
    .is_err());
    assert!(image_job(&execution, &json!({"prompt":"Create a robot"})).is_ok());
}

#[tokio::test]
async fn cancellation_of_image_wait_stops_its_native_worker_session() {
    let (_fixture, hub) = hub();
    let execution = Execution {
        hub: hub.clone(),
        id: "main".into(),
        role: Role::Builder,
        flow: Flow::Standard,
        scope: vec![".".into()],
    };
    let (child, _) = image_job(&execution, &json!({"prompt":"Create a robot"})).unwrap();
    let (session, signal) = storage::worker(&hub, &child, None).unwrap();
    hub.live.lock().unwrap().insert(child.id.clone(), session);
    cancel_image(&hub, &child.id).unwrap();
    assert!(*signal.borrow());
}

#[test]
fn every_native_delegate_can_request_guidance_without_broader_permissions() {
    let (_fixture, hub) = hub();
    for flow in [Flow::Planned, Flow::Complete, Flow::Publication] {
        for (_, role) in flow.delegations() {
            let exec = Execution {
                hub: hub.clone(),
                id: "delegated".into(),
                role: *role,
                flow,
                scope: vec!["src".into()],
            };
            let mut tools = vec![];
            exec.filter(&mut tools);
            assert!(tools
                .iter()
                .any(|tool| tool["name"] == "hub_request_guidance"));
            assert!(exec.allowed("hub_request_guidance"));
            assert!(!exec.allowed("bash"));
            assert!(!exec.allowed("browser_click"));
        }
    }
}

#[test]
fn github_can_delegate_conflict_repair_without_overlapping_child_writes() {
    let (_fixture, hub) = hub();
    let mut github = job(&hub, Role::Github, ".");
    github.status = Status::Running;
    github.options.workflow = Some(Flow::Publication);
    let mut builder = job(&hub, Role::Builder, ".");
    builder.parent_id = github.id.clone();
    let execution = Execution {
        hub: hub.clone(),
        id: github.id.clone(),
        role: Role::Github,
        flow: Flow::Publication,
        scope: vec![".".into()],
    };
    hub.mutate(|state| {
        state.jobs.insert(github.id.clone(), github.clone());
        state.jobs.insert(builder.id.clone(), builder.clone());
        assert!(admitted(state, &builder)?);
        Ok(())
    })
    .unwrap();
    assert!(execution.allowed("hub_spawn"));
    assert!(definitions(Flow::Publication, Role::Github)
        .iter()
        .any(|tool| tool["name"] == "hub_spawn"));
    let tool = ToolCall {
        id: "publish".into(),
        name: "jarvis_propose_publication".into(),
        args: json!({}),
        status: "running".into(),
        output: String::new(),
        duration_ms: 0,
    };
    assert!(execution.preflight(&tool).unwrap().contains("hub_wait"));
    hub.mutate(|state| {
        state.jobs.get_mut(&builder.id).unwrap().status = Status::Completed;
        Ok(())
    })
    .unwrap();
    assert!(execution.preflight(&tool).is_none());
    assert!(!Role::Builder.spawns(Flow::Publication, Role::Github));
}

#[test]
fn technical_parent_failure_preserves_children_for_durable_recovery_but_user_cancel_does_not() {
    for parent in ["main", "nested-coordinator"] {
        for technical in [false, true] {
            let (_fixture, hub) = hub();
            let mut worker = job(&hub, Role::Builder, ".");
            worker.parent_id = parent.into();
            worker.status = Status::Running;
            hub.mutate(|state| {
                state.jobs.insert(worker.id.clone(), worker.clone());
                Ok(())
            })
            .unwrap();
            let (session, _) = storage::worker(&hub, &worker, None).unwrap();
            let turn_id = session.snapshot().unwrap().turns[0].id.clone();
            session.update(true, |data| {
                data.turns.last_mut().unwrap().wire.push(json!({"type":"function_call_output","call_id":"confirmed","output":"Already applied"}));
            }).unwrap();
            if technical {
                interrupt_descendants(&hub, parent, "Provider unavailable").unwrap();
                let saved = storage::load(&hub.directory, &hub.root.id)
                    .unwrap()
                    .unwrap();
                assert_eq!(
                    saved.worker_interruptions[&worker.id],
                    "Provider unavailable"
                );
            }
            let result = interrupted_result(&hub, &worker.id, Err(AgentError::cancelled()));
            finish(&session, result.clone());
            settle(&hub, &worker, &result, Some(20)).unwrap();
            drop(session);
            hub.mutate(|state| {
                state.root_status = Status::Failed;
                Ok(())
            })
            .unwrap();
            let saved = storage::load(&hub.directory, &hub.root.id)
                .unwrap()
                .unwrap();
            let (recovered, workers) =
                storage::prepare_recovery(&hub.directory, saved, Flow::Complete, "run", vec![])
                    .unwrap();
            if technical {
                assert_eq!(workers.len(), 1);
                assert!(recovered.worker_interruptions.is_empty());
                *hub.manifest.lock().unwrap() = recovered;
                let (session, _) = storage::resume_worker(&hub, &workers[0]).unwrap();
                let data = session.data.lock().unwrap();
                assert_eq!(data.turns.len(), 1);
                assert_eq!(data.turns[0].turn.id, turn_id);
                assert_eq!(
                    data.turns[0]
                        .wire
                        .iter()
                        .filter(|item| item["call_id"] == "confirmed")
                        .count(),
                    1
                );
            } else {
                assert!(workers.is_empty());
                assert_eq!(recovered.jobs[&worker.id].status, Status::Cancelled);
            }
        }
    }
}

#[test]
fn technical_stop_does_not_replace_a_confirmed_handoff() {
    let (_fixture, hub) = hub();
    let mut worker = job(&hub, Role::Builder, ".");
    worker.status = Status::Running;
    worker.handoff = Some(completed_handoff("task"));
    hub.mutate(|state| {
        state.jobs.insert(worker.id.clone(), worker.clone());
        Ok(())
    })
    .unwrap();
    interrupt_descendants(&hub, "main", "Provider failed").unwrap();
    let result = interrupted_result(&hub, &worker.id, Ok(()));
    settle(&hub, &worker, &result, Some(30)).unwrap();
    assert_eq!(hub.job(&worker.id).unwrap().status, Status::Completed);
}

#[test]
fn explicit_stop_wins_over_a_pending_technical_interruption() {
    let (_fixture, hub) = hub();
    let mut worker = job(&hub, Role::Builder, ".");
    worker.status = Status::Running;
    hub.mutate(|state| {
        state.jobs.insert(worker.id.clone(), worker.clone());
        Ok(())
    })
    .unwrap();
    interrupt_descendants(&hub, "main", "Provider failed").unwrap();
    hub.root
        .data
        .lock()
        .unwrap()
        .active
        .as_ref()
        .unwrap()
        .cancel
        .send_replace(true);
    let error = interrupted_result(&hub, &worker.id, Err(AgentError::cancelled())).unwrap_err();
    assert_eq!(error.code, "cancelled");
}

#[tokio::test]
async fn coordinator_shutdown_persists_interrupted_worker_before_recovery() {
    let (_fixture, hub) = hub();
    let mut worker = job(&hub, Role::Builder, ".");
    worker.status = Status::Running;
    hub.mutate(|state| {
        state.jobs.insert(worker.id.clone(), worker.clone());
        Ok(())
    })
    .unwrap();
    let (session, mut signal) = storage::worker(&hub, &worker, None).unwrap();
    hub.live
        .lock()
        .unwrap()
        .insert(worker.id.clone(), session.clone());
    let task_hub = hub.clone();
    let id = worker.id.clone();
    let work = tokio::spawn(async move {
        cancelled(&mut signal).await;
        let result = interrupted_result(&task_hub, &worker.id, Err(AgentError::cancelled()));
        finish(&session, result.clone());
        task_hub.live.lock().unwrap().remove(&worker.id);
        settle(&task_hub, &worker, &result, Some(12)).unwrap();
    });
    // A provider callback can fail while its publication cleanup still needs
    // draining. That cleanup must not become an explicit user cancellation.
    hub.root.drain_interactions(true).await;
    assert!(
        !*hub.root_signal.borrow(),
        "technical interaction cleanup must preserve recoverable descendants"
    );
    tokio::time::timeout(
        Duration::from_secs(2),
        super::stop_workers(&hub, &Err(AgentError::new("provider_request", "Rejected"))),
    )
    .await
    .unwrap()
    .unwrap();
    work.await.unwrap();
    assert!(hub.live.lock().unwrap().is_empty());
    let saved = storage::load(&hub.directory, &hub.root.id)
        .unwrap()
        .unwrap();
    assert_eq!(saved.jobs[&id].status, Status::Interrupted);
    let turns = journal::read_only(&hub.directory.join(format!("{id}.jsonl")))
        .unwrap()
        .0;
    assert_eq!(turns.last().unwrap().turn.status, TurnStatus::Interrupted);
}

#[test]
fn retry_keeps_the_same_worker_turn_and_confirmed_results() {
    let (_fixture, hub) = hub();
    let mut worker = job(&hub, Role::Builder, ".");
    let (session, _) = storage::worker(&hub, &worker, None).unwrap();
    let original = session.snapshot().unwrap().turns[0].id.clone();
    session.update(true, |data| {
        let turn = data.turns.last_mut().unwrap();
        turn.wire.push(json!({"type":"function_call_output","call_id":"confirmed","output":"Already applied"}));
        turn.turn.steps.push(Step { text: "Verified existing result".into(), ..Step::default() });
    }).unwrap();
    finish(&session, Err(AgentError::internal()));
    drop(session);
    worker.recovery = Some(RecoveryCheckpoint::new(vec![]));
    worker.options.model = "updated-model".into();
    hub.manifest
        .lock()
        .unwrap()
        .jobs
        .insert(worker.id.clone(), worker.clone());
    let (resumed, _) =
        storage::worker(&hub, &worker, Some("Finish the remaining test".into())).unwrap();
    let data = resumed.data.lock().unwrap();
    assert_eq!(data.turns.len(), 1);
    let turn = &data.turns[0];
    assert_eq!(turn.turn.id, original);
    assert_eq!(turn.turn.user, worker.prompt);
    assert_eq!(turn.turn.options.model, "updated-model");
    assert_eq!(
        turn.mcp_intent,
        Some(hub.manifest.lock().unwrap().mcp_intent.clone())
    );
    assert_eq!(turn.turn.steps[0].text, "Verified existing result");
    assert_eq!(
        turn.wire
            .iter()
            .filter(|item| item["call_id"] == "confirmed")
            .count(),
        1
    );
    assert!(turn.wire.iter().any(|item| item["_jarvis_runtime"] == true
        && item["content"]
            .as_str()
            .is_some_and(|s| s.contains("Finish the remaining test"))));
}

#[test]
fn dependency_failure_does_not_count_queued_hours_as_work() {
    let (_fixture, hub) = hub();
    let worker = job(&hub, Role::Designer, ".");
    hub.manifest
        .lock()
        .unwrap()
        .jobs
        .insert(worker.id.clone(), worker.clone());
    let (session, _) = storage::worker(&hub, &worker, None).unwrap();
    session
        .data
        .lock()
        .unwrap()
        .turns
        .last_mut()
        .unwrap()
        .turn
        .created_at = now().saturating_sub(9 * 3_600_000);
    let result = Err(invalid("Uma dependência não foi concluída com sucesso."));
    finish(&session, result.clone());
    let duration = session.snapshot().unwrap().turns[0].duration_ms;
    settle(&hub, &worker, &result, Some(duration)).unwrap();
    assert_eq!(duration, 0);
    assert_eq!(hub.job(&worker.id).unwrap().duration_ms, 0);
}

#[test]
fn coordinator_clock_runs_only_while_a_descendant_is_doing_work() {
    use crate::agent::turn_state::TurnPhase;
    let (_fixture, hub) = hub();
    let worker = job(&hub, Role::Builder, ".");
    hub.manifest
        .lock()
        .unwrap()
        .jobs
        .insert(worker.id.clone(), worker.clone());
    let (session, _) = storage::worker(&hub, &worker, None).unwrap();
    hub.live
        .lock()
        .unwrap()
        .insert(worker.id.clone(), session.clone());
    hub.root.transition(TurnPhase::WaitingForAgents).unwrap();
    hub.refresh_waiting_clocks();
    assert!(hub.root.snapshot().unwrap().turns[0].active_since.is_none());
    session.transition(TurnPhase::Sampling).unwrap();
    assert!(hub.root.snapshot().unwrap().turns[0].active_since.is_some());
    session.transition(TurnPhase::WaitingForApproval).unwrap();
    assert!(hub.root.snapshot().unwrap().turns[0].active_since.is_none());
    session.transition(TurnPhase::ExecutingTools).unwrap();
    assert!(hub.root.snapshot().unwrap().turns[0].active_since.is_some());
    finish(&session, Err(AgentError::cancelled()));
    assert!(hub.root.snapshot().unwrap().turns[0].active_since.is_none());
}

fn planned_flow_evaluation_case() -> Value {
    serde_json::from_str(include_str!(
        "../fixtures/evaluations/movarte-planned-flow.json"
    ))
    .unwrap()
}

fn spawn_roles(flow: Flow, role: Role) -> Vec<String> {
    definitions(flow, role)
        .into_iter()
        .find(|definition| definition["name"] == "hub_spawn")
        .unwrap()["parameters"]["properties"]["role"]["enum"]
        .as_array()
        .unwrap()
        .iter()
        .map(|value| value.as_str().unwrap().to_owned())
        .collect()
}

#[test]
fn harness_evaluation_spawn_schema_only_advertises_allowed_roles() {
    let case = planned_flow_evaluation_case();
    let expected: Vec<String> = case["expectations"]["allowedPlannerSpawnRoles"]
        .as_array()
        .unwrap()
        .iter()
        .map(|role| role.as_str().unwrap().to_owned())
        .collect();
    assert_eq!(spawn_roles(Flow::Planned, Role::Planner), expected);
    assert_eq!(
        spawn_roles(Flow::Complete, Role::Planner),
        vec!["investigator", "writer", "orchestrator"]
    );
    assert_eq!(
        spawn_roles(Flow::Complete, Role::Orchestrator),
        vec!["planner", "designer", "builder", "reviewer"]
    );
}

#[test]
fn workflow_check_rejects_missing_bun_scripts_and_lists_real_options() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(
        directory.path().join("package.json"),
        r#"{"scripts":{"test":"vitest","build":"vite build"}}"#,
    )
    .unwrap();
    let error = workflow_command(directory.path(), "bun_typecheck").unwrap_err();
    assert!(error.message.contains("typecheck"));
    assert!(error.message.contains("build"));
    assert!(error.message.contains("test"));
    assert_eq!(
        workflow_command(directory.path(), "bun_test").unwrap(),
        "bun run test"
    );
}

#[test]
fn workflow_check_uses_the_existing_type_check_script() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(
        directory.path().join("package.json"),
        r#"{"scripts":{"type-check":"tsc --noEmit"}}"#,
    )
    .unwrap();
    assert_eq!(
        workflow_command(directory.path(), "bun_typecheck").unwrap(),
        "bun run type-check"
    );
}

#[test]
fn designer_is_always_an_implementation_role_and_complete_routes_it_through_orchestrator() {
    assert!(validate_phase(
        Flow::Complete,
        Role::Planner,
        Role::Designer,
        Phase::Discovery
    )
    .is_err());
    assert!(validate_phase(
        Flow::Complete,
        Role::Planner,
        Role::Designer,
        Phase::Implementation
    )
    .is_err());
    assert!(validate_phase(
        Flow::Complete,
        Role::Orchestrator,
        Role::Designer,
        Phase::Implementation
    )
    .is_ok());
    assert!(validate_phase(
        Flow::Planned,
        Role::Planner,
        Role::Designer,
        Phase::Implementation
    )
    .is_ok());
    assert!(validate_phase(
        Flow::Planned,
        Role::Planner,
        Role::Builder,
        Phase::Discovery
    )
    .is_err());
    let phases = definitions(Flow::Planned, Role::Planner)
        .into_iter()
        .find(|definition| definition["name"] == "hub_spawn")
        .unwrap()["parameters"]["properties"]["phase"]["enum"]
        .as_array()
        .unwrap()
        .clone();
    assert_eq!(phases, vec![json!("implementation")]);
}

#[test]
fn dependent_work_waits_and_failed_dependencies_block_instead_of_running() {
    let (_fixture, hub) = hub();
    let mut dependency = job(&hub, Role::Builder, "src/a");
    dependency.status = Status::Running;
    let mut next = job(&hub, Role::Reviewer, ".");
    next.dependencies = vec![dependency.id.clone()];
    let mut state = hub.manifest.lock().unwrap();
    state.jobs.insert(dependency.id.clone(), dependency.clone());
    assert!(!admitted(&state, &next).unwrap());
    state.jobs.get_mut(&dependency.id).unwrap().status = Status::Failed;
    assert!(admitted(&state, &next).is_err());
    state.jobs.get_mut(&dependency.id).unwrap().status = Status::Completed;
    assert!(admitted(&state, &next).unwrap());
}

#[test]
fn independent_scopes_run_in_parallel_but_overlap_and_leaf_limit_queue() {
    let (_fixture, hub) = hub();
    let mut first = job(&hub, Role::Builder, "src/a");
    first.status = Status::Running;
    let same = job(&hub, Role::Designer, "src/a/nested");
    let independent = job(&hub, Role::Builder, "src/b");
    let mut state = hub.manifest.lock().unwrap();
    state.jobs.insert(first.id.clone(), first);
    assert!(!admitted(&state, &same).unwrap());
    assert!(admitted(&state, &independent).unwrap());
    for _ in 0..3 {
        let mut other = independent.clone();
        other.id = library::new_id().unwrap();
        other.role = Role::Investigator;
        other.status = Status::Running;
        state.jobs.insert(other.id.clone(), other);
    }
    assert!(!admitted(&state, &independent).unwrap());
    let mut coordinator = independent.clone();
    coordinator.role = Role::Orchestrator;
    assert!(admitted(&state, &coordinator).unwrap());
}

#[tokio::test]
async fn completion_wakes_parent_once_even_when_it_arrives_before_wait_registration() {
    let (_fixture, hub) = hub();
    let mut child = job(&hub, Role::Investigator, ".");
    child.status = Status::Running;
    child.handoff = Some(Handoff {
        verdict: Verdict::Completed,
        summary: "Evidence collected".into(),
        outcomes: vec!["Located handler".into()],
        evidence: vec!["src/handler.ts:14".into()],
        validation: vec![],
        limitations: vec![],
        task_ids: vec![],
    });
    hub.mutate(|state| {
        state.jobs.insert(child.id.clone(), child.clone());
        Ok(())
    })
    .unwrap();
    let generation = super::super::super::generation::Metrics {
        output_tokens: 100,
        duration_ms: 1_000,
        estimated: false,
    };
    settle_generation(&hub, &child, &Ok(()), Some(1200), Some(generation.clone())).unwrap();
    let persisted = storage::load(&hub.directory, &hub.root.id)
        .unwrap()
        .unwrap();
    assert_eq!(
        persisted.jobs[&child.id].generation,
        Some(generation.clone())
    );
    let roster = commands::snapshot(&persisted, None).unwrap();
    let serialized = serde_json::to_value(roster).unwrap();
    assert_eq!(
        serialized["agents"][1]["generation"],
        serde_json::to_value(generation).unwrap()
    );
    let messages = tokio::time::timeout(
        Duration::from_secs(1),
        hub.wait("main", hub.root_signal.clone()),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(messages.len(), 1);
    assert!(messages[0].text.contains("Evidence collected"));
    assert!(hub
        .wait("main", hub.root_signal.clone())
        .await
        .unwrap()
        .is_empty());
}

#[tokio::test]
async fn waiting_is_cancellable_and_never_busy_polls() {
    let (_fixture, hub) = hub();
    let mut child = job(&hub, Role::Investigator, ".");
    child.status = Status::Running;
    hub.mutate(|state| {
        state.jobs.insert(child.id.clone(), child);
        Ok(())
    })
    .unwrap();
    let revision = hub.manifest.lock().unwrap().revision;
    let (cancel, signal) = watch::channel(false);
    let h = hub.clone();
    let wait = tokio::spawn(async move { h.wait("main", signal).await });
    tokio::task::yield_now().await;
    assert_eq!(hub.manifest.lock().unwrap().revision, revision);
    assert!(!wait.is_finished());
    cancel.send_replace(true);
    assert_eq!(wait.await.unwrap().unwrap_err().code, "cancelled");
}

#[test]
fn writer_scope_rejects_product_code_absolute_escapes_and_traversal() {
    let root = Path::new("/project");
    let scope = vec!["docs".into()];
    assert!(path_allowed(
        root,
        &json!({"path":"docs/PLAN-feature.md"}),
        &scope,
        Role::Writer
    ));
    for path in [
        "src/app.ts",
        "docs/README.md",
        "docs/../src/app.ts",
        "/etc/secret",
        "docs/PLAN-x.ts",
    ] {
        assert!(!path_allowed(
            root,
            &json!({"path":path}),
            &scope,
            Role::Writer
        ));
    }
}

#[test]
fn beads_dispatch_checks_real_status_and_blocking_dependencies() {
    assert!(validate_bead(
        &json!([{"status":"open","dependencies":[{"dependency_type":"blocks","status":"closed"}]}]),
        &[]
    )
    .is_ok());
    assert!(validate_bead(
        &json!([{"status":"open","dependencies":[{"dependency_type":"blocks","status":"open"}]}]),
        &[]
    )
    .is_err());
    for status in ["closed", "blocked", "deferred"] {
        assert!(validate_bead(&json!({"status":status}), &[]).is_err());
    }
    assert!(validate_bead(&json!([]), &[]).is_err());
}

fn completed_handoff(id: &str) -> Handoff {
    Handoff {
        verdict: Verdict::Completed,
        summary: "Implementation verified".into(),
        outcomes: vec!["Acceptance criteria met".into()],
        evidence: vec!["Focused regression passed".into()],
        validation: vec![],
        limitations: vec![],
        task_ids: vec![id.into()],
    }
}

#[test]
fn blocked_handoff_reports_the_blocker_without_reopening_its_bead() {
    let (_fixture, hub) = hub();
    let mut worker = job(&hub, Role::Builder, ".");
    worker.bead_id = Some("task".into());
    let state = hub.manifest.lock().unwrap();
    let task = json!({"id":"task","status":"blocked",
        "assignee":format!("jarvis-{}", state.conversation_id),
        "dependencies":[{"id":"access","dependency_type":"blocks","status":"blocked"}]});
    let handoff = Handoff {
        verdict: Verdict::Blocked,
        summary: "Configured access is unavailable".into(),
        limitations: vec!["A current parent decision is required".into()],
        ..completed_handoff("task")
    };
    assert!(validate_completion_bead(&state, &worker, &task, &handoff).is_ok());
    assert_eq!(task["status"], "blocked");
    assert!(validate_bead(&task, &[]).is_err());
    for verdict in [Verdict::Completed, Verdict::Approved, Verdict::Rework] {
        let success = Handoff {
            verdict,
            ..handoff.clone()
        };
        assert!(validate_completion_bead(&state, &worker, &task, &success).is_err());
    }
    let mut wrong_task = handoff.clone();
    wrong_task.task_ids = vec!["another-task".into()];
    assert!(validate_completion_bead(&state, &worker, &task, &wrong_task).is_err());
    let mut foreign = task.clone();
    foreign["assignee"] = json!("another-conversation");
    assert!(validate_completion_bead(&state, &worker, &foreign, &handoff).is_err());
    worker.run_id = "previous-run".into();
    assert!(validate_completion_bead(&state, &worker, &task, &handoff).is_err());
}

#[test]
fn unresolved_dependencies_allow_a_blocked_report_but_not_completion() {
    let (_fixture, hub) = hub();
    let worker = job(&hub, Role::Builder, ".");
    let state = hub.manifest.lock().unwrap();
    let task = json!({"status":"in_progress",
        "dependencies":[{"id":"access","dependency_type":"blocks","status":"open"}]});
    let mut handoff = completed_handoff("task");
    assert!(validate_completion_bead(&state, &worker, &task, &handoff).is_err());
    handoff.verdict = Verdict::Blocked;
    assert!(validate_completion_bead(&state, &worker, &task, &handoff).is_ok());
    assert!(validate_bead(&task, &[]).is_err());
}

#[test]
fn approved_closed_epic_can_finish_without_becoming_executable_again() {
    let (_fixture, hub) = hub();
    let mut coordinator = job(&hub, Role::Orchestrator, ".");
    coordinator.bead_id = Some("epic".into());
    let mut reviewer = job(&hub, Role::Reviewer, ".");
    reviewer.status = Status::Completed;
    reviewer.handoff = Some(Handoff {
        verdict: Verdict::Approved,
        ..completed_handoff("epic")
    });
    let mut state = hub.manifest.lock().unwrap();
    state.flow = Flow::Complete;
    state.options.manual_validation = false;
    state.jobs.insert(reviewer.id.clone(), reviewer.clone());
    let task = json!({"id":"epic","issue_type":"epic","status":"closed",
        "assignee":format!("jarvis-{}", state.conversation_id),"comments":[]});
    let handoff = completed_handoff("epic");
    assert!(validate_completion_bead(&state, &coordinator, &task, &handoff).is_ok());
    assert!(validate_bead(&task, &[]).is_err());
    for status in ["blocked", "deferred"] {
        let mut blocked = task.clone();
        blocked["status"] = json!(status);
        assert!(validate_completion_bead(&state, &coordinator, &blocked, &handoff).is_err());
    }
    let mut foreign = task.clone();
    foreign["assignee"] = json!("another-conversation");
    assert!(validate_completion_bead(&state, &coordinator, &foreign, &handoff).is_err());
    state.options.manual_validation = true;
    assert!(validate_completion_bead(&state, &coordinator, &task, &handoff).is_err());
    state.validation = Some(validation::Batch {
        id: "acceptance".into(),
        flow: Flow::Complete,
        run_id: state.run_id.clone(),
        epic_ids: vec!["epic".into()],
        submitted: true,
        stale: false,
        created_at: now(),
        items: vec![validation::Item {
            id: "criterion".into(),
            title: "Acceptance".into(),
            steps: vec!["Verify the outcome".into()],
            expected: "Works".into(),
            decision: validation::Decision::Approved,
            reason: None,
        }],
    });
    assert!(validate_completion_bead(&state, &coordinator, &task, &handoff).is_ok());
    state.validation.as_mut().unwrap().stale = true;
    assert!(validate_completion_bead(&state, &coordinator, &task, &handoff).is_err());
    state.options.manual_validation = false;
    state.jobs.remove(&reviewer.id);
    assert!(validate_completion_bead(&state, &coordinator, &task, &handoff).is_err());
}

#[tokio::test]
async fn completion_retries_preserve_the_accepted_handoff() {
    let (_fixture, hub) = hub();
    let child = job(&hub, Role::Investigator, ".");
    hub.mutate(|state| {
        state.jobs.insert(child.id.clone(), child.clone());
        Ok(())
    })
    .unwrap();
    let exec = Execution {
        hub: hub.clone(),
        id: child.id.clone(),
        role: child.role,
        flow: Flow::Complete,
        scope: child.scope.clone(),
    };
    let handoff = completed_handoff("evidence");
    let (_, signal) = watch::channel(false);
    complete(&exec, handoff.clone(), signal.clone())
        .await
        .unwrap();
    complete(&exec, handoff.clone(), signal.clone())
        .await
        .unwrap();
    let mut replacement = handoff.clone();
    replacement.summary = "Different outcome".into();
    assert!(complete(&exec, replacement, signal).await.is_err());
    assert_eq!(hub.job(&child.id).unwrap().handoff, Some(handoff));
}

#[test]
fn verified_progress_allows_recovery_but_a_failed_child_does_not() {
    let (_fixture, hub) = hub();
    let mut parent = job(&hub, Role::Orchestrator, ".");
    parent.status = Status::Failed;
    parent.recovery_attempts = 2;
    let mut child = job(&hub, Role::Builder, "src");
    child.parent_id = parent.id.clone();
    child.handoff = Some(completed_handoff("task"));
    hub.mutate(|state| {
        state.jobs.insert(parent.id.clone(), parent.clone());
        state.jobs.insert(child.id.clone(), child.clone());
        Ok(())
    })
    .unwrap();
    settle(&hub, &child, &Err(AgentError::internal()), None).unwrap();
    assert_eq!(hub.job(&parent.id).unwrap().recovery_attempts, 2);
    settle(&hub, &child, &Ok(()), None).unwrap();
    let mut state = hub.manifest.lock().unwrap();
    let recovered = prepare_retry(
        &mut state,
        "main",
        Flow::Complete,
        Role::Planner,
        &parent.id,
        None,
    )
    .unwrap();
    assert_eq!(recovered.recovery_attempts, 0);
    drop(state);
    begin_recovery_attempt(&hub, &parent.id).unwrap();
    assert_eq!(hub.job(&parent.id).unwrap().recovery_attempts, 1);
    let exec = Execution {
        hub: hub.clone(),
        id: parent.id.clone(),
        role: parent.role,
        flow: Flow::Complete,
        scope: parent.scope,
    };
    exec.record_confirmed_progress().unwrap();
    assert_eq!(hub.job(&parent.id).unwrap().recovery_attempts, 0);
    hub.mutate(|state| {
        let parent = state.jobs.get_mut(&parent.id).unwrap();
        parent.run_id = "next-run".into();
        parent.recovery_attempts = 2;
        Ok(())
    })
    .unwrap();
    settle(&hub, &child, &Ok(()), None).unwrap();
    assert_eq!(hub.job(&parent.id).unwrap().recovery_attempts, 2);
}

#[test]
fn harness_evaluation_resumed_repair_gets_the_entire_finding_contract() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../fixtures/evaluations/movarte-resumed-review.json"
    ))
    .unwrap();
    let handoff: Handoff = serde_json::from_value(fixture["handoff"].clone()).unwrap();
    assert!(handoff.summary.len() > 300);
    for role in [Role::Builder, Role::Designer, Role::Custom] {
        let (_fixture, hub) = hub();
        let mut reviewer = job(
            &hub,
            if role == Role::Custom {
                Role::Custom
            } else {
                Role::Reviewer
            },
            "src/integration",
        );
        reviewer.status = Status::Blocked;
        reviewer.handoff = Some(handoff.clone());
        let mut worker = job(&hub, role, "src/integration");
        worker.dependencies = vec![reviewer.id.clone()];
        let unrelated = job(&hub, role, "unrelated");
        hub.mutate(|state| {
            for job in [&worker, &reviewer, &unrelated] {
                state.jobs.insert(job.id.clone(), job.clone());
            }
            Ok(())
        })
        .unwrap();
        let exec = Execution {
            hub: hub.clone(),
            id: worker.id,
            role,
            flow: Flow::Complete,
            scope: worker.scope,
        };
        let context = exec.context().unwrap();
        assert!(
            context.contains(&json!(handoff).to_string()),
            "repair contract was truncated"
        );
        let other = Execution {
            id: unrelated.id,
            scope: unrelated.scope,
            ..exec
        };
        assert!(!other.context().unwrap().contains("reviewFindings"));
        assert!(
            !technically_approved(&hub.manifest.lock().unwrap(), "integration-report"),
            "a repair contract is not independent approval"
        );
    }
}

#[test]
fn bead_checkpoint_tracks_requirements_and_comments_without_progress_noise() {
    let source = json!([{
        "id":"jproject-task",
        "title":"Implement feature",
        "description":"Keep the existing contract",
        "status":"in_progress",
        "assignee":"jarvis-conversation",
        "notes":"Started",
        "updated_at":"2026-09-11T10:00:00Z",
        "dependencies":[],
        "comments":[{"id":1,"author":"Você","text":"Preserve the API"}]
    }]);
    let initial = bead_checkpoint(&source).unwrap();
    let mut progress = source.clone();
    progress[0]["notes"] = json!("Validation passed");
    progress[0]["updated_at"] = json!("2026-09-11T11:00:00Z");
    assert_eq!(
        initial.fingerprint,
        bead_checkpoint(&progress).unwrap().fingerprint
    );

    let mut commented = progress.clone();
    commented[0]["comments"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":2,"author":"Você","text":"Also cover the empty state"}));
    let refreshed = bead_checkpoint(&commented).unwrap();
    assert_ne!(initial.fingerprint, refreshed.fingerprint);
    let error = bead_changed_error(&refreshed);
    assert_eq!(error.code, "beads_changed");
    assert!(error
        .tool_result
        .unwrap()
        .contains("Also cover the empty state"));

    let mut changed_requirement = source;
    changed_requirement[0]["description"] = json!("Keep the API and add pagination");
    assert_ne!(
        initial.fingerprint,
        bead_checkpoint(&changed_requirement).unwrap().fingerprint
    );
}

#[test]
fn assigned_bead_comments_are_injected_before_worker_execution() {
    let (_fixture, hub) = hub();
    let child = job(&hub, Role::Builder, "src");
    let (session, _) = storage::worker(&hub, &child, None).unwrap();
    let checkpoint = bead_checkpoint(&json!([{
        "id":"jproject-task",
        "title":"Implement feature",
        "status":"open",
        "comments":[{"id":3,"author":"Você","text":"Use the shared component"}]
    }]))
    .unwrap();
    inject_bead_checkpoint(&session, "jproject-task", &checkpoint).unwrap();
    let input = session.input().unwrap();
    let content = input
        .iter()
        .filter_map(|item| item["content"].as_str())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(content.contains("Assigned Beads task snapshot"));
    assert!(content.contains("Use the shared component"));
    assert!(content.contains("re-read this task and its comments"));
}

#[test]
fn review_admits_completed_implementation_without_removing_its_beads_dependency() {
    let (_fixture, hub) = hub();
    let mut builder = job(&hub, Role::Builder, "src");
    builder.bead_id = Some("implementation".into());
    builder.status = Status::Completed;
    builder.handoff = Some(Handoff {
        verdict: Verdict::Completed,
        summary: "Implemented".into(),
        outcomes: vec!["Works".into()],
        evidence: vec![],
        validation: vec![],
        limitations: vec![],
        task_ids: vec!["implementation".into()],
    });
    let mut reviewer = job(&hub, Role::Reviewer, ".");
    reviewer.dependencies = vec![builder.id.clone()];
    let mut state = hub.manifest.lock().unwrap();
    state.jobs.insert(builder.id.clone(), builder.clone());
    let task = json!({"status":"open","dependencies":[{"id":"implementation","dependency_type":"blocks","status":"in_progress"}]});
    assert!(validate_bead(&task, &review_dependencies(&state, &reviewer)).is_ok());
    assert!(validate_bead(&task, &review_dependencies(&state, &builder)).is_err());
    for role in [Role::Builder, Role::Designer] {
        let mut dependent = reviewer.clone();
        dependent.role = role;
        dependent.dependencies = vec![builder.id.clone()];
        assert!(validate_bead(&task, &review_dependencies(&state, &dependent)).is_ok());
    }
    for status in [Status::Running, Status::Blocked, Status::Failed] {
        state.jobs.get_mut(&builder.id).unwrap().status = status;
        assert!(validate_bead(&task, &review_dependencies(&state, &reviewer)).is_err());
    }
    state.jobs.get_mut(&builder.id).unwrap().status = Status::Completed;
    state.jobs.get_mut(&builder.id).unwrap().run_id = "previous".into();
    assert!(validate_bead(&task, &review_dependencies(&state, &reviewer)).is_err());
    let blocked = json!({"status":"open","dependencies":[{"id":"implementation","dependency_type":"blocks","status":"blocked"}]});
    assert!(validate_bead(&blocked, &["implementation".into()]).is_err());
}

#[test]
fn repair_can_depend_on_a_review_with_findings_but_not_a_failed_review() {
    let (_fixture, hub) = hub();
    let mut review = job(&hub, Role::Reviewer, "backend");
    review.status = Status::Blocked;
    review.handoff = Some(Handoff {
        verdict: Verdict::Rework,
        summary: "Fix the missing validation".into(),
        outcomes: vec![],
        evidence: vec![],
        validation: vec![],
        limitations: vec![],
        task_ids: vec![],
    });
    let mut repair = job(&hub, Role::Builder, "backend");
    repair.dependencies = vec![review.id.clone()];
    let mut state = hub.manifest.lock().unwrap();
    state.jobs.insert(review.id.clone(), review.clone());
    assert!(admitted(&state, &repair).unwrap());
    repair.role = Role::Reviewer;
    assert!(admitted(&state, &repair).is_err());
    repair.role = Role::Builder;
    state
        .jobs
        .get_mut(&review.id)
        .unwrap()
        .handoff
        .as_mut()
        .unwrap()
        .verdict = Verdict::Blocked;
    assert!(admitted(&state, &repair).is_err());
    state.jobs.get_mut(&review.id).unwrap().status = Status::Failed;
    assert!(admitted(&state, &repair).is_err());
}

#[test]
fn reviewers_and_overlapping_writers_never_run_together() {
    let (_fixture, hub) = hub();
    let mut builder = job(&hub, Role::Builder, "src");
    builder.status = Status::Running;
    let mut reviewer = job(&hub, Role::Reviewer, ".");
    let mut state = hub.manifest.lock().unwrap();
    state.jobs.insert(builder.id.clone(), builder.clone());
    assert!(!admitted(&state, &reviewer).unwrap());
    state.jobs.clear();
    reviewer.status = Status::Running;
    state.jobs.insert(reviewer.id.clone(), reviewer);
    assert!(!admitted(&state, &builder).unwrap());
}

#[tokio::test]
async fn failed_coordinator_waits_for_children_to_settle_before_retry_can_start() {
    let (_fixture, hub) = hub();
    let mut child = job(&hub, Role::Builder, "src");
    child.status = Status::Running;
    hub.mutate(|state| {
        state.jobs.insert(child.id.clone(), child.clone());
        Ok(())
    })
    .unwrap();
    let task_hub = hub.clone();
    let waiting = tokio::spawn(async move { await_children_settled(&task_hub, "main").await });
    tokio::task::yield_now().await;
    assert!(!waiting.is_finished());
    settle(&hub, &child, &Err(AgentError::cancelled()), Some(800)).unwrap();
    tokio::time::timeout(Duration::from_secs(1), waiting)
        .await
        .unwrap()
        .unwrap();
}

#[tokio::test]
async fn cancellation_reaches_nested_workers_and_failure_recovery_limit_is_enforced() {
    let (_fixture, hub) = hub();
    let mut parent = job(&hub, Role::Orchestrator, ".");
    parent.status = Status::Running;
    let mut child = job(&hub, Role::Builder, "src");
    child.parent_id = parent.id.clone();
    let (session, signal) = storage::worker(&hub, &child, None).unwrap();
    hub.live.lock().unwrap().insert(child.id.clone(), session);
    hub.mutate(|state| {
        state.jobs.insert(parent.id.clone(), parent.clone());
        state.jobs.insert(child.id.clone(), child.clone());
        Ok(())
    })
    .unwrap();
    cancel_tree(&hub, &parent.id).unwrap();
    assert!(*signal.borrow());
    hub.mutate(|state| {
        let job = state.jobs.get_mut(&parent.id).unwrap();
        job.status = Status::Failed;
        job.attempts = 3;
        job.recovery_attempts = 2;
        Ok(())
    })
    .unwrap();
    let exec = Execution {
        hub,
        id: "main".into(),
        role: Role::Planner,
        flow: Flow::Complete,
        scope: vec![".".into()],
    };
    assert!(retry(
        &exec,
        &parent.id,
        "Try after inspecting the checkpoint",
        None
    )
    .is_err());
    let mut state = exec.hub.manifest.lock().unwrap();
    state.run_id = "new-user-turn-after-provider-repair".into();
    let recovered = prepare_retry(
        &mut state,
        "main",
        Flow::Complete,
        Role::Planner,
        &parent.id,
        None,
    )
    .unwrap();
    assert_eq!(recovered.recovery_attempts, 0);
    assert_eq!(recovered.id, parent.id);
}

#[test]
fn failed_retry_dependencies_preserve_the_worker_checkpoint_and_budget() {
    let (_fixture, hub) = hub();
    let mut builder = job(&hub, Role::Builder, "backend");
    builder.status = Status::Failed;
    let mut active = job(&hub, Role::Reviewer, "other");
    active.status = Status::Running;
    let mut designer = job(&hub, Role::Designer, "frontend");
    designer.status = Status::Failed;
    designer.error = Some("Provider retry budget exhausted".into());
    designer.recovery_attempts = 1;
    designer.dependencies = vec![active.id.clone(), builder.id.clone()];
    let mut state = hub.manifest.lock().unwrap();
    state.jobs.insert(builder.id.clone(), builder.clone());
    state.jobs.insert(active.id.clone(), active);
    state.jobs.insert(designer.id.clone(), designer.clone());
    let before = serde_json::to_value(&*state).unwrap();
    let error = prepare_retry(
        &mut state,
        "main",
        Flow::Planned,
        Role::Planner,
        &designer.id,
        None,
    )
    .unwrap_err();
    assert!(error.message.contains(&builder.id));
    assert_eq!(serde_json::to_value(&*state).unwrap(), before);
}

#[tokio::test]
async fn dependency_failure_while_queued_does_not_spend_the_last_worker_recovery() {
    let (_fixture, hub) = hub();
    let mut builder = job(&hub, Role::Builder, "backend");
    builder.status = Status::Running;
    let mut designer = job(&hub, Role::Designer, "frontend");
    designer.status = Status::Failed;
    designer.recovery_attempts = 1;
    designer.dependencies = vec![builder.id.clone()];
    let queued = hub
        .mutate(|state| {
            state.jobs.insert(builder.id.clone(), builder.clone());
            state.jobs.insert(designer.id.clone(), designer.clone());
            prepare_retry(
                state,
                "main",
                Flow::Planned,
                Role::Planner,
                &designer.id,
                None,
            )
        })
        .unwrap();
    assert_eq!(queued.recovery_attempts, 1);
    assert!(continuation_instructions(&queued).contains("uncertain tool action"));
    launch(
        hub.clone(),
        queued,
        Some("Recover the remaining work".into()),
    )
    .unwrap();
    settle(&hub, &builder, &Err(AgentError::internal()), None).unwrap();
    tokio::time::timeout(Duration::from_secs(2), await_children_settled(&hub, "main"))
        .await
        .unwrap();
    let failed = hub.job(&designer.id).unwrap();
    assert_eq!(failed.status, Status::Failed);
    assert_eq!(failed.recovery_attempts, 1);
    assert!(failed.error.unwrap().contains(&builder.id));
    hub.mutate(|state| {
        state.jobs.get_mut(&builder.id).unwrap().status = Status::Completed;
        prepare_retry(
            state,
            "main",
            Flow::Planned,
            Role::Planner,
            &designer.id,
            None,
        )
    })
    .unwrap();
    begin_recovery_attempt(&hub, &designer.id).unwrap();
    assert_eq!(hub.job(&designer.id).unwrap().recovery_attempts, 2);
    settle(&hub, &designer, &Err(AgentError::internal()), None).unwrap();
    assert!(hub
        .mutate(|state| prepare_retry(
            state,
            "main",
            Flow::Planned,
            Role::Planner,
            &designer.id,
            None,
        ))
        .is_err());
}

#[tokio::test]
async fn restarted_recovery_charges_only_the_attempt_that_had_not_started() {
    for started in [false, true] {
        let (_fixture, hub) = hub();
        let mut worker = job(&hub, Role::Designer, "frontend");
        worker.status = Status::Failed;
        worker.recovery_attempts = 1;
        let queued = hub
            .mutate(|state| {
                state.jobs.insert(worker.id.clone(), worker.clone());
                prepare_retry(
                    state,
                    "main",
                    Flow::Planned,
                    Role::Planner,
                    &worker.id,
                    None,
                )
            })
            .unwrap();
        let (session, signal) =
            storage::worker(&hub, &queued, Some("Resume the repair".into())).unwrap();
        if started {
            await_admission(&hub, &queued, signal).await.unwrap();
            begin_recovery_attempt(&hub, &worker.id).unwrap();
        }
        drop(session);
        let saved: Value =
            serde_json::from_slice(&std::fs::read(hub.directory.join("state.json")).unwrap())
                .unwrap();
        assert_eq!(
            saved["jobs"][&worker.id].get("recoveryAttemptPending"),
            (!started).then_some(&Value::Bool(true)),
        );
        let loaded = storage::load(&hub.directory, &hub.root.id)
            .unwrap()
            .unwrap();
        let (recovered, mut resumed) =
            storage::prepare_recovery(&hub.directory, loaded, Flow::Complete, "run", vec![])
                .unwrap();
        assert_eq!(resumed.len(), 1);
        assert_eq!(resumed[0].recovery_attempt_pending, !started);
        assert_eq!(resumed[0].recovery_attempts, if started { 2 } else { 1 });
        *hub.manifest.lock().unwrap() = recovered;
        // The isolated fixture has no provider account; reaching that preflight
        // failure proves the resumed worker started without a network request.
        resume(hub.clone(), resumed.remove(0)).unwrap();
        tokio::time::timeout(Duration::from_secs(2), await_children_settled(&hub, "main"))
            .await
            .unwrap();
        let finished = hub.job(&worker.id).unwrap();
        assert_eq!(finished.status, Status::Failed);
        assert_eq!(finished.recovery_attempts, 2);
        assert!(!finished.recovery_attempt_pending);
        begin_recovery_attempt(&hub, &worker.id).unwrap();
        assert_eq!(hub.job(&worker.id).unwrap().recovery_attempts, 2);
    }
}

fn dispatch_for(job: &Job) -> Dispatch {
    Dispatch {
        role: job.role,
        phase: job.phase,
        title: job.title.clone(),
        prompt: job.prompt.clone(),
        acceptance: job.acceptance.clone(),
        scope: job.scope.clone(),
        bead_id: job.bead_id.clone(),
        dependencies: job.dependencies.clone(),
    }
}

#[test]
fn harness_evaluation_duplicate_active_task_returns_its_worker_without_discarding_new_instructions_silently(
) {
    let (_fixture, hub) = hub();
    let mut worker = job(&hub, Role::Builder, "backend");
    worker.bead_id = Some("task-api".into());
    worker.status = Status::Running;
    hub.mutate(|state| {
        state.jobs.insert(worker.id.clone(), worker.clone());
        Ok(())
    })
    .unwrap();
    let exec = Execution {
        hub: hub.clone(),
        id: "main".into(),
        role: Role::Planner,
        flow: Flow::Planned,
        scope: vec![".".into()],
    };
    let mut input = dispatch_for(&worker);
    input.prompt = "Also test the empty request".into();
    input.acceptance.push("Reject empty requests".into());
    let result: Value = serde_json::from_str(&spawn(&exec, input).unwrap()).unwrap();
    assert_eq!(result["existingAgentId"], worker.id);
    assert_eq!(result["instructionScheduled"], false);
    assert!(result["nextAction"].as_str().unwrap().contains("hub_send"));
    let state = hub.manifest.lock().unwrap();
    assert_eq!(state.jobs.len(), 1);
    assert!(state.messages.is_empty());
    assert_eq!(state.jobs[&worker.id].prompt, worker.prompt);
    assert_eq!(state.jobs[&worker.id].acceptance, worker.acceptance);
    assert!(hub.live.lock().unwrap().is_empty());
}

#[test]
fn duplicate_detection_preserves_independent_tasks_roles_scopes_and_runs() {
    let (_fixture, hub) = hub();
    let mut worker = job(&hub, Role::Builder, "backend");
    worker.bead_id = Some("task-api".into());
    let mut state = hub.manifest.lock().unwrap();
    state.jobs.insert(worker.id.clone(), worker.clone());
    let mut input = dispatch_for(&worker);
    assert!(active_duplicate(&state, "main", &input).is_some());
    input.scope = vec!["frontend".into()];
    assert!(active_duplicate(&state, "main", &input).is_none());
    input = dispatch_for(&worker);
    input.bead_id = Some("other-task".into());
    assert!(active_duplicate(&state, "main", &input).is_none());
    input = dispatch_for(&worker);
    input.role = Role::Reviewer;
    assert!(active_duplicate(&state, "main", &input).is_none());
    input = dispatch_for(&worker);
    assert!(active_duplicate(&state, "other-parent", &input).is_none());
    state.jobs.get_mut(&worker.id).unwrap().run_id = "old-run".into();
    assert!(active_duplicate(&state, "main", &input).is_none());
    let stored = state.jobs.get_mut(&worker.id).unwrap();
    stored.run_id = "run".into();
    stored.status = Status::Completed;
    assert!(active_duplicate(&state, "main", &input).is_none());
    let stored = state.jobs.get_mut(&worker.id).unwrap();
    stored.status = Status::Running;
    stored.bead_id = None;
    stored.role = Role::Investigator;
    input = dispatch_for(stored);
    input.prompt = "Investigate a different part of the feature".into();
    assert!(active_duplicate(&state, "main", &input).is_none());
}

#[test]
fn harness_evaluation_successful_followups_keep_worker_context_and_models_beyond_two_rounds() {
    let (_fixture, hub) = hub();
    let mut worker = job(&hub, Role::Builder, "backend");
    worker.status = Status::Completed;
    worker.options.account = "worker-account".into();
    worker.options.model = "worker-model".into();
    worker.recovery_attempts = 2;
    let mut state = hub.manifest.lock().unwrap();
    state.jobs.insert(worker.id.clone(), worker.clone());
    for round in 2..=9 {
        let continued = prepare_retry(
            &mut state,
            "main",
            Flow::Planned,
            Role::Planner,
            &worker.id,
            None,
        )
        .unwrap();
        assert_eq!(continued.id, worker.id);
        assert_eq!(continued.attempts, round);
        assert_eq!(continued.recovery_attempts, 0);
        assert_eq!(continued.options.account, "worker-account");
        assert_eq!(continued.options.model, "worker-model");
        state.jobs.get_mut(&worker.id).unwrap().status = Status::Completed;
    }
    assert_eq!(state.jobs.len(), 1);
}

#[test]
fn harness_evaluation_rework_reuses_builder_and_reviewer_with_completed_prior_round_dependencies() {
    let (_fixture, hub) = hub();
    let mut builder = job(&hub, Role::Builder, "backend");
    builder.status = Status::Completed;
    let mut reviewer = job(&hub, Role::Reviewer, "backend");
    reviewer.status = Status::Blocked;
    reviewer.recovery_attempts = 2;
    reviewer.handoff = Some(Handoff {
        verdict: Verdict::Rework,
        summary: "Add the missing validation".into(),
        outcomes: vec![],
        evidence: vec!["backend/handler.ts".into()],
        validation: vec![],
        limitations: vec![],
        task_ids: vec![],
    });
    reviewer.dependencies = vec![builder.id.clone()];
    let mut state = hub.manifest.lock().unwrap();
    state.jobs.insert(builder.id.clone(), builder.clone());
    state.jobs.insert(reviewer.id.clone(), reviewer.clone());
    let repairing = prepare_retry(
        &mut state,
        "main",
        Flow::Complete,
        Role::Orchestrator,
        &builder.id,
        Some(vec![reviewer.id.clone()]),
    )
    .unwrap();
    assert!(admitted(&state, &repairing).unwrap());
    state.jobs.get_mut(&builder.id).unwrap().status = Status::Completed;
    let rechecking = prepare_retry(
        &mut state,
        "main",
        Flow::Complete,
        Role::Orchestrator,
        &reviewer.id,
        Some(vec![builder.id.clone()]),
    )
    .unwrap();
    assert_eq!(rechecking.recovery_attempts, 0);
    assert!(admitted(&state, &rechecking).unwrap());
    assert_eq!(state.jobs.len(), 2);
}

#[test]
fn invalid_retry_dependencies_leave_the_checkpoint_unchanged() {
    let (_fixture, hub) = hub();
    let mut builder = job(&hub, Role::Builder, "backend");
    builder.status = Status::Completed;
    let mut waiting = job(&hub, Role::Reviewer, "backend");
    waiting.dependencies = vec![builder.id.clone()];
    let mut stranger = waiting.clone();
    stranger.id = "stranger".into();
    stranger.parent_id = "another-parent".into();
    let mut state = hub.manifest.lock().unwrap();
    state.jobs.insert(builder.id.clone(), builder.clone());
    state.jobs.insert(waiting.id.clone(), waiting.clone());
    state.jobs.insert(stranger.id.clone(), stranger.clone());
    let before = serde_json::to_value(&*state).unwrap();
    for dependency in [
        builder.id.as_str(),
        waiting.id.as_str(),
        stranger.id.as_str(),
        "missing",
    ] {
        assert!(prepare_retry(
            &mut state,
            "main",
            Flow::Planned,
            Role::Planner,
            &builder.id,
            Some(vec![dependency.to_owned()])
        )
        .is_err());
        assert_eq!(serde_json::to_value(&*state).unwrap(), before);
    }
}

#[tokio::test]
async fn failure_retry_preserves_uncertain_effects_until_a_specific_inspection_is_resolved() {
    let (_fixture, hub) = hub();
    let mut worker = job(&hub, Role::Builder, "backend");
    worker.status = Status::Interrupted;
    worker.recovery = Some(RecoveryCheckpoint::new(vec!["write".into()]));
    worker.recovery.as_mut().unwrap().inspected = true;
    let mutation = ToolCall {
        id: "uncertain-write".into(),
        name: "write".into(),
        args: json!({"path":"backend/task.ts","content":"changed"}),
        status: "pending".into(),
        output: String::new(),
        duration_ms: 0,
    };
    std::fs::create_dir_all(hub.root.root.join("backend")).unwrap();
    std::fs::write(hub.root.root.join("backend/task.ts"), "changed").unwrap();
    let (session, _) = storage::worker(&hub, &worker, None).unwrap();
    session.update(true, |data| {
        let turn = data.turns.last_mut().unwrap();
        turn.turn.steps.push(Step { tools: vec![mutation.clone()], ..Step::default() });
        turn.wire.push(json!({"type":"function_call", "call_id":mutation.id, "name":mutation.name, "arguments":mutation.args.to_string()}));
    }).unwrap();
    finish(
        &session,
        Err(AgentError::new("interrupted", "Runtime stopped")),
    );
    drop(session);
    hub.mutate(|state| {
        state.jobs.insert(worker.id.clone(), worker.clone());
        prepare_retry(
            state,
            "main",
            Flow::Planned,
            Role::Planner,
            &worker.id,
            None,
        )
    })
    .unwrap();
    let retry = hub.job(&worker.id).unwrap();
    let (session, _) =
        storage::worker(&hub, &retry, Some("Resume from saved effects".into())).unwrap();
    hub.live
        .lock()
        .unwrap()
        .insert(worker.id.clone(), session.clone());
    let exec = Execution {
        hub: hub.clone(),
        id: worker.id.clone(),
        role: Role::Builder,
        flow: Flow::Planned,
        scope: worker.scope.clone(),
    };
    assert!(exec.context().unwrap().contains("uncertain-write"));
    assert_eq!(
        exec.recovery_preflight(&mutation, false).unwrap_err().code,
        "recovery_inspection_required"
    );
    let read = ToolCall {
        id: "inspect-write".into(),
        name: "read".into(),
        args: json!({"path":"backend/task.ts"}),
        output: "changed".into(),
        ..mutation.clone()
    };
    exec.observe_recovery_result(&read, false, false, |_| None)
        .unwrap();
    let resolve = json!({"callId":mutation.id,"evidenceCallId":read.id,"outcome":"applied"});
    assert!(exec.resolve_recovery(&resolve).is_err());
    session
        .update(true, |data| {
            data.turns.last_mut().unwrap().wire.push(
                json!({"type":"function_call_output","call_id":read.id,"output":read.output}),
            );
        })
        .unwrap();
    exec.observe_recovery_result(&read, false, true, |_| None)
        .unwrap();
    assert_eq!(
        exec.recovery_preflight(&mutation, false).unwrap_err().code,
        "recovery_inspection_required"
    );
    exec.resolve_recovery(&resolve).unwrap();
    assert_eq!(
        exec.recovery_preflight(&mutation, false).unwrap_err().code,
        "recovery_already_applied"
    );
    let independent = ToolCall {
        args: json!({"path":"backend/other.ts","content":"independent"}),
        ..mutation.clone()
    };
    assert!(exec.recovery_preflight(&independent, false).is_ok());
    hub.mutate(|state| {
        state.jobs.get_mut(&worker.id).unwrap().status = Status::Completed;
        let followup = prepare_retry(
            state,
            "main",
            Flow::Planned,
            Role::Planner,
            &worker.id,
            None,
        )?;
        assert!(followup.recovery.is_none());
        Ok(())
    })
    .unwrap();
}

#[test]
fn narrow_write_scope_allows_project_reads_but_rejects_out_of_scope_mutations() {
    let (_fixture, hub) = hub();
    let exec = Execution {
        hub,
        id: "worker".into(),
        role: Role::Builder,
        flow: Flow::Planned,
        scope: vec!["frontend".into()],
    };
    let call = ToolCall {
        id: "read-contract".into(),
        name: "read".into(),
        args: json!({"path":"backend/contracts.ts"}),
        status: "pending".into(),
        output: String::new(),
        duration_ms: 0,
    };
    assert!(exec.preflight(&call).is_none());
    assert!(exec
        .preflight(&ToolCall {
            name: "write".into(),
            ..call
        })
        .is_some());
}

#[test]
fn screenshot_exports_respect_the_assigned_write_scope_without_restricting_visual_inspection() {
    let (_fixture, hub) = hub();
    for role in [Role::Builder, Role::Designer] {
        let exec = Execution {
            hub: hub.clone(),
            id: "worker".into(),
            role,
            flow: Flow::Planned,
            scope: vec!["frontend".into()],
        };
        let call = |args| ToolCall {
            id: "capture".into(),
            name: "browser_screenshot".into(),
            args,
            status: "pending".into(),
            output: String::new(),
            duration_ms: 0,
        };
        assert!(exec.preflight(&call(json!({"id":"tab"}))).is_none());
        assert!(exec
            .preflight(&call(
                json!({"id":"tab","savePath":"frontend/assets/page.png"})
            ))
            .is_none());
        for path in ["backend/page.png", "../outside.png"] {
            assert_eq!(
                exec.preflight(&call(json!({"id":"tab","savePath":path}))),
                Some("O arquivo está fora do escopo atribuído ao agente.".into())
            );
        }
    }
}
