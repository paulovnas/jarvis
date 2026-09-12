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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub native_role: Option<Role>,
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

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BuiltinAgentDefinition {
    pub id: String,
    pub name: &'static str,
    pub description: &'static str,
    pub instructions: &'static str,
    pub role: Role,
    pub usage: AgentUsage,
    pub capability: Capability,
    pub appearance: Appearance,
    pub immutable: bool,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionKind {
    Delegation,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Connection {
    pub id: String,
    pub source: String,
    pub target: String,
    pub kind: ConnectionKind,
    pub label: &'static str,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BuiltinFlowDefinition {
    pub id: Flow,
    pub name: &'static str,
    pub description: &'static str,
    pub entry: String,
    pub max_steps: u8,
    pub steps: Vec<Step>,
    pub connections: Vec<Connection>,
    pub appearance: Appearance,
    pub immutable: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogView {
    pub revision: u64,
    pub agents: Vec<AgentDefinition>,
    pub flows: Vec<FlowDefinition>,
    pub builtin_agents: Vec<BuiltinAgentDefinition>,
    pub builtin_flows: Vec<BuiltinFlowDefinition>,
}

fn role_description(role: Role) -> &'static str {
    match role {
        Role::Planner => "Transforma o pedido em resultados observáveis, tarefas e dependências antes de delegar a execução.",
        Role::Investigator => "Investiga código, documentação e histórico e entrega evidências focadas para orientar decisões.",
        Role::Writer => "Registra especificações, critérios de aceite e dependências de forma executável.",
        Role::Orchestrator => "Coordena dependências, execução paralela, revisões e retomadas do fluxo.",
        Role::Designer => "Implementa interfaces e interações dentro do sistema visual e valida o escopo de frontend.",
        Role::Builder => "Implementa o comportamento solicitado, executa verificações e corrige o próprio trabalho.",
        Role::Reviewer => "Revisa a implementação de forma independente e decide se há correções pendentes.",
        Role::Github => "Prepara e executa publicações Git e GitHub somente após a aprovação explícita do usuário.",
        Role::Custom => "Agente definido pelo usuário.",
    }
}

fn role_capability(role: Role) -> Capability {
    match role {
        Role::Designer | Role::Builder | Role::Github => Capability::Commands,
        Role::Writer => Capability::WriteFiles,
        Role::Planner | Role::Investigator | Role::Orchestrator | Role::Reviewer | Role::Custom => {
            Capability::ReadOnly
        }
    }
}

fn role_appearance(role: Role) -> Appearance {
    use appearance::{Color, Icon};
    match role {
        Role::Planner => Appearance {
            icon: Icon::Brain,
            color: Color::Purple,
        },
        Role::Investigator => Appearance {
            icon: Icon::Search,
            color: Color::Cyan,
        },
        Role::Writer => Appearance {
            icon: Icon::Pen,
            color: Color::Red,
        },
        Role::Orchestrator => Appearance {
            icon: Icon::Workflow,
            color: Color::Yellow,
        },
        Role::Designer => Appearance {
            icon: Icon::Palette,
            color: Color::Purple,
        },
        Role::Builder => Appearance {
            icon: Icon::Code,
            color: Color::Blue,
        },
        Role::Reviewer => Appearance {
            icon: Icon::Shield,
            color: Color::Green,
        },
        Role::Github => Appearance {
            icon: Icon::Rocket,
            color: Color::Neutral,
        },
        Role::Custom => Appearance {
            icon: Icon::Bot,
            color: Color::Neutral,
        },
    }
}

pub(crate) fn builtin_agent(role: Role) -> Option<BuiltinAgentDefinition> {
    Some(BuiltinAgentDefinition {
        id: role.builtin_id()?,
        name: role.label(),
        description: role_description(role),
        instructions: role.contract(),
        role,
        usage: AgentUsage::FlowOnly,
        capability: role_capability(role),
        appearance: role_appearance(role),
        immutable: true,
    })
}

pub(crate) fn builtin_agents() -> Vec<BuiltinAgentDefinition> {
    [
        Role::Planner,
        Role::Investigator,
        Role::Writer,
        Role::Orchestrator,
        Role::Designer,
        Role::Builder,
        Role::Reviewer,
    ]
    .into_iter()
    .filter_map(builtin_agent)
    .collect()
}

fn runtime_builtin_agent(role: Role) -> Option<AgentDefinition> {
    let builtin = builtin_agent(role)?;
    Some(AgentDefinition {
        id: builtin.id,
        name: builtin.name.into(),
        description: builtin.description.into(),
        instructions: builtin.instructions.into(),
        native_role: Some(role),
        usage: builtin.usage,
        capability: builtin.capability,
        denied_tools: vec![],
        model: None,
        appearance: Some(builtin.appearance),
    })
}

fn flow_identity(flow: Flow) -> (&'static str, &'static str, Appearance) {
    use appearance::{Color, Icon};
    match flow {
        Flow::Standard => (
            "Padrão",
            "Da sua instrução à implementação.",
            Appearance {
                icon: Icon::Code,
                color: Color::Blue,
            },
        ),
        Flow::Designer => (
            "Designer",
            "Implementação especializada em design e frontend.",
            Appearance {
                icon: Icon::Palette,
                color: Color::Purple,
            },
        ),
        Flow::Planned => (
            "Planejado",
            "Planejamento seguido por execução especializada e validação do usuário.",
            Appearance {
                icon: Icon::Route,
                color: Color::Purple,
            },
        ),
        Flow::Complete => (
            "Completo",
            "Equipe coordenada da investigação à revisão independente.",
            Appearance {
                icon: Icon::Workflow,
                color: Color::Yellow,
            },
        ),
        Flow::Publication => (
            "Publicação",
            "Publicação supervisionada com Git e GitHub.",
            Appearance {
                icon: Icon::Rocket,
                color: Color::Green,
            },
        ),
        Flow::Custom => (
            "Customizado",
            "Fluxo definido pelo usuário.",
            Appearance {
                icon: Icon::Workflow,
                color: Color::Neutral,
            },
        ),
    }
}

fn node_position(flow: Flow, role: Role) -> Position {
    let (x, y) = match (flow, role) {
        (Flow::Standard, Role::Builder) | (Flow::Designer, Role::Designer) => (80.0, 120.0),
        (Flow::Planned, Role::Planner) => (40.0, 150.0),
        (Flow::Planned, Role::Builder) => (380.0, 40.0),
        (Flow::Planned, Role::Designer) => (380.0, 260.0),
        (Flow::Complete, Role::Planner) => (20.0, 210.0),
        (Flow::Complete, Role::Investigator) => (330.0, 20.0),
        (Flow::Complete, Role::Writer) => (330.0, 170.0),
        (Flow::Complete, Role::Orchestrator) => (330.0, 340.0),
        (Flow::Complete, Role::Builder) => (670.0, 80.0),
        (Flow::Complete, Role::Designer) => (670.0, 230.0),
        (Flow::Complete, Role::Reviewer) => (670.0, 380.0),
        _ => (40.0, 40.0),
    };
    Position { x, y }
}

fn node_id(flow: Flow, role: Role) -> String {
    format!("builtin:{}:{}", flow.id(), role.id())
}

fn delegation_label(role: Role) -> &'static str {
    match role {
        Role::Planner => "Replanejar",
        Role::Investigator => "Investigar",
        Role::Writer => "Especificar",
        Role::Orchestrator => "Coordenar",
        Role::Designer => "Implementar interface",
        Role::Builder => "Implementar",
        Role::Reviewer => "Revisar",
        Role::Github => "Publicar",
        Role::Custom => "Delegar",
    }
}

pub(crate) fn builtin_flows() -> Vec<BuiltinFlowDefinition> {
    [
        Flow::Standard,
        Flow::Designer,
        Flow::Planned,
        Flow::Complete,
    ]
    .into_iter()
    .map(|flow| {
        let roles = settings::roster(flow);
        let steps = roles
            .iter()
            .map(|role| Step {
                id: node_id(flow, *role),
                agent_id: role.builtin_id().expect("native roster excludes custom"),
                instructions: String::new(),
                position: node_position(flow, *role),
                next: None,
                on_rework: None,
            })
            .collect();
        let connections = flow
            .delegations()
            .iter()
            .map(|(source, target)| Connection {
                id: format!("{}:{}", node_id(flow, *source), node_id(flow, *target)),
                source: node_id(flow, *source),
                target: node_id(flow, *target),
                kind: ConnectionKind::Delegation,
                label: delegation_label(*target),
            })
            .collect();
        let (name, description, appearance) = flow_identity(flow);
        BuiltinFlowDefinition {
            id: flow,
            name,
            description,
            entry: node_id(flow, flow.root()),
            max_steps: 48,
            steps,
            connections,
            appearance,
            immutable: true,
        }
    })
    .collect()
}

impl From<Catalog> for CatalogView {
    fn from(catalog: Catalog) -> Self {
        Self {
            revision: catalog.revision,
            agents: catalog.agents,
            flows: catalog.flows,
            builtin_agents: builtin_agents(),
            builtin_flows: builtin_flows(),
        }
    }
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
            if agent.native_role.is_some()
                || !text_valid(&agent.name, 100, true)
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
            if let Some(agent) = self.agents.iter().find(|agent| agent.id == step.agent_id) {
                if !agent.usage.allows_flow() {
                    return Err(invalid(
                        "Agentes Solo não podem fazer parte de fluxos. Altere o uso para Misto ou Somente em fluxos.",
                    ));
                }
            } else if Role::from_builtin_id(&step.agent_id).is_none() {
                return Err(invalid("Todas as etapas precisam de agentes existentes."));
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
        let mut ids = BTreeSet::new();
        let mut agents = Vec::new();
        for step in &flow.steps {
            if !ids.insert(step.agent_id.as_str()) {
                continue;
            }
            if let Some(agent) = self.agents.iter().find(|agent| agent.id == step.agent_id) {
                agents.push(agent.clone());
            } else if let Some(role) = Role::from_builtin_id(&step.agent_id) {
                agents.push(runtime_builtin_agent(role).ok_or_else(AgentError::internal)?);
            }
        }
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
    let path = crate::data_dir::root(home).join("workflow-catalog.json");
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
    let directory = crate::data_dir::root(home);
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
) -> Result<CatalogView, AgentError> {
    let state = state.inner().clone();
    let home = app.path().home_dir().map_err(|_| AgentError::storage())?;
    let catalog = tauri::async_runtime::spawn_blocking(move || {
        state.with_connection(&home, |db| read_configured(db, &home))
    })
    .await
    .map_err(|_| AgentError::internal())??;
    Ok(catalog.into())
}

#[tauri::command]
pub async fn mutate_workflow_catalog(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    revision: u64,
    mutation: Mutation,
) -> Result<CatalogView, AgentError> {
    let state = state.inner().clone();
    let home = app.path().home_dir().map_err(|_| AgentError::storage())?;
    let result = tauri::async_runtime::spawn_blocking(move || {
        mutate_configured(&state, &home, revision, mutation)
    })
    .await
    .map_err(|_| AgentError::internal())??;
    let _ = app.emit("workflow-catalog:changed", ());
    Ok(result.into())
}

#[cfg(test)]
pub(crate) mod tests;
