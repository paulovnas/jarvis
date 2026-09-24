# Residual harness efficiency implementation

Baseline: `6aaae74` (Jarvis 1.4.0). Work: `jarvis-i1q3` and its six child tasks.
The reference checkouts remain unchanged. This change does not change provider,
model, reasoning effort, approval policy or the automatic 80% context threshold.

## Delivered behavior

| Area | Implementation | Observable validation |
| --- | --- | --- |
| Measurement | Content-free preparation, journal, dispatch queue, handler, Core postprocessing and human-wait spans; sample counts and p50/p95; dropped-record counter. Paired task observations require the same task/model/reasoning/configuration/sample and functional criteria. | Overlapping spans report occupied time separately from summed work. Actual local file tasks verify their artifacts. A completed runtime with a failed artifact check fails evaluation. |
| Persistence | The async turn loop queues ordered immutable snapshots, releases the conversation mutex and awaits a oneshot acknowledgement. Provider envelopes, completed results, usage and compaction retain durability barriers. Worker destruction queues a drain without joining a disk worker on the executor. | A deliberately paused journal does not prevent snapshots on a single-thread executor. Cancelling a waiter does not cancel queued writes. Concurrent acknowledgements emit only their own persisted revision. Existing disk-failure, incremental replay, queue and approval regressions remain in the suite. |
| Compaction | Input portions scale with the model window, response reserve, previous summary and tokenizer margin. Prefer message boundaries; split oversized messages at UTF-8 boundaries. A real overflow reduces the submitted portion before retrying. | A fitting large history needs one summary call. Small-window, Unicode, overflow, cancellation, intent preservation and durable-reload cases exercise the fallback. Original history is not pruned before the checkpoint acknowledgement. |
| Preparation | An eight-project immutable runtime skill cache checks config and metadata of discovered files/directories, including missing roots and symlink targets. Unchanged catalogues avoid parsing/recovery/discovery. Context input is borrowed for inspection, its serialized size is reused, and replay clones only the retained portion. | Edit, install, removal, disabled status and project-switch invalidation; old snapshots remain immutable. `read_skill` still revalidates the current catalogue and permission. A 128-action fixture measures preparation and inspection allocations/latency while asserting exact durable replay. |
| MCP discovery | Search automatically exposes up to three highest-ranked matches within 24 KiB. The next inference can use their validated ordinary schemas. The eight-tool working set evicts older schemas when it exceeds 64 KiB; one deliberately loaded large schema remains usable. Explicit load remains compatible. | Search followed by direct use without load-only inference, role/read-only restrictions, explicit integration scope, catalogue removal and reconnection. No integration-specific special cases. |
| Streaming execution | Responses completed-item events carry a complete call and its provider replay prefix. Up to four independent native reads can begin while the provider continues. Unsupported, dependent, invalid or effectful operations end this prefix. | Completed reads and opaque continuation metadata survive disconnect. Replayed call IDs do not start a second job. Partial/invalid arguments cannot pass dispatch validation. Messages, Chat Completions and Antigravity retain terminal-response validation. |

The synchronous persistence APIs used by native control operations remain
available. The change targets the async inference/tool path; it does not remove
the durable acknowledgement required when accepting a user's queued message,
approval or configuration mutation. The historical replay format stays readable.

## Running evaluations

`bun run eval:harness` includes the existing sanitized Movarte regressions,
provider stream replays and the new deterministic whole-task comparison. It
requires no account, network service, credentials or paid model request.

The local paired samples deliberately run the same implementation on each side
to test the comparison mechanism. Their output is a calibration of the runner,
not evidence of improvement over the baseline commit. Missing provider token
usage is `null`, never an invented zero.

The live corpus is
`src-tauri/src/agent/fixtures/evaluations/whole-task-corpus.json`: explicit MCP
fidelity, publication in multiple repositories, migration application and the
planned status-selection flow, plus direct editing, long-session compaction
and interrupted-task resume. Its criteria refer to resulting artifacts and
confirmed effects, not the agent's completion claim. Use isolated fixture
repositories/databases and explicitly authorized provider accounts for live runs.

The opt-in comparator accepts a JSON array of paired observations:

- `identity`: `task` (corpus ID), `model`, `reasoning`, `configuration`, `sample`.
- `variant`: `baseline` or `candidate`; `source`: `live`.
- `runtimeCompleted`, and `checks`: every named corpus criterion with its
  independently verified boolean result.
- `wallMs`, `humanWaitMs`, `calls`, `errors`, `recoveries`, `lostEvents`.
- `inputTokens`, `outputTokens`, `cacheReadTokens`, `cacheWriteTokens`: numbers
  from recorded provider usage, or `null` when unknown.
- `phases`: phase names mapped to arrays of individual measured durations.

```bash
JARVIS_EVAL_RUNS=/absolute/path/paired-runs.json \
  cargo test --manifest-path src-tauri/Cargo.toml \
  harness_evaluation_whole_task_live_pairs -- --ignored --nocapture
```

The comparator reports per-task/model/configuration cohorts, samples, p50/p95,
calls, recovery, usage/cache and loss. Mismatched samples, sources or functional
criteria are rejected. Any failed candidate outcome or missing candidate events
fails this live acceptance check. It does not create PRs, run migrations or call
providers itself.

## Interpretation

Phase times can overlap: a tool may execute during generation, and a human wait
may be nested in a handler. They must never be added to claim total elapsed time.
The trace reports both per-span work and interval union within each turn for
each phase. Turn wall time comes from the turn lifecycle. The loss counter is
explicitly scoped to the current app run; historical missing events cannot be
reconstructed.

Automated contract tests demonstrate the removed intermediate operations and
preserved recovery behavior. They do not establish a percentage of production
latency/token improvement, live-provider quality, or packaged Windows/macOS UAT.
Those require paired real sessions. No such production performance claim is
made by this implementation.

## Local preparation measurement

The deterministic 128-action fixture ran 25 samples in the macOS debug test
build. Allocation tracking is test-only and thread-local, so background journal
workers are excluded. Counts include allocation/reallocation requests and
cumulative requested bytes, not peak or retained memory.

| Phase | p50 / p95 latency | p50 allocation requests | p50 requested bytes |
| --- | --- | --- | --- |
| Current complete step preparation | 24.809 / 26.168 ms | 2,102 | 1,559,924 |
| Prior telemetry inspection (two clones and serialization) | 9.707 / 9.997 ms | 4,121 | 2,008,892 |
| Current borrowed inspection | 0.000750 / 0.001125 ms | 0 | 0 |

The second row reproduces the old inspection operations against the same
immutable step; it is not a benchmark of the complete baseline application.
These figures show which repeated work was removed, not a percentage improvement
for a real agent task. Tests assert identical visible input and zero allocations
for borrowed inspection; they do not assert noisy timing thresholds.

```bash
cargo test --manifest-path src-tauri/Cargo.toml \
  harness_evaluation_profiles_preparation_and_inspection_for_128_actions -- --nocapture
```

## Validation

Final automated checks on 2026-09-24:

- `bun run check`: lint, typecheck, build and IPC contract check passed;
  665 frontend tests passed, 1 skipped.
- `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings`: passed.
- `cargo test`: 811 passed, 20 explicitly ignored, zero failures. This includes
  the current generated IPC contract, deterministic evaluations, concurrent
  journal acknowledgements and provider ordering/fallback regressions.
- `git diff --check`: passed. Codex, OpenCode and OMP reference trees unchanged.

One intermediate full run failed the existing Beads busy-cleanup test. Its
isolated rerun and the next two full runs passed without changing the test or
the Beads cleanup implementation; the intermittent cause was not established.

Real-provider paired runs, summary-quality comparison and packaged native UAT
are tracked separately in `jarvis-8s54`. No provider account was charged by the
deterministic evaluation. No release was produced as part of this validation.
