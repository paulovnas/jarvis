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
