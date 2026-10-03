# Jarvis browser extension

Manifest V3 extensions for Chrome, Edge, Brave and compatible Chromium browsers (125+), and Firefox (140+). Both connect to Jarvis over an authenticated loopback WebSocket. Chromium uses its debugger API; Firefox uses WebExtensions scripting, tabs and webRequest APIs. No remote debugging port or separate Node service is required.

## Build and install externally

1. Run `bun install` and `bun run build:extension` from the repository root. Run the extension build before invoking Cargo directly on a clean checkout; Tauri includes these files as resources. `bun run dev`, `bun run build`, and the release workflow already build them.
2. In Jarvis, select **Configurações → Navegador → Extensão do navegador**, choose your browser, and prepare the extension. Jarvis copies that browser's packaged assets to a separate application data directory and supplies the connection code.
3. For Chromium, open `chrome://extensions` (or the equivalent page), enable developer mode, select **Carregar sem compactação**, and choose the directory supplied by Jarvis. For Firefox, open `about:debugging#/runtime/this-firefox`, select **Carregar extensão temporária**, and choose that directory's `manifest.json`. Grant site access if requested. Development output is `browser-extension/dist` for Chromium and `browser-extension/dist-firefox` for Firefox.
4. Open the extension's options from its toolbar action, paste the connection code, and connect. Keep that code private; it grants access to this local Jarvis connection.
5. After updating Jarvis or rebuilding the extension, prepare the packaged files again and reload the extension on the browser's extensions page. External installations cannot silently replace a running extension.

Firefox temporary installations disappear when the browser restarts. Permanent external installation in standard Firefox requires Mozilla signing, including for self-distribution without a public store listing. The current build is unsigned; do not treat its directory or an unsigned XPI as a permanent installer. When changing a paired browser/profile, revoke the old connection in Jarvis and generate a new pairing code.

The Firefox manifest declares `browsingActivity`, `websiteContent`, and `websiteActivity` under required data collection permissions. Mozilla's [data taxonomy](https://extensionworkshop.com/documentation/develop/firefox-builtin-data-consent/) includes data handled outside the extension or local browser, even when the immediate destination is the paired local Jarvis application. Firefox's installation prompt provides consent for these categories; the extension options explain that available tab URLs and connected page content, interactions, and diagnostics reach Jarvis and may be processed by its configured AI providers. The extension does not contact an AI provider directly or add telemetry.

## Behavior and limits

- Opening a page creates a new tab. Existing personal tabs are only adopted explicitly; each belongs to one conversation. Cleanup and disconnection detach them without closing them.
- Chromium displays its own debugger notice. Opening DevTools can detach the debugger; close DevTools and reconnect that tab when ready. Firefox does not use this debugger connection.
- Worker suspension preserves ownership in session storage. Restarting the browser changes its session epoch, invalidating old handles. Reconnection never repeats a request. If a connection drops while an action is executing, inspect the page before repeating it.
- Console and network capture begin after attachment and keep the last 200 entries per tab. Network lists exclude bodies; inspect a selected response separately. Chromium fetches it on demand (up to 64,000 characters); Firefox retains a bounded copy during capture (up to 64,000 bytes). Truncation is reported.
- Screenshots capture the page viewport, not browser chrome or the desktop. Snapshots include semantic names, roles, disabled/checked/expanded states and open shadow DOM. They are scoped to one document, expose a frame catalog, and support `frameId`, `offset` and `limit`. Cross-origin out-of-process frames use child debugger sessions. Closed shadow roots are unavailable.
- Interactions accept a current element ID or one unique semantic locator (`role` with optional `name`, `label`, `text`, or `testId`). Matching is exact by default; `exact: false` allows partial names/text. Replaced snapshot elements can be resolved again only when an equivalent target is unique. Ambiguity never chooses the first match.
- Before sending input, actions wait for visibility/enabled/editable state; pointer actions also wait for stable geometry and verify the hit target. Nested frame clicks translate coordinates and check ancestor frames. Transformed/zoomed frame clicks return an explicit preflight limitation instead of guessing coordinates; scoped CDP remains available for inspection.
- `browser_wait` confirms document readiness or an explicit element state without sending input. Action waits default to 5 seconds and allow a caller-selected deadline up to 15 seconds (`timeoutMs: 0` checks once). These are browser-operation deadlines, not limits on the agent's overall task.
- Click guards prevent mismatched pointer targets, with automatic cleanup after interruption. Fill/press verify focus before input. Successful input dispatch is not proof that the site's operation succeeded: confirm the expected result with wait/snapshot. An interrupted or partially dispatched action is never replayed automatically. Error diagnostics identify preparation versus dispatch and the elapsed time.
- Snapshots inspect up to 5,000 DOM elements per document and report truncation. Locators do not claim uniqueness from a truncated scan. Console/network buffers remain bounded, including child-frame requests. Chromium response bodies are fetched on demand; Firefox response copies stay bounded in memory.

- Protected browser/store pages are unavailable. Generic CDP is confined to page domains and excludes browser management, persistent scripts, file upload, global cookie operations and download-policy changes. Text fill supports editable fields and native selects, not file inputs.
- Each conversation can own up to 12 tabs. Explicitly closing a connected tab closes it; deleting a conversation only releases ownership.
- Pairing credentials stay in trusted extension storage and Jarvis's private pairing file, outside preference backups. The setup screen never exposes credentials to page JavaScript.

## Reference decisions

### Firefox-specific behavior

- The background event page uses module scripts rather than Chrome's service worker. Session storage preserves tab ownership and handles across background restarts; alarms reconnect without replaying requests.
- Screenshots use `tabs.captureTab`, so capturing a background tab does not activate it. DOM snapshots, semantic locators, actionability waits, native select filling and frame-scoped inspection reuse the common page contract.
- Firefox actions use DOM activation and synthetic input events. Sites requiring trusted events can reject them; successful dispatch does not prove the action succeeded. Unsupported browser keyboard defaults return an explicit error instead of claiming to send trusted keys.
- Console capture wraps the controlled page's console from attachment onward. Earlier logs, worker logs and browser-internal diagnostics are unavailable. Page scripts can modify these logs; they remain untrusted evidence.
- Network capture observes only conversation-owned tabs. Response filters pass original bytes through immediately and keep bounded copies of the last 200 requests, up to 64,000 bytes per response. Streaming responses must finish before body inspection; unavailable bodies never cause a request to be repeated.
- Arbitrary evaluation runs in the page's MAIN world and respects its CSP. CDP commands are unavailable: use snapshot, console, network, response bodies, screenshots and evaluation instead. Internal and Mozilla-protected pages cannot be controlled.

API and distribution references: [background pages](https://developer.mozilla.org/en-US/docs/Mozilla/Add-ons/WebExtensions/manifest.json/background), [response filters](https://developer.mozilla.org/en-US/docs/Mozilla/Add-ons/WebExtensions/API/webRequest/filterResponseData), [captureTab](https://developer.mozilla.org/en-US/docs/Mozilla/Add-ons/WebExtensions/API/tabs/captureTab), [signing and self-distribution](https://extensionworkshop.com/documentation/publish/signing-and-distribution-overview/).

The reliability contracts follow Playwright's actionability, semantic locators, frame coordinate translation and hit-target interception patterns. The implementation is independent; no Playwright runtime, MCP server or additional browser installation is required. Jarvis retains conversation ownership, authenticated loopback transport and its existing native browser tools. Automated fixtures exercise replacement, ambiguity, overlays, focus, child sessions and cancellation; this does not provide the complete Playwright test runner, browser isolation, videos or cross-browser testing.

Focused validation: `bunx vitest run browser-extension/browser.test.ts browser-extension/page.test.ts browser-extension/frames.test.ts`, `bunx tsc -p tsconfig.extension.json`, and `bun run build:extension`. The complete application gates also include extension lint, tests, types and packaging.
