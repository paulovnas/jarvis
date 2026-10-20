# Image Cockpit adoption: source and parity inventory

This document records the supplied upstream implementation, the adopted runtime boundaries and actual verification evidence. Early feature tables use descriptive operation names; the dynamic runtime catalog and the evidence ledger below record the implemented commands. An available catalog entry is not evidence that its execution passed. Implementation tracking lives in Beads (`jarvis-i2l5`); this document is a source reference and acceptance matrix.

The latest user direction requires **both** the complete original suite in a Jarvis chat tab and operation through the exclusive pixel-art agent. Manual controls and chat commands must act on the same runtime, selected assets, jobs, review decisions and timeline state. The earlier agent-only presentation boundary is superseded; all original functionality remains required.

Source root: `docs/image-cockpit-for-codex-workflows/`. Line references are relative to that root and refer to the source inspected on 2026-10-20. The package identifies itself as `image-cockpit-for-codex-workflows` version `0.2.1`.

## Current verification boundary

All four original Codex workflows produced real artifacts. Additional actual runs covered the minimum batch, three-candidate Best pilot, technical winner adoption, two-direction expansion, targeted front repair, explicit acceptance and revalidation. The batch correctly rejected one near-static candidate and preserved it; three cells published. Cancellation, import-failure recovery, exact-call acknowledgment recovery, physical file export, editor/library operations and populated reset scopes have separate evidence below.

Every one of the 73 public App entry points now has a recorded direct execution or an explicitly identified shared/guarded contract: 44 direct operations, 11 shared callbacks (three ending in a review requirement), 12 shared adapters, one source-traced invocation, and five separately classified managed/partial/fallback/idempotency/guard outcomes. This is an entry-point ledger, not a claim about every parameter combination or artistic quality. Sixteen library modules and three editor controllers are covered separately. Real chat GIF playback, native suite viewport sizing and bidirectional manual/agent shared-edit acceptance passed with separately recorded evidence.

## Provenance and adoption boundary

- `LICENSE:1` is MIT, copyright 2026 dreiachse-cyber. Preserve the copyright and permission notice in copied/adapted distributions, and record the upstream source revision and adopted-file hashes.
- `README.md:17` explicitly excludes the DemonBlade Pixel: Hyakki Senkou game artwork in its promotional Production Context section. Its Steam-hosted header is a remote README image, not a required runtime asset; do not download, bundle or reproduce it in Jarvis. The only source/docs references to that game name or Steam app ID found in this snapshot are in `README.md`.
- `README.md:275` identifies the bundled samples as original generated demo assets for this repository. They are separate from the excluded promotional artwork: `public/samples/` contains 17 PNGs (5,889,397 bytes), and `public/prompt-examples/` contains 137 PNGs (30,463,739 bytes). The sample seed and 16 hidden-library sheets reference the first directory; prompt catalog entries reference the second. Retaining their catalog previews is valid source parity, but these assets cannot serve as proof of newly executed generation.
- `README.md:21` describes the production generation path as a local Codex handoff using `codex exec` and its imagegen capability, without an OpenAI API key. The procedural local PNG generator is explicitly a development/fallback implementation, not equivalent image-quality evidence (`README.md:25`).
- `README.md:51` distinguishes the code license from generated-asset rights. Adopted code licensing does not assert ownership or clearance of user inputs/outputs.
- The nested `AGENTS.md` refers to external operation files; no additional nested `AGENTS.md` was found inside the source tree. The adoption should preserve the source snapshot and make Jarvis changes in a maintained integration directory.
- The product is not just its HTTP server. `src/App.tsx` contains approximately 16,600 lines, including generation contracts, browser Canvas processing, orchestration, quality validation and artifact assembly. `src/lib/` contains reusable recipe, tournament, pack, review, storage and export implementations.
- Retaining only the original server routes would omit quality-critical processing. Retaining only the main App hook would omit subcomponent-owned timeline, review and VFX operations. Preserve the complete suite in the requested Jarvis tab and expose equivalent operations to the dedicated chat agent.

## Shared execution contract

| Capability | Upstream implementation | Proposed agent operation | Required retained behavior |
| --- | --- | --- | --- |
| Four primary workflows | `src/App.tsx:426`, `:7013`, `:7094` | `generate`, `edit`, `animate`, `effect` | Explicit workflow distinction, workflow-specific validation, original prompts/contracts and postprocessing |
| Provider selection | `src/types.ts:1`; `src/App.tsx:7019` | `configure` / generation `provider` | Codex handoff, local procedural generator, local file import, local inbox import; procedural results visibly identified |
| Generation draft | `src/App.tsx:7094`, `:7427` | Generation arguments | Prompt, negative prompt, notes, seed, size, count, quality; source identity/image; grid, cell, action, directions, frames per direction, chroma, recipe, profile, fingerprint and effect context |
| Queue and admission | `src/App.tsx:7037`, `:7462`, `:7497`; `src/lib/animationTournament.ts:108` | `jobs`, `status`, `cancel` | Maximum three active Codex runners, server/local/reserved slots considered, initial tournament admission atomic, queued jobs recoverable |
| Preflight and health | `src/App.tsx:8038`, `:8105`, `:14618` | `diagnose`, `repair` | Runner readiness, API paths/origin/health, inbox/outbox/status/log availability; errors explain the failed dependency |
| Outbox import and recovery | `src/App.tsx:7728`, `:7813`, `:7919`, `:7973`, `:8985` | `import`, `recover`, `deduplicate` | Job-specific association, import fingerprints, skip component/intermediate/quarantined files, retry an import without regenerating paid output, recover unimported completed outputs |
| Logs and lifecycle | `src/App.tsx:7600`, `:7629`, `:7678`, `:12817`, `:14695` | `logs`, status events | Job ID, label, status, timestamps, log/status/outbox paths, diagnostic category, user message and suggestion, cancellation/exit state; original log content retained |
| History and persistence | `src/App.tsx:7836`, `:7851`, `:9340`, `:9533`; `src/lib/storage.ts` | `history`, `select`, `adopt`, `restore` | Sources and derivatives, source fingerprint, recipe and quality metadata, annotations, frames, actions, pending jobs and adopted marker survive reload; no silent duplicates |
| Downloads | `src/App.tsx:10144`, `:15339`; `src/lib/exporters.ts` | `export` | Explicit artifact paths and types; image/GIF previews in Jarvis; no invisible browser download left as the only output |

### Progress fidelity

Upstream's percentage is time-estimated, not measured: `src/App.tsx:467` uses 8/10/30/18-minute estimates for image/edit/animation/effects; `:16424` maps elapsed time to 7–96%, and `:16434` similarly guesses starting/generating/checking. These must not be represented as real measurements in Jarvis. Preserve equivalent progress visibility using actual runner state, completed candidates/directions, import/QC/export phases, and an indeterminate indicator when the runner reports no denominator. A completed task must correspond to durable artifacts and final processing, not elapsed time.

Logs are polled every two seconds, tail size 32,768 bytes and recent log panels limited to three jobs (`src/App.tsx:198`). Full logs remain addressable by path. Runner capacity polling is every second; health polling every fifteen seconds.

### Original providers and modes

`src/types.ts:1` and `server/index.ts:444` define four original providers:

| Provider | Actual original contract | Verification boundary |
| --- | --- | --- |
| `codex-handoff` | Local authenticated `codex exec`, original job contract, status/log files and outbox | Four real AI workflows and later recovery/batch evidence are recorded below. A CLI/account readiness failure must remain an explicit dependency error. |
| `local-file` | Import existing image files from the user's workspace | Real image imports, edited-source reuse and library imports were exercised. This path does not generate new art. |
| `local-inbox` | Import and recover existing results from the handoff outbox | Actual generated outputs, job association and duplicate protection were exercised. Import retry is tested independently from generation retry. |
| `local-generator` | Internal procedural PNG generator in `server/local-generator.ts`, served by the same real `/api/generate` endpoint; image and sprite workflows only | Browser B50 launched the actual bundled server and exercised manual generation and worker generation. No fake HTTP response or external model endpoint was substituted. This proves the original diagnostic/development fallback, **not** AI quality or a substitute for the four Codex flows. |

No additional external generator endpoint is required for the original procedural mode. Disabled Hatch functionality and legacy-only variants remain identified as disabled upstream; neither is presented as a successfully exercised active workflow.

## Pixel-art generation and source management

| Feature | Source | Proposed mapping | Runtime evidence |
| --- | --- | --- | --- |
| Generate raster art with Codex | `src/App.tsx:7013`, `:7094`, `:7497` | `generate` | A newly generated PNG returned by the real Codex runner, imported into history and previewed in the chat |
| Image import, multiple files and drag/drop | `src/App.tsx:6986` | `import` | Import actual supported images; preserve real dimensions, source names and data |
| Prompt example catalog | `src/App.tsx:3073`, `:9603`, `:9624`, `:14826`; `docs/prompt-examples/` | `catalog prompts`, `generate` using example | Search/read/load/copy equivalent; complete categories retained, not a few examples |
| Procedural fallback | `src/App.tsx:8131`; `server/local-generator.ts` | `generate` with explicit local provider | Deterministic artifacts tested separately and never labeled real Codex generation |
| Source selection/adoption/download | `src/App.tsx:9259`, `:9340`, `:9362`, `:9533` | `select`, `adopt`, `export` | Selected source/derivative identity remains stable across operations |

Generation defaults (`src/App.tsx:4805`): prompt and negative prompt are ordinary editable text; seed `24682`, size `1024x1024`, count `1`, quality `auto`. Some tuning controls are hidden by the upstream compact UI but still exist in the draft.

Prompt catalog families: basic characters, profession characters, monsters, monster-girl chibi, animals, chibi boys, chibi girls and beastfolk (`src/App.tsx:3073–3098`). The maintained integration should enumerate the actual catalog rather than hardcode an abbreviated list.

## Image editing and annotation

| Feature | Source | Proposed mapping | Runtime evidence |
| --- | --- | --- | --- |
| Source-preserving edit handoff | `src/App.tsx:7191`, `:13213`, `:13245` | `edit` | Real source plus edit instruction returns a new image; original retained with derivative linkage and before/after preview |
| Numbered rectangular regions and comments | `src/App.tsx:4073`, `:9455`, `:9507`, `:13273` | `annotate`, `edit` | Agent supplies source-pixel coordinates and comment; generated edit changes the intended region |
| Coordinate mapping | `src/App.tsx:4048`, `:4073`, `:13290` | Annotation coordinates | Preserve original image dimensions and map display coordinates accurately; do not send display-space rectangles as source-space rectangles |
| Brush/rectangle/arrow annotations, color, deletion/clear | `src/types.ts:3`; `src/App.tsx:4815`, `:9455`, `:9517`, `:9525`, `:16332` | `annotate`, `annotation remove/clear` | Exercise each retained annotation type and persisted comments, then an edit with annotations |
| Final-result protections | `src/App.tsx:7154` | Edit validation | Animation/effect final sheets cannot accidentally become ordinary image-edit sources; user sees a clear reason and intended source-selection path |

The active image-edit UI primarily presents numbered rectangles. Other annotation tools exist behind low-priority UI controls. They remain part of the source capability inventory rather than being silently discarded.

## Character animation

### Generation parameters and recipes

Source: `src/App.tsx:192–334`, `:2776`, `:2844`, `:7094`; `src/lib/motionRecipes.ts`.

- Source is a static original/imported image; finalized animation/effect sheets are rejected as sources (`src/App.tsx:7164`, `:13405`).
- Direction sets: five (`front`, `front three-quarter`, `side`, `back three-quarter`, `back`), three (`front`, `side`, `back`), or any single supported direction; default five, default single side (`src/App.tsx:208–237`).
- Frames per direction: `4, 6, 8, 12, 16, 20`; 16 and 20 are marked experimental. Default 8.
- Frame cell: standard 256×256; original direction image grids and composed rows remain unchanged.
- Body topologies: biped, quadruped, serpentine/body-contact, floating, winged-flying, multi-leg. Contact validation changes with anatomy rather than assuming every source has human feet.
- Modifiers: intensity (`subtle/normal/strong`), tempo (`slow/normal/fast`), weight (`light/normal/heavy`), exaggeration (`low/normal/high`), handedness (`inherit/left/right/ambidextrous`), weapon class (`none/unarmed/sword/heavy-weapon/polearm/bow/firearm/staff/shield`), travel (`in-place/short/medium/long`), secondary motion (`low/normal/high`), VFX amount (`none/low/normal/high`).
- Recipe compiler validates supported topology/modifier combinations and records applied/removed modifiers, contract/recipe/compiler versions, phases, negative prompt, contact constraints and FPS (`src/lib/motionRecipes.ts:659`, `:747`, `:819`).
- Preserve all 22 recipes: `idle-breathing`, `walk-cycle`, `run-cycle`, `basic-attack`, `hurt-reaction`, `death-downed`, `spell-cast`, `jump-hop`, `guard-block`, `victory-cheer`, `interact-pickup`, `ranged-attack`, `skill-release`, `knockback`, `item-use`, `talk`, `dash`, `dodge-roll`, `charge-heavy-attack`, `combo-attack`, `stun`, `get-up`.
- Recipe browsing supports search, family, verified/experimental, topology, loop, weapon, movement, frame count, favorites and recent selections (`src/App.tsx:15146`).
- Chroma is selected from green or magenta by analyzing the actual source palette (`src/App.tsx:13435`), not blindly fixed to green.

### Production flow and quality behavior

| Feature | Source | Proposed mapping | Runtime evidence |
| --- | --- | --- | --- |
| Compile recipe and direction-specific generation | `src/App.tsx:7094`, `:13467`, `:13519` | `animate` | Real direction sheets from source identity and original compiled recipe |
| Profiles Fast/Balanced/Best | `src/lib/animationTournament.ts:69` | `animate` profile | Fast: 1/1 candidates; Balanced: 2 initially, optional third for uncertainty; Best: 3, strict pass / Smart Race / full comparison |
| Persisted tournament and capacity | `src/App.tsx:5966`, `:6163`, `:6453`, `:6517`, `:7319`, `:7373` | `tournament status/start/cancel` | Reopen/reload resumes observation without duplicate submissions; explicit cancelled jobs do not restart |
| Candidate evaluation, ranking and publish | `src/App.tsx:6408`, `:6743`, `:8426`, `:8459`; `src/lib/animationTournament.ts` | `tournament evaluate/adopt` | Rank actual candidate outputs; only accepted output enters normal history; alternatives/evidence retained |
| Source/direction composition | `src/App.tsx:13671` | Postprocessing | Split actual grids; validate dimensions/frame counts; nearest-neighbor normalization and original direction order |
| Transparency/component cleanup | `src/App.tsx:13801`, `:14233`, `:14362`, `:15883`, `:16001`, `:16176`, `:16224` | `process` / automatic import | Connected background removal, chroma/guide cleanup, primary-component preservation, detached-garbage thresholds, edge residue cleanup/despill |
| Stable scale/contact normalization | `src/App.tsx:13885` | Automatic animation normalization | Shared scale, consistent anchor/footline, retained per-frame correction records; source anatomy respected |
| Structural and temporal QC | `src/App.tsx:13977`; `src/lib/animationQuality.ts` | `quality` | Empty/clipped/detached components, holes, drift, silhouette/size consistency, static-copy/weak-motion, loop seam; preserve hard failures and warnings distinctly |
| Quality v2 | `src/lib/animationQuality.ts`; `src/types.ts:55` | `quality` report | Raw/normalized metrics, identity, palette, silhouette, contact, loop, phase, motion and correction scores; shadow policy remains distinguishable from hard gate |
| Targeted direction repair and acceptance | `src/App.tsx:6564`, `:6588`, `:6608`; `src/lib/animationTournament.ts:271` | `repair directions`, `accept repair` | Generate only selected directions and verify all untargeted direction hashes unchanged |
| Motion pilot (experimental, default off) | `src/App.tsx:263`, `:6333`, `:6645`; `src/lib/motionPilot.ts` | `animate` pilot / fallback / revalidate | Best with 3/5 directions only; pilot adoption/expansion, explicit Balanced fallback and revalidation |
| Batch source×recipe matrix | `src/App.tsx:6654`, `:6666` | `batch animate` | Multiple sources/presets obey runner admission; each job remains attributable to its source and recipe |

### Review, timeline and exports

| Feature | Source | Proposed mapping | Runtime evidence |
| --- | --- | --- | --- |
| Candidate visual review | `src/AnimationReviewCockpit.tsx`; `src/App.tsx:6221` | `review inspect` | Candidate/direction/frame comparison with play/pause, speed, raw/normalized mode, source overlay, onion skin, pixel diff, background and contact/pivot/socket/hitbox/hurtbox overlays |
| QC matrix and calibration | `src/lib/animationReview.ts:8`, `:148`, `:171`, `:202` | `review report/export` | Per-direction/frame heatmaps for foot drift, bounds, identity, seam, static copy and clipping; full calibration JSON |
| Explicit review decisions | `src/AnimationReviewCockpit.tsx`; `src/lib/animationReview.ts:88`, `:122`; `src/App.tsx:6283`, `:6297` | `review decide/adopt` | Winner/reject/hold, reason tags, notes (up to 1,000 chars), manual winner and direction repair. Agent-authored review must not pretend a human approved it |
| Timeline editing | `src/App.tsx:15481`; `src/lib/animationPackV2.ts:216–280` | `timeline edit` | Per-frame duration, hold, duplicate, reverse, loop/one-shot/ping-pong, move event, per-direction overrides; undo/redo and reset-to-recipe defaults |
| Geometry/event metadata | `src/App.tsx:15752`; `src/lib/animationPackV2.ts:421` | `timeline metadata` | Add/edit pivots, anchors, named sockets, hitboxes, hurtboxes, trim rectangles and events with normalized coordinate validation |
| Timeline save and round trip | `src/App.tsx:9905`, `:9922` | `timeline save`, `pack verify` | Stored metadata survives export/import, including direction timing/event overrides |
| GIF/APNG/animated WebP | `src/App.tsx:9203`; `src/lib/exporters.ts:69`, `:117`, `:181` | `export` | Actual multi-frame files with intended timing/alpha and individual direction previews, playable in the chat |
| PNG sheet and individual frame ZIP | `src/lib/exporters.ts:31`, `:58` | `export` | Pixel dimensions and transparent frames verified; correct frame order/grid |
| Animation packs v1/v2 and engine data | `src/lib/exporters.ts:243`; `src/lib/animationPack.ts`; `src/lib/animationPackV2.ts:305`, `:404` | `pack export/import` | Legacy migration, safe ZIP paths/size limits, metadata and previews; generic, Godot, Phaser, Aseprite and Unity JSON |
| Reusable animation library | `src/App.tsx:9715–9857`; `src/lib/officialAnimations.ts` | `library import/list/use/rename/delete/export` | Source supports official/user libraries and pack reuse; UI hidden in this upstream version, so expose only with correct source classification |

The timeline, review and VFX React subcomponents own meaningful state and operations. The adopted suite retains these components and exposes their live controllers to the agent, so manual edits and tool calls use the same state and library functions.

## Effect animation

Source: `src/App.tsx:427–704`, `:8728`, `:8810`, `:8828`; `src/lib/effectQuality.ts`.

All 11 categories and 33 types:

| Category | Types |
| --- | --- |
| `slash-arc` | `crescent`, `cross`, `spinning` |
| `hit-spark` | `burst`, `metal`, `shield` |
| `magic-cast` | `circle`, `runes`, `summon` |
| `projectile` | `bolt`, `fireball`, `arrow-trail` |
| `impact` | `explosion`, `dust`, `shockwave` |
| `telegraph-aoe` | `danger-ring`, `cone`, `target-zone` |
| `aura-status` | `power-aura`, `poison-status`, `freeze-status` |
| `heal-buff` | `heal-rise`, `stat-up`, `cleanse` |
| `barrier-shield` | `dome`, `guard-plane`, `shield-pulse` |
| `spawn-portal` | `portal-open`, `spawn-flash`, `despawn-wisp` |
| `movement-trail` | `dash-trail`, `footstep`, `landing-dust` |

Options: styles `pixel-clean/painterly-soft/arcade-bold/anime-flare`; palettes `cyan-white/violet-gold/ember-orange/toxic-green/mono-white`; frames `4/6/8/12/16`; canvas `64/96/128/192/256`; grid `grid-4x2/horizontal-strip/grid-4x4`; loop `one-shot/loop/ping-pong-loop`; anchors `center/bottom-center/caster-hand/projectile-center/impact-ground`; category-dependent normal/additive/screen blend and energy envelope. Prompt, negatives and notes remain available.

Defaults: slash arc/crescent, clean pixel style, cyan-white, 8 frames, 128×128, 4×2, 12 FPS, one-shot, center; category changes can select its own default loop/anchor/blend/frame count/canvas.

| Feature | Source | Proposed mapping | Runtime evidence |
| --- | --- | --- | --- |
| Structured effect recipe and real generation | `src/App.tsx:7195`, `:8728` | `effect` | Actual newly generated effect sheet, complete metadata and recipe; no character source requirement |
| Sheet normalization and QC | `src/App.tsx:8810`, `:8828`, `:8872`; `src/lib/effectQuality.ts` | `effect quality` | Exact cell/frame count, transparency and alpha continuity, centroid, brightness envelope/peak, clipping/overdraw, palette and loop score |
| Effect exports | `src/App.tsx:9215–9254`; `src/lib/exporters.ts:288` | `export` | GIF, APNG, sheet PNG, individual-frame ZIP, metadata JSON, complete effect pack |
| Effect preview backgrounds | `src/App.tsx:698`, `:11126` | `preview` | Checkerboard/dark/light/neutral for alpha inspection; export transparency preserved |

## Character + effect composition

Source: `src/VfxCompositeStage.tsx`; `src/lib/vfxComposite.ts`.

- Select existing character animation and effect; choose character direction.
- Attach to `hand/weapon-tip/feet/body-center/projectile-origin/impact-point`; synchronize to `startup/charge/active/impact/recovery`, by start or peak. Preserve fallback provenance when a socket is estimated.
- Configure front/back layer, normal/additive/screen blend, X/Y offsets, scale, rotation, opacity, start/peak frame and time scale.
- Preview backgrounds checkerboard/light/dark/game; overlays safe area/clipping/hitbox/hurtbox/effect bounds.
- Save setup, reset, undo/redo; import JSON or composite ZIP.
- Export composed GIF, APNG, PNG sheet, original character/effect layers, composite JSON and composite ZIP containing layers, previews and generic/Godot/Phaser engine metadata.
- Proposed operations: `compose`, `compose edit/save/import/export/inspect`.
- Runtime proof must combine actual generated character/effect assets, demonstrate synchronized attachment through multiple frames, and round-trip the composite manifest/pack without altering source assets.

## Legacy, hidden and administrative surfaces

| Surface | Source evidence | Parity classification |
| --- | --- | --- |
| Low-priority provider/tuning/grid/history controls | `src/App.tsx:188`, `:11292`, `:11366`, `:11682` | Implemented but hidden; retain callable operations and original validation, not the old panels |
| Sprite action editor/manual frame tools | `src/App.tsx:189`, `:9177`, `:9266`, `:9298`, `:9312`, `:9380`, `:9406` | Hidden editor supports grid split/gutter, add/move/remove frame, chroma tolerance, action/FPS/loop/cell tuning; provide equivalent tools |
| Official/user animation library UI | `src/App.tsx:191`, `:10570`; `README.md:44` | Hidden; shipped sample sheets are fixtures/demos, not newly generated user results |
| Hatch pet and directional Hatch pet | `src/App.tsx:335`, `:8293`, `:8969`, `:13360`, `:13561` | Legacy support/import/export exists, but new draft hardcodes standard and both Hatch flags false at `:7105–7107`; do not claim these are active generation modes |
| `sprite-edit` workflow | `src/App.tsx:426`, `:10095` | Legacy/internal mode; not one of the four active tabs |
| Local state safe mode/reset | `src/App.tsx:9980–10005`; `src/lib/storage.ts` | Recovery operations, not automatic destructive cleanup; preserve history and require the user's intended reset scope |
| Environment report | `src/App.tsx:4571`, `:4594`, `:5598` | Redacted Markdown/JSON report, diagnostics and health repair; no secrets in logs/chat |
| Notifications/localization | `src/App.tsx:356`, `:7650`, `:16448` | Map job completion to Jarvis notifications; all new visible Jarvis strings use pt-BR/design system |
| Local development supervisor repair | `src/App.tsx:14645`, `:14657` | Original developer-tool behavior; map packaged engine health/restart appropriately, never require a separately operated original UI |

## Required runtime evidence before claiming complete parity

The final verification record must associate each operation family above with a real command, actual job IDs, inputs and outputs, timestamps, result and artifact directory. Catalog-only inspection and mocked unit tests are useful but insufficient completion evidence.

1. Through the dedicated Jarvis agent, generate real pixel art using the production Codex imagegen path; then edit it using a numbered source-coordinate region. Show original and resulting images in the Jarvis transcript.
2. Animate an actual static source, retain all direction outputs/compiled recipe/normalization/QC, play the GIF in chat, and inspect the final sheet and timing. Exercise profile selection, non-default frame/topology/modifier settings, candidate review/adoption, targeted direction repair with untouched-hash evidence and resume/cancel.
3. Generate a real effect with non-default parameters; verify transparency/frame count/timing and preserve quality report/logs. Play its animation in chat.
4. Exercise all downstream operations on real generated assets: annotations; source/history selection; manual frame/grid/chroma operations; timeline duration/hold/duplicate/reverse/loop/events/geometry; every supported export family; pack v1 migration/v2 round trip; library operations; character/effect composition and its round trip.
5. Exercise catalog enumeration and validation for every recipe, topology, modifier, effect category/type and allowed enum, including invalid combinations. Real generation should cover each operation/flow; catalog coverage must not be confused with generating every Cartesian combination.
6. Exercise experimental pilot/fallback/revalidate and batch matrix using actual jobs, with explicit usage and output accounting. Disabled legacy generation modes must be labeled as such; supported legacy import/export compatibility needs real fixtures.
7. Observe actual agent execution in Jarvis: job progress corresponds to real state, logs are accessible and preserved, generated files appear at durable local paths, GIF frames advance, errors remain actionable, cancellation/reload/import retry do not cause duplicate paid jobs.
8. Inspect packaged runtime prerequisites and run without requiring a separately launched original app or an untracked development server. Validate trust boundaries for paths, imports, archives, annotations and job arguments; preserve project assets and production credentials.
9. Verify the shared manual/chat contract: an agent-created job/result appears in the suite tab; changing the selection or timeline manually is visible to the next agent operation; closing/reopening the tab preserves the runtime and does not duplicate jobs.

At inventory time, no generation or end-to-end test was performed by the inventory investigation. No parity row is evidence of an implemented or validated Jarvis command until the integration records that proof.

## Verification harness design

The runtime sequence reuses generated artifacts between operations. It does not repeat expensive generation for each export format. Concrete requests in `scripts/pixel-cockpit/runtime-smoke.ts` are bound to the adopted runtime's exported catalog; each real receipt records the command and arguments alongside its result.

| Scenario | Inputs and operations | Required evidence |
| --- | --- | --- |
| Catalog and preflight | Inspect the runtime catalog, recipe/effect catalogs and health; validate all supported enums and reject malformed arguments | Catalog snapshot with every operation family mapped; actual runner identity/readiness and isolated output root; no credentials in report |
| Source and edited derivative | Generate one new source through Codex, select/adopt it, create a bounded annotation with comment, edit through Codex, import/reopen the result | Two real job receipts/logs, source and derivative hashes/dimensions, changed-region evidence and chat previews |
| Character production | Animate the source with the selected original recipe/profile/directions; inspect and save review; export current best candidate | Candidate jobs and decision, raw direction sheets, normalized sheet, source fingerprint, quality/normalization report, animated preview |
| Repair and recovery | Request a targeted direction repair, record accepted untargeted hashes before/after, retry import, reload observer, inspect persisted job state | Only intended directions change; no duplicate creation after reload/import retry; recorded cancel/resume behavior |
| Frame and timeline editing | Split sheet, add/move/remove a frame, chroma cleanup on a copy; modify duration/hold/duplicate/reverse/loop/event, add/edit geometry, undo/redo, restore/save | Copy-local edits with original asset hashes unchanged; timeline metadata before/after; each operation recorded |
| Export and library | Export PNG sheet, frame ZIP, GIF, APNG, animated WebP, v1/v2 packs, all five engine metadata formats; import/use/rename/export/delete the imported library entry | File signatures, dimensions/frame counts/delays, safe ZIP paths, resolvable manifest references, semantic round-trip equality |
| Effect production | Generate one real effect with non-default settings, inspect quality, export sheet/frames/GIF/APNG/metadata/pack | Actual effect job/logs, transparent pixels and all requested frames, original quality scores and animation preview |
| Composite | Combine real character and effect, choose direction/socket/event, retime start/peak, transform/layer/blend, undo/redo/reset/save/import; export preview/sheet/layers/pack/engine metadata | Retained source layers, animated composition, exact manifest round trip, Generic/Godot/Phaser references |
| Advanced generation | Execute batch, Motion Pilot and explicit fallback/revalidate only after initial four-flow success | Actual source×recipe jobs, admission counts, pilot-to-expansion lineage, explicit additional-use accounting |
| In-product verification | Execute through the exclusive Jarvis pixel-art agent, open actual generated artifacts in the transcript and inspect progress/logs during each phase; use the suite tab to change selection/timeline between agent operations | Native tool calls/results, durable artifact paths, moving GIF, true job lifecycle and clear recoverable failures; manual/chat actions observe the same state |

Existing upstream tests provide useful assertions, but they do not replace these live scenarios:

- `src/lib/animationPackV2.test.ts:46`: ping-pong does not duplicate seam endpoints; `:59` geometry/events serialized; `:79` semantic round trip; `:119` all five engine metadata formats; `:137` ZIP includes manifest, sheet and engine handoffs.
- `src/lib/vfxComposite.test.ts:17`: attachment/transform/timing round trip; `:26` named socket vs legacy pivot fallback; `:35` loop/one-shot/ping-pong frame mapping; `:43` event synchronization; `:51` separate layers and engine references.
- `src/lib/animationReview.test.ts:64`: eight quality dimensions; `:71` accessible QC cells; `:78` persistent decisions; `:89` calibration metadata.
- `src/lib/animationTournament.test.ts:53`: exclusive initial-wave admission; `:114` strict solo gate; `:190` Smart Race; `:293` deterministic ranking; `:307` untargeted-direction hash protection.

Artifact inspection establishes file integrity, frame counts, timing, dimensions, checksums and metadata references. It must explicitly remain separate from visual-quality acceptance and proof that the agent and Jarvis transcript actually exercised the workflow. Evidence for live generation must identify a real Codex job rather than a procedural or imported fixture.

### Native verification driver

The opt-in `browser-probe` binary switches to `src-tauri/src/agent/pixel_art_probe.rs` when `JARVIS_PIXEL_PROBE=1`. It creates an isolated workspace, project and conversation under `.codex/pixel-review/<run-id>/private-home`; it does not edit the user's Jarvis database. The actual adopted engine inherits the authenticated Codex CLI environment. Its loopback driver requires the random token stored in that run's `connection.json` and permits only the four native pixel-art tools.

Run `bun scripts/pixel-cockpit/run-native-probe.ts <connection.json> --generate` to explicitly submit the four real generation workflows and subsequent editor/export scenarios. Without `--generate`, supply `--source=<id> --animation=<id> --effect=<id>` for assets already in that same live session. The driver uses `pixel_art_docs` for the catalog, `pixel_art_run` for original actions, and `pixel_art_status` for the returned job IDs. It never resubmits an uncertain generation. `JARVIS_PIXEL_PROBE_KEEP_OPEN=1` preserves the visible suite for manual inspection after the driver finishes.

`native-tool-report.json` records operations, receipt IDs, original logs, artifacts, terminal errors and the precise limits of that run. `artifact-inspection.json` records hashes, structure, dimensions and animation timing; its alpha flag describes format support rather than a visual transparency verdict. `suite-document.json` records the actual CEF document and exposed controllers, not a screenshot or visual acceptance. Generated files and original handoff logs remain under the private home's pixel-cockpit session directory. This driver proves native tools plus the actual original runtime; a real dedicated-agent conversation, chat preview playback and manual/agent interleaving remain separate acceptance evidence.

The dynamic catalog is the authority for exact parameters. Shared UI actions use `invoke {controller,method,args}` (`app`, `timeline`, `review`, `vfx`); pure source utilities use `invoke {module,method,args}`. Argument references resolve current shared state, actual history/frame/action IDs, or saved utility results. `timeline_open`, `review_open` and `vfx_open` mount the original controllers before invoking their actions. The source inventory's descriptive mappings above describe feature families rather than additional invented APIs.

The probe's `/agent` route uses an actual enabled Codex account through the Jarvis runtime and `builtin:pixel_art`; it copies only the account identity into the private database and leaves credentials in the native secret store. Its transcript and report are separate from the dispatcher-only report. `/ui` records the production chat components and suite-tab hooks through the native CEF frontend, with no simulated event bridge.

`JARVIS_PIXEL_PROBE_OUTPUT` can resume an existing direct child of `.codex/pixel-review`; the probe reuses that private project's conversation. Stop the probe and its owned engine before copying private data or changing the profile. A diagnostic migration between `.jarvis-dev` and `.jarvis` must preserve the source, rebase only absolute path-valued JSON fields within that exact private prefix, and verify persisted job previews and shared suite state after reopening. Never migrate or replace the user's production database for this test.

### Recorded native run: 2026-10-20

The local evidence directory `.codex/pixel-review/a19cda3ac28068bf1033f1be02f8cda9` preserves the original native run, including `native-tool-report.json`, `artifact-inspection-rechecked.json` and `edit-pixel-evidence.json`. It contains actual Codex generation, not fixture or procedural substitutes. Its 98 native operations completed without a command failure; all 36 exported artifacts passed structural reinspection. The initial report retains a verifier false positive for a v1 pack's `direction: "side"` metadata; the corrected verifier and regression distinguish direction labels from file references.

| Flow | Actual Codex job | Observed output and limits |
| --- | --- | --- |
| Image generation | `codex-job-2026-10-20T05-40-18-425Z-vkxbzh` | Original blue robot, transparent PNG 1254×1254; actual provider dimensions retained rather than reported as the 1024 hint |
| Image editing | `codex-job-2026-10-20T05-43-35-427Z-11p6sm` | Independent derivative; decoded comparison finds 19,227 changed pixels confined to `[393,480,528,660]`, with zero alpha changes |
| Character animation | `codex-job-2026-10-20T05-49-31-685Z-jp38dp` | Four real side-walk poses, sheet 1024×256 and GIF 256×256 with four 80ms frames; original normalization, quality metrics and warning retained |
| Effect animation | `codex-job-2026-10-20T05-58-21-414Z-y8eyr1` | Eight real 128×128 burst frames, transparent sheet and GIF, original gold quality result; requested overrides were four frames at 96px, exposing a preset-reset defect that requires corrected real execution |

This run also exercised the actual timeline controller (duration, hold, duplicate, reverse, ping-pong, geometry, events, undo/redo, save and round trip), review overlays/calibration, character/effect composition, all requested export families and outbox recovery/deduplication. Commands and artifacts remain in the report; source and derivative files are retained under `project/fixtures` for later tests without generating them again.

This is dispatcher-and-engine evidence. It does not by itself establish full-agent selection, chat GIF playback, manual/agent state interleaving, artistic approval, advanced pilot/batch/repair coverage, or corrected effect-override behavior. Those require their own actual evidence. The private runtime profile was copied after shutdown with the original retained; `private-migration.json` records 48 JSON files whose absolute private paths were rebased, without rewriting prompts or logs.

### Evidence boundary when verification was stopped

The user explicitly stopped further generation and extensive validation on 2026-10-20 to preserve account limits and test the integration themselves. No batch or Motion Pilot generation was started. The following matrix distinguishes actual exercised operations from equivalent entry points and unverified behavior; a callable catalog entry does not mean its execution passed.

| Public operation family / entry points | Evidence classification and observed result |
| --- | --- |
| `catalog`, `state`, `inspect`; recipe/topology/profile catalogs | Actual native runtime, original catalogs and generated assets. Native report records the requests' results; browser reports retain exact arguments. |
| `generate`: image, edit, sprite, effect | Four actual Codex flows produced the files listed above. The first effect used original default 8×128 because of the discovered override defect. Corrected 4×96 settings and restoration passed browser validation; corrected real generation was not completed before testing stopped. |
| `select`, `selectHistoryResult`, `selectSourceFromPreview`, `adoptSelected` | Selection/adoption exercised on real generated source and derivative. Alternate UI handlers sharing those operations are equivalent entry points, not additional independent generation proofs. |
| `annotate` rectangle/brush/arrow; `updateAnnotationComment`, `removeAnnotation`, `clearSelectedAnnotations` | Native rectangle drove the actual image edit; all three drawing tools and annotation maintenance ran in the real browser on an imported fixture. |
| `preset`, body/frame/profile/motion settings, favorites and prompt examples | Recipe selection and non-default frame/direction/profile settings exercised. Catalog and parameter validation covered allowed options; every recipe/topology combination, favorite toggle and prompt-example shortcut was not generated independently. |
| `splitSelectedToTimeline`, `addSelectedAsFrame`, `moveFrame`, `removeFrame`, `applyChromaToSelectedFrame`, `updateCell`, `updateActiveAction` | Real browser operations on imported image pixels, with actual exported PNG. These are processing tests, not AI-quality claims. |
| Timeline open/configure/commit, duration, hold, duplicate, reverse, ping-pong, point/rect geometry, event movement, undo/redo, save, round trip, v2 export | Actual native controller and original library operations using the generated four-frame animation. Browser independently verified timeline duration, save/export/reimport and stale-controller rejection. Reset-to-recipe and every individual geometry field remain unverified. |
| `review_open`, review playback/overlays, calibration export | Actual native review on the generated tournament. Human decision, winner adoption, repair, pilot admission, expansion, fallback and revalidation were not completed. |
| `export`: image, directional GIF/APNG/WebP, animation pack; frame ZIP and metadata utilities | Actual generated assets; archive integrity, frame counts/timing and references verified. v1/v2 compatibility and five original animation-engine metadata formats exercised. |
| Effect GIF/APNG/sheet/frame ZIP/metadata/pack | Actual generated eight-frame effect; files inspected and original quality report preserved. Non-default effect generation after the override correction remains unverified. |
| `vfx_open`; character/effect selection, timing, transform/layer/blend, undo/redo/save; GIF/APNG/sheet/pack/manifest/layers | Actual generated character and effect combined by the original controller. Exported artifacts passed inspection. Composite import/reset and every socket/event/blend combination were not independently exercised. |
| `import_image`, official library use, `import_animation_pack`, user library rename/export/delete | Actual original sample and exported v2 pack exercised in browser. Imported sample is clearly separate from the four Codex-generated assets. |
| `recoverUnimportedOutboxResults`, `dedupeLocalInboxHistoryNow`, health check | Actual native session exercised these calls without regenerating confirmed output. Persisted-history dedupe, failed-import retry, packaged repair/restart, and scoped state reset remain unverified. |
| `exportEnvironmentReport` | Redacted JSON/Markdown adapter implemented; final browser evidence records export checks. No account token is included in the evidence contract. |
| `pixel_art_cancel`, `cancelCodexJob`, tournament cancellation | A real early-cancel scenario found a defect: the receipt became `cancelled` before a subsequently submitted Codex child had stopped. This scenario is **not a passed cancellation proof**; see details below. No tournament cancellation was tested. |
| Native `save_artifact` | Targeted native regression validates copying and no-clobber behavior. The planned live copy/hash test was not run after the user stopped verification. |
| Shared manual/agent state, actual ChatArea Composer, approval, progress/logs and GIF playback | Actual native ChatArea and dedicated-agent submission were started. A full approved generation, moving GIF in the production transcript, and bidirectional manual/agent editing were not demonstrated before the user stopped the probe. |
| Batch matrix, Motion Pilot, targeted direction repair, fallback/revalidation and lost-ACK LLM recovery | Implemented interfaces and targeted tests do not replace live execution. These actual-provider scenarios remain unverified; none was silently launched after the stop request. |
| Disabled Hatch generation and legacy internal modes | Disabled upstream functionality is documented as such. Its presence in source is not counted as an active generated workflow. |

Browser evidence `.codex/pixel-cockpit-browser/2026-10-20T06-28-27-937Z/evidence.json` supersedes the earlier browser snapshots for the adopted UI revision. Expected rejection cases must be reported as validated rejection behavior, not as successful execution of the rejected operation. The original native 98-operation report predates per-request argument recording in the runner; preserve that limit instead of backfilling invented request records.

The isolated cancellation session `cbce4b758e42584d7b4b44292c02b7e0` recorded native operation `5a5b09238a0fdb6615287e26678efb7e` and real Codex job `codex-job-2026-10-20T06-32-21-413Z-oyki64`. The receipt reported cancelled at `06:32:25Z`, but the original status file still said running and its child process was observed alive. Evidence is retained in `advanced-cancel-1792477928554/cancel.json` and the session's original status/log files. The user then closed the diagnostic window; the host exited with code 0, both identified host/child PIDs disappeared, and no process using that private run path remained. The additional cancellation request could not be sent after the host closed. Do not rewrite the original running status file to pretend a cancellation acknowledgment was received.

The first read-only request after mounting a new CEF host also exposed a readiness race: JavaScript returned an undefined response before the bridge mounted. A later read succeeded. The native agent prepared a bounded readiness-read retry and regression; this source change still requires the next build before it can be claimed as the behavior of the last observed executable.

### Resumed verification

The user subsequently authorized completing the remaining actual-provider checks. The preceding stopped-run boundary remains historical evidence; it is not a claim that later work was cancelled or that the unverified scenarios passed.

On the updated native diagnostic host, `save_artifact` copied the confirmed four-frame GIF into the isolated project. The source, copied file and both files after the rejected repeat share SHA-256 `4f0214f3a58966cfe7583b2562f46cd7f34993b43cb9894f2afeefde5effb9bb`. A new call targeting the same destination returned HTTP 400 without overwriting it. `native-save-proof.json` records both real native calls and the four hashes. This test did not start a generation or modify the original artifact.

The verification driver now relies on the native round-trip receipt and the original semantic round-trip check rather than matching English/Japanese words in the UI confirmation. Its regression accepts a successful localized confirmation and rejects a failed receipt. The focused runtime-driver and artifact-inspector run passed 12 tests in two files.

The interrupted real Composer operation is distinct from the initial eight-frame effect. Its original Codex job, `codex-job-2026-10-20T06-31-52-415Z-yz2n0k`, produced a real intermediate PNG before the diagnostic host closed. Its native operation remains interrupted; an intermediate image is not a completed effect, valid export, cancellation acknowledgment or recovered old receipt. Any processing recovery must be explicit, preserve provenance and original prompt history, and must not automatically repeat image generation.

The corrected runtime subsequently passed two separate native cancellation scenarios. The early request (`advanced-cancel-1792479924056`) observed `running → cancelled` before an original Codex job was submitted. The active request (`advanced-cancel-active-1792479983457`) observed original job `codex-job-2026-10-20T07-06-38-408Z-lwpopp` running with PID 9684, then its terminal `SIGTERM` status and the disappearance of that process before the native receipt was accepted as cancelled. Both retained the imported source hash. The original raw status uses `failed` for the terminated runner; the native operation distinguishes user cancellation. Its original tournament-specific cancellation message was identified as misleading for a standalone effect and reported for correction.

The first active-cancellation driver attempt was rejected before dispatch because its test call ID collided with a previous session's recorded call. The driver was corrected to use a conversation-specific call ID; the rejected attempt is retained as such, not counted as a generated job or successful cancellation.

The real dedicated-agent conversation then recovered the interrupted effect through a new explicit Composer request. The original job finished processing at `07:14:12Z`, preserving its job ID and the original generated source SHA-256 `e1166d38c723bedc650169e86824ae7e0f7a196bb0409f23af1d094aa98bbd98`. The separate recovery operation does not rewrite the old interrupted native receipt into a successful acknowledgment. Its runner segment records normalization and exports from the preserved source; the test did not request another image generation.

`same-job-recovery/artifact-inspection.json` links the actual agent export receipts to a 384×96 PNG sheet, a 96×96 GIF with four 80ms frames and effect metadata. The GIF is 5,774 bytes with SHA-256 `6ebbf0cbac1e6f0e88870499b737c6aec17bf6ed56cf27421d76ca6f005f6791`; its native attachment has MIME `image/gif`. This demonstrates corrected four-frame/96px processing and export on an actual provider-produced image. Native export evidence alone still does not establish that the GIF advances in the chat; that observation must be recorded separately.

`render-sampling-20261020T082327872/animation-render-proof.json` subsequently records the production ChatArea displaying those unchanged GIF bytes. Eight captures of the same native CEF image rectangle produced three distinct rendered hashes; `frame-0.png` and `frame-1.png` were visually inspected and show different explosion/dissipation phases. This is actual rendered-animation evidence. The earlier `canvas.drawImage` hash was not used as proof of playback. Successful suite viewport and manual/agent shared-edit checks are recorded independently below.

`pixel-suite-hook-proof.json` records the native suite child remaining visible at `(0, 72)` with dimensions `1280×828`, matching the actual chat-tab slot after capture. Its bounds were driven by the production suite hook rather than a diagnostic manual-resize endpoint. The three-column suite screenshot was inspected. Earlier `1024×768` observations were preserved as the viewport regression that led to this fix; they are not counted as successful acceptance.

`shared-edit-proof.json` passed at `09:18:16Z`: entering text through the real suite textarea was reflected by `pixel_art_run state`; configuring through the agent tool updated that same existing field before any UI input event. The original prompt and workflow were restored, all three existing project-export hashes remained unchanged, and an oversized UI edit was rejected with HTTP 400. The test generated no model images and retained the correct `1280×828` viewport. `shared-ui-to-agent.png`, `shared-agent-to-ui.png` and `suite-translation-final.png` preserve the UI evidence. The final screenshot was visually inspected: all three columns fit, and the footer and preview grade are in pt-BR.

`advanced-postprocessing/operations.json` and `result.json` record 20 additional real native operations without generating another image. The original timeline's reset-to-recipe changed the draft, and undo restored the saved pack exactly. The character/effect composite ZIP and JSON each imported a manifest equal to the original exported manifest. Composite reset changed the settings, and undo restored them exactly. The original selection was restored, and the source, edited image, animation sheet and effect sheet retained their four original SHA-256 hashes.

`lost-ack-evidence.json` records an actual Codex-agent recovery against the previously completed `save_artifact` call `save-native-1792479755198`. Only its acknowledgment was withheld in the diagnostic conversation. The agent called `pixel_art_status` with that exact call ID, then `recovery_resolve` with `outcome: applied` and the actual status-evidence call; the runtime returned `pending: false`. It did not repeat the copy, export or image generation. The original generic probe report remains unchanged: it incorrectly required a fresh catalog/mutation pair for this read-only recovery scenario, and the diagnostic route omitted the production turn finalizer. Both probe-source issues were corrected separately; this evidence proves recovery-tool behavior, not the unrebuilt fixture's final UI state.

`advanced-batch-1792480051707/actual-cell-evidence.json` records the actual minimum original batch: two real source images × two recipes, one admitted Fast candidate per cell. All four Codex processes ended, three cells published original accepted artifacts, and the second cell's idle animation was rejected by the original quality check for almost-static motion (1.1% average frame change). Its raw images and original internal attempts remain preserved; no failed candidate was relabeled as accepted. All four cells used the recipes' original eight-frame batch setting, not the unrelated four-frame draft hint.

The batch exposed a terminal-state bug: the rejected candidate was marked `failed` while its tournament retained `running`, so the aggregate observer continued waiting after the fourth real process ended. The older receipt and driver connection failure remain unchanged as evidence. The corrected observer has a source regression for this actual manifest; reconciliation and UI verification on the consolidated runtime are separate checks. The diagnostic host was closed only after all four real Codex PIDs had disappeared. `pre-restart-1792482745206/preservation.json` records copies and hashes of 237 private JSON files (32,698,018 bytes), including receipts, source contracts, tournament manifests and provenance. Original binary artifacts were left untouched.

The consolidated native maintenance run (`advanced-maintenance-1792483053520/operations.json`) records 41 operations: 40 completed calls and one expected rejection of an unsupported recipe/topology combination. It covers prompt/example selection, animation favorites, supported topology/frame controls, batch selections, download/source selection, outbox recovery and deduplication, health/repair and environment reports. The invalid combination left the controller usable. The original source SHA-256 remained unchanged.

`reset-populated-1792483863583` exercises the four reset scopes with real imported state: frames 33→0, user library 1→0, and removal of the user source from history and the combined state. The original App automatically seeds its shipped example when history becomes empty; the remaining example and its 32 frames are recorded rather than mislabeled as undeleted user data. The original source and original/exported ZIP hashes stayed equal. An initial diagnostic assertion used the wrong frame field (`sourceImageId` instead of the original `sourceId`); that failed assertion is retained, and the corrected check was applied to the unchanged recorded operations without repeating them.

`import-retry-1792483850379` records a real import failure and recovery without starting another Codex runner. The driver copied the actual four-frame effect contract, manifest and sheet into a disposable session, damaged only that copied PNG, and invoked the original import path. Its real decode failure produced the original failure notice. After restoring the exact bytes, `retryCodexFailureImport` imported the 384×96 effect and removed the notice. Both original and copied contract/manifest/image hashes matched, and the private runner-status directory remained empty. Earlier animation-import attempts are retained as unfulfilled test setups: the original verifier correctly waited for stable artifacts rather than producing the required failure notice.

The Best Motion Pilot run (`advanced-pilot-1792483039959`) produced three real candidates for the side direction. All three original Codex processes completed; tournament `anim-tournament_33w978_mvge4uw3` reached `pilot-review` with scores 3400, 3375 and 3375. The native receipt ended with a resumable review requirement and an explicit `review_open` next action, not a false completion. No candidate was adopted or expanded before the preservation checkpoint. `pre-restart-1792484030673/preservation.json` records 366 copied JSON files (72,154,211 bytes); original candidate image files remain in their tournament directories. Adoption, expansion and repair require their own subsequent evidence.

After restart, `advanced-batch-1792480051707/restored-reconciliation.json` records three accepted monitors, one failed monitor, no pending jobs and recovery of all three already-published animation sheets into history. The rejected near-static cell was not imported as accepted. No generation was repeated to restore this state.

The original review controller saved a technical winner decision for pilot candidate A, including the explicit note that this is an integration test and not the user's artistic approval. The existing sheet was split/chroma-processed/exported by the original tools into a 256×256 GIF with four 130ms frames (`pilot-preview-1792484385122/artifact-inspection.json`). The unchanged candidate source hash is recorded. An initial diagnostic export selected the old action name after renaming; explicit selection of the renamed action corrected the test without regenerating an image.

Calibration export completed in the engine, but its first native publication returned a storage interruption. Querying the exact original job/call recovered the completed receipt and its 4,777-byte JSON artifact; neither the export nor generation was repeated. Both the interrupted observation and `calibration-recovered-status.json` are retained. The exact cause of that original transient filesystem error was not inferred from the successful recovery.

`pilot-fallback-1792484862717` uses a byte-preserved copy of the actual review checkpoint. The nominal initial-wave resume call refused already-existing candidates. Its upstream message inaccurately called the fully started wave “partial”; the presentation was subsequently corrected without changing the guard. The nominal individual-candidate call reused the existing candidate and returned the review requirement, without a new runner. The fallback operation registered the original Balanced plan, then the native operation and original tournament queue were cancelled before any candidate was launched. The primary pilot was independently expanding. These are guard, idempotency and fallback-transition proofs, not additional image-generation evidence.

The adopted pilot then expanded through one original job, `codex-job-2026-10-20T08-29-42-814Z-21sf5n`, producing four front frames and four back frames. Its actual process completed at `08:40:37Z`; the original candidate reached `quality-evaluated` with score 3400, zero warnings and identity score 78.76. The native operation `023fcb1513ebf1110b0deae734433878` returned the explicit review requirement instead of declaring publication complete. The original state was `pilot-expanding` until the separate “Accept Pilot Expansion” operation; neither a finished process nor technical inspection substitutes for that acceptance. Both strips were visually inspected for identity, scale and alternating poses. This is diagnostic acceptance evidence, not the user's artistic approval or a finished animation for Projeto Z. `pre-restart-1792485999248/preservation.json` preserves 422 original private JSON files (81,747,981 bytes) before the next safe runtime restart.

After restart, the original `acceptDirectionRepairFromUi` accepted that existing expansion through receipt `6721bcb0b4ec1ba6296dc12aa37de8bf`. `expansion-before-manifest.json`, `expansion-after-manifest.json` and `accept-expansion-operations.json` record `pilot-expanding → accepted`, `pilotState: completed`, publication of front/back and the unchanged original side hash. No image was generated again to accept or restore the expanded result.

The only targeted repair then generated `front` through original job `codex-job-2026-10-20T08-55-22-092Z-7ke9ji`; its process completed at `09:02:12Z`. The original quality gate evaluated it at score 3400, zero warnings and identity score 84.75. Receipt `1cfd8e66e86764f21ea3a3aea8174e09` finished at `09:06:07Z` with the explicit review requirement; the interval includes real import, logs and finalization, not another generation. The raw embedded English review text was reported for presentation correction while preserving the upstream log.

The final canonical worker was rebuilt at `09:10:14Z`, SHA-256 `A03D756AF37E89796DD39EA4F9F5233E5FBE2121D117EC373001F3F905949450`. Its regression based on this exact outcome distinguishes a quality-evaluated repair awaiting acceptance inside an already-accepted tournament and presents “Reparo gerado; aguardando aceitação”. The 33 focused engine tests passed. The original historical receipt remains unchanged; no image generation was repeated to validate that presentation correction.

Acceptance (`b0f21c33db57e7a164e2bf3686bab4d3`), original pilot revalidation (`d1e712801648b30164f08d0bf824283e`), three directional GIF exports (`54ec4ee51f5837908916d6251883c57f`) and review closure (`9d4ee8bd9a9eb8c94339f686c9a91adc`) all completed. `final-artifact-inspection.json` decodes each 256×256/four-frame GIF, checks the real published PNG bytes against the manifest and proves that only the targeted front hash changed. Side remained `b86eb49397fe818fe1d74b6fea296946b9245aec6d5f133e07b171717fc84648`; back remained `7d823fa8c4496ba4166a90f056e846401cc48c3050438949854f7fbe137bf2bb`. The repaired front is `8f9e269802c150227e7bcc3cc4528740744223466f5eacff893f0e9fc620b74d`. No direction was regenerated for acceptance, revalidation or GIF export. This remains a technical integration test, not the user's artistic acceptance or a Projeto Z delivery.

### Public App method evidence ledger

This ledger snapshots the 73 public App methods in `scripts/pixel-cockpit/contract.ts` on 2026-10-20. The earlier set had 72; the extra method is the managed `resumeInterruptedCodexJob` extension, not an upstream project update. Source locations refer to the read-only original App. Counts describe entry-point evidence, not the number of tools, generated images, covered parameter combinations or artistic approvals.

`N98` = original native 98-operation report; `B50` = browser evidence `2026-10-20T06-52-21-489Z`; `B20` = browser evidence `2026-10-20T07-24-45-917Z`; `P20` = `advanced-postprocessing/operations.json`; `M-old` = successful records preceding the topology failure in `advanced-maintenance-1792480066598/operations.json`; `M41` = corrected native maintenance `advanced-maintenance-1792483053520/operations.json`; `I6` = actual damaged-effect import/retry `import-retry-1792483850379`; `S17` = populated reset `reset-populated-1792483863583`; `C-active` = corrected active-cancellation report; `R4` = the real Composer recovery and inspected corrected four-frame effect. Full receipt IDs and paths are retained in the evidence directory's `public-method-matrix.json` and referenced reports.

A shared adapter/callback means source tracing identifies the exact original method called by the exercised public entry point. It does not claim that a second spelling of the command was separately submitted. The original N98 report did not retain request arguments, so its mappings additionally rely on explicit adapter dispatch or the driver sequence plus the observed result; these limitations are stated in each row. Utility-only exports do not count as proof of an unexercised App export method.

| Original method | Source | Evidence classification | Recorded proof / limit |
| --- | --- | --- | --- |
| `acceptDirectionRepairFromUi` | `App.tsx:6588` | Direct actual operation | Pilot3 6721bcb0; Pilot3 b0f21c33: Request arguments and completed receipt are recorded. |
| `addSelectedAsFrame` | `App.tsx:9266` | Direct actual operation | S17 46ea36df; S17 031a608d: Request arguments and completed receipt are recorded. |
| `adoptAnimationReviewWinner` | `App.tsx:6297` | Shared callback with review outcome | Pilot 023fcb15 + expansion 21sf5n: Original review adoptWinner invoked this App callback, started the original expansion and produced front/back. Receipt then required explicit expansion review, preserving its actual quality-evaluated state rather than claiming publication. |
| `adoptSelected` | `App.tsx:9533` | Direct actual operation | B20 335f0fef; M-old 0ad6299f: Request arguments and completed receipt are recorded. |
| `applyChromaToSelectedFrame` | `App.tsx:9380` | Direct actual operation | B50 92d6bc84: Request arguments and completed receipt are recorded. |
| `beginWorkflow` | `App.tsx:9538` | Direct actual operation | B20 6cb90546; M-old 0d0f90b6: Request arguments and completed receipt are recorded. |
| `buildSelectedAnimationPackV2` | `App.tsx:9861` | Shared callback | N98 b11823ab; B50 186955c5: Called by `openAnimationTimelineEditor`; populated original v2 draft observed. |
| `cancelCodexJob` | `App.tsx:8451` | Shared adapter | C-active c3ef69e4: Native cancellation calls this original method; observed live original process then terminal SIGTERM and absent PID. |
| `cancelTournamentFromUi` | `App.tsx:6542` | Direct actual operation | F-checkpoint cce36dcd: Request arguments and completed receipt are recorded. |
| `clearLocalStateScope` | `App.tsx:10005` | Direct actual operation | S17 cf7d6614; S17 b6eeb5b4: Request arguments and completed receipt are recorded. |
| `clearSelectedAnnotations` | `App.tsx:9525` | Direct actual operation | B50 0970adfd: Request arguments and completed receipt are recorded. |
| `closeAnimationReviewCockpit` | `App.tsx:6205` | Shared callback | Pilot review close 9d4ee8bd: Recorded review-controller close invokes this exact original App callback after repair acceptance/revalidation/export. |
| `closeVfxCompositeStage` | `App.tsx:6185` | Shared callback | P20 close: Original VFX controller `close` invokes this App callback; editor unmounted. |
| `dedupeLocalInboxHistoryNow` | `App.tsx:7875` | Direct actual operation | M41 39fa6773: Request arguments and completed receipt are recorded. |
| `dedupePersistedHistoryNow` | `App.tsx:7889` | Direct actual operation | M41 8671450e: Request arguments and completed receipt are recorded. |
| `deleteUserAnimationItem` | `App.tsx:9857` | Direct actual operation | B50 7aa37a5d: Request arguments and completed receipt are recorded. |
| `downloadSelectedImage` | `App.tsx:9259` | Direct actual operation | M41 5c139b63: Request arguments and completed receipt are recorded. |
| `exportAnimationEngineMetadata` | `App.tsx:9918` | Source-traced invocation | N98 d426ce29, bf65e7c0, d3b76b98, 9dfc473a, 5faccb55: Original driver calls this App method for all five engine exports; receipt filenames and actual JSON agree. Old receipts lack request arguments. |
| `exportAnimationLibraryItem` | `App.tsx:9795` | Direct actual operation | B50 bf633249: Request arguments and completed receipt are recorded. |
| `exportDirectionalAnimations` | `App.tsx:9203` | Shared adapter | N98 044eaaf3, ba1c5bec, 914d07e9: `export` dispatches GIF/APNG/WebP to this method; actual files inspected. |
| `exportEnvironmentReport` | `Managed adapter` | Direct actual operation | B50 66617faf; B50 f906a5f7: Request arguments and completed receipt are recorded. |
| `exportSelectedAnimationPack` | `App.tsx:9943` | Shared adapter | N98 8819323c: `export(animation-pack)` invokes this method; real v1 ZIP inspected. |
| `exportSelectedAnimationPackV2` | `App.tsx:9912` | Shared callback | B50 e214ad57: Original timeline `exportPack` callback invokes this method; real v2 ZIP reimported. |
| `exportSelectedEffectApng` | `App.tsx:9221` | Shared adapter | N98 a1a9965b: `export(effect-apng)` invokes this method; real APNG inspected. |
| `exportSelectedEffectFramesZip` | `App.tsx:9233` | Shared adapter | N98 6294bf2f: `export(effect-frames)` invokes this method; archive frames inspected. |
| `exportSelectedEffectGif` | `App.tsx:9215` | Shared adapter | N98 19ca2114; recovered 34281478: `export(effect-gif)` invokes this method; both 8-frame and corrected 4-frame files inspected. |
| `exportSelectedEffectMetadata` | `App.tsx:9238` | Shared adapter | N98 42898249; recovered 3f1f5ed0: `export(effect-metadata)` invokes this method; actual JSON inspected. |
| `exportSelectedEffectPack` | `App.tsx:9243` | Shared adapter | N98 1392c03d: `export(effect-pack)` invokes this method; complete ZIP inspected. |
| `exportSelectedEffectSheet` | `App.tsx:9227` | Shared adapter | N98 e1134663; recovered 80ced9f0: `export(effect-sheet)` invokes this method; actual PNG sheets inspected. |
| `fallbackMotionPilotFromUi` | `App.tsx:6333` | Actual registered/cancelled outcome | Fallback 86b782b6 + cce36dcd: On a byte-preserved original review checkpoint copy, registered a Balanced fallback then cancelled it with no candidate jobs. Primary pilot continued expansion. |
| `importLatestOutboxResult` | `App.tsx:8985` | Direct actual operation | I6 7df538cf: Request arguments and completed receipt are recorded. |
| `loadCodexAnimationTournament` | `App.tsx:6163` | Direct actual operation | Pilot3 927e117b; Pilot3 95835e75: Request arguments and completed receipt are recorded. |
| `loadOutboxResults` | `App.tsx:7919` | Direct actual operation | M41 c57ae497: Request arguments and completed receipt are recorded. |
| `moveFrame` | `App.tsx:9298` | Direct actual operation | B50 2c5c63ee: Request arguments and completed receipt are recorded. |
| `openAnimationReviewCockpit` | `App.tsx:6221` | Shared adapter | N98 3fead0ab: `review_open` invokes this exact method with the persisted tournament. |
| `openAnimationTimelineEditor` | `App.tsx:9897` | Shared adapter | N98 b11823ab; B50 186955c5: `timeline_open` invokes this exact App method. |
| `openVfxCompositeStage` | `App.tsx:6176` | Shared adapter | N98 e63ea0af; P20: `vfx_open` invokes this exact App method. |
| `openVfxSourceResult` | `App.tsx:6200` | Direct actual operation | M41 58e13d50: Request arguments and completed receipt are recorded. |
| `recoverUnimportedOutboxResults` | `App.tsx:7973` | Direct actual operation | M41 de17cfc6; Batch4 a0e55ecf: Request arguments and completed receipt are recorded. |
| `removeAnnotation` | `App.tsx:9517` | Direct actual operation | B50 165cc324: Request arguments and completed receipt are recorded. |
| `removeFrame` | `App.tsx:9312` | Direct actual operation | B50 44490fda: Request arguments and completed receipt are recorded. |
| `renameUserAnimationItem` | `App.tsx:9840` | Direct actual operation | B50 afd7c2d1: Request arguments and completed receipt are recorded. |
| `repairAnimationReviewDirection` | `App.tsx:6346` | Shared callback with review outcome | Pilot repair 1cfd8e66 + actual7ke9ji: Original review repairDirection called this App callback for front; a real four-frame repair passed original QC and required explicit review. It was later accepted by the separate original acceptance operation. |
| `repairCockpit` | `App.tsx:8105` | Direct actual operation | B50 92cb0807; M41 951aee6e: Request arguments and completed receipt are recorded. |
| `resumeInterruptedCodexJob` | `Managed adapter` | Managed extension | R4 same original yz2n0k: Actual dedicated-agent Composer request resumed explicit process_existing; original job completed without another image generation. |
| `retryCodexFailureImport` | `App.tsx:7813` | Direct actual operation | I6 e4306c68: Request arguments and completed receipt are recorded. |
| `revalidateAcceptedPilotFromUi` | `App.tsx:6645` | Direct actual operation | Pilot3 d1e71280: Request arguments and completed receipt are recorded. |
| `runAnimationBatchMatrix` | `App.tsx:6666` | Actual partial outcome | Batch4 actual-cell-evidence.json: All four real cells ran: three published, one correctly rejected by original motion QC. The old aggregate observer bug and later restored failed/accepted states remain explicit. |
| `runCockpitHealthCheck` | `App.tsx:8038` | Direct actual operation | B50 8f775ade; M41 31342e0c: Request arguments and completed receipt are recorded. |
| `saveAnimationHumanReview` | `App.tsx:6283` | Shared callback | Pilot review 34063d4b + actual tournament review: Original review-controller saveReview called this exact App method. Persisted winner/note explicitly identify a technical test, not user artistic approval. |
| `saveAnimationTimelinePack` | `App.tsx:9905` | Shared callback | B50 4ee04770: Original timeline `save` dispatches its `onSave` callback to this App method; saved/reloaded draft inspected. |
| `selectAnimationBodyTopology` | `App.tsx:9667` | Direct actual operation | B20 3f348c0d; B20 2cd0e758: Request arguments and completed receipt are recorded. |
| `selectAnimationFrameCount` | `App.tsx:9674` | Direct actual operation | M41 60f51ff0; M41 45dc0c36: Request arguments and completed receipt are recorded. |
| `selectAnimationPreset` | `App.tsx:9644` | Shared callback | N98 19625a64; B20: `preset(kind=animation)` calls `useAnimationPresetExample`, which calls this exact selector. |
| `selectHistoryResult` | `App.tsx:9340` | Direct actual operation | B20 f4a36128; M-old 095ffa69: Request arguments and completed receipt are recorded. |
| `selectSourceFromPreview` | `App.tsx:9362` | Direct actual operation | B20 90b9150c; M-old 9f1e4a93: Request arguments and completed receipt are recorded. |
| `splitSelectedToTimeline` | `App.tsx:9177` | Direct actual operation | B50 8e500959: Request arguments and completed receipt are recorded. |
| `startDirectionRepairFromUi` | `App.tsx:6564` | Shared callback with review outcome | Pilot repair 1cfd8e66; final-artifact-inspection.json: repairAnimationReviewDirection awaits this exact App method with [front]. The original repair produced a changed front sheet; physical side/back hashes remained unchanged after acceptance. |
| `startPersistedTournamentCandidate` | `App.tsx:6517` | Named idempotent reuse | Fallback checkpoint 695c04a2: Nominal call retrieved the same completed candidate index0 and review-required state; original runner count remained three copied statuses. No new image was generated for spelling coverage. |
| `startPersistedTournamentInitialCandidatesFromUi` | `App.tsx:6471` | Named guard + shared helper | Fallback checkpoint 1b789e40; Pilot3: Nominal call rejected the already-started wave without new jobs. The launch helper/initial-candidates endpoint is the exact helper exercised by the actual three-candidate pilot. Original guard text calls any existing wave partial, including 3/3. |
| `toggleBatchMatrixPreset` | `App.tsx:6660` | Direct actual operation | M41 d9e85c16; M41 097e8359: Request arguments and completed receipt are recorded. |
| `toggleBatchMatrixSource` | `App.tsx:6654` | Direct actual operation | M41 432864cc; M41 9c687bbb: Request arguments and completed receipt are recorded. |
| `toggleDirectionRepairSelection` | `App.tsx:6555` | Direct actual operation | Pilot3 34685dfb: Request arguments and completed receipt are recorded. |
| `toggleFavoriteAnimationPreset` | `App.tsx:9663` | Direct actual operation | B20 241efaff; B20 b7523c42: Request arguments and completed receipt are recorded. |
| `updateActiveAction` | `App.tsx:9406` | Direct actual operation | B50 53e1d393: Request arguments and completed receipt are recorded. |
| `updateAnimationMotionVariant` | `App.tsx:9679` | Direct actual operation | M41 9eb6637a: Request arguments and completed receipt are recorded. |
| `updateAnnotationComment` | `App.tsx:9507` | Direct actual operation | B50 040c00ed: Request arguments and completed receipt are recorded. |
| `updateCell` | `App.tsx:9412` | Direct actual operation | B50 fc656d01: Request arguments and completed receipt are recorded. |
| `useAnimationLibraryItem` | `App.tsx:9746` | Direct actual operation | B50 6fcb7be4: Request arguments and completed receipt are recorded. |
| `useAnimationPresetExample` | `App.tsx:9687` | Direct actual operation | B20 48417f26; M-old 27eaaf7f: Request arguments and completed receipt are recorded. |
| `usePromptExample` | `App.tsx:9624` | Direct actual operation | B20 9e77b037; M-old 7e5bcdef: Request arguments and completed receipt are recorded. |
| `useQuickPrompt` | `App.tsx:9635` | Direct actual operation | B20 daf35552; M-old ff3b070e: Request arguments and completed receipt are recorded. |
| `verifyAnimationPackV2RoundTrip` | `App.tsx:9922` | Shared callback | B50 53507fdf: Timeline `roundTrip` invokes this App callback and the original semantic assertion. |

### Library and controller contracts outside the App-method ledger

The build's diagnostic counts (`methods: 175`, `settings: 101`, `modules: 16`) describe the extractor's source metadata, not 175 independently exposed chat tools. `extractController` discovers original App callbacks and state setters; `publicAppMethods`, `configurableNames`, `suiteSettingNames` and `publicModuleMethod` then restrict the callable contract. The 73-row ledger above covers only the public App entry points, including the managed explicit-resume extension. The three mounted editor controllers and 16 original library modules below are additional contracts, while DOM handlers and internal storage mutation helpers stay internal. A source-discovered helper is neither an unimplemented user feature nor evidence that its arbitrary parameter combinations were tested.

The 73-method ledger is not the complete capability count. The runtime also exposes the original timeline, review and composite controllers, shared action adapters, and 16 library modules through `invoke {module,method,args}`. The module catalog is generated from the supplied source rather than a hand-maintained subset. Constants, schemas, catalogs and pure helpers are not independent UI tools or additional generation workflows.

The adapter excludes internal storage writes/clears and functions requiring DOM elements, pointer/mouse/UI events or callback arguments. Their original user actions remain available through scoped import/export, annotation, configuration and controller commands. `loadImage`, canvas conversion and ID generation remain internal implementation helpers. No missing original user-facing feature is considered satisfied merely because a similarly named utility was exposed.

| Original library / controller | Exposed contract and retained behavior | Actual evidence and boundary |
| --- | --- | --- |
| `animationAlignment` | Shared normalization footline and export anchor from actual dimensions | Real character import/QC/export used original alignment. Actual pose sheet and footprint were inspected; every alternative ratio is not a separate generated test. |
| `animationPack` | Validate/import safe v1 packs and preserve library/history provenance | Actual original v1 ZIP structurally inspected; v2 import into the user library and re-export exercised. Malformed path regression is separate from live successful import. |
| `animationPackV2` | Timeline creation/migration/serialization; frame duration, hold, duplicate, reverse, loop, events, geometry, ZIP and five engine metadata formats | Real generated animation edited and exported in N98; actual browser save/import/round-trip in B50; reset and undo in P20. Observed output files and timing, not a mocked serializer, establish the processing path. |
| `animationQuality` | Action-specific loop/phase contracts and original structural/temporal quality report | All real character jobs pass through original QC. Actual warnings are retained; a successful runtime import is not artistic approval or proof that every quality-gate branch was triggered. |
| `animationReview` and review controller | Candidate scores, dimensions/QC matrix, overlays, playback, decision/reason/note, calibration, adoption and repair | N98 opened a real tournament and toggled overlays. The actual Best pilot saved its technical winner decision and note, exported calibration, adopted that candidate, accepted expansion and generated/accepted a front repair. Each review requirement remained distinct from final publication; closure has its own receipt. |
| `animationTournament` / `animationTournamentGate` | Admission, ranking, Smart Race, additional candidate decision, first-qualified thresholds and untargeted hash checks | Real Fast generation and the four-cell batch exercise admission/QC, including a real rejected near-static candidate. The Best pilot, expansion and targeted repair were generated and accepted; cancellation and existing-job reuse have separate receipts. Physical final PNG hashes verify untargeted preservation, rather than inferring it from helper tests. |
| `effectQuality` | Original effect quality report and phase/palette/energy metrics | Both the original 8×128 effect and recovered 4×96 effect were processed. Root inspected the actual four-phase sheet; original centroid/palette warnings remain, and the review is diagnostic QA rather than user acceptance. |
| `exporters` | PNG sheet, frame ZIP, GIF, APNG, animated WebP, metadata, character/effect ZIP packages | N98 produced actual files across every export family. The artifact verifier decoded structure, frame counts/delays and archive references. Export aliases sharing these functions are identified separately from App methods. |
| `image` | Original file/data URL/Blob conversion and artifact download capture | Real PNG import, agent editing input, and all actual binary exports exercise these conversions. DOM canvas/image loading stays internal; no separate “tool” is claimed for each conversion. |
| `motionPilot` | Direction cost estimates, pilot review decision, benchmark summary and adoption gate | Actual three-candidate Best pilot, technical review decision, two-direction expansion, acceptance and final revalidation have their own receipts. A private copy of the real review checkpoint exercises fallback registration/cancellation and nominal idempotent reuse without additional images. |
| `motionRecipes` | All 22 recipes, schema/compiler versions, anatomy profiles, phases, modifier validation, grids and experimental frame flags | Actual walk and idle jobs compile original recipes. Browser B20 verifies rejected incompatible anatomy does not unmount the suite and a supported recipe accepts quadruped/biped. This is not a claim that all recipe/anatomy/modifier combinations were generated. |
| `officialAnimations` | Complete original demo-library constant and previews | Real sample selection/import and library actions in B50; sample artwork is explicitly not newly generated test evidence. |
| `sprite` | Grid cells, sheet packing/metadata, playback sequence, chroma cleanup, edge/size summaries | Browser operations on actual imported pixels and native exports exercise splitting, editing and assembly. Real character/effect normalization uses the same processing contracts. |
| `storage` | Read/persist/retention/preflight summaries, dedupe/fingerprint and frame-source remapping | Actual browser reload, user-library persistence and all four populated reset scopes were exercised. A damaged private copy of a real effect produced the original decode error; restoring exact bytes and invoking retry imported it without generation. The consolidated runtime restored three accepted batch outputs and preserved the fourth failed candidate. Original schema/race and transient publication observations remain recorded without rewriting old receipts. |
| `vfxComposite` and composite controller | Socket/event mapping, timing, transformation, blend/layer, undo/redo/reset, original manifest import/export and per-engine handoff | N98 composed actual character/effect outputs and exported all formats/layers; P20 imported original ZIP and JSON, compared manifests, reset and undid exactly, preserving source hashes. Arbitrary combinations of all sockets/blends are not implied. |
| Timeline controller | Draft, selected direction/frame, playback speed, geometry editor, history, save/round-trip/export/close | Actual native and browser edits operated on the mounted original component. Controller access after close is rejected. Moving transcript GIF playback is confirmed by native captures; `shared-edit-proof.json` separately demonstrates bidirectional UI/agent shared state while preserving existing exports. |

## Maintained production extensions

These additions do not replace any original capability or introduce a fal.ai dependency. The **Preparar pixel art** panel uses the same dispatcher as the dedicated agent.

| Extension | Shared agent/suite behavior | Native verification |
| --- | --- | --- |
| `recover_grid` | Palette quantization, periodic pixel-grid recovery including fractional pitch/phase, manual fallback, integer enlargement and explicit background removal; original preserved | Real 1024×1024 PNG recovered to 98×96 native pixels and enlarged 3×; PNGs and processing report exported. |
| `extract_poses` | Whole foreground components, detached-part association, occupied-cell override, uniform canvases, preserved jump offsets or explicit bottom alignment, new independent timeline action | Nine complete poses extracted without discarding foreground; 260×260 frames, atlas, GIF and v2 packages inspected. Native geometry, anchor, frame order and edited durations survived session reopening. |
| `production_recipe` | Two identity-reference recipes plus idle, walk, attack, hurt, jump and downed presets; production discipline augments the original motion compiler without replacing its anatomy, phases, modifiers or QA | All eight applied through native tools without generating images. Six animation presets preserved a custom six-frame budget and slow tempo. Source-based tests also cover every original motion recipe's compiler contracts. |

The private native run records 42 completed operations and 17 inspected exported files under `.codex/pixel-review/processing-ready-20261020/`. Its processing reports, receipt ledger, reload check, recipe parameter check and suite screenshot document this extension separately from the earlier four-flow generation evidence. Automatic grouping does not establish artistic approval; touching poses and ambiguous pixel periodicity still require inspection or explicit parameters.

## Suite presentation adaptation

The original supports 15 locale IDs including `pt-BR` (`src/App.tsx:356`), but its advanced review/VFX labels only guarantee Japanese/English. The adopted build initializes pt-BR and applies maintained Portuguese presentation mappings to the original controls, modals, dynamic statuses and tool errors. These transformations live outside the read-only source snapshot. Diagnostic raw state and original logs preserve their upstream text; translating presentation must not rewrite their recorded technical meaning.

`src/styles.css` defines semantic variables `--panel`, `--panel-soft`, `--line`, `--line-strong`, `--text-muted`, `--green`, `--green-soft`, `--teal`, `--amber`, `--amber-soft` and `--danger`, alongside hardcoded light colors. The maintained suite adapter maps surfaces, text, borders, fonts and actions to Jarvis tokens, replaces interactive primitives with the project's shadcn components and preserves the original editor layout. Browser evidence includes actual dark-theme fonts, controls, history and timeline rendering. Sprite pixels, canvases, GIFs, chroma keys and palette colors are not recolored by the theme.
