# Native Hyperframes composition contract

Use Hyperframes for requested video deliverables, with the managed Jarvis tools.
Keep one editable composition directory inside the project, including its HTML,
local assets and Hyperframes configuration. Deliver the rendered MP4 as well as
the editable source. Never claim a video is ready from an HTML file or a running
render session.

## Work efficiently

Read the relevant `video_docs` sections once. Inspect existing compositions with
`video_run(action="timeline")` before editing their timing. For new compositions,
use `video_run(action="init", path="videos/<name>")` in a new/empty directory.
Initialization also includes pinned local GSAP at `assets/vendor/gsap.min.js`;
reuse that script path for offline animation instead of a CDN.
Use existing file tools to write/edit source, keeping all assets project-local.
For actual page imagery, use `browser_screenshot` with
`savePath="videos/<name>/assets/page.png"` in Build mode. Both browser backends
save the original PNG and return its project-relative `path` plus an attachment
for `vision`. Reuse that file directly in the composition; do not embed base64
or recapture an unchanged page. Existing assets are never overwritten.
Do not install a second CLI, invoke upstream skills installers or require the
user to configure Node, Chromium or FFmpeg manually. Jarvis manages that runtime.

`video_docs(topic="composition")` always returns this fixed contract; `file`
does not select another document for that topic. For official CLI/media/workflow
references, omit `file` or use null/blank to read `SKILL.md`; use an exact relative
Markdown path only when that document points to a reference.

Only `init` uses `resolution`; only `render` uses `quality` and `output`.
Other action fields are ignored, so optional provider placeholders do not block
scaffolding or diagnostics. Set the actual desired render settings when calling
`render`; a filename supplied to `init` does not request a render.

## Creative direction before production

Read relevant `project_knowledge`, references, brand assets and actual pages.
Recommend a project-specific demonstration, promotion or montage; offer clicks,
cursor/callouts, narration and music instead of a raw silent screenshot sequence.
Honor explicit slideshow, silence or script choices. Use `ask_user` only for
material missing choices: at most three concise questions, short presets and a
recommended option. Do not repeat known answers or restart intake for an edit,
accepted brief or request to decide and execute.

Show a compact script/storyboard: message, hook, feature/benefit sequence, closing
action; per scene, visuals, interaction/motion, spoken line, audio and rough time.
Retain this direction in `presentation.json`; approval is required only at a
user-requested review checkpoint. Verify real page navigation/clicks and capture
before/after states. A reconstruction with animated cursor is possible, but is
not a screen recording. Disclose unobservable interactions and propose a truthful
alternative. Reuse project design, preserve user data and avoid invented claims.
Pace from content and measured voice, about 2–2.5 spoken words/second plus pauses.

## Composition and timing

For presentations, keep an editable `presentation.json` in the composition:

```json
{"project":{"name":"Example"},"voice":{"language":"pt-BR","voice":"pm_alex","speed":1},"music":{"path":"assets/audio/music/soundtrack.wav","volume":0.16},"scenes":[{"id":"intro","duration":3,"narration":"Conheça o nosso projeto.","visual":"Product introduction"}]}
```

Use `video_audio(action="narrate", text="...", voice="pm_alex",
output="videos/<name>/assets/audio/voice/intro.wav")` for each scene. PT-BR
voices are `pm_alex`, `pm_santa`, `pf_dora`. Generation is local after the
required Core's initial model downloads; no external API or local conversational
LLM is needed. CPU inference can take time. Use the returned session/cursor with
`video_wait`; cancel with `video_cancel`. Heavy generation queues rather than
loading multiple models across chats. Do not start a second copy while waiting.
Generated voice preserves phrase boundaries, with 0.25s leading and 0.20s
trailing silence per WAV plus sentence/clause pauses. Keep this padding and
complete syllables; do not guess media offsets. Establish visuals before speech
and hold after it.

`video_presentation(path="videos/<name>")` reads the manifest without changing
it and returns missing/stale narration tasks, scene starts/durations and actual
audio lengths. A scene lasts at least its requested duration or the actual voice
plus `tail` (default 0.35 seconds), whichever is longer. Use that same timeline
for visuals, narration and captions; the measured WAV already includes its
leading/trailing silence, so do not add another manual narration start offset.
Insert the returned `audioHtml` inside the
root once; optional `captionHtml` needs project-appropriate styling. Set root
duration and GSAP endpoints from the returned total duration. Do not create a
second independent timing plan or stretch speech to fit a guessed duration.
Each visual scene's timed element uses the manifest's exact `id`, `data-start`
and `data-duration`. Keep the returned audio IDs/paths/timing in static HTML.
Native rendering verifies that these declarations match the manifest, preventing
an old or silent composition from being published as a completed presentation.

Keep unchanged WAVs/receipts, including legacy narration: new padding is not
retroactive. For changed text or a requested improvement to an older recording,
generate to a new WAV path and update only that scene's `audio`; never overwrite.
Explicit `audio` accepts supplied PCM16 WAV; the generated default path is
`assets/audio/voice/<scene-id>.wav`.

Music generation uses MusicGen-small weights under **CC-BY-NC-4.0**. Ask about
usage only if not already answered; pass `nonCommercial=true` only following the
user's confirmation. For professional/commercial presentations use a supplied
licensed soundtrack instead. A supplied licensed track needs no repeat question.
Continue scripting, captures and voice generation while an unavailable licensed
soundtrack is pending; report that missing element before final delivery.
`video_audio(action="music", prompt="instrumental subtle
synths, no vocals", duration=45, nonCommercial=true, output=".../soundtrack.wav")`
generates a short seed and crossfade-loops it to the requested duration. Set
`music.path` in the manifest after narration determines the actual total length.
Omit `music` when the brief selects no soundtrack. Offer the choice rather than
silently defaulting a product presentation to silence. The returned audio markup uses
Hyperframes volume automation for fades and ducking (default bed gain 0.16),
so preview and render share their mix. Inspect with `video_presentation` again
after changing any voice or audio asset. Do not trim longer narration silently.

The entry file is `index.html`. Define a root with `data-composition-id`,
`data-width`, `data-height` and `data-duration` (seconds). Use `data-start` and
`data-duration` for timed clips/scenes, and `data-track-index` for timeline tracks.
Timed nodes need `class="clip"` and stable, human-readable IDs for editing.
Put a standalone root directly under body,
with width/height filling its canvas. Give each audio element a unique ID;
video elements need `muted playsinline`. Do not time both an ordinary clip
wrapper and its descendant video, which would apply offsets twice.
Put each visual scene with nested layout into a separate HTML sub-composition
under `compositions/`; mount it with `data-composition-src` in `index.html`.
Keep the manifest scene ID on the host mount along with its `data-start`,
`data-duration`, `data-track-index`, `data-width` and `data-height`.
The host's `data-composition-id` must match the child root's composition ID and
the child's registered timeline key. Inside each child file, put its styles,
markup and scripts inside `<template>`; the child `<head>` is not mounted.
Style the child root by its unique ID, and keep child animation times local
(starting at zero). Assets in child files also use composition-root-relative
paths such as `assets/page.png`, never `../assets/page.png`. Read `video_docs(topic="core",
file="references/sub-compositions.md")` for this cross-file contract before
splitting scenes. Do not merely add attributes to silence diagnostics.
The managed runtime flags `nested_structure_needs_subcomposition` for ordinary
timed scene wrappers with nested layout, and `timeline_track_too_dense` for dense
inline scene tracks. Proper sub-composition mounts avoid these warnings while
preserving the scene timeline. Audio and caption markup returned by
`video_presentation` can stay at the root; it does not generate visual layouts.
Place picture/audio media through the framework's timed media attributes rather
than custom autoplay, setTimeout or animation-frame loops. Media timing belongs
to Hyperframes; animate an inner wrapper for crop/zoom/visual transitions.

Rendering seeks to arbitrary timestamps. Animation must be deterministic and
seekable: use a paused GSAP timeline registered in `window.__timelines` under the
root composition ID (or the framework's documented animation adapters). Never
derive animation state from Date.now, unseeded randomness or elapsed wall time.
Register the timeline after any asynchronous initialization finishes. Animate
the visual wrapper inside a clip; never animate display, visibility or autoAlpha
on the timed `.clip` itself, whose activation is owned by Hyperframes.
Avoid CSS animation/transition side effects whose state cannot be reconstructed
at an arbitrary frame. Read managed CLI/media references before introducing
advanced media, audio or framework APIs. Keep fonts and required assets local
when practical so rendering remains reproducible.

Read the scaffold's actual HTML and configuration before editing. Match the
requested resolution: landscape 1920x1080, portrait 1080x1920, square 1080x1080.
Use coherent typography, safe margins, readable text, intentional motion and
sufficient contrast. Do not put implementation/debug labels into the video.

## Validate and deliver

Use `video_run(action="check")` to obtain lint, runtime, layout and motion
diagnostics. The check is strict: fix its errors and warnings (including authoring
structure warnings) before rendering. Validate a representative scene early,
then the complete composition; preserve screenshots, valid scenes and confirmed
audio while repairing only the reported issue. Check the output against the
accepted storyboard, legibility, navigation/click sequence and complete speech
boundaries. When playback/audio inspection is available, listen to the opening,
scene joins and ending; a passing technical check does not prove speech sounds
natural. Report that limitation when listening is unavailable.
`video_run(action="render")`
uses MP4, strict validation and no best-effort fallback; quality is `draft`,
`looks` (default) or `delivery`. It creates a new output file rather than silently
overwriting an existing deliverable. An explicit `output` is project-relative.

Every run may yield a `sessionId`, `cursor` and running status. Continue using
`video_wait` with that same handle/cursor until completion; never restart a
running render. Waits yield instead of imposing a total rendering timeout. Use
`video_cancel` to stop work that is no longer needed. Turn cancellation stops
the owned process tree. After failure, inspect the retained diagnostics and
produced files before retrying; uncertain results are not permission to repeat.

A completed render receipt includes `action="render"`, `status="completed"`
and the verified project-relative MP4 `path`. Jarvis opens that video in a chat
file tab with playback and save/open actions. Report the actual source/output
paths, relevant validation and any remaining limitations concisely.
