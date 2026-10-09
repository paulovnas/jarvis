use super::*;
use std::fs;

mod mcp_continuation;

#[cfg(unix)]
#[tokio::test]
async fn completion_hook_can_request_work_then_release_without_replaying_startup() {
    let fixture = Fixture::new();
    let session = session(&fixture);
    let signal = session
        .reserve("Entregar resultado".into(), options(ApprovalMode::Yolo))
        .unwrap();
    let hooks = manual_hook_runtime(&fixture,&session,crate::hooks::Event::Stop,
        "if grep -q '\"stop_hook_active\":false'; then printf '%s' '{\"decision\":\"block\",\"reason\":\"Validate the result before completion.\"}'; else printf '%s' '{}'; fi","");
    let mut continuations = 0;
    assert!(check_completion_hooks(
        &session,
        &hooks,
        None,
        "Primeiro resultado",
        &mut continuations,
        signal.clone()
    )
    .await
    .unwrap());
    assert!(session
        .data
        .lock()
        .unwrap()
        .turns
        .last()
        .unwrap()
        .wire
        .iter()
        .any(
            |message| message["content"].as_str().is_some_and(|text| text
                .contains("Validate the result before completion.")
                && text.contains("grants no permissions"))
        ));
    assert!(!check_completion_hooks(
        &session,
        &hooks,
        None,
        "Resultado validado",
        &mut continuations,
        signal
    )
    .await
    .unwrap());
}

#[cfg(unix)]
#[tokio::test]
async fn completion_hook_cannot_loop_without_limit() {
    let fixture = Fixture::new();
    let session = session(&fixture);
    let signal = session
        .reserve("Entregar resultado".into(), options(ApprovalMode::Yolo))
        .unwrap();
    let hooks = manual_hook_runtime(
        &fixture,
        &session,
        crate::hooks::Event::Stop,
        "cat >/dev/null; printf '%s' '{\"decision\":\"block\",\"reason\":\"More work.\"}'",
        "",
    );
    let mut continuations = 0;
    for _ in 0..3 {
        assert!(check_completion_hooks(
            &session,
            &hooks,
            None,
            "Resultado",
            &mut continuations,
            signal.clone()
        )
        .await
        .unwrap());
    }
    let failure = check_completion_hooks(
        &session,
        &hooks,
        None,
        "Resultado",
        &mut continuations,
        signal,
    )
    .await
    .unwrap_err();
    assert_eq!(failure.code, "hook_completion_limit");
}

#[cfg(unix)]
#[tokio::test]
async fn interrupted_turn_runs_interrupt_but_session_end_waits_for_shutdown() {
    let fixture = Fixture::new();
    let session = session(&fixture);
    session
        .reserve("Teste de interrupção".into(), options(ApprovalMode::Yolo))
        .unwrap();
    let interrupted = fixture.root.join("interrupted");
    let ended = fixture.root.join("ended");
    manual_hook_runtime(
        &fixture,
        &session,
        crate::hooks::Event::Interrupt,
        &format!("cat >/dev/null; touch '{}'", interrupted.display()),
        "",
    );
    let state = AppState::default();
    crate::hooks::upsert(
        &state,
        &fixture.root,
        crate::hooks::Hook {
            id: "f".repeat(32),
            name: "Encerrar sessão".into(),
            event: crate::hooks::Event::SessionEnd,
            command: format!("cat >/dev/null; touch '{}'", ended.display()),
            matcher: String::new(),
            timeout_seconds: 5,
            enabled: true,
        },
        1,
    )
    .unwrap();
    let agent = AgentState::default();
    let oauth = OpenAiCodexState::default();
    let mcp = crate::mcp::McpState::default();
    let (_sender, signal) = watch::channel(true);
    assert!(run_turn(
        &session,
        TurnRuntime {
            grants: &agent.grants,
            state: &state,
            oauth: &oauth,
            mcp: &mcp,
            home: &fixture.root
        },
        signal,
        None
    )
    .await
    .is_err());
    assert!(interrupted.exists());
    assert!(!ended.exists());
    agent
        .sessions
        .lock()
        .unwrap()
        .insert(session.id.clone(), session);
    agent.finish_hook_sessions(&fixture.root);
    assert!(ended.exists());
}

#[cfg(unix)]
pub(super) fn manual_hook_runtime(
    fixture: &Fixture,
    session: &Session,
    event: crate::hooks::Event,
    command: &str,
    matcher: &str,
) -> crate::hooks::runtime::Runtime {
    crate::hooks::upsert(
        &AppState::default(),
        &fixture.root,
        crate::hooks::Hook {
            id: "0123456789abcdef0123456789abcdef".into(),
            name: "Teste".into(),
            event,
            command: command.into(),
            matcher: matcher.into(),
            timeout_seconds: 5,
            enabled: true,
        },
        0,
    )
    .unwrap();
    let turn_id = session
        .data
        .lock()
        .unwrap()
        .turns
        .last()
        .unwrap()
        .turn
        .id
        .clone();
    crate::hooks::runtime::Runtime::load(&fixture.root, &fixture.root, &session.id, &turn_id)
        .unwrap()
}

#[cfg(unix)]
#[tokio::test]
async fn permission_hook_can_deny_without_creating_or_approving_a_human_request() {
    let fixture = Fixture::new();
    let session = session(&fixture);
    let approval_options = options(ApprovalMode::Manual);
    let signal = session
        .reserve("Publicar".into(), approval_options.clone())
        .unwrap();
    let hooks = manual_hook_runtime(
        &fixture,
        &session,
        crate::hooks::Event::PermissionRequest,
        "cat >/dev/null; printf '%s' 'hook bloqueou' >&2; exit 2",
        "bash",
    );
    assert!(hooks.has_blocking_tool_hooks("bash"));
    assert!(!hooks.has_blocking_tool_hooks("read"));
    let tool = ToolCall {
        id: "publish".into(),
        name: "bash".into(),
        args: json!({"command":"touch forbidden"}),
        status: "pending".into(),
        output: String::new(),
        duration_ms: 0,
    };
    let error = authorize_declared(
        ApprovalRequest {
            session: &session,
            tool: &tool,
            options: &approval_options,
            policy: None,
            sandbox: None,
            project_id: None,
            manual_hooks: Some(&hooks),
            explicit_video_approval: false,
            signal,
        },
        tool_contract::ApprovalPolicy::AccordingToTurn,
        tool_contract::Handler::Native,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "hook_denied");
    assert!(error.message.contains("hook bloqueou"));
    assert!(session.snapshot().unwrap().pending_approval.is_none());
    assert!(!fixture.root.join("forbidden").exists());
}

#[cfg(unix)]
#[tokio::test]
async fn mandatory_authoring_reviews_emit_permission_hooks_even_in_yolo() {
    for approval_mode in [ApprovalMode::Manual, ApprovalMode::Yolo] {
        for name in [
            "jarvis_propose_agent",
            "jarvis_propose_flow",
            "jarvis_propose_mcp",
            "jarvis_propose_hook",
            "jarvis_propose_plugin",
            "jarvis_propose_project_instructions",
        ] {
            let fixture = Fixture::new();
            let session = session(&fixture);
            let selected = options(approval_mode);
            let signal = session
                .reserve("Configurar o Jarvis".into(), selected.clone())
                .unwrap();
            let hooks = manual_hook_runtime(
                &fixture,
                &session,
                crate::hooks::Event::PermissionRequest,
                "printf '%s' 'review blocked' >&2; exit 2",
                name,
            );
            let tool = ToolCall {
                id: "proposal".into(),
                name: name.into(),
                args: json!({"summary":"x".repeat(512 * 1024)}),
                status: "pending".into(),
                output: String::new(),
                duration_ms: 0,
            };
            let error = authorize_declared(
                ApprovalRequest {
                    session: &session,
                    tool: &tool,
                    options: &selected,
                    policy: None,
                    sandbox: None,
                    project_id: None,
                    manual_hooks: Some(&hooks),
                    explicit_video_approval: false,
                    signal,
                },
                tool_contract::ApprovalPolicy::Never,
                tool_contract::Handler::JarvisAuthoring,
            )
            .await
            .unwrap_err();
            assert_eq!(error.code, "hook_denied");
            assert_eq!(error.message, "review blocked");
            let snapshot = session.snapshot().unwrap();
            assert!(snapshot.pending_approval.is_none());
            assert!(snapshot.pending_authoring.is_none());
            assert!(snapshot.turns[0]
                .steps
                .iter()
                .all(|step| step.tools.is_empty()));
        }
    }
}

#[cfg(unix)]
#[tokio::test]
async fn post_hook_failure_preserves_checkpoint_and_context_is_untrusted() {
    let fixture = Fixture::new();
    let session = session(&fixture);
    let signal = session
        .reserve("Implementar".into(), options(ApprovalMode::Yolo))
        .unwrap();
    let hooks = manual_hook_runtime(
        &fixture,
        &session,
        crate::hooks::Event::PostToolUse,
        "cat >/dev/null; printf '%s' 'diagnóstico' >&2; exit 1",
        "write",
    );
    let tool = ToolCall {
        id: "write-1".into(),
        name: "write".into(),
        args: json!({"path":"saved.txt"}),
        status: "running".into(),
        output: String::new(),
        duration_ms: 0,
    };
    session
        .update(true, |data| {
            data.turns.last_mut().unwrap().turn.steps.push(Step {
                tools: vec![tool.clone()],
                ..Step::default()
            })
        })
        .unwrap();
    core_runtime::checkpoint_tool(&session, &tool, "Arquivo salvo.", "completed", 3, None)
        .await
        .unwrap();
    assert!(run_manual_hook(&session, &hooks, crate::hooks::Event::PostToolUse, json!({"tool_name":tool.name,"tool_input":tool.args,"tool_response":"Arquivo salvo.","tool_use_id":tool.id}), signal).await.unwrap().is_none());
    session.flush().unwrap();
    let (loaded, _) = journal::read_only(&session.journal).unwrap();
    let turn = &loaded[0];
    assert_eq!(turn.turn.steps[0].tools[0].status, "completed");
    assert_eq!(turn.turn.steps[0].tools[0].output, "Arquivo salvo.");
    assert!(turn
        .wire
        .iter()
        .any(|entry| entry["_jarvis_manual_hook"] == "PostToolUse"
            && entry["content"]
                .as_str()
                .unwrap()
                .contains("untrusted reference data")));
    assert!(journal::uncertain_tool_names(turn).is_empty());
    assert!(journal::safe_to_resume(turn));
    assert_eq!(turn.turn.steps[0].tools.len(), 1);
    assert!(turn.turn.steps[0].core_activities.iter().any(|activity| {
        activity.action == "PostToolUse"
            && activity.status == crate::core::activity::Status::Unavailable
            && activity.component == crate::core::activity::ActivityComponent::Hooks
            && activity.resource_id.is_some()
            && activity.resource_name.is_some()
    }));
}

pub(super) struct Fixture {
    pub root: PathBuf,
}
impl Fixture {
    pub fn new() -> Self {
        let root =
            std::env::temp_dir().join(format!("jarvis-agent-{}", library::new_id().unwrap()));
        fs::create_dir(&root).unwrap();
        Self {
            root: fs::canonicalize(root).unwrap(),
        }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

pub(super) fn options(approval_mode: ApprovalMode) -> TurnOptions {
    TurnOptions {
        executor: crate::claude::Executor::Jarvis,
        account: "account".into(),
        model: "model".into(),
        reasoning: None,
        service_tier: None,
        mode: Mode::Build,
        workflow: None,
        custom_workflow_id: None,
        custom_agent_id: None,
        approval_mode,
        manual_validation: false,
        automatic_publication: None,
        model_selection: None,
    }
}

#[test]
fn publication_authoring_catalog_exposes_only_metadata_and_build_mcp_registration() {
    for (mode, expected) in [
        (Mode::Build, vec!["jarvis_catalog", "jarvis_propose_mcp"]),
        (Mode::Plan, vec!["jarvis_catalog"]),
    ] {
        let definitions = authoring_tools_for_turn(mode, true);
        let names: Vec<_> = definitions
            .iter()
            .map(|definition| definition["name"].as_str().unwrap())
            .collect();
        assert_eq!(names, expected);
    }
}

#[test]
fn normal_authoring_preserves_agent_and_flow_tools_with_build_only_mcp_registration() {
    for mode in [Mode::Plan, Mode::Build] {
        let definitions = authoring_tools_for_turn(mode, false);
        for name in [
            "jarvis_catalog",
            "jarvis_propose_agent",
            "jarvis_propose_flow",
        ] {
            assert!(definitions
                .iter()
                .any(|definition| definition["name"] == name));
        }
        assert_eq!(
            definitions
                .iter()
                .any(|definition| definition["name"] == "jarvis_propose_mcp"),
            mode == Mode::Build
        );
        assert_eq!(
            definitions
                .iter()
                .any(|definition| definition["name"] == "jarvis_propose_hook"),
            mode == Mode::Build
        );
        assert_eq!(
            definitions
                .iter()
                .any(|definition| { definition["name"] == "jarvis_propose_project_instructions" }),
            mode == Mode::Build
        );
    }
}

#[test]
fn custom_agent_turns_are_direct_while_custom_graphs_remain_coordinated() {
    let mut selected = options(ApprovalMode::Yolo);
    selected.workflow = Some(workflow::Flow::Custom);
    selected.custom_agent_id = Some("a".repeat(32));
    assert!(selected.direct());
    selected.custom_agent_id = None;
    selected.custom_workflow_id = Some("b".repeat(32));
    assert!(!selected.direct());
}

#[test]
fn mcp_failures_keep_a_readable_history_and_a_structured_provider_result() {
    let mut failure = crate::mcp::coded_error(
        "mcp_invalid_arguments",
        "MCP 'docs', ferramenta 'lookup': argumentos inválidos.",
    );
    failure.metadata.server = Some("docs".into());
    failure.metadata.tool = Some("lookup".into());
    failure.metadata.validation_errors = vec![crate::mcp::McpValidationIssue {
        path: "$.query".into(),
        keyword: "type".into(),
        message: "tipo inválido; esperado texto".into(),
    }];

    let (history, status, provider_result) =
        settle_tool_result(Err(AgentError::from(failure))).unwrap();

    assert_eq!(status, "error");
    assert_eq!(
        history,
        "MCP 'docs', ferramenta 'lookup': argumentos inválidos."
    );
    let provider_result: Value =
        serde_json::from_str(&provider_result.expect("structured MCP failure")).unwrap();
    assert_eq!(provider_result["ok"], false);
    assert_eq!(provider_result["error"]["code"], "mcp_invalid_arguments");
    assert_eq!(
        provider_result["error"]["validationErrors"][0]["path"],
        "$.query"
    );
}

#[test]
fn a_new_user_turn_inherits_the_latest_durable_mcp_intent() {
    let fixture = Fixture::new();
    let session = session(&fixture);
    session
        .reserve(
            "Use o MCP Gemini Notebook.".into(),
            options(ApprovalMode::Yolo),
        )
        .unwrap();
    let intent = crate::mcp::McpIntent {
        mode: crate::mcp::McpIntentMode::Explicit,
        servers: vec![crate::mcp::McpIntentServer {
            id: "notebook-id".into(),
            name: "gemini-notebook-mcp".into(),
        }],
        ..crate::mcp::McpIntent::default()
    };
    session
        .update(true, |data| {
            data.turns.last_mut().unwrap().mcp_intent = Some(intent.clone());
        })
        .unwrap();
    finish(&session, Ok(()));
    session
        .reserve(
            "Continue e confirme a informação.".into(),
            options(ApprovalMode::Yolo),
        )
        .unwrap();

    let data = session.data.lock().unwrap();
    let (turn_id, inherited, unresolved, auxiliary_count) =
        pending_mcp_intent_resolution(&data.turns, &data.inherited_mcp_intent)
            .expect("new turn needs resolution");
    assert_eq!(turn_id, data.turns.last().unwrap().turn.id);
    assert_eq!(inherited, intent);
    assert_eq!(unresolved, vec!["Continue e confirme a informação."]);
    assert_eq!(auxiliary_count, 0);
}

#[test]
fn global_jarvito_chat_keeps_its_title() {
    let fixture = Fixture::new();
    let session = session_with_id(&fixture, companion_chat::GLOBAL_CONVERSATION_ID);
    session
        .reserve(
            "Lembre: sempre use o Select do projeto e exiba o label.".into(),
            options(ApprovalMode::Yolo),
        )
        .unwrap();

    assert!(title_request(&session).is_none());
}

#[tokio::test]
async fn live_user_mcp_guidance_updates_durable_intent_once_without_replaying_results() {
    let fixture = Fixture::new();
    let session = session(&fixture);
    let state = AppState::default();
    let mcp = crate::mcp::McpState::default();
    state.with_connection(&fixture.root, |db| {
        db.execute("INSERT INTO mcp_servers (id,name,kind,enabled,configured,revision) VALUES ('voice-id','voicestudio','local',1,1,1)", [])?;
        Ok::<_, crate::mcp::McpError>(())
    }).unwrap();
    let signal = session
        .reserve("Criar a apresentação".into(), options(ApprovalMode::Yolo))
        .unwrap();
    preserve_user_mcp_intent(&session, &mcp, &state, &fixture.root, signal.clone())
        .await
        .unwrap();
    session
        .update(true, |data| {
            data.turns.last_mut().unwrap().wire.push(
                json!({"type":"function_call_output", "call_id":"confirmed", "output":"preserved"}),
            );
        })
        .unwrap();
    let guidance = "Se puder usar a narração usando mcp do voicestudio por favor, la o audio vem melhor e mais bonito";
    session
        .submit(guidance.into(), options(ApprovalMode::Yolo))
        .unwrap();
    let id = session.snapshot().unwrap().queued_messages[0].id.clone();
    session.promote_queued(&id).unwrap();
    queue::inject_pending_auxiliary(&session, &fixture.root)
        .await
        .unwrap();
    preserve_user_mcp_intent(&session, &mcp, &state, &fixture.root, signal.clone())
        .await
        .unwrap();
    let (turns, _) = journal::load_all(&session.journal).unwrap();
    let current = turns.last().unwrap();
    let intent = current.mcp_intent.as_ref().unwrap();
    assert_eq!(intent.mode, crate::mcp::McpIntentMode::Explicit);
    assert_eq!(intent.servers[0].id, "voice-id");
    assert_eq!(current.mcp_intent_auxiliary_count, 1);
    assert_eq!(current.turn.auxiliary_messages[0].content, guidance);
    assert!(current
        .wire
        .iter()
        .any(|item| item["call_id"] == "confirmed" && item["output"] == "preserved"));
    let revision = session.snapshot().unwrap().revision;
    preserve_user_mcp_intent(&session, &mcp, &state, &fixture.root, signal)
        .await
        .unwrap();
    assert_eq!(session.snapshot().unwrap().revision, revision);
}

#[test]
fn a_running_first_turn_is_immediately_available_for_title_generation() {
    let fixture = Fixture::new();
    let session = session(&fixture);
    session
        .reserve(
            "Planeje uma migração longa sem bloquear o título.".into(),
            options(ApprovalMode::Yolo),
        )
        .unwrap();

    let request = title_request(&session).expect("running turn should be eligible");

    assert_eq!(
        request.message,
        "Planeje uma migração longa sem bloquear o título."
    );
    assert_eq!(
        session.data.lock().unwrap().turns[0].turn.status,
        TurnStatus::Running
    );
}

#[test]
fn a_claude_first_turn_can_save_a_local_title_before_the_answer_without_replacing_manual_names() {
    let fixture = Fixture::new();
    let state = AppState::default();
    let session = session(&fixture);
    let mut claude = options(ApprovalMode::Yolo);
    claude.executor = crate::claude::Executor::Claude;
    claude.account.clear();
    claude.model = "sonnet".into();
    session
        .reserve("Verificar o MCP do database".into(), claude)
        .unwrap();
    let request = title_request(&session).unwrap();
    let local = title::local(&request.message).unwrap();
    assert_eq!(request.options.executor, crate::claude::Executor::Claude);
    assert_eq!(
        session.snapshot().unwrap().turns[0].status,
        TurnStatus::Running
    );
    state.with_connection(&fixture.root, |db| {
        db.execute("INSERT INTO workspaces (id,name) VALUES ('w','Workspace')", [])?;
        db.execute("INSERT INTO projects (id,workspace_id,name,path) VALUES ('p','w','Project',?1)", [fixture.root.to_string_lossy()])?;
        db.execute("INSERT INTO conversations (id,project_id,title,title_source) VALUES ('conversation','p','Nova Conversa','default')", [])?;
        Ok::<_, library::LibraryError>(())
    }).unwrap();
    assert!(library::save_generated_title(&state, &fixture.root, "conversation", &local).unwrap());
    assert_eq!(
        library::notification_names(&state, &fixture.root, "conversation")
            .unwrap()
            .1,
        local
    );
    state.with_connection(&fixture.root, |db| {
        db.execute("UPDATE conversations SET display_title='Nome escolhido',title_source='manual' WHERE id='conversation'", [])?;
        Ok::<_, library::LibraryError>(())
    }).unwrap();
    assert!(!library::save_generated_title(&state, &fixture.root, "conversation", &local).unwrap());
    assert_eq!(
        library::notification_names(&state, &fixture.root, "conversation")
            .unwrap()
            .1,
        "Nome escolhido"
    );
}

#[tokio::test]
async fn failed_title_credentials_return_a_local_title_without_switching_the_chat_model() {
    let fixture = Fixture::new();
    let state = AppState::default();
    let request = TitleRequest {
        message: "Revisar integração com Salesforce".into(),
        options: options(ApprovalMode::Yolo),
    };
    let generated = title_text(
        "conversation",
        &request,
        &state,
        &OpenAiCodexState::default(),
        &fixture.root,
    )
    .await;
    assert!(generated.is_none());
    assert_eq!(
        title::resolve(generated.as_deref(), &request.message),
        Some(request.message.clone())
    );
    assert_eq!(request.options.account, "account");
    assert_eq!(request.options.model, "model");
}

#[test]
fn title_generation_guard_deduplicates_and_allows_retry_after_completion() {
    let state = AgentState::default();
    let first = state
        .begin_title_generation("conversation")
        .unwrap()
        .expect("first request should acquire the guard");

    assert!(state
        .begin_title_generation("conversation")
        .unwrap()
        .is_none());

    drop(first);

    assert!(state
        .begin_title_generation("conversation")
        .unwrap()
        .is_some());
}

#[tokio::test]
async fn antigravity_thought_only_recovery_is_bounded_and_journals_after_confirmed_output() {
    let fixture = Fixture::new();
    let session = session(&fixture);
    session
        .reserve("Explique o resultado".into(), options(ApprovalMode::Yolo))
        .unwrap();
    let thought = json!({
        "type":"reasoning", "summary":[{"text":"The result is ready"}],
        "_antigravity_model":"gemini-3.8-flash",
        "_antigravity_part":{"thought":true,"text":"The result is ready","thoughtSignature":"signed-thought"},
    });
    session
        .update_async(|data| {
            data.turns.last_mut().unwrap().wire.push(thought.clone());
        })
        .await
        .unwrap();
    session.flush_async().await.unwrap();
    let mut reminded = false;
    remind_antigravity_final_output(&session, &mut reminded)
        .await
        .unwrap();
    session.flush_async().await.unwrap();
    let replay = history::HistoryState::default()
        .load_replay(&session.journal, &fixture.root)
        .unwrap();
    let input = &replay.turns.last().unwrap().wire;
    assert_eq!(input[input.len() - 2], thought);
    assert_eq!(input.last().unwrap()["_jarvis_runtime"], true);
    assert!(input.last().unwrap()["content"]
        .as_str()
        .unwrap()
        .contains("do not repeat completed actions"));
    let before = input.len();
    assert_eq!(
        remind_antigravity_final_output(&session, &mut reminded)
            .await
            .unwrap_err()
            .code,
        "provider_retry_exhausted"
    );
    assert_eq!(
        session
            .data
            .lock()
            .unwrap()
            .turns
            .last()
            .unwrap()
            .wire
            .len(),
        before
    );
}

pub(super) fn session(fixture: &Fixture) -> Arc<Session> {
    session_with_id(fixture, "conversation")
}

pub(super) fn session_with_id(fixture: &Fixture, id: &str) -> Arc<Session> {
    let journal = fixture.root.join("session.jsonl");
    fs::write(&journal, "{}\n").unwrap();
    let writer = session_writer::SessionWriter::start(journal.clone(), id.into(), None).unwrap();
    Arc::new(Session {
        id: id.into(),
        journal,
        root: fixture.root.clone(),
        journal_maintenance: Default::default(),
        writer,
        emit: Arc::new(|_| {}),
        data: Mutex::new(SessionData {
            turns: vec![],
            turn_base: 0,
            wire_base: 0,
            inherited_mcp_intent: crate::mcp::McpIntent::default(),
            active: None,
            recovery: None,
            revision: 1,
            storage_failed: false,
            last_emit: std::time::Instant::now(),
            extras: journal::Extras::default(),
            compacting: false,
            manual_compaction: false,
        }),
    })
}
#[test]
fn activity_tracks_loaded_sessions_without_exposing_history() {
    let fixture = Fixture::new();
    let session = session(&fixture);
    let state = AgentState::default();
    state
        .sessions
        .lock()
        .unwrap()
        .insert(session.id.clone(), session.clone());
    assert!(state.activity().unwrap()[0].active_turn_id.is_none());
    session
        .reserve("Private request".into(), options(ApprovalMode::Yolo))
        .unwrap();
    let running = state.activity().unwrap();
    assert!(running[0].active_turn_id.is_some());
    let revision = running[0].revision;
    let public = serde_json::to_value(&running).unwrap();
    assert_eq!(public[0].as_object().unwrap().len(), 4);
    assert!(!public.to_string().contains("Private request"));
    finish(&session, Err(AgentError::cancelled()));
    let ended = state.activity().unwrap();
    assert!(ended[0].active_turn_id.is_none());
    assert!(ended[0].revision > revision);
}

#[tokio::test]
async fn panicked_runs_release_the_chat_and_preserve_progress_without_replaying_the_queue() {
    let fixture = Fixture::new();
    let mut session = session(&fixture);
    let emitted = Arc::new(Mutex::new(None));
    let capture = emitted.clone();
    Arc::get_mut(&mut session).unwrap().emit = Arc::new(move |snapshot| {
        *capture.lock().unwrap() = Some(snapshot);
    });
    let mut signal = session
        .reserve("Prepare o projeto".into(), options(ApprovalMode::Yolo))
        .unwrap();
    session
        .submit(
            "Depois confira o resultado".into(),
            options(ApprovalMode::Yolo),
        )
        .unwrap();

    let completed = finish_run(&session, async {
        session.update(false, |data| {
            data.turns.last_mut().unwrap().turn.steps.push(Step {
                text: "Progresso confirmado".into(),
                ..Step::default()
            });
        })?;
        tokio::task::yield_now().await;
        panic!("synthetic provider panic containing private details");
    })
    .await;

    assert!(!completed, "a failed run must not advance the queue");
    let snapshot = emitted.lock().unwrap().clone().unwrap();
    assert!(snapshot.active_turn_id.is_none());
    assert_eq!(snapshot.turns[0].status, TurnStatus::Error);
    assert_eq!(snapshot.turns[0].steps[0].text, "Progresso confirmado");
    let error = snapshot.turns[0].error.as_ref().unwrap();
    assert_eq!(error.code, "internal");
    assert!(!error.message.contains("private details"));
    assert_eq!(snapshot.queued_messages.len(), 1);
    assert_eq!(
        snapshot.queued_messages[0].content,
        "Depois confira o resultado"
    );
    let (stored, extras) = journal::read_only(&session.journal).unwrap();
    assert_eq!(stored[0].turn.status, TurnStatus::Error);
    assert_eq!(stored[0].turn.steps[0].text, "Progresso confirmado");
    assert_eq!(extras.queue.len(), 1);
    tokio::time::timeout(Duration::from_secs(1), cancelled(&mut signal))
        .await
        .expect("child cancellation must not retain an abandoned run");
    assert!(session.reserve_next().unwrap().is_some());
}

#[test]
fn harness_evaluation_direct_recovery_preserves_durable_results_and_new_messages() {
    let fixture = Fixture::new();
    let session = session(&fixture);
    let mut direct = options(ApprovalMode::Yolo);
    direct.workflow = Some(workflow::Flow::Designer);
    let recovered = StoredTurn {
        excluded_queue_ms: 0,
        mcp_intent: None,
        mcp_parent_intent: None,
        mcp_intent_auxiliary_count: 0,
        turn: Turn {
            id: "recovered-turn".into(),
            active_since: None,
            created_at: 1,
            duration_ms: 0,
            user: "Ajuste o layout".into(),
            parts: vec![],
            auxiliary_messages: vec![],
            options: direct,
            context_window: Some(128_000),
            status: TurnStatus::Running,
            tasks: vec![tasks::Task {
                id: "design".into(),
                title: "Ajustar o layout".into(),
                status: tasks::Status::InProgress,
            }],
            steps: vec![Step {
                tools: vec![ToolCall {
                    id: "read-1".into(),
                    name: "read".into(),
                    args: json!({"path":"README.md"}),
                    status: "completed".into(),
                    output: "# Jarvis".into(),
                    duration_ms: 1,
                }],
                ..Step::default()
            }],
            error: None,
        },
        wire: vec![
            json!({"role":"user","content":"Ajuste o layout"}),
            json!({"type":"function_call","call_id":"read-1","name":"read","arguments":"{\"path\":\"README.md\"}"}),
            json!({"type":"function_call_output","call_id":"read-1","output":"# Jarvis"}),
        ],
    };
    assert!(resumable_direct_turn(&recovered));
    let mut missing_output = recovered.clone();
    missing_output.wire.pop();
    assert!(!resumable_direct_turn(&missing_output));
    journal::mark_interrupted(&mut missing_output);
    assert!(!resumable_direct_turn(&missing_output));
    let mut planned = recovered.clone();
    planned.turn.options.workflow = Some(workflow::Flow::Planned);
    assert!(!resumable_direct_turn(&planned));
    let mut prior_runtime = recovered;
    journal::mark_interrupted(&mut prior_runtime);
    assert!(resumable_direct_turn(&prior_runtime));

    {
        let mut data = session.data.lock().unwrap();
        data.turns.push(prior_runtime);
        data.recovery = Some("recovered-turn".into());
    }
    assert!(session
        .resume_recovered_turn(RecoveryTrigger::UserAction)
        .unwrap()
        .is_some());
    let snapshot = session.snapshot().unwrap();
    assert_eq!(snapshot.active_turn_id.as_deref(), Some("recovered-turn"));
    assert_eq!(snapshot.turns[0].status, TurnStatus::Running);
    assert_eq!(snapshot.turns[0].tasks[0].id, "design");
    assert!(session
        .submit("Continue depois".into(), options(ApprovalMode::Yolo))
        .unwrap()
        .is_none());
    assert_eq!(session.snapshot().unwrap().queued_messages.len(), 1);

    let (persisted, _) = journal::read_only(&session.journal).unwrap();
    assert!(persisted[0].wire.iter().any(|item| item["content"]
        .as_str()
        .is_some_and(|text| text.contains("runtime restarted"))));
    let persisted_results = persisted[0]
        .wire
        .iter()
        .filter(|item| item["type"] == "function_call_output")
        .count() as u64;
    evaluation::assert_runtime_report(
        "direct-turn-resume",
        evaluation::RuntimeReport::new(
            "resumed",
            [
                ("persistedToolResults", persisted_results),
                ("queuedMessages", 1),
                ("recoveries", 1),
                ("toolCalls", 1),
            ],
        ),
    );
}

#[test]
fn harness_evaluation_progress_pause_waits_for_user_before_direct_resume() {
    let fixture = Fixture::new();
    let session = session(&fixture);
    session
        .reserve(
            "Investigue e corrija o problema".into(),
            options(ApprovalMode::Yolo),
        )
        .unwrap();
    finish(
        &session,
        Err(AgentError::new(
            "progress_paused",
            "A execução foi pausada com o estado preservado.",
        )),
    );

    assert_eq!(
        session.snapshot().unwrap().turns[0].status,
        TurnStatus::Interrupted
    );
    assert!(session
        .resume_recovered_turn(RecoveryTrigger::PassiveOpen)
        .unwrap()
        .is_none());
    assert!(session
        .submit(
            "Continue usando outra estratégia".into(),
            options(ApprovalMode::Yolo)
        )
        .unwrap()
        .is_none());
    assert!(session
        .resume_recovered_turn(RecoveryTrigger::UserAction)
        .unwrap()
        .is_some());
    let snapshot = session.snapshot().unwrap();
    assert_eq!(snapshot.turns[0].status, TurnStatus::Running);
    assert_eq!(snapshot.queued_messages.len(), 1);
    let (persisted, _) = journal::read_only(&session.journal).unwrap();
    assert!(persisted[0].wire.iter().any(|item| {
        item["_jarvis_runtime"] == true
            && item["content"]
                .as_str()
                .is_some_and(|text| text.contains("progress watchdog"))
    }));
    evaluation::assert_runtime_report(
        "progress-pause-resume",
        evaluation::RuntimeReport::new(
            "resumed",
            [
                ("passiveResumes", 0),
                ("pauses", 1),
                ("queuedMessages", 1),
                ("recoveries", 1),
            ],
        ),
    );
}

#[test]
fn explicit_retry_continues_failed_direct_turn_without_replaying_uncertain_tools() {
    let fixture = Fixture::new();
    let session = session(&fixture);
    let mut direct = options(ApprovalMode::Yolo);
    direct.workflow = Some(workflow::Flow::Designer);
    let mut failed = StoredTurn {
        excluded_queue_ms: 0,
        mcp_intent: None,
        mcp_parent_intent: None,
        mcp_intent_auxiliary_count: 0,
        turn: Turn {
            id: "failed-turn".into(),
            active_since: None,
            created_at: 1,
            duration_ms: 3_000,
            user: "Ajuste o layout".into(),
            parts: vec![],
            auxiliary_messages: vec![],
            options: direct,
            context_window: Some(128_000),
            status: TurnStatus::Error,
            tasks: vec![],
            steps: vec![Step {
                tools: vec![ToolCall {
                    id: "write-1".into(),
                    name: "write".into(),
                    args: json!({"path":"src/app.tsx","content":"changed"}),
                    status: "running".into(),
                    output: String::new(),
                    duration_ms: 0,
                }],
                ..Step::default()
            }],
            error: Some(AgentError::new(
                "provider_retry_exhausted",
                "A conexão com o provedor falhou.",
            )),
        },
        wire: vec![
            json!({"role":"user","content":"Ajuste o layout"}),
            json!({"type":"function_call","call_id":"write-1","name":"write","arguments":"{}"}),
        ],
    };
    let evidence = "saved result 🦀\n".repeat(700_000);
    failed
        .wire
        .push(json!({"role":"assistant","content":evidence}));
    assert!(serde_json::to_vec(&failed).unwrap().len() > 10 * 1024 * 1024);
    journal::append(&session.journal, &failed).unwrap();
    session.data.lock().unwrap().turns.push(failed);

    let unavailable = session.retry_failed_turn("older-turn", None).unwrap_err();
    assert_eq!(unavailable.code, "retry_unavailable");

    let (signal, workflow_recovery) = session.retry_failed_turn("failed-turn", None).unwrap();
    assert!(!*signal.borrow());
    assert!(workflow_recovery.is_none());
    let snapshot = session.snapshot().unwrap();
    assert_eq!(snapshot.active_turn_id.as_deref(), Some("failed-turn"));
    assert_eq!(snapshot.turns[0].status, TurnStatus::Running);
    assert!(snapshot.turns[0].error.is_none());

    // The previous 3s are preserved; the time since created_at (including an
    // overnight stop) must never be charged to the resumed execution.
    assert_eq!(snapshot.turns[0].duration_ms, 3_000);
    assert!(snapshot.turns[0].active_since.is_none());
    session.transition(turn_state::TurnPhase::Sampling).unwrap();
    assert!(session.snapshot().unwrap().turns[0].duration_ms < 4_000);

    let (stored, _) = journal::read_only(&session.journal).unwrap();
    let retried = stored.last().unwrap();
    assert!(retried.wire.iter().any(|item| item["content"] == evidence));
    assert_eq!(
        retried
            .wire
            .iter()
            .filter(|item| item["role"] == "user" && item["_jarvis_runtime"] != true)
            .count(),
        1
    );
    assert_eq!(
        retried
            .wire
            .iter()
            .filter(|item| item["type"] == "function_call_output" && item["call_id"] == "write-1")
            .count(),
        1
    );
    assert!(retried
        .wire
        .iter()
        .any(|item| item["_jarvis_retry"] == true));
    assert!(retried.turn.steps[0].tools[0]
        .output
        .contains("resultado desconhecido"));
}

#[test]
fn explicit_retry_uses_current_model_and_preserves_the_original_request_and_receipts() {
    let fixture = Fixture::new();
    let session = session(&fixture);
    let mut original = options(ApprovalMode::Yolo);
    original.workflow = Some(workflow::Flow::Designer);
    let _signal = session
        .reserve("Ajuste este layout".into(), original.clone())
        .unwrap();
    let attachment = skill_input::MessagePart::Attachment {
        attachment: attachments::Attachment {
            id: "image-reference".into(),
            conversation_id: session.id.clone(),
            name: "image.png".into(),
            mime: "image/png".into(),
            size: 99_911,
            kind: "image".into(),
        },
    };
    let receipt =
        json!({"type":"function_call_output","call_id":"saved-1","output":"Arquivo salvo"});
    session
        .update(true, |data| {
            let current = data.turns.last_mut().unwrap();
            current.turn.parts.push(attachment.clone());
            current.turn.context_window = Some(128_000);
            current.wire.extend([
                json!({"type":"reasoning","encrypted_content":"original-private-state"}),
                json!({"type":"function_call","call_id":"saved-1","name":"write","arguments":"{}"}),
                receipt.clone(),
            ]);
        })
        .unwrap();
    finish(
        &session,
        Err(AgentError::new("provider_request", "Falha do provedor")),
    );
    session
        .update(true, |data| {
            data.turns[0].turn.options.executor = crate::claude::Executor::Unavailable;
        })
        .unwrap();
    let id = session.snapshot().unwrap().turns[0].id.clone();
    let mut choice = workflow::settings::ModelChoice {
        executor: crate::claude::Executor::Jarvis,
        account: "new-account".into(),
        model: "new-model".into(),
        reasoning: Some("high".into()),
        service_tier: Some(workflow::settings::ServiceTier::Priority),
        fallback: None,
    };
    choice.account.clear();
    assert!(session.retry_failed_turn(&id, Some(&choice)).is_err());
    let failed = session.snapshot().unwrap();
    assert_eq!(failed.active_turn_id, None);
    assert_eq!(failed.turns[0].status, TurnStatus::Error);
    assert_eq!(failed.turns[0].options.account, original.account);
    choice.account = "new-account".into();
    let (_, workflow) = session.retry_failed_turn(&id, Some(&choice)).unwrap();
    assert!(workflow.is_none());
    let (stored, _) = journal::read_only(&session.journal).unwrap();
    let retried = &stored[0];
    assert_eq!(retried.turn.id, id);
    assert_eq!(retried.turn.user, "Ajuste este layout");
    assert_eq!(
        serde_json::to_value(&retried.turn.parts).unwrap(),
        json!([attachment])
    );
    assert_eq!(retried.turn.options.account, choice.account);
    assert_eq!(retried.turn.options.model, choice.model);
    assert_eq!(retried.turn.options.reasoning, choice.reasoning);
    assert_eq!(retried.turn.options.service_tier, choice.service_tier);
    assert_eq!(retried.turn.options.model_selection, Some(choice));
    assert_eq!(retried.turn.options.workflow, original.workflow);
    assert_eq!(retried.turn.options.approval_mode, original.approval_mode);
    assert_eq!(retried.turn.context_window, None);
    assert_eq!(
        retried.wire.iter().filter(|item| **item == receipt).count(),
        1
    );
    assert!(retried.wire.iter().any(model_fallback::boundary));
    assert!(!model_fallback::used(retried));
    let replay = compaction::input(&session.data.lock().unwrap());
    assert!(replay.contains(&receipt));
    assert!(!replay.iter().any(|item| item["type"] == "reasoning"));
}

#[test]
fn explicit_retry_restarts_workflow_preparation_when_no_manifest_was_created() {
    let fixture = Fixture::new();
    let session = session_with_id(&fixture, "aabbccddaabbccddaabbccddaabbccdd");
    let mut planned = options(ApprovalMode::Yolo);
    planned.workflow = Some(workflow::Flow::Planned);
    let failed = StoredTurn {
        excluded_queue_ms: 0,
        mcp_intent: None,
        mcp_parent_intent: None,
        mcp_intent_auxiliary_count: 0,
        turn: Turn {
            id: "planned-turn".into(),
            active_since: None,
            created_at: 1,
            duration_ms: 500,
            user: "Planeje e implemente".into(),
            parts: vec![],
            auxiliary_messages: vec![],
            options: planned,
            context_window: Some(128_000),
            status: TurnStatus::Error,
            tasks: vec![],
            steps: vec![],
            error: Some(AgentError::new(
                "provider_retry_exhausted",
                "O provedor falhou antes de preparar o fluxo.",
            )),
        },
        wire: vec![json!({"role":"user","content":"Planeje e implemente"})],
    };
    journal::append(&session.journal, &failed).unwrap();
    session.data.lock().unwrap().turns.push(failed);

    assert!(!workflow::recovery_checkpoint_available(&fixture.root, &session, None).unwrap());
    let choice = workflow::settings::chat::effective_choice(
        &session.data.lock().unwrap().turns[0].turn.options,
        None,
    );
    session
        .update(true, |data| {
            data.turns[0].turn.options.executor = crate::claude::Executor::Unavailable;
        })
        .unwrap();
    assert!(workflow::recovery_checkpoint_available(&fixture.root, &session, None).is_err());
    assert!(
        !workflow::recovery_checkpoint_available(&fixture.root, &session, Some(&choice)).unwrap()
    );
    let (_, workflow_recovery) = session
        .retry_failed_turn("planned-turn", Some(&choice))
        .unwrap();

    assert_eq!(workflow_recovery, Some(vec![]));
    assert_eq!(
        session.snapshot().unwrap().active_turn_id.as_deref(),
        Some("planned-turn")
    );
}

#[test]
fn explicit_retry_restarts_publication_with_current_repository_state() {
    let fixture = Fixture::new();
    let session = session(&fixture);
    let mut publication = options(ApprovalMode::Yolo);
    publication.workflow = Some(workflow::Flow::Publication);
    let failed = StoredTurn {
        excluded_queue_ms: 0,
        mcp_intent: None,
        mcp_parent_intent: None,
        mcp_intent_auxiliary_count: 0,
        turn: Turn {
            id: "publication-turn".into(),
            active_since: None,
            created_at: 1,
            duration_ms: 500,
            user: "Faça commit, push, PR e merge".into(),
            parts: vec![],
            auxiliary_messages: vec![],
            options: publication,
            context_window: Some(128_000),
            status: TurnStatus::Error,
            tasks: vec![],
            steps: vec![],
            error: Some(AgentError::new(
                "provider_retry_exhausted",
                "A publicação foi interrompida.",
            )),
        },
        wire: vec![json!({"role":"user","content":"Faça commit, push, PR e merge"})],
    };
    journal::append(&session.journal, &failed).unwrap();
    session.data.lock().unwrap().turns.push(failed);

    let (_, workflow_recovery) = session.retry_failed_turn("publication-turn", None).unwrap();

    assert!(workflow_recovery.is_none());
    let (stored, _) = journal::read_only(&session.journal).unwrap();
    assert!(stored.last().unwrap().wire.iter().any(|item| {
        item["_jarvis_retry"] == true
            && item["content"].as_str().is_some_and(|content| {
                content.contains("inspect the current project and task state")
            })
    }));
}

#[test]
fn harness_evaluation_coordinated_recovery_pairs_uncertain_tools_without_replay() {
    let fixture = Fixture::new();
    let session = session(&fixture);
    let mut coordinated = options(ApprovalMode::Yolo);
    coordinated.workflow = Some(workflow::Flow::Complete);
    let turn = StoredTurn {
        excluded_queue_ms: 0,
        mcp_intent: None,
        mcp_parent_intent: None,
        mcp_intent_auxiliary_count: 0,
        turn: Turn {
            id: "workflow-turn".into(),
            active_since: None,
            created_at: 1,
            duration_ms: 0,
            user: "Execute o fluxo".into(),
            parts: vec![],
            auxiliary_messages: vec![],
            options: coordinated,
            context_window: Some(128_000),
            status: TurnStatus::Interrupted,
            tasks: vec![],
            steps: vec![Step {
                tools: vec![ToolCall {
                    id: "write-1".into(),
                    name: "write".into(),
                    args: json!({"path":"src/app.ts","content":"changed"}),
                    status: "running".into(),
                    output: String::new(),
                    duration_ms: 0,
                }],
                ..Step::default()
            }],
            error: Some(AgentError::new(
                "interrupted",
                "O Jarvis foi encerrado durante esta execução.",
            )),
        },
        wire: vec![
            json!({"role":"user","content":"Execute o fluxo"}),
            json!({"type":"function_call","call_id":"write-1","name":"write","arguments":"{}"}),
        ],
    };
    journal::append(&session.journal, &turn).unwrap();
    {
        let mut data = session.data.lock().unwrap();
        data.turns.push(turn.clone());
    }

    assert!(session
        .resume_recovered_turn(RecoveryTrigger::UserAction)
        .unwrap()
        .is_none());
    let (signal, uncertain) = session.resume_interrupted_workflow_turn().unwrap();
    assert_eq!(uncertain, vec!["write"]);
    assert!(!*signal.borrow());
    let snapshot = session.snapshot().unwrap();
    assert_eq!(snapshot.active_turn_id.as_deref(), Some("workflow-turn"));
    assert_eq!(snapshot.turns[0].status, TurnStatus::Running);
    let (stored, _) = journal::read_only(&session.journal).unwrap();
    let recovered = stored.last().unwrap();
    assert_eq!(recovered.turn.steps[0].tools[0].status, "error");
    assert!(recovered.turn.steps[0].tools[0]
        .output
        .contains("resultado desconhecido"));
    assert_eq!(
        recovered
            .wire
            .iter()
            .filter(|item| item["call_id"] == "write-1" && item["type"] == "function_call_output")
            .count(),
        1
    );
    assert!(recovered
        .wire
        .iter()
        .any(|item| item["_jarvis_retry"] == true));
    let persisted_results = recovered
        .wire
        .iter()
        .filter(|item| item["type"] == "function_call_output")
        .count() as u64;
    evaluation::assert_runtime_report(
        "workflow-turn-resume",
        evaluation::RuntimeReport::new(
            "resumed",
            [
                ("errors", 1),
                ("persistedToolResults", persisted_results),
                ("recoveries", 1),
                ("toolCalls", 1),
                ("uncertainCalls", uncertain.len() as u64),
            ],
        ),
    );
}

#[test]
fn direct_task_contract_is_present_once_for_native_custom_and_legacy_prompts() {
    let native = [
        (workflow::Flow::Standard, workflow::Role::Builder),
        (workflow::Flow::Designer, workflow::Role::Designer),
    ]
    .map(|(flow, role)| {
        workflow::settings::get_agent_instructions(flow, role)
            .unwrap()
            .into_iter()
            .map(|section| section.content)
            .collect::<String>()
    });
    let mut prompts = native.to_vec();
    prompts.push("Custom direct agent instructions.\n".into());
    prompts.push(tools::instructions(
        Path::new("/project"),
        Mode::Plan,
        ApprovalMode::Yolo,
    ));
    for mut prompt in prompts {
        let original = prompt.clone();
        append_direct_task_instructions(&mut prompt);
        assert!(prompt.starts_with(&original));
        assert_eq!(prompt.matches(tasks::INSTRUCTIONS).count(), 1);
        let rebuilt = prompt.clone();
        append_direct_task_instructions(&mut prompt);
        assert_eq!(prompt, rebuilt);
    }
}

#[test]
fn direct_tasks_are_durable_and_reset_for_each_new_turn() {
    let fixture = Fixture::new();
    let session = session(&fixture);
    let mut direct = options(ApprovalMode::Yolo);
    direct.workflow = Some(workflow::Flow::Standard);
    session
        .reserve("Implementar".into(), direct.clone())
        .unwrap();

    let output = tasks::execute(
        &session,
        &json!({"tasks":[
            {"id":"inspect","title":"Analisar o escopo","status":"completed"},
            {"id":"implement","title":"Implementar a mudança","status":"in_progress"}
        ]}),
    )
    .unwrap();
    assert!(output.contains("\"updated\":2"));
    assert_eq!(session.snapshot().unwrap().turns[0].tasks.len(), 2);
    let (stored, _) = journal::read_only(&session.journal).unwrap();
    assert_eq!(
        stored[0].turn.tasks,
        session.snapshot().unwrap().turns[0].tasks
    );

    finish(&session, Ok(()));
    session
        .reserve("Próxima solicitação".into(), direct)
        .unwrap();
    assert!(session.snapshot().unwrap().turns[0].tasks.is_empty());
}

#[test]
fn durable_updates_include_transient_streaming_state_in_the_journal_delta() {
    let fixture = Fixture::new();
    let session = session(&fixture);
    session
        .reserve("Analisar o design".into(), options(ApprovalMode::Yolo))
        .unwrap();

    session
        .update(false, |data| {
            data.turns.last_mut().unwrap().turn.steps.push(Step {
                text: "Análise parcial".into(),
                ..Step::default()
            });
        })
        .unwrap();
    session
        .update(true, |data| {
            data.turns.last_mut().unwrap().turn.steps[0].duration_ms = 42;
        })
        .unwrap();

    let (persisted, _) = journal::read_only(&session.journal).unwrap();
    assert_eq!(persisted[0].turn.steps[0].text, "Análise parcial");
    assert_eq!(persisted[0].turn.steps[0].duration_ms, 42);
}

#[test]
fn failed_task_checkpoint_does_not_change_the_in_memory_list() {
    let fixture = Fixture::new();
    let session = session(&fixture);
    let mut direct = options(ApprovalMode::Yolo);
    direct.workflow = Some(workflow::Flow::Designer);
    session.reserve("Criar layout".into(), direct).unwrap();
    fs::remove_file(&session.journal).unwrap();

    let result = tasks::execute(
        &session,
        &json!({"tasks":[{"id":"design","title":"Criar o layout","status":"in_progress"}]}),
    );
    assert_eq!(result.unwrap_err().code, "session_storage");
    assert!(session.snapshot().unwrap().turns[0].tasks.is_empty());
}

#[test]
fn deletion_blocks_active_turns_evicts_idle_sessions_and_rejects_late_writes() {
    let fixture = Fixture::new();
    let state = AppState::default();
    let agent = AgentState::default();
    let id = library::new_id().unwrap();
    let project_id = library::new_id().unwrap();
    state.with_connection(&fixture.root, |connection| {
        connection.execute("INSERT INTO workspaces (id, name) VALUES ('workspace', 'Test')", [])?;
        connection.execute("INSERT INTO projects (id, workspace_id, name, path) VALUES (?1, 'workspace', 'Project', ?2)", rusqlite::params![project_id, fixture.root.to_string_lossy()])?;
        connection.execute("INSERT INTO conversations (id, project_id, title) VALUES (?1, ?2, 'Test')", rusqlite::params![id, project_id])?;
        Ok::<_, library::LibraryError>(())
    }).unwrap();
    let mut session = session(&fixture);
    let path = crate::data_dir::root(&fixture.root)
        .join("sessions")
        .join(&project_id)
        .join(format!("{id}.jsonl"));
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, "test history").unwrap();
    let mutable = Arc::get_mut(&mut session).unwrap();
    mutable.id = id.clone();
    mutable.journal = path.clone();
    mutable.writer = session_writer::SessionWriter::start(path.clone(), id.clone(), None).unwrap();
    agent
        .sessions
        .lock()
        .unwrap()
        .insert(id.clone(), session.clone());
    let _signal = session
        .reserve("hello".into(), options(ApprovalMode::Yolo))
        .unwrap();
    let target = library::deletion::DeleteTarget::Conversation(id.clone());
    assert!(agent
        .delete_library_item(&state, &fixture.root, &target)
        .is_err());
    assert!(path.exists());
    finish(&session, Err(AgentError::cancelled()));
    agent
        .delete_library_item(&state, &fixture.root, &target)
        .unwrap();
    assert!(!path.exists());
    assert!(agent.existing(&id).is_err());
    assert!(session
        .reserve("late request".into(), options(ApprovalMode::Yolo))
        .is_err());
    assert!(!library::save_generated_title(&state, &fixture.root, &id, "Late title").unwrap());
}

pub(super) struct TerminalTestGuard(pub(super) AgentState);
impl Drop for TerminalTestGuard {
    fn drop(&mut self) {
        self.0.terminals.stop_all();
    }
}

pub(super) async fn open_project_terminal(
    agent: &AgentState,
    project: &str,
    conversation: Option<&str>,
    root: &std::path::Path,
    service: bool,
) -> String {
    let call = ToolCall {
        id: library::new_id().unwrap(),
        name: if service {
            "process_start"
        } else {
            "terminal_start"
        }
        .into(),
        args: if service {
            let command = if cfg!(windows) {
                "Start-Sleep 60"
            } else {
                "sleep 60"
            };
            json!({"title":"Project watcher", "kind":"watcher", "command":command})
        } else {
            json!({"title":"Project shell"})
        },
        status: "running".into(),
        output: String::new(),
        duration_ms: 0,
    };
    let scope = terminals::TerminalScope {
        project,
        conversation,
    };
    let output = if service {
        agent
            .processes
            .execute(
                scope,
                root,
                "test-run:builder",
                &call,
                None,
                terminals::silent_events(),
            )
            .await
    } else {
        agent
            .terminals
            .execute(
                scope,
                root,
                "test-run:builder",
                &call,
                None,
                terminals::silent_events(),
            )
            .await
    }
    .unwrap();
    serde_json::from_str::<Value>(&output).unwrap()["id"]
        .as_str()
        .unwrap()
        .into()
}

fn terminal_project_library(
    fixture: &Fixture,
    conversations: usize,
) -> (AppState, String, String, Vec<String>) {
    let state = AppState::default();
    let workspace = library::new_id().unwrap();
    let project = library::new_id().unwrap();
    let ids: Vec<_> = (0..conversations)
        .map(|_| library::new_id().unwrap())
        .collect();
    state.with_connection(&fixture.root, |connection| {
        connection.execute("INSERT INTO workspaces (id, name) VALUES (?1, 'Test')", [&workspace])?;
        connection.execute("INSERT INTO projects (id, workspace_id, name, path) VALUES (?1, ?2, 'Project', ?3)", rusqlite::params![project, workspace, fixture.root.to_string_lossy()])?;
        for id in &ids {
            connection.execute("INSERT INTO conversations (id, project_id, title) VALUES (?1, ?2, 'Test')", rusqlite::params![id, project])?;
        }
        Ok::<_, library::LibraryError>(())
    }).unwrap();
    (state, workspace, project, ids)
}

#[tokio::test]
async fn deleting_a_creator_chat_preserves_project_shells_and_services_for_other_chats() {
    let fixture = Fixture::new();
    let (state, _, project, chats) = terminal_project_library(&fixture, 2);
    let guarded = TerminalTestGuard(AgentState::default());
    let agent = &guarded.0;
    let terminal =
        open_project_terminal(agent, &project, Some(&chats[0]), &fixture.root, false).await;
    let service =
        open_project_terminal(agent, &project, Some(&chats[0]), &fixture.root, true).await;
    agent
        .delete_library_item(
            &state,
            &fixture.root,
            &library::deletion::DeleteTarget::Conversation(chats[0].clone()),
        )
        .unwrap();
    assert!(agent.terminals.has_running());
    assert!(agent.terminals.context(&project).contains(&terminal));
    assert!(agent.terminals.context(&project).contains(&service));
    let scope = terminals::TerminalScope {
        project: &project,
        conversation: Some(&chats[1]),
    };
    for (name, id) in [("terminal_output", &terminal), ("process_output", &service)] {
        let call = ToolCall {
            id: "read-after-delete".into(),
            name: name.into(),
            args: json!({"id":id}),
            status: "running".into(),
            output: String::new(),
            duration_ms: 0,
        };
        let result = if name == "terminal_output" {
            agent
                .terminals
                .execute(
                    scope,
                    &fixture.root,
                    "another-run:builder",
                    &call,
                    None,
                    terminals::silent_events(),
                )
                .await
        } else {
            agent
                .processes
                .execute(
                    scope,
                    &fixture.root,
                    "another-run:builder",
                    &call,
                    None,
                    terminals::silent_events(),
                )
                .await
        };
        assert!(
            result.is_ok(),
            "another chat can inspect the surviving {name}: {result:?}"
        );
    }
}

#[tokio::test]
async fn successful_project_deletion_stops_terminals_even_without_chats() {
    let fixture = Fixture::new();
    let (state, _, project, _) = terminal_project_library(&fixture, 0);
    let guarded = TerminalTestGuard(AgentState::default());
    let agent = &guarded.0;
    open_project_terminal(agent, &project, None, &fixture.root, false).await;
    open_project_terminal(agent, &project, None, &fixture.root, true).await;
    assert!(agent.terminals.has_running());
    agent
        .delete_library_item(
            &state,
            &fixture.root,
            &library::deletion::DeleteTarget::Project(project.clone()),
        )
        .unwrap();
    assert!(!agent.terminals.has_running());
    assert!(agent.terminals.context(&project).is_empty());
}

#[tokio::test]
async fn committed_project_deletion_stops_terminals_when_beads_cleanup_is_busy() {
    use fs2::FileExt;
    let fixture = Fixture::new();
    let (state, _, project, _) = terminal_project_library(&fixture, 0);
    let private = crate::core::beads::storage(&fixture.root, &project);
    fs::create_dir_all(&private).unwrap();
    fs::write(private.join("tasks.db"), "private tasks").unwrap();
    let locks = crate::data_dir::root(&fixture.root).join("beads/locks");
    fs::create_dir_all(&locks).unwrap();
    let lock = fs::File::create(locks.join(format!("{project}.lock"))).unwrap();
    FileExt::lock_exclusive(&lock).unwrap();
    let guarded = TerminalTestGuard(AgentState::default());
    let agent = &guarded.0;
    open_project_terminal(agent, &project, None, &fixture.root, false).await;
    open_project_terminal(agent, &project, None, &fixture.root, true).await;
    let result = agent.delete_library_item(
        &state,
        &fixture.root,
        &library::deletion::DeleteTarget::Project(project.clone()),
    );
    assert!(result.is_err());
    let exists = state
        .with_connection(&fixture.root, |connection| {
            Ok::<_, library::LibraryError>(connection.query_row(
                "SELECT EXISTS(SELECT 1 FROM projects WHERE id = ?1)",
                [&project],
                |row| row.get::<_, bool>(0),
            )?)
        })
        .unwrap();
    assert!(
        !exists,
        "metadata committed before optional Beads cleanup failed"
    );
    assert!(private.exists());
    assert!(agent.terminals.context(&project).is_empty());
    assert!(!agent.terminals.has_running());
}

#[tokio::test]
async fn failed_workspace_deletion_preserves_terminals_and_success_stops_all_its_projects() {
    let fixture = Fixture::new();
    let (state, workspace, project, _) = terminal_project_library(&fixture, 0);
    let second = library::new_id().unwrap();
    let other_workspace = library::new_id().unwrap();
    let unrelated = library::new_id().unwrap();
    state.with_connection(&fixture.root, |connection| {
        connection.execute("INSERT INTO workspaces (id, name) VALUES (?1, 'Other')", [&other_workspace])?;
        for (id, workspace) in [(&second, &workspace), (&unrelated, &other_workspace)] {
            let path = fixture.root.join(id);
            fs::create_dir(&path).unwrap();
            connection.execute("INSERT INTO projects (id, workspace_id, name, path) VALUES (?1, ?2, 'Project', ?3)", rusqlite::params![id, workspace, path.to_string_lossy()])?;
        }
        connection.execute_batch("CREATE TRIGGER prevent_workspace_delete BEFORE DELETE ON workspaces BEGIN SELECT RAISE(FAIL, 'synthetic failure'); END;")?;
        Ok::<_, library::LibraryError>(())
    }).unwrap();
    let guarded = TerminalTestGuard(AgentState::default());
    let agent = &guarded.0;
    for project in [&project, &second, &unrelated] {
        open_project_terminal(agent, project, None, &fixture.root, false).await;
    }
    let target = library::deletion::DeleteTarget::Workspace(workspace);
    assert!(agent
        .delete_library_item(&state, &fixture.root, &target)
        .is_err());
    for project in [&project, &second, &unrelated] {
        assert!(!agent.terminals.context(project).is_empty());
    }
    state
        .with_connection(&fixture.root, |connection| {
            connection.execute_batch("DROP TRIGGER prevent_workspace_delete")?;
            Ok::<_, library::LibraryError>(())
        })
        .unwrap();
    agent
        .delete_library_item(&state, &fixture.root, &target)
        .unwrap();
    assert!(agent.terminals.context(&project).is_empty());
    assert!(agent.terminals.context(&second).is_empty());
    assert!(!agent.terminals.context(&unrelated).is_empty());
}

#[test]
fn workspace_deletion_stops_a_project_created_while_waiting_for_session_gates() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let fixture = Fixture::new();
    let (state, workspace, project, chats) = terminal_project_library(&fixture, 1);
    let guarded = TerminalTestGuard(AgentState::default());
    let agent = &guarded.0;
    runtime.block_on(open_project_terminal(
        agent,
        &project,
        Some(&chats[0]),
        &fixture.root,
        false,
    ));
    let gate = agent.session_gate(&chats[0]).unwrap();
    let held = gate.lock().unwrap();
    let deleting_agent = AgentState::clone(agent);
    let deleting_state = state.clone();
    let deleting_home = fixture.root.clone();
    let deleting_workspace = workspace.clone();
    let deletion = std::thread::spawn(move || {
        deleting_agent.delete_library_item(
            &deleting_state,
            &deleting_home,
            &library::deletion::DeleteTarget::Workspace(deleting_workspace),
        )
    });
    // Acquiring a reference to this existing gate happens after the preliminary
    // database lookup. The held guard pauses the real deletion without a test hook.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while Arc::strong_count(&gate) < 2 {
        assert!(
            std::time::Instant::now() < deadline,
            "deletion reached its session gate"
        );
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    let late_project = library::new_id().unwrap();
    let late_root = fixture.root.join(&late_project);
    fs::create_dir(&late_root).unwrap();
    state.with_connection(&fixture.root, |connection| {
        connection.execute(
            "INSERT INTO projects (id, workspace_id, name, path) VALUES (?1, ?2, 'Late project', ?3)",
            rusqlite::params![late_project, workspace, late_root.to_string_lossy()],
        )?;
        Ok::<_, library::LibraryError>(())
    }).unwrap();
    runtime.block_on(open_project_terminal(
        agent,
        &late_project,
        None,
        &late_root,
        false,
    ));
    assert!(!agent.terminals.context(&late_project).is_empty());
    drop(held);
    deletion.join().unwrap().unwrap();
    assert!(agent.terminals.context(&project).is_empty());
    assert!(agent.terminals.context(&late_project).is_empty());
    assert!(!agent.terminals.has_running());
}

#[test]
fn concurrent_reservation_accepts_one_turn_and_failed_storage_blocks_retries() {
    let fixture = Fixture::new();
    let session = session(&fixture);
    let handles: Vec<_> = (0..2)
        .map(|_| {
            let session = session.clone();
            std::thread::spawn(move || {
                session
                    .reserve("Hello".into(), options(ApprovalMode::Manual))
                    .is_ok()
            })
        })
        .collect();
    let accepted = handles
        .into_iter()
        .filter_map(|handle| handle.join().ok())
        .filter(|accepted| *accepted)
        .count();
    assert_eq!(accepted, 1);
    assert_eq!(session.snapshot().unwrap().turns.len(), 1);
    finish(&session, Err(AgentError::cancelled()));
    fs::remove_file(&session.journal).unwrap();
    assert!(session
        .reserve("Second".into(), options(ApprovalMode::Yolo))
        .is_err());
    fs::write(&session.journal, "{}\n").unwrap();
    assert!(session
        .reserve("Third".into(), options(ApprovalMode::Yolo))
        .is_err());
    assert_eq!(session.snapshot().unwrap().turns.len(), 1);
}
#[tokio::test]
async fn manual_waits_for_matching_approval_and_yolo_does_not_prompt() {
    let fixture = Fixture::new();
    let session = session(&fixture);
    let signal = session
        .reserve("Edit".into(), options(ApprovalMode::Manual))
        .unwrap();
    let tool = ToolCall {
        id: "tool".into(),
        name: "write".into(),
        args: json!({"path":"a.txt","content":"proposed"}),
        status: "pending".into(),
        output: String::new(),
        duration_ms: 0,
    };
    let task_session = session.clone();
    let task_tool = tool.clone();
    let task_signal = signal.clone();
    let pending = tokio::spawn(async move {
        authorize(
            &task_session,
            &task_tool,
            &options(ApprovalMode::Manual),
            false,
            task_signal,
        )
        .await
    });
    tokio::time::timeout(Duration::from_secs(1), async {
        while session.snapshot().unwrap().pending_approval.is_none() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert!(!pending.is_finished());
    assert!(!fixture.root.join("a.txt").exists());
    let turn = session.snapshot().unwrap().active_turn_id.unwrap();
    assert!(answer_approval(&session, "old-turn", "tool", true).is_err());
    assert!(answer_approval(&session, &turn, "other-tool", true).is_err());
    answer_approval(&session, &turn, "tool", false).unwrap();
    assert!(!pending.await.unwrap().unwrap());
    assert!(session.snapshot().unwrap().pending_approval.is_none());
    assert!(
        authorize(&session, &tool, &options(ApprovalMode::Yolo), false, signal)
            .await
            .unwrap()
    );
    assert!(session.snapshot().unwrap().pending_approval.is_none());
}

#[tokio::test]
async fn an_approved_decision_creates_a_reusable_project_grant() {
    let fixture = Fixture::new();
    let session = session(&fixture);
    let signal = session
        .reserve("Publicar".into(), options(ApprovalMode::Manual))
        .unwrap();
    let tool = ToolCall {
        id: "push".into(),
        name: "bash".into(),
        args: json!({"command":"git push origin feature"}),
        status: "pending".into(),
        output: String::new(),
        duration_ms: 0,
    };
    let policy = execution_policy::inspect_tool(
        &fixture.root,
        &tool,
        tool_contract::Capabilities {
            effect: tool_contract::Effect::Mutating,
            approval: tool_contract::ApprovalPolicy::AccordingToTurn,
            parallel_safe: false,
        },
    )
    .unwrap()
    .unwrap();
    assert_eq!(
        policy.outcome.decision,
        execution_policy::ExecutionDecision::Ask
    );
    let store = execution_grants::GrantStore::default();
    store
        .setup(fixture.root.join("execution-grants.json"), now())
        .unwrap();
    let task_session = session.clone();
    let task_tool = tool.clone();
    let pending = tokio::spawn(async move {
        let approval_options = options(ApprovalMode::Manual);
        authorize_declared(
            ApprovalRequest {
                session: &task_session,
                tool: &task_tool,
                options: &approval_options,
                policy: Some(policy),
                sandbox: None,
                project_id: Some("project"),
                manual_hooks: None,
                explicit_video_approval: false,
                signal,
            },
            tool_contract::ApprovalPolicy::AccordingToTurn,
            tool_contract::Handler::Native,
        )
        .await
    });
    tokio::time::timeout(Duration::from_secs(1), async {
        while session.snapshot().unwrap().pending_approval.is_none() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    let turn = session.snapshot().unwrap().active_turn_id.unwrap();
    answer_approval_decision(
        &store,
        &session,
        &turn,
        &tool.id,
        ApprovalDecision {
            approved: true,
            grant: Some(ApprovalGrantRequest {
                scope: ApprovalGrantScope::Project,
                duration: ApprovalGrantDuration::Persistent,
                match_kind: execution_grants::GrantMatch::Exact,
            }),
        },
    )
    .unwrap();

    assert!(pending.await.unwrap().unwrap());
    let grants = store.list_for_project("project", now()).unwrap();
    assert_eq!(grants.len(), 1);
    assert_eq!(grants[0].subject, "git push origin feature");
    assert_eq!(grants[0].scope, execution_grants::GrantScopeKind::Project);
}

#[tokio::test]
async fn terminal_control_is_preapproved_in_yolo_and_waits_in_manual_mode() {
    let fixture = Fixture::new();
    let session = session(&fixture);
    let manual = options(ApprovalMode::Manual);
    let signal = session
        .reserve("Fechar terminal".into(), manual.clone())
        .unwrap();
    let tool = ToolCall {
        id: "terminal-close".into(),
        name: "terminal_close".into(),
        args: json!({"id":"terminal-user","reason":"A verificação terminou."}),
        status: "pending".into(),
        output: String::new(),
        duration_ms: 0,
    };
    let automatic = options(ApprovalMode::Yolo);
    assert!(tokio::time::timeout(
        Duration::from_secs(1),
        authorize_with_policy(&session, &tool, &automatic, false, true, signal.clone()),
    )
    .await
    .expect("YOLO must not wait for terminal approval")
    .unwrap());
    assert!(session.snapshot().unwrap().pending_approval.is_none());
    let (task_session, task_tool, task_options) = (session.clone(), tool.clone(), manual);
    let pending = tokio::spawn(async move {
        authorize_with_policy(
            &task_session,
            &task_tool,
            &task_options,
            false,
            true,
            signal,
        )
        .await
    });
    tokio::time::timeout(Duration::from_secs(1), async {
        while session.snapshot().unwrap().pending_approval.is_none() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert!(!pending.is_finished());
    let turn = session.snapshot().unwrap().active_turn_id.unwrap();
    answer_approval(&session, &turn, &tool.id, true).unwrap();
    assert!(pending.await.unwrap().unwrap());
    assert!(session.snapshot().unwrap().pending_approval.is_none());
}

#[tokio::test]
async fn yolo_executes_native_sandbox_recovery_without_waiting_for_permission() {
    for executor in [
        crate::claude::Executor::Jarvis,
        crate::claude::Executor::Claude,
    ] {
        let fixture = Fixture::new();
        let session = session(&fixture);
        let mut automatic = options(ApprovalMode::Yolo);
        automatic.executor = executor;
        let signal = session
            .reserve("Executar".into(), automatic.clone())
            .unwrap();
        let tool = ToolCall {
            id: "native-recovery".into(),
            name: "bash".into(),
            args: json!({
                "command": if cfg!(windows) {
                    "Set-Content -Encoding utf8 result.txt yolo"
                } else {
                    "echo yolo > result.txt"
                },
                "sandboxPermissions": "require_escalated",
                "justification": "A verificação precisa executar nativamente."
            }),
            status: "pending".into(),
            output: String::new(),
            duration_ms: 0,
        };
        let catalog = tool_contract::Catalog::new(&tools::definitions(Mode::Build));
        catalog.validate(&tool).unwrap();
        let prepared = tool_contract::PreparedTool {
            capabilities: catalog.capabilities(&tool.name).unwrap(),
            handler: tool_contract::Handler::Native,
        };
        let policy = execution_policy::inspect_tool(&fixture.root, &tool, prepared.capabilities)
            .unwrap()
            .unwrap();
        let sandbox = execution_sandbox::prepare(&policy).unwrap();
        assert_eq!(
            policy.outcome.decision,
            execution_policy::ExecutionDecision::Ask
        );
        assert!(sandbox.requires_informed_approval(&policy.outcome.effects));
        assert!(tokio::time::timeout(
            Duration::from_secs(1),
            authorize_prepared(
                ApprovalRequest {
                    session: &session,
                    tool: &tool,
                    options: &automatic,
                    policy: Some(policy),
                    sandbox: Some(&sandbox),
                    project_id: Some("project"),
                    manual_hooks: None,
                    explicit_video_approval: false,
                    signal: signal.clone(),
                },
                prepared,
                false,
                true,
                true,
            ),
        )
        .await
        .expect("YOLO must not wait for native execution approval")
        .unwrap());
        assert!(session.snapshot().unwrap().pending_approval.is_none());
        tools::execute_with_revision_sandboxed(
            &fixture.root,
            &tool,
            Mode::Build,
            Some(&sandbox),
            signal,
        )
        .await
        .unwrap();
        assert!(fs::read_to_string(fixture.root.join("result.txt"))
            .unwrap()
            .contains("yolo"));
    }
}

#[tokio::test]
async fn yolo_does_not_override_cancellation() {
    let fixture = Fixture::new();
    let session = session(&fixture);
    let automatic = options(ApprovalMode::Yolo);
    session
        .reserve("Executar".into(), automatic.clone())
        .unwrap();
    let (_sender, signal) = watch::channel(true);
    let tool = ToolCall {
        id: "cancelled-command".into(),
        name: "bash".into(),
        args: json!({"command":"echo cancelled"}),
        status: "pending".into(),
        output: String::new(),
        duration_ms: 0,
    };
    let error = authorize_with_policy(&session, &tool, &automatic, false, true, signal)
        .await
        .unwrap_err();
    assert_eq!(error.code, "cancelled");
    assert!(session.snapshot().unwrap().pending_approval.is_none());
}

fn openmontage_approval_arguments() -> impl Iterator<Item = Value> {
    std::iter::once(
        json!({"action":"tool","path":"videos/demo","tool":"remote-media","arguments":{}}),
    )
    .chain(
        ["idea", "script", "scene_plan", "assets"].map(
            |stage| json!({"action":"approve","path":"videos/demo","arguments":{"stage":stage}}),
        ),
    )
}

#[tokio::test]
async fn openmontage_execution_and_stages_are_preapproved_in_yolo() {
    for executor in [
        crate::claude::Executor::Jarvis,
        crate::claude::Executor::Claude,
    ] {
        for args in openmontage_approval_arguments() {
            let fixture = Fixture::new();
            let session = session(&fixture);
            let mut automatic = options(ApprovalMode::Yolo);
            automatic.executor = executor;
            let signal = session
                .reserve("Produzir o vídeo".into(), automatic.clone())
                .unwrap();
            let call = ToolCall {
                id: "video-approval".into(),
                name: "video_run".into(),
                args,
                status: "pending".into(),
                output: String::new(),
                duration_ms: 0,
            };
            assert!(tokio::time::timeout(
                Duration::from_secs(1),
                authorize_declared(
                    ApprovalRequest {
                        session: &session,
                        tool: &call,
                        options: &automatic,
                        policy: None,
                        sandbox: None,
                        project_id: None,
                        manual_hooks: None,
                        explicit_video_approval: true,
                        signal,
                    },
                    tool_contract::ApprovalPolicy::Always,
                    tool_contract::Handler::Native,
                ),
            )
            .await
            .expect("YOLO must not wait for OpenMontage execution approval")
            .unwrap());
            assert!(session.snapshot().unwrap().pending_approval.is_none());
        }
    }
}

#[tokio::test]
async fn openmontage_execution_and_stages_wait_for_manual_acceptance_or_denial() {
    for executor in [
        crate::claude::Executor::Jarvis,
        crate::claude::Executor::Claude,
    ] {
        for args in openmontage_approval_arguments() {
            for approved in [false, true] {
                let fixture = Fixture::new();
                let session = session(&fixture);
                let mut manual = options(ApprovalMode::Manual);
                manual.executor = executor;
                let signal = session
                    .reserve("Produzir o vídeo".into(), manual.clone())
                    .unwrap();
                let call = ToolCall {
                    id: "video-approval".into(),
                    name: "video_run".into(),
                    args: args.clone(),
                    status: "pending".into(),
                    output: String::new(),
                    duration_ms: 0,
                };
                let task_session = session.clone();
                let task_call = call.clone();
                let pending = tokio::spawn(async move {
                    authorize_declared(
                        ApprovalRequest {
                            session: &task_session,
                            tool: &task_call,
                            options: &manual,
                            policy: None,
                            sandbox: None,
                            project_id: None,
                            manual_hooks: None,
                            explicit_video_approval: true,
                            signal,
                        },
                        tool_contract::ApprovalPolicy::AccordingToTurn,
                        tool_contract::Handler::Native,
                    )
                    .await
                });
                tokio::time::timeout(Duration::from_secs(2), async {
                    while session.snapshot().unwrap().pending_approval.is_none() {
                        tokio::task::yield_now().await;
                    }
                })
                .await
                .unwrap();
                assert!(!pending.is_finished());
                let snapshot = session.snapshot().unwrap();
                assert_eq!(snapshot.pending_approval.unwrap().tool.name, "video_run");
                answer_approval(
                    &session,
                    &snapshot.active_turn_id.unwrap(),
                    &call.id,
                    approved,
                )
                .unwrap();
                assert_eq!(pending.await.unwrap().unwrap(), approved);
                assert!(session.snapshot().unwrap().pending_approval.is_none());
            }
        }
    }
}

#[tokio::test]
async fn beads_mutations_require_manual_approval_but_queries_do_not() {
    let fixture = Fixture::new();
    let session = session(&fixture);
    let signal = session
        .reserve("Track work".into(), options(ApprovalMode::Manual))
        .unwrap();
    for name in [
        "beads_create",
        "beads_update",
        "beads_claim",
        "beads_close",
        "beads_dependency",
    ] {
        let tool = ToolCall {
            id: name.into(),
            name: name.into(),
            args: json!({}),
            status: "pending".into(),
            output: String::new(),
            duration_ms: 0,
        };
        let (s, t, cancel) = (session.clone(), tool.clone(), signal.clone());
        let pending = tokio::spawn(async move {
            authorize(&s, &t, &options(ApprovalMode::Manual), false, cancel).await
        });
        tokio::time::timeout(Duration::from_secs(1), async {
            while session.snapshot().unwrap().pending_approval.is_none() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        assert!(!pending.is_finished());
        let turn = session.snapshot().unwrap().active_turn_id.unwrap();
        answer_approval(&session, &turn, &tool.id, false).unwrap();
        assert!(!pending.await.unwrap().unwrap());
        assert!(authorize(
            &session,
            &tool,
            &options(ApprovalMode::Yolo),
            false,
            signal.clone()
        )
        .await
        .unwrap());
        assert!(session.snapshot().unwrap().pending_approval.is_none());
    }
    for name in ["beads_list", "beads_ready", "beads_show"] {
        let tool = ToolCall {
            id: name.into(),
            name: name.into(),
            args: json!({}),
            status: "pending".into(),
            output: String::new(),
            duration_ms: 0,
        };
        assert!(authorize(
            &session,
            &tool,
            &options(ApprovalMode::Manual),
            false,
            signal.clone()
        )
        .await
        .unwrap());
        assert!(session.snapshot().unwrap().pending_approval.is_none());
    }
}

#[tokio::test]
async fn read_only_mcp_calls_do_not_prompt_but_mutations_still_require_approval() {
    let fixture = Fixture::new();
    let session = session(&fixture);
    let mut plan = options(ApprovalMode::Manual);
    plan.mode = Mode::Plan;
    let signal = session
        .reserve("Look up documentation".into(), plan.clone())
        .unwrap();
    let tool = ToolCall {
        id: "mcp-call".into(),
        name: "mcp_docs_lookup_hash".into(),
        args: json!({"query":"React"}),
        status: "pending".into(),
        output: String::new(),
        duration_ms: 0,
    };
    assert!(authorize(&session, &tool, &plan, false, signal.clone())
        .await
        .unwrap());
    assert!(session.snapshot().unwrap().pending_approval.is_none());
    let (s, t, p, cancel) = (session.clone(), tool.clone(), plan.clone(), signal.clone());
    let pending = tokio::spawn(async move { authorize(&s, &t, &p, true, cancel).await });
    tokio::time::timeout(Duration::from_secs(1), async {
        while session.snapshot().unwrap().pending_approval.is_none() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert!(!pending.is_finished());
    let turn = session.snapshot().unwrap().active_turn_id.unwrap();
    answer_approval(&session, &turn, &tool.id, false).unwrap();
    assert!(!pending.await.unwrap().unwrap());
    plan.approval_mode = ApprovalMode::Yolo;
    assert!(authorize(&session, &tool, &plan, true, signal)
        .await
        .unwrap());
}

#[test]
fn ipc_snapshot_never_contains_provider_replay_or_credentials() {
    let fixture = Fixture::new();
    let (cancel, _) = watch::channel(false);
    let journal = fixture.root.join("session.jsonl");
    fs::write(&journal, "").unwrap();
    let session = Session {
        id: "conversation".into(),
        journal: journal.clone(),
        root: fixture.root.clone(),
        journal_maintenance: Default::default(),
        writer: session_writer::SessionWriter::start(journal.clone(), "conversation".into(), None)
            .unwrap(),
        emit: Arc::new(|_| {}),
        data: Mutex::new(SessionData {
            revision: 3,
            turn_base: 0,
            wire_base: 0,
            inherited_mcp_intent: crate::mcp::McpIntent::default(),
            active: Some(Active::new("turn".into(), cancel)),
            recovery: None,
            storage_failed: false,
            last_emit: std::time::Instant::now(),
            extras: journal::Extras::default(),
            compacting: false,
            manual_compaction: false,
            turns: vec![StoredTurn {
                excluded_queue_ms: 0,
                wire: vec![json!({"encrypted_content":"private-replay"})],
                mcp_intent: None,
                mcp_parent_intent: None,
                mcp_intent_auxiliary_count: 0,
                turn: Turn {
                    active_since: None,
                    id: "turn".into(),
                    user: "hello".into(),
                    parts: vec![],
                    auxiliary_messages: vec![],
                    context_window: None,
                    created_at: 1,
                    duration_ms: 0,
                    options: TurnOptions {
                        executor: crate::claude::Executor::Jarvis,
                        account: "account-alias".into(),
                        model: "model".into(),
                        reasoning: None,
                        service_tier: None,
                        mode: Mode::Build,
                        workflow: None,
                        custom_workflow_id: None,
                        custom_agent_id: None,
                        approval_mode: ApprovalMode::Manual,
                        manual_validation: false,
                        automatic_publication: None,
                        model_selection: None,
                    },
                    status: TurnStatus::Running,
                    tasks: vec![],
                    steps: vec![],
                    error: None,
                },
            }],
        }),
    };
    let snapshot = serde_json::to_string(&session.snapshot().unwrap()).unwrap();
    assert!(snapshot.contains("hello"));
    assert!(snapshot.contains("account-alias"));
    assert!(!snapshot.contains("private-replay"));
    assert!(!snapshot.contains("wire"));
}

#[tokio::test]
async fn already_cancelled_signal_does_not_wait_for_another_change() {
    let (sender, mut receiver) = watch::channel(false);
    sender.send(true).unwrap();
    tokio::time::timeout(Duration::from_millis(50), cancelled(&mut receiver))
        .await
        .unwrap();
}
