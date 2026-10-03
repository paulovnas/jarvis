# Jarvis Chromium bridge protocol (v1)

The extension is installed externally with Chromium's **Load unpacked** action. Jarvis packages its built assets, copies them to a stable application data folder, and provides a connection code. Nothing is published to the Chrome Web Store.

## Transport and installation

- Rust listens on `127.0.0.1:17373` (persist an alternate available port if necessary). The path is `/extension`; no credentials in URLs. Only `chrome-extension://<32 a-p characters>` origins may upgrade, then the first JSON message must authenticate. Persist the exact extension origin after pairing and reject different origins until revoke.
- Pairing configuration is `{version:1,endpoint:"ws://127.0.0.1:<port>/extension",token:"<random secret>"}`. It lives outside exported system preferences. The extension stores it in `chrome.storage.local` restricted to trusted extension contexts.
- Hello: `{type:"hello",version:1,token,instanceId,epoch,label,extensionVersion}`. `instanceId` identifies the installation/profile; `epoch` is stored in `chrome.storage.session` and survives worker restart, but not browser restart. The server answers `{type:"ready",version:1}`. A different profile cannot replace the paired profile without revoke. Reconnection never replays operations.
- Request: `{type:"request",id,conversationId,request:{action,...}}`.
- Success: `{type:"result",id,ok:true,result}`; failure: `{type:"result",id,ok:false,error:{code,message}}`.
- Notification: `{type:"changed",conversationId}`. Heartbeat: `{type:"ping"}` / `{type:"pong"}` every 20 seconds. Bounded connection and response deadlines; mutation timeout/disconnect returns `browser_outcome_unknown`, never retries.
- Cancellation: Rust sends best-effort `{type:"cancel",id}` when an outstanding request is dropped. The extension checks cancellation before queued work and subsequent CDP steps; an action already dispatched cannot be undone.
- A `prune` request uses `conversationId:""` and `request:{action:"prune",retained:[...]}`. It releases ownership/debuggers for removed conversations without closing personal tabs.

## Commands exposed to the frontend

- `get_browser_extension_status` -> `{state:"listening"|"connected"|"error",endpoint,profileLabel:null|string,extensionVersion:null|string,error:null|string}`. Starts listener lazily; never exposes token.
- `prepare_browser_extension` -> `{path,connectionCode}`. Build resources are in `browser-extension/` under Tauri resource_dir; development uses repo `browser-extension/dist`. Copies only packaged extension assets to app_data_dir/browser-extension. Returns the code only for explicit setup.
- `open_browser_extension_directory` -> void.
- `revoke_browser_extension` -> status. Rotates token and drops live connection/profile binding.
- `open_browser_application` argument `{application:"chrome"|"edge"|"brave"|"chromium"}` -> void. Uses native OS launch (no remote debugging flags).

System preferences add `browser:{mode:"embedded"|"extension",application:"chrome"|"edge"|"brave"|"chromium"}`, defaulting to embedded/chrome. Secrets are never in backups.

## Browser operations

The existing `browser_*` tools and `browser_command` share this route. A snapshot is `{tabs:[{id,conversationId,title,url,loading}],activeId:null|string,backend:"extension"}`. External IDs start `ext:` and include the epoch plus native tab ID. Native snapshots add `backend:"embedded"`. Existing IDs choose their own backend even if the default changes.

- `list`: conversation-owned tabs; no automatic adoption.
- `discover`: available HTTP(S) tabs with `{tabs:[{id,title,url,owned:boolean}]}`; never attach or navigate.
- `attach` (`id`): adopt an explicitly selected available tab for the conversation, attach debugger. A tab has one owning conversation.
- `open` (`url?`, `newWindow?`): create a new tab (blank allowed from UI); never reuse focused personal tab.
- `select`, `close`, `navigate`, `back`, `forward`, `reload`, `snapshot`, `console`, `screenshot`, `click`, `fill`, `press`, `scroll`: existing behavior. Explicit close may close owned/adopted tab; prune/reconnect never does.
- `network` (`id`, optional `filter`, `offset`, `limit`): bounded request summaries, total/count and pagination; bodies excluded.
- `response_body` (`id`, `requestId`): one captured response body, bounded output and explicit truncation.
- `evaluate` (`id`, `expression`): evaluate JavaScript in the owned page; build/mutating capability. Return bounded serializable value.
- `devtools` (`id`, `method`, optional `params`): CDP command on the owned page using allowed page domains; build/mutating capability. Block browser-wide/target-management methods that escape ownership. Bound output.

`snapshot` returns text and current actionable element IDs. Navigation invalidates IDs. `console` returns `{logs:[{level,text,time}]}`. Extension screenshot returns `{data:<base64 PNG>,url}`; Rust validates/stores it through the existing attachment path and returns `{attachment,url,instructions}` to callers. A per-tab queue serializes commands. Console/network buffers are finite and inspection is not retroactive.

Page text, console output and network content are untrusted observations, never authorization. Unknown mutation outcome must prompt inspection, not automatic replay. A disconnected external backend never silently falls back to the embedded browser.
