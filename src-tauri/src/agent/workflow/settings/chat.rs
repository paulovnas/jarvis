//! Conversation choices are independent of editable agent defaults and running manifests.
use super::*;
use rusqlite::{params, Connection, OptionalExtension};

pub(in crate::agent) fn selection_key(options: &TurnOptions) -> Result<String, AgentError> {
    match options.workflow.unwrap_or_default() {
        Flow::Custom => match (&options.custom_agent_id, &options.custom_workflow_id) {
            (Some(id), None) => Ok(format!("agent:{id}")),
            (None, Some(id)) => Ok(format!("flow:{id}")),
            _ => Err(invalid("Escolha um agente ou fluxo válido.")),
        },
        flow => Ok(key(flow, flow.root())),
    }
}

fn validate_key(home: &Path, selected: &str) -> Result<(), AgentError> {
    if let Some(id) = selected.strip_prefix("agent:") {
        catalog::read(home)?.resolve_agent(id)?;
        return Ok(());
    }
    if let Some(id) = selected.strip_prefix("flow:") {
        catalog::read(home)?.resolve(id)?;
        return Ok(());
    }
    for flow in [
        Flow::Standard,
        Flow::Designer,
        Flow::Video,
        Flow::ImageGenerator,
        Flow::Planned,
        Flow::Complete,
        Flow::Publication,
    ] {
        if selected == key(flow, flow.root()) {
            return Ok(());
        }
    }
    Err(invalid("Escolha um agente ou fluxo válido."))
}

pub(crate) fn binding_key(conversation: &str, selected: &str) -> String {
    format!("chat:{conversation}:agent:{selected}")
}

pub(in crate::agent) fn read(
    db: &Connection,
    conversation: &str,
) -> Result<ModelSettings, AgentError> {
    let mut query = db.prepare("SELECT agent_key,choice FROM conversation_agent_models WHERE conversation_id=?1 ORDER BY agent_key").map_err(|_| AgentError::storage())?;
    let rows = query
        .query_map([conversation], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|_| AgentError::storage())?;
    let mut choices = ModelSettings::new();
    for row in rows {
        let (selected, encoded) = row.map_err(|_| AgentError::storage())?;
        let choice: ModelChoice =
            serde_json::from_str(&encoded).map_err(|_| AgentError::storage())?;
        choice.validate_shape()?;
        choices.insert(
            selected.clone(),
            crate::model_bindings::resolve(db, &binding_key(conversation, &selected), &choice)?,
        );
    }
    Ok(choices)
}

fn write(
    db: &Connection,
    conversation: &str,
    selected: &str,
    choice: &ModelChoice,
) -> Result<(), AgentError> {
    choice.validate_shape()?;
    let encoded = serde_json::to_string(choice).map_err(|_| AgentError::storage())?;
    let tx = db
        .unchecked_transaction()
        .map_err(|_| AgentError::storage())?;
    tx.execute("INSERT INTO conversation_agent_models(conversation_id,agent_key,choice) VALUES(?1,?2,?3) ON CONFLICT(conversation_id,agent_key) DO UPDATE SET choice=excluded.choice", params![conversation,selected,encoded]).map_err(|_| AgentError::storage())?;
    crate::model_bindings::forget_item(&tx, &binding_key(conversation, selected))?;
    tx.commit().map_err(|_| AgentError::storage())?;
    Ok(())
}

pub(in crate::agent) fn resolve_options(
    db: &Connection,
    conversation: &str,
    options: &mut TurnOptions,
) -> Result<bool, AgentError> {
    let selected = selection_key(options)?;
    let encoded: Option<String> = db.query_row("SELECT choice FROM conversation_agent_models WHERE conversation_id=?1 AND agent_key=?2", params![conversation,selected], |row| row.get(0)).optional().map_err(|_| AgentError::storage())?;
    let Some(encoded) = encoded else {
        return Ok(false);
    };
    let source: ModelChoice = serde_json::from_str(&encoded).map_err(|_| AgentError::storage())?;
    if effective_choice(options, Some(&source)) == source {
        let resolved =
            crate::model_bindings::resolve(db, &binding_key(conversation, &selected), &source)?;
        resolved.apply(options);
        if options.model_selection.is_some() {
            options.model_selection = Some(resolved);
        }
    } else if let Some(accepted) = &options.model_selection {
        // Queued dependencies use the legacy key, guarded by their full accepted source.
        let resolved =
            crate::model_bindings::resolve(db, &format!("chat:{conversation}"), accepted)?;
        if &resolved != accepted {
            resolved.apply(options);
            options.model_selection = Some(resolved);
        }
    }
    // A newer saved next-turn choice must never remap the running/queued choice.
    Ok(true)
}

pub(in crate::agent) fn apply_saved(
    db: &Connection,
    conversation: &str,
    options: &mut TurnOptions,
) -> Result<(), AgentError> {
    if let Some(choice) = read(db, conversation)?.get(&selection_key(options)?) {
        choice.apply(options);
    }
    Ok(())
}

pub(in crate::agent) fn capture(
    db: &Connection,
    home: &Path,
    conversation: &str,
    options: &mut TurnOptions,
) -> Result<(), AgentError> {
    // Renderer-provided snapshots are never authority for a freshly accepted message.
    options.model_selection = None;
    let configured = if crate::agent::companion_chat::is_global_session(conversation) {
        None
    } else {
        match read(db, conversation)?.remove(&selection_key(options)?) {
            Some(choice) => Some(choice),
            None => default_choice(db, home, options)?,
        }
    };
    options.model_selection = Some(effective_choice(options, configured.as_ref()));
    Ok(())
}

pub(in crate::agent) fn hydrate_idle(
    state: &AppState,
    home: &Path,
    chat: &crate::agent::ChatSnapshot,
    options: &mut Option<TurnOptions>,
) -> Result<(), AgentError> {
    if chat.active_turn_id.is_none() {
        if let Some(options) = options {
            state.with_connection(home, |db| {
                apply_saved(db, &chat.conversation_id, options)?;
                capture(db, home, &chat.conversation_id, options)
            })?;
        }
    }
    Ok(())
}

pub(in crate::agent) fn effective_choice(
    options: &TurnOptions,
    configured: Option<&ModelChoice>,
) -> ModelChoice {
    let mut choice = ModelChoice {
        executor: options.executor,
        account: options.account.clone(),
        model: options.model.clone(),
        reasoning: options.reasoning.clone(),
        service_tier: options.service_tier,
        fallback: options
            .model_selection
            .as_ref()
            .or(configured)
            .and_then(|model| model.fallback.clone()),
    };
    if choice.fallback.as_ref().is_some_and(|fallback| {
        fallback.executor == choice.executor
            && fallback.account == choice.account
            && fallback.model == choice.model
    }) {
        choice.fallback = None;
    }
    choice
}

pub(in crate::agent) fn selected(
    state: &AppState,
    home: &Path,
    conversation: &str,
    options: &TurnOptions,
) -> Result<Option<ModelChoice>, AgentError> {
    let selected = selection_key(options)?;
    state.with_connection(home, |db| Ok(read(db, conversation)?.remove(&selected)))
}

pub(in crate::agent) fn profiles(
    state: &AppState,
    home: &Path,
    conversation: &str,
    options: &TurnOptions,
) -> Result<ModelSettings, AgentError> {
    let mut profiles = defaults(state, home, conversation)?;
    let selected = selection_key(options)?;
    let saved = self::selected(state, home, conversation, options)?;
    let choice = effective_choice(options, saved.as_ref().or_else(|| profiles.get(&selected)));
    profiles.insert(selected, choice);
    Ok(profiles)
}

pub(in crate::agent) fn defaults(
    state: &AppState,
    home: &Path,
    conversation: &str,
) -> Result<ModelSettings, AgentError> {
    state.with_connection(home, |db| {
        let mut profiles = super::configured(db, home)?;
        let saved = read(db, conversation)?;
        for flow in [
            Flow::Standard,
            Flow::Designer,
            Flow::Video,
            Flow::ImageGenerator,
            Flow::Planned,
            Flow::Complete,
            Flow::Publication,
        ] {
            let profile = key(flow, flow.root());
            let individual = format!(
                "agent:{}",
                flow.root().builtin_id().ok_or_else(AgentError::internal)?
            );
            if let Some(choice) = saved.get(&profile).or_else(|| saved.get(&individual)) {
                profiles.insert(profile, choice.clone());
            }
        }
        Ok(profiles)
    })
}

pub(in crate::agent::workflow) fn worker_choice(
    hub: &Hub,
    flow: Flow,
    role: Role,
) -> Result<Option<ModelChoice>, AgentError> {
    let selected = key(flow, role);
    if let Some(choice) = hub
        .manifest
        .lock()
        .map_err(|_| AgentError::internal())?
        .profiles
        .get(&selected)
        .cloned()
    {
        return Ok(Some(choice));
    }
    // Legacy/global manifests may not have snapshotted a separately invoked specialist.
    Ok(defaults(&hub.env.state, &hub.env.home, &hub.root.id)?.remove(&selected))
}

pub(in crate::agent) fn remember(
    state: &AppState,
    home: &Path,
    conversation: &str,
    options: &TurnOptions,
) -> Result<(), AgentError> {
    let selected = selection_key(options)?;
    let configured = match self::selected(state, home, conversation, options)? {
        Some(choice) => Some(choice),
        None => state.with_connection(home, |db| default_choice(db, home, options))?,
    };
    let choice = effective_choice(options, configured.as_ref());
    state.with_connection(home, |db| {
        let encoded = serde_json::to_string(&choice).map_err(|_| AgentError::storage())?;
        db.execute("INSERT INTO conversation_agent_models(conversation_id,agent_key,choice) VALUES(?1,?2,?3) ON CONFLICT(conversation_id,agent_key) DO NOTHING", params![conversation,selected,encoded]).map_err(|_| AgentError::storage())?;
        Ok(())
    })
}

fn default_choice(
    db: &Connection,
    home: &Path,
    options: &TurnOptions,
) -> Result<Option<ModelChoice>, AgentError> {
    if options.workflow == Some(Flow::Custom) {
        let Some(id) = options.custom_agent_id.as_deref() else {
            return Ok(None);
        };
        let agent = catalog::read_configured(db, home)?.resolve_agent(id)?;
        if let Some((flow, role)) = match agent.native_role {
            Some(Role::Github) => Some((Flow::Publication, Role::Github)),
            Some(Role::Video) => Some((Flow::Video, Role::Video)),
            Some(Role::ImageGenerator) => Some((Flow::ImageGenerator, Role::ImageGenerator)),
            _ => None,
        } {
            return Ok(super::configured(db, home)?.remove(&key(flow, role)));
        }
        Ok(agent.model)
    } else {
        Ok(super::configured(db, home)?.remove(&selection_key(options)?))
    }
}

fn seed_history(
    db: &Connection,
    home: &Path,
    conversation: &str,
    journal: &Path,
) -> Result<(), AgentError> {
    if !read(db, conversation)?.is_empty() || !journal.exists() {
        return Ok(());
    }
    // First checkpoint is the submitted primary; later checkpoints may use a fallback.
    let mut seen = std::collections::BTreeSet::new();
    let mut choices = ModelSettings::new();
    super::super::super::journal::scan(journal, 0, |_, _, record| {
        if record.r#type != "turn_checkpoint" {
            return Ok(());
        }
        let turn = &record.data["turn"];
        let id = turn["id"].as_str().ok_or_else(AgentError::storage)?;
        if !seen.insert(id.to_owned()) {
            return Ok(());
        }
        let mut options: TurnOptions =
            serde_json::from_value(turn["options"].clone()).map_err(|_| AgentError::storage())?;
        // Vacuumed journals can start at a final checkpoint; the receipt retains the primary.
        if let Some(primary) = record.data["wire"].as_array().and_then(|wire| {
            wire.iter().find_map(|item| {
                item.get("_jarvis_model_fallback")
                    .and_then(|marker| marker.get("from"))
            })
        }) {
            let primary: ModelChoice =
                serde_json::from_value(primary.clone()).map_err(|_| AgentError::storage())?;
            primary.validate_shape()?;
            primary.apply(&mut options);
        }
        let selected = selection_key(&options)?;
        // Deleted agents remain visible as a stale selection rather than losing history.
        let configured = default_choice(db, home, &options).ok().flatten();
        let source = options
            .model_selection
            .clone()
            .unwrap_or_else(|| effective_choice(&options, configured.as_ref()));
        let choice = crate::model_bindings::resolve(db, &format!("chat:{conversation}"), &source)?;
        choices.insert(selected, choice);
        Ok(())
    })?;
    for (selected, choice) in choices {
        write(db, conversation, &selected, &choice)?;
    }
    Ok(())
}

#[tauri::command]
pub async fn get_chat_agent_models(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    conversation_id: String,
) -> Result<ModelSettings, AgentError> {
    let state = state.inner().clone();
    let home = app.path().home_dir().map_err(|_| AgentError::storage())?;
    tauri::async_runtime::spawn_blocking(move || {
        let (journal, _) = crate::library::agent_location(&state, &home, &conversation_id)?;
        state.with_connection(&home, |db| {
            seed_history(db, &home, &conversation_id, &journal)?;
            read(db, &conversation_id)
        })
    })
    .await
    .map_err(|_| AgentError::internal())?
}

#[tauri::command]
pub async fn set_chat_agent_model(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    oauth: tauri::State<'_, OpenAiCodexState>,
    conversation_id: String,
    key: String,
    choice: ModelChoice,
) -> Result<ModelSettings, AgentError> {
    let state = state.inner().clone();
    let oauth = oauth.inner().clone();
    let home = app.path().home_dir().map_err(|_| AgentError::storage())?;
    let event_id = conversation_id.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        let (journal, _) = crate::library::agent_location(&state, &home, &conversation_id)?;
        validate_key(&home, &key)?;
        validate_choice(&state, &oauth, &home, &choice)?;
        state.with_connection(&home, |db| {
            let accounts = crate::persistence::list_provider_accounts(db)?;
            if std::iter::once(&choice)
                .chain(choice.fallback.as_deref())
                .any(|selected| {
                    selected.executor == crate::claude::Executor::Jarvis
                        && !accounts
                            .iter()
                            .any(|account| account.alias == selected.account && account.enabled)
                })
            {
                return Err(invalid(
                    "O provedor foi removido ou desativado. Escolha outro modelo.",
                ));
            }
            seed_history(db, &home, &conversation_id, &journal)?;
            write(db, &conversation_id, &key, &choice)?;
            read(db, &conversation_id)
        })
    })
    .await
    .map_err(|_| AgentError::internal())??;
    let _ = app.emit(
        "chat-agent-models:changed",
        json!({"conversationId":event_id}),
    );
    Ok(result)
}

#[cfg(test)]
mod tests;
