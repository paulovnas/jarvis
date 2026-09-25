# Workflow and subagent efficiency in 1.5.1

Tracked by `jarvis-aasw`, following the [Movarte incident](INCIDENT-MOVARTE-LONG-WORKFLOW.md).
The changes apply to the shared Rust harness, including user-defined canvas
workflows. They preserve configured agents, models, permissions and graph routes.

## Evidence behind the changes

The incident took 11h53m06s and created 32 child jobs with 3,775 tool calls.
Beyond the patch, dependency and authentication defects corrected in the incident
report, the lifecycle and context inspection found:

- Two Builders assigned the same Bead and scope within 23 seconds while the first
  was still active. Their instructions differed, so silently dropping the second
  instruction would also have been wrong.
- Nine fresh Reviewers for the same backend Bead and scope, each starting at
  attempt one instead of reusing the prior review context.
- Automatic team snapshots included unrelated siblings and historical runs.
  Stored child turns contained 183 snapshots totaling about 2.22 million
  characters. These are stored payload counts, not billed tokens.
- Each of the Designer's five snapshots listed 44 jobs. Filtering its job list
  to itself, its parent and two declared dependencies reduces that JSON from
  18,187 to 1,388 characters (92.4%). This is an offline projection of one part
  of the input, not an end-to-end speed measurement.
- Canvas correction routes always created a new worker, even when returning to
  the same node. The next worker received shortened summaries without the
  immediate handoff's exact evidence and validation results.

## Runtime changes

### Relevant, replaceable coordination state

The root receives the current run's team. Each child receives itself, its parent,
its direct children and explicitly declared dependencies. Historical dependencies
remain available when explicitly referenced. Full agent history remains available
on demand through `hub_list` and in the UI.

New automatic workflow checkpoints carry an explicit internal marker. Provider
input and compaction replay only the latest marked snapshot. Durable journals,
real user directions, task evidence and tool call/result pairs remain unchanged.
Unmarked legacy messages are not guessed at or deleted.

### Reuse productive workers and reviewers

Native dispatch detects an already-active child assigned to the same Bead, role,
phase and scope. It returns the existing identity and explicitly states that the
new instruction was not scheduled, allowing the coordinator to send the change
to that child rather than launch duplicate work.

Follow-up rounds can reuse the same durable worker and update its dependencies.
Dependency validation rejects self-dependencies and active circular waits while
allowing completed prior-round review evidence. Productive follow-ups do not
consume the consecutive failure-recovery allowance. Repeated actual execution
failures remain bounded; uncertain mutations are not automatically replayed.

The dispatch contract now describes a write scope accurately: scoped workers
can inspect related project files without widening their write ownership to the
whole project. This avoids serializing independent work merely to permit reads.

### Canvas correction rounds retain context

A canvas node reuses its worker within the same run. Separate nodes using the
same agent remain isolated, as do separate runs. Returning to a node appends a
focused follow-up with handoffs since its previous round; the original work,
selected context and confirmed results remain in the worker's durable history.

Fresh steps receive the immediate predecessor's full structured handoff,
including review findings, paths, validation and task IDs. The runtime still
follows the user's configured success/rework routes and step limit. A failed or
cancelled execution is not mistaken for a normal correction route.

### Carry forward the incident fixes

This release also includes the corrected patch parser/context lookup, completed
implementation dependencies before final review closure, admission of review
rework handoffs, existing type-check script aliases and same-account OAuth
renewal during long turns. See the incident report for their evidence and limits.

## Reference decisions

- Codex `codex-rs/core/src/tools/handlers/multi_agents_v2/message_tool.rs`:
  continuation of an existing worker preserves useful context.
- Codex `codex-rs/core/src/tools/handlers/multi_agents_v2/list_agents.rs`:
  concise agent metadata is available on demand.
- Codex `codex-rs/core/src/tools/handlers/multi_agents_spec.rs`:
  delegate bounded independent work and avoid duplicating worker exploration.
- OpenCode `packages/opencode/src/tool/task.ts` reuses a known task/session
  identity for continuation.
- OMP's authentication retry contracts distinguish account renewal from repeated
  inference failures; no automatic account switch was added to Jarvis.
- [OpenAI's subagent guidance](https://learn.chatgpt.com/docs/agent-configuration/subagents#why-subagent-workflows-help):
  keep noisy intermediate work out of the main context, return summaries and
  parallelize independent work while respecting conflicting writes.

The skills' advice to bound failures and isolate context is applied. Fresh-agent
replacement on every correction is deliberately not adopted: the recorded
incident shows repeated discovery, and the independent Reviewer already provides
separation from the implementation agent.

## Validation and limits

Regression cases are included in the existing `bun run eval:harness` suite for
scoped context, bounded checkpoint replay, native lifecycle and canvas history
reuse. They exercise deterministic runtime behavior without contacting production
providers or mutating the Movarte project.

Local validation on macOS passed: frontend lint, TypeScript, production build,
124 Vitest files (686 passed, one skipped), Rust formatting, Clippy with warnings
denied and the full Rust suite (839 passed, 20 ignored), including the generated
IPC contract and new harness regressions. Skipped/ignored tests are not counted
as evidence. The release pipeline separately validates the three desktop targets.

These changes remove specific sources of wasted work. They do not promise a
fixed duration for arbitrary tasks or eliminate upstream provider latency.
Paired live-provider timing remains tracked by `jarvis-8s54`; no percentage of
wall-time or token-cost improvement is asserted from the payload measurements.
