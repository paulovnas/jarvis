//! Opt-in CEF verification against the real adopted cockpit and authenticated Codex.
//!
//! This does not fabricate model responses or change the user's Jarvis database.
//! The loopback driver only invokes the same native pixel tools as the agent.
use super::{pixel_art, AgentError, ToolCall};
use axum::{
    extract::{DefaultBodyLimit, State},
    http::{HeaderMap, StatusCode},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    fs,
    path::PathBuf,
    sync::{Arc, Mutex, OnceLock},
    time::Duration,
};
use tauri::Manager;
use tokio::sync::watch;

static PROBE_HOME: OnceLock<PathBuf> = OnceLock::new();

pub(crate) fn home() -> Option<PathBuf> {
    PROBE_HOME.get().cloned()
}

#[derive(Clone)]
struct Probe {
    app: tauri::AppHandle,
    home: PathBuf,
    project: PathBuf,
    output: PathBuf,
    conversation: String,
    token: String,
    cancellation: Arc<watch::Sender<bool>>,
    agent_result: Arc<tokio::sync::Mutex<Value>>,
    calls: Arc<Mutex<HashMap<String, (String, ToolCall)>>>,
    core_prepared: Arc<tokio::sync::Mutex<bool>>,
}

type Response = Result<Json<Value>, (StatusCode, Json<Value>)>;

fn rejected(message: &str) -> (StatusCode, Json<Value>) {
    (StatusCode::BAD_REQUEST, Json(json!({"error":message})))
}

fn authorize(headers: &HeaderMap, probe: &Probe) -> Result<(), (StatusCode, Json<Value>)> {
    if headers
        .get("x-jarvis-probe")
        .and_then(|value| value.to_str().ok())
        != Some(probe.token.as_str())
    {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(json!({"error":"Unauthorized probe request"})),
        ));
    }
    Ok(())
}

async fn info(State(probe): State<Probe>, headers: HeaderMap) -> Response {
    authorize(&headers, &probe)?;
    Ok(Json(json!({
        "ready":true, "engine":"native CEF / original Image Cockpit",
        "conversationId":probe.conversation, "home":probe.home,
        "project":probe.project, "output":probe.output,
        "scope":"Real native tools; LLM agent selection is a separate acceptance check"
    })))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Command {
    name: String,
    arguments: Value,
    conversation_id: Option<String>,
    call_id: Option<String>,
}

async fn tool(
    State(probe): State<Probe>,
    headers: HeaderMap,
    Json(command): Json<Command>,
) -> Response {
    authorize(&headers, &probe)?;
    let conversation = command
        .conversation_id
        .as_deref()
        .unwrap_or(&probe.conversation);
    let exists = probe
        .app
        .state::<crate::persistence::AppState>()
        .with_connection(&probe.home, |db| {
            db.query_row(
                "SELECT EXISTS(SELECT 1 FROM conversations c JOIN projects p ON p.id=c.project_id WHERE c.id=?1 AND p.path=?2)",
                rusqlite::params![conversation, probe.project.to_string_lossy()],
                |row| row.get::<_, bool>(0),
            ).map_err(|_| AgentError::storage())
        }).map_err(|cause| rejected(&cause.message))?;
    if !exists {
        return Err(rejected(
            "The requested conversation is outside this private probe project.",
        ));
    }
    if ![
        "pixel_art_docs",
        "pixel_art_run",
        "pixel_art_status",
        "pixel_art_cancel",
    ]
    .contains(&command.name.as_str())
        || !command.arguments.is_object()
    {
        return Err(rejected(
            "Only the native pixel-art tools are available in this probe.",
        ));
    }
    let call_id = match command.call_id {
        Some(id)
            if !id.is_empty()
                && id.len() <= 256
                && id
                    .chars()
                    .all(|value| value.is_ascii_alphanumeric() || "_-.".contains(value)) =>
        {
            id
        }
        Some(_) => return Err(rejected("The explicit tool call ID is invalid.")),
        None => crate::library::new_id().map_err(|_| rejected("Unable to identify probe call"))?,
    };
    let call = ToolCall {
        id: call_id,
        name: command.name,
        args: command.arguments,
        status: "running".into(),
        output: String::new(),
        duration_ms: 0,
    };
    {
        let mut calls = probe
            .calls
            .lock()
            .map_err(|_| rejected("Unable to record the actual probe tool call"))?;
        if let Some((existing_conversation, existing)) = calls.get(&call.id) {
            if existing_conversation != conversation
                || existing.name != call.name
                || existing.args != call.args
            {
                return Err(rejected(
                    "A recorded call ID cannot be reused for different arguments.",
                ));
            }
        } else {
            calls.insert(call.id.clone(), (conversation.to_owned(), call.clone()));
        }
    }
    let result = pixel_art::execute(
        Some(&probe.app),
        &probe.home,
        &probe.project,
        conversation,
        "main",
        &call,
        probe.cancellation.subscribe(),
    )
    .await
    .map_err(|cause| rejected(&cause.message))?;
    Ok(Json(serde_json::from_str(&result).map_err(|_| {
        rejected("Native tool returned invalid JSON")
    })?))
}

async fn create_session(State(probe): State<Probe>, headers: HeaderMap) -> Response {
    authorize(&headers, &probe)?;
    let conversation = probe
        .app
        .state::<crate::persistence::AppState>()
        .with_connection(&probe.home, |db| {
            use std::io::Write;
            let project: String = db
                .query_row(
                    "SELECT id FROM projects WHERE path=?1",
                    [probe.project.to_string_lossy().as_ref()],
                    |row| row.get(0),
                )
                .map_err(|_| AgentError::storage())?;
            // Advanced verification needs separate real sessions even before an
            // LLM turn exists. The normal UI deliberately reuses empty chats.
            let id = crate::library::new_id().map_err(AgentError::from)?;
            let tx = db.transaction().map_err(|_| AgentError::storage())?;
            tx.execute(
                "INSERT INTO conversations(id,project_id,title,title_source) VALUES(?1,?2,'Pixel parity','manual')",
                rusqlite::params![id, project],
            ).map_err(|_| AgentError::storage())?;
            let created_at: i64 = tx.query_row("SELECT created_at FROM conversations WHERE id=?1", [&id], |row| row.get(0))
                .map_err(|_| AgentError::storage())?;
            let path = crate::library::session_path(&probe.home, &project, &id, true).map_err(AgentError::from)?;
            let header = json!({"type":"session","version":1,"id":id,"projectId":project,
                "cwd":probe.project,"title":"Pixel parity","createdAt":created_at});
            let mut file = fs::OpenOptions::new().write(true).create_new(true).open(&path)
                .map_err(|_| AgentError::storage())?;
            writeln!(file, "{header}").and_then(|()| file.sync_all()).map_err(|_| AgentError::storage())?;
            tx.commit().map_err(|_| AgentError::storage())?;
            Ok::<_, AgentError>(id)
        })
        .map_err(|cause| rejected(&cause.message))?;
    crate::library::agent_location(
        &probe.app.state::<crate::persistence::AppState>(),
        &probe.home,
        &conversation,
    )
    .map_err(|cause| rejected(&AgentError::from(cause).message))?;
    Ok(Json(
        json!({"conversationId":conversation,"project":probe.project}),
    ))
}

async fn suite(
    State(probe): State<Probe>,
    headers: HeaderMap,
    Json(input): Json<Value>,
) -> Response {
    authorize(&headers, &probe)?;
    let view = probe
        .app
        .get_webview(&format!("pixel-worker-{}", probe.conversation))
        .ok_or_else(|| rejected("Start a pixel tool before showing its suite"))?;
    if input["visible"] == false {
        view.hide().map_err(|_| rejected("Unable to hide suite"))?;
    } else if input["inspectOnly"] != true {
        view.set_bounds(tauri::Rect {
            position: tauri::LogicalPosition::new(0., 0.).into(),
            size: tauri::LogicalSize::new(1280., 900.).into(),
        })
        .map_err(|_| rejected("Unable to place suite"))?;
        view.show().map_err(|_| rejected("Unable to show suite"))?;
    }
    if input["openProcessingPanel"] == true {
        let opened = evaluate_preview(&view, "(() => { const button=[...document.querySelectorAll('button')].find(item=>item.textContent?.trim()==='Preparar pixel art'); if(!button)return {error:'Pixel preparation controls are unavailable'}; if(button.getAttribute('data-state')!=='open')button.click(); button.scrollIntoView({block:'start'}); return {found:true}; })()".into()).await?;
        if let Some(cause) = opened["error"].as_str() { return Err(rejected(cause)); }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    let prompt_edit = if let Some(text) = input["editPrompt"].as_str() {
        if text.chars().count() > 1200 {
            return Err(rejected("The verification prompt is limited to 1200 characters"));
        }
        let edit = evaluate_preview(&view, format!(
            "(() => {{const field=document.querySelector('textarea:not([disabled]):not([readonly])');const setter=Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype,'value')?.set;if(!field||!setter)return {{error:'No editable prompt is available'}};field.scrollIntoView({{block:'center'}});field.focus();const previous=field.value;setter.call(field,{});field.dispatchEvent(new Event('input',{{bubbles:true}}));field.dispatchEvent(new Event('change',{{bubbles:true}}));return {{id:field.id,name:field.name,previous,value:field.value}};}})()",
            json!(text),
        )).await?;
        if let Some(cause) = edit["error"].as_str() {
            return Err(rejected(cause));
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
        Some(edit)
    } else { None };
    let (send, receive) = tokio::sync::oneshot::channel();
    let send = Mutex::new(Some(send));
    view.eval_with_callback(
        "({title:document.title,width:innerWidth,height:innerHeight,controllers:window.__jarvisPixel?.snapshot()?.controllers,canvasCount:document.querySelectorAll('canvas').length,bodyText:document.body.innerText.slice(0,2000)})",
        move |value| { if let Ok(mut send) = send.lock() { if let Some(send) = send.take() { let _ = send.send(value); } } },
    ).map_err(|_| rejected("Unable to inspect suite document"))?;
    let value = tokio::time::timeout(Duration::from_secs(10), receive)
        .await
        .map_err(|_| rejected("Suite document inspection timed out"))?
        .map_err(|_| rejected("Suite closed during inspection"))?;
    let mut evidence: Value = serde_json::from_str(&value)
        .map_err(|_| rejected("Suite inspection returned invalid JSON"))?;
    if let Some(edit) = prompt_edit {
        evidence["promptEdit"] = edit;
    }
    if input["capture"] == true {
        let bytes = super::browser::capture_probe_view(&view)
            .await
            .map_err(|cause| rejected(&cause.message))?;
        let path = probe.output.join("suite-native.png");
        fs::write(&path, bytes).map_err(|_| rejected("Unable to preserve suite screenshot"))?;
        evidence["screenshotPath"] = json!(path);
    }
    fs::write(
        probe.output.join("suite-document.json"),
        serde_json::to_vec_pretty(&evidence).unwrap_or_default(),
    )
    .map_err(|_| rejected("Unable to save suite evidence"))?;
    Ok(Json(evidence))
}

#[tauri::command]
pub(crate) fn pixel_probe_snapshot(
    app: tauri::AppHandle,
    conversation_id: String,
) -> Result<Vec<Value>, AgentError> {
    let home = home().ok_or_else(AgentError::storage)?;
    crate::library::agent_location(
        &app.state::<crate::persistence::AppState>(),
        &home,
        &conversation_id,
    )?;
    let directory = crate::data_dir::root(&home)
        .join("pixel-cockpit")
        .join(conversation_id);
    if !directory.exists() {
        return Ok(vec![]);
    }
    let mut receipts = Vec::new();
    for entry in fs::read_dir(directory).map_err(|_| AgentError::storage())? {
        let entry = entry.map_err(|_| AgentError::storage())?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if !name.starts_with("job-")
            || !name.ends_with(".json")
            || entry
                .file_type()
                .map_err(|_| AgentError::storage())?
                .is_symlink()
        {
            continue;
        }
        let bytes = super::attachments::bounded_read(&entry.path(), 8 * 1024 * 1024)?;
        receipts.push(serde_json::from_slice::<Value>(&bytes).map_err(|_| AgentError::storage())?);
    }
    receipts.sort_by(|left, right| left["startedAt"].as_str().cmp(&right["startedAt"].as_str()));
    Ok(receipts)
}

async fn ui(State(probe): State<Probe>, headers: HeaderMap, Json(input): Json<Value>) -> Response {
    authorize(&headers, &probe)?;
    let action = input["action"].as_str().unwrap_or("inspect");
    if ![
        "inspect", "chat", "suite", "close", "logs", "type", "flow", "send", "approve", "retry",
    ]
    .contains(&action)
    {
        return Err(rejected("Unknown preview action"));
    }
    let view = probe
        .app
        .get_webview("main")
        .ok_or_else(|| rejected("Main preview is unavailable"))?;
    let expression = format!(
        "(() => {{const preview=window.__jarvisPixelPreview;if(!preview) return {{error:'Preview fixture is not ready'}};const action={};if(action!=='inspect') preview[action]({});return {{requested:action}};}})()",
        json!(action),
        json!(input["text"].as_str().unwrap_or("")),
    );
    let requested = evaluate_preview(&view, expression).await?;
    if requested.get("error").is_some() {
        return Ok(Json(requested));
    }
    tokio::time::sleep(Duration::from_millis(250)).await;
    let evidence = evaluate_preview(&view, "window.__jarvisPixelPreview.inspect()".into()).await?;
    fs::write(
        probe.output.join(format!("ui-{action}.json")),
        serde_json::to_vec_pretty(&evidence).unwrap_or_default(),
    )
    .map_err(|_| rejected("Unable to preserve preview evidence"))?;
    let bytes = super::browser::capture_probe_view(&view)
        .await
        .map_err(|cause| rejected(&cause.message))?;
    fs::write(probe.output.join(format!("ui-{action}.png")), bytes)
        .map_err(|_| rejected("Unable to preserve preview screenshot"))?;
    Ok(Json(evidence))
}

async fn evaluate_preview(
    view: &tauri::Webview,
    expression: String,
) -> Result<Value, (StatusCode, Json<Value>)> {
    let (send, receive) = tokio::sync::oneshot::channel();
    let send = Mutex::new(Some(send));
    view.eval_with_callback(expression, move |value| {
        if let Ok(mut send) = send.lock() {
            if let Some(send) = send.take() {
                let _ = send.send(value);
            }
        }
    })
    .map_err(|_| rejected("Unable to inspect real preview components"))?;
    let evidence = tokio::time::timeout(Duration::from_secs(10), receive)
        .await
        .map_err(|_| rejected("Preview did not respond"))?
        .map_err(|_| rejected("Preview closed"))?;
    serde_json::from_str(&evidence).map_err(|_| rejected("Invalid preview evidence"))
}

async fn finish(
    State(probe): State<Probe>,
    headers: HeaderMap,
    Json(report): Json<Value>,
) -> Response {
    authorize(&headers, &probe)?;
    let bytes = serde_json::to_vec_pretty(&report).map_err(|_| rejected("Invalid report"))?;
    fs::write(probe.output.join("native-tool-report.json"), bytes)
        .map_err(|_| rejected("Unable to preserve verification report"))?;
    if !std::env::var("JARVIS_PIXEL_PROBE_KEEP_OPEN").is_ok_and(|value| value == "1") {
        let code = if report["status"] == "passed" { 0 } else { 1 };
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(Duration::from_millis(500)).await;
            probe.app.exit(code);
        });
    }
    Ok(Json(json!({"saved":true})))
}

fn copy_core(source_home: &std::path::Path, home: &std::path::Path) -> Result<(), AgentError> {
    let source = crate::core::root(source_home);
    let target = crate::core::root(home);
    let bytes = fs::read(source.join("manifest.json")).map_err(|_| {
        AgentError::new(
            "pixel_probe",
            "Install Core before running the real agent probe.",
        )
    })?;
    let manifest: crate::core::Manifest =
        serde_json::from_slice(&bytes).map_err(|_| AgentError::storage())?;
    for (id, record) in &manifest.installations {
        if !crate::core::ComponentId::ALL.contains(id)
            || *id == crate::core::ComponentId::Impeccable
        {
            continue;
        }
        let package = record.path(source_home).map_err(AgentError::from)?;
        let canonical_package = fs::canonicalize(&package).map_err(|_| AgentError::storage())?;
        let mut files = record.files.clone();
        if *id == crate::core::ComponentId::Openmontage {
            let receipt: Value = serde_json::from_slice(
                &fs::read(package.join("jarvis-openmontage.json"))
                    .map_err(|_| AgentError::storage())?,
            )
            .map_err(|_| AgentError::storage())?;
            if let Some(sources) = receipt["sources"].as_object() {
                files.extend(sources.keys().map(|file| format!("OpenMontage/{file}")));
            }
        }
        if *id == crate::core::ComponentId::Graft {
            files.push("node_modules/web-tree-sitter/package.json".into());
        }
        files.sort_unstable();
        files.dedup();
        for file in &files {
            let source = fs::canonicalize(package.join(file)).map_err(|_| AgentError::storage())?;
            if !source.starts_with(&canonical_package) {
                return Err(AgentError::new(
                    "pixel_probe",
                    "Core source escaped its installed package.",
                ));
            }
            let destination = target.join(&record.directory).join(file);
            fs::create_dir_all(destination.parent().ok_or_else(AgentError::storage)?)
                .map_err(|_| AgentError::storage())?;
            fs::copy(source, destination).map_err(|_| AgentError::storage())?;
        }
    }
    fs::write(target.join("manifest.json"), bytes).map_err(|_| AgentError::storage())
}

async fn prepare_private_agent(
    probe: &Probe,
) -> Result<crate::persistence::ProviderAccountRecord, AgentError> {
    let state = probe
        .app
        .state::<crate::persistence::AppState>()
        .inner()
        .clone();
    let source_home = probe
        .app
        .path()
        .home_dir()
        .map_err(|_| AgentError::storage())?;
    // Only the non-secret account identity is copied. Credentials stay in the OS keychain.
    let account = {
        let source_db = rusqlite::Connection::open_with_flags(
            crate::data_dir::root_for(&source_home, crate::data_dir::Profile::Production)
                .join("jarvis.db"),
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .map_err(|_| {
            AgentError::new(
                "pixel_probe",
                "The production account database is unavailable for read-only inspection.",
            )
        })?;
        crate::persistence::list_provider_accounts(&source_db)
            .map_err(|_| AgentError::storage())?
            .into_iter()
            .find(|record| record.enabled && record.provider_kind == "openai-codex")
            .ok_or_else(|| {
                AgentError::new(
                    "pixel_probe",
                    "No enabled native Codex account is available for the real agent probe.",
                )
            })?
    };
    state
        .with_connection(&probe.home, |db| {
            if !crate::persistence::provider_account_exists(
                db,
                &account.alias,
                &account.account_id,
            )? {
                crate::persistence::insert_provider_account(
                    db,
                    &account.alias,
                    &account.account_id,
                    account.display_name.as_deref(),
                )?;
            }
            Ok::<(), crate::persistence::PersistenceError>(())
        })
        .map_err(|_| AgentError::storage())?;
    let private_home = probe.home.clone();
    let package_home = source_home.clone();
    let resource = probe
        .app
        .path()
        .resource_dir()
        .map_err(|_| AgentError::storage())?;
    let mut prepared = probe.core_prepared.lock().await;
    if !*prepared {
        tokio::task::spawn_blocking(move || {
            copy_core(&package_home, &private_home)?;
            crate::core::design::setup(&resource, &private_home).map_err(AgentError::from)?;
            crate::core::require_ready(&private_home).map_err(AgentError::from)
        })
        .await
        .map_err(|_| AgentError::internal())??;
        *prepared = true;
    }
    Ok(account)
}

#[tauri::command]
pub(crate) async fn pixel_probe_context(
    app: tauri::AppHandle,
    conversation_id: String,
) -> Result<Value, AgentError> {
    let probe = app.state::<Probe>().inner().clone();
    crate::library::agent_location(
        &app.state::<crate::persistence::AppState>(),
        &probe.home,
        &conversation_id,
    )?;
    prepare_private_agent(&probe).await?;
    crate::library::get_library_snapshot(app.clone(), app.state::<crate::persistence::AppState>())
        .await
        .map_err(AgentError::from)?;
    let library = crate::library::select_library_item(
        app.clone(),
        app.state::<crate::persistence::AppState>(),
        crate::library::LibraryTarget::Conversation(conversation_id),
    )
    .await
    .map_err(AgentError::from)?;
    let accounts = crate::openai_codex::list_provider_accounts(
        app.clone(),
        app.state::<crate::persistence::AppState>(),
        app.state::<crate::openai_codex::OpenAiCodexState>(),
        None,
    )
    .await
    .map_err(AgentError::from)?;
    Ok(json!({"library":library,"accounts":accounts}))
}

async fn run_agent(
    probe: &Probe,
    prompt: String,
    requested_model: Option<String>,
    recover_call_id: Option<String>,
) -> Result<Value, AgentError> {
    let recover_call = if let Some(call_id) = recover_call_id {
        let (conversation, call) = probe
            .calls
            .lock()
            .map_err(|_| AgentError::internal())?
            .get(&call_id)
            .cloned()
            .ok_or_else(|| {
                AgentError::new(
                    "pixel_probe",
                    "Recovery requires an actual tool call issued by this live probe.",
                )
            })?;
        if conversation != probe.conversation || call.name != "pixel_art_run" {
            return Err(AgentError::new(
                "pixel_probe",
                "Recovery requires the main probe's own pixel_art_run call.",
            ));
        }
        let status = ToolCall {
            id: crate::library::new_id().map_err(AgentError::from)?,
            name: "pixel_art_status".into(),
            args: json!({"callId":call.id,"yieldTimeMs":0}),
            status: "running".into(),
            output: String::new(),
            duration_ms: 0,
        };
        let output = pixel_art::execute(
            Some(&probe.app),
            &probe.home,
            &probe.project,
            &probe.conversation,
            "main",
            &status,
            probe.cancellation.subscribe(),
        )
        .await?;
        let receipt: Value = serde_json::from_str(&output).map_err(|_| AgentError::storage())?;
        if receipt["status"] != "completed" || receipt["callId"] != call.id {
            return Err(AgentError::new(
                "pixel_probe",
                "The real call has no matching completed receipt; recovery injection refused.",
            ));
        }
        Some(call)
    } else {
        None
    };
    let state = probe
        .app
        .state::<crate::persistence::AppState>()
        .inner()
        .clone();
    let oauth = probe
        .app
        .state::<crate::openai_codex::OpenAiCodexState>()
        .inner()
        .clone();
    let account = prepare_private_agent(probe).await?;
    let model_state = state.clone();
    let model_oauth = oauth.clone();
    let model_home = probe.home.clone();
    let alias = account.alias.clone();
    let models = tokio::task::spawn_blocking(move || {
        model_oauth
            .credential_and_models(&model_state, &model_home, &alias)
            .map(|(_, models)| models)
            .map_err(AgentError::from)
    })
    .await
    .map_err(|_| AgentError::internal())??;
    let model = models
        .into_iter()
        .find(|model| {
            requested_model
                .as_ref()
                .is_none_or(|requested| requested == &model.id)
        })
        .ok_or_else(|| {
            AgentError::new(
                "pixel_probe",
                "Requested model is not present in the actual Codex account catalog.",
            )
        })?;
    let reasoning = model
        .reasoning_levels
        .iter()
        .find(|level| level.as_str() == "low")
        .cloned()
        .or(model.default_reasoning_level);
    let session = probe.app.state::<super::AgentState>().session(
        &probe.app,
        &state,
        &probe.home,
        &probe.conversation,
    )?;
    let options = super::TurnOptions {
        executor: crate::claude::Executor::Jarvis,
        account: account.alias.clone(),
        model: model.id.clone(),
        reasoning,
        service_tier: None,
        model_selection: None,
        mode: super::Mode::Build,
        workflow: Some(super::workflow::Flow::Custom),
        custom_workflow_id: None,
        custom_agent_id: Some("builtin:pixel_art".into()),
        approval_mode: super::ApprovalMode::Yolo,
        manual_validation: false,
        automatic_publication: None,
    };
    let signal = session
        .submit_message(prompt, options, vec![])?
        .ok_or_else(|| {
            AgentError::new(
                "pixel_probe",
                "The real agent conversation already has an active turn.",
            )
        })?;
    if let Some(call) = &recover_call {
        // Preserve a real operation and model only its lost acknowledgment.
        session.update(true, |data| {
            if let Some(turn) = data.turns.last_mut() {
                turn.wire.push(json!({
                    "type":"function_call","call_id":call.id,"name":call.name,
                    "arguments":call.args.to_string()
                }));
                turn.wire.push(json!({
                    "type":"function_call_output","call_id":call.id,
                    "output":super::journal::UNKNOWN_TOOL_OUTPUT
                }));
                turn.wire.push(json!({
                    "role":"user",
                    "content":"Execution resumed after acknowledgment loss; inspect the exact saved call before proceeding.",
                    "_jarvis_runtime":true,"_jarvis_retry":true
                }));
            }
        })?;
    }
    let result = tokio::time::timeout(
        Duration::from_secs(1200),
        super::workflow::run(
            &session,
            (
                state,
                oauth,
                probe.app.state::<crate::mcp::McpState>().inner().clone(),
                probe.home.clone(),
            ),
            &probe.app,
            signal,
            None,
        ),
    )
    .await;
    let result = result
        .map_err(|_| AgentError::new("pixel_probe", "The actual agent exceeded the verification deadline. Its transcript was preserved; do not automatically repeat the request."))
        .and_then(std::convert::identity);
    let failure = result.as_ref().err().cloned();
    session.drain_interactions(result.is_err()).await;
    super::finish(&session, result);
    let snapshot = session.snapshot()?;
    fs::write(
        probe.output.join("native-agent-transcript.json"),
        serde_json::to_vec_pretty(&snapshot).map_err(|_| AgentError::storage())?,
    )
    .map_err(|_| AgentError::storage())?;
    if let Some(failure) = failure {
        return Err(failure);
    }
    let tools = snapshot
        .turns
        .iter()
        .flat_map(|turn| &turn.steps)
        .flat_map(|step| &step.tools)
        .collect::<Vec<_>>();
    let used_docs = tools
        .iter()
        .any(|tool| tool.name == "pixel_art_docs" && tool.status == "completed");
    let used_run = tools
        .iter()
        .any(|tool| tool.name == "pixel_art_run" && tool.status == "completed");
    let recovery_resolved = recover_call.as_ref().is_none_or(|call| {
        tools.iter().any(|tool| {
            tool.name == "recovery_resolve"
                && tool.status == "completed"
                && tool.args["callId"] == call.id
                && tool.args["outcome"] == "applied"
                && serde_json::from_str::<Value>(&tool.output).is_ok_and(|receipt| {
                    receipt["callId"] == call.id && receipt["outcome"] == "applied"
                })
        })
    });
    let report = json!({
        "status":if agent_verification_passed(recover_call.is_some(), used_docs, used_run, recovery_resolved) {"passed"} else {"blocked"},
        "scope":"Actual Codex provider -> builtin:pixel_art -> native pixel-art tools; inspect transcript and artifacts for delivery",
        "model":model.id, "provider":"openai-codex", "conversationId":probe.conversation,
        "usedCatalog":used_docs, "usedPixelTools":used_run, "tools":tools,
        "injectedUnknownCallId":recover_call.as_ref().map(|call| &call.id),
        "recoveryResolved":recovery_resolved,
        "transcript":probe.output.join("native-agent-transcript.json")
    });
    fs::write(
        probe.output.join("native-agent-report.json"),
        serde_json::to_vec_pretty(&report).map_err(|_| AgentError::storage())?,
    )
    .map_err(|_| AgentError::storage())?;
    Ok(report)
}

fn agent_verification_passed(
    recovering: bool,
    used_docs: bool,
    used_run: bool,
    recovery_resolved: bool,
) -> bool {
    // Recovery must prove the previous mutation, never repeat it just to satisfy a test.
    if recovering {
        recovery_resolved
    } else {
        used_docs && used_run
    }
}

async fn agent_status(State(probe): State<Probe>, headers: HeaderMap) -> Response {
    authorize(&headers, &probe)?;
    Ok(Json(probe.agent_result.lock().await.clone()))
}

async fn agent_start(
    State(probe): State<Probe>,
    headers: HeaderMap,
    Json(input): Json<Value>,
) -> Response {
    authorize(&headers, &probe)?;
    let prompt = input["prompt"]
        .as_str()
        .filter(|value| !value.trim().is_empty() && value.len() <= 64000)
        .ok_or_else(|| rejected("An explicit bounded prompt is required."))?
        .to_owned();
    let model = input["model"].as_str().map(str::to_owned);
    let recover_call_id = input["recoverCallId"].as_str().map(str::to_owned);
    let mut result = probe.agent_result.lock().await;
    if result["status"] == "running" {
        return Err(rejected("The actual agent is already running."));
    }
    *result = json!({"status":"running"});
    drop(result);
    tauri::async_runtime::spawn(async move {
        let result = match run_agent(&probe, prompt, model, recover_call_id).await {
            Ok(report) => report,
            Err(cause) => json!({"status":"blocked","error":cause.message}),
        };
        *probe.agent_result.lock().await = result;
    });
    Ok(Json(json!({"status":"running"})))
}

fn initialize(app: &mut tauri::App, output: PathBuf) -> Result<Probe, AgentError> {
    let home = output.join("private-home");
    let project = output.join("project");
    fs::create_dir_all(&home).map_err(|_| AgentError::storage())?;
    fs::create_dir_all(&project).map_err(|_| AgentError::storage())?;
    // Library project roots are canonical, including the Windows verbatim prefix.
    let output = fs::canonicalize(output).map_err(|_| AgentError::storage())?;
    let home = fs::canonicalize(home).map_err(|_| AgentError::storage())?;
    let project = fs::canonicalize(project).map_err(|_| AgentError::storage())?;
    PROBE_HOME
        .set(home.clone())
        .map_err(|_| AgentError::new("pixel_probe", "Probe is already initialized."))?;
    let workspace = crate::library::new_id().map_err(AgentError::from)?;
    let project_id = crate::library::new_id().map_err(AgentError::from)?;
    let conversation = app
        .state::<crate::persistence::AppState>()
        .with_connection(&home, |db| {
            use rusqlite::OptionalExtension;
            if let Ok(bytes) = fs::read(output.join("primary-session.json")) {
                let saved: Value = serde_json::from_slice(&bytes).map_err(|_| AgentError::storage())?;
                let id = saved["conversationId"].as_str().ok_or_else(AgentError::storage)?;
                let exists: bool = db.query_row(
                    "SELECT EXISTS(SELECT 1 FROM conversations c JOIN projects p ON p.id=c.project_id WHERE c.id=?1 AND p.path=?2)",
                    rusqlite::params![id, project.to_string_lossy()], |row| row.get(0),
                ).map_err(|_| AgentError::storage())?;
                if !exists { return Err(AgentError::storage()); }
                return Ok(id.to_owned());
            }
            if let Some(conversation) = db
                .query_row(
                    "SELECT c.id FROM conversations c JOIN projects p ON p.id=c.project_id WHERE p.path=?1 ORDER BY c.created_at DESC LIMIT 1",
                    [project.to_string_lossy().as_ref()],
                    |row| row.get::<_, String>(0),
                )
                .optional()
                .map_err(|_| AgentError::storage())?
            {
                return Ok(conversation);
            }
            db.execute(
                "INSERT INTO workspaces(id,name) VALUES(?1,'Pixel verification')",
                [&workspace],
            )
            .map_err(|_| AgentError::storage())?;
            db.execute(
            "INSERT INTO projects(id,workspace_id,name,path) VALUES(?1,?2,'Pixel verification',?3)",
            rusqlite::params![project_id, workspace, project.to_string_lossy()],
        ).map_err(|_| AgentError::storage())?;
            crate::library::companion::create_conversation_silently(db, &home, &project_id)
                .map_err(AgentError::from)
        })?;
    fs::write(
        output.join("primary-session.json"),
        json!({"conversationId":conversation}).to_string(),
    )
    .map_err(|_| AgentError::storage())?;
    let (cancellation, _) = watch::channel(false);
    Ok(Probe {
        app: app.handle().clone(),
        home,
        project,
        output,
        conversation,
        token: crate::library::new_id().map_err(AgentError::from)?,
        cancellation: Arc::new(cancellation),
        agent_result: Arc::new(tokio::sync::Mutex::new(json!({"status":"idle"}))),
        calls: Arc::new(Mutex::new(HashMap::new())),
        core_prepared: Arc::new(tokio::sync::Mutex::new(false)),
    })
}

pub(crate) fn run() {
    let output_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.codex/pixel-review");
    fs::create_dir_all(&output_root).expect("create probe output root");
    let output_root = fs::canonicalize(output_root).expect("canonical probe root");
    let output = if let Ok(requested) = std::env::var("JARVIS_PIXEL_PROBE_OUTPUT") {
        let requested = fs::canonicalize(requested).expect("existing probe output");
        assert_eq!(
            requested.parent(),
            Some(output_root.as_path()),
            "Only a dedicated pixel-review directory can be resumed"
        );
        requested
    } else {
        output_root.join(crate::library::new_id().expect("probe identifier"))
    };
    let mut context = crate::application_context();
    if let Ok(frontend) = std::env::var("JARVIS_PIXEL_PROBE_URL") {
        let origin = preview_origin(&frontend).expect("local pixel preview origin");
        // Match the normal desktop frontend's local-origin IPC classification.
        // This is confined to the opt-in diagnostic app, never the worker.
        context.config_mut().build.dev_url = Some(origin.clone());
        context.config_mut().build.frontend_dist =
            Some(tauri::utils::config::FrontendDist::Url(origin));
    }
    context.config_mut().identifier = "com.foxtag.jarvis.pixel-probe".into();
    context.config_mut().app.windows.clear();
    tauri::Builder::default()
        .runtime(tauri_runtime_cef::Cef::default().root_cache_path(output.join("cef")))
        .manage(crate::persistence::AppState::default())
        .manage(pixel_art::State::default())
        .manage(super::AgentState::default())
        .manage(crate::openai_codex::OpenAiCodexState::default())
        .manage(crate::mcp::McpState::default())
        .manage(crate::core::CoreState::default())
        .manage(crate::agent::browser::BrowserState::default())
        .manage(crate::agent::computer::ComputerState::default())
        .manage(crate::core::impeccable_live::ImpeccableLiveState::default())
        .manage(crate::updater::UpdateState::default())
        .manage(crate::voice::VoiceState::default())
        .manage(crate::http_client::HttpState::default())
        .manage(crate::agent::browser::extension::ExtensionState::default())
        .manage(crate::agent::learning::capture::LearningJobs::default())
        .manage(crate::system::SystemState::default())
        .manage(crate::claude::ClaudeState::default())
        .manage(crate::muse::MuseState::default())
        .plugin(tauri_plugin_dialog::init())
        .plugin(pixel_art::processing::plugin())
        .invoke_handler(|invoke| crate::main_only(invoke, tauri::generate_handler![
            pixel_art::processing::process_pixel_art_image,
            super::attachments::get_chat_attachment_image,
            pixel_art::get_pixel_art_session,
            pixel_art::get_pixel_art_jobs,
            pixel_art::save_pixel_art_artifact,
            pixel_art::open_pixel_art_suite,
            pixel_art::set_pixel_art_viewport,
            pixel_probe_snapshot,
            pixel_probe_context,
            crate::voice::get_voice_settings,
            crate::http_client::get_http_snapshot,
            crate::agent::browser::get_browser_tabs,
            crate::agent::browser::browser_command,
            crate::agent::browser::set_browser_viewport,
            crate::agent::browser::extension::get_browser_extension_status,
            crate::agent::learning::get_project_learning,
            crate::core::impeccable_live::get_impeccable_live,
            crate::agent::computer::commands::get_computer_use_status,
            crate::agent::subscribe_chat,
            crate::agent::workflow::get_workflow,
            crate::agent::workflow::catalog::get_workflow_catalog,
            crate::agent::workflow::settings::chat::get_chat_agent_models,
            crate::agent::workflow::settings::chat::set_chat_agent_model,
            crate::agent::workflow::get_workflow_transcript,
            crate::agent::history::get_chat_history,
            crate::agent::start_agent_turn,
            crate::agent::retry_agent_turn,
            crate::agent::cancel_agent_turn,
            crate::agent::approve_agent_tool,
            crate::agent::questions::answer_agent_question,
            crate::agent::provider_links::get_provider_model_references,
            crate::agent::provider_links::clear_chat_model_binding,
            crate::claude::get_claude_runtime,
            crate::muse::get_muse_runtime,
            crate::agent::message_prediction::get_chat_prediction,
            crate::agent::message_prediction::dismiss_chat_prediction,
            crate::skills::list_skills,
        ]))
        .setup(move |app| {
            let probe = initialize(app, output).map_err(|cause| std::io::Error::other(cause.message))?;
            app.state::<crate::system::SystemState>()
                .initialize_tray_probe(crate::data_dir::root(&probe.home).join("system.json"))
                .map_err(std::io::Error::other)?;
            app.manage(probe.clone());
            let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
            listener.set_nonblocking(true)?;
            let address = listener.local_addr()?;
            let connection = probe.output.join("connection.json");
            fs::write(&connection, serde_json::to_vec_pretty(&json!({
                "url":format!("http://{address}"),"token":probe.token,"output":probe.output
            }))?)?;
            println!("Pixel probe connection: {}", connection.display());
            let conversation = probe.conversation.clone();
            let router = Router::new()
                .route("/", get(|| async { "Jarvis Pixel Cockpit verification. The suite opens after its first native command." }))
                .route("/info", get(info))
                .route("/session", post(create_session))
                .route("/tool", post(tool))
                .route("/suite", post(suite))
                .route("/ui", post(ui))
                .route("/agent", get(agent_status).post(agent_start))
                .route("/finish", post(finish))
                .layer(DefaultBodyLimit::max(8 * 1024 * 1024))
                .with_state(probe);
            tauri::async_runtime::spawn(async move {
                let listener = tokio::net::TcpListener::from_std(listener).expect("probe listener");
                if let Err(cause) = axum::serve(listener, router).await {
                    eprintln!("Pixel probe server failed: {cause}");
                }
            });
            let mut frontend = tauri::Url::parse(
                &std::env::var("JARVIS_PIXEL_PROBE_URL").unwrap_or_else(|_| format!("http://{address}/")),
            )?;
            if !matches!(frontend.host_str(), Some("localhost" | "127.0.0.1")) {
                return Err(std::io::Error::other("The probe frontend must be local.").into());
            }
            frontend.query_pairs_mut().append_pair("pixelProbe", "1").append_pair("conversationId", &conversation);
            let window = tauri::WebviewWindowBuilder::new(
                app, "main", tauri::WebviewUrl::External(frontend),
            ).title("Jarvis — verificação real de pixel art").inner_size(1280., 900.).visible(true).build()?;
            // Production shows its initially hidden parent after desktop setup.
            // This fixture creates an already-visible parent, so also notify CEF
            // that its child is visible before sampling animations or viewport RAFs.
            let main: &tauri::Webview = window.as_ref();
            main.show()?;
            main.set_focus()?;
            Ok(())
        })
        .run(context)
        .expect("run pixel cockpit probe");
}

fn preview_origin(frontend: &str) -> Result<tauri::Url, std::io::Error> {
    let mut url = tauri::Url::parse(frontend)
        .map_err(|_| std::io::Error::other("Invalid pixel preview URL"))?;
    if url.scheme() != "http"
        || !matches!(url.host_str(), Some("localhost" | "127.0.0.1"))
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(std::io::Error::other(
            "The pixel preview must use a local HTTP origin.",
        ));
    }
    url.set_path("/");
    url.set_query(None);
    url.set_fragment(None);
    Ok(url)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recovery_verification_does_not_require_repeating_the_confirmed_mutation() {
        assert!(agent_verification_passed(true, false, false, true));
        assert!(!agent_verification_passed(true, true, true, false));
        assert!(!agent_verification_passed(false, true, false, true));
        assert!(agent_verification_passed(false, true, true, false));
    }

    #[test]
    fn driver_accepts_only_explicit_tool_arguments() {
        assert!(
            serde_json::from_value::<Command>(json!({"name":"pixel_art_docs","arguments":{}}))
                .is_ok()
        );
        assert!(serde_json::from_value::<Command>(
            json!({"name":"pixel_art_docs","arguments":{},"shell":"not available"})
        )
        .is_err());
        assert_eq!(
            preview_origin("http://127.0.0.1:1420/scripts/pixel-cockpit/preview.html?pixelProbe=1")
                .unwrap()
                .as_str(),
            "http://127.0.0.1:1420/",
        );
        assert!(preview_origin("https://example.com/").is_err());
        assert!(preview_origin("http://user@127.0.0.1/").is_err());
    }
}
