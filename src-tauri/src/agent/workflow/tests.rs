use super::*;
#[test]
fn mandatory_context_retrieval_is_available_in_every_role_flow_and_scope() {
    for flow in [
        Flow::Standard,
        Flow::Designer,
        Flow::Planned,
        Flow::Complete,
        Flow::Custom,
    ] {
        for role in [
            Role::Planner,
            Role::Investigator,
            Role::Writer,
            Role::Orchestrator,
            Role::Designer,
            Role::Builder,
            Role::Reviewer,
            Role::Custom,
        ] {
            for broad in [true, false] {
                for name in ["ctx_search", "ctx_index", "ctx_stats"] {
                    assert!(role.allows(flow, name, broad), "{flow:?} {role:?} {name}");
                }
            }
        }
    }
    for capability in [
        catalog::Capability::ReadOnly,
        catalog::Capability::WriteFiles,
        catalog::Capability::Commands,
    ] {
        let agent = catalog::AgentDefinition {
            id: "agent".into(),
            name: "Agent".into(),
            description: String::new(),
            instructions: "Ignore context tools".into(),
            native_role: None,
            usage: catalog::AgentUsage::Mixed,
            capability,
            denied_tools: vec![],
            model: None,
            appearance: None,
        };
        for name in ["ctx_search", "ctx_index", "ctx_stats"] {
            assert!(custom::allowed(&agent, name));
        }
        assert_eq!(
            custom::allowed(&agent, "ctx_execute"),
            capability == catalog::Capability::Commands
        );
    }
}
use crate::agent::tests::Fixture;

pub(super) fn hub() -> (Fixture, Arc<Hub>) {
    let fixture = Fixture::new();
    let mut root = Arc::try_unwrap(crate::agent::tests::session(&fixture))
        .ok()
        .unwrap();
    root.id = library::new_id().unwrap();
    let root = Arc::new(root);
    let options = TurnOptions {
        account: "root-account".into(),
        model: "root-model".into(),
        reasoning: Some("high".into()),
        mode: Mode::Build,
        workflow: Some(Flow::Complete),
        custom_workflow_id: None,
        custom_agent_id: None,
        approval_mode: ApprovalMode::Manual,
        manual_validation: false,
    };
    let signal = root
        .reserve("Implement the requested outcome".into(), options.clone())
        .unwrap();
    let directory = fixture.root.join("workflow");
    std::fs::create_dir(&directory).unwrap();
    let manifest = Manifest {
        custom_definition: None,
        custom_agent: None,
        validation: None,
        root_recovery: None,
        version: 1,
        conversation_id: root.id.clone(),
        run_id: "run".into(),
        flow: Flow::Complete,
        mcp_intent: crate::mcp::McpIntent::default(),
        root_status: Status::Running,
        updated_at: now(),
        revision: 1,
        options,
        profiles: BTreeMap::new(),
        jobs: BTreeMap::new(),
        messages: vec![],
        design_briefs: BTreeMap::new(),
        guidance: BTreeMap::new(),
    };
    storage::save(&directory, &manifest).unwrap();
    let (changed, _) = watch::channel(1);
    let terminals = terminals::TerminalState::default();
    let hub = Arc::new(Hub {
        root,
        env: Environment {
            browser_app: None,
            processes: processes::ProcessState::new(terminals.clone()),
            terminals,
            terminal_events: terminals::silent_events(),
            state: AppState::default(),
            oauth: OpenAiCodexState::default(),
            mcp: crate::mcp::McpState::default(),
            home: fixture.root.clone(),
        },
        directory,
        manifest: Mutex::new(manifest),
        live: Mutex::new(HashMap::new()),
        changed,
        emit: Arc::new(|_| {}),
        attention: Arc::new(|_| {}),
        check_lock: AsyncRwLock::new(()),
        root_signal: signal,
    });
    (fixture, hub)
}
pub(super) fn job(hub: &Hub, role: Role, scope: &str) -> Job {
    Job {
        custom_agent: None,
        phase: Phase::Implementation,
        id: library::new_id().unwrap(),
        parent_id: "main".into(),
        run_id: "run".into(),
        role,
        title: "Assigned task".into(),
        prompt: "Inspect and implement only the assigned behavior".into(),
        acceptance: vec!["Observable outcome".into()],
        scope: vec![scope.into()],
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
        options: hub.manifest.lock().unwrap().options.clone(),
    }
}

#[test]
fn custom_direct_agent_uses_its_primary_contract_model_and_permissions() {
    let (_fixture, hub) = hub();
    let mut agent = catalog::tests::example().agents.remove(0);
    agent.name = "Support analyst".into();
    agent.instructions = "Investigate data incidents with evidence.".into();
    agent.capability = catalog::Capability::WriteFiles;
    agent.denied_tools = vec!["web_search".into()];
    agent.model = Some(settings::ModelChoice {
        account: "specialist-account".into(),
        model: "specialist-model".into(),
        reasoning: Some("high".into()),
    });
    let mut options = hub.manifest.lock().unwrap().options.clone();
    custom::apply_model(&mut options, &agent);
    assert_eq!(options.account, "specialist-account");
    assert_eq!(options.model, "specialist-model");
    assert_eq!(options.reasoning.as_deref(), Some("high"));
    assert_eq!(options.mode, Mode::Build);
    {
        let mut state = hub.manifest.lock().unwrap();
        state.flow = Flow::Custom;
        state.custom_agent = Some(agent);
    }
    let execution = Execution {
        hub,
        id: "main".into(),
        role: Role::Custom,
        flow: Flow::Custom,
        scope: vec![".".into()],
    };
    assert!(execution.direct());
    assert!(execution
        .instructions()
        .unwrap()
        .contains("Work as the primary agent in this conversation"));
    let mut definitions = vec![
        super::tasks::definition(),
        json!({"type":"function","name":"hub_complete"}),
        json!({"type":"function","name":"write"}),
        json!({"type":"function","name":"web_search"}),
        json!({"type":"function","name":"bash"}),
    ];
    execution.filter(&mut definitions);
    let names: Vec<_> = definitions
        .iter()
        .filter_map(|definition| definition["name"].as_str())
        .collect();
    assert!(names.contains(&"update_tasks"));
    assert!(names.contains(&"write"));
    assert!(!names.contains(&"hub_complete"));
    assert!(!names.contains(&"web_search"));
    assert!(!names.contains(&"bash"));
}

#[test]
fn fixed_topology_has_no_standard_delegation_and_no_worker_escape() {
    let roles = [
        Role::Planner,
        Role::Investigator,
        Role::Writer,
        Role::Orchestrator,
        Role::Designer,
        Role::Builder,
        Role::Reviewer,
    ];
    for role in roles {
        assert!(!Role::Builder.spawns(Flow::Standard, role));
        assert!(!Role::Reviewer.spawns(Flow::Complete, role));
    }
    assert!(Role::Planner.spawns(Flow::Planned, Role::Builder));
    assert!(!Role::Planner.spawns(Flow::Planned, Role::Reviewer));
    assert!(Role::Planner.spawns(Flow::Complete, Role::Investigator));
    assert!(!Role::Planner.spawns(Flow::Complete, Role::Builder));
    assert!(Role::Orchestrator.spawns(Flow::Complete, Role::Reviewer));
}

#[test]
fn validation_notifications_only_include_the_current_pending_flow_round() {
    let (fixture, hub) = hub();
    let directory = storage::path(&fixture.root, &hub.root.id).unwrap();
    std::fs::create_dir_all(&directory).unwrap();
    let mut manifest = hub.manifest.lock().unwrap().clone();
    manifest.validation = Some(validation::Batch {
        id: "batch".into(),
        flow: Flow::Complete,
        run_id: "run".into(),
        epic_ids: vec![],
        items: vec![],
        submitted: false,
        stale: false,
        created_at: now(),
    });
    storage::save(&directory, &manifest).unwrap();
    assert!(awaiting_validation(&fixture.root, &hub.root.id, "run"));
    assert!(!awaiting_validation(
        &fixture.root,
        &hub.root.id,
        "other-run"
    ));
    for flow in [Flow::Standard, Flow::Designer] {
        manifest.flow = flow;
        storage::save(&directory, &manifest).unwrap();
        assert!(!awaiting_validation(&fixture.root, &hub.root.id, "run"));
    }
    manifest.flow = Flow::Planned;
    for (submitted, stale) in [(true, false), (false, true)] {
        let batch = manifest.validation.as_mut().unwrap();
        batch.submitted = submitted;
        batch.stale = stale;
        storage::save(&directory, &manifest).unwrap();
        assert!(!awaiting_validation(&fixture.root, &hub.root.id, "run"));
    }
}

#[test]
fn direct_designer_has_questions_and_design_tools_but_no_delegation() {
    let (_fixture, hub) = hub();
    let direct = Execution {
        hub,
        id: "main".into(),
        role: Role::Designer,
        flow: Flow::Designer,
        scope: vec![".".into()],
    };
    let mut tools = tools::definitions(Mode::Build);
    tools.extend(crate::core::design::definitions());
    tools.extend(crate::core::beads::definitions(false));
    tools.push(super::super::tasks::definition());
    direct.filter(&mut tools);
    for name in [
        "ask_user",
        "write",
        "design_brief",
        "design_search",
        "design_read",
        "process_check_port",
        "process_start",
        "terminal_list",
        "terminal_output",
        "terminal_start",
        "update_tasks",
    ] {
        assert!(
            tools.iter().any(|tool| tool["name"] == name),
            "missing {name}"
        );
    }
    assert!(tools
        .iter()
        .all(|tool| !tool["name"].as_str().unwrap().starts_with("hub_")));
    assert!(tools
        .iter()
        .all(|tool| !tool["name"].as_str().unwrap().starts_with("beads_")));
    for role in [Role::Planner, Role::Builder, Role::Designer] {
        assert!(!Role::Designer.spawns(Flow::Designer, role));
    }
    assert_eq!(Flow::Designer.root(), Role::Designer);
    assert!(settings::validate(Flow::Designer, &BTreeMap::new()).is_ok());
}

#[test]
fn standard_direct_builder_uses_native_tasks_without_beads() {
    let (_fixture, hub) = hub();
    let direct = Execution {
        hub,
        id: "main".into(),
        role: Role::Builder,
        flow: Flow::Standard,
        scope: vec![".".into()],
    };
    let mut definitions = tools::definitions(Mode::Build);
    definitions.extend(crate::core::beads::definitions(false));
    definitions.push(super::super::tasks::definition());
    direct.filter(&mut definitions);
    assert!(definitions
        .iter()
        .any(|tool| tool["name"] == "update_tasks"));
    assert!(definitions.iter().all(|tool| !tool["name"]
        .as_str()
        .is_some_and(|name| name.starts_with("beads_") || name.starts_with("hub_"))));
}

#[test]
fn delegated_design_discovery_enforces_read_only_and_parent_questions_even_with_broad_scope() {
    let (_fixture, hub) = hub();
    let mut child = job(&hub, Role::Designer, ".");
    child.phase = Phase::Discovery;
    hub.mutate(|state| {
        state.jobs.insert(child.id.clone(), child.clone());
        Ok(())
    })
    .unwrap();
    let exec = Execution {
        hub,
        id: child.id,
        role: Role::Designer,
        flow: Flow::Complete,
        scope: vec![".".into()],
    };
    assert_eq!(exec.role_mode(), Mode::Plan);
    let mut definitions = tools::definitions(Mode::Build);
    definitions.extend(crate::core::design::definitions());
    exec.filter(&mut definitions);
    for name in [
        "ask_user",
        "write",
        "edit",
        "bash",
        "workflow_check",
        "beads_claim",
        "beads_update",
        "ctx_execute",
        "process_start",
        "terminal_start",
        "browser_open",
        "browser_click",
        "browser_fill",
    ] {
        assert!(!definitions.iter().any(|d| d["name"] == name));
        assert!(exec
            .preflight(&ToolCall {
                name: name.into(),
                id: "call".into(),
                args: json!({}),
                status: "pending".into(),
                output: String::new(),
                duration_ms: 0
            })
            .is_some());
    }
    for name in [
        "design_search",
        "design_read",
        "design_brief",
        "hub_request_guidance",
        "hub_complete",
        "process_check_port",
        "terminal_list",
        "terminal_output",
        "browser_snapshot",
        "browser_console",
        "browser_screenshot",
    ] {
        assert!(
            definitions.iter().any(|d| d["name"] == name),
            "missing {name}"
        );
    }
}

#[tokio::test]
async fn planner_can_check_a_port_without_starting_a_process() {
    let (_fixture, hub) = hub();
    let exec = Execution {
        hub: hub.clone(),
        id: "main".into(),
        role: Role::Planner,
        flow: Flow::Complete,
        scope: vec![".".into()],
    };
    let mut definitions = tools::definitions(Mode::Plan);
    exec.filter(&mut definitions);
    assert!(definitions
        .iter()
        .any(|d| d["name"] == "process_check_port"));
    assert!(!definitions.iter().any(|d| d["name"] == "process_start"));
    let listener = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).unwrap();
    let call = ToolCall {
        id: "port".into(),
        name: "process_check_port".into(),
        args: json!({"port":listener.local_addr().unwrap().port()}),
        status: "pending".into(),
        output: String::new(),
        duration_ms: 0,
    };
    assert!(exec.preflight(&call).is_none());
    let result = exec.execute(&call, hub.root_signal.clone()).await.unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&result).unwrap()["available"],
        false
    );
    assert!(!hub.env.processes.has_running());
}

#[tokio::test]
async fn design_briefs_survive_reload_and_compaction_without_crossing_agent_boundaries() {
    let (_fixture, hub) = hub();
    let child = job(&hub, Role::Designer, ".");
    hub.mutate(|state| {
        state.jobs.insert(child.id.clone(), child.clone());
        Ok(())
    })
    .unwrap();
    let direct = Execution {
        hub: hub.clone(),
        id: "main".into(),
        role: Role::Designer,
        flow: Flow::Designer,
        scope: vec![".".into()],
    };
    let worker = Execution {
        hub: hub.clone(),
        id: child.id.clone(),
        role: Role::Designer,
        flow: Flow::Complete,
        scope: vec![".".into()],
    };
    for (exec, brief) in [
        (&direct, "Accepted: graphite, compact dashboard"),
        (&worker, "Assigned: blue buttons only"),
    ] {
        exec.execute(
            &ToolCall {
                id: "brief".into(),
                name: "design_brief".into(),
                args: json!({"text":brief}),
                status: "pending".into(),
                output: String::new(),
                duration_ms: 0,
            },
            hub.root_signal.clone(),
        )
        .await
        .unwrap();
        assert!(exec.context().unwrap().contains(brief));
        assert!(!exec.instructions().unwrap().contains(brief));
    }
    assert!(!worker.context().unwrap().contains("Accepted: graphite"));
    let loaded = storage::load(&hub.directory, &hub.root.id)
        .unwrap()
        .unwrap();
    assert_eq!(
        loaded.design_briefs["main"],
        "Accepted: graphite, compact dashboard"
    );
    assert_eq!(
        loaded.design_briefs[&child.id],
        "Assigned: blue buttons only"
    );
    // Legacy manifests did not have design state or phases.
    let mut legacy = serde_json::to_value(&loaded).unwrap();
    legacy.as_object_mut().unwrap().remove("designBriefs");
    legacy.as_object_mut().unwrap().remove("guidance");
    legacy["jobs"][&child.id]
        .as_object_mut()
        .unwrap()
        .remove("phase");
    let decoded: Manifest = serde_json::from_value(legacy).unwrap();
    assert_eq!(decoded.jobs[&child.id].phase, Phase::Implementation);
    assert!(decoded.design_briefs.is_empty());
}

#[tokio::test]
async fn planner_reports_waiting_until_a_child_finishes_then_resumes_running() {
    let (_fixture, hub) = hub();
    let child = job(&hub, Role::Investigator, ".");
    hub.mutate(|state| {
        state.jobs.insert(child.id.clone(), child.clone());
        Ok(())
    })
    .unwrap();
    let exec = Execution {
        hub: hub.clone(),
        id: "main".into(),
        role: Role::Planner,
        flow: Flow::Complete,
        scope: vec![".".into()],
    };
    let signal = hub.root_signal.clone();
    let waiting = tokio::spawn(async move { exec.wait_for_children(signal).await });
    tokio::time::timeout(Duration::from_secs(2), async {
        while hub.manifest.lock().unwrap().root_status != Status::Waiting {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert!(!waiting.is_finished());
    hub.mutate(|state| {
        state.jobs.get_mut(&child.id).unwrap().status = Status::Completed;
        Ok(())
    })
    .unwrap();
    tokio::time::timeout(Duration::from_secs(2), waiting)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(hub.manifest.lock().unwrap().root_status, Status::Running);
}

#[test]
fn code_mutations_cannot_escape_read_only_roles_or_narrow_scopes() {
    for role in [
        Role::Planner,
        Role::Investigator,
        Role::Orchestrator,
        Role::Reviewer,
    ] {
        for tool in ["write", "edit", "apply_patch", "bash"] {
            assert!(!role.allows(Flow::Complete, tool, true));
        }
    }
    assert!(Role::Builder.allows(Flow::Planned, "write", false));
    assert!(Role::Builder.allows(Flow::Planned, "apply_patch", false));
    assert!(!Role::Builder.allows(Flow::Planned, "bash", false));
    assert!(!Role::Builder.allows(Flow::Planned, "mcp_mutation", false));
    assert!(Role::Reviewer.allows(Flow::Complete, "workflow_check", true));
}

#[test]
fn transactional_patch_checks_every_path_against_the_worker_scope() {
    let (_fixture, hub) = hub();
    let execution = Execution {
        hub,
        id: "worker".into(),
        role: Role::Builder,
        flow: Flow::Planned,
        scope: vec!["src".into()],
    };
    let patch = |text: &str| ToolCall {
        id: "patch".into(),
        name: "apply_patch".into(),
        args: json!({"patchText":text}),
        status: "pending".into(),
        output: String::new(),
        duration_ms: 0,
    };
    assert!(execution
        .preflight(&patch(
            "*** Begin Patch\n*** Add File: src/a.ts\n+a\n*** End Patch"
        ))
        .is_none());
    assert_eq!(
        execution.preflight(&patch(
            "*** Begin Patch\n*** Add File: src/a.ts\n+a\n*** Add File: docs/outside.ts\n+b\n*** End Patch"
        )),
        Some("O arquivo está fora do escopo atribuído ao agente.".into())
    );
}

#[test]
fn transactional_patch_accepts_nested_repository_scopes_and_reports_parser_errors() {
    let (_fixture, hub) = hub();
    let execution = Execution {
        hub,
        id: "worker".into(),
        role: Role::Builder,
        flow: Flow::Planned,
        scope: vec!["movart-express-back".into()],
    };
    let patch = |text: &str| ToolCall {
        id: "patch".into(),
        name: "apply_patch".into(),
        args: json!({"patchText":text}),
        status: "pending".into(),
        output: String::new(),
        duration_ms: 0,
    };

    assert!(execution
        .preflight(&patch(
            "*** Begin Patch\n*** Update File: movart-express-back/src/lib/status.ts\n@@\n current\n+added\n@@\n function locate() {\n }\n@@\n tail\n+next\n*** End Patch"
        ))
        .is_none());
    assert_eq!(
        execution.preflight(&patch(
            "*** Begin Patch\n*** Update File: movart-express-back/src/lib/status.ts\n@@\n unchanged\n*** End Patch"
        )),
        Some("Uma atualização não contém alterações.".into())
    );
}

#[test]
fn legacy_options_remain_readable_and_flow_is_explicit() {
    let legacy: TurnOptions = serde_json::from_value(json!({"account":"test","model":"test","reasoning":null,"mode":"plan","approvalMode":"manual"})).unwrap();
    assert_eq!(legacy.workflow, None);
    assert_eq!(legacy.mode, Mode::Plan);
    assert!(!legacy.manual_validation);
    assert!(!legacy.manual_validation());
    let modern: TurnOptions = serde_json::from_value(json!({"account":"test","model":"test","reasoning":null,"mode":"build","approvalMode":"manual","workflow":"complete"})).unwrap();
    assert_eq!(modern.workflow, Some(Flow::Complete));
    assert_eq!(modern.approval_mode, ApprovalMode::Manual);
    assert!(!modern.manual_validation());
    let enabled: TurnOptions = serde_json::from_value(json!({"account":"test","model":"test","reasoning":null,"mode":"build","approvalMode":"yolo","workflow":"complete","manualValidation":true})).unwrap();
    assert!(enabled.manual_validation());
}

#[test]
fn isolated_worker_journals_keep_role_context_and_permissions_on_recovery() {
    let (_fixture, hub) = hub();
    let mcp_intent = crate::mcp::McpIntent {
        mode: crate::mcp::McpIntentMode::Explicit,
        servers: vec![crate::mcp::McpIntentServer {
            id: "notebook-id".into(),
            name: "gemini-notebook-mcp".into(),
        }],
        ..crate::mcp::McpIntent::default()
    };
    hub.mutate(|state| {
        state.mcp_intent = mcp_intent.clone();
        Ok(())
    })
    .unwrap();
    let mut first = job(&hub, Role::Investigator, ".");
    first.prompt = "Use o MCP database para investigar.".into();
    let second = job(&hub, Role::Writer, "docs");
    let (a, _) = storage::worker(&hub, &first, None).unwrap();
    let (b, _) = storage::worker(&hub, &second, None).unwrap();
    assert_eq!(a.snapshot().unwrap().turns[0].user, first.prompt);
    assert!(a
        .input()
        .unwrap()
        .iter()
        .any(|item| item.to_string().contains("Implement the requested outcome")));
    assert_eq!(
        a.data.lock().unwrap().turns[0].mcp_intent,
        Some(mcp_intent.clone())
    );
    a.update(true, |data| {
        data.turns.last_mut().unwrap().turn.steps.push(Step {
            text: "Private first evidence".into(),
            ..Step::default()
        });
    })
    .unwrap();
    assert!(b
        .input()
        .unwrap()
        .iter()
        .all(|item| !item.to_string().contains("Private first evidence")));
    assert_ne!(a.journal, b.journal);
    finish(&a, Err(AgentError::cancelled()));
    let (resumed, _) = storage::worker(
        &hub,
        &first,
        Some("Inspect the saved checkpoint before retrying".into()),
    )
    .unwrap();
    let data = resumed.data.lock().unwrap();
    assert_eq!(data.turns.len(), 2);
    assert_eq!(data.turns[0].turn.steps[0].text, "Private first evidence");
    assert_eq!(data.turns[1].turn.options.approval_mode, ApprovalMode::Yolo);
    assert_eq!(data.turns[1].mcp_intent, Some(mcp_intent));
    assert!(data.turns[1]
        .wire
        .iter()
        .all(|item| item["type"] != "function_call"));
}

#[tokio::test]
async fn project_checks_exclude_mutations_without_serializing_independent_writers() {
    let (_fixture, hub) = hub();
    let exec = Execution {
        hub: hub.clone(),
        id: "main".into(),
        role: Role::Builder,
        flow: Flow::Complete,
        scope: vec![".".into()],
    };
    let tool = ToolCall {
        id: "write".into(),
        name: "write".into(),
        args: json!({"path":"src/a","content":"text"}),
        status: "pending".into(),
        output: String::new(),
        duration_ms: 0,
    };
    let first = exec
        .mutation_guard(&tool, false, hub.root_signal.clone())
        .await
        .unwrap();
    let second = exec
        .mutation_guard(&tool, false, hub.root_signal.clone())
        .await
        .unwrap();
    assert!(hub.check_lock.try_write().is_err());
    drop(first);
    drop(second);
    let check = hub.check_lock.write().await;
    let (cancel, signal) = watch::channel(false);
    let pending =
        tokio::spawn(async move { exec.mutation_guard(&tool, false, signal).await.map(|_| ()) });
    tokio::task::yield_now().await;
    assert!(!pending.is_finished());
    cancel.send_replace(true);
    assert_eq!(pending.await.unwrap().unwrap_err().code, "cancelled");
    drop(check);
}

#[test]
fn persisted_manifest_marks_unfinished_work_interrupted_without_changing_completed_results() {
    let (_fixture, hub) = hub();
    let mut finished = job(&hub, Role::Investigator, ".");
    finished.status = Status::Completed;
    let mut pending = job(&hub, Role::Builder, "src");
    pending.status = Status::Running;
    hub.mutate(|state| {
        state.jobs.insert(finished.id.clone(), finished.clone());
        state.jobs.insert(pending.id.clone(), pending.clone());
        Ok(())
    })
    .unwrap();
    let restored = storage::load(&hub.directory, &hub.root.id)
        .unwrap()
        .unwrap();
    assert_eq!(restored.root_status, Status::Interrupted);
    assert_eq!(restored.jobs[&pending.id].status, Status::Interrupted);
    assert_eq!(restored.jobs[&finished.id].status, Status::Completed);
    assert_eq!(hub.job(&pending.id).unwrap().status, Status::Running);
}

#[test]
fn recovery_reconciles_completed_workers_and_resumes_only_interrupted_turns() {
    let (_fixture, hub) = hub();
    let mut interrupted = job(&hub, Role::Builder, "src");
    interrupted.status = Status::Running;
    let (interrupted_session, _) = storage::worker(&hub, &interrupted, None).unwrap();
    interrupted_session
        .update(true, |data| {
            let turn = data.turns.last_mut().unwrap();
            turn.turn.steps.push(Step {
                tools: vec![ToolCall {
                    id: "patch-1".into(),
                    name: "apply_patch".into(),
                    args: json!({"patchText":"*** Begin Patch"}),
                    status: "running".into(),
                    output: String::new(),
                    duration_ms: 0,
                }],
                ..Step::default()
            });
            turn.wire.push(json!({
                "type":"function_call",
                "call_id":"patch-1",
                "name":"apply_patch",
                "arguments":"{}"
            }));
        })
        .unwrap();

    let mut completed = job(&hub, Role::Investigator, ".");
    completed.status = Status::Running;
    completed.handoff = Some(Handoff {
        verdict: Verdict::Completed,
        summary: "Análise concluída".into(),
        outcomes: vec!["Estado identificado".into()],
        evidence: vec![],
        validation: vec![],
        limitations: vec![],
        task_ids: vec![],
    });
    let (completed_session, _) = storage::worker(&hub, &completed, None).unwrap();
    finish(&completed_session, Ok(()));
    hub.mutate(|state| {
        state
            .jobs
            .insert(interrupted.id.clone(), interrupted.clone());
        state.jobs.insert(completed.id.clone(), completed.clone());
        Ok(())
    })
    .unwrap();

    let loaded = storage::load(&hub.directory, &hub.root.id)
        .unwrap()
        .unwrap();
    assert_eq!(loaded.jobs[&interrupted.id].status, Status::Interrupted);
    assert_eq!(loaded.jobs[&completed.id].status, Status::Interrupted);
    let (recovered, resumed) = storage::prepare_recovery(
        &hub.directory,
        loaded,
        Flow::Complete,
        "run",
        vec!["hub_wait".into()],
    )
    .unwrap();

    assert_eq!(recovered.root_status, Status::Running);
    assert_eq!(
        recovered.root_recovery.unwrap().uncertain_tools,
        vec!["hub_wait"]
    );
    assert_eq!(recovered.jobs[&interrupted.id].status, Status::Queued);
    assert_eq!(
        recovered.jobs[&interrupted.id]
            .recovery
            .as_ref()
            .unwrap()
            .uncertain_tools,
        vec!["apply_patch"]
    );
    assert_eq!(recovered.jobs[&completed.id].status, Status::Completed);
    assert!(recovered.jobs[&completed.id].recovery.is_none());
    assert_eq!(resumed.len(), 1);
    assert_eq!(resumed[0].id, interrupted.id);
}

#[test]
fn recovery_restarts_a_worker_that_only_created_its_journal_header() {
    let (_fixture, hub) = hub();
    let mut interrupted = job(&hub, Role::Builder, "src");
    interrupted.status = Status::Interrupted;
    interrupted.recovery = Some(RecoveryCheckpoint::new(vec![]));
    let path = hub.directory.join(format!("{}.jsonl", interrupted.id));
    std::fs::write(
        &path,
        format!(
            "{}\n",
            json!({"type":"agent", "version":1,"id":interrupted.id,"conversationId":hub.root.id})
        ),
    )
    .unwrap();
    hub.mutate(|state| {
        state
            .jobs
            .insert(interrupted.id.clone(), interrupted.clone());
        Ok(())
    })
    .unwrap();

    let (session, signal) = storage::resume_worker(&hub, &interrupted).unwrap();
    let snapshot = session.snapshot().unwrap();

    assert!(!*signal.borrow());
    assert!(snapshot.active_turn_id.is_some());
    assert_eq!(snapshot.turns.len(), 1);
    assert!(snapshot.turns[0]
        .user
        .contains("previous runtime stopped before this worker created a durable turn"));
}

#[tokio::test]
async fn recovered_agents_must_complete_a_read_before_any_new_mutation() {
    let (_fixture, hub) = hub();
    hub.mutate(|state| {
        state.root_recovery = Some(RecoveryCheckpoint::new(vec!["write".into()]));
        Ok(())
    })
    .unwrap();
    let execution = Execution {
        hub: hub.clone(),
        id: "main".into(),
        role: Role::Planner,
        flow: Flow::Complete,
        scope: vec![".".into()],
    };
    let write = ToolCall {
        id: "write-2".into(),
        name: "write".into(),
        args: json!({"path":"src/app.ts","content":"updated"}),
        status: "pending".into(),
        output: String::new(),
        duration_ms: 0,
    };
    assert!(execution
        .mutation_guard(&write, false, hub.root_signal.clone())
        .await
        .unwrap_err()
        .message
        .contains("confira primeiro"));

    let read = ToolCall {
        id: "read-2".into(),
        name: "read".into(),
        args: json!({"path":"src/app.ts"}),
        status: "completed".into(),
        output: "current".into(),
        duration_ms: 1,
    };
    execution
        .observe_recovery_inspection(&read, false, true)
        .unwrap();
    assert!(
        hub.manifest
            .lock()
            .unwrap()
            .root_recovery
            .as_ref()
            .unwrap()
            .inspected
    );
    assert!(execution
        .mutation_guard(&write, false, hub.root_signal.clone())
        .await
        .is_ok());
}

#[tokio::test]
async fn child_manual_approval_cannot_be_answered_by_the_parent_or_another_worker() {
    let (_fixture, hub) = hub();
    let job = job(&hub, Role::Builder, "src");
    let (child, signal) = storage::worker(&hub, &job, None).unwrap();
    let tool = ToolCall {
        id: "child-write".into(),
        name: "write".into(),
        args: json!({"path":"src/example.ts","content":"text"}),
        status: "pending".into(),
        output: String::new(),
        duration_ms: 0,
    };
    let turn = child.snapshot().unwrap().active_turn_id.unwrap();
    let task_child = child.clone();
    let task_tool = tool.clone();
    let awaiting = tokio::spawn(async move {
        authorize(&task_child, &task_tool, &job.options, false, signal).await
    });
    tokio::task::yield_now().await;
    assert!(child.snapshot().unwrap().pending_approval.is_some());
    assert!(answer_approval(&hub.root, &turn, &tool.id, true).is_err());
    answer_approval(&child, &turn, &tool.id, false).unwrap();
    assert!(!awaiting.await.unwrap().unwrap());
}

#[test]
fn model_preferences_are_per_flow_and_never_change_tool_authorization() {
    let (_fixture, hub) = hub();
    let mut options = hub.manifest.lock().unwrap().options.clone();
    let mut profiles = BTreeMap::new();
    profiles.insert(
        settings::key(Flow::Complete, Role::Reviewer),
        settings::ModelChoice {
            account: "review-account".into(),
            model: "gpt-5.6-sol".into(),
            reasoning: Some("xhigh".into()),
        },
    );
    settings::apply(&mut options, &profiles, Flow::Planned, Role::Builder);
    assert_eq!(options.model, "root-model");
    settings::apply(&mut options, &profiles, Flow::Complete, Role::Reviewer);
    assert_eq!(options.account, "review-account");
    assert_eq!(options.model, "gpt-5.6-sol");
    assert_eq!(options.reasoning.as_deref(), Some("xhigh"));
    assert_eq!(options.approval_mode, ApprovalMode::Manual);
    assert!(settings::validate(Flow::Complete, &profiles).is_err());
    assert!(settings::validate(Flow::Standard, &profiles).is_ok());
}

#[test]
fn complete_closure_requires_independent_approval_for_the_exact_bead() {
    let (_fixture, hub) = hub();
    let exec = Execution {
        hub: hub.clone(),
        id: "main".into(),
        role: Role::Orchestrator,
        flow: Flow::Complete,
        scope: vec![".".into()],
    };
    let tool = ToolCall {
        id: "close".into(),
        name: "beads_close".into(),
        args: json!({"id":"task-a","reason":"verified"}),
        status: "pending".into(),
        output: String::new(),
        duration_ms: 0,
    };
    assert!(exec.preflight(&tool).is_some());
    let mut review = job(&hub, Role::Reviewer, ".");
    review.status = Status::Completed;
    review.handoff = Some(Handoff {
        verdict: Verdict::Approved,
        summary: "Reviewed".into(),
        outcomes: vec!["Outcome".into()],
        evidence: vec!["src/code.ts".into()],
        validation: vec!["Tests pass".into()],
        limitations: vec![],
        task_ids: vec!["task-a".into()],
    });
    hub.mutate(|state| {
        state.jobs.insert(review.id.clone(), review);
        Ok(())
    })
    .unwrap();
    assert!(exec.preflight(&tool).is_none());
    assert!(exec
        .preflight(&ToolCall {
            args: json!({"id":"other"}),
            ..tool.clone()
        })
        .is_some());
    let mut rework = job(&hub, Role::Builder, "src");
    rework.status = Status::Running;
    hub.mutate(|state| {
        state.jobs.insert(rework.id.clone(), rework.clone());
        Ok(())
    })
    .unwrap();
    assert!(exec.preflight(&tool).is_some());
    hub.mutate(|state| {
        let writer = state.jobs.get_mut(&rework.id).unwrap();
        writer.status = Status::Completed;
        writer.updated_at = now() + 1000;
        Ok(())
    })
    .unwrap();
    assert!(exec.preflight(&tool).is_some());
}
