# Jarvis macOS computer use: implementation evidence

Date: 2026-10-12. Initial tracking: `jarvis-vp9q` and its implementation children;
policy and UX revision: `jarvis-bp8c`.
This is an independent Jarvis implementation using public macOS APIs. The static
ChatGPT investigation is design evidence, not a runtime dependency or proof of
behavioral parity.

## What the implementation reaches

| Boundary | Relevant implementation | Observable behavior |
| --- | --- | --- |
| Native applications | `src-tauri/src/agent/computer/macos.rs`: `apps`, `running_target_app`, `current_window` | Running bundled apps and windows, canonical bundle/executable paths, bundle ID, PID, launch identity and window ownership. |
| Observation | `macos.rs`: `observe`, `capture`, `capture_geometry` | Bounded AX nodes and a selected-window PNG; image dimensions, logical window bounds and separate X/Y scales. Partial AX preserves useful nodes and capture. |
| Native input | `macos.rs`: `act`, `input_check`, `hit_check`, `insert_text` | Element or pixel click, Unicode text, key combinations, scroll and drag. AX actions are preferred; synthetic events and releases go to the validated PID through public `CGEventPostToPid`. |
| Host state | `computer.rs`: `OperationGuard`; `computer/controller.rs`: `run`, `store_observation`, `confirmed`, `reconciled` | Conversation/run ownership, generations, fresh observation references, serialized native operations, cancellation and structured receipts. |
| Authorization | `system.rs`: `computer_use_mode`; `computer/controller.rs`: `app_authorized`; `agent/execution_grants.rs` | Persistent Manual/YOLO policy shared by chats. Manual uses canonical per-conversation app grants; YOLO permits every app, including Jarvis, without grants or action review. |
| Agent policy | `agent/tool_contract.rs`, `agent.rs`: `authorize_declared`; `agent/claude_executor/bridge.rs`; `computer/tools.rs` | Both executors use the selected computer mode. Manual requires one-action human review even when other tools use YOLO; computer YOLO preauthorizes native input. Plan mode still exposes observation, not control. |
| Recovery | `agent/workflow/recovery.rs`: `Call::related`, `record_computer_result`, `resolve_recovery_with`; both executor result boundaries | Durable receipts are preserved. Same-target reads support reconciliation; input remains blocked across workers until success is durably acknowledged or a validated resolution is saved. |
| Images | `computer/controller.rs`, existing `agent/attachments.rs` and `agent/vision.rs` | Captures become ordinary conversation image attachments. `vision` consumes their IDs through the existing provider/model configuration. No new image/provider service. |
| Desktop UI | `ComputerUseSettings.tsx`, `ComputerUseSession.tsx`, `SettingsDialog.tsx`, `use-computer-use.ts` | A discreet composer icon, policy selector, direct permission setup shortcut, live activity and stop. Manual additionally displays app grants and one-action review. |
| Agent cursor | `computer/cursor.rs`; `macos.rs`: `act`, `indicate_element` | Click-through, nonactivating AppKit overlay showing a teal pointer and Jarvis label at the input location without moving the physical mouse. |
| IPC registration | `computer/commands.rs`, `src-tauri/src/lib.rs` | Native status, inventory, permission, mode, grant/revoke and stop commands backed by managed state. |

The exposed tool family is `computer_status`, `computer_list_apps`,
`computer_select`, `computer_observe`, `computer_click`, `computer_type`,
`computer_press`, `computer_scroll`, `computer_drag`, and `computer_stop`.
The existing `recovery_resolve` handles reconciliation.

In Manual mode, app authorization is an explicit UI action. The agent cannot manufacture a grant
by supplying a conversation ID or changing an approval request. The host supplies
conversation and run identity. Selection requires a fresh inventory identity;
control selection additionally requires the corresponding native tool permission.
Jarvis's own windows can be selected and controlled under the same policy as other
apps. The earlier own-process restriction has been removed.

The mode is saved in system preferences and loaded at startup. Changing it cancels
pending native work and invalidates observations; uncertain-input receipts remain
blocked until reconciled. YOLO changes Jarvis authorization, not macOS TCC consent.
The agent receives the effective mode and OS permission state before inference and
must explain missing access before attempting control. Native preflight returns a
structured setup error before input, with a direct shortcut to the Computer Use
section. Returning to Jarvis refreshes the permission state.

Explicit permission buttons also open the relevant macOS privacy pane when access
is still missing, including previously denied requests. These links follow the
[Chromium system settings implementation](https://chromium.googlesource.com/chromium/src.git/+/refs/tags/144.0.7559.228/base/mac/mac_util.mm#507).
The cursor uses logical Quartz coordinates converted to AppKit screen coordinates;
it appears for semantic/pixel clicks, typing, key input, scrolling and drag steps,
and hides on stop, mode changes or eight seconds without another indication.

## Observation, action and recovery contract

An action refers to the latest observation of the selected target. Element IDs
belong to that observation. Pixel coordinates refer to its actual PNG dimensions,
then map into top-left Quartz screen coordinates using the observed logical
window bounds and scales. A moved/resized window, restarted app, wrong owner,
expired observation, protected field or out-of-image point is refused before input.

Receipts distinguish `prepared`, `inputSent`, `observed`, `uncertain`, `error` and
`stopped` activity. An observed state is evidence to inspect; it is not an automatic
assertion that the intended effect happened. Screen text and AX data are marked
untrusted. The model must compare the new evidence before deciding its next step.

Cancellation and first dispatch share an ordered guard boundary. A blocking native
worker retains the operation lock after its caller times out; successful workers
retain it until the caller publishes their result. Cancellation, timeout or error
after dispatch returns `computer_action_uncertain` with `inputSent: true`. There is
no automatic replay. Stop prevents following input and does not undo previous input.

Before dispatch, the target records a pending call/operation marker. Dropping the
caller also cancels its native worker; dispatched input remains blocked even if the
caller never saves a result. Both executors acknowledge an observed input only after
their result is durable. Exact call/operation matching prevents an old ACK from
unlocking a newer input. Resume can safely acknowledge already-preserved successes;
it never resends them. A worker that exits before any dispatch clears only its own
pending marker.

An uncertain target keeps the same ID when reselected with a different access
level. Another worker's observation does not unlock it. `recovery_resolve` unlocks
only after validating same-target durable evidence and saving the outcome; replaying
an older resolution cannot unlock a later incident. Reconstruction preserves
previously confirmed resolution receipts. Internal native call keys include the
stable host executor ID, so identical provider call IDs in different workers cannot
acknowledge or reconcile each other's input. Journal/model IDs remain unchanged;
the key survives a run restart and supports recovery when a receipt was lost.

Authoritative fresh instance/window disappearance produces a successful read-only
`computer_target_unavailable` receipt. After a cold restart, a missing host binding
also produces an explicit `host_binding_missing` receipt with `effect: unknown`;
it proves only that the old binding is unavailable, not that the physical app or
input effect disappeared. Old references cannot send input. New control requires
fresh observation and, in Manual mode, UI authorization. Metadata failures, malformed or truncated
inventory, geometry changes and AX ambiguity do not prove disappearance.
The `unavailable` recovery outcome preserves an unconfirmed prior effect and retires
that old input; it does not claim `applied` or `not_applied`.

Payloads and native work are bounded: a 12-second operation deadline, 60-second
observation lifetime, 16 native snapshots, 400 AX nodes with bounded depth/children,
25-million-pixel capture budget, existing 20 MB attachment limit, 8 KB UTF-8 input
text limit and bounded key/scroll/coordinate schemas.

## Research, documented APIs and implementation decisions

The REA reports confirm static references/contracts for combined accessibility,
screenshots, element/coordinate actions, target identity, authorization and
post-action observation. See [native-static.md](native-static.md),
[cua-protocol.md](cua-protocol.md), [javascript-flow.md](javascript-flow.md) and
[native-call-evidence.json](native-call-evidence.json). Partial decompilation does
not establish runtime timing, background control parity or all native decisions.
The seven remaining questions in `jarvis-mc9s` remain research work.

The reference comparison and source map are in
[jarvis-reference-map.md](jarvis-reference-map.md). Codex informed host-owned target
identity, exact originating-action approval and cancellation boundaries; OpenCode
informed abort/session cleanup and reuse of managed attachments; OMP informed
bounded frames, coordinate conversion and snapshot-scoped references. No source
was copied textually or changed in the three reference trees.

Jarvis deliberately uses public NSWorkspace/NSRunningApplication, AXUIElement,
CGWindow inventory, ScreenCaptureKit and PID-targeted CGEvents. It does not load
ChatGPT code/services, private SkyLight symbols or private AX window-ID helpers.

Relevant public API documentation:

- [NSRunningApplication](https://developer.apple.com/documentation/appkit/nsrunningapplication)
- [AXUIElement](https://developer.apple.com/documentation/applicationservices/axuielement)
- [SCScreenshotManager](https://developer.apple.com/documentation/screencapturekit/scscreenshotmanager)
- [CGEvent](https://developer.apple.com/documentation/coregraphics/cgevent)

These are documented platform contracts. Their existence does not prove a Jarvis
runtime test passed. Runtime observations are recorded separately below.

## Initial implementation validation record

The following historical record precedes the Manual/YOLO and cursor revision.

Frontend gates completed in the required order: `bun run lint`, `bun run typecheck`,
`bun run test`, `bun run build`. Result: 255 test files and 2,100 tests passed;
3 files/10 tests were skipped by their existing environment conditions. The final
lint/typecheck/build streams exited zero and emitted no warnings.

`cargo clippy --all-targets -- -D warnings` completed successfully with no warnings.
The final integrated `cargo test -- --test-threads=2` passed 2,056 tests, with
41 explicitly ignored tests and no failures. It includes
`generated_contracts_are_current`, the existing IPC contract gate. Runtime
inspection found that the separate Settings window's command allowlist omitted
`get_computer_use_status` and `request_computer_permission`. These two commands
were added for Settings, with a regression test keeping app grants and control
restricted to the main window. The corrected bundle was rebuilt and opened;
Settings now renders the three actual permission states without the IPC error.
Cold-restart coverage passed. A revoked, removed binding also returns unavailable
metadata; its test explicitly requires no observation, attachment or input.

Behavioral tests cover canonical grants, cross-conversation isolation, catalog and
approval policy, stale instances/geometry/references, Retina/negative screen origin,
pre/post-dispatch cancellation, late observations, uncertain receipts, shared
worker blocking, disappeared-target evidence, durable recovery and image attachment
compatibility, including dropped callers, lost acknowledgements, stale ACKs,
identical provider IDs across real worker journals, resumed executor identity,
and lost host bindings after cold restart.
Frontend tests cover real capability states, authorization, stop,
approval restrictions, inventory/status races and absent capture rendering.

The frontend suite encountered three existing wall-clock timeouts while the
native compiler was rebuilding dependencies. The complete suite was rerun with
`bun run test --maxWorkers=2`, followed by `bun run build`; all 2,100 tests passed.
No timeout limits or assertions were relaxed.

The real smoke fixture is in `src-tauri/tests/fixtures/computer-use/`. It is a small
Cocoa app with dummy text, one confirmation button and local-only receipts. The
ignored `native_disposable_fixture_smoke` test requires explicitly supplied fixture
paths and exact canonical identity; it never targets accounts or documents. It
tests discovery, AX, actual PNG, text, keyboard, semantic/pixel click, moved/stale
geometry and stopped queued input. OS permission failure is a failed smoke,
reported as `SMOKE_BLOCKED`, rather than a passing mock.

The compiled Rust backend's live preflight reported `accessibility: false`,
`screenCapture: true`, `inputPosting: false`, `captureSupported: true`, and
`sessionStatus: "interactive"`. The final smoke therefore verified discovery and
read-only capture before refusing input. It selected the exact canonical fixture,
window ID 2156, logical bounds `{x:400,y:410,width:580,height:332}`, and stored a
real 49,419-byte PNG of 1160 × 664 pixels with both scales equal to 2.0. Attachment
ID in temporary test storage: `71b402d264c5f3b3e98cd108bc4ff719`. With Accessibility absent, its AX element
list was empty; visual observation still worked.

The full smoke exited 101 with `SMOKE_INPUT_BLOCKED` before any native input. The
fixture receipt retained `text: "fixture"`, `clicks: 0`, and `result: "Aguardando
teste"`. This is a successful native observation and attachment assertion, not a
passing full smoke or agent/input cycle. Test-process permissions do not establish
the signed desktop bundle's permissions. Manual native UAT is tracked in
`jarvis-vp9q.5`.

The signed desktop bundle was opened from Downloads (the running executable's
path was verified). A new chat, `Teste de status do sistema`, called the real
`computer_status` tool once with `{}`. Its completed receipt reported
`accessibility: false`, `screenCapture: false`, `inputPosting: false`,
`captureSupported: true`, `supported: true`, `sessionStatus: "interactive"`.
The result appeared in the actual tool activity history, in 0.03 seconds; this
proves status/catalog/agent integration, not native input, capture or unlocked
screen control. The chat's computer panel correctly refused observation pending
OS permission. The actual main-window stop command also completed, and the panel
displayed `Uso do computador interrompido`. This was an idle stop, with no active
native input; stopping physical input mid-dispatch remains manual UAT. No app was
authorized and no input was sent to another app.

The final production bundle was generated with
`env CARGO_BUILD_JOBS=2 bun run tauri build --ci --bundles app` and copied to
`/Users/enesolucoes/Downloads/Jarvis-Uso-do-Computador-2026-10-12/Jarvis.app`.
`codesign --verify --deep --strict --verbose=2` passed on both the source and final
copy, including the CEF helpers/framework. Executable SHA-256 is identical in both:
`ca376fb68d2846275edbbc8c10918455c9c5e3c7617ad2e5cf21929776a3362e`.
The final app is open at Settings → Ferramentas → Uso do computador. Its three
permission rows are all `Não concedida`; no permission was granted by automation.
Its running executable path was verified at PID 24686 in the Downloads bundle.
The signed bundle's permissions, rather than the test executable's permissions,
remain the prerequisite for full native UAT.

Beads implementation children `jarvis-vp9q.1` through `.4` are completed for code,
automated validation, observed status/capture and the final build. Full native
input UAT is open as `jarvis-vp9q.5`; the parent `jarvis-vp9q` remains in progress.

For that UAT, grant the permissions to the final Jarvis bundle and restart it.
Open a new chat, use `Usar computador`, and authorize the disposable
`Jarvis Computer Fixture` app for control. Ask the agent to observe its window,
fill `Entrada fictícia`, click `Confirmar teste`, and inspect the dummy result;
approve each proposed input separately in Manual mode. In YOLO, no per-app grant
or action review is needed. Then exercise moved/stale geometry and
stop during execution. The fixture and its local-only receipt are available at
`/Users/enesolucoes/Library/Caches/JarvisComputerFixture/Fixture.app` and
`/Users/enesolucoes/Library/Caches/JarvisComputerFixture/state.json`.

## Manual/YOLO revision validation

Tracking: `jarvis-bp8c`. Final source passed frontend lint, typecheck, the full
Vitest suite (2,123 passed; 10 existing skips) and production build in the required
order. Clippy passed with `--all-targets -- -D warnings`; the complete Rust suite
passed 2,065 tests, with 41 explicitly ignored tests and no failures or warnings.

Regression coverage includes a fresh YOLO conversation without app grants,
preauthorization in the shared Jarvis/Claude executor path, Manual review,
mode changes invalidating ungranted access, OS setup before native input,
direct settings navigation, missing capture permission, policy refresh in an
existing and new chat, permission refresh on desktop focus, and cursor coordinate
conversion for primary, negative-origin and upper displays. Composer integration
tests exercise the discreet popover and cancellation of both computer work and
the active agent turn.

Production bundling caught a static settings import in the chat dependency graph.
The settings fallback now loads on demand; the settings chunk is 262.17 kB under
its unchanged 500 kB budget. A preexisting jsdom editor test also depended on click
geometry that jsdom does not provide. It now places the caret through the public
composer focus API before typing, preserving all draft/editability assertions.

The previously running signed desktop bundle reported all three macOS permissions
as granted when inspected through its actual Settings UI. This is OS consent for
that bundle; mocks and test-process permission state are not substituted for it.
Native interaction and final artifact observations for this revision follow below.

## Manual/YOLO revision: native observations

The signed development bundle was launched from Downloads, and its running
process path was checked. In a new chat using Claude Code Haiku with medium
reasoning, only the disposable Cocoa fixture was authorized for Manual control.
The agent observed the window, focused its field, selected its old text with
Cmd+A, typed `Cursor Jarvis OK`, clicked `Confirmar teste`, and observed
`Resultado fictício: Cursor Jarvis OK`. Window screenshots showed the text and
result, and the live fixture receipt at `/tmp/jarvis-computer-fixture.json`
reported matching values and exactly one click. The agent finished nine actions.
No other application was authorized or controlled during this test.

The Settings menu exposed both Manual and YOLO, while Manual remained selected.
The composer shortcut opened the Tools tab, but the asynchronously loaded Core
list pushed computer setup downward. Computer Use now precedes Core in that tab;
the regression test checks the first accessible heading and section focus.
The final frontend gates passed after that correction: lint, typecheck, 257 test
files and 2,123 tests, and production build. Three files and ten tests are skipped.
The unchanged Rust source passed Clippy with warnings denied and 2,065 tests;
41 tests are ignored. These results do not count ignored tests as native UAT.

The native input test preceded the final settings-order correction. No Computer
Use controller, input, cursor, or approval code changed after that native test.
YOLO policy and persistence passed automated tests; YOLO was not activated on
this desktop. The separate AppKit cursor is implemented and coordinate mapping
is tested, but its visible arrow and badge were not confirmed: the available
window captures omit overlay panels. Remaining physical cursor, multi-display,
stale-geometry, pixel/drag/scroll, and interruption checks remain tracked in
`jarvis-vp9q.5`.

The final bundle is retained at
`/Users/enesolucoes/Downloads/Jarvis-Computador-YOLO-2026-10-12/Jarvis.app`.
`codesign --verify --deep --strict` passed for both the generated and copied
bundles. The identifier remains `com.foxtag.jarvis`, signed by Apple Development
under Team `9KXT6JDW8H`. Both binaries had SHA-256
`d685c032888fd2fc3f7f7a9b3b72342a97140b8b779d1c7f1d8aa07653d31292`.
This is a local development build; notarization was skipped without release
credentials. The final copied application was opened again. Its composer
shortcut showed Computer Use first in Tools, with Manual selected and all three
macOS permissions visibly granted. This confirms the settings-order fix in the
final native artifact without claiming a second full input UAT run.

## First-version limits

Selected-window capture requires macOS 14+; the app's macOS 13 minimum remains,
with clear capture capability reporting. Other platforms report unsupported.
Foreground app/window checks are mandatory for input. A validated AX window root
is required even for coordinate input; no root means visual observation only with
`computer_input_unavailable`, while a partial element tree can still use supported
coordinate/keyboard actions. Protected fields reject typing.

Public APIs do not provide a reliable unlocked-screen indicator. `interactive`
reports documented GUI-session flags, not a claim that a lock test passed.
Locked-session control, privileged installers, private APIs, background parity
and a generic persistent REPL are outside this delivery. Letter shortcuts currently
use ANSI virtual key mapping; Unicode text and standard named keys are separate.

Native approval verification is local-desktop scoped. A remote client's missing
native adapter fails closed; remote computer control is not claimed. Multi-monitor
conversion tests do not replace physical UAT on every display arrangement.

No commit, push, merge, deployment or remote Beads synchronization is authorized
or performed for this implementation. Existing unrelated work is preserved.
