use super::*;
use std::fs;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModelChoice { pub account: String, pub model: String, pub reasoning: Option<String> }
pub type ModelSettings = BTreeMap<String, ModelChoice>;

pub(super) fn key(flow: Flow, role: Role) -> String {
    format!("{}/{}", serde_json::to_value(flow).unwrap().as_str().unwrap(), serde_json::to_value(role).unwrap().as_str().unwrap())
}
pub(super) fn roster(flow: Flow) -> &'static [Role] { match flow {
    Flow::Standard => &[Role::Builder], Flow::Designer => &[Role::Designer], Flow::Planned => &[Role::Planner, Role::Builder, Role::Designer],
    Flow::Complete => &[Role::Planner, Role::Investigator, Role::Writer, Role::Orchestrator, Role::Designer, Role::Builder, Role::Reviewer],
} }
fn read(home: &Path) -> Result<ModelSettings, AgentError> {
    let path = home.join(".jarvis/agents.json");
    let meta = match fs::symlink_metadata(&path) { Ok(meta) => meta, Err(cause) if cause.kind() == std::io::ErrorKind::NotFound => return Ok(BTreeMap::new()), Err(_) => return Err(AgentError::storage()) };
    if !meta.is_file() || meta.is_symlink() || meta.len() > 64 * 1024 { return Err(AgentError::storage()); }
    let settings: ModelSettings = serde_json::from_slice(&fs::read(path).map_err(|_| AgentError::storage())?).map_err(|_| AgentError::storage())?;
    if settings.len() > 12 || settings.keys().any(|name| ![Flow::Standard, Flow::Designer, Flow::Planned, Flow::Complete].iter().any(|flow| roster(*flow).iter().any(|role| key(*flow, *role) == *name))) { return Err(invalid("Configuração de agentes inválida.")); }
    Ok(settings)
}
pub(in crate::agent) fn load(state: &AppState, home: &Path) -> Result<ModelSettings, AgentError> { state.with_connection(home, |_| read(home)) }
pub(in crate::agent) fn validate(flow: Flow, profiles: &ModelSettings) -> Result<(), AgentError> {
    if flow.direct() { return Ok(()); }
    let missing: Vec<_> = roster(flow).iter().filter(|role| !profiles.contains_key(&key(flow, **role))).map(|role| role.label()).collect();
    if missing.is_empty() { Ok(()) } else { Err(invalid(&format!("Escolha os modelos em Configurações > Agentes: {}.", missing.join(", ")))) }
}
pub(super) fn apply(options: &mut TurnOptions, profiles: &ModelSettings, flow: Flow, role: Role) {
    if let Some(choice) = profiles.get(&key(flow, role)) {
        options.account.clone_from(&choice.account); options.model.clone_from(&choice.model); options.reasoning.clone_from(&choice.reasoning);
    }
}
#[tauri::command]
pub async fn get_agent_models(app: tauri::AppHandle, state: tauri::State<'_, AppState>) -> Result<ModelSettings, AgentError> {
    let state = state.inner().clone(); let home = app.path().home_dir().map_err(|_| AgentError::storage())?;
    tauri::async_runtime::spawn_blocking(move || load(&state, &home)).await.map_err(|_| AgentError::internal())?
}
#[tauri::command]
pub async fn set_agent_model(app: tauri::AppHandle, state: tauri::State<'_, AppState>, oauth: tauri::State<'_, OpenAiCodexState>, flow: Flow, role: Role, choice: ModelChoice) -> Result<ModelSettings, AgentError> {
    if !roster(flow).contains(&role) { return Err(invalid("Este agente não pertence ao fluxo.")); }
    let state = state.inner().clone(); let oauth = oauth.inner().clone(); let home = app.path().home_dir().map_err(|_| AgentError::storage())?;
    let result = tauri::async_runtime::spawn_blocking(move || {
        oauth.inference_model(&state, &home, &choice.account, &choice.model, choice.reasoning.as_deref())?;
        state.with_connection(&home, |_| {
            let mut config = read(&home)?; config.insert(key(flow, role), choice);
            let directory = home.join(".jarvis");
            let mut file = tempfile::NamedTempFile::new_in(&directory).map_err(|_| AgentError::storage())?;
            serde_json::to_writer(file.as_file_mut(), &config).map_err(|_| AgentError::storage())?;
            file.as_file_mut().sync_all().map_err(|_| AgentError::storage())?;
            file.persist(directory.join("agents.json")).map_err(|_| AgentError::storage())?;
            #[cfg(unix)] fs::File::open(directory).and_then(|file| file.sync_all()).map_err(|_| AgentError::storage())?;
            Ok::<_, AgentError>(config)
        })
    }).await.map_err(|_| AgentError::internal())??;
    let _ = app.emit("agent-models:changed", ()); Ok(result)
}
