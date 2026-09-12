use super::*;
use std::fs;

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

fn options(approval_mode: ApprovalMode) -> TurnOptions {
    TurnOptions {
        account: "account".into(),
        model: "model".into(),
        reasoning: None,
        mode: Mode::Build,
        workflow: None,
        custom_workflow_id: None,
        custom_agent_id: None,
        approval_mode,
        manual_validation: false,
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
    let (turn_id, inherited, unresolved) =
        pending_mcp_intent_resolution(&data.turns).expect("new turn needs resolution");
    assert_eq!(turn_id, data.turns.last().unwrap().turn.id);
    assert_eq!(inherited, intent);
    assert_eq!(unresolved, vec!["Continue e confirme a informação."]);
}

pub(super) fn session(fixture: &Fixture) -> Arc<Session> {
    let journal = fixture.root.join("session.jsonl");
    fs::write(&journal, "{}\n").unwrap();
    Arc::new(Session {
        id: "conversation".into(),
        journal,
        root: fixture.root.clone(),
        journal_maintenance: Default::default(),
        emit: Arc::new(|_| {}),
        data: Mutex::new(SessionData {
            turns: vec![],
            durable_turn: None,
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

#[test]
fn harness_evaluation_direct_recovery_preserves_durable_results_and_new_messages() {
    let fixture = Fixture::new();
    let session = session(&fixture);
    let mut direct = options(ApprovalMode::Yolo);
    direct.workflow = Some(workflow::Flow::Designer);
    let recovered = StoredTurn {
        mcp_intent: None,
        turn: Turn {
            id: "recovered-turn".into(),
            created_at: 1,
            duration_ms: 0,
            user: "Ajuste o layout".into(),
            parts: vec![],
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
fn harness_evaluation_coordinated_recovery_pairs_uncertain_tools_without_replay() {
    let fixture = Fixture::new();
    let session = session(&fixture);
    let mut coordinated = options(ApprovalMode::Yolo);
    coordinated.workflow = Some(workflow::Flow::Complete);
    let turn = StoredTurn {
        mcp_intent: None,
        turn: Turn {
            id: "workflow-turn".into(),
            created_at: 1,
            duration_ms: 0,
            user: "Execute o fluxo".into(),
            parts: vec![],
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
        data.durable_turn = Some(turn);
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
        .any(|item| item["_jarvis_workflow_recovery"] == true));
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
async fn ownership_sensitive_actions_still_prompt_in_automatic_mode() {
    let fixture = Fixture::new();
    let session = session(&fixture);
    let automatic = options(ApprovalMode::Yolo);
    let signal = session
        .reserve("Fechar terminal".into(), automatic.clone())
        .unwrap();
    let tool = ToolCall {
        id: "terminal-close".into(),
        name: "terminal_close".into(),
        args: json!({"id":"terminal-user","reason":"A verificação terminou."}),
        status: "pending".into(),
        output: String::new(),
        duration_ms: 0,
    };
    let (task_session, task_tool, task_options) =
        (session.clone(), tool.clone(), automatic.clone());
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
    let session = Session {
        id: "conversation".into(),
        journal: fixture.root.join("session.jsonl"),
        root: fixture.root.clone(),
        journal_maintenance: Default::default(),
        emit: Arc::new(|_| {}),
        data: Mutex::new(SessionData {
            revision: 3,
            active: Some(Active {
                id: "turn".into(),
                cancel,
                approval: None,
                question: None,
                authoring: None,
                accepting_auxiliary: true,
            }),
            recovery: None,
            storage_failed: false,
            last_emit: std::time::Instant::now(),
            extras: journal::Extras::default(),
            compacting: false,
            manual_compaction: false,
            turns: vec![StoredTurn {
                wire: vec![json!({"encrypted_content":"private-replay"})],
                mcp_intent: None,
                turn: Turn {
                    id: "turn".into(),
                    user: "hello".into(),
                    parts: vec![],
                    context_window: None,
                    created_at: 1,
                    duration_ms: 0,
                    options: TurnOptions {
                        account: "account-alias".into(),
                        model: "model".into(),
                        reasoning: None,
                        mode: Mode::Build,
                        workflow: None,
                        custom_workflow_id: None,
                        custom_agent_id: None,
                        approval_mode: ApprovalMode::Manual,
                        manual_validation: false,
                    },
                    status: TurnStatus::Running,
                    tasks: vec![],
                    steps: vec![],
                    error: None,
                },
            }],
            durable_turn: None,
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
