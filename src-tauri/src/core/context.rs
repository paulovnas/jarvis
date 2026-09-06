use super::{
    error,
    hooks::{Event, Hooks},
    install, installed, ComponentId, CoreError,
};
use crate::mcp::{
    config::Config,
    runtime::{connect, Client},
    Server,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};
use tokio::sync::watch;

const TOOLS: [&str; 7] = [
    "ctx_execute",
    "ctx_execute_file",
    "ctx_batch_execute",
    "ctx_index",
    "ctx_search",
    "ctx_fetch_and_index",
    "ctx_stats",
];
pub fn storage(home: &Path, session: &str) -> PathBuf {
    home.join(".jarvis/context-mode")
        .join(format!("{:x}", Sha256::digest(session.as_bytes())))
}
pub(super) fn environment(
    package: &Path,
    storage: &Path,
    root: &Path,
    session: &str,
) -> BTreeMap<String, String> {
    let mut paths = vec![install::node_path(package).parent().unwrap().to_path_buf()];
    paths.extend(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    ));
    paths.extend(["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin", "/bin"].map(PathBuf::from));
    BTreeMap::from([
        (
            "PATH".into(),
            std::env::join_paths(paths)
                .unwrap_or_default()
                .to_string_lossy()
                .into(),
        ),
        ("CONTEXT_MODE_DIR".into(), storage.to_string_lossy().into()),
        ("CONTEXT_MODE_PLATFORM".into(), "claude-code".into()),
        ("CLAUDE_PROJECT_DIR".into(), root.to_string_lossy().into()),
        (
            "CONTEXT_MODE_PROJECT_DIR".into(),
            root.to_string_lossy().into(),
        ),
        ("CONTEXT_MODE_SESSION_SUFFIX".into(), String::new()),
        ("CLAUDE_SESSION_ID".into(), session.into()),
        ("PWD".into(), root.to_string_lossy().into()),
        // Constrain host auto-discovery to Jarvis, even if inherited from a CLI.
        (
            "CLAUDE_CONFIG_DIR".into(),
            storage.join("host").to_string_lossy().into(),
        ),
        ("NODE_OPTIONS".into(), String::new()),
        ("NODE_NO_WARNINGS".into(), "1".into()),
    ])
}
pub(super) async fn cancelled(signal: &mut watch::Receiver<bool>) {
    loop {
        if *signal.borrow_and_update() || signal.changed().await.is_err() {
            return;
        }
    }
}
pub struct ContextMode {
    client: Client,
    pub hooks: Hooks,
    root: PathBuf,
}
impl ContextMode {
    pub async fn open(
        home: &Path,
        root: &Path,
        session: &str,
        signal: watch::Receiver<bool>,
    ) -> Result<Self, CoreError> {
        let package = installed(home, ComponentId::ContextMode)?.path(home)?;
        let hooks = Hooks::new(home, root, session)?;
        let mut context = Self::at(&package, &storage(home, session), root, session, signal).await?;
        context.hooks = hooks;
        Ok(context)
    }
    async fn at(
        package: &Path,
        storage: &Path,
        root: &Path,
        session: &str,
        signal: watch::Receiver<bool>,
    ) -> Result<Self, CoreError> {
        let root = &std::fs::canonicalize(root)?;
        let hooks = Hooks::at(package, storage, root, session);
        hooks
            .run(Event::SessionStart, json!({}), signal.clone())
            .await?;
        let config = Config::Local {
            command: vec![
                install::node_path(package).to_string_lossy().into(),
                "--no-warnings".into(),
                package
                    .join("node_modules/context-mode/server.bundle.mjs")
                    .to_string_lossy()
                    .into(),
            ],
            cwd: None,
            environment: environment(package, storage, root, session),
            enabled: true,
            timeout: 120_000,
        };
        let server = Server {
            id: "jarvis-core-context-mode".into(),
            name: "Context-mode".into(),
            kind: "local".into(),
            enabled: true,
            configured: true,
            revision: 1,
            last_check: None,
        };
        let client = connect(server, config, root, signal)
            .await
            .map_err(|cause| error(cause.message))?;
        let names: Vec<_> = client
            .core_definitions()
            .iter()
            .filter_map(|d| d["name"].as_str().map(String::from))
            .collect();
        if TOOLS.iter().any(|name| !names.iter().any(|n| n == name)) {
            return Err(error("O Context-mode instalado não oferece as ferramentas necessárias. Reinstale o Core."));
        }
        Ok(Self {
            client,
            hooks,
            root: root.into(),
        })
    }
    pub fn definitions(&self, plan: bool) -> Vec<Value> {
        self.client
            .core_definitions()
            .into_iter()
            .filter(|definition| {
                definition["name"]
                    .as_str()
                    .is_some_and(|name| allowed(name, plan))
            })
            .collect()
    }
    pub async fn execute(
        &self,
        name: &str,
        args: &Value,
        plan: bool,
        signal: watch::Receiver<bool>,
    ) -> Result<String, CoreError> {
        if !allowed(name, plan) {
            return Err(error("Ferramenta do Core indisponível neste modo."));
        }
        if let Some(path) = args["path"].as_str() {
            let path = std::fs::canonicalize(self.root.join(path))
                .map_err(|_| error("Arquivo indisponível."))?;
            if !path.starts_with(&self.root) {
                return Err(error("O arquivo precisa estar dentro do projeto."));
            }
        }
        if args["background"] == true {
            return Err(error(
                "Execuções do Core precisam terminar nesta interação.",
            ));
        }
        if let Some(cwd) = args["cwd"].as_str() {
            let path = std::fs::canonicalize(self.root.join(cwd))
                .map_err(|_| error("Pasta indisponível."))?;
            if !path.starts_with(&self.root) {
                return Err(error("A pasta precisa estar dentro do projeto."));
            }
        }
        self.client
            .core_call(name, args, signal.clone())
            .await
            .map_err(|cause| {
                if *signal.borrow() {
                    super::cancelled_error()
                } else {
                    error(cause.message)
                }
            })
    }
    pub async fn post_tool(
        &self,
        name: &str,
        args: &Value,
        output: &str,
        failed: bool,
        call_id: &str,
        signal: watch::Receiver<bool>,
    ) -> Result<Option<String>, CoreError> {
        let hook_args: serde_json::Map<String, Value> = args
            .as_object()
            .into_iter()
            .flatten()
            .map(|(key, value)| {
                (
                    key.clone(),
                    value
                        .as_str()
                        .map(|text| json!(text.chars().take(16_000).collect::<String>()))
                        .unwrap_or_else(|| value.clone()),
                )
            })
            .collect();
        self.hooks
            .run(
                Event::PostTool,
                json!({"name":name,"args":hook_args,"output":output,"failed":failed}),
                signal.clone(),
            )
            .await?;
        // Preserve exact edit inputs/read content and already compact Context-mode outputs.
        if output.len() <= 8_000
            || name.starts_with("ctx_")
            || !matches!(name, "bash" | "search" | "list" | "web_search")
                && !name.starts_with("mcp_")
                && !name.starts_with("beads_")
        {
            return Ok(None);
        }
        let source = format!("tool-{call_id}");
        let indexed = self
            .client
            .core_call(
                "ctx_index",
                &json!({"content":output,"source":source}),
                signal,
            )
            .await
            .map_err(|cause| error(cause.message))?;
        Ok(Some(format!("{}\n\n{indexed}\nResultado completo indexado como {source}. Use ctx_search com source para consultar detalhes.", output.chars().take(1000).collect::<String>())))
    }
    pub async fn close(&mut self) {
        self.client.close().await;
    }
}
pub fn needs_approval(name: &str) -> bool {
    matches!(
        name,
        "ctx_execute" | "ctx_execute_file" | "ctx_batch_execute"
    )
}
fn allowed(name: &str, plan: bool) -> bool {
    TOOLS.contains(&name) && (!plan || !needs_approval(name))
}
pub const INSTRUCTIONS: &str = "\nJarvis Core provides Context-mode. Prefer ctx_batch_execute for related research commands, ctx_execute/ctx_execute_file to analyze large data and print only conclusions, ctx_fetch_and_index for URLs, and ctx_search for previously indexed content and session memory. Use direct read for exact code you will edit and native write/edit for mutations. Context-mode processes run in the project; they are not a filesystem sandbox. Respect project scope and Manual approvals, never bypass a denied tool. Plan mode does not expose execution tools. ctx_index stores content in this conversation's private knowledge base. Large external tool results may be indexed automatically; use their source to retrieve details. Core installation and upgrades are managed exclusively by Jarvis Settings, never by tool commands.\n";
pub(super) async fn verify(package: &Path) -> Result<(), CoreError> {
    let test = tempfile::tempdir_in(package)?;
    let (_sender, signal) = watch::channel(false);
    let mut context = ContextMode::at(
        package,
        &test.path().join("data"),
        test.path(),
        "installation-probe",
        signal.clone(),
    )
    .await?;
    context.execute("ctx_index", &json!({"content":"Jarvis core validation: copper lighthouse marker.", "source":"installation"}), false, signal.clone()).await?;
    let result = context
        .execute(
            "ctx_search",
            &json!({"queries":["copper lighthouse"],"source":"installation"}),
            false,
            signal.clone(),
        )
        .await?;
    if !result.contains("copper") {
        return Err(error("A busca do Context-mode não passou na verificação."));
    }
    let executed = context
        .execute(
            "ctx_execute",
            &json!({"language":"javascript", "code":"console.log(6 * 7)"}),
            false,
            signal.clone(),
        )
        .await?;
    if !executed.contains("42") {
        return Err(error(
            "A execução do Context-mode não passou na verificação.",
        ));
    }
    context
        .hooks
        .run(
            Event::UserPrompt,
            json!({"text":"Preserve copper lighthouse"}),
            signal.clone(),
        )
        .await?;
    let snapshot = context
        .hooks
        .run(Event::PreCompact, json!({}), signal.clone())
        .await?;
    context
        .hooks
        .run(
            Event::PostCompact,
            json!({"text":"Keep copper lighthouse for the next step."}),
            signal.clone(),
        )
        .await?;
    let restored = context
        .hooks
        .run(Event::SessionStart, json!({}), signal.clone())
        .await?;
    let raw = "Copper lighthouse indexed output.\n".repeat(400);
    let compact = context
        .post_tool(
            "search",
            &json!({"path":".","query":"copper"}),
            &raw,
            false,
            "probe-large-result",
            signal,
        )
        .await?;
    context.close().await;
    if !snapshot.contains("copper")
        || !restored.contains("copper")
        || compact
            .is_none_or(|text| text.len() >= raw.len() || !text.contains("tool-probe-large-result"))
    {
        return Err(error(
            "Os hooks do Context-mode não passaram na verificação.",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn plan_excludes_execution_and_internal_maintenance_is_never_exposed() {
        for name in ["ctx_execute", "ctx_execute_file", "ctx_batch_execute"] {
            assert!(!allowed(name, true));
            assert!(needs_approval(name));
            assert!(allowed(name, false));
        }
        for name in ["ctx_upgrade", "ctx_purge", "ctx_doctor", "other"] {
            assert!(!allowed(name, false));
        }
        assert!(allowed("ctx_search", true));
        assert!(allowed("ctx_index", true));
    }
    #[test]
    fn conversation_memory_is_isolated_and_host_environment_is_explicit() {
        let home = Path::new("/home/test");
        assert_ne!(storage(home, "a"), storage(home, "b"));
        assert!(storage(home, "../../escape").starts_with(home.join(".jarvis/context-mode")));
        let env = environment(home, &storage(home, "a"), Path::new("/project"), "a");
        assert_eq!(env["CONTEXT_MODE_PROJECT_DIR"], "/project");
        assert_eq!(env["CONTEXT_MODE_PLATFORM"], "claude-code");
        assert_eq!(env["CLAUDE_PROJECT_DIR"], "/project");
        assert!(env["CLAUDE_CONFIG_DIR"].contains(".jarvis/context-mode"));
    }
}
