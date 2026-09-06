//! Fixed, internal middleware. No shell hooks or user-editable hook registry.
use super::{context, error, install, installed, ponytail::Ponytail, ComponentId, CoreError};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
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
        })
    }
    pub(super) fn at(package: &Path, storage: &Path, root: &Path, session: &str) -> Self {
        Self {
            package: package.into(),
            storage: storage.into(),
            root: root.into(),
            session: session.into(),
            ponytail: None,
        }
    }
    /// Reapply the frozen policy to each model request, including after compaction.
    /// Installation probes use context-only hooks and never load global rules.
    pub fn before_agent(&self, instructions: &mut String) {
        if let Some(ponytail) = &self.ponytail {
            ponytail.append_to(instructions);
        }
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
