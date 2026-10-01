//! Disposable desktop presentation. Agent execution remains owned by the runtime.
use crate::desktop::CompanionPosition;
use crate::openai_codex::usage::AccountUsage;
use serde::Serialize;
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Mutex,
};
use std::time::Duration;
use tauri::{Emitter, Listener, Manager, PhysicalPosition, PhysicalSize};

pub(crate) const SUPPORTED: bool = cfg!(any(target_os = "macos", target_os = "windows"));
const LABEL: &str = "companion";
const PET_WIDTH: f64 = 96.0;
const PET_HEIGHT: f64 = 112.0;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Geometry {
    expanded: bool,
    bubble: bool,
    width: f64,
    height: f64,
    robot_side: &'static str,
    robot_vertical: &'static str,
}

#[derive(Clone, Copy)]
struct Screen {
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    scale: f64,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Presentation {
    Compact,
    Bubble,
    Island,
}

struct Placement {
    anchor: CompanionPosition,
    position: PhysicalPosition<i32>,
    size: PhysicalSize<u32>,
    geometry: Geometry,
}

fn placement(
    anchor: Option<CompanionPosition>,
    screens: &[Screen],
    mode: Presentation,
) -> Option<Placement> {
    let screen = screens
        .iter()
        .find(|s| {
            anchor.is_some_and(|p| {
                i64::from(p.x) >= i64::from(s.x)
                    && i64::from(p.x) < i64::from(s.x) + i64::from(s.width)
                    && i64::from(p.y) >= i64::from(s.y)
                    && i64::from(p.y) < i64::from(s.y) + i64::from(s.height)
            })
        })
        .or_else(|| screens.first())?;
    let scale = screen.scale;
    let pet_width = (PET_WIDTH * scale).round().max(1.0) as u32;
    let pet_height = (PET_HEIGHT * scale).round().max(1.0) as u32;
    let x_max = i64::from(screen.x) + i64::from(screen.width.saturating_sub(pet_width));
    let y_max = i64::from(screen.y) + i64::from(screen.height.saturating_sub(pet_height));
    let default = CompanionPosition {
        x: (x_max - (24.0 * scale) as i64).max(i64::from(screen.x)) as i32,
        y: (y_max - (24.0 * scale) as i64).max(i64::from(screen.y)) as i32,
    };
    let anchor = anchor.unwrap_or(default);
    let x = i64::from(anchor.x).clamp(i64::from(screen.x), x_max);
    let y = i64::from(anchor.y).clamp(i64::from(screen.y), y_max);
    let (width, height) = match mode {
        Presentation::Compact => (pet_width, pet_height),
        Presentation::Bubble => (
            (360.0 * scale).round() as u32,
            (300.0 * scale).round() as u32,
        ),
        Presentation::Island => (
            (460.0 * scale).round() as u32,
            (600.0 * scale).round() as u32,
        ),
    };
    let left = x - i64::from(screen.x);
    let right = x_max - x;
    let above = y - i64::from(screen.y);
    let below = y_max - y;
    let on_right = left >= right;
    let on_bottom = above >= below;
    // Every surface grows into the available space around the same robot origin.
    // Near the screen center, reduce the surface rather than moving the pet.
    let width = width
        .min(screen.width)
        .min(pet_width.saturating_add(left.max(right) as u32));
    let height = height
        .min(screen.height)
        .min(pet_height.saturating_add(above.max(below) as u32));
    let position = PhysicalPosition::new(
        (x - if on_right {
            i64::from(width.saturating_sub(pet_width))
        } else {
            0
        })
        .clamp(
            i64::from(screen.x),
            i64::from(screen.x) + i64::from(screen.width - width),
        ) as i32,
        (y - if on_bottom {
            i64::from(height.saturating_sub(pet_height))
        } else {
            0
        })
        .clamp(
            i64::from(screen.y),
            i64::from(screen.y) + i64::from(screen.height - height),
        ) as i32,
    );
    Some(Placement {
        anchor: CompanionPosition {
            x: x as i32,
            y: y as i32,
        },
        position,
        size: PhysicalSize::new(width, height),
        geometry: Geometry {
            expanded: mode == Presentation::Island,
            bubble: mode == Presentation::Bubble,
            width: f64::from(width) / scale,
            height: f64::from(height) / scale,
            robot_side: if on_right { "right" } else { "left" },
            robot_vertical: if on_bottom { "bottom" } else { "top" },
        },
    })
}

#[derive(Default)]
pub(crate) struct CompanionState {
    enabled: AtomicBool,
    worker_started: AtomicBool,
    wake: tokio::sync::Notify,
    expanded: AtomicBool,
    collapse_requested: AtomicBool,
    dragging: AtomicBool,
    bubble: AtomicBool,
    applying_geometry: AtomicBool,
    revision: AtomicU64,
    anchor: Mutex<Option<CompanionPosition>>,
    error: Mutex<Option<String>>,
    usage: Mutex<Vec<AccountUsage>>,
    #[cfg(target_os = "windows")]
    expected_position: Mutex<Option<PhysicalPosition<i32>>>,
    #[cfg(target_os = "windows")]
    pointer_worker_started: AtomicBool,
    #[cfg(target_os = "windows")]
    pointer_wake: tokio::sync::Notify,
    #[cfg(target_os = "macos")]
    panel: Mutex<Option<macos::Panel>>,
}

impl CompanionState {
    #[cfg(any(target_os = "macos", target_os = "windows", test))]
    fn request_external_collapse(&self) -> bool {
        self.expanded.load(Ordering::SeqCst)
            && !self.dragging.load(Ordering::SeqCst)
            && !self.collapse_requested.swap(true, Ordering::SeqCst)
    }
}

pub(crate) fn error(app: &tauri::AppHandle) -> Option<String> {
    app.state::<CompanionState>().error.lock().ok()?.clone()
}

pub(crate) fn setup(app: &tauri::AppHandle) {
    if !SUPPORTED {
        return;
    }
    for event in ["workflow:changed", "library:changed"] {
        let app_handle = app.clone();
        app.listen(event, move |_| {
            crate::agent::companion::changed(&app_handle)
        });
    }
    configure(app);
}

pub(crate) fn configure(app: &tauri::AppHandle) {
    let state = app.state::<CompanionState>();
    let enabled = SUPPORTED
        && app
            .state::<crate::system::SystemState>()
            .preferences()
            .is_ok_and(|preferences| preferences.companion_enabled);
    let result = if enabled {
        create(app)
    } else {
        state.expanded.store(false, Ordering::SeqCst);
        state.collapse_requested.store(false, Ordering::SeqCst);
        state.dragging.store(false, Ordering::SeqCst);
        state.bubble.store(false, Ordering::SeqCst);
        #[cfg(target_os = "macos")]
        macos::close(app, true);
        if let Some(window) = app.get_webview_window(LABEL) {
            let _ = window.close();
        }
        state.expanded.store(false, Ordering::SeqCst);
        Ok(())
    };
    let enabled = enabled && result.is_ok();
    state.enabled.store(enabled, Ordering::SeqCst);
    if let Ok(mut error) = state.error.lock() {
        *error = result.err();
    }
    if enabled && !state.worker_started.swap(true, Ordering::SeqCst) {
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            loop {
                let state = app.state::<CompanionState>();
                if state.enabled.load(Ordering::SeqCst) {
                    let usage = refresh_usage(&app).await;
                    if state.enabled.load(Ordering::SeqCst) {
                        if let Ok(mut cache) = state.usage.lock() {
                            *cache = usage;
                        }
                        let _ = app.emit_to(LABEL, "companion:usage", ());
                    }
                }
                tokio::select! {
                    _ = state.wake.notified() => {},
                    _ = tokio::time::sleep(Duration::from_secs(61)) => {},
                }
            }
        });
    }
    state.wake.notify_one();
    #[cfg(target_os = "windows")]
    windows::start_pointer_watch(app);
}

#[cfg(target_os = "macos")]
fn screens(_window: &tauri::WebviewWindow) -> Result<Vec<Screen>, String> {
    Ok(macos::screens())
}

#[cfg(not(target_os = "macos"))]
fn screens(window: &tauri::WebviewWindow) -> Result<Vec<Screen>, String> {
    let mut monitors = window.available_monitors().map_err(|e| e.to_string())?;
    if let Some(primary) = window.primary_monitor().map_err(|e| e.to_string())? {
        monitors.sort_by_key(|monitor| monitor.position() != primary.position());
    }
    Ok(monitors
        .iter()
        .map(|monitor| {
            let area = monitor.work_area();
            Screen {
                x: area.position.x,
                y: area.position.y,
                width: area.size.width,
                height: area.size.height,
                scale: monitor.scale_factor(),
            }
        })
        .collect())
}

fn create(app: &tauri::AppHandle) -> Result<(), String> {
    if app.get_webview_window(LABEL).is_some() {
        return Ok(());
    }
    let state = app.state::<CompanionState>();
    *state
        .anchor
        .lock()
        .map_err(|_| "Posição do assistente indisponível.")? = app
        .state::<crate::desktop::DesktopState>()
        .companion_position();
    let development_origin = app.config().build.dev_url.clone();
    let builder =
        tauri::WebviewWindowBuilder::new(app, LABEL, tauri::WebviewUrl::App("index.html".into()))
            .on_navigation(move |url| local_navigation(url, development_origin.as_ref()))
            .title("Jarvis · Assistente flutuante")
            .inner_size(PET_WIDTH, PET_HEIGHT)
            .transparent(true)
            .background_color(tauri::utils::config::Color(0, 0, 0, 0))
            .decorations(false)
            .resizable(false)
            .always_on_top(true)
            .skip_taskbar(true)
            .focused(false)
            .focusable(false)
            .visible(false)
            .shadow(false);
    #[cfg(target_os = "macos")]
    let builder = builder
        .visible_on_all_workspaces(true)
        .accept_first_mouse(true);
    let window = builder.build().map_err(|e| e.to_string())?;
    if let Err(error) = apply_geometry(app, false) {
        #[cfg(target_os = "macos")]
        macos::close(app, true);
        let _ = window.close();
        return Err(error);
    }
    // The renderer positions its first frame before revealing the transparent window.
    Ok(())
}

fn show_nonactivating(window: &tauri::WebviewWindow) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        macos::show(window.app_handle())
    }
    #[cfg(not(target_os = "macos"))]
    window.show().map_err(|e| e.to_string())
}

fn local_navigation(url: &tauri::Url, development: Option<&tauri::Url>) -> bool {
    (url.scheme() == "tauri" && url.host_str() == Some("localhost"))
        || (matches!(url.scheme(), "http" | "https") && url.host_str() == Some("tauri.localhost"))
        || (cfg!(debug_assertions)
            && development.is_some_and(|origin| origin.origin() == url.origin()))
}

fn apply_geometry(app: &tauri::AppHandle, expanded: bool) -> Result<Geometry, String> {
    let window = app
        .get_webview_window(LABEL)
        .ok_or("Assistente flutuante indisponível.")?;
    let state = app.state::<CompanionState>();
    let anchor = *state
        .anchor
        .lock()
        .map_err(|_| "Posição do assistente indisponível.")?;
    let mode = if expanded {
        Presentation::Island
    } else if state.bubble.load(Ordering::SeqCst) {
        Presentation::Bubble
    } else {
        Presentation::Compact
    };
    let placement =
        placement(anchor, &screens(&window)?, mode).ok_or("Nenhuma tela disponível.")?;
    *state
        .anchor
        .lock()
        .map_err(|_| "Posição do assistente indisponível.")? = Some(placement.anchor);
    state.expanded.store(expanded, Ordering::SeqCst);
    if !expanded {
        state.collapse_requested.store(false, Ordering::SeqCst);
    }
    #[cfg(target_os = "macos")]
    macos::set_geometry(&window, &placement)?;
    #[cfg(not(target_os = "macos"))]
    {
        #[cfg(target_os = "windows")]
        {
            *state
                .expected_position
                .lock()
                .map_err(|_| "Posição do assistente indisponível.")? =
                (window.outer_position().map_err(|error| error.to_string())? != placement.position)
                    .then_some(placement.position);
            state.pointer_wake.notify_one();
        }
        window
            .set_position(placement.position)
            .map_err(|e| e.to_string())?;
        window.set_size(placement.size).map_err(|e| e.to_string())?;
        if !expanded {
            window.set_focusable(false).map_err(|e| e.to_string())?;
        }
    }
    window
        .emit("companion:geometry", &placement.geometry)
        .map_err(|e| e.to_string())?;
    Ok(placement.geometry)
}

fn compact_geometry(app: &tauri::AppHandle) -> Result<Geometry, String> {
    let window = app
        .get_webview_window(LABEL)
        .ok_or("Assistente flutuante indisponível.")?;
    let anchor = *app
        .state::<CompanionState>()
        .anchor
        .lock()
        .map_err(|_| "Posição do assistente indisponível.")?;
    placement(anchor, &screens(&window)?, Presentation::Compact)
        .map(|placement| placement.geometry)
        .ok_or_else(|| "Nenhuma tela disponível.".into())
}

fn finish_drag(app: &tauri::AppHandle) {
    if app
        .state::<CompanionState>()
        .dragging
        .swap(false, Ordering::SeqCst)
    {
        let _ = app.emit_to(LABEL, "companion:drag-end", ());
    }
}

fn observe_position(app: &tauri::AppHandle, position: CompanionPosition) {
    let state = app.state::<CompanionState>();
    if !state.enabled.load(Ordering::SeqCst)
        || state.expanded.load(Ordering::SeqCst)
        || state.bubble.load(Ordering::SeqCst)
        || state.applying_geometry.load(Ordering::SeqCst)
    {
        return;
    }
    if let Ok(mut anchor) = state.anchor.lock() {
        if *anchor == Some(position) {
            return;
        }
        *anchor = Some(position);
    }
    let revision = state.revision.fetch_add(1, Ordering::SeqCst) + 1;
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_millis(250)).await;
        let state = app.state::<CompanionState>();
        if state.revision.load(Ordering::SeqCst) != revision {
            return;
        }
        let anchor = state.anchor.lock().ok().and_then(|anchor| *anchor);
        if let Some(anchor) = anchor {
            if let Err(error) = app
                .state::<crate::desktop::DesktopState>()
                .save_companion_position(anchor)
            {
                if let Ok(mut stored) = state.error.lock() {
                    *stored = Some(error);
                }
            }
        }
    });
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
fn collapse_on_external_interaction(app: &tauri::AppHandle) {
    let state = app.state::<CompanionState>();
    if state.request_external_collapse() {
        // Keep the native frame intact until the renderer finishes collapsing
        // the island. Resizing immediately clips its closing animation.
        if let Err(error) = app.emit_to(LABEL, "companion:collapse-request", ()) {
            state.collapse_requested.store(false, Ordering::SeqCst);
            if let Ok(mut stored) = state.error.lock() {
                *stored = Some(error.to_string());
            }
        }
    }
}

#[cfg(any(target_os = "macos", target_os = "windows", test))]
fn contains_point(rect: [f64; 4], point: [f64; 2]) -> bool {
    point[0] >= rect[0]
        && point[0] < rect[0] + rect[2]
        && point[1] >= rect[1]
        && point[1] < rect[1] + rect[3]
}

fn visual_drag_anchor(
    origin: CompanionPosition,
    logical_size: [f64; 2],
    scale: f64,
    robot_x: Option<f64>,
    robot_y: Option<f64>,
) -> Result<Option<CompanionPosition>, String> {
    let (x, y) = match (robot_x, robot_y) {
        (None, None) => return Ok(None),
        (Some(x), Some(y)) => (x, y),
        _ => return Err("Posição visual do assistente inválida.".into()),
    };
    if !scale.is_finite()
        || scale <= 0.0
        || logical_size
            .iter()
            .any(|size| !size.is_finite() || *size <= 0.0)
        || !x.is_finite()
        || !y.is_finite()
        || x < -1.0
        || y < -1.0
        || x > logical_size[0] + 1.0
        || y > logical_size[1] + 1.0
    {
        return Err("Posição visual do assistente inválida.".into());
    }
    // CSS coordinates are logical pixels; Windows' global coordinates are
    // physical pixels, while AppKit's coherent screen space uses scale 1.
    // Validate the visual origin: rotation and short-screen scaling can change
    // the pet's DOM bounds. The resulting compact frame is clamped to the screen.
    let x = f64::from(origin.x) + x.max(0.0) * scale;
    let y = f64::from(origin.y) + y.max(0.0) * scale;
    if x < f64::from(i32::MIN)
        || x > f64::from(i32::MAX)
        || y < f64::from(i32::MIN)
        || y > f64::from(i32::MAX)
    {
        return Err("Posição visual do assistente inválida.".into());
    }
    Ok(Some(CompanionPosition {
        x: x.round() as i32,
        y: y.round() as i32,
    }))
}

pub(crate) fn on_window_event(window: &tauri::Window, event: &tauri::WindowEvent) {
    if window.label() == "main"
        && matches!(event, tauri::WindowEvent::Destroyed)
        && window.app_handle().get_webview_window(LABEL).is_some()
    {
        // The auxiliary window must not change Jarvis's normal close-to-quit behavior.
        window.app_handle().exit(0);
        return;
    }
    if window.label() != LABEL {
        return;
    }
    let app = window.app_handle();
    let state = app.state::<CompanionState>();
    match event {
        tauri::WindowEvent::Moved(position) if !cfg!(target_os = "macos") => {
            #[cfg(target_os = "windows")]
            {
                if let Ok(mut expected) = state.expected_position.lock() {
                    if let Some(target) = *expected {
                        if *position == target {
                            *expected = None;
                        }
                        return;
                    }
                }
            }
            observe_position(
                app,
                CompanionPosition {
                    x: position.x,
                    y: position.y,
                },
            );
        }
        tauri::WindowEvent::ScaleFactorChanged { .. } => {
            if !state.dragging.load(Ordering::SeqCst) {
                let _ = apply_geometry(app, state.expanded.load(Ordering::SeqCst));
            }
        }
        tauri::WindowEvent::Focused(false) => {
            let _ = window.set_focusable(false);
            #[cfg(target_os = "windows")]
            collapse_on_external_interaction(app);
        }
        #[cfg(target_os = "macos")]
        tauri::WindowEvent::Destroyed => macos::close(app, false),
        _ => {}
    }
}

#[tauri::command]
pub(crate) fn set_companion_expanded(
    app: tauri::AppHandle,
    expanded: bool,
) -> Result<Geometry, String> {
    let state = app.state::<CompanionState>();
    if state.dragging.load(Ordering::SeqCst) {
        return compact_geometry(&app);
    }
    state.collapse_requested.store(false, Ordering::SeqCst);
    if expanded {
        app.state::<CompanionState>()
            .bubble
            .store(false, Ordering::SeqCst);
    }
    #[cfg(target_os = "macos")]
    macos::create(
        &app.get_webview_window(LABEL)
            .ok_or("Assistente flutuante indisponível.")?,
    )?;
    let geometry = apply_geometry(&app, expanded)?;
    let window = app
        .get_webview_window(LABEL)
        .ok_or("Assistente flutuante indisponível.")?;
    show_nonactivating(&window)?;
    Ok(geometry)
}

#[tauri::command]
pub(crate) fn companion_start_drag(
    app: tauri::AppHandle,
    robot_x: Option<f64>,
    robot_y: Option<f64>,
) -> Result<(), String> {
    #[cfg(not(target_os = "macos"))]
    let window = app
        .get_webview_window(LABEL)
        .ok_or("Assistente flutuante indisponível.")?;
    #[cfg(target_os = "macos")]
    let anchor = macos::drag_anchor(&app, robot_x, robot_y)?;
    #[cfg(not(target_os = "macos"))]
    let anchor = {
        let position = window.outer_position().map_err(|error| error.to_string())?;
        let size = window.inner_size().map_err(|error| error.to_string())?;
        let scale = window.scale_factor().map_err(|error| error.to_string())?;
        visual_drag_anchor(
            CompanionPosition {
                x: position.x,
                y: position.y,
            },
            [
                f64::from(size.width) / scale,
                f64::from(size.height) / scale,
            ],
            scale,
            robot_x,
            robot_y,
        )?
    };
    let state = app.state::<CompanionState>();
    if let Some(anchor) = anchor {
        *state
            .anchor
            .lock()
            .map_err(|_| "Posição do assistente indisponível.")? = Some(anchor);
    }
    state.bubble.store(false, Ordering::SeqCst);
    state.dragging.store(true, Ordering::SeqCst);
    let result = (|| {
        apply_geometry(&app, false)?;
        #[cfg(target_os = "macos")]
        {
            macos::drag(&app)
        }
        #[cfg(not(target_os = "macos"))]
        window.start_dragging().map_err(|e| e.to_string())
    })();
    if result.is_err() {
        finish_drag(&app);
    }
    result
}

#[tauri::command]
pub(crate) fn set_companion_bubble(
    app: tauri::AppHandle,
    visible: bool,
) -> Result<Geometry, String> {
    let state = app.state::<CompanionState>();
    if state.dragging.load(Ordering::SeqCst) {
        return compact_geometry(&app);
    }
    let expanded = state.expanded.load(Ordering::SeqCst);
    state.bubble.store(visible && !expanded, Ordering::SeqCst);
    let geometry = apply_geometry(&app, expanded)?;
    if let Some(window) = app.get_webview_window(LABEL) {
        show_nonactivating(&window)?;
    }
    Ok(geometry)
}

#[tauri::command]
pub(crate) fn companion_set_interacting(app: tauri::AppHandle, active: bool) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    return macos::set_interacting(&app, active);
    #[cfg(not(target_os = "macos"))]
    {
        let window = app
            .get_webview_window(LABEL)
            .ok_or("Assistente flutuante indisponível.")?;
        window.set_focusable(active).map_err(|e| e.to_string())?;
        if active {
            window.set_focus().map_err(|e| e.to_string())?;
        }
        Ok(())
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use super::*;
    use block2::RcBlock;
    use dispatch2::{run_on_main, MainThreadBound};
    use objc2::runtime::AnyObject;
    use objc2::{
        define_class, msg_send, rc::Retained, runtime::ProtocolObject, MainThreadMarker,
        MainThreadOnly,
    };
    use objc2_app_kit::{
        NSApplication, NSAutoresizingMaskOptions, NSBackingStoreType, NSColor, NSEvent,
        NSEventMask, NSEventType, NSPanel, NSScreen, NSView, NSWindow, NSWindowCollectionBehavior,
        NSWindowDidMoveNotification, NSWindowDidResignKeyNotification, NSWindowStyleMask,
    };
    use objc2_foundation::{
        ns_string, NSNotification, NSNotificationCenter, NSObjectProtocol, NSPoint, NSRect, NSSize,
    };
    use std::ptr::NonNull;

    define_class!(
        #[unsafe(super(NSPanel))]
        #[name = "JarvisCompanionPanel"]
        pub(super) struct NativePanel;
        impl NativePanel {
            #[unsafe(method(canBecomeKeyWindow))]
            fn can_become_key(&self) -> bool { true }
            #[unsafe(method(canBecomeMainWindow))]
            fn can_become_main(&self) -> bool { false }
        }
    );

    pub(super) struct PanelHost {
        window: Retained<NativePanel>,
        center: Retained<NSNotificationCenter>,
        notifications: Vec<Retained<ProtocolObject<dyn NSObjectProtocol>>>,
        monitors: Vec<Retained<AnyObject>>,
    }

    impl Drop for PanelHost {
        fn drop(&mut self) {
            // SAFETY: MainThreadBound drops this host on AppKit's thread. All
            // tokens came from the corresponding native registration APIs.
            unsafe {
                for observer in &self.notifications {
                    let object: &AnyObject = (**observer).as_ref();
                    self.center.removeObserver(object);
                }
                for monitor in &self.monitors {
                    NSEvent::removeMonitor(monitor);
                }
            }
        }
    }

    pub(super) type Panel = MainThreadBound<PanelHost>;

    fn panel(
        app: &tauri::AppHandle,
        mtm: MainThreadMarker,
    ) -> Result<Retained<NativePanel>, String> {
        app.state::<CompanionState>()
            .panel
            .lock()
            .map_err(|_| "Janela do assistente indisponível.")?
            .as_ref()
            .map(|panel| panel.get(mtm).window.clone())
            .ok_or_else(|| "Janela do assistente indisponível.".into())
    }

    pub(super) fn create(window: &tauri::WebviewWindow) -> Result<(), String> {
        run_on_main(|mtm| {
            if window
                .app_handle()
                .state::<CompanionState>()
                .panel
                .lock()
                .map_err(|_| "Janela do assistente indisponível.")?
                .is_some()
            {
                return Ok(());
            }
            let pointer = window.ns_window().map_err(|e| e.to_string())?;
            // SAFETY: The retained Tauri window lives throughout this closure on AppKit's thread.
            let owner = unsafe { &*pointer.cast::<NSWindow>() };
            let content = owner
                .contentView()
                .ok_or("Conteúdo do assistente indisponível.")?;
            // Use a real panel rather than changing Tao's ObjC class: Tao relies on its own ivars.
            // The renderer calls this after WKWebView creation: keep Tao's contentView
            // intact and move only its children into the panel, preserving raw handles.
            let children = content.subviews();
            if children.count() == 0 {
                return Err("O conteúdo do assistente ainda não está disponível.".into());
            }
            let allocated = NativePanel::alloc(mtm).set_ivars(());
            // SAFETY: Allocated NSPanel subclass initialized with AppKit's designated initializer.
            let panel: Retained<NativePanel> = unsafe {
                msg_send![super(allocated),
                    initWithContentRect: owner.frame(),
                    styleMask: NSWindowStyleMask::Borderless | NSWindowStyleMask::NonactivatingPanel,
                    backing: NSBackingStoreType::Buffered,
                    defer: false
                ]
            };
            panel.setTitle(ns_string!("Jarvito"));
            panel.setOpaque(false);
            panel.setBackgroundColor(Some(&NSColor::clearColor()));
            panel.setHasShadow(false);
            panel.setFloatingPanel(true);
            panel.setBecomesKeyOnlyIfNeeded(true);
            panel.setHidesOnDeactivate(false);
            panel.setLevel(25); // NSMainMenuWindowLevel: visible over full-screen application windows.
            panel.setCollectionBehavior(
                NSWindowCollectionBehavior::CanJoinAllSpaces
                    | NSWindowCollectionBehavior::Stationary
                    | NSWindowCollectionBehavior::FullScreenAuxiliary
                    | NSWindowCollectionBehavior::IgnoresCycle,
            );
            // SAFETY: Retained ownership is managed exclusively by MainThreadBound, not AppKit close().
            unsafe {
                panel.setReleasedWhenClosed(false);
            }
            let container = NSView::new(mtm);
            container.setFrame(content.bounds());
            container.setAutoresizingMask(
                NSAutoresizingMaskOptions::ViewWidthSizable
                    | NSAutoresizingMaskOptions::ViewHeightSizable,
            );
            panel.setContentView(Some(&container));
            for child in children.iter() {
                child.setAutoresizingMask(
                    NSAutoresizingMaskOptions::ViewWidthSizable
                        | NSAutoresizingMaskOptions::ViewHeightSizable,
                );
                container.addSubview(&child);
            }
            // Exercise the same raw-view path Tao needs during launch and later
            // webview operations: its original view must survive the reparenting.
            debug_assert_eq!(
                window.ns_view().map_err(|error| error.to_string())?,
                Retained::as_ptr(&content).cast_mut().cast()
            );
            let host = install_observers(window.app_handle(), panel);
            *window
                .app_handle()
                .state::<CompanionState>()
                .panel
                .lock()
                .map_err(|_| "Janela do assistente indisponível.")? =
                Some(MainThreadBound::new(host, mtm));
            Ok(())
        })
    }

    pub(super) fn show(app: &tauri::AppHandle) -> Result<(), String> {
        run_on_main(|mtm| {
            panel(app, mtm)?.orderFrontRegardless();
            Ok(())
        })
    }

    pub(super) fn close(app: &tauri::AppHandle, restore_owner: bool) {
        run_on_main(|mtm| {
            let state = app.state::<CompanionState>();
            let Some(panel) = state.panel.lock().ok().and_then(|mut stored| stored.take()) else {
                return;
            };
            let host = panel.into_inner(mtm);
            let panel = host.window.clone();
            drop(host); // Remove callbacks before destroying or reparenting their native window.
            panel.orderOut(None);
            if let Some(window) = app.get_webview_window(LABEL).filter(|_| restore_owner) {
                if let Ok(pointer) = window.ns_window() {
                    // SAFETY: The live Tauri owner and panel share the AppKit thread.
                    unsafe {
                        if let (Some(owner), Some(content)) = (
                            (&*pointer.cast::<NSWindow>()).contentView(),
                            panel.contentView(),
                        ) {
                            for child in content.subviews().iter() {
                                owner.addSubview(&child);
                            }
                        }
                    }
                }
            }
            panel.close();
        });
    }

    fn primary_height(mtm: MainThreadMarker) -> f64 {
        NSScreen::screens(mtm)
            .firstObject()
            .map_or(0.0, |screen| screen.frame().size.height)
    }

    pub(super) fn screens() -> Vec<Screen> {
        run_on_main(|mtm| {
            let primary_height = primary_height(mtm);
            NSScreen::screens(mtm)
                .iter()
                .map(|screen| {
                    let frame = screen.visibleFrame();
                    // AppKit's global logical space stays coherent across displays
                    // with different backing scales; native views handle Retina pixels.
                    Screen {
                        x: frame.origin.x.round() as i32,
                        y: (primary_height - frame.origin.y - frame.size.height).round() as i32,
                        width: frame.size.width.round() as u32,
                        height: frame.size.height.round() as u32,
                        scale: 1.0,
                    }
                })
                .collect()
        })
    }

    pub(super) fn set_geometry(
        window: &tauri::WebviewWindow,
        placement: &Placement,
    ) -> Result<(), String> {
        run_on_main(|mtm| {
            let scale = f64::from(placement.size.width) / placement.geometry.width;
            let frame = NSRect::new(
                NSPoint::new(
                    f64::from(placement.position.x) / scale,
                    primary_height(mtm)
                        - f64::from(placement.position.y) / scale
                        - placement.geometry.height,
                ),
                NSSize::new(placement.geometry.width, placement.geometry.height),
            );
            let state = window.app_handle().state::<CompanionState>();
            state.applying_geometry.store(true, Ordering::SeqCst);
            let result = (|| {
                if let Ok(panel) = panel(window.app_handle(), mtm) {
                    panel.setFrame_display(frame, true);
                    if !placement.geometry.expanded && panel.isKeyWindow() {
                        panel.resignKeyWindow();
                    }
                }
                let pointer = window.ns_window().map_err(|e| e.to_string())?;
                // SAFETY: Keep Tao's hidden monitor/geometry metadata in sync on AppKit's thread.
                unsafe {
                    (&*pointer.cast::<NSWindow>()).setFrame_display(frame, false);
                }
                Ok(())
            })();
            state.applying_geometry.store(false, Ordering::SeqCst);
            result
        })
    }

    pub(super) fn set_interacting(app: &tauri::AppHandle, active: bool) -> Result<(), String> {
        run_on_main(|mtm| {
            let panel = panel(app, mtm)?;
            // A nonactivating panel can take keyboard focus without activating Jarvis or its main window.
            if active {
                panel.makeKeyWindow();
            }
            // DOM blur also fires when a Select moves focus to its portal. AppKit
            // relinquishes key focus on an external click; collapse resigns explicitly.
            Ok(())
        })
    }

    pub(super) fn drag(app: &tauri::AppHandle) -> Result<(), String> {
        run_on_main(|mtm| {
            let panel = panel(app, mtm)?;
            let event = NSApplication::sharedApplication(mtm)
                .currentEvent()
                .ok_or("Arraste o assistente mantendo o botão do mouse pressionado.")?;
            panel.performWindowDragWithEvent(&event);
            // AppKit may finish dragging after this call returns. DidMove, not
            // this initial frame, is the authority for the persisted anchor.
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                // AppKit's tracking loop can consume mouse-up before local
                // monitors see it. The read-only button state covers that case.
                while app
                    .state::<CompanionState>()
                    .dragging
                    .load(Ordering::SeqCst)
                {
                    if NSEvent::pressedMouseButtons() & 1 == 0 {
                        finish_drag(&app);
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(25)).await;
                }
            });
            Ok(())
        })
    }

    pub(super) fn drag_anchor(
        app: &tauri::AppHandle,
        robot_x: Option<f64>,
        robot_y: Option<f64>,
    ) -> Result<Option<CompanionPosition>, String> {
        run_on_main(|mtm| {
            let frame = panel(app, mtm)?.frame();
            visual_drag_anchor(
                CompanionPosition {
                    x: frame.origin.x.round() as i32,
                    y: (primary_height(mtm) - frame.origin.y - frame.size.height).round() as i32,
                },
                [frame.size.width, frame.size.height],
                1.0,
                robot_x,
                robot_y,
            )
        })
    }

    fn moved(app: &tauri::AppHandle) {
        let Some(mtm) = MainThreadMarker::new() else {
            return;
        };
        if let Ok(panel) = panel(app, mtm) {
            let frame = panel.frame();
            observe_position(
                app,
                CompanionPosition {
                    x: frame.origin.x.round() as i32,
                    y: (primary_height(mtm) - frame.origin.y - frame.size.height).round() as i32,
                },
            );
        }
    }

    fn outside_pointer(app: &tauri::AppHandle) {
        if !app
            .state::<CompanionState>()
            .expanded
            .load(Ordering::SeqCst)
        {
            return;
        }
        let Some(mtm) = MainThreadMarker::new() else {
            return;
        };
        if let Ok(panel) = panel(app, mtm) {
            let frame = panel.frame();
            let cursor = NSEvent::mouseLocation();
            if !contains_point(
                [
                    frame.origin.x,
                    frame.origin.y,
                    frame.size.width,
                    frame.size.height,
                ],
                [cursor.x, cursor.y],
            ) {
                collapse_on_external_interaction(app);
            }
        }
    }

    fn pointer_event(app: &tauri::AppHandle, event: NonNull<NSEvent>) {
        // SAFETY: AppKit owns this live event for the duration of the monitor callback.
        let kind = unsafe { event.as_ref() }.r#type();
        if kind == NSEventType::LeftMouseUp {
            finish_drag(app);
        } else {
            outside_pointer(app);
        }
    }

    fn install_observers(app: &tauri::AppHandle, window: Retained<NativePanel>) -> PanelHost {
        let center = NSNotificationCenter::defaultCenter();
        let mut notifications = Vec::new();
        // SAFETY: NativePanel is an Objective-C object retained by the host.
        let object = unsafe { &*Retained::as_ptr(&window).cast::<AnyObject>() };
        for (name, callback) in [
            // SAFETY: These framework notification-name statics are initialized by AppKit.
            (
                unsafe { NSWindowDidMoveNotification },
                moved as fn(&tauri::AppHandle),
            ),
            (
                unsafe { NSWindowDidResignKeyNotification },
                collapse_on_external_interaction as fn(&tauri::AppHandle),
            ),
        ] {
            let app = app.clone();
            let block = RcBlock::new(move |_: NonNull<NSNotification>| callback(&app));
            // SAFETY: Foundation copies the block; registrations and removal use
            // the same center and are kept alive by MainThreadBound until close.
            notifications.push(unsafe {
                center.addObserverForName_object_queue_usingBlock(
                    Some(name),
                    Some(object),
                    None,
                    &block,
                )
            });
        }
        let mask = NSEventMask::LeftMouseDown
            | NSEventMask::RightMouseDown
            | NSEventMask::OtherMouseDown
            | NSEventMask::LeftMouseUp;
        let global_app = app.clone();
        let global = RcBlock::new(move |event: NonNull<NSEvent>| pointer_event(&global_app, event));
        let mut monitors: Vec<_> =
            NSEvent::addGlobalMonitorForEventsMatchingMask_handler(mask, &global)
                .into_iter()
                .collect();
        let local_app = app.clone();
        let local = RcBlock::new(move |event: NonNull<NSEvent>| {
            pointer_event(&local_app, event);
            event.as_ptr()
        });
        // SAFETY: The local monitor returns the original, live event unchanged.
        monitors
            .extend(unsafe { NSEvent::addLocalMonitorForEventsMatchingMask_handler(mask, &local) });
        PanelHost {
            window,
            center,
            notifications,
            monitors,
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use objc2::ClassType;

        #[test]
        fn companion_uses_a_real_panel_instead_of_reclassifying_taos_window() {
            assert_eq!(NativePanel::class().superclass(), Some(NSPanel::class()));
        }
    }
}

#[cfg(target_os = "windows")]
mod windows {
    use super::*;
    use windows_sys::Win32::{
        Foundation::{POINT, RECT},
        UI::{
            Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LBUTTON, VK_MBUTTON, VK_RBUTTON},
            WindowsAndMessaging::{GetCursorPos, GetForegroundWindow, GetWindowRect},
        },
    };

    fn mouse_pressed() -> bool {
        // SAFETY: Virtual-key values are defined by Win32; the API has no borrowed pointers.
        unsafe {
            [VK_LBUTTON, VK_MBUTTON, VK_RBUTTON]
                .iter()
                .any(|button| GetAsyncKeyState(i32::from(*button)) & i16::MIN != 0)
        }
    }

    pub(super) fn start_pointer_watch(app: &tauri::AppHandle) {
        let state = app.state::<CompanionState>();
        if !state.pointer_worker_started.swap(true, Ordering::SeqCst) {
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                loop {
                    let state = app.state::<CompanionState>();
                    while !state.enabled.load(Ordering::SeqCst)
                        || !(state.expanded.load(Ordering::SeqCst)
                            || state.dragging.load(Ordering::SeqCst))
                    {
                        state.pointer_wake.notified().await;
                    }
                    let Some(window) = app.get_webview_window(LABEL) else {
                        tokio::time::sleep(Duration::from_millis(100)).await;
                        continue;
                    };
                    let Ok(handle) = window.hwnd() else {
                        tokio::time::sleep(Duration::from_millis(100)).await;
                        continue;
                    };
                    let handle = handle.0 as usize;
                    // SAFETY: Reading the current foreground HWND does not access application memory.
                    let mut foreground = unsafe { GetForegroundWindow() } as usize;
                    let mut pressed = mouse_pressed();
                    while state.enabled.load(Ordering::SeqCst)
                        && (state.expanded.load(Ordering::SeqCst)
                            || state.dragging.load(Ordering::SeqCst))
                    {
                        let next_pressed = mouse_pressed();
                        if state.dragging.load(Ordering::SeqCst) && !next_pressed {
                            finish_drag(&app);
                        }
                        // SAFETY: Same read-only foreground query; null means no foreground window.
                        let next_foreground = unsafe { GetForegroundWindow() } as usize;
                        let changed_app = next_foreground != 0
                            && next_foreground != foreground
                            && next_foreground != handle;
                        let mut outside = false;
                        if next_pressed && !pressed {
                            let mut cursor = POINT { x: 0, y: 0 };
                            let mut rect = RECT {
                                left: 0,
                                top: 0,
                                right: 0,
                                bottom: 0,
                            };
                            // SAFETY: Both pointers are valid stack allocations; HWND remains owned by Tauri.
                            if unsafe {
                                GetCursorPos(&mut cursor) != 0
                                    && GetWindowRect(handle as _, &mut rect) != 0
                            } {
                                outside = !contains_point(
                                    [
                                        f64::from(rect.left),
                                        f64::from(rect.top),
                                        f64::from(rect.right - rect.left),
                                        f64::from(rect.bottom - rect.top),
                                    ],
                                    [f64::from(cursor.x), f64::from(cursor.y)],
                                );
                            }
                        }
                        if changed_app || outside {
                            collapse_on_external_interaction(&app);
                        }
                        foreground = next_foreground;
                        pressed = next_pressed;
                        // ponytail: poll only while the island is open or dragged; use WH_MOUSE_LL
                        // if sub-25ms clicks are ever observed to escape detection.
                        tokio::time::sleep(Duration::from_millis(25)).await;
                    }
                }
            });
        }
        state.pointer_wake.notify_one();
    }
}

#[tauri::command]
pub(crate) async fn get_companion_usage(
    app: tauri::AppHandle,
    refresh: Option<bool>,
) -> Result<Vec<AccountUsage>, String> {
    let state = app.state::<CompanionState>();
    if refresh == Some(true) && state.enabled.load(Ordering::SeqCst) {
        let usage = refresh_usage(&app).await;
        *state.usage.lock().map_err(|_| "Limites indisponíveis.")? = usage;
    }
    state
        .usage
        .lock()
        .map(|usage| usage.clone())
        .map_err(|_| "Limites indisponíveis.".into())
}

fn unavailable(alias: String, message: String) -> AccountUsage {
    AccountUsage {
        alias,
        fetched_at: None,
        email: None,
        plan: None,
        windows: vec![],
        reset_credits: None,
        error: Some(message),
    }
}

async fn refresh_usage(app: &tauri::AppHandle) -> Vec<AccountUsage> {
    let mut probes = tokio::task::JoinSet::new();
    if let Ok(home) = app.path().home_dir() {
        if let Ok(accounts) = app
            .state::<crate::persistence::AppState>()
            .list_provider_accounts(&home)
        {
            for account in accounts.into_iter().filter(|account| {
                account.enabled
                    && account.show_usage
                    && matches!(
                        account.provider_kind.as_str(),
                        "openai-codex" | "antigravity"
                    )
            }) {
                let app = app.clone();
                probes.spawn(async move {
                    let mut usage = crate::openai_codex::usage::get_provider_usage(
                        app.clone(),
                        app.state(),
                        app.state(),
                        account.alias.clone(),
                    )
                    .await
                    .unwrap_or_else(|error| unavailable(account.alias, error.message));
                    usage
                        .windows
                        .retain(|window| !window.third_party || account.show_third_party_usage);
                    usage
                });
            }
        }
    }
    if let Ok(preferences) = app.state::<crate::system::SystemState>().preferences() {
        if preferences.claude.enabled && preferences.claude.show_usage {
            let app = app.clone();
            probes.spawn(async move {
                crate::claude::get_claude_usage(app.state(), app.state())
                    .await
                    .unwrap_or_else(|error| unavailable("Claude Code".into(), error))
            });
        }
    }
    let mut usage = Vec::new();
    while let Some(result) = probes.join_next().await {
        if let Ok(value) = result {
            usage.push(value);
        }
    }
    usage.sort_by(|a, b| a.alias.cmp(&b.alias));
    usage
}

#[tauri::command]
pub(crate) async fn ack_companion_item(
    app: tauri::AppHandle,
    conversation_id: String,
    agent_id: Option<String>,
    attention_id: String,
) -> Result<crate::agent::companion::Snapshot, crate::agent::AgentError> {
    crate::agent::companion::acknowledge_item(app, conversation_id, agent_id, attention_id).await
}

#[tauri::command]
pub(crate) async fn get_companion_snapshot(
    app: tauri::AppHandle,
) -> Result<crate::agent::companion::Snapshot, crate::agent::AgentError> {
    crate::agent::companion::snapshot(app).await
}

#[tauri::command]
pub(crate) async fn companion_open_conversation(
    app: tauri::AppHandle,
    conversation_id: String,
) -> Result<(), crate::agent::AgentError> {
    crate::agent::companion::open_conversation(app, conversation_id).await
}

#[tauri::command]
pub(crate) async fn companion_answer_question(
    app: tauri::AppHandle,
    conversation_id: String,
    agent_id: Option<String>,
    turn_id: String,
    tool_id: String,
    response: crate::agent::companion::QuestionResponse,
) -> Result<(), crate::agent::AgentError> {
    crate::agent::companion::answer_question(
        app,
        conversation_id,
        agent_id,
        turn_id,
        tool_id,
        response,
    )
    .await
}

#[tauri::command]
pub(crate) async fn companion_pause_question(
    app: tauri::AppHandle,
    conversation_id: String,
    agent_id: Option<String>,
    turn_id: String,
    tool_id: String,
) -> Result<(), crate::agent::AgentError> {
    crate::agent::companion::pause_question(app, conversation_id, agent_id, turn_id, tool_id).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn robot_origin(placement: &Placement, scale: f64) -> CompanionPosition {
        let width = (PET_WIDTH * scale).round() as u32;
        let height = (PET_HEIGHT * scale).round() as u32;
        CompanionPosition {
            x: placement.position.x
                + if placement.geometry.robot_side == "right" {
                    placement.size.width.saturating_sub(width) as i32
                } else {
                    0
                },
            y: placement.position.y
                + if placement.geometry.robot_vertical == "bottom" {
                    placement.size.height.saturating_sub(height) as i32
                } else {
                    0
                },
        }
    }

    #[test]
    fn companion_navigation_stays_in_the_local_application() {
        for url in [
            "tauri://localhost/index.html",
            "http://tauri.localhost/index.html",
        ] {
            assert!(local_navigation(&tauri::Url::parse(url).unwrap(), None));
        }
        for url in [
            "https://example.com",
            "https://localhost.example.com",
            "file:///tmp/page.html",
        ] {
            assert!(!local_navigation(&tauri::Url::parse(url).unwrap(), None));
        }
    }
    #[test]
    fn expansion_preserves_anchor_and_stays_inside_work_area() {
        for scale in [1.0, 1.5, 2.0] {
            let screen = Screen {
                x: -1600,
                y: 24,
                width: 1600,
                height: 900,
                scale,
            };
            for x in [-1600, -1000, -150] {
                for y in [24, 400, 750] {
                    let compact = placement(
                        Some(CompanionPosition { x, y }),
                        &[screen],
                        Presentation::Compact,
                    )
                    .unwrap();
                    let expanded =
                        placement(Some(compact.anchor), &[screen], Presentation::Island).unwrap();
                    assert_eq!(compact.anchor, expanded.anchor);
                    assert_eq!(robot_origin(&expanded, scale), compact.anchor);
                    let collapsed =
                        placement(Some(expanded.anchor), &[screen], Presentation::Compact).unwrap();
                    assert_eq!(collapsed.position, compact.position);
                    assert!(expanded.position.x >= screen.x && expanded.position.y >= screen.y);
                    assert!(i64::from(expanded.position.x) + i64::from(expanded.size.width) <= 0);
                    assert!(
                        i64::from(expanded.position.y) + i64::from(expanded.size.height) <= 924
                    );
                }
            }
        }
    }
    #[test]
    fn island_fits_short_work_areas_without_moving_the_top_left_robot() {
        let screen = Screen {
            x: 0,
            y: 40,
            width: 320,
            height: 440,
            scale: 1.0,
        };
        let expanded = placement(
            Some(CompanionPosition { x: 0, y: 40 }),
            &[screen],
            Presentation::Island,
        )
        .unwrap();
        assert_eq!(expanded.position, PhysicalPosition::new(0, 40));
        assert_eq!(expanded.size, PhysicalSize::new(320, 440));
        assert_eq!(expanded.geometry.robot_side, "left");
        assert_eq!(expanded.geometry.robot_vertical, "top");
        assert_eq!(expanded.anchor, CompanionPosition { x: 0, y: 40 });
    }
    #[test]
    fn removed_display_and_changed_scale_restore_a_reachable_robot() {
        let screen = Screen {
            x: 0,
            y: 40,
            width: 1920,
            height: 1040,
            scale: 2.0,
        };
        let restored = placement(
            Some(CompanionPosition { x: -3000, y: 8000 }),
            &[screen],
            Presentation::Compact,
        )
        .unwrap();
        assert_eq!(restored.anchor, CompanionPosition { x: 0, y: 856 });
        assert_eq!(restored.geometry.width, PET_WIDTH);
        assert!(placement(None, &[], Presentation::Compact).is_none());
    }

    #[test]
    fn repeated_drags_and_bubbles_keep_the_users_robot_position() {
        for scale in [1.0, 1.5, 2.0] {
            let screen = Screen {
                x: -1800,
                y: 24,
                width: 1800,
                height: 1100,
                scale,
            };
            for point in [
                CompanionPosition { x: -1800, y: 24 },
                CompanionPosition { x: -1200, y: 440 },
                CompanionPosition { x: -500, y: 850 },
                CompanionPosition { x: -1500, y: 90 },
            ] {
                let compact = placement(Some(point), &[screen], Presentation::Compact).unwrap();
                let bubble =
                    placement(Some(compact.anchor), &[screen], Presentation::Bubble).unwrap();
                assert_eq!(robot_origin(&bubble, scale), compact.anchor);
                assert!(bubble.geometry.bubble && !bubble.geometry.expanded);
                let island =
                    placement(Some(bubble.anchor), &[screen], Presentation::Island).unwrap();
                assert_eq!(robot_origin(&island, scale), compact.anchor);
                let collapsed =
                    placement(Some(island.anchor), &[screen], Presentation::Compact).unwrap();
                assert_eq!(collapsed.position, compact.position);
                assert_eq!(collapsed.anchor, compact.anchor);
            }
        }
    }

    #[test]
    fn outside_click_coordinates_exclude_the_other_application() {
        let island = [-460.0, 24.0, 460.0, 600.0];
        assert!(contains_point(island, [-459.0, 25.0]));
        assert!(!contains_point(island, [-461.0, 300.0]));
        assert!(!contains_point(island, [1.0, 300.0]));
        assert!(!contains_point(island, [-250.0, 624.0]));
    }

    #[test]
    fn bubbles_and_islands_follow_all_four_screen_corners_without_teleporting() {
        for scale in [1.0, 1.5, 2.0] {
            let screen = Screen {
                x: -1600,
                y: 24,
                width: 1600,
                height: 900,
                scale,
            };
            let right = screen.x + (screen.width - (PET_WIDTH * scale).round() as u32) as i32;
            let bottom = screen.y + (screen.height - (PET_HEIGHT * scale).round() as u32) as i32;
            for (x, y, side, vertical) in [
                (screen.x, screen.y, "left", "top"),
                (right, screen.y, "right", "top"),
                (screen.x, bottom, "left", "bottom"),
                (right, bottom, "right", "bottom"),
            ] {
                let anchor = CompanionPosition { x, y };
                for mode in [
                    Presentation::Compact,
                    Presentation::Bubble,
                    Presentation::Island,
                ] {
                    let surface = placement(Some(anchor), &[screen], mode).unwrap();
                    assert_eq!(robot_origin(&surface, scale), anchor);
                    assert_eq!(surface.geometry.robot_side, side);
                    assert_eq!(surface.geometry.robot_vertical, vertical);
                    assert!(surface.position.x >= screen.x && surface.position.y >= screen.y);
                    assert!(
                        surface.position.x + surface.size.width as i32
                            <= screen.x + screen.width as i32
                    );
                    assert!(
                        surface.position.y + surface.size.height as i32
                            <= screen.y + screen.height as i32
                    );
                }
            }
        }
    }

    #[test]
    fn island_shrinks_at_interior_anchors_before_moving_the_robot() {
        let screen = Screen {
            x: 0,
            y: 40,
            width: 800,
            height: 800,
            scale: 1.0,
        };
        let anchor = CompanionPosition { x: 352, y: 384 };
        let island = placement(Some(anchor), &[screen], Presentation::Island).unwrap();
        assert_eq!(island.geometry.width, 448.0);
        assert_eq!(island.geometry.height, 456.0);
        assert_eq!(robot_origin(&island, 1.0), anchor);
        let compact = placement(Some(island.anchor), &[screen], Presentation::Compact).unwrap();
        assert_eq!(robot_origin(&compact, 1.0), anchor);
    }

    #[test]
    fn external_collapse_is_requested_once_until_the_renderer_finishes() {
        let state = CompanionState::default();
        assert!(!state.request_external_collapse());
        state.expanded.store(true, Ordering::SeqCst);
        state.dragging.store(true, Ordering::SeqCst);
        assert!(!state.request_external_collapse());
        state.dragging.store(false, Ordering::SeqCst);
        assert!(state.request_external_collapse());
        assert!(!state.request_external_collapse());
        // The native frame remains expanded while its renderer animates.
        assert!(state.expanded.load(Ordering::SeqCst));
        state.expanded.store(false, Ordering::SeqCst);
        state.collapse_requested.store(false, Ordering::SeqCst);
        assert!(!state.request_external_collapse());
        state.expanded.store(true, Ordering::SeqCst);
        assert!(state.request_external_collapse());
    }

    #[test]
    fn dragging_a_walking_robot_preserves_its_visible_position_across_scales() {
        for scale in [1.0, 1.5, 2.0] {
            let screen = Screen {
                x: -1800,
                y: 24,
                width: 1800,
                height: 1200,
                scale,
            };
            let island = placement(
                Some(CompanionPosition { x: -400, y: 900 }),
                &[screen],
                Presentation::Island,
            )
            .unwrap();
            for robot_x in [0.0, 120.25, island.geometry.width - PET_WIDTH - 4.0] {
                let robot_y = island.geometry.height - PET_HEIGHT;
                let visible = visual_drag_anchor(
                    CompanionPosition {
                        x: island.position.x,
                        y: island.position.y,
                    },
                    [island.geometry.width, island.geometry.height],
                    scale,
                    Some(robot_x),
                    Some(robot_y),
                )
                .unwrap()
                .unwrap();
                let compact = placement(Some(visible), &[screen], Presentation::Compact).unwrap();
                assert_eq!(
                    compact.position,
                    PhysicalPosition::new(
                        (f64::from(island.position.x) + robot_x * scale).round() as i32,
                        (f64::from(island.position.y) + robot_y * scale).round() as i32,
                    )
                );
            }
        }
    }

    #[test]
    fn visual_drag_coordinates_require_a_complete_finite_point_inside_the_window() {
        let origin = CompanionPosition { x: -400, y: 24 };
        for (x, y) in [
            (Some(0.0), None),
            (None, Some(0.0)),
            (Some(f64::NAN), Some(0.0)),
            (Some(0.0), Some(f64::INFINITY)),
            (Some(-2.0), Some(0.0)),
            (Some(462.0), Some(0.0)),
            (Some(0.0), Some(602.0)),
        ] {
            assert!(visual_drag_anchor(origin, [460.0, 600.0], 1.0, x, y).is_err());
        }
        assert_eq!(
            visual_drag_anchor(origin, [460.0, 600.0], 1.0, None, None).unwrap(),
            None
        );
        assert!(visual_drag_anchor(origin, [460.0, 600.0], 0.0, Some(0.0), Some(0.0)).is_err());
        // A rotated or scaled pet need not have the canonical 96x112 DOM bounds.
        assert!(visual_drag_anchor(origin, [460.0, 600.0], 1.0, Some(220.0), Some(511.0)).is_ok());
        assert!(visual_drag_anchor(origin, [320.0, 360.0], 1.0, Some(240.0), Some(276.0)).is_ok());
    }
}
