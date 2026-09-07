//! Session-owned development services. Runtime handles are never restored from PIDs.
use super::{cancelled, now, AgentError, AgentState, Mode, ToolCall};
use crate::{library, persistence::AppState};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{collections::{HashMap, VecDeque}, path::Path, sync::{Arc, Mutex, atomic::{AtomicU32, Ordering}}, time::Duration};
use tauri::{Emitter, Manager};
use tokio::{io::{AsyncRead, AsyncReadExt}, sync::watch};

mod ports;

const LOG_LIMIT: usize = 32 * 1024;
fn invalid(message: &str) -> AgentError { AgentError::new("process", message) }
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessInfo { id: String, conversation_id: String, title: String, command: String, cwd: String, pid: u32, started_at: u64, ended_at: Option<u64>, exit_code: Option<i32>, status: String }
impl ProcessInfo { fn running(&self) -> bool { matches!(self.status.as_str(), "running" | "stopping") } }
struct Group(AtomicU32);
impl Group { fn stop(&self) {
    let pid = self.0.swap(0, Ordering::SeqCst);
    #[cfg(unix)] if pid > 0 { /* SAFETY: this PID belongs to a process group created by this registry. */ unsafe { libc::kill(-(pid as i32), libc::SIGKILL); } }
    #[cfg(not(unix))] let _ = pid;
} }
impl Drop for Group { fn drop(&mut self) { self.stop(); } }
struct Entry { info: ProcessInfo, call_id: String, log: Arc<Mutex<VecDeque<u8>>>, cancel: watch::Sender<bool>, group: Arc<Group> }
#[derive(Default, Clone)]
pub(crate) struct ProcessState(Arc<Mutex<HashMap<String, Entry>>>);

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum ProcessKind { Server, Watcher }

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

fn required_port(kind: Option<ProcessKind>, port: Option<u16>, command: &str) -> Result<Option<u16>, AgentError> {
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

async fn drain(mut pipe: impl AsyncRead + Unpin, log: Arc<Mutex<VecDeque<u8>>>) {
    let mut buffer = [0; 4096];
    while let Ok(size) = pipe.read(&mut buffer).await {
        if size == 0 { break; }
        if let Ok(mut log) = log.lock() { log.extend(&buffer[..size]); let excess = log.len().saturating_sub(LOG_LIMIT); log.drain(..excess); }
    }
}
impl ProcessState {
    pub(crate) fn has_running(&self) -> bool { self.0.lock().map_or(true, |entries| entries.values().any(|entry| entry.info.running())) }
    pub(crate) fn stop_all(&self) { if let Ok(entries) = self.0.lock() { for entry in entries.values().filter(|entry| entry.info.running()) { entry.cancel.send_replace(true); entry.group.stop(); } } }
    pub(crate) fn stop_conversation(&self, conversation: &str) { if let Ok(entries) = self.0.lock() { for entry in entries.values().filter(|entry| entry.info.conversation_id == conversation && entry.info.running()) { entry.cancel.send_replace(true); entry.group.stop(); } } }
    fn list(&self, conversation: &str) -> Result<Vec<ProcessInfo>, AgentError> {
        let mut items: Vec<_> = self.0.lock().map_err(|_| AgentError::internal())?.values().filter(|entry| entry.info.conversation_id == conversation).map(|entry| entry.info.clone()).collect();
        items.sort_by_key(|item| std::cmp::Reverse(item.started_at)); Ok(items)
    }
    fn output(&self, conversation: &str, id: &str) -> Result<Value, AgentError> {
        let entries = self.0.lock().map_err(|_| AgentError::internal())?;
        let entry = entries.get(id).filter(|entry| entry.info.conversation_id == conversation).ok_or_else(|| invalid("Processo não encontrado nesta conversa."))?;
        let bytes: Vec<_> = entry.log.lock().map_err(|_| AgentError::internal())?.iter().copied().collect();
        let output: String = String::from_utf8_lossy(&bytes).chars().filter(|c| !c.is_control() || matches!(c, '\n' | '\r' | '\t')).collect();
        Ok(json!({"process":entry.info,"output":output}))
    }
    fn stop(&self, conversation: &str, id: &str) -> Result<(), AgentError> {
        let mut entries = self.0.lock().map_err(|_| AgentError::internal())?;
        let entry = entries.get_mut(id).filter(|entry| entry.info.conversation_id == conversation).ok_or_else(|| invalid("Processo não encontrado nesta conversa."))?;
        if entry.info.running() { entry.info.status = "stopping".into(); entry.cancel.send_replace(true); entry.group.stop(); }
        Ok(())
    }
    fn remove(&self, conversation: &str, id: &str) -> Result<(), AgentError> {
        let mut entries = self.0.lock().map_err(|_| AgentError::internal())?;
        let entry = entries.get(id).filter(|entry| entry.info.conversation_id == conversation)
            .ok_or_else(|| invalid("Processo não encontrado nesta conversa."))?;
        if entry.info.running() { return Err(invalid("Pare o processo antes de removê-lo.")); }
        entries.remove(id);
        Ok(())
    }
    async fn start(&self, conversation: &str, root: &Path, call_id: &str, args: &Value, changed: Arc<dyn Fn() + Send + Sync>) -> Result<ProcessInfo, AgentError> {
        #[derive(Deserialize)] #[serde(deny_unknown_fields)] struct Args { title: String, command: String, #[serde(default)] port: Option<u16>, #[serde(default)] kind: Option<ProcessKind> }
        let args: Args = serde_json::from_value(args.clone()).map_err(|_| invalid("Informe o nome e o comando do processo."))?;
        if args.title.trim().is_empty() || args.title.chars().count() > 80 || args.command.trim().is_empty() || args.command.len() > 8000 || args.command.contains('\0') { return Err(invalid("Nome ou comando inválido.")); }
        let port = required_port(args.kind, args.port, &args.command)?;
        let mut entries = self.0.lock().map_err(|_| AgentError::internal())?;
        if let Some(existing) = entries.values().find(|e| e.info.conversation_id == conversation && (e.call_id == call_id || e.info.running() && e.info.command.trim() == args.command.trim())) { return Ok(existing.info.clone()); }
        if let Some(port) = port {
            if !ports::check(port)?.available {
                return Err(invalid(&format!("A porta TCP {port} já está ocupada. Nenhum processo foi iniciado. Não encerre o serviço existente nem escolha outra porta sem orientação do usuário.")));
            }
        }
        if entries.values().filter(|e| e.info.running()).count() >= 32 || entries.values().filter(|e| e.info.conversation_id == conversation && e.info.running()).count() >= 8 { return Err(invalid("Limite de processos ativos atingido. Pare um processo antes de iniciar outro.")); }
        if entries.len() >= 64 { entries.retain(|_, entry| entry.info.running()); }
        let mut command = tokio::process::Command::new("/bin/bash");
        crate::mcp::executable::configure(&mut command, false);
        command.args(["--noprofile", "--norc", "-c", &args.command]).current_dir(root).stdin(std::process::Stdio::null()).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped()).kill_on_drop(true);
        #[cfg(unix)] command.process_group(0);
        let mut child = command.spawn().map_err(|_| invalid("Não foi possível iniciar o processo."))?;
        let group = Arc::new(Group(AtomicU32::new(child.id().ok_or_else(AgentError::internal)?)));
        let info = ProcessInfo { id: library::new_id()?, conversation_id: conversation.into(), title: args.title, command: args.command, cwd: root.to_string_lossy().into_owned(), pid: child.id().unwrap(), started_at: now(), ended_at: None, exit_code: None, status: "running".into() };
        let log = Arc::new(Mutex::new(VecDeque::new())); let (cancel, mut signal) = watch::channel(false);
        let stdout = tokio::spawn(drain(child.stdout.take().unwrap(), log.clone())); let stderr = tokio::spawn(drain(child.stderr.take().unwrap(), log.clone()));
        entries.insert(info.id.clone(), Entry { info: info.clone(), call_id: call_id.into(), log, cancel, group: group.clone() });
        drop(entries); changed();
        let registry = self.clone(); let id = info.id.clone();
        tokio::spawn(async move {
            let status = tokio::select! { result = child.wait() => result.ok(), _ = cancelled(&mut signal) => { group.stop(); let _ = child.kill().await; child.wait().await.ok() } };
            group.stop();
            for mut reader in [stdout, stderr] { if tokio::time::timeout(Duration::from_secs(1), &mut reader).await.is_err() { reader.abort(); } }
            if let Ok(mut entries) = registry.0.lock() { if let Some(entry) = entries.get_mut(&id) { entry.info.status = if *signal.borrow() { "stopped" } else if status.is_some_and(|s| s.success()) { "exited" } else { "failed" }.into(); entry.info.exit_code = status.and_then(|s| s.code()); entry.info.ended_at = Some(now()); } }
            changed();
        });
        Ok(info)
    }
    pub(super) async fn execute(&self, conversation: &str, root: &Path, call: &ToolCall, changed: Arc<dyn Fn() + Send + Sync>) -> Result<String, AgentError> {
        let result = match call.name.as_str() {
            "process_start" => serde_json::to_value(self.start(conversation, root, &call.id, &call.args, changed).await?).map_err(|_| AgentError::internal())?,
            "process_list" => json!(self.list(conversation)?),
            "process_output" => self.output(conversation, call.args["id"].as_str().ok_or_else(|| invalid("Informe o processo."))?)?,
            "process_check_port" => ports::execute(&call.args)?,
            _ => return Err(invalid("Ferramenta de processo inválida.")),
        }; Ok(result.to_string())
    }
}
pub(super) fn definitions(mode: Mode) -> Vec<Value> {
    let mut values = vec![json!({"type":"function","name":"process_list","description":"List development processes owned by this conversation. Status and metadata only.","parameters":{"type":"object","properties":{},"additionalProperties":false}}), json!({"type":"function","name":"process_output","description":"Read the bounded recent output and actual status of a conversation process. Starting a process is not proof it is ready; inspect its output. Do not poll in a loop.","parameters":{"type":"object","properties":{"id":{"type":"string"}},"required":["id"],"additionalProperties":false}})];
    values.push(ports::definition());
    if mode == Mode::Build { values.push(json!({"type":"function","name":"process_start","description":"Start a persistent development server or watcher in the project directory only when needed for the USER's manual validation. In direct Standard/Designer flows, require the user's request to start a service; do not start one routinely after edits. Check process_list and process_check_port first. Set kind to server and provide the project's actual port for every TCP server; a busy port prevents startup and the command is never run on a fallback port. Set kind to watcher only for a non-network watcher. If occupied, report that no new service was started; do not kill its owner or silently switch ports. Follow the project's packageManager and lockfile. Use verified executables, never invent paths in Jarvis Core runtimes. Run in the foreground (no &, nohup, daemon mode); Jarvis manages lifecycle and logs. Use bash for finite unit/lint/typecheck/build commands. Never automate browser tests. The user manages stopping.","parameters":{"type":"object","properties":{"title":{"type":"string","maxLength":80},"command":{"type":"string","maxLength":8000},"kind":{"type":"string","enum":["server","watcher"],"description":"server for a TCP service; watcher only when no TCP listener is started."},"port":{"type":"integer","minimum":1,"maximum":65535,"description":"Required when kind is server: the actual port from project config/startup command. Omit for a watcher. Availability is rechecked before spawning."}},"required":["title","command","kind"],"additionalProperties":false}})); }
    values
}
#[tauri::command]
pub async fn list_chat_processes(app: tauri::AppHandle, persistence: tauri::State<'_, AppState>, agent: tauri::State<'_, AgentState>, conversation_id: String) -> Result<Vec<ProcessInfo>, AgentError> {
    let home = app.path().home_dir().map_err(|_| AgentError::storage())?; let state = persistence.inner().clone(); let processes = agent.processes.clone();
    tauri::async_runtime::spawn_blocking(move || { library::agent_location(&state, &home, &conversation_id)?; processes.list(&conversation_id) }).await.map_err(|_| AgentError::internal())?
}
#[tauri::command]
pub fn read_chat_process(agent: tauri::State<'_, AgentState>, conversation_id: String, id: String) -> Result<Value, AgentError> { agent.processes.output(&conversation_id, &id) }
#[tauri::command]
pub fn stop_chat_process(app: tauri::AppHandle, agent: tauri::State<'_, AgentState>, conversation_id: String, id: String, confirmed: bool) -> Result<(), AgentError> {
    if !confirmed { return Err(invalid("Confirme que deseja parar o processo.")); }
    agent.processes.stop(&conversation_id, &id)?; let _ = app.emit("processes:changed", json!({"conversationId":conversation_id})); Ok(())
}
#[tauri::command]
pub fn remove_chat_process(app: tauri::AppHandle, agent: tauri::State<'_, AgentState>, conversation_id: String, id: String) -> Result<(), AgentError> {
    agent.processes.remove(&conversation_id, &id)?;
    let _ = app.emit("processes:changed", json!({"conversationId":conversation_id}));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    async fn wait(state: &ProcessState, conversation: &str, predicate: impl Fn(&[ProcessInfo]) -> bool) {
        tokio::time::timeout(Duration::from_secs(5), async { loop { if predicate(&state.list(conversation).unwrap()) { break; } tokio::time::sleep(Duration::from_millis(10)).await; } }).await.unwrap();
    }
    #[tokio::test]
    async fn services_survive_tool_return_are_deduplicated_scoped_and_stoppable() {
        let root = tempfile::tempdir().unwrap(); let state = ProcessState::default();
        let args = json!({"title":"Servidor", "command":"printf ready; exec sleep 60"});
        let process = state.start("a", root.path(), "call", &args, Arc::new(|| {})).await.unwrap();
        let duplicate = state.start("a", root.path(), "another-call", &args, Arc::new(|| {})).await.unwrap();
        assert_eq!(process.id, duplicate.id);
        assert!(state.list("b").unwrap().is_empty());
        assert!(state.output("b", &process.id).is_err()); assert!(state.stop("b", &process.id).is_err());
        tokio::time::timeout(Duration::from_secs(5), async { while !state.output("a", &process.id).unwrap()["output"].as_str().unwrap().contains("ready") { tokio::time::sleep(Duration::from_millis(10)).await; } }).await.unwrap();
        assert!(state.list("a").unwrap()[0].running());
        state.stop("a", &process.id).unwrap(); wait(&state,"a", |items| !items[0].running()).await;
        assert_eq!(state.list("a").unwrap()[0].status,"stopped");
        state.stop("a", &process.id).unwrap();
    }
    #[tokio::test]
    async fn output_is_bounded_and_exit_failure_is_reported() {
        let root = tempfile::tempdir().unwrap(); let state = ProcessState::default();
        let process = state.start("a",root.path(),"call", &json!({"title":"Saída", "command":"head -c 100000 /dev/zero | tr '\\0' x; printf fim; exit 7"}), Arc::new(|| {})).await.unwrap();
        wait(&state,"a", |items| !items[0].running()).await;
        let result = state.output("a",&process.id).unwrap(); let output = result["output"].as_str().unwrap();
        assert!(output.len() <= LOG_LIMIT); assert!(output.ends_with("fim"));
        assert_eq!(result["process"]["status"],"failed"); assert_eq!(result["process"]["exitCode"],7);
    }
    #[tokio::test]
    async fn only_inactive_owned_processes_can_be_removed() {
        let root = tempfile::tempdir().unwrap(); let state = ProcessState::default();
        let active = state.start("a", root.path(), "active", &json!({"title":"Keep", "command":"exec sleep 60"}), Arc::new(|| {})).await.unwrap();
        assert!(state.remove("a", &active.id).is_err());
        for (call, command, status) in [("missing", "/jarvis-test-missing-runtime/bin/npm", "failed"), ("exit", "exit 0", "exited")] {
            let item = state.start("a", root.path(), call, &json!({"title":call,"command":command}), Arc::new(|| {})).await.unwrap();
            wait(&state, "a", |items| items.iter().any(|i| i.id == item.id && i.status == status)).await;
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
        let root = tempfile::tempdir().unwrap(); let state = ProcessState::default();
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let mut args = json!({"title":"Server", "command":"touch started", "kind":"server", "port":port});
        let error = state.start("a", root.path(), "server", &args, Arc::new(|| {})).await.err().unwrap();
        assert!(error.message.contains("Nenhum processo foi iniciado"));
        for port in [0, -1, 65536] {
            assert!(state.start("a", root.path(), "invalid", &json!({"title":"Server","command":"touch started","port":port}), Arc::new(|| {})).await.is_err());
        }
        assert!(!root.path().join("started").exists());
        assert!(state.list("a").unwrap().is_empty());
        assert_eq!(listener.local_addr().unwrap().port(), port);
        drop(listener);
        // Other parallel tests can acquire an ephemeral port as soon as we
        // release it. A check is advisory; retry only that specific collision.
        for _ in 0..16 {
            match state.start("a", root.path(), "server", &args, Arc::new(|| {})).await {
                Ok(_) => {
                    wait(&state, "a", |items| items[0].status == "exited").await;
                    assert!(root.path().join("started").exists());
                    return;
                }
                Err(error) => assert!(error.message.contains("já está ocupada"), "{}", error.message),
            }
            let candidate = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
            args["port"] = json!(candidate.local_addr().unwrap().port());
        }
        panic!("Could not acquire an available temporary test port");
    }
    #[tokio::test]
    async fn development_servers_need_the_declared_port_and_never_fall_back() {
        let root = tempfile::tempdir().unwrap();
        let state = ProcessState::default();
        let missing = match state.start("a", root.path(), "vite-missing", &json!({"title":"Vite", "command":"bun run dev", "kind":"server"}), Arc::new(|| {})).await {
            Err(error) => error,
            Ok(_) => panic!("A server without a declared port must not start"),
        };
        assert!(missing.message.contains("porta TCP real"));
        assert!(state.list("a").unwrap().is_empty());

        use std::net::{Ipv4Addr, TcpListener};
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let occupied = match state.start("a", root.path(), "vite-occupied", &json!({"title":"Vite", "command":"bun run dev", "kind":"server", "port":port}), Arc::new(|| {})).await {
            Err(error) => error,
            Ok(_) => panic!("An occupied server port must not start a fallback process"),
        };
        assert!(occupied.message.contains("Nenhum processo foi iniciado"));
        assert!(state.list("a").unwrap().is_empty());
        assert_eq!(listener.local_addr().unwrap().port(), port);

        let misclassified = match state.start("a", root.path(), "vite-watcher", &json!({"title":"Vite", "command":"bun run dev", "kind":"watcher"}), Arc::new(|| {})).await {
            Err(error) => error,
            Ok(_) => panic!("A recognized server must not be classified as a watcher"),
        };
        assert!(misclassified.message.contains("parece iniciar um servidor TCP"));
    }
    #[tokio::test]
    async fn deleting_one_conversation_does_not_stop_another_and_shutdown_stops_all() {
        let root = tempfile::tempdir().unwrap(); let state = ProcessState::default();
        for id in ["a","b"] { state.start(id,root.path(),"call", &json!({"title":"Service", "command":"sleep 60"}), Arc::new(|| {})).await.unwrap(); }
        state.stop_conversation("a"); wait(&state,"a",|items| !items[0].running()).await;
        assert!(state.list("b").unwrap()[0].running());
        state.stop_all(); wait(&state,"b",|items| !items[0].running()).await;
    }
    #[cfg(unix)]
    #[tokio::test]
    async fn stop_kills_descendants_in_the_owned_process_group() {
        let root = tempfile::tempdir().unwrap(); let state = ProcessState::default();
        let process = state.start("a",root.path(),"call",&json!({"title":"Tree", "command":"(sleep 1; touch survivor) & echo ready; wait"}),Arc::new(|| {})).await.unwrap();
        tokio::time::timeout(Duration::from_secs(5), async { while !state.output("a", &process.id).unwrap()["output"].as_str().unwrap().contains("ready") { tokio::time::sleep(Duration::from_millis(10)).await; } }).await.unwrap();
        state.stop("a",&process.id).unwrap(); wait(&state,"a",|items| !items[0].running()).await;
        tokio::time::sleep(Duration::from_millis(1200)).await;
        assert!(!root.path().join("survivor").exists());
    }
}
