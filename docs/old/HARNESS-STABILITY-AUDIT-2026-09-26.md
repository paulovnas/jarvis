# Recovery stability audit against Codex

## Scope and evidence

The review began as a read-only comparison of the Jarvis runtime, including the uncommitted initial-provider-rejection recovery fix. All six findings are now implemented and passed the integrated checks below. Production conversations were not resumed or modified. These are independently identified code paths, not proven causes of the latest Movarte provider rejection. The previous runtime did not retain enough upstream details to establish that cause. Source locations in the finding descriptions refer to the initial audit snapshot and may have shifted during implementation.

Codex is the primary reference for lifecycle and recovery contracts. OMP contributes first-event and progress-aware stream deadlines. This review does not recommend more generic approval prompts or action-count-based stopping rules. Durable work tracking and acceptance criteria are in Beads.

OpenCode's `packages/opencode/src/session/processor.ts` preserves tool metadata on failure, waits briefly for settlement and marks abandoned calls interrupted; `session/prompt.ts` keeps those orphans from restarting the tool loop. Jarvis retains its journal and native publication contracts: an accepted publication must finish process cleanup and persist its receipt before the turn closes, rather than using OpenCode's short settlement grace period as evidence of completion.

## Findings in recommended implementation order

### 1. Technical parent failures cancel recoverable descendants — jarvis-gprv

`src-tauri/src/agent/workflow.rs:1407–1464` converts shutdown-cancelled workers back to `Interrupted` only for `progress_paused`. Other coordinator failures leave children `Cancelled`; `workflow/storage.rs:276–313` excludes them from recovery. Delegated coordinators have the same distinction in `workflow/dispatch.rs:1108`. An interruption caused by the runtime can therefore behave like an explicit user cancellation and require a new worker round.

Preserve the interruption cause and recoverable child turn across parent shutdown, while retaining explicit cancellation semantics. The relevant Codex contracts are `docs/codex/codex-rs/core/src/agent/control/legacy.rs` and the descendant reopening tests in `control_tests.rs:4450,4642`.

Acceptance requires a parent provider failure while a child has confirmed results, followed by persisted restart and resume. Worker identity, turn, receipts and completed effects must survive. The corresponding explicit-user-cancellation case must remain cancelled.

Implementation outcome: native and delegated coordinator shutdown now records technical interruption causes before stopping descendants. Settlement persists those children as interrupted, preserving their turn and receipts for recovery. An explicit user cancellation takes precedence, including cancellation that arrives after the technical marker was saved. Completed handoffs remain accepted. Regression coverage exercises persisted restart and asynchronous worker settlement, rather than relying only on status conversion in memory.

### 2. Canvas workflows lack equivalent continuation — jarvis-2qlq

`src-tauri/src/agent.rs:1557–1580` excludes `Flow::Custom` from the existing workflow recovery routes. `workflow/custom.rs:163–164` starts from the graph entry with an empty result list; job reuse at `:219` is scoped to the current run. After A completes and B fails, a new run can execute A again.

Persist graph position, accepted handoffs and rework state, and resume the interrupted node. Codex's persisted agent identity and context restoration in `docs/codex/codex-rs/core/src/agent/control/spawn.rs:1236` is a lifecycle reference, not a ready-made canvas engine.

Acceptance requires A completed → B failed → runtime restart → only B continues, including a graph with a rework edge and its existing iteration limits.

Implementation outcome: canvas manifests now persist the next node, accepted handoffs, visited nodes and active worker. Recovery uses the saved graph definition, resumes the same failed or interrupted worker turn and reuses an accepted handoff if shutdown occurred before the cursor advanced. Rework history and iteration limits survive restart. Final manual validation is idempotent for the same run. The composer also requires confirmation before replacing an existing canvas with a direct agent; individual custom agents retain direct-agent behavior.

Compatibility limits: the new manifest fields have serialization defaults, so existing native and canvas histories remain readable. A legacy canvas with existing jobs but no saved cursor cannot reliably reconstruct rework order; it reports that its results were preserved and requests a new task scoped to the remaining work instead of replaying accepted nodes. A deliberate blocked verdict is distinct from a technical interruption and is not automatically converted into a retry. These changes do not promise automatic repair of historical runs whose required checkpoint was never recorded.

### 3. Publication approval can block snapshots and cancellation — jarvis-xb6r

`src-tauri/src/agent/authoring.rs:575–602` holds `Session.data` while applying an approved mutation. Publication runs inside that call (`:691`), and `publication.rs:805–819` uses synchronous `Command.output()` without process cancellation. A stuck Git hook or transport can prevent both `snapshot` and `cancel_agent_turn` from acquiring the same state lock. Question answers also flush synchronously under the lock (`questions.rs:313,356`). The gh HTTP timeout does not bound arbitrary Git processes or hooks.

Extend the existing asynchronous writer pattern to these paths: commit the decision durably, release the state lock, run controlled work, and persist its result. Codex removes the pending approval under lock and delivers the decision after releasing it (`docs/codex/codex-rs/core/src/session/mod.rs:3364–3377`).

Acceptance requires artificially slow publication and persistence while snapshots and cancellation remain responsive, with no duplicate publication after interruption. Jarvis already has a related lock-release test for `update_async` in `session_writer.rs:362`.

Implementation outcome: answers are claimed once under the session lock, then persisted and executed outside it. Git/GitHub commands reuse the supervised shell process lifecycle, cancel the process tree and enforce an inactivity timeout instead of an absolute duration limit. Fetch/push emit progress. Accepted interactions retain a completion receipt until execution and journal flush finish, including when Claude drops its tool callback on cancellation. Turn finalization drains these receipts before another turn can begin. Slow disk, duplicate answers, stuck descendants and cancelled autonomous publication have deterministic regressions.

Technical cleanup has a separate cancellation signal from an explicit user stop. CLI-initiated callback cancellation is classified as an interruption unless the user stop signal is set. Integration regressions verify that cleanup leaves descendants recoverable and that a user cancellation still takes precedence.

### 4. A total HTTP deadline cuts otherwise active streams — jarvis-d9um

`src-tauri/src/agent/provider.rs:282` and `provider/custom/mod.rs:101` set a 600-second total request deadline. Stream readers separately enforce idle limits. A valid response lasting longer than ten minutes can therefore be cut while data continues arriving, then enter the bounded retry path in `provider/retry.rs`. This can waste substantial time without any defective tool call.

Separate connection, first-event and idle deadlines from the lifetime of an actively progressing response. Keep cancellation and bounded retries. Codex's SSE reader uses an idle timeout (`docs/codex/codex-rs/codex-api/src/sse/responses.rs:584`); OMP explicitly distinguishes first-event and progress deadlines (`docs/omp/packages/ai/src/providers/openai-codex-responses.ts:3897–3941`).

Acceptance uses a local stream with scaled or virtual time: ongoing events beyond the former total deadline complete once; a silent stream times out; cancellation promptly stops either case.

Implementation outcome: shared HTTP and custom-provider clients retain connection and read-idle deadlines without a total response deadline. SSE separately bounds the first meaningful event, so keepalive comments cannot keep an empty response alive indefinitely. A simulated stream runs successfully for 700 virtual seconds; header/body stalls and cancellation retain bounded behavior. Antigravity uses the same first-event contract. Non-streaming authenticated operations retain their separate operation timeout.

### 5. Byte-triggered compaction can still leave an oversized request — jarvis-lbyj

`src-tauri/src/agent/compaction.rs:574–577` detects replay larger than 7 MiB, but its tail selection (`:592`) and post-summary checks (`:674–678`) use token estimates that exclude encrypted content (`:86–103`). The next `StepContext::capture` rejects requests above 8 MiB (`context_manager.rs:113`). Compaction can therefore report success without removing the condition that aborts the next step.

A synthetic in-memory probe retained 9,217,206 bytes from an original 9,347,192 bytes while estimated tokens dropped from 43,692 to 364. This was a calculation against the selection logic, not a live provider run or a new Rust regression test. Codex accounts for encrypted reasoning in its estimate (`docs/codex/codex-rs/core/src/context_manager/history.rs:799–804,873–886`).

Select a safe suffix using the actual byte budget and validate the projected request before accepting the new checkpoint. Acceptance must combine large opaque blocks, low estimated tokens, preserved user intent and receipts, and a successful next context capture.

Implementation outcome: byte pressure selects a safe compaction boundary and the final serialized request is checked before the summary is committed. Tests cover opaque replay larger than 8 MiB with little visible text, preserved original/latest user intent and confirmed mutation receipts, followed by the real next-step context capture. A summary that leaves the preserved request oversized is rejected without committing it.

### 6. An unrelated read can clear the uncertain-effect recovery guard — jarvis-g84s

`src-tauri/src/agent/workflow.rs:674–696` sets a single `recovery.inspected` flag after any allowed read. The allowlist (`:1006`) includes `ctx_stats`, `find_skills` and `web_search`. These operations do not establish whether an interrupted push, file write or external mutation actually succeeded. The checkpoint retains tool names, while more useful call IDs and arguments remain in the journal.

Reconcile the affected operation automatically using its saved call and applicable evidence, instead of treating any read as proof or asking for generic approval. For arbitrary external tools, the runtime must preserve an unknown outcome rather than silently declaring it reconciled.

Acceptance requires an effect that succeeds before its acknowledgement is lost: an unrelated read does not clear that uncertainty; relevant verification discovers the completed effect and continuation does not repeat it. Codex's `core/tests/suite/multi_agent_resume.rs:168,370,426,462` provides an integrated simulated-server/restart/compaction testing pattern. This review did not find a complete generic external-effect reconciler in Codex and does not claim one exists.

Direct-retry integration: retries for direct agents now derive the root recovery checkpoint from the latest stored turn's explicit runtime retry marker and uncertain tool calls. A new user turn clears that checkpoint. This extends the guard beyond coordinated workflows without treating ordinary follow-up messages as retries. Read-only legacy planning sessions retain their existing route. Generic external effects still require relevant service evidence; this integration does not establish universal exactly-once execution.

Implementation outcome: uncertain operations retain call IDs, argument fingerprints and affected resources. `recovery_resolve` records applied, not-applied or unknown outcomes using successful relevant inspections whose results exist in the current journal. Full writes additionally verify the complete file hash; multi-file patches require every path, and a directory listing only covers absent files in that directory. Path aliases and symlinks resolve to the same resource. Remote publication requires remote evidence from the same repository, and MCP evidence matches the actual server and resource selectors. Metadata-identified read-only MCP calls are removed from the uncertain mutation set. The guard applies to both native and Claude execution, returns recoverable tool errors and allows independent work to continue.

Evidence limits: repository/service inspection identifies relevant evidence, but the model still interprets semantic outcomes such as a remote merge or arbitrary patch. Generic tools without observable resource identifiers can remain unknown; the runtime does not invent a receipt or silently replay them. Confirmed verification commands can run again, while an interrupted check still requires reconciliation. These boundaries deliberately avoid claiming exactly-once execution for external services.

## Validation and release implication

Final local checks on the combined implementation:

- `CARGO_INCREMENTAL=0 CARGO_NET_OFFLINE=true bun run check`: passed lint, strict TypeScript checks, 728 frontend tests (one skipped), production frontend build and the generated IPC contract check.
- `cargo fmt --all -- --check`: passed.
- `CARGO_INCREMENTAL=0 cargo clippy --all-targets --offline -- -D warnings`: passed without warnings.
- `CARGO_INCREMENTAL=0 cargo test --offline`: 963 passed, zero failed, 23 ignored. Ignored live tests, diagnostics and subprocess-only fixtures are not counted as provider acceptance.
- `git diff --check`: passed.

Rust build artifacts exceeded the repository's maintenance threshold during implementation. After verifying that no process was using the target directory, `cargo clean` removed 51.3 GiB; subsequent builds used `CARGO_INCREMENTAL=0`, and the target measured 6.4 GiB after validation. No production conversation or running Jarvis instance was restarted for these tests. No commit, push or release was performed.

The local whole-task evaluator in `src-tauri/src/agent/evaluation/whole_task.rs:227–301` exercises scripted file handlers; its live comparison at `:338` is ignored by default and consumes externally collected observations. Passing those tests is not evidence that a long workflow survives the entire failure/restart/resume sequence. New lifecycle regressions cover persisted restart, worker identity, accepted handoffs, cancellation precedence and canvas rework; provider behavior and production acceptance still require their own evidence.

The release criterion should include completed artifacts, preserved identities and receipts, responsive cancellation and absence of duplicated effects after injected failures. Add a small real-provider smoke run separately to detect provider-contract changes; keep its evidence distinct from deterministic regression tests. The original provider rejection remains pending real-provider acceptance as tracked in `jarvis-rlxr`.
