# Jarvis efficiency review after native Core integration

Date: 2026-09-24. Jarvis baseline: `6aaae74`, version `1.4.0`.
Primary reference: local Codex checkout `a592c38c16` (2026-09-13).
Review: `jarvis-vzw1`. Recommended follow-up backlog: `jarvis-i1q3`.

Implementation follow-up: the six recommendations were subsequently authorized
and implemented. See [implementation and evaluation notes](evaluations/efficiency-2026-09-24.md).
Source line references and findings below describe the audited baseline.

## Conclusion

There are still concrete improvements worth pursuing. The main architectural contracts are already present; the remaining opportunities concern unnecessary inference round trips, repeated context preparation, and synchronous waits inside an asynchronous runtime. Another broad prompt rewrite would not establish that these costs have been removed.

Six follow-ups are justified below. Their existence is supported by source inspection; their production latency and token savings have **not** been measured. In particular, passing contract tests does not prove that a model completes a real task quickly, correctly, or without unnecessary exploration.

The next cycle should establish comparable measurements, then improve persistence, compaction, context preparation and MCP discovery. Starting tools during streaming is a later, more invasive optimization. Preserve the autonomy and recovery behavior already delivered.

## Scope and evidence boundaries

Reviewed the current turn loop, provider adapters, streaming dispatch, context preparation, compaction, journal writer, revisioned events, tool contracts, parallel reads, MCP discovery, skill discovery, telemetry and evaluation fixtures. Cross-checked the earlier reports:

- `docs/CODEX-HARNESS-REASSESSMENT.md`
- `docs/CODEX-EFFICIENCY-RUNTIME.md`
- `docs/AGENT-HARNESS-EFFICIENCY-AUDIT.md`
- `docs/CONTEXT-EFFICIENCY.md`
- `docs/core-integration.md`

Codex is the primary source. OpenCode's pruning policy and OMP's provider-aware compaction budget supply useful complementary contracts. Reference checkouts were inspected without modification. This review describes those local snapshots, not an exhaustive inventory of the newest upstream commits or proprietary Codex desktop behavior.

No paid-provider comparison or new real-project execution was performed. No harness trace files were found in the expected production/development telemetry directories during this review; that alone does not establish a telemetry defect. No user conversations, project repositories, credentials or running app processes were changed.

## Mechanisms already implemented

These are existing foundations to preserve, not recommendations to implement again.

| Area | Jarvis evidence | Assessment |
| --- | --- | --- |
| Explicit intent and corrections | `agent/context_manager.rs`, `agent/compaction.rs`, `mcp/runtime.rs` | Original user authorization and requested MCP scope are represented separately from summaries and runtime guidance. |
| Recoverable tool errors | `agent/tool_contract.rs`, `mcp/runtime.rs` | Names, schemas, scope and execution contracts are checked before effects; malformed arguments can return to the model. |
| Repetition and long tasks | `agent/tool_loop.rs`, `agent/progress.rs` | Repetition recovery exists; raw action volume is no longer treated as proof of stagnation. |
| Durable replay and resumption | `agent/journal.rs`, `agent/history.rs`, `agent/session_writer.rs` | Incremental journal records, bounded replay and explicit durability barriers already exist. The synchronous acknowledgement path still needs refinement. |
| Renderer continuity | `agent/events.rs`, `src/core/agent-events.ts` | Revisioned deltas, bounded replay and authoritative resynchronization exist. Smaller IPC payloads do not eliminate all internal snapshot copies. |
| Immutable inference settings | `agent/context_manager.rs`, `agent/provider/capabilities.rs` | Provider capabilities and settings are frozen for an inference step. |
| Provider retry/conformance | `agent/provider/retry/tests.rs`, `agent/provider/conformance.rs` | Local fixtures cover transient failures, cancellation, fragmented streams and preservation of completed tool results. |
| Parallel reads | `agent/parallel_tools.rs`, `agent.rs:2874` | Eligible reads can overlap after the provider response completes. This is distinct from starting a tool while that response is still streaming. |
| Command lifecycle | `agent/command_sessions.rs`, `agent/turn_state.rs` | Yielding commands have handles and bounded output; cancellation, waiters and ownership are explicit. |
| Publication and permissions | `agent/publication.rs`, `agent/execution_policy.rs` | Publication previews, grants and recovery are established contracts. Do not add approval ceremonies merely to appear safer. |
| Incremental Responses | `agent/provider/incremental.rs` | Connection-local response reuse already exists as an opt-in experiment with invalidation and fallback. It is not a missing feature. |
| Native Core integration | `agent/core_runtime.rs`, `core/activity.rs`, `core/context.rs` | Automatic design context, post-edit LSP feedback, bounded auxiliary hooks and durable Core receipts were delivered in the baseline commit. |

Paths abbreviated with `agent/`, `core/` or `mcp/` in this report are relative to `src-tauri/src/`.

## Recommended follow-ups

### 1. Measure complete tasks and attribute their cost

Bead: `jarvis-i1q3.1`. Start here and keep this measurement active throughout the other changes.

**Current evidence.** `agent/telemetry.rs:164` records provider requests/responses, context sizes, tools, policy decisions, compaction and recovery. `agent/evaluation.rs:354` compares actual runtime reports to scripted expectations. The versioned runtime suite contains 14 scenarios. This is useful existing coverage, not an absent evaluation system.

The missing layer is a repeatable whole-task comparison with functional acceptance criteria. Current events do not separate preparation, journal acknowledgement, tool queueing, handler execution, Core post-processing and human wait. `TelemetryState::record` uses `try_lock` and can discard a record without an aggregate loss counter (`agent/telemetry.rs:482`). Aggregate successful turns are not a measure of successful user outcomes.

**Reference.** Codex `core/src/tools/call_trace.rs:32` records receipt/readiness, and `core/src/tools/parallel.rs:324` separates dispatch waiting from handler duration. These are useful attribution concepts; copying its entire telemetry stack is unnecessary.

**Change.** Extend the existing evaluator and sanitized local telemetry. Start with a small corpus covering direct edits, nested-repository publication against local fixtures, execution of an established migration, explicit MCP selection/recovery, planned handoffs, long-session compaction and restart continuity. Compare the same task, model, reasoning effort, account capabilities and project state. Record functional success, unnecessary calls, recoveries, input/output/cache tokens and phase timing. Report sample count, warm/cold cache conditions and event loss; separate human wait from agent latency. Keep live-provider measurements opt-in and distinct from deterministic local replay.

**Acceptance.** A regression changes an observable task outcome or measured budget, rather than merely changing a prompt string. Percentiles must disclose sample size. Never sum overlapping subagent/tool durations and present the total as wall-clock time. Never claim a percentage improvement from an unpaired run or a tiny sample.

### 2. Make journal acknowledgements asynchronous

Bead: `jarvis-i1q3.2`. High priority for responsiveness and stability under slow storage or concurrent chats.

**Current evidence.** `SessionWriter` owns a dedicated OS thread, but `agent/session_writer.rs:82` waits through `std::sync::mpsc::Receiver::recv_timeout`, with a five-second bound. `Session::persist_turn` appends and immediately flushes (`agent.rs:591`). `Session::update` invokes it while retaining the conversation's `std::sync::Mutex` guard (`agent.rs:934`). Thus the caller still waits synchronously for disk acknowledgement, including when invoked from the asynchronous turn loop. The existing worker does not remove that wait from the executor or from the state lock.

This establishes a blocking path, not evidence that every write takes five seconds or that it caused a particular prior production incident.

**Reference.** Codex `rollout/src/recorder.rs:1052` sends a flush command and awaits a oneshot acknowledgement. `core/src/session/mod.rs:1326` exposes that asynchronous barrier at the session boundary.

**Change.** Keep the single ordered writer and its recovery policy, but acknowledge durability asynchronously. Separate pending state from acknowledged state, avoid holding the conversation mutex across disk waits, and retain the necessary barriers before tool effects and subsequent inference can rely on a record. Consolidate redundant barriers only when ordering proves it safe. Shutdown and failure propagation need the same contract.

**Acceptance.** With an intentionally delayed writer, another chat and cancellation remain responsive. An actual persistence failure still prevents dependent effects. Crash/restart fixtures preserve user messages, call envelopes, results and approvals. An unacknowledged operation must never be reported as durable success.

### 3. Size compaction requests to the summarizer's usable context

Bead: `jarvis-i1q3.3`. High priority for long tasks and avoidable model round trips.

**Current evidence.** `agent/compaction.rs:554` converts the selected history to text and processes it in sequential chunks. The chunk size is `(window / 2).clamp(1000, 48_000)` **bytes**, not a budget calculated from the summarizer's tokens. Each call carries the previous summary. Overflow already halves the current portion and the original history is retained until a valid checkpoint is durable; those protections are valuable.

For a selected ASCII history of 480,000 bytes and a model window large enough to reach the cap, the loop requires ten summary calls in the no-retry case. That is arithmetic from the implementation, not a measured incident or a prediction of token savings. A large model window cannot increase the current 48,000-byte cap.

**References.** Codex `core/src/compact_remote_v2_attempt.rs:31` prepares and trims a capability-specific compaction request. OMP `packages/agent/src/compaction/compaction.ts:797` budgets against model context and summary reserves; at line 880 it uses a single summarization window when the material fits and splits only when necessary. OpenCode `packages/opencode/src/session/compaction.ts:271` offers a complementary example of pruning older tool outputs while protecting recent material.

**Change.** First improve portable summarization: account for system instructions, prior summary, output reserve and a safety margin, then pack complete conversation/tool boundaries into the usable budget. Try one call when it fits. Preserve adaptive shrinking on real overflow. Evaluate provider-native compaction separately behind explicit capability support and a tested fallback. Avoid introducing image-based history compression or removing arbitrary old evidence just because a reference implements it.

**Acceptance.** A history fitting the usable budget needs one summary request. Large histories, Unicode, small windows, overstated provider limits, errors and cancellation preserve the original until completion. Requests, corrections, approvals, uncertain effects and recent receipts survive. Keep the 80% automatic trigger and verify continuation quality, not just reduced summary length.

### 4. Cache skill discovery and remove repeated context preparation work

Bead: `jarvis-i1q3.4`. Begin with catalogue reuse; use profiling to decide subsequent allocation work.

**Current evidence.** Every non-GitHub inference iteration calls `skills::active` (`agent.rs:2574`). It obtains the shared catalogue lock and calls `snapshot` (`skills/mod.rs:302`), which invokes recovery, reads configuration and discovers skills (`skills/mod.rs:234`). Discovery walks directories and reads/parses skill metadata (`skills/catalog.rs:102`). The marketplace repository cache is a different cache and does not replace this runtime scan.

Context preparation also has repeated work: `compaction::input` clones replay; `StepContext::from_data` serializes/hashes it more than once; `StepContext::input` returns another cloned vector (`agent/context_manager.rs:90`, `:109`, `:157`). The telemetry calls at `agent.rs:2674` request that cloned input twice. Streaming creates a current-turn snapshot every emitted update, even though the final renderer protocol is incremental (`agent.rs:934`, `agent/events.rs:250`). The cost grows with retained/current-turn content; its share of real latency is unmeasured.

**Reference.** Codex `ext/skills/src/host_service.rs:240` reuses immutable skill snapshots by directory/configuration and supports forced reload/invalidation. This is the directly transferable pattern. Codex is not assumed to be allocation-free.

**Change.** Cache the effective skills catalogue by project and relevant configuration revision, with invalidation for installation, removal, enablement, editing and external changes. Preserve stable ordering and recheck availability at execution boundaries. Subsequently share immutable replay slices, reuse already computed sizes/hashes and profile snapshot generation before replacing it. Preserve the static prompt prefix and the dynamic runtime state already separated in Jarvis.

**Acceptance.** Unchanged rounds do not rescan the catalogue. Changes become visible at the next valid boundary, and a cached entry never authorizes a disabled skill. A 100+ action fixture measures preparation latency and allocations while preserving exact replay, immutable step contracts, event ordering and reconnect behavior.

### 5. Combine focused MCP discovery with bounded schema exposure

Bead: `jarvis-i1q3.5`. A targeted refinement of the existing on-demand catalogue.

**Current evidence.** `mcp/runtime.rs:1304` searches deferred tools. Its result only contains metadata and instructs the model to call `mcp_load_tool` (`:1433`). Loading is a separate operation (`:1442`), with the schema becoming available on a subsequent step. The normal path is search, another inference to choose/load, and another inference to use the tool. This deliberately limits exposure, but it adds a model round trip to discovery.

**Reference.** Codex `core/src/tools/handlers/tool_search.rs:191` returns selected loadable specifications through `ToolSearchOutput`; `core/src/tools/context.rs:194` serializes those specifications as the search result. The useful principle is discovery that also makes the selected capability available.

**Change.** Let a focused search expose a small, validated set within a schema budget in the next step, removing the mandatory load-only inference. Keep an explicit loading route for compatibility and exceptional cases. Use native tool-search payloads only where supported; ordinary function schemas can implement the same behavior for other providers.

**Acceptance.** Search then use succeeds without a required intermediate load call. Ambiguous results do not expose a whole integration. User-selected MCP scope, read-only restrictions, approvals, capability checks and changed/removed schemas still apply. This must work generically, without special treatment for Gemini Notebook, Database or any named server.

### 6. Dispatch completed tool calls while the provider continues streaming

Bead: `jarvis-i1q3.6`. Later priority because it changes replay, retry and lifecycle timing.

**Current evidence.** Jarvis records `response.output_item.done` internally (`agent/provider.rs:775`), but the loop callback only exposes text, summary, retry and reset (`:25`). `agent.rs:2703` awaits the whole response, then saves the call envelope and starts handlers (`:2790`, `:2874`). Existing parallel reads therefore overlap each other, not the remainder of provider generation.

**Reference.** Codex `core/src/stream_events_utils.rs:300` persists a completed call and creates its execution future. `core/src/tools/parallel.rs:150` starts dispatch while the stream can continue. The integration test `core/tests/suite/tool_parallelism.rs:304`, `shell_tools_start_before_response_completed_when_stream_delayed`, verifies that distinction.

**Change.** Add a typed completed-call event and begin with independently eligible reads. Validate the full name/arguments against the captured step, preserve call identity and ordering, and retain exclusive boundaries for dependent operations. Expand to effectful tools only after durable admission, uncertain-result and retry contracts are covered. Protocols without a reliable completed-call boundary retain the existing path.

**Acceptance.** A delayed-completion stream starts an eligible tool after its arguments are complete and before the response ends. Partial JSON never executes. Stream failure, cancellation and retry never duplicate an admitted mutation or discard a completed result. Approvals remain correlated; the next inference cannot consume undurable results. Test Responses, Chat Completions, Messages and Antigravity individually.

## Features to treat as experiments or keep out of this cycle

| Candidate | Decision and reason |
| --- | --- |
| Codex-style code-mode tool composition | Potentially valuable for batches of dependent reads and result filtering; `codex-rs/tools/src/code_mode.rs:67` exposes typed nested tools. It needs an isolated runtime, cancellation and the same approval/journal path for every nested call. Consider after measuring remaining tool round trips; Context-mode's existing execution tools do not automatically provide this host-tool orchestration contract. |
| Provider-native compaction | A candidate within follow-up 3, enabled only after route-specific compatibility and continuation tests. An OpenAI-compatible endpoint is not proof of support. |
| Enable incremental WebSockets by default | Already implemented as an experiment. Broader live-provider/packaged-app validation should precede changing the default. |
| Automatic model downgrades or additional reviewers on every task | Could change quality, cost and user intent. Preserve the chosen model/effort and add routing only when an explicit product option and evaluations justify it. |
| General semantic caching of external results | Do not infer freshness for changing databases, GitHub state or side-effectful tools. Existing content-validated local read reuse is the stronger starting point. |
| Mandatory ceremonial Core/tool calls | Native preparation and receipts already provide integration evidence. More compulsory calls can increase latency without improving the result. Context7 should remain capability/task driven. |
| Adopt all upstream subsystems | Codex's cloud, enterprise and experimental subsystems do not automatically improve a local multi-provider desktop product. Every adoption needs a Jarvis problem and an observable acceptance criterion. |

## Recommended sequence and delivery evidence

The backlog records the sequence: whole-task measurements, asynchronous persistence, adaptive compaction, cached preparation, combined MCP discovery/exposure, then streaming dispatch. Measurement should continue after each individual change. Do not combine all six into an unmeasured rewrite.

Native acceptance work also remains relevant independently of this review: the existing Beads items for clean onboarding (`jarvis-pam`, `jarvis-lug.2`), Windows interruption (`jarvis-9c9`), published macOS restart (`jarvis-9cq.3`) and native process interaction (`jarvis-rsm.3`) were still open. They are not new harness discoveries and should not be represented as completed by source-level tests.

Commit `6aaae74` contains the previously implemented native Core work. Its existing validation logs were checked: frontend lint/typecheck/build and 665 tests passed (one skipped); Rust Clippy passed with warnings denied, and 798 tests passed (19 opt-in tests ignored). The isolated installed Context-mode smoke passed separately. Those results validate that commit's automated checks; they are not new performance measurements for the proposals here.

This review added documentation and recommended Beads work only. Its evidence consists of source/contract comparison, inspection of existing test cases and validation logs, and the explicitly labelled compaction arithmetic. None of the six follow-ups has been implemented or benchmarked by this review. No push or release was performed.
