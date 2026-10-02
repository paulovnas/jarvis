//! The LAN client uses the desktop's sessions and commands; it owns no agent loop.
use super::*;
use futures_util::{stream, StreamExt};
use serde::de::DeserializeOwned;

#[derive(Clone, Default)]
pub(crate) struct RuntimeState {
    discovered: Arc<Mutex<HashMap<String, u64>>>,
    discovering: Arc<AtomicBool>,
    pending_validation: Arc<Mutex<HashSet<String>>>,
    failed_discovery: Arc<Mutex<Vec<String>>>,
}

impl RuntimeState {
    fn observe(&self, id: &str, workflow: &Value) -> Result<(), AgentError> {
        let mut pending = self
            .pending_validation
            .lock()
            .map_err(|_| AgentError::internal())?;
        if pending_validation(workflow) {
            pending.insert(id.into());
        } else {
            pending.remove(id);
        }
        Ok(())
    }

    fn discovery_candidates(
        &self,
        ids: &HashSet<String>,
        finished: &HashMap<String, u64>,
    ) -> Result<HashSet<String>, AgentError> {
        let mut known = self.discovered.lock().map_err(|_| AgentError::internal())?;
        known.retain(|id, _| ids.contains(id));
        let mut candidates: HashSet<_> = std::mem::take(
            &mut *self
                .failed_discovery
                .lock()
                .map_err(|_| AgentError::internal())?,
        )
        .into_iter()
        .filter(|id| ids.contains(id))
        .collect();
        for id in ids {
            let revision = finished.get(id).copied().unwrap_or(0);
            if !known.contains_key(id) || revision > known[id] {
                candidates.insert(id.clone());
                known.insert(id.clone(), revision);
            }
        }
        Ok(candidates)
    }

    fn discover(
        &self,
        app: &tauri::AppHandle,
        ids: &HashSet<String>,
        finished: &HashMap<String, u64>,
    ) -> Result<(), AgentError> {
        if self.discovering.swap(true, Ordering::AcqRel) {
            return Ok(());
        }
        let candidates = match self.discovery_candidates(ids, finished) {
            Ok(candidates) => candidates,
            Err(error) => {
                self.discovering.store(false, Ordering::Release);
                return Err(error);
            }
        };
        if candidates.is_empty() {
            self.discovering.store(false, Ordering::Release);
            return Ok(());
        }
        let app = app.clone();
        let state = self.clone();
        tauri::async_runtime::spawn(async move {
            let mut scans = stream::iter(candidates)
                .map(|id| {
                    let app = app.clone();
                    async move {
                        let result = std::panic::AssertUnwindSafe(workflow_view(&app, id.clone()))
                            .catch_unwind()
                            .await
                            .unwrap_or_else(|_| Err(AgentError::internal()));
                        (id, result)
                    }
                })
                .buffer_unordered(4);
            let mut failed = vec![];
            while let Some((id, result)) = scans.next().await {
                if result.and_then(|view| state.observe(&id, &view)).is_err() {
                    failed.push(id);
                }
            }
            if let Ok(mut retry) = state.failed_discovery.lock() {
                *retry = failed;
            }
            state.discovering.store(false, Ordering::Release);
        });
        Ok(())
    }
}

fn invalid() -> AgentError {
    AgentError::new(
        "remote_invalid_request",
        "A solicitação remota é inválida. Atualize a página e tente novamente.",
    )
}

fn decode<T: DeserializeOwned>(params: Value) -> Result<T, AgentError> {
    serde_json::from_value(params).map_err(|_| invalid())
}

fn encode(value: impl Serialize) -> Result<Value, AgentError> {
    serde_json::to_value(value).map_err(|_| AgentError::internal())
}

fn identifier<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<String, D::Error> {
    let value = String::deserialize(deserializer)?;
    if value.is_empty() || value.len() > 200 || value.chars().any(char::is_control) {
        return Err(serde::de::Error::custom("invalid identifier"));
    }
    Ok(value)
}

fn root_agent() -> String {
    "main".into()
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Empty {}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Conversation {
    #[serde(deserialize_with = "identifier")]
    conversation_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct History {
    #[serde(deserialize_with = "identifier")]
    conversation_id: String,
    before: Option<usize>,
    after: Option<usize>,
    around: Option<usize>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Message {
    #[serde(deserialize_with = "identifier")]
    conversation_id: String,
    content: String,
    options: Option<TurnOptions>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Question {
    #[serde(deserialize_with = "identifier")]
    conversation_id: String,
    #[serde(default = "root_agent", deserialize_with = "identifier")]
    agent_id: String,
    #[serde(deserialize_with = "identifier")]
    turn_id: String,
    #[serde(deserialize_with = "identifier")]
    tool_id: String,
    #[serde(default)]
    pause: bool,
    response: Option<questions::Response>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Approval {
    #[serde(deserialize_with = "identifier")]
    conversation_id: String,
    #[serde(default = "root_agent", deserialize_with = "identifier")]
    agent_id: String,
    #[serde(deserialize_with = "identifier")]
    turn_id: String,
    #[serde(deserialize_with = "identifier")]
    tool_id: String,
    decision: ApprovalDecision,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ReviewDecision {
    #[serde(deserialize_with = "identifier")]
    turn_id: String,
    #[serde(deserialize_with = "identifier")]
    tool_id: String,
    approved: bool,
    note: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Authoring {
    #[serde(deserialize_with = "identifier")]
    conversation_id: String,
    #[serde(default = "root_agent", deserialize_with = "identifier")]
    agent_id: String,
    decision: ReviewDecision,
}

#[derive(Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
enum Validation {
    Item {
        #[serde(deserialize_with = "identifier")]
        conversation_id: String,
        #[serde(deserialize_with = "identifier")]
        batch_id: String,
        #[serde(deserialize_with = "identifier")]
        item_id: String,
        decision: workflow::validation::Decision,
        reason: Option<String>,
    },
    Submit {
        #[serde(deserialize_with = "identifier")]
        conversation_id: String,
        #[serde(deserialize_with = "identifier")]
        batch_id: String,
    },
    Publication {
        #[serde(deserialize_with = "identifier")]
        conversation_id: String,
        #[serde(default = "root_agent", deserialize_with = "identifier")]
        agent_id: String,
        decision: ReviewDecision,
    },
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Cancel {
    #[serde(deserialize_with = "identifier")]
    conversation_id: String,
    #[serde(deserialize_with = "identifier")]
    turn_id: String,
}

fn inherited_options(chat: &ChatSnapshot) -> Option<TurnOptions> {
    chat.turns
        .iter()
        .rev()
        .find(|turn| chat.active_turn_id.as_ref().is_none_or(|id| turn.id == *id))
        .map(|turn| turn.options.clone())
        .or_else(|| {
            chat.queued_messages
                .last()
                .map(|message| message.options.clone())
        })
}

async fn chat(app: &tauri::AppHandle, conversation_id: String) -> Result<ChatSnapshot, AgentError> {
    get_chat(
        app.clone(),
        app.state::<AppState>(),
        app.state::<AgentState>(),
        app.state::<OpenAiCodexState>(),
        conversation_id,
    )
    .await
}

async fn chat_result(app: &tauri::AppHandle, chat: ChatSnapshot) -> Result<Value, AgentError> {
    let workflow = workflow::get_workflow(
        app.clone(),
        app.state::<AppState>(),
        app.state::<AgentState>(),
        chat.conversation_id.clone(),
    )
    .await?;
    app.state::<RuntimeState>()
        .observe(&chat.conversation_id, &encode(&workflow)?)?;
    let options = inherited_options(&chat);
    Ok(json!({"chat":chat,"workflow":workflow,"options":options}))
}

async fn workflow_view(app: &tauri::AppHandle, id: String) -> Result<Value, AgentError> {
    encode(
        workflow::get_workflow(
            app.clone(),
            app.state::<AppState>(),
            app.state::<AgentState>(),
            id,
        )
        .await?,
    )
}

fn pending_validation(workflow: &Value) -> bool {
    workflow.get("validation").is_some_and(|batch| {
        batch.get("stale").and_then(Value::as_bool) == Some(false)
            && batch.get("submitted").and_then(Value::as_bool) == Some(false)
            && batch
                .get("items")
                .and_then(Value::as_array)
                .is_some_and(|items| !items.is_empty())
    })
}

fn attention(view: &Value, agent_id: &str, result: &mut Vec<Value>) {
    for (field, kind) in [
        ("pendingQuestion", "question"),
        ("pendingApproval", "approval"),
        ("pendingAuthoring", "authoring"),
    ] {
        let Some(pending) = view.get(field).filter(|value| value.is_object()) else {
            continue;
        };
        let kind = if kind == "authoring" && pending["target"]["kind"] == "publication" {
            "publication"
        } else {
            kind
        };
        let turn = if kind == "approval" {
            &view["activeTurnId"]
        } else {
            &pending["turnId"]
        };
        let tool = if kind == "approval" {
            &pending["tool"]["id"]
        } else {
            &pending["toolId"]
        };
        let item = json!({"kind":kind,"agentId":agent_id,"turnId":turn,"toolId":tool});
        if !result.contains(&item) {
            result.push(item);
        }
    }
}

fn summary(id: &str, chat: Option<&Value>, workflow: Option<&Value>) -> Value {
    let mut attention_items = vec![];
    if let Some(chat) = chat {
        attention(chat, "main", &mut attention_items);
    }
    let mut root = None;
    if let Some(workflow) = workflow {
        if let Some(agents) = workflow["agents"].as_array() {
            for agent in agents {
                let Some(agent_id) = agent["id"].as_str() else {
                    continue;
                };
                attention(agent, agent_id, &mut attention_items);
                if agent_id == "main" {
                    root = Some(agent);
                }
            }
        }
        if pending_validation(workflow) {
            attention_items.push(json!({"kind":"validation","agentId":"main","batchId":workflow["validation"]["id"]}));
        }
    }
    let active_turn_id = chat
        .and_then(|chat| chat.get("activeTurnId"))
        .or_else(|| root.and_then(|root| root.get("activeTurnId")));
    let compacting = chat
        .and_then(|chat| chat["compacting"].as_bool())
        .unwrap_or(false);
    let status = if !attention_items.is_empty() {
        "waiting"
    } else if compacting || active_turn_id.is_some_and(|id| id.is_string()) {
        "running"
    } else {
        chat.and_then(|chat| chat["turns"].as_array()?.last()?["status"].as_str())
            .or_else(|| root.and_then(|root| root["status"].as_str()))
            .map(|status| if status == "error" { "failed" } else { status })
            .unwrap_or("idle")
    };
    json!({
        "conversationId":id,
        "revision":chat.and_then(|chat| chat["revision"].as_u64()).unwrap_or(0).max(workflow.and_then(|workflow| workflow["revision"].as_u64()).unwrap_or(0)),
        "activeTurnId":active_turn_id,
        "compacting":compacting,
        "status":status,
        "attention":attention_items,
    })
}

async fn library_result(app: &tauri::AppHandle) -> Result<Value, AgentError> {
    let library =
        encode(library::get_library_snapshot(app.clone(), app.state::<AppState>()).await?)?;
    let visible: HashSet<String> = library["conversations"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|conversation| conversation["id"].as_str().map(str::to_owned))
        .collect();
    let state = app.state::<RuntimeState>().inner().clone();
    let companion = encode(companion::snapshot(app.clone()).await?)?;
    let mut recent = HashMap::new();
    for item in companion["items"].as_array().into_iter().flatten() {
        if let Some(id) = item["conversationId"].as_str() {
            recent.entry(id.to_owned()).or_insert(item);
        }
    }
    let agent = app.state::<AgentState>().inner().clone();
    let (chats, mut workflows) = tauri::async_runtime::spawn_blocking(move || {
        let sessions: Vec<_> = agent
            .sessions
            .lock()
            .map_err(|_| AgentError::internal())?
            .values()
            .cloned()
            .collect();
        let chats = sessions
            .into_iter()
            .map(|session| Ok((session.id.clone(), encode(session.snapshot()?)?)))
            .collect::<Result<HashMap<_, _>, AgentError>>()?;
        let workflows = agent
            .workflows
            .loaded_snapshots()?
            .into_iter()
            .map(|snapshot| {
                let view = encode(snapshot)?;
                Ok((
                    view["conversationId"]
                        .as_str()
                        .ok_or_else(AgentError::internal)?
                        .to_owned(),
                    view,
                ))
            })
            .collect::<Result<HashMap<_, _>, AgentError>>()?;
        Ok::<_, AgentError>((chats, workflows))
    })
    .await
    .map_err(|_| AgentError::internal())??;
    // Finished hubs are removed before a phone may poll. Discover each final revision
    // once, including validations omitted from the companion's 32-item display limit.
    let finished = chats
        .iter()
        .filter(|(id, view)| view["activeTurnId"].is_null() && !workflows.contains_key(*id))
        .map(|(id, view)| (id.clone(), view["revision"].as_u64().unwrap_or(0)))
        .collect();
    state.discover(app, &visible, &finished)?;
    let mut candidates = state
        .pending_validation
        .lock()
        .map_err(|_| AgentError::internal())?
        .clone();
    candidates.extend(
        companion["items"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|item| item["status"] == "waiting")
            .filter_map(|item| item["conversationId"].as_str().map(str::to_owned)),
    );
    candidates.retain(|id| visible.contains(id) && !workflows.contains_key(id));
    let mut views = stream::iter(candidates)
        .map(|id| {
            let app = app.clone();
            async move {
                let view = workflow_view(&app, id.clone()).await;
                (id, view)
            }
        })
        .buffer_unordered(4);
    let mut errors = HashMap::new();
    while let Some((id, view)) = views.next().await {
        match view {
            Ok(view) => {
                workflows.insert(id, view);
            }
            Err(error) => {
                errors.insert(id, error);
            }
        }
    }
    for (id, view) in &workflows {
        state.observe(id, view)?;
    }
    let mut ids: Vec<_> = chats
        .keys()
        .chain(workflows.keys())
        .chain(errors.keys())
        .chain(recent.keys())
        .filter(|id| visible.contains(*id))
        .cloned()
        .collect();
    ids.sort_unstable();
    ids.dedup();
    let runtime: Vec<_> = ids
        .iter()
        .map(|id| {
            let mut view = summary(id, chats.get(id), workflows.get(id));
            if view["status"] == "idle" {
                if let Some(recent) = recent.get(id) {
                    view["status"] = recent["status"].clone();
                }
            }
            if let Some(error) = errors.get(id) {
                view["error"] = json!(error);
                view["status"] = json!("failed");
            }
            view
        })
        .collect();
    let failed = !state
        .failed_discovery
        .lock()
        .map_err(|_| AgentError::internal())?
        .is_empty();
    Ok(
        json!({"library":library,"runtime":runtime,"discoveringAttention":state.discovering.load(Ordering::Acquire),"attentionDiscoveryFailed":failed}),
    )
}

async fn review(app: &tauri::AppHandle, params: Authoring) -> Result<(), AgentError> {
    let decision = decode(encode(params.decision)?)?;
    if params.agent_id == "main" {
        authoring::answer_agent_authoring(
            app.clone(),
            app.state::<AppState>(),
            app.state::<AgentState>(),
            params.conversation_id,
            decision,
        )
        .await
        .map(|_| ())
    } else {
        workflow::answer_workflow_authoring(
            app.clone(),
            app.state::<AppState>(),
            app.state::<AgentState>(),
            params.conversation_id,
            params.agent_id,
            decision,
        )
        .await
    }
}

fn cancel(session: &Session, turn_id: &str) -> Result<(), AgentError> {
    let mut data = session.data.lock().map_err(|_| AgentError::internal())?;
    let active = data
        .active
        .as_mut()
        .filter(|active| active.id == turn_id)
        .ok_or_else(|| {
            AgentError::new(
                "stale_cancel",
                "Esta execução não está mais ativa. Atualize a conversa.",
            )
        })?;
    active.cancel();
    Ok(())
}

pub(crate) async fn dispatch(
    app: tauri::AppHandle,
    method: &str,
    params: Value,
) -> Result<Value, AgentError> {
    match method {
        "library" => {
            let _: Empty = decode(params)?;
            return library_result(&app).await;
        }
        "chat" => {
            let params: Conversation = decode(params)?;
            return chat_result(&app, chat(&app, params.conversation_id).await?).await;
        }
        "history" => {
            let params: History = decode(params)?;
            if [params.before, params.after, params.around]
                .iter()
                .filter(|value| value.is_some())
                .count()
                > 1
            {
                return Err(invalid());
            }
            return encode(
                history::get_chat_history(
                    app.clone(),
                    app.state::<AppState>(),
                    app.state::<AgentState>(),
                    params.conversation_id,
                    params.before,
                    params.after,
                    params.around,
                )
                .await?,
            );
        }
        "message" => {
            let params: Message = decode(params)?;
            if params.content.trim().is_empty() || params.content.len() > 100_000 {
                return Err(AgentError::new(
                    "invalid_message",
                    "Envie uma mensagem entre 1 e 100.000 bytes.",
                ));
            }
            let options = match params.options {
                Some(options) => options,
                None => inherited_options(&chat(&app, params.conversation_id.clone()).await?).ok_or_else(|| {
                    AgentError::new("remote_missing_options", "Envie a primeira mensagem desta conversa no desktop para definir o modelo.")
                })?,
            };
            let snapshot = start_agent_turn(
                app.clone(),
                app.state::<AppState>(),
                app.state::<AgentState>(),
                params.conversation_id,
                params.content,
                options,
                None,
            )
            .await?;
            return chat_result(&app, snapshot).await;
        }
        "question" => {
            let params: Question = decode(params)?;
            if params.pause == params.response.is_some() {
                return Err(invalid());
            }
            if params.pause {
                if params.agent_id == "main" {
                    questions::pause_agent_question(
                        app.state::<AgentState>(),
                        params.conversation_id,
                        params.turn_id,
                        params.tool_id,
                    )?;
                } else {
                    workflow::pause_workflow_question(
                        app.state::<AgentState>(),
                        params.conversation_id,
                        params.agent_id,
                        params.turn_id,
                        params.tool_id,
                    )?;
                }
            } else {
                let response = params.response.ok_or_else(invalid)?;
                if params.agent_id == "main" {
                    questions::answer_agent_question(
                        app.state::<AgentState>(),
                        params.conversation_id,
                        params.turn_id,
                        params.tool_id,
                        response,
                    )
                    .await?;
                } else {
                    workflow::answer_workflow_question(
                        app.state::<AgentState>(),
                        params.conversation_id,
                        params.agent_id,
                        params.turn_id,
                        params.tool_id,
                        response,
                    )
                    .await?;
                }
            }
        }
        "approval" => {
            let params: Approval = decode(params)?;
            if params.agent_id == "main" {
                approve_agent_tool(
                    app.state::<AgentState>(),
                    params.conversation_id,
                    params.turn_id,
                    params.tool_id,
                    params.decision,
                )?;
            } else {
                workflow::approve_workflow_tool(
                    app.state::<AgentState>(),
                    params.conversation_id,
                    params.agent_id,
                    params.turn_id,
                    params.tool_id,
                    params.decision,
                )?;
            }
        }
        "validation" => match decode::<Validation>(params)? {
            Validation::Item {
                conversation_id,
                batch_id,
                item_id,
                decision,
                reason,
            } => {
                workflow::validation::decide_workflow_validation(
                    app.clone(),
                    app.state::<AgentState>(),
                    conversation_id,
                    batch_id,
                    item_id,
                    decision,
                    reason,
                )
                .await?;
            }
            Validation::Submit {
                conversation_id,
                batch_id,
            } => {
                workflow::validation::submit_workflow_validation(
                    app.clone(),
                    app.state::<AppState>(),
                    app.state::<AgentState>(),
                    conversation_id,
                    batch_id,
                )
                .await?;
            }
            Validation::Publication {
                conversation_id,
                agent_id,
                decision,
            } => {
                review(
                    &app,
                    Authoring {
                        conversation_id,
                        agent_id,
                        decision,
                    },
                )
                .await?;
            }
        },
        "authoring" => review(&app, decode(params)?).await?,
        "cancel" => {
            let params: Cancel = decode(params)?;
            let session = app
                .state::<AgentState>()
                .existing(&params.conversation_id)?;
            cancel(&session, &params.turn_id)?;
        }
        _ => {
            return Err(AgentError::new(
                "remote_method_not_found",
                "Esta operação não está disponível no acesso remoto.",
            ))
        }
    }
    Ok(json!({"ok":true}))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::tests::{options, session, Fixture};

    #[test]
    fn attention_discovery_reads_new_and_finished_chats_once_and_retries_failures() {
        let state = RuntimeState::default();
        let mut ids = HashSet::from(["old".to_owned()]);
        let mut finished = HashMap::new();
        assert_eq!(state.discovery_candidates(&ids, &finished).unwrap(), ids);
        assert!(state
            .discovery_candidates(&ids, &finished)
            .unwrap()
            .is_empty());

        ids.insert("new".into());
        assert_eq!(
            state.discovery_candidates(&ids, &finished).unwrap(),
            HashSet::from(["new".to_owned()])
        );
        // A finished run may disappear from the live hub and the compact pet list
        // before the next phone poll. Its changed revision still discovers validation.
        finished.insert("new".into(), 20);
        assert_eq!(
            state.discovery_candidates(&ids, &finished).unwrap(),
            HashSet::from(["new".to_owned()])
        );
        assert!(state
            .discovery_candidates(&ids, &finished)
            .unwrap()
            .is_empty());
        state
            .failed_discovery
            .lock()
            .unwrap()
            .extend(["new".into(), "deleted".into()]);
        assert_eq!(
            state.discovery_candidates(&ids, &finished).unwrap(),
            HashSet::from(["new".to_owned()])
        );
        finished.insert("new".into(), 21);
        assert_eq!(
            state.discovery_candidates(&ids, &finished).unwrap(),
            HashSet::from(["new".to_owned()])
        );
    }

    #[test]
    fn remote_requests_reject_unknown_fields_and_keep_exact_prompt_ids() {
        assert!(decode::<Message>(
            json!({"conversationId":"chat","content":"Oi","model":"injected"})
        )
        .is_err());
        assert!(decode::<Conversation>(json!({"conversationId":""})).is_err());
        assert!(decode::<Question>(
            json!({"conversationId":"chat","turnId":"turn\n","toolId":"tool"})
        )
        .is_err());
        let request: Question = decode(json!({
            "conversationId":"chat", "agentId":"worker", "turnId":"turn-2", "toolId":"call_3",
            "response":{"cancelled":false,"answers":[{"id":"choice","value":"Sim","selectedLabel":"Sim"}]}
        })).unwrap();
        assert_eq!(
            (
                request.agent_id.as_str(),
                request.turn_id.as_str(),
                request.tool_id.as_str()
            ),
            ("worker", "turn-2", "call_3")
        );
        assert!(request.response.is_some());
        assert!(decode::<Approval>(json!({"conversationId":"chat","turnId":"turn","toolId":"tool","decision":{"approved":true,"scope":"project"}})).is_err());
        assert!(decode::<Validation>(
            json!({"kind":"submit","conversationId":"chat","batchId":"batch","approved":true})
        )
        .is_err());
    }

    #[test]
    fn remote_message_inherits_active_settings_and_uses_the_native_queue() {
        let fixture = Fixture::new();
        let session = session(&fixture);
        let mut original = options(ApprovalMode::Manual);
        original.workflow = Some(workflow::Flow::Complete);
        original.manual_validation = true;
        session
            .reserve("Primeiro pedido".into(), original.clone())
            .unwrap();
        let before = session.snapshot().unwrap();
        let inherited = inherited_options(&before).unwrap();
        assert_eq!(
            encode(&inherited).unwrap(),
            encode(&before.turns.last().unwrap().options).unwrap()
        );
        assert!(session
            .submit_message("Continue".into(), inherited, vec![])
            .unwrap()
            .is_none());
        let queued = session.snapshot().unwrap();
        assert_eq!(queued.active_turn_id, before.active_turn_id);
        assert_eq!(queued.turns.len(), 1);
        assert_eq!(queued.queued_messages.len(), 1);
        assert_eq!(queued.queued_messages[0].content, "Continue");
        assert_eq!(queued.queued_messages[0].options.account, original.account);
        assert_eq!(
            queued.queued_messages[0].options.workflow,
            original.workflow
        );
        assert!(queued.queued_messages[0].options.manual_validation);
    }

    #[test]
    fn stale_cancellation_cannot_interrupt_a_different_turn() {
        let fixture = Fixture::new();
        let session = session(&fixture);
        let signal = session
            .reserve("Pedido".into(), options(ApprovalMode::Manual))
            .unwrap();
        assert_eq!(
            cancel(&session, "old-turn").unwrap_err().code,
            "stale_cancel"
        );
        assert!(!*signal.borrow());
        cancel(
            &session,
            session
                .snapshot()
                .unwrap()
                .active_turn_id
                .as_deref()
                .unwrap(),
        )
        .unwrap();
        assert!(*signal.borrow());
    }

    #[test]
    fn remote_permission_decisions_keep_grants_and_reject_stale_tools() {
        let fixture = Fixture::new();
        let session = session(&fixture);
        session
            .reserve("Pedido".into(), options(ApprovalMode::Manual))
            .unwrap();
        let turn = session.snapshot().unwrap().active_turn_id.unwrap();
        let (reply, mut received) = oneshot::channel();
        session
            .data
            .lock()
            .unwrap()
            .active
            .as_mut()
            .unwrap()
            .wait_for_approval(super::super::Approval::new(
                ToolCall {
                    id: "current-tool".into(),
                    name: "bash".into(),
                    args: json!({"command":"pwd"}),
                    status: "pending".into(),
                    output: String::new(),
                    duration_ms: 0,
                },
                None,
                None,
                None,
                reply,
            ));
        let stale: Approval = decode(json!({"conversationId":"conversation","turnId":turn,"toolId":"stale-tool","decision":{"approved":true}})).unwrap();
        let store = execution_grants::GrantStore::default();
        assert_eq!(
            answer_approval_decision(
                &store,
                &session,
                &stale.turn_id,
                &stale.tool_id,
                stale.decision
            )
            .unwrap_err()
            .code,
            "stale_approval"
        );
        assert!(received.try_recv().is_err());
        assert!(session.snapshot().unwrap().pending_approval.is_some());
        let decision: Approval = decode(json!({"conversationId":"conversation","turnId":turn,"toolId":"current-tool","decision":{"approved":true,"grant":{"scope":"project","duration":"session","matchKind":"exact"}}})).unwrap();
        let grant = decision.decision.grant.unwrap();
        assert!(matches!(grant.scope, ApprovalGrantScope::Project));
        assert!(matches!(grant.duration, ApprovalGrantDuration::Session));
        let decision: Approval = decode(json!({"conversationId":"conversation","turnId":turn,"toolId":"current-tool","decision":{"approved":false}})).unwrap();
        answer_approval_decision(
            &store,
            &session,
            &decision.turn_id,
            &decision.tool_id,
            decision.decision,
        )
        .unwrap();
        assert!(!received.try_recv().unwrap());
    }

    #[test]
    fn runtime_summary_keeps_root_worker_and_validation_attention() {
        let chat = json!({"revision":10,"activeTurnId":"root-turn","pendingApproval":{"tool":{"id":"root-tool"}}});
        let workflow = json!({"revision":3,"agents":[
            {"id":"main","activeTurnId":"root-turn","pendingApproval":{"tool":{"id":"root-tool"}}},
            {"id":"worker","activeTurnId":"worker-turn","pendingQuestion":{"turnId":"worker-turn","toolId":"worker-tool"}},
            {"id":"github","pendingAuthoring":{"turnId":"git-turn","toolId":"proposal","target":{"kind":"publication"}}}
        ],"validation":{"id":"batch","submitted":false,"stale":false,"items":[{"id":"item"}]}});
        let view = summary("conversation", Some(&chat), Some(&workflow));
        assert_eq!(view["status"], "waiting");
        assert_eq!(view["revision"], 10);
        assert_eq!(view["attention"].as_array().unwrap().len(), 4);
        assert_eq!(
            view["attention"][1],
            json!({"kind":"question","agentId":"worker","turnId":"worker-turn","toolId":"worker-tool"})
        );
        assert_eq!(view["attention"][2]["kind"], "publication");
        assert_eq!(view["attention"][3]["batchId"], "batch");
        let state = RuntimeState::default();
        state.observe("conversation", &workflow).unwrap();
        assert!(state
            .pending_validation
            .lock()
            .unwrap()
            .contains("conversation"));
        let mut finished = workflow;
        finished["validation"]["submitted"] = json!(true);
        state.observe("conversation", &finished).unwrap();
        assert!(state.pending_validation.lock().unwrap().is_empty());
    }
}
