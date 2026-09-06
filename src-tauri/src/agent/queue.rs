use super::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct QueuedMessage {
    pub id: String,
    pub content: String,
    pub options: TurnOptions,
    #[serde(default)]
    pub parts: Vec<skill_input::MessagePart>,
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
        if data.storage_failed {
            return Err(AgentError::storage());
        }
        if data.compacting || data.manual_compaction {
            return Err(AgentError::new(
                "compacting",
                "Aguarde a compactação terminar.",
            ));
        }
        if data.active.is_none() && data.extras.queue.is_empty() {
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
            .filter(|_| data.active.is_some())
            .map(|turn| turn.turn.options.clone())
            .unwrap_or(options);
        options.approval_mode = ApprovalMode::Yolo;
        let mut queue = data.extras.queue.clone();
        queue.push(QueuedMessage {
            id: library::new_id()?,
            content,
            options,
            parts,
        });
        self.checkpoint(&mut data, "queue_checkpoint", &queue)?;
        data.extras.queue = queue;
        data.revision += 1;
        Ok(None)
    }

    pub(super) fn reserve_next(&self) -> Result<Option<watch::Receiver<bool>>, AgentError> {
        let mut data = self.data.lock().map_err(|_| AgentError::internal())?;
        if data.active.is_some() || data.storage_failed || data.compacting || data.manual_compaction
        {
            return Ok(None);
        }
        let Some(message) = data.extras.queue.first().cloned() else {
            return Ok(None);
        };
        let signal = self.reserve_locked(
            &mut data,
            message.content,
            message.options,
            Some(message.id),
            message.parts,
        )?;
        data.extras.queue.remove(0);
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
            .position(|message| message.id == id)
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
        data.revision += 1;
        Ok(removed)
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemovedMessage {
    message: QueuedMessage,
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
    let session = agent.runtime_session(&app, &persistence, &conversation_id).await?;
    let message = session.remove_queued(&message_id)?;
    let snapshot = session.snapshot()?;
    (session.emit)(snapshot.clone());
    agent.release_idle(&session);
    Ok(RemovedMessage { message, snapshot })
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
            mode: Mode::Build, workflow: None,
            approval_mode: ApprovalMode::Manual,
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
            let queue = vec![QueuedMessage { id: library::new_id().unwrap(), content: "resume".into(), options: tests_options(), parts: vec![] }];
            session.checkpoint(&mut data, "queue_checkpoint", &queue).unwrap();
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
