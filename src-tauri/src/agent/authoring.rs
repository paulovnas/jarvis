//! Supervised authoring for user-owned Jarvis agents and workflows.
use super::{
    cancelled, journal, next_revision, workflow, AgentError, AgentState, ChatSnapshot, Session,
    ToolCall,
};
use crate::{openai_codex::OpenAiCodexState, persistence::AppState};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{collections::BTreeSet, path::Path};
use tauri::{Emitter, Manager};
use tokio::sync::{oneshot, watch};

pub const INSTRUCTIONS: &str = r#"
Jarvis product capabilities: Jarvis is a local desktop coding-agent environment organized as workspaces, projects and durable conversations. It provides direct, planned, complete and user-defined workflows; native direct-task tracking; Beads planning for delegated work; Context-mode retrieval and compaction; Context7 documentation; Open Design resources; skills; MCP tools; attachments, Vision and image generation when configured; web search; persistent processes, terminals and an integrated browser; project validation and user notifications. Only capabilities whose tools are present in the current turn are actually available.

Users own custom agents and custom workflows. Custom agents declare where they can run: solo as the primary chat agent, flow_only as a workflow step, or mixed in both contexts. Built-in Jarvis agents may be referenced as immutable steps in custom workflows; built-in flows are immutable templates whose real topology is available in the catalog. When the user asks to create or edit an agent or workflow, inspect the current catalog with jarvis_catalog, clarify only material missing choices with ask_user, then submit the smallest complete proposal with jarvis_propose_agent or jarvis_propose_flow. Never write Jarvis configuration files with filesystem or shell tools. A proposal does not change settings until the user explicitly approves it in Jarvis. After rejection, respect the user's note and do not resubmit an unchanged proposal. Re-read the catalog before a dependent or revised proposal because every accepted change advances its revision.
"#;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    Create,
    Update,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentReference {
    id: String,
    name: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Target {
    Agent {
        before: Option<workflow::catalog::AgentDefinition>,
        after: workflow::catalog::AgentDefinition,
    },
    Flow {
        before: Option<workflow::catalog::FlowDefinition>,
        after: workflow::catalog::FlowDefinition,
    },
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingProposal {
    pub(super) turn_id: String,
    pub(super) tool_id: String,
    pub(super) action: Action,
    pub(super) summary: String,
    pub(super) catalog_revision: u64,
    pub(super) target: Target,
    pub(super) agent_references: Vec<AgentReference>,
}

pub(super) struct Pending {
    pub request: PendingProposal,
    mutation: workflow::catalog::Mutation,
    started: std::time::Instant,
    reply: oneshot::Sender<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AgentRequest {
    action: Action,
    catalog_revision: u64,
    summary: String,
    agent: workflow::catalog::AgentDefinition,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct FlowRequest {
    action: Action,
    catalog_revision: u64,
    summary: String,
    flow: workflow::catalog::FlowDefinition,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Decision {
    turn_id: String,
    tool_id: String,
    approved: bool,
    note: Option<String>,
}

fn invalid(message: &str) -> AgentError {
    AgentError::new("invalid_authoring_proposal", message)
}

fn bounded_summary(summary: String) -> Result<String, AgentError> {
    let summary = summary.trim().to_owned();
    if summary.is_empty() || summary.chars().count() > 1_000 || summary.contains('\0') {
        return Err(invalid(
            "Explique a alteração proposta em até 1.000 caracteres.",
        ));
    }
    Ok(summary)
}

fn agent_schema() -> Value {
    json!({
        "type":"object", "additionalProperties":false,
        "required":["id","name","description","instructions","usage","capability","model"],
        "properties":{
            "id":{"type":"string","pattern":"^[a-fA-F0-9]{32}$","description":"Stable 32-character hexadecimal ID. Preserve it when editing."},
            "name":{"type":"string","minLength":1,"maxLength":100},
            "description":{"type":"string","maxLength":500},
            "instructions":{"type":"string","minLength":1,"maxLength":16000},
            "usage":{"type":"string","enum":["solo","mixed","flow_only"],"description":"Where this agent can run: solo as the primary chat agent, flow_only only as a workflow step, or mixed in both contexts."},
            "capability":{"type":"string","enum":["read_only","write_files","commands"]},
            "deniedTools":{"type":"array","maxItems":256,"items":{"type":"string","minLength":1,"maxLength":128}},
            "model":{"anyOf":[{"type":"null"},{"type":"object","additionalProperties":false,"required":["account","model","reasoning"],"properties":{"account":{"type":"string","minLength":1,"maxLength":200},"model":{"type":"string","minLength":1,"maxLength":200},"reasoning":{"anyOf":[{"type":"null"},{"type":"string","minLength":1,"maxLength":40}]}}}]},
            "appearance":{"anyOf":[{"type":"null"},{"type":"object","additionalProperties":false,"required":["icon","color"],"properties":{"icon":{"type":"string","enum":["bot","workflow","route","brain","search","code","palette","shield","terminal","wrench","book","sparkles","target","pen","lightbulb","rocket"]},"color":{"type":"string","enum":["blue","green","cyan","yellow","red","purple","neutral"]}}}]}
        }
    })
}

fn flow_schema() -> Value {
    json!({
        "type":"object", "additionalProperties":false,
        "required":["id","name","description","entry","maxSteps","steps"],
        "properties":{
            "id":{"type":"string","pattern":"^[a-fA-F0-9]{32}$","description":"Stable 32-character hexadecimal ID. Preserve it when editing."},
            "name":{"type":"string","minLength":1,"maxLength":100},
            "description":{"type":"string","maxLength":500},
            "entry":{"type":"string","pattern":"^[a-fA-F0-9]{32}$"},
            "maxSteps":{"type":"integer","minimum":1,"maximum":48},
            "steps":{"type":"array","minItems":1,"maxItems":24,"items":{"type":"object","additionalProperties":false,"required":["id","agentId","instructions","position","next","onRework"],"properties":{
                "id":{"type":"string","pattern":"^[a-fA-F0-9]{32}$"},
                "agentId":{"description":"A custom 32-character hexadecimal agent ID or an immutable builtin:* ID returned by jarvis_catalog.","anyOf":[{"type":"string","pattern":"^[a-fA-F0-9]{32}$"},{"type":"string","enum":["builtin:planner","builtin:investigator","builtin:writer","builtin:orchestrator","builtin:designer","builtin:builder","builtin:reviewer"]}]},
                "instructions":{"type":"string","maxLength":8000},
                "position":{"type":"object","additionalProperties":false,"required":["x","y"],"properties":{"x":{"type":"number","minimum":-100000,"maximum":100000},"y":{"type":"number","minimum":-100000,"maximum":100000}}},
                "next":{"anyOf":[{"type":"null"},{"type":"string","pattern":"^[a-fA-F0-9]{32}$"}]},
                "onRework":{"anyOf":[{"type":"null"},{"type":"string","pattern":"^[a-fA-F0-9]{32}$"}]}
            }}},
            "appearance":{"anyOf":[{"type":"null"},{"type":"object","additionalProperties":false,"required":["icon","color"],"properties":{"icon":{"type":"string","enum":["bot","workflow","route","brain","search","code","palette","shield","terminal","wrench","book","sparkles","target","pen","lightbulb","rocket"]},"color":{"type":"string","enum":["blue","green","cyan","yellow","red","purple","neutral"]}}}]}
        }
    })
}

fn proposal_definition(name: &str, description: &str, field: &str, schema: Value) -> Value {
    json!({
        "type":"function", "name":name, "description":description,
        "parameters":{"type":"object","additionalProperties":false,"required":["action","catalogRevision","summary",field],"properties":{
            "action":{"type":"string","enum":["create","update"]},
            "catalogRevision":{"type":"integer","minimum":0,"description":"Exact revision returned by the latest jarvis_catalog call."},
            "summary":{"type":"string","minLength":1,"maxLength":1000,"description":"Concise Brazilian Portuguese explanation of the intended behavior and material choices for the approval drawer."},
            (field):schema
        }}
    })
}

pub(super) fn definitions() -> Vec<Value> {
    vec![
        json!({"type":"function","name":"jarvis_catalog","description":"Inspect Jarvis capabilities and the current user-owned agent/workflow catalog before proposing a creation or edit. Use overview first; request one custom agent or flow by its ID for full editable details. Built-in agents and flows are listed as immutable and can never be edited.","parameters":{"type":"object","additionalProperties":false,"required":["view"],"properties":{"view":{"type":"string","enum":["overview","agent","flow"]},"id":{"type":"string","description":"Required for agent or flow detail."}}}}),
        proposal_definition("jarvis_propose_agent", "Propose creating or editing one user-owned Jarvis agent. The call waits for explicit approval in a Jarvis drawer; it never changes built-in agents. Call jarvis_catalog immediately beforehand and use its exact revision. For create, generate a new 32-character hexadecimal ID. For update, preserve the existing ID. Choose whether it runs solo, only in flows, or both. A null model inherits the current chat model.", "agent", agent_schema()),
        proposal_definition("jarvis_propose_flow", "Propose creating or editing one user-owned Jarvis workflow. The call waits for explicit approval in a Jarvis drawer; it never changes built-in flows. Call jarvis_catalog immediately beforehand and use its exact revision. A flow may reference immutable builtin:* agents or mixed/flow_only custom agents from that revision. Generate stable 32-character hexadecimal IDs for a new flow and its steps; preserve existing IDs when editing.", "flow", flow_schema()),
    ]
}

fn overview(catalog: &workflow::catalog::Catalog) -> Value {
    let agents: Vec<_> = catalog
        .agents
        .iter()
        .map(|agent| {
            json!({"id":agent.id,"name":agent.name,"description":agent.description,"usage":agent.usage,"capability":agent.capability,"model":agent.model,"mutable":true})
        })
        .collect();
    let flows: Vec<_> = catalog
        .flows
        .iter()
        .map(|flow| {
            json!({"id":flow.id,"name":flow.name,"description":flow.description,"steps":flow.steps.len(),"mutable":true})
        })
        .collect();
    let builtin_agents: Vec<_> = workflow::catalog::builtin_agents()
        .into_iter()
        .map(|agent| json!({"id":agent.id,"name":agent.name,"description":agent.description,"role":agent.role,"capability":agent.capability,"mutable":false}))
        .collect();
    let builtin_flows: Vec<_> = workflow::catalog::builtin_flows()
        .into_iter()
        .map(|flow| json!({
            "id":flow.id,
            "name":flow.name,
            "description":flow.description,
            "entry":flow.entry,
            "steps":flow.steps.iter().map(|step| json!({"id":step.id,"agentId":step.agent_id})).collect::<Vec<_>>(),
            "connections":flow.connections,
            "mutable":false
        }))
        .collect();
    json!({
        "revision":catalog.revision,
        "limits":{"agents":64,"flows":32,"stepsPerFlow":24,"executionsPerFlow":48},
        "idFormat":"32 hexadecimal characters",
        "capabilities":{
            "read_only":"Pesquisa, leitura, perguntas e ferramentas sem mutação.",
            "write_files":"Também pode criar e editar arquivos e manter o planejamento permitido pelo fluxo.",
            "commands":"Também pode executar comandos, processos, terminais, MCPs e ações de navegador permitidas."
        },
        "agentUsage":{
            "solo":"Selectable as the primary agent in a chat and unavailable to workflows.",
            "mixed":"Selectable in chats and available as a workflow step.",
            "flow_only":"Available only as a workflow step. This is the compatibility default for older saved agents."
        },
        "toolCatalog":workflow::catalog::permissions::builtin_permissions(),
        "builtIn":{"mutable":false,"flows":builtin_flows,"agents":builtin_agents},
        "custom":{"agents":agents,"flows":flows},
        "rules":["Built-in definitions are immutable.","Every proposal requires explicit user approval.","Read the latest revision again before a dependent proposal."]
    })
}

pub(super) fn catalog_output(
    catalog: &workflow::catalog::Catalog,
    args: &Value,
) -> Result<String, AgentError> {
    let view = args["view"]
        .as_str()
        .ok_or_else(|| invalid("Escolha overview, agent ou flow."))?;
    let value = match view {
        "overview" => overview(catalog),
        "agent" => {
            let id = args["id"]
                .as_str()
                .ok_or_else(|| invalid("Informe o ID do agente."))?;
            if let Some(agent) = catalog.agents.iter().find(|agent| agent.id == id) {
                json!({"revision":catalog.revision,"mutable":true,"agent":agent})
            } else {
                let agent = workflow::catalog::builtin_agents()
                    .into_iter()
                    .find(|agent| agent.id == id)
                    .ok_or_else(|| invalid("Agente não encontrado."))?;
                json!({"revision":catalog.revision,"mutable":false,"agent":agent})
            }
        }
        "flow" => {
            let id = args["id"]
                .as_str()
                .ok_or_else(|| invalid("Informe o ID do fluxo."))?;
            if let Some(flow) = catalog.flows.iter().find(|flow| flow.id == id) {
                json!({"revision":catalog.revision,"mutable":true,"flow":flow,"agents":references_for_flow(catalog, flow)})
            } else {
                let flow = workflow::catalog::builtin_flows()
                    .into_iter()
                    .find(|flow| flow.id.id() == id)
                    .ok_or_else(|| invalid("Fluxo não encontrado."))?;
                json!({"revision":catalog.revision,"mutable":false,"flow":flow,"agents":workflow::catalog::builtin_agents()})
            }
        }
        _ => return Err(invalid("Visualização do catálogo inválida.")),
    };
    serde_json::to_string(&value).map_err(|_| AgentError::internal())
}

fn references_for_flow(
    catalog: &workflow::catalog::Catalog,
    flow: &workflow::catalog::FlowDefinition,
) -> Vec<AgentReference> {
    let ids: BTreeSet<_> = flow
        .steps
        .iter()
        .map(|step| step.agent_id.as_str())
        .collect();
    let mut references: Vec<_> = catalog
        .agents
        .iter()
        .filter(|agent| ids.contains(agent.id.as_str()))
        .map(|agent| AgentReference {
            id: agent.id.clone(),
            name: agent.name.clone(),
        })
        .collect();
    references.extend(
        workflow::catalog::builtin_agents()
            .into_iter()
            .filter(|agent| ids.contains(agent.id.as_str()))
            .map(|agent| AgentReference {
                id: agent.id,
                name: agent.name.into(),
            }),
    );
    references
}

fn references(catalog: &workflow::catalog::Catalog, target: &Target) -> Vec<AgentReference> {
    let Target::Flow { after, .. } = target else {
        return vec![];
    };
    let ids: BTreeSet<_> = after
        .steps
        .iter()
        .map(|step| step.agent_id.as_str())
        .collect();
    let mut references: Vec<_> = catalog
        .agents
        .iter()
        .filter(|agent| ids.contains(agent.id.as_str()))
        .map(|agent| AgentReference {
            id: agent.id.clone(),
            name: agent.name.clone(),
        })
        .collect();
    references.extend(
        workflow::catalog::builtin_agents()
            .into_iter()
            .filter(|agent| ids.contains(agent.id.as_str()))
            .map(|agent| AgentReference {
                id: agent.id,
                name: agent.name.into(),
            }),
    );
    references
}

fn prepare(
    catalog: &workflow::catalog::Catalog,
    tool: &ToolCall,
) -> Result<(PendingProposal, workflow::catalog::Mutation), AgentError> {
    let (action, revision, summary, mutation, target) = match tool.name.as_str() {
        "jarvis_propose_agent" => {
            let request: AgentRequest = serde_json::from_value(tool.args.clone())
                .map_err(|_| invalid("Proposta de agente inválida."))?;
            let before = catalog
                .agents
                .iter()
                .find(|agent| agent.id == request.agent.id)
                .cloned();
            let mutation = workflow::catalog::Mutation::SaveAgent {
                agent: request.agent.clone(),
            };
            (
                request.action,
                request.catalog_revision,
                bounded_summary(request.summary)?,
                mutation,
                Target::Agent {
                    before,
                    after: request.agent,
                },
            )
        }
        "jarvis_propose_flow" => {
            let request: FlowRequest = serde_json::from_value(tool.args.clone())
                .map_err(|_| invalid("Proposta de fluxo inválida."))?;
            let before = catalog
                .flows
                .iter()
                .find(|flow| flow.id == request.flow.id)
                .cloned();
            let mutation = workflow::catalog::Mutation::SaveFlow {
                flow: request.flow.clone(),
            };
            (
                request.action,
                request.catalog_revision,
                bounded_summary(request.summary)?,
                mutation,
                Target::Flow {
                    before,
                    after: request.flow,
                },
            )
        }
        _ => return Err(invalid("Ferramenta de autoria desconhecida.")),
    };
    if revision != catalog.revision {
        return Err(invalid(
            "O catálogo mudou. Consulte jarvis_catalog novamente antes de propor.",
        ));
    }
    let exists = match &target {
        Target::Agent { before, .. } => before.is_some(),
        Target::Flow { before, .. } => before.is_some(),
    };
    match (action, exists) {
        (Action::Create, true) => {
            return Err(invalid(
                "Este ID já pertence a um item customizado. Consulte o catálogo e gere outro ID.",
            ))
        }
        (Action::Update, false) => {
            return Err(invalid(
                "Somente itens customizados existentes podem ser editados. Agentes e fluxos nativos são imutáveis.",
            ))
        }
        _ => {}
    }
    let candidate = workflow::catalog::preview(catalog, mutation.clone())?;
    let agent_references = references(&candidate, &target);
    Ok((
        PendingProposal {
            turn_id: String::new(),
            tool_id: tool.id.clone(),
            action,
            summary,
            catalog_revision: revision,
            target,
            agent_references,
        },
        mutation,
    ))
}

pub(super) async fn execute(
    session: &Session,
    state: &AppState,
    oauth: &OpenAiCodexState,
    home: &Path,
    tool: &ToolCall,
    mut signal: watch::Receiver<bool>,
) -> Result<String, AgentError> {
    let catalog = state.with_connection(home, |db| workflow::catalog::read_configured(db, home))?;
    if tool.name == "jarvis_catalog" {
        return catalog_output(&catalog, &tool.args);
    }
    let (mut request, mutation) = prepare(&catalog, tool)?;
    if let Target::Agent { after, .. } = &request.target {
        if let Some(model) = &after.model {
            oauth.inference_model(
                state,
                home,
                &model.account,
                &model.model,
                model.reasoning.as_deref(),
            )?;
        }
    }
    if *signal.borrow() {
        return Err(AgentError::cancelled());
    }
    let (reply, received) = oneshot::channel();
    session.update(true, |data| {
        if let Some(active) = &mut data.active {
            request.turn_id.clone_from(&active.id);
            active.authoring = Some(Pending {
                request,
                mutation,
                started: std::time::Instant::now(),
                reply,
            });
        }
    })?;
    tokio::select! {
        _ = cancelled(&mut signal) => Err(AgentError::cancelled()),
        result = received => result.map_err(|_| AgentError::cancelled()),
    }
}

pub(super) fn cancelled_output() -> String {
    json!({"approved":false,"status":"cancelled","note":"A solicitação foi cancelada antes de uma decisão."}).to_string()
}

fn answer_with(
    session: &Session,
    turn_id: &str,
    tool_id: &str,
    approved: bool,
    note: Option<String>,
    apply: impl FnOnce(u64, workflow::catalog::Mutation) -> Result<u64, AgentError>,
) -> Result<(ChatSnapshot, bool), AgentError> {
    let note = note
        .map(|note| note.trim().to_owned())
        .filter(|note| !note.is_empty());
    if note
        .as_ref()
        .is_some_and(|note| note.chars().count() > 2_000 || note.contains('\0'))
    {
        return Err(invalid("A observação pode ter até 2.000 caracteres."));
    }
    let mut data = session.data.lock().map_err(|_| AgentError::internal())?;
    if data.storage_failed {
        return Err(AgentError::storage());
    }
    let pending = data
        .active
        .as_ref()
        .filter(|active| active.id == turn_id && !*active.cancel.borrow())
        .and_then(|active| active.authoring.as_ref())
        .filter(|pending| pending.request.tool_id == tool_id)
        .ok_or_else(|| {
            AgentError::new(
                "stale_authoring_proposal",
                "Esta proposta não está mais aguardando aprovação.",
            )
        })?;
    let mutation = pending.mutation.clone();
    let revision = pending.request.catalog_revision;
    let elapsed = pending.started.elapsed().as_millis() as u64;
    let catalog_revision = if approved {
        Some(apply(revision, mutation)?)
    } else {
        None
    };
    let output = json!({
        "approved": approved,
        "status": if approved { "applied" } else { "rejected" },
        "note": note,
        "catalogRevision": catalog_revision,
    })
    .to_string();
    let previous = data
        .turns
        .last()
        .filter(|turn| turn.turn.id == turn_id)
        .cloned()
        .ok_or_else(AgentError::internal)?;
    let mut current = previous.clone();
    let tool = current
        .turn
        .steps
        .iter_mut()
        .flat_map(|step| &mut step.tools)
        .find(|tool| {
            tool.id == tool_id
                && matches!(
                    tool.name.as_str(),
                    "jarvis_propose_agent" | "jarvis_propose_flow"
                )
        })
        .ok_or_else(AgentError::internal)?;
    tool.output.clone_from(&output);
    tool.status = "completed".into();
    tool.duration_ms = elapsed;
    current
        .wire
        .push(json!({"type":"function_call_output","call_id":tool_id,"output":output}));
    if journal::append_update(&session.journal, &previous, &current).is_err() {
        data.storage_failed = true;
        if let Some(active) = &data.active {
            let _ = active.cancel.send(true);
        }
        return Err(AgentError::storage());
    }
    *data.turns.last_mut().ok_or_else(AgentError::internal)? = current;
    let pending = data
        .active
        .as_mut()
        .and_then(|active| active.authoring.take())
        .ok_or_else(AgentError::internal)?;
    data.revision = next_revision();
    let snapshot = session.snapshot_data(&data);
    drop(data);
    (session.emit)(snapshot.clone());
    let _ = pending.reply.send(output);
    Ok((snapshot, approved))
}

pub(super) fn answer(
    app: &tauri::AppHandle,
    state: &AppState,
    home: &Path,
    session: &Session,
    decision: Decision,
) -> Result<ChatSnapshot, AgentError> {
    let Decision {
        turn_id,
        tool_id,
        approved,
        note,
    } = decision;
    let (snapshot, changed) = answer_with(
        session,
        &turn_id,
        &tool_id,
        approved,
        note,
        |revision, mutation| {
            workflow::catalog::mutate_configured(state, home, revision, mutation)
                .map(|catalog| catalog.revision)
        },
    )?;
    if changed {
        let _ = app.emit("workflow-catalog:changed", ());
    }
    Ok(snapshot)
}

#[tauri::command]
pub fn answer_agent_authoring(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    agent: tauri::State<'_, AgentState>,
    conversation_id: String,
    decision: Decision,
) -> Result<ChatSnapshot, AgentError> {
    let home = app.path().home_dir().map_err(|_| AgentError::storage())?;
    let session = agent.existing(&conversation_id)?;
    answer(&app, state.inner(), &home, &session, decision)
}

#[cfg(test)]
mod tests;
