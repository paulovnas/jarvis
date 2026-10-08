//! Trusted MCP hooks bypass the hook pipeline, preventing recursive invocation.
use super::runtime::{Cancelled, Output};
use crate::{
    mcp::{self, config::Config, McpError, McpState, Server},
    persistence::AppState,
};
use serde_json::Value;
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::sync::watch;

#[derive(Clone)]
pub(crate) struct Context {
    mcp: McpState,
    state: AppState,
    home: PathBuf,
    root: PathBuf,
    servers: Vec<(Server, Result<Config, String>)>,
    activity: Arc<Mutex<Vec<crate::core::activity::Activity>>>,
    plugin_owners: std::collections::HashMap<String, String>,
}

impl Context {
    pub(crate) fn new(
        mcp: McpState,
        state: AppState,
        home: &Path,
        root: &Path,
    ) -> Result<Self, McpError> {
        let mut servers = Vec::new();
        let (plugin_configs, plugin_owners) = mcp.plugin_configs_with_owners(home, root)?;
        for server in mcp
            .list_for_project(&state, home, Some(root))?
            .into_iter()
            .filter(|server| server.enabled && server.configured)
        {
            let config = if server.id.starts_with("plugin-") {
                plugin_configs
                    .get(&server.id)
                    .map(|(_, config)| config.clone())
                    .ok_or_else(|| {
                        "O MCP do plugin mudou enquanto a configuração era preparada.".into()
                    })
            } else {
                mcp.active_config(&state, home, &server)
                    .map_err(|cause| cause.message)
                    .and_then(|value| {
                        value.map(|(_, config)| config).ok_or_else(|| {
                            "O MCP mudou enquanto a configuração era preparada.".into()
                        })
                    })
            };
            servers.push((server, config));
        }
        Ok(Self {
            mcp,
            state,
            home: home.to_owned(),
            root: root.to_owned(),
            servers,
            activity: Default::default(),
            plugin_owners,
        })
    }

    pub(crate) fn take_activity(&self) -> Vec<crate::core::activity::Activity> {
        self.activity
            .lock()
            .map(|mut activity| std::mem::take(&mut *activity))
            .unwrap_or_default()
    }
}

pub(crate) async fn execute(
    context: &Context,
    server: &str,
    tool: &str,
    args: &Value,
    timeout: Duration,
    mut signal: watch::Receiver<bool>,
) -> Result<Result<Output, String>, Cancelled> {
    if *signal.borrow() {
        return Err(Cancelled);
    }
    let mut matching = context
        .servers
        .iter()
        .filter(|(candidate, _)| candidate.id == server || candidate.name == server);
    let Some((selected, config)) = matching.next() else {
        return Ok(Err(
            "O servidor MCP do hook não está configurado ou ativo.".into()
        ));
    };
    if matching.next().is_some() {
        return Ok(Err(
            "O nome do servidor MCP do hook é ambíguo. Use seu ID completo.".into(),
        ));
    }
    let config = match config {
        Ok(config) => config.clone(),
        Err(cause) => return Ok(Err(cause.clone())),
    };
    if !context.mcp.frozen_config_current(
        &context.state,
        &context.home,
        &context.root,
        selected,
        &config,
    ) {
        return Ok(Err(
            "O servidor MCP deste hook foi desativado, removido ou alterado.".into(),
        ));
    }
    let task_signal = signal.clone();
    let task = async move {
        let client = mcp::runtime::connect_with_state(
            &context.mcp,
            selected.clone(),
            config,
            &context.root,
            task_signal.clone(),
        )
        .await
        .map_err(|cause| cause.message)?;
        // core_call validates the exact advertised tool/arguments, makes one call,
        // and bounds/redacts output. It never invokes tool hooks or retries.
        let usage = context
            .plugin_owners
            .get(&selected.id)
            .map(|owner| mcp::runtime::plugin_activity(selected, tool, owner));
        let stdout = client
            .core_call_with_activity(
                tool,
                args,
                task_signal,
                usage.map(|activity| (activity, context.activity.as_ref())),
            )
            .await
            .map_err(|cause| cause.message)?;
        Ok(Output {
            code: Some(0),
            stdout,
            stderr: String::new(),
            truncated: false,
        })
    };
    tokio::select! {
        biased;
        _ = async { loop { if *signal.borrow_and_update() || signal.changed().await.is_err() { break; } } } => Err(Cancelled),
        result = tokio::time::timeout(timeout,task) => Ok(result.unwrap_or_else(|_| Err("O hook MCP excedeu o tempo limite. Confira o resultado antes de repetir uma ação.".into()))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn unavailable_or_cancelled_hook_never_starts_a_server() {
        let home = tempfile::tempdir().unwrap();
        let context = Context::new(
            McpState::default(),
            AppState::default(),
            home.path(),
            home.path(),
        )
        .unwrap();
        let (_sender, signal) = watch::channel(false);
        assert!(matches!(
            execute(
                &context,
                "missing",
                "write",
                &serde_json::json!({}),
                Duration::from_secs(1),
                signal
            )
            .await,
            Ok(Err(_))
        ));
        let (_sender, signal) = watch::channel(true);
        assert!(execute(
            &context,
            "missing",
            "write",
            &serde_json::json!({}),
            Duration::from_secs(1),
            signal
        )
        .await
        .is_err());
    }
}
