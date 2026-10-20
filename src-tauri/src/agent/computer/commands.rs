use super::{controller::ComputerSnapshot, *};
use crate::{
    agent::{execution_grants::GrantSubject, AgentState},
    library,
    persistence::AppState,
};
use tauri::Manager;

pub(super) async fn require_conversation(
    app: &tauri::AppHandle,
    conversation: &str,
) -> Result<(), AgentError> {
    let app = app.clone();
    let conversation = conversation.to_owned();
    tauri::async_runtime::spawn_blocking(move || {
        let home = crate::data_dir::app_home(&app).map_err(|_| AgentError::storage())?;
        library::agent_location(&app.state::<AppState>(), &home, &conversation)
            .map(|_| ())
            .map_err(AgentError::from)
    })
    .await
    .map_err(|_| AgentError::internal())?
}

#[tauri::command]
pub(crate) async fn get_computer_use_status(
    app: tauri::AppHandle,
    conversation_id: Option<String>,
) -> Result<ComputerSnapshot, AgentError> {
    if let Some(conversation) = &conversation_id {
        require_conversation(&app, conversation).await?;
    }
    let state = app.state::<ComputerState>().inner().clone();
    let grants = app.state::<AgentState>().grants.clone();
    tauri::async_runtime::spawn_blocking(move || {
        state.snapshot(conversation_id.as_deref(), &grants)
    })
    .await
    .map_err(|_| AgentError::internal())?
}

#[tauri::command]
pub(crate) async fn list_computer_apps(
    app: tauri::AppHandle,
) -> Result<Vec<ComputerApp>, AgentError> {
    app.state::<ComputerState>().apps().await
}

#[tauri::command]
pub(crate) async fn request_computer_permission(
    app: tauri::AppHandle,
    permission: ComputerPermission,
) -> Result<ComputerCapabilities, AgentError> {
    app.state::<ComputerState>().permission(permission).await
}

#[tauri::command]
pub(crate) async fn set_computer_use_mode(
    app: tauri::AppHandle,
    mode: ComputerMode,
) -> Result<ComputerSnapshot, AgentError> {
    app.state::<crate::system::SystemState>()
        .save_computer_mode(&app, mode)
        .await
        .map_err(|message| AgentError::new("computer_settings", &message))?;
    app.state::<ComputerState>()
        .snapshot(None, &app.state::<AgentState>().grants)
}

#[tauri::command]
pub(crate) async fn authorize_computer_app(
    app: tauri::AppHandle,
    conversation_id: String,
    app_id: String,
    access: ComputerAccess,
) -> Result<ComputerSnapshot, AgentError> {
    require_conversation(&app, &conversation_id).await?;
    if app_id.is_empty() || app_id.len() > 200 {
        return Err(AgentError::new("computer_invalid", "Aplicativo inválido."));
    }
    let state = app.state::<ComputerState>();
    let selected = state
        .apps()
        .await?
        .into_iter()
        .find(|item| item.app_id == app_id)
        .ok_or_else(|| {
            AgentError::new(
                "computer_stale_app",
                "O aplicativo encerrou ou reiniciou. Atualize a lista.",
            )
        })?;
    let grants = &app.state::<AgentState>().grants;
    // This UI command is the only authority to create an application grant.
    state.revoked(&conversation_id, &selected.identity)?;
    grants
        .create_computer(
            &conversation_id,
            selected.identity,
            access,
            crate::agent::now(),
        )
        .map_err(|message| AgentError::new("computer_grant", &message))?;
    state.authorized(&conversation_id)?;
    state.emit(Some(&app), &conversation_id, grants);
    state.snapshot(Some(&conversation_id), grants)
}

#[tauri::command]
pub(crate) async fn revoke_computer_app(
    app: tauri::AppHandle,
    conversation_id: String,
    grant_id: String,
) -> Result<ComputerSnapshot, AgentError> {
    require_conversation(&app, &conversation_id).await?;
    let grants = &app.state::<AgentState>().grants;
    let selected = grants
        .computer_grants(&conversation_id, crate::agent::now())
        .map_err(|message| AgentError::new("computer_grant", &message))?
        .into_iter()
        .find(|grant| grant.id == grant_id)
        .ok_or_else(|| {
            AgentError::new(
                "computer_grant",
                "Autorização não pertence a esta conversa.",
            )
        })?;
    let state = app.state::<ComputerState>();
    if let GrantSubject::NativeApp { identity, .. } = selected.subject {
        state.revoked(&conversation_id, &identity)?;
    }
    grants
        .revoke_computer(&conversation_id, &grant_id)
        .map_err(|message| AgentError::new("computer_grant", &message))?;
    state.emit(Some(&app), &conversation_id, grants);
    state.snapshot(Some(&conversation_id), grants)
}

#[tauri::command]
pub(crate) async fn stop_computer_use(
    app: tauri::AppHandle,
    conversation_id: String,
) -> Result<ComputerSnapshot, AgentError> {
    require_conversation(&app, &conversation_id).await?;
    let state = app.state::<ComputerState>();
    state.stop(&conversation_id)?;
    // Cancel the owner turn as well so another tool cannot continue after a UI stop.
    if let Ok(session) = app.state::<AgentState>().existing(&conversation_id) {
        if let Ok(mut data) = session.data.lock() {
            if let Some(active) = &mut data.active {
                active.cancel();
            }
        }
    }
    let grants = &app.state::<AgentState>().grants;
    state.emit(Some(&app), &conversation_id, grants);
    state.snapshot(Some(&conversation_id), grants)
}
