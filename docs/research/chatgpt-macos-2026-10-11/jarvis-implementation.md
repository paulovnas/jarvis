# Jarvis implementation of the static investigation findings

Date: 2026-10-11 (America/Sao_Paulo). Tracking: `jarvis-xd4s`, with seven implementation issues and one integrated validation issue (`jarvis-xd4s.1` through `.8`).

The implementation translates applicable client mechanisms into the existing Rust harness and React chat. The source investigation and reference comparison are in [README.md](/Users/enesolucoes/Documents/Pessoal/jarvis/docs/research/chatgpt-macos-2026-10-11/README.md) and [jarvis-reference-map.md](/Users/enesolucoes/Documents/Pessoal/jarvis/docs/research/chatgpt-macos-2026-10-11/jarvis-reference-map.md). Existing revision replay, history bounds, cancellation, durable tool receipts and provider retry policies remain part of the implementation.

## Implemented behavior and evidence

| Bead | Behavior | Main files | Behavior evidence |
| --- | --- | --- | --- |
| `.1` | Cache stable Markdown blocks and parse the mutable tail. Cache resets on identity/content replacement and terminal rendering; document-wide definitions use a complete parse. Bounds: 100 blocks and 131,072 UTF-16 code units. | [markdown-prefix.ts](/Users/enesolucoes/Documents/Pessoal/jarvis/src/components/chat/markdown-prefix.ts), [ChatMarkdown.tsx](/Users/enesolucoes/Documents/Pessoal/jarvis/src/components/chat/ChatMarkdown.tsx) | [ChatMarkdown.streaming.test.tsx](/Users/enesolucoes/Documents/Pessoal/jarvis/src/components/chat/ChatMarkdown.streaming.test.tsx): streamed markup matches complete parsing, including lists, fences, tables, quotes and references. |
| `.2` | Buffer presentation text using animation frames and an adaptive arrival rate. A 160 ms deadline drains a backlog; terminal state, retry/reset, identity change, hidden views and reduced motion flush. Canonical text remains available to copy/export. Applied to main responses, observations, reasoning and project/global companion output. | [use-streamed-text.ts](/Users/enesolucoes/Documents/Pessoal/jarvis/src/components/chat/use-streamed-text.ts), [AssistantWorkCollapse.tsx](/Users/enesolucoes/Documents/Pessoal/jarvis/src/components/chat/AssistantWorkCollapse.tsx), [CompanionAnswer.tsx](/Users/enesolucoes/Documents/Pessoal/jarvis/src/components/companion/CompanionAnswer.tsx) | [use-streamed-text.test.tsx](/Users/enesolucoes/Documents/Pessoal/jarvis/src/components/chat/use-streamed-text.test.tsx) and caller tests exercise bounded lag, final content, Unicode, resets and disposal. Seven renderer/caller test files passed together: 107 tests. |
| `.3` | Render measured visible turns with two-row overscan. Preserve the reading anchor and retain active/pending, focused, selected and expanded rows present in the loaded snapshot. Equal heights avoid redundant updates; measurement processing is bounded per frame. | [Transcript.tsx](/Users/enesolucoes/Documents/Pessoal/jarvis/src/components/chat/Transcript.tsx), [use-transcript-virtualization.ts](/Users/enesolucoes/Documents/Pessoal/jarvis/src/components/chat/use-transcript-virtualization.ts) | [Transcript.test.tsx](/Users/enesolucoes/Documents/Pessoal/jarvis/src/components/chat/Transcript.test.tsx): measured growth, interaction retention, page navigation, rail jumps and pending-row identity. |
| `.4` | Bound pending IPC batches to 512 batches / 4 MiB. Yield between atomic revisions after 200 events or 8 ms; release consumed slots. Update canonical state immediately while coalescing streamed observer publications by frame, with a background deadline. Recover gaps/overflow using replay or a fresh snapshot, including losses during an in-flight subscription. | [agent-event-queue.ts](/Users/enesolucoes/Documents/Pessoal/jarvis/src/core/agent-event-queue.ts), [chat-store.ts](/Users/enesolucoes/Documents/Pessoal/jarvis/src/core/chat-store.ts), [use-chat.ts](/Users/enesolucoes/Documents/Pessoal/jarvis/src/hooks/use-chat.ts) | [agent-event-queue.test.ts](/Users/enesolucoes/Documents/Pessoal/jarvis/src/core/agent-event-queue.test.ts), [chat-store.test.ts](/Users/enesolucoes/Documents/Pessoal/jarvis/src/core/chat-store.test.ts), [use-chat.test.tsx](/Users/enesolucoes/Documents/Pessoal/jarvis/src/hooks/use-chat.test.tsx): exact burst output, final flush, gaps, count/byte overflow and disposal. Removing the recovery-generation guard deliberately made its regression test fail. |
| `.5` | Retain compatible completed HTTP/WebSocket transports across conversation turns, separately from loaded journals. Scope by conversation, credential/model configuration, request URL and headers. Preserve `store:false` and prefix validation before sending a suffix with `previous_response_id`. Fresh capabilities, auth and tracing are bound per turn. Invalidate on failure, cancellation, journal failure, account/configuration change and deletion; stale leases cannot resurrect a removed slot. Pool bounds: 16 idle entries / 32 MiB; five-minute idle expiry is checked on access. Ephemeral workflow workers own disposable pools. | [provider.rs](/Users/enesolucoes/Documents/Pessoal/jarvis/src-tauri/src/agent/provider.rs), [incremental.rs](/Users/enesolucoes/Documents/Pessoal/jarvis/src-tauri/src/agent/provider/incremental.rs), [agent.rs](/Users/enesolucoes/Documents/Pessoal/jarvis/src-tauri/src/agent.rs) | [transport_tests.rs](/Users/enesolucoes/Documents/Pessoal/jarvis/src-tauri/src/agent/provider/transport_tests.rs): seven tests with local HTTP/WebSocket peers and actual library deletion cover reuse, incompatible identity/history, cancellation, dropped futures, fallback, bounds and disposal. |
| `.6` | Revalidate cached inference credentials against enabled account identity and provider kind in public SQLite metadata. Valid cached tokens avoid Keychain access and model rediscovery. Preserve the existing single eligible 401 refresh, bounded inference retry policy, safe request routing and durable action receipts. | [auth.rs](/Users/enesolucoes/Documents/Pessoal/jarvis/src-tauri/src/agent/provider/auth.rs) | The removed/disabled/replaced account cases failed against the previous cached early return and passed after the guard. Existing auth, incremental transport and retry suites passed in the complete Rust gate. |
| `.7` | Reconcile older/equal-revision snapshots and revisionless history pages by turn identity. Preserve confirmed final results, citations, metadata, current steering and queued messages. A newer explicit retry/reset still replaces old state. Navigation preview follows the authoritative final turn. | [chat-history.ts](/Users/enesolucoes/Documents/Pessoal/jarvis/src/core/chat-history.ts) | [chat-history.test.ts](/Users/enesolucoes/Documents/Pessoal/jarvis/src/core/chat-history.test.ts): stale-page tool/citation loss and equal-revision steering downgrade were reproduced before the change; newer explicit retries remain supported. |

## Confirmed deterministic reductions

- The Markdown fixture with 21 updates feeds less than one third of the repeated whole-document input into the parser, while comparing rendered output with complete parsing.
- A measured 100-turn transcript fixture mounts at most 10 rows before additional interaction retention.
- Fifty authoritative store revisions produce one scheduled frame publication; the latest revision remains immediately readable, and terminal publication cancels the pending frame.
- Two compatible provider turns reuse one local WebSocket, with the second request carrying the validated new suffix and previous response ID.

These results establish behavior and work reduction in controlled fixtures. Production latency, FPS, memory savings and OpenAI server-side caching remain unmeasured. Client Markdown reuse is independent of provider prompt caching.

## Integrated validation

All required checks completed successfully on the final code:

- `bun run lint`: zero errors/warnings.
- `bun run typecheck`: passed all three TypeScript configurations.
- `bun run test --reporter=dot`: 252 files / 2,063 tests passed; three files / 10 tests were already configured as skipped.
- `bun run build`: passed the main application and Chrome/Firefox extension builds. A filtered repeat retained all warning/error output and reported none.
- `bun run ipc:check`: generated contracts are current.
- `cargo fmt --all --check`: passed.
- `cargo clippy --all-targets -- -D warnings`: passed.
- `cargo test --quiet`: 2,007 passed, zero failed, 39 preconfigured ignored; binary and doc-test targets completed successfully.
- `git diff --check`: passed.

No commit, push, deployment or Dolt remote sync was performed. The reference trees were used for comparison and not edited; unrelated existing working-tree changes were preserved.

## Runtime limits and artifact maintenance

Real WebView scroll/ResizeObserver geometry and live-provider performance have not received UAT. Interaction retention applies to rows still present in the existing bounded history snapshot. A revision batch remains atomic, so one unusually expensive batch can itself exceed the between-batch work budget. Idle socket expiry is lazy on pool access rather than a timed background sweep.

The separate runtime-comparison follow-up remains tracked as `jarvis-bg4r`. This implementation has fixture evidence rather than production benchmark results.

After the gates, with no Cargo, rustc, Jarvis or `tauri dev` process running, `cargo clean` enforced the repository disk-pressure rule. Cargo reported 20,682 files / 24.0 GiB removed; available volume space increased from 52 GiB to 72 GiB. The next native build will regenerate its artifacts.

## Native follow-up

On 2026-10-11/12, an optimized signed app was built from this implementation,
moved to Downloads, launched and exercised through a new native chat with a
real provider. Six turns covered structured output, continuation, cancellation,
recovery, background completion and a provider response after a clean restart.
Native scroll-window reconstruction, offscreen selection retention and exact
Markdown export also passed. See [the native UAT evidence](jarvis-native-uat.md)
for process identity, turn IDs, output checks and the automation-window limitation.
Performance gains and a physical WebSocket reuse measurement remain unverified.
