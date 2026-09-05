mod journal;
mod provider;
mod title;
mod tools;

use crate::{library, openai_codex::OpenAiCodexState, persistence::AppState};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tauri::{Emitter, Manager};
use tokio::sync::{oneshot, watch};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentError {
    code: String,
    message: String,
}
impl AgentError {
    fn new(code: &str, message: &str) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }
    fn storage() -> Self {
        Self::new("session_storage", "Não foi possível salvar o histórico. A execução foi interrompida para preservar a conversa.")
    }
    fn cancelled() -> Self {
        Self::new("cancelled", "Execução interrompida.")
    }
    fn internal() -> Self {
        Self::new(
            "internal",
            "Não foi possível concluir a execução do agente.",
        )
    }
}
impl From<crate::persistence::PersistenceError> for AgentError {
    fn from(_: crate::persistence::PersistenceError) -> Self {
        Self::storage()
    }
}
impl From<crate::library::LibraryError> for AgentError {
    fn from(_: crate::library::LibraryError) -> Self {
        Self::new("conversation_unavailable", "A conversa ou a pasta do projeto está indisponível. Verifique o caminho e o histórico.")
    }
}
impl From<crate::openai_codex::ProviderError> for AgentError {
    fn from(value: crate::openai_codex::ProviderError) -> Self {
        Self {
            code: value.code,
            message: value.message,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    Plan,
    Build,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ApprovalMode {
    Manual,
    Yolo,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TurnOptions {
    account: String,
    model: String,
    reasoning: Option<String>,
    mode: Mode,
    approval_mode: ApprovalMode,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum TurnStatus {
    Running,
    Completed,
    Cancelled,
    Error,
    Interrupted,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolCall {
    id: String,
    name: String,
    args: Value,
    status: String,
    output: String,
    duration_ms: u64,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Usage {
    input_tokens: u64,
    output_tokens: u64,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Step {
    #[serde(default)]
    duration_ms: u64,
    text: String,
    summary: String,
    tools: Vec<ToolCall>,
    usage: Option<Usage>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Turn {
    id: String,
    created_at: u64,
    duration_ms: u64,
    user: String,
    options: TurnOptions,
    status: TurnStatus,
    steps: Vec<Step>,
    error: Option<AgentError>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredTurn {
    turn: Turn,
    wire: Vec<Value>,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatSnapshot {
    conversation_id: String,
    revision: u64,
    turns: Vec<Turn>,
    active_turn_id: Option<String>,
    pending_approval: Option<ToolCall>,
}
struct Approval {
    tool: ToolCall,
    reply: oneshot::Sender<bool>,
}
struct Active {
    id: String,
    cancel: watch::Sender<bool>,
    approval: Option<Approval>,
}
struct SessionData {
    turns: Vec<StoredTurn>,
    active: Option<Active>,
    revision: u64,
    storage_failed: bool,
    last_emit: std::time::Instant,
}
struct Session {
    id: String,
    journal: PathBuf,
    root: PathBuf,
    data: Mutex<SessionData>,
    emit: Arc<dyn Fn(ChatSnapshot) + Send + Sync>,
}
impl Session {
    fn reserve(
        &self,
        content: String,
        options: TurnOptions,
    ) -> Result<watch::Receiver<bool>, AgentError> {
        let mut data = self.data.lock().map_err(|_| AgentError::internal())?;
        if data.active.is_some() {
            return Err(AgentError::new(
                "already_running",
                "Esta conversa já possui uma execução em andamento.",
            ));
        }
        if data.storage_failed {
            return Err(AgentError::storage());
        }
        let id = library::new_id()?;
        let turn = StoredTurn {
            wire: vec![json!({"role":"user", "content":content})],
            turn: Turn {
                id: id.clone(),
                created_at: now(),
                duration_ms: 0,
                user: content,
                options,
                status: TurnStatus::Running,
                steps: vec![],
                error: None,
            },
        };
        if journal::append(&self.journal, &turn).is_err() {
            data.storage_failed = true;
            return Err(AgentError::storage());
        }
        let (cancel, signal) = watch::channel(false);
        data.active = Some(Active {
            id,
            cancel,
            approval: None,
        });
        data.turns.push(turn);
        data.revision += 1;
        Ok(signal)
    }
    fn snapshot_data(&self, data: &SessionData) -> ChatSnapshot {
        ChatSnapshot {
            conversation_id: self.id.clone(),
            revision: data.revision,
            turns: data.turns.iter().map(|item| item.turn.clone()).collect(),
            active_turn_id: data.active.as_ref().map(|active| active.id.clone()),
            pending_approval: data.active.as_ref().and_then(|active| {
                active
                    .approval
                    .as_ref()
                    .map(|approval| approval.tool.clone())
            }),
        }
    }
    fn snapshot(&self) -> Result<ChatSnapshot, AgentError> {
        let data = self.data.lock().map_err(|_| AgentError::internal())?;
        Ok(self.snapshot_data(&data))
    }
    fn update(
        &self,
        durable: bool,
        change: impl FnOnce(&mut SessionData),
    ) -> Result<(), AgentError> {
        let mut data = self.data.lock().map_err(|_| AgentError::internal())?;
        change(&mut data);
        data.revision += 1;
        if durable {
            if let Some(last) = data.turns.last() {
                if journal::append(&self.journal, last).is_err() {
                    data.storage_failed = true;
                    return Err(AgentError::storage());
                }
            }
        }
        let should_emit = durable || data.last_emit.elapsed() >= Duration::from_millis(50);
        let snapshot = should_emit.then(|| self.snapshot_data(&data));
        if should_emit {
            data.last_emit = std::time::Instant::now();
        }
        drop(data);
        if let Some(snapshot) = snapshot {
            (self.emit)(snapshot);
        }
        Ok(())
    }
    fn input(&self) -> Result<Vec<Value>, AgentError> {
        let data = self.data.lock().map_err(|_| AgentError::internal())?;
        let input: Vec<Value> = data
            .turns
            .iter()
            .flat_map(|turn| turn.wire.iter().cloned())
            .collect();
        if serde_json::to_vec(&input)
            .map_err(|_| AgentError::internal())?
            .len()
            > 8 * 1024 * 1024
        {
            return Err(AgentError::new("context_limit", "Esta conversa atingiu o limite de contexto local. Inicie uma nova conversa para continuar."));
        }
        Ok(input)
    }
}
#[derive(Clone, Default)]
pub struct AgentState {
    sessions: Arc<Mutex<HashMap<String, Arc<Session>>>>,
}
impl AgentState {
    pub(crate) fn delete_library_item(
        &self,
        state: &AppState,
        home: &std::path::Path,
        target: &library::deletion::DeleteTarget,
    ) -> Result<library::LibrarySnapshot, library::LibraryError> {
        let internal = || {
            library::LibraryError::new(
                "agent_state",
                "Não foi possível verificar as execuções. Reabra o aplicativo e tente novamente.",
            )
        };
        // Keep the same registry -> database order as session loading. Holding each idle
        // session lock also prevents a previously acquired Arc from reserving a late turn.
        let mut sessions = self.sessions.lock().map_err(|_| internal())?;
        state.with_connection(home, |connection| {
            let ids = library::deletion::conversation_ids(connection, target)?;
            let targets: Vec<_> = ids
                .iter()
                .filter_map(|id| sessions.get(id).cloned())
                .collect();
            let locked = targets
                .iter()
                .map(|session| session.data.lock().map_err(|_| internal()))
                .collect::<Result<Vec<_>, _>>()?;
            if locked.iter().any(|data| data.active.is_some()) {
                return Err(library::LibraryError::new(
                    "active_conversation",
                    "Interrompa as conversas em execução antes de excluir este item.",
                ));
            }
            let result = library::deletion::delete(connection, home, target);
            // Evict even when committed metadata is waiting for filesystem cleanup.
            for id in ids {
                let exists = connection.query_row(
                    "SELECT EXISTS(SELECT 1 FROM conversations WHERE id = ?1)",
                    [&id],
                    |row| row.get::<_, bool>(0),
                )?;
                if !exists {
                    sessions.remove(&id);
                }
            }
            result
        })
    }

    fn activity(&self) -> Result<Vec<AgentActivity>, AgentError> {
        let sessions = self.sessions.lock().map_err(|_| AgentError::internal())?;
        sessions
            .values()
            .map(|session| {
                let data = session.data.lock().map_err(|_| AgentError::internal())?;
                Ok(AgentActivity {
                    conversation_id: session.id.clone(),
                    revision: data.revision,
                    active_turn_id: data.active.as_ref().map(|active| active.id.clone()),
                })
            })
            .collect()
    }
    fn session(
        &self,
        app: &tauri::AppHandle,
        state: &AppState,
        home: &std::path::Path,
        id: &str,
    ) -> Result<Arc<Session>, AgentError> {
        let mut sessions = self.sessions.lock().map_err(|_| AgentError::internal())?;
        if let Some(session) = sessions.get(id) {
            return Ok(session.clone());
        }
        let (path, root) = library::agent_location(state, home, id)?;
        let turns = journal::load(&path)?;
        let handle = app.clone();
        let session = Arc::new(Session {
            id: id.into(),
            journal: path,
            root,
            data: Mutex::new(SessionData {
                turns,
                active: None,
                revision: 1,
                storage_failed: false,
                last_emit: std::time::Instant::now(),
            }),
            emit: Arc::new(move |snapshot| {
                let _ = handle.emit("agent:updated", snapshot);
            }),
        });
        sessions.insert(id.into(), session.clone());
        Ok(session)
    }
    fn existing(&self, id: &str) -> Result<Arc<Session>, AgentError> {
        self.sessions
            .lock()
            .map_err(|_| AgentError::internal())?
            .get(id)
            .cloned()
            .ok_or_else(AgentError::cancelled)
    }
}
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
async fn cancelled(signal: &mut watch::Receiver<bool>) {
    loop {
        if *signal.borrow_and_update() {
            return;
        }
        if signal.changed().await.is_err() {
            return;
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentActivity {
    conversation_id: String,
    revision: u64,
    active_turn_id: Option<String>,
}

#[tauri::command]
pub fn get_agent_activity(
    agent: tauri::State<'_, AgentState>,
) -> Result<Vec<AgentActivity>, AgentError> {
    agent.activity()
}

#[tauri::command]
pub async fn get_chat(
    app: tauri::AppHandle,
    persistence: tauri::State<'_, AppState>,
    agent: tauri::State<'_, AgentState>,
    conversation_id: String,
) -> Result<ChatSnapshot, AgentError> {
    let state = persistence.inner().clone();
    let agent = agent.inner().clone();
    let home = app.path().home_dir().map_err(|_| AgentError::storage())?;
    tauri::async_runtime::spawn_blocking(move || {
        agent
            .session(&app, &state, &home, &conversation_id)?
            .snapshot()
    })
    .await
    .map_err(|_| AgentError::internal())?
}

#[tauri::command]
pub async fn start_agent_turn(
    app: tauri::AppHandle,
    persistence: tauri::State<'_, AppState>,
    oauth: tauri::State<'_, OpenAiCodexState>,
    agent: tauri::State<'_, AgentState>,
    conversation_id: String,
    content: String,
    options: TurnOptions,
) -> Result<ChatSnapshot, AgentError> {
    let content = content.trim().to_owned();
    if content.is_empty() || content.len() > 100_000 || options.model.len() > 200 {
        return Err(AgentError::new(
            "invalid_message",
            "Envie uma mensagem entre 1 e 100.000 bytes.",
        ));
    }
    let state = persistence.inner().clone();
    let oauth = oauth.inner().clone();
    let agent = agent.inner().clone();
    let home = app.path().home_dir().map_err(|_| AgentError::storage())?;
    let run_app = app.clone();
    let run_state = state.clone();
    let run_home = home.clone();
    let (session, signal) = tauri::async_runtime::spawn_blocking(move || {
        let session = agent.session(&app, &state, &home, &conversation_id)?;
        // Revalidate the project for every turn, including already loaded conversations.
        library::agent_location(&state, &home, &conversation_id)?;
        let signal = session.reserve(content, options)?;
        Ok::<_, AgentError>((session, signal))
    })
    .await
    .map_err(|_| AgentError::internal())??;
    let initial = session.snapshot()?;
    (session.emit)(initial.clone());
    tauri::async_runtime::spawn(async move {
        let result = run_turn(&session, &run_state, &oauth, &run_home, signal.clone()).await;
        let completed = result.is_ok();
        finish(&session, result);
        if completed {
            generate_title(&session, &run_state, &oauth, &run_home, &run_app).await;
        }
    });
    Ok(initial)
}

#[tauri::command]
pub fn cancel_agent_turn(
    agent: tauri::State<'_, AgentState>,
    conversation_id: String,
    turn_id: String,
) -> Result<(), AgentError> {
    let session = agent.existing(&conversation_id)?;
    let data = session.data.lock().map_err(|_| AgentError::internal())?;
    if let Some(active) = &data.active {
        if active.id == turn_id {
            let _ = active.cancel.send(true);
        }
    }
    Ok(())
}
#[tauri::command]
pub fn approve_agent_tool(
    agent: tauri::State<'_, AgentState>,
    conversation_id: String,
    turn_id: String,
    tool_id: String,
    approved: bool,
) -> Result<(), AgentError> {
    let session = agent.existing(&conversation_id)?;
    answer_approval(&session, &turn_id, &tool_id, approved)
}

fn answer_approval(
    session: &Session,
    turn_id: &str,
    tool_id: &str,
    approved: bool,
) -> Result<(), AgentError> {
    let mut data = session.data.lock().map_err(|_| AgentError::internal())?;
    let active = data
        .active
        .as_mut()
        .filter(|active| active.id == turn_id)
        .ok_or_else(AgentError::cancelled)?;
    if !active
        .approval
        .as_ref()
        .is_some_and(|approval| approval.tool.id == tool_id)
    {
        return Err(AgentError::new(
            "stale_approval",
            "Esta solicitação de autorização não está mais ativa.",
        ));
    }
    if let Some(approval) = active.approval.take() {
        let _ = approval.reply.send(approved);
    }
    Ok(())
}

async fn authorize(
    session: &Session,
    tool: &ToolCall,
    options: &TurnOptions,
    mut signal: watch::Receiver<bool>,
) -> Result<bool, AgentError> {
    if *signal.borrow() {
        return Err(AgentError::cancelled());
    }
    if !tools::needs_approval(&tool.name)
        || options.approval_mode == ApprovalMode::Yolo
        || options.mode == Mode::Plan
    {
        return Ok(true);
    }
    let (reply, received) = oneshot::channel();
    session.update(true, |data| {
        data.active.as_mut().unwrap().approval = Some(Approval {
            tool: tool.clone(),
            reply,
        });
    })?;
    let approved = tokio::select! {
        _ = cancelled(&mut signal) => return Err(AgentError::cancelled()),
        result = received => result.unwrap_or(false),
    };
    session.update(true, |data| {
        data.active.as_mut().unwrap().approval = None;
    })?;
    Ok(approved)
}

async fn run_turn(
    session: &Arc<Session>,
    state: &AppState,
    oauth: &OpenAiCodexState,
    home: &std::path::Path,
    mut signal: watch::Receiver<bool>,
) -> Result<(), AgentError> {
    let options = session
        .data
        .lock()
        .map_err(|_| AgentError::internal())?
        .turns
        .last()
        .ok_or_else(AgentError::internal)?
        .turn
        .options
        .clone();
    let auth_state = state.clone();
    let auth_oauth = oauth.clone();
    let auth_home = home.to_path_buf();
    let auth_options = options.clone();
    let auth = tauri::async_runtime::spawn_blocking(move || {
        auth_oauth.inference_credential(
            &auth_state,
            &auth_home,
            &auth_options.account,
            &auth_options.model,
            auth_options.reasoning.as_deref(),
        )
    });
    let credential = tokio::select! {
        _ = cancelled(&mut signal) => return Err(AgentError::cancelled()),
        result = auth => result.map_err(|_| AgentError::internal())??,
    };
    let instructions = tools::instructions(&session.root, options.mode);
    for _ in 0..32 {
        let step_started = std::time::Instant::now();
        if *signal.borrow() {
            return Err(AgentError::cancelled());
        }
        session.update(false, |data| {
            data.turns
                .last_mut()
                .unwrap()
                .turn
                .steps
                .push(Step::default());
        })?;
        let input = session.input()?;
        let response = provider::stream(
            &credential,
            &session.id,
            &options,
            &instructions,
            input,
            tools::definitions(options.mode),
            signal.clone(),
            |delta| {
                session.update(false, |data| {
                    let step = data
                        .turns
                        .last_mut()
                        .unwrap()
                        .turn
                        .steps
                        .last_mut()
                        .unwrap();
                    match delta {
                        provider::Delta::Text(text) => step.text.push_str(&text),
                        provider::Delta::Summary(text) => step.summary.push_str(&text),
                    }
                })
            },
        )
        .await?;
        let calls = provider::tool_calls(&response.output)?;
        let previous = session.input()?;
        if calls
            .iter()
            .any(|call| previous.iter().any(|item| item["call_id"] == call.id))
        {
            return Err(AgentError::new("duplicate_tool_call", "O provedor repetiu um identificador de ferramenta. A execução foi interrompida antes de repetir a ação."));
        }
        session.update(true, |data| {
            let current = data.turns.last_mut().unwrap();
            let step = current.turn.steps.last_mut().unwrap();
            step.text = response.text;
            step.summary = response.summary;
            step.usage = response.usage;
            step.duration_ms = step_started.elapsed().as_millis() as u64;
            step.tools = calls.clone();
            current.wire.extend(response.output);
        })?;
        if calls.is_empty() {
            return Ok(());
        }
        for tool in calls {
            if *signal.borrow() {
                return Err(AgentError::cancelled());
            }
            let permitted = authorize(session, &tool, &options, signal.clone()).await?;
            let started = std::time::Instant::now();
            session.update(true, |data| {
                let step = data
                    .turns
                    .last_mut()
                    .unwrap()
                    .turn
                    .steps
                    .last_mut()
                    .unwrap();
                if let Some(item) = step.tools.iter_mut().find(|item| item.id == tool.id) {
                    item.status = "running".into();
                }
            })?;
            let result = if permitted {
                tools::execute(&session.root, &tool, options.mode, signal.clone()).await
            } else {
                Err(AgentError::new(
                    "denied",
                    "A execução desta ferramenta foi recusada pelo usuário.",
                ))
            };
            let (output, status) = match result {
                Ok(output) => (output, "completed"),
                Err(error) => (error.message, "error"),
            };
            session.update(true, |data| {
                let current = data.turns.last_mut().unwrap();
                current.wire.push(
                    json!({"type":"function_call_output", "call_id":tool.id, "output":output}),
                );
                let step = current.turn.steps.last_mut().unwrap();
                step.duration_ms = step_started.elapsed().as_millis() as u64;
                if let Some(item) = step.tools.iter_mut().find(|item| item.id == tool.id) {
                    item.status = status.into();
                    item.output = output;
                    item.duration_ms = started.elapsed().as_millis() as u64;
                }
            })?;
        }
    }
    Err(AgentError::new("turn_limit", "O agente atingiu o limite de 32 etapas nesta interação. Revise o progresso e envie uma nova instrução para continuar."))
}

fn finish(session: &Session, result: Result<(), AgentError>) {
    let update = session.update(true, |data| {
        let current = data.turns.last_mut().unwrap();
        journal::interrupt_tools(current);
        current.turn.duration_ms = now().saturating_sub(current.turn.created_at);
        match result {
            Ok(()) => current.turn.status = TurnStatus::Completed,
            Err(error) => {
                current.turn.status = if error.code == "cancelled" {
                    TurnStatus::Cancelled
                } else {
                    TurnStatus::Error
                };
                current.turn.error = Some(error);
            }
        }
        data.active = None;
    });
    if update.is_err() {
        // Surface journal failure even if the final checkpoint could not be written.
        let _ = session.update(false, |data| {
            data.active = None;
            if let Some(current) = data.turns.last_mut() {
                current.turn.status = TurnStatus::Error;
                current.turn.error = Some(AgentError::storage());
            }
            data.last_emit = std::time::Instant::now() - Duration::from_secs(1);
        });
    }
}

async fn generate_title(
    session: &Session,
    state: &AppState,
    oauth: &OpenAiCodexState,
    home: &std::path::Path,
    app: &tauri::AppHandle,
) {
    let first = session.data.lock().ok().and_then(|data| {
        data.turns
            .iter()
            .find(|item| item.turn.status == TurnStatus::Completed)
            .cloned()
    });
    let Some(first) = first else {
        return;
    };
    if !library::needs_generated_title(state, home, &session.id).unwrap_or(false) {
        return;
    }
    let state_clone = state.clone();
    let oauth = oauth.clone();
    let home_clone = home.to_path_buf();
    let options = first.turn.options.clone();
    let options_clone = options.clone();
    let auth = tauri::async_runtime::spawn_blocking(move || {
        oauth.inference_credential(
            &state_clone,
            &home_clone,
            &options_clone.account,
            &options_clone.model,
            options_clone.reasoning.as_deref(),
        )
    })
    .await;
    let Ok(Ok(credential)) = auth else {
        return;
    };
    let reply: String = first
        .turn
        .steps
        .iter()
        .map(|step| step.text.as_str())
        .collect();
    let input = vec![
        json!({"role":"user", "content":format!("Pedido: {}\nResposta: {}", first.turn.user.chars().take(2000).collect::<String>(), reply.chars().take(3000).collect::<String>())}),
    ];
    let (_sender, signal) = watch::channel(false);
    let result = tokio::time::timeout(
        Duration::from_secs(45),
        provider::stream(
            &credential,
            &session.id,
            &options,
            title::INSTRUCTIONS,
            input,
            vec![],
            signal,
            |_| Ok(()),
        ),
    )
    .await;
    if let Ok(Ok(response)) = result {
        if let Some(title) = title::normalize(&response.text) {
            if library::save_generated_title(state, home, &session.id, &title).unwrap_or(false) {
                let _ = app.emit("library:changed", &session.id);
            }
        }
    }
}

#[cfg(test)]
mod tests;
