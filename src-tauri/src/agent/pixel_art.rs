//! Adopted cockpit execution in an isolated, invisible CEF worker per conversation.
pub(crate) mod processing;
use super::{attachments, tools, AgentError, Mode, ToolCall};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::Stdio,
    sync::{Arc, Mutex},
    time::Duration,
};
use tauri::{Emitter, Manager, Webview, WebviewBuilder, WebviewUrl};
use tokio::{
    io::AsyncBufReadExt,
    sync::{watch, Mutex as AsyncMutex},
};

type SharedHost = Arc<AsyncMutex<Host>>;
#[derive(Default)]
pub(crate) struct State(AsyncMutex<BTreeMap<String, SharedHost>>);
struct Host {
    child: Box<dyn process_wrap::tokio::ChildWrapper>,
    view: Webview,
    directory: PathBuf,
    jobs: BTreeMap<String, Value>,
    images: BTreeMap<String, Value>,
    manual: BTreeMap<String, Value>,
}
impl Drop for Host {
    fn drop(&mut self) {
        let _ = self.view.close();
    }
}
fn error(message: &str) -> AgentError {
    AgentError::new("pixel_art", message)
}
const ACTIONS: &[&str] = &[
    "state",
    "configure",
    "import_image",
    "import_animation_pack",
    "generate",
    "select",
    "preset",
    "annotate",
    "timeline_open",
    "review_open",
    "vfx_open",
    "inspect",
    "export",
    "invoke",
    "save_artifact",
    "recover_grid",
    "extract_poses",
    "production_recipe",
];

pub(super) fn definitions(mode: Mode) -> Vec<Value> {
    let mut result = vec![
        tools::definition("pixel_art_docs", "Read the complete adopted Image Cockpit action catalog, parameter schemas, processing operations, generation/animation controls, exports and QA. Without filters returns every available action/module name; select query, section, controller, module or method for exact schemas and instructions. Read before invoking a tool; do not invent options. The engine uses the installed authenticated Codex CLI, independent of the chat model.", json!({"query":{"type":"string","maxLength":200},"section":{"type":"string","maxLength":100},"controller":{"type":"string","maxLength":100},"module":{"type":"string","maxLength":100},"method":{"type":"string","maxLength":100}}), &[]),
        tools::definition("pixel_art_status", "Observe an existing pixel-art job without restarting it. Pass jobId from its receipt, or callId of an interrupted pixel_art_run when its acknowledgment was lost. Returns real progress, original logs, quality results and verified artifacts. While generation is running, prefer yieldTimeMs=30000 to avoid rapid polling. Wait until completed/blocked/failed/cancelled before reporting delivery. Safe across resumed turns; interrupted operations are never automatically repeated.", json!({"jobId":{"type":"string","minLength":1},"callId":{"type":"string","minLength":1,"maxLength":512},"yieldTimeMs":{"type":"integer","minimum":0,"maximum":30000}}), &[]),
    ];
    if mode == Mode::Build {
        result.extend([
            tools::definition("pixel_art_run", "Operate the adopted Image Cockpit through its exact catalog. Supports generation, editing, sprite animation, effect animation and every original processing/QA/export operation. Read pixel_art_docs first. import_image accepts an existing project-relative path or attachmentId from this conversation. save_artifact copies a confirmed artifact into the project: {jobId, path, destination: project-relative path, overwrite: false}. Arguments follow the catalog. Each call starts one job and returns jobId; follow with pixel_art_status instead of starting duplicate work. Original exports and accepted sources are preserved.", json!({"action":{"type":"string","enum":ACTIONS},"arguments":{"type":"object"},"yieldTimeMs":{"type":"integer","minimum":0,"maximum":30000}}), &["action","arguments"]),
            tools::definition("pixel_art_cancel", "Cancel the current cockpit operation and its generation subprocesses, preserving confirmed artifacts and original logs. Never silently restart cancelled or uncertain paid generation.", json!({"jobId":{"type":"string","minLength":1}}), &["jobId"]),
        ]);
    }
    result
}
pub(super) fn restrict_catalog(definitions: &mut Vec<Value>, pixel_agent: bool) {
    if !pixel_agent {
        definitions.retain(|definition| {
            !definition["name"]
                .as_str()
                .is_some_and(|name| name.starts_with("pixel_art_"))
        });
    }
}

fn identifier(value: &str) -> Result<(), AgentError> {
    if value.len() != 32 || !value.bytes().all(|c| c.is_ascii_hexdigit()) {
        return Err(error("Identificador de operação inválido."));
    }
    Ok(())
}
pub(super) fn operation_id(conversation: &str, executor: &str, call: &str) -> String {
    format!(
        "{:x}",
        Sha256::digest(format!("pixel-art:{conversation}:{executor}:{call}"))
    )[..32]
        .into()
}
fn directory(home: &Path, conversation: &str) -> Result<PathBuf, AgentError> {
    identifier(conversation)?;
    let root = crate::data_dir::root(home);
    let parent = root.join("pixel-cockpit");
    let directory = parent.join(conversation);
    for path in [&root, &parent, &directory] {
        if fs::symlink_metadata(path).is_ok_and(|meta| meta.is_symlink() || !meta.is_dir()) {
            return Err(error("A pasta privada de pixel art não é válida."));
        }
    }
    fs::create_dir_all(&directory).map_err(|_| AgentError::storage())?;
    directory.canonicalize().map_err(|_| AgentError::storage())
}
fn runtime(home: &Path) -> Result<PathBuf, AgentError> {
    // Reuse a verified Core runtime; an installed Node/Bun is also supported.
    for component in [
        crate::core::ComponentId::ContextMode,
        crate::core::ComponentId::Context7,
        crate::core::ComponentId::Graft,
    ] {
        if let Ok(package) = crate::core::installed(home, component).and_then(|p| p.path(home)) {
            let node = package.join(if cfg!(windows) {
                "runtime/node.exe"
            } else {
                "runtime/bin/node"
            });
            if node.is_file() {
                return Ok(node);
            }
        }
    }
    for name in if cfg!(windows) {
        ["node.exe", "bun.exe"]
    } else {
        ["node", "bun"]
    } {
        for folder in std::env::split_paths(&crate::mcp::executable::configured_path()) {
            let path = folder.join(name);
            if path.is_file() {
                return Ok(path);
            }
        }
    }
    Err(error("O runtime Node não está disponível. Instale ou repare o Context-mode no Core e tente novamente; nenhuma geração foi iniciada."))
}
fn engine_command(
    runtime: &Path,
    resources: &Path,
    directory: &Path,
    project: &Path,
    token: &str,
) -> tokio::process::Command {
    // Node 22 cannot resolve a Windows verbatim entrypoint. Canonical paths remain
    // authoritative internally; only external-process inputs use ordinary paths.
    let external = |path: &Path| {
        PathBuf::from(crate::library::strip_verbatim(&path.to_string_lossy()).as_ref())
    };
    let mut command = crate::background::tokio_command(external(runtime));
    command
        .arg(external(&resources.join("server.mjs")))
        .current_dir(external(directory))
        .env(
            "IMAGE_COCKPIT_HANDOFF_DIR",
            external(&directory.join("handoff")),
        )
        .env("IMAGE_COCKPIT_API_PORT", "0")
        .env("JARVIS_PIXEL_RESOURCE_DIR", external(resources))
        .env("JARVIS_PIXEL_SESSION_TOKEN", token)
        .env("JARVIS_PIXEL_WORKSPACE", external(project))
        .env("PATH", crate::mcp::executable::configured_path());
    command
}
async fn evaluate(view: &Webview, expression: String) -> Result<Value, AgentError> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let tx = Mutex::new(Some(tx));
    view.eval_with_callback(format!("(() => {{ try {{ return {expression}; }} catch (error) {{ return {{bridgeError:String(error)}}; }} }})()"), move |value| {
        if let Ok(mut tx) = tx.lock() { if let Some(tx) = tx.take() { let _ = tx.send(value); } }
    }).map_err(|_| error("Não foi possível consultar o motor de pixel art."))?;
    let response = tokio::time::timeout(Duration::from_secs(10), rx).await
        .map_err(|_| error("O motor de pixel art não respondeu em 10 segundos. Os artefatos foram preservados."))?
        .map_err(|_| error("O motor de pixel art foi encerrado."))?;
    if response.len() > 8 * 1024 * 1024 {
        return Err(error(
            "A resposta do motor excedeu 8 MB. Consulte os logs preservados na pasta da sessão.",
        ));
    }
    let value: Value = serde_json::from_str(&response)
        .map_err(|_| error("O motor retornou uma resposta inválida."))?;
    if let Some(message) = value["bridgeError"].as_str() {
        return Err(error(message));
    }
    Ok(value)
}
async fn await_worker_ready<F, Fut>(mut probe: F, limit: Duration) -> Result<(), AgentError>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<bool, AgentError>>,
{
    let deadline = tokio::time::Instant::now() + limit;
    let mut last_error = None;
    loop {
        // A new CEF child can receive evaluation before its JS context exists.
        // Readiness checks are safe to retry; effectful commands are not.
        match tokio::time::timeout_at(deadline, probe()).await {
            Ok(Ok(true)) => return Ok(()),
            Ok(Ok(false)) => {}
            Ok(Err(cause)) => last_error = Some(cause.message),
            Err(_) => break,
        }
        if tokio::time::Instant::now() >= deadline {
            break;
        }
        tokio::time::sleep_until(
            (tokio::time::Instant::now() + Duration::from_millis(100)).min(deadline),
        )
        .await;
    }
    Err(error(&format!(
        "O processador gráfico não iniciou. Os arquivos existentes foram preservados.{}",
        last_error
            .map(|cause| format!(" Detalhe: {cause}"))
            .unwrap_or_default()
    )))
}
async fn start_host(
    app: &tauri::AppHandle,
    home: &Path,
    project: &Path,
    conversation: &str,
) -> Result<Host, AgentError> {
    let directory = directory(home, conversation)?;
    let resources = app
        .path()
        .resolve("pixel-cockpit", tauri::path::BaseDirectory::Resource)
        .map_err(|_| AgentError::storage())?;
    let resources = if resources.join("server.mjs").is_file() {
        resources
    } else {
        let development = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/pixel-cockpit");
        if !cfg!(debug_assertions) || !development.join("server.mjs").is_file() {
            return Err(error("O pacote Image Cockpit está ausente neste Jarvis. Instale uma versão com o pacote completo; nenhuma geração foi iniciada."));
        }
        development
    };
    let token = crate::library::new_id()?;
    let mut command = engine_command(&runtime(home)?, &resources, &directory, project, &token);
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::from(
            fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(directory.join("server.log"))
                .map_err(|_| AgentError::storage())?,
        ))
        .kill_on_drop(true);
    crate::background::prepare_node(&mut command)
        .map_err(|_| error("Não foi possível preparar o runtime de pixel art."))?;
    let mut wrapped = process_wrap::tokio::CommandWrap::from(command);
    #[cfg(unix)]
    wrapped.wrap(process_wrap::tokio::ProcessGroup::leader());
    #[cfg(windows)]
    crate::background::windows_job(&mut wrapped);
    wrapped.wrap(process_wrap::tokio::KillOnDrop);
    let mut child = wrapped.spawn().map_err(|_| {
        error("Não foi possível iniciar o motor de pixel art. Confira o runtime do Core.")
    })?;
    let stdout = child.stdout().take().ok_or_else(AgentError::internal)?;
    let mut lines = tokio::io::BufReader::new(stdout).lines();
    let ready = tokio::time::timeout(Duration::from_secs(20), async {
        while let Some(line) = lines
            .next_line()
            .await
            .map_err(|_| AgentError::internal())?
        {
            if line.len() > 64 * 1024 {
                return Err(error("Resposta de inicialização inválida."));
            }
            if let Ok(value) = serde_json::from_str::<Value>(&line) {
                if value["type"] == "ready" {
                    return Ok(value);
                }
            }
        }
        Err(error(
            "O motor encerrou antes de iniciar. Consulte server.log na pasta da sessão.",
        ))
    })
    .await
    .map_err(|_| error("O motor de pixel art demorou para iniciar; consulte server.log."))??;
    // Drain remaining diagnostics so a full pipe cannot stall generation.
    tauri::async_runtime::spawn(async move {
        while lines.next_line().await.ok().flatten().is_some() {}
    });
    let port = ready["port"]
        .as_u64()
        .filter(|p| *p > 0 && *p <= u16::MAX as u64)
        .ok_or_else(|| error("Porta local inválida."))?;
    let origin = format!("http://127.0.0.1:{port}");
    let url = tauri::Url::parse(&format!("{origin}/?token={token}"))
        .map_err(|_| AgentError::internal())?;
    let allowed_origin = origin.clone();
    let builder = WebviewBuilder::new(
        format!("pixel-worker-{conversation}"),
        WebviewUrl::External(url),
    )
    .incognito(true)
    .focused(false)
    .on_navigation(move |url| url.origin().ascii_serialization() == allowed_origin)
    .on_download(|_, _| false)
    .on_new_window(|_, _| tauri::webview::NewWindowResponse::Deny);
    let window = app
        .get_window("main")
        .ok_or_else(|| error("Janela principal indisponível."))?;
    let view = window
        .add_child(
            builder,
            tauri::LogicalPosition::new(0., 0.),
            tauri::LogicalSize::new(1024., 768.),
        )
        .map_err(|_| error("Não foi possível iniciar o processamento gráfico de pixel art."))?;
    view.set_auto_resize(false)
        .map_err(|_| AgentError::internal())?;
    view.hide().map_err(|_| AgentError::internal())?;
    let host = Host {
        child,
        view,
        directory,
        jobs: BTreeMap::new(),
        images: BTreeMap::new(),
        manual: BTreeMap::new(),
    };
    await_worker_ready(
        || async {
            evaluate(
                &host.view,
                "Boolean(window.__jarvisPixel?.snapshot()?.ready)".into(),
            )
            .await?
            .as_bool()
            .ok_or_else(|| error("O processador gráfico ainda não está disponível."))
        },
        Duration::from_secs(25),
    )
    .await?;
    Ok(host)
}
async fn host(
    app: &tauri::AppHandle,
    home: &Path,
    project: &Path,
    conversation: &str,
) -> Result<SharedHost, AgentError> {
    let state = app.state::<State>();
    let mut hosts = state.0.lock().await;
    if let Some(host) = hosts.get(conversation) {
        if host
            .lock()
            .await
            .child
            .try_wait()
            .map_err(|_| AgentError::internal())?
            .is_none()
        {
            return Ok(host.clone());
        }
    }
    hosts.remove(conversation);
    let host = Arc::new(AsyncMutex::new(
        start_host(app, home, project, conversation).await?,
    ));
    hosts.insert(conversation.into(), host.clone());
    let _ = app.emit(
        "pixel-art-session",
        json!({"conversationId":conversation,"sessionId":conversation}),
    );
    monitor_activity(
        app.clone(),
        Arc::downgrade(&host),
        home.into(),
        conversation.into(),
    );
    Ok(host)
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Viewport {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}
fn authorized_location(
    app: &tauri::AppHandle,
    conversation: &str,
) -> Result<(PathBuf, PathBuf), AgentError> {
    let home = crate::data_dir::app_home(&app).map_err(|_| AgentError::storage())?;
    let (_, project) = crate::library::agent_location(
        &app.state::<crate::persistence::AppState>(),
        &home,
        conversation,
    )?;
    Ok((home, project))
}
#[tauri::command]
pub async fn get_pixel_art_session(
    app: tauri::AppHandle,
    conversation_id: String,
) -> Result<Option<Value>, AgentError> {
    let (home, _) = authorized_location(&app, &conversation_id)?;
    let present = app
        .state::<State>()
        .0
        .lock()
        .await
        .contains_key(&conversation_id)
        || crate::data_dir::root(&home)
            .join("pixel-cockpit")
            .join(&conversation_id)
            .is_dir();
    Ok(present.then(|| json!({"conversationId":conversation_id,"sessionId":conversation_id})))
}
#[tauri::command]
pub async fn get_pixel_art_jobs(
    app: tauri::AppHandle,
    conversation_id: String,
) -> Result<Vec<Value>, AgentError> {
    let (home, _) = authorized_location(&app, &conversation_id)?;
    let path = crate::data_dir::root(&home)
        .join("pixel-cockpit")
        .join(&conversation_id);
    if !path.exists() {
        return Ok(vec![]);
    }
    let directory = directory(&home, &conversation_id)?;
    let shared = app
        .state::<State>()
        .0
        .lock()
        .await
        .get(&conversation_id)
        .cloned();
    let live = if let Some(shared) = shared {
        let mut host = shared.lock().await;
        if matches!(host.child.try_wait(), Ok(None)) {
            host.jobs.clone()
        } else {
            BTreeMap::new()
        }
    } else {
        BTreeMap::new()
    };
    tauri::async_runtime::spawn_blocking(move || {
        let mut paths = Vec::new();
        for entry in fs::read_dir(&directory).map_err(|_| AgentError::storage())? {
            let entry = entry.map_err(|_| AgentError::storage())?;
            if !entry
                .file_type()
                .map_err(|_| AgentError::storage())?
                .is_file()
            {
                continue;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            let Some(id) = name
                .strip_prefix("job-")
                .and_then(|name| name.strip_suffix(".json"))
            else {
                continue;
            };
            if identifier(id).is_err() {
                continue;
            }
            let changed = entry.metadata().and_then(|m| m.modified()).ok();
            paths.push((changed, id.to_owned(), entry.path()));
        }
        paths.sort_by(|a, b| b.0.cmp(&a.0));
        let mut results = Vec::new();
        for (_, id, path) in paths.into_iter().take(500) {
            let value = if let Some(value) = live.get(&id) {
                value.clone()
            } else {
                let (value, interrupted) = recover_receipt(&path)?;
                if interrupted {
                    let _ = app.emit("pixel-art-progress", &value);
                }
                value
            };
            if value["conversationId"] != conversation_id || value["jobId"] != id {
                return Err(error("O recibo de pixel art não pertence à conversa."));
            }
            results.push(presentation_receipt(value, &path));
        }
        Ok(results)
    })
    .await
    .map_err(|_| AgentError::internal())?
}
fn presentation_receipt(mut value: Value, path: &Path) -> Value {
    if let Some(object) = value.as_object_mut() {
        object.retain(|key, _| {
            matches!(
                key.as_str(),
                "kind"
                    | "sessionId"
                    | "conversationId"
                    | "jobId"
                    | "callId"
                    | "action"
                    | "status"
                    | "phase"
                    | "progress"
                    | "logs"
                    | "images"
                    | "artifacts"
                    | "error"
                    | "nextAction"
                    | "revision"
                    | "manual"
                    | "logPath"
                    | "startedAt"
                    | "finishedAt"
            )
        });
    }
    if let Some(logs) = value["logs"].as_array_mut() {
        if logs.len() > 200 {
            logs.drain(..logs.len() - 200);
            if value["logPath"].is_null() {
                value["logPath"] = json!(path);
            }
        }
    }
    value
}

fn artifact_source(
    directory: &Path,
    conversation: &str,
    job: &str,
    path: &str,
) -> Result<PathBuf, AgentError> {
    let receipt_path = receipt(directory, job)?;
    let receipt_path = tools::scoped(directory, &receipt_path.to_string_lossy(), false)?;
    let value: Value =
        serde_json::from_slice(&attachments::bounded_read(&receipt_path, 8 * 1024 * 1024)?)
            .map_err(|_| error("O recibo de pixel art é inválido."))?;
    if value["conversationId"] != conversation || value["jobId"] != job {
        return Err(error("O artefato não pertence a esta conversa."));
    }
    let source = tools::scoped(directory, path, false)?;
    let listed = value["artifacts"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|artifact| artifact["path"].as_str())
        .any(|listed| tools::scoped(directory, listed, false).is_ok_and(|listed| listed == source));
    if !listed || !source.is_file() {
        return Err(error(
            "O arquivo não é um artefato confirmado desta operação.",
        ));
    }
    Ok(source)
}

fn copy_artifact(source: &Path, destination: &Path, overwrite: bool) -> Result<u64, AgentError> {
    if source == destination {
        return Err(error("Escolha um destino diferente do arquivo original."));
    }
    if let Ok(meta) = fs::symlink_metadata(destination) {
        if !meta.is_file() || meta.is_symlink() {
            return Err(error(
                "O destino precisa ser um arquivo regular, sem link simbólico.",
            ));
        }
        if !overwrite {
            return Err(error("O destino já existe e foi preservado. Escolha outro caminho ou informe overwrite=true."));
        }
    }
    let parent = destination
        .parent()
        .ok_or_else(|| error("Destino inválido."))?;
    let mut staged = tempfile::NamedTempFile::new_in(parent)
        .map_err(|_| error("Não foi possível preparar o arquivo no destino."))?;
    let mut input =
        fs::File::open(source).map_err(|_| error("Não foi possível ler o artefato."))?;
    let bytes = std::io::copy(&mut input, &mut staged).map_err(|_| {
        error("Não foi possível copiar o artefato; o destino anterior foi preservado.")
    })?;
    staged
        .as_file()
        .sync_all()
        .map_err(|_| AgentError::storage())?;
    if overwrite {
        staged
            .persist(destination)
            .map_err(|_| error("Não foi possível substituir o destino."))?;
    } else {
        staged.persist_noclobber(destination).map_err(|_| error("Não foi possível salvar sem substituir um arquivo existente. O destino foi preservado."))?;
    }
    Ok(bytes)
}

fn save_artifact_to_project(
    directory: &Path,
    project: &Path,
    conversation: &str,
    args: &Value,
) -> Result<Value, AgentError> {
    let job = args["jobId"]
        .as_str()
        .ok_or_else(|| error("Informe o jobId de origem."))?;
    let path = args["path"]
        .as_str()
        .or_else(|| args["artifactPath"].as_str())
        .ok_or_else(|| error("Informe o caminho do artefato confirmado."))?;
    let target = args["destination"]
        .as_str()
        .filter(|path| !path.trim().is_empty())
        .ok_or_else(|| error("Informe o destino relativo ao projeto."))?;
    if Path::new(target).is_absolute() {
        return Err(error("O destino precisa ser relativo à pasta do projeto."));
    }
    let overwrite = match args.get("overwrite") {
        None => false,
        Some(value) => value
            .as_bool()
            .ok_or_else(|| error("overwrite precisa ser true ou false."))?,
    };
    let source = artifact_source(directory, conversation, job, path)?;
    let destination = tools::scoped(project, target, true)?;
    let bytes = copy_artifact(&source, &destination, overwrite)?;
    Ok(
        json!({"savedPath":destination,"bytes":bytes,"sourceJobId":job,
        "sourcePath":source,"overwrite":overwrite}),
    )
}

#[tauri::command]
pub async fn save_pixel_art_artifact(
    app: tauri::AppHandle,
    conversation_id: String,
    job_id: String,
    path: String,
) -> Result<bool, AgentError> {
    use tauri_plugin_dialog::DialogExt;
    let (home, _) = authorized_location(&app, &conversation_id)?;
    tauri::async_runtime::spawn_blocking(move || {
        let directory = directory(&home, &conversation_id)?;
        let source = artifact_source(&directory, &conversation_id, &job_id, &path)?;
        let name = source
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| error("O arquivo não tem um nome válido."))?;
        let Some(selected) = app
            .dialog()
            .file()
            .set_title("Salvar arquivo de pixel art")
            .set_file_name(name)
            .blocking_save_file()
        else {
            return Ok(false);
        };
        let destination = selected
            .into_path()
            .map_err(|_| error("Selecione um caminho local."))?;
        // Recheck the source after the user finishes the dialog.
        let source = artifact_source(&directory, &conversation_id, &job_id, &path)?;
        copy_artifact(&source, &destination, true)?;
        Ok(true)
    })
    .await
    .map_err(|_| AgentError::internal())?
}

fn native_catalog() -> Value {
    json!({"save_artifact":{
        "description":"Copy one confirmed artifact into this conversation's project, preserving its original bytes and source. Does not generate or reprocess.",
        "arguments":{"jobId":"source job ID","path":"exact artifact path from that job",
            "destination":"project-relative file path", "overwrite":false},
        "result":"savedPath, bytes, sourceJobId, sourcePath",
        "restrictions":"Source must belong to the conversation's confirmed job receipt. Destination must stay inside the project without symbolic links; existing files are preserved unless overwrite=true."
    }})
}
#[tauri::command]
pub async fn open_pixel_art_suite(
    app: tauri::AppHandle,
    conversation_id: String,
) -> Result<Value, AgentError> {
    let (home, project) = authorized_location(&app, &conversation_id)?;
    host(&app, &home, &project, &conversation_id).await?;
    Ok(json!({"conversationId":conversation_id,"sessionId":conversation_id}))
}
#[tauri::command]
pub async fn set_pixel_art_viewport(
    app: tauri::AppHandle,
    conversation_id: String,
    viewport: Option<Viewport>,
) -> Result<(), AgentError> {
    let (home, project) = authorized_location(&app, &conversation_id)?;
    if viewport.is_some() {
        host(&app, &home, &project, &conversation_id).await?;
    }
    let hosts = app
        .state::<State>()
        .0
        .lock()
        .await
        .values()
        .cloned()
        .collect::<Vec<_>>();
    for shared in hosts {
        let inner = shared.lock().await;
        if inner.view.label() == format!("pixel-worker-{conversation_id}") {
            if let Some(rect) = &viewport {
                if ![rect.x, rect.y, rect.width, rect.height]
                    .iter()
                    .all(|v| v.is_finite())
                    || rect.x < 0.
                    || rect.y < 0.
                    || rect.width < 20.
                    || rect.height < 20.
                {
                    return Err(error("Dimensões da suíte inválidas."));
                }
                inner
                    .view
                    .set_bounds(tauri::Rect {
                        position: tauri::LogicalPosition::new(rect.x, rect.y).into(),
                        size: tauri::LogicalSize::new(rect.width, rect.height).into(),
                    })
                    .map_err(|_| error("Não foi possível posicionar a suíte de pixel art."))?;
                inner
                    .view
                    .show()
                    .map_err(|_| error("Não foi possível mostrar a suíte de pixel art."))?;
            } else {
                inner
                    .view
                    .hide()
                    .map_err(|_| error("Não foi possível ocultar a suíte de pixel art."))?;
            }
        } else if viewport.is_some() {
            let _ = inner.view.hide();
        }
    }
    Ok(())
}
fn receipt(directory: &Path, id: &str) -> Result<PathBuf, AgentError> {
    identifier(id)?;
    Ok(directory.join(format!("job-{id}.json")))
}
fn receipt_storage_error(operation: &str, cause: std::io::Error) -> AgentError {
    crate::diagnostics::record_storage_failure("pixel_art_receipt", None);
    AgentError::new(
        "pixel_art_receipt_storage",
        &format!("Não foi possível {operation} o recibo local de pixel art: {cause}. O recibo anterior e os artefatos foram preservados. Consulte o status desta mesma operação com pixel_art_status antes de repetir uma geração ou exportação."),
    )
}
fn write_receipt(path: &Path, value: &Value) -> Result<(), AgentError> {
    use std::io::Write;
    let bytes = serde_json::to_vec(value).map_err(|_| AgentError::internal())?;
    if bytes.len() > 8 * 1024 * 1024 {
        return Err(error("O recibo de pixel art excedeu 8 MB."));
    }
    if fs::symlink_metadata(path).is_ok_and(|metadata| !metadata.is_file() || metadata.is_symlink())
    {
        return Err(error("O recibo precisa ser um arquivo regular."));
    }
    let mut staged =
        tempfile::NamedTempFile::new_in(path.parent().ok_or_else(AgentError::storage)?)
            .map_err(|cause| receipt_storage_error("preparar", cause))?;
    staged
        .write_all(&bytes)
        .map_err(|cause| receipt_storage_error("gravar", cause))?;
    staged
        .as_file()
        .sync_all()
        .map_err(|cause| receipt_storage_error("sincronizar", cause))?;
    let mut retries = 0;
    loop {
        match staged.persist(path) {
            Ok(_) => return Ok(()),
            Err(cause) => {
                // A Windows reader without FILE_SHARE_DELETE can temporarily
                // deny MoveFileExW. Retry only that same atomic replacement,
                // never the engine operation or the artifact export.
                if !cfg!(windows)
                    || !matches!(cause.error.raw_os_error(), Some(5 | 32))
                    || retries == 4
                {
                    return Err(receipt_storage_error("publicar", cause.error));
                }
                staged = cause.file;
                std::thread::sleep(Duration::from_millis(25 << retries));
                retries += 1;
            }
        }
    }
}
fn recover_receipt(path: &Path) -> Result<(Value, bool), AgentError> {
    let mut value: Value =
        serde_json::from_slice(&attachments::bounded_read(path, 8 * 1024 * 1024)?)
            .map_err(|_| error("Recibo de pixel art inválido."))?;
    if !value.is_object()
        || !matches!(
            value["status"].as_str(),
            Some("running" | "completed" | "blocked" | "failed" | "cancelled")
        )
    {
        return Err(error("Recibo de pixel art inválido."));
    }
    let interrupted = !terminal(&value);
    if interrupted {
        value["status"] = json!("blocked");
        value["phase"] = json!("Execução interrompida");
        value["revision"] = json!(value["revision"].as_u64().unwrap_or(0) + 1);
        value["error"] = json!("O Jarvis reiniciou antes da confirmação desta operação. Os arquivos foram preservados.");
        value["nextAction"] =
            json!("Inspecione os artefatos da sessão antes de autorizar uma nova geração.");
        write_receipt(path, &value)?;
    }
    Ok((value, interrupted))
}
fn recover_job_receipt(
    path: &Path,
    conversation: &str,
    id: &str,
) -> Result<(Value, bool), AgentError> {
    let (value, interrupted) = recover_receipt(path)?;
    if value["conversationId"] != conversation || value["jobId"] != id {
        return Err(error("O recibo de pixel art não pertence a esta operação."));
    }
    Ok((value, interrupted))
}
fn cache_publication(
    path: &Path,
    jobs: &mut BTreeMap<String, Value>,
    id: &str,
    mut value: Value,
) -> Result<(Value, bool), AgentError> {
    if let Some(prior) = jobs.get(id) {
        let revision = value["revision"].clone();
        value["revision"] = prior["revision"].clone();
        if &value == prior && prior["observationInterrupted"] != true {
            return Ok((value, false));
        }
        value["revision"] = revision;
    }
    if let Err(cause) = write_receipt(path, &value) {
        // Observation is already confirmed by the worker. Keep it recoverable
        // even when the local receipt cannot be replaced, without resubmitting.
        if value["observedStatus"].is_null() {
            value["observedStatus"] = value["status"].clone();
        }
        value["status"] = json!("blocked");
        value["phase"] = json!("Falha ao atualizar o estado local");
        value["error"] = json!(cause.message);
        value["observationInterrupted"] = json!(true);
        value["nextAction"] = json!("Consulte pixel_art_status desta mesma operação para recuperar a confirmação local. O resultado observado e os artefatos foram preservados; não repita a geração ou exportação.");
        jobs.insert(id.into(), value);
        return Err(cause);
    }
    jobs.insert(id.into(), value.clone());
    Ok((value, true))
}
fn publish(
    app: &tauri::AppHandle,
    host: &mut Host,
    home: &Path,
    conversation: &str,
    id: &str,
    mut value: Value,
) -> Result<Value, AgentError> {
    if !value.is_object() {
        return Err(error("Estado de pixel art inválido."));
    }
    if !matches!(
        value["status"].as_str(),
        Some("running" | "completed" | "blocked" | "failed" | "cancelled")
    ) {
        return Err(error(
            "O motor retornou um status de pixel art desconhecido.",
        ));
    }
    let prior = host.jobs.get(id);
    value["kind"] = json!("pixel_art");
    value["sessionId"] = json!(conversation);
    value["conversationId"] = json!(conversation);
    value["jobId"] = json!(id);
    for key in ["callId", "executorId"] {
        if value[key].is_null() {
            if let Some(identity) = prior.and_then(|value| value.get(key)) {
                value[key] = identity.clone();
            }
        }
    }
    value["revision"] = json!(prior.and_then(|p| p["revision"].as_u64()).unwrap_or(0) + 1);
    if value["phase"].is_null() {
        value["phase"] = json!("Processando pixel art");
    }
    if value["logs"].is_null() {
        value["logs"] = json!([]);
    }
    let mut images = Vec::new();
    let mut artifacts = Vec::new();
    let mut preview_warnings = Vec::new();
    for artifact in value["artifacts"].as_array().into_iter().flatten() {
        let path = artifact["path"]
            .as_str()
            .ok_or_else(|| error("Artefato sem caminho."))?;
        let path = tools::scoped(&host.directory, path, false)?;
        if !path.is_file() {
            return Err(error("O motor anunciou um artefato ausente."));
        }
        let name = path
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or_else(AgentError::internal)?;
        let mut artifact = artifact.clone();
        artifact["name"] = json!(name);
        artifact["path"] = json!(path);
        if matches!(
            path.extension()
                .and_then(|s| s.to_str())
                .map(str::to_lowercase)
                .as_deref(),
            Some("png" | "apng" | "gif" | "webp" | "jpg" | "jpeg")
        ) {
            let preview = (|| {
                let bytes = attachments::bounded_read(&path, 20 * 1024 * 1024)?;
                let identity = format!("{}:{:x}", path.display(), Sha256::digest(&bytes));
                if let Some(previous) = host.images.get(&identity) {
                    return Ok(previous.clone());
                }
                let image = serde_json::to_value(attachments::store_generated(
                    home,
                    conversation,
                    name,
                    &bytes,
                )?)
                .map_err(|_| AgentError::internal())?;
                host.images.insert(identity, image.clone());
                Ok::<Value, AgentError>(image)
            })();
            match preview {
                Ok(image) => {
                    artifact["mime"] = image["mime"].clone();
                    images.push(image);
                }
                Err(cause) => {
                    // Export fidelity is independent of the chat preview budget.
                    // Keep the original downloadable even when rendering is unsafe.
                    artifact["previewError"] = json!(cause.message);
                    preview_warnings.push(format!(
                        "Preview indisponível para {name}: {} O arquivo original foi preservado.",
                        cause.message
                    ));
                }
            }
        }
        artifacts.push(artifact);
    }
    value["artifacts"] = json!(artifacts);
    value["images"] = json!(images);
    if let Some(logs) = value["logs"].as_array_mut() {
        for warning in preview_warnings {
            let warning = json!(warning);
            if !logs.contains(&warning) {
                logs.push(warning);
            }
        }
    }
    match cache_publication(&receipt(&host.directory, id)?, &mut host.jobs, id, value) {
        Ok((value, changed)) => {
            if changed {
                let _ = app.emit("pixel-art-progress", &value);
            }
            Ok(value)
        }
        Err(cause) => {
            if let Some(interrupted) = host.jobs.get(id) {
                let _ = app.emit("pixel-art-progress", interrupted);
            }
            Err(cause)
        }
    }
}
fn terminal(value: &Value) -> bool {
    matches!(
        value["status"].as_str(),
        Some("completed" | "blocked" | "failed" | "cancelled")
    )
}
fn manual_id(engine_id: &str) -> String {
    // Worker IDs are untrusted labels, never filesystem components.
    format!("{:x}", Sha256::digest(engine_id.as_bytes()))[..32].to_owned()
}
fn monitor_activity(
    app: tauri::AppHandle,
    host: std::sync::Weak<AsyncMutex<Host>>,
    home: PathBuf,
    conversation: String,
) {
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(1)).await;
            let Some(host) = host.upgrade() else {
                break;
            };
            let mut host = host.lock().await;
            if !matches!(host.child.try_wait(), Ok(None)) {
                let pending = host
                    .jobs
                    .iter()
                    .filter(|(_, value)| !terminal(value))
                    .map(|(id, value)| (id.clone(), value.clone()))
                    .collect::<Vec<_>>();
                for (id, mut value) in pending {
                    value["status"] = json!("blocked");
                    value["phase"] = json!("Motor de pixel art interrompido");
                    value["error"] =
                        json!("O processo local do motor encerrou antes de confirmar o resultado.");
                    value["nextAction"] = json!("Revise os logs e os artefatos preservados antes de retomar. A geração não foi repetida.");
                    let _ = publish(&app, &mut host, &home, &conversation, &id, value);
                }
                break;
            }
            let Ok(activities) =
                evaluate(&host.view, "window.__jarvisPixel.activity()".into()).await
            else {
                continue;
            };
            for activity in activities.as_array().into_iter().flatten() {
                let Some(engine_id) = activity["id"].as_str() else {
                    continue;
                };
                if host.manual.get(engine_id) == Some(activity) {
                    continue;
                }
                let id = manual_id(engine_id);
                let mut value = activity.clone();
                value["manual"] = json!(true);
                if publish(&app, &mut host, &home, &conversation, &id, value).is_ok() {
                    host.manual.insert(engine_id.into(), activity.clone());
                }
            }
        }
    });
}
async fn refresh(
    app: &tauri::AppHandle,
    host: &SharedHost,
    home: &Path,
    conversation: &str,
    id: &str,
) -> Result<Value, AgentError> {
    let mut host = host.lock().await;
    if !matches!(host.child.try_wait(), Ok(None)) {
        return Err(error(
            "O processo local do motor de pixel art encerrou antes de confirmar o resultado.",
        ));
    }
    let value = evaluate(
        &host.view,
        format!("window.__jarvisPixel.snapshot({})", json!(id)),
    )
    .await?;
    publish(app, &mut host, home, conversation, id, value)
}
async fn await_cancel_confirmation<F, Fut>(
    mut observe: F,
    limit: Duration,
) -> Result<Value, AgentError>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<Value, AgentError>>,
{
    let deadline = tokio::time::Instant::now() + limit;
    loop {
        let value = tokio::time::timeout_at(deadline, observe())
            .await
            .map_err(|_| error("O motor não confirmou o cancelamento no prazo. A execução pode continuar; consulte seu estado antes de iniciar outra geração."))??;
        if terminal(&value) {
            return Ok(value);
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(error("O motor ainda não confirmou o cancelamento. A execução pode continuar; consulte seu estado antes de iniciar outra geração."));
        }
        tokio::time::sleep_until(
            (tokio::time::Instant::now() + Duration::from_millis(250)).min(deadline),
        )
        .await;
    }
}
async fn stop_operation(
    app: &tauri::AppHandle,
    host: &SharedHost,
    home: &Path,
    conversation: &str,
    id: &str,
) -> Result<(), AgentError> {
    {
        let mut inner = host.lock().await;
        let mut value = inner
            .jobs
            .get(id)
            .cloned()
            .ok_or_else(AgentError::internal)?;
        if terminal(&value) {
            return Ok(());
        }
        let engine_id = value["id"].as_str().unwrap_or(id);
        evaluate(
            &inner.view,
            format!("window.__jarvisPixel.cancel({})", json!(engine_id)),
        )
        .await?;
        value["phase"] = json!("Cancelamento solicitado; aguardando o motor");
        publish(app, &mut inner, home, conversation, id, value)?;
    }
    // The local server must stay alive long enough to cancel its original job
    // and persist that acknowledgment. Killing it here can leave a paid child
    // running and cause an unsafe submission when its journal is reopened.
    await_cancel_confirmation(
        || refresh(app, host, home, conversation, id),
        Duration::from_secs(100),
    )
    .await?;
    Ok(())
}
fn monitor(
    app: tauri::AppHandle,
    host: SharedHost,
    home: PathBuf,
    conversation: String,
    id: String,
    mut signal: watch::Receiver<bool>,
) {
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::select! {
                _ = super::cancelled(&mut signal) => {
                    if let Err(cause) = stop_operation(&app, &host, &home, &conversation, &id).await {
                        let mut inner = host.lock().await;
                        let mut value = inner.jobs.get(&id).cloned().unwrap_or_else(|| json!({}));
                        value["status"] = json!("blocked");
                        value["phase"] = json!("Cancelamento não confirmado");
                        value["error"] = json!(cause.message);
                        value["observationInterrupted"] = json!(true);
                        value["nextAction"] = json!("Consulte o estado desta operação e os logs preservados. O motor foi mantido ativo para concluir o cancelamento; nenhuma nova geração foi iniciada.");
                        let _ = publish(&app, &mut inner, &home, &conversation, &id, value);
                    }
                    break;
                }
                _ = tokio::time::sleep(Duration::from_millis(750)) => {}
            }
            match refresh(&app, &host, &home, &conversation, &id).await {
                Ok(value) if terminal(&value) => break,
                Ok(_) => {}
                Err(cause) => {
                    let mut host = host.lock().await;
                    let mut value = host.jobs.get(&id).cloned().unwrap_or_else(|| json!({}));
                    value["status"] = json!("blocked");
                    value["phase"] = json!("Acompanhamento interrompido");
                    value["error"] = json!(cause.message);
                    value["observationInterrupted"] = json!(true);
                    value["nextAction"] = json!("Consulte os logs e os artefatos preservados antes de tentar novamente. A geração não foi repetida.");
                    let _ = publish(&app, &mut host, &home, &conversation, &id, value);
                    break;
                }
            }
        }
    });
}
pub(super) async fn running(app: &tauri::AppHandle, conversation: &str) -> Vec<String> {
    let state = app.state::<State>();
    let host = state.0.lock().await.get(conversation).cloned();
    match host {
        Some(host) => host
            .lock()
            .await
            .jobs
            .iter()
            .filter(|(_, value)| !terminal(value))
            .map(|(id, _)| id.clone())
            .collect(),
        None => vec![],
    }
}
pub(super) async fn execute(
    app: Option<&tauri::AppHandle>,
    home: &Path,
    project: &Path,
    conversation: &str,
    executor_id: &str,
    tool: &ToolCall,
    signal: watch::Receiver<bool>,
) -> Result<String, AgentError> {
    let app = app.ok_or_else(|| {
        error("O agente de pixel art precisa do Jarvis desktop para processar as imagens.")
    })?;
    if tool.name == "pixel_art_run" {
        let id = operation_id(conversation, executor_id, &tool.id);
        let path = receipt(&directory(home, conversation)?, &id)?;
        if path.exists() {
            let shared = app
                .state::<State>()
                .0
                .lock()
                .await
                .get(conversation)
                .cloned();
            if let Some(shared) = shared {
                if let Some(existing) = shared.lock().await.jobs.get(&id) {
                    return Ok(existing.to_string());
                }
            }
            let (existing, _) = recover_receipt(&path)?;
            return Ok(existing.to_string());
        }
    }
    if tool.name == "pixel_art_run" && tool.args["action"] == "save_artifact" {
        let folder = directory(home, conversation)?;
        let project = project.to_path_buf();
        let conversation = conversation.to_owned();
        let args = tool.args["arguments"].clone();
        let call = tool.id.clone();
        let executor = executor_id.to_owned();
        let result = tauri::async_runtime::spawn_blocking(move || {
            let saved = save_artifact_to_project(&folder, &project, &conversation, &args)?;
            let id = operation_id(&conversation, &executor, &call);
            let value = json!({"kind":"pixel_art","conversationId":conversation,
                "sessionId":conversation,"jobId":id,"callId":call,"executorId":executor,"action":"save_artifact","status":"completed",
                "phase":"Arquivo salvo no projeto","revision":1,"images":[],
                "logs":[format!("Arquivo salvo: {} ({} bytes)",saved["savedPath"].as_str().unwrap_or(""),saved["bytes"])],
                "artifacts":[{"path":saved["sourcePath"],"name":Path::new(saved["sourcePath"].as_str().unwrap_or("")).file_name().unwrap_or_default().to_string_lossy(),"mime":"application/octet-stream"}],
                "result":saved});
            write_receipt(&receipt(&folder, &id)?, &value)?;
            Ok::<Value, AgentError>(value)
        }).await.map_err(|_| AgentError::internal())??;
        let _ = app.emit("pixel-art-progress", &result);
        return Ok(result.to_string());
    }
    if tool.name == "pixel_art_docs"
        && (tool.args["section"] == "native" || tool.args["method"] == "save_artifact")
    {
        return Ok(native_catalog().to_string());
    }
    if tool.name == "pixel_art_status" {
        let id = match (tool.args["jobId"].as_str(), tool.args["callId"].as_str()) {
            (Some(id), None) => id.to_owned(),
            (None, Some(call)) if !call.is_empty() && call.len() <= 512 => {
                operation_id(conversation, executor_id, call)
            }
            _ => {
                return Err(error(
                    "Informe jobId ou callId da operação, somente um deles.",
                ))
            }
        };
        let path = receipt(&directory(home, conversation)?, &id)?;
        let state = app.state::<State>();
        let active = state.0.lock().await.get(conversation).cloned();
        if let Some(host) = active {
            let wait = tool.args["yieldTimeMs"].as_u64().unwrap_or(1000).min(30000);
            tokio::time::sleep(Duration::from_millis(wait)).await;
            let mut cached = host.lock().await.jobs.get(&id).cloned();
            if cached.is_none() && path.exists() {
                let (value, _) = recover_job_receipt(&path, conversation, &id)?;
                // Reattach durable identity and revision before observing a restored
                // worker operation. A recovered snapshot is never a new submission.
                host.lock().await.jobs.insert(id.clone(), value.clone());
                cached = Some(value);
            }
            if let Some(value) = cached {
                if value["observationInterrupted"] == true
                    || matches!(value["status"].as_str(), Some("blocked" | "failed"))
                {
                    // Reconnect to the same operation; never submit it again.
                    if let Ok(current) = refresh(app, &host, home, conversation, &id).await {
                        if !terminal(&current) {
                            monitor(
                                app.clone(),
                                host.clone(),
                                home.into(),
                                conversation.into(),
                                id.clone(),
                                signal,
                            );
                        }
                        return Ok(current.to_string());
                    }
                }
                return Ok(value.to_string());
            }
            // If the start acknowledgment was lost, its stable call-derived ID
            // can inspect the original worker without creating another operation.
            if tool.args["callId"].is_string() {
                if let Ok(mut current) = refresh(app, &host, home, conversation, &id).await {
                    current["callId"] = tool.args["callId"].clone();
                    current["executorId"] = json!(executor_id);
                    let current = publish(
                        app,
                        &mut *host.lock().await,
                        home,
                        conversation,
                        &id,
                        current,
                    )?;
                    if !terminal(&current) {
                        monitor(
                            app.clone(),
                            host.clone(),
                            home.into(),
                            conversation.into(),
                            id.clone(),
                            signal,
                        );
                    }
                    return Ok(current.to_string());
                }
            }
        }
        let (value, interrupted) = recover_job_receipt(&path, conversation, &id)?;
        if interrupted {
            let _ = app.emit("pixel-art-progress", &value);
        }
        return Ok(value.to_string());
    }
    let host = host(app, home, project, conversation).await?;
    if tool.name == "pixel_art_docs" {
        let mut catalog = evaluate(
            &host.lock().await.view,
            format!("window.__jarvisPixel.catalog({})", tool.args),
        )
        .await?;
        if let Some(object) = catalog.as_object_mut() {
            object.insert("nativeActions".into(), native_catalog());
        } else {
            return Err(error("O catálogo do motor de pixel art é inválido."));
        }
        return Ok(catalog.to_string());
    }
    if tool.name == "pixel_art_cancel" {
        let id = tool.args["jobId"]
            .as_str()
            .ok_or_else(|| error("Informe o jobId."))?;
        identifier(id)?;
        let mut inner = host.lock().await;
        if !inner.jobs.contains_key(id) {
            return Err(error("Esta operação não pertence à sessão atual."));
        }
        if terminal(&inner.jobs[id]) {
            return Ok(inner.jobs[id].to_string());
        }
        let engine_id = inner.jobs[id]["id"].as_str().unwrap_or(id);
        evaluate(
            &inner.view,
            format!("window.__jarvisPixel.cancel({})", json!(engine_id)),
        )
        .await?;
        let mut value = inner.jobs[id].clone();
        value["phase"] = json!("Cancelamento solicitado; aguardando o motor");
        return publish(app, &mut inner, home, conversation, id, value).map(|v| v.to_string());
    }
    let action = tool.args["action"]
        .as_str()
        .filter(|a| ACTIONS.contains(a))
        .ok_or_else(|| error("Ação de pixel art inválida. Consulte pixel_art_docs."))?;
    let mut args = tool.args["arguments"].clone();
    if !args.is_object() {
        return Err(error("Argumentos de pixel art inválidos."));
    }
    if matches!(action, "import_image" | "import_animation_pack") {
        if let Some(id) = args["attachmentId"].as_str() {
            let item = attachments::metadata(home, conversation, id)?;
            if action == "import_image" && item.kind != "image" {
                return Err(error("O anexo selecionado não é uma imagem."));
            }
            let bytes = attachments::bounded_read(
                &attachments::location(home, conversation, id)?.join("source"),
                20 * 1024 * 1024,
            )?;
            let inner = host.lock().await;
            let path = tools::scoped(
                &inner.directory,
                &format!("references/{}-{}", item.id, item.name),
                true,
            )?;
            fs::write(&path, bytes).map_err(|_| AgentError::storage())?;
            args.as_object_mut().unwrap().remove("attachmentId");
            args["path"] = json!(path);
            args["name"] = json!(item.name);
        } else if let Some(path) = args["path"].as_str() {
            args["path"] = json!(tools::scoped(project, path, false)?);
        }
    }
    let id = operation_id(conversation, executor_id, &tool.id);
    {
        let mut inner = host.lock().await;
        if inner.jobs.values().any(|v| !terminal(v)) {
            return Err(error("Já existe uma operação de pixel art em andamento nesta conversa. Use pixel_art_status ou pixel_art_cancel."));
        }
        // A lost observation must not cause another paid generation while the
        // original worker is still active.
        let active = evaluate(
            &inner.view,
            "(() => { const id = window.__jarvisPixel.snapshot().activeId; return id ? window.__jarvisPixel.snapshot(id) : null; })()".into(),
        ).await?;
        if active.is_object() && !terminal(&active) {
            return Err(error(&format!(
                "O motor ainda executa a operação {}. Consulte o estado ou cancele essa operação antes de iniciar outra.",
                active["id"].as_str().unwrap_or("anterior"),
            )));
        }
        let initial = publish(
            app,
            &mut inner,
            home,
            conversation,
            &id,
            json!({"callId":tool.id,"executorId":executor_id,"action":action,"status":"running","phase":"Preparando pixel art","logs":[],"artifacts":[]}),
        )?;
        if let Err(cause) = evaluate(
            &inner.view,
            format!(
                "window.__jarvisPixel.start({})",
                json!({"id":id,"action":action,"args":args})
            ),
        )
        .await
        {
            // Persist intent before dispatch so a lost acknowledgment can be
            // reconciled with this exact operation instead of generating again.
            let mut interrupted = initial;
            interrupted["status"] = json!("blocked");
            interrupted["phase"] = json!("Confirmação de início interrompida");
            interrupted["error"] = json!(cause.message);
            interrupted["observationInterrupted"] = json!(true);
            interrupted["nextAction"] = json!("Consulte pixel_art_status com este jobId ou callId. O resultado ainda é incerto; não inicie outra geração.");
            return publish(app, &mut inner, home, conversation, &id, interrupted)
                .map(|value| value.to_string());
        }
    }
    monitor(
        app.clone(),
        host.clone(),
        home.into(),
        conversation.into(),
        id.clone(),
        signal,
    );
    let wait = tool.args["yieldTimeMs"].as_u64().unwrap_or(1000).min(30000);
    tokio::time::sleep(Duration::from_millis(wait)).await;
    let result = host.lock().await.jobs[&id].to_string();
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(windows)]
    #[test]
    fn pixel_art_receipt_failed_publication_keeps_the_observed_result_recoverable() {
        use std::os::windows::fs::OpenOptionsExt;

        let directory = tempfile::tempdir().unwrap();
        let id = "c".repeat(32);
        let path = receipt(directory.path(), &id).unwrap();
        let artifact = directory.path().join("confirmed.json");
        fs::write(&artifact, b"{\"confirmed\":true}").unwrap();
        let original = json!({"jobId":id,"callId":"same-call","status":"running","revision":1});
        let completed = json!({
            "jobId":id,"callId":"same-call","status":"completed","revision":2,
            "artifacts":[{"path":artifact}],"result":{"confirmed":true}
        });
        let mut jobs = BTreeMap::new();
        cache_publication(&path, &mut jobs, &id, original.clone()).unwrap();
        let reader = fs::OpenOptions::new()
            .read(true)
            .share_mode(1 | 2)
            .open(&path)
            .unwrap();
        let cause = cache_publication(&path, &mut jobs, &id, completed.clone()).unwrap_err();
        assert_eq!(cause.code, "pixel_art_receipt_storage");
        let interrupted = &jobs[&id];
        assert_eq!(interrupted["status"], "blocked");
        assert_eq!(interrupted["observationInterrupted"], true);
        assert_eq!(interrupted["observedStatus"], "completed");
        assert_eq!(interrupted["artifacts"], completed["artifacts"]);
        assert_eq!(interrupted["result"], completed["result"]);
        assert!(interrupted["phase"]
            .as_str()
            .unwrap()
            .contains("estado local"));
        assert_eq!(
            serde_json::from_slice::<Value>(&fs::read(&path).unwrap()).unwrap(),
            original
        );
        // The monitor also attempts to publish its blocked result. A second
        // locked write must not erase the same operation's observed completion.
        let interrupted = jobs[&id].clone();
        assert!(cache_publication(&path, &mut jobs, &id, interrupted).is_err());
        assert_eq!(jobs[&id]["observationInterrupted"], true);
        assert_eq!(jobs[&id]["observedStatus"], "completed");
        assert_eq!(jobs[&id]["artifacts"], completed["artifacts"]);
        drop(reader);
        // Status observes the same worker snapshot; no export is submitted again.
        let pending = jobs[&id].clone();
        let (_, changed) = cache_publication(&path, &mut jobs, &id, pending.clone()).unwrap();
        assert!(
            changed,
            "An interrupted publication must retry its identical pending receipt"
        );
        assert_eq!(
            serde_json::from_slice::<Value>(&fs::read(&path).unwrap()).unwrap(),
            pending
        );
        let (recovered, changed) =
            cache_publication(&path, &mut jobs, &id, completed.clone()).unwrap();
        assert!(changed);
        assert_eq!(recovered, completed);
        assert_eq!(jobs[&id], completed);
        assert_eq!(
            serde_json::from_slice::<Value>(&fs::read(path).unwrap()).unwrap(),
            completed
        );
        assert_eq!(fs::read(artifact).unwrap(), b"{\"confirmed\":true}");
    }
    #[cfg(windows)]
    #[test]
    fn pixel_art_receipt_publication_recovers_after_a_temporary_reader_releases_the_target() {
        use std::os::windows::fs::OpenOptionsExt;

        let directory = tempfile::tempdir().unwrap();
        let path = receipt(directory.path(), &"a".repeat(32)).unwrap();
        let original = json!({"status":"running","revision":1});
        let completed =
            json!({"status":"completed","revision":2,"artifacts":[{"path":"confirmed.json"}]});
        write_receipt(&path, &original).unwrap();
        let reader = fs::OpenOptions::new()
            .read(true)
            .share_mode(1 | 2)
            .open(&path)
            .unwrap();
        std::thread::scope(|scope| {
            scope.spawn(move || {
                std::thread::sleep(Duration::from_millis(120));
                drop(reader);
            });
            let result = write_receipt(&path, &completed);
            assert!(
                result.is_ok(),
                "The confirmed receipt must survive a temporary reader: {result:?}"
            );
        });
        assert_eq!(
            serde_json::from_slice::<Value>(&fs::read(path).unwrap()).unwrap(),
            completed
        );
    }
    #[cfg(windows)]
    #[test]
    fn pixel_art_receipt_publication_preserves_previous_receipt_and_reports_a_persistent_lock() {
        use std::os::windows::fs::OpenOptionsExt;

        let directory = tempfile::tempdir().unwrap();
        let path = receipt(directory.path(), &"b".repeat(32)).unwrap();
        let original = json!({"status":"running","revision":1});
        write_receipt(&path, &original).unwrap();
        let _reader = fs::OpenOptions::new()
            .read(true)
            .share_mode(1 | 2)
            .open(&path)
            .unwrap();
        let start = std::time::Instant::now();
        let cause = write_receipt(&path, &json!({"status":"completed","revision":2})).unwrap_err();
        assert!(start.elapsed() < Duration::from_secs(2));
        assert_eq!(cause.code, "pixel_art_receipt_storage");
        assert!(cause.message.contains("publicar"));
        assert!(cause.message.contains("os error 5") || cause.message.contains("os error 32"));
        assert!(cause.message.contains("pixel_art_status"));
        assert_eq!(
            serde_json::from_slice::<Value>(&fs::read(path).unwrap()).unwrap(),
            original
        );
    }
    #[tokio::test]
    async fn pixel_art_stop_waits_for_original_acknowledgment_and_preserves_uncertainty() {
        let mut observations = vec![
            json!({"status":"cancelled","artifacts":[{"path":"accepted.png"}]}),
            json!({"status":"running","phase":"Cancelling original job"}),
        ];
        let result = await_cancel_confirmation(
            || std::future::ready(Ok(observations.pop().unwrap())),
            Duration::from_secs(2),
        )
        .await
        .unwrap();
        assert!(observations.is_empty());
        assert_eq!(result["status"], "cancelled");
        assert_eq!(result["artifacts"][0]["path"], "accepted.png");
        let finished = json!({"status":"completed","artifacts":[{"path":"confirmed.gif"}]});
        assert_eq!(
            await_cancel_confirmation(
                || std::future::ready(Ok(finished.clone())),
                Duration::from_secs(1),
            )
            .await
            .unwrap(),
            finished,
        );
        let unresolved = await_cancel_confirmation(
            || std::future::ready(Ok(json!({"status":"running"}))),
            Duration::from_millis(5),
        )
        .await
        .unwrap_err();
        assert!(unresolved.message.contains("não confirmou"));
        assert!(unresolved.message.contains("pode continuar"));
    }
    #[test]
    fn pixel_art_restored_status_preserves_the_exact_call_and_revision() {
        let directory = tempfile::tempdir().unwrap();
        let conversation = "c".repeat(32);
        let id = operation_id(&conversation, "main", "call-original");
        let path = receipt(directory.path(), &id).unwrap();
        let original = json!({
            "kind":"pixel_art","conversationId":conversation,"jobId":id,
            "callId":"call-original","executorId":"main","status":"running",
            "revision":24,"artifacts":[{"path":"confirmed-partial.png"}]
        });
        write_receipt(&path, &original).unwrap();
        let (restored, interrupted) = recover_job_receipt(&path, &conversation, &id).unwrap();
        assert!(interrupted);
        assert_eq!(restored["status"], "blocked");
        assert_eq!(restored["callId"], original["callId"]);
        assert_eq!(restored["executorId"], original["executorId"]);
        assert_eq!(restored["artifacts"], original["artifacts"]);
        assert_eq!(restored["revision"], 25);
        assert!(recover_job_receipt(&path, &"d".repeat(32), &id).is_err());
        assert!(recover_job_receipt(&path, &conversation, &"a".repeat(32)).is_err());
    }
    #[tokio::test]
    async fn pixel_art_waits_for_cef_context_without_replaying_an_operation() {
        let mut attempts = 0;
        await_worker_ready(
            || {
                attempts += 1;
                let result = match attempts {
                    1 => Err(error("O motor retornou uma resposta inválida.")),
                    2 => Ok(false),
                    _ => Ok(true),
                };
                std::future::ready(result)
            },
            Duration::from_secs(2),
        )
        .await
        .unwrap();
        assert_eq!(attempts, 3);
        let result = await_worker_ready(
            || std::future::ready(Err(error("Contexto JavaScript ausente"))),
            Duration::from_millis(5),
        )
        .await
        .unwrap_err();
        assert!(result.message.contains("Contexto JavaScript ausente"));
        assert!(result
            .message
            .contains("arquivos existentes foram preservados"));
    }
    #[cfg(windows)]
    #[test]
    fn pixel_art_windows_launch_supports_core_node_and_preserves_path_identity() {
        let root = tempfile::tempdir().unwrap();
        let canonical = root.path().canonicalize().unwrap();
        assert!(canonical.to_string_lossy().starts_with(r"\\?\"));
        let command = engine_command(
            &canonical.join("runtime/node.exe"),
            &canonical.join("resources"),
            &canonical.join("session"),
            &canonical.join("project"),
            "private-token",
        );
        let command = command.as_std();
        let paths = std::iter::once(command.get_program())
            .chain(command.get_args())
            .chain(command.get_current_dir().map(Path::as_os_str))
            .chain(command.get_envs().filter_map(|(key, value)| {
                matches!(
                    key.to_str(),
                    Some(
                        "IMAGE_COCKPIT_HANDOFF_DIR"
                            | "JARVIS_PIXEL_RESOURCE_DIR"
                            | "JARVIS_PIXEL_WORKSPACE"
                    )
                )
                .then_some(value)
                .flatten()
            }));
        for path in paths {
            assert!(!path.to_string_lossy().starts_with(r"\\?\"));
            assert!(Path::new(path).starts_with(root.path()));
        }
        // Conversion is restricted to the launch boundary, not the stored scope.
        assert!(canonical.to_string_lossy().starts_with(r"\\?\"));
        let network = engine_command(
            Path::new(r"\\?\UNC\server\share\node.exe"),
            Path::new(r"\\?\UNC\server\share\resources"),
            Path::new(r"\\?\UNC\server\share\session"),
            Path::new(r"\\?\UNC\server\share\project"),
            "private-token",
        );
        assert_eq!(network.as_std().get_program(), r"\\server\share\node.exe");
        assert_eq!(
            network.as_std().get_current_dir(),
            Some(Path::new(r"\\server\share\session"))
        );
    }
    #[test]
    fn pixel_art_delivery_preserves_bytes_originals_and_existing_destinations() {
        let directory = tempfile::tempdir().unwrap();
        let project = tempfile::tempdir().unwrap();
        let session = directory.path().canonicalize().unwrap();
        let project = project.path().canonicalize().unwrap();
        let conversation = "c".repeat(32);
        let job = "a".repeat(32);
        let source = session.join("animation.zip");
        let bytes = b"PK\x03\x04\x00\xff original export";
        fs::write(&source, bytes).unwrap();
        fs::write(
            receipt(&session, &job).unwrap(),
            json!({
                "conversationId":conversation,"jobId":job,"status":"completed",
                "artifacts":[{"path":source,"name":"animation.zip","mime":"application/zip"}]
            })
            .to_string(),
        )
        .unwrap();
        let mut args = json!({"jobId":job,"path":source,"destination":"sprites/animation.zip"});
        let result = save_artifact_to_project(&session, &project, &conversation, &args).unwrap();
        let destination = project.join("sprites/animation.zip");
        assert_eq!(result["savedPath"], json!(destination));
        assert_eq!(fs::read(&destination).unwrap(), bytes);
        assert_eq!(fs::read(&source).unwrap(), bytes);
        fs::write(&destination, b"accepted version").unwrap();
        assert!(save_artifact_to_project(&session, &project, &conversation, &args).is_err());
        assert_eq!(fs::read(&destination).unwrap(), b"accepted version");
        args["overwrite"] = json!(true);
        save_artifact_to_project(&session, &project, &conversation, &args).unwrap();
        assert_eq!(fs::read(&destination).unwrap(), bytes);
        args["destination"] = json!("../outside.zip");
        assert!(save_artifact_to_project(&session, &project, &conversation, &args).is_err());
        args["destination"] = json!(source);
        assert!(save_artifact_to_project(&session, &project, &conversation, &args).is_err());
        args["destination"] = json!("sprites/unlisted.zip");
        let unlisted = session.join("private.json");
        fs::write(&unlisted, b"not a published artifact").unwrap();
        args["path"] = json!(unlisted);
        assert!(save_artifact_to_project(&session, &project, &conversation, &args).is_err());
        args["path"] = json!(source);
        assert!(save_artifact_to_project(&session, &project, &"d".repeat(32), &args).is_err());
        assert!(!project.join("sprites/unlisted.zip").exists());
    }
    #[test]
    fn pixel_art_lost_acknowledgment_keeps_a_stable_conversation_scoped_job_id() {
        let first = operation_id("conversation-a", "main", "call:provider/unsafe-id");
        assert!(identifier(&first).is_ok());
        assert_eq!(
            first,
            operation_id("conversation-a", "main", "call:provider/unsafe-id")
        );
        assert_ne!(
            first,
            operation_id("conversation-b", "main", "call:provider/unsafe-id")
        );
        assert_ne!(
            first,
            operation_id("conversation-a", "main", "another-call")
        );
        assert_ne!(
            first,
            operation_id("conversation-a", "child", "call:provider/unsafe-id")
        );
    }
    #[test]
    fn pixel_art_interrupted_work_keeps_artifacts_and_never_becomes_completed() {
        let directory = tempfile::tempdir().unwrap();
        let path = receipt(directory.path(), &"a".repeat(32)).unwrap();
        let original = json!({
            "status":"running", "revision":3, "jobId":"a".repeat(32),
            "artifacts":[{"name":"accepted.gif","path":"preserved/accepted.gif"}],
            "logs":["Confirmed frame generation"]
        });
        fs::write(&path, original.to_string()).unwrap();
        let (recovered, changed) = recover_receipt(&path).unwrap();
        assert!(changed);
        assert_eq!(recovered["status"], "blocked");
        assert_eq!(recovered["revision"], 4);
        assert_eq!(recovered["artifacts"], original["artifacts"]);
        assert_eq!(recovered["logs"], original["logs"]);
        assert!(recovered["nextAction"]
            .as_str()
            .unwrap()
            .contains("Inspecione"));
        let (again, changed) = recover_receipt(&path).unwrap();
        assert!(!changed);
        assert_eq!(again, recovered);
        assert!(receipt(directory.path(), "../outside").is_err());
        fs::write(&path, "[]").unwrap();
        assert!(recover_receipt(&path).is_err());
    }
    #[test]
    fn pixel_art_receipts_preserve_large_original_results_during_recovery() {
        let directory = tempfile::tempdir().unwrap();
        let path = receipt(directory.path(), &"b".repeat(32)).unwrap();
        let original = json!({
            "status":"running","revision":1,
            "result":{"report":"x".repeat(2 * 1024 * 1024)},
            "logs":["Original result preserved"]
        });
        write_receipt(&path, &original).unwrap();
        let (recovered, changed) = recover_receipt(&path).unwrap();
        assert!(changed);
        assert_eq!(recovered["result"], original["result"]);
        assert_eq!(recovered["logs"], original["logs"]);
        assert_eq!(recovered["status"], "blocked");
    }
    #[test]
    fn pixel_art_presentation_does_not_inline_engine_state_or_erase_original_logs() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("receipt.json");
        let original = json!({"jobId":"a".repeat(32),"callId":"original-provider-call","status":"completed","manual":true,
            "logs":(0..250).map(|index| format!("line {index}")).collect::<Vec<_>>(),
            "result":{"privateLargeState":"not a chat card"}});
        fs::write(&path, original.to_string()).unwrap();
        let presentation = presentation_receipt(original.clone(), &path);
        assert!(presentation.get("result").is_none());
        assert_eq!(presentation["callId"], "original-provider-call");
        assert_eq!(presentation["manual"], true);
        assert_eq!(presentation["logs"].as_array().unwrap().len(), 200);
        assert_eq!(presentation["logs"][0], "line 50");
        assert_eq!(presentation["logPath"], json!(path));
        assert_eq!(
            serde_json::from_slice::<Value>(&fs::read(path).unwrap()).unwrap(),
            original
        );
    }
    #[test]
    fn pixel_tools_are_scoped_and_mutation_requires_build_mode() {
        let names = |mode| {
            definitions(mode)
                .into_iter()
                .map(|t| t["name"].as_str().unwrap().to_owned())
                .collect::<Vec<_>>()
        };
        assert_eq!(names(Mode::Plan), ["pixel_art_docs", "pixel_art_status"]);
        assert_eq!(names(Mode::Build).len(), 4);
        assert!(tools::needs_approval("pixel_art_run"));
        assert!(!tools::needs_approval("pixel_art_status"));
        assert!(identifier("../other").is_err());
        assert!(identifier(&"a".repeat(32)).is_ok());
        assert!(identifier(&manual_id("manual-untrusted/../../path")).is_ok());
        for action in [
            "import_animation_pack",
            "select",
            "timeline_open",
            "review_open",
            "vfx_open",
        ] {
            assert!(ACTIONS.contains(&action), "{action}");
        }
        for status in ["completed", "blocked", "failed", "cancelled"] {
            assert!(terminal(&json!({"status":status})));
        }
        assert!(!terminal(&json!({"status":"running"})));
        let mut catalog = definitions(Mode::Build);
        catalog.push(json!({"name":"read"}));
        restrict_catalog(&mut catalog, false);
        assert_eq!(catalog, [json!({"name":"read"})]);
        let mut catalog = definitions(Mode::Build);
        restrict_catalog(&mut catalog, true);
        assert_eq!(catalog.len(), 4);
    }
    #[test]
    fn pixel_agent_is_selectable_and_exclusively_uses_its_engine() {
        use super::super::workflow::{Flow, Role};
        let agent = super::super::workflow::catalog::builtin_agents()
            .into_iter()
            .find(|a| a.id == "builtin:pixel_art")
            .unwrap();
        assert_eq!(
            agent.usage,
            super::super::workflow::catalog::AgentUsage::Mixed
        );
        for name in [
            "pixel_art_docs",
            "pixel_art_run",
            "pixel_art_status",
            "pixel_art_cancel",
            "ctx_search",
            "read_attachment",
        ] {
            assert!(Role::PixelArt.allows(Flow::Custom, name, true), "{name}");
        }
        for name in [
            "generate_image",
            "image_process",
            "bash",
            "video_run",
            "mcp_arbitrary",
        ] {
            assert!(!Role::PixelArt.allows(Flow::Custom, name, true), "{name}");
        }
        assert!(!Role::Builder.allows(Flow::Standard, "pixel_art_run", true));
    }
}
