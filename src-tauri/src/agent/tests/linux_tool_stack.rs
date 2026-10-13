use std::process::Stdio;

const CHILD_TEST: &str = "agent::tests::linux_tool_stack::native_tool_stack_child";
const STACK_MARKER: &str = "JARVIS_AGENT_STACK_TEST";

#[test]
fn linux_workers_have_headroom_for_nested_native_tools() {
    // Stack overflow aborts the process; keep the worker probe outside this runner.
    let output = crate::background::command(std::env::current_exe().unwrap())
        .args([
            "--ignored",
            "--exact",
            CHILD_TEST,
            "--nocapture",
            "--test-threads=1",
        ])
        .env(STACK_MARKER, "1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "Linux native tool stack probe failed: {}\n{stdout}\n{stderr}",
        output.status
    );
    assert!(
        stdout.contains("1 passed"),
        "The isolated child did not execute:\n{stdout}\n{stderr}"
    );
}

#[inline(never)]
fn nested_native_frame() -> u8 {
    let mut frame = [0_u8; 3 * 1024 * 1024];
    let bytes = std::hint::black_box(&mut frame);
    bytes[0] = 17;
    *bytes.last_mut().unwrap() = 25;
    let bytes = std::hint::black_box(&frame);
    bytes[0] + bytes[bytes.len() - 1]
}

#[test]
#[ignore = "launched by linux_workers_have_headroom_for_nested_native_tools to isolate fatal stack overflows"]
fn native_tool_stack_child() {
    assert_eq!(std::env::var(STACK_MARKER).unwrap(), "1");
    crate::initialize_linux_async_runtime();
    // Initialization is idempotent and retains the runtime while its work executes.
    crate::initialize_linux_async_runtime();
    let answer = tauri::async_runtime::block_on(async {
        tauri::async_runtime::spawn(async { nested_native_frame() })
            .await
            .unwrap()
    });
    assert_eq!(answer, 42);
}
