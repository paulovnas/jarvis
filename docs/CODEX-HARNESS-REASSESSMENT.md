# Codex-based harness reassessment

Date: 2026-09-13. Implementation tracked by Beads epic `jarvis-z8w` and incident `jarvis-svg`.

Codex is the primary reference for Jarvis harness work. OpenCode and OMP remain complementary references. `AGENTS.md`, `CLAUDE.md` and `.gitmodules` now reflect that choice. The local Codex reference was inspected at commit `a592c38c16cdd7623dacc9168926ebccedfb67d3`; reference repositories were not modified.

## Production evidence

The Movarte conversation **Finalizar pendências do projeto** requested commit, push, PR and merge across independent repositories. Jarvis 1.0.0 interrupted that task with `progress_paused`. The session was inspected read-only; no Movarte files, Git state or publication were changed during this investigation.

| Measurement from the durable turn | Observed value |
| --- | ---: |
| Duration | 385,130 ms (6m25s) |
| Provider steps / tool calls | 69 / 69 |
| File reads | 43 |
| Shell calls | 18 |
| Search / Context-mode search / task updates | 4 / 1 / 1 |
| Recovery checkpoints attempted | 2 |
| Tool errors | 5 |
| Cumulative input tokens | 3,976,686 |
| Input tokens reported as cache reads | 3,709,184 (~93.3%) |
| Output tokens | 11,122 |

Input tokens are cumulative across inference requests. Cache reads are a subset of input, not additional usage. This trace already had a high cache-hit ratio. Excessive tool turns and repeated inspection still consumed time and tokens.

The proven failure sequence was:

1. The watchdog counted successful exploration toward a limit of 64 actions without a material mutation, even when results contained new evidence.
2. It required `progress_checkpoint` before any other tool.
3. The runtime appended that tool after role filtering, so the provider saw it.
4. The GitHub role's execution whitelist rejected it. A subsequent validation command was also denied because the checkpoint remained pending.
5. These impossible recovery attempts exhausted the checkpoint violation counter. The chat reported a "second stagnation", although the required recovery had never been permitted to execute.

The sanitized baseline is versioned in `src-tauri/src/agent/fixtures/evaluations/movarte-github-publication.json`. It preserves metrics and error classes without repository content, credentials or private conversation text.

## Reference comparison and implemented decisions

| Area | Codex/reference behavior studied | Jarvis implementation |
| --- | --- | --- |
| Turn continuation | `codex-rs/core/src/session/turn.rs`: tool follow-ups and pending user input drive continuation; ordinary reads do not need to produce file mutations | Removed the exploratory-action limit and fatal watchdog phases. Recovery is advisory and does not reject a final response or demand another user message |
| Tool dispatch | `core/src/tools/router.rs`, `registry.rs`, `spec_plan.rs`: advertised specifications and executable handlers belong to a finalized plan; recoverable call errors return to the model | Added `agent/tool_contract.rs`. Each provider step retains its exact visible schemas for preflight. Hidden/unknown native tools and invalid arguments return structured errors before approval or side effects |
| Recovery tools | Executable tool contracts cannot contradict recovery instructions | `progress_checkpoint` is allowed for all native roles and custom capabilities. The definition goes through the same role filter as the rest of the catalog |
| Repeated calls | OMP `packages/ai/src/utils/tool-call-loop-guard.ts` and OpenCode `packages/opencode/src/session/processor.ts` distinguish repeated calls from useful work | Five identical results cause steering; the next unchanged attempt is suppressed, while the chat can choose another action. The tool error no longer terminates the entire turn |
| Result identity | Runtime wrappers and rendering metadata are not semantic progress | Loop/progress observations use the original tool output, before Context-mode indexing adds changing identifiers. Changed polling output is new evidence; an identical task update does not reset progress |
| Arguments and recovery | Codex handlers return `RespondToModel` for malformed function arguments | Malformed JSON/non-object arguments become a recoverable tool error. The UI still receives a valid tool record. Native schema errors report missing fields, types, bounds and unexpected fields without echoing argument values |
| Commands | `core/src/tools/handlers/unified_exec/exec_command.rs`: explicit working directory and bounded execution | `bash` accepts `workdir`, scoped to an existing project directory. Nested repository commands no longer require `cd` chains. Scoped AGENTS.md loading also follows workdir |
| Output budgets | `utils/output-truncation/src/lib.rs`: preserve useful beginning/end with explicit truncation | Command capture drains the process with bounded memory, retains beginning and end, and reports omitted bytes. Timeouts retain partial output and tell the model to inspect effects before retrying |
| Local read reuse | Reuse existing evidence without pretending stale data is current | A request fully contained in an earlier retained range can reuse that result only when the whole file's SHA-256 still matches. Truncated/indexed results are not treated as complete retained source. Compaction invalidates the cache |
| User intent | `core/src/context_manager/history_user_authorization.rs`: original user instructions are retained separately from lossy summaries | Compaction preserves original user messages from the current and preceding turn, including auxiliary corrections, in chronological order. Runtime guidance is marked separately |
| Tool outcome continuity | Codex context management and OpenCode pruning protect useful recent tool state | A compacted checkpoint includes up to six bounded receipts for recent effectful/publication/check/handoff tools. Partial/uncertain outcomes and truncation remain explicit reference data; they do not authorize a retry |
| Publication | Codex focused instructions emphasize the actual requested outcome and reuse established evidence | Added a native read-only repository inspection; narrowed the GitHub role and default publication prompt to relevant diffs, required checks, one concrete multi-repository proposal and missing postconditions |
| Per-step memory | Avoid copying complete history for a small identity lookup | Duplicate call-ID checks collect only IDs instead of cloning the entire wire history on every inference step |
| Prompt composition | Stable, non-duplicated instructions improve cacheability and clarity | Removed a duplicate injection of the native authoring instructions. User model/account/reasoning choices remain intact |

Concepts and contracts were adapted to Jarvis. Codex's code, CLI surfaces and app-server architecture were not copied wholesale.

## Autonomous recovery contract

There is no global read count, total action limit or second-stagnation pause. After a streak of unchanged results/errors, Jarvis suggests a checkpoint and a bounded next action. The model may use the checkpoint, act directly on new evidence, ask for genuinely missing input, or finish with an accurate explanation of an external blocker.

The repeated-call guard still suppresses a redundant action. Failed edits still require fresh source before another mutation to that path. This prevents waste and stale writes without turning an individual call failure into a chat failure.

Actual cancellation, inability to durably save history, broken mandatory Core dependencies and invalid provider envelopes still have their own failure paths. The old `progress_paused` journal/status readers remain for compatibility with conversations produced by older releases. They do not generate new watchdog pauses.

The repeated-call prompt preserves the user's requested sources; it does not suggest escaping an explicitly selected MCP by switching to an unrelated integration.

## Focused GitHub publication

`jarvis_inspect_publication` consolidates local inspection into one model-visible call. It returns:

- Independent repository roots, including a parent repository and nested repositories.
- Branch/upstream status, changed/untracked entries, staged and unstaged diff summaries.
- Remote names, whitespace-check results and available package script names.
- Explicit field/discovery truncation and per-repository errors.

Discovery skips hidden/generated/dependency directories and symlinks. It has directory/repository bounds and accepts explicit known paths for subsequent refreshes. Each Git subprocess has a 15-second timeout, uses the platform process-group/job cleanup, supports cancellation and does not make network requests. No staging, commit, push, branch change, PR or merge occurs during inspection.

The intended sequence is local overview, relevant diff inspection, required validation or reuse of valid prior checks, a concrete proposal, and verification of the approved result. Full-file reading is reserved for a specific unresolved question; a publication request does not imply a new whole-project audit.

The existing supervised proposal remains responsible for actual Git/GitHub mutations, exact files, per-repository operations, existing PR reuse and partial-failure reporting. A declined proposal grants no permission. An uncertain push or partial publication must be inspected before a follow-up proposal. Git pushes remain available when `gh` is absent; PR/merge require the GitHub CLI.

Saved project publication prompts are not silently rewritten. The new default applies to default/new configuration, while the native GitHub contract provides focused execution guidance in existing projects.

## Context and provider boundaries

Compaction still keeps function-call/result boundaries intact and saves the new context only after a valid, space-reducing summary is produced. Original journal events remain durable. The new user-message list and tool receipts round-trip through the same checkpoint; older checkpoints without these optional fields remain readable. The dashboard/history token estimate includes these retained messages.

Recent original messages and auxiliary corrections are preserved exactly. Older conversation history remains summarized. Tool receipts are bounded, explicitly identified as untrusted reference data, and marked when shortened; a shortened receipt is not sufficient proof of success. Provider payload and context limits still apply to unusually large user messages.

Existing provider-specific streaming, retry and caching behavior was reviewed and retained: Codex prompt-cache keys, Antigravity session/signature handling, usage breakdowns, bounded inference retries and durable tool results are separate concerns. Tool arguments rejected locally are corrected by a subsequent model turn; Jarvis does not blindly resend a side effect.

The selected model and reasoning effort influence latency and decisions. The deterministic suite proves harness behavior, not a fixed execution time or the absence of all model hallucinations. No provider benchmark is fabricated from scripted tests.

## Explicit runtime contracts added in the second phase

The follow-up refactor moves the main harness boundaries from implicit conventions into small contracts that can be validated independently:

| Boundary | Implemented contract | Resulting behavior |
| --- | --- | --- |
| Session persistence | One `SessionWriter` owns each conversation journal, its ordered queue, last durable turn, bounded retries, flush barrier and shutdown drain | Transient write failures retain the pending operation in order. The agent waits only at declared durability boundaries, while filesystem I/O remains outside the async agent loop |
| Tool runtime | `Handler`, `Effect`, `ApprovalPolicy`, `Capabilities` and `PreparedTool` bind the exact model-visible schema to executable behavior | Unknown tools and malformed arguments fail as structured, recoverable results before approval or side effects. Approval and parallel eligibility derive from the registered contract instead of scattered name checks |
| Turn ownership | `ActiveTurn`, `TurnPhase`, a single mailbox and `TurnAdmission` model sampling, tool execution, user waiters, cancellation and drain | Approvals are correlated to their tool, auxiliary messages stop entering a draining turn, and an update cannot begin while turns are active or admit new work after its drain starts |
| Renderer events | Rust emits versioned `agent:event` batches with conversation, base revision, new revision and typed deltas | The renderer applies changes in order, ignores duplicate or stale batches and reloads an authoritative snapshot when it detects a revision gap. Full snapshots remain the resynchronization path |
| Model context | Typed context items and an immutable `StepContext` capture provider options, instructions, visible tools, input and original user authorization for one inference step | Runtime-only metadata is removed at the provider boundary, compaction cannot replace the user's authorization, and configuration changes cannot mutate a request already in flight |
| Provider execution | One `TurnSession` retains the credential, conversation session ID and pooled HTTP client for the complete turn; eligible native reads execute concurrently | Connection and retry identity are reused without leaking settings from another turn. Mutations remain serial, and parallel results are correlated by call ID then returned in the provider's original call order with individual durations |

The initial phase kept reuse limited to the HTTP connection pool, conversation session ID and provider cache key. A later review of Codex's live Responses WebSocket contract established that `store: false` can coexist with connection-local `previous_response_id`. Jarvis now offers that transport as an account-specific, default-off experiment. It reuses a response ID only on the same live connection after verifying the full request prefix and metadata; durable local replay remains authoritative. See [Runtime efficiency contracts](CODEX-EFFICIENCY-RUNTIME.md) for invalidation, fallback and recovery boundaries.

## Native relaunch after an installed update

The former updater spawned the updated executable before the current process exited and waited for a loopback readiness acknowledgement. On macOS, the current process still owned `tauri-plugin-single-instance`'s lock, so the plugin terminated the successor during setup and Jarvis reported that the new instance had exited before opening.

The updater now starts an admission drain, installs the artifact, flushes desktop state, performs the idempotent service/diagnostic shutdown and calls Tauri's native `AppHandle::request_restart()`. Tauri tears down the current runtime before launching the installed executable, which releases the single-instance lock in the required order. The obsolete port, token, child-process and window-ready handshake was removed. A runtime-contract test fixes the required sequence as `prepare_exit` followed by `request_restart`; a packaged-app update remains the necessary end-to-end confirmation because a test process cannot replace its installed application bundle.

## Existing mechanisms retained after review

| Subsystem | Reason to retain it |
| --- | --- |
| Journal deltas, fsync and validated replacement | Already preserve completed results and recover incomplete tails without repeating tools |
| Workflow recovery and isolated worker journals | Preserve assigned scope, original request, known outcomes and uncertain actions across restarts |
| MCP deferred discovery and per-tool activation | Already reduce the exposed catalog and preserve explicit server intent; schema validation/reconnection remain in the MCP client |
| Transactional patch and LSP integration | Already provide coherent multi-file edits and bounded diagnostics; no evidence justified replacing them during this incident |
| Core output indexing | Already keeps large outputs outside model context; observation fingerprints now avoid counting indexing metadata as progress |
| Manual publication review and workflow approvals | Preserve concrete user-visible decisions; autonomy improvements remove impossible harness gates rather than bypassing those decisions |

## Evaluation and delivery evidence

The regression suite covers native argument errors, malformed JSON recovery, hidden tools, direct/delegated GitHub recovery permissions, 2,000 useful reads without interruption, repeated-call suppression followed by a different action, repeated errors followed by recovery, changing terminal output, valid read containment, changed-file invalidation, truncated-read exclusion, nested command directories, bounded head/tail output, independent repository discovery, and compaction/reload with original requests and partial publication receipts.

Baseline observations and executable regressions are separate: the old 6m25s trace is retained as observed evidence; new tests run the actual contracts against synthetic/local fixtures. The reference suite does not publish Movarte or call paid providers.

Validation on macOS: `bun run check` passed lint, typecheck, 114 frontend test files (575 passed, one intentionally skipped) and production build. `cargo clippy -- -D warnings` passed. `cargo test` passed 649 tests; 18 environment-dependent tests remained intentionally ignored. `git diff --check` and `cargo fmt --check` passed. The Rust target measured 24 GiB with 94 GiB free on the volume, below the repository's cleanup thresholds.

Installed-app behavior, live provider timing and native Windows execution require their respective runtime environments; macOS source-level checks do not constitute a Windows smoke test. No application release, commit, push, or Movarte publication was performed as part of this reassessment.
