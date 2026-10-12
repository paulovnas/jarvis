# ChatGPT macOS — initial static REA investigation

Date: 2026-10-11 (America/Sao_Paulo). Target: `/Applications/ChatGPT.app`. Beads: `jarvis-gqhy`; optional runtime follow-up: `jarvis-bg4r`.

The installed package contains concrete chat optimizations: measured-height conversation virtualization, adaptive streaming Markdown, cached parsing of stable Markdown prefixes, bounded app-server message draining, and snapshot reconciliation on resume. Its shared HTTP client also implements cached authentication and a constrained refresh/retry path. These are confirmed algorithms in shipped code; their actual activation and performance gains were not measured.

The artifact's visible name is **ChatGPT**, version **26.1007.30912**, build **20156**, ARM64. Its bundle identifier is **`com.openai.codex`**, and the ASAR package names itself **`openai-codex-electron`**. The package contains both ChatGPT conversation code and Codex integration. A name or bundled dependency does not establish which path a particular chat executes.

## How to read the evidence

- **Confirmed static observation:** bytes, parsed metadata, recovered native instructions, or a specific algorithm/control-flow branch in an identified shipped JavaScript member. This does not mean that the app executed that branch.
- **Inference:** the purpose of a mechanism, a possible benefit, or a candidate design for Jarvis. These claims require validation before adoption.
- **Unknown:** active flags/code paths, measured latency/memory, network traffic, or behavior implemented only on OpenAI servers.

`*.js` paths below are members inside `Contents/Resources/app.asar`, not loose installed files. The companion reports give SHA-256 identities and zero-based UTF-8 byte positions. REA's line/column ranges and native virtual addresses use different coordinate systems. Minified function names identify only this exact build.

Read the detailed evidence in [renderer-static.md](renderer-static.md), [transport-static.md](transport-static.md), and [package-resources.md](package-resources.md). The complete exported REA ledger is retained locally as `rea-evidence.json` (118,480,291 bytes) and excluded from Git. The versioned [integrity manifest](rea-evidence.manifest.json) records its SHA-256, counts and evidence identifiers; it does not contain the full observations. The read-only Jarvis/reference comparison is [jarvis-reference-map.md](jarvis-reference-map.md).

The subsequent authorized implementation is recorded separately in [jarvis-implementation.md](/Users/enesolucoes/Documents/Pessoal/jarvis/docs/research/chatgpt-macos-2026-10-11/jarvis-implementation.md): seven implemented mechanisms, behavior tests, complete quality gates and remaining runtime limits, tracked by `jarvis-xd4s`.

## Findings

| Finding | Relevant file/function | Confirmed static evidence | Inference or boundary |
| --- | --- | --- | --- |
| Native bootstrap delegates to the Chromium/Owl shell | `Contents/MacOS/ChatGPT`, `_main` at `0x100000748` | It builds the versioned Codex Framework path, calls `dlopen`, resolves `ChromeMain` with `dlsym`, and calls the returned pointer. REA E3; corroborated assembly calls at `0x1000007e4`, `0x1000007f8`, and `0x10000080c`. | This identifies the bootstrap; it does not recover the framework's entire internal architecture. |
| ChatGPT conversation rows are virtualized | `webview/assets/chatgpt-conversation-page-23bea3bc76ac.js`; `virtualized-turn-list` function `P`; `thread-virtualizer` helpers | The conversation page supplies entries and its row component to the virtualized list, with retained turn keys and measured viewport preservation. The list renders a range plus retained rows. Height measurements/estimates feed binary-search range selection; `ResizeObserver` collects changed measurements. | Reduced offscreen render/layout work is a plausible purpose. Actual cost and accessibility/selection behavior were not measured. |
| Streaming text is buffered on a specific Markdown path | `webview/assets/app-initial-6350d04672d0.js`: `Rsl`, `Bsl`, `usl`/`dsl`; ChatGPT assistant `Ln` → Markdown `hu` | The assistant caller supplies adaptive buffering and a stream ID derived from conversation/message IDs. The buffering implementation resets on non-append changes, schedules drains, publishes through a guarded animation-frame callback, and flushes full text when streaming ends. | Activation depends on caller streaming/animation conditions. This is not evidence that every Markdown surface uses buffering, or that these timings should be copied into Jarvis. |
| Stable Markdown prefixes have their own parser cache | Same `app-initial` member: `Usl`, `Osl`, `ksl` | Streaming parsing separates a stable prefix from a tail and reuses the longest cached prefix. Cache state persists per component; extension-object identity separates maps via `WeakMap`. Each map caps at 100 entries and evicts the first inserted key. | This is a client parser cache, with insertion-order eviction; it does not prove LRU, model prompt caching, or a server cache hit. |
| Local app-server transport avoids an unbounded synchronous drain | `.vite/build/application-network-startup-CfAQmSYg.js`: `Us`, `wr.drain`, `Ws.connect`; constants `As=200`, `js=8` | The stdio path writes JSON plus a newline and parses stdout as JSONL. Queue draining yields after 200 messages or an 8 ms budget using `setImmediate`; consumed references are deleted and the queue compacts after 1,024 consumed items. Exit handling settles the parser and flushes queued messages. | This is a local IPC scheduling mechanism. It does not establish the remote model's SSE/WebSocket protocol or prove a renderer frame rate. |
| Oversized JSONL lines have a parser fallback | Same network member: `bs.consumeBuffer`, `appendLineChunk`, `createStreamingParser`, `completeLine` | Ordinary lines are buffered; exceeding `min(268435456, floor(MAX_STRING_LENGTH/2))` switches to incremental JSON parsing. The parser rejects multiple values per line and reports malformed/incomplete messages. | This guards very large local messages. Assembled objects still exist; it does not prove constant memory or producer backpressure. |
| HTTP/authentication is shared but request profiles differ | `.vite/build/main-CqEoQDOU.js`: `Ude.fetchAuthenticatedJson`, `performDesktopFetch`; `A_` | Code distinguishes `codex`, `chatgpt-native`, and browser/other profiles. It assembles profile-specific headers, rejects authenticated destinations outside an allowlist, reuses/loads/refreshes credentials, and checks principal continuity. On the eligible authenticated `401` path, it cancels the previous body, refreshes, and retries once. | Retry exclusions matter: `retry=never`, selected workspace/IP errors and the specific form-fill operation do not follow that replay path. This cannot be generalized to replaying arbitrary failed tool actions. No actual tokens were read. |
| Backend URLs participate in account-aware routing | `.vite/build/bootstrap-CooIJpZm.js`: `Ut`, `Wt`, `$t`, `en`, `rn`, `an`; main client configuration | Production/local literals are `https://chatgpt.com/backend-api` and `http://localhost:8000/api`. Selection accepts explicit environment overrides. Request routing replaces a caller-supplied routing-override header using authenticated workspace state and checks principal continuity. | These are actual static URL/header-selection paths; the selected environment, outgoing URL and server implementation remain unobserved. |
| General successful HTTP responses can retain a stream body | `Ude.fetchHttp` in the main member | The success path returns a response backed by the body's stream rather than universally reading the whole body first. A separate upload-progress path intentionally buffers its response body and uses progress polling. | This identifies a transport capability; it does not prove which provider/chat operation uses it, the wire format, or network backpressure under load. |
| Resume reconciles snapshots instead of blindly replacing all state | `.vite/build/conversation-turn-mutations-szl89rld.js`: `K`, `Rn` | Item IDs index maps for merging; identical final-answer text can be deduplicated while retaining memory citations. Resume reconciliation handles pagination/stop IDs/reconnect cursors, steering state and an optional preservation of existing terminal status. | Concrete recovery logic exists. Its behavior for real disconnects and concurrent edits was not exercised. |
| Packaging and observability settings are additional leads | `Contents/Info.plist`, runtime configuration and third-party notices | The package declares Chromium 155.0.8059.40, Owl, allocation/graphics settings and components/declarations for Codex CLI, computer-use Node, Sparkle, Sentry, OpenTelemetry, Datadog and query/Markdown libraries. | Package presence and license notices alone do not establish initialized telemetry, sampling, cache behavior, or a performance gain. |

## Native evidence in more detail

The executable's exported `_main` matches the Mach-O entry at file offset **1,864**. REA E3 recovered the framework path:

`../Frameworks/Codex Framework.framework/Versions/155.0.8059.40/Codex Framework`

The relevant recovered instruction sequence includes:

```text
0x1000007e4: bl  imp___stubs__dlopen
0x1000007f8: bl  imp___stubs__dlsym
0x10000080c: blr x8
```

The dossier also records the literal `ChromeMain`. The framework's exported `_ChromeMain` independently corroborates the loader target. Hopper does not expose complete function body ranges through its public Python API; the report therefore makes no complete native call-graph, full-function-boundary, or original-source recovery claim.

## What this suggests for Jarvis

These are comparison candidates, not an implementation plan or demonstrated gains. No Jarvis behavior was changed.

| Candidate | Current verified Jarvis behavior | Evidence needed before choosing a change |
| --- | --- | --- |
| Cache stable Markdown prefixes and gate visual publications | `ChatMarkdown` receives the full current string; Markdown is lazy-loaded and turn bodies are memoized. | Compare parse/layout time for long streamed responses; preserve extension identity, edits, stream resets and final flushes. |
| Add measured row virtualization where long turns still dominate | `Transcript` uses paged history and maps the current page; history already has page/index/cache limits. | Measure DOM/layout pressure within a page, including large tool blocks; verify scroll anchoring, expanded content, selection and accessibility. |
| Bound queue work at the appropriate IPC boundary | Rust emits nondurable updates on a 50 ms elapsed-time gate; frontend applies accepted revision batches directly. | Measure queue bursts and frontend long tasks. A 200-message/8 ms main-process drain and a 50 ms emission gate solve different boundaries. |
| Evaluate connection lifetime across turns | Jarvis owns HTTP pooling and incremental WebSocket state for one turn. The current Codex Rust reference returns WebSocket state to the wider session when a turn client drops. | Compare connection setup and first-event timing across turns, with explicit invalidation on auth/account/model/request changes and cancellation. This comparison is established from reference source, not from decompiling the bundled native CLI. |
| Retain explicit recovery and identity contracts | Jarvis already has revision-gap recovery, bounded replay, incremental-input validation, cancellation and preservation of confirmed tool results. | Use real regression fixtures for disconnects and edits; evaluate the app's ID-based reconciliation as a concept while keeping Jarvis's typed contracts. |

The mandatory Codex, OpenCode and OMP source comparison, including existing tests, is linked in [jarvis-reference-map.md](/Users/enesolucoes/Documents/Pessoal/jarvis/docs/research/chatgpt-macos-2026-10-11/jarvis-reference-map.md). OpenCode's compatible delta coalescing/frame cadence and OMP's stream/Markdown contracts provide additional comparison points. None of those tests were rerun during this investigation.

## Reproducible artifact identity

| Artifact | SHA-256 or directory identity |
| --- | --- |
| `Contents/MacOS/ChatGPT` (71,008 bytes) | `2053a78d225a1492d0cb6896cfd83254af06c3277547e83d4d23fef88f00426e` |
| `Contents/Resources/app.asar` (567,757,587 bytes) | `75e397e2280dde93b70a85aa926f4282c728eacc84dfeb15ba7fb6ddd84628af` |
| Versioned Codex Framework (272,897,824 bytes) | `a6daa6b1284b8f41264196d37307957a518c1d498ee2881acd93d0c729265dd5` |
| REA application-directory identity | `11987fdc3b01ea720a9351d433fcf738587e26ec3beb8ccb513c21cc99a9bb73` |
| Main member `.vite/build/main-CqEoQDOU.js` | `09d19386e1f23cf4ccbc950fc40d85a1e8a064a08be588b2a4be6a89f6ada9e6` |
| Renderer member `webview/assets/app-initial-6350d04672d0.js` | `604b5d2de4fbb41f3d0dc5b3662129c558e1c7b8aac8cc87225286f58dcfaf8f` |

The package inventory covers **3,977 regular outer files**. The ASAR header/extraction identifies **21,089 internal members**, including **15,067 JavaScript files** and no `.map` members in that bounded header inventory. Some JavaScript files still declare source-map URLs; a declaration does not supply the absent map.

REA materialized copies into owned temporary directories for static inspection. Original installed bytes were not edited. After exporting the ledger and closing the REA session, those copies were moved reversibly to macOS Trash; the original temporary paths no longer exist. The reports and evidence bundle are retained in this research directory; temporary extraction paths are provenance, not durable report links.

## Integrity and coverage limits

REA recorded **seven** declared-vs-observed integrity contradictions in unpacked native ASAR members. These include Worklouder permissions/HID/serial bindings, `better-sqlite3`, `node-pty` and `objc-js`. Their bytes remain classified **observed-untrusted**. The packed JavaScript members used for the findings have matching observed hashes; the discrepancy was not silently applied to them.

Read-only macOS `codesign --verify --deep --strict` succeeded. macOS signing and the ASAR's member-declared hashes are separate checks. The mismatch's cause is unknown; post-archive signing/stripping is a possible explanation suggested by REA's diagnostic, not a finding of this investigation. Neither corruption nor tampering was established.

The REA AST pass deliberately covered the **34 JavaScript files in `.vite/build`**, reading **10,333,211 bytes**, with **zero parse failures**. Its application/semantic graph coverage is **partial**: relationship/result budgets reserved some files' static observations without semantic graph expansion, and the isolated worker exhausted its heap on semantic analysis of the Zod bundle. Complete retained records preserve these limitations. Manually reviewed, hash-bound control flow supplies the detailed transport findings; it is not represented as a globally complete automated graph.

The large package/extraction responses exceeded the MCP transport frame. REA retained the complete records, and the investigation exported them instead of repeating the producer or treating the visible preview as canonical JSON. Ghidra was not configured and IDA was unavailable; the available Hopper provider supplied the native dossier. No analysis-engine installation or configuration repair was performed.

## Evidence index

All IDs below resolve in the locally retained `rea-evidence.json` and are listed in the versioned integrity manifest. The numbered aliases are report labels, not additional REA observations.

| Alias | REA Evidence ID | Scope |
| --- | --- | --- |
| E1 | `ev_70fee48e02cb55a5faa2a9e960b27927c5bb0feb24ba9dcc76f318a81292ee00` | Whole package inventory, hashes and integrity contradictions |
| E2 | `ev_dcb7919ebcb5cbbe936e176c23cb3cd1248cff487fbeb8cc390be1ed75cb47b1` | Main Mach-O metadata/imports/entry/export evidence |
| E3 | `ev_f39166a73ce427b4022d5d9de2e32161a8ef42ff625913b67abf4c4426a734a4` | Hopper `_main` dossier: pseudocode, assembly and explicit limitations |
| E4 | `ev_7f6b556fa07391d8c6510e678eddd4c01fb7e9cad1f6112d5e647faaa08bf3bd` | Parsed outer Info.plist |
| E5 | `ev_bb6e095aa440be97ef81f62516ea4b989a01f3843ef2acb109ab57046cd6df2f` | ASAR extraction, exact member identities and integrity status |
| E6 | `ev_f63aa33e160596f7ea537667bb67977c4938a44fb83b1f8692b4d28b4220c9bb` | Bounded `.vite/build` AST/application analysis with partial semantic coverage |
| E7 | `ev_f8c6c6ec61530a2c23e4fea4a375a1a785839a6750f2e2d595bbba91a1617e88` | Exact main JS asset, retained static facts and source ranges |
| E8 | `ev_e369b8b3b03e966250b88a6f3cc8488c982140b5c27eaadd33c1c2a449256c01` | Conversation mutation/resume JS asset and static facts |
| E9 | `ev_9475f4d4cc858b66945b3e26d5801a21c8833f21f830c7b2ff54c73fb2d27b8c` | Preload asset; `electron` import, contextBridge and IPC observations |

## Question coverage and remaining unknowns

| Question | Status of this static pass |
| --- | --- |
| What is the actual installed artifact/runtime? | Answered within package and bootstrap coverage. |
| Which concrete chat/UI optimizations are shipped? | Answered for the inspected ChatGPT conversation/Markdown/list paths; activation and gains unmeasured. |
| How does local backend communication avoid synchronous queue bursts? | Answered for the inspected JSONL/stdio queue and optional local-daemon path. |
| How are OpenAI HTTP/auth requests assembled and recovered? | Partially answered by the shared main-process client and profiles; actual outgoing traffic and the native model-provider implementation were not traced end to end. |
| What caching is established? | Client Markdown/token/snapshot mechanisms are established where documented. Model prompt/server caches are unknown. |
| How are interrupted chats resumed? | Selected static merge/pagination/terminal-state mechanisms are established; real reconnection behavior untested. |
| Which concepts already exist in Jarvis/references? | Answered by current read-only source and test inspection; no implementation or benchmark. |

REA retained three explicit unresolved questions:

- `unk_e90b59d630ff8d79d6e272d9dc7456ccfc8bce6d30622d7e6717d8e93ad2f65c`: active packaged paths and actual outbound requests for a defined chat scenario.
- `unk_1ca6b5efbc78fa10893fcda5f747c5ce2d190c9209c71e0b02ae8cea245d9771`: measured rendering/latency/memory benefit under comparable workloads.
- `unk_9b28372270f512df433cf6c65e6d3c7bd2664de8950ee8675e62129bc212c098`: server-side caching, routing, inference, compaction and accounting beyond the desktop boundary.

This pass did not launch or attach to ChatGPT, capture live requests, inspect account/user state, benchmark performance, fully decompile the bundled Codex CLI or the Chromium framework, or change Jarvis source. Static evidence cannot quantify an improvement or reveal OpenAI's complete server implementation. No commits, push or remote Beads sync were performed.

Report validation checked that cited REA IDs and three unknown IDs exist in the exported ledger, 63 local report/reference links resolve with valid line targets, and 17 selected member identity/range ledger entries agree with original ASAR bytes and declared hashes. The retained ledger contains 20 records. No frontend/Rust quality gates were run because this pass contains research artifacts and no implementation.
