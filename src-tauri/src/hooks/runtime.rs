//! Frozen manual command hooks. Native Core middleware remains independent.
use super::{Event, Hook, HooksError};
use crate::core::activity::{Activity, Status};
use serde_json::{json, Value};
use std::{
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU16, Ordering},
        Mutex,
    },
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWriteExt},
    sync::watch,
};

const MAX_INPUT: usize = 1024 * 1024;
const MAX_OUTPUT: usize = 64 * 1024;
const MAX_CONTEXT: usize = 16_000;

pub(crate) struct Runtime {
    hooks: Vec<BoundHook>,
    root: PathBuf,
    session: String,
    turn: String,
    lifecycle: AtomicU16,
    mcp: Option<super::mcp_dispatch::Context>,
    home: Option<PathBuf>,
    activity: Mutex<Vec<Activity>>,
}

#[derive(Default)]
pub(crate) struct Outcome {
    pub additional_context: String,
    pub denial: Option<String>,
    pub stop_reason: Option<String>,
    pub diagnostics: Vec<String>,
}

pub(crate) struct Cancelled;
struct BoundHook {
    hook: Hook,
    handler: Handler,
    environment: std::collections::BTreeMap<String, String>,
    plugin: Option<crate::plugins::HookSource>,
}
enum Handler {
    Command,
    Impeccable,
    Mcp {
        server: String,
        tool: String,
        input: Value,
    },
}

struct HookRun<'a> {
    runtime: &'a Runtime,
    bound: &'a BoundHook,
    event: Event,
    started: Instant,
    status: Status,
    summary: String,
}
impl Drop for HookRun<'_> {
    fn drop(&mut self) {
        self.runtime.record_hook(
            self.bound,
            self.event,
            self.status,
            &self.summary,
            self.started,
        );
    }
}

impl Runtime {
    pub(crate) fn load(
        home: &Path,
        root: &Path,
        session: &str,
        turn: &str,
    ) -> Result<Self, HooksError> {
        let mut runtime = Self::with_hooks(
            super::read(home)?.trusted_hooks().cloned().collect(),
            root,
            session,
            turn,
        );
        runtime.home = Some(home.to_owned());
        let overlay =
            crate::plugins::load_active_for_project(home, Some(root)).map_err(|cause| {
                HooksError {
                    code: "plugin_hooks",
                    message: cause.message,
                }
            })?;
        for source in overlay
            .hook_sources
            .into_iter()
            .filter(|source| source.trusted)
        {
            runtime.hooks.extend(plugin_hooks(&source)?);
        }
        if crate::core::design::skill_directory(home).is_ok() {
            prefer_native_impeccable(&mut runtime.hooks);
        }
        Ok(runtime)
    }

    pub(crate) fn inactive() -> Self {
        Self::with_hooks(vec![], Path::new(""), "", "")
    }

    fn with_hooks(hooks: Vec<Hook>, root: &Path, session: &str, turn: &str) -> Self {
        Self {
            hooks: hooks
                .into_iter()
                .map(|hook| BoundHook {
                    hook,
                    handler: Handler::Command,
                    environment: Default::default(),
                    plugin: None,
                })
                .collect(),
            root: root.into(),
            session: session.into(),
            turn: turn.into(),
            lifecycle: AtomicU16::new(0),
            mcp: None,
            home: None,
            activity: Mutex::new(Vec::new()),
        }
    }

    pub(crate) fn needs_mcp(&self) -> bool {
        self.hooks
            .iter()
            .any(|bound| matches!(&bound.handler, Handler::Mcp { .. }))
    }

    pub(crate) fn with_mcp(mut self, context: super::mcp_dispatch::Context) -> Self {
        self.mcp = Some(context);
        self
    }

    pub(crate) fn take_activity(&self) -> Vec<Activity> {
        let mut activity = self
            .activity
            .lock()
            .map(|mut pending| std::mem::take(&mut *pending))
            .unwrap_or_default();
        if let Some(context) = &self.mcp {
            activity.extend(context.take_activity());
        }
        activity
    }

    fn record_hook(
        &self,
        bound: &BoundHook,
        event: Event,
        status: Status,
        summary: &str,
        started: Instant,
    ) {
        let summary: String = summary.chars().take(1_200).collect();
        let mut activity = if matches!(bound.handler, Handler::Impeccable) {
            Activity::new(
                crate::core::ComponentId::Impeccable,
                event.as_str(),
                &summary,
            )
        } else {
            Activity::hook(
                event.as_str(),
                &summary,
                &bound.hook.id,
                &bound.hook.name,
                bound.plugin.as_ref().map(|source| source.plugin_id.clone()),
            )
        };
        activity.status = status;
        activity.duration_ms = started.elapsed().as_millis().try_into().unwrap_or(u64::MAX);
        if let Ok(mut pending) = self.activity.lock() {
            pending.push(activity);
        }
    }

    pub(crate) fn has_blocking_tool_hooks(&self, name: &str) -> bool {
        self.hooks.iter().any(|hook| {
            hook.hook.matches(Event::PreToolUse, Some(name))
                || hook.hook.matches(Event::PermissionRequest, Some(name))
        })
    }

    /// Fallback to another executor must not rerun startup commands in this turn.
    pub(crate) async fn run_once(
        &self,
        event: Event,
        payload: Value,
        signal: watch::Receiver<bool>,
    ) -> Result<Outcome, Cancelled> {
        let bit = match event {
            Event::SessionStart => 1,
            Event::UserPromptSubmit => 2,
            // Completion checks may request another model step and run again.
            Event::Stop => 0,
            Event::SubagentStart => 8,
            Event::SubagentStop => 0,
            Event::Interrupt => 32,
            Event::SessionEnd => 64,
            _ => 0,
        };
        if bit != 0 && self.lifecycle.fetch_or(bit, Ordering::Relaxed) & bit != 0 {
            return Ok(Outcome::default());
        }
        self.run(event, payload, signal).await
    }

    pub(crate) async fn run(
        &self,
        event: Event,
        mut payload: Value,
        signal: watch::Receiver<bool>,
    ) -> Result<Outcome, Cancelled> {
        if cancelled_now(&signal) {
            return Err(Cancelled);
        }
        let matcher = payload["tool_name"]
            .as_str()
            .or_else(|| payload["source"].as_str())
            .or_else(|| payload["trigger"].as_str())
            .or_else(|| payload["agent_type"].as_str())
            .or_else(|| payload["reason"].as_str());
        let hooks: Vec<_> = self
            .hooks
            .iter()
            .filter(|hook| {
                hook.hook.matches(
                    if matches!(hook.handler, Handler::Impeccable) {
                        impeccable_event(event)
                    } else {
                        event
                    },
                    matcher,
                )
            })
            .filter(|hook| {
                !matches!(hook.handler, Handler::Impeccable)
                    || match impeccable_event(event) {
                        Event::PostToolUse => {
                            if !ui_edit(&payload) {
                                false
                            } else {
                                self.lifecycle.fetch_or(256, Ordering::Relaxed);
                                true
                            }
                        }
                        Event::Stop => self.lifecycle.load(Ordering::Relaxed) & 256 != 0,
                        _ => true,
                    }
            })
            .filter(|hook| {
                !matches!(hook.handler, Handler::Impeccable)
                    || impeccable_event(event) != Event::Stop
                    || self.lifecycle.fetch_or(128, Ordering::Relaxed) & 128 == 0
            })
            .collect();
        if hooks.is_empty() {
            return Ok(Outcome::default());
        }
        payload["hook_event_name"] = json!(event);
        payload["session_id"] = json!(self.session);
        payload["turn_id"] = json!(self.turn);
        payload["cwd"] = json!(self.root);
        payload["transcript_path"] = Value::Null;
        let input = serde_json::to_vec(&payload).unwrap_or_default();
        let mut outcome = Outcome::default();
        if input.len() > MAX_INPUT {
            let message = "O evento do hook excedeu 1 MiB; nenhum comando foi executado.";
            for bound in hooks {
                self.record_hook(bound, event, Status::Unavailable, message, Instant::now());
            }
            outcome.diagnostics.push(message.into());
            return Ok(outcome);
        }
        for bound in hooks {
            let hook = &bound.hook;
            let started = Instant::now();
            if let Some(source) = &bound.plugin {
                let home = self.home.clone();
                let root = self.root.clone();
                let source = source.clone();
                let authorized = tokio::task::spawn_blocking(move || {
                    home.is_some_and(|home| {
                        crate::plugins::hook_source_authorized(&home, Some(&root), &source)
                    })
                })
                .await
                .unwrap_or(false);
                if !authorized {
                    let message = format!("{}: o hook do plugin foi desativado, removido, alterado ou teve a autorização revogada; nenhuma ação foi executada.",hook.name);
                    self.record_hook(bound, event, Status::Unavailable, &message, started);
                    outcome.diagnostics.push(message);
                    continue;
                }
            }
            let mut receipt = HookRun {
                runtime: self,
                bound,
                event,
                started,
                status: Status::Unavailable,
                summary: "Hook interrompido; confira eventuais efeitos antes de repetir.".into(),
            };
            let result = match &bound.handler {
                Handler::Command => {
                    execute_with_environment(
                        hook,
                        &self.root,
                        &input,
                        signal.clone(),
                        &bound.environment,
                    )
                    .await
                }
                Handler::Impeccable => match self
                    .home
                    .as_deref()
                    .map(|home| crate::core::design::command(home, &self.root))
                {
                    Some(Ok(mut command)) => {
                        command.arg("hook").env("IMPECCABLE_HOOK_HARNESS", "codex");
                        let input = impeccable_input(event, &payload);
                        execute_process(command, &input, hook.timeout_seconds, signal.clone()).await
                    }
                    _ => Ok(Err("O runtime do Impeccable está indisponível.".into())),
                },
                Handler::Mcp {
                    server,
                    tool,
                    input,
                } => {
                    if let Some(context) = &self.mcp {
                        match expand_mcp_input(input, &payload) {
                            Ok(input) => {
                                super::mcp_dispatch::execute(
                                    context,
                                    server,
                                    tool,
                                    &input,
                                    Duration::from_secs(hook.timeout_seconds),
                                    signal.clone(),
                                )
                                .await
                            }
                            Err(cause) => Ok(Err(cause)),
                        }
                    } else {
                        Ok(Err(
                            "O contexto MCP não está disponível para este hook.".into()
                        ))
                    }
                }
            };
            let result = match result {
                Ok(result) => result,
                Err(cancelled) => {
                    return Err(cancelled);
                }
            };
            let (status, summary): (Status, String) = match result {
                Ok(output) => {
                    let successful_exit = output.code == Some(0);
                    let diagnostics_before = outcome.diagnostics.len();
                    let native_diagnostic = if matches!(bound.handler, Handler::Impeccable) {
                        parse_impeccable(impeccable_event(event), hook, output, &mut outcome)
                    } else {
                        parse(event, hook, output, &mut outcome);
                        false
                    };
                    if outcome.denial.is_some() {
                        (Status::Issues, "O hook bloqueou a ação.".into())
                    } else if outcome.stop_reason.is_some() {
                        (
                            Status::Issues,
                            "O hook solicitou encerrar a execução.".into(),
                        )
                    } else if !successful_exit
                        || native_diagnostic
                        || outcome.diagnostics.len() > diagnostics_before
                    {
                        (
                            if successful_exit {
                                Status::Issues
                            } else {
                                Status::Unavailable
                            },
                            if successful_exit {
                                "O hook concluiu com avisos; consulte o diagnóstico registrado."
                                    .into()
                            } else {
                                "O hook não concluiu; consulte o diagnóstico registrado.".into()
                            },
                        )
                    } else {
                        (Status::Applied, "Hook executado com sucesso".into())
                    }
                }
                Err(message) => {
                    if !matches!(bound.handler, Handler::Impeccable) {
                        outcome
                            .diagnostics
                            .push(format!("{}: {message}", hook.name));
                    }
                    (
                        Status::Unavailable,
                        "O hook não concluiu; consulte o diagnóstico registrado.".into(),
                    )
                }
            };
            receipt.status = status;
            receipt.summary = summary;
            if outcome.denial.is_some() || outcome.stop_reason.is_some() {
                break;
            }
        }
        Ok(outcome)
    }
}

fn expand_mcp_input(template: &Value, event: &Value) -> Result<Value, String> {
    match template {
        Value::Object(fields) => fields
            .iter()
            .map(|(name, value)| Ok((name.clone(), expand_mcp_input(value, event)?)))
            .collect::<Result<serde_json::Map<_, _>, _>>()
            .map(Value::Object),
        Value::Array(values) => values
            .iter()
            .map(|value| expand_mcp_input(value, event))
            .collect::<Result<Vec<_>, _>>()
            .map(Value::Array),
        Value::String(text) => {
            static PATTERN: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
                regex::Regex::new(r"\$\{([^{}]+)\}").expect("static hook input template pattern")
            });
            let resolve = |path: &str| {
                path.split('.')
                    .try_fold(event, |value, key| value.get(key))
                    .ok_or_else(|| {
                        format!(
                            "O campo {path} não existe neste evento; o hook MCP não foi chamado."
                        )
                    })
            };
            let matches: Vec<_> = PATTERN.captures_iter(text).collect();
            if matches.len() == 1
                && matches[0]
                    .get(0)
                    .is_some_and(|found| found.start() == 0 && found.end() == text.len())
            {
                return resolve(&matches[0][1]).cloned();
            }
            let mut result = String::new();
            let mut end = 0;
            for capture in matches {
                let found = capture.get(0).expect("regex whole match");
                result.push_str(&text[end..found.start()]);
                let value = resolve(&capture[1])?;
                match value {
                    Value::String(text) => result.push_str(text),
                    _ => result.push_str(&value.to_string()),
                }
                end = found.end();
            }
            result.push_str(&text[end..]);
            Ok(Value::String(result))
        }
        value => Ok(value.clone()),
    }
}

fn blocking(event: Event) -> bool {
    matches!(
        event,
        Event::PreToolUse
            | Event::PermissionRequest
            | Event::UserPromptSubmit
            | Event::Stop
            | Event::SubagentStop
    )
}
fn cancelled_now(signal: &watch::Receiver<bool>) -> bool {
    *signal.borrow() || signal.has_changed().is_err()
}
async fn cancelled(signal: &mut watch::Receiver<bool>) {
    loop {
        if *signal.borrow_and_update() || signal.changed().await.is_err() {
            return;
        }
    }
}

pub(crate) struct Output {
    pub(crate) code: Option<i32>,
    pub(crate) stdout: String,
    pub(crate) stderr: String,
    pub(crate) truncated: bool,
}

async fn drain(mut stream: impl AsyncRead + Unpin) -> std::io::Result<(String, bool)> {
    let mut captured = Vec::new();
    let mut truncated = false;
    let mut buffer = [0; 4096];
    loop {
        let size = stream.read(&mut buffer).await?;
        if size == 0 {
            break;
        }
        let kept = size.min(MAX_OUTPUT.saturating_sub(captured.len()));
        captured.extend_from_slice(&buffer[..kept]);
        truncated |= kept < size;
    }
    Ok((String::from_utf8_lossy(&captured).into_owned(), truncated))
}

#[cfg(test)]
async fn execute(
    hook: &Hook,
    root: &Path,
    input: &[u8],
    signal: watch::Receiver<bool>,
) -> Result<Result<Output, String>, Cancelled> {
    execute_with_environment(hook, root, input, signal, &Default::default()).await
}
async fn execute_with_environment(
    hook: &Hook,
    root: &Path,
    input: &[u8],
    signal: watch::Receiver<bool>,
    environment: &std::collections::BTreeMap<String, String>,
) -> Result<Result<Output, String>, Cancelled> {
    let child =
        match crate::agent::shell::spawn_hook_with_environment(&hook.command, root, environment) {
            Ok(child) => child,
            Err(_) => return Ok(Err("Não foi possível iniciar o comando.".into())),
        };
    execute_child(child, input, hook.timeout_seconds, signal).await
}

pub(crate) async fn execute_process(
    command: tokio::process::Command,
    input: &[u8],
    timeout_seconds: u64,
    signal: watch::Receiver<bool>,
) -> Result<Result<Output, String>, Cancelled> {
    let child = match crate::agent::shell::spawn_process_with_stdin(
        command,
        std::process::Stdio::piped(),
    ) {
        Ok(child) => child,
        Err(_) => return Ok(Err("Não foi possível iniciar o comando.".into())),
    };
    execute_child(child, input, timeout_seconds, signal).await
}

async fn execute_child(
    mut child: Box<dyn process_wrap::tokio::ChildWrapper>,
    input: &[u8],
    timeout_seconds: u64,
    mut signal: watch::Receiver<bool>,
) -> Result<Result<Output, String>, Cancelled> {
    let (Some(stdout), Some(stderr), Some(mut stdin)) = (
        child.stdout().take(),
        child.stderr().take(),
        child.stdin().take(),
    ) else {
        let _ = child.start_kill();
        return Ok(Err(
            "O comando não disponibilizou seus canais de entrada e saída.".into(),
        ));
    };
    let work = async {
        let write = async move {
            stdin.write_all(input).await?;
            stdin.shutdown().await?;
            drop(stdin);
            Ok::<_, std::io::Error>(())
        };
        let (stdout, stderr, status, written) =
            tokio::join!(drain(stdout), drain(stderr), child.wait(), write);
        // A hook may deliberately decide without reading stdin. Its exit status
        // and decision remain authoritative when the pipe closes early.
        if let Err(error) = written {
            if error.kind() != std::io::ErrorKind::BrokenPipe {
                return Err("O comando não recebeu o evento JSON.".to_string());
            }
        }
        let status = status.map_err(|_| "Não foi possível aguardar o comando.".to_string())?;
        let (stdout, out_truncated) =
            stdout.map_err(|_| "Não foi possível ler a saída do hook.".to_string())?;
        let (stderr, err_truncated) =
            stderr.map_err(|_| "Não foi possível ler o diagnóstico do hook.".to_string())?;
        Ok(Output {
            code: status.code(),
            stdout,
            stderr,
            truncated: out_truncated || err_truncated,
        })
    };
    let result = tokio::select! {
        _ = cancelled(&mut signal) => Err(Cancelled),
        result = tokio::time::timeout(Duration::from_secs(timeout_seconds), work) => Ok(result.unwrap_or_else(|_| Err("Tempo limite atingido; o processo foi interrompido. Confira eventuais efeitos antes de repetir.".into()))),
    };
    if !matches!(&result, Ok(Ok(_))) {
        // Tokio's kill_on_drop terminates only the direct process on Unix.
        // The managed wrapper's explicit kill targets the group / JobObject.
        let _ = child.start_kill();
    }
    result
}

fn bounded(text: &str) -> String {
    text.chars().take(MAX_CONTEXT).collect()
}
fn append(outcome: &mut Outcome, text: &str) {
    if !text.trim().is_empty() {
        outcome.additional_context =
            bounded(&format!("{}\n{}", outcome.additional_context, text.trim()));
    }
}

fn impeccable_hooks() -> Vec<BoundHook> {
    [
        (Event::SessionStart, 5),
        (Event::PostToolUse, 5),
        (Event::Stop, 30),
    ]
    .into_iter()
    .map(|(event, timeout_seconds)| BoundHook {
        hook: Hook {
            id: format!("native-impeccable-{}", event.as_str()),
            name: format!("Impeccable: {}", event.as_str()),
            event,
            command: "Impeccable Core: hook".into(),
            matcher: if event == Event::PostToolUse {
                "write|edit|apply_patch|Write|Edit".into()
            } else {
                String::new()
            },
            timeout_seconds,
            enabled: true,
        },
        handler: Handler::Impeccable,
        environment: Default::default(),
        plugin: None,
    })
    .collect()
}

fn impeccable_event(event: Event) -> Event {
    // A delegated edit deserves the same deep review; this has no notification side effects.
    if event == Event::SubagentStop {
        Event::Stop
    } else {
        event
    }
}

fn ui_edit(payload: &Value) -> bool {
    let input = &payload["tool_input"];
    let ui_path = |path: &str| {
        Path::new(path)
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| {
                matches!(
                    extension.to_ascii_lowercase().as_str(),
                    "html"
                        | "htm"
                        | "css"
                        | "scss"
                        | "sass"
                        | "less"
                        | "jsx"
                        | "tsx"
                        | "vue"
                        | "svelte"
                        | "astro"
                        | "svg"
                )
            })
    };
    if ["path", "file_path"]
        .into_iter()
        .filter_map(|key| input[key].as_str())
        .any(ui_path)
    {
        return true;
    }
    if let Some(patch) = input["patchText"]
        .as_str()
        .or_else(|| input["command"].as_str())
    {
        if patch
            .lines()
            .filter_map(|line| {
                ["*** Add File: ", "*** Update File: ", "*** Move to: "]
                    .into_iter()
                    .find_map(|prefix| line.strip_prefix(prefix))
            })
            .any(ui_path)
        {
            return true;
        }
    }
    // Plain JS/TS can contain UI, but backend edits must not start an unsolicited design loop.
    let script = ["path", "file_path"]
        .into_iter()
        .filter_map(|key| input[key].as_str())
        .any(|path| {
            Path::new(path)
                .extension()
                .is_some_and(|extension| extension == "js" || extension == "ts")
        });
    script
        && ["content", "newText"]
            .into_iter()
            .filter_map(|key| input[key].as_str())
            .any(|text| {
                [
                    "className",
                    "innerHTML",
                    "createElement(",
                    "styled.",
                    "<div",
                    "<button",
                    "<input",
                    "<main",
                ]
                .into_iter()
                .any(|marker| text.contains(marker))
            })
}

fn impeccable_input(event: Event, payload: &Value) -> Vec<u8> {
    let mut payload = payload.clone();
    payload["hook_event_name"] = json!(impeccable_event(event));
    if payload["tool_name"] == "apply_patch" {
        if let Some(patch) = payload["tool_input"]["patchText"]
            .as_str()
            .map(str::to_owned)
        {
            payload["tool_input"]["command"] = json!(patch);
            if let Some(input) = payload["tool_input"].as_object_mut() {
                input.remove("patchText");
            }
        }
    }
    serde_json::to_vec(&payload).unwrap_or_default()
}

fn prefer_native_impeccable(hooks: &mut Vec<BoundHook>) {
    hooks
        .retain(|bound| bound.plugin.is_none() || !equivalent_impeccable_hook(&bound.hook.command));
    hooks.extend(impeccable_hooks());
}

fn equivalent_impeccable_hook(command: &str) -> bool {
    let command = command.replace('\\', "/");
    command.contains("skills/impeccable/scripts/")
        && (command.contains("impeccable") && command.trim_end().ends_with(" hook")
            || command.contains("hook.mjs"))
}

fn parse_impeccable(event: Event, hook: &Hook, output: Output, outcome: &mut Outcome) -> bool {
    let mut advisory = Outcome::default();
    parse(event, hook, output, &mut advisory);
    // Clean scans and detector infrastructure are Core receipts, not user-facing tasks.
    if !advisory
        .additional_context
        .to_ascii_lowercase()
        .contains("no deterministic")
    {
        append(outcome, &advisory.additional_context);
    }
    let diagnostic = !advisory.diagnostics.is_empty();
    if event == Event::Stop {
        // The runtime gives this native review one correction pass; it cannot trap a turn.
        outcome.denial = advisory.denial;
    }
    diagnostic
}

fn parse(event: Event, hook: &Hook, output: Output, outcome: &mut Outcome) {
    if output.truncated {
        outcome.diagnostics.push(format!(
            "{}: a saída excedeu 64 KiB e foi truncada.",
            hook.name
        ));
        return;
    }
    if output.code == Some(2) {
        let reason = bounded(&output.stderr);
        if reason.trim().is_empty() {
            outcome.diagnostics.push(format!(
                "{}: código 2 sem uma justificativa; a falha do hook não alterou a autorização.",
                hook.name
            ));
        } else if blocking(event) {
            outcome.denial = Some(reason);
        } else {
            outcome.diagnostics.push(format!("{}: {reason}", hook.name));
        }
        return;
    }
    if output.code != Some(0) {
        outcome.diagnostics.push(format!(
            "{}: comando terminou com código {:?}. {}",
            hook.name,
            output.code,
            bounded(&output.stderr)
        ));
        return;
    }
    let text = output.stdout.trim();
    if text.is_empty() {
        return;
    }
    let value: Value = match serde_json::from_str(text) {
        Ok(value) => value,
        Err(_) if !text.starts_with('{') => {
            append(outcome, text);
            return;
        }
        Err(_) => {
            outcome
                .diagnostics
                .push(format!("{}: resposta JSON inválida.", hook.name));
            return;
        }
    };
    let specific = &value["hookSpecificOutput"];
    if specific["hookEventName"]
        .as_str()
        .is_some_and(|name| name != event.as_str())
    {
        outcome.diagnostics.push(format!(
            "{}: resposta pertence a outro evento de hook.",
            hook.name
        ));
        return;
    }
    if let Some(context) = specific["additionalContext"].as_str() {
        append(outcome, context);
    }
    if let Some(message) = value["systemMessage"].as_str() {
        outcome
            .diagnostics
            .push(format!("{}: {}", hook.name, bounded(message)));
    }
    let permission = specific["permissionDecision"].as_str();
    let behavior = specific["decision"]["behavior"].as_str();
    if value["continue"] == false {
        outcome.stop_reason = Some(bounded(
            value["stopReason"]
                .as_str()
                .map(str::trim)
                .filter(|reason| !reason.is_empty())
                .unwrap_or("O hook solicitou encerrar esta execução."),
        ));
        return;
    }
    if matches!(event, Event::PreToolUse | Event::PermissionRequest)
        && (permission == Some("deny") || behavior == Some("deny") || value["decision"] == "block")
    {
        outcome.denial = Some(bounded(
            specific["permissionDecisionReason"]
                .as_str()
                .or_else(|| specific["decision"]["message"].as_str())
                .or_else(|| value["reason"].as_str())
                .unwrap_or("O hook recusou esta ação."),
        ));
    }
    if matches!(
        event,
        Event::UserPromptSubmit | Event::Stop | Event::SubagentStop
    ) && value["decision"] == "block"
    {
        if let Some(reason) = value["reason"]
            .as_str()
            .map(str::trim)
            .filter(|reason| !reason.is_empty())
        {
            outcome.denial = Some(bounded(reason));
        } else {
            outcome.diagnostics.push(format!("{}: decisão de bloqueio sem uma justificativa; a falha do hook não interrompeu a execução.",hook.name));
        }
    }
    if permission.is_some_and(|value| value != "deny")
        || behavior.is_some_and(|value| value != "deny")
        || value["decision"] == "approve"
        || specific.get("updatedInput").is_some()
        || (!blocking(event) && value["decision"] == "block")
    {
        outcome.diagnostics.push(format!("{}: alteração de argumentos, aprovação automática e controle do turno não são suportados; a autorização do Jarvis foi preservada.", hook.name));
    }
}

fn plugin_hooks(source: &crate::plugins::HookSource) -> Result<Vec<BoundHook>, HooksError> {
    use sha2::{Digest, Sha256};
    let mut environment = std::collections::BTreeMap::new();
    for (name, path) in [
        ("CODEX_PLUGIN_ROOT", &source.root),
        ("CLAUDE_PLUGIN_ROOT", &source.root),
        ("CODEX_PLUGIN_DATA", &source.data_path),
        ("CLAUDE_PLUGIN_DATA", &source.data_path),
    ] {
        environment.insert(name.into(), path.to_string_lossy().into_owned());
    }
    for (name, folder) in [("CODEX_HOME", "codex"), ("CLAUDE_CONFIG_DIR", "claude")] {
        let directory = source.data_path.join(folder);
        std::fs::create_dir_all(&directory).map_err(|_| HooksError {
            code: "plugin_hooks",
            message: "Não foi possível preparar o ambiente privado do plugin.".into(),
        })?;
        environment.insert(name.into(), directory.to_string_lossy().into_owned());
    }
    let events = source
        .definition
        .get("hooks")
        .unwrap_or(&source.definition)
        .as_object()
        .ok_or_else(|| HooksError {
            code: "plugin_hooks",
            message: "O catálogo de hooks do plugin é inválido.".into(),
        })?;
    let mut hooks = Vec::new();
    for (event_name, groups) in events {
        let event: Event = serde_json::from_value(json!(event_name)).map_err(|_| HooksError {
            code: "plugin_hooks",
            message: format!("O evento {event_name} ainda não está disponível no Jarvis."),
        })?;
        if event == Event::BeforeAgent {
            continue;
        }
        for (group_index, group) in groups.as_array().into_iter().flatten().enumerate() {
            let matcher = group["matcher"].as_str().unwrap_or("");
            for (index, entry) in group["hooks"].as_array().into_iter().flatten().enumerate() {
                let handler = match entry["type"].as_str() {
                    Some("command") => Handler::Command,
                    Some("mcp_tool") if event != Event::SessionEnd => Handler::Mcp {
                        server: {
                            let server = entry["server"].as_str().unwrap_or("");
                            if server.starts_with("plugin-")
                                || server.starts_with("builtin-")
                                || (server.len() == 32
                                    && server.bytes().all(|byte| byte.is_ascii_hexdigit()))
                            {
                                server.into()
                            } else {
                                crate::mcp::plugin_server_id(
                                    &source.plugin_id,
                                    &format!("mcp:{server}"),
                                )
                            }
                        },
                        tool: entry["tool"].as_str().unwrap_or("").into(),
                        input: entry.get("input").cloned().unwrap_or_else(|| json!({})),
                    },
                    _ => continue,
                };
                let command = match &handler {
                    Handler::Command => entry["command"].as_str().unwrap_or("").to_owned(),
                    Handler::Mcp {
                        server,
                        tool,
                        input,
                    } => format!("MCP {server}/{tool}: {input}"),
                    Handler::Impeccable => unreachable!("native hooks are not plugin entries"),
                };
                let digest = Sha256::digest(
                    format!(
                        "{}\0{}\0{event_name}\0{group_index}\0{index}",
                        source.plugin_id, source.component_id
                    )
                    .as_bytes(),
                );
                let hook = Hook {
                    id: digest[..16]
                        .iter()
                        .map(|byte| format!("{byte:02x}"))
                        .collect(),
                    name: format!("{}: {event_name}", source.name),
                    event,
                    command,
                    matcher: matcher.into(),
                    timeout_seconds: entry["timeout"].as_u64().unwrap_or(600),
                    enabled: true,
                };
                super::validate(&hook)?;
                hooks.push(BoundHook {
                    hook,
                    handler,
                    environment: environment.clone(),
                    plugin: Some(source.clone()),
                });
            }
        }
    }
    Ok(hooks)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_quality_hooks_adapt_patch_and_child_completion_without_changing_manual_contracts() {
        let patch =
            "*** Begin Patch\n*** Update File: src/Screen.tsx\n@@\n-Old\n+New\n*** End Patch";
        let payload = json!({"tool_name":"apply_patch","tool_input":{"patchText":patch},"session_id":"child","turn_id":"turn"});
        assert!(ui_edit(&payload));
        let adapted: Value =
            serde_json::from_slice(&impeccable_input(Event::PostToolUse, &payload)).unwrap();
        assert_eq!(adapted["tool_input"]["command"], patch);
        assert_eq!(adapted["session_id"], "child");
        assert!(payload["tool_input"].get("command").is_none());
        let adapted: Value =
            serde_json::from_slice(&impeccable_input(Event::SubagentStop, &payload)).unwrap();
        assert_eq!(adapted["hook_event_name"], "Stop");
        assert!(!ui_edit(
            &json!({"tool_name":"edit","tool_input":{"path":"src/database.ts","newText":"return rows;"}})
        ));
        assert!(ui_edit(
            &json!({"tool_name":"write","tool_input":{"path":"src/dom.js","content":"node.innerHTML = '<button>Go</button>';"}})
        ));
    }

    #[tokio::test]
    async fn native_design_review_is_internal_once_and_only_after_ui_edits() {
        let root = tempfile::tempdir().unwrap();
        let mut runtime = Runtime::with_hooks(vec![], root.path(), "session", "turn");
        runtime.hooks = impeccable_hooks();
        let (_sender, signal) = watch::channel(false);
        for (event, payload) in [
            (Event::Stop, json!({})),
            (
                Event::PostToolUse,
                json!({"tool_name":"write","tool_input":{"path":"server.rs","content":"fn main() {}"}}),
            ),
            (Event::SubagentStop, json!({"agent_type":"designer"})),
        ] {
            runtime
                .run(event, payload, signal.clone())
                .await
                .unwrap_or_else(|_| panic!("cancelled"));
            assert!(runtime.take_activity().is_empty());
        }
        let outcome = runtime
            .run(
                Event::PostToolUse,
                json!({"tool_name":"write","tool_input":{"path":"src/Screen.tsx"}}),
                signal.clone(),
            )
            .await
            .unwrap_or_else(|_| panic!("cancelled"));
        assert!(
            outcome.diagnostics.is_empty(),
            "Unavailable native engines remain internal"
        );
        assert!(outcome.additional_context.is_empty());
        let receipts = runtime.take_activity();
        assert_eq!(receipts.len(), 1);
        assert_eq!(
            receipts[0].component,
            crate::core::ComponentId::Impeccable.into()
        );
        assert_eq!(receipts[0].status, Status::Unavailable);
        runtime
            .run(Event::SubagentStop, json!({}), signal.clone())
            .await
            .unwrap_or_else(|_| panic!("cancelled"));
        let receipts = runtime.take_activity();
        assert_eq!(receipts.len(), 1);
        assert_eq!(receipts[0].action, "SubagentStop");
        runtime
            .run(Event::SubagentStop, json!({}), signal)
            .await
            .unwrap_or_else(|_| panic!("cancelled"));
        assert!(runtime.take_activity().is_empty());
    }

    #[test]
    fn native_design_hints_suppress_clean_and_infrastructure_output_but_keep_real_findings() {
        let hook = impeccable_hooks().remove(1).hook;
        let output = |value: Value| Output {
            code: Some(0),
            stdout: value.to_string(),
            stderr: String::new(),
            truncated: false,
        };
        let mut outcome = Outcome::default();
        assert!(!parse_impeccable(
            Event::PostToolUse,
            &hook,
            output(
                json!({"hookSpecificOutput":{"additionalContext":"No deterministic design findings."}})
            ),
            &mut outcome
        ));
        assert!(outcome.additional_context.is_empty());
        parse_impeccable(
            Event::PostToolUse,
            &hook,
            output(
                json!({"hookSpecificOutput":{"additionalContext":"Screen.tsx: primary action is unreadable."}}),
            ),
            &mut outcome,
        );
        assert!(outcome
            .additional_context
            .contains("primary action is unreadable"));
        assert!(parse_impeccable(
            Event::PostToolUse,
            &hook,
            Output {
                code: Some(1),
                stdout: String::new(),
                stderr: "Missing engine dependency".into(),
                truncated: false
            },
            &mut outcome
        ));
        assert!(outcome.diagnostics.is_empty());
        parse_impeccable(
            Event::Stop,
            &hook,
            output(
                json!({"decision":"block","reason":"Fix the unreadable primary action in the requested screen."}),
            ),
            &mut outcome,
        );
        assert!(outcome
            .denial
            .unwrap()
            .contains("unreadable primary action"));
    }

    #[test]
    fn native_quality_hooks_replace_equivalent_plugin_hooks_and_preserve_other_hooks() {
        let root = tempfile::tempdir().unwrap();
        let source = crate::plugins::HookSource {
            plugin_id: "impeccable@local".into(),
            plugin_hash: "immutable".into(),
            component_id: "hooks:main".into(),
            name: "Impeccable".into(),
            definition: json!({"hooks":{"PostToolUse":[{"matcher":"Edit|Write","hooks":[{"type":"command","command":"\"${CODEX_PLUGIN_ROOT}/skills/impeccable/scripts/impeccable\" hook"},{"type":"command","command":"echo another hook"}]}]}}),
            root: root.path().into(),
            data_path: root.path().join("data"),
            trusted: true,
        };
        let mut hooks = plugin_hooks(&source).unwrap();
        let manual = Hook {
            id: "manual".into(),
            name: "Manual design check".into(),
            event: Event::PostToolUse,
            command: "\"/local/skills/impeccable/scripts/impeccable\" hook".into(),
            matcher: String::new(),
            timeout_seconds: 5,
            enabled: true,
        };
        hooks.extend(Runtime::with_hooks(vec![manual], root.path(), "s", "t").hooks);
        prefer_native_impeccable(&mut hooks);
        assert_eq!(hooks.len(), 5);
        assert!(hooks
            .iter()
            .any(|bound| bound.hook.command == "echo another hook"));
        assert!(hooks.iter().any(|bound| bound.hook.id == "manual"));
        assert_eq!(
            hooks
                .iter()
                .filter(|bound| matches!(bound.handler, Handler::Impeccable))
                .count(),
            3
        );
        assert!(equivalent_impeccable_hook(
            "\"C:\\plugin\\skills\\impeccable\\scripts\\impeccable.cmd\" hook"
        ));
        assert!(!equivalent_impeccable_hook("echo unrelated hook"));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn managed_native_process_preserves_literal_arguments() {
        let root = tempfile::tempdir().unwrap();
        let literal = "hello; touch escaped; $(touch another)";
        let mut command = tokio::process::Command::new("/usr/bin/printf");
        command.current_dir(root.path()).args(["%s", literal]);
        let (_sender, signal) = watch::channel(false);
        let output = execute_process(command, b"", 5, signal)
            .await
            .unwrap_or_else(|_| panic!("cancelled"))
            .unwrap();
        assert_eq!(output.stdout, literal);
        assert!(!root.path().join("escaped").exists());
        assert!(!root.path().join("another").exists());
    }
    #[test]
    fn mcp_hook_input_templates_preserve_json_types_and_missing_fields_never_dispatch() {
        let event = json!({"hook_event_name":"PreToolUse","tool_input":{"count":3,"path":"app.ts","safe":true}});
        let expanded = expand_mcp_input(&json!({"count":"${tool_input.count}","nested":{"safe":"${tool_input.safe}"},"list":["${tool_input}"],"label":"${hook_event_name}: ${tool_input.path}"}),&event).unwrap();
        assert_eq!(expanded["count"], 3);
        assert_eq!(expanded["nested"]["safe"], true);
        assert_eq!(expanded["list"][0], event["tool_input"]);
        assert_eq!(expanded["label"], "PreToolUse: app.ts");
        assert!(
            expand_mcp_input(&json!({"value":"${tool_input.missing}"}), &event)
                .unwrap_err()
                .contains("não foi chamado")
        );
    }
    #[test]
    fn lifecycle_blocks_require_feedback_and_never_turn_failures_into_blocks() {
        for event in [Event::UserPromptSubmit, Event::Stop, Event::SubagentStop] {
            let hook = hook(event, "fixture");
            for (code, stdout, stderr) in [
                (
                    0,
                    r#"{"decision":"block","reason":"Verify the evidence first."}"#,
                    "",
                ),
                (2, "", "Verify the evidence first."),
            ] {
                let mut outcome = Outcome::default();
                parse(
                    event,
                    &hook,
                    Output {
                        code: Some(code),
                        stdout: stdout.into(),
                        stderr: stderr.into(),
                        truncated: false,
                    },
                    &mut outcome,
                );
                assert_eq!(
                    outcome.denial.as_deref(),
                    Some("Verify the evidence first.")
                );
            }
            for (code, stdout, stderr) in [
                (0, r#"{"decision":"block","reason":" "}"#, ""),
                (2, "", ""),
                (1, "", "unexpected failure"),
            ] {
                let mut outcome = Outcome::default();
                parse(
                    event,
                    &hook,
                    Output {
                        code: Some(code),
                        stdout: stdout.into(),
                        stderr: stderr.into(),
                        truncated: false,
                    },
                    &mut outcome,
                );
                assert!(outcome.denial.is_none());
                assert!(!outcome.diagnostics.is_empty());
            }
        }
        let mut outcome = Outcome::default();
        parse(Event::Stop,&hook(Event::Stop,"fixture"),Output { code:Some(0),stdout:r#"{"continue":false,"stopReason":"Waiting for the user.","decision":"block","reason":"Do not continue."}"#.into(),stderr:String::new(),truncated:false },&mut outcome);
        assert_eq!(
            outcome.stop_reason.as_deref(),
            Some("Waiting for the user.")
        );
        assert!(outcome.denial.is_none());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn completion_hooks_can_recheck_after_requested_continuation() {
        let root = tempfile::tempdir().unwrap();
        let runtime = Runtime::with_hooks(
            vec![hook(
                Event::Stop,
                r#"printf '%s' '{"decision":"block","reason":"Check again."}'"#,
            )],
            root.path(),
            "session",
            "turn",
        );
        let (_sender, signal) = watch::channel(false);
        for active in [false, true] {
            let outcome = runtime
                .run_once(
                    Event::Stop,
                    json!({"stop_hook_active":active}),
                    signal.clone(),
                )
                .await
                .unwrap_or_else(|_| panic!("cancelled"));
            assert_eq!(outcome.denial.as_deref(), Some("Check again."));
        }
    }

    fn hook(event: Event, command: &str) -> Hook {
        Hook {
            id: "test".into(),
            name: "Teste".into(),
            event,
            command: command.into(),
            matcher: "".into(),
            timeout_seconds: 1,
            enabled: true,
        }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn silent_hook_receipts_only_report_executed_matches_and_startup_once() {
        let root = tempfile::tempdir().unwrap();
        let mut enabled = hook(Event::SessionStart, "cat >/dev/null");
        enabled.id = "quiet".into();
        enabled.name = "Quiet startup".into();
        let mut disabled = enabled.clone();
        disabled.id = "disabled".into();
        disabled.enabled = false;
        let other_event = hook(Event::PostToolUse, "touch forbidden");
        let runtime = Runtime::with_hooks(
            vec![enabled, disabled, other_event],
            root.path(),
            "session",
            "turn",
        );
        let (_sender, signal) = watch::channel(false);
        let outcome = runtime
            .run_once(Event::SessionStart, json!({}), signal.clone())
            .await
            .unwrap_or_else(|_| panic!("cancelled"));
        assert!(outcome.diagnostics.is_empty());
        assert!(outcome.additional_context.is_empty());
        let receipts = runtime.take_activity();
        assert_eq!(receipts.len(), 1);
        assert_eq!(
            receipts[0].component,
            crate::core::activity::ActivityComponent::Hooks
        );
        assert_eq!(receipts[0].resource_id.as_deref(), Some("quiet"));
        assert_eq!(receipts[0].resource_name.as_deref(), Some("Quiet startup"));
        assert_eq!(receipts[0].action, "SessionStart");
        assert_eq!(receipts[0].status, Status::Applied);
        assert!(receipts[0].plugin_id.is_none());
        assert!(!root.path().join("forbidden").exists());
        assert!(runtime.take_activity().is_empty());
        assert!(runtime
            .run_once(Event::SessionStart, json!({}), signal)
            .await
            .is_ok());
        assert!(runtime.take_activity().is_empty());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn oversized_hook_event_records_unavailability_without_running_commands() {
        let root = tempfile::tempdir().unwrap();
        let runtime = Runtime::with_hooks(
            vec![hook(Event::PreToolUse, "touch forbidden")],
            root.path(),
            "session",
            "turn",
        );
        let (_sender, signal) = watch::channel(false);
        let outcome = runtime
            .run(
                Event::PreToolUse,
                json!({"content":"x".repeat(MAX_INPUT)}),
                signal,
            )
            .await
            .unwrap_or_else(|_| panic!("cancelled"));
        assert_eq!(outcome.diagnostics.len(), 1);
        assert!(outcome.denial.is_none());
        assert!(!root.path().join("forbidden").exists());
        let receipts = runtime.take_activity();
        assert_eq!(receipts.len(), 1);
        assert_eq!(receipts[0].status, Status::Unavailable);
        assert_eq!(receipts[0].resource_id.as_deref(), Some("test"));
        assert!(receipts[0].summary.contains("nenhum comando foi executado"));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn hook_receipts_distinguish_denial_stop_failure_and_feedback() {
        let root = tempfile::tempdir().unwrap();
        for (command, status, denial, stop) in [
            (
                r#"cat >/dev/null; printf '%s' '{"decision":"block","reason":"Review first."}'"#,
                Status::Issues,
                true,
                false,
            ),
            (
                r#"cat >/dev/null; printf '%s' '{"continue":false,"stopReason":"Wait for user."}'"#,
                Status::Issues,
                false,
                true,
            ),
            (
                "cat >/dev/null; printf receipt-secret-value >&2; exit 1",
                Status::Unavailable,
                false,
                false,
            ),
            (
                r#"cat >/dev/null; printf '%s' '{"systemMessage":"Review warning."}'"#,
                Status::Issues,
                false,
                false,
            ),
            (
                r#"cat >/dev/null; printf '%s' '{bad json'"#,
                Status::Issues,
                false,
                false,
            ),
        ] {
            let runtime = Runtime::with_hooks(
                vec![
                    hook(Event::Stop, command),
                    hook(Event::Stop, "touch forbidden"),
                ],
                root.path(),
                "session",
                "turn",
            );
            let (_sender, signal) = watch::channel(false);
            let outcome = runtime
                .run_once(Event::Stop, json!({}), signal)
                .await
                .unwrap_or_else(|_| panic!("cancelled"));
            assert_eq!(outcome.denial.is_some(), denial);
            assert_eq!(outcome.stop_reason.is_some(), stop);
            let receipts = runtime.take_activity();
            assert!(!serde_json::to_string(&receipts)
                .unwrap()
                .contains("receipt-secret-value"));
            assert_eq!(receipts[0].status, status);
            if denial || stop {
                assert_eq!(receipts.len(), 1);
                assert!(!root.path().join("forbidden").exists());
            } else {
                assert_eq!(receipts.len(), 2);
                assert_eq!(receipts[1].status, Status::Applied);
                assert!(root.path().join("forbidden").exists());
                std::fs::remove_file(root.path().join("forbidden")).unwrap();
            }
        }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn dropped_hook_future_preserves_completed_and_interrupted_receipts() {
        let root = tempfile::tempdir().unwrap();
        let mut first = hook(Event::Interrupt, "cat >/dev/null");
        first.id = "completed".into();
        let mut second = hook(Event::Interrupt, "cat >/dev/null; sleep 30");
        second.id = "interrupted".into();
        let runtime = Runtime::with_hooks(vec![first, second], root.path(), "session", "turn");
        let (_sender, signal) = watch::channel(false);
        assert!(tokio::time::timeout(
            Duration::from_millis(100),
            runtime.run_once(Event::Interrupt, json!({}), signal)
        )
        .await
        .is_err());
        let receipts = runtime.take_activity();
        assert_eq!(receipts.len(), 2);
        assert_eq!(receipts[0].resource_id.as_deref(), Some("completed"));
        assert_eq!(receipts[0].status, Status::Applied);
        assert_eq!(receipts[1].resource_id.as_deref(), Some("interrupted"));
        assert_eq!(receipts[1].status, Status::Unavailable);
        assert!(runtime.take_activity().is_empty());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn cancellation_preserves_finished_hook_receipts_and_reports_current_hook() {
        let root = tempfile::tempdir().unwrap();
        let mut quiet = hook(Event::PreToolUse, "cat >/dev/null");
        quiet.id = "completed".into();
        let mut waiting = hook(Event::PreToolUse, "cat >/dev/null; touch started; sleep 30");
        waiting.id = "cancelled".into();
        waiting.timeout_seconds = 10;
        let runtime = Runtime::with_hooks(vec![quiet, waiting], root.path(), "session", "turn");
        let (sender, signal) = watch::channel(false);
        let cancel = async {
            tokio::time::timeout(Duration::from_secs(5), async {
                while !root.path().join("started").exists() {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await
            .unwrap();
            sender.send(true).unwrap();
        };
        let (result, ()) = tokio::join!(runtime.run(Event::PreToolUse, json!({}), signal), cancel);
        assert!(result.is_err());
        let receipts = runtime.take_activity();
        assert_eq!(receipts.len(), 2);
        assert_eq!(receipts[0].resource_id.as_deref(), Some("completed"));
        assert_eq!(receipts[0].status, Status::Applied);
        assert_eq!(receipts[1].resource_id.as_deref(), Some("cancelled"));
        assert_eq!(receipts[1].status, Status::Unavailable);
        assert!(runtime.take_activity().is_empty());
        let cancelled = watch::channel(true);
        assert!(runtime
            .run(Event::PreToolUse, json!({}), cancelled.1)
            .await
            .is_err());
        assert!(runtime.take_activity().is_empty());
    }

    #[tokio::test]
    async fn revoked_plugin_hook_receipts_preserve_provenance_without_executing() {
        let root = tempfile::tempdir().unwrap();
        let source = crate::plugins::HookSource {
            plugin_id: "guard@local".into(),
            plugin_hash: "frozen".into(),
            component_id: "hooks:guard".into(),
            name: "Guard".into(),
            definition: json!({"hooks":{"PreToolUse":[{"hooks":[{"type":"command","command":"touch forbidden"}]}]}}),
            root: root.path().into(),
            data_path: root.path().join("data"),
            trusted: true,
        };
        let mut runtime = Runtime::with_hooks(vec![], root.path(), "session", "turn");
        runtime.hooks = plugin_hooks(&source).unwrap();
        let expected_id = runtime.hooks[0].hook.id.clone();
        let expected_name = runtime.hooks[0].hook.name.clone();
        let (_sender, signal) = watch::channel(false);
        let outcome = runtime
            .run(Event::PreToolUse, json!({}), signal)
            .await
            .unwrap_or_else(|_| panic!("cancelled"));
        assert_eq!(outcome.diagnostics.len(), 1);
        assert!(outcome.denial.is_none());
        assert!(!root.path().join("forbidden").exists());
        let receipts = runtime.take_activity();
        assert_eq!(receipts.len(), 1);
        assert_eq!(receipts[0].status, Status::Unavailable);
        assert_eq!(receipts[0].plugin_id.as_deref(), Some("guard@local"));
        assert_eq!(
            receipts[0].resource_id.as_deref(),
            Some(expected_id.as_str())
        );
        assert_eq!(
            receipts[0].resource_name.as_deref(),
            Some(expected_name.as_str())
        );
        assert!(receipts[0].summary.contains("nenhuma ação foi executada"));
    }
    #[test]
    fn decisions_only_deny_and_never_rewrite_or_approve() {
        let hook = hook(Event::PreToolUse, "");
        let mut outcome = Outcome::default();
        parse(Event::PreToolUse, &hook, Output { code: Some(0), stdout: json!({"hookSpecificOutput":{"permissionDecision":"allow","updatedInput":{"secret":"value"},"additionalContext":"reference"}}).to_string(), stderr: String::new(), truncated: false }, &mut outcome);
        assert!(outcome.denial.is_none());
        assert_eq!(outcome.additional_context.trim(), "reference");
        assert_eq!(outcome.diagnostics.len(), 1);
        parse(Event::PermissionRequest, &hook, Output { code: Some(0), stdout: json!({"hookSpecificOutput":{"decision":{"behavior":"deny","message":"blocked"}}}).to_string(), stderr: String::new(), truncated: false }, &mut outcome);
        assert_eq!(outcome.denial.as_deref(), Some("blocked"));
    }
    #[test]
    fn malformed_or_failed_hooks_do_not_become_permission_denials() {
        let hook = hook(Event::PreToolUse, "");
        for (code, stdout, stderr, truncated) in [
            (0, "{invalid", "", false),
            (2, "", "", false),
            (1, "", "failure", false),
            (0, "{}", "", true),
        ] {
            let mut outcome = Outcome::default();
            parse(
                Event::PreToolUse,
                &hook,
                Output {
                    code: Some(code),
                    stdout: stdout.into(),
                    stderr: stderr.into(),
                    truncated,
                },
                &mut outcome,
            );
            assert!(outcome.denial.is_none());
            assert!(!outcome.diagnostics.is_empty());
        }
    }
    #[cfg(unix)]
    #[tokio::test]
    async fn large_stdin_and_both_output_pipes_are_drained_without_deadlock() {
        let directory = tempfile::tempdir().unwrap();
        let mut hook = hook(
            Event::SessionStart,
            "head -c 131072 /dev/zero; head -c 131072 /dev/zero >&2; cat > input.json",
        );
        hook.timeout_seconds = 5;
        let (_sender, signal) = watch::channel(false);
        let input = vec![b'x'; 196608];
        let output = execute(&hook, directory.path(), &input, signal)
            .await
            .unwrap_or_else(|_| panic!("unexpected cancellation"))
            .unwrap();
        assert_eq!(output.code, Some(0));
        assert!(output.truncated);
        assert_eq!(output.stdout.len(), MAX_OUTPUT);
        assert_eq!(output.stderr.len(), MAX_OUTPUT);
        assert_eq!(
            std::fs::read(directory.path().join("input.json")).unwrap(),
            input
        );
    }
    #[cfg(unix)]
    #[tokio::test]
    async fn early_exit_denial_survives_large_unread_stdin() {
        let directory = tempfile::tempdir().unwrap();
        for event in [Event::PreToolUse, Event::PermissionRequest] {
            let runtime = Runtime::with_hooks(
                vec![hook(
                    event,
                    "printf '%s' 'blocked before tool execution' >&2; exit 2",
                )],
                directory.path(),
                "session",
                "turn",
            );
            let (_sender, signal) = watch::channel(false);
            let outcome = runtime
                .run(
                    event,
                    json!({"tool_name":"bash", "tool_input":{"command":"x".repeat(512 * 1024)}}),
                    signal,
                )
                .await
                .unwrap_or_else(|_| panic!("unexpected cancellation"));
            assert_eq!(
                outcome.denial.as_deref(),
                Some("blocked before tool execution")
            );
            assert!(outcome.diagnostics.is_empty());
        }
    }
    #[cfg(unix)]
    #[tokio::test]
    async fn stdin_context_startup_once_and_disabled_hooks_are_observable() {
        let directory = tempfile::tempdir().unwrap();
        let mut enabled = hook(Event::SessionStart, "cat > event.json; printf '%s' '{\"hookSpecificOutput\":{\"additionalContext\":\"readied\"}}'");
        enabled.timeout_seconds = 5;
        let mut disabled = hook(Event::SessionStart, "touch forbidden");
        disabled.enabled = false;
        let runtime =
            Runtime::with_hooks(vec![enabled, disabled], directory.path(), "session", "turn");
        let (_sender, signal) = watch::channel(false);
        let first = runtime
            .run_once(
                Event::SessionStart,
                json!({"source":"startup"}),
                signal.clone(),
            )
            .await
            .unwrap_or_else(|_| panic!("unexpected cancellation"));
        assert_eq!(first.additional_context.trim(), "readied");
        let event: Value =
            serde_json::from_slice(&std::fs::read(directory.path().join("event.json")).unwrap())
                .unwrap();
        assert_eq!(event["session_id"], "session");
        assert_eq!(event["hook_event_name"], "SessionStart");
        assert!(!directory.path().join("forbidden").exists());
        std::fs::remove_file(directory.path().join("event.json")).unwrap();
        assert!(runtime
            .run_once(Event::SessionStart, json!({}), signal)
            .await
            .is_ok());
        assert!(!directory.path().join("event.json").exists());
    }
    #[cfg(unix)]
    #[tokio::test]
    async fn timeout_and_cancellation_stop_process_descendants() {
        let directory = tempfile::tempdir().unwrap();
        let hook = hook(
            Event::PreToolUse,
            "cat >/dev/null; (sleep 2; touch escaped) & wait",
        );
        let (_sender, signal) = watch::channel(false);
        let result = execute(&hook, directory.path(), b"{}", signal)
            .await
            .unwrap_or_else(|_| panic!("unexpected cancellation"));
        assert!(result.is_err());
        tokio::time::sleep(Duration::from_millis(1300)).await;
        assert!(!directory.path().join("escaped").exists());
        let (sender, signal) = watch::channel(false);
        let work = execute(&hook, directory.path(), b"{}", signal);
        let cancel = async {
            tokio::time::sleep(Duration::from_millis(30)).await;
            sender.send(true).unwrap();
        };
        let (result, ()) = tokio::join!(work, cancel);
        assert!(result.is_err());
        tokio::time::sleep(Duration::from_millis(2300)).await;
        assert!(!directory.path().join("escaped").exists());
    }
    #[tokio::test]
    async fn plugin_hooks_have_isolated_environment_and_preserve_explicit_mcp_identity() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("bundle");
        std::fs::create_dir_all(&root).unwrap();
        let source = crate::plugins::HookSource {
            plugin_id: "helper@local".into(),
            plugin_hash: "immutable".into(),
            component_id: "hooks:main".into(),
            name: "Helper".into(),
            definition: json!({"hooks":{"PreToolUse":[{"matcher":"write_file","hooks":[{"type":"command","command":"printf '%s' \"$CODEX_PLUGIN_ROOT|$CODEX_PLUGIN_DATA|$CODEX_HOME|$CLAUDE_CONFIG_DIR\"","timeout":2},{"type":"mcp_tool","server":"guard","tool":"inspect","input":{"path":"x"}}]}]}}),
            root: root.clone(),
            data_path: temp.path().join("data"),
            trusted: true,
        };
        let hooks = plugin_hooks(&source).unwrap();
        assert_eq!(hooks.len(), 2);
        assert!(hooks[0].hook.matches(Event::PreToolUse, Some("write_file")));
        assert!(!hooks[0].hook.matches(Event::PreToolUse, Some("read_file")));
        let (_sender, signal) = watch::channel(false);
        let output = execute_with_environment(
            &hooks[0].hook,
            temp.path(),
            b"{}",
            signal,
            &hooks[0].environment,
        )
        .await
        .unwrap_or_else(|_| panic!("cancelled"))
        .unwrap();
        assert!(output.stdout.contains(&root.to_string_lossy().into_owned()));
        assert!(output.stdout.contains(
            &source
                .data_path
                .join("codex")
                .to_string_lossy()
                .into_owned()
        ));
        assert!(output.stdout.contains(
            &source
                .data_path
                .join("claude")
                .to_string_lossy()
                .into_owned()
        ));
        match &hooks[1].handler {
            Handler::Mcp {
                server,
                tool,
                input,
            } => {
                assert_eq!(
                    server,
                    &crate::mcp::plugin_server_id("helper@local", "mcp:guard")
                );
                assert_eq!(tool, "inspect");
                assert_eq!(input["path"], "x");
            }
            Handler::Command | Handler::Impeccable => panic!("MCP handler lost"),
        }
    }
}
