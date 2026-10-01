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

Confirm subject, target audience, duration, aspect ratio and important content
from the request/project context. Ask only for missing decisions that materially
change the deliverable. A short requested demo does not require an elaborate
production pipeline. Reuse project design tokens, brand assets and components
when relevant. Source/generate images or audio only when they improve the brief;
do not fabricate claims about provided products or people.

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

`video_presentation(path="videos/<name>")` reads the manifest without changing
it and returns missing/stale narration tasks, scene starts/durations and actual
audio lengths. A scene lasts at least its requested duration or the actual voice
plus `tail` (default 0.35 seconds), whichever is longer. Use that same timeline
for visuals, narration and captions. Insert the returned `audioHtml` inside the
root once; optional `captionHtml` needs project-appropriate styling. Set root
duration and GSAP endpoints from the returned total duration. Do not create a
second independent timing plan or stretch speech to fit a guessed duration.
Each visual scene's timed element uses the manifest's exact `id`, `data-start`
and `data-duration`. Keep the returned audio IDs/paths/timing in static HTML.
Native rendering verifies that these declarations match the manifest, preventing
an old or silent composition from being published as a completed presentation.

Confirmed identical requests reuse their WAV and receipt. When narration changes,
choose a new WAV path and update that scene's `audio` property; other scenes are
preserved. Explicit `audio` can also refer to a provided PCM16 WAV recording.
The default generated scene path is `assets/audio/voice/<scene-id>.wav`.

Music generation uses MusicGen-small weights under **CC-BY-NC-4.0**. Ask about
usage before generating; pass `nonCommercial=true` only following the user's
confirmation. For professional/commercial presentations use a supplied licensed
soundtrack instead. `video_audio(action="music", prompt="instrumental subtle
synths, no vocals", duration=45, nonCommercial=true, output=".../soundtrack.wav")`
generates a short seed and crossfade-loops it to the requested duration. Set
`music.path` in the manifest after narration determines the actual total length.
Omit `music` when no soundtrack is requested. The returned audio markup uses
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
Nested compositions use `data-composition-src` with project-relative HTML paths.
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
diagnostics. Correct material failures before rendering. `video_run(action="render")`
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
