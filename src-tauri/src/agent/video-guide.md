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
