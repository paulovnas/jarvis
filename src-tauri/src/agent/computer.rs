//! Native application control with persistent Manual/YOLO policy and scoped targets.

pub(crate) mod commands;
mod controller;
#[cfg(target_os = "macos")]
mod cursor;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
mod preview;
mod tools;

use super::{AgentError, ToolCall};
use serde::{Deserialize, Serialize};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::Instant,
};

pub(super) use controller::execute_request;
#[cfg(test)]
pub(super) use controller::tests::ready_state;
pub(crate) use controller::ComputerState;
pub(in crate::agent) use tools::{definitions, execute, mutating};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum ComputerMode {
    #[default]
    Manual,
    Yolo,
}

pub(crate) fn configure(app: &tauri::AppHandle) {
    use tauri::{Emitter, Manager};
    let state = app.state::<ComputerState>();
    if let Ok(preferences) = app.state::<crate::system::SystemState>().preferences() {
        if state.mode() != preferences.computer_use_mode
            && state.set_mode(preferences.computer_use_mode).is_ok()
        {
            let _ = app.emit("computer-use-mode-changed", preferences.computer_use_mode);
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct AppIdentity {
    pub bundle_id: String,
    pub bundle_path: String,
    pub executable_path: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum ComputerAccess {
    Observe,
    Control,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Bounds {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ComputerWindow {
    pub id: u32,
    pub title: String,
    pub bounds: Bounds,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ComputerApp {
    pub app_id: String,
    pub name: String,
    pub identity: AppIdentity,
    pub process_id: i32,
    pub launch_id: String,
    pub windows: Vec<ComputerWindow>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ComputerCapabilities {
    pub platform: String,
    pub supported: bool,
    pub accessibility: bool,
    pub screen_capture: bool,
    pub input_posting: bool,
    pub capture_supported: bool,
    pub session_status: String,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum ComputerPermission {
    Accessibility,
    ScreenCapture,
    InputPosting,
}

impl ComputerPermission {
    fn label(&self) -> &'static str {
        match self {
            Self::Accessibility => "Acessibilidade",
            Self::ScreenCapture => "Captura de tela",
            Self::InputPosting => "Envio de entradas",
        }
    }
}

pub(in crate::agent) fn context(app: &tauri::AppHandle) -> Result<String, AgentError> {
    use tauri::Manager;
    let state = app.state::<ComputerState>();
    let capabilities = state.capabilities()?;
    Ok(format!("\nCurrent native computer configuration (recheck computer_status before desktop work): {}. Use its mode as the authority for desktop approvals, independent of command approval mode. If the task needs desktop access and required macOS permissions are missing, explain the missing permissions and the Configure permissions shortcut in the composer before attempting control. macOS permissions are app-wide; starting a new chat does not fix missing OS access. YOLO needs no per-app grants, including for Jarvis itself. Manual uses the computer icon to authorize apps and native per-action review. Preserve the user's requested target and constraints.\n", serde_json::json!({"mode":state.mode(),"capabilities":capabilities})))
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ComputerElement {
    pub id: String,
    pub role: String,
    pub title: String,
    pub value: Option<String>,
    pub bounds: Option<Bounds>,
    pub enabled: bool,
    pub protected: bool,
    pub actions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct CaptureGeometry {
    pub pixel_width: u32,
    pub pixel_height: u32,
    pub scale_x: f64,
    pub scale_y: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Observation {
    pub id: String,
    pub target_id: String,
    pub generation: u64,
    pub revision: u64,
    pub window: ComputerWindow,
    pub elements: Vec<ComputerElement>,
    pub truncated: bool,
    pub capture: Option<CaptureGeometry>,
    pub errors: Vec<String>,
}

pub(super) struct NativeObservation {
    pub window: ComputerWindow,
    pub elements: Vec<ComputerElement>,
    pub truncated: bool,
    pub png: Option<Vec<u8>>,
    pub capture: Option<CaptureGeometry>,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone)]
pub(super) struct NativeTarget {
    pub app: ComputerApp,
    pub window: ComputerWindow,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Point {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub(super) enum ComputerAction {
    Click {
        element: Option<String>,
        x: Option<f64>,
        y: Option<f64>,
    },
    Type {
        text: String,
        element: Option<String>,
    },
    Press {
        key: String,
    },
    Scroll {
        delta_x: f64,
        delta_y: f64,
        x: f64,
        y: f64,
    },
    Drag {
        from: Point,
        to: Point,
    },
}

#[derive(Clone)]
pub(super) struct OperationGuard {
    pub cancelled: Arc<AtomicBool>,
    pub dispatched: Arc<AtomicBool>,
    pub deadline: Instant,
    dispatch_lock: Arc<Mutex<()>>,
    pub(super) signal: Option<tokio::sync::watch::Receiver<bool>>,
    pub(super) feedback: Option<Arc<AtomicBool>>,
    pub(super) context_check: Option<InputContextCheck>,
}

type InputContextCheck = Arc<dyn Fn(&OperationGuard) -> Result<(), AgentError> + Send + Sync>;

impl OperationGuard {
    pub(super) fn new(deadline: Instant) -> Self {
        Self {
            cancelled: Arc::new(AtomicBool::new(false)),
            dispatched: Arc::new(AtomicBool::new(false)),
            deadline,
            dispatch_lock: Arc::default(),
            signal: None,
            feedback: None,
            context_check: None,
        }
    }

    pub fn cancel(&self) -> bool {
        let _lock = self
            .dispatch_lock
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        self.cancelled.store(true, Ordering::SeqCst);
        self.dispatched.load(Ordering::SeqCst)
    }
    pub fn check(&self) -> Result<(), AgentError> {
        if self.cancelled.load(Ordering::SeqCst)
            || self.signal.as_ref().is_some_and(|signal| *signal.borrow())
            || Instant::now() >= self.deadline
        {
            return Err(AgentError::new(
                "computer_stopped",
                "Operação do computador interrompida. Observe o alvo antes de continuar.",
            ));
        }
        Ok(())
    }

    #[cfg(any(target_os = "macos", test))]
    pub fn before_focus(&self) -> Result<(), AgentError> {
        if let Some(check) = &self.context_check {
            self.check()?;
            check(self)?;
        }
        let _lock = self
            .dispatch_lock
            .lock()
            .map_err(|_| AgentError::internal())?;
        self.check()
    }

    #[cfg(any(target_os = "macos", test))]
    pub fn before_dispatch(&self) -> Result<(), AgentError> {
        if let Some(check) = &self.context_check {
            self.check()?;
            check(self)?;
        }
        let _lock = self
            .dispatch_lock
            .lock()
            .map_err(|_| AgentError::internal())?;
        self.check()?;
        self.dispatched.store(true, Ordering::SeqCst);
        Ok(())
    }
}

pub(super) trait NativeBackend: Send + Sync {
    fn capabilities(&self) -> Result<ComputerCapabilities, AgentError>;
    fn apps(&self, guard: &OperationGuard) -> Result<Vec<ComputerApp>, AgentError>;
    fn window(
        &self,
        target: &NativeTarget,
        guard: &OperationGuard,
    ) -> Result<ComputerWindow, AgentError>;
    fn observe(
        &self,
        target: &NativeTarget,
        observation_id: &str,
        capture: bool,
        guard: &OperationGuard,
    ) -> Result<NativeObservation, AgentError>;
    fn act(
        &self,
        target: &NativeTarget,
        observation: &Observation,
        action: &ComputerAction,
        guard: &OperationGuard,
    ) -> Result<(), AgentError>;
    fn request_permission(
        &self,
        permission: ComputerPermission,
    ) -> Result<ComputerCapabilities, AgentError>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn focus_only_failure_does_not_record_input() {
        let guard = OperationGuard::new(Instant::now() + Duration::from_secs(1));
        let result: Result<(), AgentError> = (|| {
            guard.before_focus()?;
            Err(AgentError::new("computer_stale_target", "Target changed"))
        })();
        assert_eq!(result.unwrap_err().code, "computer_stale_target");
        assert!(!guard.dispatched.load(Ordering::SeqCst));
        assert!(!guard.cancel());
    }

    #[test]
    fn cancelled_focus_refuses_the_native_side_effect() {
        let guard = OperationGuard::new(Instant::now() + Duration::from_secs(1));
        assert!(!guard.cancel());
        let result = guard.before_focus();
        assert_eq!(result.unwrap_err().code, "computer_stopped");
        assert!(!guard.dispatched.load(Ordering::SeqCst));
    }

    #[test]
    fn expired_focus_refuses_the_native_side_effect() {
        let guard = OperationGuard::new(Instant::now() - Duration::from_secs(1));
        let result = guard.before_focus();
        assert_eq!(result.unwrap_err().code, "computer_stopped");
        assert!(!guard.dispatched.load(Ordering::SeqCst));
    }

    #[test]
    fn failed_input_keeps_dispatch_for_recovery() {
        let guard = OperationGuard::new(Instant::now() + Duration::from_secs(1));
        let result: Result<(), AgentError> = (|| {
            guard.before_dispatch()?;
            Err(AgentError::new("computer_stale_target", "Target changed"))
        })();
        assert_eq!(result.unwrap_err().code, "computer_stale_target");
        assert!(guard.dispatched.load(Ordering::SeqCst));
        assert!(guard.cancel());
        assert_eq!(guard.check().unwrap_err().code, "computer_stopped");
    }

    #[test]
    fn changed_context_at_the_native_boundary_refuses_focus_and_input() {
        let mut guard = OperationGuard::new(Instant::now() + Duration::from_secs(1));
        guard.context_check = Some(Arc::new(|_| {
            Err(AgentError::new(
                "computer_context_changed",
                "Tab switched after preparation",
            ))
        }));
        assert_eq!(
            guard.before_focus().unwrap_err().code,
            "computer_context_changed"
        );
        assert_eq!(
            guard.before_dispatch().unwrap_err().code,
            "computer_context_changed"
        );
        assert!(!guard.dispatched.load(Ordering::SeqCst));
        assert!(!guard.cancel());
    }

    #[test]
    fn cancellation_during_context_validation_prevents_dispatch_without_holding_the_lock() {
        let mut guard = OperationGuard::new(Instant::now() + Duration::from_secs(1));
        guard.context_check = Some(Arc::new(|guard| {
            if guard.dispatch_lock.try_lock().is_err() {
                return Err(AgentError::new(
                    "cancellation_blocked",
                    "Context validation must not hold the dispatch lock",
                ));
            }
            assert!(!guard.cancel());
            Ok(())
        }));
        assert_eq!(
            guard.before_dispatch().unwrap_err().code,
            "computer_stopped"
        );
        assert!(!guard.dispatched.load(Ordering::SeqCst));
    }

    #[test]
    fn valid_context_keeps_dispatched_input_for_recovery() {
        let mut guard = OperationGuard::new(Instant::now() + Duration::from_secs(1));
        guard.context_check = Some(Arc::new(|_| Ok(())));
        guard.before_dispatch().unwrap();
        assert!(guard.cancel());
        assert!(guard.dispatched.load(Ordering::SeqCst));
        assert_eq!(guard.check().unwrap_err().code, "computer_stopped");
    }
}
