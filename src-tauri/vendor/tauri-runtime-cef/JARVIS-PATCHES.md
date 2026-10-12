# Jarvis native runtime fixes

This directory vendors only `crates/tauri-runtime-cef` from the official
[Tauri repository](https://github.com/tauri-apps/tauri/tree/9d1a5b176e337c8395362a4e7c99bac99b36a1d1/crates/tauri-runtime-cef),
commit `9d1a5b176e337c8395362a4e7c99bac99b36a1d1` (runtime version `0.1.0`).
The upstream MIT and Apache-2.0 licenses are included unchanged.

The standalone manifest resolves inherited workspace metadata and dependencies
to the same versions and Git commit used by Jarvis. Other Tauri crates remain
the official pinned dependencies; the root Cargo patch replaces only this crate.

Local fixes:

- `src/runtime.rs` retains the macOS `LibraryLoader` in `CefRuntime`. The loader
  previously dropped at the end of initialization, unloading the framework
  while CEF objects and API pointers were still in use. The main loop now owns
  the loader through `cef::shutdown()` and releases it afterward.
- `src/webview.rs` makes DevTools evaluation completion safe when its callback
  and observer registration destroy the observer reentrantly. Completion now
  keeps the shared mutex alive independently of the observer, releases the
  mutex guard before invoking the callback, and drops the registration outside
  the mutex. Previously, unregistering inside a temporary guard could destroy
  the mutex before the guard unlocked it.
- `src/runtime.rs`, `src/window.rs`, and `src/window_handle.rs` retain shared
  ownership of live native windows for read-only queries on the main thread.
  AppKit notifications, libdispatch callbacks, and synchronous CEF IPC handlers
  can run outside a winit callback; queueing a getter there blocked the main
  thread waiting for its own event loop. Native handles, scale, geometry,
  monitor, title, theme, and winit window-state queries now read that registry
  directly. Worker queries keep the original message/reply path, conversions,
  and errors. Closed windows leave the registry, and native calls never hold
  its mutex or retain a callback's borrowed `ActiveEventLoop`.
- `src/window.rs` and `src/platform/macos/window.rs` share a read-only
  maximized-state query for both queued worker requests and direct native
  callbacks. The pinned Winit getter temporarily changed a borderless window's
  style mask, generating resize notifications that queried the same getter
  again and repeatedly rebuilt its native frame. Following the mature Tao
  approach, titled/resizable windows use AppKit's zoom state; other windows
  compare their frame size with the current screen's visible frame in points.
  An unavailable screen falls back to AppKit's unmodified zoom query, never
  Winit's style-changing getter. Explicit maximize/unmaximize operations keep
  the upstream setter and must be checked in native acceptance testing.
- `src/cef_impl/request_handler.rs` replaces a new OS thread per local protocol
  request with eight dispatch workers and a bounded queue of 256 requests.
  Admission never waits on CEF's IO/UI threads. Saturation or worker startup
  failure responds with HTTP 503, JSON, and the Tauri IPC/CORS error headers so
  the frontend does not replay the invocation through its fetch-failure
  fallback. IPC OPTIONS requests retain the official static CORS response
  outside the queue so a saturated preflight cannot enable that fallback.
  An async protocol handler releases its worker when dispatch returns;
  retaining its responder does not reserve a worker until the reply arrives.
  Panicking handlers reply once with HTTP 500 and leave the pool usable.
- `src/cef_impl/client/life_span.rs` replaces the blank-page reload loop with
  one delayed CEF UI task. It retries only an idle blank document, never a
  navigation already loading or the inert document awaiting initialization.
  The previous 100ms loop could repeatedly abort a slow initial local response,
  replaying startup IPC and causing flicker while blocked requests accumulated.

The native browser probe exercises the actual bundled framework. Its source
fixture also guards framework ownership and the shutdown-before-unload order
without initializing AppKit or CEF on the unit test worker thread.

The observer regression fixture can be run from `src-tauri` without opening a
browser:

```sh
cargo test -p tauri-runtime-cef --lib completion_keeps_the_mutex_alive_when_callback_and_unregister_destroy_the_observer --locked
```

The main-thread getter fixtures use a bounded message/reply probe and shared
window ownership, without initializing any native event loop:

```sh
cargo test -p tauri-runtime-cef --lib native_getter --locked
cargo test -p tauri-runtime-cef --lib native_callbacks_read_window_geometry_and_monitors_without_queueing --locked
cargo test -p tauri-runtime-cef --lib native_position_errors_and_worker_queue_behavior_are_preserved --locked
cargo test -p tauri-runtime-cef --lib maximized_state_tests --locked
```

The protocol-pressure and startup-navigation fixtures avoid native CEF/AppKit:

```sh
cargo test -p tauri-runtime-cef --lib protocol_dispatch_tests --locked
cargo test -p tauri-runtime-cef --lib blank_recovery --locked
```

Unit fixtures and source review do not establish native browser acceptance;
the bundled probe and interactive validation are separate checks.
