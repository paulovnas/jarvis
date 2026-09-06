use super::*;

struct CompactionLease(Arc<Session>);
impl Drop for CompactionLease {
    fn drop(&mut self) {
        let _ = self.0.update(false, |data| {
            data.manual_compaction = false;
            data.compacting = false;
            data.last_emit = std::time::Instant::now() - Duration::from_secs(1);
        });
    }
}

fn begin(session: Arc<Session>) -> Result<(CompactionLease, TurnOptions), AgentError> {
    let mut data = session.data.lock().map_err(|_| AgentError::internal())?;
    if data.active.is_some() || data.compacting || data.manual_compaction {
        return Err(AgentError::new(
            "already_running",
            "Aguarde a execução atual terminar.",
        ));
    }
    if data.storage_failed {
        return Err(AgentError::storage());
    }
    let options = data
        .turns
        .last()
        .map(|turn| turn.turn.options.clone())
        .ok_or_else(|| {
            AgentError::new(
                "nothing_to_compact",
                "Ainda não há histórico para compactar.",
            )
        })?;
    if !compaction::can_compact(&data) {
        return Err(AgentError::new(
            "nothing_to_compact",
            "Ainda não há histórico suficiente para compactar.",
        ));
    }
    data.manual_compaction = true;
    data.revision += 1;
    let snapshot = session.snapshot_data(&data);
    drop(data);
    (session.emit)(snapshot);
    Ok((CompactionLease(session), options))
}

#[tauri::command]
pub async fn compact_agent_context(
    app: tauri::AppHandle,
    persistence: tauri::State<'_, AppState>,
    oauth: tauri::State<'_, OpenAiCodexState>,
    agent: tauri::State<'_, AgentState>,
    conversation_id: String,
) -> Result<ChatSnapshot, AgentError> {
    let session = agent.existing(&conversation_id)?;
    let home = app.path().home_dir().map_err(|_| AgentError::storage())?;
    crate::core::require_ready(&home)?;
    let hooks = crate::core::hooks::Hooks::new(&home, &session.root, &session.id)?;
    library::agent_location(&persistence, &home, &conversation_id)?;
    let (lease, options) = begin(session.clone())?;
    let state = persistence.inner().clone();
    let oauth = oauth.inner().clone();
    let skill_home = home.clone();
    let auth_options = options.clone();
    let (credential, model) = tauri::async_runtime::spawn_blocking(move || {
        oauth.inference_model(
            &state,
            &home,
            &auth_options.account,
            &auth_options.model,
            auth_options.reasoning.as_deref(),
        )
    })
    .await
    .map_err(|_| AgentError::internal())??;
    session.update(true, |data| {
        data.turns.last_mut().unwrap().turn.context_window = model.context_window;
    })?;
    let (_cancel, signal) = watch::channel(false);
    let beads = crate::core::beads::Beads::new(&skill_home, session.project_id()?, &session.id, options.mode == Mode::Plan)?;
    let beads_snapshot = beads.resume(signal.clone(), || library::agent_location(&persistence, &skill_home, &session.id).map(|_| ()).map_err(|_| crate::core::error("Projeto ou conversa indisponível."))).await?;
    let skills = crate::skills::active(&skill_home, &session.root).await.map_err(|cause| AgentError::new("skill_error", &cause.message))?;
    let mut instructions = tools::instructions(&session.root, options.mode);
    instructions.push_str(crate::core::context::INSTRUCTIONS);
    instructions.push_str(crate::core::beads::INSTRUCTIONS);
    instructions.push_str(&crate::skills::prompt(&skills));
    hooks.before_agent(&mut instructions);
    let mut definitions = tools::definitions(options.mode);
    definitions.extend(crate::core::beads::definitions(options.mode == Mode::Plan));
    if !skills.is_empty() { definitions.extend([crate::skills::definition(), crate::skills::search_definition()]); }
    let overhead = compaction::estimate(&json!({"instructions": instructions, "tools": definitions, "beads_snapshot": beads_snapshot}));
    let result = tokio::time::timeout(
        Duration::from_secs(600),
        compaction::ensure(&session, &credential, &options, overhead, true, signal, Some(&hooks)),
    )
    .await
    .map_err(|_| {
        AgentError::new(
            "compaction_timeout",
            "A compactação excedeu o tempo limite. Tente novamente.",
        )
    })?;
    drop(lease);
    result?;
    session.snapshot()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn manual_compaction_exclusively_locks_chat_without_creating_a_turn_and_always_unlocks() {
        let fixture = crate::agent::tests::Fixture::new();
        let session = crate::agent::tests::session(&fixture);
        let options = TurnOptions {
            account: "test".into(),
            model: "model".into(),
            reasoning: None,
            mode: Mode::Plan,
            approval_mode: ApprovalMode::Manual,
        };
        assert!(begin(session.clone()).is_err());
        session.reserve("Request".into(), options.clone()).unwrap();
        assert!(begin(session.clone()).is_err());
        session.update(true, |data| { data.turns.last_mut().unwrap().wire.push(json!({"type":"message","role":"assistant","content":[{"type":"output_text","text":"Response"}]})); }).unwrap();
        finish(&session, Ok(()));
        let (lease, _) = begin(session.clone()).unwrap();
        assert!(session.snapshot().unwrap().context.compacting);
        assert!(session.submit("blocked".into(), options.clone()).is_err());
        assert!(session.reserve_next().unwrap().is_none());
        assert!(begin(session.clone()).is_err());
        assert_eq!(session.snapshot().unwrap().turns.len(), 1);
        drop(lease);
        assert!(!session.snapshot().unwrap().context.compacting);
        assert!(session.submit("next".into(), options).unwrap().is_some());
    }
}
