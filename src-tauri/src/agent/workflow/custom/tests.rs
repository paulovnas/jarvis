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
