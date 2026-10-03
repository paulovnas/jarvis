# Settings workspace and unified terminals

Implemented for Beads `jarvis-ytg` on 2026-09-08.

## Behavior

- The bottom panel has one terminal list. Services started by `process_start` appear beside manually opened shells, with the same live output, keyboard input, renaming and confirmed closure.
- Existing process tool names remain compatible. Their identifiers point to the shared terminal registry. Port checks, duplicate prevention, scoped access, output limits and process-tree cleanup remain enforced.
- Exited and failed services remain available as terminal tabs until the user closes them. Collapsing the panel or changing conversations preserves the stored panel size, open state and selected tab.
- Settings use a stable, spacious window with a fixed side navigation and independently scrolling content. Simple forms have a bounded width; Workflow and other collections use the available space. The existing selected-section preference is preserved.
- The agent editor separates identity and permissions from instructions and model selection. Its header and actions remain outside the scrolling form. In narrow windows it becomes one column.
- Agent and flow icon buttons are fixed 32-pixel squares with accessible names, selection states and semantic colors.

## Architecture

The Metis settings selector and child-process lifecycle were reviewed before implementation. Jarvis keeps its native PTY implementation and adapts legacy process tools onto that registry instead of maintaining a separate stdout-only process manager.

The Windows PTY master is released after the child exits so final output can drain before exit status is published. Process-tree termination is guarded against repeated kills when an inactive tab is later removed.

The settings navigation composes the existing shadcn tab parts with the Base UI root, forwarding its vertical orientation for matching keyboard navigation. Generated registry files were not modified.

## Validation

- `CI=1 bun run check`: lint, strict type checks, 420 frontend tests and production build passed. Three existing skipped tests remain.
- `cargo clippy -- -D warnings`: passed.
- `cargo test -- --test-threads=1 --quiet`: 405 passed, with 16 existing ignored tests.
- `git diff --check`: passed. The existing Windows line-ending notices do not indicate whitespace errors.

The native Windows linker continues to print its existing informational library-generation warning, tracked separately by `jarvis-9au`. Clippy reported no warnings.

The shared-service regression test starts a real PTY service, finds it through the user terminal registry, sends keyboard input, checks the resulting output and exit status, and verifies scope and tool-call idempotency. Existing port, duplicate, bounded-output and descendant-cleanup tests remain covered. Frontend tests cover one tab strip, service input, logs, closure, chat isolation and vertical settings keyboard navigation.

An initial full-suite Kanban timing failure passed in isolation and on the final complete run. The flow icon test now awaits asynchronous menu opening before checking the selected icon.

Visual review used the actual settings and editor components with fixture data, at 1280×720, 1024×480 and 600×720. All 16 icons measured 32×32 pixels, and the save action remained in the viewport. Navigation and dialogs stayed within the viewport after resizing.

Native execution was tested on Windows. macOS and Linux still require device-level smoke testing of the shared PTY lifecycle, tracked by `jarvis-890`.

## Acceptance check

1. Open Settings and switch between General, Workflow, Providers and the other sections. The window should retain its size, with navigation always accessible.
2. Open Workflow → Agents → Add/Edit. Select a color and icon; inspect the compact icon buttons, instructions area and fixed save/cancel actions. Existing built-in agents and flows remain protected.
3. Start a development server through the agent, supplying the project's actual port. Open the terminal panel and select the service tab. Its logs should stream, and interactive commands supported by that service should accept keyboard input.
4. Collapse the panel and visit another chat, then return. The panel should keep its saved open state, size and selected terminal.
5. Close an inactive tab while another is selected. Confirmation should name the requested tab. Confirming closure of a running service should terminate its descendants.
6. Try a duplicate service and an occupied port: no duplicate or alternate-port service should start.

## Windows build

`bun run tauri build --ci --bundles nsis --no-sign` completed successfully. The existing local beta packaging remains unsigned. The installer was generated without running it over the user's installation.

- Executable: `src-tauri/target/release/jarvis.exe`.
- Installer: `src-tauri/target/release/bundle/nsis/Jarvis_0.8.5-beta_x64-setup.exe` (14,923,177 bytes).
- Executable SHA-256: `128221F6903D664D689836B62610F220965D0D6A083E88F438A1FBF37CC81246`.
- Installer SHA-256: `9DB9E20E324750B1A14C0171D18917EF5EF31E5A2626F810AC9D045D63DC9C15`.

## Changed entry points

- Native: `src-tauri/src/agent.rs`, `agent/processes.rs`, `agent/terminals.rs`, `agent/shell.rs`, `agent/workflow.rs` and `agent/workflow/tests.rs`.
- Terminal UI: `src/components/chat/TerminalWorkspace.tsx`, its tests, and `src/core/terminals.ts`. The obsolete `ManagedProcessesPanel.tsx` and `src/core/processes.ts` were removed.
- Settings UI: `src/components/settings/SettingsDialog.tsx`, its tests, `workflow/CustomAgentEditor.tsx`, `workflow/AppearancePicker.tsx` and `src/index.css`.
- Test stabilization: `src/components/chat/FlowPicker.test.tsx` awaits menu opening.

No commit or push was made for this change.
