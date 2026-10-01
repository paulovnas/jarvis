# Jarvis desktop companion — Coucou research

Research date: 2026-10-01. The optional floating companion is implemented for
macOS and Windows; the broader recommendations below describe future possibilities.
Coucou reference: `docs/coucou`, commit `835421c7fff260f0f0be48927591b96bfad81cad`.
Jarvis reference: `c9f8df7`, using Tauri 2.11.5, Wry 0.55.1 and Tao 0.35.3.

## Recommendation

Add an optional companion window to the existing Jarvis application. Its compact
robot and expanding dark island would show ongoing work, required interaction,
recent completion/errors and available provider usage. It should observe the same
execution state as the main application, rather than run another agent or maintain
its own provider connections.

The companion remains available while Jarvis is minimized or another application
is foreground. Keeping it after closing the main window is a separate lifecycle
choice: Jarvis currently exits on close. That behavior would require a user option
to continue in the background, a tray/menu-bar entry and an explicit quit action.

## What Coucou actually implements

Coucou uses SwiftUI/AppKit with a borderless `NSPanel` on macOS, and Rust/Tauri 2
with a transparent, always-on-top window on Windows. Windows therefore provides
a directly relevant architectural reference, while the Mac notch behavior is not
a portable web component.

Its presentation state machine distinguishes hidden, compact, expanded home and greeting
states. Mouse entry, activity and intentional clicks drive expansion. The pet is
procedurally drawn with Canvas and spring motion; it is not a looping video or a
required Rive/Lottie dependency. Windows stops drawing when hidden and parks its
native cursor worker on a condition variable when inactive.

Windows also handles physical/logical coordinates and display scale, applies
non-activating window styles, temporarily permits focus for text input, and
manages click-through regions. These behaviors matter more than simply setting
`alwaysOnTop=true`: an invisible panel must not intercept the user's desktop.

The Claude integration is an external observer using hooks plus a Unix socket or
Windows named pipe. That bridge is unnecessary for Jarvis-managed execution.
Coucou also collapses Claude sessions into one integration task and infers some
questions from text punctuation. Jarvis already has project/conversation/run IDs
and typed pending interactions, which should remain the source of truth.

## Proposed experience

| Presentation | Purpose |
| --- | --- |
| Compact robot | Quiet presence, active-work count and a small attention indicator. |
| Expanded capsule | Project/chat, current agent, short activity and active elapsed time. |
| Expanded panel | Ongoing chats and subagents, recent results, required questions and provider usage. |

The window should expand on hover or intentional click, without automatically
focusing another application. A question or error may gently highlight it; typing
or selecting an explicit navigation action may request focus. Several chats must
remain distinct, with pending interaction prioritized over routine progress.

Questions can reuse the existing response paths and clear immediately after an
answer. Manual approvals remain conditional on the chat's current behavior; the
companion must not add new approvals or interrupt YOLO execution. Detailed
publication or other complex approvals can lead to the existing conversation UI.

Task summaries should use the current task title, agent role, tool activity and
existing workflow summary. A passive glance should not make an additional model
request. Reconnecting is a separate state from working, waiting and failure.
Elapsed time must reuse `durationMs + activeSince`, freezing when active time
stops. Completion must respect pending work and queued messages.

Support a screen-edge island and a movable floating placement as presentations
of the same companion. Preferences can cover enablement, display/position,
visibility while idle, size, sound and reduced motion. Reposition into a visible
work area when a display is removed or its scale changes. Provider limits must
show their account alias, data age and unavailable state when the provider does
not supply them.

The original robot can follow the supplied visual reference: a compact graphite
head, dark visor, cyan eyes and restrained blue/cyan illumination. Simplify the
details so expressions remain readable at small sizes. Idle, working, waiting,
success and error expressions should be original and tied to real runtime state.
Use Jarvis design tokens and shadcn controls for the expanded interface; a Canvas
or SVG character is a specialized visual rather than a replacement UI toolkit.

## Reuse and required changes in Jarvis

- `agent/events.rs` already publishes revisioned `agent:event` updates from Rust.
  `agent.rs` holds sessions and active-turn state independently of the selected UI.
- `agent/workflow/commands.rs` exposes subagents, summaries, statuses, pending
  interactions and active durations. `agent/tasks.rs` exposes current tasks.
- `agent/desktop_events.rs`, `system.rs` and `system/unread.rs` already classify
  attention, completion, errors and unread activity. Reuse that semantics.
- A small Rust projection should provide an initial compact snapshot and changes
  to a dedicated companion entry point. It should not mount the entire main App,
  run onboarding/bootstrap again, or stream whole transcripts into a second view.
- `get_chat`/`subscribe_chat` can call `resume_recovered_turn(PassiveOpen)` and start
  execution. Monitoring must read runtime state or non-resuming persisted data;
  merely displaying the companion must never resume an interrupted task.
- Existing response/navigation actions are reusable, but navigation also needs to
  notify, restore and show the main window when requested from the companion.
- `lib.rs:38-48` currently restricts all application commands to the `main`
  webview. Add an explicit companion command allowlist and its own capability;
  preserve the exclusion of browser tabs from application commands.
- Usage caches already exist, but periodic refresh currently belongs to
  `use-provider-usage.ts`. Background usage needs one shared native refresh path,
  or initially cached data with explicit refresh. Mounting a second polling hook
  would duplicate network/process activity and depend on renderer throttling.

The companion renderer must be disposable: closing, disabling or recreating it
must not cancel a task, require permission again or change execution state.
Invisible animation work should stop; model/network refresh should not be tied
to animation frames. Visible counters can tick locally while a task is active.

## Platform evidence and limits

| Environment | Finding |
| --- | --- |
| macOS | Tauri supports a transparent auxiliary window; the pinned 2.11.5 stack requires its macOS private-API configuration for transparency. Native panel/safe-area behavior should be evaluated for notch positioning and fullscreen Spaces. Focus restoration needs a native smoke test. |
| Windows | Coucou demonstrates the Tauri/Win32 pattern, including activation and DPI handling. Always-on-top does not itself guarantee visibility across all virtual desktops; Tauri documents all-workspaces visibility as unsupported on Windows. |
| Linux | A floating window is a reasonable baseline, but placement and stacking depend on the compositor/window manager. GTK explicitly treats move and keep-above as requests that may be ignored. In particular, Wayland behavior needs environment-specific validation rather than a promise of notch parity. |

The research phase did not perform a cross-platform runtime test. Validate
focus while typing in another app, genuine
pending interaction, answered-question clearing, multiple simultaneous chats,
active-time clocks, reconnecting, missed-event recovery, monitor removal/DPI,
fullscreen/virtual desktops and idle CPU. Test renderer failure without changing
an ongoing agent's execution.

## Implemented scope

- One disposable Tauri window, separate lazy renderer and original SVG robot.
  Opt-in under General settings, unavailable on Linux.
- Saved physical robot anchor, monitor work-area fitting and scale-aware expansion
  toward the available space; compact dragging does not navigate or open the panel.
- Passive loaded-state projection with bounded recent root summaries, active
  durations, reconnecting, subagents, typed questions and pending manual validation.
- Question interaction pauses the existing automatic-answer timer. Complex or
  visual approvals navigate to the main conversation only on an explicit action.
- Shared provider quota caches refreshed natively while enabled, including
  unavailable/error states. No second agent or provider credentials are created.
- Restricted local navigation and a companion-only IPC allowlist; browser views
  remain unable to call application commands.
- macOS nonactivating display uses AppKit ordering. Windows virtual desktops and
  macOS fullscreen Spaces remain subject to native window-manager behavior.
- Closing the main application still quits. A background tray mode, anchored
  hardware-notch presentation and sound are outside this implementation.

## Licensing

Coucou's source is MIT. Its separate `LICENSE-ASSETS.md` reserves the names,
Mochi character/design/expressions/animations, icons, sounds and demo/design
media. Our robot, expressions, icon and sounds must be original. If any permitted
source is adapted, preserve the required copyright/license notices; architectural
reference alone does not require shipping Coucou itself.

## Sources

- Coucou [README](https://github.com/Louis-CFM/coucou/blob/835421c7fff260f0f0be48927591b96bfad81cad/README.md), [asset license](https://github.com/Louis-CFM/coucou/blob/835421c7fff260f0f0be48927591b96bfad81cad/LICENSE-ASSETS.md) and [Windows island implementation](https://github.com/Louis-CFM/coucou/blob/835421c7fff260f0f0be48927591b96bfad81cad/windows/src-tauri/src/island.rs).
- Local Coucou presentation/animation: `windows/src/island/fsm.ts`,
  `windows/src/island/island.ts`, `windows/src/island/hooks.ts`; Mac hook handling:
  `NotchBuddy/Sources/App/IslandWindowController.swift`,
  `NotchBuddy/Sources/App/IslandStateMachine.swift` and the Mac `HookServer.swift` implementation.
- Jarvis: `src-tauri/src/agent.rs`, `agent/events.rs`, `agent/desktop_events.rs`,
  `agent/workflow/commands.rs`, `system.rs`, `system/unread.rs`, `lib.rs`,
  `src/hooks/use-provider-usage.ts` and `src/hooks/use-running-clock.ts`.
- Codex `docs/codex/codex-rs/app-server/src/thread_status.rs`: backend-owned
  status, subscriptions and distinction between active work and required input.
- OpenCode `docs/opencode/packages/opencode/src/session/status.ts` and
  `packages/app/src/context/notification.tsx`: event-driven status/notifications.
- OMP `docs/omp/packages/coding-agent/src/modes/components/status-line/segments.ts`:
  compact roster, model and timer derived from existing session state.
- [Tauri window API](https://docs.rs/tauri/2.11.5/tauri/window/struct.Window.html),
  [capabilities](https://v2.tauri.app/security/capabilities/), and the pinned local
  `tauri-utils-2.9.3/src/config.rs` transparency documentation.
- GTK [move](https://docs.gtk.org/gtk3/method.Window.move.html) and
  [keep above](https://docs.gtk.org/gtk3/method.Window.set_keep_above.html) contracts.
