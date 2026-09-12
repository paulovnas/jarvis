use super::*;

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Flow {
    #[default]
    Standard,
    Designer,
    Planned,
    Complete,
    Publication,
    Custom,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Planner,
    Investigator,
    Writer,
    Orchestrator,
    Designer,
    Builder,
    Reviewer,
    Github,
    Custom,
}

impl Flow {
    pub(in crate::agent) fn id(self) -> &'static str {
        match self {
            Self::Standard => "standard",
            Self::Designer => "designer",
            Self::Planned => "planned",
            Self::Complete => "complete",
            Self::Publication => "publication",
            Self::Custom => "custom",
        }
    }
    pub(in crate::agent) fn root(self) -> Role {
        match self {
            Self::Standard => Role::Builder,
            Self::Designer => Role::Designer,
            Self::Publication => Role::Github,
            Self::Custom => Role::Custom,
            _ => Role::Planner,
        }
    }
    pub(in crate::agent) fn direct(self) -> bool {
        matches!(self, Self::Standard | Self::Designer)
    }
    pub(super) fn roster(self) -> &'static [Role] {
        match self {
            Self::Custom => &[],
            Self::Standard => &[Role::Builder],
            Self::Designer => &[Role::Designer],
            Self::Publication => &[Role::Github],
            Self::Planned => &[Role::Planner, Role::Builder, Role::Designer],
            Self::Complete => &[
                Role::Planner,
                Role::Investigator,
                Role::Writer,
                Role::Orchestrator,
                Role::Designer,
                Role::Builder,
                Role::Reviewer,
            ],
        }
    }
    pub(super) fn delegations(self) -> &'static [(Role, Role)] {
        match self {
            Self::Planned => &[
                (Role::Planner, Role::Builder),
                (Role::Planner, Role::Designer),
            ],
            Self::Complete => &[
                (Role::Planner, Role::Investigator),
                (Role::Planner, Role::Writer),
                (Role::Planner, Role::Orchestrator),
                (Role::Orchestrator, Role::Planner),
                (Role::Orchestrator, Role::Designer),
                (Role::Orchestrator, Role::Builder),
                (Role::Orchestrator, Role::Reviewer),
            ],
            Self::Standard | Self::Designer | Self::Publication | Self::Custom => &[],
        }
    }
}
impl Role {
    pub(super) fn id(self) -> &'static str {
        match self {
            Self::Planner => "planner",
            Self::Investigator => "investigator",
            Self::Writer => "writer",
            Self::Orchestrator => "orchestrator",
            Self::Designer => "designer",
            Self::Builder => "builder",
            Self::Reviewer => "reviewer",
            Self::Github => "github",
            Self::Custom => "custom",
        }
    }
    pub(super) fn from_builtin_id(id: &str) -> Option<Self> {
        match id {
            "builtin:planner" => Some(Self::Planner),
            "builtin:investigator" => Some(Self::Investigator),
            "builtin:writer" => Some(Self::Writer),
            "builtin:orchestrator" => Some(Self::Orchestrator),
            "builtin:designer" => Some(Self::Designer),
            "builtin:builder" => Some(Self::Builder),
            "builtin:reviewer" => Some(Self::Reviewer),
            "builtin:github" => Some(Self::Github),
            _ => None,
        }
    }
    pub(super) fn builtin_id(self) -> Option<String> {
        (self != Self::Custom).then(|| format!("builtin:{}", self.id()))
    }
    pub(super) fn coordinator(self) -> bool {
        matches!(self, Self::Planner | Self::Orchestrator)
    }
    pub(super) fn writes(self) -> bool {
        matches!(self, Self::Builder | Self::Designer | Self::Writer)
    }
    pub(in crate::agent) fn label(self) -> &'static str {
        match self {
            Self::Planner => "Planejador",
            Self::Investigator => "Investigador",
            Self::Writer => "Redator",
            Self::Orchestrator => "Orquestrador",
            Self::Designer => "Designer",
            Self::Builder => "Construtor",
            Self::Reviewer => "Revisor",
            Self::Github => "GitHub",
            Self::Custom => "Customizado",
        }
    }
    pub(super) fn spawns(self, flow: Flow, role: Self) -> bool {
        flow.delegations().contains(&(self, role))
    }
    pub(super) fn allows(self, flow: Flow, tool: &str, broad: bool) -> bool {
        if self == Self::Github {
            return flow == Flow::Publication
                && matches!(
                    tool,
                    "read"
                        | "list"
                        | "search"
                        | "bash"
                        | "ask_user"
                        | "jarvis_propose_publication"
                        | "hub_complete"
                        | "hub_list"
                        | "workflow_check"
                        | "ctx_search"
                        | "ctx_index"
                        | "ctx_stats"
                );
        }
        if crate::agent::browser::mutating(tool) {
            return broad && matches!(self, Self::Builder | Self::Designer);
        }
        if tool.starts_with("hub_") {
            return tool != "hub_spawn" || self.coordinator();
        }
        if tool.starts_with("beads_") {
            if flow.direct() {
                return false;
            }
            return !crate::core::beads::needs_approval(tool)
                || match self {
                    Self::Planner => flow == Flow::Planned || tool == "beads_close",
                    Self::Investigator | Self::Reviewer => tool == "beads_update",
                    Self::Builder | Self::Designer => {
                        matches!(tool, "beads_claim" | "beads_update")
                    }
                    Self::Writer => tool != "beads_close",
                    Self::Orchestrator => true,
                    Self::Github | Self::Custom => false,
                };
        }
        if tool == "workflow_check" {
            return matches!(self, Self::Builder | Self::Designer | Self::Reviewer);
        }
        if matches!(tool, "write" | "edit" | "apply_patch") {
            return self.writes();
        }
        if matches!(
            tool,
            "bash" | "process_start" | "terminal_start" | "terminal_close"
        ) {
            return broad && matches!(self, Self::Builder | Self::Designer);
        }
        if tool.starts_with("ctx_") && crate::core::context::needs_approval(tool) {
            return broad && matches!(self, Self::Builder | Self::Designer);
        }
        // Read-only MCP filtering additionally uses server annotations in run_turn.
        if tool.starts_with("mcp_") {
            return broad || !self.writes();
        }
        true
    }
    pub(super) fn contract(self) -> &'static str {
        match self {
        Self::Custom => "Execute only the configured custom workflow step.",
        Self::Planner => "Decompose the user request into observable outcomes and acceptance criteria. Inspect existing Beads before planning to avoid duplicates. Use plan-and-execute and replan from evidence. In Planned, persist an epic/tasks/dependencies and delegate implementation to Builder, then visual work to Designer when applicable; independently check each outcome. In Complete, delegate focused research to Investigator, specifications and Beads creation to Writer, validate their handoffs, then dispatch Orchestrator with the resulting epic and outcomes. Never implement source code yourself. Use ask_user only for unresolved material user decisions. For simple informational requests answer proportionally; do not invent implementation work.",
        Self::Investigator => "Investigate the actual code path, versions, project instructions and Beads history. Refine queries, combine lexical searches with available contextual retrieval, rank evidence by relevance and compress returned excerpts. Cite paths/symbols or verified URLs; distinguish confirmed facts, hypotheses and unknowns. Do not claim vector search or visual inspection unless a real available tool performed it. Return a focused discovery handoff; do not change project files.",
        Self::Writer => "Turn the approved decisions into an executable specification: outcomes, current/desired behavior, scope exclusions, risks, task descriptions, objective acceptance checks and dependency edges. Persist epics/tasks/dependencies with native beads_* tools and return their exact IDs. Consult existing items before creating duplicates. You may write only docs/PLAN-*.md documents; no product code or shell. Do not invent architectural decisions missing from the Planner's dispatch.",
        Self::Orchestrator => "Use Beads as task-state authority. Read the epic and tasks, route dependency-ready work to Builder, Designer for visual changes and Reviewer for independent final assessment. hub_spawn dependencies form an execution DAG; use non-overlapping explicit paths for parallel writers and '.' only for whole-project commands. Wait through hub_wait; never poll logs. Check each structured handoff against the acceptance criteria. Require Reviewer approval before closing implementation Beads in Complete. Findings require focused rework and another review; at most two rework rounds, then report a concrete blocker. Route architecture corrections to Planner. Never implement code yourself or equate technical approval with commit/deploy authorization.",
        Self::Designer => "Read the assigned Bead, existing design system, tokens, components, states and applicable skills. Preserve product identity. Implement only assigned visual scope, critique hierarchy, spacing, typography, accessibility, responsiveness and interaction. Inspect screenshots/reference evidence with available tools; distinguish actual visual validation from static review. Re-read shared files before edits. Run applicable checks; return evidence, outcomes and honest visual-validation limits.",
        Self::Builder => "Read the assigned task and relevant project instructions before editing. Implement the smallest complete solution within the dispatch scope. Preserve unknown working-tree changes and re-read files before mutation. Use tools/MCP/skills when relevant, execute required lint/tests/build, inspect failures and correct your work. In delegated workflows, record progress and discovered work in Beads; direct flows use their native task list instead. Return implementation outcomes, paths and actual validation evidence. In delegated workflows the coordinator owns final task closure.",
        Self::Reviewer => "Independently inspect the implemented files against the original Bead and acceptance rubric. Do not trust the implementation handoff as proof. Look for regressions, edge cases, unmet outcomes and unsafe assumptions; use workflow_check for available project checks. Do not modify product code. Return an approved/rework/blocked verdict with concrete findings, file references, validation results and limitations. Distinguish automated checks, visual review and user acceptance.",
        Self::Github => "Act as Jarvis's dedicated GitHub publication agent. Inspect every Git repository with changed files under the project root, including independent nested repositories. Do not edit product files or publish through shell, terminals, processes or MCPs. Use read-only Git commands and the smallest relevant project checks, follow the project publication and pull-request instructions, ask the configured pull-request question when required, and submit one complete multi-repository proposal through jarvis_propose_publication. After the user decides, verify the resulting Git and GitHub state with read-only commands and finish with hub_complete. Never infer permission for a commit, push, pull request or merge from prior conversation activity.",
    }
    }
}

#[derive(Serialize)]
pub struct InstructionSection {
    pub title: &'static str,
    pub content: &'static str,
}

fn supplemental_instructions(flow: Flow, role: Role) -> Vec<InstructionSection> {
    let mut sections = vec![];
    if role == Role::Designer {
        sections.push(InstructionSection {
            title: "Open Design",
            content: crate::core::design::INSTRUCTIONS,
        });
    }
    if role.coordinator() {
        sections.push(InstructionSection { title: "Coordenação de design", content: "\nVisual work belongs to Designer as an implementation specialist, with the same ownership expected from Builder inside its assigned frontend/design scope. In Planned, Planner dispatches Designer directly with a real Beads task. In Complete, Planner sends the executable plan to Orchestrator, which dispatches Designer for visual implementation and Builder for other product work. Use Investigator for read-only discovery. Do not ask Designer for an assessment and then duplicate its implementation elsewhere. Give it accepted decisions, resource IDs, explicit paths and observable acceptance criteria. Delegated Designers cannot question the user; respond promptly to hub_request_guidance through hub_respond_guidance with that exact requestId. Resolve from known requirements first; if insufficient, use ask_user or request guidance from your parent. Never hub_wait while an unresolved child guidance request requires your response. Relay the actual decision; do not invent user approval.\n" });
    }
    if flow == Flow::Designer {
        sections.push(InstructionSection { title: "Designer direto", content: "\nYou are the direct Designer and talk to the user yourself. Deliver the requested design outcome end to end. No dispatch or assigned Bead is required to start. Use ask_user when needed; do not call hub tools.\n" });
    }
    if flow.direct() {
        sections.push(InstructionSection {
            title: "Tarefas do fluxo direto",
            content: crate::agent::tasks::INSTRUCTIONS,
        });
    }
    sections
}

pub(super) fn instruction_sections(flow: Flow, role: Role) -> Vec<InstructionSection> {
    let mut sections = vec![
        InstructionSection {
            title: "Papel do agente",
            content: role.contract(),
        },
        InstructionSection {
            title: "Diretrizes comuns",
            content: include_str!("common.md"),
        },
    ];
    sections.extend(supplemental_instructions(flow, role));
    sections
}

pub(super) fn prompt(flow: Flow, role: Role, id: &str) -> String {
    let mut text = format!("\nJarvis built-in workflow: {flow:?}. Your immutable role is {role:?}; agent ID {id}.\n{}\n{}\n", include_str!("common.md"), role.contract());
    for section in supplemental_instructions(flow, role) {
        text.push_str(section.content);
    }
    text
}
