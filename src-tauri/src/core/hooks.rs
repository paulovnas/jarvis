//! Fixed, internal middleware. No shell hooks or user-editable hook registry.
use super::{context, error, install, installed, ponytail::Ponytail, ComponentId, CoreError};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicBool, AtomicU8, Ordering},
    Mutex,
};
use tokio::sync::watch;

#[derive(Clone, Copy)]
pub enum Event {
    SessionStart,
    UserPrompt,
    PostTool,
    PreCompact,
    PostCompact,
    TurnEnd,
}
impl Event {
    fn name(self) -> &'static str {
        match self {
            Self::SessionStart => "session_start",
            Self::UserPrompt => "user_prompt",
            Self::PostTool => "post_tool",
            Self::PreCompact => "pre_compact",
            Self::PostCompact => "post_compact",
            Self::TurnEnd => "turn_end",
        }
    }
}
pub struct Hooks {
    package: PathBuf,
    storage: PathBuf,
    root: PathBuf,
    session: String,
    ponytail: Option<Ponytail>,
    degraded: AtomicBool,
    recorded: AtomicU8,
    activity: Mutex<Vec<super::activity::Activity>>,
}
impl Hooks {
    pub fn new(home: &Path, root: &Path, session: &str) -> Result<Self, CoreError> {
        let ponytail = installed(home, ComponentId::Ponytail)?;
        Ok(Self {
            package: installed(home, ComponentId::ContextMode)?.path(home)?,
            storage: context::storage(home, session),
            root: root.into(),
            session: session.into(),
            ponytail: Some(Ponytail::at(&ponytail.path(home)?, &ponytail.version)?),
            degraded: AtomicBool::new(false),
            recorded: AtomicU8::new(0),
            activity: Mutex::new(Vec::new()),
        })
    }
    pub(super) fn at(package: &Path, storage: &Path, root: &Path, session: &str) -> Self {
        Self {
            package: package.into(),
            storage: storage.into(),
            root: root.into(),
            session: session.into(),
            ponytail: None,
            degraded: AtomicBool::new(false),
            recorded: AtomicU8::new(0),
            activity: Mutex::new(Vec::new()),
        }
    }
    /// Reapply the frozen policy to each model request, including after compaction.
    /// Installation probes use context-only hooks and never load global rules.
    pub fn before_agent(&self, instructions: &mut String) {
        if let Some(ponytail) = &self.ponytail {
            ponytail.append_to(instructions);
            if self.recorded.fetch_or(64, Ordering::Relaxed) & 64 == 0 {
                if let Ok(mut activity) = self.activity.lock() {
                    activity.push(ponytail.activity());
                }
            }
        }
    }
    /// Runtime memory hooks are auxiliary. Installation probes still use `run`
    /// strictly; cancellation and durable journal failures are never downgraded.
    pub async fn run_resilient(
        &self,
        event: Event,
        payload: Value,
        signal: watch::Receiver<bool>,
    ) -> Result<String, CoreError> {
        if context::is_cancelled(&signal) {
            return Err(super::cancelled_error());
        }
        if self.degraded.load(Ordering::Relaxed) {
            return Ok(String::new());
        }
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(3),
            self.run(event, payload, signal.clone()),
        )
        .await
        .unwrap_or_else(|_| Err(error("Tempo limite da memória auxiliar atingido.")));
        if context::is_cancelled(&signal) {
            return Err(super::cancelled_error());
        }
        match result {
            Ok(value) => {
                let bit = 1 << event as u8;
                if self.recorded.fetch_or(bit, Ordering::Relaxed) & bit == 0 {
                    if let Ok(mut activity) = self.activity.lock() {
                        activity.push(super::activity::Activity::new(
                            ComponentId::ContextMode,
                            event.name(),
                            "Evento de memória registrado automaticamente",
                        ));
                    }
                }
                Ok(value)
            }
            Err(cause) if cause.code == "cancelled" => Err(cause),
            Err(cause) => {
                self.degraded.store(true, Ordering::Relaxed);
                if let Ok(mut activity) = self.activity.lock() {
                    activity.push(super::activity::Activity::unavailable(ComponentId::ContextMode, "session_memory", &format!("Memória auxiliar indisponível nesta execução; o histórico local permanece preservado. {}", cause.message.chars().take(300).collect::<String>())));
                }
                Ok(String::new())
            }
        }
    }
    pub fn take_activity(&self) -> Vec<super::activity::Activity> {
        self.activity
            .lock()
            .map(|mut activity| std::mem::take(&mut *activity))
            .unwrap_or_default()
    }
    pub async fn run(
        &self,
        event: Event,
        mut payload: Value,
        mut signal: watch::Receiver<bool>,
    ) -> Result<String, CoreError> {
        payload["event"] = json!(event.name());
        payload["session_id"] = json!(self.session);
        payload["cwd"] = json!(self.root);
        let bytes = serde_json::to_vec(&payload).map_err(|_| error("Evento de Core inválido."))?;
        if bytes.len() > 1024 * 1024 {
            return Err(error("Evento de Core excedeu o limite."));
        }
        let mut cmd = tokio::process::Command::new(install::node_path(&self.package));
        cmd.arg("--no-warnings")
            .arg(self.package.join("jarvis-hook.mjs"))
            .current_dir(&self.root)
            .envs(context::environment(
                &self.package,
                &self.storage,
                &self.root,
                &self.session,
            ));
        let output = tokio::select! {
            _ = context::cancelled(&mut signal) => return Err(super::cancelled_error()),
            result = install::command_input(&mut cmd, 15, Some(bytes)) => result?,
        };
        let value: Value = serde_json::from_str(&output)
            .map_err(|_| error("O hook do Core retornou uma resposta inválida."))?;
        Ok(value["context"]
            .as_str()
            .unwrap_or_default()
            .chars()
            .take(16_000)
            .collect())
    }
}

// Routing, not a security sandbox. The regular authorization path still applies
// after preflight, including calls to executable Context-mode tools.
pub fn pre_tool(name: &str, args: &Value) -> Option<&'static str> {
    if name != "bash" {
        return None;
    }
    let command = args["command"].as_str().unwrap_or_default();
    if [
        "curl ",
        "wget ",
        "fetch(",
        "requests.get(",
        "requests.post(",
        "Invoke-WebRequest",
    ]
    .iter()
    .any(|pattern| command.contains(pattern))
    {
        Some("Use ctx_fetch_and_index para baixar e consultar conteúdo, ou ctx_execute para processar a resposta antes de retorná-la.")
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn unavailable_hook_warns_once_and_never_downgrades_cancellation() {
        let directory = tempfile::tempdir().unwrap();
        let hooks = Hooks::at(directory.path(), directory.path(), directory.path(), "test");
        let (sender, signal) = watch::channel(false);
        assert_eq!(
            hooks
                .run_resilient(Event::PostTool, json!({}), signal.clone())
                .await
                .unwrap(),
            ""
        );
        let activity = hooks.take_activity();
        assert_eq!(activity.len(), 1);
        assert_eq!(
            activity[0].status,
            crate::core::activity::Status::Unavailable
        );
        assert!(hooks
            .run_resilient(Event::PostCompact, json!({}), signal.clone())
            .await
            .is_ok());
        assert!(hooks.take_activity().is_empty());
        // Installation probes still reject the missing executable.
        assert!(hooks
            .run(Event::SessionStart, json!({}), signal.clone())
            .await
            .is_err());
        sender.send(true).unwrap();
        assert_eq!(
            hooks
                .run_resilient(Event::TurnEnd, json!({}), signal)
                .await
                .unwrap_err()
                .code,
            "cancelled"
        );
        let (sender, closed) = watch::channel(false);
        drop(sender);
        assert_eq!(
            hooks
                .run_resilient(Event::TurnEnd, json!({}), closed)
                .await
                .unwrap_err()
                .code,
            "cancelled"
        );
    }
}
