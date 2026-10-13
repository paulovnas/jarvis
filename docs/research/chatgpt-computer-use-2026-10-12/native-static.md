# Computer Use: native static evidence

Target: `/Applications/ChatGPT.app`, inspected 2026-10-12. This is static evidence from shipped bytes. There was no controlled ChatGPT Computer Use session, target-service execution, account-state inspection, or network capture. Hopper was operated as an analysis tool.

## Identity and evidence boundaries

| Component | Installed file | Bytes | SHA-256 |
| --- | --- | ---: | --- |
| Electron bootstrap | `/Applications/ChatGPT.app/Contents/MacOS/ChatGPT` | 71,008 | `2053a78d225a1492d0cb6896cfd83254af06c3277547e83d4d23fef88f00426e` |
| Native desktop service | `/Applications/ChatGPT.app/Contents/Resources/cua_node/lib/node_modules/@oai/sky/Codex Computer Use.app/Contents/MacOS/SkyComputerUseService` | 24,583,472 | `ac8bfe406d01a7cb43f7550c080587ae3a5f404f82fa6dcc399bf72a85cd765d` |
| Electron native overlay bridge | `/Applications/ChatGPT.app/Contents/Resources/native/sky.node` | 1,385,120 | `0f64109efdecbc9734a201ee0ac25c8e7f6a69878194b55231b44fa1fe4476a7` |

The service Mach-O record is **`ev_2ac66ed834a728ec93dfbef85c75a7576af095c894136c046ab2974d747b5b75`**. It retains dependencies, imports and 66,825 exported entries. An export is not necessarily a unique function: Swift metadata, async descriptors, aliases and accessors are included. The overlay Mach-O record is **`ev_e06a9334ab56c50ccb7b68ecd6bd295525e2f2e036a46c0be89c93549f34b3af`**.

The identities were freshly hashed again at the end of the static pass. `codesign --verify --deep --strict /Applications/ChatGPT.app` exited 0 with empty output. This confirms that the local verification command accepted the installed signing state; it does not establish every runtime security property or explain the ASAR contradictions below.

## Native roles are distinct

**Confirmed static.** `SkyComputerUseService` has request handlers for application inventory, policy, skyshots, actions and audio. Its exports include `ComputerUseAppController`, `SkyshotOperation`, `ComputerUseIPCServer`, `ApplicationUIElement`, focus enforcement, virtual cursor and intervention state. Its JavaScript client is the framed native-pipe implementation in companion [cua-protocol.md](/Users/enesolucoes/Documents/Pessoal/jarvis/docs/research/chatgpt-computer-use-2026-10-12/cua-protocol.md).

**Confirmed static.** `sky.node` includes native PIP/overlay, status-item and modifier-input infrastructure. The successfully analyzed procedure below bootstraps a remotely hosted PIP endpoint. It is not evidence that `sky.node` alone executes all desktop actions.

**Inference.** The service is the main macOS app-control implementation, while the Electron module connects it to product presentation and lifecycle. This is supported by independent JS routing, IPC types and native call edges, rather than names alone. A complete dispatch and caller-authentication graph remains open.

## Native request and action call edges

REA's Hopper provider timed out while initializing the larger service. The following additional observations therefore use the installed Apple LLVM disassembler against the same hashed file, correlated with REA's exact export addresses. They are supplemental **N** identifiers, not fabricated REA Evidence IDs.

[native-call-evidence.json](/Users/enesolucoes/Documents/Pessoal/jarvis/docs/research/chatgpt-computer-use-2026-10-12/native-call-evidence.json) retains the command template, tool version, bounded regions, assembly-output hashes, call-site addresses, callee symbols and limitations. `xcrun swift-demangle --compact` decoded retained Swift symbols. Imported stubs were mapped using `llvm-objdump --macho --indirect-symbols`.

Addresses are static Mach-O VM addresses, not file byte offsets or runtime ASLR addresses. Regions are half-open inspection spans, **not verified complete procedure bodies**. Swift async continuations and indirect calls may cross these regions. Nearest-symbol annotations from `llvm-objdump` were ignored because they sometimes assign unrelated labels to numerical addresses. Shared-address aliases remain ambiguous.

| ID / associated entry | Region | Observed call site → exact callee | Conclusion and limit |
| --- | --- | --- | --- |
| N01: `ComputerUseIPCAppPerformActionRequest.handle(senderContext:)` | `[0x100154d90,0x100155a00)` | `0x100154e4c` → `0x100260850`, `NSWorkspace.resolveApplicationTarget(for:)`; `0x100154e7c` → `0x100255ed0`, `BundleIdentifiers.isForbiddenComputerUseTarget`; `0x10015585c` → `0x10073e43c`, `SyntheticAppFocusEnforcer.init(pid:)` | Target resolution, a forbidden-target decision and PID focus machinery are present in this request region. Complete error and dispatch paths remain unresolved. |
| N02: `ComputerUseIPCAppGetSkyshotRequest.handle(senderContext:)` | `[0x10015d598,0x10015e600)` | `0x10015da08` → `0x100074880`, `ComputerUseAppController.activate`; `0x10015dcbc` → `0x100074394`, `updateSkyshotSettlingIfNeeded(disableAXDiffing:)`; `0x10015de68` → `0x1000a9288`, `clearUserInterruptedInterventionAfterStateRequery(for:)` | A state requery is connected to UI settling and intervention-state handling. This does not prove that all user intervention can safely be resumed. |
| N03: coordinate `ComputerUseAppController.click(at:...)` | `[0x10008234c,0x100083a00)` | `0x1000825e8` → `0x100084938`, `orderedWindows`; `0x100082610` → `0x1007039b0`, `target(forMouseEventAt:with:)`; `0x1000831ac` → `0x1006fa474`, `ApplicationUIElement.sendClick`; `0x100083240` → `0x1007058fc`, `waitForUIToSettle`; `0x1000838b0` → `0x10007177c`, `updateSkyshot` | Window targeting, event delivery, settling and fresh observation are connected by direct calls in this region. Runtime branch choices and successful effects were not observed. |
| N04: `ApplicationUIElement.sendClick(to:at:...)` | `[0x1006fa474,0x1006fc700)` | `0x1006faa34` → `0x10073f634`, `safelyEnforceActiveState`; `0x1006facb0` and `0x1006fad14` → `0x100256e64`, `CGEventAPI.postToPid`; adjacent calls use `setTimestamp` | The click region contains target-process event posting and focus enforcement. The lower wrapper's indirect/dynamic implementation remains unresolved. |
| N05: `SkyshotOperation.captureScreenshot(imageSize:)` | `[0x100233b44,0x100234064)` | `0x100233e80` → `0x100efc28c`, `ScreenshotImplementation.captureScreenshotBuffer`; `0x100233fdc` → `0x100efc3a0`, `writeScreenshotToFile` | Screenshot buffer creation and file materialization are present. The complete capture backend, composition and file lifetime remain unresolved. |
| N06: `SkyIPCRequirement.evaluate(senderAuditTokenData:)` | `[0x100f45b9c,0x100f46000)` | `0x100f45e08` → imported stub `0x1010b9c2c`, `_SecCodeCopyGuestWithAttributes` | The requirement-evaluation region examines code identity using Security.framework. This alone does not prove which IPC paths require it or what signer policy is enforced. |

The N01 policy-result branch is observed directly:

```asm
0x100154e7c: bl  0x100255ed0    ; exact export: isForbiddenComputerUseTarget
0x100154e80: ldr x24, [x22, #0x128]
0x100154e84: tbz w0, #0x0, 0x100154ed0
```

**Confirmed static:** the Boolean result controls a branch. **Unresolved:** the full error construction and whether every entry path reaches this branch. Do not upgrade this to a complete native policy audit.

### Geometry is implemented, but its calibration is still unknown

N03 contains imported calls to `CGAffineTransformMakeScale`, `CGAffineTransformMakeTranslation` and `CGPointApplyAffineTransform`, including the group at `0x100082554`, `0x100082578`, `0x100082590`, `0x1000825b0`. This confirms that coordinate transformation participates in the inspected click region. Its input parameters and reference frames were not recovered completely, so a Retina/multi-display mapping formula would be an inference. The same region references `UIElementAction.press`, but the full semantic AX action-versus-coordinate fallback selection was not resolved.

### Capture and permission imports

**Confirmed declarations/imports in the service Mach-O:**

- AX trust and element operations: `AXIsProcessTrusted`, attribute reads, hit testing, action-name lookup, `AXUIElementPerformAction`, value setting and AX observer infrastructure.
- Capture permission: `CGPreflightScreenCaptureAccess`, `CGRequestScreenCaptureAccess`; a ScreenCaptureKit dependency and `SCScreenshotManager` class reference.
- Sender/process identity: `audit_token_to_pid`, `SecTaskCreateWithAuditToken`, `SecCodeCopyGuestWithAttributes`, signing-information reads and NSXPC infrastructure.
- Local transport: socket/listen/accept/connect functions.

**Confirmed export:** `ScreenshotImplementation.captureScreenshotWithSkyLight(windowIDs:rect:)` at `0x100efae18`, plus capture-buffer and capture-file interfaces. **Inference:** more than one capture path may exist. Imports and names do not establish which backend a specific request uses. In particular, this pass does not demonstrate ScreenCaptureKit-only capture or private-SkyLight execution.

No TCC database, permission settings, current app inventory, clipboard or user screenshot was read. A linked API or entitlement is not proof of granted OS permission.

## PIP endpoint bootstrap: recovered pseudocode

REA successfully analyzed `sky.node` procedure **`-[RemoteHostedPIPContentService sendBootstrapToServiceWithProcessIdentifier:rendezvousPort:attempt:]`**, static address **`0x12428`**, Evidence **`ev_5afd957de2f3039f39c98adc8edd649323d958d973c958c328d2f3d984b95ad5`**.

The recovered pseudocode shows:

1. Construct an Apple Event addressed to the expected process, using class `0x536b4375` (`SkCu`) and event ID `0x5069504e` (`PiPN`).
2. Supply the rendezvous Mach port and version string `CodexComputerUseNativeBridge-1`.
3. Receive an XPC response, obtain its audit token and convert it to a PID.
4. Require that PID to match the expected service process, then construct a typed `NSXPCListenerEndpoint` from the returned endpoint.

**Confirmed in decompiler output:** the relationship between expected PID, XPC audit-token PID and endpoint acceptance. **Limit:** Hopper's pseudocode contains reconstructed types/registers; the provider did not expose complete procedure body ranges. This is a PIP bootstrap authentication observation, not proof of authorization for every action request or browser connection.

Supporting overlay records: overview `ev_80db31ecc9768c4cc3587cac6d9891945fa47cfd192e24efa7674ba263db615e` (6,199 procedures, 2,046 strings); procedure search `ev_f6b588b6b6d713d611776d4e67e52a275e49c63c139e22beb0174c81afde7c78`; string search `ev_16e5e9ad98add3ef2f006e3817ace6df788715b6b01498ce7a3e289b7e837f96`. Those inventories establish discoverable analysis entries, not 6,199 audited procedures.

## App identity, helper topology and entitlements

Main Info.plist Evidence `ev_7f6b556fa07391d8c6510e678eddd4c01fb7e9cad1f6112d5e647faaa08bf3bd` and service Info.plist Evidence `ev_c83d04c8de6606afd48e524cf68475cc6e90ac295d2090b94d1c786e1799316c` distinguish the visible installed app from its helper:

| Component | Confirmed declaration | Meaning / limit |
| --- | --- | --- |
| Main app | Version `26.1007.30912`, build `20156`; bundle identifier `com.openai.codex`; visible installed target `/Applications/ChatGPT.app` | The user-supplied application contains an Electron/Codex package. The directory name cannot identify the entire product architecture alone. |
| Computer Use service | `com.openai.sky.CUAService`; executable `SkyComputerUseService`; version `26.1005.1001425`, build `1001425`; minimum macOS `14.4`; `LSUIElement=true` | Separate helper/version/capability boundary. Main-app compatibility does not prove Computer Use compatibility. |
| CLI helper | `com.openai.sky.CUAService.cli`; `LSUIElement=true` | Shipped client helper, not proof it ran. Plist Evidence `ev_855122aac1af3b488fb5d77c191a4cc7b67251c36af75fb82a0398f9a7371492`. |
| Guardian | `com.openai.sky.CUAService.guardian`; `LSUIElement=true` | Shipped lock-screen support topology, not an installed capability. Plist Evidence `ev_8a598f64e1e5ce14dd35a96f8cd951fa0876aed53263bbbca31fc3587c2caf1f`. |
| Installer and authorization plug-in | `com.openai.sky.CUAService.AuthorizationPluginInstaller` and `com.openai.sky.CUAService.AuthorizationPlugin` | Separate privileged/locked-use components exist. Their behavior was not recovered or installed. Their exact paths/hashes are in the protocol report. |

Signature metadata Evidence **`ev_f4bf96c6a88bbb34bea2b5894af33a123df81c75ecbabf4ec592b706863b44f2`** (main app) and **`ev_4b6aa507f839e729884cee97dcf294500773932d3f52e7f32d9142e3e786eade`** (service) declare team `2DC432GLL2`, OpenAI OpCo, and hardened runtime. Main entitlements include `app-sandbox=false`, Apple Events automation, JIT/unsigned executable memory and the `2DC432GLL2.com.openai.sky.CUAService` application group. The helper also declares that group, automation and selected contacts/calendar/keychain access. These are signing declarations, not evidence that all capabilities were exercised.

The absence of main-app App Sandbox does not settle the native REPL's internal isolation. Its evaluator and trusted-service admission still require a separate analysis.

## Integrity contradictions and reproducibility

REA's strict ASAR inspection initially stopped on declared-versus-observed hashes for unpacked native content. Repeating with explicit `integrity_policy: "record-and-continue"` retained **seven contradictions**. The affected members are Worklouder permissions, HID and serial-port binaries, better-sqlite3, node-pty/spawn-helper and objc-js. The exact paths and both hashes are in [evidence-manifest.json](/Users/enesolucoes/Documents/Pessoal/jarvis/docs/research/chatgpt-computer-use-2026-10-12/evidence-manifest.json) and the full REA bundle.

REA labels those observations **observed-untrusted** relative to the declared ASAR integrity metadata. The mismatch does not itself demonstrate malicious modification. Acceptance by local `codesign` also does not resolve the packaging inconsistency. Conclusions here are bound to installed byte hashes, with the mismatch retained as an open provenance question.

## Analysis-provider limitation

Opening the service worked and its portable Mach-O metadata was retained. Hopper initialization for the larger service exceeded REA's 120-second startup timeout; a subsequent request returned unreadable output, and close reported **`cleanup_incomplete`**. A provider-neutral snapshot was saved as [rea-analysis.snapshot.json](/Users/enesolucoes/Documents/Pessoal/jarvis/docs/research/chatgpt-computer-use-2026-10-12/rea-analysis.snapshot.json). A Hopper `.hop` database save was attempted, but no resulting file was verified; this report does not claim it exists.

The REA active target was closed, but authenticated provider-resource cleanup was not confirmed. Subsequent engine opening was blocked by that cleanup state. Static LLVM reads and retained-evidence import/export allowed useful work to continue without modifying REA configuration, installing another engine, changing the target or launching the Computer Use service. Recovering the REA/Hopper connection is required before resuming full native pseudocode analysis. No decompilation of the service is claimed from the saved snapshot.

## Remaining native questions

The open questions are registered in the REA bundle and summarized in the main report: complete IPC sender policy; action-validation/dispatch graph; cancellation/intervention race behavior; clipboard restoration; capture geometry and backend choice; native `node_repl` isolation; installer/guardian/locked-session behavior. Static declarations and bounded call edges materially narrow these questions, but do not close them.
