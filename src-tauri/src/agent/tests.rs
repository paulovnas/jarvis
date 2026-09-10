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
pub(super) fn session(fixture: &Fixture) -> Arc<Session> {
    let journal = fixture.root.join("session.jsonl");
    fs::write(&journal, "{}\n").unwrap();
    Arc::new(Session {
        id: "conversation".into(),
        journal,
        root: fixture.root.clone(),
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
fn direct_recovery_resumes_only_durable_tool_results_and_preserves_new_messages() {
    let fixture = Fixture::new();
    let session = session(&fixture);
    let mut direct = options(ApprovalMode::Yolo);
    direct.workflow = Some(workflow::Flow::Designer);
    let recovered = StoredTurn {
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
    assert!(session.resume_recovered_turn().unwrap().is_some());
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
    let path = fixture
        .root
        .join(".jarvis/sessions")
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
        authorize(&session, &tool, &options(ApprovalMode::Yolo), signal)
            .await
            .unwrap()
    );
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
        let pending =
            tokio::spawn(
                async move { authorize(&s, &t, &options(ApprovalMode::Manual), cancel).await },
            );
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
            signal.clone()
        )
        .await
        .unwrap());
        assert!(session.snapshot().unwrap().pending_approval.is_none());
    }
}

#[tokio::test]
async fn mcp_calls_require_manual_approval_even_in_plan() {
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
    let (s, t, p, cancel) = (session.clone(), tool.clone(), plan.clone(), signal.clone());
    let pending = tokio::spawn(async move { authorize(&s, &t, &p, cancel).await });
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
    assert!(authorize(&session, &tool, &plan, signal).await.unwrap());
}

#[test]
fn ipc_snapshot_never_contains_provider_replay_or_credentials() {
    let fixture = Fixture::new();
    let (cancel, _) = watch::channel(false);
    let session = Session {
        id: "conversation".into(),
        journal: fixture.root.join("session.jsonl"),
        root: fixture.root.clone(),
        emit: Arc::new(|_| {}),
        data: Mutex::new(SessionData {
            revision: 3,
            active: Some(Active {
                id: "turn".into(),
                cancel,
                approval: None,
                question: None,
                authoring: None,
            }),
            recovery: None,
            storage_failed: false,
            last_emit: std::time::Instant::now(),
            extras: journal::Extras::default(),
            compacting: false,
            manual_compaction: false,
            turns: vec![StoredTurn {
                wire: vec![json!({"encrypted_content":"private-replay"})],
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
