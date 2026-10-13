//! Opt-in native agent pipeline with private journals, Core copies and loopback inference.
use super::*;
use crate::agent::{workflow, AgentState, ApprovalMode, Mode, TurnOptions};
use crate::openai_codex::{custom, OpenAiCodexState};
use axum::{extract::State, http::header::CONTENT_TYPE, routing::post, Json};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, OnceLock,
    },
};

static PROBE_HOME: OnceLock<PathBuf> = OnceLock::new();

struct ProbeState {
    _directory: tempfile::TempDir,
    home: PathBuf,
    conversation: String,
}

pub(crate) fn home() -> Option<&'static Path> {
    PROBE_HOME.get().map(PathBuf::as_path)
}

pub(super) fn page() -> Option<String> {
    fs::read_to_string(home()?.parent()?.join("project/index.html")).ok()
}

pub(super) fn initialize(app: &mut tauri::App) -> Result<(), AgentError> {
    let directory = tempfile::tempdir().map_err(|_| AgentError::storage())?;
    let home = directory.path().join("home");
    let project = directory.path().join("project");
    fs::create_dir_all(&home).map_err(|_| AgentError::storage())?;
    fs::create_dir_all(&project).map_err(|_| AgentError::storage())?;
    fs::write(project.join("index.html"), super::PAGE).map_err(|_| AgentError::storage())?;
    fs::create_dir_all(project.join(".impeccable/live")).map_err(|_| AgentError::storage())?;
    fs::write(
        project.join(".impeccable/live/config.json"),
        json!({"files":["index.html"],"insertBefore":"</body>","commentSyntax":"html"}).to_string(),
    )
    .map_err(|_| AgentError::storage())?;
    let source_home = app.path().home_dir().map_err(|_| AgentError::storage())?;
    let source = crate::core::root(&source_home);
    let target = crate::core::root(&home);
    let bytes = fs::read(source.join("manifest.json")).map_err(|_| AgentError::storage())?;
    let manifest: crate::core::Manifest =
        serde_json::from_slice(&bytes).map_err(|_| error("Invalid source Core manifest"))?;
    for (id, record) in &manifest.installations {
        if !crate::core::ComponentId::ALL.contains(id)
            || *id == crate::core::ComponentId::Impeccable
        {
            continue;
        }
        let package = record.path(&source_home).map_err(AgentError::from)?;
        let mut files = record.files.clone();
        if *id == crate::core::ComponentId::Openmontage {
            files
                .extend(crate::core::openmontage::probe_files(&package).map_err(AgentError::from)?);
        }
        if *id == crate::core::ComponentId::Graft {
            // validate_graft_wasm also requires this metadata outside record.files.
            files.push("node_modules/web-tree-sitter/package.json".into());
        }
        files.sort_unstable();
        files.dedup();
        for file in &files {
            let source = fs::canonicalize(package.join(file)).map_err(|_| AgentError::storage())?;
            if !source.starts_with(&package) {
                return Err(error(
                    "Native probe assets must remain inside their Core package",
                ));
            }
            let destination = target.join(&record.directory).join(file);
            fs::create_dir_all(destination.parent().ok_or_else(AgentError::storage)?)
                .map_err(|_| AgentError::storage())?;
            fs::copy(source, destination).map_err(|_| AgentError::storage())?;
        }
    }
    fs::write(target.join("manifest.json"), bytes).map_err(|_| AgentError::storage())?;
    crate::core::design::setup(
        &app.path()
            .resource_dir()
            .map_err(|_| AgentError::storage())?,
        &home,
    )
    .map_err(AgentError::from)?;
    crate::core::require_ready(&home).map_err(AgentError::from)?;
    let state = app.state::<AppState>();
    let workspace = library::new_id().map_err(AgentError::from)?;
    let project_id = library::new_id().map_err(AgentError::from)?;
    let conversation = state.with_connection(&home, |db| {
        db.execute(
            "INSERT INTO workspaces(id,name) VALUES(?1,'Native probe')",
            [&workspace],
        )
        .map_err(|_| AgentError::storage())?;
        db.execute(
            "INSERT INTO projects(id,workspace_id,name,path) VALUES(?1,?2,'Native probe',?3)",
            rusqlite::params![project_id, workspace, project.to_string_lossy()],
        )
        .map_err(|_| AgentError::storage())?;
        library::companion::create_conversation_silently(db, &home, &project_id)
            .map_err(AgentError::from)
    })?;
    PROBE_HOME
        .set(home.clone())
        .map_err(|_| error("Native agent probe already initialized"))?;
    app.manage(ProbeState {
        _directory: directory,
        home,
        conversation,
    });
    Ok(())
}

#[derive(Clone)]
struct Script {
    requests: Arc<AtomicUsize>,
    page: String,
}

async fn completion(
    State(script): State<Script>,
    Json(request): Json<Value>,
) -> impl axum::response::IntoResponse {
    let step = script.requests.fetch_add(1, Ordering::SeqCst);
    let tasks = |status: &str| json!({"tasks":[{"id":"live","title":"Abrir o navegador e ativar o Live","status":status}]});
    let call = match step {
        0 => Some(("update_tasks", tasks("in_progress"))),
        1 => Some(("impeccable", json!({"command":"live-status","args":[]}))),
        2 => {
            let evidence = request["messages"]
                .as_array()
                .into_iter()
                .flatten()
                .rev()
                .find_map(|message| {
                    let content = message["content"].as_str()?;
                    let calls = content
                        .split_once("Calls: ")?
                        .1
                        .split_once(". Legacy tool names: ")?
                        .0;
                    let calls: Vec<Value> = serde_json::from_str(calls).ok()?;
                    let call = calls.iter().find(|call| call["id"] == "interrupted-live")?;
                    call["evidence"].as_object()?.keys().next().cloned()
                })
                .expect(
                    "The actual inspection ID must be visible in the model's recovery checkpoint",
                );
            Some((
                "recovery_resolve",
                json!({"callId":"interrupted-live","evidenceCallId":evidence,"outcome":"not_applied"}),
            ))
        }
        3 => Some(("browser_open", json!({"url":script.page}))),
        4 => Some((
            "impeccable",
            json!({"command":"live","args":["--dev-url",script.page]}),
        )),
        5 => Some(("update_tasks", tasks("completed"))),
        _ => None,
    };
    let (delta, reason) = match call {
        Some((name, arguments)) => (
            json!({"tool_calls":[{"index":0,"id":format!("probe-{step}"),"type":"function","function":{"name":name,"arguments":arguments.to_string()}}]}),
            "tool_calls",
        ),
        None => (
            json!({"content":"Navegador aberto e Live ativado."}),
            "stop",
        ),
    };
    let body = format!(
        "data: {}\n\ndata: {}\n\ndata: [DONE]\n\n",
        json!({"choices":[{"index":0,"delta":delta,"finish_reason":null}]}),
        json!({"choices":[{"index":0,"delta":{},"finish_reason":reason}]})
    );
    ([(CONTENT_TYPE, "text/event-stream")], body)
}

pub(super) async fn check(
    app: &tauri::AppHandle,
    base: &str,
    output: &Path,
) -> Result<(), AgentError> {
    fs::create_dir_all(output).map_err(|_| AgentError::storage())?;
    let probe = app.state::<ProbeState>();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|_| error("Cannot bind the isolated provider"))?;
    let endpoint = format!(
        "http://{}",
        listener.local_addr().map_err(|_| AgentError::storage())?
    );
    let script = Script {
        requests: Arc::new(AtomicUsize::new(0)),
        page: format!("{base}/agent"),
    };
    let router = axum::Router::new()
        .route("/chat/completions", post(completion))
        .with_state(script.clone());
    let state = app.state::<AppState>().inner().clone();
    let config: custom::Config = serde_json::from_value(json!({
        "baseUrl":endpoint,"protocol":"openai-completions","authMode":"bearer","tokenField":"max_tokens",
        "models":[{"id":"stack-probe","name":"Stack probe","contextWindow":128000,"maxOutputTokens":4000,
        "supportsImages":false,"supportsTools":true,"reasoning":"none","reasoningLevels":[],"defaultReasoningLevel":null,"thinkingBudget":null}]
    })).map_err(|_| error("Invalid isolated provider configuration"))?;
    let oauth = OpenAiCodexState::probe(&state, &probe.home, config)?;
    app.manage(oauth.clone());
    let session =
        app.state::<AgentState>()
            .session(app, &state, &probe.home, &probe.conversation)?;
    let options = TurnOptions {
        executor: crate::claude::Executor::Jarvis,
        account: "native-probe".into(),
        model: "stack-probe".into(),
        reasoning: None,
        service_tier: None,
        mode: Mode::Build,
        workflow: Some(workflow::Flow::Designer),
        custom_workflow_id: None,
        custom_agent_id: None,
        approval_mode: ApprovalMode::Yolo,
        manual_validation: false,
        automatic_publication: None,
        model_selection: None,
    };
    let signal = session
        .submit_message(
            "Abra a página local e ative o Impeccable Live.".into(),
            options,
            vec![],
        )?
        .ok_or_else(|| error("Native agent probe did not reserve its turn"))?;
    session.update(true, |data| {
        let turn = data.turns.last_mut().unwrap();
        turn.wire.extend([
            json!({"type":"function_call","call_id":"interrupted-live","name":"impeccable","arguments":json!({"command":"live","args":["--dev-url",script.page]}).to_string()}),
            json!({"type":"function_call_output","call_id":"interrupted-live","output":crate::agent::journal::UNKNOWN_TOOL_OUTPUT}),
            json!({"role":"user","_jarvis_runtime":true,"_jarvis_retry":true,"content":"Resume this interrupted turn from its durable state. Inspect the managed Live state before starting it again."}),
        ]);
    })?;
    let server = tauri::async_runtime::spawn(async move { axum::serve(listener, router).await });
    println!("Native agent probe: entering workflow::run on the default Tokio worker");
    let result = tokio::time::timeout(
        Duration::from_secs(120),
        workflow::run(
            &session,
            (
                state,
                oauth,
                crate::mcp::McpState::default(),
                probe.home.clone(),
            ),
            app,
            signal,
            None,
        ),
    )
    .await
    .map_err(|_| error("Native agent pipeline timed out"))
    .and_then(|result| result);
    let validated = async {
        result?;
        let status = serde_json::to_value(crate::core::impeccable_live::get_impeccable_live(app.clone(), probe.conversation.clone()).await.map_err(AgentError::from)?).map_err(|_| AgentError::storage())?;
        let tools = session.snapshot()?.turns.into_iter().flat_map(|turn| turn.steps).flat_map(|step| step.tools).collect::<Vec<_>>();
        if status["state"] != "ready" || tools.iter().any(|tool| tool.status != "completed") {
            return Err(error(&format!("Native agent pipeline failed: {status}; tools: {}", json!(tools))));
        }
        for name in ["recovery_resolve", "browser_open", "impeccable"] {
            if !tools.iter().any(|tool| tool.name == name) {
                return Err(error("The native tool dispatcher did not execute the complete probe"));
            }
        }
        let tab = status["tabId"].as_str().ok_or_else(|| error("Live tab missing"))?;
        set_browser_viewport(app.clone(), probe.conversation.clone(), Some(tab.into()), Some(Viewport {
            x: 20., y: 60., width: 900., height: 520.,
        })).await?;
        let view = app.get_webview(&label(tab)).ok_or_else(|| error("Native Live WebView missing"))?;
        wait_page(&view, "Browser probe").await?;
        let report = json!({
            "passed":true,"pipeline":"workflow::run -> visible recovery evidence -> recovery_resolve -> browser_open + impeccable.live",
            "live":status,"tools":tools,"providerRequests":script.requests.load(Ordering::SeqCst),"isolated":true
        });
        fs::write(output.join("native-agent-live.png"), capture::capture(&view).await?)
            .map_err(|_| AgentError::storage())?;
        Ok(report)
    }.await;
    let cleanup =
        crate::core::impeccable_live::stop_impeccable_live(app.clone(), probe.conversation.clone())
            .await;
    server.abort();
    let report = validated?;
    cleanup.map_err(AgentError::from)?;
    fs::write(
        output.join("native-agent-report.json"),
        serde_json::to_vec_pretty(&report).map_err(|_| AgentError::storage())?,
    )
    .map_err(|_| AgentError::storage())?;
    println!("Native agent probe: full browser and Live dispatcher passed");
    Ok(())
}
