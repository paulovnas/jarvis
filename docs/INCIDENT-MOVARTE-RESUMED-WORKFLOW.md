# Movarte resumed workflow: latency and finalization failure

Date: 2026-09-25. Diagnostic task: `jarvis-mere`. The diagnostic evidence below describes the incident; the final section records the subsequent implementation.

## Finding

The resumed run performed substantial implementation and obtained independent technical approval, but its execution did **not** finish correctly. A reproducible Jarvis completion-contract defect rejected the orchestrator's final handoff after it closed the approved epic. A subsequent provider transport interruption and an overly coarse recovery counter left the worker marked failed while the main chat reported completion.

The additional 1h27m06s cannot be described as an unavoidable or healthy completion time. Some of it addressed real implementation defects; the records also expose repeated review/correction rounds, large reused context, redundant coordination and avoidable recovery failures. No reliable minute-level speed target can be inferred without measuring a corrected run.

## Evidence and scope

- Project: Movarte, workspace eNe; conversation: “Correções na integração Conta Azul”.
- Conversation ID: `0935840f6fcd1d03b755d749921bf66d`.
- Resumed root turn: `948599e7783b7766249d9e1c6a8db1c1`.
- Screenshot version: 1.5.1. The completion, recovery-counter and incremental-transport paths inspected remain present in the 1.5.2 checkout.
- Sources: root session journal, five relevant worker journals and workflow manifest under the local Jarvis data directory. Finished turns were read from their latest complete checkpoints; relevant journal tails end in final checkpoints.
- Only worker turns created during this root turn were counted. Lifetime attempt counters include earlier work and are not counts of this continuation.
- The audit did not alter Movarte files, its Beads, journals or running state. It did not rerun Movarte tests, access a database, call Conta Azul or perform functional acceptance.
- Sanitized diagnostics end before this run. They do not establish the underlying network/socket cause of either transport interruption.

## Where the time went

All times below are local, UTC−03:00, on September 25.

| Interval | Recorded work |
| --- | --- |
| 09:33:43–09:39:18 | State reconstruction and orchestrator restart. Its first attempt reused an old access-denied conclusion without making a fresh Designer attempt; the parent corrected this. |
| 09:39:18–09:50:17 | Designer completes the missing frontend; two test failures are corrected. |
| 09:51:08–09:57:47 | Integrated reviewer finds a diagnostic-data leak and inconsistent backend status for standalone services. |
| 09:57:47–10:20:55 | Correction routing: premature Designer admission fails on an open Beads dependency; an obsolete worker ID is corrected; backend repair suffers a transport interruption, resumes and receives a separate backend review. |
| 10:20:55–10:26:57 | Frontend correction, including a TypeScript regex-target compatibility error. |
| 10:27:55–10:30:27 | Review finds personal names still exposed through free text. |
| 10:31:36–10:37:37 | Designer replaces free-text diagnostics with controlled messages. |
| 10:39:15–10:43:09 | Review finds unrecognized diagnostic codes still printed verbatim. |
| 10:43:52–10:49:06 | Designer fixes catalog validation and the UI/copy consumers. |
| 10:49:36–10:53:06 | Integrated review approves the final result. |
| 10:53:06–11:00:49 | Task closure, rejected final handoff, guidance, transport interruption, denied recovery and main-chat summary. |

Root elapsed time: **5,225,732 ms (1h27m05.7s)**.

The union of implementation/reviewer activity is **58m04.1s**: about 28m18s Designer, 11m51s Builder and 17m54s reviewers. These worker spans did not overlap. The other **29m01.6s** had no implementation/reviewer worker active; this includes coordinator inference, routing, inspections and recovery. It is not all idle time and cannot all be promised as removable overhead.

There were **5 reused worker identities, 14 worker turns, 498 worker tool calls and 9 worker tool errors**. The parent added 18 calls and one retry error: **516 calls and 10 tool errors** in this continuation. The dependency-admission failure and provider interruptions are additional runtime failures, not tool-call errors.

The 40 `workflow_check` calls total **9m16.9s** of recorded tool duration. The tests/builds therefore do not explain the entire 87 minutes. Parent and orchestrator `hub_wait` durations overlap child execution and must never be added to it.

The prior run created 32 worker identities; this continuation reused five. That supports the effectiveness of worker reuse, but these are different work segments, not a controlled before/after speed benchmark.

## The finalization failure, in order

1. The integrated reviewer returned `approved`, explicitly including the reviewed implementation, review task and epic IDs.
2. The orchestrator closed the relevant child tasks and then its own epic at **10:54:32**. The epic remained closed with the review evidence.
3. It submitted `hub_complete` with a structured handoff. Jarvis returned: **“A tarefa do Beads não está disponível para execução.”**
4. It inspected the closed state and asked its parent for guidance. The parent proposed reopening only the epic administratively, submitting the handoff and then closing it again.
5. Before that recovery happened, the orchestrator ended at **10:57:47** with `provider_transport_interrupted`. Its next recorded step has no text/tools and zero duration. No reopening is recorded.
6. The parent's next `hub_retry` was rejected: **“Duas retomadas consecutivas falharam.”**
7. The main turn completed at **11:00:49**, with the independently approved work and closed Beads preserved, but the orchestrator still failed and without an accepted final handoff.

This is not evidence of lost source edits or a Rust panic. It is an inconsistent workflow completion state followed by a transport failure.

### Confirmed contract defect

`src-tauri/src/agent/workflow/dispatch.rs:623` calls `check_bead` before accepting completion. That function calls `validate_bead` at line 894. At lines 928–930, the validator accepts only `open` and `in_progress`; a closed task is rejected before the final handoff is stored at line 650.

The same validator serves **admission to work** and **registration of finished work**, which require different conditions. The native workflow permits technically approved closure when final manual validation is disabled. Closing the approved epic therefore makes the next legitimate completion operation fail.

The correction should separate these validations, retain ownership/review/child-settlement checks and make handoff registration idempotent. Reopening a correctly completed epic should not be part of normal finalization.

### Confirmed recovery-counter defect

`prepare_retry`, in `workflow/dispatch.rs:512–572`, counts recoveries across an entire worker run. It resets for a completed worker or a reviewer `rework` handoff, but does not recognize intervening material progress inside a subsequently failed turn.

The orchestrator retained `recoveryAttempts: 2` after the initial stale blocker and the following **79m21.7s productive turn**. The new transport interruption was consequently treated as another attempt in the same failed sequence. The rejection occurred even though children completed, review approved and Beads closed.

Recovery limits should apply to repeated failure without new evidence, with explicit failure identity and verified progress. This does not justify unlimited retries or replaying uncertain mutations.

### Transport cause: known class, missing detail

There were two `provider_transport_interrupted` failures: one during backend repair and one during orchestrator finalization. The builder recovered; the orchestrator could not because of the counter above.

`provider/incremental.rs:225–238` maps a 120-second receive timeout, EOF, socket errors and close frames to the same error. Other send/flush failure paths also lose detail. The journal cannot establish whether the last failure was an idle connection closure, a network interruption or another socket error. The empty zero-duration final step is consistent with an immediate failure on reuse, but does not prove its cause.

The runtime should preserve sanitized phase/cause/close metadata and recover the inference request from durable receipts where safe. Never retry an uncertain side effect merely because its provider connection failed.

## Was the additional review work warranted?

The recorded findings were concrete, not arbitrary style requests:

1. The first integrated review found short/nested structured secrets surviving the diagnostic sanitizer, plus a backend status endpoint that disagreed with preflight about standalone services.
2. The second review found personal names still present in free-text diagnostics.
3. The third found unrecognized error codes accepted by a character-pattern check and displayed/copied verbatim.
4. The final review verified catalog membership, controlled fallback messages and the UI/copy paths, then approved.

The weak point was the **piecemeal repair of the same privacy boundary** across three corrections. A complete correction contract should have covered free text, structured payloads, unknown codes and both display/copy consumers in the first pass. Removing review would hide these defects; improving the finding and repair contract can avoid repeated full cycles.

The final reviewer recorded successful frontend tests (35 suites/190 tests), typecheck, lint and build; lint/build still reported pre-existing warnings. Backend review recorded 45 files/401 tests, with earlier build/lint evidence reused for unchanged content. These are recorded results, not tests rerun by this audit. They support technical approval, not proof of real Conta Azul behavior, database concurrency, deployment or visual acceptance.

## Efficiency findings from the incident

**Reused context needs better hygiene.** Designer provider usage grew from **207,862 to 345,352 input tokens per step** across this continuation. Its seven saved turns contain approximately 1.46 MiB of wire items, including full-file writes, large edit arguments and repeated task snapshots. No Designer compaction event appears in that journal. Summed per-step token counts include repeated context and must not be described as unique tokens or inferred financial cost.

Reuse is still preferable to repeatedly rediscovering the task. The improvement is to retain the original request, current state and validated evidence while replacing stale snapshots and shortening obsolete tool receipts. Use existing context machinery and preserve the configured **80% automatic-compaction threshold**. These observations establish excess input, not the exact latency saved by reducing it.

**Re-review should follow changed evidence.** This run made 156 reads, 88 searches, 63 Beads reads, 27 repository inspections and 16 process-list calls in workers. Counts alone do not establish waste. Together with repeated instructions to revisit all original criteria after small corrections, however, they justify a focused evaluation of targeted re-review and reuse of checks on unchanged content.

**Dependency admission still requires exact prerequisites.** The first frontend rework was queued after the backend Bead reopened, with only the reviewer declared as a dependency. The worker then failed before doing any work. The correction retains the existing exact Bead-ID checks and the distinction between completed implementation and final closure. Independent frontend/backend corrections may run concurrently when their actual contracts and scopes permit; independence must not be inferred from filenames alone. The evidence does not justify bypassing a pending backend correction or removing the final independent review.

## Reference comparison

- **Codex:** `core/src/session/turn.rs:1561–1645` recreates retry input from authoritative session history and attaches executed-tool receipts. `core/src/responses_retry.rs:51–143` scopes reconnection to a sampling operation, applies backoff and supports transport fallback. This directly informs safe recovery without discarding confirmed work.
- **Codex agents:** `core/src/tools/handlers/multi_agents_v2/message_tool.rs` distinguishes sending a message from triggering a follow-up turn in the existing worker. Productive follow-up and failure recovery should remain different operations even when Jarvis exposes a shared public tool.
- **OpenCode:** `packages/opencode/src/tool/task.ts` reuses a requested task session and returns distinct completed/error/cancelled outcomes. The parent should retain the real child outcome rather than letting a successful final message conceal incomplete runtime finalization.
- **OMP:** `packages/ai/src/auth-retry.ts:139–183` tracks retries against a concrete logical operation and attempted credentials. Its authentication rotation policy is not a transport fix to copy; the useful principle is typed, bounded recovery state instead of one worker-lifetime counter.

## Implemented corrections

| Priority | Bead | Deliverable |
| --- | --- | --- |
| 1 | `jarvis-j2yj` | Separate completion from admission. Accept the approved, owned closed Bead with the exact task ID, settled children and required manual acceptance. Repeated completion returns the same accepted handoff. |
| 2 | `jarvis-utmf` | Reset recovery attempts after a durably recorded file mutation or a successfully completed child from the same run. Reads, elapsed time, failed children and old-run results do not reset the budget. |
| 3 | `jarvis-a9f5` | Fall back from an interrupted Responses WebSocket to HTTP replay with existing bounded inference retries, when all tools execute locally. Retain prior receipts, reset provisional text, and preserve sanitized phase/cause/close-code diagnostics. |
| 4 | `jarvis-ekn0` | Project settled worker turns into historical receipts, shorten obsolete local file payloads, and keep the latest Beads snapshot. Preserve user instructions, handoff evidence, current signed envelopes and every uncertain side effect. Raw journals and the 80% threshold remain unchanged. |
| 5 | `jarvis-bx9p` | Deliver the full dependency review handoff directly to the repairing worker; preserve all related input classes and consumers. Focus re-review on changed evidence and remove the contradictory two-rework-round instruction. |

### Measured replay reduction

A read-only diagnostic copied the actual seven-turn Designer journal into a temporary fixture and simulated the next worker continuation through the production replay code. No model request was sent and the original journal was not modified.

| Replay measure | Original replay | Projected replay | Reduction |
| --- | ---: | ---: | ---: |
| Serialized context bytes | 1,535,454 | 890,809 | 42.0% |
| Local estimated context tokens | 343,699 | 231,488 | 32.6% |

These are context measurements for a simulated next continuation, not provider-billed tokens or measured end-to-end latency. The conservative projection preserves entire turns containing uncertain effects; it does not claim to remove every obsolete byte. Regression tests also cover multiple correction rounds, unchanged current envelopes, full finding delivery, missing receipts, structured results and the measurement boundary between old and new turns.

The sanitized `movarte-resumed-review.json` fixture covers short/nested structured content, free-text personal data, unknown codes and both UI/copy consumers. It is used by deterministic context tests and registered in the whole-task evaluation corpus for a future live comparison. The deterministic tests verify delivery of the complete contract and retention of independent approval, not a guarantee that every model will repair it in one attempt.

These corrections are local changes after 1.5.2. Publication and a real long-running workflow validation are separate from code checks; neither is claimed here.

### Validation

- `bun run check`: lint and TypeScript passed; 126 frontend suites, 703 tests passed and one existing skipped test; production build and generated IPC contract check passed.
- `cargo clippy --all-targets -- -D warnings`: passed without warnings.
- `cargo test`: 852 tests passed, 21 opt-in tests ignored, no failures. Live-provider/environment-specific tests were not enabled.
- `cargo fmt --check` and `git diff --check`: passed.
- The opt-in local replay measurement above passed separately, using a temporary copy of the selected Designer journal and no provider calls.

The remaining validation is an end-to-end run with the user's actual provider and workflow. The current evidence establishes repaired runtime contracts and reduced input, not a promised completion time or live functional acceptance of Movarte.
