//! Turn-owned command sessions. Yielding returns a handle, never a timeout kill.
use super::{cancelled, execution_sandbox::SandboxPlan, AgentError, ToolCall};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, VecDeque},
    path::Path,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncRead, AsyncReadExt},
    sync::{watch, Notify},
};

const OUTPUT_LIMIT: usize = 64 * 1024;
const MAX_RUNNING: usize = 8;
const MAX_RETAINED: usize = 64;

#[derive(Default)]
struct Output {
    bytes: VecDeque<u8>,
    start: u64,
    delivered: u64,
    end: u64,
    finished: bool,
    exit_code: Option<i32>,
    cancelled: bool,
    failed: bool,
    duration_ms: Option<u64>,
}

struct Command {
    output: Mutex<Output>,
    changed: Notify,
    cancel: watch::Sender<bool>,
    started: Instant,
    args: Value,
    sandbox: Option<SandboxPlan>,
}

impl Command {
    fn append(&self, bytes: &[u8]) {
        if let Ok(mut output) = self.output.lock() {
            output.end += bytes.len() as u64;
            output.bytes.extend(bytes);
            let discard = output.bytes.len().saturating_sub(OUTPUT_LIMIT);
            output.bytes.drain(..discard);
            output.start += discard as u64;
        }
        self.changed.notify_one();
    }

    fn snapshot(&self, id: &str, cursor: Option<u64>) -> Result<Value, AgentError> {
        let mut output = self.output.lock().map_err(|_| AgentError::internal())?;
        let requested = cursor.unwrap_or(output.delivered);
        if requested > output.end {
            return Err(AgentError::new(
                "command_cursor",
                "O cursor é posterior à saída disponível.",
            ));
        }
        let offset = requested.max(output.start);
        let mut bytes: Vec<u8> = output
            .bytes
            .iter()
            .skip((offset - output.start) as usize)
            .copied()
            .collect();
        // Keep an incomplete UTF-8 suffix for the next output chunk.
        if !output.finished {
            if let Err(error) = std::str::from_utf8(&bytes) {
                if error.error_len().is_none() {
                    bytes.truncate(error.valid_up_to());
                }
            }
        }
        let cursor = offset + bytes.len() as u64;
        output.delivered = cursor;
        Ok(
            json!({"sessionId":id,"status":if !output.finished {"running"} else if output.cancelled {"cancelled"} else if output.failed {"failed"} else {"completed"},"exitCode":output.exit_code,"output":String::from_utf8_lossy(&bytes),"cursor":cursor,"truncated":requested < output.start,"durationMs":output.duration_ms.unwrap_or_else(|| self.started.elapsed().as_millis() as u64)}),
        )
    }
}

#[derive(Default)]
pub(super) struct CommandSessions {
    commands: BTreeMap<String, Arc<Command>>,
}

impl Drop for CommandSessions {
    fn drop(&mut self) {
        for command in self.commands.values() {
            let _ = command.cancel.send(true);
        }
    }
}

impl CommandSessions {
    pub(super) fn handles(name: &str) -> bool {
        matches!(name, "bash" | "bash_wait" | "bash_cancel")
    }

    pub(super) fn running_ids(&self) -> Vec<String> {
        self.commands
            .iter()
            .filter_map(|(id, command)| {
                command
                    .output
                    .lock()
                    .ok()
                    .filter(|output| !output.finished)
                    .map(|_| id.clone())
            })
            .collect()
    }

    pub(super) async fn execute(
        &mut self,
        root: &Path,
        tool: &ToolCall,
        sandbox: Option<&SandboxPlan>,
        signal: watch::Receiver<bool>,
    ) -> Result<String, AgentError> {
        if *signal.borrow() {
            return Err(AgentError::cancelled());
        }
        let id = if tool.name == "bash" {
            self.start(root, &tool.args, sandbox, signal.clone())?
        } else {
            tool.args["sessionId"]
                .as_str()
                .ok_or_else(|| {
                    AgentError::new(
                        "command_session",
                        "Informe sessionId retornado pelo comando.",
                    )
                })?
                .to_owned()
        };
        let command = self.commands.get(&id).ok_or_else(|| AgentError::new("command_session", "A sessão de comando não pertence a esta execução ou já foi encerrada. Verifique os resultados anteriores antes de iniciar outro comando."))?;
        if tool.name == "bash_cancel" {
            let _ = command.cancel.send(true);
        }
        let wait = tool.args["yieldTimeMs"]
            .as_u64()
            .unwrap_or_else(|| {
                if tool.name == "bash" {
                    tool.args["timeoutSeconds"]
                        .as_u64()
                        .map_or(1_000, |seconds| seconds.saturating_mul(1_000))
                } else {
                    10_000
                }
            })
            .clamp(1, 30_000);
        let cursor = tool.args["cursor"].as_u64();
        let snapshot = wait_for_output(
            command,
            &id,
            cursor,
            Duration::from_millis(wait),
            tool.name == "bash_wait",
            signal,
        )
        .await?;
        if snapshot["status"] == "failed" {
            // Admission recovery must see earlier chunks too, even if the caller
            // already consumed the line that explains the failure.
            let retained_output = {
                let output = command.output.lock().map_err(|_| AgentError::internal())?;
                String::from_utf8_lossy(&output.bytes.iter().copied().collect::<Vec<_>>())
                    .into_owned()
            };
            let mut error = super::execution_sandbox::command_failure(
                command.sandbox.as_ref(),
                &command.args,
                snapshot["exitCode"].as_i64().map(|value| value as i32),
                &retained_output,
            );
            let mut result = error
                .tool_result
                .as_deref()
                .and_then(|value| serde_json::from_str::<Value>(value).ok())
                .unwrap_or_else(|| json!({"error":{"code":error.code,"message":error.message}}));
            result["execution"] = snapshot;
            error.tool_result = Some(result.to_string());
            return Err(error);
        }
        Ok(snapshot.to_string())
    }

    fn start(
        &mut self,
        root: &Path,
        args: &Value,
        sandbox: Option<&SandboxPlan>,
        signal: watch::Receiver<bool>,
    ) -> Result<String, AgentError> {
        if self.running_ids().len() >= MAX_RUNNING {
            return Err(AgentError::new(
                "command_capacity",
                "Aguarde ou encerre um dos oito comandos em execução antes de iniciar outro.",
            ));
        }
        if self.commands.len() >= MAX_RETAINED {
            let completed = self.commands.iter().find_map(|(id, command)| {
                command
                    .output
                    .lock()
                    .ok()
                    .filter(|output| output.finished)
                    .map(|_| id.clone())
            });
            if let Some(id) = completed {
                self.commands.remove(&id);
            }
        }
        let directory = super::tools::scoped(root, args["workdir"].as_str().unwrap_or("."), false)?;
        let text = args["command"]
            .as_str()
            .filter(|command| {
                !command.trim().is_empty() && command.len() <= 16_000 && !command.contains('\0')
            })
            .ok_or_else(|| AgentError::new("command_arguments", "Informe um comando válido."))?;
        let mut child = super::shell::spawn(text, &directory, sandbox)
            .map_err(|_| AgentError::new("command_start", "Não foi possível iniciar o comando."))?;
        let stdout = child.stdout().take().ok_or_else(AgentError::internal)?;
        let stderr = child.stderr().take().ok_or_else(AgentError::internal)?;
        let id = crate::library::new_id().map_err(|_| AgentError::internal())?;
        let (cancel, mut stop) = watch::channel(false);
        let command = Arc::new(Command {
            output: Mutex::new(Output::default()),
            changed: Notify::new(),
            cancel,
            started: Instant::now(),
            args: args.clone(),
            sandbox: sandbox.cloned(),
        });
        self.commands.insert(id.clone(), command.clone());
        tokio::spawn(async move {
            let stdout = tokio::spawn(capture(stdout, command.clone()));
            let stderr = tokio::spawn(capture(stderr, command.clone()));
            let mut signal = signal;
            let status = tokio::select! {
                biased;
                _ = cancelled(&mut signal) => None,
                _ = cancelled(&mut stop) => None,
                result = child.wait() => Some(result),
            };
            // Also reap descendants that kept inherited output pipes open.
            let _ = Box::into_pin(child.kill()).await;
            let _ = child.wait().await;
            finish_capture(stdout).await;
            finish_capture(stderr).await;
            if let Ok(mut output) = command.output.lock() {
                output.finished = true;
                output.cancelled = status.is_none();
                output.failed = status
                    .as_ref()
                    .is_some_and(|result| result.as_ref().map_or(true, |status| !status.success()));
                output.exit_code = status.and_then(Result::ok).and_then(|status| status.code());
                output.duration_ms = Some(command.started.elapsed().as_millis() as u64);
            }
            command.changed.notify_one();
        });
        Ok(id)
    }
}

async fn capture(mut reader: impl AsyncRead + Unpin, command: Arc<Command>) {
    let mut buffer = [0; 8 * 1024];
    loop {
        match reader.read(&mut buffer).await {
            Ok(0) | Err(_) => break,
            Ok(count) => command.append(&buffer[..count]),
        }
    }
}

async fn finish_capture(mut task: tokio::task::JoinHandle<()>) {
    if tokio::time::timeout(Duration::from_secs(2), &mut task)
        .await
        .is_err()
    {
        task.abort();
        let _ = task.await;
    }
}

async fn wait_for_output(
    command: &Command,
    id: &str,
    cursor: Option<u64>,
    wait: Duration,
    yield_on_output: bool,
    mut signal: watch::Receiver<bool>,
) -> Result<Value, AgentError> {
    let deadline = tokio::time::Instant::now() + wait;
    loop {
        let changed = command.changed.notified();
        {
            let output = command.output.lock().map_err(|_| AgentError::internal())?;
            if output.finished
                || (yield_on_output && output.end > cursor.unwrap_or(output.delivered))
            {
                break;
            }
        }
        tokio::select! {
            biased;
            _ = cancelled(&mut signal) => return Err(AgentError::cancelled()),
            _ = tokio::time::sleep_until(deadline) => break,
            _ = changed => {},
        }
    }
    command.snapshot(id, cursor)
}

pub(super) fn definitions() -> Vec<Value> {
    vec![
        super::tools::definition("bash_wait", "Wait for new output or completion of a command started by bash in this execution. Use the returned sessionId and cursor; only new output is returned. Waiting never restarts the command. Prefer a 10-30 second wait over rapid polling. A session cannot be used in another conversation or execution.", json!({"sessionId":{"type":"string","minLength":1},"cursor":{"type":"integer","minimum":0},"yieldTimeMs":{"type":"integer","minimum":1,"maximum":30000}}), &["sessionId"]),
        super::tools::definition("bash_cancel", "Cancel a still-running command started by this execution, including its process tree. This never closes a user terminal or another agent's process. Returns its final status or a handle to wait for shutdown.", json!({"sessionId":{"type":"string","minLength":1},"yieldTimeMs":{"type":"integer","minimum":1,"maximum":30000}}), &["sessionId"]),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(name: &str, args: Value) -> ToolCall {
        ToolCall {
            id: "call".into(),
            name: name.into(),
            args,
            status: "pending".into(),
            output: String::new(),
            duration_ms: 0,
        }
    }

    #[tokio::test]
    async fn short_commands_finish_in_the_initial_call_even_when_they_print_output() {
        let fixture = super::super::tests::Fixture::new();
        let mut sessions = CommandSessions::default();
        let (_cancel, signal) = watch::channel(false);
        let command = if cfg!(windows) {
            "Write-Output complete"
        } else {
            "printf complete"
        };
        let result: Value = serde_json::from_str(
            &sessions
                .execute(
                    &fixture.root,
                    &call("bash", json!({"command":command,"yieldTimeMs":5000})),
                    None,
                    signal,
                )
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(result["status"], "completed");
        assert_eq!(result["output"].as_str().unwrap().trim(), "complete");
        assert!(sessions.running_ids().is_empty());
    }

    #[tokio::test]
    async fn yields_and_resumes_the_same_command_with_incremental_output() {
        let fixture = super::super::tests::Fixture::new();
        let mut sessions = CommandSessions::default();
        let (_cancel, signal) = watch::channel(false);
        let script = if cfg!(windows) {
            "Write-Output first; while (!(Test-Path proceed)) { Start-Sleep -Milliseconds 10 }; Write-Output second"
        } else {
            "printf 'first\\n'; while [ ! -f proceed ]; do sleep 0.01; done; printf 'second\\n'"
        };
        let first: Value = serde_json::from_str(
            &sessions
                .execute(
                    &fixture.root,
                    &call("bash", json!({"command":script,"yieldTimeMs":1000})),
                    None,
                    signal.clone(),
                )
                .await
                .unwrap(),
        )
        .unwrap();
        assert!(first["output"].as_str().unwrap().contains("first"));
        assert_eq!(first["status"], "running");
        std::fs::write(fixture.root.join("proceed"), "continue").unwrap();
        let mut next = first.clone();
        let mut remaining = String::new();
        while next["status"] == "running" {
            next = serde_json::from_str(&sessions.execute(&fixture.root, &call("bash_wait", json!({"sessionId":first["sessionId"],"cursor":next["cursor"],"yieldTimeMs":1000})), None, signal.clone()).await.unwrap()).unwrap();
            assert!(!next["output"].as_str().unwrap().contains("first"));
            remaining.push_str(next["output"].as_str().unwrap());
        }
        assert!(remaining.contains("second"));
        assert_eq!(next["exitCode"], 0);
        assert!(sessions.running_ids().is_empty());
        assert_eq!(
            CommandSessions::default()
                .execute(
                    &fixture.root,
                    &call("bash_wait", json!({"sessionId":first["sessionId"]})),
                    None,
                    signal
                )
                .await
                .unwrap_err()
                .code,
            "command_session"
        );
    }

    #[tokio::test]
    async fn cancel_reaps_a_command_instead_of_restarting_it() {
        let fixture = super::super::tests::Fixture::new();
        let mut sessions = CommandSessions::default();
        let (_cancel, signal) = watch::channel(false);
        let script = if cfg!(windows) {
            "Start-Sleep -Seconds 30"
        } else {
            "sleep 30"
        };
        let start: Value = serde_json::from_str(
            &sessions
                .execute(
                    &fixture.root,
                    &call("bash", json!({"command":script,"yieldTimeMs":5})),
                    None,
                    signal.clone(),
                )
                .await
                .unwrap(),
        )
        .unwrap();
        let stop: Value = serde_json::from_str(
            &sessions
                .execute(
                    &fixture.root,
                    &call("bash_cancel", json!({"sessionId":start["sessionId"]})),
                    None,
                    signal,
                )
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(stop["status"], "cancelled");
    }

    #[test]
    fn bounded_output_reports_truncation_and_does_not_repeat_delivered_bytes() {
        let (cancel, _rx) = watch::channel(false);
        let command = Command {
            output: Mutex::new(Output::default()),
            changed: Notify::new(),
            cancel,
            started: Instant::now(),
            args: Value::Null,
            sandbox: None,
        };
        command.append(&vec![b'a'; OUTPUT_LIMIT + 40]);
        let first = command.snapshot("id", Some(0)).unwrap();
        assert_eq!(first["truncated"], true);
        assert_eq!(first["output"].as_str().unwrap().len(), OUTPUT_LIMIT);
        assert_eq!(command.snapshot("id", None).unwrap()["output"], "");
    }

    #[test]
    fn completed_duration_is_frozen_and_partial_utf8_is_delivered_once_complete() {
        let (cancel, _rx) = watch::channel(false);
        let command = Command {
            output: Mutex::new(Output::default()),
            changed: Notify::new(),
            cancel,
            started: Instant::now(),
            args: Value::Null,
            sandbox: None,
        };
        command.append(&[0xc3]);
        assert_eq!(command.snapshot("id", None).unwrap()["output"], "");
        command.append(&[0xa7]);
        assert_eq!(command.snapshot("id", None).unwrap()["output"], "ç");
        {
            let mut output = command.output.lock().unwrap();
            output.finished = true;
            output.duration_ms = Some(42);
        }
        let final_output = command.snapshot("id", None).unwrap();
        assert_eq!(final_output["output"], "");
        assert_eq!(final_output["durationMs"], 42);
    }
}
