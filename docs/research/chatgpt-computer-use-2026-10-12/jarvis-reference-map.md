# Computer use: Jarvis source and reference map

Static source review, 2026-10-12. This companion report maps existing Jarvis capabilities and the mandatory Codex, OpenCode and OMP reference trees. It is not evidence of how the shipped ChatGPT application executes. Findings about that application must come from the separate REA investigation.

## Evidence boundary

- **Confirmed source** means a declaration, control flow or test assertion was inspected in the current local tree. Tests were read, not executed.
- **Inferred implication** means a consequence of that source which still needs runtime verification.
- **Recommendation** means a reconstruction choice for Jarvis, not recovered ChatGPT implementation.
- No native application, browser, page, model call, TCC permission request or desktop action was executed in this pass. No source implementation, commit, push or reference-tree edit was performed.

Source identities:

| Tree | Inspected Git HEAD |
| --- | --- |
| Jarvis | `43d27efd12ae70e36a5146cffcffa7c0bafefaf2` |
| `docs/codex` | `0df76892b13277eca943a6507c7964e4b0f99a5b` |
| `docs/opencode` | `108b988a08227df45417f27905a4d6b27ad49b6d` |
| `docs/omp` | `fa786c0d790a6f8408960386530efe95ef45f8c6` |

The Jarvis checkout was dirty. Jarvis findings refer to inspected working-tree bytes, not exclusively the committed HEAD. Three useful reproduction fingerprints are:

| Source | SHA-256 |
| --- | --- |
| `src-tauri/src/agent/browser/tools.rs` | `f39948e8c0b1b668c5cb7f8301045b541512e02cee98e8d996889687f6947b6d` |
| `src-tauri/src/agent/browser/cdp/actions.rs` | `2a7ef289b9cf3bf0d1eecccc727a36e33a587a8a5a906c62cc3242d7bdc954db` |
| `src-tauri/src/agent/workflow/recovery.rs` | `2cd10f83a7f8e3470a2f68cf2536c08d4471601b839fd71837a7775d065d4457` |

## What Jarvis already implements

| ID | Confirmed source finding | Concrete evidence |
| --- | --- | --- |
| J1 | Native integrated browser tabs are owned by a conversation. A supplied tab ID must exist in that conversation. A shared asynchronous operations lock serializes integrated browser commands. | [BrowserState and tab lookup](/Users/enesolucoes/Documents/Pessoal/jarvis/src-tauri/src/agent/browser.rs:77), lines 77–84, 177–199; [command](/Users/enesolucoes/Documents/Pessoal/jarvis/src-tauri/src/agent/browser.rs:444), lines 444–475. |
| J2 | The model catalog offers list, snapshot, wait, console, screenshot, discover, network, response body, open, attach, navigate, close, click, fill, press, scroll, evaluate and DevTools tools. Interactions target one current DOM ID or semantic locator, with frame support. Page key events exclude browser/OS shortcuts. Plan mode removes mutating tools. | [definitions and mutating](/Users/enesolucoes/Documents/Pessoal/jarvis/src-tauri/src/agent/browser/tools.rs:5), lines 5–60. |
| J3 | Integrated screenshots use Chromium `Page.captureScreenshot` with viewport-only PNG output, a 15-second timeout and a 20 MiB image limit. Original bytes become a conversation attachment; the returned JSON asks the agent to call `vision`. | [capture](/Users/enesolucoes/Documents/Pessoal/jarvis/src-tauri/src/agent/browser/capture.rs:6), lines 6–36; [screenshot handler](/Users/enesolucoes/Documents/Pessoal/jarvis/src-tauri/src/agent/browser.rs:581), lines 581–595. |
| J4 | A screenshot can optionally be exported to a new project-relative PNG file in Build mode. Export changes admission to a filesystem write; ordinary capture does not require execution approval. The export path cannot cross the extension boundary. | [tool schema](/Users/enesolucoes/Documents/Pessoal/jarvis/src-tauri/src/agent/browser/tools.rs:54), lines 54–60; [Orchestrator preflight](/Users/enesolucoes/Documents/Pessoal/jarvis/src-tauri/src/agent/tool_contract.rs:355), lines 355–369; [routing](/Users/enesolucoes/Documents/Pessoal/jarvis/src-tauri/src/agent/browser/routing.rs:318), lines 318–345. |
| J5 | CDP click/fill/press first wait for actionability, verify the target and send input once. After input dispatch, a subsequent failure becomes `browser_outcome_unknown`; success only confirms dispatch and instructs the agent to inspect the resulting state. | [Session interaction](/Users/enesolucoes/Documents/Pessoal/jarvis/src-tauri/src/agent/browser/cdp/actions.rs:389), lines 389–463. |
| J6 | External browser discovery and attachment are separate tools. New navigation stays integrated. Explicit external IDs retain their backend. The extension transport distinguishes read-only interruption from an uncertain mutation. | [catalog](/Users/enesolucoes/Documents/Pessoal/jarvis/src-tauri/src/agent/browser/tools.rs:33), lines 33–45; [backend selection](/Users/enesolucoes/Documents/Pessoal/jarvis/src-tauri/src/agent/browser/routing.rs:311), lines 311–340; [interrupted](/Users/enesolucoes/Documents/Pessoal/jarvis/src-tauri/src/agent/browser/extension.rs:605), lines 605–623. |
| J7 | Browser execution races the turn cancellation signal. Dropping an extension request removes its pending record and sends a best-effort cancellation message for remaining steps. CDP requests have bounded waits and remove pending correlation entries on drop. Cancellation never undoes input already dispatched. | [execute](/Users/enesolucoes/Documents/Pessoal/jarvis/src-tauri/src/agent/browser/tools.rs:64), lines 64–95; [PendingGuard](/Users/enesolucoes/Documents/Pessoal/jarvis/src-tauri/src/agent/browser/extension.rs:625), lines 625–641; [CDP request](/Users/enesolucoes/Documents/Pessoal/jarvis/src-tauri/src/agent/browser/cdp.rs:131), lines 131–159. |
| J8 | Model-visible tools and executable tools share a finalized catalog. Preflight validates tool names and argument schemas. Workflow filtering combines role, flow, user-defined capability and discovery restrictions, then execution rechecks browser mutation versus role mode. | [Orchestrator](/Users/enesolucoes/Documents/Pessoal/jarvis/src-tauri/src/agent/tool_contract.rs:337), lines 337–377; [filter and allowed](/Users/enesolucoes/Documents/Pessoal/jarvis/src-tauri/src/agent/workflow.rs:860), lines 860–974; [browser dispatch](/Users/enesolucoes/Documents/Pessoal/jarvis/src-tauri/src/agent/workflow.rs:1336), lines 1336–1348. |
| J9 | Existing grants cover conversation, project or repository scopes; once, session, expiry or persistent duration; exact commands, command prefixes or exact tool arguments. Effects currently describe filesystem, network, process, destructive, dynamic and unknown operations. | [GrantScope, GrantDuration and GrantSubject](/Users/enesolucoes/Documents/Pessoal/jarvis/src-tauri/src/agent/execution_grants.rs:32), lines 32–99; [ExecutionEffects](/Users/enesolucoes/Documents/Pessoal/jarvis/src-tauri/src/agent/execution_policy.rs:37), lines 37–45. |
| J10 | Interrupted calls without durable results are collected for recovery. Repeating an already-confirmed action is blocked; unresolved actions are blocked by signature or related resource selectors. Independent work remains possible. | [collect](/Users/enesolucoes/Documents/Pessoal/jarvis/src-tauri/src/agent/workflow/recovery.rs:169), lines 169–219; [recovery_preflight](/Users/enesolucoes/Documents/Pessoal/jarvis/src-tauri/src/agent/workflow/recovery.rs:488), lines 488–545. |
| J11 | Vision checks conversation-owned image attachments, accepts at most four images and a specific question, and instructs the selected provider to distinguish visible facts from inference and ignore instructions in images. | [vision contract](/Users/enesolucoes/Documents/Pessoal/jarvis/src-tauri/src/agent/vision.rs:140), lines 140–175, 236–244; [ownership assertion](/Users/enesolucoes/Documents/Pessoal/jarvis/src-tauri/src/agent/vision.rs:463), lines 463 onward. |
| J12 | The thin frontend already has browser schemas, native command forwarding, state subscriptions and a native viewport component. These are integration points for presenting a future computer session, not an existing OS controller. | [browser.ts](/Users/enesolucoes/Documents/Pessoal/jarvis/src/core/browser.ts:6), lines 6–19; [useBrowser](/Users/enesolucoes/Documents/Pessoal/jarvis/src/hooks/use-browser.ts:8), lines 8–44; [NativeViewport](/Users/enesolucoes/Documents/Pessoal/jarvis/src/components/browser/BrowserPanel.tsx:31), lines 31–69. |

Two subtleties matter when reusing this code:

1. Ordinary browser capture is permitted without execution approval, but browser tool capabilities are classified as stateful rather than generically parallel-safe reads. The screenshot test name says “read-only”; its assertion checks approval, not `Effect::ReadOnly`. Do not derive scheduler behavior from that test name. See [Capabilities::native](/Users/enesolucoes/Documents/Pessoal/jarvis/src-tauri/src/agent/tool_contract.rs:130), lines 130–204, and [screenshot test](/Users/enesolucoes/Documents/Pessoal/jarvis/src-tauri/src/agent/tool_contract.rs:632), lines 632–664.
2. Browser dispatch has uncertainty handling, but durable recovery lacks a browser-specific evidence relation. `Call::related` handles files, MCP selectors, repositories, Beads and process/terminal inspection, then returns false; it has no `browser_*` branch. `observe_recovery_result` records evidence only through that relation, and `resolve_recovery` requires recorded durable evidence. **Confirmed source gap:** a successful browser snapshot cannot currently reconcile an interrupted browser action through this mechanism. **Inferred implication:** after restart, a same-target browser mutation can remain blocked until a supported reconciliation path is added. See [Call::related](/Users/enesolucoes/Documents/Pessoal/jarvis/src-tauri/src/agent/workflow/recovery.rs:280), lines 280–346; [observe_recovery_result and resolve_recovery](/Users/enesolucoes/Documents/Pessoal/jarvis/src-tauri/src/agent/workflow/recovery.rs:550), lines 550–653. Runtime reproduction was not performed.

### Bounded desktop absence search

A search over `src-tauri/src`, `src` and `scripts` for `CGEvent`, `AXUI`, `AXIs`, `ScreenCaptureKit`, `SCStream`, `osascript`, `screencapture`, `computer_use` and `ComputerUse` found no matches. The inspected first-party browser catalog exposes DOM/page input, not display/window enumeration or native AX targets. [desktop.rs](/Users/enesolucoes/Documents/Pessoal/jarvis/src-tauri/src/desktop.rs:1), lines 1–36, stores Jarvis layout preferences.

**Confirmed boundary:** no native desktop automation implementation was found within that search and the inspected tool/catalog paths. This is not proof of absence across installed MCP servers, plugins, other checkout paths or generated binaries; those were not inventoried here.

## Mandatory references, studied in order

### Codex: harness identity, admission and cancellation

The local Codex Rust tree is the primary reference for the host contract. It does not by itself establish the implementation of ChatGPT's shipped computer controller.

| ID | Confirmed source | Transferable concept |
| --- | --- | --- |
| C1 | `is_node_repl_backed_server` recognizes `node_repl` and `cua_repl`. Registered connector identity can also identify the OpenAI browser connector; comments explicitly exclude tool arguments and provider-supplied elicitation metadata as identity sources. | The host establishes controller identity before reviewing a call. Treating a tool as computer use must not depend on untrusted argument text. [protocol/mcp.rs](/Users/enesolucoes/Documents/Pessoal/jarvis/docs/codex/codex-rs/protocol/src/mcp.rs:35), lines 35–64. |
| C2 | `GuardianScope::for_mcp_connector` classifies these connectors as `ComputerUse`; other MCP connectors use `Mcp`. The model's review mode determines whether computer-use review is required. | Computer control is a distinct approval domain even when transported through a generic MCP/REPL bridge. [guardian.rs](/Users/enesolucoes/Documents/Pessoal/jarvis/docs/codex/codex-rs/protocol/src/openai_models/guardian.rs:125), lines 125–178. |
| C3 | Computer-use configuration supports a default app access policy and macOS bundle-ID overrides. Browser policies separately cover origin access, downloads, uploads, history and full-CDP access. | App identity, web origins, uploads/downloads and broad debugging privileges are different boundaries. [computer_use.rs](/Users/enesolucoes/Documents/Pessoal/jarvis/docs/codex/codex-rs/config/src/computer_use.rs:9), lines 9–34; [browser_use.rs](/Users/enesolucoes/Documents/Pessoal/jarvis/docs/codex/codex-rs/config/src/browser_use.rs:9), lines 9–22. |
| C4 | Host confirmation policy documents travel as `openai/confirmation_policies`; actor calls receive the issuing model step's browser/computer policy snapshot. Guardian review sessions are excluded. An empty object clears startup defaults. | Freeze the relevant policy at an action boundary; do not silently adopt stale startup policy or let reviewer state inherit actor privileges. [build_confirmation_policies_request_meta](/Users/enesolucoes/Documents/Pessoal/jarvis/docs/codex/codex-rs/core/src/mcp_tool_call.rs:1376), lines 1376–1407. |
| C5 | MCP elicitation obtains an active turn and cancellation token. Even full access preserves cancellation checks; a cancelled request yields Abort. Nested review identifies originating calls through host-registered invocation metadata. | Execution authorization and active session lifetime are independent conditions. [session/mcp.rs](/Users/enesolucoes/Documents/Pessoal/jarvis/docs/codex/codex-rs/core/src/session/mcp.rs:763), lines 763–835. |

Relevant inspected test declarations/assertions include [computer_use_config_round_trips](/Users/enesolucoes/Documents/Pessoal/jarvis/docs/codex/codex-rs/config/src/computer_use_tests.rs:6), [browser_use_origin_policies_round_trip](/Users/enesolucoes/Documents/Pessoal/jarvis/docs/codex/codex-rs/config/src/browser_use_tests.rs:7), [guardian_mcp_tool_call_request_meta_excludes_actor_confirmation_policy](/Users/enesolucoes/Documents/Pessoal/jarvis/docs/codex/codex-rs/core/src/mcp_tool_call_tests.rs:1256), and [MCP serialized/read-only concurrency tests](/Users/enesolucoes/Documents/Pessoal/jarvis/docs/codex/codex-rs/core/tests/suite/rmcp_client.rs:2450), lines 2450, 2569, 2672. Their existence is not a current passing test result.

### OpenCode: generic tool contracts and operational permissions

The inspected OpenCode built-in registry lists shell, file, task, web, skill, question, LSP and plan tools; it contains no built-in native computer controller. A bounded search in `packages/opencode/src/tool`, `src/permission` and `src/session` for computer-use, Playwright, screenshot and browser terms found no equivalent controller. Plugins and MCP supply separately registered capabilities, so that bounded result must not be generalized to every OpenCode installation.

| ID | Confirmed source | Transferable concept |
| --- | --- | --- |
| O1 | Tool execution has a typed parameter decoder, a canonical invalid-arguments error, an `AbortSignal`, an `ask` permission hook and attachments in its result. | Keep cancellation, permission requests, result attachments and validation at the same tool boundary. [tool.ts](/Users/enesolucoes/Documents/Pessoal/jarvis/docs/opencode/packages/opencode/src/tool/tool.ts:18), lines 18–64. |
| O2 | `Permission.ask` evaluates every resource pattern against rules; deny fails immediately, allow continues, unresolved requests wait on a deferred reply and remove pending state in finalization. | A reusable scoped permission state machine is preferable to per-action ad hoc UI. [permission/index.ts](/Users/enesolucoes/Documents/Pessoal/jarvis/docs/opencode/packages/opencode/src/permission/index.ts:67), lines 67–107. |
| O3 | Rejection rejects related pending requests in the same session. “Always” approval records patterns and resolves matching pending requests. Finalization fails all remaining requests. | Approval waits have lifecycle and cleanup semantics; closing a session is not approval. [permission/index.ts](/Users/enesolucoes/Documents/Pessoal/jarvis/docs/opencode/packages/opencode/src/permission/index.ts:54), lines 54–60, 109–166. |
| O4 | The registry merges agent/session permissions and filters visible tools; code mode separately describes permitted MCP tools. | Keep advertised capabilities consistent with executable capabilities. [registry.ts](/Users/enesolucoes/Documents/Pessoal/jarvis/docs/opencode/packages/opencode/src/tool/registry.ts:279), lines 279–338; [built-in registry](/Users/enesolucoes/Documents/Pessoal/jarvis/docs/opencode/packages/opencode/src/tool/registry.ts:209), lines 209–225. |

Operational trade-off: OpenCode supplies reusable generic permission/tool patterns in this tree; OMP supplies a concrete OS controller. Rebuilding a desktop driver from the OpenCode registry alone would require inventing the missing native behavior.

### OMP: complete native desktop reference

| ID | Confirmed source | Transferable concept and limit |
| --- | --- | --- |
| M1 | The enabled computer prelude exposes run, call, capabilities and close. Schemas reject unknown fields; read-only runs/direct inspection map to read approval. The worker independently guards mutations using the current run context. | Compact persistent scripting can reduce round trips, but a read-only flag must be enforced by the controller rather than trusted as model intent. [computer.ts](/Users/enesolucoes/Documents/Pessoal/jarvis/docs/omp/packages/coding-agent/src/tools/computer.ts:65), lines 65–118, 121–158; [guardRun](/Users/enesolucoes/Documents/Pessoal/jarvis/docs/omp/packages/coding-agent/src/tools/computer/worker.ts:179), lines 179–182. |
| M2 | A native desktop session supports display/window listing, screenshots, pointer movement/click/drag/scroll, text/key input, raising windows and AX read/action operations. It is backed by the Rust desktop module. | Separate target enumeration, observation, native input and semantic accessibility actions. [NativeDesktopSession](/Users/enesolucoes/Documents/Pessoal/jarvis/docs/omp/packages/coding-agent/src/tools/computer/worker.ts:39), lines 39–74; [DesktopSession](/Users/enesolucoes/Documents/Pessoal/jarvis/docs/omp/crates/pi-natives/src/desktop/mod.rs:720), lines 720–1024. |
| M3 | macOS capability checks use screen-capture preflight and AX trust. Input/AX and capture permission states are reported separately; background-window input depends on SkyLight availability. | Expose actual OS capability/permission state; process presence or installed code does not prove permission. [macos/mod.rs](/Users/enesolucoes/Documents/Pessoal/jarvis/docs/omp/crates/pi-natives/src/desktop/macos/mod.rs:36), lines 36–70; [AX trust](/Users/enesolucoes/Documents/Pessoal/jarvis/docs/omp/crates/pi-natives/src/desktop/macos/ax.rs:61), lines 61–64; [capture preflight](/Users/enesolucoes/Documents/Pessoal/jarvis/docs/omp/crates/pi-natives/src/desktop/macos/capture.rs:25), lines 25–31. |
| M4 | Screenshots retain emitted and original dimensions plus target identity. Rust `FrameGeometry::map_point` translates captured pixels to desktop coordinates, validates bounds, reanchors moved windows and rejects resized windows. | The capture is a coordinate frame, not just a PNG. Provider resizing, Retina scale, moved/resized windows and multi-display gaps must be explicit. [protocol.ts](/Users/enesolucoes/Documents/Pessoal/jarvis/docs/omp/packages/coding-agent/src/tools/computer/protocol.ts:36), lines 36–44; [FrameGeometry](/Users/enesolucoes/Documents/Pessoal/jarvis/docs/omp/crates/pi-natives/src/desktop/frame.rs:97), lines 97–151. |
| M5 | Native image transport constraints can cap capture dimensions to 1280×896 for providers that cannot preserve original detail. The screenshot worker emits text, image pixels and screenshot metadata. | Preserve model-visible versus input-coordinate equivalence rather than assuming every provider sends unchanged pixels. [computer.ts](/Users/enesolucoes/Documents/Pessoal/jarvis/docs/omp/packages/coding-agent/src/tools/computer.ts:28), lines 28–44, 238–253; [captureScreenshot](/Users/enesolucoes/Documents/Pessoal/jarvis/docs/omp/packages/coding-agent/src/tools/computer/worker.ts:184), lines 184–221. |
| M6 | The supervisor forwards abort to the worker and nested tool calls. Timeout plus grace terminates a stalled worker, rejects pending operations and permits a fresh worker on a later request. Close has a bounded forced-termination fallback. | Give cancellation and timeout a concrete controller lifetime; never confuse a killed worker with a reverted OS effect. [supervisor.run](/Users/enesolucoes/Documents/Pessoal/jarvis/docs/omp/packages/coding-agent/src/tools/computer/supervisor.ts:187), lines 187–215; [timeout and terminate](/Users/enesolucoes/Documents/Pessoal/jarvis/docs/omp/packages/coding-agent/src/tools/computer/supervisor.ts:319), lines 319–373. |
| M7 | macOS capture invokes `/usr/sbin/screencapture`; input uses CGEvent and AX helpers. Background/takeover support dynamically opens a private SkyLight framework and resolves private symbols. | Study delivery semantics and guarded fallback, but copying this private API strategy would create an OS-version compatibility burden. This is an OMP fact, not evidence of ChatGPT's implementation. [capture_window](/Users/enesolucoes/Documents/Pessoal/jarvis/docs/omp/crates/pi-natives/src/desktop/macos/capture.rs:187), lines 187–204, 312–348; [input.rs](/Users/enesolucoes/Documents/Pessoal/jarvis/docs/omp/crates/pi-natives/src/desktop/macos/input.rs:9), lines 9–14; [SkyLight loading](/Users/enesolucoes/Documents/Pessoal/jarvis/docs/omp/crates/pi-natives/src/desktop/macos/skylight.rs:197), lines 197–208. |

Relevant source-level regression contracts: [computer.test.ts](/Users/enesolucoes/Documents/Pessoal/jarvis/docs/omp/packages/coding-agent/test/tools/computer.test.ts:760), screenshot output at 760, scaled dimensions at 786, read-only input blocking at 810, leaked continuation authority at 1034, timeout recovery at 1160; [desktop.test.ts](/Users/enesolucoes/Documents/Pessoal/jarvis/docs/omp/packages/natives/test/desktop.test.ts:34), capability shape at 35, coordinates before capture at 71, calls after close at 89; [frame.rs tests](/Users/enesolucoes/Documents/Pessoal/jarvis/docs/omp/crates/pi-natives/src/desktop/frame.rs:266), Retina mapping at 266, moved-window mapping at 273, cap scaling at 278. They were not run.

## Reconstruction implications for Jarvis

The following are **recommendations**, pending the ChatGPT artifact findings and a later approved implementation scope.

1. Keep the current browser tools for DOM and CDP work. Add native desktop observation/input as a distinct Rust capability. A full OS controller cannot be created by renaming `browser_click` because its targets, coordinate frame and privileges differ.
2. Reuse `tool_contract::Orchestrator`, `Execution::allowed/filter`, approval UI, scoped grant lifetimes, conversation attachments and cancellation signals. Add app identity and observation/input boundaries explicitly; current filesystem/process/network effects and exact-tool grants do not encode native app allowlists or TCC capability state.
3. Bind each observed window/app handle and screenshot to a conversation/controller generation. Store bundle ID, PID/window ID, capture dimensions, logical bounds and scaling with the capture. Expire handles or reject input when target identity/geometry no longer matches.
4. Prefer native semantic AX operations where observable support exists, then bounded coordinate input tied to a capture. Require verified capability for each path. Do not infer successful action from an event being posted.
5. Preserve Jarvis's existing prepare/dispatch/observe distinction. Return whether input was dispatched and whether the effect was confirmed. Cancellation, disconnect and worker timeout can stop future steps but cannot safely authorize replay of an action that might already have happened.
6. Extend durable recovery with browser and desktop target relations plus explicit observed result evidence. Reuse `recovery_resolve`; add the missing relation rather than building a separate replay framework.
7. Start with a bounded controller/tool surface. The OMP persistent REPL is one design option with an additional run-authority and cleanup contract; its existence is not a requirement to add arbitrary code execution to the first native desktop iteration.

The essential flow is:

```text
user objective
  -> current role/catalog + host-owned app/target authorization
  -> observe (AX and/or capture with geometry)
  -> prepare and validate a specific action
  -> dispatch once through native Rust controller
  -> record dispatch receipt
  -> observe outcome
  -> continue, reconcile uncertainty, or report a bounded blocker
```

## Existing regression anchors for future implementation

| Concern | Current source/test anchor |
| --- | --- |
| Read-only catalogs and bounded requests | [browser/tests.rs](/Users/enesolucoes/Documents/Pessoal/jarvis/src-tauri/src/agent/browser/tests.rs:23), read-only exclusions at 23, bounds at 112, unknown fields at 143, provider-compatible schemas at 151. |
| Dispatched input and stale targets | [cdp/actions.rs](/Users/enesolucoes/Documents/Pessoal/jarvis/src-tauri/src/agent/browser/cdp/actions.rs:680), preparation without dispatch at 680; [cdp.rs](/Users/enesolucoes/Documents/Pessoal/jarvis/src-tauri/src/agent/browser/cdp.rs:572), frame correlation/expired IDs at 572, cancellation retirement at 605. |
| Disconnect/cancellation and structured diagnostics | [extension.rs](/Users/enesolucoes/Documents/Pessoal/jarvis/src-tauri/src/agent/browser/extension.rs:1195), uncertainty at 1195, bounded dispatch diagnostics at 1229, cancel-on-drop at 1250. |
| Scope-safe screenshot export | [routing.rs](/Users/enesolucoes/Documents/Pessoal/jarvis/src-tauri/src/agent/browser/routing.rs:422), bytes/no overwrite at 422, path validation at 480, symlinks at 510, parent recheck at 523. |
| Role/capability intersections | [workflow/tests.rs](/Users/enesolucoes/Documents/Pessoal/jarvis/src-tauri/src/agent/workflow/tests.rs:1600), browser mutation exclusions and retained observations at 1600–1667; [catalog/permissions.rs](/Users/enesolucoes/Documents/Pessoal/jarvis/src-tauri/src/agent/workflow/catalog/permissions.rs:8), mandatory tools and denial validation at 8–42. |
| Durable uncertain effects | [workflow/recovery.rs](/Users/enesolucoes/Documents/Pessoal/jarvis/src-tauri/src/agent/workflow/recovery.rs:833), exact durable file evidence at 833, all patch targets at 869, same-server/resource MCP evidence at 1012, independent work at 1072. A browser/desktop reconciliation fixture is still needed. |
| Frontend state ownership | [use-browser.test.ts](/Users/enesolucoes/Documents/Pessoal/jarvis/src/hooks/use-browser.test.ts:86), old-conversation result at 86, navigation during open at 103, newer Chat selection at 117, semantic/frame/wait forwarding at 139. |

## Coverage status

| Question | Status |
| --- | --- |
| What browser observation/action machinery exists in Jarvis? | Answered from source. |
| Does this pass find a first-party native desktop controller in Jarvis? | No controller found within the stated bounded search; external MCP/plugin capabilities unresolved. |
| Which existing contracts can be reused? | Answered from source; concrete integration sites above. |
| Which required references solve equivalent concerns? | Codex host admission, OpenCode generic lifecycle/permissions, OMP native controller and geometry studied. |
| How do cancellation and uncertain action effects work? | Source paths and test assertions identified; runtime behavior unverified. |
| Does restart recovery reconcile browser/desktop effects? | Browser evidence relation is absent in inspected recovery code; native desktop relation is not implemented there. |
| How does the installed ChatGPT application implement computer use? | Outside this source-mapping pass; requires the parent REA investigation. |

Skipped: runtime/native actions, tests and builds, installed external MCP/plugin inventory, and source implementation. Risk: source presence and test assertions establish contracts but do not demonstrate successful macOS permission, pixel mapping, input delivery or recovery on the user's hardware.
