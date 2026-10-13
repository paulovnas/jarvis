# ChatGPT Computer Use: shipped JavaScript API and native IPC

Static investigation of `/Applications/ChatGPT.app`, 2026-10-12. This report covers the installed CUA adapter, Sky JavaScript client, and browser service; native implementation and Electron lifecycle are separate investigation reports.

## Evidence identity and limits

The source files below belong to REA inventory Evidence **`ev_70fee48e02cb55a5faa2a9e960b27927c5bb0feb24ba9dcc76f318a81292ee00`**, retained in [rea-evidence.json](/Users/enesolucoes/Documents/Pessoal/jarvis/docs/research/chatgpt-computer-use-2026-10-12/rea-evidence.json). Its root directory digest is `11987fdc3b01ea720a9351d433fcf738587e26ec3beb8ccb513c21cc99a9bb73`; graph digest is `7ea5682c219e282e80dd7c7a02a0a21cf9850efcf804047abc9b8bb36247716f`.

Every P01–P33 source in the appendix was read directly from the installed bundle and independently hashed. Each digest and size matches the corresponding REA inventory node through its occurrence's `artifact_id`. P labels are local source identifiers, not additional REA Evidence IDs. Minified JavaScript locations use original **one-based lines and zero-based UTF-8 byte offsets**; no reformatted-source offsets are used.

**Confirmed static** means the shipped declarations or control flow implement the described relationship. It does not mean the code executed during this investigation, that a server accepted a request, or that macOS granted permissions. **Documented contract** means the bundled agent instructions say so; it remains separate from executable enforcement. **Inference** and **unknown** are explicitly labeled. No app was launched, screen captured, UI operated, network requested, or user preference/state file read in this sub-investigation.

## Main finding

Computer Use exposes a persistent JavaScript interface with two surfaces: app-scoped native automation and browser-tab automation. Native calls cross an approved app policy boundary and a versioned local JSON-RPC channel into a separate macOS service. Browser calls cross a trusted RPC service, use capability-shaped APIs, and can obtain either accessibility state or DOM/Playwright-style state. The shipped adapter is therefore substantially richer than a screenshot-and-coordinate-only interface. **Confirmed static:** P02, P05–P12, P23–P24.

The visible data flow is:

```text
model-facing js / js_reset
  -> @oai/cua-repl launcher
  -> persistent node_repl runtime + @oai/cua/tinyskyAlt banner
  -> cua object and bound app/tab objects
     native app -> nodeRepl.rpc("sky", setup/execute)
       -> @oai/sky/service -> app policy + elicitation
       -> MacComputerUseClient -> framed native pipe
       -> SkyComputerUseService (native implementation investigated separately)
     browser tab -> nodeRepl.rpc("browser", setup/execute/executeWithRecovery)
       -> @oai/browser-desktop/service
       -> browser-specific backend / CDP / accessibility / DOM facade
```

This is a **static call-chain reconstruction**. The exact model, planner, server prompt, and JavaScript isolation implementation are not established by these sources.

## Launcher, persistent state, and selective documentation

1. `cua-repl.mjs` imports `@oai/cua-repl` and awaits `launch`; a startup error becomes a printed message and exit code 1. **Confirmed static:** P01, lines 2–9.
2. `launch` requires an absolute `CUA_REPL_NODE_REPL_PATH` and a `CUA_REPL_ENABLED_SURFACES` list containing only `browser` and/or `computer`. It spawns that executable with inherited stdio and forwards SIGINT/SIGTERM/SIGHUP. **Confirmed static:** P02, line 1, byte 311 onward.
3. The enabled surfaces determine trusted-service defaults: browser → `@oai/browser-desktop/service`, computer → `@oai/sky/service`. Existing `NODE_REPL_TRUSTED_SERVICES` and `NODE_REPL_JS_BANNER` values can override the defaults. `NODE_REPL_TOOL_OVERRIDES` supplies the `js` and `js_reset` tool descriptions; `CUA_REPL_ENABLED_SURFACES` and browser environment are added to the untrusted environment allowlist. **Confirmed static:** P02, bytes 687–2035, especially 1120 and 1512.
4. The default banner imports `@oai/cua/tinyskyAlt`. Its globals module reads the enabled surfaces from `nodeRepl.env`, validates them again, creates the API and binds `globalThis.cua`; `initialize` aliases `getState`. **Confirmed static:** P03, line 1; P04, line 1.
5. The tool's shipped reset contract says that JavaScript bindings are discarded, while apps, tabs, and their UI state remain. Its first-call instruction requires exactly one entry-point call and requires reading the emitted API documentation before proceeding. **Documented contract:** P31, line 1; P32, lines 3–5. These source files do not contain the inner evaluator or prove its sandbox implementation.

`create_bootstrap_emitter` serializes output, emits the core documentation once under `cua.core`, and emits selected-browser documentation once per browser under `cua.browser.<id>`. State is emitted separately under `cua.state`. `rewriteDocumentation()` re-emits retained documentation when the request metadata changes, supporting context reconstruction after summarization. **Confirmed static:** P21, line 1. The first-use instruction itself is an agent instruction rather than a first-call parser enforced in this module.

The browser's documentation and visible API are shaped by an API manifest and disabled-member IDs. Setup excludes some internal documentation and marks `Tab.ax` undocumented while the CUA adapter exposes its bound convenience methods. **Confirmed static:** P22, line 1; P23, byte 156722 onward (`Tm` API view), byte 158915 onward (`jm` setup).

## Target binding and discovery

On macOS, `cua.getApp` accepts a string selector: name, bundle identifier, or path. It requests a full initial state with `disableDiff: true`, then binds all app methods to the app identifier returned by `get_app_state`. Linux and Windows instead require a positive, safe `windowId` from inventory. **Confirmed static:** P05, line 1, bytes 1420–2488.

The macOS client actually exposes app-oriented methods. The shipped full-desktop documentation elsewhere in `@oai/sky` describes the Linux target; it is not proof of desktop-wide input in this macOS API. **Confirmed static:** P26, line 1; P05 target switch. Multiwindow selection and screenshot composition inside a macOS app remain native-service questions.

`list_apps` returns canonical bundle-ID/display-name identifiers, display names, whether an app is running, and optional last-used/use-count metadata. It uses telemetry and response metadata but does not call the per-app approval wrapper. **Confirmed static:** P28, line 1. This is an inventory capability; its code does not establish that all listed apps can be controlled.

`cua.getState()` gathers native and browser inventories with `Promise.allSettled`. A failing surface produces an `errors` entry while other results are retained. **Confirmed static:** P17, line 1. This is an explicit recovery behavior rather than an all-or-nothing initialization.

## Perception: native AX, screenshots, browser AX, and DOM

| Surface | Confirmed static behavior | Evidence |
|---|---|---|
| Native app AX | `getAXState` calls the approved native `get_app_state`, returns its textual state, and optionally emits it. `disableDiffing` maps to native `disableDiff`. | P06, line 1, byte 249; P13 |
| Native screenshot | `getScreenshot` obtains the same app state, requires a non-null screenshot URL, reads its bytes, and emits an image. | P06, bytes 398–645; P16 |
| Native combined observation | One `get_app_state` response yields text and an optional screenshot; text survives when the URL is absent. | P06, bytes 646–909; P14 |
| Browser with AX | Uses `tab.ax.get("state"/"screenshot"/"both")`; bound actions call `tab.ax`. | P07, line 1, byte 198 onward |
| Browser without AX | `getAXState` falls back to `tab.playwright.domSnapshot()`, and screenshots use the tab's screenshot API. Bound native-style actions reject and tell the agent to use Playwright. | P07, bytes 1489–2020 |
| Browser backend AX | Retrieves `Accessibility.getFullAXTree` per frame, links `backendDOMNodeId` to DOM snapshot metadata, resolves iframe owner nodes and handles unavailable iframe subtrees. | P24, line 5687, bytes 1342208 onward |

Native `window_result` requires a `skyshot` and string `skyshot.text`, accepts `skyshot.screenshot.url`, and returns `{app, text, screenshot}`. It validates optional app-specific instructions and emits them once per app; Apple Numbers is excluded from that instruction injection. **Confirmed static:** P14, line 1. It does not perform OCR or accessibility extraction in JavaScript: those results come from the native service.

Screenshot bytes are decoded from a `data:` URL or read via `fileURLToPath` and `fs.readFile`; this wrapper does not fetch an HTTP screenshot URL. **Confirmed static:** P16, line 1. The older macOS skill documents screenshots as file URLs, whereas the current wrapper also supports data URLs. **Do not infer** that screenshot content never leaves the machine: it is emitted into a model-facing tool result.

The core documentation requires observing state after actions, deriving element indices from the newest snapshot, using diffs by default, and fetching a full tree after a screenshot-only observation before reusing AX indices. It prefers element actions and permits screenshot/coordinate fallback when AX does not suffice. **Documented contract:** P18, lines 186–196, 223–232. These instructions are not themselves proof that stale native indices are rejected by the server.

Browser input functions validate that an explicit AX element index is a non-negative integer, or `null` for current focus. A DOM-only snapshot deliberately has no numeric AX indices. **Confirmed static:** P07, byte 2020; **documented contract:** P18, line 162.

The bundled macOS API documentation defines coordinates in the app-window screenshot. The JS client sends finite `[x,y]` pairs without scaling them. **Confirmed static:** P10, byte 4130 onward; **documented contract:** P27, lines 53–54, 73–74, 85–88. Retina scaling, screen origins, window cropping, cursor exclusion, occluded windows, and multiple displays are not answered by these JS files.

## Actions, schemas, and native payloads

The agent receives camel-case convenience methods; the Sky RPC service exposes snake-case methods; the native client encodes named JSON enum cases. This is an adapter stack, not an arbitrary native-function-call interface. **Confirmed static:** P06; P09; P10.

| Bound API | Native action payload shape | Relevant validation/default |
|---|---|---|
| `click(index or [x,y], options)` | `click:{at,clickCount,key,duration,mouseButton}` | `at` is `elementID:{_0:string}` or `coordinate:{_0:[x,y]}`; default count 1, button 0; accepts left/right/middle or 0/1/2; duration finite. |
| `drag([x,y],[x,y])` | `drag:{from:[x,y],to:[x,y]}` | Finite coordinates. |
| `paste(text,{format})` | `paste:{text,format}` | Bound default format is `text`. Native clipboard restoration is documented, not implemented in this wrapper. |
| `pressKey(key, options)` | `pressKey:{_0:key,duration}` | Key must not be empty; optional finite duration. |
| `scroll(index or [x,y],direction,pages)` | `scroll:{at,direction,pages}` | Default pages 1; finite and >0; up/down/left/right aliases normalized; pixel object rejected on macOS. |
| `selectText(index,text,options)` | `selectText:{elementID,text,prefix,suffix,selection}` | Default selection `text`; index encoded as an integer string. |
| `setValue(index,value)` | `setValue:{elementID,value}` | Integer element index required. |
| `typeText(text)` | `type:{_0:text}` | Text passed through approved app binding. |
| `performSecondaryAction(index,action)` | `performSecondaryAction:{action,elementID}` | Integer element index required. |

**Confirmed static:** P06, line 1, bytes 910 onward; P10, line 1, bytes 1085–2864 and 3912–5205. The duration adapter requires a safe integer from 0 through 30000 milliseconds and converts it to seconds (P33, line 1). The table describes payload construction, not complete native semantic validation. The client recursively removes undefined object fields before transmission (P10, byte 4991).

Sky's trusted-service dispatcher permits only own-property function values on its constructed client. Unknown runtime method names are rejected. `setup` returns the target and the available method names; the untrusted-facing proxy creates RPC wrappers from those names. **Confirmed static:** P09, line 1, byte 677; P08, line 1, bytes 683–2075.

The browser client likewise creates a custom Playwright-style facade and serializes named commands over RPC; `domSnapshot` sends a command, rather than invoking an imported Playwright browser directly. It carries command/payload Zod schemas, and specific capabilities explicitly parse their payload/result. **Confirmed static:** P23, byte 11702 (command `parse`/`toJSON`), bytes 115415 and 123388 (locators and DOM snapshots), byte 155880 (FunctionAgentTransport). The transport itself serializes `toJSON()`; this report does **not** claim every browser command is universally parsed at that one boundary. P24, byte 1357411, provides a concrete server-side payload parse and policy checks for raw CDP.

## Native IPC, lifecycle, deadlines, and errors

`MacComputerUseClient` selects API version **`CodexComputerUseIPC-5`**, defaults to a 120-second operation timeout, and caches transports per version. It dispatches these request types:

| Native request type | Purpose visible in client |
|---|---|
| `ComputerUseIPCAppPolicyRequest` | Resolve app and policy before operation. |
| `ComputerUseIPCAppGetSkyshotRequest` | Obtain app state, screenshot, AX text. |
| `ComputerUseIPCListAppsRequest` | Inventory. |
| `ComputerUseIPCAppPerformActionRequest` | Tagged action described above. |
| `ComputerUseIPCAppStartRequest` | Start an app through the low-level client. |
| `ComputerUseIPCStartAudioRecordingRequest` | Start optional computer-audio capture. |
| `ComputerUseIPCStopAudioRecordingRequest` | Stop optional computer-audio capture. |

**Confirmed static:** P10, line 1, bytes 224–1084. The higher-level macOS client constructed in P26 does not expose `startApp` directly. The initial app-state request and bundled docs describe resolving/launching an app as needed; launch behavior itself belongs to native analysis.

`MacNativePipeTransport` requires `nodeRepl.nativePipe.createConnection`. Its default socket is `~/Library/Group Containers/2DC432GLL2.com.openai.sky.CUAService/IPC/computeruse.sock`, overridable with `SKY_CUA_SERVICE_NATIVE_PIPE_PATH`. Connection flow:

1. Try the native socket within 250 ms.
2. On unavailable connection, ask the host-services pipe for JSON-RPC `ensureService` with `{service:"computer-use"}` when `NODE_REPL_HOST_SERVICES_PIPE_PATH` exists. Connect and ensure each have a five-second bound.
3. Otherwise request `nodeRepl.launchServices.openApplication`: explicit `SKY_CUA_SERVICE_PATH`, then an existing `CODEX_HOME/computer-use/Codex Computer Use.app`, then bundle ID `com.openai.sky.CUAService`.
4. Connect/ping within five seconds, with 100-ms retry intervals and at most one-second version pings. API mismatch is terminal rather than retried as ordinary unavailability.

**Confirmed static:** P11, line 1, bytes 425–3711. These are supported branches; this source does not prove which branch was selected in production. In particular, the Electron report must determine whether a host-services pipe is provisioned for the installed build.

Wire frames contain a **4-byte little-endian UTF-8 byte length followed by JSON**, capped at **8,388,608 bytes**. Partial/multiple frames are retained/decoded; oversized frames fail. JSON-RPC 2.0 uses a numeric `id` and method `ping` or `request`. `ping` includes `clientApiVersion`; the response must contain the same `serverApiVersion`. **Confirmed static:** P11, bytes 403, 4561–4849, 4850–5456.

Each operation's `request` parameters contain `clientApiVersion`, `requestType`, `request`, `deadlineUnixMilliseconds`, and optional `codexTurnMetadata`. The metadata derives from `x-codex-turn-metadata` when not explicitly supplied. Operations are serialized through a promise queue. IDs increase from 1; a pending map routes replies and owns timeout handles. A response must have `jsonrpc:"2.0"`, a numeric ID, and exactly one of result/error; errors require numeric code/string message. **Confirmed static:** P10, bytes 2864–3911; P11, bytes 4203–4849, 5457–6210, 6795 onward.

Timeout removes the pending entry, so late replies are ignored. Socket error/close rejects pending requests and invalidates the transport. A later operation may obtain a fresh transport; the failed action is not automatically replayed by this client. **Confirmed static:** P11, bytes 6795–7957; P10 request/getTransport control flow. **Unknown:** whether a timed-out native operation was applied before its reply was lost. Jarvis must preserve that uncertainty rather than repeating a destructive action blindly.

Named server errors include authentication/bootstrap failures, unavailable/ambiguous app, AX failures, permission pending/denied, no active session, user stop/intervention, blocked URL, incompatible client version, and `screenLocked`:

| Error | Code |
|---|---:|
| `senderProcessNotAuthenticated` | -10000 |
| `accessibilityError` | -10008 |
| `permissionsNotGranted` / `permissionsPending` | -10009 / -10014 |
| `noActiveSession` | -10011 |
| `userStoppedSession` / `userIntervened` | -10012 / -10016 |
| `incompatibleClientVersion` | -10013 |
| `blockedURL` | -10015 |
| `couldNotGetSenderPID` / `couldNotGetBootstrapPort` | -10017 / -10019 |
| `ambiguousApp` | -10018 |
| `screenLocked` | -10020 |

**Confirmed static:** P15, line 1. Declaring an error identifies a supported failure class; it does not establish when its native checks run or how a locked session behaves.

## App authorization and action confirmation are different boundaries

The macOS wrapper `withComputerUsePolicy` runs for app observation as well as app actions:

1. Copy input using own property descriptors, require a plain non-empty `app` property, reject accessor properties, and freeze the copy.
2. Call native `getAppPolicy` and inspect its decision. `denied` reports organization policy; `forbidden` reports safety policy; only `allowed` continues.
3. Ask `nodeRepl.createElicitation` for app access. Metadata includes connector ID `computer-use`, target bundle ID/display name, risk level, optional warning subtitle, tool name/call ID, and allowed persistence `session`, plus `always` only when the native policy allows it.
4. Require `action:"accept"` and replace the caller's app selector with the approved target's canonical `appPath` before executing.
5. Execute with the Node REPL timeout suspended around the approved operation and record terminal status.

**Confirmed static:** P12, line 1, bytes 408–2371, 2937 onward. Native server decisions and host persisted-grant processing remain independent enforcement layers. Elicitation is invoked on each wrapped operation; host persistence can satisfy it. This code does not prove a human sees a new prompt on every call. A persisted response is specially tagged `computer-use-persisted-state` and avoids counting a fresh approval interaction.

The descriptor copy and approved-path rebinding protect against changing an app selector while policy/approval is awaiting. **Inference, supported by control flow:** this is a TOCTOU mitigation; P12, byte 3900 onward. Its guarantees do not extend to arbitrary mutable nested values or server-side path identity without further evidence.

Separately, core documentation appends a **confirmation policy for consequential actions**: user handoff for certain safety/password steps, action-time confirmations for deletions/communication/financial actions, and narrowly scoped preapproval for uploads/logins/data transmission. It treats third-party text as untrusted, and typing sensitive values as transmission. **Documented contract:** P19, lines 9–21, 23–78. These are agent-facing instructions, not evidence that the macOS click handler understands every such action's semantic meaning.

The confirmation prose can be supplied by trusted request metadata at `openai/confirmation_policies.computer_use`, restricted to a non-empty string at most 12,000 UTF-8 bytes; otherwise the shipped confirmation file is used. `orbit` and `training` environments have different omission behavior. **Confirmed static:** P20, line 1. The current request's contract therefore cannot safely be inferred only from the fallback document.

## Browser service authorization and credential protection

Browser setup returns an API manifest, disabled-member IDs, and typed credential-recovery support. Method dispatch accepts only own-property functions of a fixed service object (`setup`, `execute`, `executeWithRecovery`); the service requires privileged Node REPL capabilities. **Confirmed static:** P23, byte 158915; P24, line 8871, bytes 1912873–1915180.

An additional local-browser gate is implemented by `TL`/`Gte`:

- If `BROWSER_USE_ACCESS_EXECUTOR_ID` is configured, require current session/turn IDs and cache the verification per pair.
- Send executor/session/turn IDs and a 32-byte random nonce via `BROWSER_USE_ACCESS_PIPE_PATH`, command `browser/access`.
- Load `BROWSER_USE_ACCESS_VERIFYING_IDENTITY` as a DER SPKI public key, verify the signature, and require the exact payload `{domain:"codex.browserAccess.v1",request,allowed:true}`.
- Reject unauthenticated responses or disabled local-browser settings. The pipe is closed after verification, with a ten-second cleanup bound.

**Confirmed static:** P24, line 5430, byte 597283 (constants), line 8846, bytes 1783727–1784820 (verification); `executeAgentCommand` calls `assertBrowserAccess` before dispatch at byte 1751706 onward. **Boundary limit:** when no executor ID is configured, this particular gate returns without verification; this is a conditional architecture, not unconditional global authorization.

The browser service separately tracks native credential use and gates observation before and after commands through `assertObservationAllowed` and `assertRetainedObservationAllowed`. Raw CDP is refused while credential saving is protected, requests are intercepted, or owned paused document responses are resolving; it also checks full-CDP and URL policy. **Confirmed static:** P24, line 8846, byte 1717185 (`P_`); line 5687, byte 1357411 onward. This establishes security checks in shipped code, not a complete proof that every observation path is free of credential leaks.

## Privacy, telemetry, helper identity, and lock state

The app-facing observation outputs include screenshot pixels, accessibility/UI text and, optionally, app-specific instructions. Discovery can include app-usage metadata. Model-facing tool results therefore contain user-interface data. **Confirmed static:** P06–P07, P14, P28. Which model/server receives it, retention, redaction completeness, and the active policy/settings are outside this sub-investigation.

The Sky telemetry module records app bundle ID, tool name, duration, terminal status, approval result/persistence, and optional thread/turn/item/model/reasoning metadata. It initializes Statsig against `ab.chatgpt.com/v1` and logs via `chatgpt.com/ces/v1/rgstr`; it can retrieve account identity/email via `nodeRepl.fetch("https://chatgpt.com/backend-api/me")`. `NODE_REPL_DISABLE_ANALYTICS=1` or `BROWSER_USE_DISABLE_AMBIENT_NETWORK=1` disables this module. **Confirmed static:** P25, line 1. No screenshot or AX payload is assembled by this telemetry module; that limited negative finding must not be generalized to the whole product or its model-facing output path.

Computer-audio methods are exposed only when `SKY_ENABLE_AUDIO=1`. Starting requires a separate high-risk session-only audio elicitation, validates an integer recording duration 100–300000 ms (default 60000), and stopping accepts only a local file URL, returning WAV bytes/data URL. **Confirmed static:** P26, line 1; P12, byte 2372; P29, line 1.

The main helper plist identifies `com.openai.sky.CUAService`, executable `SkyComputerUseService`, display name `ChatGPT Computer Use`, version `26.1005.1001425` / build `1001425`, `LSUIElement:true`, minimum macOS `14.4`. It declares Contacts, Calendar and AppleEvents usage descriptions. **Confirmed static declarations:** P30, plist keys `/CFBundleIdentifier`, `/CFBundleExecutable`, `/CFBundleDisplayName`, `/CFBundleShortVersionString`, `/CFBundleVersion`, `/LSUIElement`, `/LSMinimumSystemVersion`, `/NSContactsUsageDescription`, `/NSCalendarsFullAccessUsageDescription`, `/NSAppleEventsUsageDescription`. Usage descriptions alone do not prove those frameworks are invoked by this feature.

Companion plists identify a guardian, a CLI helper, and an authorization-plugin installer/plugin:

| Absolute installed plist | Identifier / static identity | Bytes; SHA-256 |
|---|---|---|
| `/Applications/ChatGPT.app/Contents/Resources/cua_node/lib/node_modules/@oai/sky/Codex Computer Use.app/Contents/SharedSupport/CUALockScreenGuardian.app/Contents/Info.plist` | `/CFBundleIdentifier`: `com.openai.sky.CUAService.guardian`; minimum macOS 14.4; LSUIElement true | 1616; `baec502a69aab62e732941c0be436862eddee11fd31f030465944f2e25a536ff` |
| `/Applications/ChatGPT.app/Contents/Resources/cua_node/lib/node_modules/@oai/sky/Codex Computer Use.app/Contents/SharedSupport/SkyComputerUseClient.app/Contents/Info.plist` | `/CFBundleIdentifier`: `com.openai.sky.CUAService.cli`; minimum macOS 14.4; LSUIElement true | 1493; `02383c5956e49cddb9cbddb1f83e01b1f37769337a9fa94779cf7a3502631906` |
| `/Applications/ChatGPT.app/Contents/Resources/cua_node/lib/node_modules/@oai/sky/Codex Computer Use.app/Contents/SharedSupport/Codex Computer Use Installer.app/Contents/Info.plist` | `/CFBundleIdentifier`: `com.openai.sky.CUAService.AuthorizationPluginInstaller` | 1668; `9bdf439aa5d49fea653b1942295ddcb183f1019a5796f6f47b3a71743269f738` |
| `/Applications/ChatGPT.app/Contents/Resources/cua_node/lib/node_modules/@oai/sky/Codex Computer Use.app/Contents/SharedSupport/Codex Computer Use Installer.app/Contents/Resources/CodexComputerUseAuthorizationPlugin.bundle/Contents/Info.plist` | `/CFBundleIdentifier`: `com.openai.sky.CUAService.AuthorizationPlugin` | 1489; `ab99618e17c669e05cdcb2aecfef71986b6d72c20512c662bf6e46108b1d01ea` |

**Unknown here:** lock-screen authorization flow, installed plugin state, whether the guardian locks or continues a session, and the native input behavior when the screen locks. `screenLocked=-10020` and the guardian's presence are evidence of handling a lock condition, not evidence that an agent can bypass a lock screen.

## Reconstruction implications for Jarvis

These are **design implications from the evidence**, not claims about existing Jarvis implementation:

1. Preserve separate observation/action APIs and target bindings. Prefer AX element identities and fresh state; retain screenshot coordinate fallback and a browser DOM/locator path.
2. Put native execution behind a Rust-owned local service/command boundary. Reuse the existing Jarvis harness for tools, approvals, recovery and events; do not copy shipped code or identifiers.
3. Treat app policy/OS permissions, app grants, per-action confirmation, and browser/credential policy as separate decisions. Canonicalize the approved target and rebind execution to it.
4. Maintain a typed versioned request contract, frame limits, operation IDs, explicit deadlines, serial app actions and structured cancellation/intervention errors. Preserve uncertain results after lost replies; do not automatically replay an action.
5. Shape the exposed tool catalog by the enabled surface and backend capabilities. Emit just the required documentation and retain it for context replay.
6. Keep the first implementation app/window-scoped. Full-desktop interaction, protected credential handling and lock-screen integration require additional native evidence and separate design; generic docs and strings do not establish parity.

The repository's required Codex/OpenCode/OMP reference comparison is handled by the main investigation; this report did not inspect those reference trees or implement Jarvis changes.

## Question coverage

| Question | Static status |
|---|---|
| Tool entry point, enabled surfaces, globals, app/tab bindings | Answered in shipped JS. |
| Persistent REPL reset contract and documentation replay | Answered as contract; evaluator/isolation internals unresolved. |
| Native method names, action schemas, validations, request encoding | Answered for JS client; native semantic validation separate. |
| Framing, IDs, version ping, request sequencing, deadlines, error cleanup | Answered in native-pipe client. |
| App grants, policy decision, canonical target, accessor rejection | Answered in policy wrapper; server decision/persistence behavior separate. |
| AX versus screenshot versus DOM/locator perception | Answered at adapters; native capture/geometry implementation separate. |
| Per-app versus full-desktop targeting | Answered for macOS exposed JS; native multiwindow behavior separate. |
| Browser turn authorization and credential observation checks | Specific code paths answered; complete bypass analysis unresolved. |
| Telemetry payload and disabled-analytics branches | Answered for Sky telemetry module; whole-product privacy unresolved. |
| Locked-computer behavior | Partially answered: error vocabulary and helper declarations only. |
| Real runtime behavior, permissions, model choice, capture quality | Not observed. |

## Source registry

All paths are absolute installed paths. The REA inventory match for P01–P33 was verified at inspection time. Full-source SHA-256 permits reproducing each claim without relying on package names/version strings alone.

| ID | Absolute installed file | Bytes | SHA-256 |
|---|---|---:|---|
| P01 | `/Applications/ChatGPT.app/Contents/Resources/cua_node/lib/node_modules/@oai/cua-repl/bin/cua-repl.mjs` | 284 | `992174a5e637645aeb444adfdb1bae688e997bb84d7db07532f68e358e60f278` |
| P02 | `/Applications/ChatGPT.app/Contents/Resources/cua_node/lib/node_modules/@oai/cua-repl/dist/lib/js/oai_js_cua_repl/src/launch.js` | 2037 | `3e52a41f4da41d35be8cc3723fe97251711087a998c1ed7f2cfdf7e20b50fc2b` |
| P03 | `/Applications/ChatGPT.app/Contents/Resources/cua_node/lib/node_modules/@oai/cua-repl/instructions/banner.js` | 37 | `db4a56c4a1cb6f9ec2ca663694fbe8941846aaa83d0bf12d3a5699f92126105a` |
| P04 | `/Applications/ChatGPT.app/Contents/Resources/cua_node/lib/node_modules/@oai/cua/dist/lib/js/oai_js_cua/src/tinysky_alt/globals.js` | 583 | `18b8039f846a6e00206128530c5f59ff05bd361c655a858a4a27059ec0f63d23` |
| P05 | `/Applications/ChatGPT.app/Contents/Resources/cua_node/lib/node_modules/@oai/cua/dist/lib/js/oai_js_cua/src/tinysky_alt/create_tinysky_alt.js` | 3182 | `dbe5670314df83184341e58ce0a8e1555537930613cde772905ce18c31744200` |
| P06 | `/Applications/ChatGPT.app/Contents/Resources/cua_node/lib/node_modules/@oai/cua/dist/lib/js/oai_js_cua/src/tinysky_alt/bind_mac_app.js` | 2437 | `bdca688b5fd3e71b76227bec572efd17d5618f52cda46a354d4f8a33a760f189` |
| P07 | `/Applications/ChatGPT.app/Contents/Resources/cua_node/lib/node_modules/@oai/cua/dist/lib/js/oai_js_cua/src/tinysky_alt/bind_tab.js` | 2309 | `a693e078ecbaa34020cf0caa373edb9c525dad87fd3ada62706a07eadaa6dc2a` |
| P08 | `/Applications/ChatGPT.app/Contents/Resources/cua_node/lib/node_modules/@oai/sky/dist/project/cua/sky_js/src/sky.js` | 2077 | `0123da875a2eef5648fac407fcfedad5147ef9a1616b623ad615f4c337aee285` |
| P09 | `/Applications/ChatGPT.app/Contents/Resources/cua_node/lib/node_modules/@oai/sky/dist/project/cua/sky_js/src/service.js` | 1763 | `5b2fa2f0e19c8401a9be8c71be17008d88049ab107972c1e4a5cb615f4ef5993` |
| P10 | `/Applications/ChatGPT.app/Contents/Resources/cua_node/lib/node_modules/@oai/sky/dist/project/cua/sky_js/src/targets/mac/client.js` | 5207 | `ae51951e7087203d473127ccee7725fc61528682f6866d60595a963ef6da56c9` |
| P11 | `/Applications/ChatGPT.app/Contents/Resources/cua_node/lib/node_modules/@oai/sky/dist/project/cua/sky_js/src/targets/mac/native-pipe.js` | 8213 | `2f1bcc0b4cd013ef3d267f6d93bd2aa29770712687c8db087d55ec9d173380e8` |
| P12 | `/Applications/ChatGPT.app/Contents/Resources/cua_node/lib/node_modules/@oai/sky/dist/project/cua/sky_js/src/targets/mac/computer-use-policy.js` | 4886 | `8cd1272d7aa836479be6c3f4cc5b0b80526339eedd4852cbb6558f893f6721f8` |
| P13 | `/Applications/ChatGPT.app/Contents/Resources/cua_node/lib/node_modules/@oai/sky/dist/project/cua/sky_js/src/targets/mac/get_app_state.js` | 506 | `7b7fe2af461c7cd348c07535b1deae9ea14837dbdac9985ba479eb2e53e5248d` |
| P14 | `/Applications/ChatGPT.app/Contents/Resources/cua_node/lib/node_modules/@oai/sky/dist/project/cua/sky_js/src/targets/mac/window_result.js` | 1408 | `9169daf53e7516eecd2ba8680e990bc6570d16d955f0ffed183572202c5d5448` |
| P15 | `/Applications/ChatGPT.app/Contents/Resources/cua_node/lib/node_modules/@oai/sky/dist/project/cua/sky_js/src/targets/mac/errors.js` | 1049 | `9dd27e4cf0ea4456b3ad472fab8cbbe093ad1184d0cb25a8c05e738ee00cfd43` |
| P16 | `/Applications/ChatGPT.app/Contents/Resources/cua_node/lib/node_modules/@oai/cua/dist/lib/js/oai_js_cua/src/core/screenshot_bytes.js` | 394 | `1898dbdcc479016a588ca1fcc6e58f0d62074266f4a7f80530603491fd483071` |
| P17 | `/Applications/ChatGPT.app/Contents/Resources/cua_node/lib/node_modules/@oai/cua/dist/lib/js/oai_js_cua/src/get_state.js` | 759 | `d6a44bdff38413cdcaea83c791267ad0bed0e1913bdc60a8c227b3c12d3cc372` |
| P18 | `/Applications/ChatGPT.app/Contents/Resources/cua_node/lib/node_modules/@oai/cua/docs/tinysky-alt-core-cua-repl.md` | 15462 | `bb21d6158c43d27d9c1e9380664c300d8f1f747dc763494c290d11974de9a8ac` |
| P19 | `/Applications/ChatGPT.app/Contents/Resources/cua_node/lib/node_modules/@oai/cua/docs/tinysky-alt-confirmations.md` | 5607 | `68474884543a6a1e72275b59f27f383313fd6aa8604ccedabb5d5cd37077e6fc` |
| P20 | `/Applications/ChatGPT.app/Contents/Resources/cua_node/lib/node_modules/@oai/cua/dist/lib/js/oai_js_cua/src/tinysky_alt/create_documentation.js` | 1042 | `25dca856fdf773ee1a54ba2772ac025a164b06fa93b5879705b8c6adc137eea3` |
| P21 | `/Applications/ChatGPT.app/Contents/Resources/cua_node/lib/node_modules/@oai/cua/dist/lib/js/oai_js_cua/src/tinysky_alt/create_bootstrap_emitter.js` | 1602 | `9628b40a5069d4bca0083d712a30f28a84199eadedcf7b394a32db4ed26fbbe7` |
| P22 | `/Applications/ChatGPT.app/Contents/Resources/cua_node/lib/node_modules/@oai/cua/dist/lib/js/oai_js_cua/src/tinysky_alt/setup_browser.js` | 540 | `a1cf70b2ec89d51f9bc33b056dbe31af9cc6fd605c365e93f5d0649d12748216` |
| P23 | `/Applications/ChatGPT.app/Contents/Resources/cua_node/lib/node_modules/@oai/cua/dist/lib/js/oai_js_browser/dist/skill/scripts/browser-client.js` | 160575 | `0420baac078ed7c1544659618436c3c0e8f766eaccd9f521684ea5115a9608e3` |
| P24 | `/Applications/ChatGPT.app/Contents/Resources/cua_node/lib/node_modules/@oai/browser-desktop/scripts/browser-service.mjs` | 1915181 | `8c83e93bdca10cdffa9f7af80db7775ddf1d993186dbf4b8b3d32d11e9bd9666` |
| P25 | `/Applications/ChatGPT.app/Contents/Resources/cua_node/lib/node_modules/@oai/sky/dist/project/cua/sky_js/src/targets/mac/computer-use-telemetry.js` | 4435 | `3645bebe7ff8badca86e57367e7961e1b4ac1fb0e743c65b3e1d3e308847208e` |
| P26 | `/Applications/ChatGPT.app/Contents/Resources/cua_node/lib/node_modules/@oai/sky/dist/project/cua/sky_js/src/targets/mac/create_client.js` | 1119 | `901b7f850b7b4ceaecefe7727a7a4aecd3c22a05fb807ffed5cfcca000906217` |
| P27 | `/Applications/ChatGPT.app/Contents/Resources/cua_node/lib/node_modules/@oai/sky/docs/sky-window-api.md` | 6471 | `18d387e9c5d2a131fb697dce038ccd58e78059eb406d010db171e987de1cc6f5` |
| P28 | `/Applications/ChatGPT.app/Contents/Resources/cua_node/lib/node_modules/@oai/sky/dist/project/cua/sky_js/src/targets/mac/list_apps.js` | 726 | `77ec31ff376b5065b08ca1116484b3e69878c2a4a7ad6dac7f29170b965d91df` |
| P29 | `/Applications/ChatGPT.app/Contents/Resources/cua_node/lib/node_modules/@oai/sky/dist/project/cua/sky_js/src/targets/mac/audio_recording.js` | 1659 | `439d5adfb320845fab760caa449019fc9c48115b07593fb6f160be7d0ed328be` |
| P30 | `/Applications/ChatGPT.app/Contents/Resources/cua_node/lib/node_modules/@oai/sky/Codex Computer Use.app/Contents/Info.plist` | 2475 | `eb2a09c19bda0f50f6d4ee721380bc2a5860003630662b391d966826e2605ec8` |
| P31 | `/Applications/ChatGPT.app/Contents/Resources/cua_node/lib/node_modules/@oai/cua-repl/instructions/reset.md` | 240 | `8293a9ec45999391940bdfce1637fb5c4d708f89b5d726b4d03800df9cb795af` |
| P32 | `/Applications/ChatGPT.app/Contents/Resources/cua_node/lib/node_modules/@oai/cua-repl/instructions/macos/description.md` | 856 | `096e5cea5931c9c23adc839211bfe3764039a2e68dd47b627ae0d8bc22fdaed6` |
| P33 | `/Applications/ChatGPT.app/Contents/Resources/cua_node/lib/node_modules/@oai/cua/dist/lib/js/oai_js_cua/src/tinysky_alt/duration_seconds.js` | 181 | `6d38d0a24c934f1141cf35236f1f423b398da2521d2c94b12387fe54f60df6c9` |

The duplicated `client.js`, `native-pipe.js`, `computer-use-policy.js`, and `sky.js` inside `@oai/cua/dist/project/cua/sky_js/src` are byte-identical to the corresponding `@oai/sky` sources cited above. The embedded CUA browser client and standalone `@oai/browser-desktop` service are separate bundles, so their offsets/hashes must not be mixed.

No runtime, UAT, screenshots, model calls, or code-quality tests were performed. Remaining risks are native geometry/permission/lock semantics, evaluator isolation, and complete credential/privacy coverage; the static findings do not certify implementation parity.
