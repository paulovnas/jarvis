//! One exit confirmation for both the window close button and native Quit.
use serde::Serialize;
use std::sync::Mutex;
use tauri::{Emitter, Manager};

#[derive(Default)]
pub(crate) struct ExitState(Mutex<ExitDecision>);

#[derive(Default)]
struct ExitDecision {
    pending: bool,
    allowed: bool,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ShutdownStatus {
    pub(crate) active_chats: usize,
    pub(crate) active_processes: usize,
    pub(crate) restartable_processes: usize,
}

pub(crate) fn status(app: &tauri::AppHandle) -> Result<ShutdownStatus, String> {
    let agent = app.state::<crate::agent::AgentState>();
    let activities = agent
        .terminals
        .shutdown_activity()
        .map_err(|error| error.message().to_owned())?;
    Ok(ShutdownStatus {
        active_chats: agent
            .shutdown_active_chats()
            .map_err(|error| error.message().to_owned())?,
        active_processes: activities.iter().filter(|item| item.active).count(),
        restartable_processes: activities.iter().filter(|item| item.restartable).count(),
    })
}

#[tauri::command]
pub(crate) fn get_app_shutdown_status(app: tauri::AppHandle) -> Result<ShutdownStatus, String> {
    status(&app)
}

#[tauri::command]
pub(crate) fn get_pending_app_exit(
    app: tauri::AppHandle,
    state: tauri::State<'_, ExitState>,
) -> Result<Option<ShutdownStatus>, String> {
    let pending = state
        .0
        .lock()
        .map_err(|_| "Encerramento indisponível.")?
        .pending;
    if pending {
        status(&app).map(Some)
    } else {
        Ok(None)
    }
}

impl ExitDecision {
    fn request(&mut self, status: &ShutdownStatus) -> (bool, bool) {
        if self.allowed || (status.active_processes == 0 && status.active_chats == 0) {
            return (false, false);
        }
        let notify = !self.pending;
        self.pending = true;
        (true, notify)
    }
}

pub(crate) fn allow(app: &tauri::AppHandle) {
    if let Ok(mut decision) = app.state::<ExitState>().0.lock() {
        decision.allowed = true;
        decision.pending = false;
    }
}

/// Returns true when Tauri must keep the app/window alive.
pub(crate) fn request(app: &tauri::AppHandle) -> bool {
    match request_inner(app) {
        Ok(prevent) => prevent,
        Err(error) => {
            let _ = app.emit_to("main", "app:exit-error", error);
            true
        }
    }
}

fn request_inner(app: &tauri::AppHandle) -> Result<bool, String> {
    let state = app.state::<ExitState>();
    if state
        .0
        .lock()
        .map_err(|_| "Encerramento indisponível.")?
        .allowed
    {
        return Ok(false);
    }
    let status = status(app)?;
    let (prevent, notify) = state
        .0
        .lock()
        .map_err(|_| "Encerramento indisponível.")?
        .request(&status);
    if notify {
        if let Some(window) = app.get_webview_window("main") {
            let _ = window.show();
            let _ = window.unminimize();
            let _ = window.set_focus();
        }
        if app.emit_to("main", "app:exit-requested", status).is_err() {
            state
                .0
                .lock()
                .map_err(|_| "Encerramento indisponível.")?
                .pending = false;
            return Err("Não foi possível mostrar a confirmação de encerramento.".into());
        }
    }
    if !prevent {
        crate::prepare_exit(app)?;
    }
    Ok(prevent)
}

#[tauri::command]
pub(crate) fn cancel_app_exit(state: tauri::State<'_, ExitState>) -> Result<(), String> {
    state
        .0
        .lock()
        .map_err(|_| "Encerramento indisponível.")?
        .pending = false;
    Ok(())
}

#[tauri::command]
pub(crate) fn confirm_app_exit(app: tauri::AppHandle) -> Result<(), String> {
    // Save before granting exit. A storage error leaves all processes intact.
    crate::prepare_exit(&app)?;
    app.exit(0);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn activity(chats: usize, processes: usize) -> ShutdownStatus {
        ShutdownStatus {
            active_chats: chats,
            active_processes: processes,
            restartable_processes: 0,
        }
    }

    #[test]
    fn idle_shells_do_not_need_exit_confirmation() {
        assert_eq!(
            ExitDecision::default().request(&activity(0, 0)),
            (false, false)
        );
    }

    #[test]
    fn window_and_native_quit_share_one_pending_confirmation() {
        let mut decision = ExitDecision::default();
        assert_eq!(decision.request(&activity(0, 1)), (true, true));
        assert_eq!(decision.request(&activity(0, 1)), (true, false));
        decision.pending = false;
        assert_eq!(decision.request(&activity(0, 1)), (true, true));
    }

    #[test]
    fn confirmed_restart_bypasses_the_normal_exit_dialog() {
        let mut decision = ExitDecision {
            allowed: true,
            pending: false,
        };
        assert_eq!(decision.request(&activity(1, 1)), (false, false));
    }

    #[test]
    fn an_active_chat_also_requires_confirmation_without_terminal_commands() {
        assert_eq!(
            ExitDecision::default().request(&activity(1, 0)),
            (true, true)
        );
    }
}
