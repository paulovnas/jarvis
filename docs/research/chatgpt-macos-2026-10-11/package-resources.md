# ChatGPT macOS: package, runtime, and resource evidence

Static inspection dated 2026-10-11 of the user-selected `/Applications/ChatGPT.app`. This report covers the outer application package. The ASAR application logic is investigated separately. The app was not launched or attached, and no user data, credentials, or network traffic were inspected.

## Identity and evidence boundaries

The installed artifact is named **ChatGPT**, but its package identity is **`com.openai.codex`**. It packages a Chromium/Owl runtime, an Electron-style ASAR application, a native Codex CLI, and a separate computer-use Node runtime. These observations establish this measured package's structure; they do not establish an unmodified stock Electron distribution.

| Evidence | What it establishes |
| --- | --- |
| REA inventory `ev_70fee48e02cb55a5faa2a9e960b27927c5bb0feb24ba9dcc76f318a81292ee00` | Parent investigation's package inventory. Directory-artifact digest: `11987fdc3b01ea720a9351d433fcf738587e26ec3beb8ccb513c21cc99a9bb73`. This is REA's directory identity, not a file SHA-256. |
| REA retained inspection `ev_cdc0982d0431df6af646c81a54d4ce4194f7be270e352c16d307fae80b34a3ee` | Parent's complete package inspection retained despite exceeding the MCP transport frame. Available in the locally retained `rea-evidence.json` beside this report; the versioned [integrity manifest](rea-evidence.manifest.json) identifies that export. |
| REA plist `ev_7f6b556fa07391d8c6510e678eddd4c01fb7e9cad1f6112d5e647faaa08bf3bd` | Parent's parsed main `Info.plist`. Independently corroborated here with Python `plistlib` and original XML line numbers. |
| REA Mach-O `ev_dcb7919ebcb5cbbe936e176c23cb3cd1248cff487fbeb8cc390be1ed75cb47b1` | Parent's native launcher metadata. |
| REA main dossier `ev_f39166a73ce427b4022d5d9de2e32161a8ef42ff625913b67abf4c4426a734a4` | Parent's static `_main` analysis at virtual address `0x100000748`: `dlopen` of the versioned Codex Framework, `dlsym("ChromeMain")`, and invocation through the recovered function pointer. Static control flow is not observed execution. |

These REA IDs were supplied by the parent investigation. This worker did not open or close another REA native target. Explicit-path `inspect_plist` returned `target_unavailable` in this worker's session; the supplementary observations below use read-only host tools, not invented REA Evidence IDs.

All paths below are relative to `/Applications/ChatGPT.app/` unless stated otherwise. All line numbers refer to the original installed files, not extracted or reformatted copies.

## Confirmed runtime structure

| Finding | Original source |
| --- | --- |
| Visible name `ChatGPT`; identifier `com.openai.codex`; version `26.1007.30912`, build `20156`. | `Contents/Info.plist`: lines 22–23, 195–196, 203–204, 220–221. |
| The main executable is an ARM64 Mach-O, 71,008 bytes. Its static dylib load table lists only `/usr/lib/libSystem.B.dylib`. | `Contents/MacOS/ChatGPT`; `/usr/bin/file` and `/usr/bin/otool -L`. |
| The main function dynamically loads `../Frameworks/Codex Framework.framework/Versions/155.0.8059.40/Codex Framework` and resolves `ChromeMain`. | Main dossier `ev_f39166a73ce427b4022d5d9de2e32161a8ef42ff625913b67abf4c4426a734a4`, `_main` at `0x100000748`. |
| The framework exports `_ChromeMain`. `dyld_info -exports` reports `0x00005268`; this report does not treat that display value as an original-file offset. | `Contents/Frameworks/Codex Framework.framework/Versions/155.0.8059.40/Codex Framework`; `/usr/bin/dyld_info -exports`. |
| Chromium version metadata is `155.0.8059.40`; the principal application class is `BrowserCrApplication`. | `Contents/Info.plist`: lines 224–225 and 306–307. |
| The framework declares identifier `com.openai.codex.framework` and version `155.0.8059.40`. | `Contents/Frameworks/Codex Framework.framework/Versions/155.0.8059.40/Resources/Info.plist`: lines 9–10 and 15–16. |
| `runtimeName` is `owl`; a build path contains `codex/codex-apps/electron/out/ChatGPT-darwin-arm64/ChatGPT.app`. | `Contents/Resources/owl-electron-app.json`: lines 2–4. The build path is metadata, not a local source checkout. |
| The resource config declares `UserDataDirectoryName=Codex` and `AppVersion=26.1007.30912`. | `Contents/Resources/owl-app.ini`: lines 1–3. The named user-data directory was not opened. |
| An ASAR application and unpacked native members are shipped. The plist has `ElectronAsarIntegrity` metadata. | `Contents/Resources/app.asar`, `Contents/Resources/app.asar.unpacked/`; `Contents/Info.plist`: lines 242–250. |

The framework's static load table includes AppKit, Foundation, CFNetwork, Network, Metal, CoreVideo, Security, ScreenCaptureKit, AuthenticationServices, and Accessibility. This confirms declared native dependencies, not which APIs a chat uses or whether a GPU path is active. Only two direct framework directories are present: `Codex Framework.framework` and `Sparkle.framework`.

Packaged helper applications include `Codex (Renderer).app`, `Codex (GPU).app`, `Codex (Service).app`, alert helpers, and Aperitif variants under the versioned framework's `Helpers/`. Their plists identify the renderer separately (`com.openai.codex.helper.renderer`). The artifacts support an inference of separated browser-related roles; no live process count or isolation behavior was measured.

A bounded search of exported names did not match twelve selected `cef_*` entry points, including `cef_initialize` and `cef_execute_process`. This does not establish absence of CEF internals. The confirmed classification is a Chromium/Owl shell with Electron-style packaging; the exact compatibility architecture requires application-code or deeper framework analysis.

## Packaged native tools and integrations

| Confirmed package fact | Original source | Limit |
| --- | --- | --- |
| Codex CLI layout version 1, version `0.162.0-alpha.17.2`, target `aarch64-apple-darwin`, entry point `bin/codex`. | `Contents/Resources/codex-cli/codex-package.json`: lines 2–6. | Declared package metadata; the CLI was not executed. |
| The shell wrapper forwards arguments to `../CodexCLI.app/Contents/MacOS/codex`. | `Contents/Resources/codex-cli/bin/codex`: line 10. | Static forwarding logic; does not prove a particular GUI flow launches it. |
| The native CLI image is 246,046,384 bytes. A code-mode host is also packaged. | `Contents/Resources/codex-cli/CodexCLI.app/Contents/MacOS/codex`, `Contents/Resources/codex-cli/bin/codex-code-mode-host`. | No internal CLI functions were decompiled in this resource pass. |
| A separate computer-use runtime declares Node `24.21.0-cua.1`, binary version `24.21.0`, ARM64 macOS, and `bin/node_repl`. | `Contents/Resources/cua_node/manifest.json`: lines 2–7 and 10–12. | Runtime declarations, not launch observations. |
| A voice host and GStreamer/Opus libraries/plugins are packaged. | `Contents/Resources/codex-cli/codex-resources/voice/bin/codex-voice-host`, `voice/lib/`, `voice/plugins/`. | Does not establish the voice transport or active codec selection. |
| Native modules include `sky.node`, `devicecheck.node`, `input-monitoring-permission.node`, `browser-use-peer-authorization.node`, `remote-control-device-key.node`, `usb_webauthn.node`, and audio/HID modules. | `Contents/Resources/native/`. | Filenames establish candidate subsystems only; their behavior is not recovered here. |
| Sparkle is version `2.9.1`; `sparkle.node` has an actual `@rpath/Sparkle.framework/Versions/B/Sparkle` load command. | Framework plist lines 19–20; `Contents/Resources/native/sparkle.node`, `otool -L`. | Stronger than a filename, but no updater invocation was observed. |

`Contents/Resources/default_app/` contains `main.js`, `preload.js`, `index.html`, and `styles.css`, with a `package.json` naming `electron` and `main.js`. This is a separately packaged default application resource. Its existence should not be mistaken for the ChatGPT conversation renderer or used to infer that renderer's CSS/Markdown behavior. The primary ASAR analysis owns those claims.

## Network and OS configuration

| Observation | Original source | What remains unknown |
| --- | --- | --- |
| A proxy-origin resource contains exactly `https://codex-portals.api.openai.com`. | `Contents/Resources/masque-proxy-origins.txt`: line 1. | Consumer code, use conditions, actual requests, backend capabilities, and whether it participates in model inference. |
| Main app ATS config declares `NSAllowsArbitraryLoads=true`. | `Contents/Info.plist`: lines 271–275. | This is an OS policy declaration, not evidence that TLS is disabled, certificates are ignored, or a provider request uses HTTP. Chromium-specific policies need separate analysis. |
| Sparkle's downloader XPC plist declares `NSAllowsArbitraryLoads=false`. | `Contents/Frameworks/Sparkle.framework/Versions/B/XPCServices/Downloader.xpc/Contents/Info.plist`: lines 49–50. | The two plists belong to distinct bundles; they should not be flattened into a single networking-policy claim. |
| URL schemes include `codex`, `http`, and `https`. | `Contents/Info.plist`: lines 212–217. | Registration does not establish every implemented navigation or authentication flow. |
| Browser-authentication capability metadata declares supported, ephemeral-session, and callback-matching support. | `Contents/Info.plist`: `ASWebAuthenticationSessionWebBrowserSupportCapabilities`. | Actual login flow, provider protocol, cookies, and credentials were not inspected. |
| Apple Events usage text and a `scripting.sdef` declaration are present. | `Contents/Info.plist`: lines 276–279 and 324–325; `Contents/Resources/scripting.sdef`. | These establish intended OS integration, not granted TCC permissions or executed automation. |

These outer resources do **not** establish which OpenAI inference protocol the application uses, its HTTP payloads, stream parser, retry policy, prompt cache policy, session resumability, or token accounting. A hostname or permission declaration alone is insufficient for those conclusions.

## Signing observations and their limits

Read-only `/usr/bin/codesign --verify --deep --strict /Applications/ChatGPT.app` returned exit code **0**. Signature display reported:

- Identifier `com.openai.codex`, signer `Developer ID Application: OpenAI OpCo, LLC (2DC432GLL2)`.
- Hardened-runtime flag `0x10000(runtime)` and a stapled notarization ticket.
- CodeDirectory SHA-256 digest `73332b93baf1f0bc5ed95ca079cfa6aeceecb677c03a3ff15464aa7407f1dc9b`. This is not the SHA-256 of the executable file.
- Sealed-resource metadata: version 2, 13 rules, 3,616 files.

`codesign -d --entitlements - --xml` statically reported `app-sandbox=false`, `allow-jit=true`, `allow-unsigned-executable-memory=true`, and `network.client=true`. It also declares camera/audio, selected-file read/write, Apple Events, calendar, notification, keychain-group, and application-group capabilities. `get-task-allow` was not present in the returned main entitlement dictionary.

These are declared process entitlements, not observations of accessed data or runtime tool containment. The parent investigation retained **seven** declared-versus-observed ASAR integrity contradictions on unpacked native modules, including `permissions.node`. Successful macOS signature verification and ASAR-internal integrity comparisons are different checks; the signature result here does not resolve those mismatches and does not justify silently treating mismatched bytes as trusted original members. Their complete records remain in the parent REA inventory and extraction Evidence `ev_bb6e095aa440be97ef81f62516ea4b989a01f3843ef2acb109ab57046cd6df2f`.

## Optimization and observability evidence

Three performance-related settings are visible in the main plist:

| Setting | Original source | Evidence classification |
| --- | --- | --- |
| `LSEnvironment.MallocNanoZone="0"` | `Contents/Info.plist`: lines 256–260. | Confirmed allocation-environment setting. Its motivation and performance effect are unknown. It is not evidence of reduced memory consumption. |
| `GPUEjectPolicy="wait"` | `Contents/Info.plist`: lines 252–253. | Confirmed policy declaration, not a throughput benchmark. |
| `NSSupportsAutomaticGraphicsSwitching=true` | `Contents/Info.plist`: lines 316–317. | Confirmed graphics capability declaration, not observed GPU use. |

The shipped `THIRD_PARTY_NOTICES.txt` has dependency declarations relevant to deeper investigation:

| Declaration | Original line |
| --- | --- |
| `@sentry/electron (7.5.0)` | 6901 |
| `@sentry/node (10.29.0)` | 6929 |
| `@datadog/browser-rum (7.9.0)` | 42214 |
| `@statsig/statsig-node-core-darwin-arm64 (0.19.1)` | 7803 |
| `@tanstack/react-query (5.101.2)` | 8203 |
| `react-markdown (9.1.0)` and `(10.1.0)` | 30326, 30353 |
| `remark-gfm (4.0.1)` | 31148 |
| `rehype-katex (7.0.1)` | 31014 |
| `katex (0.16.45)` and `(0.18.7)` | 19029, 19056 |
| `mermaid (11.16.0)` | 20812 |
| `react-dom (19.2.7)` and `(19.2.8)` | 30126, 30153 |

The notices explicitly say the software **may be included**. Confirmed: these declarations are present. Inferred leads: error reporting, RUM, feature/config evaluation, query management, Markdown/math/diagram rendering. Unknown: loaded versions, initialization options, event fields, sampling, cache semantics, sanitization, and which renderers use each package. Multiple versions in the notice do not prove simultaneous use in a conversation renderer.

The unexpanded app directory contains **3,977 regular, non-symlink files totaling 1,729,703,201 bytes**. This count includes the ASAR as one file; it does not count its internal members or filesystem allocation overhead. Selected non-overlapping package sections are:

| Section | Regular-file count | Bytes |
| --- | ---: | ---: |
| `Contents/Frameworks/` | 367 | 383,572,612 |
| `Contents/Resources/app.asar` | 1 | 567,757,587 |
| `Contents/Resources/app.asar.unpacked/` | 19 | 4,736,306 |
| `Contents/Resources/codex-cli/` | 47 | 337,554,799 |
| `Contents/Resources/cua_node/` | 2,519 | 278,674,027 |
| `Contents/Resources/plugins/` | 845 | 50,834,266 |

These are disk-byte measurements. They establish where package weight resides, not startup cost, RAM use, lazy loading, or chat latency. They are useful boundaries for later performance experiments; they do not justify porting a setting into Jarvis without checking its actual code path and relevant reference implementations.

## Reproducible identity ledger

SHA-256 values below were computed directly from installed file bytes, with streaming reads for large images. The runtime archive hash in `owl-electron-app.json` is a declaration about a separate archive and should not be compared as though it were the framework file hash.

| Original file | SHA-256 |
| --- | --- |
| `Contents/Info.plist` | `d75273f1d1b91273aaef67db1e52276cf7aa3f50c5505b9081252172fae57410` |
| `Contents/MacOS/ChatGPT` | `2053a78d225a1492d0cb6896cfd83254af06c3277547e83d4d23fef88f00426e` |
| `Contents/Resources/app.asar` | `75e397e2280dde93b70a85aa926f4282c728eacc84dfeb15ba7fb6ddd84628af` |
| `Contents/Frameworks/Codex Framework.framework/Versions/155.0.8059.40/Codex Framework` | `a6daa6b1284b8f41264196d37307957a518c1d498ee2881acd93d0c729265dd5` |
| `Contents/Frameworks/Codex Framework.framework/Versions/155.0.8059.40/Resources/Info.plist` | `43c0456be6019d77328e34023e220cf3ef237bf095c840d2928ef510f26d8ff2` |
| `Contents/Frameworks/Sparkle.framework/Versions/B/Resources/Info.plist` | `acd6fcd92131b9d76929e47195ba2e9b4deaf88f8ec13b39ea69d74e1d589ded` |
| `Contents/Frameworks/Sparkle.framework/Versions/B/XPCServices/Downloader.xpc/Contents/Info.plist` | `0fe36b6c14c608730f2a20a117fa8a0a0f64a72c3de0d3c5957febebf5ff1430` |
| `Contents/Resources/owl-app.ini` | `96938e60cce9a894c7c90a5af53aec3675d5e0287abc63ce3a6a83ecb4a584f6` |
| `Contents/Resources/owl-electron-app.json` | `0a1a92738ea5da37625be1ddb7d2284265d6bb77b85e90059cf52bd3aa64e644` |
| `Contents/Resources/masque-proxy-origins.txt` | `bd3edc95c73d4fb77b8c95fd9e5ffee0ecab4a6c30bdbba7ef9dbe3b9b4c9379` |
| `Contents/Resources/codex-cli/codex-package.json` | `95ea87bed0ff10cb2cbd9b0541b0e332b4c87f74efd961924029beb33bebb407` |
| `Contents/Resources/codex-cli/bin/codex` | `50ab38ba21d0d9f8346f32f41848382f15b556190f3c7a07e885a4fb73e379c8` |
| `Contents/Resources/codex-cli/CodexCLI.app/Contents/MacOS/codex` | `0d5ac7aae20c4eda6242428d5e218c55ba96d8763fda2a271088ff37104b9d28` |
| `Contents/Resources/cua_node/manifest.json` | `e4aa2e0b8aee3854d2139437d47b0f7a77832d8e16bf1770c3c27ce69212aac9` |
| `Contents/Resources/native/sparkle.node` | `d95a8c8a6af94274b2dbfb5e1e4c53a593923a87f5b619618f072c1ec4211508` |
| `Contents/Resources/THIRD_PARTY_NOTICES.txt` | `c165b70975e18b881676030278e6091fb4fdd21d464bd7caf8b6d9716287333b` |

## Coverage and unresolved questions

Answered: installed identity, outer runtime bootstrap, framework and native-tool packaging, selected configuration, code-signing observations, dependency-notice leads, and package disk size.

Partially answered: the architecture boundary between Chromium/Owl and the ASAR app; the parent application's static JavaScript analysis provides the next evidence layer. Packaging alone does not establish active Markdown, telemetry, or query behavior.

Unresolved here: provider inference requests and stream protocol, actual retries and cancellation, chat storage and rendering strategy, memory/latency performance, and runtime permissions. No runtime test, app execution, network probe, user-data inspection, source change to Jarvis, commit, or push was performed in this resource pass.
