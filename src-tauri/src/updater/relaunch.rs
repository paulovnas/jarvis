//! Relaunch through Tauri so the current process releases the single-instance lock first.

trait RelaunchRuntime {
    fn prepare_exit_for_update(&self);
    fn request_restart(&self);
}

impl RelaunchRuntime for tauri::AppHandle {
    fn prepare_exit_for_update(&self) {
        crate::prepare_exit_for_update(self);
    }

    fn request_restart(&self) {
        tauri::AppHandle::request_restart(self);
    }
}

fn relaunch(runtime: &impl RelaunchRuntime) {
    runtime.prepare_exit_for_update();
    // Tauri starts the successor after its runtime exits. Spawning here would make
    // tauri-plugin-single-instance terminate the updated process during setup.
    runtime.request_restart();
}

pub(crate) fn launch_updated(app: &tauri::AppHandle) {
    relaunch(app);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct RecordedRuntime {
        calls: std::sync::Mutex<Vec<&'static str>>,
    }

    impl RelaunchRuntime for RecordedRuntime {
        fn prepare_exit_for_update(&self) {
            self.calls.lock().unwrap().push("prepare_exit");
        }

        fn request_restart(&self) {
            self.calls.lock().unwrap().push("request_restart");
        }
    }

    #[test]
    fn relaunch_prepares_exit_before_requesting_runtime_restart() {
        let runtime = RecordedRuntime::default();
        relaunch(&runtime);
        assert_eq!(
            *runtime.calls.lock().unwrap(),
            ["prepare_exit", "request_restart"]
        );
    }
}
