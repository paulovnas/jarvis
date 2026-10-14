# GW GA4 harness correction receipt

This receipt follows the read-only [chat audit](ga4-gw-chat-audit-2026-10-14.md). It records implementation and verification separately from completion of the user's original Google Cloud / GA4 task. The real cloud task was not replayed.

## Source changes

The active worktree is `codex/mcp-manual-oauth`, based on `38612450a0752fd1c8f1c1e027b9f8df9a7bf766`. The existing manual/plugin MCP OAuth, callback design and MCP section changes remain in place. No new Git commit, push or release is part of these corrections.

| Defect | Resulting behavior | Main source |
| --- | --- | --- |
| Firefox inspection waits for `document_idle`; expired input may execute later | Immediate isolated-world inspection, distinct readonly errors, and a single-use input grant checked against connection, tab, frame, deadline and cancellation. MAIN execution with an unconfirmed outcome remains uncertain, including falsy error values; CSP remains enforced. | `browser-extension/firefox.ts`, `firefox-page.ts`, `page.ts` |
| Native capture follows another tab in the same browser window | An owned extension tab is bound by its exact handle, browser family, window and foreground revision. Native title/frame/process affinity is checked before capture, after capture before publication, and before focus/input. Changed contexts invalidate old observations. | `src-tauri/src/agent/browser/routing.rs`, `computer/controller.rs`, `computer/macos.rs` |
| Primary model receives screenshot metadata and must call a second model | Image-capable primary models receive the latest causal screenshot as a transient, scoped PNG preview. Preview geometry and original-pixel coordinates are explicit. Text-only models and oversized contexts retain the configured Vision fallback. | `src-tauri/src/agent/context_manager.rs`, `agent.rs`, provider protocol tests |
| AX child enumeration returns `-25201`; focus is reported as input | Empty AX child lists are checked by count before paging. Focus/raise use a cancellation-aware boundary that does not mark mouse/key/AX mutation dispatch. Only actual input crosses the input boundary. | `src-tauri/src/agent/computer.rs`, `computer/macos.rs` |
| A delayed resource effect causes attempted reclassification of a settled action | Confirmed historical outcomes stay immutable. A conflicting recovery request receives the original structured receipt; guidance separates current resource state from attribution and avoids duplicate mutations. | `src-tauri/src/agent/workflow.rs`, `workflow/recovery.rs` |

The generated native Chromium payload `browser/page-cdp.js` was regenerated from the shared extension page/frame guards. No dependency was added. Reference implementation comparison remains documented in the original audit; the Codex, OpenCode and OMP reference trees were not changed.

## Controlled runtime evidence

- [Firefox smoke receipt](ga4-firefox-controlled-smoke-2026-10-14.json): Firefox 157.0.1, separate headless profile, localhost fixtures. Default injection waited 23,022 ms on a parser-blocked load. Immediate frame inspection completed in 95 ms while the document was loading. Canceled and expired queued inputs did not apply later; replayed click grants did not dispatch twice. Four MAIN evaluation cases mutated once and then threw or failed serialization, preserving `browser_outcome_unknown` / `dispatched:true`. A CSP-blocked mutation stayed absent.
- [Native browser calibration receipt](ga4-native-browser-calibration-2026-10-14.json): visible disposable Firefox profile, localhost fixtures, only the verified fixture PID queried. Extension and Quartz titles matched, and frame deltas were zero for two differently positioned/sized windows. The extracted current production predicates produced 10/10 expected outcomes, including same-title different-tab/window rejection and stale foreground revision rejection. The disposable process exited.
- A disposable Cocoa fixture with an empty AX group reproduced `-25201` when copying at index zero. Count-before-copy skipped the empty array and completed without that error. The extracted dispatch/focus boundary harness passed 12 cases; its baseline variants failed six of them.
- No personal browser screenshot or private conversation content was copied into these receipts. No credential, account or cloud permission was changed.

## Verification state

The first frontend run passed 2,201 tests and found one stale generated-payload mismatch. After regeneration, the ordered final frontend gates passed:

| Gate | Result |
| --- | --- |
| `bun run lint` | Passed; zero warnings |
| `bun run typecheck` | Passed, including extension TypeScript |
| `bun run test` | 2,202 passed; 12 skipped, 259 files passed / 4 skipped |
| `bun run build` | Passed; Chromium extension, Firefox extension and frontend built |
| `cargo clippy --locked --all-targets -- -D warnings` | Passed after correcting one temporary-reference type error; zero warnings |
| `cargo test --locked -- --test-threads=2` | 2,156 passed; zero failed; 42 ignored; all 14 `ga4_` regression tests passed |
| `bun run ipc:check` | Passed; generated IPC contracts are current |

The first full Rust execution passed 2,155 tests and failed one existing description assertion that still required the former secondary-Vision instruction. The assertion now checks direct primary previews and explicit Vision fallback. The final full Rust suite and IPC contract check passed after that update. No warning appeared in the final frontend, Clippy, native test or IPC logs.

Independent read-only review found no remaining Critical or Important findings, subject to the final required gates. The review covered both connected callers and the generated native payload.

## Runtime limits and delivery state

The calibration CLI identity lacked Accessibility permission; its external AX calls returned `-25211`. This does not describe the installed Jarvis permission state. Real external AX root/focused-window affinity and a complete visual provider-to-native-input workflow inside the updated signed Jarvis remain unverified. Other browser/display configurations were not hardware-calibrated.

The production macOS package was built, signed, installed at `/Applications/Jarvis.app` and restarted normally. The two Movarte terminal tabs were visibly restored. The final main executable has SHA-256 `d7cd18f4a285d4dfbf50b5df44961e8294ac1fbcd9414ee7aa6f5e9997c6e20f`; strict deep code-signature verification passed. The package retains application version 2.0.2 and includes the corrected Firefox 1.2.2 resources. It uses the existing Apple development identity; Apple notarization was skipped because notarization credentials were unavailable. No new release was published.

The existing Mozilla add-on was updated as an unlisted version 1.2.2 (version `6578448`, file `5122581`). Mozilla displayed **Approved**. The permanently installed add-on is active, has Firefox `signedState:2`, preserves ID `browser@jarvis.foxtag.com` and has the same host/API permissions as 1.2.1. Its `worker.js` SHA-256 is `31480e1d1a29adfeeec3034c7e37ef45c7a141af4ed74e45ab710711b28f77bb`, matching the submitted package and the final app resources. The signed XPI has SHA-256 `1bd53525437592724ce63325e76af63f82490d380c532395ea63a0e3aadbb49a`. The Mozilla source download matches the submitted source ZIP byte-for-byte (`4625f3fac70013fb40b6ce76ef5631bd408f05c2ab50d0402988520e319ca84d`); a rebuild reproduced the same worker.

Mozilla validation reported zero errors and three alerts: page-scoped MAIN evaluation in `worker.js`, plus two React runtime `innerHTML` assignments in `options.js`. The extension options source has no application `innerHTML` or `dangerouslySetInnerHTML` caller. These remote validator alerts are distinct from the zero-warning local quality gates. The unsupported MV3 `background.persistent` property was removed, and the final ordered frontend gates passed again with 2,202 tests after that manifest/test change. Rust sources did not change after their passing gates.

After the final application restart, Jarvis visibly displayed **Conectada** and **Firefox · extensão 1.2.2** without revoking or replacing the pairing. [Connection screenshot](ga4-firefox-1.2.2-connected-2026-10-14.png). Signed packages and recoverable application backups are preserved under `/Users/enesolucoes/Library/Application Support/Jarvis Updates/ga4-harness-2026-10-14.AEkW2l`.

[Installed delivery receipt](ga4-installed-delivery-2026-10-14.json). Prepared extension files also report 1.2.2 and match the signed worker. After confirming that no compiler or application used the worktree target directory, `cargo clean` removed 8.9 GiB of rebuildable artifacts; available disk space rose from 26.22 GiB to 34.31 GiB. The copied signed application and backups remain intact.

Arbitrary MAIN-world code that has already begun cannot be reliably canceled; an unconfirmed effect remains uncertain and must be inspected before any replay. The complete installed-app native visual workflow remains tracked separately in `jarvis-t2n3`. No live GA4 configuration success is claimed from source tests, a package build or a restart. Plugin shared-link installation and marketplace preview counting remain separate pending work (`jarvis-xtco`, `jarvis-001g`).

The final tracked source diff has SHA-256 `4469cf2bdf42c01e9469b7981b880f3cd0860aa7c4e8b11a23be392d8e653b74` before any publication.

The original audited GA4 journal still has SHA-256 `70fccfd09b0a2e0ef8e28cba8ff2335af1658550fc53453b8d507f8cb734cc1d` after the implementation checks.
