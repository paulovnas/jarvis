# Movarte: incomplete twelve-hour workflow

Investigation date: 2026-09-25. Tracked in Beads as `jarvis-skpe`.

## Evidence and boundaries

The reported conversation is **Correções na integração Conta Azul**, turn
`4ce7b49a2111310d774e1bc8366bbd8c`. Its final checkpoint records **11h 53m 06s**,
from September 24 at 15:20:49 to September 25 at 03:13:56 (UTC−03).
The request covered backend preflight/corrections plus a frontend dialog.
The final response explicitly left the dialog and integrated review incomplete.

The local journal, workflow manifest and sanitized diagnostics were read without
changing the conversation, credentials, Beads data or source files of Movarte.
Diagnostics identify the incident runtime as **1.4.0 on macOS**. The inspected
Jarvis checkout already contains **1.5.0**, including the Linux work; the defects
below were still present there. Linux qualification is a separate subject.

Counts below use the final checkpoint of each child turn, restricted to this
workflow run. They do not repeatedly count incremental journal snapshots.

| Observation | Recorded value |
| --- | ---: |
| Child jobs, including queued jobs that never ran | 32 |
| Builders / reviewers | 14 / 12 |
| Child tool calls / unsuccessful calls | 3,775 / 100 |
| File reads / searches | 1,767 / 942 |
| Patch executor interruptions / matching diagnostic panics | 18 / 18 |
| Validation commands | 105 |
| Sum of validation command durations | 8m 25s |
| Designer turn durations with provider failures | 45m 04s + 10m 41s |

Builder steps account for approximately 459 minutes and reviewer steps for 68
minutes in aggregate. These totals can overlap and must not be added to elapsed
time. Waiting by parent agents includes child work; it is not evidence of an
idle application. Likewise, queued child durations are not model execution time.
Validation commands were not the dominant source of the twelve-hour duration.

## Confirmed defects and changes

### Missing patch context panicked instead of returning a useful error

`find_unique_sequence` evaluated `matches[0]` eagerly inside `then_some`, even
when no match existed. A stale or incorrect patch therefore crashed the worker.
The outer error conservatively reported uncertain effects and required another
inspection. The prior atomicity test accepted any error, including this panic.

The lookup now accesses the element lazily. The regression test requires the
actual context-mismatch error and unchanged files. A second test found and
fixed a UTF-8 slicing panic for invalid non-ASCII hunk markers. Both tests failed
before the correction and pass afterward. This preserves strict unique matches
and transactional writes rather than guessing where to apply a patch.

### Implementation dependencies were confused with final task closure

Only Reviewer could consume a completed implementation whose Beads task was
still open awaiting review. Designer and regression-test Builders failed at the
same boundary, encouraging intermediate review/closure tasks simply to advance
implementation. The journal contains this exact failure and repeated guidance
to create focused reviews before releasing frontend work.

Explicit dependencies on completed Builder/Designer handoffs now work for
dependent implementation too. The handoff must belong to the same run, identify
the exact prerequisite Bead and be completed. Missing evidence, active/failed
workers, and explicitly blocked Beads still prevent execution. Final closure
retains its independent review requirement.

### A review requesting fixes was treated as a failed prerequisite for those fixes

Review `rework` handoffs settle into the existing `blocked` job status. Admission
previously rejected repair workers depending on these reviews. The incident
includes several repair jobs that failed before their first tool call and were
then recreated without the dependency.

Builder/Designer may now consume an explicit same-run Reviewer `rework`
handoff. An actual failed, cancelled or blocked review remains a blocker, and a
downstream Reviewer cannot treat `rework` as approval.

### The validation wrapper rejected an existing project script

The backend defines `type-check`, while `workflow_check` only recognized
`typecheck`. Five calls failed with that mismatch, and coordination eventually
substituted the evidence from a TypeScript build. The wrapper now resolves the
existing `typecheck`, `type-check` or `type:check` script, preferring the original
name when present. It still rejects a missing script and does not invent one.

### A long turn kept its original OAuth token

Credentials were refreshed when entering a turn, but the retained transport
session reused that token for every later request and retry. A token could
expire while tools, child work or provider reconnections were in progress.
A 401 ended the turn and advised reconnecting the account without attempting
same-account OAuth recovery.

Running Codex/Antigravity turns now renew expiring credentials before requests,
including retries, and before compaction. One HTTP 401 may trigger a same-account
refresh and replay of the inference request. A second 401 or a 403 is terminal.
The existing lock coordinates rotation with quota probes and account removal;
an already refreshed token is reused. Account identity, selected model metadata
and Antigravity endpoint remain fixed. No model catalog request is added to this
renewal path. Completed tool actions are not replayed.

The incident's final 401 is confirmed. Its exact token expiry was not recorded,
so expiry is a supported explanation, not a proven diagnosis of that individual
HTTP response. Earlier Antigravity timeouts are also confirmed; local records do
not establish their upstream cause. A successful short conversation does not
prove that the provider accepted the larger workflow requests.

## Coordination guidance

The shared workflow instructions now distinguish readiness for dependent work
from final approval. They recommend package scopes including regression tests,
reuse of eligible worker/reviewer contexts, consolidated findings and review of
the affected changes instead of repeated unrelated discovery. Explicit user
scope restrictions remain authoritative.

## Reference decisions

- Codex `codex-rs/apply-patch/src/seek_sequence.rs`: unsuccessful context lookup
  is an ordinary result, with explicit boundary handling.
- Codex `codex-rs/core/src/client.rs`: bounded authentication recovery is part of
  the transport lifecycle, preserving request identity and error distinctions.
- OMP `packages/ai/src/auth-retry.ts` and its tests: distinguish expiring tokens,
  same-account renewal and terminal failures. Jarvis does not adopt automatic
  account switching.
- OpenCode `packages/opencode/src/session/retry.ts`: classify retryable failures
  rather than retrying every refusal indefinitely.

Reference trees were not modified and their code was not copied.

## Validation and remaining evidence

Regression checks cover patch mismatch atomicity, invalid UTF-8 markers, existing
script aliases, implementation handoffs before closure, review rework admission,
token renewal/reuse with a local OAuth server, account replacement rejection and
bounded 401/403 inference behavior. All provider fixtures use synthetic data.

Validation on the local macOS checkout passed:

- `bun run check`: lint, TypeScript, 124 test files (686 passed, one skipped),
  production build and generated IPC contract check.
- `cargo fmt --all -- --check`.
- `cargo clippy --all-targets -- -D warnings`.
- `cargo test`: 827 passed, 20 ignored, no failures.

Skipped/ignored tests were not counted as validation. No new application release,
Linux runtime check or live-provider end-to-end run was performed for this fix.
Real-provider qualification remains tracked separately in `jarvis-8s54`.

These fixes remove reproduced sources of wasted work. They do not establish a
new completion-time guarantee, repair the unfinished Movarte frontend, or prove
that Antigravity will sustain the larger request. A new end-to-end run on the
updated build is still needed to measure elapsed time, repeated reads, rework and
provider latency. Never compare summed parent/child durations with wall time.
