mod model;
mod store;
#[cfg(test)]
mod tests;
mod transport;
pub(crate) use model::*;

use crate::persistence::{AppState, PersistenceError};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Mutex,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tauri::{Emitter, Manager};
use tokio::sync::watch;

#[derive(Debug, Clone, Serialize)]
pub(crate) struct HttpError {
    pub code: String,
    pub message: String,
}
fn error(code: &str, message: &str) -> HttpError {
    HttpError {
        code: code.into(),
        message: message.into(),
    }
}
fn invalid(message: &str) -> HttpError {
    error("http_invalid", message)
}
fn storage() -> HttpError {
    error(
        "http_storage",
        "Não foi possível acessar os dados do cliente HTTP.",
    )
}
fn conflict() -> HttpError {
    error(
        "http_revision_conflict",
        "A requisição ou configuração mudou. Recarregue a versão atual antes de salvar ou enviar.",
    )
}
impl From<rusqlite::Error> for HttpError {
    fn from(_: rusqlite::Error) -> Self {
        storage()
    }
}
impl From<PersistenceError> for HttpError {
    fn from(_: PersistenceError) -> Self {
        storage()
    }
}
impl From<std::io::Error> for HttpError {
    fn from(_: std::io::Error) -> Self {
        storage()
    }
}

struct Active {
    conversation: String,
    project: String,
    cancel: watch::Sender<bool>,
    done: watch::Sender<bool>,
}
#[derive(Default)]
pub(crate) struct HttpState {
    running: Mutex<HashMap<String, Active>>,
    recovered: Mutex<bool>,
    clients: Mutex<HashMap<String, reqwest::Client>>,
    terminal: Mutex<HashMap<String, Run>>,
    // Only preparation and cleanup share this lock; requests run independently.
    preparation: Mutex<()>,
}
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
fn home(app: &tauri::AppHandle) -> Result<PathBuf, HttpError> {
    app.path().home_dir().map_err(|_| storage())
}
fn directory(home: &Path, project: &str) -> Result<PathBuf, HttpError> {
    store::valid_id(project)?;
    Ok(crate::data_dir::root(home).join("http").join(project))
}
fn run_dir(home: &Path, project: &str, run: &str) -> Result<PathBuf, HttpError> {
    store::valid_id(run)?;
    Ok(directory(home, project)?.join("runs").join(run))
}
fn export_target(home: &Path, path: &str) -> Result<PathBuf, HttpError> {
    let path = Path::new(path);
    let name = path
        .file_name()
        .ok_or_else(|| invalid("Destino inválido."))?;
    let parent = std::fs::canonicalize(
        path.parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new(".")),
    )?;
    let target = parent.join(name);
    let root = std::fs::canonicalize(crate::data_dir::root(home))
        .unwrap_or_else(|_| crate::data_dir::root(home));
    if target.starts_with(root) {
        return Err(invalid(
            "Escolha uma pasta fora dos dados internos do Jarvis.",
        ));
    }
    Ok(target)
}
fn changed(app: &tauri::AppHandle, conversation: &str, run: Option<&str>) {
    let _ = app.emit(
        "http:changed",
        serde_json::json!({"conversationId":conversation,"runId":run}),
    );
}
fn project_changed(
    app: &tauri::AppHandle,
    state: &AppState,
    home: &Path,
    project: &str,
) -> Result<(), HttpError> {
    let ids = state.with_connection(home, |db| {
        let mut statement = db.prepare("SELECT id FROM conversations WHERE project_id=?1")?;
        let ids = statement
            .query_map([project], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok::<_, HttpError>(ids)
    })?;
    for id in ids {
        changed(app, &id, None);
    }
    Ok(())
}
fn recover(app: &tauri::AppHandle, home: &Path) -> Result<(), HttpError> {
    let state = app.state::<HttpState>();
    let mut recovered = state.recovered.lock().map_err(|_| storage())?;
    if *recovered {
        return Ok(());
    }
    app.state::<AppState>().with_connection(home,|db| {
        let values=db.prepare("SELECT payload FROM http_runs WHERE json_extract(payload,'$.status')='running'")?.query_map([],|row|row.get::<_,String>(0))?.collect::<Result<Vec<_>,_>>()?;
        for value in values { let mut run:Run=store::decode(value)?; run.status="interrupted".into(); run.finished_at=Some(now()); run.outcome_uncertain=true; run.error=Some("Jarvis foi reiniciado durante a requisição. Verifique os efeitos no servidor antes de reenviar.".into()); if let Ok(meta)=std::fs::metadata(run_dir(home,&run.project_id,&run.id)?.join("body")) { run.stored_bytes=meta.len(); run.truncated=true; } store::update_run(db,&run)?; } Ok::<_,HttpError>(())
    })?;
    *recovered = true;
    Ok(())
}
async fn blocking<T: Send + 'static>(
    app: tauri::AppHandle,
    operation: impl FnOnce(&tauri::AppHandle, &AppState, &Path) -> Result<T, HttpError> + Send + 'static,
) -> Result<T, HttpError> {
    tauri::async_runtime::spawn_blocking(move || {
        let home = home(&app)?;
        recover(&app, &home)?;
        operation(&app, &app.state::<AppState>(), &home)
    })
    .await
    .map_err(|_| storage())?
}

fn stored_ids(
    db: &rusqlite::Connection,
    table: &str,
) -> Result<std::collections::HashSet<String>, HttpError> {
    let mut statement = db.prepare(&format!("SELECT id FROM {table}"))?;
    let values = statement
        .query_map([], |row| row.get(0))?
        .collect::<Result<_, _>>()?;
    Ok(values)
}
fn sweep_orphans(
    home: &Path,
    projects: &std::collections::HashSet<String>,
    runs: &std::collections::HashSet<String>,
    active_projects: &std::collections::HashSet<String>,
    active_runs: &std::collections::HashSet<String>,
) -> Result<(), HttpError> {
    let entries = match std::fs::read_dir(crate::data_dir::root(home).join("http")) {
        Ok(entries) => entries,
        Err(cause) if cause.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(_) => return Err(storage()),
    };
    for entry in entries {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let project = entry.file_name().to_string_lossy().into_owned();
        if !projects.contains(&project) && !active_projects.contains(&project) {
            std::fs::remove_dir_all(entry.path())?;
            continue;
        }
        if let Ok(children) = std::fs::read_dir(entry.path().join("runs")) {
            for child in children {
                let child = child?;
                let id = child.file_name().to_string_lossy().into_owned();
                if child.file_type()?.is_dir() && !runs.contains(&id) && !active_runs.contains(&id)
                {
                    if child.path().join("redacted").exists() {
                        store::secret_delete(&project, &format!("run:{id}"));
                    }
                    std::fs::remove_dir_all(child.path())?;
                }
            }
        }
    }
    Ok(())
}
fn reconcile_terminal(
    db: &rusqlite::Connection,
    terminal: &mut HashMap<String, Run>,
) -> Result<(), HttpError> {
    for id in terminal.keys().cloned().collect::<Vec<_>>() {
        if let Some(run) = terminal.get(&id) {
            store::update_run(db, run)?;
        }
        terminal.remove(&id);
    }
    Ok(())
}
type CleanupIds = (
    std::collections::HashSet<String>,
    std::collections::HashSet<String>,
    std::collections::HashSet<String>,
);
fn prune_files(
    home: &Path,
    state: &HttpState,
    load_ids: impl FnOnce() -> Result<CleanupIds, HttpError>,
) -> Result<(), HttpError> {
    let _preparation = state.preparation.lock().map_err(|_| storage())?;
    // Load inside the preparation guard, after any asynchronous cancellation
    // wait, so newly registered runs and uploads cannot be mistaken for orphans.
    let (projects, conversations, runs) = load_ids()?;
    let active = state.running.lock().map_err(|_| storage())?;
    let active_projects = active.values().map(|a| a.project.clone()).collect();
    let active_runs = active.keys().cloned().collect();
    drop(active);
    state
        .terminal
        .lock()
        .map_err(|_| storage())?
        .retain(|_, run| conversations.contains(&run.conversation_id));
    sweep_orphans(home, &projects, &runs, &active_projects, &active_runs)
}
pub(crate) async fn prune(app: &tauri::AppHandle) -> Result<(), HttpError> {
    let conversations = blocking(app.clone(), |_, state, home| {
        state.with_connection(home, |db| stored_ids(db, "conversations"))
    })
    .await?;
    let waiting = {
        let state = app.state::<HttpState>();
        let active = state.running.lock().map_err(|_| storage())?;
        active
            .values()
            .filter(|a| !conversations.contains(&a.conversation))
            .map(|a| {
                let done = a.done.subscribe();
                let _ = a.cancel.send(true);
                done
            })
            .collect::<Vec<_>>()
    };
    for mut done in waiting {
        if !*done.borrow() {
            let _ = tokio::time::timeout(Duration::from_secs(5), done.changed()).await;
        }
    }
    blocking(app.clone(), move |app, db_state, home| {
        let state = app.state::<HttpState>();
        prune_files(home, &state, || {
            db_state.with_connection(home, |db| {
                Ok((
                    stored_ids(db, "projects")?,
                    stored_ids(db, "conversations")?,
                    stored_ids(db, "http_runs")?,
                ))
            })
        })
    })
    .await
}

#[tauri::command]
pub(crate) async fn get_project_http_settings(
    app: tauri::AppHandle,
    project_id: String,
) -> Result<Settings, HttpError> {
    blocking(app, move |_, state, home| {
        state.with_connection(home, |db| {
            store::settings(db, &project_id).map(store::masked_settings)
        })
    })
    .await
}
#[tauri::command]
pub(crate) async fn save_project_http_settings(
    app: tauri::AppHandle,
    project_id: String,
    settings: Settings,
) -> Result<Settings, HttpError> {
    blocking(app, move |app, state, home| {
        let result =
            state.with_connection(home, |db| store::save_settings(db, &project_id, settings))?;
        let conversations: Vec<String> = state.with_connection(home, |db| {
            Ok::<_, HttpError>(
                db.prepare("SELECT id FROM conversations WHERE project_id=?1")?
                    .query_map([&project_id], |row| row.get(0))?
                    .collect::<Result<_, _>>()?,
            )
        })?;
        for conversation in conversations {
            changed(app, &conversation, None);
        }
        Ok(store::masked_settings(result))
    })
    .await
}
#[tauri::command]
pub(crate) async fn get_http_snapshot(
    app: tauri::AppHandle,
    conversation_id: String,
) -> Result<Snapshot, HttpError> {
    blocking(app, move |app, state, home| {
        let mut snapshot = state.with_connection(home, |db| {
            let project = store::conversation(db, &conversation_id)?;
            Ok::<_, HttpError>(Snapshot {
                project_id: project.clone(),
                conversation_id: conversation_id.clone(),
                drafts: store::list(db, "http_drafts", "conversation_id", &conversation_id)?,
                saved_requests: store::list(db, "http_requests", "project_id", &project)?,
                runs: store::list(db, "http_runs", "conversation_id", &conversation_id)?,
                settings: store::masked_settings(store::settings(db, &project)?),
            })
        })?;
        let runtime = app.state::<HttpState>();
        let terminal = runtime.terminal.lock().map_err(|_| storage())?;
        for run in &mut snapshot.runs {
            if let Some(value) = terminal.get(&run.id) {
                *run = value.clone();
            }
        }
        Ok(snapshot)
    })
    .await
}
#[tauri::command]
pub(crate) async fn save_http_draft(
    app: tauri::AppHandle,
    conversation_id: String,
    id: Option<String>,
    revision: u64,
    mut request: Request,
    saved_request_id: Option<String>,
) -> Result<Draft, HttpError> {
    blocking(app,move|app,state,home| { let result=state.with_connection(home,|db| { let project=store::conversation(db,&conversation_id)?; if let Some(saved)=&saved_request_id { if !db.query_row("SELECT EXISTS(SELECT 1 FROM http_requests WHERE id=?1 AND project_id=?2)",params![saved,project],|row|row.get::<_,bool>(0))? { return Err(invalid("Requisição salva não pertence a este projeto.")); } } let (id,revision)=if let Some(id)=id { let old=store::draft(db,&conversation_id,&id)?; if old.revision!=revision { return Err(conflict()); } (id,revision+1) } else { if revision!=0 { return Err(conflict()); } (store::id()?,1) }; store::protect_request(&project,&mut request)?; let value=Draft{id,conversation_id:conversation_id.clone(),project_id:project,revision,saved_request_id,request}; db.execute("INSERT INTO http_drafts(id,conversation_id,payload) VALUES(?1,?2,?3) ON CONFLICT(id) DO UPDATE SET payload=excluded.payload",params![value.id,conversation_id,store::encode(&value)?])?; Ok(value) })?; changed(app,&conversation_id,None); Ok(result) }).await
}
#[tauri::command]
pub(crate) async fn close_http_draft(
    app: tauri::AppHandle,
    conversation_id: String,
    id: String,
    revision: u64,
) -> Result<(), HttpError> {
    blocking(app, move |app, state, home| {
        state.with_connection(home, |db| {
            let value = store::draft(db, &conversation_id, &id)?;
            if value.revision != revision {
                return Err(conflict());
            }
            db.execute(
                "DELETE FROM http_drafts WHERE id=?1 AND conversation_id=?2",
                params![id, conversation_id],
            )?;
            Ok::<_, HttpError>(())
        })?;
        changed(app, &conversation_id, None);
        Ok(())
    })
    .await
}
#[tauri::command]
pub(crate) async fn save_http_request(
    app: tauri::AppHandle,
    project_id: String,
    id: Option<String>,
    revision: u64,
    mut request: Request,
) -> Result<SavedRequest, HttpError> {
    blocking(app, move |app,state,home| {
        let value=state.with_connection(home, |db| {
            store::project(db,&project_id)?;
            let (id,revision)=if let Some(id)=id {
                let old:SavedRequest=store::decode(db.query_row("SELECT payload FROM http_requests WHERE id=?1 AND project_id=?2",params![id,project_id],|row|row.get(0)).optional()?.ok_or_else(||invalid("Requisição salva não encontrada."))?)?;
                if old.revision!=revision {return Err(conflict());}
                (id,revision+1)
            } else {
                if revision!=0 {return Err(conflict());}
                (store::id()?,1)
            };
            store::protect_request(&project_id,&mut request)?;
            let value=SavedRequest{id,project_id:project_id.clone(),revision,request};
            db.execute("INSERT INTO http_requests(id,project_id,payload) VALUES(?1,?2,?3) ON CONFLICT(id) DO UPDATE SET payload=excluded.payload",params![value.id,project_id,store::encode(&value)?])?;
            Ok::<_,HttpError>(value)
        })?;
        project_changed(app,state,home,&project_id)?;
        Ok(value)
    }).await
}
#[tauri::command]
pub(crate) async fn delete_http_request(
    app: tauri::AppHandle,
    project_id: String,
    id: String,
    revision: u64,
) -> Result<(), HttpError> {
    blocking(app, move |app, state, home| {
        state.with_connection(home, |db| {
            let value: SavedRequest = store::decode(
                db.query_row(
                    "SELECT payload FROM http_requests WHERE id=?1 AND project_id=?2",
                    params![id, project_id],
                    |row| row.get(0),
                )
                .optional()?
                .ok_or_else(|| invalid("Requisição salva não encontrada."))?,
            )?;
            if value.revision != revision {
                return Err(conflict());
            }
            db.execute(
                "DELETE FROM http_requests WHERE id=?1 AND project_id=?2",
                params![id, project_id],
            )?;
            Ok::<_, HttpError>(())
        })?;
        project_changed(app, state, home, &project_id)
    })
    .await
}

#[tauri::command]
pub(crate) async fn send_http_request(
    app: tauri::AppHandle,
    conversation_id: String,
    draft_id: String,
    revision: u64,
) -> Result<Run, HttpError> {
    start(app, conversation_id, draft_id, revision, None).await
}
pub(crate) async fn send_http_request_for_agent(
    app: tauri::AppHandle,
    conversation_id: String,
    draft_id: String,
    revision: u64,
    root: PathBuf,
) -> Result<Run, HttpError> {
    start(app, conversation_id, draft_id, revision, Some(root)).await
}
async fn start(
    app: tauri::AppHandle,
    conversation_id: String,
    draft_id: String,
    revision: u64,
    root: Option<PathBuf>,
) -> Result<Run, HttpError> {
    let task_app = app.clone();
    let (prepared, receiver, done) = blocking(app.clone(), move |app, state, home| {
        let runtime = app.state::<HttpState>();
        let _preparation = runtime.preparation.lock().map_err(|_| storage())?;
        let (draft, settings) = state.with_connection(home, |db| {
            let draft = store::draft(db, &conversation_id, &draft_id)?;
            if draft.revision != revision {
                return Err(conflict());
            }
            let settings = store::settings(db, &draft.project_id)?;
            Ok::<_, HttpError>((draft, settings))
        })?;
        let prepared = transport::prepare(app, home, &draft, &settings, root.as_deref())?;
        let active_state = app.state::<HttpState>();
        let mut active = active_state.running.lock().map_err(|_| storage())?;
        let saved = state.with_connection(home, |db| {
            let mut terminal = runtime.terminal.lock().map_err(|_| storage())?;
            reconcile_terminal(db, &mut terminal)?;
            drop(terminal);
            let current = store::draft(db, &conversation_id, &draft_id)?;
            if current.revision != revision
                || store::settings(db, &draft.project_id)?.revision != settings.revision
            {
                return Err(conflict());
            }
            let running: Vec<Run> =
                store::list(db, "http_runs", "conversation_id", &conversation_id)?;
            if running
                .iter()
                .any(|r| r.draft_id == draft_id && r.status == "running")
            {
                return Err(error(
                    "http_running",
                    "Esta requisição já está em execução.",
                ));
            }
            db.execute(
                "INSERT INTO http_runs(id,conversation_id,created_at,payload) VALUES(?1,?2,?3,?4)",
                params![
                    prepared.run.id,
                    conversation_id,
                    prepared.run.started_at as i64,
                    store::encode(&prepared.run)?
                ],
            )?;
            Ok::<_, HttpError>(())
        });
        if let Err(cause) = saved {
            let _ =
                std::fs::remove_dir_all(run_dir(home, &prepared.run.project_id, &prepared.run.id)?);
            store::secret_delete(
                &prepared.run.project_id,
                &format!("run:{}", prepared.run.id),
            );
            return Err(cause);
        }
        let (cancel, receiver) = watch::channel(false);
        let (done, _) = watch::channel(false);
        active.insert(
            prepared.run.id.clone(),
            Active {
                conversation: conversation_id,
                project: prepared.run.project_id.clone(),
                cancel,
                done: done.clone(),
            },
        );
        Ok((prepared, receiver, done))
    })
    .await?;
    let run = prepared.run.clone();
    changed(&app, &run.conversation_id, Some(&run.id));
    tauri::async_runtime::spawn(async move {
        transport::execute(task_app, prepared, receiver, done).await;
    });
    Ok(run)
}
#[tauri::command]
pub(crate) async fn cancel_http_request(
    app: tauri::AppHandle,
    conversation_id: String,
    run_id: String,
) -> Result<(), HttpError> {
    let owner = conversation_id.clone();
    let id = run_id.clone();
    let run = blocking(app.clone(), move |_, state, home| {
        state.with_connection(home, |db| store::run(db, &owner, &id))
    })
    .await?;
    if run.status != "running" {
        return Ok(());
    }
    let state = app.state::<HttpState>();
    let running = state.running.lock().map_err(|_| storage())?;
    if let Some(active) = running
        .get(&run_id)
        .filter(|a| a.conversation == conversation_id)
    {
        let _ = active.cancel.send(true);
    }
    Ok(())
}
#[tauri::command]
pub(crate) async fn get_http_result(
    app: tauri::AppHandle,
    conversation_id: String,
    run_id: String,
    offset: Option<u64>,
    limit: Option<usize>,
    wait_ms: Option<u64>,
) -> Result<ResultPage, HttpError> {
    if let Some(wait) = wait_ms.filter(|v| *v > 0) {
        let done = {
            let state = app.state::<HttpState>();
            let running = state.running.lock().map_err(|_| storage())?;
            running
                .get(&run_id)
                .filter(|a| a.conversation == conversation_id)
                .map(|a| a.done.subscribe())
        };
        if let Some(mut done) = done {
            if !*done.borrow() {
                let _ =
                    tokio::time::timeout(Duration::from_millis(wait.min(30000)), done.changed())
                        .await;
            }
        }
    }
    blocking(app, move |app, state, home| {
        let mut run =
            state.with_connection(home, |db| store::run(db, &conversation_id, &run_id))?;
        if let Some(terminal) = app
            .state::<HttpState>()
            .terminal
            .lock()
            .map_err(|_| storage())?
            .get(&run_id)
        {
            run = terminal.clone();
        }
        transport::read_result(
            home,
            run,
            offset.unwrap_or(0),
            limit.unwrap_or(16000).clamp(1, 65536),
        )
    })
    .await
}
#[tauri::command]
pub(crate) async fn save_http_response(
    app: tauri::AppHandle,
    conversation_id: String,
    run_id: String,
    path: String,
) -> Result<(), HttpError> {
    blocking(app, move |_, state, home| {
        let run = state.with_connection(home, |db| store::run(db, &conversation_id, &run_id))?;
        if run.status == "running" || run.body_expired {
            return Err(invalid("A resposta ainda está em execução ou expirou."));
        }
        let source = run_dir(home, &run.project_id, &run.id)?.join("body");
        let target = export_target(home, &path)?;
        let parent = target
            .parent()
            .ok_or_else(|| invalid("Destino inválido."))?;
        let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
        let mut input = std::fs::File::open(source)?;
        std::io::copy(&mut input, &mut temporary)?;
        temporary.persist(target).map_err(|_| storage())?;
        Ok(())
    })
    .await
}
#[tauri::command]
pub(crate) async fn import_http_file(
    app: tauri::AppHandle,
    project_id: String,
    path: String,
) -> Result<FileRef, HttpError> {
    blocking(app, move |app,state,home| {
        let runtime = app.state::<HttpState>();
        let _preparation = runtime.preparation.lock().map_err(|_| storage())?;
        state.with_connection(home, |db|store::project(db,&project_id))?;
        let path=std::fs::canonicalize(path)?;
        let mut input=std::fs::File::open(&path)?;
        let meta=input.metadata()?;
        if !meta.is_file()||meta.len()>20*1024*1024 {return Err(invalid("Selecione um arquivo de até 20 MB."));}
        let id=store::id()?;
        let parent=directory(home,&project_id)?.join("files");
        std::fs::create_dir_all(&parent)?;
        let staging=tempfile::tempdir_in(&parent)?;
        let file=FileRef{id,name:path.file_name().and_then(|v|v.to_str()).unwrap_or("arquivo").into(),size:meta.len(),source:path.to_string_lossy().into_owned()};
        let mut output=std::fs::File::create(staging.path().join("body"))?;
        let bytes=std::io::copy(&mut std::io::Read::take(&mut input,20*1024*1024+1),&mut output)?;
        if bytes>20*1024*1024 {return Err(invalid("Arquivo excedeu 20 MB durante a importação."));}
        std::fs::write(staging.path().join("metadata.json"),serde_json::to_vec(&serde_json::json!({"id":file.id,"name":file.name,"size":bytes,"source":file.source})).map_err(|_|storage())?)?;
        drop(output);
        std::fs::rename(staging.path(),parent.join(&file.id))?;
        Ok(FileRef{size:bytes,..file})
    }).await
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Portable {
    format: String,
    version: u32,
    settings: Settings,
    requests: Vec<SavedRequest>,
}
fn strip_portable(request: &mut Request) {
    fn clean(value: &mut String, sensitive: bool) {
        let template = value.trim().strip_prefix("Bearer ").unwrap_or(value.trim());
        let shared_reference = template
            .strip_prefix("{{")
            .and_then(|value| value.strip_suffix("}}"))
            .is_some_and(|name| !name.is_empty() && !name.contains(['{', '}', ':']));
        if value.contains("{{secret:") || (sensitive && !shared_reference) {
            value.clear();
        }
    }
    for value in [
        &mut request.auth.password,
        &mut request.auth.token,
        &mut request.auth.value,
    ] {
        clean(value, true);
    }
    for value in [
        &mut request.name,
        &mut request.url,
        &mut request.auth.username,
        &mut request.auth.name,
    ] {
        clean(value, false);
    }
    if let Some((base, query)) = request.url.split_once('?') {
        let (query, fragment) = query
            .split_once('#')
            .map_or((query, None), |(q, f)| (q, Some(f)));
        let pairs: Vec<_> = url::form_urlencoded::parse(query.as_bytes()).collect();
        if pairs.iter().any(|(name, _)| store::sensitive(name)) {
            let mut params: Vec<_> = pairs
                .into_iter()
                .map(|(name, value)| Pair {
                    name: name.into_owned(),
                    value: value.into_owned(),
                    enabled: true,
                })
                .collect();
            params.append(&mut request.params);
            request.params = params;
            request.url = match fragment {
                Some(fragment) => format!("{base}#{fragment}"),
                None => base.to_owned(),
            };
        }
    }
    for pair in request.headers.iter_mut().chain(&mut request.params) {
        clean(&mut pair.value, store::sensitive(&pair.name));
        clean(&mut pair.name, false);
    }
    request.body.file_id = None;
    for field in &mut request.body.fields {
        field.file_id = None;
        clean(&mut field.value, store::sensitive(&field.name));
        clean(&mut field.name, false);
    }
    if request.body.kind == "json" {
        fn walk(value: &mut serde_json::Value) {
            match value {
                serde_json::Value::Object(fields) => {
                    for (name, value) in fields {
                        if let Some(text) = value.as_str() {
                            let mut text = text.to_owned();
                            clean(&mut text, store::sensitive(name));
                            *value = serde_json::Value::String(text);
                        } else {
                            walk(value);
                        }
                    }
                }
                serde_json::Value::Array(items) => {
                    for item in items {
                        walk(item);
                    }
                }
                serde_json::Value::String(text) => clean(text, false),
                _ => {}
            }
        }
        if let Ok(mut value) = serde_json::from_str::<serde_json::Value>(&request.body.text) {
            walk(&mut value);
            request.body.text = serde_json::to_string_pretty(&value).unwrap_or_default();
        }
    }
    clean(&mut request.body.text, false);
}
#[tauri::command]
pub(crate) async fn export_project_http(
    app: tauri::AppHandle,
    project_id: String,
    path: String,
) -> Result<(), HttpError> {
    blocking(app, move |_, state, home| {
        let mut payload = state.with_connection(home, |db| {
            Ok::<_, HttpError>(Portable {
                format: "jarvis-http".into(),
                version: 1,
                settings: store::settings(db, &project_id)?,
                requests: store::list(db, "http_requests", "project_id", &project_id)?,
            })
        })?;
        for v in payload.settings.variables.iter_mut().chain(
            payload
                .settings
                .environments
                .iter_mut()
                .flat_map(|e| &mut e.variables),
        ) {
            v.secret_ref = None;
            if v.secret {
                v.value.clear();
                v.configured = false;
            }
        }
        payload.settings.project_id.clear();
        payload.settings.revision = 0;
        payload.settings.defaults.ca_file.clear();
        for r in &mut payload.requests {
            r.project_id.clear();
            r.revision = 0;
            strip_portable(&mut r.request);
        }
        let path = export_target(home, &path)?;
        let mut file = tempfile::NamedTempFile::new_in(
            path.parent().ok_or_else(|| invalid("Destino inválido."))?,
        )?;
        std::io::Write::write_all(
            &mut file,
            &serde_json::to_vec_pretty(&payload).map_err(|_| storage())?,
        )?;
        file.persist(path).map_err(|_| storage())?;
        Ok(())
    })
    .await
}
#[tauri::command]
pub(crate) async fn import_project_http(
    app: tauri::AppHandle,
    project_id: String,
    path: String,
    revision: u64,
) -> Result<Settings, HttpError> {
    blocking(app, move |app, state, home| {
        use std::io::Read;
        let mut bytes = Vec::new();
        std::fs::File::open(path)?
            .take(5 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() > 5 * 1024 * 1024 {
            return Err(invalid("Arquivo HTTP excede 5 MB."));
        }
        let mut value: Portable = serde_json::from_slice(&bytes)
            .map_err(|_| invalid("Arquivo de configurações HTTP inválido."))?;
        if value.format != "jarvis-http" || value.version != 1 || value.requests.len() > 1000 {
            return Err(invalid("Formato HTTP não suportado."));
        }
        value.settings.project_id = project_id.clone();
        value.settings.revision = revision;
        value.settings.defaults.ca_file.clear();
        for v in value.settings.variables.iter_mut().chain(
            value
                .settings
                .environments
                .iter_mut()
                .flat_map(|e| &mut e.variables),
        ) {
            v.secret_ref = None;
            if v.secret {
                v.value.clear();
                v.configured = false;
            }
        }
        store::validate_settings(&value.settings)?;
        for r in &mut value.requests {
            strip_portable(&mut r.request);
            store::validate_request(&r.request)?;
            r.id = store::id()?;
            r.project_id = project_id.clone();
            r.revision = 1;
        }
        let settings = state.with_connection(home, |db| {
            store::save_settings_with(db, &project_id, value.settings, |tx| {
                for r in value.requests {
                    tx.execute(
                        "INSERT INTO http_requests(id,project_id,payload)VALUES(?1,?2,?3)",
                        params![r.id, project_id, store::encode(&r)?],
                    )?;
                }
                Ok(())
            })
        })?;
        let conversations: Vec<String> = state.with_connection(home, |db| {
            let mut stmt = db.prepare("SELECT id FROM conversations WHERE project_id=?1")?;
            let result = stmt
                .query_map([&project_id], |row| row.get(0))?
                .collect::<Result<_, _>>()?;
            Ok::<_, HttpError>(result)
        })?;
        for conversation in conversations {
            changed(app, &conversation, None);
        }
        Ok(store::masked_settings(settings))
    })
    .await
}

#[cfg(test)]
mod tests_extra;
