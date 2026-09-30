//! Switch inference once, after provider retries, without creating a new task.
use super::*;
use workflow::settings::ModelChoice;

pub(super) fn used(turn: &StoredTurn) -> bool {
    turn.wire
        .iter()
        .any(|item| item["_jarvis_model_fallback"].is_object())
}

fn eligible(turn: &StoredTurn, choice: &ModelChoice, error: &AgentError, cancelled: bool) -> bool {
    !cancelled
        && error.code == "provider_retry_exhausted"
        && !used(turn)
        && (turn.turn.options.executor != choice.executor
            || turn.turn.options.account != choice.account
            || turn.turn.options.model != choice.model)
}

pub(super) async fn recover(
    session: &Arc<Session>,
    signal: &watch::Receiver<bool>,
    execution: Option<&workflow::Execution>,
    error: &AgentError,
) -> Result<bool, AgentError> {
    if error.code != "provider_retry_exhausted" || *signal.borrow() {
        return Ok(false);
    }
    let Some(execution) = execution else {
        return Ok(false);
    };
    let Some(choice) = execution.secondary_model()? else {
        return Ok(false);
    };
    {
        let data = session.data.lock().map_err(|_| AgentError::internal())?;
        let current = data.turns.last().ok_or_else(AgentError::internal)?;
        if !eligible(current, &choice, error, *signal.borrow()) {
            return Ok(false);
        }
    }
    // Authentication and context capacity are loaded by the existing turn runner.
    // Never retry a mutation here: only confirmed journal receipts are replayed.
    choice.validate_shape()?;
    session.drain_interactions(true).await;
    if !session
        .data
        .lock()
        .map_err(|_| AgentError::internal())?
        .active
        .as_mut()
        .is_some_and(turn_state::ActiveTurn::resume_inference)
    {
        return Ok(false);
    }
    session.update_async(|data| switch(data, &choice)).await?;
    execution.set_effective_model(&choice)?;
    Ok(true)
}

fn switch(data: &mut SessionData, choice: &ModelChoice) {
    let current = data.turns.last_mut().expect("active turn");
    let previous = &current.turn.options;
    let notice = format!(
        "O modelo {} esgotou as tentativas de recuperação. Continuando com o modelo secundário {} e preservando o progresso desta tarefa.",
        previous.model, choice.model,
    );
    current.wire.push(json!({
        "role":"user", "_jarvis_runtime":true,
        "_jarvis_model_fallback": {
            "from": {"executor":previous.executor,"account":previous.account,"model":previous.model},
            "to": choice,
        },
        "content":"The primary provider exhausted its retries. Continue the same task using the confirmed conversation and tool receipts. Do not repeat completed actions. Verify uncertain effects before retrying them.",
    }));
    choice.apply(&mut current.turn.options);
    current.turn.context_window = None;
    current.turn.steps.push(Step {
        text: notice,
        context_id: Some("model-fallback".into()),
        ..Step::default()
    });
    if let Some(context) = &mut data.extras.context {
        context.measured = None;
    }
}

/// Provider-private signatures belong to the original model/account. Preserve
/// public messages and call/result identities while projecting a portable copy.
pub(super) fn portable(items: &mut Vec<Value>) {
    items.retain_mut(|item| {
        if item["type"] == "reasoning" {
            return false;
        }
        if item.get("_jarvis_claude_session").is_some()
            && item.get("role").is_none()
            && item.get("type").is_none()
        {
            return false;
        }
        if let Some(object) = item.as_object_mut() {
            object.retain(|key, _| {
                key != "id"
                    && key != "encrypted_content"
                    && key != "_custom"
                    && !key.starts_with("_antigravity")
            });
        }
        true
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::tests::{options, session, Fixture};

    fn secondary() -> ModelChoice {
        ModelChoice {
            executor: crate::claude::Executor::Jarvis,
            account: "backup-account".into(),
            model: "backup-model".into(),
            reasoning: None,
            fallback: None,
        }
    }

    #[tokio::test]
    async fn model_fallback_preserves_task_and_receipts_and_survives_reload_without_private_state()
    {
        let fixture = Fixture::new();
        let session = session(&fixture);
        let _signal = session
            .reserve("Implementar a tarefa".into(), options(ApprovalMode::Yolo))
            .unwrap();
        let receipt =
            json!({"type":"function_call_output", "call_id":"write-1", "output":"Arquivo salvo"});
        session.update(true, |data| {
            let current = data.turns.last_mut().unwrap();
            current.wire.extend([
                json!({"_jarvis_claude_session":"previous-cli-session", "_jarvis_runtime":true}),
                json!({"type":"reasoning","id":"rs-1","encrypted_content":"secret", "_custom":{"blocks":[]}}),
                json!({"type":"function_call","id":"fc-1", "call_id":"write-1", "name":"write", "arguments":"{\"path\":\"src/a.ts\"}", "_antigravity_part":{"thoughtSignature":"signed"}}),
                receipt.clone(),
                json!({"role":"user", "content":"Preserve os dados existentes"}),
            ]);
            current.turn.context_window = Some(200_000);
        }).unwrap();
        let before = session.data.lock().unwrap().turns[0].clone();
        let exhausted = AgentError::new("provider_retry_exhausted", "Retries exhausted");
        for code in [
            "cancelled",
            "storage",
            "context_overflow",
            "provider_refusal",
            "permission_denied",
            "provider_transport",
        ] {
            assert!(
                !eligible(
                    &before,
                    &secondary(),
                    &AgentError::new(code, "error"),
                    false
                ),
                "{code}"
            );
        }
        assert!(!eligible(&before, &secondary(), &exhausted, true));
        assert!(eligible(&before, &secondary(), &exhausted, false));
        session
            .update_async(|data| switch(data, &secondary()))
            .await
            .unwrap();
        let (persisted, _) = journal::read_only(&session.journal).unwrap();
        assert_eq!(persisted.len(), 1);
        let switched = &persisted[0];
        assert_eq!(switched.turn.id, before.turn.id);
        assert_eq!(switched.turn.options.model, "backup-model");
        assert_eq!(
            switched.turn.options.approval_mode,
            before.turn.options.approval_mode
        );
        assert_eq!(switched.turn.context_window, None);
        assert_eq!(&switched.wire[..before.wire.len()], before.wire.as_slice());
        assert!(!eligible(switched, &secondary(), &exhausted, false));
        let mut other = secondary();
        other.model = "third-model".into();
        assert!(!eligible(switched, &other, &exhausted, false));
        let replay = session.input().unwrap();
        assert_eq!(replay.iter().filter(|item| **item == receipt).count(), 1);
        assert!(replay
            .iter()
            .any(|item| item["content"] == "Preserve os dados existentes"));
        let call = replay
            .iter()
            .find(|item| item["type"] == "function_call")
            .unwrap();
        assert_eq!(call["call_id"], "write-1");
        assert!(call.get("id").is_none());
        let text = serde_json::to_string(&replay).unwrap();
        for private in [
            "previous-cli-session",
            "encrypted_content",
            "_custom",
            "_antigravity_part",
            "secret",
            "signed",
        ] {
            assert!(!text.contains(private), "{private}");
        }
        // Signatures produced by the new provider remain usable on its next step.
        session
            .update(false, |data| {
                data.turns[0]
                    .wire
                    .push(json!({"type":"reasoning", "encrypted_content":"secondary-signature"}))
            })
            .unwrap();
        assert!(session
            .input()
            .unwrap()
            .iter()
            .any(|item| item["encrypted_content"] == "secondary-signature"));
    }
}
