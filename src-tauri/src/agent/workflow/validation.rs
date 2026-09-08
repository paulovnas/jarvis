//! Human acceptance is separate from technical review and can only be recorded by UI commands.
use super::*;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Decision {
    Pending,
    Approved,
    Rejected,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    pub id: String,
    pub title: String,
    pub steps: Vec<String>,
    pub expected: String,
    pub decision: Decision,
    pub reason: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Batch {
    pub id: String,
    pub flow: Flow,
    pub run_id: String,
    pub epic_ids: Vec<String>,
    pub items: Vec<Item>,
    pub submitted: bool,
    pub stale: bool,
    pub created_at: u64,
}
impl Batch {
    fn ready(&self) -> bool {
        !self.stale
            && !self.items.is_empty()
            && self
                .items
                .iter()
                .all(|item| item.decision != Decision::Pending)
    }
    pub(super) fn approved(&self, flow: Flow, id: &str) -> bool {
        self.flow == flow
            && self.submitted
            && self.ready()
            && self.epic_ids.iter().any(|epic| epic == id)
            && self
                .items
                .iter()
                .all(|item| item.decision == Decision::Approved)
    }
    fn decide(
        &mut self,
        id: &str,
        decision: Decision,
        reason: Option<String>,
    ) -> Result<(), AgentError> {
        if self.submitted || self.stale {
            return Err(invalid("Esta rodada de validação já foi encerrada."));
        }
        if decision == Decision::Pending {
            return Err(invalid("Escolha aprovar ou reprovar."));
        }
        let reason = reason
            .map(|text| text.trim().to_string())
            .filter(|text| !text.is_empty());
        if reason
            .as_ref()
            .is_some_and(|text| text.chars().count() > 4000)
            || decision == Decision::Rejected && reason.is_none()
        {
            return Err(invalid(
                "Informe o motivo da reprovação (até 4.000 caracteres).",
            ));
        }
        let item = self
            .items
            .iter_mut()
            .find(|item| item.id == id)
            .ok_or_else(|| invalid("Item de validação não encontrado."))?;
        item.reason = if decision == Decision::Rejected {
            reason
        } else {
            None
        };
        item.decision = decision;
        Ok(())
    }
    fn feedback(&self) -> String {
        let mut text = String::from("Resultado da validação manual do usuário\n\n");
        for item in &self.items {
            text.push_str(&format!(
                "- {}: {}{}\n",
                if item.decision == Decision::Approved {
                    "APROVADO"
                } else {
                    "REPROVADO"
                },
                item.title,
                item.reason
                    .as_ref()
                    .map_or(String::new(), |reason| format!("\n  Motivo: {reason}"))
            ));
        }
        text.push_str(&format!("\nÉpicos: {}.\n", self.epic_ids.join(", ")));
        text.push_str(if self.items.iter().all(|item| item.decision == Decision::Approved) { "Planejador: confira as evidências técnicas e finalize os épicos aprovados, se não houver outras pendências. Esta aprovação não autoriza commit, push ou publicação." } else { "Planejador: analise as reprovações, encaminhe as correções pelo mesmo fluxo, execute as verificações técnicas e publique uma nova rodada de validação manual." });
        text
    }
}

pub(super) fn definition() -> Value {
    json!({"type":"function","name":"validation_publish","description":"Publish a USER manual acceptance checklist in the Jarvis inspector after implementation and automated unit/lint/typecheck/build checks. Root Planner only in Planned/Complete. Keep epics open. Each item needs a short pt-BR title, concrete steps and expected outcome. Return control to the user after publishing; do not poll or use ask_user for acceptance. User decisions arrive in a later Planner turn. A repair needs a fresh checklist.","parameters":{"type":"object","properties":{"epicIds":{"type":"array","items":{"type":"string"},"minItems":1,"maxItems":8},"items":{"type":"array","minItems":1,"maxItems":20,"items":{"type":"object","properties":{"title":{"type":"string","maxLength":120},"steps":{"type":"array","items":{"type":"string","maxLength":1000},"minItems":1,"maxItems":12},"expected":{"type":"string","maxLength":2000}},"required":["title","steps","expected"],"additionalProperties":false}}},"required":["epicIds","items"],"additionalProperties":false}})
}

async fn bead(
    exec: &Execution,
    id: &str,
    signal: watch::Receiver<bool>,
) -> Result<Value, AgentError> {
    let beads = crate::core::beads::Beads::new(
        &exec.hub.env.home,
        exec.hub.root.project_id()?,
        &exec.hub.root.id,
        true,
    )?;
    let raw = beads
        .execute(
            "beads_show",
            &json!({"id":id}),
            "validation-read",
            signal,
            || {
                library::agent_location(&exec.hub.env.state, &exec.hub.env.home, &exec.hub.root.id)
                    .map(|_| ())
                    .map_err(|_| crate::core::CoreError {
                        code: "beads_error",
                        message: "Conversa indisponível.".into(),
                    })
            },
        )
        .await?;
    let value: Value =
        serde_json::from_str(&raw).map_err(|_| invalid("Resposta inválida do Beads."))?;
    let item = if let Some(rows) = value.as_array() {
        rows.iter()
            .find(|item| item["id"] == id)
            .cloned()
            .unwrap_or(Value::Null)
    } else {
        value
    };
    if item["id"] != id || item["issue_type"].as_str().is_none() {
        return Err(invalid("Não foi possível verificar o item do Beads."));
    }
    Ok(item)
}

pub(super) async fn publish(
    exec: &Execution,
    args: &Value,
    signal: watch::Receiver<bool>,
) -> Result<String, AgentError> {
    if exec.id != "main" || exec.role != Role::Planner || exec.flow.direct() {
        return Err(invalid(
            "Somente o Planejador deste fluxo pode publicar a validação.",
        ));
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct Request {
        epic_ids: Vec<String>,
        items: Vec<Spec>,
    }
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Spec {
        title: String,
        steps: Vec<String>,
        expected: String,
    }
    let request: Request = serde_json::from_value(args.clone())
        .map_err(|_| invalid("Checklist de validação inválido."))?;
    let valid = |text: &str, max| !text.trim().is_empty() && text.chars().count() <= max;
    if request.epic_ids.is_empty()
        || request.epic_ids.len() > 8
        || request.items.is_empty()
        || request.items.len() > 20
        || request.items.iter().any(|item| {
            !valid(&item.title, 120)
                || !valid(&item.expected, 2000)
                || item.steps.is_empty()
                || item.steps.len() > 12
                || item.steps.iter().any(|step| !valid(step, 1000))
        })
    {
        return Err(invalid(
            "Informe de 1 a 20 testes com passos e resultado esperado.",
        ));
    }
    for id in &request.epic_ids {
        let item = bead(exec, id, signal.clone()).await?;
        if item["issue_type"] != "epic" || item["status"] == "closed" {
            return Err(invalid("Vincule somente épicos em aberto deste projeto."));
        }
    }
    exec.hub.mutate(|state| {
        if state
            .jobs
            .values()
            .any(|job| job.run_id == state.run_id && job.status.active())
        {
            return Err(invalid(
                "Aguarde todos os agentes terminarem antes de publicar a validação.",
            ));
        }
        if state
            .validation
            .as_ref()
            .is_some_and(|batch| !batch.submitted && !batch.stale)
        {
            return Err(invalid(
                "Já existe uma rodada aguardando o usuário. Não substitua suas decisões.",
            ));
        }
        let items = request
            .items
            .into_iter()
            .map(|item| {
                Ok(Item {
                    id: library::new_id()?,
                    title: item.title,
                    steps: item.steps,
                    expected: item.expected,
                    decision: Decision::Pending,
                    reason: None,
                })
            })
            .collect::<Result<Vec<_>, AgentError>>()?;
        let batch = Batch {
            id: library::new_id()?,
            flow: exec.flow,
            run_id: state.run_id.clone(),
            epic_ids: request.epic_ids,
            items,
            submitted: false,
            stale: false,
            created_at: now(),
        };
        let result =
            json!({"id":batch.id,"items":batch.items.len(),"status":"awaiting_user"}).to_string();
        state.validation = Some(batch);
        Ok(result)
    })
}

pub(in crate::agent) async fn closure(
    exec: &Execution,
    tool: &ToolCall,
    signal: watch::Receiver<bool>,
) -> Result<(), AgentError> {
    if exec.flow.direct() || tool.name != "beads_close" {
        return Ok(());
    }
    let id = tool.args["id"]
        .as_str()
        .ok_or_else(|| invalid("Informe a tarefa."))?;
    let item = bead(exec, id, signal).await?;
    if item["issue_type"] == "epic" {
        let state = exec
            .hub
            .manifest
            .lock()
            .map_err(|_| AgentError::internal())?;
        if !state
            .validation
            .as_ref()
            .is_some_and(|batch| batch.approved(exec.flow, id))
        {
            return Err(invalid("Épico aguardando validação manual. O Planejador deve usar validation_publish e aguardar o usuário encaminhar todas as aprovações."));
        }
    } else if exec.flow == Flow::Complete && exec.role == Role::Planner {
        return Err(invalid(
            "No fluxo Completo, encaminhe a conclusão das tarefas ao Orquestrador.",
        ));
    }
    Ok(())
}

fn idle(data: &SessionData) -> Result<(), AgentError> {
    if data.active.is_some()
        || data.compacting
        || data.manual_compaction
        || !data.extras.queue.is_empty()
    {
        return Err(invalid(
            "Aguarde a conversa e a fila terminarem para validar.",
        ));
    }
    if data.storage_failed {
        return Err(AgentError::storage());
    }
    Ok(())
}
fn load(session: &Session, home: &Path) -> Result<(PathBuf, Manifest), AgentError> {
    let directory = storage::path(home, &session.id)?;
    let state = storage::load(&directory, &session.id)?
        .ok_or_else(|| invalid("Validação não encontrada."))?;
    if state.flow.direct() {
        return Err(invalid("Este fluxo não possui validação por tarefas."));
    }
    Ok((directory, state))
}
fn current<'a>(state: &'a mut Manifest, id: &str) -> Result<&'a mut Batch, AgentError> {
    state
        .validation
        .as_mut()
        .filter(|batch| batch.id == id && batch.flow == state.flow)
        .ok_or_else(|| invalid("A rodada de validação mudou. Atualize a conversa."))
}

#[tauri::command]
pub async fn decide_workflow_validation(
    app: tauri::AppHandle,
    agent: tauri::State<'_, AgentState>,
    conversation_id: String,
    batch_id: String,
    item_id: String,
    decision: Decision,
    reason: Option<String>,
) -> Result<(), AgentError> {
    let session = agent
        .runtime_session(&app, &app.state::<AppState>(), &conversation_id)
        .await?;
    let home = app.path().home_dir().map_err(|_| AgentError::storage())?;
    let histories = agent.histories.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let data = session.data.lock().map_err(|_| AgentError::internal())?;
        idle(&data)?;
        let (directory, mut state) = load(&session, &home)?;
        if histories.has_turn(&session.journal, &batch_id)? {
            return Err(invalid("Os resultados já foram encaminhados."));
        }
        current(&mut state, &batch_id)?.decide(&item_id, decision, reason)?;
        state.revision += 1;
        storage::save(&directory, &state)
    })
    .await
    .map_err(|_| AgentError::internal())??;
    let _ = app.emit(
        "workflow:changed",
        json!({"conversationId":conversation_id}),
    );
    Ok(())
}

// The journal turn ID is the acceptance batch ID. The append is the delivery commit point:
// retries/restarts reconcile from the journal index, even if updating state.json failed.
fn reserve(
    session: &Session,
    home: &Path,
    histories: &history::HistoryState,
    id: &str,
) -> Result<Option<watch::Receiver<bool>>, AgentError> {
    let mut data = session.data.lock().map_err(|_| AgentError::internal())?;
    let (directory, mut state) = load(session, home)?;
    let exists = histories.has_turn(&session.journal, id)?;
    let batch = current(&mut state, id)?;
    // An active Hub may already have advanced state.json. Duplicate delivery must
    // never write an offline snapshot over that live manifest.
    if exists {
        return Ok(None);
    }
    idle(&data)?;
    if batch.submitted || !batch.ready() {
        return Err(invalid(
            "Valide todos os itens da rodada atual antes de encaminhar.",
        ));
    }
    let content = batch.feedback();
    let mut options = state.options.clone();
    options.workflow = Some(state.flow);
    let signal = session.reserve_locked(&mut data, content, options, Some(id.into()), vec![])?;
    state.validation.as_mut().unwrap().submitted = true;
    state.revision += 1;
    // Never lose a reserved turn due to a secondary cache write failure.
    let _ = storage::save(&directory, &state);
    Ok(Some(signal))
}
#[tauri::command]
pub async fn submit_workflow_validation(
    app: tauri::AppHandle,
    persistence: tauri::State<'_, AppState>,
    agent: tauri::State<'_, AgentState>,
    conversation_id: String,
    batch_id: String,
) -> Result<(), AgentError> {
    let activity = crate::updater::begin_activity(&app)
        .map_err(|message| AgentError::new("app_updating", &message))?;
    let home = app.path().home_dir().map_err(|_| AgentError::storage())?;
    crate::core::require_ready(&home)?;
    let session = agent
        .runtime_session(&app, &persistence, &conversation_id)
        .await?;
    let saved = session.clone();
    let root = home.clone();
    let histories = agent.histories.clone();
    let state = persistence.inner().clone();
    let signal = tauri::async_runtime::spawn_blocking(move || {
        let (_, manifest) = load(&saved, &root)?;
        settings::validate(manifest.flow, &settings::load(&state, &root)?)?;
        reserve(&saved, &root, &histories, &batch_id)
    })
    .await
    .map_err(|_| AgentError::internal())??;
    if let Ok(snapshot) = session.snapshot() {
        (session.emit)(snapshot);
    }
    let _ = app.emit(
        "workflow:changed",
        json!({"conversationId":conversation_id}),
    );
    if let Some(signal) = signal {
        spawn_run(
            session,
            persistence.inner().clone(),
            app.state::<OpenAiCodexState>().inner().clone(),
            app.state::<crate::mcp::McpState>().inner().clone(),
            home,
            app,
            (signal, activity),
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn batch() -> Batch {
        Batch {
            id: library::new_id().unwrap(),
            run_id: "run".into(),
            flow: Flow::Complete,
            epic_ids: vec!["project-epic".into()],
            items: vec![Item {
                id: "test".into(),
                title: "Abrir projeto".into(),
                steps: vec!["Abra o projeto pela sidebar.".into()],
                expected: "O dashboard aparece.".into(),
                decision: Decision::Pending,
                reason: None,
            }],
            submitted: false,
            stale: false,
            created_at: now(),
        }
    }
    #[test]
    fn only_submitted_unanimous_user_approval_can_close_the_matching_epic() {
        let mut batch = batch();
        assert!(!batch.ready());
        assert!(!batch.approved(Flow::Complete, "project-epic"));
        assert!(batch
            .decide("test", Decision::Rejected, Some(" ".into()))
            .is_err());
        batch
            .decide("test", Decision::Rejected, Some("Tela em branco".into()))
            .unwrap();
        assert!(batch.ready());
        assert!(!batch.approved(Flow::Complete, "project-epic"));
        assert!(batch.feedback().contains("Tela em branco"));
        batch.decide("test", Decision::Approved, None).unwrap();
        assert!(!batch.approved(Flow::Complete, "project-epic"));
        batch.submitted = true;
        assert!(batch.approved(Flow::Complete, "project-epic"));
        assert!(!batch.approved(Flow::Planned, "project-epic"));
        assert!(!batch.approved(Flow::Complete, "another-epic"));
        assert!(batch
            .decide("test", Decision::Rejected, Some("later".into()))
            .is_err());
        batch.stale = true;
        assert!(!batch.approved(Flow::Complete, "project-epic"));
    }
    #[test]
    fn feedback_is_reserved_once_and_restores_from_journal_after_secondary_save_failure() {
        let (fixture, hub) = super::super::tests::hub();
        finish(&hub.root, Ok(()));
        let directory = storage::path(&fixture.root, &hub.root.id).unwrap();
        std::fs::create_dir_all(&directory).unwrap();
        let mut state = hub.manifest.lock().unwrap().clone();
        let mut batch = batch();
        batch
            .decide(
                "test",
                Decision::Rejected,
                Some("A tela fica vazia.".into()),
            )
            .unwrap();
        let id = batch.id.clone();
        state.validation = Some(batch);
        storage::save(&directory, &state).unwrap();
        let histories = history::HistoryState::default();
        assert!(reserve(&hub.root, &fixture.root, &histories, &id)
            .unwrap()
            .is_some());
        assert_eq!(
            hub.root
                .data
                .lock()
                .unwrap()
                .turns
                .last()
                .unwrap()
                .turn
                .options
                .workflow,
            Some(Flow::Complete)
        );
        assert!(hub
            .root
            .data
            .lock()
            .unwrap()
            .turns
            .last()
            .unwrap()
            .turn
            .user
            .contains("A tela fica vazia."));
        // Simulate the state.json write being lost after durable journal reservation.
        storage::save(&directory, &state).unwrap();
        assert!(reserve(
            &hub.root,
            &fixture.root,
            &history::HistoryState::default(),
            &id
        )
        .unwrap()
        .is_none());
        assert_eq!(
            journal::load_all(&hub.root.journal)
                .unwrap()
                .0
                .iter()
                .filter(|turn| turn.turn.id == id)
                .count(),
            1
        );
        assert!(history::HistoryState::default()
            .has_turn(&hub.root.journal, &id)
            .unwrap());
    }
    #[test]
    fn submission_rejects_pending_busy_stale_and_direct_flow_checklists() {
        let (fixture, hub) = super::super::tests::hub();
        let directory = storage::path(&fixture.root, &hub.root.id).unwrap();
        std::fs::create_dir_all(&directory).unwrap();
        let mut state = hub.manifest.lock().unwrap().clone();
        let batch = batch();
        let id = batch.id.clone();
        state.validation = Some(batch);
        storage::save(&directory, &state).unwrap();
        let histories = history::HistoryState::default();
        assert!(reserve(&hub.root, &fixture.root, &histories, &id).is_err());
        finish(&hub.root, Ok(()));
        assert!(reserve(&hub.root, &fixture.root, &histories, &id).is_err());
        state
            .validation
            .as_mut()
            .unwrap()
            .decide("test", Decision::Approved, None)
            .unwrap();
        state.validation.as_mut().unwrap().stale = true;
        storage::save(&directory, &state).unwrap();
        assert!(reserve(&hub.root, &fixture.root, &histories, &id).is_err());
        state.flow = Flow::Standard;
        storage::save(&directory, &state).unwrap();
        assert!(reserve(&hub.root, &fixture.root, &histories, &id).is_err());
    }
    #[tokio::test]
    async fn tools_are_exclusive_to_root_planner_and_product_mutation_invalidates_acceptance() {
        let (_fixture, hub) = super::super::tests::hub();
        for (role, flow, id, available) in [
            (Role::Planner, Flow::Planned, "main", true),
            (Role::Planner, Flow::Complete, "main", true),
            (Role::Planner, Flow::Complete, "worker", false),
            (Role::Builder, Flow::Standard, "main", false),
            (Role::Designer, Flow::Designer, "main", false),
        ] {
            let exec = Execution {
                hub: hub.clone(),
                id: id.into(),
                role,
                flow,
                scope: vec![".".into()],
            };
            let mut tools = vec![];
            exec.filter(&mut tools);
            assert_eq!(
                tools
                    .iter()
                    .any(|tool| tool["name"] == "validation_publish"),
                available
            );
        }
        let mut batch = batch();
        batch.decide("test", Decision::Approved, None).unwrap();
        batch.submitted = true;
        hub.mutate(|state| {
            state.validation = Some(batch);
            Ok(())
        })
        .unwrap();
        let exec = Execution {
            hub: hub.clone(),
            id: "main".into(),
            role: Role::Builder,
            flow: Flow::Complete,
            scope: vec![".".into()],
        };
        let tool = ToolCall {
            id: "write".into(),
            name: "write".into(),
            args: json!({}),
            status: "pending".into(),
            output: String::new(),
            duration_ms: 0,
        };
        let _guard = exec
            .mutation_guard(&tool, hub.root_signal.clone())
            .await
            .unwrap();
        assert!(
            storage::load(&hub.directory, &hub.root.id)
                .unwrap()
                .unwrap()
                .validation
                .unwrap()
                .stale
        );
    }
}
