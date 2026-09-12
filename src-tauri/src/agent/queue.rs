use super::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct QueuedMessage {
    pub id: String,
    pub content: String,
    pub options: TurnOptions,
    #[serde(default)]
    pub parts: Vec<skill_input::MessagePart>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) auxiliary_for: Option<String>,
}

impl QueuedMessage {
    pub(super) fn scheduled(&self) -> bool {
        self.auxiliary_for.is_none()
    }

    fn auxiliary_for(&self, turn_id: &str) -> bool {
        self.auxiliary_for.as_deref() == Some(turn_id)
    }
}

impl Session {
    #[cfg(test)]
    pub(super) fn submit(
        &self,
        content: String,
        options: TurnOptions,
    ) -> Result<Option<watch::Receiver<bool>>, AgentError> {
        self.submit_message(content, options, vec![])
    }

    pub(super) fn submit_message(
        &self,
        content: String,
        options: TurnOptions,
        parts: Vec<skill_input::MessagePart>,
    ) -> Result<Option<watch::Receiver<bool>>, AgentError> {
        let mut data = self.data.lock().map_err(|_| AgentError::internal())?;
        if self.journal_maintenance.load(Ordering::Acquire) {
            return Err(journal_maintenance::maintenance_error());
        }
        if data.storage_failed {
            return Err(AgentError::storage());
        }
        if data.compacting || data.manual_compaction {
            return Err(AgentError::new(
                "compacting",
                "Aguarde a compactação terminar.",
            ));
        }
        if data.active.is_none() && data.recovery.is_none() && data.extras.queue.is_empty() {
            return self
                .reserve_locked(&mut data, content, options, None, parts)
                .map(Some);
        }
        if data.extras.queue.len() >= 20 {
            return Err(AgentError::new(
                "queue_full",
                "A fila aceita até 20 mensagens. Aguarde ou retire uma mensagem.",
            ));
        }
        let mut options = data
            .turns
            .last()
            .filter(|_| data.active.is_some() || data.recovery.is_some())
            .map(|turn| turn.turn.options.clone())
            .unwrap_or(options);
        options.approval_mode = ApprovalMode::Yolo;
        let mut queue = data.extras.queue.clone();
        queue.push(QueuedMessage {
            id: library::new_id()?,
            content,
            options,
            parts,
            auxiliary_for: None,
        });
        self.checkpoint(&mut data, "queue_checkpoint", &queue)?;
        data.extras.queue = queue;
        data.revision = next_revision();
        Ok(None)
    }

    pub(super) fn reserve_next(&self) -> Result<Option<watch::Receiver<bool>>, AgentError> {
        let mut data = self.data.lock().map_err(|_| AgentError::internal())?;
        if data.active.is_some()
            || data.recovery.is_some()
            || data.storage_failed
            || data.compacting
            || data.manual_compaction
        {
            return Ok(None);
        }
        let Some((index, message)) = data
            .extras
            .queue
            .iter()
            .enumerate()
            .find(|(_, message)| message.scheduled())
            .map(|(index, message)| (index, message.clone()))
        else {
            return Ok(None);
        };
        let signal = self.reserve_locked(
            &mut data,
            message.content,
            message.options,
            Some(message.id),
            message.parts,
        )?;
        data.extras.queue.remove(index);
        Ok(Some(signal))
    }

    fn remove_queued(&self, id: &str) -> Result<QueuedMessage, AgentError> {
        let mut data = self.data.lock().map_err(|_| AgentError::internal())?;
        if data.compacting || data.manual_compaction {
            return Err(AgentError::new(
                "compacting",
                "Aguarde a compactação terminar.",
            ));
        }
        let index = data
            .extras
            .queue
            .iter()
            .position(|message| message.id == id && message.scheduled())
            .ok_or_else(|| {
                AgentError::new(
                    "queue_started",
                    "A mensagem já começou a ser enviada ou foi retirada da fila.",
                )
            })?;
        let mut queue = data.extras.queue.clone();
        let removed = queue.remove(index);
        self.checkpoint(&mut data, "queue_checkpoint", &queue)?;
        data.extras.queue = queue;
        data.revision = next_revision();
        Ok(removed)
    }

    fn reorder_queued(&self, ids: &[String]) -> Result<(), AgentError> {
        let mut data = self.data.lock().map_err(|_| AgentError::internal())?;
        if data.compacting || data.manual_compaction {
            return Err(AgentError::new(
                "compacting",
                "Aguarde a compactação terminar.",
            ));
        }
        let scheduled: Vec<_> = data
            .extras
            .queue
            .iter()
            .filter(|message| message.scheduled())
            .cloned()
            .collect();
        let unique: std::collections::HashSet<_> = ids.iter().collect();
        if ids.len() != scheduled.len()
            || unique.len() != ids.len()
            || scheduled
                .iter()
                .any(|message| !unique.contains(&message.id))
        {
            return Err(AgentError::new(
                "queue_changed",
                "A fila mudou enquanto era reordenada. Tente novamente.",
            ));
        }
        let mut by_id: std::collections::HashMap<_, _> = scheduled
            .into_iter()
            .map(|message| (message.id.clone(), message))
            .collect();
        let mut ordered = ids.iter();
        let mut queue = data.extras.queue.clone();
        for message in queue.iter_mut().filter(|message| message.scheduled()) {
            let id = ordered.next().ok_or_else(AgentError::internal)?;
            *message = by_id.remove(id).ok_or_else(AgentError::internal)?;
        }
        self.checkpoint(&mut data, "queue_checkpoint", &queue)?;
        data.extras.queue = queue;
        data.revision = next_revision();
        Ok(())
    }

    fn promote_queued(&self, id: &str) -> Result<bool, AgentError> {
        let mut data = self.data.lock().map_err(|_| AgentError::internal())?;
        if data.compacting || data.manual_compaction {
            return Err(AgentError::new(
                "compacting",
                "Aguarde a compactação terminar.",
            ));
        }
        let Some(active) = data.active.as_ref() else {
            return Ok(false);
        };
        if !active.accepting_auxiliary {
            return Ok(false);
        }
        let turn_id = active.id.clone();
        if turn_id == id {
            return Ok(false);
        }
        let index = data
            .extras
            .queue
            .iter()
            .position(|message| message.id == id && message.scheduled())
            .ok_or_else(|| {
                AgentError::new(
                    "queue_started",
                    "A mensagem já começou a ser enviada ou foi retirada da fila.",
                )
            })?;
        let mut queue = data.extras.queue.clone();
        queue[index].auxiliary_for = Some(turn_id);
        self.checkpoint(&mut data, "queue_checkpoint", &queue)?;
        data.extras.queue = queue;
        data.revision = next_revision();
        Ok(true)
    }

    fn pending_auxiliary(&self) -> Result<(String, Vec<QueuedMessage>), AgentError> {
        let data = self.data.lock().map_err(|_| AgentError::internal())?;
        let turn_id = data
            .active
            .as_ref()
            .map(|active| active.id.clone())
            .ok_or_else(AgentError::cancelled)?;
        let messages = data
            .extras
            .queue
            .iter()
            .filter(|message| message.auxiliary_for(&turn_id))
            .cloned()
            .collect();
        Ok((turn_id, messages))
    }

    fn inject_auxiliary(
        &self,
        turn_id: &str,
        rendered: Vec<(String, String)>,
    ) -> Result<(), AgentError> {
        if rendered.is_empty() {
            return Ok(());
        }
        let mut data = self.data.lock().map_err(|_| AgentError::internal())?;
        if !data
            .active
            .as_ref()
            .is_some_and(|active| active.id == turn_id)
        {
            return Err(AgentError::cancelled());
        }
        let available: std::collections::HashSet<_> = data
            .extras
            .queue
            .iter()
            .filter(|message| message.auxiliary_for(turn_id))
            .map(|message| message.id.as_str())
            .collect();
        let delivered: Vec<_> = rendered
            .into_iter()
            .filter(|(id, _)| available.contains(id.as_str()))
            .collect();
        if delivered.is_empty() {
            return Ok(());
        }
        let delivered_ids: std::collections::HashSet<_> =
            delivered.iter().map(|(id, _)| id.as_str()).collect();
        let mut current = data
            .turns
            .last()
            .filter(|turn| turn.turn.id == turn_id)
            .cloned()
            .ok_or_else(AgentError::internal)?;
        for (id, content) in &delivered {
            current.wire.push(json!({
                "role": "user",
                "_jarvis_auxiliary": true,
                "_jarvis_queue_id": id,
                "content": format!(
                    "Orientação adicional do usuário recebida enquanto esta execução estava em andamento. Incorpore-a ao trabalho atual sem descartar o contexto nem repetir ações já concluídas:\n{content}"
                ),
            }));
        }
        // Persist the turn first. If the following queue checkpoint is interrupted,
        // journal recovery removes entries carrying the same _jarvis_queue_id.
        self.persist_turn(&mut data, &current)?;
        *data.turns.last_mut().ok_or_else(AgentError::internal)? = current;
        let mut queue = data.extras.queue.clone();
        queue.retain(|message| !delivered_ids.contains(message.id.as_str()));
        self.checkpoint(&mut data, "queue_checkpoint", &queue)?;
        data.extras.queue = queue;
        data.revision = next_revision();
        let snapshot = self.snapshot_data(&data);
        data.last_emit = std::time::Instant::now();
        drop(data);
        (self.emit)(snapshot);
        Ok(())
    }

    pub(super) fn continue_for_auxiliary(&self) -> Result<bool, AgentError> {
        let mut data = self.data.lock().map_err(|_| AgentError::internal())?;
        let turn_id = data
            .active
            .as_ref()
            .map(|active| active.id.clone())
            .ok_or_else(AgentError::cancelled)?;
        let pending = data
            .extras
            .queue
            .iter()
            .any(|message| message.auxiliary_for(&turn_id));
        if !pending {
            data.active
                .as_mut()
                .ok_or_else(AgentError::cancelled)?
                .accepting_auxiliary = false;
        }
        Ok(pending)
    }

    pub(super) fn stop_auxiliary_delivery(&self) -> Result<(), AgentError> {
        let mut data = self.data.lock().map_err(|_| AgentError::internal())?;
        let Some(turn_id) = data.active.as_ref().map(|active| active.id.clone()) else {
            return Ok(());
        };
        if let Some(active) = data.active.as_mut() {
            active.accepting_auxiliary = false;
        }
        let mut queue = data.extras.queue.clone();
        let mut restored = false;
        for message in &mut queue {
            if message.auxiliary_for(&turn_id) {
                message.auxiliary_for = None;
                restored = true;
            }
        }
        if restored {
            self.checkpoint(&mut data, "queue_checkpoint", &queue)?;
            data.extras.queue = queue;
            data.revision = next_revision();
        }
        Ok(())
    }
}

pub(super) async fn inject_pending_auxiliary(
    session: &Session,
    home: &std::path::Path,
) -> Result<(), AgentError> {
    let (turn_id, messages) = session.pending_auxiliary()?;
    let mut rendered = Vec::with_capacity(messages.len());
    for message in messages {
        let content =
            skill_input::render(home, &session.root, &message.content, &message.parts).await?;
        rendered.push((message.id, content));
    }
    session.inject_auxiliary(&turn_id, rendered)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemovedMessage {
    message: QueuedMessage,
    snapshot: ChatSnapshot,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QueuedDelivery {
    delivered: bool,
    snapshot: ChatSnapshot,
}

#[tauri::command]
pub async fn remove_queued_message(
    app: tauri::AppHandle,
    persistence: tauri::State<'_, AppState>,
    agent: tauri::State<'_, AgentState>,
    conversation_id: String,
    message_id: String,
) -> Result<RemovedMessage, AgentError> {
    let session = agent
        .runtime_session(&app, &persistence, &conversation_id)
        .await?;
    let message = session.remove_queued(&message_id)?;
    let snapshot = session.snapshot()?;
    (session.emit)(snapshot.clone());
    agent.release_idle(&session);
    Ok(RemovedMessage { message, snapshot })
}

#[tauri::command]
pub async fn delete_queued_message(
    app: tauri::AppHandle,
    persistence: tauri::State<'_, AppState>,
    agent: tauri::State<'_, AgentState>,
    conversation_id: String,
    message_id: String,
) -> Result<ChatSnapshot, AgentError> {
    let session = agent
        .runtime_session(&app, &persistence, &conversation_id)
        .await?;
    session.remove_queued(&message_id)?;
    let snapshot = session.snapshot()?;
    (session.emit)(snapshot.clone());
    agent.release_idle(&session);
    Ok(snapshot)
}

#[tauri::command]
pub async fn reorder_queued_messages(
    app: tauri::AppHandle,
    persistence: tauri::State<'_, AppState>,
    agent: tauri::State<'_, AgentState>,
    conversation_id: String,
    message_ids: Vec<String>,
) -> Result<ChatSnapshot, AgentError> {
    let session = agent
        .runtime_session(&app, &persistence, &conversation_id)
        .await?;
    session.reorder_queued(&message_ids)?;
    let snapshot = session.snapshot()?;
    (session.emit)(snapshot.clone());
    agent.release_idle(&session);
    Ok(snapshot)
}

#[tauri::command]
pub async fn send_queued_message_now(
    app: tauri::AppHandle,
    persistence: tauri::State<'_, AppState>,
    agent: tauri::State<'_, AgentState>,
    conversation_id: String,
    message_id: String,
) -> Result<QueuedDelivery, AgentError> {
    let session = agent
        .runtime_session(&app, &persistence, &conversation_id)
        .await?;
    let delivered = session.promote_queued(&message_id)?;
    let snapshot = session.snapshot()?;
    (session.emit)(snapshot.clone());
    agent.release_idle(&session);
    Ok(QueuedDelivery {
        delivered,
        snapshot,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::tests::Fixture;

    #[test]
    fn queued_messages_keep_order_options_and_cannot_be_removed_after_reservation() {
        let fixture = Fixture::new();
        let session = crate::agent::tests::session(&fixture);
        let options = tests_options();
        session.reserve("first".into(), options.clone()).unwrap();
        let mut other = options.clone();
        other.model = "ignored-change".into();
        assert!(session.submit("second".into(), other).unwrap().is_none());
        session.submit("third".into(), options).unwrap();
        let queued = session.snapshot().unwrap().queued_messages;
        assert_eq!(queued[0].options.model, "model");
        assert_eq!(queued[0].options.approval_mode, ApprovalMode::Yolo);
        assert_eq!(
            session.remove_queued(&queued[1].id).unwrap().content,
            "third"
        );
        finish(&session, Ok(()));
        assert!(session.reserve_next().unwrap().is_some());
        assert!(session.remove_queued(&queued[0].id).is_err());
        let (turns, extras) = journal::load_all(&session.journal).unwrap();
        assert_eq!(turns.len(), 2);
        assert_eq!(turns[1].turn.user, "second");
        assert_eq!(turns[1].turn.options.approval_mode, ApprovalMode::Yolo);
        assert!(extras.queue.is_empty());
    }

    fn tests_options() -> TurnOptions {
        TurnOptions {
            account: "test".into(),
            model: "model".into(),
            reasoning: None,
            mode: Mode::Build,
            workflow: None,
            custom_workflow_id: None,
            custom_agent_id: None,
            approval_mode: ApprovalMode::Manual,
            manual_validation: false,
        }
    }

    #[test]
    fn legacy_manual_queue_resumes_automatically_without_rewriting_past_turns() {
        let fixture = Fixture::new();
        let session = crate::agent::tests::session(&fixture);
        session.reserve("historic".into(), tests_options()).unwrap();
        finish(&session, Ok(()));
        {
            let mut data = session.data.lock().unwrap();
            data.turns[0].turn.options.approval_mode = ApprovalMode::Manual;
            journal::append(&session.journal, &data.turns[0]).unwrap();
            let queue = vec![QueuedMessage {
                id: library::new_id().unwrap(),
                content: "resume".into(),
                options: tests_options(),
                parts: vec![],
                auxiliary_for: None,
            }];
            session
                .checkpoint(&mut data, "queue_checkpoint", &queue)
                .unwrap();
            data.extras.queue = queue;
        }
        assert!(session.reserve_next().unwrap().is_some());
        let (turns, _) = journal::load_all(&session.journal).unwrap();
        assert_eq!(turns[0].turn.options.approval_mode, ApprovalMode::Manual);
        assert_eq!(turns[1].turn.options.approval_mode, ApprovalMode::Yolo);
    }

    #[test]
    fn cancellation_keeps_queue_durable_and_removal_survives_restart() {
        let fixture = Fixture::new();
        let session = crate::agent::tests::session(&fixture);
        session.reserve("first".into(), tests_options()).unwrap();
        session.submit("second".into(), tests_options()).unwrap();
        session.submit("third".into(), tests_options()).unwrap();
        finish(&session, Err(AgentError::cancelled()));
        let (turns, extras) = journal::load_all(&session.journal).unwrap();
        assert_eq!(
            extras
                .queue
                .iter()
                .map(|message| message.content.as_str())
                .collect::<Vec<_>>(),
            ["second", "third"]
        );
        {
            let mut data = session.data.lock().unwrap();
            data.turns = turns;
            data.extras = extras;
        }
        let queued = session.snapshot().unwrap().queued_messages;
        assert_eq!(
            session.remove_queued(&queued[0].id).unwrap().content,
            "second"
        );
        assert_eq!(
            journal::load_all(&session.journal).unwrap().1.queue[0].content,
            "third"
        );
        assert!(session.snapshot().unwrap().active_turn_id.is_none());
    }

    #[tokio::test]
    async fn queued_messages_reorder_and_promote_without_interrupting_the_active_turn() {
        let fixture = Fixture::new();
        let session = crate::agent::tests::session(&fixture);
        let signal = session.reserve("first".into(), tests_options()).unwrap();
        session.submit("second".into(), tests_options()).unwrap();
        session.submit("third".into(), tests_options()).unwrap();
        let queued = session.snapshot().unwrap().queued_messages;
        let second = queued[0].id.clone();
        let third = queued[1].id.clone();

        session
            .reorder_queued(&[third.clone(), second.clone()])
            .unwrap();
        assert_eq!(
            session
                .snapshot()
                .unwrap()
                .queued_messages
                .iter()
                .map(|message| message.content.as_str())
                .collect::<Vec<_>>(),
            ["third", "second"]
        );

        assert!(session.promote_queued(&third).unwrap());
        assert!(!*signal.borrow());
        assert_eq!(
            session
                .snapshot()
                .unwrap()
                .queued_messages
                .iter()
                .map(|message| message.content.as_str())
                .collect::<Vec<_>>(),
            ["second"]
        );
        inject_pending_auxiliary(&session, &fixture.root)
            .await
            .unwrap();
        let (turns, extras) = journal::load_all(&session.journal).unwrap();
        let guidance = turns[0]
            .wire
            .iter()
            .find(|message| message["_jarvis_queue_id"] == third)
            .unwrap();
        assert!(guidance["content"].as_str().unwrap().contains("third"));
        assert_eq!(extras.queue.len(), 1);
        assert_eq!(extras.queue[0].id, second);

        assert!(!session.continue_for_auxiliary().unwrap());
        assert!(!session.promote_queued(&second).unwrap());
        assert_eq!(session.snapshot().unwrap().queued_messages.len(), 1);
    }

    #[test]
    fn failed_turn_restores_undelivered_auxiliary_message_to_the_queue() {
        let fixture = Fixture::new();
        let session = crate::agent::tests::session(&fixture);
        session.reserve("first".into(), tests_options()).unwrap();
        session.submit("guidance".into(), tests_options()).unwrap();
        let id = session.snapshot().unwrap().queued_messages[0].id.clone();
        assert!(session.promote_queued(&id).unwrap());
        assert!(session.snapshot().unwrap().queued_messages.is_empty());

        finish(&session, Err(AgentError::cancelled()));

        let queued = session.snapshot().unwrap().queued_messages;
        assert_eq!(queued.len(), 1);
        assert_eq!(queued[0].content, "guidance");
        assert!(queued[0].auxiliary_for.is_none());
        let (_, extras) = journal::load_all(&session.journal).unwrap();
        assert_eq!(extras.queue.len(), 1);
        assert!(extras.queue[0].auxiliary_for.is_none());
    }

    #[test]
    fn journal_recovery_deduplicates_an_auxiliary_persisted_before_its_queue_checkpoint() {
        let fixture = Fixture::new();
        let session = crate::agent::tests::session(&fixture);
        session.reserve("first".into(), tests_options()).unwrap();
        session.submit("guidance".into(), tests_options()).unwrap();
        let id = session.snapshot().unwrap().queued_messages[0].id.clone();
        session
            .update(true, |data| {
                data.turns.last_mut().unwrap().wire.push(json!({
                    "role": "user",
                    "content": "guidance",
                    "_jarvis_auxiliary": true,
                    "_jarvis_queue_id": id,
                }));
            })
            .unwrap();

        let (_, extras) = journal::load_all(&session.journal).unwrap();
        assert!(extras.queue.is_empty());
    }

    #[test]
    fn delete_and_reorder_reject_stale_queue_identifiers() {
        let fixture = Fixture::new();
        let session = crate::agent::tests::session(&fixture);
        session.reserve("first".into(), tests_options()).unwrap();
        session.submit("second".into(), tests_options()).unwrap();
        session.submit("third".into(), tests_options()).unwrap();
        let queued = session.snapshot().unwrap().queued_messages;
        assert_eq!(
            session
                .reorder_queued(&[queued[0].id.clone(), "missing".into()])
                .unwrap_err()
                .code,
            "queue_changed"
        );
        assert_eq!(
            session.remove_queued(&queued[0].id).unwrap().content,
            "second"
        );
        assert_eq!(session.snapshot().unwrap().queued_messages.len(), 1);
    }

    #[test]
    fn concurrent_submissions_start_exactly_one_turn_and_enforce_queue_bound() {
        let fixture = Fixture::new();
        let session = crate::agent::tests::session(&fixture);
        let starts: Vec<_> = std::thread::scope(|scope| {
            let handles: Vec<_> = (0..8)
                .map(|index| {
                    let session = &session;
                    scope.spawn(move || {
                        session
                            .submit(format!("message {index}"), tests_options())
                            .unwrap()
                            .is_some()
                    })
                })
                .collect();
            handles
                .into_iter()
                .map(|handle| handle.join().unwrap())
                .collect()
        });
        assert_eq!(starts.iter().filter(|started| **started).count(), 1);
        for _ in 7..20 {
            session.submit("queued".into(), tests_options()).unwrap();
        }
        assert_eq!(
            session
                .submit("overflow".into(), tests_options())
                .unwrap_err()
                .code,
            "queue_full"
        );
        assert_eq!(session.snapshot().unwrap().queued_messages.len(), 20);
    }
}
