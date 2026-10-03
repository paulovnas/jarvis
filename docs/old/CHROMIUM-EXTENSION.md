# Chromium browser integration

Jarvis can use its embedded browser or connect to a locally installed Chromium extension. The default remains embedded. Both API agents and the Claude executor use the same native `browser_*` tool dispatcher.

## External installation

1. Open **Configurações → Navegador** and select **Extensão Chromium**.
2. Select an installed Chrome, Edge, Brave, or Chromium application.
3. Choose **Preparar extensão**. Jarvis copies bundled files to a stable application-data folder. Open or copy that folder from the screen.
4. In the browser's extensions page (`chrome://extensions`, `edge://extensions`, or `brave://extensions`), enable developer mode and choose **Load unpacked / Carregar sem compactação**. Select the prepared folder.
5. Open the Jarvis extension's options, paste the connection code copied from Jarvis, and connect. Keep the browser open.

The extension requires Chromium 125 or later. It is not published in an extension store. Enterprise policy may prohibit unpacked extensions. Chrome's debugger permission/banner is controlled by the browser.

After updating Jarvis, prepare the extension again and reload it from the browser's extensions page. Its folder remains stable so the unpacked extension identity and pairing can be preserved. If the local port changes or the connection is revoked, copy a fresh connection code.

## Available behavior

- Open tabs or a new browser window, navigate, reload, go back/forward, focus and explicitly close tabs.
- Discover existing tabs and explicitly link one to a conversation. New tabs never replace the user's focused personal tab.
- Read page text and actionable element IDs; click, fill fields, select options, press keys, and scroll.
- Capture a page viewport as a normal Jarvis image attachment, available to vision and the chat UI.
- Read console messages and network summaries with filtering/pagination; request an individual response body when needed.
- Evaluate JavaScript and use scoped CDP commands for inspection, emulation and debugging on the linked page.
- Use the browser panel to link/focus tabs and inspect console, network and screenshots. Browser-required pages remain external, not embedded inside the Jarvis window.

The selected mode controls new tabs. Existing tab IDs retain their original backend even after switching the default. Each external tab belongs to one conversation; its subagents share that conversation's ownership. Deleting a conversation releases the debugger and ownership without closing browser tabs. An explicit agent request to open a new tab can launch the configured browser when it has already been paired; status reads do not launch applications.

## Recovery and boundaries

Jarvis owns an authenticated loopback WebSocket server. A connection code authorizes one extension installation/profile. Both an exact extension origin and a secret token are verified; the token is stored separately from exported preferences. Revoke from Jarvis or disconnect from the extension to end access.

The extension reconnects after temporary transport loss. Operations are not replayed. If a mutation's result is lost, the tool reports an unknown outcome and requires inspection before another attempt. Tab handles include a browser-session epoch so a browser restart cannot silently retarget an old ID.

Opening DevTools or another debugger can detach Jarvis. Close the competing debugger and link the same tab again. There is no silent fallback to the embedded browser when the extension is unavailable.

Console/network capture starts when a tab is attached and uses bounded buffers; it is not a complete historical trace. Body and generic CDP results explicitly indicate truncation when applicable. Screenshots show the page viewport, not the operating system or browser toolbar. Standard snapshots cover the top document; frame/shadow-DOM inspection can use scoped CDP.

Internal browser pages, the extension store and other extension pages cannot be controlled as normal pages. Browser-wide target management, unrestricted file uploads, persistent page-injection configuration and profile-wide cookie commands are not exposed through generic CDP. Downloads remain subject to the browser's normal behavior. No remote-debugging flags or separate Node service are used by the product.

## Development and validation

Run `bun install` and `bun run build:extension` before directly running Cargo in a clean checkout. `bun run dev` and `bun run build` prepare the extension automatically; release validation includes that build before the Rust bundle. Generated `browser-extension/dist` assets are ignored by Git and bundled as Tauri resources.

`bun run check` includes extension type checking and Vitest coverage alongside the application gates. Rust validation uses `cargo clippy --all-targets -- -D warnings` and `cargo test` from `src-tauri`.

A real isolated Chrome for Testing smoke on macOS verified MV3 loading, options/CSP, WebSocket connection, tabs, DOM inspection, field replacement, clicks, JavaScript, console, network/body, PNG capture, conversation isolation, stale-element rejection, scoped CDP rejection, non-destructive cleanup and reconnection. This does not certify interactive installation/launch on Windows/Linux or every Chromium variant. Those native installation and launch paths require platform testing.

Wire formats are defined in [CHROMIUM-EXTENSION-PROTOCOL.md](CHROMIUM-EXTENSION-PROTOCOL.md); research and source links are in [CHROMIUM-EXTENSION-RESEARCH.md](CHROMIUM-EXTENSION-RESEARCH.md).
