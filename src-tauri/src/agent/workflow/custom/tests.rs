use super::*;

#[test]
fn native_canvas_instructions_match_the_available_tools_and_routing() {
    let (_fixture, hub) = super::super::tests::hub();
    for role in [
        Role::Planner,
        Role::Investigator,
        Role::Writer,
        Role::Orchestrator,
        Role::Designer,
        Role::Video,
        Role::Builder,
        Role::Reviewer,
        Role::Github,
    ] {
        let mut catalog = catalog::tests::example();
        for step in &mut catalog.flows[0].steps {
            step.agent_id = role.builtin_id().unwrap();
        }
        let definition = catalog.resolve(&catalog.flows[0].id).unwrap();
        let agent = definition.agents[0].clone();
        let mut job = super::super::tests::job(&hub, role, ".");
        job.custom_agent = Some(agent.clone());
        let id = job.id.clone();
        hub.manifest.lock().unwrap().jobs.insert(id.clone(), job);
        let exec = Execution {
            hub: hub.clone(),
            id,
            role,
            flow: Flow::Custom,
            scope: vec![".".into()],
        };
        let mut definitions = crate::agent::tools::definitions(exec.role_mode());
        exec.filter(&mut definitions);
        let exposed: Vec<_> = definitions
            .iter()
            .filter_map(|tool| tool["name"].as_str())
            .collect();
        let prompt = exec.instructions().unwrap();
        for available in ["ask_user", "hub_complete"] {
            assert!(exposed.contains(&available), "{role:?}: {available}");
            assert!(prompt.contains(available), "{role:?}: {available}");
        }
        assert_eq!(exposed.contains(&"design_brief"), role == Role::Designer);
        assert_eq!(prompt.contains("Use design_brief"), role == Role::Designer);
        for unavailable in [
            "hub_spawn",
            "hub_wait",
            "hub_retry",
            "hub_request_guidance",
            "hub_respond_guidance",
            "validation_publish",
        ] {
            assert!(!exposed.contains(&unavailable), "{role:?}: {unavailable}");
            assert!(!prompt.contains(unavailable), "{role:?}: {unavailable}");
        }
        assert!(prompt.contains("graph runtime owns routing"));
        assert!(
            prompt.contains("A Beads task is required only when the step explicitly assigns one")
        );
    }
}

#[test]
fn standalone_github_contract_uses_direct_questions_and_completion() {
    let (_fixture, hub) = super::super::tests::hub();
    let agent = catalog::Catalog::default()
        .resolve_agent("builtin:github")
        .unwrap();
    hub.manifest.lock().unwrap().custom_agent = Some(agent);
    let exec = Execution {
        hub,
        id: "main".into(),
        role: Role::Github,
        flow: Flow::Custom,
        scope: vec![".".into()],
    };
    let prompt = exec.instructions().unwrap();
    assert!(prompt.contains("Use ask_user only for a material unanswered user question"));
    assert!(prompt.contains("Respond directly to the user with actual outcomes"));
    for unavailable in [
        "hub_spawn",
        "hub_wait",
        "hub_complete",
        "hub_request_guidance",
        "validation_publish",
    ] {
        assert!(!prompt.contains(unavailable), "{unavailable}");
    }
    assert!(prompt.contains("never ask again for those actions"));
    assert!(!prompt.contains("Finish the assigned step normally"));
}

#[test]
fn standalone_native_agents_keep_the_configured_secondary_and_resumed_effective_model() {
    for (id, profile_key) in [
        ("builtin:github", "publication/github"),
        ("builtin:video", "video/video"),
    ] {
        let fixture = crate::agent::tests::Fixture::new();
        let state = AppState::default();
        state
            .with_connection(&fixture.root, |_| Ok::<_, AgentError>(()))
            .unwrap();
        let choice: settings::ModelChoice = serde_json::from_value(json!({
            "executor":"claude", "account":"", "model":"sonnet", "reasoning":null,
            "fallback":{"executor":"claude", "account":"", "model":"opus", "reasoning":null}
        }))
        .unwrap();
        std::fs::write(
            crate::data_dir::root(&fixture.root).join("agents.json"),
            json!({(profile_key):choice}).to_string(),
        )
        .unwrap();
        let mut options = crate::agent::tests::options(ApprovalMode::Yolo);
        options.workflow = Some(Flow::Custom);
        options.custom_agent_id = Some(id.into());
        choice.apply(&mut options);
        let agent = resolve_agent(
            &state,
            &OpenAiCodexState::default(),
            &fixture.root,
            &options,
            "direct-chat",
            false,
        )
        .unwrap();
        assert_eq!(agent.model.as_ref(), Some(&choice));
        choice.fallback.as_ref().unwrap().apply(&mut options);
        let resumed = resolve_agent(
            &state,
            &OpenAiCodexState::default(),
            &fixture.root,
            &options,
            "direct-chat",
            true,
        )
        .unwrap();
        assert_eq!(resumed.model.as_ref(), Some(&choice));
        assert_eq!(options.model, "opus");
    }
}

#[tokio::test]
async fn canvas_cursor_survives_failure_and_rework_without_repeating_confirmed_steps() {
    for fail_at in [1, 2] {
        let (_fixture, hub) = super::super::tests::hub();
        let definition = definition();
        let (_sender, signal) = watch::channel(false);
        let mut visited = vec![];
        let failure = walk_from(
            &definition,
            signal,
            Cursor::start(&definition),
            |step, _, index| {
                visited.push(step.id);
                async move {
                    if index == fail_at {
                        return Err(invalid("Provider unavailable"));
                    }
                    Ok(handoff(if index == 1 {
                        Verdict::Rework
                    } else {
                        Verdict::Completed
                    }))
                }
            },
            |cursor| {
                hub.mutate(|state| {
                    state.custom_cursor = Some(cursor.clone());
                    Ok(())
                })
            },
        )
        .await;
        assert!(failure.is_err());
        let saved = storage::load(&hub.directory, &hub.root.id)
            .unwrap()
            .unwrap();
        let cursor = saved.custom_cursor.unwrap();
        assert_eq!(cursor.results.len(), fail_at);
        assert_eq!(cursor.visited.len(), fail_at);
        let (_sender, signal) = watch::channel(false);
        let mut resumed = vec![];
        let result = walk_from(
            &definition,
            signal,
            cursor,
            |step, previous, index| {
                resumed.push(step.id);
                assert_eq!(previous.len(), index);
                async { Ok(handoff(Verdict::Completed)) }
            },
            |_| Ok(()),
        )
        .await
        .unwrap();
        assert_eq!(resumed[0], visited[fail_at]);
        assert_eq!(result.len(), if fail_at == 1 { 2 } else { 4 });
        assert_eq!(resumed.len(), if fail_at == 1 { 1 } else { 2 });
    }
}

#[test]
fn failed_canvas_worker_reopens_the_same_turn_and_preserves_tool_receipts() {
    let (_fixture, hub) = super::super::tests::hub();
    let definition = definition();
    hub.mutate(|state| {
        state.flow = Flow::Custom;
        state.options.workflow = Some(Flow::Custom);
        state.custom_definition = Some(definition.clone());
        state.custom_cursor = Some(Cursor::start(&definition));
        Ok(())
    })
    .unwrap();
    let mut worker = prepare(&hub, &definition, &definition.flow.steps[0], &[], 0).unwrap();
    let (session, _) = storage::worker(&hub, &worker, None).unwrap();
    let original = session.snapshot().unwrap().turns[0].id.clone();
    session
        .update(true, |data| {
            data.turns.last_mut().unwrap().wire.push(
                json!({"type":"function_call_output","call_id":"saved","output":"Verified result"}),
            );
        })
        .unwrap();
    super::super::super::finish(&session, Err(AgentError::internal()));
    drop(session);
    worker.status = Status::Failed;
    hub.mutate(|state| {
        state.root_status = Status::Failed;
        state.jobs.insert(worker.id.clone(), worker.clone());
        state.custom_cursor.as_mut().unwrap().active = Some(worker.id.clone());
        Ok(())
    })
    .unwrap();
    let saved = storage::load(&hub.directory, &hub.root.id)
        .unwrap()
        .unwrap();
    let (recovered, workers) =
        storage::prepare_recovery(&hub.directory, saved, Flow::Custom, "run", vec![]).unwrap();
    assert_eq!(workers.len(), 1);
    *hub.manifest.lock().unwrap() = recovered;
    let resumed = prepare(&hub, &definition, &definition.flow.steps[0], &[], 0).unwrap();
    assert_eq!(resumed.id, worker.id);
    let (session, _) = storage::resume_worker(&hub, &resumed).unwrap();
    let data = session.data.lock().unwrap();
    assert_eq!(data.turns.len(), 1);
    assert_eq!(data.turns[0].turn.id, original);
    assert_eq!(
        data.turns[0]
            .wire
            .iter()
            .filter(|item| item["call_id"] == "saved")
            .count(),
        1
    );
}

#[tokio::test]
async fn accepted_canvas_handoff_is_reused_after_crash_before_cursor_advance() {
    let (_fixture, hub) = super::super::tests::hub();
    let definition = definition();
    let mut worker = prepare(&hub, &definition, &definition.flow.steps[0], &[], 0).unwrap();
    // The handoff transaction committed, but the worker never persisted its
    // final status (nor did the canvas advance its cursor).
    worker.status = Status::Running;
    worker.handoff = Some(handoff(Verdict::Completed));
    hub.mutate(|state| {
        let mut cursor = Cursor::start(&definition);
        cursor.active = Some(worker.id.clone());
        state.flow = Flow::Custom;
        state.root_status = Status::Failed;
        state.custom_cursor = Some(cursor);
        state.jobs.insert(worker.id.clone(), worker.clone());
        Ok(())
    })
    .unwrap();
    let saved = storage::load(&hub.directory, &hub.root.id)
        .unwrap()
        .unwrap();
    let (recovered, workers) =
        storage::prepare_recovery(&hub.directory, saved, Flow::Custom, "run", vec![]).unwrap();
    assert!(workers.is_empty());
    *hub.manifest.lock().unwrap() = recovered;
    let recovered = prepare(&hub, &definition, &definition.flow.steps[0], &[], 0).unwrap();
    assert_eq!(recovered.attempts, 1);
    let result = execute_step(hub.clone(), recovered).await.unwrap();
    assert_eq!(result.summary, "Evidence-backed outcome");
    assert!(hub.live.lock().unwrap().is_empty());
    assert!(!hub.directory.join(format!("{}.jsonl", worker.id)).exists());
}

fn definition() -> RunDefinition {
    let c = catalog::tests::example();
    c.resolve(&c.flows[0].id).unwrap()
}
fn handoff(verdict: Verdict) -> Handoff {
    Handoff {
        verdict,
        summary: "Evidence-backed outcome".into(),
        outcomes: vec!["Done".into()],
        evidence: vec!["src/file.rs".into()],
        validation: vec![],
        limitations: vec![],
        task_ids: vec![],
    }
}

#[tokio::test]
async fn follows_entry_success_and_rework_connections_and_passes_previous_results() {
    let definition = definition();
    let (_sender, signal) = watch::channel(false);
    let mut visited = vec![];
    let result = walk(&definition, signal, |step, previous, index| {
        visited.push(step.id.clone());
        assert_eq!(previous.len(), index);
        let verdict = if index == 1 {
            Verdict::Rework
        } else {
            Verdict::Completed
        };
        async move { Ok(handoff(verdict)) }
    })
    .await
    .unwrap();
    assert_eq!(
        visited,
        vec![
            definition.flow.steps[0].id.clone(),
            definition.flow.steps[1].id.clone(),
            definition.flow.steps[0].id.clone(),
            definition.flow.steps[1].id.clone()
        ]
    );
    assert_eq!(result.len(), 4);
}

#[tokio::test]
async fn stops_on_blockers_missing_correction_routes_errors_limits_and_cancellation() {
    for verdict in [Verdict::Blocked, Verdict::Rework] {
        let (_sender, signal) = watch::channel(false);
        assert!(walk(&definition(), signal, |_, _, _| {
            let v = verdict.clone();
            async { Ok(handoff(v)) }
        })
        .await
        .is_err());
    }
    let (_sender, signal) = watch::channel(false);
    let limited = walk(&definition(), signal, |_, _, index| async move {
        Ok(handoff(if index % 2 == 1 {
            Verdict::Rework
        } else {
            Verdict::Completed
        }))
    })
    .await
    .unwrap_err();
    assert!(limited.message.contains("limite"));
    let (_sender, signal) = watch::channel(false);
    assert!(walk(&definition(), signal, |_, _, _| async {
        Err(invalid("Provider failed"))
    })
    .await
    .unwrap_err()
    .message
    .contains("Provider failed"));
    let (sender, signal) = watch::channel(false);
    let mut called = false;
    let error = walk(&definition(), signal, |_, _, _| {
        called = true;
        sender.send_replace(true);
        std::future::pending::<Result<Handoff, AgentError>>()
    })
    .await
    .unwrap_err();
    assert!(called);
    assert_eq!(error.code, "cancelled");
}

#[test]
fn custom_capabilities_models_and_instructions_cannot_escape_configured_scope() {
    let mut agent = definition().agents[0].clone();
    for tool in [
        "write",
        "edit",
        "bash",
        "process_start",
        "terminal_start",
        "hub_spawn",
        "hub_retry",
        "mcp_server_tool",
        "workflow_check",
    ] {
        assert!(!allowed(&agent, tool), "{tool}");
    }
    for tool in ["read", "ls", "hub_complete", "ask_user"] {
        assert!(allowed(&agent, tool));
    }
    agent.capability = Capability::WriteFiles;
    assert!(allowed(&agent, "write"));
    assert!(!allowed(&agent, "bash"));
    agent.capability = Capability::Commands;
    assert!(allowed(&agent, "bash"));
    assert!(!allowed(&agent, "hub_spawn"));
    let (_fixture, hub) = super::super::tests::hub();
    let mut options = hub.manifest.lock().unwrap().options.clone();
    agent.model = Some(settings::ModelChoice {
        executor: crate::claude::Executor::Jarvis,
        account: "chosen".into(),
        model: "chosen-model".into(),
        reasoning: Some("high".into()),
        fallback: None,
    });
    apply_model(&mut options, &agent);
    assert_eq!(options.account, "chosen");
    assert_eq!(options.model, "chosen-model");
    assert!(instructions(&agent).contains(&agent.instructions));
    assert!(!instructions(&agent).contains("immutable role"));
}

#[test]
fn prepared_workers_freeze_config_and_inherit_conversation_approval_mode() {
    let (_fixture, hub) = super::super::tests::hub();
    hub.root.update(true, |data| {
        data.turns.last_mut().unwrap().wire[0] = json!({"role":"user","content":"Original task with explicitly selected skill instructions"});
    }).unwrap();
    let definition = definition();
    let job = prepare(
        &hub,
        &definition,
        &definition.flow.steps[0],
        &[handoff(Verdict::Completed)],
        1,
    )
    .unwrap();
    assert_eq!(job.role, Role::Custom);
    assert_eq!(job.options.mode, Mode::Plan);
    assert_eq!(job.options.approval_mode, ApprovalMode::Manual);
    assert!(job.prompt.contains("Evidence-backed outcome"));
    assert!(job
        .prompt
        .contains("explicitly selected skill instructions"));
    assert!(job.custom_agent.is_some());
    assert!(!job.writes());
}

#[test]
fn native_designer_steps_execute_with_the_fixed_design_contract_and_mutation_tools() {
    let (_fixture, hub) = super::super::tests::hub();
    let mut catalog = catalog::tests::example();
    for step in &mut catalog.flows[0].steps {
        step.agent_id = "builtin:designer".into();
    }
    let definition = catalog.resolve(&catalog.flows[0].id).unwrap();
    let job = prepare(&hub, &definition, &definition.flow.steps[0], &[], 0).unwrap();
    let agent = job.custom_agent.as_ref().unwrap();
    assert_eq!(job.role, Role::Designer);
    assert_eq!(job.options.mode, Mode::Build);
    assert!(job.writes());
    for tool in [
        "write",
        "edit",
        "apply_patch",
        "bash",
        "workflow_check",
        "design_brief",
    ] {
        assert!(allowed(agent, tool), "missing {tool}");
    }
    assert!(!allowed(agent, "hub_spawn"));
    let prompt = instructions(agent);
    assert!(prompt.contains("immutable role is Designer"));
    assert!(prompt.contains("embedded in a user-defined workflow"));
    assert!(prompt
        .contains("Deliver the requested frontend/design outcome within the authorized scope"));
}

fn designer_definition() -> RunDefinition {
    let mut catalog = catalog::tests::example();
    for step in &mut catalog.flows[0].steps {
        step.agent_id = "builtin:designer".into();
    }
    catalog.resolve(&catalog.flows[0].id).unwrap()
}

fn brief_tool(args: Value) -> ToolCall {
    ToolCall {
        id: "brief".into(),
        name: "design_brief".into(),
        args,
        status: "pending".into(),
        output: String::new(),
        duration_ms: 0,
    }
}

#[test]
fn designer_brief_permission_requires_the_exact_native_identity() {
    let mut agent = designer_definition().agents[0].clone();
    assert!(allowed(&agent, "design_brief"));
    agent.id = "builtin:builder".into();
    assert!(!allowed(&agent, "design_brief"));
    agent.id = "builtin:designer".into();
    agent.native_role = Some(Role::Builder);
    assert!(!allowed(&agent, "design_brief"));
    agent.native_role = None;
    agent.name = "Designer".into();
    for capability in [
        Capability::ReadOnly,
        Capability::WriteFiles,
        Capability::Commands,
    ] {
        agent.capability = capability;
        assert!(!allowed(&agent, "design_brief"));
    }
}

#[tokio::test]
async fn native_canvas_design_briefs_survive_compaction_restart_and_rework_per_node() {
    let (_fixture, hub) = super::super::tests::hub();
    let definition = designer_definition();
    hub.mutate(|state| {
        state.flow = Flow::Custom;
        state.options.workflow = Some(Flow::Custom);
        state.custom_definition = Some(definition.clone());
        state
            .design_briefs
            .insert("main".into(), "Previous direct direction".into());
        Ok(())
    })
    .unwrap();
    let first = prepare(&hub, &definition, &definition.flow.steps[0], &[], 0).unwrap();
    let second = prepare(&hub, &definition, &definition.flow.steps[1], &[], 1).unwrap();
    assert_ne!(first.id, second.id);
    hub.mutate(|state| {
        state.jobs.insert(first.id.clone(), first.clone());
        state.jobs.insert(second.id.clone(), second.clone());
        Ok(())
    })
    .unwrap();
    let execution = |job: &Job| Execution {
        hub: hub.clone(),
        id: job.id.clone(),
        role: Role::Designer,
        flow: Flow::Custom,
        scope: vec![".".into()],
    };
    let first_exec = execution(&first);
    let second_exec = execution(&second);
    assert!(first_exec.design_inputs().unwrap().1.is_empty());
    for (exec, brief) in [
        (
            &first_exec,
            "Accepted: graphite; system=agentic; mobile states required",
        ),
        (
            &second_exec,
            "Accepted: restrained motion; keyboard review required",
        ),
    ] {
        exec.execute(&brief_tool(json!({"text":brief})), hub.root_signal.clone())
            .await
            .unwrap();
        let result: Value = serde_json::from_str(
            &exec
                .execute(&brief_tool(json!({})), hub.root_signal.clone())
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(result["brief"], brief);
        assert_eq!(exec.design_inputs().unwrap().1, brief);
        assert!(exec.context().unwrap().contains(brief));
        assert!(!exec.instructions().unwrap().contains(brief));
    }
    assert!(!first_exec
        .context()
        .unwrap()
        .contains("Accepted: restrained motion"));
    assert!(!second_exec
        .context()
        .unwrap()
        .contains("Accepted: graphite"));
    let (session, _) = storage::worker(&hub, &first, None).unwrap();
    session.update(true, |data| {
        data.turns.last_mut().unwrap().wire.extend([
            json!({"type":"function_call","call_id":"reference","name":"design_read","arguments":"{}"}),
            json!({"type":"function_call_output","call_id":"reference","output":"Relevant component reference. ".repeat(1000)}),
        ]);
    }).unwrap();
    assert!(crate::agent::compaction::ensure_with(
        &session,
        0,
        true,
        hub.root_signal.clone(),
        |_| async { Ok("Continue the assigned canvas step from confirmed work.".into()) }
    )
    .await
    .unwrap());
    assert!(first_exec.context().unwrap().contains("Accepted: graphite"));
    super::super::super::finish(&session, Ok(()));
    drop(session);
    let saved = storage::load(&hub.directory, &hub.root.id)
        .unwrap()
        .unwrap();
    *hub.manifest.lock().unwrap() = saved;
    assert!(first_exec.context().unwrap().contains("Accepted: graphite"));
    assert!(!second_exec
        .context()
        .unwrap()
        .contains("Accepted: graphite"));
    hub.mutate(|state| {
        let job = state.jobs.get_mut(&first.id).unwrap();
        job.status = Status::Completed;
        job.handoff = Some(handoff(Verdict::Completed));
        Ok(())
    })
    .unwrap();
    let reworked = prepare(&hub, &definition, &definition.flow.steps[0], &[], 2).unwrap();
    assert_eq!(reworked.id, first.id);
    assert_eq!(
        execution(&reworked).design_inputs().unwrap().1,
        first_exec.design_inputs().unwrap().1
    );
    hub.mutate(|state| {
        state.run_id = "next-run".into();
        Ok(())
    })
    .unwrap();
    let next_run = prepare(&hub, &definition, &definition.flow.steps[0], &[], 0).unwrap();
    assert_ne!(next_run.id, first.id);
    hub.mutate(|state| {
        state.jobs.insert(next_run.id.clone(), next_run.clone());
        Ok(())
    })
    .unwrap();
    let next_exec = execution(&next_run);
    assert!(next_exec.design_inputs().unwrap().1.is_empty());
    let result: Value = serde_json::from_str(
        &next_exec
            .execute(&brief_tool(json!({})), hub.root_signal.clone())
            .await
            .unwrap(),
    )
    .unwrap();
    assert!(result["brief"].is_null());
    assert!(first_exec
        .execute(
            &brief_tool(json!({"text":"Stale overwrite"})),
            hub.root_signal.clone()
        )
        .await
        .is_err());
}

#[tokio::test]
async fn canvas_design_brief_rejects_foreign_targets_invalid_identity_and_oversized_text() {
    let (_fixture, hub) = super::super::tests::hub();
    let definition = designer_definition();
    let job = prepare(&hub, &definition, &definition.flow.steps[0], &[], 0).unwrap();
    hub.mutate(|state| {
        state.flow = Flow::Custom;
        state.jobs.insert(job.id.clone(), job.clone());
        Ok(())
    })
    .unwrap();
    let exec = Execution {
        hub: hub.clone(),
        id: job.id.clone(),
        role: Role::Designer,
        flow: Flow::Custom,
        scope: vec![".".into()],
    };
    let accepted = "界".repeat(4000);
    exec.execute(
        &brief_tool(json!({"text":accepted})),
        hub.root_signal.clone(),
    )
    .await
    .unwrap();
    for args in [
        json!({"text":"界".repeat(4001)}),
        json!({"text":"Overwrite", "id":"main"}),
    ] {
        assert!(exec
            .execute(&brief_tool(args), hub.root_signal.clone())
            .await
            .is_err());
        assert_eq!(exec.design_inputs().unwrap().1, accepted);
    }
    hub.mutate(|state| {
        state
            .jobs
            .get_mut(&job.id)
            .unwrap()
            .custom_agent
            .as_mut()
            .unwrap()
            .id = "builtin:builder".into();
        Ok(())
    })
    .unwrap();
    assert!(!exec.allowed("design_brief"));
    assert!(exec
        .execute(&brief_tool(json!({})), hub.root_signal.clone())
        .await
        .is_err());
    assert_eq!(
        hub.manifest.lock().unwrap().design_briefs[&job.id],
        accepted
    );
}

#[test]
fn harness_evaluation_canvas_rework_reuses_the_step_history_and_exact_review_evidence() {
    let (_fixture, hub) = super::super::tests::hub();
    let definition = definition();
    let step = &definition.flow.steps[0];
    let mut first = prepare(&hub, &definition, step, &[], 0).unwrap();
    let (session, _) = storage::worker(&hub, &first, None).unwrap();
    session.update(true, |data| {
        data.turns.last_mut().unwrap().wire.push(json!({"role":"assistant","content":"Implemented the parser in src/parser.rs; verified existing cases."}));
    }).unwrap();
    super::super::super::finish(&session, Ok(()));
    drop(session);
    first.status = Status::Completed;
    first.handoff = Some(handoff(Verdict::Completed));
    hub.mutate(|state| {
        state.jobs.insert(first.id.clone(), first.clone());
        Ok(())
    })
    .unwrap();
    let mut review = handoff(Verdict::Rework);
    review.evidence = vec!["src/parser.rs:42 missing empty-input case".into()];
    review.validation = vec!["existing parser tests passed; add the empty-input regression".into()];
    let followup = prepare(&hub, &definition, step, &[review], 2).unwrap();
    assert_eq!(followup.id, first.id);
    assert_eq!(followup.attempts, 2);
    assert_eq!(followup.status, Status::Queued);
    assert!(followup.handoff.is_none());
    assert!(followup.prompt.contains("src/parser.rs:42"));
    assert!(followup.prompt.contains("add the empty-input regression"));
    let (session, _) = storage::worker(&hub, &followup, None).unwrap();
    {
        let data = session.data.lock().unwrap();
        assert_eq!(data.turns.len(), 2);
        assert!(data.turns[0].wire.iter().any(|item| item["content"]
            .as_str()
            .is_some_and(|text| text.contains("Implemented the parser"))));
        assert!(data.turns[1].wire[0]["content"]
            .as_str()
            .unwrap()
            .contains("src/parser.rs:42"));
    }
    super::super::super::finish(&session, Ok(()));
}

#[test]
fn canvas_context_reuse_is_scoped_to_the_node_and_run_and_never_replays_failure() {
    let (_fixture, hub) = super::super::tests::hub();
    let definition = definition();
    let step = &definition.flow.steps[0];
    let mut first = prepare(&hub, &definition, step, &[], 0).unwrap();
    first.status = Status::Blocked;
    first.handoff = Some(handoff(Verdict::Rework));
    hub.mutate(|state| {
        state.jobs.insert(first.id.clone(), first.clone());
        Ok(())
    })
    .unwrap();
    assert_eq!(
        prepare(&hub, &definition, step, &[], 2).unwrap().id,
        first.id
    );
    let mut other_step = step.clone();
    other_step.id = "different-node-same-agent".into();
    assert_ne!(
        prepare(&hub, &definition, &other_step, &[], 1).unwrap().id,
        first.id
    );
    for status in [
        Status::Running,
        Status::Failed,
        Status::Cancelled,
        Status::Interrupted,
    ] {
        hub.mutate(|state| {
            state.jobs.get_mut(&first.id).unwrap().status = status;
            Ok(())
        })
        .unwrap();
        assert!(prepare(&hub, &definition, step, &[], 2).is_err());
    }
    hub.mutate(|state| {
        state.run_id = "new-run".into();
        Ok(())
    })
    .unwrap();
    assert_ne!(
        prepare(&hub, &definition, step, &[], 0).unwrap().id,
        first.id
    );
    let mut legacy = json!(first);
    legacy.as_object_mut().unwrap().remove("customStepId");
    assert!(serde_json::from_value::<Job>(legacy)
        .unwrap()
        .custom_step_id
        .is_none());
}

#[test]
fn fresh_canvas_step_receives_full_immediate_handoff_including_checks_and_task_ids() {
    let (_fixture, hub) = super::super::tests::hub();
    let definition = definition();
    let mut previous = handoff(Verdict::Completed);
    previous.evidence = vec!["src/service.ts:82".into()];
    previous.validation = vec!["service.test.ts: 9 tests passed".into()];
    previous.task_ids = vec!["project-backend".into()];
    previous.limitations = vec![format!("{} REQUIRED_FOLLOWUP", "detail ".repeat(80))];
    let next = prepare(&hub, &definition, &definition.flow.steps[1], &[previous], 1).unwrap();
    for expected in [
        "src/service.ts:82",
        "9 tests passed",
        "project-backend",
        "REQUIRED_FOLLOWUP",
    ] {
        assert!(next.prompt.contains(expected), "Missing {expected}");
    }
}
