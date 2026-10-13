# Computer Use cursor and live preview

Date: 2026-10-12. Scope: the installed `/Applications/ChatGPT.app`, the user's Jarvis crash report, and the current Jarvis implementation. Static reconstruction does not establish visual runtime parity.

## Confirmed static evidence

The installed main executable, `app.asar`, `native/sky.node`, and `SkyComputerUseService` still match the SHA-256 identities in `evidence-manifest.json`. The installed package identifies itself as Codex despite its `ChatGPT.app` directory name. Findings apply to these bytes.

The six complete REA records returned during this pass are preserved verbatim in [rea-cursor-preview-evidence.json](./rea-cursor-preview-evidence.json). They were collected from the returned records because the provider's failed service startup/cleanup cleared its retained session ledger; the final native export contained zero records. The restored ledger must not be described as a successful export of the failed service analysis.

The portable bundle was canonicalized without changing record content or Evidence IDs. `rea inspect-evidence-bundle` validated all six complete records, semantic IDs and manifests successfully.

| Evidence | File/function | Confirmed observation |
| --- | --- | --- |
| `ev_c6efab5f562653e839d46d9167d991a84269ec5404fc876fb8d72363793dc971` | `sky.node`, procedure search | Separate cursor-location callbacks, video-frame callbacks, native video presentation, placement and thread visibility methods exist. Names alone do not prove their rendered appearance. |
| `ev_d6c524c315d2b6abfb9647877711306686ea279e9ac4774028da4ff95b33ff8e` | `sky.node`, string search | Cursor-location handler, presentation/thread state, native-video and placement identifiers corroborate the scoped procedure candidates. |
| `ev_44b88c5bfb2c444d1fff473af9c36466c814c4e84d94d8bfe1a3797f827a30eb` | `0xb0b8`, cursor-location block | Packages the point/active payload and uses `napi_call_threadsafe_function`, with mutex/lifetime checks. This is bridge dispatch, not drawing code. |
| `ev_afc6edb884ebfc8e15f52d223a7fc6f080eb4f38b0345c1fa83a9957667a24a8` | `0x15e04`, `setComputerUseCursorLocation:isActive:` | Stores the location and active flag, then calls `publishComputerUseLookTarget`. |
| `ev_e26df42c68f8b810a160ad9f1ce64b7eb908ef1ffa346cc1a8dd5b052534cb14` | `0x15e10`, `publishComputerUseLookTarget` | Uses a hosted avatar presentation center when available, otherwise the stored cursor location/active state, then invokes its handler. It must not be mistaken for the cursor renderer. |
| `ev_4268d2cdec9aa723463f41500a00d3c2ddd64be6510db7a0ab67e8db0d5a0062` | `0xbf24`, video-frame block | Creates JS payload fields `presentationID`, `threadID`, `codec`, `decoderConfiguration`, `timestamp`, `isKeyFrame`, and `videoData`, copying the native buffers. The preview is a video transport, not merely a static screenshot card. |

Supplemental static observations used Apple's `nm`, `swift-demangle` and `assetutil` on the same bundle:

- `ComputerUse.ComputerUseCursor` exposes movement, press/activity and fade state, target-window/application identity and motion configuration. `Style.current` is declared `@Swift.MainActor`; `FogCursorStyle` owns an `NSHostingView<ComputerUse.CursorView>` and `FogCursorViewModel` exposes velocity, pressed, attachment, angle and stretch state. These are declarations, not recovered shader parameters or measured animation.
- `Package_ComputerUse.bundle/Contents/Resources/Assets.car` contains a 200×230 `SoftwareCursor` image. A resource-only AppKit extraction was inspected locally. That legacy cursor/floppy image does not establish which asset the active Fog cursor renders; it is not copied into Jarvis.

## Confirmed Jarvis crash cause

The supplied report identifies Jarvis 1.9.9, PID 44769, `EXC_BREAKPOINT/SIGTRAP` on thread 61 (`tokio-rt-worker`). Its stack goes through Jarvis `macos::click` → `focus` → `ax_perform(AXRaise)` → `AXUIElementPerformAction` → the same process's `accessibilityPerformRaise` → CEF/AppKit `makeKeyAndOrderFront`/window ordering. This is a same-process AppKit invocation on a worker, not evidence of an OpenAI provider failure.

`macos.rs` routes same-process Accessibility reads and mutations to the main queue. Own-window focus is checked with AppKit's `keyWindow` and public window number; raising that window uses `makeKeyAndOrderFront` on main. Target, element freshness and cancellation are revalidated after the queue wait; dispatch is marked at the native mutation. Cross-process element queries remain worker-side IPC. System-wide hit testing runs on main because it can reach Jarvis's overlays even when the selected target is external. Only owned Rust values and retained AX client handles cross back to the worker. The fix keeps self-control enabled and retains uncertainty handling.

## Implementation choices in Jarvis

These are adaptations, not recovered ChatGPT source or a claim of pixel-identical rendering:

- `cursor.rs`: nonactivating, click-through AppKit pointer with a blue/white shape, small Jarvis label and click pulse. It never warps the physical mouse. Reduced-motion mode uses a static short-lived click marker.
- `preview.rs`: nonactivating AppKit panel in the upper-right visible area of the target's display. The initial/baseline PNG is the exact byte buffer stored as the agent's observation attachment. `SCStream` then shows a live preview of that same selected window, with a separate cursor marker mapped through the letterbox geometry.
- The whole preview surface accepts a first click and uses AppKit's native window drag. Observation updates preserve its position; stream replacement carries that position to the new panel. The panel cannot become key/main and dragging does not grant additional computer permissions.
- Capture excludes the physical cursor and window shadow; a desktop-independent window filter prevents recursive capture of the overlays. No extra Chromium window, audio stream or continuous PNG transport through React is added.
- Stream configuration requests at most 60 fps and 640×420 pixels, native queue depth three, and a latest-frame mailbox with one pending main-queue delivery. These are limits/configuration, not a measured 60 fps guarantee.
- Labels distinguish the observed frame (`Observado agora`), live frames (`Prévia ao vivo`) and a capture fallback (`Última captura`). Live intermediate frames are not presented as new AI observations.
- `controller.rs`: feedback has a turn/target token separate from the per-action guard. Normal turn completion, interruption, target/owner replacement, revocation, mode change and errors retire the appropriate feedback. A stopped conversation cannot hide a different owner's newer panel.

The relevant design references were Codex's thread-scoped screenshot provenance/size bounds, OMP's explicit capture geometry and resize checks, and OpenCode's tool cancellation/attachment contracts. The reference trees were not changed.

## Follow-up from user runtime test

The user confirmed that the signed preview looked correct, but it could not be moved. Source inspection confirmed two causes: mouse events were ignored, and each observation reset the panel origin. Both are changed above.

Session `a3017a77ac7aa129fdcb1fb505e1c316` recorded a 3024x1898 capture and one `computer_click` at image coordinates `(18, 1868)`. It returned `computer_action_uncertain`, `inputSent: true`, after 864 ms. The next observation and user report confirmed that Settings did not open; the original process stayed alive. The AX snapshot was partial (one root, error -25201). The receipt did not retain its inner native error, so the exact failing stage cannot be established from that receipt. Its duration is compatible with the 800 ms focus confirmation loop; that is an inference, not a recovered stack trace. The vision tool's horizontal coordinate also differed from the visible gear center, making semantic AX targeting preferable when available.

Own-process observation now sends `AXEnhancedUserInterface` on main, matching the existing CEF runtime hook that publishes web accessibility. OMP's `enable_web_accessibility` provided the comparison for lazy Chromium AX activation; Jarvis uses its own runtime's signal. Uncertain receipts now retain a structured native `cause` while preserving the no-replay contract. Native verification of these follow-up changes is pending below.

## Unknowns and validation boundary

Hopper's full `SkyComputerUseService` analysis exceeded its 120-second bridge-readiness deadline; its background pass completed after approximately 3m40s, but the generated Python bridge remained blocked and authenticated cleanup could not be confirmed. No complete service body decompilation is claimed. The smaller `sky.node` dossier above completed successfully.

Exact Fog cursor colors, paths, spring/motion values and live PIP placement in a controlled ChatGPT turn remain unknown. Static evidence alone does not establish visual parity or measured frame cadence.

## Delivered build and acceptance

The final frontend passed lint with zero warnings, typecheck, all 2,123 tests (10 skipped), and the production build. Generated IPC contracts passed separately. The final native source passed Clippy for all targets with warnings denied, formatting checks, and the complete Rust suite: 2,079 passed, zero failed, 41 ignored. An earlier Beads shared-lock timing failure did not recur with test concurrency limited to four.

The normal release bundle was built and copied to `/Users/enesolucoes/Downloads/Jarvis-Cursor-Movel-2026-10-12/Jarvis.app`. Deep/strict signature verification passed. Its identifier is `com.foxtag.jarvis`, signing team `9KXT6JDW8H`, and main executable SHA-256 `9d216dd259967a97d321269735bf0952ad56dfd35cda7dc94a3bd8a80fefa4ef`. This is a local developer-signed build, not a notarized release.

Native agent verification used new conversation `772f19523c8d9954ca51f7f824641b7f` on that exact bundle, PID 22224. After observing again because the accessibility tree was partial, the agent clicked the accessible Settings element once. The receipt reported `inputSent: true`, phase `observed`; the new `Configurações · Jarvis` window (2717) appeared while the selected target remained the main window (2700). CUA independently observed the Settings window and the process remained alive. The desktop capture showed the preview still displaying the selected main window with its live status. Partial AX warnings remain recoverable; the agent found and used the Settings element without Vision.

The final automated preview-drag attempt was interrupted by the user, so it is not presented as an agent-verified drag result. The user then explicitly confirmed that all requested behavior had been tested and authorized commit/push. Acceptance of the cursor appearance and direct preview drag is user-confirmed UAT. Physical multi-display arrangements, exhaustive stale-geometry/stop-during-dispatch checks, exact ChatGPT visual parity, and measured presentation FPS remain outside this acceptance evidence.
