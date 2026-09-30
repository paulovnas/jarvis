# Jarvis browser extension

Manifest V3 extension for Chrome, Edge, Brave and compatible Chromium browsers (125+). It connects to Jarvis over an authenticated loopback WebSocket and uses the browser's debugger API. No remote debugging port, separate Node service or store publication is required.

## Build and install externally

1. Run `bun install` and `bun run build:extension` from the repository root. Run the extension build before invoking Cargo directly on a clean checkout; Tauri includes these files as resources. `bun run dev`, `bun run build`, and the release workflow already build them.
2. In Jarvis, select **Configurações → Navegador → Extensão Chromium** and prepare the extension. Jarvis copies the packaged assets to its application data directory and supplies the connection code.
3. Open `chrome://extensions` (or the equivalent page in your Chromium browser), enable developer mode, select **Carregar sem compactação**, and choose the directory supplied by Jarvis. For development, use `browser-extension/dist` directly.
4. Open the extension's options from its toolbar action, paste the connection code, and connect. Keep that code private; it grants access to this local Jarvis connection.
5. After updating Jarvis or rebuilding the extension, prepare the packaged files again and reload the extension on the browser's extensions page. External installations cannot silently replace a running extension.

## Behavior and limits

- Opening a page creates a new tab. Existing personal tabs are only adopted explicitly; each belongs to one conversation. Cleanup and disconnection detach them without closing them.
- The browser displays its own debugger notice. Opening DevTools can detach the debugger; close DevTools and reconnect that tab when ready. Jarvis does not compete for the debugger.
- Worker suspension preserves ownership in session storage. Restarting the browser changes its session epoch, invalidating old handles. Reconnection never repeats a request. If a connection drops while an action is executing, inspect the page before repeating it.
- Console and network capture begin after attachment and keep the last 200 entries per tab. Network lists exclude bodies; fetch a selected response body separately (up to 64,000 characters, with truncation reported).
- Screenshots capture the page viewport, not browser chrome or the desktop. Snapshots include semantic names, roles, disabled/checked/expanded states and open shadow DOM. They are scoped to one document, expose a frame catalog, and support `frameId`, `offset` and `limit`. Cross-origin out-of-process frames use child debugger sessions. Closed shadow roots are unavailable.
- Interactions accept a current element ID or one unique semantic locator (`role` with optional `name`, `label`, `text`, or `testId`). Matching is exact by default; `exact: false` allows partial names/text. Replaced snapshot elements can be resolved again only when an equivalent target is unique. Ambiguity never chooses the first match.
- Before sending input, actions wait for visibility/enabled/editable state; pointer actions also wait for stable geometry and verify the hit target. Nested frame clicks translate coordinates and check ancestor frames. Transformed/zoomed frame clicks return an explicit preflight limitation instead of guessing coordinates; scoped CDP remains available for inspection.
- `browser_wait` confirms document readiness or an explicit element state without sending input. Action waits default to 5 seconds and allow a caller-selected deadline up to 15 seconds (`timeoutMs: 0` checks once). These are browser-operation deadlines, not limits on the agent's overall task.
- Click guards prevent mismatched pointer targets, with automatic cleanup after interruption. Fill/press verify focus before input. Successful input dispatch is not proof that the site's operation succeeded: confirm the expected result with wait/snapshot. An interrupted or partially dispatched action is never replayed automatically. Error diagnostics identify preparation versus dispatch and the elapsed time.
- Snapshots inspect up to 5,000 DOM elements per document and report truncation. Locators do not claim uniqueness from a truncated scan. Console/network buffers remain bounded, including child-frame requests; response bodies are fetched only when requested.

- Protected browser/store pages are unavailable. Generic CDP is confined to page domains and excludes browser management, persistent scripts, file upload, global cookie operations and download-policy changes. Text fill supports editable fields and native selects, not file inputs.
- Each conversation can own up to 12 tabs. Explicitly closing a connected tab closes it; deleting a conversation only releases ownership.
- Pairing credentials stay in trusted extension storage and Jarvis's private pairing file, outside preference backups. The setup screen never exposes credentials to page JavaScript.

## Reference decisions

The reliability contracts follow Playwright's actionability, semantic locators, frame coordinate translation and hit-target interception patterns. The implementation is independent; no Playwright runtime, MCP server or additional browser installation is required. Jarvis retains conversation ownership, authenticated loopback transport and its existing native browser tools. Automated fixtures exercise replacement, ambiguity, overlays, focus, child sessions and cancellation; this does not provide the complete Playwright test runner, browser isolation, videos or cross-browser testing.

Focused validation: `bunx vitest run browser-extension/browser.test.ts browser-extension/page.test.ts browser-extension/frames.test.ts`, `bunx tsc -p tsconfig.extension.json`, and `bun run build:extension`. The complete application gates also include extension lint, tests, types and packaging.
