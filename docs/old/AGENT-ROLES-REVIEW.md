# Native agent instructions and efficiency review

Reviewed on 2026-09-27 against release 1.7.1 and the local Planner/Builder fixes
documented in [the incident review](AGENT-INVESTIGATION-REVIEW.md). This review
covers Investigator, Writer, Orchestrator, Designer, Reviewer and Github, plus
their shared instructions and native, direct and custom-canvas invocation modes.

## Confirmed problems

Specialized roles were receiving the same full coordination contract even when
the corresponding tools were unavailable. Canvas agents cannot spawn arbitrary
workers or request native parent guidance: the saved graph routes them, and its
steps use `ask_user` for material unanswered user decisions and `hub_complete`
for results. Designer additionally received unconditional instructions to save
`design_brief`, unavailable in canvas steps. Direct Github received delegation
instructions despite direct chats excluding hub tools.

The fix separates common behavioral rules from the native coordination
supplement. Canvas/direct prompts retain specialization, authorization and scope
without demanding unavailable orchestration. Native flows retain their existing
delegation, independent review, exact Beads approval IDs and optional final
manual acceptance. No tool permissions were broadened to accommodate prompts.

Native Github already supports answering Builder guidance. That permission was
checked and retained; an initial audit suspicion about it was incorrect.
Its publication job no longer adds a contradictory requirement to inspect every
repository or always submit one supervised proposal. Job criteria respect the
requested repositories, compatible operations and current authorization policy.
Github publication uses direct user questions for material missing decisions;
only workers with a parent-guidance tool receive instructions to request it.

## Changes by role

| Role | Instruction problem | Resulting contract |
| --- | --- | --- |
| Investigator | A list of search techniques without a clear research question or completion boundary could encourage unnecessary exploration. | Answer assigned questions with the smallest relevant evidence set; reuse verified facts, distinguish hypotheses, and stop when those questions are answered or accessible evidence is exhausted. Research completion does not mean the wider implementation is complete. |
| Writer | A fixed list of specification sections and task artifacts could expand a small follow-up or duplicate a plan. | Update existing plans and Beads; preserve acceptance criteria; create only actual dependencies and the detail needed to implement. Resolve repository facts before asking about material decisions. A saved plan is not implementation. |
| Orchestrator | The native routing contract contradicted deterministic canvas routing. | Keep evidence triage and dependency ownership as the specialization; expose native dispatch/review/closure instructions only in native coordinated flows. Reuse workers, deliver related findings together and preserve useful results. |
| Designer | Mandatory assigned-Bead reading, a fresh brief before every implementation and universal state coverage could turn a small correction into another discovery/design exercise. | Read an assigned task when one exists, preserve identity, fix the affected surface and its relevant states, and update accepted decisions only when they change. Clarification and checkpoints follow available tools. |
| Reviewer | Verdict meanings and initial check reuse were underspecified; optional concerns could become rework. | Independently inspect actual changes and affected consumers. Rework requires a demonstrated repairable failure; a missing tool is a limitation unless it prevents required assessment. Reuse valid check results, rerunning for changed inputs, required gates or specific uncertainty. |
| Github | Demanding one proposal for every operation contradicted typed constraints such as mutually exclusive reset/sync; delegation language also leaked into direct/canvas modes. | Group compatible authorized operations, perform required preparation separately and continue only remaining work. Reconcile uncertain results and reuse existing PRs. Native publication can delegate conflict repair; direct/canvas invocation resolves bounded conflicts within its own capabilities. |

Planner and Builder retain the preceding incident corrections. Planner's canvas
variant produces the assigned planning outcome instead of inventing native
delegation stages. Custom user-written instructions remain intact; the wrapper
now describes the actual routing and clarification channels. Direct chats no
longer receive the canvas manual-validation instruction.

## Context efficiency

The shared instruction file decreased from **9,934 to 3,983 UTF-8 bytes** compared
with committed 1.7.1. Direct/canvas prompts avoid the additional **3,640-byte**
native coordination supplement; native coordinated prompts include it. These
figures describe only these instruction blocks, not total request tokens or
measured latency. The repeated native task-list instruction in Standard/Designer
system prompts is also emitted only once.

The changes add no model calls, evaluator agents, retries, state machines or
dependencies. Existing tool policy and runtime validation remain authoritative.

## Reference decisions

- Codex `core/src/agent/role.rs`: focused explorer questions, explicit worker
  ownership and reusing existing agents/evidence.
- Codex `collaboration-mode-templates/templates/plan.md`: discover repository
  facts before asking; produce a plan sufficient to implement.
- Codex `prompts/templates/review/rubric.md`: actionable, demonstrated findings,
  distinguish pre-existing issues and avoid speculative defects.
- OpenCode `packages/opencode/src/agent/prompt/explore.txt` and
  `session/prompt/codex.txt`: proportionate exploration and preservation of the
  existing design system.
- OMP `packages/coding-agent/src/prompts/system/system-prompt.md`: real
  dependencies, observable evidence and complete requested outcomes.
- [OpenAI Codex prompting guide](https://developers.openai.com/cookbook/examples/gpt-5/codex_prompting_guide):
  action-oriented autonomy, focused tool use and evaluation of prompt changes.
- [Anthropic: Building effective agents](https://www.anthropic.com/engineering/building-effective-agents):
  simple composable workflows, clear tool contracts, and the latency/cost tradeoff
  of unnecessary orchestration.

Reference contracts were adapted, not copied. Their differing browser/approval
policies do not replace Jarvis's user-owned final UI acceptance or authorization.

## Validation boundary

Regression coverage checks emitted instructions against the actual tools of all
eight native roles in a canvas, native/direct mode separation, existing scope
restrictions, role-specific completion rules, and single emission of task-list
instructions. Six isolated scenarios were added to the existing whole-task
evaluation corpus for subsequent paired model runs, one for each reviewed role.

Prompt-contract tests validate what Jarvis sends and permits. They do not prove
that a model always makes the right decision or establish a percentage gain in
task success or duration. Real-provider paired evaluation and native application
acceptance remain separate from automated checks.

Completed automated validation:

- `bun run check`: lint, TypeScript, 773 tests passed (one skipped), production
  build and generated IPC contract validation passed.
- `cargo clippy --all-targets -- -D warnings`: passed without diagnostics.
- `cargo test`: 1,032 passed, 24 ignored, no failures in the final run.
- `cargo fmt --check` and `git diff --check`: passed.

Existing assertions tied to the previous Designer sentence were updated to its
new scoped implementation contract. No running user chat, application instance,
provider setting or customer project was changed during this review.
