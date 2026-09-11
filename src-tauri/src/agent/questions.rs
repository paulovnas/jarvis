use super::{
    cancelled, journal, next_revision, AgentError, AgentState, ChatSnapshot, Session, ToolCall,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashSet;
use tokio::sync::{oneshot, watch};
mod visual;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct QuestionOption {
    label: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    preview: Option<visual::Preview>,
    #[serde(default)]
    recommended: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Question {
    id: String,
    question: String,
    #[serde(default)]
    options: Vec<QuestionOption>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    questions: Vec<Question>,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingQuestion {
    pub(super) turn_id: String,
    pub(super) tool_id: String,
    pub(super) questions: Vec<Question>,
    pub(super) deadline_at: u64,
}
pub(super) struct Pending {
    pub request: PendingQuestion,
    started: std::time::Instant,
    reply: oneshot::Sender<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Answer {
    id: String,
    value: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    selected_label: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Response {
    cancelled: bool,
    answers: Vec<Answer>,
}
fn invalid(message: &str) -> AgentError {
    AgentError::new("invalid_question", message)
}
fn bounded(value: &str, maximum: usize) -> bool {
    !value.trim().is_empty() && value.chars().count() <= maximum
}
fn parse_request(args: &Value) -> Result<Request, AgentError> {
    let request: Request = serde_json::from_value(args.clone())
        .map_err(|_| invalid("Formato de perguntas inválido."))?;
    if !(1..=3).contains(&request.questions.len()) {
        return Err(invalid("Envie de uma a três perguntas por vez."));
    }
    let mut ids = HashSet::new();
    for question in &request.questions {
        if !bounded(&question.id, 64)
            || !ids.insert(&question.id)
            || !bounded(&question.question, 1000)
            || question.options.len() > 6
        {
            return Err(invalid(
                "Perguntas devem ter identificadores únicos, texto curto e até seis opções.",
            ));
        }
        let mut labels = HashSet::new();
        let mut recommended = 0;
        for option in &question.options {
            recommended += usize::from(option.recommended);
            if option
                .preview
                .as_ref()
                .is_some_and(|preview| !preview.valid())
            {
                return Err(invalid(
                    "Prévia visual inválida. Verifique os limites e o formato.",
                ));
            }
            if !bounded(&option.label, 200)
                || !labels.insert(option.label.trim())
                || option
                    .description
                    .as_ref()
                    .is_some_and(|text| text.chars().count() > 500)
            {
                return Err(invalid("As opções devem ser curtas e distintas."));
            }
        }
        if recommended > 1 {
            return Err(invalid(
                "Cada pergunta pode ter apenas uma opção recomendada.",
            ));
        }
    }
    Ok(request)
}
fn validate_response(request: &PendingQuestion, response: &Response) -> Result<(), AgentError> {
    if response.cancelled {
        return if response.answers.is_empty() {
            Ok(())
        } else {
            Err(invalid(
                "Uma solicitação cancelada não pode conter respostas.",
            ))
        };
    }
    if response.answers.len() != request.questions.len() {
        return Err(invalid("Responda todas as perguntas antes de enviar."));
    }
    let mut ids = HashSet::new();
    for answer in &response.answers {
        let question = request
            .questions
            .iter()
            .find(|question| question.id == answer.id)
            .ok_or_else(|| invalid("Esta resposta não corresponde às perguntas abertas."))?;
        if !ids.insert(&answer.id) || !bounded(&answer.value, 4000) {
            return Err(invalid(
                "Cada pergunta precisa de uma resposta válida, com até 4.000 caracteres.",
            ));
        }
        if let Some(label) = &answer.selected_label {
            if answer.value != *label
                || !question.options.iter().any(|option| option.label == *label)
            {
                return Err(invalid(
                    "A opção escolhida não foi oferecida nesta pergunta.",
                ));
            }
        }
    }
    Ok(())
}
fn recommended_response(request: &Request) -> Option<Response> {
    let answers = request
        .questions
        .iter()
        .map(|question| {
            let option = question.options.iter().find(|option| option.recommended)?;
            Some(Answer {
                id: question.id.clone(),
                value: option.label.clone(),
                selected_label: Some(option.label.clone()),
            })
        })
        .collect::<Option<Vec<_>>>()?;
    Some(Response {
        cancelled: false,
        answers,
    })
}
pub(super) fn definition() -> Value {
    json!({
        "type": "function", "name": "ask_user",
        "description": "Ask the user 1-3 concise clarification questions and wait for their answers in the Jarvis UI. Use this instead of listing questions/options in chat when missing preferences, requirements or decisions materially affect the task and cannot be resolved from available evidence. Write questions and choices in the response language configured by the shared system instructions. Supply 0-6 distinct choices per question; descriptions are optional and should only explain useful tradeoffs. Mark exactly one option as recommended whenever choices are supplied: after the user's configured countdown, Jarvis may automatically use the recommendation for unanswered questions. The UI always includes a free-text answer: do not add Other/manual answer options. A cancelled response means the user did not answer: do not invent an answer or treat it as authorization. This tool collects user input; it does not replace tool execution approvals.",
        "parameters": {
            "type": "object", "additionalProperties": false, "required": ["questions"],
            "properties": {"questions": {"type": "array", "minItems": 1, "maxItems": 3,
                "items": {"type": "object", "additionalProperties": false, "required": ["id", "question"],
                    "properties": {
                        "id": {"type": "string", "minLength": 1, "maxLength": 64},
                        "question": {"type": "string", "minLength": 1, "maxLength": 1000},
                        "options": {"type": "array", "maxItems": 6, "items": {
                            "type": "object", "additionalProperties": false, "required": ["label"],
                            "properties": {"label": {"type": "string", "minLength": 1, "maxLength": 200}, "description": {"type": "string", "maxLength": 500}, "recommended": {"type": "boolean", "description": "Mark one option per question as the recommended default."}, "preview": visual::schema()}
                        }}
                    }
                }
            }}
        }
    })
}
pub(super) fn cancelled_output() -> String {
    json!({"cancelled": true, "answers": []}).to_string()
}
pub(super) async fn execute(
    session: &Session,
    tool: &ToolCall,
    mut signal: watch::Receiver<bool>,
    timeout_seconds: u16,
) -> Result<String, AgentError> {
    let request = parse_request(&tool.args)?;
    if *signal.borrow() {
        return Err(AgentError::cancelled());
    }
    let mut automatic = recommended_response(&request);
    let (reply, received) = oneshot::channel();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX);
    let deadline_at = now.saturating_add(u64::from(timeout_seconds) * 1_000);
    let mut turn_id = String::new();
    session.update(true, |data| {
        if let Some(active) = &mut data.active {
            turn_id.clone_from(&active.id);
            active.question = Some(Pending {
                request: PendingQuestion {
                    turn_id: active.id.clone(),
                    tool_id: tool.id.clone(),
                    questions: request.questions,
                    deadline_at,
                },
                started: std::time::Instant::now(),
                reply,
            });
        }
    })?;
    let timeout = tokio::time::sleep(std::time::Duration::from_secs(u64::from(timeout_seconds)));
    tokio::pin!(timeout);
    tokio::select! {
        biased;
        _ = cancelled(&mut signal) => Err(AgentError::cancelled()),
        result = received => result.map_err(|_| AgentError::cancelled()),
        _ = &mut timeout, if automatic.is_some() => {
            let response = automatic.take().ok_or_else(AgentError::internal)?;
            let output = serde_json::to_string(&response).map_err(|_| AgentError::internal())?;
            answer(session, &turn_id, &tool.id, response)?;
            Ok(output)
        },
    }
}
#[tauri::command]
pub fn answer_agent_question(
    agent: tauri::State<'_, AgentState>,
    conversation_id: String,
    turn_id: String,
    tool_id: String,
    response: Response,
) -> Result<ChatSnapshot, AgentError> {
    let session = agent.existing(&conversation_id)?;
    answer(&session, &turn_id, &tool_id, response)
}
pub(super) fn answer(
    session: &Session,
    turn_id: &str,
    tool_id: &str,
    response: Response,
) -> Result<ChatSnapshot, AgentError> {
    let mut data = session.data.lock().map_err(|_| AgentError::internal())?;
    if data.storage_failed {
        return Err(AgentError::storage());
    }
    let pending = data
        .active
        .as_ref()
        .filter(|active| active.id == turn_id && !*active.cancel.borrow())
        .and_then(|active| active.question.as_ref())
        .filter(|pending| pending.request.tool_id == tool_id)
        .ok_or_else(|| {
            AgentError::new(
                "stale_question",
                "Esta solicitação de perguntas não está mais ativa.",
            )
        })?;
    validate_response(&pending.request, &response)?;
    let output = serde_json::to_string(&response).map_err(|_| AgentError::internal())?;
    let elapsed = pending.started.elapsed().as_millis() as u64;
    // Acknowledge only after both the visible answer and provider result are durable.
    let previous = data
        .turns
        .last()
        .filter(|turn| turn.turn.id == turn_id)
        .cloned()
        .ok_or_else(AgentError::internal)?;
    let mut current = previous.clone();
    let tool = current
        .turn
        .steps
        .iter_mut()
        .flat_map(|step| &mut step.tools)
        .find(|tool| tool.id == tool_id && tool.name == "ask_user")
        .ok_or_else(AgentError::internal)?;
    tool.output = output.clone();
    tool.status = "completed".into();
    tool.duration_ms = elapsed;
    current
        .wire
        .push(json!({"type":"function_call_output", "call_id":tool_id, "output":output}));
    if journal::append_update(&session.journal, &previous, &current).is_err() {
        data.storage_failed = true;
        if let Some(active) = &data.active {
            let _ = active.cancel.send(true);
        }
        return Err(AgentError::storage());
    }
    *data.turns.last_mut().ok_or_else(AgentError::internal)? = current;
    let pending = data
        .active
        .as_mut()
        .and_then(|active| active.question.take())
        .ok_or_else(AgentError::internal)?;
    data.revision = next_revision();
    let snapshot = session.snapshot_data(&data);
    drop(data);
    (session.emit)(snapshot.clone());
    let _ = pending.reply.send(output);
    Ok(snapshot)
}

#[cfg(test)]
mod tests;
