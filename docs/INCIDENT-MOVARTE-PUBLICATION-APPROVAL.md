# Movarte publication approval incident

Date: 2026-09-25. Bead: `jarvis-pt33`.

## Confirmed causes

Read-only inspection of the Movarte chats **Ajuste na modal de pré-análise** and
**Publicação e merge em homologação** showed the same failure. The GitHub agent
submitted `jarvis_propose_publication` with `previewOnly: false`, current-request
authorization and `confirmedProposalId: ""`.

The runtime treated the empty string as a previous preview receipt ID. It returned
`publication_confirmation_stale` before registering the native approval. The agent
then fell back to a conversational preview and asked the user to type “pode fazer”.
The blocked task was a consequence of that unnecessary confirmation detour.

The second request also exposed a separate validation gap: “Suba tudo…” was not
recognized as requesting commit and normal push. Fixing only the empty ID would
have exposed `invalid_publication_authorization` for the same request.

The first chat later encountered a genuine GitHub merge conflict after its commit
and push succeeded. That conflict is distinct from the approval defect. The second
chat completed both publications after the extra user confirmation.

In the first chat's subsequent Builder turn, the user explicitly requested conflict
recovery and answered the native question choosing to preserve the existing PR.
The next proposal contained only a local `hml` fast-forward synchronization, with
`authorization: null` and the same empty confirmation ID. It failed before native
review for the identical reason, then fell back to “pode fazer” again. This was not
a missing user decision; it was the shared publication preparation defect.

## Correction

- Missing, null and whitespace-only confirmation IDs use the normal publication
  path. The schema accepts null and retains compatibility with empty placeholders.
- A nonempty ID still requires a matching, recent, unused receipt from the preceding
  turn, with unchanged operations and repository state.
- Ordinary publication requests such as “suba tudo” and “publique as alterações”
  cover commit and normal push. PR, merge, reset, synchronization, force push and
  branch deletion retain their separate scope requirements.
- Explicit requests retain the native drawer. Automatic execution still requires
  a current verbatim authorization that explicitly grants autonomy.
- Tool instructions reserve conversational previews for user-requested previews.
  Validation recovery corrects and resubmits the proposal instead of requesting a
  typed confirmation.

The existing approval channel is reused. This follows the runtime-owned permission
decision and pending-approval behavior studied in Codex's tool approval/sandboxing
modules and OpenCode's permission service.

## Regression evidence

Before the correction, the two new tests reproduced both production error codes.
After the correction, the focused publication suite passed all 41 tests.

The runtime test submits both observed publication request shapes and the Builder's
conflict-recovery request against a disposable local repository, using empty,
whitespace-only, null and absent IDs. It covers both explicit authorization and
the null-authorization local synchronization. It observes native pending approval,
confirms HEAD and working/index state remain unchanged, then rejects through the
normal approval channel. The authorization test covers the complete two-repository
commit/push/PR/merge scope and rejects unrequested additional operations and autonomy
without an explicit grant.

Existing tests cover stale/replayed confirmations, changed repository state,
approval with an observation, partial publication and real merge conflicts.

Final gates passed: `bun run check` (703 tests passed, one skipped, including lint,
typecheck, build and IPC check), `cargo fmt --check`,
`cargo clippy --all-targets -- -D warnings`, `cargo test` (854 passed, 21 ignored)
and `git diff --check`. The Rust suite includes the latest Builder regression.

These are automated runtime tests; the actual provider and desktop interaction
remain to be verified after installing the next build. The live Jarvis sessions
and Movarte repositories were not modified.

## Full conversation follow-up

Bead: `jarvis-mbxw`. This follow-up supersedes the conversational-preview behavior
described above. These fixes ship in `v1.5.4`; `v1.5.3` contains only the initial
correction documented in the preceding sections.

The complete journal for **Ajuste na modal de pré-análise** contains 11 completed
turns in the inspected snapshot. The initial design implementation took 9m53s and
33 tool calls. The following 10 publication/recovery turns accumulated **27m05s of
recorded runtime and 79 tool calls**, spanning roughly one hour with user replies.
Runtime includes provider/tool work; it is not a measurement of reasoning alone.

Those turns made 19 publication-tool calls: eight conversational previews, seven
validation errors and four execution attempts, two of which ended partially.

| Local time (America/Sao_Paulo) | Observed outcome |
| --- | --- |
| 12:02 | Commit/push/PR/merge requested; empty confirmation ID rejected before native review. |
| 12:08 | Commit `142d77f` and push succeeded. PR #141 had a real merge conflict with `hml`. |
| 12:20 | User chose to preserve the branch and PR. Two read-only inspections were blocked by task bookkeeping; local fast-forward was rejected for the empty ID. |
| 12:46, 12:48 | Natural instructions to proceed were rejected because they did not exactly match an affirmative phrase whitelist; another preview followed each rejection. |
| 12:50 | Exact affirmative accepted; `hml` fast-forward succeeded. The next proposal failed and became another preview, now for branch selection alone. |
| 12:52 | Branch selection succeeded. Agent restored workflow files to `hml` content and proposed reset/commit/force-with-lease/PR/merge. |
| 12:56 | Soft reset succeeded, but commit stopped with `publication_staged_scope`. Another preview followed. |
| 12:59 | A clear natural instruction to publish failed the same phrase validator. |
| 13:01 | Even the requested exact phrase failed because the receipt was no longer in the immediately preceding turn. Another preview followed. |

The real PR conflict explains why recovery was needed. It does not explain or
justify the repeated confirmation loops. The user had already selected the
recovery strategy: retain the existing branch/PR, use `hml` as the base, reapply
only the two modal files and update the branch using force-with-lease.

### Additional runtime defects

**Literal reply loop.** Receipt validity and user-message interpretation shared
one failure path. Extra wording, observations or a refusal looked like an expired
receipt. The model was then offered another conversational preview, multiplying
confirmation turns. Later, an intervening failed reply invalidated the preceding
receipt even when the user finally sent the exact phrase requested.

**Incorrect stage equality after reset.** The proposal correctly listed five
affected paths: two modal files and three workflow files. Restoring the workflow
files to the target branch made those three paths unchanged after the soft reset.
The resulting stage contained only the two intended modal files. The runtime
required exact equality with all five approved paths and failed *after moving
HEAD* from `142d77f` to `9883567`. This was a Jarvis validation bug, not an
unauthorized file or a GitHub failure.

**Read-only task preflight.** The bookkeeping exemption only understood simple
unquoted Git commands. It treated `ctx_batch_execute` and compound Git/gh
inspection as changes, including branch listing and merge-base inspection. This
forced additional `update_tasks` recovery before gathering the evidence needed
to plan the recovery itself.

### Follow-up corrections

- Proposals needing review use the native drawer. `previewOnly=true` forces
  native review instead of creating a conversational receipt. No exact typed
  phrase is requested. Legacy receipts remain readable for existing histories;
  explicitly authorized automatic execution retains its scope validation.
- Valid legacy receipts with fuller replies return `revision_requested` with the
  user's complete message as guidance, without executing or marking the tool as
  failed. Observations and withdrawals must be interpreted before proceeding.
- Local branch selection/creation and fast-forward synchronization follow the
  turn's existing approval policy. Reset, rebase, commit, push, PR and merge
  retain scoped authorization or native review.
- After staging, approved paths that are now unchanged may be absent. Unapproved
  staged paths remain forbidden, and an empty stage does not create an empty
  commit. Confirmed reset results remain available in the operation result.
- Read-only compound inspections use the existing command parser, including
  batches whose every command is read-only. This only changes task bookkeeping;
  execution, publication and sandbox permissions still apply independently.

Disposable Git repositories reproduce the five-approved/two-changed reset case,
complete cancellation of a proposed diff, and refusal of outside-scope staged
changes. Runtime tests also cover native review, forced review, observations and
local preparation. The Movarte project has `ask_pr_merge` configured: normal
proposals retain that question when the user has not made the choice; an explicit
native-review proposal presents the complete operations in the drawer and never
executes automatically.

Final follow-up validation passed: `bun run check` (703 frontend tests passed,
one skipped, plus lint, typecheck, build and IPC contract validation),
`cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`
(862 passed, 21 ignored), and `git diff --check`. No warnings were emitted.
The reset regressions first failed with the production `publication_staged_scope`
error and then passed with the corrected implementation. Live provider/desktop
behavior still requires testing with the updated build. During the investigation,
no Movarte repository, live conversation, installed application or remote release
was modified.
