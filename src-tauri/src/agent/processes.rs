//! Session-owned development services. Runtime handles are never restored from PIDs.
use super::{cancelled, now, AgentError, AgentState, Mode, ToolCall};
use crate::{library, persistence::AppState};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{collections::{HashMap, VecDeque}, path::Path, sync::{Arc, Mutex, atomic::{AtomicU32, Ordering}}, time::Duration};
use tauri::{Emitter, Manager};
use tokio::{io::{AsyncRead, AsyncReadExt}, sync::watch};

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

async fn drain(mut pipe: impl AsyncRead + Unpin, log: Arc<Mutex<VecDeque<u8>>>) {
    let mut buffer = [0; 4096];
    while let Ok(size) = pipe.read(&mut buffer).await {
        if size == 0 { break; }
        if let Ok(mut log) = log.lock() { log.extend(&buffer[..size]); let excess = log.len().saturating_sub(LOG_LIMIT); log.drain(..excess); }
    }
}
impl ProcessState {
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
    async fn start(&self, conversation: &str, root: &Path, call_id: &str, args: &Value, changed: Arc<dyn Fn() + Send + Sync>) -> Result<ProcessInfo, AgentError> {
        #[derive(Deserialize)] #[serde(deny_unknown_fields)] struct Args { title: String, command: String }
        let args: Args = serde_json::from_value(args.clone()).map_err(|_| invalid("Informe o nome e o comando do processo."))?;
        if args.title.trim().is_empty() || args.title.chars().count() > 80 || args.command.trim().is_empty() || args.command.len() > 8000 || args.command.contains('\0') { return Err(invalid("Nome ou comando inválido.")); }
        let mut entries = self.0.lock().map_err(|_| AgentError::internal())?;
        if let Some(existing) = entries.values().find(|e| e.info.conversation_id == conversation && (e.call_id == call_id || e.info.running() && e.info.command.trim() == args.command.trim())) { return Ok(existing.info.clone()); }
        if entries.values().filter(|e| e.info.running()).count() >= 32 || entries.values().filter(|e| e.info.conversation_id == conversation && e.info.running()).count() >= 8 { return Err(invalid("Limite de processos ativos atingido. Pare um processo antes de iniciar outro.")); }
        if entries.len() >= 64 { entries.retain(|_, entry| entry.info.running()); }
        let mut command = tokio::process::Command::new("/bin/bash");
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
            _ => return Err(invalid("Ferramenta de processo inválida.")),
        }; Ok(result.to_string())
    }
}
pub(super) fn definitions(mode: Mode) -> Vec<Value> {
    let mut values = vec![json!({"type":"function","name":"process_list","description":"List development processes owned by this conversation. Status and metadata only.","parameters":{"type":"object","properties":{},"additionalProperties":false}}), json!({"type":"function","name":"process_output","description":"Read the bounded recent output and actual status of a conversation process. Starting a process is not proof it is ready; inspect its output. Do not poll in a loop.","parameters":{"type":"object","properties":{"id":{"type":"string"}},"required":["id"],"additionalProperties":false}})];
    if mode == Mode::Build { values.push(json!({"type":"function","name":"process_start","description":"Start a persistent development server or watcher in the project directory for the USER to test manually. Run in the foreground (no &, nohup, daemon mode); Jarvis manages its lifecycle and bounded logs. It survives tool calls/turns until the user stops it in the composer or Jarvis exits. Use bash for finite unit/lint/typecheck/build commands. Never automate browser tests. Check process_list before starting duplicate services. The user manages stopping; do not kill unrelated processes.","parameters":{"type":"object","properties":{"title":{"type":"string","maxLength":80},"command":{"type":"string","maxLength":8000}},"required":["title","command"],"additionalProperties":false}})); }
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
