# Workflow wait accounting and journal recovery

## Evidence

Read-only inspection of the affected local 1.5.5 workflow found:

- The dependent Designer accumulated 34,588,988 ms (about 9h36m) without a model step. Its turn was reserved before dependency admission, and finalization charged all time since reservation.
- The Builder recorded 680 steps. One step took 23,273,041 ms (about 6h28m), while its shell command ran for 2,611 ms. This is consistent with the reported approval wait. Other recorded steps total about 3h04m; the entire elapsed period was not idle.
- Diagnostics recorded `session_journal_writer` and `session_storage` failures. They do not identify an OS error or conclusively distinguish a timeout from an I/O failure.
- The coordinator requested recovery of the same worker with the same objective and existing file changes. There is no evidence that it deliberately restarted discovery from scratch.
- The worker journal nevertheless contains old-turn deltas after the recovery checkpoint and the cancellation checkpoint of a newer turn. This is direct evidence of overlapping journal writer lifetimes.

## Root causes and changes

1. **Elapsed time:** use a monotonic active-work clock. Queued workers and human waits pause it. A coordinator includes productive descendant execution, but pauses when descendants are all queued or waiting. Retries preserve measured work without charging offline time. The same timing sample feeds the transcript, active status and agent cards.
2. **Writer lifetime:** remove the five-second acknowledgement deadline, which could report storage failure while successful writes were still pending. A stable OS file lock covers replay through the writer's final drain, including lightweight file-inspector sessions. A successor cannot read an incomplete checkpoint and start a competing writer. Real write errors still fail closed.
3. **Continuation:** failed recoverable worker turns reuse their existing turn, results and elapsed work. Coordinator guidance is appended as runtime guidance. Successful handoffs followed by new review work still create a new round. Uncertain effects continue to require inspection before mutation.
4. **Legacy journals:** a terminal checkpoint seals an older turn. Late deltas targeting that sealed turn cannot reopen it or invalidate a newer turn. Both replay and history indexing use this rule; damaged cached indexes are rebuilt. Unknown or invalid deltas on the current turn remain errors. Original journal bytes are retained.
5. **Large checkpoints:** records larger than 10 MiB are framed into ordered version-2 chunks inside the same JSONL file. Each physical line remains below the existing limit. The reconstructed version-1 payload must match its SHA-256 digest before replay exposes it. Indexed offsets span the whole logical record, so historical tool details, recovery and vacuum share the same reader. A missing final chunk is treated as an incomplete suffix: preserve the original file, then recover the previous complete record. Invalid ordering or checksums remain errors. Writer retries roll back their unacknowledged suffix before retrying, including partial writes and complete writes with failed sync, without replaying confirmed deltas.

## Reference decisions

- Codex `rollout/src/recorder.rs`: acknowledgements follow actual persistence; no short elapsed-time cutoff substitutes for writer completion.
- Codex records append events individually, while OMP also stages atomic entry batches. Jarvis retains its indexed checkpoint contract and implements atomic logical records through bounded physical framing, without adding payload sidecars or changing backup paths.
- Codex `rollout/src/writer_lock_tests.rs`: competing journal owners are excluded through the owner's lifetime.
- Codex `tui/src/status_indicator_widget/timer.rs` and its tests: pause/resume preserves active duration and excludes wait time.
- OMP `session/session-manager.ts`: single append ownership and fencing against stale persistence work.
- OpenCode `session/session.ts`: transactional state updates rather than overlapping independent journal owners.

## Validation boundaries

Regressions cover delayed persistence beyond five seconds, replay waiting for the previous writer, overnight waits, queued dependency failures, coordinator timing, preservation of confirmed results during worker recovery, and out-of-order legacy records. These tests use isolated local fixtures; they do not resume or modify the user's stopped workflow.

Validated locally: `bun run check` (726 tests passed, one skipped, plus lint, TypeScript, build and IPC contract checks), `cargo clippy --all-targets --offline -- -D warnings`, `cargo test --offline` (928 passed, 22 ignored), formatting and whitespace checks.

Follow-up `jarvis-b3cy` covers the additional checkpoint limit. The inspected checkpoint was 10,153,860 bytes, below 10 MiB; exceeding it was not established as the incident's cause. The regression fixtures exceed that limit and cover completion, cancellation, interrupted recovery, retry with uncertain effects, historical paging and deferred details, vacuum, torn chunks, corrupt chunks and an injected partial append failure. Existing version-1 journals remain readable; older application versions cannot read new version-2 chunks. Logical records still need memory proportional to their content when opened; framing does not discard results or make the full checkpoint constant-space.

The earlier YOLO fix is already committed separately but was absent from the installed 1.5.5 runtime in this incident. This change does not retroactively estimate human-wait durations in older stored turns, nor does it establish that the remaining model work was efficient. A full production workflow and Windows/Linux runtime behavior still require validation on the updated application.

## Follow-up: initial provider rejection on 1.6.0

Read-only inspection of the next Builder attempt found `provider_request` after approximately five seconds, one empty step and no new tool calls. The previous worker turn had been cancelled, so opening a new round was expected. The retained call/result suffix had no unmatched IDs, malformed argument JSON or missing encrypted reasoning content. This does not prove that the final projected provider request was valid.

The runtime did not record this incremental provider failure in diagnostics. Its upstream code, HTTP status, request ID and rejected field cannot be recovered from the saved journal. The precise reason for the original rejection remains unknown; neither a malformed legacy history nor an unsupported model has been established.

The confirmed recovery gap was in the WebSocket transport: only three special initial rejection codes triggered the existing HTTP fallback. A general initial request rejection ended the worker immediately. The shared transport now switches once to HTTP with the complete input after a pre-acceptance request rejection and disables WebSocket reuse for the remaining provider turn. Existing HTTP retries for transient failures remain unchanged. It does not reexecute local tools. Authentication, rate limits, context overflow and errors after any accepted response event remain outside this fallback. A persistent HTTP request error still ends the attempt instead of creating a retry loop.

Recovered and terminal incremental failures now record sanitized provider metadata. Recognized protocol field paths can appear in the user-facing error; raw upstream messages, arbitrary schema property names and echoed input are excluded. Semantic error categories take precedence over an envelope's generic HTTP 400 while diagnostics retain the actual status.

This follows Codex's persistent per-session transport fallback and structured WebSocket error handling. Regressions use local HTTP/WebSocket servers to verify full replay with confirmed receipts, no WebSocket retry on later steps, no fallback after response creation or a text delta, and consistent sanitized error details across transports. The stopped production workflow was not resumed or modified; real-provider acceptance of the original request remains unverified.

Local validation: `bun run check` passed (726 tests, one skipped, lint, TypeScript, build and IPC contracts); Rust formatting and Clippy passed without warnings; 75 focused provider tests passed (two live tests ignored). The first full Rust run had one existing LSP test fail (zero applied activities instead of four); it passed unchanged in isolation. The full suite then passed with `cargo test --offline --quiet -- --test-threads=4` (932 passed, 22 ignored). Follow-up `jarvis-iwca` tracks that test's sensitivity under load. These checks do not establish acceptance by the live provider.
