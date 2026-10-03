# Docked terminal workspace

The composer terminal button now toggles a bottom panel inside the conversation column. The transcript and composer remain usable while the terminal is open. The panel starts at 40% of the available conversation height; dragging its top separator adjusts the height. Its height, open state and selected terminal are now saved per conversation and restored after navigation or application restart, using the existing desktop preference store. See `docs/terminal-layout-and-reauthorization.md` for the persistence follow-up and its validation.

The panel keeps terminal tabs, creation, rename, confirmed termination, agent-created terminals, and managed process logs. Collapsing the panel disposes only its frontend renderer; the backend shell and its processes continue running. Closing a terminal tab still requires confirmation and selects another remaining terminal.

The `Terminais` / `Processos` section selector uses a filled blue background and border, separate from the shell tabs' underline. The new-terminal button sits directly after the final shell tab in their shared horizontal scroll area. Long tab lists scroll together with the button. Both navigation levels have distinct accessible names and retain keyboard navigation. This refinement is tracked as `jarvis-7k6`; Edge checks covered three tabs at 1000 px and twelve tabs at 420 px, including creation and section changes without page overflow.

## Rendering and platform paths

- `TerminalWorkspace.tsx` owns panel layout and terminal controls; `ChatArea.tsx` places the transcript and composer above it. `ChatComposer.tsx` receives the launcher as a slot.
- `TerminalSurface.tsx` loads lazily. It passes the resolved JetBrains Mono font family to xterm, waits for the font before opening and measuring the terminal, and fits the PTY to container size changes. Font loading failure does not prevent opening the terminal.
- On Windows, `agent/shell.rs` passes a regular Win32 or UNC working directory to the interactive shell. `agent/terminals.rs` uses the existing display-path serializer for terminal metadata. Internal canonical paths remain unchanged for backend checks.
- On macOS, shell selection and native working-directory handling are unchanged. Windows-only shell discovery helpers are excluded from Unix builds.
- The footer shows the project path, exposes the complete path on hover, and keeps the terminal status visible.

The implementation was informed by the read-only metis shell, path and Windows references under `docs/metis/src/utils` and `docs/metis/docs`, and uses the existing shadcn resizable and tab components.

## Validation on Windows

Validation completed on 2026-09-07:

| Check | Result |
| --- | --- |
| `bun run lint` | Passed, no warnings |
| `bun run typecheck` | Passed |
| `bun run test --maxWorkers=1` | 70 files passed; 360 tests passed, 3 skipped |
| `bun run build` | Passed, no build warnings |
| `cargo clippy -- -D warnings` | Passed |
| `cargo test -- --test-threads=2` | 351 tests passed, 14 ignored |
| Edge browser smoke test | Passed at 1100×820 and 560×620 |

The browser smoke test used mocked Tauri data with the actual terminal workspace, composer, xterm and stylesheet. It verified panel placement below the composer, editable chat input, drag resizing, font metrics, PTY resize calls, and height preservation after collapse. Separate tests launched a real PowerShell PTY from a canonical directory containing spaces, accents and brackets, and verified the absence of verbatim/provider prefixes in the prompt. UNC path conversion is also covered. A native macOS runtime was not available for execution.

Initial concurrent suite runs hit MCP and Kanban timing failures; the MCP suite passed with two Rust test threads, and the complete frontend suite passed with one worker. The localized MSVC linker still prints an informational `linker_messages` warning when creating import libraries. This prevents claiming completely warning-free Rust test output and is tracked separately as `jarvis-9au`; no compiler or linker diagnostics were suppressed.

Implementation task: `jarvis-dsd`. Existing uncommitted project changes were preserved. No commit, push or remote Beads sync was performed.

Restart the rebuilt Jarvis backend and create a new terminal to apply the PowerShell working-directory correction. Existing terminal output is not rewritten.
