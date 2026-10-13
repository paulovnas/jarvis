//! Durable queue controls share the same admission lock as the Live pump.
use super::*;

async fn queue_location(
    app: &tauri::AppHandle,
    id: &str,
) -> Result<(PathBuf, PathBuf, u64), CoreError> {
    let (home, project) = location(app, id).await?;
    let active = app
        .state::<ImpeccableLiveState>()
        .entries
        .lock()
        .map_err(|_| error("Estado Live indisponível."))?
        .get(id)
        .map(|entry| (entry.root.clone(), entry.activation_cutoff_ms));
    let (root, cutoff) = match active {
        Some(active) => active,
        None => {
            let root = read_record(&ownership_path(&home, &project))?
                .and_then(|owner| owner["root"].as_str().map(PathBuf::from))
                .or_else(|| {
                    read_record(&project.join(".impeccable/live/app-root.json"))
                        .ok()
                        .flatten()
                        .and_then(|pointer| pointer["appRoot"].as_str().map(PathBuf::from))
                })
                .filter(|root| {
                    root.canonicalize()
                        .is_ok_and(|root| root.starts_with(&project))
                })
                .unwrap_or(project);
            (root, tasks::now_ms())
        }
    };
    Ok((home, root, cutoff))
}

pub(super) async fn refresh(app: &tauri::AppHandle, id: &str) -> Result<LiveStatus, CoreError> {
    let (home, root, cutoff) = queue_location(app, id).await?;
    let tasks = tauri::async_runtime::spawn_blocking(move || {
        tasks::scan(
            &root,
            &home,
            tasks::ScanPolicy {
                activation_cutoff_ms: cutoff,
                recover_action_id: None,
            },
        )
    })
    .await
    .map_err(|_| error("Não foi possível carregar a fila do Designer."))??;
    let state = app.state::<ImpeccableLiveState>();
    let mut entries = state
        .entries
        .lock()
        .map_err(|_| error("Estado Live indisponível."))?;
    let mut status = entries
        .get(id)
        .map(|entry| entry.status.clone())
        .unwrap_or_else(|| LiveStatus::off(id));
    status.tasks = tasks;
    status.queue_count = status
        .tasks
        .iter()
        .filter(|task| task.status == tasks::Status::Pending)
        .count();
    if let Some(entry) = entries.get_mut(id) {
        let changed = serde_json::to_value(&entry.status.tasks).ok()
            != serde_json::to_value(&status.tasks).ok();
        entry.status.tasks = status.tasks.clone();
        entry.status.queue_count = status.queue_count;
        if changed {
            notify(app, &entry.status);
        }
    }
    Ok(status)
}

async fn project_id(app: &tauri::AppHandle, home: &Path, id: &str) -> Result<String, CoreError> {
    let (journal, _) = library::agent_location(&app.state::<AppState>(), home, id)
        .map_err(|_| error("Projeto Live indisponível."))?;
    journal
        .parent()
        .and_then(Path::file_name)
        .and_then(|name| name.to_str())
        .map(str::to_owned)
        .ok_or_else(|| error("Identidade do projeto Live inválida."))
}

pub(super) async fn sync_bead(
    app: &tauri::AppHandle,
    id: &str,
    task: &tasks::Task,
    status: tasks::Status,
    reason: &str,
    editing: bool,
    signal: watch::Receiver<bool>,
) -> Result<String, CoreError> {
    let (home, root, _) = queue_location(app, id).await?;
    let project = project_id(app, &home, id).await?;
    let request = task.request_id.clone();
    let revision = task.revision;
    let check = || {
        let (journal, _) = library::agent_location(&app.state::<AppState>(), &home, id)
            .map_err(|_| error("A conversa ou o projeto Live deixou de estar disponível."))?;
        if journal
            .parent()
            .and_then(Path::file_name)
            .and_then(|name| name.to_str())
            != Some(project.as_str())
        {
            return Err(error("A conversa mudou de projeto. A fila foi preservada."));
        }
        let current = tasks::read(&root, &home)?;
        if current
            .iter()
            .any(|task| task.request_id == request && task.revision == revision)
        {
            Ok(())
        } else {
            Err(error(
                "Esta solicitação já mudou. Atualize a fila do Designer.",
            ))
        }
    };
    let mut synopsis = task.event.clone();
    synopsis["freeformPrompt"] = json!(task.content);
    let bead = match &task.bead_id {
        Some(bead) => bead.clone(),
        None => {
            beads::ensure(
                Some(app),
                &home,
                &project,
                id,
                &task.request_id,
                &synopsis,
                signal.clone(),
                &check,
            )
            .await?
        }
    };
    let status = match status {
        tasks::Status::Pending => beads::Status::Pending,
        tasks::Status::Paused => beads::Status::Paused,
        tasks::Status::Running => beads::Status::Running,
        tasks::Status::Completed => beads::Status::Completed,
        tasks::Status::Cancelled => beads::Status::Cancelled,
        tasks::Status::Blocked => beads::Status::Blocked,
    };
    beads::sync(
        Some(app),
        &home,
        &project,
        id,
        &task.request_id,
        &bead,
        status,
        reason,
        editing.then_some(&synopsis),
        signal,
        check,
    )
    .await?;
    Ok(bead)
}

pub(super) async fn provision(
    app: &tauri::AppHandle,
    id: &str,
    signal: watch::Receiver<bool>,
) -> Result<(), CoreError> {
    let state = app.state::<ImpeccableLiveState>();
    let _operation = state.operations.lock().await;
    let (home, root, _) = queue_location(app, id).await?;
    let tasks = tasks::read(&root, &home)?;
    let mut changed = false;
    for task in tasks
        .iter()
        .filter(|task| task.status == tasks::Status::Pending && task.bead_id.is_none())
    {
        let bead = sync_bead(
            app,
            id,
            task,
            tasks::Status::Pending,
            "Solicitação recebida pela fila do Designer.",
            false,
            signal.clone(),
        )
        .await?;
        tasks::transition(
            &root,
            &home,
            &task.request_id,
            task.status,
            Some(&bead),
            None,
        )?;
        changed = true;
    }
    if changed {
        refresh(app, id).await?;
    }
    Ok(())
}

pub(super) async fn settle(
    app: &tauri::AppHandle,
    id: &str,
    status: tasks::Status,
    reason: &str,
) -> Result<(), CoreError> {
    let (home, root, _) = queue_location(app, id).await?;
    let current = tasks::read(&root, &home)?;
    for task in current
        .iter()
        .filter(|task| task.status == tasks::Status::Running)
    {
        let (_sender, signal) = watch::channel(false);
        let bead = sync_bead(app, id, task, status, reason, false, signal).await?;
        tasks::transition(
            &root,
            &home,
            &task.request_id,
            status,
            Some(&bead),
            task.turn_id.as_deref(),
        )?;
    }
    refresh(app, id).await?;
    Ok(())
}

pub(super) async fn retire_legacy_chat_queue(
    app: &tauri::AppHandle,
    id: &str,
) -> Result<(), CoreError> {
    let tasks = refresh(app, id).await?.tasks;
    let chat = snapshot(app, id).await?;
    for message in chat["queuedMessages"].as_array().into_iter().flatten() {
        let Some(action) = message["content"].as_str().and_then(legacy_action_id) else {
            continue;
        };
        if tasks.iter().any(|task| task.action_id == action) {
            if let Some(message_id) = message["id"].as_str() {
                // The canonical request is preserved in the Designer queue; never drain its old chat copy.
                agent::queue::delete_queued_message(
                    app.clone(),
                    app.state::<AppState>(),
                    app.state::<agent::AgentState>(),
                    id.into(),
                    message_id.into(),
                )
                .await
                .map_err(|cause| error(cause.message()))?;
            }
        }
    }
    Ok(())
}

pub(super) fn legacy_action_id(content: &str) -> Option<&str> {
    content
        .lines()
        .next()?
        .strip_prefix("Impeccable Live · ação ")?
        .split([':', ' '])
        .next()
        .filter(|id| !id.is_empty())
}

#[tauri::command]
pub async fn edit_impeccable_live_task(
    app: tauri::AppHandle,
    conversation_id: String,
    request_id: String,
    expected_revision: u64,
    content: String,
) -> Result<LiveStatus, CoreError> {
    let state = app.state::<ImpeccableLiveState>();
    let _operation = state.operations.lock().await;
    let (home, root, _) = queue_location(&app, &conversation_id).await?;
    let task = tasks::edit(&root, &home, &request_id, expected_revision, &content)?;
    let (_sender, signal) = watch::channel(false);
    let bead = sync_bead(
        &app,
        &conversation_id,
        &task,
        task.status,
        "Pedido editado na fila do Designer.",
        true,
        signal,
    )
    .await?;
    tasks::transition(&root, &home, &request_id, task.status, Some(&bead), None)?;
    refresh(&app, &conversation_id).await
}

#[tauri::command]
pub async fn remove_impeccable_live_task(
    app: tauri::AppHandle,
    conversation_id: String,
    request_id: String,
    expected_revision: u64,
) -> Result<LiveStatus, CoreError> {
    let state = app.state::<ImpeccableLiveState>();
    let _operation = state.operations.lock().await;
    let (home, root, _) = queue_location(&app, &conversation_id).await?;
    let task = tasks::remove(&root, &home, &request_id, expected_revision)?;
    let (_sender, signal) = watch::channel(false);
    let bead = sync_bead(
        &app,
        &conversation_id,
        &task,
        task.status,
        "Removido da fila pelo usuário. O histórico e alterações anteriores foram preservados.",
        false,
        signal,
    )
    .await?;
    tasks::transition(&root, &home, &request_id, task.status, Some(&bead), None)?;
    acknowledge_removed_request(&app, &conversation_id, &task).await?;
    refresh(&app, &conversation_id).await
}

#[tauri::command]
pub async fn resume_impeccable_live_task(
    app: tauri::AppHandle,
    conversation_id: String,
    request_id: String,
    expected_revision: u64,
    options: agent::TurnOptions,
) -> Result<LiveStatus, CoreError> {
    {
        let state = app.state::<ImpeccableLiveState>();
        let _operation = state.operations.lock().await;
        let (home, root, _) = queue_location(&app, &conversation_id).await?;
        let task = tasks::resume(&root, &home, &request_id, expected_revision)?;
        let (_sender, signal) = watch::channel(false);
        let bead = sync_bead(
            &app,
            &conversation_id,
            &task,
            task.status,
            "Retomada solicitada pelo usuário.",
            false,
            signal,
        )
        .await?;
        tasks::transition(&root, &home, &request_id, task.status, Some(&bead), None)?;
    }
    start_impeccable_live(app, conversation_id, options, None).await
}

fn helper_client(timeout: Duration) -> Result<reqwest::Client, CoreError> {
    reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(timeout)
        .build()
        .map_err(|_| error("Conexão Live indisponível."))
}

async fn response_json(mut response: reqwest::Response) -> Result<Value, CoreError> {
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| error("Resposta Live incompleta."))?
    {
        if bytes.len() + chunk.len() > MAX_OUTPUT as usize {
            return Err(error("Resposta Live muito grande."));
        }
        bytes.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&bytes).map_err(|_| error("Resposta Live inválida."))
}

/// Old helpers ignore unknown fields, so protected requests require this
/// capability before using the queue protocol. Boot upgrades owned helpers.
pub(super) async fn helper_compatible(
    identity: &(PathBuf, u16, String),
) -> Result<bool, CoreError> {
    if !verify_helper_identity(identity)? {
        return Ok(false);
    }
    let response = helper_client(Duration::from_secs(3))?
        .get(format!("http://127.0.0.1:{}/health", identity.1))
        .send()
        .await
        .map_err(|_| error("O helper Live não respondeu à verificação nativa."))?;
    if !response.status().is_success() {
        return Ok(false);
    }
    let capabilities = response_json(response).await?;
    Ok(
        capabilities["jarvisQueueProtocol"] == 1
            && capabilities["jarvisDesignBindingProtocol"] == 1,
    )
}

fn request_sequence(task: &tasks::Task) -> Result<u64, CoreError> {
    task.request_id
        .strip_prefix(&format!("{}:", task.action_id))
        .and_then(|sequence| sequence.parse::<u64>().ok())
        .filter(|sequence| *sequence > 0)
        .ok_or_else(|| error("A identidade deste pedido Live é inválida."))
}

fn cancellation_payload(task: &tasks::Task) -> Result<Option<Value>, CoreError> {
    if task.status != tasks::Status::Cancelled {
        return Err(error(
            "Só um pedido removido pode ser encerrado na página Live.",
        ));
    }
    let Some(pending_sequence) = task.event["_jarvisPendingEventSeq"]
        .as_u64()
        .filter(|seq| *seq > 0)
    else {
        // A blocked historical task may have no canonical pending event.
        return Ok(None);
    };
    let source = task.event["type"]
        .as_str()
        .ok_or_else(|| error("Evento Live inválido."))?;
    if task.event["id"] != task.action_id
        || !matches!(
            source,
            "generate"
                | "steer"
                | "manual_edit_apply"
                | "variant_mount_failed"
                | "accept"
                | "discard"
                | "carbonize_cleanup"
        )
    {
        return Err(error("Evento Live inválido."));
    }
    Ok(Some(json!({
        "id":task.action_id,"type":"error","sourceEventType":source,"jarvisCancel":true,
        "jarvisRequestSeq":request_sequence(task)?,"jarvisPendingEventSeq":pending_sequence,
        "message":"Solicitação removida da fila do Designer. As alterações anteriores foram preservadas."
    })))
}

async fn acknowledge_removed_request(
    app: &tauri::AppHandle,
    id: &str,
    task: &tasks::Task,
) -> Result<(), CoreError> {
    let Some(mut payload) = cancellation_payload(task)? else {
        return Ok(());
    };
    let identity = app
        .state::<ImpeccableLiveState>()
        .entries
        .lock()
        .map_err(|_| error("Estado Live indisponível."))?
        .get(id)
        .and_then(|entry| entry.helper_identity.clone());
    let Some(identity) = identity else {
        return Ok(());
    };
    if !helper_compatible(&identity).await? || !verify_helper_identity(&identity)? {
        return Err(error("O pedido foi removido. O helper Live precisa da atualização nativa antes de encerrar sua visualização."));
    }
    payload["token"] = json!(identity.2);
    let response = helper_client(Duration::from_secs(10))?
        .post(format!("http://127.0.0.1:{}/poll", identity.1))
        .json(&payload)
        .send()
        .await
        .map_err(|_| {
            error("O pedido foi removido, mas o helper ainda não confirmou o encerramento visual.")
        })?;
    if response.status() == reqwest::StatusCode::CONFLICT {
        // A newer request/phase replaced this one. Never clear its browser UI.
        return Ok(());
    }
    if !response.status().is_success() || response_json(response).await?["ok"] != true {
        return Err(error(
            "O pedido foi removido, mas o helper ainda não confirmou o encerramento visual.",
        ));
    }
    Ok(())
}

/// A request removed while Live was off keeps its durable tombstone. Once its
/// owned helper is available, settle only that exact canonical pending event.
pub(super) async fn reconcile_cancelled(app: &tauri::AppHandle, id: &str) -> Result<(), CoreError> {
    let (home, root, _) = queue_location(app, id).await?;
    for task in tasks::read(&root, &home)?
        .iter()
        .filter(|task| task.status == tasks::Status::Cancelled)
    {
        if tasks::has_current_pending(&root, task)? {
            acknowledge_removed_request(app, id, task).await?;
        }
    }
    Ok(())
}

pub(super) async fn lease(
    app: &tauri::AppHandle,
    id: &str,
    task: &tasks::Task,
    signal: &mut watch::Receiver<bool>,
) -> Result<Value, CoreError> {
    let identity = app
        .state::<ImpeccableLiveState>()
        .entries
        .lock()
        .map_err(|_| error("Estado Live indisponível."))?
        .get(id)
        .and_then(|entry| entry.helper_identity.clone())
        .ok_or_else(|| error("Helper Live indisponível."))?;
    let request_sequence = request_sequence(task)?.to_string();
    let client = helper_client(Duration::from_secs(180))?;
    // Scoped host polls are protected atomically by the native helper. They
    // never park a callback capable of leasing a later reused action ID.
    let request = async {
        if !helper_compatible(&identity).await? || !verify_helper_identity(&identity)? {
            return Err(error(
                "O helper Live ainda não suporta a fila nativa do Designer.",
            ));
        }
        let mut endpoint = url::Url::parse(&format!("http://127.0.0.1:{}/poll", identity.1))
            .map_err(|_| error("Endereço do helper Live inválido."))?;
        endpoint.query_pairs_mut().extend_pairs([
            ("token", identity.2.as_str()),
            ("id", task.action_id.as_str()),
            ("jarvisRequestSeq", request_sequence.as_str()),
            ("timeout", "1000"),
            ("leaseMs", "600000"),
        ]);
        let response = client
            .get(endpoint)
            .send()
            .await
            .map_err(|_| error("O helper Live não respondeu à solicitação selecionada."))?;
        if response.status() == reqwest::StatusCode::CONFLICT {
            return Ok(json!({"type":"jarvis_request_changed"}));
        }
        if !response.status().is_success() {
            return Err(error("O helper recusou a solicitação Live selecionada."));
        }
        let event = response_json(response).await?;
        if event["type"] != "timeout" && event["id"] != task.action_id {
            return Err(error(
                "O helper respondeu a outra solicitação. Nenhuma ação foi executada.",
            ));
        }
        Ok(event)
    };
    tokio::select! { result = request => result, _ = signal.changed() => Err(super::super::cancelled_error()) }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cancelled_task() -> tasks::Task {
        tasks::Task {
            request_id: "action:1".into(),
            action_id: "action".into(),
            revision: 2,
            content: "Ajustar ícones".into(),
            event: json!({"id":"action","type":"generate","_jarvisPendingEventSeq":8}),
            status: tasks::Status::Cancelled,
            bead_id: None,
            turn_id: None,
            created_at: 1,
        }
    }

    #[test]
    fn legacy_headers_retire_exact_canonical_action_without_draining_other_chat_messages() {
        assert_eq!(
            legacy_action_id("Impeccable Live · ação 794ffcde\nPedido"),
            Some("794ffcde")
        );
        assert_eq!(
            legacy_action_id("Impeccable Live · ação 794ffcde: generate\nPedido"),
            Some("794ffcde")
        );
        assert_eq!(legacy_action_id("Solicitação normal: 794ffcde"), None);
        assert_eq!(legacy_action_id("Impeccable Live · ação "), None);
    }

    #[test]
    fn removal_carries_human_identity_and_exact_pending_phase_without_source_write_payload() {
        let payload = cancellation_payload(&cancelled_task()).unwrap().unwrap();
        assert_eq!(payload["jarvisRequestSeq"], 1);
        assert_eq!(payload["jarvisPendingEventSeq"], 8);
        assert_eq!(payload["sourceEventType"], "generate");
        assert_eq!(payload["jarvisCancel"], true);
        assert!(payload.get("file").is_none());
        assert!(payload.get("data").is_none());
        let mut task = cancelled_task();
        task.request_id = "another:1".into();
        assert!(cancellation_payload(&task).is_err());
        task = cancelled_task();
        task.status = tasks::Status::Running;
        assert!(cancellation_payload(&task).is_err());
        task = cancelled_task();
        task.event
            .as_object_mut()
            .unwrap()
            .remove("_jarvisPendingEventSeq");
        assert!(cancellation_payload(&task).unwrap().is_none());
    }
}
