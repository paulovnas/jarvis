# Planner and Builder investigation review

Reviewed against Jarvis 1.7.1 on 2026-09-27. The source was an actual multi-day
integration conversation, inspected read-only. Credentials, HTTP payloads,
customer identifiers and business data are intentionally excluded.

## What the evidence supports

The conversation contains 42 root turns and 11 persisted worker jobs. It includes
several distinct requests: an earlier quotation was successfully sent before a
different quotation exposed the remaining authorization failure. It would be
incorrect to describe all three days as one unchanged task with no progress.

Earlier turns encountered provider request failures, oversized compaction input
and failed worker resumptions. Those failures contributed to the experience, but
the final premature response had no provider error and retained the relevant
integration context. Increasing reasoning effort alone does not address it.

In the final investigation, a worker confirmed a valid token and business API
responses with HTTP 403. The coordinator requested VM access. After the user
challenged that request, the direct agent used already available configuration
and the identity endpoint to obtain additional evidence. This proves that a
useful authorized diagnostic step remained before asking for access; it does not
prove that VM access could never be needed.

The task list also changed its criterion from determining the cause and next
action to consolidating authentication evidence and explaining the remaining
gap. It marked that weaker criterion complete while the answer acknowledged
that the requested operation remained unresolved. Runtime completion was not
task success. The cause of the third-party HTTP 403 was not established by this
review; investigating or changing that integration is outside this Jarvis fix.

## Confirmed defects and changes

| Boundary | Defect | Change |
| --- | --- | --- |
| Shared and role prompts | Efficiency rules emphasized the latest request, bounded discovery and stopping, without clearly preserving an unresolved outcome through corrective messages. | Preserve the ongoing objective, respect explicit scope changes, distinguish facts from hypotheses and verify the requested result before completion. |
| Diagnostic access | A worker's restricted tool set could be treated as missing user access; existing configuration and integrations were not consistently investigated before asking. | Inspect available access without exposing secrets; distinguish worker capability limits from unavailable project access and escalate through the coordinator. |
| Direct task completion | The final reminder instructed the model to leave no unfinished items, encouraging bookkeeping closure. | Continue actionable work; do not rename, remove or weaken unmet criteria to finish a turn. An unknown cause is not by itself an external blocker. |
| Worker guidance | Builder was instructed to request native guidance, but that tool was exposed only to Designer and coordinators. | Expose the existing correlated parent-guidance mechanism to delegated workers, retaining parent-only responses and cancellation. |
| Blocked handoff | A blocked Bead prevented a worker from reporting a blocked result, leading it to reopen and claim the task just to deliver its report. | Accept a blocked report for a blocked task without task mutation; continue rejecting successful completion and new implementation against a blocked task. |
| New worker context | A new worker received only the latest root message, labeled as the original request. Auxiliary directions and recent findings were omitted. | Pass bounded historical context plus the current user directions, distinguish steering from assignment and preserve supersession/cancellation. |
| Dependency context | Most dependency handoffs were reduced to a 300-character summary, losing evidence, checks and limitations. | Deliver the existing structured handoff to explicitly dependent workers; unrelated agents retain compact checkpoints. |

The last two defects are reproducible structural gaps. The incident does not
establish them as the cause of the final agent's decision: its relevant context
was still available. No claim is made that these changes guarantee every future
model decision or eliminate all provider failures.

Persistence is bounded by useful available evidence: when relevant checks are
exhausted, agents should report an inconclusive result and the exact missing
evidence. They must neither repeat probes indefinitely nor claim a repair.
Implementation remains conditional on authorization; diagnosis-only and Plan
requests keep their existing limits.

## Reference decisions

- Codex `codex-rs/core/gpt_5_2_prompt.md`: carry an action request through its
  actual outcome; do not confuse a plan or intermediate finding with completion.
- Codex `codex-rs/core/src/tools/handlers/multi_agents_spec.rs` and native agent
  communication: isolated workers need explicit context and usable communication
  with their parent. Jarvis retains its existing correlated guidance mechanism.
- OpenCode `packages/opencode/src/session/prompt/gpt.txt` and `codex.txt`: inspect
  available evidence before asking for missing input; continue authorized work.
- OMP `packages/coding-agent/src/prompts/system/system-prompt.md` and
  `mid-run-todo-nudge.md`: continue unfinished work and distinguish an untested
  possibility from an actual external dependency.

The implementation adapts these contracts. It does not introduce an LLM judge,
automatic forced continuation based on answer keywords, a second task system or
additional providers. Historical context does not renew publication authority.

## Validation boundary

Regression tests cover prompt assembly, unchanged Plan/approval restrictions,
worker guidance delivery/cancellation, blocked-versus-successful handoff
validation, context after worker reload, current user corrections, explicit
historical truncation and full dependency evidence reaching only its consumers.

The `movarte-investigation-followup` entry in the existing whole-task evaluation
corpus defines observable criteria for paired real-model runs using isolated
data. Unit and contract tests verify the harness; they are not a measured
improvement in model success rate. A live before/after comparison and functional
validation in the application remain separate from automated quality gates.

Automated checks completed for this change:

- `bun run check`: lint, typecheck, 770 frontend tests passed (one skipped),
  production build and the IPC contract check passed.
- `cargo clippy --all-targets -- -D warnings`: passed without diagnostics.
- `cargo test`: 1,026 passed, 24 ignored; formatting and diff checks passed.

The first complete Rust run had one intermittent failure in the existing
`beads_busy_cleanup_recovers_after_committed_project_deletion` test. Both the
isolated retry and the next complete run passed without source changes. The
underlying OS error is hidden by the generic cleanup error; concurrent inherited
file locks are a hypothesis, not a demonstrated cause. No library/cleanup code
was changed as part of this investigation.
