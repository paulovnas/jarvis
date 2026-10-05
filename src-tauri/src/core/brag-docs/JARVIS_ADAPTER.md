# Brag in Jarvis

This adapter takes precedence over every upstream Brag document and example.
Use Brag only when the user explicitly asks for a brag or chooses this creative
workflow. A request for a short launch/showcase video alone does not enable
Brag. Ordinary videos keep their current
workflow. Brag is a creative skill package, not a separate executable or model.

## Managed execution

- Always use the full HyperFrames composition workflow. Do not dispatch to
  brag-slim based on the selected model, and do not load `slim.md`.
- Use `video_docs` to read Brag and HyperFrames documents as needed; do not load
  the entire reference library into the initial prompt.
  For Brag Step 3, read the managed `hyperframes-core` contract with
  `video_docs(topic="core")`, `hyperframes-audio` with `video_docs(topic="audio")`,
  and `hyperframes-cli` with `video_docs(topic="cli")`. The three additional
  domains ship inside this package: with `topic="brag"`, read
  `file="hyperframes/hyperframes-animation/SKILL.md"`,
  `file="hyperframes/hyperframes-creative/SKILL.md"`, or
  `file="hyperframes/hyperframes-keyframes/SKILL.md"` as needed. Relative Markdown
  links in those documents resolve within their own domain directory; pass the
  corresponding complete `hyperframes/<domain>/<relative-file>` path to
  `video_docs`. These unmodified documents are pinned to the same HyperFrames
  revision as the managed core/audio contracts. Their Apache-2.0 license and
  per-file provenance are under `hyperframes/`. Helpers/scripts and additional
  animation runtimes mentioned by upstream are not installed; use Jarvis's
  supported operations and managed GSAP unless other assets are already
  provided by the user. Ignore upstream plugin/global setup instructions.
- Use `video_brag_assets` to browse the reviewed CC0 effects or attributed
  music, and `video_brag_asset` to import a selected asset into the project
  composition. Music can be converted to PCM WAV by the managed import tool
  when needed by the presentation renderer. Preserve its generated credit
  sidecar and include those credits with delivered outputs.
  Resolve assets from the returned managed package, never from `docs/brag`, a
  global skill directory, or an assumed installation path.
- Use `video_run` for its supported `init`, `timeline`, `check`, and `render`
  operations. Use validation evidence returned by `check`. Upstream snapshot,
  preview, `npx hyperframes`, global CLI installation,
  Python/uv helper commands, and direct FFmpeg recipes are examples of creative
  intent; they are not execution instructions in Jarvis. Do not install tools
  or dependencies to satisfy those examples.
- The inspected project is source material. Confirm product claims from actual
  code, user-provided evidence, or an observed product. Never invent results,
  customer counts, integrations, capabilities, or a working screen.

## Creative delivery

Keep the product-specific hook, planning rubric, storyboard, tone options,
legible typography, authentic UI evidence, share copy, and professional audio
timing from the full Brag workflow. Default to 15–25 seconds unless the user
asks otherwise. Respect their format and creative direction. Use a fresh
project output directory so previous videos remain intact.

Narration is opt-in. If requested, use Jarvis's configured managed narration
tools; never force the upstream Kokoro provider or enable narration silently.
Use the verified duration of the generated audio when planning scene timing.

The managed package includes 260 CC0 effects: 228 Kenney OGGs and 32 keyboard
WAVs by unicae_games. `asset-catalog.json` records each file's license, origin,
size, and hash. Upstream SFX analysis covers the 228 Kenney effects only; do not
infer acoustic measurements for the keyboard set.

Five upstream music tracks and their cue presets are included under CC BY 4.0.
Read `MUSIC_NOTICE.md`; the music has its own license and is not MIT. The music
family in `video_brag_assets` includes per-track source, required attribution,
and license URL. Preserve the returned credits alongside share copy/delivered
outputs and disclose trimming, mixing, and format conversion. Do not run Python
cue analysis. Use the reviewed presets as optional timing guidance, or licensed
user media / Jarvis's managed audio tools. Honor `--no-music` / `--no-sfx`.
Optional beat analysis must never block the render. Keep effects restrained,
aligned to visual events, and safe for the selected tone.

Validate with Jarvis's supported checks and inspect the available visual
evidence before rendering. Deliver only verified artifacts, clearly identify any
unavailable optional poster or audio treatment, and never claim a file exists
or was inspected before a tool confirms it.
