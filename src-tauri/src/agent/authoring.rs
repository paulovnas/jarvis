//! Supervised authoring for user-owned Jarvis agents and workflows.
use super::{
    cancelled, next_revision, publication, workflow, AgentError, AgentState, ChatSnapshot, Session,
    ToolCall,
};
use crate::{openai_codex::OpenAiCodexState, persistence::AppState};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::BTreeSet,
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};
use tauri::{Emitter, Manager};
use tokio::sync::{oneshot, watch};

mod hook;
mod mcp;
mod plugin;
mod project_instructions;
mod redaction;
pub(super) use mcp::Values as McpValues;
pub(in crate::agent) use redaction::sanitize_mcp_args;

pub const INSTRUCTIONS: &str = r#"
Jarvis product capabilities: Jarvis is a local desktop coding-agent environment organized as workspaces, projects and durable conversations. It provides direct, planned, complete and user-defined workflows; native direct-task tracking; Beads planning for delegated work; Context-mode retrieval and compaction; Context7 documentation; Impeccable design skills, inspection hooks and live visual iteration; skills; MCP tools; attachments, Vision and image generation when configured; web search; persistent processes, terminals and an integrated browser; project validation and user notifications. Only capabilities whose tools are present in the current turn are actually available.

Users own custom agents and custom workflows. Custom agents declare where they can run: solo as the primary chat agent, flow_only as a workflow step, or mixed in both contexts. Built-in Jarvis agents may be referenced as immutable steps in custom workflows; built-in flows are immutable templates whose real topology is available in the catalog. When the user asks to create or edit an agent or workflow, inspect the current catalog with jarvis_catalog, clarify only material missing choices with ask_user, then submit the smallest complete proposal with jarvis_propose_agent or jarvis_propose_flow. Never write Jarvis configuration files with filesystem or shell tools. A proposal does not change settings until the user explicitly approves it in Jarvis. After rejection, respect the user's note and do not resubmit an unchanged proposal. Re-read the catalog before a dependent or revised proposal because every accepted change advances its revision.

When asked to configure an MCP, inspect the public mcpServers metadata in jarvis_catalog and use jarvis_propose_mcp. The native panel always asks the user to approve this global registration, even in YOLO. Never include credentials in a proposal, arguments, URL or summary: list envKeys/headerKeys and let the user supply their values privately in that panel. Do not request existing stored credentials. Registration does not install, connect or authorize every operation of the MCP. After approval, use the MCP discovery tools to find and activate it in this conversation; report actual connection/authentication failures and preserve the saved registration.

When asked to configure hooks, read the hooks catalog with jarvis_catalog view=hooks and the native jarvis-hooks skill. Use jarvis_propose_hook for create/update/delete at its exact hooks revision. Native hooks are immutable. Command hooks run locally in project conversations; never write their settings through shell/filesystem tools. Every proposal requires explicit native approval, even in YOLO. Preserve unrelated hooks, use bounded matchers/timeouts and never include credentials in commands. Saved changes apply to subsequent turns, not the running turn.

When asked to manage plugins or marketplaces, read jarvis_catalog view=plugins and the native jarvis-plugins skill, then use jarvis_propose_plugin at the exact pluginsRevision. Review the actual package, dependencies and native component conflicts. Installation and hook trust are separate approvals. Never edit plugin registries through shell/filesystem tools, include credentials in drafts, promise unavailable Apps or repeat an uncertain mutation. Newly approved plugin skills and MCP servers become discoverable on the next model step; versions already pinned in this turn update on the next turn. Disabling a plugin/component or revoking its authorization takes effect immediately. Hook configuration and trust changes keep their existing subsequent-turn contract.

When asked to create or update a project's AGENTS.md, read jarvis_catalog view=project_instructions and the native jarvis-authoring skill. Inspect the relevant project conventions and confirmed validation commands, then propose a short project-specific section with jarvis_propose_project_instructions at the exact file revision. The native review always requires explicit approval, including YOLO, and only changes Jarvis's own section in the project root. Preserve user rules, managed sections and nested instruction files. Never overwrite the complete file or bypass review through shell/filesystem tools. If AGENTS.md is missing and persistent project guidance would help, offer creation once at a natural completion or project-setup moment; keep the current task moving, do not repeat the offer on unrelated turns, and wait for a request before drafting. Do not install a global behavior plugin or copy third-party guidance wholesale. Changes apply to subsequent turns.
"#;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    Create,
    Update,
    Publish,
    Delete,
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
    Publication {
        after: publication::Proposal,
    },
    Mcp {
        server: mcp::Draft,
    },
    Hook {
        before: Option<crate::hooks::Hook>,
        after: Option<crate::hooks::Hook>,
    },
    Plugin {
        preview: crate::plugins::Preview,
    },
    ProjectInstructions {
        path: String,
        before: Option<String>,
        after: String,
    },
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingProposal {
    pub(super) turn_id: String,
    pub(super) tool_id: String,
    pub(super) action: Action,
    pub(super) summary: String,
    pub(super) catalog_revision: Option<u64>,
    pub(super) target: Target,
    pub(super) agent_references: Vec<AgentReference>,
}

pub(super) struct Pending {
    pub request: PendingProposal,
    mutation: Mutation,
    started: std::time::Instant,
    reply: oneshot::Sender<String>,
    claimed: Arc<AtomicBool>,
    _receipt: Option<super::turn_state::InteractionReceipt>,
}

#[derive(Clone)]
enum Mutation {
    Catalog(workflow::catalog::Mutation),
    Publication(publication::Proposal),
    Mcp(mcp::Draft),
    Hook(hook::Change),
    Plugin(Box<crate::plugins::Prepared>),
    ProjectInstructions(project_instructions::Change),
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct McpRequest {
    summary: String,
    server: mcp::Draft,
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

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Decision {
    pub(super) turn_id: String,
    pub(super) tool_id: String,
    pub(super) approved: bool,
    pub(super) note: Option<String>,
    #[serde(default)]
    pub(super) mcp_values: Option<mcp::Values>,
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

pub(in crate::agent) fn agent_schema() -> Value {
    let primary = json!({"type":"object","additionalProperties":false,"required":["account","model","reasoning"],"properties":{"executor":{"type":"string","enum":["jarvis","claude"],"description":"Defaults to jarvis. Claude uses the official local CLI and an empty account."},"account":{"type":"string","maxLength":200},"model":{"type":"string","minLength":1,"maxLength":200},"reasoning":{"anyOf":[{"type":"null"},{"type":"string","minLength":1,"maxLength":40}]},"serviceTier":{"anyOf":[{"type":"null"},{"type":"string","enum":["priority"]}],"description":"Use priority only when the selected ChatGPT model advertises Fast. Omit or null keeps Normal."}}});
    let mut model = primary.clone();
    model["properties"]["fallback"] = json!({"anyOf":[{"type":"null"},primary]});
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
            "model":{"anyOf":[{"type":"null"},model]},
            "appearance":{"anyOf":[{"type":"null"},{"type":"object","additionalProperties":false,"required":["icon","color"],"properties":{"icon":{"type":"string","enum":["bot","workflow","route","brain","search","code","palette","film","shield","terminal","wrench","book","sparkles","target","pen","lightbulb","rocket"]},"color":{"type":"string","enum":["blue","green","cyan","yellow","red","purple","neutral"]}}}]}
        }
    })
}

pub(in crate::agent) fn flow_schema() -> Value {
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
                "agentId":{"description":"A custom 32-character hexadecimal agent ID or an immutable builtin:* ID returned by jarvis_catalog.","anyOf":[{"type":"string","pattern":"^[a-fA-F0-9]{32}$"},{"type":"string","enum":["builtin:planner","builtin:investigator","builtin:writer","builtin:orchestrator","builtin:designer","builtin:video","builtin:builder","builtin:reviewer","builtin:github"]}]},
                "instructions":{"type":"string","maxLength":8000},
                "position":{"type":"object","additionalProperties":false,"required":["x","y"],"properties":{"x":{"type":"number","minimum":-100000,"maximum":100000},"y":{"type":"number","minimum":-100000,"maximum":100000}}},
                "next":{"anyOf":[{"type":"null"},{"type":"string","pattern":"^[a-fA-F0-9]{32}$"}]},
                "onRework":{"anyOf":[{"type":"null"},{"type":"string","pattern":"^[a-fA-F0-9]{32}$"}]}
            }}},
            "appearance":{"anyOf":[{"type":"null"},{"type":"object","additionalProperties":false,"required":["icon","color"],"properties":{"icon":{"type":"string","enum":["bot","workflow","route","brain","search","code","palette","film","shield","terminal","wrench","book","sparkles","target","pen","lightbulb","rocket"]},"color":{"type":"string","enum":["blue","green","cyan","yellow","red","purple","neutral"]}}}]}
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
        json!({"type":"function","name":"jarvis_catalog","description":"Inspect Jarvis capabilities and configured agents, flows, hooks and plugins before proposing changes. Use overview first; agent/flow by ID for editable details; hooks or plugins for their separate exact revisions and component definitions; project_instructions for the current project's root AGENTS.md and exact file revision. Native entries are immutable.","parameters":{"type":"object","additionalProperties":false,"required":["view"],"properties":{"view":{"type":"string","enum":["overview","agent","flow","hooks","plugins","project_instructions"]},"id":{"type":"string","description":"Required for agent or flow detail."}}}}),
        proposal_definition("jarvis_propose_agent", "Propose creating or editing one user-owned Jarvis agent. The call waits for explicit approval in a Jarvis drawer; it never changes built-in agents. Call jarvis_catalog immediately beforehand and use its exact revision. For create, generate a new 32-character hexadecimal ID. For update, preserve the existing ID. Choose whether it runs solo, only in flows, or both. A null model inherits the current chat model.", "agent", agent_schema()),
        proposal_definition("jarvis_propose_flow", "Propose creating or editing one user-owned Jarvis workflow. The call waits for explicit approval in a Jarvis drawer; it never changes built-in flows. Call jarvis_catalog immediately beforehand and use its exact revision. A flow may reference immutable builtin:* agents or mixed/flow_only custom agents from that revision. Generate stable 32-character hexadecimal IDs for a new flow and its steps; preserve existing IDs when editing.", "flow", flow_schema()),
        mcp::definition(),
        hook::definition(),
        plugin::definition(),
        project_instructions::definition(),
    ]
}

pub(in crate::agent) fn mcp_definition() -> Value {
    mcp::definition()
}

pub(in crate::agent) fn hook_definition() -> Value {
    hook::definition()
}

pub(in crate::agent) fn plugin_definition() -> Value {
    plugin::definition()
}

pub(in crate::agent) fn project_instructions_definition() -> Value {
    project_instructions::definition()
}

pub(in crate::agent) fn plugins_output(home: &Path) -> Result<Value, AgentError> {
    let mut output =
        serde_json::to_value(crate::plugins::catalog(home)?).map_err(|_| AgentError::internal())?;
    output["instructions"] = json!(include_str!("../skills/builtin/jarvis-plugins.md"));
    Ok(output)
}

pub(in crate::agent) fn hooks_output(state: &AppState, home: &Path) -> Result<Value, AgentError> {
    let mut output = serde_json::to_value(crate::hooks::load(state, home)?)
        .map_err(|_| AgentError::internal())?;
    // The global companion has no project skill reader or filesystem access.
    output["instructions"] = json!(include_str!("../skills/builtin/jarvis-hooks.md"));
    Ok(output)
}

pub(in crate::agent) fn mcp_metadata(
    mcp_state: &crate::mcp::McpState,
    state: &AppState,
    home: &Path,
) -> Result<Value, AgentError> {
    let servers: Vec<_> = mcp_state.list(state, home)?.into_iter().map(|server|
        json!({"id":server.id,"name":server.name,"kind":server.kind,"enabled":server.enabled,"configured":server.configured,"revision":server.revision})
    ).collect();
    Ok(json!(servers))
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
        Target::Publication { .. }
        | Target::Mcp { .. }
        | Target::Hook { .. }
        | Target::Plugin { .. }
        | Target::ProjectInstructions { .. } => false,
    };
    match (action, exists) {
        (Action::Delete | Action::Publish, _) => {
            return Err(invalid("Esta ferramenta somente cria ou edita agentes e fluxos."));
        }
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
            catalog_revision: Some(revision),
            target,
            agent_references,
        },
        mutation,
    ))
}

// Keep the caller-owned runtime services explicit across native/CLI executors.
#[allow(clippy::too_many_arguments)]
pub(super) async fn execute(
    session: &Arc<Session>,
    owner: &Session,
    state: &AppState,
    oauth: &OpenAiCodexState,
    mcp_state: &crate::mcp::McpState,
    home: &Path,
    tool: &ToolCall,
    mut signal: watch::Receiver<bool>,
) -> Result<String, AgentError> {
    let (mut request, mutation) = if tool.name == "jarvis_propose_publication" {
        let question_answered = session
            .data
            .lock()
            .map_err(|_| AgentError::internal())?
            .turns
            .last()
            .is_some_and(|turn| {
                turn.turn
                    .steps
                    .iter()
                    .flat_map(|step| &step.tools)
                    .any(publication::answered_publication_question)
            });
        let (current_user_request, approval_mode) = owner
            .data
            .lock()
            .map_err(|_| AgentError::internal())?
            .turns
            .last()
            .map(|turn| (turn.turn.user.clone(), turn.turn.options.approval_mode))
            .ok_or_else(AgentError::internal)?;
        let confirmation = publication::confirmation_receipt(
            &session
                .data
                .lock()
                .map_err(|_| AgentError::internal())?
                .turns,
            tool,
        );
        let automatic = {
            let owner_options = owner
                .data
                .lock()
                .map_err(|_| AgentError::internal())?
                .turns
                .last()
                .map(|turn| turn.turn.options.clone())
                .ok_or_else(AgentError::internal)?;
            let data = session.data.lock().map_err(|_| AgentError::internal())?;
            data.turns
                .last()
                .filter(|turn| {
                    session.id != owner.id
                        && turn.turn.options.workflow == Some(workflow::Flow::Publication)
                        && turn.turn.options.automatic_publication
                            == owner_options.automatic_publication
                })
                .and_then(|turn| turn.turn.options.automatic_publication.clone())
        };
        let proposal = match publication::prepare_with_confirmation(
            state,
            home,
            owner.project_id()?,
            &session.root,
            &current_user_request,
            question_answered,
            tool,
            confirmation.as_ref(),
            automatic.as_ref(),
        )? {
            publication::PreparedPublication::Ready(proposal) => proposal,
            publication::PreparedPublication::RevisionRequested => {
                return Ok(publication_revision_output(&current_user_request));
            }
        };
        if (automatic.is_some() || tool.args["previewOnly"] != true)
            && (publication::executes_without_review(&proposal)
                || publication::prepares_locally_without_review(
                    &proposal,
                    &current_user_request,
                    approval_mode,
                ))
        {
            if *signal.borrow() {
                return Err(AgentError::cancelled());
            }
            let (turn_id, receipt, cleanup) = {
                let mut data = session.data.lock().map_err(|_| AgentError::internal())?;
                let active = data
                    .active
                    .as_mut()
                    .filter(|active| active.accepts_interaction())
                    .ok_or_else(AgentError::cancelled)?;
                (
                    active.id.clone(),
                    active.claim_interaction(),
                    active.interaction_cleanup_signal(),
                )
            };
            let worker = session.clone();
            let tool_id = tool.id.clone();
            return tauri::async_runtime::spawn_blocking(move || {
                let _receipt = receipt;
                let started = std::time::Instant::now();
                let output = publication::apply_with_cancel(
                    &worker.root,
                    &proposal,
                    None,
                    signal,
                    Some(cleanup),
                );
                worker.record_interaction_result(
                    &turn_id,
                    &tool_id,
                    &output,
                    started.elapsed().as_millis() as u64,
                )?;
                Ok(output)
            })
            .await
            .map_err(|_| AgentError::internal())?;
        }
        (
            PendingProposal {
                turn_id: String::new(),
                tool_id: tool.id.clone(),
                action: Action::Publish,
                summary: proposal.summary.clone(),
                catalog_revision: None,
                target: Target::Publication {
                    after: proposal.clone(),
                },
                agent_references: vec![],
            },
            Mutation::Publication(proposal),
        )
    } else if tool.name == "jarvis_propose_project_instructions" {
        let (request, change) = project_instructions::prepare(&session.root, tool)?;
        (request, Mutation::ProjectInstructions(change))
    } else if tool.name == "jarvis_propose_plugin" {
        let (request, prepared) = plugin::prepare(home, tool).await?;
        (request, Mutation::Plugin(Box::new(prepared)))
    } else if tool.name == "jarvis_propose_hook" {
        let (request, change) = hook::prepare(state, home, tool)?;
        (request, Mutation::Hook(change))
    } else if tool.name == "jarvis_propose_mcp" {
        if sanitize_mcp_args(&tool.name, tool.args.clone()) != tool.args {
            return Err(invalid("A proposta deve conter apenas a configuração pública. Use envKeys/headerKeys para preencher credenciais privadamente no painel de aprovação."));
        }
        let draft: McpRequest = serde_json::from_value(tool.args.clone())
            .map_err(|_| invalid("Proposta de MCP inválida."))?;
        draft.server.validate()?;
        if mcp_state
            .list(state, home)?
            .iter()
            .any(|server| server.name == draft.server.name)
        {
            return Err(invalid("Este MCP já está cadastrado. Consulte jarvis_catalog; esta ferramenta apenas adiciona servidores."));
        }
        (
            PendingProposal {
                turn_id: String::new(),
                tool_id: tool.id.clone(),
                action: Action::Create,
                summary: bounded_summary(draft.summary)?,
                catalog_revision: None,
                target: Target::Mcp {
                    server: draft.server.clone(),
                },
                agent_references: vec![],
            },
            Mutation::Mcp(draft.server),
        )
    } else {
        if tool.name == "jarvis_catalog" && tool.args["view"] == "project_instructions" {
            return Ok(project_instructions::catalog(&session.root)?.to_string());
        }
        if tool.name == "jarvis_catalog" && tool.args["view"] == "plugins" {
            return Ok(plugins_output(home)?.to_string());
        }
        if tool.name == "jarvis_catalog" && tool.args["view"] == "hooks" {
            return Ok(hooks_output(state, home)?.to_string());
        }
        let catalog =
            state.with_connection(home, |db| workflow::catalog::read_configured(db, home))?;
        if tool.name == "jarvis_catalog" {
            let output = catalog_output(&catalog, &tool.args)?;
            if tool.args["view"] == "overview" {
                let mut value: Value =
                    serde_json::from_str(&output).map_err(|_| AgentError::internal())?;
                value["mcpServers"] = mcp_metadata(mcp_state, state, home)?;
                let hooks = crate::hooks::load(state, home)?;
                value["hooks"] = json!({"revision":hooks.revision,"manualCount":hooks.hooks.len(),"detailView":"hooks","nativeMutable":false});
                let plugins = crate::plugins::catalog(home)?;
                value["plugins"] = json!({"revision":plugins.revision,"installedCount":plugins.installed.len(),"detailView":"plugins","authoringTool":"jarvis_propose_plugin","requiresNativeApproval":true});
                return Ok(value.to_string());
            }
            return Ok(output);
        }
        let (request, mutation) = prepare(&catalog, tool)?;
        (request, Mutation::Catalog(mutation))
    };
    if let Target::Agent { after, .. } = &request.target {
        if let Some(model) = &after.model {
            workflow::settings::validate_choice(state, oauth, home, model)?;
        }
    }
    if *signal.borrow() {
        return Err(AgentError::cancelled());
    }
    let (reply, mut received) = oneshot::channel();
    let claimed = Arc::new(AtomicBool::new(false));
    session
        .update_async(|data| {
            if let Some(active) = &mut data.active {
                request.turn_id.clone_from(&active.id);
                active.wait_for_authoring(Pending {
                    request,
                    mutation,
                    started: std::time::Instant::now(),
                    reply,
                    claimed: claimed.clone(),
                    _receipt: None,
                });
            }
        })
        .await?;
    let _human_wait = session.measure(super::telemetry::Phase::HumanWait);
    tokio::select! {
        biased;
        _ = cancelled(&mut signal) => {
            // An accepted decision owns its process cleanup and durable receipt.
            // Keep the turn alive until that receipt arrives, even after cancel.
            if claimed.load(Ordering::Acquire) {
                let _ = (&mut received).await;
            }
            Err(AgentError::cancelled())
        },
        result = &mut received => result.map_err(|_| AgentError::cancelled()),
    }
}

pub(super) fn cancelled_output() -> String {
    json!({"approved":false,"status":"cancelled","note":"A solicitação foi cancelada antes de uma decisão."}).to_string()
}

fn publication_revision_output(note: &str) -> String {
    json!({
        "approved":false,
        "status":"revision_requested",
        "note":note,
        "guidance":"Interpret the current user message in the context of the requested task. Incorporate any observations or scope changes before proceeding; stop if the user withdraws the request. Then submit a revised jarvis_propose_publication proposal with previewOnly=false and confirmedProposalId=null for native review. Preserve explicitly granted autonomy only within its scope. Do not create another conversational preview, mark the task blocked, or ask the user to repeat an exact confirmation phrase."
    }).to_string()
}

fn answer_with(
    session: &Session,
    turn_id: &str,
    tool_id: &str,
    approved: bool,
    note: Option<String>,
    apply: impl FnOnce(
        Mutation,
        Option<u64>,
        Option<&str>,
        (watch::Receiver<bool>, watch::Receiver<bool>),
    ) -> Result<(String, bool), AgentError>,
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
    let active = data
        .active
        .as_mut()
        .filter(|active| active.id == turn_id && active.accepts_interaction())
        .filter(|active| {
            active
                .pending_authoring()
                .is_some_and(|pending| pending.request.tool_id == tool_id)
        })
        .ok_or_else(|| {
            AgentError::new(
                "stale_authoring_proposal",
                "Esta proposta não está mais aguardando aprovação.",
            )
        })?;
    let signal = active.cancel.subscribe();
    let cleanup = active.interaction_cleanup_signal();
    // Claim once before releasing the state lock. A second click cannot apply
    // the same decision while its durable receipt or external effect is pending.
    let mut pending = active.take_authoring().ok_or_else(AgentError::internal)?;
    pending._receipt = Some(active.claim_interaction());
    pending.claimed.store(true, Ordering::Release);
    let mutation = pending.mutation;
    let catalog_revision = pending.request.catalog_revision;
    let elapsed = pending.started.elapsed().as_millis() as u64;
    let publication_revision_requested =
        approved && note.is_some() && matches!(&mutation, Mutation::Publication(_));
    let current = data
        .turns
        .iter_mut()
        .find(|turn| turn.turn.id == turn_id)
        .ok_or_else(AgentError::internal)?;
    current.wire.push(json!({
        "role":"user", "_jarvis_runtime":true, "_jarvis_authoring_decision":true,
        "content":format!("Native proposal decision for call {tool_id}: {}. This records the user's decision, not proof that the operation executed. Preserve this decision during recovery and inspect actual state before repeating an uncertain effect.",
            json!({"approved":approved && !publication_revision_requested,"revisionRequested":publication_revision_requested,"note":note}))
    }));
    session.writer.append_turn(current.clone())?;
    data.sync_timing();
    data.revision = next_revision();
    let snapshot = session.snapshot_data(&data);
    drop(data);
    session.flush()?;
    (session.emit)(snapshot);
    let (output, changed) = if *signal.borrow() || *cleanup.borrow() {
        (cancelled_output(), false)
    } else if publication_revision_requested {
        (
            publication_revision_output(note.as_deref().unwrap_or_default()),
            false,
        )
    } else if approved {
        apply(mutation, catalog_revision, note.as_deref(), (signal, cleanup)).unwrap_or_else(|cause| (
            json!({"approved":true,"status":"failed","error":{"code":cause.code,"message":cause.message}}).to_string(),
            false,
        ))
    } else {
        (
            json!({
                "approved":false,
                "status":"rejected",
                "note":note,
                "catalogRevision":Value::Null,
            })
            .to_string(),
            false,
        )
    };
    let snapshot = session.record_interaction_result(turn_id, tool_id, &output, elapsed)?;
    let _ = pending.reply.send(output);
    Ok((snapshot, changed))
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
        mcp_values,
    } = decision;
    // Invalid/missing private inputs leave the proposal pending for correction.
    // They are never included in the durable decision, snapshot or tool result.
    if approved {
        let data = session.data.lock().map_err(|_| AgentError::internal())?;
        if let Some(pending) = data
            .active
            .as_ref()
            .filter(|active| active.id == turn_id)
            .and_then(|active| active.pending_authoring())
            .filter(|pending| pending.request.tool_id == tool_id)
        {
            if let Mutation::Mcp(draft) = &pending.mutation {
                draft.config(mcp_values.as_ref().unwrap_or(&mcp::Values::default()))?;
            } else if mcp_values.is_some() {
                return Err(invalid("Valores de MCP não pertencem a esta proposta."));
            }
        }
    }
    let (snapshot, changed) = answer_with(
        session,
        &turn_id,
        &tool_id,
        approved,
        note,
        |mutation, catalog_revision, note, (signal, cleanup)| match mutation {
            Mutation::Catalog(mutation) => {
                let revision = catalog_revision.ok_or_else(AgentError::internal)?;
                let revision =
                    workflow::catalog::mutate_configured(state, home, revision, mutation)?.revision;
                Ok((
                    json!({"approved":true,"status":"applied","note":note,"catalogRevision":revision}).to_string(),
                    true,
                ))
            }
            Mutation::Publication(proposal) => Ok((
                publication::apply_with_cancel(
                    &session.root,
                    &proposal,
                    note,
                    signal,
                    Some(cleanup),
                ),
                false,
            )),
            Mutation::Mcp(draft) => {
                let config =
                    draft.config(mcp_values.as_ref().unwrap_or(&mcp::Values::default()))?;
                let server =
                    app.state::<crate::mcp::McpState>()
                        .add(state, home, &draft.name, &config)?;
                let _ = app.emit("mcp-servers:changed", ());
                Ok((json!({"approved":true,"status":"applied","kind":"mcp","server":{"id":server.id,"name":server.name,"kind":server.kind,"enabled":server.enabled,"configured":server.configured},"next":"Use MCP discovery/activation in a project conversation where those tools are available to connect this registered server. Registration does not grant approval for every MCP operation."}).to_string(), false))
            }
            Mutation::Hook(change) => {
                let revision = catalog_revision.ok_or_else(AgentError::internal)?;
                let catalog = hook::apply(state, home, change, revision)?;
                let _ = app.emit("hooks:changed", ());
                Ok((json!({"approved":true,"status":"applied","kind":"hook","hooksRevision":catalog.revision,"next":"This change applies to subsequent project turns. The current turn keeps its frozen hook configuration."}).to_string(), false))
            }
            Mutation::Plugin(prepared) => {
                let catalog = crate::plugins::apply(home, &prepared)?;
                crate::plugins::commands::emit_changed(app);
                Ok((json!({"approved":true,"status":"applied","kind":"plugin","pluginsRevision":catalog.revision,"next":"Inspect view=plugins for the actual components, requirements and authentication status. Newly approved plugin skills and MCP servers become discoverable on the next model step; already pinned versions update on the next turn. Disablement and authorization revocation apply immediately. Installation does not trust hooks; hook configuration and trust changes apply to subsequent turns."}).to_string(), false))
            }
            Mutation::ProjectInstructions(change) => {
                let revision = project_instructions::apply(&session.root, change)?;
                Ok((json!({"approved":true,"status":"applied","kind":"project_instructions","path":"AGENTS.md","revision":revision,"next":"Only the reviewed Jarvis section changed. These project instructions apply to subsequent turns."}).to_string(), false))
            }
        },
    )?;
    if changed {
        let _ = app.emit("workflow-catalog:changed", ());
    }
    Ok(snapshot)
}

#[tauri::command]
pub async fn answer_agent_authoring(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    agent: tauri::State<'_, AgentState>,
    conversation_id: String,
    decision: Decision,
) -> Result<ChatSnapshot, AgentError> {
    let home = app.path().home_dir().map_err(|_| AgentError::storage())?;
    let state = state.inner().clone();
    let agent = agent.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let session = agent.existing(&conversation_id)?;
        answer(&app, &state, &home, &session, decision)
    })
    .await
    .map_err(|_| AgentError::internal())?
}

#[cfg(test)]
mod tests;
