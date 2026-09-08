# Terminal layout persistence and provider reauthorization

Implemented for Beads `jarvis-c7c` and `jarvis-1i6` on 2026-09-07.

## Terminal panel

The panel below the chat now saves its open state, height proportion and selected terminal separately for each conversation. Its width follows the chat column, whose dimensions already use desktop preferences. Navigating away and returning restores the panel without stopping its shells. Collapsing it retains its chosen height.

The existing desktop preference store persists `terminalPanels` in `.jarvis/desktop.json`. Older preference files remain readable through defaults. Rust validates dimensions between 20% and 65%. Only user resize events update the saved height; initial layout and visibility changes cannot overwrite it. The frontend uses the existing ordered saves in `DesktopLayoutProvider`.

The saved panel state also survives restarting the application. Native terminal processes still follow the application's existing lifecycle; a saved terminal selection falls back to an available terminal when the previous session no longer exists.

Main files: `src/core/desktop-layout.ts`, `src/components/chat/TerminalWorkspace.tsx`, and `src-tauri/src/desktop.rs`, with regression tests alongside the components and Rust store.

## Reauthorize a provider

Open **Configurações → Provedores**, select an OpenAI Codex or Antigravity card, then click **Re-autorizar** inside its details. Complete the login in the browser. The account list and model catalog reload after success, and the native usage cache is invalidated.

The alias, original creation date, activation status and usage display settings are preserved. Custom providers continue to use their API-key editing form. The new OAuth flow stores fresh credentials only after authentication succeeds, serializing the replacement with existing credential refresh and disconnection operations. The authenticated account identifier may change, provided it is not already assigned to another alias.

Cancellation and authentication failures retain the previous credentials and offer a retry for the same alias. Missing credentials can be repaired. Database and secure-storage errors roll back the change; a flow started before the target is removed or replaced cannot recreate it.

Main files: `src/components/settings/ProviderAccountCard.tsx`, `SettingsDialog.tsx`, `src-tauri/src/openai_codex.rs`, `openai_codex/reauthorization.rs`, `openai_codex/usage.rs`, and command registration in `src-tauri/src/lib.rs`.

The implementation follows the read-only metis references for persisted layout and successful-login credential replacement, and reuses the existing shadcn components and OAuth flow.

## Validation

| Check | Result |
| --- | --- |
| `bun run lint` | Passed with zero warnings |
| `bun run typecheck` | Passed |
| `bun run test --maxWorkers=1` | 70 files, 366 passed, 3 skipped |
| `bun run build` | Passed with zero build warnings |
| `cargo clippy -- -D warnings` | Passed |
| `cargo test -- --test-threads=2` | 359 passed, 14 ignored |
| Edge UI check, 1100×820 and 560×620 | Passed, no page errors |

The browser check used actual components, styles and xterm with mocked Tauri data. Dragging saved a panel size of 54.426%; navigation and page reload restored the same 417 px panel and selected terminal. Provider details exposed the new action, completed the simulated login and retained settings. Rust tests exercised successful and failed token exchanges against a local fake issuer, cancellation, duplicate identities, missing credentials, and rollback after a failed database commit. No live provider account was reauthorized during automated validation.

The localized MSVC linker continues to emit its existing informational import-library creation warning during `cargo test`. It is tracked separately in `jarvis-9au`; no diagnostics were suppressed. A native macOS runtime was not available for execution.

Restart the development app with the rebuilt Rust backend before using the new authorization command. Existing uncommitted work was preserved; no commit, push or remote Beads sync was performed.
