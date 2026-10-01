use super::*;

#[test]
fn global_companion_validates_explicit_model_independently_of_a_retired_project_profile() {
    let home = tempfile::tempdir().unwrap();
    let directory = crate::data_dir::root(home.path());
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(
        directory.join("agents.json"),
        r#"{
        "standard/builder":{"executor":"agy","account":"","model":"gemini-3-pro","reasoning":"high"}
    }"#,
    )
    .unwrap();
    let state = AppState::default();
    let oauth = OpenAiCodexState::default();
    let mut explicit = super::super::tests::options(ApprovalMode::Yolo);
    explicit.executor = crate::claude::Executor::Claude;
    explicit.account.clear();
    explicit.model = "sonnet".into();
    explicit.reasoning = Some("high".into());
    explicit.workflow = Some(Flow::Standard);
    let global_id = super::super::companion_chat::GLOBAL_CONVERSATION_ID;
    validate_options(&state, &oauth, home.path(), &explicit, global_id).unwrap();
    assert_eq!(
        validate_options(&state, &oauth, home.path(), &explicit, &"a".repeat(32))
            .unwrap_err()
            .code,
        "unsupported_executor"
    );
    assert_eq!(
        settings::read(home.path()).unwrap()["standard/builder"].executor,
        crate::claude::Executor::Unavailable
    );
    explicit.executor = crate::claude::Executor::Unavailable;
    assert_eq!(
        validate_options(&state, &oauth, home.path(), &explicit, global_id)
            .unwrap_err()
            .code,
        "unsupported_executor"
    );
    explicit.executor = crate::claude::Executor::Claude;
    explicit.custom_agent_id = Some("b".repeat(32));
    assert!(validate_options(&state, &oauth, home.path(), &explicit, global_id).is_err());
}

#[test]
fn global_companion_preserves_explicit_model_while_project_chat_uses_its_profile() {
    let mut explicit = super::super::tests::options(ApprovalMode::Yolo);
    explicit.executor = crate::claude::Executor::Claude;
    explicit.account.clear();
    explicit.model = "sonnet".into();
    explicit.reasoning = Some("high".into());
    let profile = settings::ModelChoice {
        executor: crate::claude::Executor::Jarvis,
        account: "configured-account".into(),
        model: "configured-model".into(),
        reasoning: None,
        fallback: None,
    };
    let profiles = BTreeMap::from([(settings::key(Flow::Standard, Role::Builder), profile)]);
    let mut global = explicit.clone();
    apply_root_model(
        &mut global,
        &profiles,
        Flow::Standard,
        super::super::companion_chat::GLOBAL_CONVERSATION_ID,
    );
    assert_eq!(global.executor, explicit.executor);
    assert_eq!(global.account, explicit.account);
    assert_eq!(global.model, explicit.model);
    assert_eq!(global.reasoning, explicit.reasoning);
    let mut project = explicit;
    apply_root_model(&mut project, &profiles, Flow::Standard, &"a".repeat(32));
    assert_eq!(project.executor, crate::claude::Executor::Jarvis);
    assert_eq!(project.account, "configured-account");
    assert_eq!(project.model, "configured-model");
}

#[test]
fn global_companion_workflow_never_adds_project_tools() {
    let (_fixture, mut hub) = hub();
    let hub_mut = Arc::get_mut(&mut hub).unwrap();
    Arc::get_mut(&mut hub_mut.root).unwrap().id =
        crate::agent::companion_chat::GLOBAL_CONVERSATION_ID.to_owned();
    // Old manifests can still contain a project profile after reopening. Its
    // fallback is not an authorization to change the globally selected model.
    let mut retired: settings::ModelChoice = serde_json::from_value(json!({
        "executor":"agy","account":"","model":"gemini-3-pro","reasoning":"high"
    }))
    .unwrap();
    retired.fallback = Some(Box::new(settings::ModelChoice {
        executor: crate::claude::Executor::Claude,
        account: String::new(),
        model: "sonnet".into(),
        reasoning: None,
        fallback: None,
    }));
    hub_mut
        .manifest
        .lock()
        .unwrap()
        .profiles
        .insert(settings::key(Flow::Complete, Role::Builder), retired);
    let execution = Execution {
        hub,
        id: "main".into(),
        role: Role::Builder,
        flow: Flow::Complete,
        scope: vec![".".into()],
    };
    assert!(execution.secondary_model().unwrap().is_none());
    let mut definitions = crate::agent::tools::definitions(Mode::Build);
    definitions.extend(crate::agent::companion_chat::tools());
    execution.filter(&mut definitions);
    assert_eq!(definitions.len(), 5);
    assert!(definitions.iter().all(|definition| {
        crate::agent::companion_chat::allowed_tool(definition["name"].as_str().unwrap())
    }));
    for name in [
        "write",
        "bash",
        "browser_open",
        "http_send",
        "terminal_start",
        "hub_spawn",
        "mcp_activate",
        "jarvito_confirm_project",
    ] {
        assert!(!execution.allowed(name), "{name}");
    }
}

#[test]
fn all_roles_can_retrieve_project_knowledge() {
    for role in [
        Role::Planner,
        Role::Investigator,
        Role::Writer,
        Role::Orchestrator,
        Role::Builder,
        Role::Designer,
        Role::Reviewer,
        Role::Github,
        Role::Custom,
    ] {
        assert!(role.allows(Flow::Custom, crate::agent::knowledge::TOOL, false));
    }
}

#[tokio::test]
async fn secondary_model_is_optional_switches_once_and_updates_native_and_custom_workers() {
    for custom in [false, true] {
        let (fixture, hub) = hub();
        let mut task = job(&hub, Role::Builder, ".");
        let secondary = settings::ModelChoice {
            executor: crate::claude::Executor::Jarvis,
            account: "secondary-account".into(),
            model: "secondary-model".into(),
            reasoning: None,
            fallback: None,
        };
        let primary = settings::ModelChoice {
            executor: task.options.executor,
            account: task.options.account.clone(),
            model: task.options.model.clone(),
            reasoning: task.options.reasoning.clone(),
            fallback: Some(Box::new(secondary.clone())),
        };
        if custom {
            let mut agent = catalog::tests::example().agents.remove(0);
            agent.capability = catalog::Capability::WriteFiles;
            agent.model = Some(primary.clone());
            task.custom_agent = Some(agent);
        }
        hub.manifest
            .lock()
            .unwrap()
            .jobs
            .insert(task.id.clone(), task.clone());
        let execution = Execution {
            hub: hub.clone(),
            id: task.id.clone(),
            role: task.role,
            flow: if custom { Flow::Custom } else { Flow::Complete },
            scope: vec![".".into()],
        };
        let (session, signal) = storage::worker(&hub, &task, None).unwrap();
        let initial =
            "<!doctype html><html lang=\"pt-BR\"><body><h1>Salesforce CLI</h1></body></html>";
        let create = ToolCall {
            id: "create-presentation".into(),
            name: "write".into(),
            args: json!({"path":"index.html", "content":initial}),
            status: "running".into(),
            output: String::new(),
            duration_ms: 0,
        };
        assert!(execution.allowed(&create.name));
        let output =
            super::super::tools::execute(&fixture.root, &create, Mode::Build, signal.clone())
                .await
                .unwrap();
        session.update(true, |data| {
            data.turns.last_mut().unwrap().wire.extend([
                json!({"type":"function_call", "call_id":create.id, "name":create.name, "arguments":create.args.to_string()}),
                json!({"type":"function_call_output", "call_id":create.id, "output":output}),
            ]);
        }).unwrap();
        let exhausted = if custom {
            session.update_async(|data| {
                data.turns.last_mut().unwrap().wire.push(json!({
                    "type":"reasoning", "summary":[{"text":"The saved change is ready"}],
                    "_antigravity_model":task.options.model,
                    "_antigravity_part":{"thought":true,"text":"The saved change is ready","thoughtSignature":"signed-thought"},
                }));
            }).await.unwrap();
            session.flush_async().await.unwrap();
            let mut reminded = false;
            super::super::remind_antigravity_final_output(&session, &mut reminded)
                .await
                .unwrap();
            super::super::remind_antigravity_final_output(&session, &mut reminded)
                .await
                .unwrap_err()
        } else {
            AgentError::new("provider_retry_exhausted", "Provider unavailable")
        };
        if !custom {
            assert!(!super::super::model_fallback::recover(
                &session,
                &signal,
                Some(&execution),
                &exhausted
            )
            .await
            .unwrap());
            hub.manifest
                .lock()
                .unwrap()
                .profiles
                .insert(settings::key(Flow::Complete, task.role), primary);
        }
        assert_eq!(execution.secondary_model().unwrap(), Some(secondary));
        assert!(super::super::model_fallback::recover(
            &session,
            &signal,
            Some(&execution),
            &exhausted
        )
        .await
        .unwrap());
        assert_eq!(hub.job(&task.id).unwrap().options.model, "secondary-model");
        assert_eq!(
            session.data.lock().unwrap().turns[0].turn.options.model,
            "secondary-model"
        );
        let manifest = storage::load(&hub.directory, &hub.root.id)
            .unwrap()
            .unwrap();
        assert_eq!(manifest.jobs[&task.id].options.model, "secondary-model");
        assert!(!super::super::model_fallback::recover(
            &session,
            &signal,
            Some(&execution),
            &exhausted
        )
        .await
        .unwrap());
        // Simulate a crash after the journal switched, before the manifest did.
        let turn_id = session.data.lock().unwrap().turns[0].turn.id.clone();
        super::super::finish(&session, Err(exhausted));
        drop(session);
        task.recovery = Some(RecoveryCheckpoint::new(vec![]));
        assert_ne!(task.options.model, "secondary-model");
        let (resumed, signal) = storage::worker(&hub, &task, Some("Continue".into())).unwrap();
        {
            let data = resumed.data.lock().unwrap();
            assert_eq!(data.turns.len(), 1);
            assert_eq!(data.turns[0].turn.id, turn_id);
            assert_eq!(data.turns[0].turn.options.model, "secondary-model");
        }
        assert_eq!(
            std::fs::read_to_string(fixture.root.join("index.html")).unwrap(),
            initial
        );
        assert_eq!(
            resumed
                .input()
                .unwrap()
                .iter()
                .filter(|item| {
                    item["type"] == "function_call_output" && item["call_id"] == create.id
                })
                .count(),
            1
        );
        let extend = ToolCall {
            id: "complete-presentation".into(),
            name: "edit".into(),
            args: json!({"path":"index.html", "oldText":"</body>", "newText":"<section><h2>Comandos</h2><code>sf version</code></section></body>"}),
            status: "running".into(),
            output: String::new(),
            duration_ms: 0,
        };
        assert!(execution.allowed(&extend.name));
        super::super::tools::execute(&fixture.root, &extend, Mode::Build, signal)
            .await
            .unwrap();
        let completed = std::fs::read_to_string(fixture.root.join("index.html")).unwrap();
        assert!(completed.contains("<h1>Salesforce CLI</h1>"));
        assert!(completed.contains("<code>sf version</code>"));
        assert_eq!(completed.matches("<h1>").count(), 1);
    }
}

#[test]
fn github_recovery_tool_is_advertised_and_executable_in_direct_and_delegated_flows() {
    let mut recovered = 0;
    for flow in [Flow::Custom, Flow::Publication] {
        let (_fixture, hub) = hub();
        {
            let mut state = hub.manifest.lock().unwrap();
            state.flow = flow;
            if flow == Flow::Custom {
                state.custom_agent = Some(
                    catalog::tests::example()
                        .resolve_agent("builtin:github")
                        .unwrap(),
                );
            }
        }
        let exec = Execution {
            hub,
            id: "main".into(),
            role: Role::Github,
            flow,
            scope: vec![".".into()],
        };
        let mut watchdog = super::super::progress::Watchdog::default();
        let mut call = ToolCall {
            id: "recovery".into(),
            name: "read".into(),
            args: json!({"path":"README.md"}),
            status: "pending".into(),
            output: String::new(),
            duration_ms: 0,
        };
        for _ in 0..8 {
            watchdog.observe(&call, true, "missing path", false);
        }
        let mut definitions = super::super::tools::definitions(Mode::Build);
        definitions.push(watchdog.definition().unwrap());
        exec.filter(&mut definitions);
        call.name = "progress_checkpoint".into();
        call.args = json!({"objective":"Publish the requested pending changes", "evidence":["The relevant repositories were found"], "nextAction":"Inspect only the pending diff"});
        assert!(super::super::tool_contract::Catalog::new(&definitions)
            .validate(&call)
            .is_ok());
        assert!(exec.preflight(&call).is_none());
        assert!(watchdog.preflight(&call).is_ok());
        assert!(watchdog.checkpoint(&call.args).is_ok());
        recovered += 1;
    }
    super::super::evaluation::assert_runtime_report(
        "github-checkpoint-contract",
        super::super::evaluation::RuntimeReport::new(
            "completed",
            [("checkpointExecutions", recovered), ("roleRejections", 0)],
        ),
    );
}

#[test]
fn recovery_contract_is_available_to_every_native_role_and_custom_capability() {
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
        Role::Custom,
    ] {
        for flow in [
            Flow::Standard,
            Flow::Designer,
            Flow::Planned,
            Flow::Complete,
            Flow::Publication,
            Flow::Custom,
        ] {
            assert!(role.allows(flow, "progress_checkpoint", false));
        }
    }
    for capability in [
        catalog::Capability::ReadOnly,
        catalog::Capability::WriteFiles,
        catalog::Capability::Commands,
    ] {
        let mut agent = catalog::tests::example().agents.remove(0);
        agent.capability = capability;
        assert!(custom::allowed(&agent, "progress_checkpoint"));
    }
}

#[test]
fn workflow_tools_keep_narrow_follow_ups_and_checks_proportional() {
    let planner = dispatch::definitions(Flow::Planned, Role::Planner);
    let spawn = planner
        .iter()
        .find(|tool| tool["name"] == "hub_spawn")
        .unwrap();
    let spawn_description = spawn["description"].as_str().unwrap();
    assert!(spawn_description.contains("dispatch one worker directly"));
    assert!(spawn_description.contains("do not pre-read source files or runbooks"));

    let builder = dispatch::definitions(Flow::Planned, Role::Builder);
    let check = builder
        .iter()
        .find(|tool| tool["name"] == "workflow_check")
        .unwrap();
    let check_description = check["description"].as_str().unwrap();
    assert!(check_description.contains("when source changed"));
    assert!(check_description.contains("no source edit"));
}

#[test]
fn mandatory_context_retrieval_is_available_in_every_role_flow_and_scope() {
    for flow in [
        Flow::Standard,
        Flow::Designer,
        Flow::Planned,
        Flow::Complete,
        Flow::Publication,
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
    for broad in [true, false] {
        for name in ["ctx_search", "ctx_index", "ctx_stats"] {
            assert!(Role::Github.allows(Flow::Publication, name, broad));
            assert!(Role::Github.allows(Flow::Custom, name, broad));
        }
    }
    assert!(!Role::Github.allows(Flow::Complete, "ctx_search", true));
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
        executor: crate::claude::Executor::Jarvis,
        account: "root-account".into(),
        model: "root-model".into(),
        reasoning: Some("high".into()),
        mode: Mode::Build,
        workflow: Some(Flow::Complete),
        custom_workflow_id: None,
        custom_agent_id: None,
        approval_mode: ApprovalMode::Manual,
        manual_validation: false,
        automatic_publication: None,
    };
    let signal = root
        .reserve("Implement the requested outcome".into(), options.clone())
        .unwrap();
    let directory = fixture.root.join("workflow");
    std::fs::create_dir(&directory).unwrap();
    let manifest = Manifest {
        worker_interruptions: BTreeMap::new(),
        custom_cursor: None,
        custom_definition: None,
        custom_agent: None,
        validation: None,
        root_recovery: None,
        publication_baseline: None,
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
            grants: execution_grants::GrantStore::default(),
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
        custom_step_id: None,
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
        recovery_attempts: 0,
        recovery_attempt_pending: false,
        handoff: None,
        error: None,
        recovery: None,
        options: hub.manifest.lock().unwrap().options.clone(),
    }
}

#[test]
fn harness_evaluation_workflow_context_keeps_relevant_agents_without_replaying_unrelated_history() {
    for flow in [Flow::Planned, Flow::Complete, Flow::Custom] {
        let (_fixture, hub) = hub();
        let parent = job(&hub, Role::Orchestrator, ".");
        let mut worker = job(&hub, Role::Designer, "frontend");
        worker.parent_id = parent.id.clone();
        let mut dependency = job(&hub, Role::Builder, "backend");
        dependency.run_id = "previous-run".into();
        worker.dependencies.push(dependency.id.clone());
        let mut child = job(&hub, Role::Investigator, "frontend");
        child.parent_id = worker.id.clone();
        let sibling = job(&hub, Role::Builder, "unrelated");
        let mut historical = job(&hub, Role::Reviewer, ".");
        historical.run_id = "previous-run".into();
        {
            let mut state = hub.manifest.lock().unwrap();
            state.flow = flow;
            for item in [&parent, &worker, &dependency, &child, &sibling, &historical] {
                state.jobs.insert(item.id.clone(), item.clone());
            }
        }
        let exec = Execution {
            hub: hub.clone(),
            id: worker.id.clone(),
            role: worker.role,
            flow,
            scope: worker.scope.clone(),
        };
        let before = exec.context().unwrap();
        for relevant in [&parent, &worker, &dependency, &child] {
            assert!(before.contains(&relevant.id));
        }
        for unrelated in [&sibling, &historical] {
            assert!(!before.contains(&unrelated.id));
        }
        hub.manifest
            .lock()
            .unwrap()
            .jobs
            .get_mut(&sibling.id)
            .unwrap()
            .status = Status::Completed;
        assert_eq!(exec.context().unwrap(), before);
        let root = Execution {
            id: "main".into(),
            role: Role::Planner,
            ..exec
        };
        let root_context = root.context().unwrap();
        assert!(root_context.contains(&sibling.id));
        assert!(!root_context.contains(&historical.id));
        assert_eq!(hub.manifest.lock().unwrap().jobs.len(), 6);
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
        executor: crate::claude::Executor::Jarvis,
        account: "specialist-account".into(),
        model: "specialist-model".into(),
        reasoning: Some("high".into()),
        fallback: None,
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
fn video_flow_has_native_commands_and_tasks_without_coordinated_agents() {
    let (_fixture, hub) = hub();
    let direct = Execution {
        hub,
        id: "main".into(),
        role: Role::Video,
        flow: Flow::Video,
        scope: vec![".".into()],
    };
    let mut definitions = tools::definitions(Mode::Build);
    definitions.extend(crate::agent::video::definitions(Mode::Build));
    definitions.extend(crate::core::beads::definitions(false));
    definitions.push(super::super::tasks::definition());
    direct.filter(&mut definitions);
    for name in [
        "ask_user",
        "write",
        "video_docs",
        "video_run",
        "video_wait",
        "video_cancel",
        "update_tasks",
    ] {
        assert!(
            definitions.iter().any(|tool| tool["name"] == name),
            "missing {name}"
        );
    }
    assert!(definitions.iter().all(|tool| {
        let name = tool["name"].as_str().unwrap();
        !name.starts_with("hub_") && !name.starts_with("beads_")
    }));
    assert_eq!(Flow::Video.root(), Role::Video);
    assert!(Flow::Video.direct());
    assert_eq!(Flow::Video.roster(), &[Role::Video]);
    assert!(Flow::Video.delegations().is_empty());
    assert!(!Flow::Planned.roster().contains(&Role::Video));
    assert!(!Flow::Complete.roster().contains(&Role::Video));
    assert!(settings::validate(Flow::Video, &BTreeMap::new()).is_ok());
    assert!(Role::Video.contract().contains("video_docs"));
}

#[test]
fn video_commands_require_command_capability_while_docs_allow_every_role() {
    use catalog::Capability;
    for role in [
        Role::Planner,
        Role::Investigator,
        Role::Writer,
        Role::Orchestrator,
        Role::Builder,
        Role::Designer,
        Role::Video,
        Role::Reviewer,
        Role::Github,
        Role::Custom,
    ] {
        assert!(role.allows(Flow::Custom, "video_docs", false));
        for name in ["video_run", "video_wait", "video_cancel"] {
            assert_eq!(
                role.allows(Flow::Custom, name, true),
                matches!(role, Role::Builder | Role::Designer | Role::Video)
            );
        }
    }
    for capability in [
        Capability::ReadOnly,
        Capability::WriteFiles,
        Capability::Commands,
    ] {
        assert!(custom::capability_allows(capability, "video_docs"));
        for name in ["video_run", "video_wait", "video_cancel"] {
            assert_eq!(
                custom::capability_allows(capability, name),
                capability == Capability::Commands
            );
        }
    }
    assert!(recovery_inspection_tool("video_docs", false));
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
        "browser_attach",
        "browser_evaluate",
        "browser_devtools",
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
        "browser_wait",
        "browser_console",
        "browser_screenshot",
        "browser_discover",
        "browser_network",
        "browser_response_body",
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
        for tool in [
            "write",
            "edit",
            "apply_patch",
            "bash",
            "http_send",
            "http_save_request",
        ] {
            assert!(!role.allows(Flow::Complete, tool, true));
        }
        assert!(role.allows(Flow::Complete, "http_requests", true));
        assert!(role.allows(Flow::Complete, "http_result", true));
    }
    assert!(Role::Builder.allows(Flow::Planned, "write", false));
    assert!(Role::Builder.allows(Flow::Planned, "apply_patch", false));
    assert!(!Role::Builder.allows(Flow::Planned, "bash", false));
    assert!(!Role::Builder.allows(Flow::Planned, "mcp_mutation", false));
    assert!(Role::Reviewer.allows(Flow::Complete, "workflow_check", true));
    assert!(Role::Builder.allows(Flow::Planned, "http_send", true));
    assert!(!Role::Builder.allows(Flow::Planned, "http_send", false));
    assert!(Role::Github.allows(Flow::Publication, "http_send", true));
    assert!(!custom::capability_allows(
        catalog::Capability::ReadOnly,
        "http_send"
    ));
    assert!(!custom::capability_allows(
        catalog::Capability::WriteFiles,
        "http_send"
    ));
    assert!(custom::capability_allows(
        catalog::Capability::Commands,
        "http_send"
    ));
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
    drop(a);
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
fn recovery_resumes_failed_root_and_failed_worker_from_their_journals() {
    let (_fixture, hub) = hub();
    let mut failed = job(&hub, Role::Builder, "src");
    failed.status = Status::Running;
    let (failed_session, _) = storage::worker(&hub, &failed, None).unwrap();
    finish(
        &failed_session,
        Err(AgentError::new(
            "provider_retry_exhausted",
            "A conexão com o provedor falhou.",
        )),
    );
    failed.status = Status::Failed;
    failed.error = Some("A conexão com o provedor falhou.".into());
    hub.mutate(|state| {
        state.root_status = Status::Failed;
        state.jobs.insert(failed.id.clone(), failed.clone());
        Ok(())
    })
    .unwrap();

    let loaded = storage::load(&hub.directory, &hub.root.id)
        .unwrap()
        .unwrap();
    let (recovered, resumed) =
        storage::prepare_recovery(&hub.directory, loaded, Flow::Complete, "run", vec![]).unwrap();

    assert_eq!(recovered.root_status, Status::Running);
    assert_eq!(recovered.jobs[&failed.id].status, Status::Queued);
    assert!(recovered.jobs[&failed.id].error.is_none());
    assert_eq!(resumed.len(), 1);
    assert_eq!(resumed[0].id, failed.id);
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
async fn recovered_agents_resolve_the_specific_effect_without_blocking_independent_work() {
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
    std::fs::create_dir_all(hub.root.root.join("src")).unwrap();
    std::fs::write(hub.root.root.join("src/app.ts"), "updated").unwrap();
    hub.root.update(true, |data| {
        let turn = data.turns.last_mut().unwrap();
        turn.turn.steps.push(Step { tools: vec![write.clone()], ..Step::default() });
        turn.wire.push(json!({"type":"function_call", "call_id":write.id, "name":write.name, "arguments":write.args.to_string()}));
    }).unwrap();
    assert_eq!(
        execution
            .recovery_preflight(&write, false)
            .unwrap_err()
            .code,
        "recovery_inspection_required"
    );
    let independent = ToolCall {
        args: json!({"path":"src/other.ts","content":"independent"}),
        ..write.clone()
    };
    assert!(execution.recovery_preflight(&independent, false).is_ok());

    let read = ToolCall {
        id: "read-2".into(),
        name: "read".into(),
        args: json!({"path":"src/app.ts"}),
        status: "completed".into(),
        output: "updated".into(),
        duration_ms: 1,
    };
    hub.root
        .update(true, |data| {
            data.turns.last_mut().unwrap().wire.push(
                json!({"type":"function_call_output","call_id":read.id,"output":read.output}),
            );
        })
        .unwrap();
    execution
        .observe_recovery_result(&read, false, true, |_| None)
        .unwrap();
    assert_eq!(
        execution
            .recovery_preflight(&write, false)
            .unwrap_err()
            .code,
        "recovery_inspection_required"
    );
    execution
        .resolve_recovery(&json!({"callId":write.id,"evidenceCallId":read.id,"outcome":"applied"}))
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
    assert_eq!(
        execution
            .recovery_preflight(&write, false)
            .unwrap_err()
            .code,
        "recovery_already_applied"
    );
    assert!(execution.recovery_preflight(&independent, false).is_ok());
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
            executor: crate::claude::Executor::Jarvis,
            account: "review-account".into(),
            model: "gpt-5.6-sol".into(),
            reasoning: Some("xhigh".into()),
            fallback: None,
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
