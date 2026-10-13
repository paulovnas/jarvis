# Computer Use: packaged Electron integration

Date: 2026-10-12. Target: `/Applications/ChatGPT.app`. This report examines the installed package, not a live Computer Use session.

## Evidence boundary

The application at the supplied path contains an Electron/Codex package. The previous research directory was used to locate likely members; all identities and code findings below were checked against the current installed ASAR. No application code was imported or executed. No user settings, credentials, approval files, screenshots, or application state were read. No application was launched.

The package/extraction context is retained by REA:

- Inventory: `ev_70fee48e02cb55a5faa2a9e960b27927c5bb0feb24ba9dcc76f318a81292ee00`.
- Extraction: `ev_4f9dd575e0e57bcd7ec5ad9740463d861ad0a8d8dd81a797364557726a294cd3`.
- REA app graph SHA-256: `11987fdc3b01ea720a9351d433fcf738587e26ec3beb8ccb513c21cc99a9bb73`.

The focused source inspection here supplements those REA records. `JS-CU-*` identifiers below are local finding identifiers, not invented REA Evidence IDs. They bind to the original member digests and byte positions in the next table. “Confirmed” means the packaged control flow or metadata was inspected. It never means execution occurred.

Original carrier: `/Applications/ChatGPT.app/Contents/Resources/app.asar`, 567,757,587 bytes, SHA-256 `75e397e2280dde93b70a85aa926f4282c728eacc84dfeb15ba7fb6ddd84628af`. Its JSON header describes 21,089 members; the file-data base is byte 5,857,252. All positions below are zero-based UTF-8 member byte positions. Add the member's absolute ASAR offset to obtain an original-carrier position. Ranges are half-open inspection windows, not decompiler virtual addresses.

Parsed `package.json`: `openai-codex-electron`, product `Codex`, version `26.1007.30912`, entry `.vite/build/early-bootstrap.js`, flavor `prod`. Its 5,754 bytes start at absolute ASAR byte 25,842,464 and hash to `97e6c90d6760d306852720fcdbc4a2946e54bc406406e1d03711a194955d1e2e`. The installed directory name and internal product metadata differ; this report follows the supplied installed target.

| Alias | Original ASAR member | Bytes | Absolute ASAR offset | SHA-256 |
| --- | --- | ---: | ---: | --- |
| Main | `.vite/build/main-CqEoQDOU.js` | 3,824,411 | 7,673,309 | `09d19386e1f23cf4ccbc950fc40d85a1e8a064a08be588b2a4be6a89f6ada9e6` |
| Shared | `.vite/build/src-CP7XXlpZ.js` | 203,781 | 13,204,354 | `79623d50989f9efbcb8e5752c56b3cc608a0f5993c05112e03e11b718ac7a586` |
| Network | `.vite/build/application-network-startup-CfAQmSYg.js` | 290,566 | 5,860,269 | `cd6e8a8fbc4354a6e6ff19c6e4bf6fc1fb3960c63708c83871357c6238f516a8` |
| Worker | `.vite/build/worker.js` | 2,381,605 | 13,527,515 | `df97ab945ce9850480ccdd0317b2d194febcc580c81ab25adfcfa6d265a00ae1` |
| Renderer shared | `webview/assets/app-shared-8bd9d0399038.js` | 8,175,575 | 68,915,869 | `607ad0866dd25ed3764711b4467a9b60708353a599485b5249ff4cbe0e870245` |
| Approval card | `webview/assets/computer-use-app-approval-request-card-89cbdba76563.js` | 4,237 | 135,494,862 | `6404db4603cfe7556dc2dc8641474234fe9941fb8700a4f928fb5bd75bf6764e` |
| Approval queries | `webview/assets/computer-use-app-approvals-query-9907ea92c222.js` | 2,228 | 135,499,099 | `e7dda48b32f554e47cbc71edb3536ac59eae15df7d64576034da675b25a900ba` |
| App metadata | `webview/assets/computer-use-native-app-metadata-db64f29da0ce.js` | 879 | 135,501,327 | `7b322dad7d6c7a824226b0c11beff734fe1e92bb865a2f097d1e1ba79b927e58` |
| Settings | `webview/assets/computer-use-settings-672bfa843f15.js` | 71,775 | 135,523,297 | `55ae3df4d8907f8ec32825deb9a5eeedd4cdf3188b2c96494a28ae3dbb3421bc` |
| Locked-use gate | `webview/assets/use-can-use-locked-computer-use-38193b1b99be.js` | 658 | 529,594,945 | `6f4dfc4b05d84f2072a794032e293204a47ae8850188b90d26c5bd4d14777b19` |

Names and byte positions identify this installed build only.

## JS-CU-01 — availability has several independent gates

**Confirmed static.** Main's desktop feature state defaults `computerUse`, `browserUseTinysky`, `cuaPIP`, and `deferCuaTools` to false. `K` reads that state; `ji` updates it after validation elsewhere. `Mi` adds a Windows environment override and allows a validated JSON feature override in the development flavor. This is capability/configuration plumbing, not proof that the user's account enables any feature.

Sources: Main `[67600,71400)`, `K` at 69,545; `Mi` at 70,323.

The renderer's `pKr` combines a Statsig gate (`1506311413`), the app-server `computer_use` feature, compatible macOS/Windows platform, and requirements that allow browser/computer use. `nKr` reads `requirements.allowBrowserAndComputerUse`; a pending or absent requirements response does not immediately become available. On Windows it also checks account settings including `beta_settings.windows_computer_use`. `hKr` exposes concrete unavailable reasons: loading, configuration requirement disabled, unsupported platform, Statsig disabled, disabled window type, or explicitly disabled.

Sources: Renderer shared `[5351200,5358723)`, `uKr` at 5,354,838; `fKr` at 5,355,406; `pKr` at 5,355,456; `hKr` at 5,357,892.

**Inference.** A Jarvis equivalent needs an explicit capability/availability state. Finding binaries or installing a plugin alone does not establish authorization or readiness.

**Unknown.** The effective account gates, remotely supplied requirements, actual installed/enabled plugin state, and model selection in the current user session were not observed.

## JS-CU-02 — unified Computer Use is a selected local MCP surface

**Confirmed static.** The loose resource `Contents/Resources/plugins/openai-bundled/plugins/unified-computer-use/.mcp.json` ships `cua_repl` disabled, with `js`, `js_reset`, and `turn_ended`; it initially omits tools from code mode and deferred exposure. The resource manifest is a default, not the final installed configuration.

Original loose resource: 349 bytes, SHA-256 `e7297a8851d46bb5d7f53d57e82d86cfdf4b04524bedf8c2ce6cf4a80ffe71da`.

Main's `ks` and `ire` select the actual surfaces. The unified route requires Node and the REPL executable, no WSL-path route, an installed/enabled/available `unified-computer-use` plugin, the Tinysky feature, and app-server support for `mcpToolExposure`. The `computer` surface further requires macOS, the Computer Use feature, an enabled/available computer-use plugin, and a resolved service-app path. Browser surfaces are selected separately: `chrome`, `iab`, and optionally `mcpapps`.

Sources: Main `ks` at 156,536, `ire` at 158,208; inspected `[156536,158820)`. `Pa` at 99,832 and `Fa` at 100,048 inspect the marketplace plugin and service path.

`rne` rewrites the installed plugin's MCP JSON using a temporary file and rename. Its command becomes the selected Node binary; its argument is `@oai/cua-repl/bin/cua-repl.mjs`. The original native REPL executable is passed as `CUA_REPL_NODE_REPL_PATH`; `CUA_REPL_ENABLED_SURFACES` enumerates the chosen surfaces. Trusted service mappings point browser to `@oai/browser-desktop/service` and computer to `@oai/sky/service`. `deferCuaTools` changes the omission list.

Sources: Main `[98200,102400)`, `rne` at 100,642.

Small source excerpts: Main byte 101,213 assigns `c.enabled=n`; byte 157,456 gates the computer surface with ``h&&_.push(`computer`)``. The surrounding functions above establish what `n` and `h` mean; neither expression alone proves feature activation.

`hc` waits for local marketplace selection, synthesizes the old `node_repl` configuration, rewrites the unified plugin, and sends `config/batchWrite` to the app server. `tre` removes standalone browser/computer skill directories when the unified surface handles them; otherwise it can restore the corresponding shipped skill with version checks and staged replacement. `La` empties legacy browser/computer instruction banners for selected unified surfaces. `_c` creates thread-specific overrides using that synchronized selection.

Sources: Main `[155028,156536)`, `tre` at 155,028; `[177608,179750)`, `hc` at 177,608, `_c` at 179,263, and `La` at 100,535.

**Inference.** Two catalog paths coexist for migration/compatibility: unified `cua_repl` and the older Node-REPL/standalone-skill path. The product actively avoids exposing redundant guidance for the selected path.

**Unknown.** Whether this installed production session took either path. Windows helper support elsewhere in the package does not prove that this build's unified `computer` surface is enabled on Windows.

## JS-CU-03 — native REPL and trusted host services are separate boundaries

**Confirmed static.** `aa` chooses runtime paths, accepting explicit environment overrides or resolving bundled executables. Network's `Gt`/`Qt`/`rn` resolve `Contents/Resources/cua_node/bin/node_repl`; `Kt` resolves the sibling `node`, and `qt` supplies the runtime's module directory. They validate that the candidates are executable files. The macOS CLI resolver checks `codex-cli/CodexCLI.app/Contents/MacOS/codex`.

Sources: Main `aa` at 90,934, `[90934,94930)`; Network `[43400,50200)`.

Shared's `E` constructs an MCP entry with the native REPL executable as the command, no arguments, and a 120-second startup timeout. It sets the selected Node executable, module directories, trusted code paths, Codex home, and a native-pipe connection timeout of 1,000 ms. It can pass the Codex CLI path on macOS, Linux, and Windows.

Sources: Shared `[6430,7630)`, `E` at 6,749.

Main's `ic`/`ac` attach selected browser backends, trusted services, the computer-use instruction banner, optional model-check enforcement, the Sky service-app path, and optionally a host-services socket. Audio is enabled only when both related environment switches are set. Sensitive process-derived details are omitted from the persistent shared configuration and can be introduced into per-thread configuration.

Sources: Main `ic` at 173,106, `ac` at 174,455; `[173106,177200)`.

The optional Electron host-services socket is narrowly scoped: JSON-RPC `ensureService`, one service registry entry for `computer-use`, 8-MiB incoming/outgoing frame bounds, socket mode 0600, and a peer-authorizer. Invalid or unsupported service requests produce structured errors. On macOS `cp` can load `native/browser-use-peer-authorization.node` and authorize the socket file descriptor; packaged builds with missing build flavor fail closed. Whether the native authorization validates a particular signer must be established in the native analysis.

Sources: Main `jse` at 360,411, `Mse` at 361,182, `[359600,362170)`; `cp` at 304,742, `[304550,305900)`.

The build-flavor helper includes production among macOS peer-authorization flavors. That helper is `.vite/build/build-flavor-CYXpoCFI.js`, 3,853 bytes, absolute ASAR offset 7,284,192, SHA-256 `23229dac961a82c52237acc32598ddb2c1cf370b24c64fc507b9c24bcf2c4d93`; inspect `[650,780)` and `[2750,2870)`.

**Important limit.** Main creates that host-services pipe only in an internal-build branch. The current package metadata is production; the optional internal route must not be presented as the production route actually used. Production can rely on the native REPL/service's own launcher path, analyzed separately.

Sources: Main startup `[3812050,3813000)`; Shared `[46450,46820)` defines internal flavors as development, agent, nightly, or internal-alpha. The effective flavor can be overridden by process environment; no such runtime environment was read.

**Unknown.** Native REPL sandboxing and the actual trust/permission enforcement require analysis of `cua_node/bin/node_repl` and the macOS service. The environment contract alone does not prove a sandbox implementation.

## JS-CU-04 — managed macOS service lifecycle is explicit

**Confirmed static.** The bundled service candidate is `cua_node/lib/node_modules/@oai/sky/Codex Computer Use.app`. `Oie` copies it with `/usr/bin/ditto --noqtn` to a canonical `CODEX_HOME/computer-use/Codex Computer Use.app`, after preparing/removing the old destination. The feature can skip this refresh by an explicit environment switch. This is inspected code, not an action performed in this investigation.

Sources: Main `[98200,100048)`, `Da`/`Oa`; `Oie` at 275,184, `[275000,276000)`.

Class `kie` manages `Contents/MacOS/SkyComputerUseService`. Its enabled state is the union of appshots, computer use, and computer history. It serializes operations, validates an existing process is alive and matches the expected executable, preserves an expected live PID during invalidation, and replaces stale or mismatched PIDs. It requires a positive safe integer from the spawn operation, reports first launch versus respawn, and supplies recovery callbacks. Its JavaScript `dispose` clears flags; this function alone does not establish whether the native service is terminated.

Sources: Main `[275900,279150)`, class `kie`; startup `[3812050,3815450)`.

`pd` and `md` bridge to `native/sky.node` methods `spawnComputerUseService` and `computerUseServiceProcessMatchesExecutablePath`. The same addon has application icons, foreground-window information, window dragging, and native hosted PIP methods; these are distinct from the desktop action implementation.

Sources: Main `pd` at 231,176, `md` at 231,425, `[227800,233800)`.

**Inference.** Stable app identity and a managed service allow native macOS permissions and overlays to be owned outside Electron/renderer lifecycle. The native analysis must establish the exact TCC ownership and process isolation.

## JS-CU-05 — app authorization uses MCP elicitation, with scoped persistence

**Confirmed static.** Renderer shared `QDr` recognizes a computer-use app approval from an MCP tool-call elicitation: normalized connector ID must be `computer-use` or its prefix, recognized persistence modes must be nonempty, and an app name must be available. It derives the displayed app from tool-parameter display metadata or the string `tool_params.app`; this name is presentation data, not proven native identity validation.

Source: Renderer shared `[5075350,5076390)`, `QDr`, `$Dr`, `eOr`.

The approval card shows the target app and first-use disclosure about app access and screenshots. It supports conversation approval (`session` when offered), persistent approval (`always` when offered), and decline. A high risk level gets an elevated-risk treatment. It suppresses duplicate response submission while pending and restores the response state if sending fails. Responses go through `replyWithMcpServerElicitationResponse` with the current host, conversation and request ID; accepted persistence is placed in response metadata.

Source: entire Approval card, function `E`. These are existing UI choices, not a runtime approval granted by this investigation.

The settings query layer exposes grant listing/removal. Main `Ghe` delegates to `Yw`/`lhe`; on macOS `Xw` derives the persistent grant file:

`~/Library/Group Containers/2DC432GLL2.com.openai.sky.CUAService/Library/Application Support/Software/ComputerUseAppApprovals.json`

`she` validates an array of approved bundle identifiers; `Zw` deduplicates/normalizes; removal filters the selected identifier and `fhe` writes the new JSON. This report inspected those functions, not that private file. Windows uses TOML instead and has a migration for legacy application grants.

Sources: Approval queries, functions `u`/`T`; Main `[1033710,1037500)`, `Yw` at 1,035,972, `lhe` at 1,036,294, `Xw` at 1,036,411, `fhe` at 1,036,975; `Ghe` in `[1043200,1044400)`.

**Unknown.** Which individual native operations require an app grant, how session approvals expire, and when the native service reloads a revoked grant. That enforcement is outside these Electron/renderer functions.

## JS-CU-06 — app capture has a dedicated asynchronous bridge

**Confirmed static.** Electron's appshot/capture worker uses Apple Events to the managed service rather than renderer screenshot APIs. Worker `Q5`/`bFe` create an event targeting a service PID, attach a request type/client API version/JSON payload, call `AESendMessage`, dispose descriptors, and parse reply data/errors. `t7` retries only an allowlist of Apple Event failures with delays of 50, 100, 200, 400, and 800 ms.

Sources: Worker `Q5` at 2,179,888, `bFe` at 2,180,185, `t7` at 2,181,735; `[2179888,2182280)`.

The request types include app state/skyshot, start capture, and next capture update. `OFe` sends a version-2 request with app, animation target, permission request ID and capture request ID. Its schema distinguishes started from abandoned permission acquisition and represents no permission, Accessibility only, Screen Recording only, or both. Updates are separately typed metadata, accessibility text, screenshot, completed, or failed.

Sources: Worker `[2182200,2185825)`, `DFe` at 2,183,058, `OFe` at 2,183,289, `kFe` at 2,184,454, `AFe` at 2,185,089.

`a7` accepts a local file URL or an absolute path, then canonicalizes both target and expected temporary root, rejects paths escaping that root, requires a regular PNG/JPEG file, and limits it to 26,214,400 bytes. `s7` reads accepted bytes and produces a data URL. No such file was read in this investigation.

Source: Worker `[2185825,2187300)`, `a7` at 2,185,825.

Small source excerpt at Worker byte 2,186,177: ``if(r.startsWith(`..`)||g.default.isAbsolute(r))return null``. Here `r` is the relative path after canonicalizing both the expected root and returned image; it is an explicit escape check.

Main `Wme` rejects empty inputs and duplicate request IDs, shares a pending permission request ID, subscribes to capture updates, and manages an abort controller and 120-second timeout. A structured pending-permission error retries the start path; other errors become failed updates. `Rw` sends updates to the originating view and releases state on terminal events; `zw` clears the timer, aborts, and unsubscribes. `Gme` requests a final capture update and reports failure rather than silently completing.

Sources: Main `[1030045,1033710)`, `Wme` at 1,030,045, `Gme` at 1,031,718, `Rw` at 1,033,071, `zw` at 1,033,418, `Bw` at 1,033,516, `Vw` at 1,033,683.

**Limit.** This proves an appshot/capture integration sharing the native Computer Use service. It must not be conflated with every screenshot issued by the CUA action REPL; that route has separate source and native evidence.

## JS-CU-07 — task activity and overlay configuration follow the conversation

**Confirmed static.** `Use` listens to internal app-server item notifications. It marks activity for the old `computer-use` server or a Node/CUA REPL tool result bearing metadata `codex/toolSurface` with kind `computerUse`. Turn completion clears activity; archive/close/delete clears it too, and deletion removes task state. This gives the Electron UI per-thread Computer Use activity.

Sources: Main `[364200,365800)`, `Use` at 364,592; integration startup `[3809400,3810650)`.

`Iie` listens for `turn/completed`: a completed turn finishes PIP, other terminal statuses invalidate that turn, and it calls the active-turn close operation. The active-turn pipe adapter shown near `Ys` is Windows-only, so its helper close semantics must not be attributed to the macOS transport.

Sources: Main `Iie` at 283,271, `[283271,283810)`; `Ys` in `[170220,172609)`.

`jc` derives native overlay locale, writing direction, accent color, and the messages indicating computer activity and Escape cancellation. `Ic` atomically writes `CODEX_HOME/computer-use/config.json`; native hosted PIP bridge methods in `sky.node` track active thread, suppressed threads and task actions.

Sources: Main `jc` at 180,534 and `Ic` at 181,353, `[179750,182050)`; `[232037,233800)`.

**Unknown.** Those labels/configuration do not prove Escape cancellation, takeover detection, native input suppression or an overlay actually appearing. These require native control-flow or runtime evidence.

## JS-CU-08 — locked use is a separate installation and policy capability

**Confirmed static.** The renderer shows locked-use settings only with its feature/remote-control compatibility checks and a requirements field that does not prohibit `allowLockedComputerUse`. It queries the current install state and calls a mutation for enable/disable.

Sources: entire Locked-use gate; Settings `[63400,65000)` and `[42000,44100)`; Approval queries `v`/`T`.

Main derives a canonical `Codex Computer Use Installer.app/Contents/MacOS/Codex Computer Use Installer` executable inside the computer-use app's SharedSupport. `Bhe` invokes install or uninstall and checks status; `sT` recognizes installed only from the expected status string. There is a 120-second subprocess timeout. This is separate from ordinary in-session computer use.

Sources: Main `[1041700,1042840)`, `Rhe` at 1,042,152, `Bhe` at 1,042,495, `cT` at 1,042,670.

**Unknown.** The installer's system modifications, elevated privileges, lock-screen lifecycle, guardian behavior and recovery mechanism must be established from its native executable. No installer was run.

## Reconstruction implications and remaining work

The inspected integration supports a concrete architectural decomposition for Jarvis: Rust owns runtime/service lifecycle, typed request validation, per-thread state, app-grant enforcement, OS permission acquisition and cancellation; React owns capability status, app approvals, progress and settings. This is a proposed design, not a recovered Jarvis implementation or proof that OpenAI uses identical internals in every release.

Keep three separate concepts: feature/account eligibility, native OS permissions, and the user's app-specific grants. Keep browser backends and native app automation as selected surfaces with shared conversation identity. Retain typed terminal outcomes and explicit cleanup rather than interpreting absent output as success. Validate capture paths and payload size at the native/renderer boundary. Treat locked use as its own capability with a separately reviewable installer.

This source pass answers activation plumbing, catalog selection, launch configuration, UI approvals, persisted-grant schema, managed-service behavior, appshot capture and task state. It leaves action execution, TCC enforcement, trusted/untrusted JS isolation, app-grant enforcement, takeover, cancellation and locked-use internals to the companion CUA/native analyses. Account activation and actual execution remain runtime questions. There were no source implementation changes, tests, application launch or runtime captures.
