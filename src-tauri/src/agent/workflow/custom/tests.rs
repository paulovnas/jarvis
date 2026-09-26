use super::*;

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
    for tool in ["write", "edit", "apply_patch", "bash", "workflow_check"] {
        assert!(allowed(agent, tool), "missing {tool}");
    }
    assert!(!allowed(agent, "hub_spawn"));
    let prompt = instructions(agent);
    assert!(prompt.contains("immutable role is Designer"));
    assert!(prompt.contains("embedded in a user-defined workflow"));
    assert!(prompt.contains("Implement only assigned visual scope"));
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
