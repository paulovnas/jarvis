//! Relaunch through Tauri so the current process releases the single-instance lock first.

trait RelaunchRuntime {
    fn prepare_exit_for_update(&self) -> Result<(), String>;
    fn request_restart(&self);
}

impl RelaunchRuntime for tauri::AppHandle {
    fn prepare_exit_for_update(&self) -> Result<(), String> {
        crate::prepare_exit_for_update(self)
    }

    fn request_restart(&self) {
        tauri::AppHandle::request_restart(self);
    }
}

fn relaunch(runtime: &impl RelaunchRuntime) -> Result<(), String> {
    runtime.prepare_exit_for_update()?;
    // Tauri starts the successor after its runtime exits. Spawning here would make
    // tauri-plugin-single-instance terminate the updated process during setup.
    runtime.request_restart();
    Ok(())
}

pub(crate) fn launch_updated(app: &tauri::AppHandle) -> Result<(), String> {
    relaunch(app)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct RecordedRuntime {
        calls: std::sync::Mutex<Vec<&'static str>>,
        storage_error: bool,
    }

    impl RelaunchRuntime for RecordedRuntime {
        fn prepare_exit_for_update(&self) -> Result<(), String> {
            self.calls.lock().unwrap().push("prepare_exit");
            if self.storage_error {
                Err("Storage unavailable".into())
            } else {
                Ok(())
            }
        }

        fn request_restart(&self) {
            self.calls.lock().unwrap().push("request_restart");
        }
    }

    #[test]
    fn relaunch_prepares_exit_before_requesting_runtime_restart() {
        let runtime = RecordedRuntime::default();
        relaunch(&runtime).unwrap();
        assert_eq!(
            *runtime.calls.lock().unwrap(),
            ["prepare_exit", "request_restart"]
        );
    }

    #[test]
    fn relaunch_preserves_the_current_app_when_terminal_state_cannot_be_saved() {
        let runtime = RecordedRuntime {
            storage_error: true,
            ..Default::default()
        };
        assert!(relaunch(&runtime).is_err());
        assert_eq!(*runtime.calls.lock().unwrap(), ["prepare_exit"]);
    }
}
