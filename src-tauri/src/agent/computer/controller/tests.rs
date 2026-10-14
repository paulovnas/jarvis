use super::*;
use std::sync::atomic::AtomicUsize;

#[derive(Default)]
struct FakeNative {
    input: Mutex<String>,
    moved: AtomicBool,
    context_changed: AtomicBool,
    change_during_capture: AtomicBool,
    restarted: AtomicBool,
    unavailable: AtomicBool,
    permission: AtomicBool,
    uncertain: AtomicBool,
    pause_input: AtomicBool,
    png: AtomicBool,
    pause_observation: AtomicBool,
    observing: AtomicBool,
    effects: AtomicUsize,
}

#[tokio::test]
async fn desktop_feedback_survives_an_action_but_closes_with_its_turn() {
    let (state, _, _, _) = setup();
    let (sender, signal) = watch::channel(false);
    let operation = guard();
    let generation = state
        .begin(
            "a",
            "run",
            "one",
            Some("target"),
            "computer_observe",
            &operation,
        )
        .unwrap();
    let token = state
        .feedback("a", "run", generation, "target", signal.clone())
        .unwrap();
    state
        .finish("a", generation, "one", "observed", "Ready")
        .unwrap();
    operation.cancel();
    assert!(
        !token.load(Ordering::SeqCst),
        "between-action preview belongs to the turn"
    );
    let next = state
        .feedback("a", "run", generation, "target", signal)
        .unwrap();
    assert!(Arc::ptr_eq(&token, &next));
    drop(sender);
    tokio::task::yield_now().await;
    assert!(
        token.load(Ordering::SeqCst),
        "normal turn completion hides feedback"
    );
}

#[tokio::test]
async fn desktop_feedback_stop_and_replacement_are_scoped_to_the_owner_and_target() {
    let (state, _, _, _) = setup();
    let (_sender, signal) = watch::channel(false);
    let a = state
        .begin(
            "a",
            "run",
            "one",
            Some("target"),
            "computer_observe",
            &guard(),
        )
        .unwrap();
    let first = state
        .feedback("a", "run", a, "target", signal.clone())
        .unwrap();
    let replacement = state
        .feedback("a", "run", a, "other-target", signal.clone())
        .unwrap();
    assert!(first.load(Ordering::SeqCst));
    let b = state
        .begin(
            "b",
            "other-run",
            "two",
            Some("target"),
            "computer_observe",
            &guard(),
        )
        .unwrap();
    let other_chat = state
        .feedback("b", "other-run", b, "target", signal.clone())
        .unwrap();
    state.stop("a").unwrap();
    assert!(replacement.load(Ordering::SeqCst));
    assert!(!other_chat.load(Ordering::SeqCst));
    let next_owner = state
        .begin(
            "b",
            "new-run",
            "three",
            Some("target"),
            "computer_observe",
            &guard(),
        )
        .unwrap();
    assert!(other_chat.load(Ordering::SeqCst));
    assert!(state
        .feedback("b", "other-run", b, "target", signal.clone())
        .is_err());
    let current = state
        .feedback("b", "new-run", next_owner, "target", signal)
        .unwrap();
    state.set_mode(ComputerMode::Yolo).unwrap();
    assert!(current.load(Ordering::SeqCst));
}

fn fixture() -> ComputerApp {
    ComputerApp {
        app_id: "fixture-42-1".into(),
        name: "Fixture".into(),
        identity: AppIdentity {
            bundle_id: "dev.jarvis.fixture".into(),
            bundle_path: "/Applications/Fixture.app".into(),
            executable_path: "/Applications/Fixture.app/Contents/MacOS/Fixture".into(),
        },
        process_id: 42,
        launch_id: "1".into(),
        windows: vec![ComputerWindow {
            id: 7,
            title: "Disposable fixture".into(),
            bounds: Bounds {
                x: -1200.0,
                y: 30.0,
                width: 500.0,
                height: 350.0,
            },
        }],
    }
}

impl NativeBackend for FakeNative {
    fn capabilities(&self) -> Result<ComputerCapabilities, AgentError> {
        Ok(ComputerCapabilities {
            platform: "macos".into(),
            supported: true,
            accessibility: self.permission.load(Ordering::SeqCst),
            screen_capture: true,
            input_posting: true,
            capture_supported: true,
            session_status: "interactive".into(),
            reason: None,
        })
    }
    fn apps(&self, guard: &OperationGuard) -> Result<Vec<ComputerApp>, AgentError> {
        guard.check()?;
        let mut app = fixture();
        if self.context_changed.load(Ordering::SeqCst) {
            app.windows[0].title = "Unrelated browser tab".into();
        }
        Ok(vec![app])
    }
    fn window(
        &self,
        target: &NativeTarget,
        guard: &OperationGuard,
    ) -> Result<ComputerWindow, AgentError> {
        guard.check()?;
        if self.unavailable.load(Ordering::SeqCst) {
            return Err(AgentError::new(
                "computer_target_unavailable",
                "A instância da fixture encerrou.",
            ));
        }
        if self.restarted.load(Ordering::SeqCst) || target.app.identity != fixture().identity {
            return Err(AgentError::new(
                "computer_stale_app",
                "Aplicativo reiniciado.",
            ));
        }
        let mut window = fixture().windows.remove(0);
        if self.moved.load(Ordering::SeqCst) {
            window.bounds.x += 25.0;
        }
        if self.context_changed.load(Ordering::SeqCst) {
            window.title = "Unrelated browser tab".into();
        }
        Ok(window)
    }
    fn observe(
        &self,
        target: &NativeTarget,
        observation_id: &str,
        _: bool,
        guard: &OperationGuard,
    ) -> Result<NativeObservation, AgentError> {
        self.observing.store(true, Ordering::SeqCst);
        while self.pause_observation.load(Ordering::SeqCst) {
            std::thread::sleep(Duration::from_millis(2));
            guard.check()?;
        }
        let observed = NativeObservation {
            window: self.window(target, guard)?,
            elements: vec![ComputerElement {
                id: format!("{observation_id}:field"),
                role: "AXTextField".into(),
                title: "Fixture input".into(),
                value: Some(self.input.lock().unwrap().clone()),
                bounds: None,
                enabled: true,
                protected: false,
                actions: Vec::new(),
            }],
            truncated: false,
            png: if self.png.load(Ordering::SeqCst) {
                let mut output = std::io::Cursor::new(Vec::new());
                image::DynamicImage::new_rgba8(1000, 700)
                    .write_to(&mut output, image::ImageFormat::Png)
                    .unwrap();
                Some(output.into_inner())
            } else {
                None
            },
            capture: Some(CaptureGeometry {
                pixel_width: 1000,
                pixel_height: 700,
                scale_x: 2.0,
                scale_y: 2.0,
            }),
            errors: Vec::new(),
        };
        if self.change_during_capture.load(Ordering::SeqCst) {
            self.context_changed.store(true, Ordering::SeqCst);
        }
        Ok(observed)
    }
    fn act(
        &self,
        _: &NativeTarget,
        _: &Observation,
        action: &ComputerAction,
        guard: &OperationGuard,
    ) -> Result<(), AgentError> {
        if !self.permission.load(Ordering::SeqCst) {
            return Err(AgentError::new(
                "computer_permission",
                "Acessibilidade não concedida.",
            ));
        }
        guard.before_dispatch()?;
        self.effects.fetch_add(1, Ordering::SeqCst);
        if let ComputerAction::Type { text, .. } = action {
            self.input.lock().unwrap().push_str(text);
        }
        while self.pause_input.load(Ordering::SeqCst) {
            std::thread::sleep(Duration::from_millis(2));
            guard.check()?;
        }
        if self.uncertain.load(Ordering::SeqCst) {
            return Err(AgentError::new("computer_native", "Falha após envio."));
        }
        Ok(())
    }
    fn request_permission(
        &self,
        _: ComputerPermission,
    ) -> Result<ComputerCapabilities, AgentError> {
        self.capabilities()
    }
}

pub(in crate::agent) fn ready_state(mode: ComputerMode) -> ComputerState {
    let native = Arc::new(FakeNative::default());
    native.permission.store(true, Ordering::SeqCst);
    let state = ComputerState::new(native);
    state.set_mode(mode).unwrap();
    state
}

fn setup() -> (
    ComputerState,
    Arc<FakeNative>,
    GrantStore,
    tempfile::TempDir,
) {
    let native = Arc::new(FakeNative::default());
    native.permission.store(true, Ordering::SeqCst);
    let state = ComputerState::new(native.clone());
    let grants = GrantStore::default();
    grants
        .create_computer(
            "a",
            fixture().identity,
            ComputerAccess::Control,
            crate::agent::now(),
        )
        .unwrap();
    (state, native, grants, tempfile::tempdir().unwrap())
}

fn tool(name: &str, args: Value) -> ToolCall {
    ToolCall {
        id: id().unwrap(),
        name: name.into(),
        args,
        status: "pending".into(),
        output: String::new(),
        duration_ms: 0,
    }
}

#[tokio::test]
async fn ga4_changed_window_context_is_rejected_before_capture_or_input() {
    let (state, native, grants, home) = setup();
    let selected = run(
        &state,
        &grants,
        home.path(),
        "run",
        tool(
            "computer_select",
            json!({"appId":"fixture-42-1","windowId":7,"access":"control"}),
        ),
    )
    .await
    .unwrap();
    let target = selected["targetId"].clone();
    let observed = run(
        &state,
        &grants,
        home.path(),
        "run",
        tool("computer_observe", json!({"targetId":target})),
    )
    .await
    .unwrap();
    native.observing.store(false, Ordering::SeqCst);
    native.context_changed.store(true, Ordering::SeqCst);
    let error = run(
        &state,
        &grants,
        home.path(),
        "run",
        tool("computer_observe", json!({"targetId":target})),
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "computer_context_changed");
    assert!(
        !native.observing.load(Ordering::SeqCst),
        "do not capture the unrelated tab"
    );
    let error = run(
        &state,
        &grants,
        home.path(),
        "run",
        tool(
            "computer_click",
            json!({"targetId":target,"observationId":observed["observation"]["id"],"x":10,"y":20}),
        ),
    )
    .await
    .unwrap_err();
    assert!(
        !serde_json::from_str::<Value>(error.tool_result.as_ref().unwrap()).unwrap()["inputSent"]
            .as_bool()
            .unwrap()
    );
    assert_eq!(native.effects.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn ga4_context_switch_during_capture_does_not_publish_observation() {
    let (state, native, grants, home) = setup();
    let selected = run(
        &state,
        &grants,
        home.path(),
        "run",
        tool(
            "computer_select",
            json!({"appId":"fixture-42-1","windowId":7,"access":"observe"}),
        ),
    )
    .await
    .unwrap();
    native.change_during_capture.store(true, Ordering::SeqCst);
    let error = run(
        &state,
        &grants,
        home.path(),
        "run",
        tool("computer_observe", json!({"targetId":selected["targetId"]})),
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "computer_context_changed");
    assert!(native.observing.load(Ordering::SeqCst));
    assert!(state.conversations.lock().unwrap()["a"]
        .observations
        .is_empty());
    assert!(!crate::data_dir::root(home.path())
        .join("attachments")
        .exists());
}

async fn run(
    state: &ComputerState,
    grants: &GrantStore,
    home: &std::path::Path,
    owner: &str,
    tool: ToolCall,
) -> Result<Value, AgentError> {
    let receipt = run_unacknowledged(state, grants, home, owner, &tool).await?;
    acknowledge_saved_input(state, "a", &tool, &receipt);
    Ok(receipt)
}

fn acknowledge_saved_input(
    state: &ComputerState,
    conversation: &str,
    tool: &ToolCall,
    receipt: &Value,
) {
    // Test callers simulate the executor's durable receipt boundary explicitly.
    if receipt["phase"] == "observed" && receipt["inputSent"] == true {
        state
            .confirmed(
                conversation,
                &tool.id,
                receipt["targetId"].as_str().unwrap(),
                receipt["operationId"].as_str().unwrap(),
            )
            .unwrap();
    }
}

async fn run_unacknowledged(
    state: &ComputerState,
    grants: &GrantStore,
    home: &std::path::Path,
    owner: &str,
    tool: &ToolCall,
) -> Result<Value, AgentError> {
    let (_send, signal) = watch::channel(false);
    state
        .run("a", owner, tool, grants, home.to_path_buf(), None, signal)
        .await
        .map(|output| serde_json::from_str(&output).unwrap())
}

#[tokio::test]
async fn computer_yolo_controls_a_fresh_conversation_without_application_grants() {
    let (state, native, _, home) = setup();
    let grants = GrantStore::default();
    state.set_mode(ComputerMode::Yolo).unwrap();
    let target = select(&state, &grants, home.path(), "run").await;
    let observation = observe(&state, &grants, home.path(), "run", &target).await;
    let receipt = run(&state, &grants, home.path(), "run", tool("computer_type", json!({
        "targetId": target, "observationId": observation["observation"]["id"], "text": "autonomous"
    }))).await.unwrap();
    assert_eq!(receipt["inputSent"], true);
    assert_eq!(*native.input.lock().unwrap(), "autonomous");
    assert!(state
        .snapshot(Some("a"), &grants)
        .unwrap()
        .grants
        .is_empty());
    assert_eq!(state.snapshot(Some("a"), &grants).unwrap().targets.len(), 1);
}

#[tokio::test]
async fn missing_macos_permissions_explain_setup_before_any_native_input_in_yolo() {
    let (state, native, _, home) = setup();
    state.set_mode(ComputerMode::Yolo).unwrap();
    native.permission.store(false, Ordering::SeqCst);
    let error = run(
        &state,
        &GrantStore::default(),
        home.path(),
        "run",
        tool(
            "computer_select",
            json!({"appId":"fixture-42-1","windowId":7,"access":"control"}),
        ),
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "computer_permission_required");
    assert!(error.message.contains("Acessibilidade"));
    let recovery: Value = serde_json::from_str(error.tool_result.as_deref().unwrap()).unwrap();
    assert_eq!(recovery["permissions"], json!(["accessibility"]));
    assert_eq!(recovery["settings"]["section"], "computer");
    assert_eq!(recovery["inputSent"], false);
    assert_eq!(native.effects.load(Ordering::SeqCst), 0);
    assert_eq!(
        state
            .snapshot(Some("a"), &GrantStore::default())
            .unwrap()
            .activity
            .unwrap()
            .phase,
        "error"
    );
}

#[tokio::test]
async fn switching_from_yolo_to_manual_revokes_ungranted_target_access() {
    let (state, native, _, home) = setup();
    let grants = GrantStore::default();
    state.set_mode(ComputerMode::Yolo).unwrap();
    let target = select(&state, &grants, home.path(), "run").await;
    let observation = observe(&state, &grants, home.path(), "run", &target).await;
    state.set_mode(ComputerMode::Manual).unwrap();
    let error = run(&state, &grants, home.path(), "run", tool("computer_type", json!({"targetId":target,"observationId":observation["observation"]["id"],"text":"blocked"}))).await.unwrap_err();
    assert_eq!(error.code, "computer_app_authorization");
    assert_eq!(native.effects.load(Ordering::SeqCst), 0);
    assert!(state
        .snapshot(Some("a"), &grants)
        .unwrap()
        .targets
        .is_empty());
}

async fn select(
    state: &ComputerState,
    grants: &GrantStore,
    home: &std::path::Path,
    owner: &str,
) -> String {
    run(
        state,
        grants,
        home,
        owner,
        tool(
            "computer_select",
            json!({"appId":"fixture-42-1","windowId":7,"access":"control"}),
        ),
    )
    .await
    .unwrap()["targetId"]
        .as_str()
        .unwrap()
        .into()
}

async fn observe(
    state: &ComputerState,
    grants: &GrantStore,
    home: &std::path::Path,
    owner: &str,
    target: &str,
) -> Value {
    run(
        state,
        grants,
        home,
        owner,
        tool(
            "computer_observe",
            json!({"targetId":target,"capture":false}),
        ),
    )
    .await
    .unwrap()
}

#[tokio::test]
async fn control_cycle_dispatches_once_and_returns_a_new_observation() {
    let (state, native, grants, home) = setup();
    let target = select(&state, &grants, home.path(), "run:agent").await;
    let before = observe(&state, &grants, home.path(), "run:agent", &target).await;
    let after = run(&state, &grants, home.path(), "run:agent", tool("computer_type", json!({"targetId":target,"observationId":before["observation"]["id"],"element":before["observation"]["elements"][0]["id"],"text":"Olá fixture"}))).await.unwrap();
    assert_eq!(after["inputSent"], true);
    assert_eq!(after["phase"], "observed");
    assert_eq!(after["observation"]["elements"][0]["value"], "Olá fixture");
    assert_ne!(after["observation"]["id"], before["observation"]["id"]);
    assert_eq!(native.effects.load(Ordering::SeqCst), 1);
    let old = run(
        &state,
        &grants,
        home.path(),
        "run:agent",
        tool(
            "computer_click",
            json!({"targetId":target,"observationId":before["observation"]["id"],"x":10,"y":10}),
        ),
    )
    .await
    .unwrap_err();
    assert_eq!(old.code, "computer_stale_observation");
    assert_eq!(native.effects.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn authorization_is_not_inherited_by_another_conversation_or_observe_only_target() {
    let (state, native, grants, home) = setup();
    let (_send, signal) = watch::channel(false);
    let select_tool = tool(
        "computer_select",
        json!({"appId":"fixture-42-1","windowId":7,"access":"control"}),
    );
    let error = state
        .run(
            "b",
            "run:agent",
            &select_tool,
            &grants,
            home.path().into(),
            None,
            signal,
        )
        .await
        .unwrap_err();
    assert_eq!(error.code, "computer_app_authorization");
    grants
        .create_computer(
            "a",
            fixture().identity,
            ComputerAccess::Observe,
            crate::agent::now(),
        )
        .unwrap();
    let error = run(&state, &grants, home.path(), "run:agent", select_tool)
        .await
        .unwrap_err();
    assert_eq!(error.code, "computer_app_authorization");
    assert_eq!(native.effects.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn moved_geometry_restarts_revocation_and_lost_permission_prevent_dispatch() {
    let (state, native, grants, home) = setup();
    let target = select(&state, &grants, home.path(), "run:agent").await;
    let before = observe(&state, &grants, home.path(), "run:agent", &target).await;
    let click = || {
        tool(
            "computer_click",
            json!({"targetId":target,"observationId":before["observation"]["id"],"x":10,"y":10}),
        )
    };
    native.moved.store(true, Ordering::SeqCst);
    assert_eq!(
        run(&state, &grants, home.path(), "run:agent", click())
            .await
            .unwrap_err()
            .code,
        "computer_stale_observation"
    );
    native.moved.store(false, Ordering::SeqCst);
    let before = observe(&state, &grants, home.path(), "run:agent", &target).await;
    native.restarted.store(true, Ordering::SeqCst);
    assert_eq!(
        run(
            &state,
            &grants,
            home.path(),
            "run:agent",
            tool(
                "computer_press",
                json!({"targetId":target,"observationId":before["observation"]["id"],"key":"Tab"})
            )
        )
        .await
        .unwrap_err()
        .code,
        "computer_stale_app"
    );
    native.restarted.store(false, Ordering::SeqCst);
    let before = observe(&state, &grants, home.path(), "run:agent", &target).await;
    native.permission.store(false, Ordering::SeqCst);
    assert_eq!(
        run(
            &state,
            &grants,
            home.path(),
            "run:agent",
            tool(
                "computer_press",
                json!({"targetId":target,"observationId":before["observation"]["id"],"key":"Tab"})
            )
        )
        .await
        .unwrap_err()
        .code,
        "computer_permission_required"
    );
    native.permission.store(true, Ordering::SeqCst);
    let grant = grants
        .computer_grants("a", crate::agent::now())
        .unwrap()
        .remove(0);
    state.revoked("a", &fixture().identity).unwrap();
    grants.revoke_computer("a", &grant.id).unwrap();
    let unavailable = run(
        &state,
        &grants,
        home.path(),
        "run:new",
        tool("computer_observe", json!({"targetId":target})),
    )
    .await
    .unwrap();
    assert_eq!(unavailable["available"], false);
    assert_eq!(unavailable["inputSent"], false);
    assert!(unavailable.get("attachment").is_none());
    assert!(unavailable.get("observation").is_none());
    assert_eq!(native.effects.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn host_generation_and_element_refs_cannot_cross_agent_runs() {
    let (state, native, grants, home) = setup();
    let target = select(&state, &grants, home.path(), "run:first").await;
    let before = observe(&state, &grants, home.path(), "run:first", &target).await;
    let error = run(&state, &grants, home.path(), "run:second", tool("computer_type", json!({"targetId":target,"observationId":before["observation"]["id"],"text":"not allowed"}))).await.unwrap_err();
    assert_eq!(error.code, "computer_stale_observation");
    let current = observe(&state, &grants, home.path(), "run:second", &target).await;
    let error = run(&state, &grants, home.path(), "run:second", tool("computer_click", json!({"targetId":target,"observationId":current["observation"]["id"],"element":before["observation"]["elements"][0]["id"]}))).await.unwrap_err();
    assert_eq!(error.code, "computer_stale_observation");
    assert_eq!(native.effects.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn cancellation_before_input_sends_nothing_and_post_dispatch_failure_is_uncertain() {
    let (state, native, grants, home) = setup();
    let target = select(&state, &grants, home.path(), "run:agent").await;
    let before = observe(&state, &grants, home.path(), "run:agent", &target).await;
    let input = tool(
        "computer_type",
        json!({"targetId":target,"observationId":before["observation"]["id"],"text":"one"}),
    );
    let (_send, signal) = watch::channel(true);
    let error = state
        .run(
            "a",
            "run:agent",
            &input,
            &grants,
            home.path().into(),
            None,
            signal,
        )
        .await
        .unwrap_err();
    assert_eq!(error.code, "cancelled");
    assert_eq!(native.effects.load(Ordering::SeqCst), 0);
    native.uncertain.store(true, Ordering::SeqCst);
    let error = run(&state, &grants, home.path(), "run:agent", input.clone())
        .await
        .unwrap_err();
    assert_eq!(error.code, "computer_action_uncertain");
    let receipt: Value = serde_json::from_str(error.tool_result.as_deref().unwrap()).unwrap();
    assert_eq!(receipt["inputSent"], true);
    assert_eq!(receipt["phase"], "uncertain");
    assert_eq!(receipt["cause"]["code"], "computer_native");
    assert_eq!(receipt["cause"]["message"], "Falha após envio.");
    assert_eq!(*native.input.lock().unwrap(), "one");
    assert!(run(&state, &grants, home.path(), "run:agent", input)
        .await
        .is_err());
    assert_eq!(native.effects.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn stopped_observation_cannot_publish_late_state_or_enable_more_input() {
    let (state, native, grants, home) = setup();
    let target = select(&state, &grants, home.path(), "run:agent").await;
    native.observing.store(false, Ordering::SeqCst);
    native.pause_observation.store(true, Ordering::SeqCst);
    let (_send, signal) = watch::channel(false);
    let worker_state = state.clone();
    let worker_grants = grants.clone();
    let worker_home = home.path().to_path_buf();
    let worker_target = target.clone();
    let task = tokio::spawn(async move {
        worker_state
            .run(
                "a",
                "run:agent",
                &tool("computer_observe", json!({"targetId":worker_target})),
                &worker_grants,
                worker_home,
                None,
                signal,
            )
            .await
    });
    for _ in 0..200 {
        if native.observing.load(Ordering::SeqCst) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(2)).await;
    }
    assert!(native.observing.load(Ordering::SeqCst));
    state.stop("a").unwrap();
    native.pause_observation.store(false, Ordering::SeqCst);
    assert!(task.await.unwrap().is_err());
    let snapshot = state.snapshot(Some("a"), &grants).unwrap();
    assert!(snapshot.stopped);
    assert_eq!(snapshot.activity.unwrap().phase, "stopped");
    assert!(state.conversations.lock().unwrap()["a"]
        .observations
        .is_empty());
    let error = run(
        &state,
        &grants,
        home.path(),
        "run:agent",
        tool(
            "computer_press",
            json!({"targetId":target,"observationId":"old","key":"Tab"}),
        ),
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "computer_stopped");
    assert_eq!(native.effects.load(Ordering::SeqCst), 0);
}

#[test]
fn coordinate_and_secure_element_checks_reject_ambiguous_or_unobserved_input() {
    let observation = Observation {
        id: "obs".into(),
        target_id: "target".into(),
        generation: 1,
        revision: 1,
        window: fixture().windows.remove(0),
        elements: vec![ComputerElement {
            id: "secret".into(),
            role: "AXSecureTextField".into(),
            title: "Password".into(),
            value: None,
            bounds: None,
            enabled: true,
            protected: true,
            actions: Vec::new(),
        }],
        truncated: false,
        capture: Some(CaptureGeometry {
            pixel_width: 1000,
            pixel_height: 700,
            scale_x: 2.0,
            scale_y: 2.0,
        }),
        errors: Vec::new(),
    };
    for action in [
        ComputerAction::Click {
            element: Some("secret".into()),
            x: Some(1.0),
            y: Some(1.0),
        },
        ComputerAction::Click {
            element: None,
            x: Some(1000.0),
            y: Some(1.0),
        },
        ComputerAction::Click {
            element: None,
            x: Some(f64::NAN),
            y: Some(1.0),
        },
        ComputerAction::Type {
            text: "secret".into(),
            element: Some("secret".into()),
        },
        ComputerAction::Drag {
            from: Point { x: 0.0, y: 0.0 },
            to: Point { x: -1.0, y: 2.0 },
        },
    ] {
        assert!(validate_action(&action, &observation).is_err());
    }
    assert!(validate_action(
        &ComputerAction::Click {
            element: None,
            x: Some(999.0),
            y: Some(699.0)
        },
        &observation
    )
    .is_ok());
    let without_capture = Observation {
        capture: None,
        ..observation
    };
    assert!(validate_action(
        &ComputerAction::Click {
            element: None,
            x: Some(1.0),
            y: Some(1.0)
        },
        &without_capture
    )
    .is_err());
}

#[tokio::test]
async fn cancellation_after_input_records_uncertainty_without_replaying_the_text() {
    let (state, native, grants, home) = setup();
    let target = select(&state, &grants, home.path(), "run:agent").await;
    let before = observe(&state, &grants, home.path(), "run:agent", &target).await;
    native.pause_input.store(true, Ordering::SeqCst);
    let (send, signal) = watch::channel(false);
    let worker_state = state.clone();
    let worker_grants = grants.clone();
    let worker_home = home.path().to_path_buf();
    let worker_target = target.clone();
    let task = tokio::spawn(async move {
        worker_state.run("a", "run:agent", &tool("computer_type", json!({"targetId":worker_target,"observationId":before["observation"]["id"],"text":"only once"})), &worker_grants, worker_home, None, signal).await
    });
    for _ in 0..200 {
        if native.effects.load(Ordering::SeqCst) == 1 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(2)).await;
    }
    assert_eq!(native.effects.load(Ordering::SeqCst), 1);
    send.send(true).unwrap();
    let error = task.await.unwrap().unwrap_err();
    assert_eq!(error.code, "computer_action_uncertain");
    assert_eq!(
        serde_json::from_str::<Value>(error.tool_result.as_deref().unwrap()).unwrap()["inputSent"],
        true
    );
    native.pause_input.store(false, Ordering::SeqCst);
    let after = observe(&state, &grants, home.path(), "run:new", &target).await;
    assert_eq!(after["observation"]["elements"][0]["value"], "only once");
    assert_eq!(native.effects.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn expired_observations_and_forged_host_scope_are_rejected_without_input() {
    let (state, native, grants, home) = setup();
    let target = select(&state, &grants, home.path(), "run:agent").await;
    let before = observe(&state, &grants, home.path(), "run:agent", &target).await;
    {
        let mut all = state.conversations.lock().unwrap();
        all.get_mut("a")
            .unwrap()
            .observations
            .get_mut(&target)
            .unwrap()
            .created = Instant::now() - OBSERVATION_AGE - Duration::from_secs(1);
    }
    let error = run(
        &state,
        &grants,
        home.path(),
        "run:agent",
        tool(
            "computer_press",
            json!({"targetId":target,"observationId":before["observation"]["id"],"key":"Tab"}),
        ),
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "computer_stale_observation");
    for args in [
        json!({"conversationId":"b"}),
        json!({"generation":999}),
        json!({"owner":"run:new"}),
    ] {
        assert_eq!(
            run(
                &state,
                &grants,
                home.path(),
                "run:agent",
                tool("computer_status", args)
            )
            .await
            .unwrap_err()
            .code,
            "computer_invalid"
        );
    }
    assert_eq!(native.effects.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn capture_is_an_existing_conversation_attachment_that_vision_can_read() {
    let (state, native, grants, home) = setup();
    let conversation = "a".repeat(32);
    grants
        .create_computer(
            &conversation,
            fixture().identity,
            ComputerAccess::Observe,
            crate::agent::now(),
        )
        .unwrap();
    native.png.store(true, Ordering::SeqCst);
    let (_send, signal) = watch::channel(false);
    let selected: Value = serde_json::from_str(
        &state
            .run(
                &conversation,
                "run:agent",
                &tool(
                    "computer_select",
                    json!({"appId":"fixture-42-1","windowId":7,"access":"observe"}),
                ),
                &grants,
                home.path().into(),
                None,
                signal.clone(),
            )
            .await
            .unwrap(),
    )
    .unwrap();
    let observed: Value = serde_json::from_str(
        &state
            .run(
                &conversation,
                "run:agent",
                &tool("computer_observe", json!({"targetId":selected["targetId"]})),
                &grants,
                home.path().into(),
                None,
                signal,
            )
            .await
            .unwrap(),
    )
    .unwrap();
    let attachment_id = observed["attachment"]["id"].as_str().unwrap();
    let metadata = attachments::metadata(home.path(), &conversation, attachment_id).unwrap();
    assert_eq!(metadata.kind, "image");
    assert_eq!(metadata.mime, "image/png");
    assert_eq!(metadata.conversation_id, conversation);
    assert_eq!(observed["observation"]["capture"]["scaleX"], 2.0);
    assert_eq!(observed["untrusted"], true);
    assert_eq!(observed["inputSent"], false);
    let bytes = attachments::bounded_read(
        &attachments::location(home.path(), &conversation, attachment_id)
            .unwrap()
            .join("source"),
        attachments::MAX_BYTES,
    )
    .unwrap();
    assert_eq!(image::load_from_memory(&bytes).unwrap().width(), 1000);
}

#[test]
fn cancellation_and_dispatch_have_one_ordered_boundary() {
    for _ in 0..100 {
        let guard = OperationGuard::new(Instant::now() + Duration::from_secs(2));
        let ready = Arc::new(std::sync::Barrier::new(2));
        let worker_guard = guard.clone();
        let worker_ready = ready.clone();
        let worker = std::thread::spawn(move || {
            worker_ready.wait();
            worker_guard.before_dispatch().is_ok()
        });
        ready.wait();
        let input_sent = guard.cancel();
        let dispatched = worker.join().unwrap();
        if !input_sent {
            assert!(
                !dispatched,
                "input cannot start after cancellation reported no dispatch"
            );
        }
    }
}

#[tokio::test]
async fn timeout_after_input_keeps_the_uncertain_receipt_and_releases_the_worker() {
    let (state, native, grants, home) = setup();
    let target = select(&state, &grants, home.path(), "run:agent").await;
    let before = observe(&state, &grants, home.path(), "run:agent", &target).await;
    native.pause_input.store(true, Ordering::SeqCst);
    let error = run(&state, &grants, home.path(), "run:agent", tool("computer_type", json!({"targetId":target,"observationId":before["observation"]["id"],"text":"timeout once"}))).await.unwrap_err();
    assert_eq!(error.code, "computer_action_uncertain");
    assert_eq!(
        serde_json::from_str::<Value>(error.tool_result.as_deref().unwrap()).unwrap()["inputSent"],
        true
    );
    native.pause_input.store(false, Ordering::SeqCst);
    let after = observe(&state, &grants, home.path(), "run:new", &target).await;
    assert_eq!(after["observation"]["elements"][0]["value"], "timeout once");
    assert_eq!(native.effects.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn uncertain_target_cannot_be_bypassed_by_reselection_or_another_worker_observation() {
    let (state, native, grants, home) = setup();
    let target = select(&state, &grants, home.path(), "run:agent").await;
    let before = observe(&state, &grants, home.path(), "run:agent", &target).await;
    native.uncertain.store(true, Ordering::SeqCst);
    let input = tool(
        "computer_type",
        json!({"targetId":target,"observationId":before["observation"]["id"],"text":"once"}),
    );
    let error = run(&state, &grants, home.path(), "run:agent", input.clone())
        .await
        .unwrap_err();
    assert_eq!(error.code, "computer_action_uncertain");
    let selected = run(
        &state,
        &grants,
        home.path(),
        "run:worker",
        tool(
            "computer_select",
            json!({"appId":"fixture-42-1","windowId":7,"access":"observe"}),
        ),
    )
    .await
    .unwrap();
    assert_eq!(selected["targetId"], target);
    assert_eq!(
        select(&state, &grants, home.path(), "run:worker").await,
        target
    );
    let after = observe(&state, &grants, home.path(), "run:worker", &target).await;
    let blocked = run(&state, &grants, home.path(), "run:worker", tool("computer_type", json!({"targetId":target,"observationId":after["observation"]["id"],"text":"duplicate"}))).await.unwrap_err();
    assert_eq!(blocked.code, "computer_recovery_required");
    assert_eq!(native.effects.load(Ordering::SeqCst), 1);
    state.reconciled("a", &target, &input.id).unwrap();
    native.uncertain.store(false, Ordering::SeqCst);
    let fresh = observe(&state, &grants, home.path(), "run:worker", &target).await;
    run(
        &state,
        &grants,
        home.path(),
        "run:worker",
        tool(
            "computer_type",
            json!({"targetId":target,"observationId":fresh["observation"]["id"],"text":"next"}),
        ),
    )
    .await
    .unwrap();
    assert_eq!(native.effects.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn input_stays_blocked_until_matching_durable_ack_and_stale_ack_cannot_clear_later_input() {
    let (state, native, grants, home) = setup();
    let target = select(&state, &grants, home.path(), "run:agent").await;
    let before = observe(&state, &grants, home.path(), "run:agent", &target).await;
    let first = tool(
        "computer_type",
        json!({"targetId":target,"observationId":before["observation"]["id"],"text":"first"}),
    );
    let receipt = run_unacknowledged(&state, &grants, home.path(), "run:agent", &first)
        .await
        .unwrap();
    assert_eq!(receipt["inputSent"], true);
    let observed = observe(&state, &grants, home.path(), "run:worker", &target).await;
    let input = tool(
        "computer_type",
        json!({"targetId":target,"observationId":observed["observation"]["id"],"text":"next"}),
    );
    let blocked = run(&state, &grants, home.path(), "run:worker", input.clone())
        .await
        .unwrap_err();
    assert_eq!(blocked.code, "computer_recovery_required");
    state
        .confirmed("a", &first.id, &target, "wrong-operation")
        .unwrap();
    assert!(
        run(&state, &grants, home.path(), "run:worker", input.clone())
            .await
            .is_err()
    );
    acknowledge_saved_input(&state, "a", &first, &receipt);
    let observed = observe(&state, &grants, home.path(), "run:worker", &target).await;
    let input = tool(
        "computer_type",
        json!({"targetId":target,"observationId":observed["observation"]["id"],"text":"next"}),
    );
    let second = run_unacknowledged(&state, &grants, home.path(), "run:worker", &input)
        .await
        .unwrap();
    acknowledge_saved_input(&state, "a", &first, &receipt);
    state.reconciled("a", &target, &first.id).unwrap();
    let observed = observe(&state, &grants, home.path(), "run:worker", &target).await;
    let blocked = run(
        &state,
        &grants,
        home.path(),
        "run:worker",
        tool(
            "computer_press",
            json!({"targetId":target,"observationId":observed["observation"]["id"],"key":"Tab"}),
        ),
    )
    .await
    .unwrap_err();
    assert_eq!(blocked.code, "computer_recovery_required");
    assert_eq!(native.effects.load(Ordering::SeqCst), 2);
    acknowledge_saved_input(&state, "a", &input, &second);
    let observed = observe(&state, &grants, home.path(), "run:worker", &target).await;
    run(
        &state,
        &grants,
        home.path(),
        "run:worker",
        tool(
            "computer_press",
            json!({"targetId":target,"observationId":observed["observation"]["id"],"key":"Tab"}),
        ),
    )
    .await
    .unwrap();
    assert_eq!(native.effects.load(Ordering::SeqCst), 3);
}

#[tokio::test]
async fn dropped_future_cancels_native_worker_and_preserves_pending_input_across_workers() {
    let (state, native, grants, home) = setup();
    let target = select(&state, &grants, home.path(), "run:agent").await;
    let before = observe(&state, &grants, home.path(), "run:agent", &target).await;
    let input = tool(
        "computer_type",
        json!({"targetId":target,"observationId":before["observation"]["id"],"text":"once"}),
    );
    native.pause_input.store(true, Ordering::SeqCst);
    let worker_state = state.clone();
    let worker_grants = grants.clone();
    let worker_home = home.path().to_path_buf();
    let worker_input = input.clone();
    let task = tokio::spawn(async move {
        run_unacknowledged(
            &worker_state,
            &worker_grants,
            &worker_home,
            "run:agent",
            &worker_input,
        )
        .await
    });
    for _ in 0..200 {
        if native.effects.load(Ordering::SeqCst) > 0 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(2)).await;
    }
    assert_eq!(native.effects.load(Ordering::SeqCst), 1);
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    // The paused native loop must exit on cancellation, without waiting for its deadline.
    let observed = tokio::time::timeout(
        Duration::from_secs(1),
        observe(&state, &grants, home.path(), "run:worker", &target),
    )
    .await
    .unwrap();
    assert_eq!(observed["observation"]["elements"][0]["value"], "once");
    let next = tool(
        "computer_press",
        json!({"targetId":target,"observationId":observed["observation"]["id"],"key":"Tab"}),
    );
    let blocked = run(&state, &grants, home.path(), "run:worker", next.clone())
        .await
        .unwrap_err();
    assert_eq!(blocked.code, "computer_recovery_required");
    state.reconciled("a", &target, &input.id).unwrap();
    native.pause_input.store(false, Ordering::SeqCst);
    let observed = observe(&state, &grants, home.path(), "run:worker", &target).await;
    let next = tool(
        "computer_press",
        json!({"targetId":target,"observationId":observed["observation"]["id"],"key":"Tab"}),
    );
    run(&state, &grants, home.path(), "run:worker", next)
        .await
        .unwrap();
    assert_eq!(native.effects.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn disappeared_target_returns_read_only_evidence_and_uncertain_target_survives_revocation() {
    let (state, native, grants, home) = setup();
    let target = select(&state, &grants, home.path(), "run:agent").await;
    let before = observe(&state, &grants, home.path(), "run:agent", &target).await;
    native.uncertain.store(true, Ordering::SeqCst);
    run(
        &state,
        &grants,
        home.path(),
        "run:agent",
        tool(
            "computer_type",
            json!({"targetId":target,"observationId":before["observation"]["id"],"text":"once"}),
        ),
    )
    .await
    .unwrap_err();
    let grant = grants
        .computer_grants("a", crate::agent::now())
        .unwrap()
        .remove(0);
    state.revoked("a", &fixture().identity).unwrap();
    grants.revoke_computer("a", &grant.id).unwrap();
    assert_eq!(
        observe_without_grant(&state, &grants, home.path(), &target)
            .await
            .code,
        "computer_app_authorization"
    );
    grants
        .create_computer(
            "a",
            fixture().identity,
            ComputerAccess::Observe,
            crate::agent::now(),
        )
        .unwrap();
    native.unavailable.store(true, Ordering::SeqCst);
    let evidence = observe(&state, &grants, home.path(), "run:new", &target).await;
    assert_eq!(evidence["targetId"], target);
    assert_eq!(evidence["code"], "computer_target_unavailable");
    assert_eq!(evidence["available"], false);
    assert_eq!(evidence["phase"], "observed");
    assert_eq!(evidence["inputSent"], false);
    assert!(evidence.get("attachment").is_none());
    assert_eq!(native.effects.load(Ordering::SeqCst), 1);
}

async fn observe_without_grant(
    state: &ComputerState,
    grants: &GrantStore,
    home: &std::path::Path,
    target: &str,
) -> AgentError {
    run(
        state,
        grants,
        home,
        "run:new",
        tool(
            "computer_observe",
            json!({"targetId":target,"capture":false}),
        ),
    )
    .await
    .unwrap_err()
}

#[tokio::test]
async fn cold_restart_retires_a_lost_binding_without_claiming_its_physical_effect() {
    let (state, native, grants, home) = setup();
    let target = select(&state, &grants, home.path(), "run:agent").await;
    let before = observe(&state, &grants, home.path(), "run:agent", &target).await;
    native.uncertain.store(true, Ordering::SeqCst);
    let input = tool(
        "computer_type",
        json!({"targetId":target,"observationId":before["observation"]["id"],"text":"once"}),
    );
    assert_eq!(
        run(&state, &grants, home.path(), "run:agent", input.clone())
            .await
            .unwrap_err()
            .code,
        "computer_action_uncertain"
    );
    drop(state);
    let restored = ComputerState::new(native.clone());
    let restored_grants = GrantStore::default();
    let evidence = observe(
        &restored,
        &restored_grants,
        home.path(),
        "new:agent",
        &target,
    )
    .await;
    assert_eq!(evidence["targetId"], target);
    assert_eq!(evidence["code"], "computer_target_unavailable");
    assert_eq!(evidence["reason"], "host_binding_missing");
    assert_eq!(evidence["effect"], "unknown");
    assert_eq!(evidence["available"], false);
    assert_eq!(evidence["inputSent"], false);
    assert!(evidence.get("attachment").is_none());
    assert!(evidence.get("observation").is_none());
    assert_eq!(
        run(&restored, &restored_grants, home.path(), "new:agent", input)
            .await
            .unwrap_err()
            .code,
        "computer_target"
    );
    assert_eq!(native.effects.load(Ordering::SeqCst), 1);
}

#[cfg(target_os = "macos")]
#[tokio::test]
#[ignore = "Requires explicit disposable fixture paths and live macOS permissions; sends input only to that fixture"]
async fn native_disposable_fixture_smoke() {
    let state = ComputerState::default();
    let capabilities = state.backend.capabilities().unwrap();
    println!(
        "COMPUTER_SMOKE_CAPABILITIES {}",
        serde_json::to_string(&capabilities).unwrap()
    );
    assert!(
        capabilities.screen_capture && capabilities.capture_supported,
        "SMOKE_BLOCKED: Screen Recording permission is required; no input was sent."
    );
    let expected = std::path::PathBuf::from(
        std::env::var("JARVIS_COMPUTER_FIXTURE_APP")
            .expect("Explicit disposable app path required"),
    )
    .canonicalize()
    .unwrap();
    let receipt = std::path::PathBuf::from(
        std::env::var("JARVIS_COMPUTER_FIXTURE_RECEIPT")
            .expect("Explicit disposable receipt path required"),
    );
    let app = state
        .apps()
        .await
        .unwrap()
        .into_iter()
        .find(|app| {
            app.identity.bundle_id == "dev.jarvis.computer-fixture"
                && std::path::Path::new(&app.identity.bundle_path) == expected
        })
        .expect("Only the canonical disposable fixture may receive input");
    let window = app
        .windows
        .iter()
        .find(|window| window.title == "Jarvis Computer Use Fixture")
        .unwrap();
    println!(
        "COMPUTER_SMOKE_TARGET {}",
        serde_json::to_string(&app).unwrap()
    );
    let grants = GrantStore::default();
    let conversation = "f".repeat(32);
    grants
        .create_computer(
            &conversation,
            app.identity.clone(),
            ComputerAccess::Control,
            crate::agent::now(),
        )
        .unwrap();
    let home = tempfile::tempdir().unwrap();
    let (_send, signal) = watch::channel(false);
    let native_run = |call: ToolCall| {
        let state = state.clone();
        let grants = grants.clone();
        let conversation = conversation.clone();
        let signal = signal.clone();
        let home = home.path().to_path_buf();
        async move {
            let receipt = state
                .run(
                    &conversation,
                    "fixture:agent",
                    &call,
                    &grants,
                    home,
                    None,
                    signal,
                )
                .await
                .map(|output| serde_json::from_str::<Value>(&output).unwrap())?;
            acknowledge_saved_input(&state, &conversation, &call, &receipt);
            Ok::<_, AgentError>(receipt)
        }
    };
    let selected = native_run(tool(
        "computer_select",
        json!({"appId":app.app_id,"windowId":window.id,"access":"control"}),
    ))
    .await
    .unwrap();
    let target = selected["targetId"].as_str().unwrap();
    let before = native_run(tool("computer_observe", json!({"targetId":target})))
        .await
        .unwrap();
    assert!(
        before["attachment"]["id"].is_string(),
        "A real PNG must reach the existing attachment flow"
    );
    println!(
        "COMPUTER_SMOKE_OBSERVATION {}",
        json!({"attachment":before["attachment"],"window":before["observation"]["window"],"capture":before["observation"]["capture"],"elementCount":before["observation"]["elements"].as_array().unwrap().len(),"inputSent":before["inputSent"]})
    );
    assert!(capabilities.accessibility && capabilities.input_posting,
        "SMOKE_INPUT_BLOCKED: Read-only capture reached the attachment flow; Accessibility and input-posting permission are required before sending input.");
    let elements = before["observation"]["elements"].as_array().unwrap();
    let field = elements
        .iter()
        .find(|element| element["role"] == "AXTextField" && element["protected"] == false)
        .expect("Fixture input in real AX tree");
    let typed = native_run(tool("computer_type", json!({"targetId":target,"observationId":before["observation"]["id"],"element":field["id"],"text":"Jarvis smoke fictício"}))).await.unwrap();
    assert_eq!(typed["inputSent"], true);
    assert!(typed["observation"]["elements"]
        .as_array()
        .unwrap()
        .iter()
        .any(|element| element["value"]
            .as_str()
            .is_some_and(|text| text.contains("Jarvis smoke fictício"))));
    let pressed = native_run(tool(
        "computer_press",
        json!({"targetId":target,"observationId":typed["observation"]["id"],"key":"Tab"}),
    ))
    .await
    .unwrap();
    assert_eq!(pressed["inputSent"], true);
    let button = pressed["observation"]["elements"]
        .as_array()
        .unwrap()
        .iter()
        .find(|element| element["role"] == "AXButton" && element["title"] == "Confirmar teste")
        .unwrap();
    let clicked = native_run(tool("computer_click", json!({"targetId":target,"observationId":pressed["observation"]["id"],"element":button["id"]}))).await.unwrap();
    let read_receipt =
        || serde_json::from_str::<Value>(&std::fs::read_to_string(&receipt).unwrap()).unwrap();
    let first_receipt = read_receipt();
    assert_eq!(first_receipt["clicks"], 1);
    assert!(first_receipt["result"]
        .as_str()
        .unwrap()
        .contains("Jarvis smoke fictício"));
    let button = clicked["observation"]["elements"]
        .as_array()
        .unwrap()
        .iter()
        .find(|element| element["role"] == "AXButton" && element["title"] == "Confirmar teste")
        .unwrap();
    let bounds = &button["bounds"];
    let geometry = &clicked["observation"]["capture"];
    let window_bounds = &clicked["observation"]["window"]["bounds"];
    let x = (bounds["x"].as_f64().unwrap() + bounds["width"].as_f64().unwrap() / 2.0
        - window_bounds["x"].as_f64().unwrap())
        * geometry["scaleX"].as_f64().unwrap();
    let y = (bounds["y"].as_f64().unwrap() + bounds["height"].as_f64().unwrap() / 2.0
        - window_bounds["y"].as_f64().unwrap())
        * geometry["scaleY"].as_f64().unwrap();
    let pixel_clicked = native_run(tool(
        "computer_click",
        json!({"targetId":target,"observationId":clicked["observation"]["id"],"x":x,"y":y}),
    ))
    .await
    .unwrap();
    let limit = Instant::now() + Duration::from_secs(2);
    while read_receipt()["clicks"] != 2 && Instant::now() < limit {
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(
        read_receipt()["clicks"],
        2,
        "PID-targeted pixel click must reach the fixture once"
    );
    let geometry_before = pixel_clicked["observation"]["window"]["bounds"].clone();
    std::fs::write(
        receipt.with_file_name("command.json"),
        "{\"action\":\"move\"}",
    )
    .unwrap();
    let old_x = read_receipt()["windowX"].clone();
    let limit = Instant::now() + Duration::from_secs(2);
    while read_receipt()["windowX"] == old_x && Instant::now() < limit {
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let moved_error = native_run(tool(
        "computer_click",
        json!({"targetId":target,"observationId":pixel_clicked["observation"]["id"],"x":x,"y":y}),
    ))
    .await
    .unwrap_err();
    assert_eq!(moved_error.code, "computer_stale_observation");
    assert_eq!(read_receipt()["clicks"], 2);
    let fresh = native_run(tool("computer_observe", json!({"targetId":target})))
        .await
        .unwrap();
    assert_ne!(fresh["observation"]["window"]["bounds"], geometry_before);
    let stale = native_run(tool(
        "computer_press",
        json!({"targetId":target,"observationId":pixel_clicked["observation"]["id"],"key":"Tab"}),
    ))
    .await
    .unwrap_err();
    assert_eq!(stale.code, "computer_stale_observation");
    // A waiting native action is stopped before it may enter the fixture.
    let held = state.operations.clone().lock_owned().await;
    let pending = native_run(tool(
        "computer_press",
        json!({"targetId":target,"observationId":fresh["observation"]["id"],"key":"Tab"}),
    ));
    let stop = async {
        tokio::time::sleep(Duration::from_millis(20)).await;
        state.stop(&conversation).unwrap();
        drop(held);
    };
    let (result, ()) = tokio::join!(pending, stop);
    assert_eq!(result.unwrap_err().code, "computer_stopped");
    assert_eq!(read_receipt()["clicks"], 2);
    println!("COMPUTER_SMOKE_PASS: real discovery, AX, PNG attachment, text, keyboard, semantic and PID-targeted pixel clicks, moved/stale geometry and queued-input stop verified on disposable fixture; full model/approval UAT remains separate.");
}

mod browser_binding {
    use super::*;

    fn native_context() -> NativeBrowserContext {
        serde_json::from_value(json!({"id":"ext:fixture:55","browser":"firefox","title":"Fixture window","url":"https://example.test/","active":true,"focused":true,"windowId":8,"foregroundRevision":3,"window":{"left":100,"top":100,"width":400,"height":300}})).unwrap()
    }

    fn browser_target() -> NativeTarget {
        let mut app = fixture();
        app.identity.bundle_id = "org.mozilla.firefox".into();
        let context = native_context();
        app.windows[0].title = context.title;
        app.windows[0].bounds = Bounds {
            x: 100.0,
            y: 100.0,
            width: 400.0,
            height: 300.0,
        };
        NativeTarget {
            window: app.windows[0].clone(),
            app,
        }
    }

    #[test]
    fn native_browser_binding_rejects_another_active_tab_with_the_same_title() {
        let target = browser_target();
        let expected = native_context();
        let mut current = expected.clone();
        current.active = false;
        assert_eq!(
            validate_browser_context(&target, &target.window, &expected, &current)
                .unwrap_err()
                .code,
            "computer_context_changed"
        );
        current.active = true;
        current.id = "ext:fixture:56".into();
        assert_eq!(
            validate_browser_context(&target, &target.window, &expected, &current)
                .unwrap_err()
                .code,
            "computer_context_changed"
        );
    }

    #[test]
    fn native_browser_binding_rejects_focus_change_and_a_tab_moved_to_another_window() {
        let target = browser_target();
        let expected = native_context();
        let mut current = expected.clone();
        current.focused = false;
        assert_eq!(
            validate_browser_context(&target, &target.window, &expected, &current)
                .unwrap_err()
                .code,
            "computer_context_changed"
        );
        current.focused = true;
        current.window_id = 9;
        assert_eq!(
            validate_browser_context(&target, &target.window, &expected, &current)
                .unwrap_err()
                .code,
            "computer_context_changed"
        );
    }

    #[test]
    fn native_browser_binding_rejects_switch_away_and_back_during_capture() {
        let target = browser_target();
        let expected = native_context();
        let mut current = expected.clone();
        current.foreground_revision = 5;
        assert_eq!(
            validate_browser_context(&target, &target.window, &expected, &current)
                .unwrap_err()
                .code,
            "computer_context_changed"
        );
    }

    #[test]
    fn native_browser_binding_requires_the_selected_browser_and_outer_window_geometry() {
        let mut target = browser_target();
        let expected = native_context();
        validate_browser_context(&target, &target.window, &expected, &expected).unwrap();
        let mut current = expected.clone();
        current.window.left += 1.0;
        assert_eq!(
            validate_browser_context(&target, &target.window, &expected, &current)
                .unwrap_err()
                .code,
            "computer_context_changed"
        );
        target.app.identity.bundle_id = "com.google.Chrome".into();
        assert_eq!(
            validate_browser_context(&target, &target.window, &expected, &expected)
                .unwrap_err()
                .code,
            "computer_context_changed"
        );
    }

    #[test]
    fn ordinary_native_windows_keep_the_selected_document_context() {
        let target = NativeTarget {
            app: fixture(),
            window: fixture().windows[0].clone(),
        };
        assert!(browser_family(&target).is_none());
        let target = Target {
            native: target,
            browser: None,
            access: ComputerAccess::Observe,
            pending_input: None,
        };
        validate_window_context(&target, &target.native.window).unwrap();
        let mut current = target.native.window.clone();
        current.title = "Another document".into();
        assert_eq!(
            validate_window_context(&target, &current).unwrap_err().code,
            "computer_context_changed"
        );
    }
}

mod native_dispatch_context {
    use super::*;

    struct ContextNative {
        native: FakeNative,
        change_before_dispatch: bool,
    }

    impl NativeBackend for ContextNative {
        fn capabilities(&self) -> Result<ComputerCapabilities, AgentError> {
            self.native.capabilities()
        }
        fn apps(&self, guard: &OperationGuard) -> Result<Vec<ComputerApp>, AgentError> {
            self.native.apps(guard)
        }
        fn window(
            &self,
            target: &NativeTarget,
            guard: &OperationGuard,
        ) -> Result<ComputerWindow, AgentError> {
            self.native.window(target, guard)
        }
        fn observe(
            &self,
            target: &NativeTarget,
            id: &str,
            capture: bool,
            guard: &OperationGuard,
        ) -> Result<NativeObservation, AgentError> {
            self.native.observe(target, id, capture, guard)
        }
        fn act(
            &self,
            target: &NativeTarget,
            observation: &Observation,
            action: &ComputerAction,
            guard: &OperationGuard,
        ) -> Result<(), AgentError> {
            let changed = Arc::new(AtomicBool::new(false));
            let context = changed.clone();
            let mut guard = guard.clone();
            guard.context_check = Some(Arc::new(move |_| {
                if context.load(Ordering::SeqCst) {
                    Err(context_changed())
                } else {
                    Ok(())
                }
            }));
            guard.check()?;
            changed.store(self.change_before_dispatch, Ordering::SeqCst);
            self.native.act(target, observation, action, &guard)
        }
        fn request_permission(
            &self,
            permission: ComputerPermission,
        ) -> Result<ComputerCapabilities, AgentError> {
            self.native.request_permission(permission)
        }
    }

    async fn prepare(
        change_before_dispatch: bool,
    ) -> (
        ComputerState,
        Arc<ContextNative>,
        GrantStore,
        tempfile::TempDir,
        Value,
    ) {
        let (_, _, grants, home) = setup();
        let native = Arc::new(ContextNative {
            native: FakeNative::default(),
            change_before_dispatch,
        });
        native.native.permission.store(true, Ordering::SeqCst);
        let state = ComputerState::new(native.clone());
        let selected = run(
            &state,
            &grants,
            home.path(),
            "run",
            tool(
                "computer_select",
                json!({"appId":"fixture-42-1","windowId":7,"access":"control"}),
            ),
        )
        .await
        .unwrap();
        let observation = run(
            &state,
            &grants,
            home.path(),
            "run",
            tool("computer_observe", json!({"targetId":selected["targetId"]})),
        )
        .await
        .unwrap();
        native.native.observing.store(false, Ordering::SeqCst);
        (state, native, grants, home, observation)
    }

    #[tokio::test]
    async fn context_switch_after_prepare_sends_no_input_or_new_capture() {
        let (state, native, grants, home, observation) = prepare(true).await;
        let error = run(&state, &grants, home.path(), "run", tool("computer_click", json!({"targetId":observation["targetId"],"observationId":observation["observation"]["id"],"x":10,"y":20}))).await.unwrap_err();
        assert_eq!(error.code, "computer_context_changed");
        let receipt: Value = serde_json::from_str(error.tool_result.as_ref().unwrap()).unwrap();
        assert_eq!(receipt["inputSent"], false);
        assert_eq!(native.native.effects.load(Ordering::SeqCst), 0);
        assert!(!native.native.observing.load(Ordering::SeqCst));
        assert!(state.conversations.lock().unwrap()["a"]
            .observations
            .is_empty());
    }

    #[tokio::test]
    async fn dispatched_input_in_valid_context_remains_uncertain_after_failure() {
        let (state, native, grants, home, observation) = prepare(false).await;
        native.native.uncertain.store(true, Ordering::SeqCst);
        let error = run(&state, &grants, home.path(), "run", tool("computer_click", json!({"targetId":observation["targetId"],"observationId":observation["observation"]["id"],"x":10,"y":20}))).await.unwrap_err();
        assert_eq!(error.code, "computer_action_uncertain");
        let receipt: Value = serde_json::from_str(error.tool_result.as_ref().unwrap()).unwrap();
        assert_eq!(receipt["inputSent"], true);
        assert_eq!(native.native.effects.load(Ordering::SeqCst), 1);
        assert!(!native.native.observing.load(Ordering::SeqCst));
    }
}
