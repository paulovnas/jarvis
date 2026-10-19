use super::*;

#[test]
fn custom_flow_nodes_generate_inline_with_their_configured_permissions() {
    for (capability, mcp_allowed) in [
        (Capability::ReadOnly, false),
        (Capability::WriteFiles, false),
        (Capability::Commands, true),
    ] {
        let (_fixture, hub) = super::super::tests::hub();
        let mut agent = catalog::tests::example().agents[0].clone();
        agent.capability = capability;
        agent.instructions =
            "Call generate_image and delegate to the managed image generator.".into();
        let mut job = super::super::tests::job(&hub, Role::Custom, ".");
        job.custom_step_id = Some("image-stage".into());
        job.custom_agent = Some(agent);
        hub.manifest
            .lock()
            .unwrap()
            .jobs
            .insert(job.id.clone(), job.clone());
        let exec = Execution {
            hub: hub.clone(),
            id: job.id,
            role: Role::Custom,
            flow: Flow::Custom,
            scope: job.scope,
        };
        let mut definitions = vec![
            image::definition(false),
            json!({"name":"mcp_comfy_generate"}),
        ];
        definitions.extend(crate::agent::image_tasks::definitions());
        exec.filter(&mut definitions);
        let image_allowed = capability != Capability::ReadOnly;
        for name in ["generate_image", "image_process"] {
            assert_eq!(
                definitions.iter().any(|tool| tool["name"] == name),
                image_allowed
            );
        }
        assert!(exec.local_image_generation());
        assert!(!exec.image_generator());
        assert_eq!(
            exec.preflight(&ToolCall {
                id: "image".into(),
                name: "generate_image".into(),
                args: json!({"prompt":"Generate a sprite"}),
                status: "pending".into(),
                output: String::new(),
                duration_ms: 0,
            })
            .is_none(),
            image_allowed
        );
        assert!(exec
            .instructions()
            .unwrap()
            .contains("within this configured step"));
        assert_eq!(exec.allowed("mcp_comfy_generate"), mcp_allowed);
        let before = hub.manifest.lock().unwrap().jobs.len();
        assert!(dispatch::image_job(&exec, &json!({"prompt":"Generate a sprite"})).is_err());
        assert_eq!(hub.manifest.lock().unwrap().jobs.len(), before);
    }
}

#[test]
fn custom_canvas_image_permissions_and_export_scope_are_enforced() {
    let (_fixture, hub) = super::super::tests::hub();
    let mut agent = catalog::tests::example().agents[0].clone();
    agent.capability = Capability::WriteFiles;
    let mut job = super::super::tests::job(&hub, Role::Custom, "assets");
    job.custom_step_id = Some("image-stage".into());
    job.custom_agent = Some(agent);
    hub.manifest
        .lock()
        .unwrap()
        .jobs
        .insert(job.id.clone(), job.clone());
    let exec = Execution {
        hub: hub.clone(),
        id: job.id.clone(),
        role: job.role,
        flow: Flow::Custom,
        scope: job.scope,
    };
    for name in ["generate_image", "image_process"] {
        let mut tool = ToolCall {
            id: name.into(),
            name: name.into(),
            args: json!({"processing":{"output_directory":"outside"}}),
            status: "pending".into(),
            output: String::new(),
            duration_ms: 0,
        };
        assert!(exec.preflight(&tool).unwrap().contains("escopo"));
        tool.args["processing"]["output_directory"] = json!("assets/sprites");
        assert!(exec.preflight(&tool).is_none());
        hub.manifest
            .lock()
            .unwrap()
            .jobs
            .get_mut(&job.id)
            .unwrap()
            .custom_agent
            .as_mut()
            .unwrap()
            .denied_tools
            .push(name.into());
        assert!(!exec.allowed(name));
        assert!(exec.preflight(&tool).is_some());
    }
}

#[test]
fn custom_flow_image_node_keeps_its_native_pipeline_without_another_worker() {
    let (_fixture, hub) = super::super::tests::hub();
    let agent = catalog::Catalog::default()
        .resolve_agent("builtin:image_generator")
        .unwrap();
    let mut job = super::super::tests::job(&hub, Role::ImageGenerator, ".jarvis-image-attachments");
    job.custom_agent = Some(agent);
    hub.manifest
        .lock()
        .unwrap()
        .jobs
        .insert(job.id.clone(), job.clone());
    let exec = Execution {
        hub: hub.clone(),
        id: job.id,
        role: job.role,
        flow: Flow::Custom,
        scope: job.scope,
    };
    let mut definitions = vec![image::definition(true)];
    definitions.extend(crate::agent::image_tasks::definitions());
    exec.filter(&mut definitions);
    for name in ["generate_image", "image_process"] {
        assert!(definitions.iter().any(|tool| tool["name"] == name));
    }
    assert!(exec.image_generator());
    let before = hub.manifest.lock().unwrap().jobs.len();
    assert!(dispatch::image_job(&exec, &json!({"prompt":"Generate a sprite"})).is_err());
    assert_eq!(hub.manifest.lock().unwrap().jobs.len(), before);
}

#[test]
fn standalone_custom_agents_can_still_delegate_images() {
    let (_fixture, hub) = super::super::tests::hub();
    let mut agent = catalog::tests::example().agents[0].clone();
    agent.capability = Capability::Commands;
    hub.manifest.lock().unwrap().custom_agent = Some(agent);
    let exec = Execution {
        hub: hub.clone(),
        id: "main".into(),
        role: Role::Custom,
        flow: Flow::Custom,
        scope: vec![".".into()],
    };
    assert!(exec.allowed("generate_image"));
    let (child, _) = dispatch::image_job(&exec, &json!({"prompt":"Generate a sprite"})).unwrap();
    assert_eq!(child.role, Role::ImageGenerator);
    assert_eq!(child.options.workflow, Some(Flow::ImageGenerator));
}

#[test]
fn custom_stages_report_delegated_activity_and_pause_only_while_workers_wait() {
    use crate::agent::turn_state::TurnPhase;
    let (_fixture, hub) = super::super::tests::hub();
    let definition = adaptive_definition();
    hub.mutate(|state| {
        state.flow = Flow::Custom;
        state.custom_definition = Some(definition.clone());
        state.custom_cursor = Some(Cursor::start(&definition));
        Ok(())
    })
    .unwrap();
    let wire = hub.root.data.lock().unwrap().turns[0].wire.clone();
    let mut planner = super::super::tests::job(&hub, Role::Custom, ".");
    planner.custom_agent = Some(definition.agents[0].clone());
    planner.custom_agent.as_mut().unwrap().name = "RPG/Planejador".into();
    activate_step(&hub, &planner).unwrap();
    let (session, _) = storage::worker(&hub, &planner, None).unwrap();
    hub.live
        .lock()
        .unwrap()
        .insert(planner.id.clone(), session.clone());
    assert!(hub.root.snapshot().unwrap().turns[0].active_since.is_none());
    session.transition(TurnPhase::Sampling).unwrap();
    let root = hub.root.snapshot().unwrap();
    assert_eq!(
        root.turns[0].steps[0].text,
        "Etapa 1 · RPG/Planejador em execução."
    );
    assert!(root.turns[0].steps[0].summary.is_empty());
    assert!(root.turns[0].active_since.is_some());
    std::thread::sleep(Duration::from_millis(15));
    session.transition(TurnPhase::WaitingForApproval).unwrap();
    let paused = hub.root.snapshot().unwrap().turns[0].duration_ms;
    assert!(paused > 0);
    assert!(hub.root.snapshot().unwrap().turns[0].active_since.is_none());
    std::thread::sleep(Duration::from_millis(15));
    assert_eq!(hub.root.snapshot().unwrap().turns[0].duration_ms, paused);
    session.transition(TurnPhase::ExecutingTools).unwrap();
    assert!(hub.root.snapshot().unwrap().turns[0].active_since.is_some());
    crate::agent::finish(&session, Err(AgentError::cancelled()));
    assert!(hub.root.snapshot().unwrap().turns[0].active_since.is_none());
    hub.mutate(|state| {
        state
            .custom_cursor
            .as_mut()
            .unwrap()
            .results
            .push(handoff(Verdict::Completed));
        Ok(())
    })
    .unwrap();
    let mut artist = planner.clone();
    artist.id = library::new_id().unwrap();
    artist.custom_agent.as_mut().unwrap().name = "RPG/Artista Técnico 3D".into();
    activate_step(&hub, &artist).unwrap();
    activate_step(&hub, &artist).unwrap();
    let (session, _) = storage::worker(&hub, &artist, None).unwrap();
    hub.live
        .lock()
        .unwrap()
        .insert(artist.id.clone(), session.clone());
    session.transition(TurnPhase::Sampling).unwrap();
    let root = hub.root.snapshot().unwrap();
    assert_eq!(root.turns[0].steps.len(), 2);
    assert_eq!(
        root.turns[0].steps[1].text,
        "Etapa 2 · RPG/Artista Técnico 3D em execução."
    );
    assert!(root.turns[0].active_since.is_some());
    assert!(root.turns[0].duration_ms >= paused);
    assert_eq!(hub.root.data.lock().unwrap().turns[0].wire, wire);
    let stored = crate::agent::journal::load_all(&hub.root.journal)
        .unwrap()
        .0;
    assert_eq!(stored[0].turn.steps.len(), 2);
}

#[test]
fn marketplace_inspection_is_read_only_but_installation_keeps_command_capability() {
    assert!(capability_allows(
        Capability::ReadOnly,
        "jarvis_skill_marketplace"
    ));
    assert!(!capability_allows(
        Capability::ReadOnly,
        "jarvis_propose_skills"
    ));
    assert!(!capability_allows(
        Capability::WriteFiles,
        "jarvis_propose_skills"
    ));
    assert!(capability_allows(
        Capability::Commands,
        "jarvis_propose_skills"
    ));
}

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
    assert!(prompt.contains("For an authorized non-Git follow-up, use relevant MCPs"));
    assert!(prompt.contains("discover the required capability before reporting unavailable tools"));
    assert!(prompt.contains("Do not mutate Git/GitHub through shell, terminals, processes or MCPs"));
    assert!(!prompt.contains("Finish the assigned step normally"));
}

#[tokio::test]
async fn standalone_github_exposes_enabled_mcp_selection_and_keeps_discovered_schemas_callable() {
    let (_fixture, hub) = super::super::tests::hub();
    hub.manifest.lock().unwrap().custom_agent = Some(
        catalog::Catalog::default()
            .resolve_agent("builtin:github")
            .unwrap(),
    );
    // Catalog discovery needs public registration metadata only; this fixture
    // never starts an MCP process or accesses a real database or credentials.
    hub.env.state.with_connection(&hub.env.home, |db| {
        db.execute("INSERT INTO mcp_servers (id,name,kind,enabled,configured,revision) VALUES ('database-id','database','local',1,1,1)", [])?;
        Ok::<_, crate::mcp::McpError>(())
    }).unwrap();
    let exec = Execution {
        hub,
        id: "main".into(),
        role: Role::Github,
        flow: Flow::Custom,
        scope: vec![".".into()],
    };
    let (_sender, signal) = watch::channel(false);
    let mut clients = crate::mcp::runtime::TurnClients::discover_for_intent(
        &exec.hub.env.mcp,
        &exec.hub.env.state,
        &exec.hub.env.home,
        &exec.hub.root.root,
        &crate::mcp::McpIntent::default(),
        signal,
    )
    .await
    .unwrap();
    let mut definitions = clients
        .definitions_with(
            &exec.hub.env.mcp,
            &exec.hub.env.state,
            &exec.hub.env.home,
            false,
            |name| exec.allowed(name),
        )
        .await;
    exec.filter(&mut definitions);
    let selector = definitions
        .iter()
        .find(|tool| tool["name"] == "mcp_activate")
        .unwrap();
    assert_eq!(
        selector["parameters"]["properties"]["server"]["enum"],
        json!(["database"])
    );
    let call = ToolCall {
        id: "mcp-selection".into(),
        name: "mcp_activate".into(),
        args: json!({"server":"database"}),
        status: "pending".into(),
        output: String::new(),
        duration_ms: 0,
    };
    assert!(crate::agent::tool_contract::Catalog::new(&definitions)
        .validate(&call)
        .is_ok());
    assert!(exec.preflight(&call).is_none());
    // Schemas delivered on subsequent model steps keep the same role contract.
    // Transport discovery/validation is exercised by the MCP runtime fixtures.
    for name in [
        "mcp_search_tools",
        "mcp_load_tool",
        "mcp_database_fixture_connect",
        "mcp_database_fixture_query",
    ] {
        let mut definitions = vec![
            json!({"type":"function","name":name,"description":"Fixture MCP capability","parameters":{"type":"object","properties":{"query":{"type":"string"}},"required":["query"],"additionalProperties":false}}),
        ];
        exec.filter(&mut definitions);
        let call = ToolCall {
            name: name.into(),
            args: json!({"query":"authorized follow-up"}),
            ..call.clone()
        };
        assert!(
            crate::agent::tool_contract::Catalog::new(&definitions)
                .validate(&call)
                .is_ok(),
            "{name}"
        );
        assert!(exec.preflight(&call).is_none(), "{name}");
    }
    for unavailable in [
        "jarvis_propose_agent",
        "jarvis_propose_flow",
        "video_run",
        "terminal_start",
    ] {
        assert!(!exec.allowed(unavailable), "{unavailable}");
    }
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

fn blocked_canvas_worker(hub: &Arc<Hub>) -> (RunDefinition, Job, String) {
    let definition = definition();
    hub.root
        .update(true, |data| {
            let turn = &mut data.turns.last_mut().unwrap().turn;
            turn.options.workflow = Some(Flow::Custom);
            turn.options.custom_workflow_id = Some(definition.flow.id.clone());
        })
        .unwrap();
    let run = hub.root.snapshot().unwrap().turns[0].id.clone();
    hub.mutate(|state| {
        state.flow = Flow::Custom;
        state.run_id = run;
        state.options.workflow = Some(Flow::Custom);
        state.options.custom_workflow_id = Some(definition.flow.id.clone());
        state.custom_definition = Some(definition.clone());
        let mut cursor = Cursor::start(&definition);
        cursor.results.push(handoff(Verdict::Completed));
        cursor.visited.push("previous-completed-stage".into());
        state.custom_cursor = Some(cursor);
        Ok(())
    })
    .unwrap();
    let mut job = prepare(hub, &definition, &definition.flow.steps[0], &[], 1).unwrap();
    let (session, _) = storage::worker(hub, &job, None).unwrap();
    let worker_turn = session.snapshot().unwrap().turns[0].id.clone();
    let vision_error = "Falha no Vision (visual-model · conta visual-account): O provedor ficou 60 segundos sem responder. Revise Vision em Configurações → Provedores → Vision.";
    session.update(true, |data| {
        let turn = data.turns.last_mut().unwrap();
        turn.wire.extend([
            json!({"type":"function_call","call_id":"saved-image","name":"generate_image","arguments":"{\"prompt\":\"Approved background\"}"}),
            json!({"type":"function_call_output","call_id":"saved-image","output":"{\"sourceImageIds\":[\"confirmed-source\"],\"exports\":[\"background.png\"]}"}),
            json!({"type":"function_call","call_id":"saved-export","name":"image_process","arguments":"{\"image_ids\":[\"confirmed-source\"]}"}),
            json!({"type":"function_call_output","call_id":"saved-export","output":"{\"exports\":[\"background.png\"]}"}),
            json!({"type":"function_call","call_id":"failed-analysis","name":"vision","arguments":"{\"ids\":[\"confirmed-source\"],\"question\":\"Inspect the export\"}"}),
            json!({"type":"function_call_output","call_id":"failed-analysis","output":vision_error}),
        ]);
        turn.turn.steps.push(Step {
            text: "Image and export confirmed; only Vision analysis is pending".into(),
            tools: vec![ToolCall {
                id: "failed-analysis".into(),
                name: "vision".into(),
                args: json!({"ids":["confirmed-source"],"question":"Inspect the export"}),
                status: "error".into(),
                output: vision_error.into(),
                duration_ms: 60000,
            }],
            ..Step::default()
        });
    }).unwrap();
    super::super::super::finish(&session, Ok(()));
    drop(session);
    job.status = Status::Blocked;
    let mut result = handoff(Verdict::Blocked);
    result.summary = "Vision timed out; keep background.png and confirmed-source".into();
    job.handoff = Some(result);
    hub.mutate(|state| {
        state.root_status = Status::Failed;
        state.custom_cursor.as_mut().unwrap().active = Some(job.id.clone());
        state.jobs.insert(job.id.clone(), job.clone());
        Ok(())
    })
    .unwrap();
    super::super::super::finish(&hub.root, Err(invalid("Vision timed out")));
    (definition, job, worker_turn)
}

#[tokio::test]
async fn blocked_canvas_notice_identifies_the_failed_vision_model() {
    let (_fixture, hub) = super::super::tests::hub();
    let (_, blocked, _) = blocked_canvas_worker(&hub);
    let error = execute_step(hub.clone(), blocked.clone())
        .await
        .unwrap_err();
    assert!(error.message.starts_with("Falha no Vision"));
    assert!(error
        .message
        .contains("visual-model · conta visual-account"));
    assert!(error.message.contains("60 segundos"));
    assert!(error
        .message
        .contains("Configurações → Provedores → Vision"));
    assert!(error.message.contains(&blocked.handoff.unwrap().summary));
    assert!(hub.live.lock().unwrap().is_empty());
}

#[tokio::test]
async fn canvas_rework_keeps_its_route_after_a_recoverable_vision_failure() {
    let (_fixture, hub) = super::super::tests::hub();
    let (_, mut job, _) = blocked_canvas_worker(&hub);
    job.handoff.as_mut().unwrap().verdict = Verdict::Rework;
    let result = execute_step(hub, job).await.unwrap();
    assert_eq!(result.verdict, Verdict::Rework);
    assert!(result.summary.contains("background.png"));
}

#[tokio::test]
async fn completed_canvas_and_new_attempts_do_not_reuse_an_old_vision_failure() {
    for completed in [false, true] {
        let (_fixture, hub) = super::super::tests::hub();
        let (_, mut job, _) = blocked_canvas_worker(&hub);
        if completed {
            job.status = Status::Completed;
            job.handoff = Some(handoff(Verdict::Completed));
        } else {
            let (session, _) = storage::worker(&hub, &job, None).unwrap();
            session
                .update(true, |data| {
                    data.turns.last_mut().unwrap().turn.steps.push(Step {
                        tools: vec![ToolCall {
                            id: "successful-analysis".into(),
                            name: "vision".into(),
                            args: json!({}),
                            status: "completed".into(),
                            output: "Image inspected successfully".into(),
                            duration_ms: 1,
                        }],
                        ..Step::default()
                    });
                })
                .unwrap();
            super::super::super::finish(&session, Ok(()));
        }
        let expected = job.handoff.as_ref().unwrap().summary.clone();
        let result = execute_step(hub, job).await.unwrap();
        assert_eq!(result.summary, expected);
    }
}

#[test]
fn explicit_canvas_retry_continues_the_blocked_step_without_replaying_confirmed_images() {
    let (_fixture, hub) = super::super::tests::hub();
    let (definition, blocked, worker_turn) = blocked_canvas_worker(&hub);
    hub.mutate(|state| {
        state
            .jobs
            .get_mut(&blocked.id)
            .unwrap()
            .recovery_attempt_pending = true;
        Ok(())
    })
    .unwrap();
    let run = hub.manifest.lock().unwrap().run_id.clone();
    hub.root.retry_failed_turn(&run, None).unwrap();
    let retried = prepare(&hub, &definition, &definition.flow.steps[0], &[], 1).unwrap();
    assert_eq!(retried.status, Status::Queued);
    assert_eq!(retried.id, blocked.id);
    assert_eq!(retried.attempts, blocked.attempts + 1);
    assert_eq!(
        serde_json::to_value(&retried.options).unwrap(),
        serde_json::to_value(&blocked.options).unwrap()
    );
    assert_eq!(
        serde_json::to_value(&retried.custom_agent).unwrap(),
        serde_json::to_value(&blocked.custom_agent).unwrap()
    );
    assert_eq!(retried.scope, blocked.scope);
    assert!(!retried.recovery_attempt_pending);
    assert!(retried.prompt.contains("background.png"));
    assert!(retried.prompt.contains("Vision timed out"));
    let cursor = hub.manifest.lock().unwrap().custom_cursor.clone().unwrap();
    assert_eq!(cursor.results.len(), 1);
    assert_eq!(cursor.visited, vec!["previous-completed-stage"]);
    // A crash after queuing must not reuse the old completed turn as the new result.
    let saved = storage::load(&hub.directory, &hub.root.id)
        .unwrap()
        .unwrap();
    let (recovered, _) =
        storage::prepare_recovery(&hub.directory, saved, Flow::Custom, &run, vec![]).unwrap();
    *hub.manifest.lock().unwrap() = recovered;
    let retried = prepare(&hub, &definition, &definition.flow.steps[0], &[], 1).unwrap();
    assert_eq!(retried.status, Status::Queued);
    assert_eq!(retried.attempts, blocked.attempts + 1);
    let (session, _) = storage::worker(&hub, &retried, None).unwrap();
    let data = session.data.lock().unwrap();
    assert_eq!(data.turns.len(), 2);
    assert_eq!(data.turns[0].turn.id, worker_turn);
    assert_eq!(data.turns[0].turn.status, TurnStatus::Completed);
    assert_eq!(data.turns[1].turn.status, TurnStatus::Running);
    assert!(data.turns[1].wire[0]["content"]
        .as_str()
        .unwrap()
        .contains("background.png"));
    assert_eq!(
        data.turns
            .iter()
            .flat_map(|turn| &turn.wire)
            .filter(|item| item["type"] == "function_call" && item["name"] == "generate_image")
            .count(),
        1
    );
    assert_eq!(
        data.turns
            .iter()
            .flat_map(|turn| &turn.wire)
            .filter(|item| item["type"] == "function_call" && item["name"] == "image_process")
            .count(),
        1
    );
    assert_eq!(
        data.turns
            .iter()
            .flat_map(|turn| &turn.wire)
            .filter(
                |item| item["type"] == "function_call_output" && item["call_id"] == "saved-image"
            )
            .count(),
        1
    );
    drop(data);
    super::super::super::finish(&session, Ok(()));
}

#[test]
fn canvas_retry_marker_is_consumed_once_even_if_the_new_blocked_handoff_precedes_a_crash() {
    let (_fixture, hub) = super::super::tests::hub();
    let (definition, blocked, _) = blocked_canvas_worker(&hub);
    let traversal_cursor = hub.manifest.lock().unwrap().custom_cursor.clone().unwrap();
    let run = hub.manifest.lock().unwrap().run_id.clone();
    hub.root.retry_failed_turn(&run, None).unwrap();
    let first_wire_length = hub
        .root
        .data
        .lock()
        .unwrap()
        .turns
        .last()
        .unwrap()
        .wire
        .len();
    let retried = prepare(&hub, &definition, &definition.flow.steps[0], &[], 1).unwrap();
    let first_retry_id = hub
        .manifest
        .lock()
        .unwrap()
        .custom_cursor
        .as_ref()
        .unwrap()
        .retry
        .as_ref()
        .unwrap()
        .root_retry_id
        .clone();
    let (session, _) = storage::worker(&hub, &retried, None).unwrap();
    session.update(true, |data| {
        data.turns.last_mut().unwrap().wire.extend([
            json!({"type":"function_call","call_id":"new-analysis","name":"vision","arguments":"{}"}),
            json!({"type":"function_call_output","call_id":"new-analysis","output":"Vision remains unavailable"}),
        ]);
    }).unwrap();
    super::super::super::finish(&session, Ok(()));
    drop(session);
    hub.mutate(|state| {
        // Traversal holds an older cursor while prepare consumes the durable retry.
        traversal_cursor.checkpoint(state);
        state.root_status = Status::Failed;
        let job = state.jobs.get_mut(&blocked.id).unwrap();
        job.status = Status::Blocked;
        job.handoff = Some(handoff(Verdict::Blocked));
        Ok(())
    })
    .unwrap();
    let saved = storage::load(&hub.directory, &hub.root.id)
        .unwrap()
        .unwrap();
    let (recovered, _) =
        storage::prepare_recovery(&hub.directory, saved, Flow::Custom, &run, vec![]).unwrap();
    *hub.manifest.lock().unwrap() = recovered;
    let preserved = prepare(&hub, &definition, &definition.flow.steps[0], &[], 1).unwrap();
    assert_eq!(preserved.status, Status::Blocked);
    assert_eq!(preserved.attempts, blocked.attempts + 1);
    assert!(hub.live.lock().unwrap().is_empty());
    // Only a new explicit request may retry the remaining analysis again.
    hub.root
        .update(true, |data| {
            data.turns
                .last_mut()
                .unwrap()
                .wire
                .push(json!({"role":"assistant","content":"Still blocked"}));
        })
        .unwrap();
    super::super::super::finish(&hub.root, Err(invalid("Still blocked")));
    hub.root.retry_failed_turn(&run, None).unwrap();
    hub.root
        .update(true, |data| {
            let turn = data.turns.last_mut().unwrap();
            let marker = turn.wire.pop().unwrap();
            assert_ne!(marker["_jarvis_retry_id"].as_str().unwrap(), first_retry_id);
            // A history rewrite can reuse a wire length; the new retry still has its own identity.
            turn.wire.truncate(first_wire_length - 1);
            turn.wire.push(marker);
            assert_eq!(turn.wire.len(), first_wire_length);
        })
        .unwrap();
    let next = prepare(&hub, &definition, &definition.flow.steps[0], &[], 1).unwrap();
    assert_eq!(next.status, Status::Queued);
    assert_eq!(next.attempts, blocked.attempts + 2);
}

#[test]
fn canvas_recovery_preserves_unrequested_blocked_and_accepted_handoffs() {
    for (verdict, explicit) in [
        (Verdict::Blocked, false),
        (Verdict::Completed, true),
        (Verdict::Approved, true),
    ] {
        let (_fixture, hub) = super::super::tests::hub();
        let (definition, blocked, _) = blocked_canvas_worker(&hub);
        hub.mutate(|state| {
            let job = state.jobs.get_mut(&blocked.id).unwrap();
            job.status = if verdict == Verdict::Blocked {
                Status::Blocked
            } else {
                Status::Completed
            };
            job.handoff = Some(handoff(verdict.clone()));
            Ok(())
        })
        .unwrap();
        if explicit {
            let run = hub.manifest.lock().unwrap().run_id.clone();
            hub.root.retry_failed_turn(&run, None).unwrap();
        }
        let saved = storage::load(&hub.directory, &hub.root.id)
            .unwrap()
            .unwrap();
        let run = saved.run_id.clone();
        let (recovered, _) =
            storage::prepare_recovery(&hub.directory, saved, Flow::Custom, &run, vec![]).unwrap();
        *hub.manifest.lock().unwrap() = recovered;
        let preserved = prepare(&hub, &definition, &definition.flow.steps[0], &[], 1).unwrap();
        assert!(!preserved.status.active());
        assert_eq!(preserved.handoff.unwrap().verdict, verdict);
        assert_eq!(preserved.attempts, blocked.attempts);
    }
}

#[test]
fn an_old_retry_marker_does_not_authorize_a_later_blocked_result() {
    let (_fixture, hub) = super::super::tests::hub();
    let (definition, blocked, _) = blocked_canvas_worker(&hub);
    let run = hub.manifest.lock().unwrap().run_id.clone();
    hub.root.retry_failed_turn(&run, None).unwrap();
    hub.root
        .update(true, |data| {
            data.turns
                .last_mut()
                .unwrap()
                .wire
                .push(json!({"role":"assistant","content":"A later blocked result"}));
        })
        .unwrap();
    let preserved = prepare(&hub, &definition, &definition.flow.steps[0], &[], 1).unwrap();
    assert_eq!(preserved.status, Status::Blocked);
    assert_eq!(preserved.attempts, blocked.attempts);
}

#[test]
fn explicit_canvas_retry_accepts_a_persisted_legacy_marker_once() {
    let (_fixture, hub) = super::super::tests::hub();
    let (definition, blocked, _) = blocked_canvas_worker(&hub);
    let run = hub.manifest.lock().unwrap().run_id.clone();
    hub.root.retry_failed_turn(&run, None).unwrap();
    hub.root
        .update(true, |data| {
            data.turns
                .last_mut()
                .unwrap()
                .wire
                .last_mut()
                .unwrap()
                .as_object_mut()
                .unwrap()
                .remove("_jarvis_retry_id");
        })
        .unwrap();
    let retried = prepare(&hub, &definition, &definition.flow.steps[0], &[], 1).unwrap();
    assert_eq!(retried.status, Status::Queued);
    assert_eq!(retried.attempts, blocked.attempts + 1);
    let unchanged = prepare(&hub, &definition, &definition.flow.steps[0], &[], 1).unwrap();
    assert_eq!(unchanged.attempts, retried.attempts);
}

#[test]
fn canvas_retry_keeps_uncertain_actions_and_cancelled_questions_blocked() {
    for cancelled_question in [false, true] {
        let (_fixture, hub) = super::super::tests::hub();
        let (definition, blocked, _) = blocked_canvas_worker(&hub);
        let (session, _) = storage::worker(&hub, &blocked, None).unwrap();
        session.update(true, |data| {
            let turn = data.turns.last_mut().unwrap();
            let name = if cancelled_question { "ask_user" } else { "write" };
            turn.wire.push(json!({"type":"function_call","call_id":"unresolved","name":name,"arguments":"{}"}));
            if cancelled_question {
                let output = super::super::super::questions::cancelled_output();
                turn.wire.push(json!({"type":"function_call_output","call_id":"unresolved","output":output}));
                turn.turn.steps.push(Step {
                    tools: vec![ToolCall {
                        id: "unresolved".into(), name: name.into(), args: json!({}),
                        status: "error".into(), output, duration_ms: 0,
                    }],
                    ..Step::default()
                });
            }
        }).unwrap();
        super::super::super::finish(&session, Ok(()));
        drop(session);
        let run = hub.manifest.lock().unwrap().run_id.clone();
        hub.root.retry_failed_turn(&run, None).unwrap();
        assert!(prepare(&hub, &definition, &definition.flow.steps[0], &[], 1).is_err());
        let state = hub.manifest.lock().unwrap();
        assert_eq!(state.jobs[&blocked.id].status, Status::Blocked);
        assert_eq!(state.jobs[&blocked.id].attempts, blocked.attempts);
        assert!(state.custom_cursor.as_ref().unwrap().retry.is_none());
    }
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

fn adaptive_catalog() -> catalog::Catalog {
    let mut value = serde_json::to_value(catalog::tests::example()).unwrap();
    let planner = "1".repeat(32);
    let specialists = ["narrative", "design", "godot", "art", "3d", "qa"];
    let base_agent = value["agents"][0].clone();
    let mut agents = vec![base_agent.clone()];
    agents.extend(specialists.iter().enumerate().map(|(i, name)| {
        let mut agent = base_agent.clone();
        agent["id"] = json!(format!("{:032x}", i + 100));
        agent["name"] = json!(name);
        agent
    }));
    value["agents"] = json!(agents);
    let mut routes: Vec<_> = specialists
        .iter()
        .enumerate()
        .map(|(i, name)| json!({"id":name,"label":name,"target":format!("{:032x}", i + 2)}))
        .collect();
    routes.push(json!({"id":"finish","label":"Entregar","target":null}));
    let mut steps = vec![
        json!({"id":planner,"agentId":value["agents"][0]["id"],"instructions":"Plan selectively","position":{"x":0,"y":0},"next":null,"onRework":null,"routes":routes}),
    ];
    steps.extend(specialists.iter().enumerate().map(|(i, name)| json!({"id":format!("{:032x}",i+2),"agentId":format!("{:032x}",i+100),"instructions":name,"position":{"x":100,"y":i*100},"next":planner,"onRework":null})));
    value["flows"][0]["entry"] = json!(planner);
    value["flows"][0]["steps"] = json!(steps);
    value["flows"][0]["maxSteps"] = json!(24);
    let catalog: catalog::Catalog = serde_json::from_value(value).unwrap();
    catalog.validate().unwrap();
    catalog
}

fn adaptive_definition() -> RunDefinition {
    let catalog = adaptive_catalog();
    catalog.resolve(&catalog.flows[0].id).unwrap()
}

#[tokio::test]
async fn adaptive_routing_selects_only_the_requested_specialists_for_five_rpg_modes() {
    let definition = adaptive_definition();
    for path in [
        vec!["godot"],
        vec!["narrative"],
        vec!["design", "godot", "qa"],
        vec!["design", "art", "3d", "godot", "qa"],
        vec!["narrative", "design", "godot", "qa"],
    ] {
        let catalog = adaptive_catalog();
        let before = serde_json::to_value(&catalog).unwrap();
        let trace: Value = serde_json::from_str(&routing::trace(&catalog, &json!({"id":definition.flow.id,"routes":path.iter().copied().chain(["finish"]).collect::<Vec<_>>()})).unwrap()).unwrap();
        assert_eq!(trace["modelsExecuted"], false);
        assert_eq!(trace["toolsExecuted"], false);
        assert_eq!(trace["terminal"], true);
        assert_eq!(
            trace["steps"]
                .as_array()
                .unwrap()
                .iter()
                .filter_map(|step| step["agentName"].as_str())
                .filter(|name| *name != "Researcher")
                .collect::<Vec<_>>(),
            path
        );
        assert_eq!(serde_json::to_value(&catalog).unwrap(), before);
        let mut choices = path.iter().copied().chain(["finish"]);
        let mut visited = vec![];
        let (_sender, signal) = watch::channel(false);
        let results = walk(&definition, signal, |step, _, _| {
            let route = if step.id == definition.flow.entry {
                Some(choices.next().unwrap())
            } else {
                visited.push(step.instructions);
                None
            };
            let mut value = serde_json::to_value(handoff(Verdict::Completed)).unwrap();
            if let Some(route) = route {
                value["route"] = json!(route);
            }
            async move { Ok(serde_json::from_value(value).unwrap()) }
        })
        .await
        .unwrap();
        assert_eq!(visited, path);
        assert_eq!(results.len(), path.len() * 2 + 1);
    }
}

#[test]
fn adaptive_catalog_rejects_ambiguous_missing_and_nonterminating_routes() {
    for case in 0..5 {
        let mut catalog = adaptive_catalog();
        let entry = &mut catalog.flows[0].steps[0];
        match case {
            0 => entry.next = Some(entry.routes[0].target.clone().unwrap()),
            1 => entry.routes.push(entry.routes[0].clone()),
            2 => entry.routes[0].id = String::new(),
            3 => entry.routes[0].target = Some("e".repeat(32)),
            _ => {
                entry.routes.pop();
            }
        }
        assert!(catalog.validate().is_err(), "case {case}");
    }
    let catalog = adaptive_catalog();
    let id = &catalog.flows[0].id;
    for choices in [
        vec![],
        vec!["invented"],
        vec!["finish", "godot"],
        vec!["narrative"; 48],
    ] {
        assert!(routing::trace(&catalog, &json!({"id":id,"routes":choices})).is_err());
    }
}

#[tokio::test]
async fn adaptive_completion_validates_the_choice_before_persisting_and_keeps_it_on_recovery() {
    let (_fixture, hub) = super::super::tests::hub();
    let definition = adaptive_definition();
    let mut job = super::super::tests::job(&hub, Role::Custom, ".");
    job.custom_step_id = Some(definition.flow.entry.clone());
    job.custom_agent = Some(definition.agents[0].clone());
    let id = job.id.clone();
    hub.mutate(|state| {
        state.flow = Flow::Custom;
        state.custom_definition = Some(definition);
        state.jobs.insert(id.clone(), job);
        Ok(())
    })
    .unwrap();
    let exec = Execution {
        hub: hub.clone(),
        id: id.clone(),
        role: Role::Custom,
        flow: Flow::Custom,
        scope: vec![".".into()],
    };
    for route in [
        None,
        Some("invented"),
        Some("00000000000000000000000000000002"),
    ] {
        let mut result = handoff(Verdict::Completed);
        result.route = route.map(str::to_owned);
        let tool = ToolCall {
            id: "choose".into(),
            name: "hub_complete".into(),
            args: json!(result),
            status: "running".into(),
            output: String::new(),
            duration_ms: 0,
        };
        let (_sender, signal) = watch::channel(false);
        assert!(dispatch::execute(&exec, &tool, signal).await.is_err());
        assert!(hub.job(&id).unwrap().handoff.is_none());
    }
    let mut result = handoff(Verdict::Completed);
    result.route = Some("narrative".into());
    let tool = ToolCall {
        id: "choose".into(),
        name: "hub_complete".into(),
        args: json!(result),
        status: "running".into(),
        output: String::new(),
        duration_ms: 0,
    };
    let (_sender, signal) = watch::channel(false);
    dispatch::execute(&exec, &tool, signal).await.unwrap();
    let recovered = storage::load(&hub.directory, &hub.root.id)
        .unwrap()
        .unwrap();
    assert_eq!(
        recovered.jobs[&id]
            .handoff
            .as_ref()
            .unwrap()
            .route
            .as_deref(),
        Some("narrative")
    );
}
fn handoff(verdict: Verdict) -> Handoff {
    Handoff {
        verdict,
        summary: "Evidence-backed outcome".into(),
        outcomes: vec!["Done".into()],
        evidence: vec!["src/file.rs".into()],
        validation: vec![],
        limitations: vec![],
        route: None,
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
    options.service_tier = Some(settings::ServiceTier::Priority);
    agent.model = Some(settings::ModelChoice {
        executor: crate::claude::Executor::Jarvis,
        account: "chosen".into(),
        model: "chosen-model".into(),
        reasoning: Some("high".into()),
        service_tier: None,
        fallback: None,
    });
    apply_model(&mut options, &agent);
    assert_eq!(options.account, "chosen");
    assert_eq!(options.model, "chosen-model");
    assert_eq!(options.service_tier, None);
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
