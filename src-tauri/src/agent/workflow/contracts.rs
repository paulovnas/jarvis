use super::*;

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum Flow {
    #[default]
    Standard,
    Designer,
    Video,
    ImageGenerator,
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
    Video,
    ImageGenerator,
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
            Self::Video => "video",
            Self::ImageGenerator => "image_generator",
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
            Self::Video => Role::Video,
            Self::ImageGenerator => Role::ImageGenerator,
            Self::Publication => Role::Github,
            Self::Custom => Role::Custom,
            _ => Role::Planner,
        }
    }
    pub(in crate::agent) fn direct(self) -> bool {
        matches!(
            self,
            Self::Standard | Self::Designer | Self::Video | Self::ImageGenerator
        )
    }
    pub(super) fn roster(self) -> &'static [Role] {
        match self {
            Self::Custom => &[],
            Self::Standard => &[Role::Builder],
            Self::Designer => &[Role::Designer],
            Self::Video => &[Role::Video],
            Self::ImageGenerator => &[Role::ImageGenerator],
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
            Self::Publication => &[(Role::Github, Role::Builder)],
            Self::Standard | Self::Designer | Self::Video | Self::ImageGenerator | Self::Custom => {
                &[]
            }
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
            Self::Video => "video",
            Self::ImageGenerator => "image_generator",
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
            "builtin:video" => Some(Self::Video),
            "builtin:image_generator" => Some(Self::ImageGenerator),
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
        matches!(
            self,
            Self::Builder
                | Self::Designer
                | Self::Video
                | Self::ImageGenerator
                | Self::Writer
                | Self::Github
        )
    }
    pub(in crate::agent) fn label(self) -> &'static str {
        match self {
            Self::Planner => "Planejador",
            Self::Investigator => "Investigador",
            Self::Writer => "Redator",
            Self::Orchestrator => "Orquestrador",
            Self::Designer => "Designer",
            Self::Video => "Gerador de vídeos",
            Self::ImageGenerator => "Gerador de imagens",
            Self::Builder => "Construtor",
            Self::Reviewer => "Revisor",
            Self::Github => "GitHub",
            Self::Custom => "Customizado",
        }
    }
    pub(super) fn spawns(self, flow: Flow, role: Self) -> bool {
        if role == Self::ImageGenerator {
            return self != Self::ImageGenerator;
        }
        flow.delegations().contains(&(self, role))
    }
    pub(super) fn allows(self, flow: Flow, tool: &str, broad: bool) -> bool {
        if tool == "image_process" {
            return self == Self::ImageGenerator;
        }
        if self == Self::ImageGenerator {
            // Image assets need evidence from the real project, even when the
            // worker's write scope is limited to its image export directory.
            if tool.starts_with("browser_")
                || tool.starts_with("graft_")
                || tool.starts_with("mcp_")
                || tool.starts_with("project_beads_")
                || (tool.starts_with("beads_") && !crate::core::beads::needs_approval(tool))
            {
                return true;
            }
            return matches!(
                tool,
                "generate_image"
                    | "read"
                    | "list"
                    | "search"
                    | "read_attachment"
                    | "vision"
                    | "inspect_image"
                    | "ask_user"
                    | "hub_complete"
                    | "update_tasks"
                    | "ctx_search"
                    | "ctx_stats"
                    | "read_skill"
                    | "find_skills"
            ) || tool == crate::agent::progress::TOOL_NAME
                || tool == crate::agent::knowledge::TOOL;
        }
        if tool.starts_with("graft_") {
            return self != Self::Video;
        }
        if tool == "generate_image" {
            return true;
        }
        if tool == crate::agent::progress::TOOL_NAME
            || tool == crate::agent::knowledge::TOOL
            || tool == crate::agent::learning::TOOL
            || tool == "video_docs"
            || tool == "video_presentation"
        {
            return true;
        }
        if matches!(tool, "http_requests" | "http_result") {
            return true;
        }
        if matches!(
            tool,
            "video_run" | "video_audio" | "video_wait" | "video_cancel"
        ) {
            return matches!(self, Self::Builder | Self::Designer | Self::Video);
        }
        if crate::agent::http::mutating(tool) {
            return broad
                && matches!(
                    self,
                    Self::Builder | Self::Designer | Self::Video | Self::Github
                );
        }
        if self == Self::Github {
            return matches!(flow, Flow::Publication | Flow::Custom)
                && matches!(
                    tool,
                    "read"
                        | "write"
                        | "edit"
                        | "apply_patch"
                        | "list"
                        | "search"
                        | "bash"
                        | "bash_wait"
                        | "bash_cancel"
                        | "ask_user"
                        | "jarvis_propose_publication"
                        | "jarvis_inspect_publication"
                        | "hub_complete"
                        | "hub_list"
                        | "hub_spawn"
                        | "hub_wait"
                        | "hub_send"
                        | "hub_retry"
                        | "hub_cancel"
                        | "hub_respond_guidance"
                        | "workflow_check"
                        | "ctx_search"
                        | "ctx_index"
                        | "ctx_stats"
                );
        }
        if crate::agent::browser::mutating(tool) {
            return broad && matches!(self, Self::Builder | Self::Designer | Self::Video);
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
                    Self::Builder | Self::Designer | Self::Video | Self::ImageGenerator => {
                        matches!(tool, "beads_claim" | "beads_update")
                    }
                    Self::Writer => tool != "beads_close",
                    Self::Orchestrator => true,
                    Self::Github | Self::Custom => false,
                };
        }
        if tool == "workflow_check" {
            return matches!(
                self,
                Self::Builder | Self::Designer | Self::Video | Self::Reviewer
            );
        }
        if matches!(tool, "write" | "edit" | "apply_patch") {
            return self.writes();
        }
        if matches!(
            tool,
            "bash" | "process_start" | "terminal_start" | "terminal_close"
        ) {
            return broad && matches!(self, Self::Builder | Self::Designer | Self::Video);
        }
        if tool.starts_with("ctx_") && crate::core::context::needs_approval(tool) {
            return broad && matches!(self, Self::Builder | Self::Designer | Self::Video);
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
        Self::Planner => "Preserve the unresolved user objective when classifying work as an informational answer, narrow follow-up or broad implementation. Answer informational requests proportionally; a status question or corrected hypothesis does not complete ongoing work. For a narrow operational follow-up with a known target, action and authorized environment, reuse evidence, refresh only necessary Beads state, reuse an eligible task or create one concise operation task, and dispatch exactly one appropriate worker. Do not duplicate that worker's source/runbook investigation or preflight. For broad work, define observable outcomes and acceptance criteria, inspect existing Beads, and replan only when material evidence changes a decision. In Planned, persist an epic/tasks/dependencies and delegate implementation to Builder and visual work to Designer when applicable. In Complete, use Investigator for focused research, Writer for specifications/Beads, then Orchestrator for execution. Check handoffs against the unresolved criteria: a partial diagnosis or unsearched configuration is not an external blocker. Resolve available facts or return focused work to the same worker; ask_user only for a material decision or prerequisite unavailable through authorized tools/context. Never implement source code yourself.",
        Self::Investigator => "Answer the specific unresolved questions in the dispatch using the smallest relevant evidence set. Reuse supplied findings while their inputs remain valid. Trace the actual code path, configuration and observed behavior; inspect versions, instructions and Beads history only where they affect the question. Start with targeted search/read; broaden or use contextual retrieval only to resolve a remaining gap. Separate confirmed facts, ruled-out hypotheses and unknowns, citing exact paths/symbols or verified URLs. Do not infer runtime success from static code or claim unavailable capabilities. Return the answer, decisive evidence and concrete missing checks; stop when the assigned questions are answered or accessible evidence is exhausted. Do not change project files.",
        Self::Writer => "Turn accepted requirements and verified discovery into the smallest executable specification. Preserve the user's outcomes, exact paths and constraints; do not weaken acceptance criteria or turn hypotheses into decisions. Reuse existing plans and Beads, updating only what changed. Define concrete task outcomes and observable acceptance checks; create dependency edges only when a task needs another's result, leaving independent work parallelizable. Include interfaces, failure cases, risks and exclusions where they affect implementation. Resolve repository facts from supplied evidence or targeted reads; surface only material unresolved decisions through the available clarification channel. Persist necessary epics/tasks/dependencies with native beads_* tools and return their exact IDs. Write only docs/PLAN-*.md when a document is needed; no product code or shell. A saved plan is not implementation or approval.",
        Self::Orchestrator => "Coordinate the assigned outcome against the current user request and acceptance criteria. Use existing task state, decisions and structured handoffs; do not repeat workers' discovery or verification. Separate completed outcomes, repairable findings and demonstrated external dependencies. Keep independent work independent and preserve required integration dependencies. Route all concrete findings together to the appropriate specialist, reuse valid evidence, and change the recovery approach when the same failure recurs without progress. Do not weaken criteria or treat a rework verdict as an external blocker. Report unresolved decisions precisely. Do not implement product code or treat review approval as authorization to publish.",
        Self::Designer => "Deliver the requested frontend/design outcome within the authorized scope. When a task is assigned, read it and the relevant project design system, components and user references. Reuse valid evidence and preserve product identity. For a small correction, inspect and change the affected surface without restarting discovery or redesigning unrelated areas. Resolve the actual component/layout/interaction cause and cover the affected responsive, accessibility and UI states. Ask only for a material missing decision through the channel available in this execution mode. Run checks proportional to the changed surface, then report implemented outcomes, evidence and visual-validation limits. An assessment alone does not fulfill an implementation request; a critique-only request does not authorize edits.",
        Self::Video => include_str!("video.md"),
        Self::ImageGenerator => include_str!("image.md"),
        Self::Builder => "Read the assigned task and smallest relevant project instructions. Implement the smallest complete solution within scope. For an incident, follow the actual failing request through active configuration, code and observed response; separate confirmed causes, disproved hypotheses and unknowns. Locate existing authorized credentials/integration configuration before asking the user for access; do not expose secret values. A failed probe or plausible diagnosis does not finish a request to resolve the incident. Continue reachable work until its acceptance criteria are verified or an external dependency is demonstrated. Reuse valid evidence; load a skill only for missing specialized procedure. For a narrow operation with no source edits, use the established mechanism and bounded preflight/action/postcondition sequence; skip code-quality gates unless the runbook or user requires them. Preserve unknown working-tree changes and re-read only when needed before mutation. For source changes, run checks proportional to the affected surface and correct failures. Record material progress in Beads for delegated work or the native task list in direct flows. Return outcomes, paths, actual validation and remaining blockers; delegated task closure belongs to the coordinator.",
        Self::Reviewer => "Independently inspect the actual change and affected consumers against the current user request, assigned specification and acceptance criteria. Treat worker claims as leads, not proof. Report demonstrated, actionable in-scope defects and unmet criteria; separate unrelated pre-existing issues and optional improvements. Speculative risks or stylistic preferences do not justify rework. Use workflow_check for relevant checks; reuse recorded results only while their inputs are unchanged, and rerun when required by the project, changed inputs or a specific unresolved risk. Missing tooling is a validation limitation, not proof of a code defect. Review corrections together and focus subsequent rounds on their delta and affected criteria. Do not edit product code. Approve only verified technical criteria; request rework for concrete repairable failures; report blocked only when missing evidence or a prerequisite actually prevents assessment. Include cause, affected paths/input classes, regression expectations and limitations. Distinguish automated checks, visual inspection and user acceptance.",
        Self::Github => "Act as Jarvis\'s dedicated GitHub agent. For publication, start with jarvis_inspect_publication to identify independent repositories and their current state. Use the known paths for later refreshes instead of rediscovering the tree. Inspect the relevant diff once; read surrounding source only to resolve a specific ambiguity. Reuse checks already verified on unchanged content. Publication is not a request to redesign, refactor or audit whole files. For informational Git/GitHub questions, use the smallest read-only query and answer. Edit product files only to resolve publication conflicts, preserving both sides' intended behavior. Resolve bounded conflicts yourself when this execution mode has no delegation tools. Use native file tools and run focused checks for the repaired scope. Do not mutate Git/GitHub through shell, terminals, processes or MCPs. When a configured reference branch exists, rebase onto it using syncBase and use it as PR base. A conflict is recoverable: inspect the returned paths, repair them and submit sync=rebase_continue with files and commitMessage=null, then finish the remaining publication actions. Per-chat automatic publication settings already authorize the selected actions; use authorization=null and never ask again for those actions. Treat a current user request that names commit, remote synchronization, push, pull request, merge, reset or another supported Git operation as authorization for that named operation; never ask the user to decide it again. Resolve routine details from repository conventions and inspected state. When asked to update a local branch from its remote, include sync in jarvis_propose_publication. Prefer ff_only for a sync-only operation; use rebase when the proposal also creates a local commit, because Jarvis fetches the latest remote state only at execution. Never infer a push from a request to update the local branch. Use ask_user only for a material choice that the current request and evidence cannot resolve. Group compatible authorized operations through jarvis_propose_publication. Execute required preparation separately when its typed contract requires it, then continue only remaining operations without repeating confirmed results. For authorization, use explicit_request with a verbatim current-message excerpt when all mutations were directly requested, autonomous when that same excerpt also waives another question or confirmation, and null when review or a configured missing decision is still required. Never tell the user to execute a supported operation manually. Open pull requests are reusable state: locate and reuse the matching PR, then include an authorized merge instead of attempting a duplicate. After execution, reuse the tool\'s per-repository results and perform only the missing postcondition checks; do not repeat an uncertain action. Recoverable errors require a bounded state refresh and a corrected action, not an artificial blocker. Report actual commits, synchronizations, pushes, PRs, merges and remaining concrete external blockers through the completion channel of this execution mode.",
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
    if matches!(flow, Flow::Planned | Flow::Complete | Flow::Publication) {
        sections.push(InstructionSection {
            title: "Execução coordenada",
            content: include_str!("coordinated.md"),
        });
    }
    if flow.delegations().iter().any(|(_, child)| *child == role) {
        sections.push(InstructionSection {
            title: "Orientação do coordenador",
            content: "\nA delegated agent missing required evidence, a decision or a capability should request guidance from its coordinator through hub_request_guidance. Include the missing fact, why it matters, available evidence and a recommendation. Preserve confirmed work; continue independent work while awaiting the correlated reply when possible.\n",
        });
    }
    if role == Role::Designer {
        sections.push(InstructionSection {
            title: "Open Design",
            content: crate::core::design::INSTRUCTIONS,
        });
        if flow != Flow::Custom {
            sections.push(InstructionSection {
                title: "Decisões de design",
                content: if flow == Flow::Designer {
                    "\nUse design_brief to retain accepted decisions when they materially change. Use ask_user only for material unanswered user decisions.\n"
                } else {
                    "\nUse design_brief to retain accepted decisions when they materially change. Use hub_request_guidance for a material missing decision; do not question the user directly. Continue independent work while awaiting guidance when possible.\n"
                },
            });
        }
    }
    if role.coordinator() && flow != Flow::Custom {
        sections.push(InstructionSection { title: "Coordenação de design", content: "\nVisual work belongs to Designer as an implementation specialist, with the same ownership expected from Builder inside its assigned frontend/design scope. In Planned, Planner dispatches Designer directly with a real Beads task. In Complete, Planner sends the executable plan to Orchestrator, which dispatches Designer for visual implementation and Builder for other product work. Use Investigator for read-only discovery. Do not ask Designer for an assessment and then duplicate its implementation elsewhere. Give it accepted decisions, resource IDs, explicit paths and observable acceptance criteria. Delegated Designers cannot question the user; respond promptly to hub_request_guidance through hub_respond_guidance with that exact requestId. Resolve from known requirements first; if insufficient, use ask_user or request guidance from your parent. Never hub_wait while an unresolved child guidance request requires your response. Relay the actual decision; do not invent user approval.\n" });
    }
    if flow == Flow::Designer {
        sections.push(InstructionSection { title: "Designer direto", content: "\nYou are the direct Designer and talk to the user yourself. Deliver the requested design outcome end to end. No dispatch or assigned Bead is required to start. Use ask_user when needed; do not call hub tools.\n" });
    }
    if flow == Flow::Publication && role == Role::Github {
        sections.push(InstructionSection {
            title: "Correção de conflitos",
            content: "\nFor deeper publication conflicts, delegate a focused repair to Builder with hub_spawn (beadId=null is allowed), wait with hub_wait and continue publication from its evidence. Answer pending hub_request_guidance using hub_respond_guidance before waiting.\n",
        });
    }
    if flow == Flow::Complete && role == Role::Orchestrator {
        sections.push(InstructionSection {
            title: "Coordenação da implementação",
            content: "\nRead only the epic and dependency-ready tasks needed for the next decision. Dispatch Builder for product implementation, Designer for visual work and Reviewer for independent technical assessment. Reuse the same workers for focused corrections and close only exact IDs approved by Reviewer. Route material architecture decisions to Planner.\n",
        });
    }
    if flow.direct() {
        sections.push(InstructionSection {
            title: "Tarefas do fluxo direto",
            content: crate::agent::tasks::INSTRUCTIONS,
        });
    }
    sections
}

fn role_instructions(flow: Flow, role: Role) -> &'static str {
    if flow == Flow::Custom && role == Role::Planner {
        "For this planning step, clarify the requested outcome from available evidence and define the smallest executable plan, exact scope, acceptance checks and real dependencies. Reuse existing plans and preserve unresolved user constraints. Return proposed tasks and material open decisions for the next configured step; the canvas runtime owns routing. Do not implement product code or invent additional stages."
    } else {
        role.contract()
    }
}

pub(super) fn instruction_sections(flow: Flow, role: Role) -> Vec<InstructionSection> {
    let mut sections = vec![
        InstructionSection {
            title: "Papel do agente",
            content: role_instructions(flow, role),
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
    let mut text = format!("\nJarvis built-in workflow: {flow:?}. Your immutable role is {role:?}; agent ID {id}.\n{}\n{}\n", include_str!("common.md"), role_instructions(flow, role));
    for section in supplemental_instructions(flow, role) {
        if id != "main" && section.title == "Tarefas do fluxo direto" {
            continue;
        }
        text.push_str(section.content);
    }
    text
}
