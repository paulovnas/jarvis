# Adopted Pixel Cockpit engine

Jarvis adopts Image Cockpit for Codex Workflows 0.2.1, copyright 2026 dreiachse-cyber, under MIT. The original source is retained in `src-tauri/resources/pixel-cockpit/upstream`; `PROVENANCE.json` records its source hashes. The upstream README separately excludes a remote DemonBlade Pixel promotional image. That image is not downloaded or bundled. The repository's generated sample and prompt-example assets are retained, with source hashes and their original sample classification.

## Reproducible build

`bun run build:pixel-cockpit` compiles the adopted source and maintained adapters. It runs from Jarvis's `dev` and `build` scripts. It does not require the original `docs` checkout.

`bun scripts/build-pixel-cockpit.ts --adopt` intentionally refreshes the maintained upstream snapshot from `docs/image-cockpit-for-codex-workflows`. Review the provenance diff before adopting another version. Do not copy `.env`, authentication, local history, Codex jobs, generated customer assets or the excluded promotional artwork.

The adapter keeps the original React controllers, canvas processing, recipe compiler, quality analysis, tournaments, timeline, review, VFX composition, exports, backend and Codex execution protocol. JSX controls are mapped to Jarvis's shadcn registry components. The suite uses a dedicated Tailwind stylesheet and a neutral dark theme; artwork, transparency grids and inspection backgrounds are not recolored.

`scripts/pixel-cockpit/jarvis-worker.ts.template` is compiled only after its original module imports and catalog have been generated. The other adapters are ordinary checked TypeScript. `JARVIS_PIXEL_BUILD_OUTPUT` can direct an isolated diagnostic build to a staging directory without replacing resources used by an active generation.

## Session and process contract

The native Rust host owns an isolated process and CEF view per conversation. The view can remain hidden for agent operation or appear in the chat's suite tab. Both paths use the same original controller state.

Launch the compiled `server.mjs` using the managed Node runtime (or Bun), with:

| Variable | Meaning |
| --- | --- |
| `IMAGE_COCKPIT_API_PORT=0` | Allocate a loopback port |
| `IMAGE_COCKPIT_HANDOFF_DIR` | Writable isolated session directory |
| `JARVIS_PIXEL_RESOURCE_DIR` | Adopted resource directory |
| `JARVIS_PIXEL_SESSION_TOKEN` | Random native-owned session secret |
| `JARVIS_PIXEL_WORKSPACE` | Authorized project root for imports |
| `IMAGE_COCKPIT_CODEX_COMMAND` | Optional Codex executable override |

The server emits `{type:"ready",port,origin}`. A root request containing the session token establishes an HttpOnly, SameSite cookie. Requests require authentication and the expected loopback host/origin. Content Security Policy restricts network requests to that session. File imports resolve canonical paths under the authorized workspace or session.

Codex generation retains upstream's CLI runner, existing subscription authentication, capacity management and process cancellation. The engine does not accept or store a new model API key.

The packaged host replaces upstream's development-only Vite supervisor. `repairCockpit` checks the protected `/jarvis/repair` route, ensures session storage is available, then runs the original API health check and outbox recovery. It neither kills jobs nor deletes outputs, status, logs or the durable snapshot. If the process exited, reopening the suite or requesting its catalog restarts the native host with the same session. A live but unresponsive service remains an explicit diagnostic; the browser never pretends to have restarted it.

## Agent operation surface

`window.__jarvisPixel.start({id,action,args})` returns a synchronous acknowledgement. `snapshot(id)` exposes the real asynchronous operation. `catalog(filter)` documents available operations and original signatures. `activity()` returns manual generation and export receipts using the same lifecycle. `cancel(id)` requests cancellation and confirms only after the backend responds.

| Source capability | Agent entry |
| --- | --- |
| Four generation workflows | `generate` with `workflow` and original parameters |
| Image sources, packs, history | `import_image`, `import_animation_pack`, `select`, `inspect`, original library methods |
| All motion recipes and prompt examples | `catalog({section:"options"})`, `preset` |
| Rectangles, brush and arrows | `annotate`, comment/delete/clear methods |
| Original recipe compilation, quality and normalization | Retained automatic pipeline and documented `invoke` library operations |
| Profiles, tournaments, pilots, batch matrices, targeted repair | Original public App methods through `invoke` |
| Timeline frames, durations, holds, duplicates, loops and metadata | `timeline_open`, original `animationPackV2` functions, timeline `commit`, undo/redo, reset/save/export/roundTrip |
| Candidate review, playback, overlays and calibration | `review_open`, safe `configure`, review decision/save/adopt/repair/export methods |
| Character/effect composition | `vfx_open`, original VFX patch, timing, source, direction, undo/redo, import and export methods |
| PNG/GIF/APNG/WebP, frames, packs, five engine metadata formats | `export` and original exporter/library methods |
| Recovery, outbox, duplicate management and health | Original recovery and diagnostic methods |
| Legacy procedural generator | Explicit `generate({provider:"local-generator",...})`; never an automatic substitute for AI generation |

`invoke` arguments can reference original objects without sending base64 through the model:

- `{state:"draft",controller:"timeline"}` reads an existing state object.
- `{ref:"history",id:"..."}` resolves a retained item.
- `{saved:"name"}` uses an earlier result stored with `saveAs`.
- `{file:{path:"..."}}` creates a File from a scoped import for original pack/composite APIs.

Reference wrappers use these exact shapes: only `state` may include the optional
`controller` key. Original JSON objects such as jobs or candidates containing
`id`, `state`, `ref`, `saved`, or `file` alongside other fields remain ordinary
records; the bridge does not discard their metadata.

Configuration validates the entire change before any mutation. Quality outcomes, history, errors and review decisions are not writable through generic configuration. Internal helpers requiring DOM events or callbacks are excluded from the public App catalog. Closing a modal unregisters its controller, preventing calls into stale React state.

The App callback audit distinguishes product operations from presentation plumbing:

| Original interface callback family | Agent equivalent |
| --- | --- |
| Generation, adoption, queue, profiles, batch, pilot and repairs | Public original methods or `generate` with validated settings |
| File inputs for images, animation packs and composites | Scoped imports create a real browser `File` before calling the original parser |
| Pointer drawing and selection | Normalized `annotate` brush/arrow/rectangle points, selection and original comment/removal methods |
| Timeline inline duration/order/metadata actions | Original pure pack operations followed by timeline `commit`; original save/export/round-trip adapters |
| Prompt example selection | `preset(kind:"prompt")` or original `usePromptExample`, preserving prompt, negative prompt and notes |
| Library rename prompt | Explicit ID/title adapter, using the same library item update |
| Clipboard diagnostic report | `exportEnvironmentReport("json"|"markdown")`, producing the original redacted report as an artifact |
| Tabs, scrolling, focus, dialogs, hover and pointer capture | Presentation controls retained in the suite; agent selects the corresponding controller/state instead of synthesizing DOM input |

The maintained audit found 174 top-level App functions and 91 JSX callbacks. This is an implementation inventory, not a claim that 174 separate user tools exist: callbacks that require DOM events, private functions or raw setters are intentionally not advertised as JSON-callable operations.

`catalog({module,method})` includes the original parameter signature and its related TypeScript field definitions. Additional nested structures are available through `catalog({section:"types",query:"AnimationFrameAnchor"})`; the field names, optional values and enums come directly from the adopted source.

## Persistence, progress and logs

The original IndexedDB cache is supplemented by `suite-state.json` under the managed session, restoring history, frames, actions, library items, settings, operation receipts and original job metadata even when the port changes. Pending submissions are preserved for explicit recovery instead of automatically replayed. Original generation jobs and outputs remain in the handoff directory.

Downloads become uniquely named managed artifacts; manual and agent exports share this path. Native chat imports verified image artifacts for previews. Original Codex logs are retained under the handoff `logs` directory. Live receipts show the original bounded UI log tail and attach the full log path for jobs belonging to that operation.

Manual generation observes the original job rather than issuing another model request. Worker-owned operations are excluded from manual discovery. A terminal receipt is published only after its files and full-log references have been persisted.

Cancellation before submission prevents the subsequent model request. If the original callback is still preparing or submitting a job, cancellation continues observing that callback and cancels newly published owned jobs before reporting success. Missing confirmation is an explicit failure; cancellation is never inferred from an acknowledgement alone.

`managed-runner.ts` replaces upstream automatic process restart during status polling. A dead executor produces `resumeRequired`; an untracked live or inaccessible process prevents concurrent resume. `invoke({method:"resumeInterruptedCodexJob",args:[jobId,"process_existing"],confirm:true})` explicitly resumes the same original job using its saved queue metadata. `process_existing` adds a processing-only directive that prohibits new image generation; `resume` continues the original task when expressly authorized. Both modes retain the original prompt and existing outputs. Status reads never initiate either mode.

Restored running operations become blocked receipts with `originalJobIds` and `recoveryRequired`. Successful original import/quality verification can reconcile a prior single-job receipt only when the persisted mapping identifies that exact job. Legacy operations without that mapping and partially recovered multi-job operations stay blocked; logs or filenames do not authorize a fabricated association.

Agent progress uses actual queue, generation, review and preview phases, plus counted candidates when available. The upstream UI's elapsed-time percentage is explicitly labeled as an estimate.

Generation receipts wait for queued batch cells and targeted repairs as well as direct generation. Motion-pilot review and failed candidate evaluation remain resumable blocked results. A previous tournament's review does not block a new unrelated image. Successfully produced artifacts remain attached when another candidate fails.

## Jarvis presentation and artifact integration

`use-pixel-art.ts` controls the conversation's suite session/tab. `FileWorkspace` and `PixelArtSuite` place the native CEF view in that tab. `use-pixel-art-results.ts` restores saved receipts and consumes live results, including manual suite exports.

`PixelArtCard.tsx` presents original images and animated GIFs, progress, logs and package-save actions in the chat. `Transcript` also displays artifacts exported manually from the suite. The native `get_pixel_art_jobs` command restores completed jobs after reload; `save_pixel_art_artifact` offers a native save dialog for every exported format. The agent's `pixel_art_run` operation `save_artifact` delivers a managed artifact to the authorized project.

Portuguese dictionaries cover the original static copy, motion recipes, body topologies, effect options, the 137 bundled examples, review/timeline/composite controls and dynamic status templates. Translation is applied to rendered text; source prompts, IDs, paths, stored metadata and original execution logs retain their original contents. Keep new labels in `portuguese-copy.ts`, `localized-labels.ts`, `advanced-labels.ts`, `asset-labels.ts`, `prompt-example-labels.ts`, `status-labels.ts` or `status-patterns.ts`, and extend their source-derived coverage tests when adopting upstream changes.

## Verification boundaries

### Native pixel preparation and production recipes

Jarvis adds three operations to the adopted controller without modifying the upstream snapshot: `recover_grid`, `extract_poses` and `production_recipe`. The **Preparar pixel art** panel and agent actions call the same worker dispatcher, create normal operation receipts and export PNG derivatives plus a JSON processing report. These operations do not call an image provider or add a fal.ai dependency.

`src-tauri/src/agent/pixel_art/processing.rs` owns deterministic processing. Its bounded `process_pixel_art_image` command accepts PNG pixels only, runs off the UI thread, and is restricted to the main view and validated pixel-worker labels. The managed worker calls the inlined `pixel-processing` plugin; its capability grants that single command to `pixel-worker-*` views on loopback HTTP origins. It does not enable application-wide remote IPC or introduce an app ACL that would change existing commands. The processor has no filesystem or network access. Input is limited to 8 megapixels / 8192 pixels per side, and combined output is limited to 24 megapixels.

- `recover_grid`: bounded palette quantization, horizontal/vertical edge profiles, periodic pitch/phase detection (including fractional upscales), edge-aligned majority resampling and optional integer nearest-neighbor enlargement. Weak residual border cuts are rejected in detected mode. Ambiguous periodicity returns an actionable error; explicit `cellWidth`, `cellHeight`, `phaseX` and `phaseY` remain available. It does not infer a desired output size from a prompt.
- `extract_poses`: eight-connected foreground components, nominal-cell assignment by occupied pixel area, and nearest-body association of detached parts. Complete components may extend beyond their nominal cells. The default preserves cell-relative offsets, including jump height; `alignment: "bottom"` explicitly opts into baseline alignment. `poseCells` optionally names the occupied cells, indexed from zero, when detached effects occupy an otherwise empty slot. The suite displays the same setting starting from 1. Touching poses and ambiguous grouping still require inspecting the derived frames; this is not an artistic approval mechanism. `minArea` defaults to 1, and any explicitly discarded pixels are counted in the report. Background removal requires an explicit color.
- `production_recipe`: neutral/directional identity anchors, idle, walk, attack, hurt, jump and downed recipes. Generation still uses the existing explicit generation action and configured provider. The original motion compiler, body topology, frame budget, motion variants and QA contracts remain active; the adoption adds identity, asymmetry, pixel-grid and phase guidance, with a version suffix for provenance. A jump is not forced onto a ground baseline, and static directional views are not presented as animation.

`processing-tools.ts` adds history entries and a separate timeline action instead of rewriting accepted references, previous frames or timing. Hydration preserves the native canvas/anchor of processed frames. Local processed animations remain labeled as imports, and use their actual frame count/grid in the original timeline, previews and exports. Pack frame rectangles and initial pivots follow the derived atlas, without inheriting an artistic QA approval. Processing completion respects a newer image, action or frame selection. Reports record the decoded source RGBA hash, processing options, geometry, warnings and source identifiers; original files remain untouched. The implementation is independent of the prompt-only repositories in `docs/chongdashu`.

Worker state synchronization yields through `MessageChannel` tasks rather than short timers, because hidden CEF views throttle timers and previously delayed recipe application for minutes. Timed generation polling remains separate. The opt-in native verification app uses its own CEF profile inside its output directory, in addition to its private Jarvis home and project.

Native verification on 2026-10-20 used existing real PNG assets through `pixel_art_run`: automatic fractional-grid recovery, extraction of nine component-grouped poses, original timeline editing, v2 package export/round-trip, GIF export and all eight production recipes. The recovered image was 98×96 native pixels; the nine poses used 260×260 canvases, with no discarded foreground pixels. Reopening the private session preserved native geometry, anchor, frame order, FPS and edited pack durations. Six motion recipes also preserved a manually configured six-frame budget and slow tempo. This verifies processing and recipe application, not a fresh AI generation or artistic acceptance. Evidence is in `.codex/pixel-review/processing-ready-20261020/`: `processing-real-report.json`, `processing-artifact-inspection.json`, `processing-reload-check.json`, `production-recipe-parameters.json` and `suite-native.png`.

Colocated tests cover adoption drift, public command validation, registry-control adapters, theme isolation, authenticated HTTP routing, scoped imports, artifact byte preservation, full-log references and durable session snapshots.

`browser-smoke.ts --recovery-only` checks interrupted-status HTTP reconciliation and explicit-resume confirmation without generating an image. The normal suite additionally covers the processing scenarios below. Font distributions include their original Roboto and JetBrains Mono license files under `worker/fonts/`.

`scripts/pixel-cockpit/browser-smoke.ts` uses a real isolated browser and original sample assets to verify import, annotations, canvas splitting, export, configuration atomicity, timeline timing/save, pack round trip and controller disposal. It explicitly uses the legacy procedural provider to verify that a real manual button click publishes exactly one receipt and a worker request does not duplicate it. These checks prove processing and delivery behavior, not AI generation quality.

`scripts/pixel-cockpit/runtime-smoke.ts` and `verify-artifacts.ts` exercise actual generated assets through the native bridge and inspect exported files. The parity inventory in `pixel-cockpit-parity.md` remains the source of acceptance coverage; completion requires the real four-flow and agent/chat evidence, including outstanding scenarios recorded there.
