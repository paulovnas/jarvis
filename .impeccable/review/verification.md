# Native macOS visual redesign verification

Date: 2026-10-12. Repository: `/Users/enesolucoes/Documents/Pessoal/jarvis`.

## Scope and direction

The user requested a radical visual replacement inspired by the supplied native macOS settings application. The first reference is authoritative; the third reference is supplementary. The user explicitly chose a discreet macOS execution indicator instead of the previous rainbow composer border. The implementation preserves the existing Tauri, shadcn/Radix controls, chat transport, provider settings, data, and interaction contracts.

The durable user-supplied critique reference is `.impeccable/references/native-macos-settings.png`. It is a design reference, not a shipping application asset. No raster artwork was created or replaced in the application.

## Automated evidence

The isolated frontend quality sequence is `bun run lint && bun run typecheck && CI=1 bun run test && bun run build`. The final UI source passed with zero errors or warnings, 252 passing test files, 2,070 passing tests, and the existing 10 skipped tests in three preconfigured skipped files. Production Vite and extension builds completed successfully. The final test stage took 250.14 seconds and the main Vite build took 11.52 seconds.

An earlier concurrent native compilation caused three existing 5-second frontend test deadlines to expire. The complete sequence passed in isolation; no deadlines or tests were relaxed. The final source includes the widened sleep mode selector, responsive settings cards, provider tool independence regression and deferred maintenance loading regression.

The manual Impeccable detector was run once against the changed UI and stylesheet sources and returned no findings. Its unmodified JSON result is `detector.json`. No generated UI registry component or reference repository was modified. The startup correction changes `src-tauri/src/desktop.rs` and two files in the tracked vendored CEF runtime.

The final native sequence `cargo clippy --all-targets -- -D warnings && cargo clippy -p tauri-runtime-cef --all-targets -- -D warnings && cargo test && cargo test -p tauri-runtime-cef --lib` exited 0 after the complete CEF correction. Both Clippy stages reported no warnings. Jarvis tests passed with 2,008 passing tests and 39 pre-existing ignored tests, including the normal-bounds persistence regression, in 86.61 seconds. The separately targeted CEF runtime suite passed all 100 tests, including work-area filling, normal-frame restoration, repeated calls and secondary-display cases. Doc-tests and the binary test target also exited successfully.

## Native evidence

The final production `.app` bundle built successfully with `bun run tauri build --bundles app` and was copied to `/Users/enesolucoes/Downloads/Jarvis-macOS-2026-10-12/Jarvis.app`. The running executable was verified at that exact Downloads path. Both the source bundle and the fresh Downloads copy passed `codesign --verify --deep --strict`. Their executable SHA-256 is `48c15e553fb867c9f1fa43340453a5d79c4d3d2af6bd951a664fe478edb6b993`. The bundle uses the existing local Apple signing configuration. Notarization was skipped because release notarization credentials were not configured. The previous Downloads bundle was retained under a hidden backup name before the fresh copy.

A fresh conversation sent this fixture through the existing configured provider: "Teste visual macOS. Sem ferramentas, sem arquivos e sem alterações no projeto. Escreva três parágrafos curtos sobre uma biblioteca fictícia e termine com UAT-MACOS-OK." The reply completed in 28 seconds, contained three paragraphs, and ended with `UAT-MACOS-OK`. The execution state showed the quiet blue dot and "Em execução". The indicator disappeared on completion and the composer remained available. Existing chat history survived reopening the application.

The final bundle sent another non-mutating fixture in that preserved test conversation, requesting two paragraphs about the fictional library in the afternoon. The provider's actual reply completed in 23 seconds and ended with `UAT-MACOS-STARTUP-OK`. The native accessibility tree independently exposed the assistant's ending, the completed duration, an empty available composer, and the absence of both `Estado da execução` and `Interromper execução`. The running and completed states were captured from this final bundle.

## Capture inventory

Captures are unedited native Appshots/CUA JPEGs at Retina scale 2. The purple monitor badge in the auxiliary native title bar and the highlighted desktop pointer belong to the capture environment. They are not Jarvis UI artwork.

| Capture | Window width in points | Evidence |
| --- | ---: | --- |
| `main-cold-launch.jpg` | 1512 | Final bundle opens at the full 949-point work-area height |
| `main-cold-launch-from-normal.jpg` | 1512 | Fresh process opens maximized despite saved maximized=false |
| `main-restored-normal.jpg` | 1512 | Title-bar double-click restores the original 917-point normal height |
| `main-wide.jpg` | 1512 | Final completed conversation, persistent navigation and inspector |
| `chat-running.jpg` | 1512 | Discreet execution state and stop control |
| `chat-completed.jpg` | 1512 | Completed provider reply and restored idle composer |
| `main-default.jpg` | 1360 | Main window at its configured default width |
| `main-narrow.jpg` | 1024 | Main window at its configured minimum width |
| `settings-wide.jpg` | 1400 | General settings at the configured default width |
| `settings-providers-wide.jpg` | 1400 | Provider summaries, white navigation glyphs and three tool columns |
| `settings-providers-narrow.jpg` | 960 | Two tool columns and scrollbar at the right window edge |
| `settings-skills-wide.jpg` | 1400 | Three independent skill columns with existing controls |
| `settings-mcps-wide.jpg` | 1400 | Three independent MCP columns with status and tool counts |
| `settings-narrow.jpg` | 960 | General settings at the configured minimum width |
| `settings-narrow-lower.jpg` | 960 | Cleanup and backup groups, persistent navigation and edge scrolling |

The minimum desktop window widths are enforced by the existing native configuration: 1024 for the main window and 960 for settings. Phone-sized viewports are not supported by these native windows. Every capture in the inventory was replaced from the final signed production bundle. Settings were inspected at 1400 and 960 points, including the three visible maintenance loading placeholders in `settings-narrow-lower.jpg`; optimization remained disabled while that data was loading.

## User feedback on the rendered build

The user identified mixed dark and white navigation glyphs, an inset scrollbar attached to the centered content column, and wasted width in independent provider tool cards. The correction makes every navigation glyph white and uses subdued tile fills; source-derived white contrast is at least 3.36:1 over the normal and selected sidebar backgrounds. The scrolling TabsContent now spans the available window width and contains a separately centered 1200px content block. Independent tools and catalog cards use 320px minimum responsive columns, while larger agent and workspace configurations use 480px columns. The same pattern applies to Skills, MCPs, Plugins, Fluxos and structural loading placeholders. Standard grouped preference rows remain aligned within their groups.

Before the correction build, the existing second native chat turn was verified complete: the provider returned two paragraphs ending in `UAT-MACOS-FINAL-OK`; the execution indicator and stop control were absent, and the empty composer was available. This verifies the previous native bundle's chat completion and persistence independently of the later settings layout changes.

## Startup correction

The production preference file confirmed the reported gap: its saved normal window was at physical `y=130`, logical width 1512 and height 917, with `maximized=false` and `fullscreen=false`. Startup previously restored those bounds and only maximized when the saved flag was true. `desktop::setup` now exits fullscreen and maximizes before showing the main window, independently of preference loading. It retains the saved normal bounds and display selection for manual restoration. The new unit regression checks that loading saved normal bounds on a secondary display does not rewrite the existing preference file. Native cold launch verification is required for the maximize call itself; no artificial window wrapper was added to test API call order.

The first cold launch showed that the maximize call alone was insufficient: the main window remained 1512 by 917 points. The native macOS Window > Zoom command also had no effect. Window > Fill reached 1512 by 949 points, reported maximized and remained outside fullscreen. This isolated a CEF runtime issue: its borderless AppKit windows were routed through native Zoom. The correction fills the current screen's `visibleFrame` with `NSWindow.setFrame_display` and retains a normal frame for Unmaximize. Titled and fullscreen windows retain the existing Winit behavior. Runtime regressions cover filling, restoration, repeated calls, secondary displays and new normal bounds.

The final bundle cold-opened at 3024 by 1898 physical pixels, corresponding to 1512 by 949 logical points. A title-bar double-click restored 3024 by 1834 physical pixels, preserving the original normal bounds and saving `maximized=false`, `fullscreen=false`. After quitting, the executable was confirmed absent before a new launch. That fresh process again opened at 3024 by 1898 pixels and saved `maximized=true`, `fullscreen=false`, while retaining the same normal bounds. The principal window was also manually resized to its 1360-point default and 1024-point minimum width, then maximized through the title bar. The floating companion was temporarily disabled under the user's prior authorization while interacting with the main window, then reenabled after testing. `companion-restored.jpg` records the restored switch; the application was left on the maximized principal window.

The relevant reference is OpenCode `docs/opencode/packages/desktop/src/main/windows.ts:169–231`: hidden window creation, saved geometry management and showing on `ready-to-show`. Jarvis preserves saved normal geometry but applies the user's explicit ordinary-window maximization policy before showing. Scoped searches in Codex and OMP found no equivalent desktop startup geometry implementation. The installed Winit AppKit fork's `window_delegate.rs::set_maximized` invokes `NSWindow.zoom(None)` for a resizable window; the native Zoom/Fill comparison established why that path does not fill Jarvis's borderless main window.

The finish reviewer identified a lower-scroll capture whose maintenance data was still loading. The registry Skeleton uses `bg-muted`; the chosen card and muted tokens match, making those placeholders indistinguishable from the panel. The application stylesheet now assigns Settings skeletons the existing secondary surface token, without editing registry code. A deferred-maintenance test confirms the loading status and disabled optimization action remain until metrics arrive, then exposes the metrics without invoking optimization.

The independent finish review returned `disposition: ship`, `faithful`, `ceiling: reached`, with no material fixes. Its original five-section verdict is retained in `finish-review.md`. The final native capture verifies the visible loading placeholders; maintenance metric completion is covered by the deferred unit test, not claimed as observed native runtime behavior.

Per the repository's disk rule, recompilable Rust artifacts were cleaned only when no Cargo, rustc or target-directory application process was running. The final cleanup removed 4.2 GiB after the independently verified Downloads copy existed. That copy subsequently passed strict code-signature verification again with the same executable hash and remains available outside the cleaned build directory.

The final system documentation is recorded in `DESIGN.md` and `.impeccable/design.json`. The YAML frontmatter parses, all eight canonical sections are ordered, and the documented palette matches the implemented source. The sidecar parses as schema version 2, has ten complete HTML/CSS previews and no duplicate primitive token arrays. These final edits only document the verified implementation; no application source changed after the passing quality gates and native UAT.

## Verification limits

Native interactions were inspected on macOS only. The chat UAT exercised one configured provider with a non-mutating text fixture. Existing automated tests cover the other affected settings and interaction contracts; this record does not claim a runtime login, payment, cleanup, migration, or provider configuration change.
