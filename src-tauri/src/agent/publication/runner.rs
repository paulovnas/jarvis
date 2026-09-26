//! A publication stays on one blocking worker; its synchronous helpers share
//! this scoped runner without holding chat state or moving control across threads.
use super::{error, AgentError};
use std::{cell::RefCell, process::Output, rc::Rc, time::Duration};
use tokio::{io::AsyncReadExt, sync::watch};

#[derive(Clone)]
struct Control {
    runtime: Rc<tokio::runtime::Runtime>,
    signal: watch::Receiver<bool>,
    cleanup: Option<watch::Receiver<bool>>,
    idle: Duration,
}

thread_local! {
    static CONTROL: RefCell<Option<Control>> = const { RefCell::new(None) };
}

struct Scope(Option<Control>);
impl Drop for Scope {
    fn drop(&mut self) {
        CONTROL.with(|slot| *slot.borrow_mut() = self.0.take());
    }
}

pub(super) fn scoped<T>(
    signal: watch::Receiver<bool>,
    operation: impl FnOnce() -> T,
) -> Result<T, AgentError> {
    with_cleanup(signal, None, Duration::from_secs(120), operation)
}

pub(super) fn scoped_with_cleanup<T>(
    signal: watch::Receiver<bool>,
    cleanup: Option<watch::Receiver<bool>>,
    operation: impl FnOnce() -> T,
) -> Result<T, AgentError> {
    with_cleanup(signal, cleanup, Duration::from_secs(120), operation)
}

#[cfg(test)]
fn with_idle<T>(
    signal: watch::Receiver<bool>,
    idle: Duration,
    operation: impl FnOnce() -> T,
) -> Result<T, AgentError> {
    with_cleanup(signal, None, idle, operation)
}

fn with_cleanup<T>(
    signal: watch::Receiver<bool>,
    cleanup: Option<watch::Receiver<bool>>,
    idle: Duration,
    operation: impl FnOnce() -> T,
) -> Result<T, AgentError> {
    let runtime = Rc::new(
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|_| AgentError::internal())?,
    );
    let _scope = Scope(CONTROL.with(|slot| {
        slot.replace(Some(Control {
            runtime,
            signal,
            cleanup,
            idle,
        }))
    }));
    Ok(operation())
}

pub(super) fn output(command: std::process::Command) -> Result<Output, AgentError> {
    let control = CONTROL.with(|slot| slot.borrow().clone());
    match control {
        Some(control) => control.runtime.clone().block_on(run(command, control)),
        None => {
            let (_cancel, signal) = watch::channel(false);
            // Standalone configuration/probe calls also have a finite idle wait.
            // Production publication wraps its whole operation in one scope.
            std::thread::spawn(move || scoped(signal, || output(command)))
                .join()
                .map_err(|_| AgentError::internal())??
        }
    }
}

async fn run(command: std::process::Command, mut control: Control) -> Result<Output, AgentError> {
    if *control.signal.borrow()
        || control
            .cleanup
            .as_ref()
            .is_some_and(|signal| *signal.borrow())
    {
        return Err(AgentError::cancelled());
    }
    let mut child = crate::agent::shell::spawn_process(tokio::process::Command::from(command))
        .map_err(|_| {
            error(
                "publication_command",
                "Não foi possível iniciar o comando de publicação.",
            )
        })?;
    let mut stdout = child.stdout().take().ok_or_else(AgentError::internal)?;
    let mut stderr = child.stderr().take().ok_or_else(AgentError::internal)?;
    let mut out = Vec::new();
    let mut err = Vec::new();
    let (mut out_open, mut err_open) = (true, true);
    let mut status = None;
    let mut out_buffer = [0; 8192];
    let mut err_buffer = [0; 8192];
    let idle = tokio::time::sleep(control.idle);
    tokio::pin!(idle);
    let result = loop {
        if let Some(status) = status.filter(|_| !out_open && !err_open) {
            break Ok(Output {
                status,
                stdout: out,
                stderr: err,
            });
        }
        let read = tokio::select! {
            biased;
            _ = crate::agent::cancelled(&mut control.signal) => break Err(AgentError::cancelled()),
            _ = cleanup_cancelled(&mut control.cleanup) => break Err(AgentError::cancelled()),
            _ = &mut idle => break Err(error("publication_idle_timeout", "O comando de publicação ficou dois minutos sem saída. Confira o estado do repositório antes de repetir a operação.")),
            result = child.wait(), if status.is_none() => {
                match result {
                    Ok(value) => status = Some(value),
                    Err(_) => break Err(error("publication_command", "Não foi possível acompanhar o comando de publicação.")),
                }
                continue;
            },
            read = stdout.read(&mut out_buffer), if out_open => (true, read),
            read = stderr.read(&mut err_buffer), if err_open => (false, read),
        };
        let (is_stdout, read) = read;
        match read {
            Ok(0) => {
                if is_stdout {
                    out_open = false
                } else {
                    err_open = false
                }
            }
            Ok(count) => {
                let (bytes, buffer) = if is_stdout {
                    (&mut out, &out_buffer)
                } else {
                    (&mut err, &err_buffer)
                };
                if bytes.len().saturating_add(count) > 16 * 1024 * 1024 {
                    break Err(error("publication_output_limit", "A saída da publicação excedeu o limite. Confira o repositório antes de repetir a operação."));
                }
                bytes.extend_from_slice(&buffer[..count]);
                idle.as_mut()
                    .reset(tokio::time::Instant::now() + control.idle);
            }
            Err(_) => {
                break Err(error(
                    "publication_command",
                    "Não foi possível ler a saída da publicação.",
                ))
            }
        }
    };
    // Kill descendants too, including hooks that retain inherited output pipes.
    let _ = Box::into_pin(child.kill()).await;
    let _ = child.wait().await;
    result
}

async fn cleanup_cancelled(signal: &mut Option<watch::Receiver<bool>>) {
    match signal {
        Some(signal) => crate::agent::cancelled(signal).await,
        None => std::future::pending().await,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{io::Write, path::Path, time::Instant};

    const PROBE: &str = "agent::publication::runner::tests::publication_probe_child";

    fn probe(mode: &str, marker: Option<&Path>) -> std::process::Command {
        let mut command = crate::background::command(std::env::current_exe().unwrap());
        command
            .args(["--exact", PROBE, "--ignored", "--nocapture"])
            .env("JARVIS_PUBLICATION_PROBE", mode);
        if let Some(marker) = marker {
            command.env("JARVIS_PUBLICATION_MARKER", marker);
        }
        command
    }

    #[test]
    #[ignore = "Subprocess fixture for publication cancellation and idle timeouts"]
    fn publication_probe_child() {
        match std::env::var("JARVIS_PUBLICATION_PROBE").unwrap().as_str() {
            "pulse" => {
                for _ in 0..8 {
                    println!("still working");
                    std::io::stdout().flush().unwrap();
                    std::thread::sleep(Duration::from_millis(300));
                }
            }
            "tree" => {
                let marker = std::env::var_os("JARVIS_PUBLICATION_MARKER").unwrap();
                let mut child = probe("marker", Some(Path::new(&marker))).spawn().unwrap();
                let _ = child.wait();
                std::thread::sleep(Duration::from_secs(60));
            }
            "marker" => {
                let marker = std::env::var_os("JARVIS_PUBLICATION_MARKER").unwrap();
                std::fs::write(Path::new(&marker).with_extension("started"), "started").unwrap();
                std::thread::sleep(Duration::from_millis(1500));
                std::fs::write(marker, "orphan survived").unwrap();
            }
            _ => std::thread::sleep(Duration::from_secs(60)),
        }
    }

    #[test]
    fn progress_extends_idle_deadline_and_silent_commands_stop() {
        let (_cancel, signal) = watch::channel(false);
        let output = with_idle(signal.clone(), Duration::from_secs(2), || {
            output(probe("pulse", None))
        })
        .unwrap()
        .unwrap();
        assert!(output.status.success());
        assert!(String::from_utf8_lossy(&output.stdout).contains("still working"));
        let started = Instant::now();
        let failure = with_idle(signal, Duration::from_millis(100), || {
            self::output(probe("stall", None))
        })
        .unwrap()
        .unwrap_err();
        assert_eq!(failure.code, "publication_idle_timeout");
        assert!(started.elapsed() < Duration::from_secs(3));
        assert!(CONTROL.with(|slot| slot.borrow().is_none()));
    }

    #[test]
    fn cancellation_reaps_descendants_and_does_not_leak_to_other_workers() {
        let directory = tempfile::tempdir().unwrap();
        let marker = directory.path().join("orphan");
        let started_marker = marker.with_extension("started");
        let (cancel, signal) = watch::channel(false);
        let task = std::thread::spawn(move || {
            scoped(signal, || output(probe("tree", Some(&marker)))).unwrap()
        });
        let (_other_cancel, other_signal) = watch::channel(false);
        let other = std::thread::spawn(move || {
            scoped(other_signal, || output(probe("pulse", None))).unwrap()
        });
        let started = Instant::now();
        while !started_marker.exists() && started.elapsed() < Duration::from_secs(3) {
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(started_marker.exists());
        cancel.send(true).unwrap();
        assert_eq!(task.join().unwrap().unwrap_err().code, "cancelled");
        assert!(other.join().unwrap().unwrap().status.success());
        assert!(!directory.path().join("orphan").exists());
    }

    #[test]
    fn scoped_control_is_restored_after_unwinding_and_nested_operations() {
        let (_cancel, signal) = watch::channel(false);
        scoped(signal.clone(), || {
            let original = CONTROL.with(|slot| slot.borrow().as_ref().unwrap().idle);
            let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                with_idle(signal, Duration::from_millis(1), || panic!("probe")).unwrap();
            }));
            assert!(panic.is_err());
            assert_eq!(
                CONTROL.with(|slot| slot.borrow().as_ref().unwrap().idle),
                original
            );
        })
        .unwrap();
        assert!(CONTROL.with(|slot| slot.borrow().is_none()));
    }
}
