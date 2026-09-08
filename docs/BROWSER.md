# Native browser workspace

## Design

Browser tabs share the center workspace with the permanent Chat tab and read-only files. Each conversation owns its tabs. The composer globe opens a blank tab and focuses its address field; the address bar supports HTTP(S), back, forward, reload, console inspection and viewport capture. Tauri's native engines are WebView2 on Windows, WKWebView on macOS and WebKitGTK on Linux. This implementation uses Tauri 2.11's multi-webview API (`unstable`, pinned through Cargo.lock). [Tauri architecture](https://github.com/tauri-apps/tauri/blob/dev/README.md)

The Metis reference (`docs/metis/desktop/main.cjs`, guest-webview attachment and browser session setup) informed the isolation boundary: web content is untrusted, only HTTP(S) navigation is accepted and browser content receives no application capabilities. Jarvis additionally rejects application command IPC from every webview except `main`. Browser JavaScript has no filesystem or shell bridge. DOM observation/action results return through native evaluation callbacks. Logs are bounded and treated as page-controlled data.

Tabs restore metadata, not running page state. The selected tab and URLs persist in `browser-tabs.json` under Tauri's application data directory. Native views are created lazily; leaving a conversation hides its views. The frontend reports viewport bounds and hides native content while an application overlay is open, avoiding native child windows covering dialogs. Closing a tab destroys its native view; deleting a conversation prunes its browser records and views. Browser cookies use an isolated ephemeral profile rather than the privileged application's profile.

Agent browser tools share the root conversation's tabs. Navigation and interactions require the existing tool approval policy, and read-only agents can only observe existing tabs. Requests for approval or user input bring Chat back into view. Screenshots become conversation image attachments, with enlarge/save controls in the transcript; the existing Vision tool inspects them with the configured image-capable model. Page text, console messages and screenshots are evidence, never instructions or authorization.

## Agent tools

| Tool | Behavior |
| --- | --- |
| `browser_list` | List conversation-owned tabs and current selection. |
| `browser_open`, `browser_navigate`, `browser_close` | Open, navigate or close a tab. |
| `browser_snapshot` | Read bounded page text and current interactive element IDs. |
| `browser_click`, `browser_fill`, `browser_press`, `browser_scroll` | Interact with current elements or scroll the viewport. |
| `browser_console` | Read bounded console output, errors and unhandled rejections. |
| `browser_screenshot` | Capture native pixels into an image attachment for `vision`. |

Element IDs include a document nonce and snapshot revision. Navigation or a new snapshot invalidates older targets. Fill uses native field setters plus input/change events so React-controlled forms receive changes. Password/file fields require user interaction; custom JavaScript evaluation is not exposed as an agent tool.

Windows capture uses WebView2 `CapturePreview` and a bounded COM memory stream. A hidden controller is temporarily rendered outside the client area because WebView2 otherwise never completes capture; a drop guard restores its hidden state on success, timeout and cancellation. WKWebView uses `takeSnapshotWithConfiguration`, and WebKitGTK uses its visible-region snapshot API. [WKWebView snapshot API](https://docs.rs/objc2-web-kit/latest/objc2_web_kit/struct.WKWebView.html#method.takeSnapshotWithConfiguration_completionHandler)

## Current limits

- Twelve tabs per conversation and twelve simultaneously loaded native views across the application.
- DOM tools inspect the top document; iframe contents, closed shadow roots, native OS dialogs and browser-reserved shortcuts are outside this API. Keyboard dispatch is a DOM event, not OS input.
- Downloads and non-HTTP(S) navigation are blocked. New-window links navigate the current browser tab.
- Console history resets on navigation and retains at most 150 entries (100 returned per read). Snapshot text is limited to 24,000 characters and 300 interactive elements.
- Tab metadata survives app restart; login cookies and unsaved page state are ephemeral.
- Captures require the Jarvis window to be visible and restored. Background tabs can still be captured while Chat or another tab is active. Screenshots are normalized by the existing attachment pipeline (up to 2048 pixels).
- Native macOS and Linux execution must be verified on those hosts; local Windows tests do not establish parity.

## Validation

The opt-in probe uses an isolated application identifier and a disposable localhost fixture. It does not touch actual provider accounts or chat histories:

```sh
cargo run --example browser-probe --features browser-probe
```

It exercises native load, metadata restoration, conversation ownership, window resize, text/element snapshots, form interaction, console logs, fresh foreground/background screenshots, application/plugin IPC rejection, navigation/back, stale element rejection and closing. Evidence is written to `.codex/browser-review/native-report.json` and PNG files. The probe is excluded from normal builds.

Frontend acceptance covers tab persistence, non-closable Chat, draft preservation, closing inactive tabs, normalized URLs, console/capture controls, screenshot enlargement/save and surfacing agent approvals. UI layout was also inspected in the in-app browser using the actual components and fixture data; native pages were verified separately in WebView2.

On 2026-09-08, `CI=1 bun run check` passed lint, type checking, 417 frontend tests and the production web build. Three existing tests remained skipped. `cargo clippy --all-targets --features browser-probe -- -D warnings` passed without diagnostics. `cargo test -- --test-threads=1 --quiet` passed 401 native tests; sixteen existing tests remained ignored. The final native browser probe also passed. Serial native execution and the repository's existing CI worker limit avoid contention in subprocess-heavy tests.

The Windows linker still emits the pre-existing localized library-creation informational message tracked in `jarvis-9au`; it is not a Clippy diagnostic. WebView2 may print `Chrome_WidgetWin_0` unregistration error 1412 during probe teardown, after all assertions have passed.

Beads: implementation `jarvis-yl9.1`, UI `jarvis-yl9.2`, agent integration and delivery `jarvis-yl9.3`. The epic `jarvis-yl9` retains native macOS/Linux validation in `jarvis-yl9.4`; those engines cannot be executed on this Windows host. No live provider was changed and no paid inference was used for validation. No commit, push or remote Beads synchronization was performed for this browser feature.

## Manual acceptance

1. Open a chat and click the globe beside the terminal button. A new central browser tab should open with the address field focused; Chat has no close button.
2. Enter a project development URL, for example `localhost:3000`. Use links and forms, then check back, forward and reload. Open the console panel and capture the viewport from the toolbar.
3. Return to Chat, switch conversations and return. Browser tab metadata and the selected tab should restore; the Chat draft should be preserved. Closing an inactive browser tab should close that tab only.
4. Open an application dialog while a page is visible. Native page content should hide behind the dialog and return after it closes. Resize the Jarvis window and both sidebars; the page should continue filling its available area.
5. Ask an agent with browsing permissions to open the local application, take a snapshot, fill/click a test form, read console errors and capture a screenshot. Tool approvals should bring Chat into view. Screenshots should appear in the transcript with enlargement/save controls and be available to the existing Vision tool.
6. Restart Jarvis. Previously saved tab URLs should return; ephemeral login state is intentionally not restored.

## Windows delivery (2026-09-08)

This records the browser-feature build. The later [workflow polish delivery](WORKFLOW-POLISH.md) supersedes these local artifacts.

`bun run tauri build --ci --bundles nsis --no-sign` completed successfully. The unsigned release and installer include the browser feature alongside the earlier workflow, provider-removal, terminal and Explorer changes in this checkout.

- Executable: `src-tauri/target/release/jarvis.exe`.
- Executable SHA-256: `2ECF9BC04D7D896A1D36FB13130C43C264AECCC9AABF835084D11386FD4910C7`.
- Installer: `src-tauri/target/release/bundle/nsis/Jarvis_0.8.5-beta_x64-setup.exe`.
- Installer SHA-256: `3A6ED1D1F2D7C9729B43610E45FCBEB20088644F10940AA2AEF3E5644A5508ED`.

The updated executable was launched directly and its Jarvis window was confirmed responsive. The installer was generated but was not run over the user's installation.
