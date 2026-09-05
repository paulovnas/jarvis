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
        approval_mode,
    }
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
            active: None,
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
            }),
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
                    context_window: None,
                    created_at: 1,
                    duration_ms: 0,
                    options: TurnOptions {
                        account: "account-alias".into(),
                        model: "model".into(),
                        reasoning: None,
                        mode: Mode::Build,
                        approval_mode: ApprovalMode::Manual,
                    },
                    status: TurnStatus::Running,
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
