# Static transport and conversation-state findings

Target: `/Applications/ChatGPT.app/Contents/Resources/app.asar`.
This is an offline examination of shipped bytes, not an observation of requests or application execution. Application code was parsed with the installed Acorn parser; it was never imported or evaluated. No user profiles, tokens, conversations, or live network traffic were read.

## Provenance and scope

ASAR SHA-256: `75e397e2280dde93b70a85aa926f4282c728eacc84dfeb15ba7fb6ddd84628af`. The archive has 567,757,587 bytes; member data begins at ASAR byte 5,857,252. All selected members below were read directly from that archive and their SHA-256 values matched their declared member integrity hashes. Byte ranges in this report are zero-based, half-open ranges in the original UTF-8 member, not JavaScript UTF-16 character positions. Add the member's ASAR start to obtain an archive range.

| ASAR member | Size | ASAR start | Observed SHA-256 |
|---|---:|---:|---|
| `package.json` | 5,754 | 25,842,464 | `97e6c90d6760d306852720fcdbc4a2946e54bc406406e1d03711a194955d1e2e` |
| `.vite/build/application-network-startup-CfAQmSYg.js` | 290,566 | 5,860,269 | `cd6e8a8fbc4354a6e6ff19c6e4bf6fc1fb3960c63708c83871357c6238f516a8` |
| `.vite/build/conversation-turn-mutations-szl89rld.js` | 52,799 | 7,292,697 | `116870fb4aa77e0d56a9c1aef70b7c6b21a7ea30ef5b601a6d2a7cb7b719c3ec` |
| `.vite/build/main-CqEoQDOU.js` | 3,824,411 | 7,673,309 | `09d19386e1f23cf4ccbc950fc40d85a1e8a064a08be588b2a4be6a89f6ada9e6` |
| `.vite/build/preload.js` | 6,909 | 11,549,044 | `a3e622f8d80a83e58fb3cfcc640fa7e14e6461cccdae9ef7d7b0c821dde256b7` |
| `.vite/build/early-bootstrap.js` | 3,304 | 7,666,401 | `36b8494989d89f9b063a27f10b60119243c8e4cc8abd2ef8a9743c507d15019c` |
| `.vite/build/build-flavor-CYXpoCFI.js` | 3,853 | 7,284,192 | `23229dac961a82c52237acc32598ddb2c1cf370b24c64fc507b9c24bcf2c4d93` |
| `.vite/build/bootstrap-CooIJpZm.js` | 461,537 | 6,150,835 | `d1ab6d0bb5aabf4d56a3cb39d979dc1145e41b47be6821b967cba3802af3fbf0` |
| `.vite/build/src-CP7XXlpZ.js` | 203,781 | 13,204,354 | `79623d50989f9efbcb8e5752c56b3cc608a0f5993c05112e03e11b718ac7a586` |

Related REA Evidence:

- Archive inventory: `ev_70fee48e02cb55a5faa2a9e960b27927c5bb0feb24ba9dcc76f318a81292ee00`.
- Archive extraction: `ev_bb6e095aa440be97ef81f62516ea4b989a01f3843ef2acb109ab57046cd6df2f`.
- JavaScript analysis parent: `ev_f63aa33e160596f7ea537667bb67977c4938a44fb83b1f8692b4d28b4220c9bb`.
- Main module view: `ev_f8c6c6ec61530a2c23e4fea4a375a1a785839a6750f2e2d595bbba91a1617e88`.
- Conversation module view: `ev_e369b8b3b03e966250b88a6f3cc8488c982140b5c27eaadd33c1c2a449256c01`.
- Preload module view: `ev_9475f4d4cc858b66945b3e26d5801a21c8833f21f830c7b2ff54c73fb2d27b8c`.

The REA parent parsed 34 sources without parse failures, but semantic coverage is partial because of analysis budget/heap limits. The control-flow findings below are analyst derivations from the independently verified original member bytes. They are not claims that REA's incomplete semantic graph established every call or that any code ran.

## Confirmed in shipped code

### 1. The packaged JavaScript identifies itself as Codex; product execution remains unobserved

`package.json` declares `name: openai-codex-electron`, `productName: Codex`, `description: Codex`, `version: 26.1007.30912`, `codexBuildFlavor: prod`, and entry point `.vite/build/early-bootstrap.js`. This is distinct from the user-supplied installed path `ChatGPT.app`.

The examined build-flavor module chooses release channels (`Dev`, `Nightly`, `InternalAlpha`, `PublicBeta`, `Prod`) through environment/package metadata. It did not establish a ChatGPT-versus-Codex product selection branch. A product choice elsewhere, native configuration, or a runtime override remains possible and unexamined here.

### 2. Local Codex app-server traffic uses JSONL and a cooperative dispatch queue

In `application-network-startup-CfAQmSYg.js`:

| Function/class | Member byte range | Static behavior |
|---|---|---|
| `Us.constructor` | `[255282,255712)` | Creates a `ws` stdout message reader; reports parse failures and enqueues each completed JSONL message. |
| `Us.spawnProcess` | `[256438,257506)` | Spawns the selected executable with stdin/stdout/stderr pipes; attaches stdout parser and process handlers. |
| `Us.send` | `[255712,255965)` | Writes the serialized message followed by a newline to child stdin. |
| `Us.scheduleStdoutDispatch` | `[259525,259665)` | Uses `setImmediate` and prevents duplicate pending dispatch handles. |
| `Us.dispatchPendingStdoutLines` | `[259727,260424)` | Drains at most 200 messages or 8 ms per ordinary slice, then schedules another slice if messages remain. |
| `Us.finishProcessExitIfReady` | `[257673,258025)` | Waits for parser completion and process exit, flushes queued messages, then emits close. |
| `wr` | `[65203,67088)` | Queue deletes consumed item references, resets when empty, and compacts after sufficient consumed items. |

Selected exact fragment from `Us.dispatchPendingStdoutLines`:

```js
this.stdoutMessageQueue.drain({flushAll:e,maxItemsPerSlice:As,maxSliceMs:js,onItem:e=>{this.onmessage?.({type:`message`,data:e})}})
```

The constants are `As=200` at `[254523,254529)`, `js=8` at `[254530,254534)`, and `Is=1024` at `[254603,254610)`; `Is` supplies the queue's consumed-item compaction threshold. This establishes cooperative scheduling and release of consumed references. It does not establish a memory limit, producer backpressure, measured responsiveness, or actual queue depths. In particular, `Us.send` does not check stdin `write()`'s return value in the inspected function.

`Ws.connect`, `[260695,261579)`, defaults to `new Us(...)`. A narrowly gated non-Windows local-daemon path instead connects to a Unix socket under `app-server-control/app-server-control.sock`, using the URL `ws://localhost/rpc`. Its conditions include `CODEX_APP_SERVER_USE_LOCAL_DAEMON=1`, absence of forced CLI/runtime overrides, and a successful availability check. This WebSocket is local app-server RPC, not proof of ChatGPT's remote conversation streaming protocol.

### 3. Large JSONL parsing has an incremental fallback

`bs.consumeBuffer`, `[249331,249671)`, splits at byte 10 (newline). `bs.appendLineChunk`, `[249897,250443)`, buffers ordinary lines and changes to a streaming parser if its threshold is exceeded. `bs.createStreamingParser`, `[250443,251204)`, enables `jsonStreaming`, rejects multiple JSON values per line, assembles objects, and uses chunked arrays. `bs.completeLine`, `[252549,253470)`, emits one complete message with receive timing and line-byte metadata; malformed/incomplete lines invoke an error handler.

The default buffered-line threshold is `min(268435456, floor(MAX_STRING_LENGTH/2))`; it is not a small per-token threshold. This is a fallback for oversized app-server messages. It does not prove constant-memory parsing of complete conversations: assembled message objects still exist.

### 4. HTTP authentication is centralized and reuses cached app-server credentials

In `main-CqEoQDOU.js`, class `Ude` spans `[688178,705451)`.

- `dde`, `[655430,655616)`, decides whether to attach credentials; it respects an existing `authorization` header rather than automatically replacing it.
- `A_`, `[655616,655964)`, calls `getCachedAuthToken()` first when refresh is not requested, otherwise loads/refreshed credentials through `getAuthToken({refreshToken})` on the local app-server client.
- `j_`, `[655964,656290)`, validates an expected account ID before applying credentials.
- `Ude.fetchAuthenticatedJson`, `[688306,689341)`, builds profile-specific headers. Profiles include `codex`, `chatgpt-native`, and a browser alternative. It supports `OAI-Language`, `oai-did`, `ChatGPT-Account-ID`, desktop/browser originators, and surface headers; a bodyless GET drops the JSON content type.
- `Ude.performDesktopFetch`, `[694261,699846)`, rejects credential/surface/integrity attachment to disallowed URLs, checks the principal again before network dispatch, and composes cancellation with a workspace-auth signal.

In `bootstrap-CooIJpZm.js`, exported helper `on` (`exports.it`), `[13870,14180)`, assembles the literal header form `Authorization: Bearer ${t}`, adds an account header and conditional FedRAMP header, and delegates surface headers to `sn`, `[14180,14319)`. These are code templates, not captured credentials.

`Ude.isDesktopAuthAllowedUrl`, `[702958,703533)`, checks HTTP(S), OpenAI/ChatGPT host patterns, explicit localhost exceptions and a particular cloud-environment status exception. `status.openai.com` is rejected. Static rules alone do not prove a live enforcement result.

### 5. The shared backend defaults to ChatGPT's backend API; it can be routed or overridden

`bootstrap-CooIJpZm.js` contains `Ut=https://chatgpt.com/backend-api`, `[11245,11281)`, and `Wt=http://localhost:8000/api`, `[11282,11312)`. `$t`, `[11846,12045)`, selects an explicit `CODEX_API_BASE_URL`, otherwise local or production options according to `CODEX_API_ENDPOINT`. `en` (`exports.dt`), `[12045,12083)`, combines that base with the requested path through `src-CP7XXlpZ.js`'s `sc`, `[88497,88735)`, which also handles `public-api` path reshaping.

The main module passes `g.gt` and `g.It` as development and production API bases at `[1445819,1445857)`. Consequently these endpoint literals participate in a real static URL-selection path, rather than merely occurring in a library string table. Actual selected environment and requested URL remain unknown.

`bootstrap.rn` (`exports.at`), `[13270,13653)`, removes a caller-supplied `X-OpenAI-Account-Routing-Override`, then derives any permitted routing header and backend origin from authenticated workspace routing state. `an`, `[13653,13870)`, rejects a workspace principal change. This is evidence of account-aware request routing; it is not proof that the inspected desktop process made a cloud request.

### 6. Authenticated HTTP retry is bounded; successful response bodies remain streams

`Ude.performDesktopFetch`, `[694261,699846)`, performs an initial fetch and has one explicit retry branch after an authenticated 401. That branch cancels the previous response body, refreshes credentials and fetches again. Retry is excluded for `retry: never`, a detected workspace IP authorization error, and a POST ending in `/flora/browser/form-fill`. There is no retry loop in this function.

`Ude.fetchHttp`, `[691823,694116)`, returns a `Response` wrapping the successful response's existing body and records `responseType: stream-open`; it does not call `text()`/`json()` on that success path. Errors are converted to structured `fetch-response` objects, including status, error kind/code and an abort status of 499.

`Ude.cancelRequest`, `[694116,694261)`, aborts the controller selected by a composite request key. Separate upload-progress handling in `Ude.performProgressRequest`, `[699846,700887)`, polls progress every 50 ms and buffers that endpoint's response before constructing a `Response`. Therefore it would be incorrect to claim every HTTP path avoids response buffering.

The generic fetch path establishes streaming body transport. It does not establish SSE framing, token parsing, remote WebSocket reuse or the model-provider stream protocol.

### 7. Snapshot merging preserves prior chat state and reconciles pagination

In `conversation-turn-mutations-szl89rld.js`, `K`, `[36491,37524)`, indexes existing/incoming items by ID and merges around existing positions. It can match `agentMessage` final answers by identical text and retain memory citations, reducing duplicate final-answer items during reconciliation.

`Rn`, `[38231,41516)`, merges resumed snapshots and tracks `stopItemId`, `olderCursorAfterReconnect`, `beforeItemId`, and `hasLoadedOldest`. Its policy can preserve existing terminal status/error instead of replacing them with an incoming `inProgress` snapshot. It also preserves `steeringUserMessage` items, prior request metadata, timing fields and completion IDs when appropriate.

This is a concrete static recovery algorithm, not a demonstration that every reconnect is handled correctly. The inspected function exposes policy flags; caller selection and operational results were not exhaustively traced.

### 8. A separate transferable IPC port connects renderer and app host

`preload.js`, `[6711,6872)`, installs a window message listener that requires `source===window` and type `connect-app-host`, then transfers the supplied port through Electron IPC. The channel variable `_`, `[2286,2320)`, is `codex_desktop:connect-app-host`. The preload also exposes `electronBridge` and `codexWindowType`.

This confirms a distinct IPC transport seam. No throughput comparison or complete trust-boundary audit of the bridge was performed.

## Inferences worth evaluating for Jarvis

These are design hypotheses, not measured improvements or implementation instructions:

1. Budgeting inbound dispatch by elapsed time and message count may keep the UI thread responsive during bursts. The 200/8 ms values are evidence of this app's choices, not recommended universal constants.
2. Releasing consumed queue references and reconciling snapshots by stable item IDs may reduce retained memory and duplicated messages without sacrificing confirmed terminal state or user steering.
3. A shared authenticated HTTP service with cached credentials, principal checks, explicit retry exceptions, streaming bodies and structured cancellation/errors is a useful separation from the agent's local RPC channel.
4. Treat local app-server JSONL, renderer IPC and remote HTTP as separate protocols. The presence of a local WebSocket does not imply remote model transport is a WebSocket.

## Remaining unknowns and bounded negatives

- The installed path and Codex package identity disagree; which product surfaces and provider/chat routes run in this installation was not established.
- Exact remote conversational event framing, connection reuse, model-request payloads, prompt caching/compaction strategy, rate-limit backoff, HTTP connection pooling and provider-side token generation are unresolved in this pass.
- Exact substrings `text/event-stream`, `/v1/responses`, `response.output_text.delta`, `backend-api/conversation` and `api.openai.com/v1` were not found in **main-CqEoQDOU.js only**. This does not establish absence in other bundled code, the CLI, native modules, server-side behavior, or runtime-generated strings.
- `coalesce`/priority options are forwarded by main `_fe.send`, `[733802,735041)`, to `connection.handleHostRequest`; the downstream coalescing algorithm was not traced. The keyword is not evidence of achieved deduplication or lower request volume.
- No application execution, live request capture, user-state examination, performance benchmark, tests of Jarvis code, or change to Jarvis application code was made.
