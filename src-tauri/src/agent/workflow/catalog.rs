//! User-owned definitions. Built-in contracts are never read from this catalog.
use super::*;
use std::{collections::BTreeSet, fs, sync::Mutex};
mod appearance;
pub(crate) mod permissions;
pub use appearance::Appearance;

static CATALOG_LOCK: Mutex<()> = Mutex::new(());

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    ReadOnly,
    WriteFiles,
    Commands,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentUsage {
    Solo,
    Mixed,
    #[default]
    FlowOnly,
}

impl AgentUsage {
    pub(super) fn allows_solo(self) -> bool {
        matches!(self, Self::Solo | Self::Mixed)
    }

    pub(super) fn allows_flow(self) -> bool {
        matches!(self, Self::Mixed | Self::FlowOnly)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentDefinition {
    pub id: String,
    pub name: String,
    pub description: String,
    pub instructions: String,
    #[serde(default)]
    pub usage: AgentUsage,
    pub capability: Capability,
    #[serde(default)]
    pub denied_tools: Vec<String>,
    pub model: Option<settings::ModelChoice>,
    #[serde(default)]
    pub appearance: Option<Appearance>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Position {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Step {
    pub id: String,
    pub agent_id: String,
    pub instructions: String,
    pub position: Position,
    pub next: Option<String>,
    pub on_rework: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FlowDefinition {
    pub id: String,
    pub name: String,
    pub description: String,
    pub entry: String,
    pub max_steps: u8,
    pub steps: Vec<Step>,
    #[serde(default)]
    pub appearance: Option<Appearance>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Catalog {
    pub revision: u64,
    pub agents: Vec<AgentDefinition>,
    pub flows: Vec<FlowDefinition>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RunDefinition {
    pub flow: FlowDefinition,
    pub agents: Vec<AgentDefinition>,
}

fn text_valid(value: &str, max: usize, required: bool) -> bool {
    value.len() <= max && (!required || !value.trim().is_empty()) && !value.contains('\0')
}

fn unique_ids<'a>(ids: impl Iterator<Item = &'a str>) -> bool {
    let mut seen = BTreeSet::new();
    ids.into_iter()
        .all(|id| storage::valid_id(id) && seen.insert(id))
}

impl Catalog {
    pub(crate) fn validate(&self) -> Result<(), AgentError> {
        if self.agents.len() > 64
            || self.flows.len() > 32
            || !unique_ids(self.agents.iter().map(|a| a.id.as_str()))
            || !unique_ids(self.flows.iter().map(|f| f.id.as_str()))
        {
            return Err(invalid(
                "Catálogo inválido. Use até 64 agentes e 32 fluxos customizados.",
            ));
        }
        for agent in &self.agents {
            permissions::validate(&agent.denied_tools)?;
            if !text_valid(&agent.name, 100, true)
                || !text_valid(&agent.description, 500, false)
                || !text_valid(&agent.instructions, 16_000, true)
                || agent.model.as_ref().is_some_and(|m| {
                    !text_valid(&m.account, 200, true)
                        || !text_valid(&m.model, 200, true)
                        || m.reasoning
                            .as_ref()
                            .is_some_and(|r| !text_valid(r, 40, true))
                })
            {
                return Err(invalid(
                    "Agente inválido: informe nome e instruções dentro dos limites.",
                ));
            }
        }
        for flow in &self.flows {
            self.validate_flow(flow)?;
        }
        Ok(())
    }

    fn validate_flow(&self, flow: &FlowDefinition) -> Result<(), AgentError> {
        if !text_valid(&flow.name, 100, true)
            || !text_valid(&flow.description, 500, false)
            || flow.steps.is_empty()
            || flow.steps.len() > 24
            || flow.max_steps < flow.steps.len() as u8
            || flow.max_steps > 48
            || !unique_ids(flow.steps.iter().map(|s| s.id.as_str()))
            || !flow.steps.iter().any(|s| s.id == flow.entry)
        {
            return Err(invalid("Fluxo inválido: escolha o início, de 1 a 24 etapas e um limite de até 48 execuções."));
        }
        let nodes: BTreeMap<_, _> = flow.steps.iter().map(|s| (s.id.as_str(), s)).collect();
        for step in &flow.steps {
            let agent = self
                .agents
                .iter()
                .find(|agent| agent.id == step.agent_id)
                .ok_or_else(|| invalid("Todas as etapas precisam de agentes existentes."))?;
            if !agent.usage.allows_flow() {
                return Err(invalid(
                    "Agentes Solo não podem fazer parte de fluxos. Altere o uso para Misto ou Somente em fluxos.",
                ));
            }
            if !text_valid(&step.instructions, 8_000, false)
                || !step.position.x.is_finite()
                || !step.position.y.is_finite()
                || step.position.x.abs() > 100_000.0
                || step.position.y.abs() > 100_000.0
                || [&step.next, &step.on_rework]
                    .into_iter()
                    .flatten()
                    .any(|id| !nodes.contains_key(id.as_str()))
            {
                return Err(invalid(
                    "Todas as etapas precisam de agentes e conexões válidos.",
                ));
            }
            let mut seen = BTreeSet::new();
            let mut cursor = Some(step.id.as_str());
            while let Some(id) = cursor {
                if !seen.insert(id) {
                    return Err(invalid("Conexões de conclusão devem chegar ao fim. Use a saída de correção para retornos."));
                }
                cursor = nodes
                    .get(id)
                    .ok_or_else(|| invalid("Conexão inválida."))?
                    .next
                    .as_deref();
            }
        }
        let mut visited = BTreeSet::new();
        let mut pending = vec![flow.entry.as_str()];
        while let Some(id) = pending.pop() {
            if visited.insert(id) {
                let step = nodes[id];
                pending.extend(
                    [step.next.as_deref(), step.on_rework.as_deref()]
                        .into_iter()
                        .flatten(),
                );
            }
        }
        if visited.len() != nodes.len() {
            return Err(invalid(
                "Há etapas desconectadas do início. Conecte ou remova esses blocos.",
            ));
        }
        Ok(())
    }

    pub(super) fn resolve(&self, id: &str) -> Result<RunDefinition, AgentError> {
        let flow = self
            .flows
            .iter()
            .find(|f| f.id == id)
            .ok_or_else(|| invalid("Este fluxo customizado não existe mais. Escolha outro fluxo."))?
            .clone();
        self.validate_flow(&flow)?;
        let agents = self
            .agents
            .iter()
            .filter(|a| flow.steps.iter().any(|s| s.agent_id == a.id))
            .cloned()
            .collect();
        Ok(RunDefinition { flow, agents })
    }

    pub(super) fn resolve_agent(&self, id: &str) -> Result<AgentDefinition, AgentError> {
        let agent = self
            .agents
            .iter()
            .find(|agent| agent.id == id)
            .ok_or_else(|| invalid("Este agente não existe mais. Escolha outro agente."))?
            .clone();
        if !agent.usage.allows_solo() {
            return Err(invalid(
                "Este agente está disponível somente dentro de fluxos.",
            ));
        }
        Ok(agent)
    }
}

pub(crate) fn read(home: &Path) -> Result<Catalog, AgentError> {
    let path = home.join(".jarvis/workflow-catalog.json");
    let meta = match fs::symlink_metadata(&path) {
        Ok(meta) => meta,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Catalog::default()),
        Err(_) => return Err(AgentError::storage()),
    };
    if !meta.is_file() || meta.is_symlink() || meta.len() > 4 * 1024 * 1024 {
        return Err(AgentError::storage());
    }
    let catalog: Catalog =
        serde_json::from_slice(&fs::read(path).map_err(|_| AgentError::storage())?).map_err(
            |_| invalid("O catálogo de Workflow não pôde ser lido. Os dados foram preservados."),
        )?;
    catalog.validate()?;
    Ok(catalog)
}

fn apply_model_bindings(
    db: &rusqlite::Connection,
    catalog: &mut Catalog,
) -> Result<(), AgentError> {
    for agent in &mut catalog.agents {
        if let Some(choice) = &agent.model {
            agent.model = Some(crate::model_bindings::resolve(
                db,
                &format!("custom:{}", agent.id),
                choice,
            )?);
        }
    }
    Ok(())
}

pub(crate) fn read_configured(
    db: &rusqlite::Connection,
    home: &Path,
) -> Result<Catalog, AgentError> {
    let mut catalog = read(home)?;
    apply_model_bindings(db, &mut catalog)?;
    catalog.revision = catalog
        .revision
        .checked_add(crate::model_bindings::revision(db)?)
        .ok_or_else(AgentError::storage)?;
    Ok(catalog)
}

fn change(
    home: &Path,
    revision: u64,
    apply: impl FnOnce(&mut Catalog) -> Result<(), AgentError>,
) -> Result<Catalog, AgentError> {
    let mut catalog = read(home)?;
    if catalog.revision != revision {
        return Err(invalid(
            "O catálogo mudou em outra janela. Reabra o editor antes de salvar.",
        ));
    }
    apply(&mut catalog)?;
    catalog.validate()?;
    catalog.revision = catalog
        .revision
        .checked_add(1)
        .ok_or_else(AgentError::storage)?;
    let directory = home.join(".jarvis");
    let mut file =
        tempfile::NamedTempFile::new_in(&directory).map_err(|_| AgentError::storage())?;
    serde_json::to_writer(file.as_file_mut(), &catalog).map_err(|_| AgentError::storage())?;
    file.as_file_mut()
        .sync_all()
        .map_err(|_| AgentError::storage())?;
    file.persist(directory.join("workflow-catalog.json"))
        .map_err(|_| AgentError::storage())?;
    #[cfg(unix)]
    fs::File::open(directory)
        .and_then(|file| file.sync_all())
        .map_err(|_| AgentError::storage())?;
    Ok(catalog)
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Mutation {
    SaveAgent { agent: AgentDefinition },
    SaveFlow { flow: FlowDefinition },
    DeleteAgent { id: String },
    DeleteFlow { id: String },
}

fn apply(catalog: &mut Catalog, mutation: Mutation) -> Result<(), AgentError> {
    match mutation {
        Mutation::SaveAgent { agent } => {
            if !storage::valid_id(&agent.id) {
                return Err(invalid("Agentes Jarvis são somente leitura."));
            }
            if let Some(current) = catalog.agents.iter_mut().find(|a| a.id == agent.id) {
                *current = agent;
            } else {
                catalog.agents.push(agent);
            }
        }
        Mutation::SaveFlow { flow } => {
            if !storage::valid_id(&flow.id) {
                return Err(invalid("Fluxos Jarvis são somente leitura."));
            }
            if let Some(current) = catalog.flows.iter_mut().find(|f| f.id == flow.id) {
                *current = flow;
            } else {
                catalog.flows.push(flow);
            }
        }
        Mutation::DeleteAgent { id } => {
            if !catalog.agents.iter().any(|a| a.id == id) {
                return Err(invalid("Agente customizado não encontrado."));
            }
            if catalog
                .flows
                .iter()
                .any(|f| f.steps.iter().any(|s| s.agent_id == id))
            {
                return Err(invalid(
                    "Este agente está vinculado a um fluxo. Remova os vínculos antes de excluí-lo.",
                ));
            }
            catalog.agents.retain(|a| a.id != id);
        }
        Mutation::DeleteFlow { id } => {
            if !catalog.flows.iter().any(|f| f.id == id) {
                return Err(invalid("Fluxo customizado não encontrado."));
            }
            catalog.flows.retain(|f| f.id != id);
        }
    }
    Ok(())
}

pub(crate) fn preview(catalog: &Catalog, mutation: Mutation) -> Result<Catalog, AgentError> {
    let mut candidate = catalog.clone();
    apply(&mut candidate, mutation)?;
    candidate.validate()?;
    candidate.revision = candidate
        .revision
        .checked_add(1)
        .ok_or_else(AgentError::storage)?;
    Ok(candidate)
}

pub(crate) fn mutate_configured(
    state: &AppState,
    home: &Path,
    revision: u64,
    mutation: Mutation,
) -> Result<Catalog, AgentError> {
    let _guard = CATALOG_LOCK.lock().map_err(|_| AgentError::storage())?;
    state.with_connection(home, |db| {
        let offset = crate::model_bindings::revision(db)?;
        let base = revision
            .checked_sub(offset)
            .ok_or_else(|| invalid("Os provedores mudaram. Reabra o editor antes de salvar."))?;
        let edited_agent = match &mutation {
            Mutation::SaveAgent { agent } => Some(agent.id.clone()),
            Mutation::DeleteAgent { id } => Some(id.clone()),
            _ => None,
        };
        let mut catalog = change(home, base, |catalog| {
            apply_model_bindings(db, catalog)?;
            apply(catalog, mutation)
        })?;
        if let Some(id) = edited_agent {
            crate::model_bindings::forget_item(db, &format!("custom:{id}"))?;
        }
        catalog.revision = catalog
            .revision
            .checked_add(offset)
            .ok_or_else(AgentError::storage)?;
        Ok(catalog)
    })
}

#[tauri::command]
pub async fn get_workflow_catalog(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<Catalog, AgentError> {
    let state = state.inner().clone();
    let home = app.path().home_dir().map_err(|_| AgentError::storage())?;
    tauri::async_runtime::spawn_blocking(move || {
        state.with_connection(&home, |db| read_configured(db, &home))
    })
    .await
    .map_err(|_| AgentError::internal())?
}

#[tauri::command]
pub async fn mutate_workflow_catalog(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    revision: u64,
    mutation: Mutation,
) -> Result<Catalog, AgentError> {
    let state = state.inner().clone();
    let home = app.path().home_dir().map_err(|_| AgentError::storage())?;
    let result = tauri::async_runtime::spawn_blocking(move || {
        mutate_configured(&state, &home, revision, mutation)
    })
    .await
    .map_err(|_| AgentError::internal())??;
    let _ = app.emit("workflow-catalog:changed", ());
    Ok(result)
}

#[cfg(test)]
pub(crate) mod tests;
