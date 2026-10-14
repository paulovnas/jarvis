# GW GA4 chat audit

Audited on 2026-10-14. This is an investigation report, not a delivered fix or a replay of the cloud configuration task.

Subsequent implementation and verification are recorded in the [harness correction receipt](ga4-gw-harness-fixes-2026-10-14.md). The source findings below describe the audited baseline, before those corrections.

## Scope and evidence

- Conversation: `Configuração de APIs do GA4`, project `GW`.
- Conversation ID: `b5066ee7bd2790f0cfb80bfec21f976c`.
- Project ID: `6c791e57463c4e7a97ce6ec878e17683`.
- Production session journal: `~/.jarvis/sessions/6c791e57463c4e7a97ce6ec878e17683/b5066ee7bd2790f0cfb80bfec21f976c.jsonl`.
- Journal SHA-256: `70fccfd09b0a2e0ef8e28cba8ff2335af1658550fc53453b8d507f8cb734cc1d`.
- Read all 334 records and the terminal checkpoints for three turns. Inspected structured tool receipts and the existing final native screenshot. Queried the conversation index using SQLite read-only mode.
- Source inspected in the active worktree, base commit `38612450a0752fd1c8f1c1e027b9f8df9a7bf766`. Reference repositories were read-only.
- No cloud actions, browser navigation, account controls, application restart, source edits, commit or push were performed for this audit. Existing OAuth/UI changes were preserved.
- No authorization codes, reauthentication parameters, credentials, private message content or screenshot pixels are copied into this report.

## Measured outcome

| Turn | Recorded execution duration | Outcome | Steps | Tool calls |
| --- | --- | --- | --- | --- |
| `50d6c78571ed46d819c3081520297f9b` | 20m 32.213s | Cancelled | 45 | 49: 41 completed, 8 errors |
| `c174be97238db6d5194f754dc8063b2c` | 1m 9.900s | Provider HTTP 503 | 4 | 2 completed |
| `e6ab1c0b97bc448413ffa87130be56c2` | 56.826s | Completed manual instructions | 2 | 0 |

The first turn used `openai-codex-ene`, `gpt-6.1-sol`, reasoning `max`, build mode and YOLO approvals. Its vision calls used `antigravity-pessoal` / Gemini 3.8 Flash. It was not an OpenCode Go execution.

The sum of recorded first-turn tool durations is 338.328 seconds, including a 142.094-second wait for the user's Google reauthentication, 100.998 seconds across five Firefox script timeouts and 59.630 seconds across three vision calls. Tool duration totals and step totals are not a wall-clock partition: waits, generation and runtime timing have different accounting. The journal does not justify labeling all remaining time as model reasoning or a precise percentage of avoidable delay.

The journal shows the Data API first disabled and later enabled. It cannot attribute that state change conclusively to the uncertain native click. It contains no successful completion evidence for the Admin API enablement or the requested GA4 Reader permission. Current cloud state was not rechecked.

## Confirmed failures

### 1. Repeated Firefox script timeouts

The user explicitly asked to use the extension because Firebase was already logged in. The queued instruction was applied after step 10. The agent attached Firefox extension tab 55 successfully and obtained its DOM. Google subsequently requested reauthentication; the agent correctly handed that step to the user.

After reauthentication, five reads failed with the same 20-second script timeout:

| Step, zero-based | Tool | Call ID | Duration |
| --- | --- | --- | --- |
| 19 | `browser_snapshot` | `call_f33f198cd3d14bbc9c686cc0419259be` | 20.862s |
| 20 | `browser_console` | `call_f25de378c9774fe8afc3be075c183a88` | 20.023s |
| 22 | `browser_snapshot` | `call_840a60a00ec549cfaf1fa3c9fd281bd8` | 20.028s |
| 23 | `browser_evaluate` | `call_38c8c0f94bf947cabc4aee9125f2f7b1` | 20.055s |
| 36 | `browser_wait` | `call_06341adbb8a945ce8588472be1428c3c` | 20.030s |

`browser-extension/firefox.ts::inject` races every script against a fixed 20-second timer, reports `browser_outcome_unknown` for all those failures and does not cancel the pending WebExtension injection. Repeated inspection used the same failing execution mechanism. Step 43 later obtained a snapshot successfully; the extension was not permanently disconnected.

The injection does not set `injectImmediately`. Mozilla documents that scripts default to `document_idle` and supports immediate injection. Waiting for a still-loading document/frame is a plausible contributor, not a proven explanation of this production event. The journal records no Firefox-side ready-state or injection-start timestamp, so a blocked load, busy content process and other Firefox faults remain distinguishable hypotheses. [Mozilla API contract](https://developer.mozilla.org/en-US/docs/Mozilla/Add-ons/WebExtensions/API/scripting/executeScript).

Required regression: a controlled slow-loading page/frame must remain inspectable without waiting for unrelated loads; a timed-out read must be distinguishable from a dispatched mutation, and late mutation completion must not be silently replayed.

### 2. Screenshot analysis outlived the observation

At step 27, `computer_observe` returned a fresh screenshot and observation. Step 28 called the separate vision model; vision itself took 30.597 seconds and that step took 57.514 seconds. Step 29 then spent 22.785 seconds and attempted a click using the older observation.

The click was rejected with `computer_stale_observation`, `inputSent:false`:

- Observation: `3d8ca43718a0c640d7416249d8c34fec`.
- Observe call: `call_06d8a38f6d944c1ab0fe3279072598a5`.
- Vision call: `call_21aa0796c6f243e0af983ec57ebec79e`.
- Rejected click: `call_dba58c4017264871b445044edc97ff2b`.

`src-tauri/src/agent/computer/controller.rs` intentionally limits observations to 60 seconds and validates owner/generation. `computer/tools.rs` and the observe result explicitly instruct the agent to call `vision`. The terminal wire contains no inline image content: the primary agent receives attachment metadata and the separate model's textual interpretation.

The freshness rejection worked correctly. The screenshot-to-action path is too slow in this observed case. Increasing the safety TTL alone would conceal stale-image behavior.

Required regression: vision-capable primary models should receive bounded screenshot content through the supported multimodal path; a configured secondary vision model must still be honored where applicable, and expired observations must be refreshed before input. Text-only providers need a tested fallback. Do not assume the absence of inline pixels proves a provider lacks vision capability.

### 3. Native capture included an unrelated browser tab

The selected native target was Firefox process 6185, window 190, target `b448b39982de1e6389adf1df32902f37`. The first four observation titles identified Google Cloud. The fifth observation, at step 42, identified Google Chat. Inspecting its existing attachment confirmed a private chat, not the intended API configuration page.

- Capture call: `call_28f99f3a6fe7478bbca3af432579e010`.
- Observation: `11e9e67016a35036e00ae46475d93106`.
- Existing attachment: `616f89f317ed8a9b34467486773a8d22`.

This was the same native window, so app/window identity validation succeeded. It did not preserve the extension's browser tab boundary. `browser_navigate` updates the exact extension tab; it does not make that tab foreground. A different foreground tab can therefore be captured by the native window fallback. The journal does not identify who switched the foreground tab. It records no native input following this unrelated capture.

Required regression: an extension-to-native handoff must validate the intended foreground tab/context before pixels leave the browser window; if the user changes tabs, suspend the handoff or re-establish the intended tab deliberately. Keep ordinary native applications supported and preserve user cancellation. Merely checking the process/window ID is insufficient for this scenario.

### 4. Accessibility failures and ambiguous native input receipt

All five native observations contain repeated AX error `-25201` and truncated accessibility trees, despite reported Accessibility and screen-capture capabilities being enabled. Repeating the observation did not recover useful page elements.

The second native click returned `computer_action_uncertain`, `inputSent:true`, with nested cause `computer_stale_target`, after 333ms (`call_d36f16d6925c49c29e115242393b35e9`). This proves that the runtime classified the operation as uncertain; it does not prove the API button received the click.

`computer/macos.rs::focus` calls `OperationGuard::before_dispatch` before application activation. The same flag is used for AX mutation and mouse/key posting. `computer/controller.rs` uses that flag for `inputSent` and uncertainty. The current receipt does not distinguish application activation/focus from the intended click. The source permits a focus-only failure to be reported as input uncertainty; this audit cannot prove that exact path caused this historical click failure.

Required regression: distinguish focus/activation side effects from dispatched mouse/key/semantic input while preserving cancellation, focus checks, release cleanup and conservative recovery after actual input. Reproduce AX failures with a disposable target before choosing a macOS backend change.

### 5. Premature recovery decision, then rejected reclassification

After observing an unchanged button, the agent resolved the uncertain click as `not_applied`, using `call_9ed341bfdef244b7bb13ba792067129c` as evidence. A later screenshot showed the Data API enabled. The agent then tried to change the same click outcome to `applied` (`call_107d65d2bbc0443cbdb386c3c67bba16`). The runtime correctly refused to rewrite a confirmed result.

An unchanged button immediately after an uncertain asynchronous operation is insufficient to establish definitive non-application. Conversely, a later enabled state does not establish who applied the change. The recovery guidance needs to distinguish current resource state from attribution of a historical operation and permit unresolved outcomes to stay unresolved while gathering appropriate evidence.

Required regression: delayed remote effects remain unknown until supported by evidence; later state changes do not rewrite confirmed operation history or trigger an automatic duplicate action.

## Other relevant observations

- `browser_list` returned `activeId:null` and `backend:embedded` while its catalog contained the owned extension tab. `browser/routing.rs::merge` intentionally uses the integrated UI selection and labels the merged catalog as embedded, even for an extension selection. This is ambiguous model-facing metadata, not proof the later explicit-tab requests used the wrong backend.
- The second turn's HTTP 503 was a provider availability failure during the request for manual instructions. It is separate from the first turn's browser and native failures. The third turn delivered instructions without more tools.
- Native capture coordinates are screenshot pixels by contract, including scale 2. The evidence does not establish a coordinate-scale defect.
- Initial permission checks, extension adoption after the user's steering, the reauthentication handoff, observation expiry protection and immutable recovery history were working safeguards. Preserve them in any fix.

## Reference comparison and implementation boundary

- Codex: `docs/codex/codex-rs/core/src/tools/handlers/view_image.rs` checks image support and returns `InputImage` in the tool output. Its log preview omits image bytes. This is the primary reference for directly showing bounded pixels without logging raw image data.
- OpenCode: `docs/opencode/packages/opencode/src/tool/read.ts` returns supported image/PDF files as attachments alongside a short result, rather than forcing every visual inspection through a separate model.
- OMP: `docs/omp/packages/coding-agent/src/tools/read.ts` emits image content; `session/provider-image-budget.ts` applies provider-aware image budgets and removes older transient images when necessary. Optional secondary image-question flows remain a separate choice.
- Port the image-delivery and budgeting concepts, not reference code. Do not bypass selected providers, tab ownership, observation freshness or uncertain-result safeguards.

## Delivery status

Investigation evidence is complete for this journal. Runtime fixes, regression tests and disposable real-browser/native validation remain pending. The currently installed application still has the reported failure paths. The existing MCP OAuth and settings-layout changes were neither altered nor revalidated by this investigation.

Tracked in Beads under `jarvis-tl5j`: `.1` browser/native scope, `.2` screenshot delivery and expiry, `.3` Firefox timeouts, `.4` focus/input receipts, `.5` delayed-effect recovery. The first three are P1; the last two are P2. No subtask is marked fixed.
