# Runtime efficiency contracts

This iteration implements the five improvements tracked by Beads epic `jarvis-thuk`. Codex remains the primary reference; OpenCode and OMP informed permission reuse and command lifecycle decisions. Concepts were adapted to Jarvis's existing typed tool registry, local journal, approval drawer and provider boundaries.

## Execution permission recovery

Command admission distinguishes hard restrictions from access that the user can approve. A command referencing an external path, or explicitly requesting `sandboxPermissions: "require_escalated"` with a justification, requests informed review for native execution. File tools remain project-scoped. The working directory still belongs to the project; an approved command can name an external target.

Native grants bind the exact parsed command, working directory and detected read/write paths. Sandboxed grants and broad command prefixes cannot silently authorize native execution. Git global directory options are considered when inspecting scope. Privilege escalation and capability mismatches remain explicit denials.

A failed isolated command preserves output and supplies structured recovery arguments when the failure resembles a permission denial. Since failure can follow partial side effects, Jarvis never automatically reruns the command. The agent must inspect what happened and submit the necessary reviewed action. The existing approval mode and matching scoped grants govern subsequent execution.

## Publication confirmation

`jarvis_propose_publication` uses the native review drawer for new proposals. The compatibility flag `previewOnly=true` forces that review, including for actions that could otherwise execute automatically; it no longer creates a conversational checkpoint or asks the user to type a confirmation phrase.

Standalone local branch selection/creation and fast-forward synchronization follow the turn approval policy. They cannot include reset, rebase, commit, push or a pull request. An explicit request for review still opens the drawer. These operations use the same typed validation and execution path; no shell bypass is introduced.

`confirmedProposalId` remains compatible with legacy previews already recorded in conversation history. The receipt must come from the previous completed turn, cannot have been attempted in the current turn, expires after 24 hours and binds repository state and operations. A short unambiguous affirmative reply can still confirm it. Fuller replies, observations and refusals return `revision_requested` as guidance for the model instead of a blocking validation error: interpret the user's current intent, incorporate changes or stop as requested, then use native review. They do not authorize the old proposal automatically. Stale or changed receipts require a fresh native proposal, never another literal text confirmation. Explicit autonomous requests and approval with observations retain their scope checks.

## Finite command sessions

`bash` yields a handle instead of killing a command after its initial wait. `bash_wait` returns incremental output/completion; `bash_cancel` stops that command's process tree. Handles belong to one execution and cannot control another conversation's commands or the user's terminals. Turn cancellation and manager disposal signal cleanup.

Limits are eight running commands, 64 retained sessions, 64 KiB of retained output per command, and a maximum 30-second wait per call. Output includes cursor/truncation, status, exit code and duration. Final durations stop advancing. Permission recovery examines retained output even when the relevant line was returned in an earlier chunk. The agent must settle running commands before its final response or workflow handoff. Commands do not survive application restart; existing journal recovery preserves uncertainty instead of replaying them automatically.

## Parallel reads

The harness groups consecutive independently eligible reads with at most four running and 16 per batch. Mutations, approvals, unavailable tools and repeated identical requests end a group. This applies in direct agents and workflows to native file reads, visible read-only MCP tools, initialized LSP tools, attachments and skills.

Registry contracts, schema validation, workflow scope, publication restrictions, instructions and execution policy still apply. MCP calls share the multiplexed peer; a failed connection is marked for exclusive reconnection without replaying actions. Validation precedes reconnection. LSP calls to one server remain serialized to preserve its protocol state while overlapping other servers and reads. Results retain call IDs, original provider order and individual durations.

## Incremental Responses transport

The provider details dialog offers **Conexão incremental · experimental** for OpenAI Codex and compatible custom OpenAI Responses accounts. The preference is off by default, stored per alias and applied to new executions. Anthropic Messages, OpenAI Chat Completions and Antigravity keep their existing transports. Provider preferences remain outside the settings backup, consistent with the existing account exclusion.

One live WebSocket belongs to one immutable provider turn session. Its upgrade uses the configured endpoint and the same authentication headers as HTTP. A request includes `previous_response_id` and only new input when all non-input request metadata and the exact prior input/output prefix still match. Compaction, instructions, model or catalog changes invalidate reuse. The local journal always retains complete replay and `store: false` is preserved.

Upgrade rejection before inference submission falls back to the ordinary HTTP transport for the rest of the turn. An explicit unsupported/stale-response rejection before any response event also permits fallback. A send failure or interrupted accepted stream is uncertain: received deltas are preserved and no automatic HTTP replay occurs. The existing user-triggered retry path remains available. Authentication data and provider content are not added to telemetry; existing request/response counters record sent bytes, timings and usage.

The implementation bounds upgrade, send and idle waits and reuses existing event/message size limits. Local WebSocket/HTTP peers test connection reuse, prefix invalidation, fallback and interrupted streams. These fixtures establish runtime behavior, not production latency gains or compatibility with every compatible endpoint. Packaged macOS/Windows/Linux execution and live-provider validation remain distinct from the automated suite.

## Reference paths

- Codex: `core/src/tools/orchestrator.rs`, `core/src/unified_exec`, `core/src/tools/parallel.rs`, `core/src/client.rs`, and `codex-api/src/endpoint/responses_websocket.rs` under `docs/codex/codex-rs`.
- OpenCode: `docs/opencode/packages/opencode/src/permission/index.ts`.
- OMP: `docs/omp/packages/coding-agent/src/tools/bash.ts`.

Reference trees were studied read-only; no source was copied textually or changed in those trees.

## Automated verification

Validated on macOS on 2026-09-24:

- `CI=1 bun run check`: lint, TypeScript, 658 passing frontend tests (one pre-existing skip), production frontend build and generated IPC contract check.
- `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings`: passed without warnings.
- `cargo test`: 784 passing Rust tests; 19 explicitly ignored environment/live integration tests were not executed.
- `git diff --check`: passed.

The CI worker limit was used locally to avoid DOM test timeouts during heavy builds. An old schema-version expectation and the new provider-preference mock were updated for the migration/UI contract. The publication-receipt fixture now records the durable call/result pair, matching production journal behavior. The installed app was not stopped and no live account was contacted by these tests.
