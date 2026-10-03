use super::{install_cancellable, ComponentId};
use std::{fs, process::Stdio};

const CHILD_TEST: &str = "core::install::stack_tests::cancelled_installers_child";
const STACK_MARKER: &str = "JARVIS_CORE_INSTALL_STACK_BYTES";
const STACK_SIZES: [usize; 3] = [256 * 1024, 1024 * 1024, 2 * 1024 * 1024];

#[test]
fn cancelled_installers_leave_stack_headroom() {
    // A stack overflow aborts the process rather than producing a JoinError.
    // Keep each stack-size probe in a subprocess so this test runner survives.
    // Leave room for Tauri's command and IPC futures on the same worker stack.
    for stack_bytes in STACK_SIZES {
        let output = crate::background::command(std::env::current_exe().unwrap())
            .args([
                "--ignored",
                "--exact",
                CHILD_TEST,
                "--nocapture",
                "--test-threads=1",
            ])
            .env(STACK_MARKER, stack_bytes.to_string())
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            output.status.success(),
            "Core installer exceeded its {stack_bytes}-byte thread stack: {}\n{stdout}\n{stderr}",
            output.status
        );
        assert!(
            stdout.contains("1 passed"),
            "The isolated child test was not executed:\n{stdout}\n{stderr}"
        );
    }
}

#[test]
#[ignore = "launched by cancelled_installers_leave_stack_headroom to isolate fatal stack overflows"]
fn cancelled_installers_child() {
    let stack_bytes: usize = std::env::var(STACK_MARKER)
        .expect("Run this fixture through its parent test")
        .parse()
        .unwrap();
    assert!(STACK_SIZES.contains(&stack_bytes));
    std::thread::Builder::new()
        .name("core-install-stack".into())
        .stack_size(stack_bytes)
        .spawn(|| {
            let home = tempfile::tempdir().unwrap();
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            let (sender, signal) = tokio::sync::watch::channel(true);
            runtime.block_on(async {
                for id in ComponentId::ALL {
                    let result = install_cancellable(
                        home.path(),
                        id,
                        |_| panic!("An already cancelled installer must not publish a stage"),
                        |_| panic!("An already cancelled installer must not download"),
                        signal.clone(),
                    )
                    .await;
                    assert_eq!(result.unwrap_err().code, "cancelled", "{id:?}");
                    assert_eq!(fs::read_dir(home.path()).unwrap().count(), 0, "{id:?}");
                }
            });
            // Retain the sender while polling so cancellation comes from its
            // true value, never from a disconnected watch channel.
            drop(sender);
        })
        .unwrap()
        .join()
        .unwrap();
}
