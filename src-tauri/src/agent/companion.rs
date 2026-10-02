//! A bounded desktop view of loaded execution state. Reading it never resumes a chat.
pub(crate) use super::questions::Response as QuestionResponse;
use super::*;
use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};
use tauri::{Emitter, Manager};

const MAX_ITEMS: usize = 32;
const MAX_RECENT: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Status {
    Running,
    Waiting,
    Reconnecting,
    Completed,
    Failed,
    Idle,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct QuestionOption {
    label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    #[serde(default)]
    recommended: bool,
    #[serde(default, skip_serializing)]
    preview: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Question {
    id: String,
    question: String,
    options: Vec<QuestionOption>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PendingQuestion {
    turn_id: String,
    tool_id: String,
    questions: Vec<Question>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    deadline_at: Option<u64>,
}

impl PendingQuestion {
    fn visual(&self) -> bool {
        self.questions.iter().any(|question| {
            question
                .options
                .iter()
                .any(|option| option.preview.is_some())
        })
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Item {
    conversation_id: String,
    agent_id: Option<String>,
    project_id: String,
    project_name: String,
    global: bool,
    title: String,
    role: String,
    status: Status,
    attention_id: String,
    acknowledged: bool,
    #[serde(rename = "revision")]
    attention_generation: u64,
    activity: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<String>,
    duration_ms: u64,
    active_since: Option<u64>,
    updated_at: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pending_question: Option<PendingQuestion>,
    requires_conversation: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Snapshot {
    items: Vec<Item>,
    truncated: bool,
}

#[derive(Clone)]
struct Record {
    revision: u64,
    turn_id: String,
    identity: Option<String>,
    item: Item,
}

impl Record {
    fn with_identity(mut self, identity: Option<String>) -> Self {
        self.identity = identity.map(|name| short(&name, 120));
        if let Some(name) = &self.identity {
            self.item.role = name.clone();
        }
        self
    }
}

type Acknowledgements = HashMap<(String, Option<String>), (u64, Vec<String>)>;

#[derive(Clone, Default)]
pub(super) struct Recent {
    records: Arc<Mutex<HashMap<String, Record>>>,
    acknowledged: Arc<Mutex<Acknowledgements>>,
}

impl Recent {
    fn record(&self, mut record: Record) -> Result<(), AgentError> {
        let mut values = self.records.lock().map_err(|_| AgentError::internal())?;
        let id = record.item.conversation_id.clone();
        if let Some(old) = values.get_mut(&id) {
            if old.revision >= record.revision {
                // A passive read can discover the loaded identity without a new
                // session revision; retain validation attention in that case.
                let identity = record
                    .identity
                    .filter(|_| old.revision == record.revision && old.turn_id == record.turn_id);
                if let Some(name) = identity {
                    old.item.role = name.clone();
                    old.identity = Some(name);
                }
                return Ok(());
            }
            if old.turn_id == record.turn_id && record.identity.is_none() {
                record = record.with_identity(old.identity.clone());
            }
            if old.turn_id == record.turn_id && old.item.status == record.item.status {
                record.item.attention_id = old.item.attention_id.clone();
                record.item.attention_generation = old.item.attention_generation;
            }
        }
        values.insert(id, record);
        if values.len() > MAX_RECENT {
            let oldest = values
                .iter()
                .min_by_key(|(_, record)| (active(record.item.status), record.item.updated_at))
                .map(|(id, _)| id.clone());
            if let Some(id) = oldest {
                values.remove(&id);
                self.acknowledged
                    .lock()
                    .map_err(|_| AgentError::internal())?
                    .retain(|(conversation, _), _| conversation != &id);
            }
        }
        Ok(())
    }

    fn items(&self) -> Result<Vec<Item>, AgentError> {
        Ok(self
            .records
            .lock()
            .map_err(|_| AgentError::internal())?
            .values()
            .map(|record| record.item.clone())
            .collect())
    }

    fn validation(&self, conversation_id: &str) {
        if let Ok(mut records) = self.records.lock() {
            if let Some(record) = records.get_mut(conversation_id) {
                record.item.status = Status::Waiting;
                record.item.attention_id =
                    attention_id(conversation_id, "root", &record.turn_id, Status::Waiting);
                record.item.activity = "Validação disponível".into();
                record.item.active_since = None;
                record.item.pending_question = None;
                record.item.requires_conversation = true;
            }
        }
    }

    fn acknowledge(&self, item: &Item) -> Result<(), AgentError> {
        if matches!(item.status, Status::Completed | Status::Failed) {
            let mut acknowledged = self
                .acknowledged
                .lock()
                .map_err(|_| AgentError::internal())?;
            let key = (item.conversation_id.clone(), item.agent_id.clone());
            let (generation, ids) = acknowledged.entry(key).or_default();
            if *generation < item.attention_generation {
                *generation = item.attention_generation;
                ids.clear();
            }
            // Outcomes can share a revision in a cached view. Keep both exact IDs
            // in that bounded generation instead of overwriting the newer acknowledgement.
            if *generation == item.attention_generation && !ids.contains(&item.attention_id) {
                ids.push(item.attention_id.clone());
            }
        }
        Ok(())
    }

    fn apply_acknowledgements(&self, items: &mut [Item]) -> Result<(), AgentError> {
        let acknowledged = self
            .acknowledged
            .lock()
            .map_err(|_| AgentError::internal())?;
        // Reads may race with newer outcomes. They never prune or overwrite acknowledgement state.
        for item in items {
            item.acknowledged = acknowledged
                .get(&(item.conversation_id.clone(), item.agent_id.clone()))
                .is_some_and(|(_, ids)| ids.contains(&item.attention_id));
        }
        Ok(())
    }
}

fn attention_id(conversation: &str, agent: &str, generation: &str, status: Status) -> String {
    format!("{conversation}/{agent}/{generation}/{status:?}")
}

fn active(status: Status) -> bool {
    matches!(
        status,
        Status::Running | Status::Waiting | Status::Reconnecting
    )
}

fn short(value: &str, maximum: usize) -> String {
    value
        .trim()
        .chars()
        .map(|c| if c.is_whitespace() { ' ' } else { c })
        .filter(|c| !c.is_control())
        .take(maximum)
        .collect()
}

fn pending(request: &questions::PendingQuestion) -> Option<PendingQuestion> {
    // The existing question parser already bounds IDs, labels, descriptions and counts.
    // Leave visual previews to the full conversation instead of duplicating their payload.
    serde_json::to_value(request)
        .ok()
        .and_then(|value| serde_json::from_value(value).ok())
}

fn project(snapshot: &ChatSnapshot) -> Option<Record> {
    let turn = snapshot.turns.last()?;
    let running = snapshot.active_turn_id.is_some() || snapshot.compacting;
    let waiting = running
        && (snapshot.pending_question.is_some()
            || snapshot.pending_approval.is_some()
            || snapshot.pending_authoring.is_some());
    let retry = running && turn.steps.last().is_some_and(|step| step.retry.is_some());
    let status = if waiting {
        Status::Waiting
    } else if retry {
        Status::Reconnecting
    } else if running {
        Status::Running
    } else {
        match turn.status {
            TurnStatus::Completed
                if snapshot
                    .queued_messages
                    .iter()
                    .any(queue::QueuedMessage::scheduled) =>
            {
                Status::Running
            }
            TurnStatus::Completed => Status::Completed,
            TurnStatus::Error => Status::Failed,
            _ => Status::Idle,
        }
    };
    let activity = match status {
        Status::Waiting => "Aguardando sua resposta".into(),
        Status::Reconnecting => "Reconectando ao provedor".into(),
        Status::Completed => "Resposta pronta".into(),
        Status::Failed => short(
            turn.error
                .as_ref()
                .map_or("A execução falhou", |error| &error.message),
            320,
        ),
        Status::Idle => "Execução pausada ou encerrada".into(),
        Status::Running => {
            if snapshot.compacting {
                "Organizando o contexto".into()
            } else if let Some(task) = turn
                .tasks
                .iter()
                .find(|task| task.status == tasks::Status::InProgress)
            {
                short(&task.title, 320)
            } else if let Some(tool) = turn
                .steps
                .iter()
                .rev()
                .flat_map(|step| step.tools.iter().rev())
                .find(|tool| matches!(tool.status.as_str(), "pending" | "running"))
            {
                format!("Usando {}", short(&tool.name, 80))
            } else {
                turn.steps
                    .iter()
                    .rev()
                    .find(|step| !step.summary.trim().is_empty())
                    .map_or_else(|| "Trabalhando…".into(), |step| short(&step.summary, 320))
            }
        }
    };
    let question = snapshot.pending_question.as_ref().and_then(pending);
    let requires_conversation = snapshot.pending_approval.is_some()
        || snapshot.pending_authoring.is_some()
        || question.as_ref().is_some_and(PendingQuestion::visual);
    Some(Record {
        revision: snapshot.revision,
        turn_id: turn.id.clone(),
        identity: None,
        item: Item {
            conversation_id: snapshot.conversation_id.clone(),
            agent_id: None,
            project_id: String::new(),
            project_name: String::new(),
            global: false,
            title: String::new(),
            role: turn
                .options
                .workflow
                .unwrap_or_default()
                .root()
                .label()
                .into(),
            status,
            attention_id: attention_id(
                &snapshot.conversation_id,
                "root",
                &format!("{}-{}", turn.id, snapshot.revision),
                status,
            ),
            acknowledged: false,
            attention_generation: snapshot.revision,
            activity,
            result: (status == Status::Completed)
                .then(|| {
                    turn.steps
                        .iter()
                        .rev()
                        .find(|step| !step.text.trim().is_empty())
                })
                .flatten()
                .map(|step| short(&step.text, 320)),
            duration_ms: turn.duration_ms,
            active_since: if active(status) && !waiting {
                turn.active_since
            } else {
                None
            },
            updated_at: now(),
            pending_question: if requires_conversation {
                None
            } else {
                question
            },
            requires_conversation,
        },
    })
}

pub(crate) fn changed(app: &tauri::AppHandle) {
    if app.get_webview_window("companion").is_some() {
        let _ = app.emit_to("companion", "companion:changed", ());
    }
}

pub(super) fn observe(app: &tauri::AppHandle, snapshot: &ChatSnapshot) {
    if let Some(record) = project(snapshot) {
        let agent = app.state::<AgentState>();
        let identity = agent
            .workflows
            .loaded_root_identity(&snapshot.conversation_id, &record.turn_id)
            .ok()
            .flatten();
        let _ = agent
            .companion_recent
            .record(record.with_identity(identity));
        changed(app);
    }
}

pub(super) fn validation(app: &tauri::AppHandle, conversation_id: &str) {
    app.state::<AgentState>()
        .companion_recent
        .validation(conversation_id);
    changed(app);
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WorkflowView {
    conversation_id: String,
    agents: Vec<WorkflowAgent>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WorkflowAgent {
    id: String,
    role: workflow::Role,
    status: String,
    #[serde(default)]
    reconnecting: bool,
    title: String,
    updated_at: u64,
    duration_ms: u64,
    active_since: Option<u64>,
    current_thought: Option<String>,
    active_turn_id: Option<String>,
    pending_question: Option<PendingQuestion>,
    pending_approval: Option<Value>,
    pending_authoring: Option<Value>,
    identity: Option<Identity>,
    error: Option<String>,
}

#[derive(Deserialize)]
struct Identity {
    name: String,
}

fn add_workflow(items: &mut Vec<Item>, value: WorkflowView) {
    let Some(root) = items
        .iter()
        .find(|item| item.conversation_id == value.conversation_id && item.agent_id.is_none())
        .cloned()
    else {
        return;
    };
    for card in value.agents {
        let role = card
            .identity
            .as_ref()
            .map_or_else(|| card.role.label(), |identity| identity.name.as_str());
        if card.id == "main" {
            if let Some(item) = items.iter_mut().find(|item| {
                item.conversation_id == value.conversation_id && item.agent_id.is_none()
            }) {
                item.role = short(role, 120);
            }
            continue;
        }
        let waiting = card.pending_question.is_some()
            || card.pending_approval.is_some()
            || card.pending_authoring.is_some();
        let status = if waiting {
            Status::Waiting
        } else if card.reconnecting {
            Status::Reconnecting
        } else if card.active_turn_id.is_some() {
            Status::Running
        } else {
            match card.status.as_str() {
                "queued" | "running" => Status::Running,
                "waiting" | "blocked" => Status::Waiting,
                "completed" => Status::Completed,
                "failed" => Status::Failed,
                _ => Status::Idle,
            }
        };
        let activity = short(
            card.error
                .as_deref()
                .or(card.current_thought.as_deref())
                .unwrap_or(&card.title),
            320,
        );
        let requires_conversation = card.pending_approval.is_some()
            || card.pending_authoring.is_some()
            || card.status == "blocked"
            || card
                .pending_question
                .as_ref()
                .is_some_and(PendingQuestion::visual);
        let attention_id = attention_id(
            &root.conversation_id,
            &card.id,
            &card.updated_at.to_string(),
            status,
        );
        items.push(Item {
            conversation_id: root.conversation_id.clone(),
            agent_id: Some(card.id),
            project_id: root.project_id.clone(),
            project_name: root.project_name.clone(),
            global: root.global,
            title: root.title.clone(),
            role: short(role, 120),
            status,
            attention_id,
            acknowledged: false,
            attention_generation: card.updated_at,
            activity,
            result: (status == Status::Completed)
                .then(|| format!("Tarefa concluída: {}", short(&card.title, 240))),
            duration_ms: card.duration_ms,
            active_since: if active(status) && !waiting {
                card.active_since
            } else {
                None
            },
            updated_at: card.updated_at,
            pending_question: if requires_conversation {
                None
            } else {
                card.pending_question
            },
            requires_conversation,
        });
    }
}

fn limit(mut items: Vec<Item>) -> Snapshot {
    items.sort_by_key(|item| {
        let priority = match item.status {
            Status::Waiting => 0,
            Status::Running | Status::Reconnecting => 1,
            Status::Failed if !item.acknowledged => 2,
            Status::Completed if !item.acknowledged => 3,
            Status::Completed | Status::Failed => 4,
            Status::Idle => 4,
        };
        (
            priority,
            std::cmp::Reverse(item.updated_at),
            item.conversation_id.clone(),
            item.agent_id.clone(),
        )
    });
    let truncated = items.len() > MAX_ITEMS;
    items.truncate(MAX_ITEMS);
    Snapshot { items, truncated }
}

pub(crate) async fn snapshot(app: tauri::AppHandle) -> Result<Snapshot, AgentError> {
    let home = app.path().home_dir().map_err(|_| AgentError::storage())?;
    let agent = app.state::<AgentState>().inner().clone();
    let persistence = app.state::<AppState>().inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let sessions: Vec<_> = agent.sessions.lock().map_err(|_| AgentError::internal())?.values().cloned().collect();
        for session in sessions {
            if let Some(record) = project(&session.snapshot()?) {
                let identity = agent.workflows.loaded_root_identity(&session.id, &record.turn_id)?;
                agent.companion_recent.record(record.with_identity(identity))?;
            }
        }
        let mut items = agent.companion_recent.items()?;
        items = persistence.with_connection(&home, |connection| -> Result<Vec<Item>, AgentError> {
            let mut names = connection.prepare("SELECT p.id, p.name, COALESCE(c.display_title, c.title) FROM conversations c JOIN projects p ON p.id = c.project_id WHERE c.id = ?1").map_err(|_| AgentError::storage())?;
            let mut present = Vec::with_capacity(items.len());
            for mut item in items {
                let metadata = names.query_row([&item.conversation_id], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?))).optional().map_err(|_| AgentError::storage())?;
                let Some((id, project, title)) = metadata else { continue; };
                item.project_id = id;
                item.global = item.project_id == library::companion::GLOBAL_PROJECT_ID;
                item.project_name = if item.global { "Chat geral".into() } else { short(&project, 120) };
                item.title = short(&title, 160);
                present.push(item);
            }
            Ok(present)
        })?;
        for workflow in agent.workflows.loaded_snapshots()? {
            let view = serde_json::from_value(serde_json::to_value(workflow).map_err(|_| AgentError::internal())?).map_err(|_| AgentError::internal())?;
            add_workflow(&mut items, view);
        }
        agent.companion_recent.apply_acknowledgements(&mut items)?;
        Ok(limit(items))
    }).await.map_err(|_| AgentError::internal())?
}

pub(crate) async fn open_conversation(
    app: tauri::AppHandle,
    conversation_id: String,
) -> Result<(), AgentError> {
    library::select_library_item(
        app.clone(),
        app.state::<AppState>(),
        library::LibraryTarget::Conversation(conversation_id),
    )
    .await?;
    let _ = app.emit("library:changed", ());
    if let Some(window) = app.get_webview_window("main") {
        window
            .show()
            .and_then(|_| window.unminimize())
            .and_then(|_| window.set_focus())
            .map_err(|_| AgentError::internal())?;
    }
    Ok(())
}

pub(crate) async fn acknowledge_item(
    app: tauri::AppHandle,
    conversation_id: String,
    agent_id: Option<String>,
    attention_id: String,
    revision: Option<u64>,
) -> Result<Snapshot, AgentError> {
    let current = snapshot(app.clone()).await?;
    if acknowledge_matching(
        &app.state::<AgentState>().companion_recent,
        &current.items,
        &conversation_id,
        agent_id.as_deref(),
        &attention_id,
        revision,
    )? {
        changed(&app);
    }
    snapshot(app).await
}

fn acknowledge_matching(
    recent: &Recent,
    items: &[Item],
    conversation_id: &str,
    agent_id: Option<&str>,
    attention_id: &str,
    revision: Option<u64>,
) -> Result<bool, AgentError> {
    if let Some(item) = items.iter().find(|item| {
        item.conversation_id == conversation_id
            && item.agent_id.as_deref() == agent_id
            && item.attention_id == attention_id
            && revision.is_none_or(|revision| revision == item.attention_generation)
            && matches!(item.status, Status::Completed | Status::Failed)
    }) {
        recent.acknowledge(item)?;
        return Ok(true);
    }
    Ok(false)
}

pub(crate) async fn answer_question(
    app: tauri::AppHandle,
    conversation_id: String,
    agent_id: Option<String>,
    turn_id: String,
    tool_id: String,
    response: QuestionResponse,
) -> Result<(), AgentError> {
    match agent_id.filter(|id| id != "main") {
        Some(id) => {
            workflow::answer_workflow_question(
                app.state::<AgentState>(),
                conversation_id,
                id,
                turn_id,
                tool_id,
                response,
            )
            .await
        }
        None => questions::answer_agent_question(
            app.state::<AgentState>(),
            conversation_id,
            turn_id,
            tool_id,
            response,
        )
        .await
        .map(|_| ()),
    }
}

pub(crate) async fn pause_question(
    app: tauri::AppHandle,
    conversation_id: String,
    agent_id: Option<String>,
    turn_id: String,
    tool_id: String,
) -> Result<(), AgentError> {
    match agent_id.filter(|id| id != "main") {
        Some(id) => workflow::pause_workflow_question(
            app.state::<AgentState>(),
            conversation_id,
            id,
            turn_id,
            tool_id,
        ),
        None => questions::pause_agent_question(
            app.state::<AgentState>(),
            conversation_id,
            turn_id,
            tool_id,
        )
        .map(|_| ()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::tests::{options, session, Fixture};

    fn running() -> (Fixture, Arc<Session>, ChatSnapshot) {
        let fixture = Fixture::new();
        let session = session(&fixture);
        session
            .reserve(
                "Private prompt that must not reach the desktop".into(),
                options(ApprovalMode::Yolo),
            )
            .unwrap();
        let snapshot = session.snapshot().unwrap();
        (fixture, session, snapshot)
    }

    fn question() -> questions::PendingQuestion {
        questions::PendingQuestion {
            turn_id: "turn-question".into(),
            tool_id: "tool-question".into(),
            questions: serde_json::from_value(json!([{"id":"target","question":"Qual ambiente?","options":[{"label":"Desenvolvimento","recommended":true},{"label":"Produção"}]}])).unwrap(),
            deadline_at: Some(60_000),
        }
    }

    #[test]
    fn desktop_projection_shows_active_task_and_excludes_prompt_and_tool_arguments() {
        let (_fixture, _session, mut snapshot) = running();
        snapshot.turns[0].duration_ms = 2_500;
        snapshot.turns[0].active_since = Some(10_000);
        snapshot.turns[0].tasks = vec![tasks::Task {
            id: "ui".into(),
            title: "Ajustando a interface".into(),
            status: tasks::Status::InProgress,
        }];
        let item = project(&snapshot).unwrap().item;
        assert_eq!(item.status, Status::Running);
        assert_eq!(item.activity, "Ajustando a interface");
        assert_eq!(item.duration_ms, 2_500);
        assert_eq!(item.active_since, Some(10_000));
        assert!(!serde_json::to_string(&item)
            .unwrap()
            .contains("Private prompt"));
    }

    #[test]
    fn waiting_freezes_the_active_clock_and_preserves_question_routing() {
        let (_fixture, _session, mut snapshot) = running();
        snapshot.turns[0].active_since = Some(10_000);
        snapshot.pending_question = Some(question());
        let item = project(&snapshot).unwrap().item;
        assert_eq!(item.status, Status::Waiting);
        assert_eq!(item.active_since, None);
        let question = item.pending_question.unwrap();
        assert_eq!(question.turn_id, "turn-question");
        assert_eq!(question.tool_id, "tool-question");
        assert_eq!(question.questions[0].options[0].label, "Desenvolvimento");
        assert!(question.questions[0].options[0].recommended);
        assert_eq!(question.deadline_at, Some(60_000));
        assert_eq!(
            serde_json::to_value(&question).unwrap()["deadlineAt"],
            60_000
        );

        snapshot.pending_question.as_mut().unwrap().deadline_at = None;
        let paused = project(&snapshot).unwrap().item.pending_question.unwrap();
        assert_eq!(paused.deadline_at, None);
        assert!(serde_json::to_value(paused)
            .unwrap()
            .get("deadlineAt")
            .is_none());
    }

    #[test]
    fn completion_exposes_only_a_bounded_final_response_not_the_request() {
        let (_fixture, _session, mut snapshot) = running();
        snapshot.active_turn_id = None;
        snapshot.turns[0].status = TurnStatus::Completed;
        snapshot.turns[0].steps = vec![
            Step {
                text: "Vou verificar o calendário.".into(),
                ..Step::default()
            },
            Step {
                text: format!(
                    "O próximo feriado é em 12 de outubro. {}",
                    "Detalhes ".repeat(80)
                ),
                ..Step::default()
            },
            Step::default(),
        ];
        let item = project(&snapshot).unwrap().item;
        assert_eq!(item.activity, "Resposta pronta");
        let result = item.result.as_deref().unwrap();
        assert!(result.starts_with("O próximo feriado é em 12 de outubro."));
        assert_eq!(result.chars().count(), 320);
        assert!(!serde_json::to_string(&item)
            .unwrap()
            .contains("Private prompt"));
        snapshot.active_turn_id = Some(snapshot.turns[0].id.clone());
        assert!(project(&snapshot).unwrap().item.result.is_none());
    }

    #[test]
    fn visual_questions_and_tool_approvals_require_the_full_conversation() {
        let (_fixture, _session, mut snapshot) = running();
        let mut request = question();
        request.questions = serde_json::from_value(json!([{"id":"layout","question":"Qual layout?","options":[{"label":"A","preview":{"type":"ascii","text":"private-preview"}}]}])).unwrap();
        snapshot.pending_question = Some(request);
        let item = project(&snapshot).unwrap().item;
        assert!(item.requires_conversation);
        assert!(item.pending_question.is_none());
        assert!(!serde_json::to_string(&item)
            .unwrap()
            .contains("private-preview"));
        snapshot.pending_question = None;
        snapshot.pending_approval = Some(PendingApproval {
            tool: ToolCall {
                id: "tool".into(),
                name: "bash".into(),
                args: json!({"command":"private command"}),
                status: "pending".into(),
                output: String::new(),
                duration_ms: 0,
            },
            policy: None,
        });
        let item = project(&snapshot).unwrap().item;
        assert_eq!(item.status, Status::Waiting);
        assert!(item.requires_conversation);
        assert!(!serde_json::to_string(&item)
            .unwrap()
            .contains("private command"));
    }

    #[test]
    fn retries_are_activity_and_completion_waits_for_the_queue() {
        let (_fixture, _session, mut snapshot) = running();
        snapshot.turns[0].steps.push(Step {
            retry: Some(provider::retry::Status {
                attempt: 2,
                max_attempts: 5,
                retry_at: 1,
                message: "Offline".into(),
            }),
            ..Step::default()
        });
        assert_eq!(
            project(&snapshot).unwrap().item.status,
            Status::Reconnecting
        );
        snapshot.active_turn_id = None;
        snapshot.turns[0].status = TurnStatus::Completed;
        assert_eq!(project(&snapshot).unwrap().item.status, Status::Completed);
        snapshot.queued_messages = serde_json::from_value(
            json!([{"id":"queued","content":"next request","options":options(ApprovalMode::Yolo)}]),
        )
        .unwrap();
        assert_eq!(project(&snapshot).unwrap().item.status, Status::Running);
        snapshot.turns[0].status = TurnStatus::Error;
        assert_eq!(project(&snapshot).unwrap().item.status, Status::Failed);
    }

    #[test]
    fn recent_results_survive_session_eviction_and_reject_stale_events() {
        let (fixture, session, _) = running();
        finish(&session, Ok(()));
        let recent = Recent::default();
        let completed = project(&session.snapshot().unwrap()).unwrap();
        recent.record(completed.clone()).unwrap();
        drop(session);
        drop(fixture);
        let mut stale = completed;
        stale.revision = stale.revision.saturating_sub(1);
        stale.item.status = Status::Running;
        recent.record(stale).unwrap();
        assert_eq!(recent.items().unwrap()[0].status, Status::Completed);
        for index in 0..MAX_RECENT + 2 {
            let mut value = recent.items().unwrap()[0].clone();
            value.conversation_id = format!("conversation-{index}");
            value.updated_at = index as u64;
            recent
                .record(Record {
                    revision: index as u64 + 1,
                    turn_id: "cached-turn".into(),
                    identity: None,
                    item: value,
                })
                .unwrap();
        }
        assert_eq!(recent.items().unwrap().len(), MAX_RECENT);
    }

    #[test]
    fn viewing_an_outcome_clears_attention_without_erasing_it_or_hiding_later_failures() {
        let (_fixture, _session, mut snapshot) = running();
        snapshot.active_turn_id = None;
        snapshot.turns[0].status = TurnStatus::Completed;
        let recent = Recent::default();
        let completed = project(&snapshot).unwrap();
        assert_eq!(
            serde_json::to_value(&completed.item).unwrap()["revision"],
            snapshot.revision
        );
        recent.record(completed.clone()).unwrap();
        recent.acknowledge(&completed.item).unwrap();
        // A passive refresh and a later revision of the same outcome preserve acknowledgement.
        snapshot.revision += 1;
        recent.record(project(&snapshot).unwrap()).unwrap();
        let mut items = recent.items().unwrap();
        recent.apply_acknowledgements(&mut items).unwrap();
        assert_eq!(items[0].status, Status::Completed);
        assert!(items[0].acknowledged);

        snapshot.revision += 1;
        snapshot.turns[0].status = TurnStatus::Error;
        recent.record(project(&snapshot).unwrap()).unwrap();
        let mut items = recent.items().unwrap();
        recent.apply_acknowledgements(&mut items).unwrap();
        assert_eq!(items[0].status, Status::Failed);
        assert!(!items[0].acknowledged);
        // A delayed acknowledgement for the old outcome must not suppress the new failure.
        recent.acknowledge(&completed.item).unwrap();
        recent.apply_acknowledgements(&mut items).unwrap();
        assert!(!items[0].acknowledged);

        let failed = items[0].clone();
        recent.acknowledge(&failed).unwrap();
        snapshot.revision += 1;
        snapshot.active_turn_id = Some(snapshot.turns[0].id.clone());
        recent.record(project(&snapshot).unwrap()).unwrap();
        snapshot.revision += 1;
        snapshot.active_turn_id = None;
        recent.record(project(&snapshot).unwrap()).unwrap();
        let mut items = recent.items().unwrap();
        // Even a fast retry without an intervening desktop read is a new outcome.
        recent.apply_acknowledgements(&mut items).unwrap();
        assert_eq!(items[0].status, Status::Failed);
        assert!(!items[0].acknowledged);
        assert_ne!(items[0].attention_id, failed.attention_id);
        recent.acknowledge(&items[0]).unwrap();
        // Neither a stale read nor a delayed old acknowledgement can undo the current one.
        let mut stale = vec![failed.clone()];
        recent.apply_acknowledgements(&mut stale).unwrap();
        recent.acknowledge(&failed).unwrap();
        recent.apply_acknowledgements(&mut items).unwrap();
        assert!(items[0].acknowledged);
    }

    #[test]
    fn acknowledgements_are_specific_to_a_child_execution_and_never_dismiss_questions() {
        let (_fixture, _session, snapshot) = running();
        let root = project(&snapshot).unwrap().item;
        let recent = Recent::default();
        let view = |updated_at, revision| {
            serde_json::from_value(json!({"conversationId":root.conversation_id,"revision":revision,"agents":[{
            "id":"designer","role":"designer","status":"failed","title":"Revise a tela","updatedAt":updated_at,"durationMs":3_000,"activeSince":null,"currentThought":null,"activeTurnId":null,"pendingQuestion":null,"pendingApproval":null,"pendingAuthoring":null,"identity":null,"error":"Offline"
        }]})).unwrap()
        };
        let mut items = vec![root.clone()];
        add_workflow(&mut items, view(1, 1));
        let displayed = items[1].clone();
        let mut refreshed = vec![root.clone()];
        add_workflow(&mut refreshed, view(1, 2));
        assert!(acknowledge_matching(
            &recent,
            &refreshed,
            &displayed.conversation_id,
            displayed.agent_id.as_deref(),
            &displayed.attention_id,
            Some(displayed.attention_generation)
        )
        .unwrap());
        recent.apply_acknowledgements(&mut items).unwrap();
        assert!(items[1].acknowledged);
        let mut next = vec![root.clone()];
        add_workflow(&mut next, view(2, 3));
        recent.apply_acknowledgements(&mut next).unwrap();
        assert!(!next[1].acknowledged);
        recent.acknowledge(&next[1]).unwrap();
        recent.acknowledge(&items[1]).unwrap();
        recent.apply_acknowledgements(&mut next).unwrap();
        assert!(next[1].acknowledged);

        let mut waiting = root;
        waiting.status = Status::Waiting;
        recent.acknowledge(&waiting).unwrap();
        let mut items = vec![waiting];
        recent.apply_acknowledgements(&mut items).unwrap();
        assert!(!items[0].acknowledged);
    }

    #[test]
    fn opening_attention_only_acknowledges_the_displayed_outcome_and_revision() {
        let (_fixture, session, _) = running();
        finish(&session, Ok(()));
        let item = project(&session.snapshot().unwrap()).unwrap().item;
        let recent = Recent::default();
        let mut items = vec![item.clone()];
        assert!(!acknowledge_matching(
            &recent,
            &items,
            &item.conversation_id,
            None,
            &item.attention_id,
            Some(item.attention_generation + 1)
        )
        .unwrap());
        assert!(!acknowledge_matching(
            &recent,
            &items,
            &item.conversation_id,
            Some("designer"),
            &item.attention_id,
            Some(item.attention_generation)
        )
        .unwrap());
        assert!(!acknowledge_matching(
            &recent,
            &items,
            &item.conversation_id,
            None,
            "older-outcome",
            Some(item.attention_generation)
        )
        .unwrap());
        recent.apply_acknowledgements(&mut items).unwrap();
        assert!(!items[0].acknowledged);
        assert!(acknowledge_matching(
            &recent,
            &items,
            &item.conversation_id,
            None,
            &item.attention_id,
            Some(item.attention_generation)
        )
        .unwrap());
        recent.apply_acknowledgements(&mut items).unwrap();
        assert!(items[0].acknowledged);
        let mut next = item.clone();
        next.attention_id = "new-outcome".into();
        next.attention_generation += 1;
        items.push(next);
        recent.apply_acknowledgements(&mut items).unwrap();
        assert!(!items[1].acknowledged);
        let mut waiting = item;
        waiting.status = Status::Waiting;
        assert!(!acknowledge_matching(
            &recent,
            std::slice::from_ref(&waiting),
            &waiting.conversation_id,
            None,
            &waiting.attention_id,
            None
        )
        .unwrap());
    }

    #[test]
    fn completed_run_with_manual_validation_is_attention_instead_of_success() {
        let (_fixture, session, _) = running();
        finish(&session, Ok(()));
        let recent = Recent::default();
        let completed = project(&session.snapshot().unwrap()).unwrap();
        recent.record(completed.clone()).unwrap();
        recent.validation(&session.id);
        recent.record(completed).unwrap();
        let item = recent.items().unwrap().pop().unwrap();
        assert_eq!(item.status, Status::Waiting);
        assert_eq!(item.activity, "Validação disponível");
        assert!(item.requires_conversation);
        assert_eq!(item.active_since, None);
    }

    #[test]
    fn workflow_questions_route_to_the_actual_child_and_limit_prioritizes_attention() {
        let (_fixture, _session, snapshot) = running();
        let root = project(&snapshot).unwrap().item;
        let mut items = vec![root.clone(); MAX_ITEMS];
        let view = serde_json::from_value(json!({"conversationId":root.conversation_id,"agents":[{
            "id":"actual-child","role":"designer","status":"waiting","title":"Revise a tela","updatedAt":1,"durationMs":3_000,"activeSince":9_000,"currentThought":null,"activeTurnId":"turn-question","pendingQuestion":question(),"pendingApproval":null,"pendingAuthoring":null,"identity":{"name":"Designer UI"},"error":null
        }]})).unwrap();
        add_workflow(&mut items, view);
        let result = limit(items);
        assert!(result.truncated);
        assert_eq!(result.items.len(), MAX_ITEMS);
        let child = &result.items[0];
        assert_eq!(child.agent_id.as_deref(), Some("actual-child"));
        assert_eq!(child.conversation_id, root.conversation_id);
        assert_eq!(child.role, "Designer UI");
        assert_eq!(child.active_since, None);
        assert_eq!(
            child.pending_question.as_ref().unwrap().tool_id,
            "tool-question"
        );
        assert_eq!(
            child.pending_question.as_ref().unwrap().deadline_at,
            Some(60_000)
        );
    }

    #[test]
    fn old_failures_cannot_evict_current_execution_from_the_bounded_view() {
        let (_fixture, _session, snapshot) = running();
        let running = project(&snapshot).unwrap().item;
        let mut failed = running.clone();
        failed.status = Status::Failed;
        failed.updated_at = 0;
        let mut items = vec![failed; MAX_ITEMS];
        items.push(running);
        let view = limit(items);
        assert!(view.truncated);
        assert_eq!(view.items[0].status, Status::Running);
    }

    #[test]
    fn custom_identity_survives_loaded_workflow_eviction_and_does_not_leak_to_a_new_turn() {
        for identity in ["Meu agente", "Meu fluxo"] {
            let (_fixture, _session, mut snapshot) = running();
            snapshot.turns[0].options.workflow = Some(workflow::Flow::Custom);
            let recent = Recent::default();
            recent.record(project(&snapshot).unwrap()).unwrap();
            // Identity can become available after the same revision was observed.
            recent
                .record(
                    project(&snapshot)
                        .unwrap()
                        .with_identity(Some(identity.into())),
                )
                .unwrap();
            assert_eq!(recent.items().unwrap()[0].role, identity);

            snapshot.revision += 1;
            snapshot.active_turn_id = None;
            snapshot.turns[0].status = TurnStatus::Completed;
            recent.record(project(&snapshot).unwrap()).unwrap();
            let completed = recent.items().unwrap().pop().unwrap();
            assert_eq!(completed.status, Status::Completed);
            assert_eq!(completed.role, identity);

            snapshot.revision += 1;
            snapshot.turns[0].id = "new-turn".into();
            recent.record(project(&snapshot).unwrap()).unwrap();
            assert_eq!(recent.items().unwrap()[0].role, "Customizado");
        }
    }
}
