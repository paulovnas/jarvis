//! Compatibility tools backed by conversation-owned interactive terminals.
use super::{
    terminals::{ChatTerminal, ServiceSpawn, TerminalEvents, TerminalState},
    AgentError, AgentState, Mode, ToolCall,
};
use crate::{library, persistence::AppState};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::Path;
use tauri::{Emitter, Manager};

mod ports;
const LOG_LIMIT: usize = 32 * 1024;
fn invalid(message: &str) -> AgentError {
    AgentError::new("process", message)
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessInfo {
    id: String,
    conversation_id: String,
    title: String,
    command: String,
    #[serde(serialize_with = "library::serialize_display_path")]
    cwd: String,
    pid: u32,
    started_at: u64,
    ended_at: Option<u64>,
    exit_code: Option<i32>,
    status: String,
}
impl ProcessInfo {
    fn running(&self) -> bool {
        matches!(self.status.as_str(), "running" | "stopping")
    }
    fn from_terminal(terminal: ChatTerminal, stopped: bool) -> Self {
        let status = if stopped {
            if terminal.status == "running" {
                "stopping".into()
            } else {
                "stopped".into()
            }
        } else {
            terminal.status
        };
        Self {
            id: terminal.id,
            conversation_id: terminal.conversation_id,
            title: terminal.title,
            command: terminal.command.unwrap_or_default(),
            cwd: terminal.cwd,
            pid: terminal.pid,
            started_at: terminal.started_at,
            ended_at: terminal.ended_at,
            exit_code: terminal.exit_code,
            status,
        }
    }
}
#[derive(Default, Clone)]
pub(crate) struct ProcessState(TerminalState);
#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum ProcessKind {
    Server,
    Watcher,
}

fn looks_like_tcp_server(command: &str) -> bool {
    let command = command.to_ascii_lowercase();
    [
        "vite",
        "next dev",
        "nuxt dev",
        "astro dev",
        "svelte-kit dev",
        "webpack serve",
        "bun run dev",
        "npm run dev",
        "pnpm run dev",
        "pnpm dev",
        "yarn dev",
        "deno task dev",
    ]
    .iter()
    .any(|pattern| command.contains(pattern))
}

fn required_port(
    kind: Option<ProcessKind>,
    port: Option<u16>,
    command: &str,
) -> Result<Option<u16>, AgentError> {
    match kind {
        Some(ProcessKind::Server) => port
            .map(Some)
            .ok_or_else(|| invalid("Informe a porta TCP real do projeto para iniciar um servidor. O Jarvis não iniciará o serviço em uma porta alternativa.")),
        Some(ProcessKind::Watcher) if port.is_some() => Err(invalid("Use o tipo servidor ao informar uma porta TCP.")),
        Some(ProcessKind::Watcher) if looks_like_tcp_server(command) => Err(invalid("Este comando parece iniciar um servidor TCP. Use o tipo servidor e informe a porta configurada no projeto.")),
        Some(ProcessKind::Watcher) => Ok(None),
        // Journals from before `kind` existed remain readable. They may still
        // start a non-network watcher, but recognizable dev-server commands
        // must prove their intended port before the shell is invoked.
        None if looks_like_tcp_server(command) && port.is_none() => Err(invalid("Informe a porta TCP real do projeto antes de iniciar este servidor. O Jarvis não usará uma porta alternativa.")),
        None => Ok(port),
    }
}

pub(super) fn ensure_available(port: u16) -> Result<(), AgentError> {
    if !ports::check(port)?.available {
        return Err(invalid(&format!("A porta TCP {port} já está ocupada. Nenhum terminal foi iniciado. Não encerre o serviço existente nem escolha outra porta sem orientação do usuário.")));
    }
    Ok(())
}
impl ProcessState {
    pub(crate) fn new(terminals: TerminalState) -> Self {
        Self(terminals)
    }
    pub(crate) fn has_running(&self) -> bool {
        self.0.has_running()
    }
    pub(crate) fn stop_all(&self) {
        self.0.stop_all();
    }
    pub(crate) fn stop_conversation(&self, conversation: &str) {
        self.0.stop_services(conversation);
    }
    fn list(&self, conversation: &str) -> Result<Vec<ProcessInfo>, AgentError> {
        let mut items: Vec<_> = self
            .0
            .services(conversation)?
            .into_iter()
            .map(|(item, stopped)| ProcessInfo::from_terminal(item, stopped))
            .collect();
        items.sort_by_key(|item| std::cmp::Reverse(item.started_at));
        Ok(items)
    }
    fn info(&self, conversation: &str, id: &str) -> Result<ProcessInfo, AgentError> {
        self.list(conversation)?
            .into_iter()
            .find(|item| item.id == id)
            .ok_or_else(|| invalid("Terminal de serviço não encontrado nesta conversa."))
    }
    fn output(&self, conversation: &str, id: &str) -> Result<Value, AgentError> {
        let info = self.info(conversation, id)?;
        let snapshot = self.0.agent_snapshot(conversation, id, LOG_LIMIT)?;
        Ok(json!({"process": info, "output": snapshot["output"]}))
    }
    fn stop(&self, conversation: &str, id: &str) -> Result<(), AgentError> {
        self.info(conversation, id)?;
        self.0.stop_service(conversation, id)
    }
    fn remove(&self, conversation: &str, id: &str) -> Result<(), AgentError> {
        if self.info(conversation, id)?.running() {
            return Err(invalid("Pare o terminal antes de removê-lo."));
        }
        self.0.remove_service(conversation, id)
    }
    async fn start(
        &self,
        conversation: &str,
        root: &Path,
        call_id: &str,
        args: &Value,
        events: TerminalEvents,
    ) -> Result<ProcessInfo, AgentError> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Args {
            title: String,
            command: String,
            #[serde(default)]
            port: Option<u16>,
            #[serde(default)]
            kind: Option<ProcessKind>,
        }
        let args: Args = serde_json::from_value(args.clone())
            .map_err(|_| invalid("Informe o nome e o comando do processo."))?;
        if args.title.trim().is_empty()
            || args.title.chars().count() > 80
            || args.command.trim().is_empty()
            || args.command.len() > 8000
            || args.command.contains('\0')
        {
            return Err(invalid("Nome ou comando inválido."));
        }
        let port = required_port(args.kind, args.port, &args.command)?;

        let terminal = self.0.start_service(
            ServiceSpawn {
                conversation,
                root,
                call_id,
                title: &args.title,
                command: &args.command,
                port,
            },
            events,
        )?;
        self.info(conversation, &terminal.id)
    }
    pub(super) async fn execute(
        &self,
        conversation: &str,
        root: &Path,
        call: &ToolCall,
        events: TerminalEvents,
    ) -> Result<String, AgentError> {
        let result = match call.name.as_str() {
            "process_start" => serde_json::to_value(
                self.start(conversation, root, &call.id, &call.args, events)
                    .await?,
            )
            .map_err(|_| AgentError::internal())?,
            "process_list" => json!(self.list(conversation)?),
            "process_output" => self.output(
                conversation,
                call.args["id"]
                    .as_str()
                    .ok_or_else(|| invalid("Informe o processo."))?,
            )?,
            "process_check_port" => ports::execute(&call.args)?,
            _ => return Err(invalid("Ferramenta de processo inválida.")),
        };
        Ok(result.to_string())
    }
}
pub(super) fn definitions(mode: Mode) -> Vec<Value> {
    let mut values = vec![
        json!({"type":"function","name":"process_list","description":"List development processes owned by this conversation. Status and metadata only.","parameters":{"type":"object","properties":{},"additionalProperties":false}}),
        json!({"type":"function","name":"process_output","description":"Read the bounded recent output and actual status of a conversation process. Starting a process is not proof it is ready; inspect its output. Do not poll in a loop.","parameters":{"type":"object","properties":{"id":{"type":"string"}},"required":["id"],"additionalProperties":false}}),
    ];
    values.push(ports::definition());
    if mode == Mode::Build {
        values.push(json!({"type":"function","name":"process_start","description":"Start a persistent development server or watcher in the project directory only when needed for the USER's manual validation. In direct Standard/Designer flows, require the user's request to start a service; do not start one routinely after edits. Check process_list and process_check_port first. Set kind to server and provide the project's actual port for every TCP server; a busy port prevents startup and the command is never run on a fallback port. Set kind to watcher only for a non-network watcher. If occupied, report that no new service was started; do not kill its owner or silently switch ports. Follow the project's packageManager and lockfile. Use verified executables, never invent paths in Jarvis Core runtimes. Run in the foreground (no &, nohup, daemon mode); Jarvis manages lifecycle and logs. Use bash for finite unit/lint/typecheck/build commands. Never automate browser tests. The user manages stopping.","parameters":{"type":"object","properties":{"title":{"type":"string","maxLength":80},"command":{"type":"string","maxLength":8000},"kind":{"type":"string","enum":["server","watcher"],"description":"server for a TCP service; watcher only when no TCP listener is started."},"port":{"type":"integer","minimum":1,"maximum":65535,"description":"Required when kind is server: the actual port from project config/startup command. Omit for a watcher. Availability is rechecked before spawning."}},"required":["title","command","kind"],"additionalProperties":false}}));
    }
    values
}
#[tauri::command]
pub async fn list_chat_processes(
    app: tauri::AppHandle,
    persistence: tauri::State<'_, AppState>,
    agent: tauri::State<'_, AgentState>,
    conversation_id: String,
) -> Result<Vec<ProcessInfo>, AgentError> {
    let home = app.path().home_dir().map_err(|_| AgentError::storage())?;
    let state = persistence.inner().clone();
    let processes = agent.processes.clone();
    tauri::async_runtime::spawn_blocking(move || {
        library::agent_location(&state, &home, &conversation_id)?;
        processes.list(&conversation_id)
    })
    .await
    .map_err(|_| AgentError::internal())?
}
#[tauri::command]
pub fn read_chat_process(
    agent: tauri::State<'_, AgentState>,
    conversation_id: String,
    id: String,
) -> Result<Value, AgentError> {
    agent.processes.output(&conversation_id, &id)
}
#[tauri::command]
pub fn stop_chat_process(
    app: tauri::AppHandle,
    agent: tauri::State<'_, AgentState>,
    conversation_id: String,
    id: String,
    confirmed: bool,
) -> Result<(), AgentError> {
    if !confirmed {
        return Err(invalid("Confirme que deseja parar o processo."));
    }
    agent.processes.stop(&conversation_id, &id)?;
    let _ = app.emit(
        "terminals:changed",
        json!({"conversationId":conversation_id}),
    );
    Ok(())
}
#[tauri::command]
pub fn remove_chat_process(
    app: tauri::AppHandle,
    agent: tauri::State<'_, AgentState>,
    conversation_id: String,
    id: String,
) -> Result<(), AgentError> {
    agent.processes.remove(&conversation_id, &id)?;
    let _ = app.emit(
        "terminals:changed",
        json!({"conversationId":conversation_id}),
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    fn cmd(unix: &str, windows: &str) -> String {
        if cfg!(windows) {
            windows.to_string()
        } else {
            unix.to_string()
        }
    }
    async fn wait(
        state: &ProcessState,
        conversation: &str,
        predicate: impl Fn(&[ProcessInfo]) -> bool,
    ) {
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if predicate(&state.list(conversation).unwrap()) {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
    }
    #[tokio::test]
    async fn service_tools_share_interactive_terminals_with_the_user() {
        let root = tempfile::tempdir().unwrap();
        let agent = AgentState::default();
        let command = cmd("echo input-ready; read value; printf 'received:%s' \"$value\"", "Write-Output input-ready; $value = Read-Host; [Console]::Out.Write(('received:' + $value))");
        let service = agent
            .processes
            .start(
                "chat",
                root.path(),
                "input",
                &json!({"title":"Interactive service", "command":command}),
                super::super::terminals::silent_events(),
            )
            .await
            .unwrap();
        let terminals = agent.terminals.list("chat").unwrap();
        assert_eq!(terminals.len(), 1);
        assert_eq!(terminals[0].id, service.id);
        assert_eq!(terminals[0].command.as_deref(), Some(command.as_str()));
        assert!(agent
            .terminals
            .write("other", &service.id, "wrong\r")
            .is_err());
        tokio::time::timeout(Duration::from_secs(5), async {
            while !agent.processes.output("chat", &service.id).unwrap()["output"]
                .as_str()
                .unwrap()
                .contains("input-ready")
            {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        agent
            .terminals
            .write("chat", &service.id, "Jarvis\r")
            .unwrap();
        wait(&agent.processes, "chat", |items| !items[0].running()).await;
        let output = agent.processes.output("chat", &service.id).unwrap();
        assert!(
            output["output"]
                .as_str()
                .unwrap()
                .contains("received:Jarvis"),
            "{output}"
        );
        assert_eq!(agent.terminals.list("chat").unwrap()[0].status, "exited");
        let repeated = agent
            .processes
            .start(
                "chat",
                root.path(),
                "input",
                &json!({"title":"Interactive service", "command":command}),
                super::super::terminals::silent_events(),
            )
            .await
            .unwrap();
        assert_eq!(repeated.id, service.id);
    }

    #[tokio::test]
    async fn services_survive_tool_return_are_deduplicated_scoped_and_stoppable() {
        let root = tempfile::tempdir().unwrap();
        let state = ProcessState::default();
        let args = json!({"title":"Servidor", "command":cmd("printf ready; exec sleep 60", "Write-Output ready; Start-Sleep 60")});
        let process = state
            .start(
                "a",
                root.path(),
                "call",
                &args,
                super::super::terminals::silent_events(),
            )
            .await
            .unwrap();
        let duplicate = state
            .start(
                "a",
                root.path(),
                "another-call",
                &args,
                super::super::terminals::silent_events(),
            )
            .await
            .unwrap();
        assert_eq!(process.id, duplicate.id);
        assert!(state.list("b").unwrap().is_empty());
        assert!(state.output("b", &process.id).is_err());
        assert!(state.stop("b", &process.id).is_err());
        tokio::time::timeout(Duration::from_secs(5), async {
            while !state.output("a", &process.id).unwrap()["output"]
                .as_str()
                .unwrap()
                .contains("ready")
            {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        assert!(state.list("a").unwrap()[0].running());
        state.stop("a", &process.id).unwrap();
        wait(&state, "a", |items| !items[0].running()).await;
        assert_eq!(state.list("a").unwrap()[0].status, "stopped");
        state.stop("a", &process.id).unwrap();
    }
    #[tokio::test]
    async fn output_is_bounded_and_exit_failure_is_reported() {
        let root = tempfile::tempdir().unwrap();
        let state = ProcessState::default();
        let process = state.start("a",root.path(),"call", &json!({"title":"Saída", "command":cmd("head -c 100000 /dev/zero | tr '\\0' x; printf fim; exit 7", "'x' * 100000; [Console]::Out.Write('fim'); exit 7")}), super::super::terminals::silent_events()).await.unwrap();
        wait(&state, "a", |items| !items[0].running()).await;
        let result = state.output("a", &process.id).unwrap();
        let output = result["output"].as_str().unwrap();
        assert!(output.len() <= LOG_LIMIT);
        assert!(output.ends_with("fim"));
        assert_eq!(result["process"]["status"], "failed");
        assert_eq!(result["process"]["exitCode"], 7);
    }
    #[tokio::test]
    async fn only_inactive_owned_processes_can_be_removed() {
        let root = tempfile::tempdir().unwrap();
        let state = ProcessState::default();
        let active = state
            .start(
                "a",
                root.path(),
                "active",
                &json!({"title":"Keep", "command":cmd("exec sleep 60", "Start-Sleep 60")}),
                super::super::terminals::silent_events(),
            )
            .await
            .unwrap();
        assert!(state.remove("a", &active.id).is_err());
        let cases: [(&str, &str, &str); 2] = if cfg!(windows) {
            [
                (
                    "missing",
                    "& 'C:\\jarvis-test-missing-runtime\\npm.cmd'",
                    "failed",
                ),
                ("exit", "exit 0", "exited"),
            ]
        } else {
            [
                ("missing", "/jarvis-test-missing-runtime/bin/npm", "failed"),
                ("exit", "exit 0", "exited"),
            ]
        };
        for (call, command, status) in cases {
            let item = state
                .start(
                    "a",
                    root.path(),
                    call,
                    &json!({"title":call,"command":command}),
                    super::super::terminals::silent_events(),
                )
                .await
                .unwrap();
            wait(&state, "a", |items| {
                items.iter().any(|i| i.id == item.id && i.status == status)
            })
            .await;
            assert!(state.remove("b", &item.id).is_err());
            assert!(state.output("a", &item.id).is_ok());
            state.remove("a", &item.id).unwrap();
            assert!(state.output("a", &item.id).is_err());
            assert_eq!(state.list("a").unwrap().len(), 1);
            assert!(state.list("a").unwrap()[0].running());
        }
        state.stop("a", &active.id).unwrap();
        assert!(state.remove("a", &active.id).is_err()); // Still stopping until reaped.
        wait(&state, "a", |items| items[0].status == "stopped").await;
        state.remove("a", &active.id).unwrap();
        assert!(state.list("a").unwrap().is_empty());
    }
    #[tokio::test]
    async fn occupied_or_invalid_port_prevents_spawn_without_touching_existing_listener() {
        use std::net::{Ipv4Addr, TcpListener};
        let root = tempfile::tempdir().unwrap();
        let state = ProcessState::default();
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let args = json!({"title":"Server", "command":cmd("touch started", "New-Item -ItemType File started | Out-Null"), "kind":"server", "port":port});
        let error = state
            .start(
                "a",
                root.path(),
                "server",
                &args,
                super::super::terminals::silent_events(),
            )
            .await
            .err()
            .unwrap();
        assert!(error.message.contains("Nenhum terminal foi iniciado"));
        for port in [0, -1, 65536] {
            assert!(state.start("a", root.path(), "invalid", &json!({"title":"Server","command":cmd("touch started", "New-Item -ItemType File started | Out-Null"),"port":port}), super::super::terminals::silent_events()).await.is_err());
        }
        assert!(!root.path().join("started").exists());
        assert!(state.list("a").unwrap().is_empty());
        assert_eq!(listener.local_addr().unwrap().port(), port);
    }
    #[tokio::test]
    async fn an_available_port_allows_the_declared_server_to_spawn() {
        use std::net::{Ipv4Addr, TcpListener};
        let root = tempfile::tempdir().unwrap();
        let state = ProcessState::default();
        let mut args = json!({"title":"Server", "command":cmd("touch started", "New-Item -ItemType File started | Out-Null"), "kind":"server"});
        // Other parallel tests can acquire an ephemeral port as soon as we
        // release it. A check is advisory; retry only that specific collision.
        for _ in 0..16 {
            let candidate = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
            args["port"] = json!(candidate.local_addr().unwrap().port());
            drop(candidate);
            match state
                .start(
                    "a",
                    root.path(),
                    "server",
                    &args,
                    super::super::terminals::silent_events(),
                )
                .await
            {
                Ok(_) => {
                    wait(&state, "a", |items| items[0].status == "exited").await;
                    assert!(root.path().join("started").exists());
                    return;
                }
                Err(error) => assert!(
                    error.message.contains("já está ocupada"),
                    "{}",
                    error.message
                ),
            }
        }
        panic!("Could not acquire an available temporary test port");
    }
    #[tokio::test]
    async fn development_servers_need_the_declared_port_and_never_fall_back() {
        let root = tempfile::tempdir().unwrap();
        let state = ProcessState::default();
        let missing = match state
            .start(
                "a",
                root.path(),
                "vite-missing",
                &json!({"title":"Vite", "command":"bun run dev", "kind":"server"}),
                super::super::terminals::silent_events(),
            )
            .await
        {
            Err(error) => error,
            Ok(_) => panic!("A server without a declared port must not start"),
        };
        assert!(missing.message.contains("porta TCP real"));
        assert!(state.list("a").unwrap().is_empty());

        use std::net::{Ipv4Addr, TcpListener};
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let occupied = match state
            .start(
                "a",
                root.path(),
                "vite-occupied",
                &json!({"title":"Vite", "command":"bun run dev", "kind":"server", "port":port}),
                super::super::terminals::silent_events(),
            )
            .await
        {
            Err(error) => error,
            Ok(_) => panic!("An occupied server port must not start a fallback process"),
        };
        assert!(occupied.message.contains("Nenhum terminal foi iniciado"));
        assert!(state.list("a").unwrap().is_empty());
        assert_eq!(listener.local_addr().unwrap().port(), port);

        let misclassified = match state
            .start(
                "a",
                root.path(),
                "vite-watcher",
                &json!({"title":"Vite", "command":"bun run dev", "kind":"watcher"}),
                super::super::terminals::silent_events(),
            )
            .await
        {
            Err(error) => error,
            Ok(_) => panic!("A recognized server must not be classified as a watcher"),
        };
        assert!(misclassified
            .message
            .contains("parece iniciar um servidor TCP"));
    }
    #[tokio::test]
    async fn deleting_one_conversation_does_not_stop_another_and_shutdown_stops_all() {
        let root = tempfile::tempdir().unwrap();
        let state = ProcessState::default();
        for id in ["a", "b"] {
            state
                .start(
                    id,
                    root.path(),
                    "call",
                    &json!({"title":"Service", "command":cmd("sleep 60", "Start-Sleep 60")}),
                    super::super::terminals::silent_events(),
                )
                .await
                .unwrap();
        }
        state.stop_conversation("a");
        wait(&state, "a", |items| !items[0].running()).await;
        assert!(state.list("b").unwrap()[0].running());
        let agent = super::AgentState {
            processes: state.clone(),
            terminals: state.0.clone(),
            ..Default::default()
        };
        crate::shutdown_services(&crate::system::SystemState::default(), &agent);
        wait(&state, "b", |items| !items[0].running()).await;
    }
    #[tokio::test]
    async fn stop_kills_descendants_in_the_owned_process_group() {
        let root = tempfile::tempdir().unwrap();
        let state = ProcessState::default();
        let process = state.start("a",root.path(),"call",&json!({"title":"Tree", "command":cmd("(sleep 1; touch survivor) & echo ready; wait", "Start-Job { Start-Sleep 1; New-Item -ItemType File survivor | Out-Null }; Write-Output ready; Wait-Job | Out-Null")}),super::super::terminals::silent_events()).await.unwrap();
        tokio::time::timeout(Duration::from_secs(5), async {
            while !state.output("a", &process.id).unwrap()["output"]
                .as_str()
                .unwrap()
                .contains("ready")
            {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        state.stop("a", &process.id).unwrap();
        wait(&state, "a", |items| !items[0].running()).await;
        tokio::time::sleep(Duration::from_millis(1200)).await;
        assert!(!root.path().join("survivor").exists());
    }
}
