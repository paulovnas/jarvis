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
const COMPACT_WIDTH: f64 = 288.0;
const COMPACT_HEIGHT: f64 = 32.0;
const ISLAND_WIDTH: f64 = 640.0;
const ISLAND_HEIGHT: f64 = 160.0;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CompanionUsage {
    provider_kind: String,
    #[serde(flatten)]
    usage: AccountUsage,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Geometry {
    expanded: bool,
    bubble: bool,
    width: f64,
    height: f64,
    robot_side: &'static str,
    robot_vertical: &'static str,
    compact_x: f64,
    compact_y: f64,
    compact_width: f64,
    compact_height: f64,
    surface_x: f64,
    surface_y: f64,
    surface_width: f64,
    surface_height: f64,
    notch_width: f64,
    notch_height: f64,
    header_height: f64,
    drag_axis: &'static str,
}

#[derive(Clone, Copy, Default)]
struct Screen {
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    scale: f64,
    fixed: bool,
    notch_width: f64,
    notch_height: f64,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Presentation {
    Compact,
    Bubble,
    Island(u32),
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
        .first()
        .filter(|screen| screen.fixed)
        .or_else(|| {
            screens.iter().find(|s| {
                anchor.is_some_and(|p| {
                    i64::from(p.x) >= i64::from(s.x)
                        && i64::from(p.x) < i64::from(s.x) + i64::from(s.width)
                        && i64::from(p.y) >= i64::from(s.y)
                        && i64::from(p.y) < i64::from(s.y) + i64::from(s.height)
                })
            })
        })
        .or_else(|| screens.first())?;
    let scale = screen.scale;
    let compact_width = ((COMPACT_WIDTH.max(screen.notch_width + 104.0) * scale)
        .round()
        .max(1.0) as u32)
        .min(screen.width);
    let compact_height = ((COMPACT_HEIGHT.max(screen.notch_height) * scale)
        .round()
        .max(1.0) as u32)
        .min(screen.height);
    let x_max = i64::from(screen.x) + i64::from(screen.width - compact_width);
    let default = CompanionPosition {
        x: (i64::from(screen.x) + i64::from((screen.width - compact_width) / 2)) as i32,
        y: screen.y,
    };
    let anchor = if screen.fixed {
        default
    } else {
        anchor.unwrap_or(default)
    };
    let x = i64::from(anchor.x).clamp(i64::from(screen.x), x_max);
    let y = i64::from(screen.y);
    let (width, height) = match mode {
        Presentation::Compact => (compact_width, compact_height),
        Presentation::Bubble => (
            (ISLAND_WIDTH * scale).round() as u32,
            ((ISLAND_HEIGHT
                + if screen.notch_width > 0.0 {
                    COMPACT_HEIGHT.max(screen.notch_height)
                } else {
                    0.0
                })
                * scale)
                .round() as u32,
        ),
        Presentation::Island(height) => (
            (ISLAND_WIDTH * scale).round() as u32,
            (f64::from(height) * scale).round() as u32,
        ),
    };
    let left = x - i64::from(screen.x);
    let right = x_max - x;
    let on_right = left >= right;
    // The compact island is the stable anchor. Larger views grow around its
    // center, and closing returns to the same saved position on every display.
    let width = width.min(screen.width).max(compact_width);
    let height = height.min(screen.height).max(compact_height);
    let position = PhysicalPosition::new(
        (x - i64::from((width - compact_width) / 2)).clamp(
            i64::from(screen.x),
            i64::from(screen.x) + i64::from(screen.width - width),
        ) as i32,
        screen.y,
    );
    Some(Placement {
        anchor: CompanionPosition {
            x: x as i32,
            y: y as i32,
        },
        position,
        size: PhysicalSize::new(width, height),
        geometry: Geometry {
            expanded: matches!(mode, Presentation::Island(_)),
            bubble: mode == Presentation::Bubble,
            width: f64::from(width) / scale,
            height: f64::from(height) / scale,
            robot_side: if on_right && !screen.fixed {
                "right"
            } else {
                "left"
            },
            robot_vertical: "top",
            compact_x: f64::from(x as i32 - position.x) / scale,
            compact_y: f64::from(y as i32 - position.y) / scale,
            compact_width: f64::from(compact_width) / scale,
            compact_height: f64::from(compact_height) / scale,
            surface_x: 0.0,
            surface_y: 0.0,
            surface_width: f64::from(width) / scale,
            surface_height: f64::from(height) / scale,
            notch_width: screen.notch_width,
            notch_height: screen.notch_height,
            header_height: f64::from(compact_height) / scale,
            drag_axis: if screen.fixed { "none" } else { "horizontal" },
        },
    })
}

fn restore_anchor(
    anchor: Option<CompanionPosition>,
    screens: &[Screen],
) -> Option<CompanionPosition> {
    let anchor = anchor?;
    // Migrate only the old automatically chosen corner. Deliberate user
    // positions remain theirs, including secondary displays and negative origins.
    let old_default = screens.iter().any(|screen| {
        let x =
            i64::from(screen.x) + i64::from(screen.width) - (120.0 * screen.scale).round() as i64;
        let y =
            i64::from(screen.y) + i64::from(screen.height) - (136.0 * screen.scale).round() as i64;
        (i64::from(anchor.x) - x).abs() <= 2 && (i64::from(anchor.y) - y).abs() <= 2
    });
    (!old_default).then_some(anchor)
}

fn requested_height(height: Option<f64>) -> Result<u32, String> {
    let height = height.unwrap_or(ISLAND_HEIGHT);
    if !height.is_finite() || !(ISLAND_HEIGHT..=600.0).contains(&height) {
        return Err("Altura da ilha inválida.".into());
    }
    Ok(height.round() as u32)
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
    island_height: Mutex<Option<u32>>,
    hit_rect: Mutex<Option<[f64; 4]>>,
    canvas_size: Mutex<Option<[f64; 2]>>,
    drag_origin: Mutex<Option<(CompanionPosition, Screen, i32)>>,
    error: Mutex<Option<String>>,
    usage: Mutex<Vec<CompanionUsage>>,
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
            let position = monitor.position();
            Screen {
                x: position.x,
                y: position.y,
                width: monitor.size().width,
                height: (i64::from(area.position.y) + i64::from(area.size.height)
                    - i64::from(position.y)) as u32,
                scale: monitor.scale_factor(),
                fixed: false,
                notch_width: 0.0,
                notch_height: 0.0,
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
            .inner_size(COMPACT_WIDTH, COMPACT_HEIGHT)
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
    let saved = *state
        .anchor
        .lock()
        .map_err(|_| "Posição do assistente indisponível.")?;
    *state
        .anchor
        .lock()
        .map_err(|_| "Posição do assistente indisponível.")? =
        restore_anchor(saved, &screens(&window)?);
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
        Presentation::Island(
            state
                .island_height
                .lock()
                .map_err(|_| "Altura da ilha indisponível.")?
                .unwrap_or(ISLAND_HEIGHT as u32),
        )
    } else if state.bubble.load(Ordering::SeqCst) {
        Presentation::Bubble
    } else {
        Presentation::Compact
    };
    let placement =
        placement(anchor, &screens(&window)?, mode).ok_or("Nenhuma tela disponível.")?;
    *state
        .canvas_size
        .lock()
        .map_err(|_| "Tamanho da ilha indisponível.")? =
        Some([placement.geometry.width, placement.geometry.height]);
    *state
        .hit_rect
        .lock()
        .map_err(|_| "Área da ilha indisponível.")? = Some([
        placement.geometry.surface_x,
        placement.geometry.surface_y,
        placement.geometry.surface_width,
        placement.geometry.surface_height,
    ]);
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
        #[cfg(target_os = "windows")]
        windows::apply_hit_rect(
            &window,
            [
                0.0,
                0.0,
                placement.geometry.width,
                placement.geometry.height,
            ],
        )?;
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
        if let Ok(mut origin) = app.state::<CompanionState>().drag_origin.lock() {
            *origin = None;
        }
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
    schedule_anchor_save(app);
}

fn schedule_anchor_save(app: &tauri::AppHandle) {
    let state = app.state::<CompanionState>();
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
    height: Option<f64>,
) -> Result<Geometry, String> {
    let state = app.state::<CompanionState>();
    if state.dragging.load(Ordering::SeqCst) {
        return compact_geometry(&app);
    }
    state.collapse_requested.store(false, Ordering::SeqCst);
    if expanded {
        *state
            .island_height
            .lock()
            .map_err(|_| "Altura da ilha indisponível.")? = Some(requested_height(height)?);
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

fn validated_hit_rect(rect: [f64; 4], canvas: [f64; 2]) -> Result<[f64; 4], String> {
    if rect.iter().any(|value| !value.is_finite())
        || rect[0] < -1.0
        || rect[1] < -1.0
        || rect[2] <= 0.0
        || rect[3] <= 0.0
        || rect[0] + rect[2] > canvas[0] + 1.0
        || rect[1] + rect[3] > canvas[1] + 1.0
    {
        return Err("Área visual da ilha inválida.".into());
    }
    Ok(rect)
}

#[tauri::command]
pub(crate) fn companion_set_hit_rect(
    app: tauri::AppHandle,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
) -> Result<(), String> {
    let state = app.state::<CompanionState>();
    let canvas = state
        .canvas_size
        .lock()
        .map_err(|_| "Tamanho da ilha indisponível.")?
        .ok_or("Tamanho da ilha indisponível.")?;
    let rect = validated_hit_rect([x, y, width, height], canvas)?;
    #[cfg(target_os = "windows")]
    {
        let window = app
            .get_webview_window(LABEL)
            .ok_or("Assistente flutuante indisponível.")?;
        windows::apply_hit_rect(&window, rect)?;
        state.pointer_wake.notify_one();
    }
    *state
        .hit_rect
        .lock()
        .map_err(|_| "Área da ilha indisponível.")? = Some(rect);
    #[cfg(target_os = "macos")]
    macos::refresh_pointer(&app);
    Ok(())
}

#[tauri::command]
pub(crate) fn get_companion_sound(app: tauri::AppHandle) -> bool {
    app.state::<crate::desktop::DesktopState>()
        .companion_sound_enabled()
}

#[tauri::command]
pub(crate) fn set_companion_sound(app: tauri::AppHandle, enabled: bool) -> Result<bool, String> {
    app.state::<crate::desktop::DesktopState>()
        .save_companion_sound_enabled(enabled)?;
    Ok(enabled)
}

#[tauri::command]
pub(crate) fn get_companion_speech(app: tauri::AppHandle) -> bool {
    app.state::<crate::desktop::DesktopState>()
        .companion_speech_enabled()
}

#[tauri::command]
pub(crate) fn set_companion_speech(app: tauri::AppHandle, enabled: bool) -> Result<bool, String> {
    app.state::<crate::desktop::DesktopState>()
        .save_companion_speech_enabled(enabled)?;
    if !enabled {
        app.state::<crate::voice::VoiceState>().silence_speech(&app);
    }
    app.emit("companion:speech_changed", enabled)
        .map_err(|_| "Não foi possível atualizar a preferência de fala.")?;
    Ok(enabled)
}

#[tauri::command]
pub(crate) fn get_companion_speech_volume(app: tauri::AppHandle) -> f32 {
    app.state::<crate::desktop::DesktopState>()
        .companion_speech_volume()
}

#[tauri::command]
pub(crate) fn set_companion_speech_volume(
    app: tauri::AppHandle,
    volume: f32,
) -> Result<f32, String> {
    let desktop = app.state::<crate::desktop::DesktopState>();
    desktop.save_companion_speech_volume(volume)?;
    app.emit(
        "companion:speech_changed",
        desktop.companion_speech_enabled(),
    )
    .map_err(|_| "Não foi possível atualizar o volume da fala.")?;
    Ok(volume)
}

#[tauri::command]
pub(crate) fn companion_start_drag(
    app: tauri::AppHandle,
    robot_x: Option<f64>,
    robot_y: Option<f64>,
) -> Result<(), String> {
    // Retain the optional arguments for old renderers; dragging is now owned by
    // the black header and never uses the character's visual position.
    let _ = (robot_x, robot_y);
    #[cfg(target_os = "windows")]
    {
        let window = app
            .get_webview_window(LABEL)
            .ok_or("Assistente flutuante indisponível.")?;
        let state = app.state::<CompanionState>();
        let anchor = *state
            .anchor
            .lock()
            .map_err(|_| "Posição do assistente indisponível.")?;
        let screens = screens(&window)?;
        let compact =
            placement(anchor, &screens, Presentation::Compact).ok_or("Nenhuma tela disponível.")?;
        let screen = screens
            .iter()
            .find(|screen| {
                contains_point(
                    [
                        f64::from(screen.x),
                        f64::from(screen.y),
                        f64::from(screen.width),
                        f64::from(screen.height),
                    ],
                    [f64::from(compact.anchor.x), f64::from(compact.anchor.y)],
                )
            })
            .copied()
            .ok_or("Nenhuma tela disponível.")?;
        let cursor = windows::cursor_x()?;
        *state
            .drag_origin
            .lock()
            .map_err(|_| "Arraste da ilha indisponível.")? = Some((compact.anchor, screen, cursor));
        state.dragging.store(true, Ordering::SeqCst);
        state.pointer_wake.notify_one();
    }
    #[cfg(not(target_os = "windows"))]
    let _ = app;
    Ok(())
}

#[cfg(any(target_os = "windows", test))]
fn horizontal_anchor(origin: CompanionPosition, screen: Screen, delta: i64) -> CompanionPosition {
    let width = ((COMPACT_WIDTH * screen.scale).round().max(1.0) as u32).min(screen.width);
    let x = (i64::from(origin.x) + delta).clamp(
        i64::from(screen.x),
        i64::from(screen.x) + i64::from(screen.width - width),
    );
    CompanionPosition {
        x: x as i32,
        y: screen.y,
    }
}

#[tauri::command]
pub(crate) fn companion_move_horizontal(app: tauri::AppHandle) -> Result<Geometry, String> {
    #[cfg(target_os = "windows")]
    {
        let state = app.state::<CompanionState>();
        let origin = *state
            .drag_origin
            .lock()
            .map_err(|_| "Arraste da ilha indisponível.")?;
        let Some((origin, screen, start)) = origin else {
            return apply_geometry(&app, state.expanded.load(Ordering::SeqCst));
        };
        let current = windows::cursor_x()?;
        let anchor = horizontal_anchor(origin, screen, i64::from(current) - i64::from(start));
        *state
            .anchor
            .lock()
            .map_err(|_| "Posição do assistente indisponível.")? = Some(anchor);
        let geometry = apply_geometry(&app, state.expanded.load(Ordering::SeqCst))?;
        schedule_anchor_save(&app);
        Ok(geometry)
    }
    #[cfg(not(target_os = "windows"))]
    apply_geometry(
        &app,
        app.state::<CompanionState>()
            .expanded
            .load(Ordering::SeqCst),
    )
}

#[tauri::command]
pub(crate) fn companion_finish_drag(app: tauri::AppHandle) {
    finish_drag(&app);
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
    interaction_focus(active, || {
        #[cfg(target_os = "macos")]
        return macos::set_interacting(&app, true);
        #[cfg(not(target_os = "macos"))]
        {
            let window = app
                .get_webview_window(LABEL)
                .ok_or("Assistente flutuante indisponível.")?;
            window.set_focusable(true).map_err(|e| e.to_string())?;
            window.set_focus().map_err(|e| e.to_string())?;
            Ok(())
        }
    })
}

fn interaction_focus(
    active: bool,
    request_focus: impl FnOnce() -> Result<(), String>,
) -> Result<(), String> {
    // DOM blur also fires when focus enters a portal. Native Focused(false)
    // and compact geometry alone restore the nonactivating window behavior.
    if active {
        request_focus()
    } else {
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
        define_class, msg_send, rc::Retained, runtime::ProtocolObject, sel, MainThreadMarker,
        MainThreadOnly,
    };
    use objc2_app_kit::{
        NSApplicationDidChangeScreenParametersNotification, NSAutoresizingMaskOptions,
        NSBackingStoreType, NSColor, NSEvent, NSEventMask, NSEventType, NSPanel, NSScreen, NSView,
        NSWindow, NSWindowCollectionBehavior, NSWindowDidMoveNotification,
        NSWindowDidResignKeyNotification, NSWindowStyleMask,
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
            panel.setAcceptsMouseMovedEvents(true);
            panel.setLevel(28); // Main menu level + 3 keeps the notch island above the menu bar.
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
        let result = run_on_main(|mtm| {
            panel(app, mtm)?.orderFrontRegardless();
            Ok(())
        });
        refresh_pointer(app);
        result
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
            let mut screens: Vec<_> = NSScreen::screens(mtm)
                .iter()
                .map(|screen| {
                    let frame = screen.frame();
                    let visible = screen.visibleFrame();
                    // Notch APIs arrived in macOS 12. Query support rather than
                    // sending unavailable selectors on older supported machines.
                    let notch_height = if screen.respondsToSelector(sel!(safeAreaInsets)) {
                        screen.safeAreaInsets().top
                    } else {
                        0.0
                    };
                    let notch_width = if notch_height > 0.0
                        && screen.respondsToSelector(sel!(auxiliaryTopLeftArea))
                        && screen.respondsToSelector(sel!(auxiliaryTopRightArea))
                    {
                        let left = screen.auxiliaryTopLeftArea();
                        let right = screen.auxiliaryTopRightArea();
                        let gap = frame.size.width - left.size.width - right.size.width;
                        if left.size.width > 0.0 && right.size.width > 0.0 && gap > 0.0 {
                            gap
                        } else {
                            184.0
                        }
                    } else if notch_height > 0.0 {
                        184.0
                    } else {
                        0.0
                    };
                    // AppKit's global logical space stays coherent across displays
                    // with different backing scales; native views handle Retina pixels.
                    Screen {
                        x: frame.origin.x.round() as i32,
                        y: (primary_height - frame.origin.y - frame.size.height).round() as i32,
                        width: frame.size.width.round() as u32,
                        height: (frame.origin.y + frame.size.height - visible.origin.y).round()
                            as u32,
                        scale: 1.0,
                        fixed: true,
                        notch_width,
                        notch_height,
                    }
                })
                .collect();
            screens.sort_by_key(|screen| screen.notch_height <= 0.0);
            screens
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

    pub(super) fn refresh_pointer(app: &tauri::AppHandle) {
        run_on_main(|mtm| {
            let state = app.state::<CompanionState>();
            let Ok(panel) = panel(app, mtm) else {
                return;
            };
            let frame = panel.frame();
            let cursor = NSEvent::mouseLocation();
            let hit = state.hit_rect.lock().ok().and_then(|rect| *rect);
            let inside = hit.is_some_and(|rect| {
                contains_point(
                    rect,
                    [
                        cursor.x - frame.origin.x,
                        frame.origin.y + frame.size.height - cursor.y,
                    ],
                )
            });
            panel.setIgnoresMouseEvents(!inside && !state.dragging.load(Ordering::SeqCst));
        });
    }

    fn outside_pointer(app: &tauri::AppHandle) {
        let state = app.state::<CompanionState>();
        if !state.expanded.load(Ordering::SeqCst) {
            return;
        }
        let Some(mtm) = MainThreadMarker::new() else {
            return;
        };
        if let Ok(panel) = panel(app, mtm) {
            let frame = panel.frame();
            let cursor = NSEvent::mouseLocation();
            let hit = state.hit_rect.lock().ok().and_then(|rect| *rect);
            if !hit.is_some_and(|rect| {
                contains_point(
                    rect,
                    [
                        cursor.x - frame.origin.x,
                        frame.origin.y + frame.size.height - cursor.y,
                    ],
                )
            }) {
                collapse_on_external_interaction(app);
            }
        }
    }

    fn pointer_event(app: &tauri::AppHandle, event: NonNull<NSEvent>) {
        // SAFETY: AppKit owns this live event for the duration of the monitor callback.
        let kind = unsafe { event.as_ref() }.r#type();
        refresh_pointer(app);
        if kind == NSEventType::LeftMouseUp {
            finish_drag(app);
        } else if matches!(
            kind,
            NSEventType::LeftMouseDown | NSEventType::RightMouseDown | NSEventType::OtherMouseDown
        ) {
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
        let screens_app = app.clone();
        let screens_changed = RcBlock::new(move |_: NonNull<NSNotification>| {
            let state = screens_app.state::<CompanionState>();
            if state.enabled.load(Ordering::SeqCst) {
                if let Err(error) =
                    apply_geometry(&screens_app, state.expanded.load(Ordering::SeqCst))
                {
                    if let Ok(mut stored) = state.error.lock() {
                        *stored = Some(error);
                    }
                }
            }
        });
        // SAFETY: AppKit supplies this notification on its main thread. Observe
        // all objects: display changes belong to the application, not the panel.
        // PanelHost removes the copied callback before dropping its native view.
        notifications.push(unsafe {
            center.addObserverForName_object_queue_usingBlock(
                Some(NSApplicationDidChangeScreenParametersNotification),
                None,
                None,
                &screens_changed,
            )
        });
        let mask = NSEventMask::LeftMouseDown
            | NSEventMask::RightMouseDown
            | NSEventMask::OtherMouseDown
            | NSEventMask::LeftMouseUp
            | NSEventMask::MouseMoved
            | NSEventMask::LeftMouseDragged;
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
        Graphics::Gdi::{CreateRoundRectRgn, DeleteObject, SetWindowRgn},
        UI::{
            Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LBUTTON, VK_MBUTTON, VK_RBUTTON},
            WindowsAndMessaging::{GetCursorPos, GetForegroundWindow, GetWindowRect},
        },
    };

    pub(super) fn cursor_x() -> Result<i32, String> {
        let mut cursor = POINT { x: 0, y: 0 };
        // SAFETY: GetCursorPos writes physical coordinates into this live stack
        // value, independent of the webview's current per-monitor DPI scale.
        if unsafe { GetCursorPos(&mut cursor) } == 0 {
            return Err("Posição do cursor indisponível.".into());
        }
        Ok(cursor.x)
    }

    pub(super) fn apply_hit_rect(
        window: &tauri::WebviewWindow,
        rect: [f64; 4],
    ) -> Result<(), String> {
        let scale = window.scale_factor().map_err(|error| error.to_string())?;
        let handle = window.hwnd().map_err(|error| error.to_string())?;
        // SAFETY: GDI allocates an independent region. Windows takes ownership
        // only after SetWindowRgn succeeds; failures release it here and keep the
        // prior region so the island remains interactive.
        unsafe {
            let corner = (rect[3].min(56.0) * scale).round() as i32;
            let region = CreateRoundRectRgn(
                (rect[0] * scale).floor() as i32,
                (rect[1] * scale).floor() as i32,
                ((rect[0] + rect[2]) * scale).ceil() as i32 + 1,
                ((rect[1] + rect[3]) * scale).ceil() as i32 + 1,
                corner,
                corner,
            );
            if region.is_null() {
                return Err("Não foi possível atualizar a área visual da ilha.".into());
            }
            if SetWindowRgn(handle.0 as _, region, 1) == 0 {
                DeleteObject(region);
                return Err("Não foi possível atualizar a área visual da ilha.".into());
            }
        }
        Ok(())
    }

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
                                let scale = window.scale_factor().unwrap_or(1.0);
                                let hit = state.hit_rect.lock().ok().and_then(|hit| *hit);
                                outside = !hit.is_some_and(|hit| {
                                    contains_point(
                                        hit,
                                        [
                                            f64::from(cursor.x - rect.left) / scale,
                                            f64::from(cursor.y - rect.top) / scale,
                                        ],
                                    )
                                });
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
) -> Result<Vec<CompanionUsage>, String> {
    let state = app.state::<CompanionState>();
    if refresh == Some(true) {
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

async fn refresh_usage(app: &tauri::AppHandle) -> Vec<CompanionUsage> {
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
                        "openai-codex" | "antigravity" | "opencode-go"
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
                    CompanionUsage {
                        provider_kind: account.provider_kind,
                        usage,
                    }
                });
            }
        }
    }
    if let Ok(preferences) = app.state::<crate::system::SystemState>().preferences() {
        if preferences.claude.enabled && preferences.claude.show_usage {
            let app = app.clone();
            probes.spawn(async move {
                let usage = crate::claude::get_claude_usage(app.state(), app.state())
                    .await
                    .unwrap_or_else(|error| unavailable("Claude Code".into(), error));
                CompanionUsage {
                    provider_kind: "claude-code".into(),
                    usage,
                }
            });
        }
    }
    let mut usage = Vec::new();
    while let Some(result) = probes.join_next().await {
        if let Ok(value) = result {
            usage.push(value);
        }
    }
    usage.sort_by(|a, b| a.usage.alias.cmp(&b.usage.alias));
    usage
}

#[tauri::command]
pub(crate) async fn ack_companion_item(
    app: tauri::AppHandle,
    conversation_id: String,
    agent_id: Option<String>,
    attention_id: String,
    revision: Option<u64>,
) -> Result<crate::agent::companion::Snapshot, crate::agent::AgentError> {
    crate::agent::companion::acknowledge_item(
        app,
        conversation_id,
        agent_id,
        attention_id,
        revision,
    )
    .await
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

    #[test]
    fn companion_file_links_can_reveal_files_without_opening_or_executing_them() {
        let capability: serde_json::Value =
            serde_json::from_str(include_str!("../capabilities/companion.json")).unwrap();
        assert_eq!(capability["webviews"], serde_json::json!(["companion"]));
        assert!(capability.get("remote").is_none());
        let permissions = capability["permissions"].as_array().unwrap();
        assert!(permissions.contains(&serde_json::json!("opener:allow-reveal-item-in-dir")));
        assert!(!permissions.iter().any(|permission| permission
            .as_str()
            .is_some_and(|name| name == "opener:default"
                || name == "opener:allow-open-path"
                || name.starts_with("shell:"))));
    }

    #[test]
    fn dom_blur_does_not_change_native_focus_and_interaction_errors_are_reported() {
        interaction_focus(false, || panic!("Portal blur must not change native focus")).unwrap();
        let mut focused = false;
        interaction_focus(true, || {
            focused = true;
            Ok(())
        })
        .unwrap();
        assert!(focused);
        assert_eq!(
            interaction_focus(true, || Err("Focus unavailable".into())),
            Err("Focus unavailable".into())
        );
    }

    #[test]
    fn companion_usage_preserves_provider_identity_and_the_flat_report() {
        for provider_kind in ["openai-codex", "antigravity", "opencode-go", "claude-code"] {
            let mut usage = unavailable("personal".into(), "offline".into());
            usage.fetched_at = Some(1_000);
            usage.email = Some("test@example.com".into());
            usage.plan = Some("pro".into());
            usage.windows.push(crate::openai_codex::usage::UsageWindow {
                id: "weekly".into(),
                group: "Models".into(),
                third_party: false,
                label: "7d".into(),
                duration_seconds: Some(604_800.0),
                remaining_percent: Some(78.0),
                resets_at: Some(302_401_000),
            });
            let mut expected = serde_json::to_value(&usage).unwrap();
            expected["providerKind"] = provider_kind.into();
            let report = CompanionUsage {
                provider_kind: provider_kind.into(),
                usage,
            };
            assert_eq!(serde_json::to_value(&report).unwrap(), expected);
        }
    }

    fn compact_origin(placement: &Placement, scale: f64) -> CompanionPosition {
        CompanionPosition {
            x: placement.position.x + (placement.geometry.compact_x * scale).round() as i32,
            y: placement.position.y + (placement.geometry.compact_y * scale).round() as i32,
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
    fn new_islands_start_at_the_top_center_and_expand_around_the_same_anchor() {
        for scale in [1.0, 1.5, 2.0] {
            let screen = Screen {
                x: -1920,
                y: 0,
                width: 1920,
                height: 1080,
                scale,
                ..Screen::default()
            };
            let compact = placement(None, &[screen], Presentation::Compact).unwrap();
            assert_eq!(compact.position.y, 0);
            assert_eq!(compact.geometry.width, COMPACT_WIDTH);
            assert_eq!(compact.geometry.height, COMPACT_HEIGHT);
            let expanded =
                placement(Some(compact.anchor), &[screen], Presentation::Island(160)).unwrap();
            assert_eq!(expanded.position.y, 0);
            assert_eq!(expanded.geometry.surface_width, ISLAND_WIDTH);
            assert_eq!(expanded.geometry.surface_height, ISLAND_HEIGHT);
            assert_eq!(expanded.geometry.compact_x, 176.0);
            assert_eq!(expanded.geometry.compact_y, 0.0);
            assert_eq!(compact_origin(&expanded, scale), compact.anchor);
            let closed =
                placement(Some(expanded.anchor), &[screen], Presentation::Compact).unwrap();
            assert_eq!(closed.position, compact.position);
        }
    }

    #[test]
    fn migration_moves_only_the_previous_default_corner() {
        let screen = Screen {
            x: -1920,
            y: 24,
            width: 1920,
            height: 1080,
            scale: 2.0,
            ..Screen::default()
        };
        assert_eq!(
            restore_anchor(Some(CompanionPosition { x: -240, y: 832 }), &[screen]),
            None
        );
        let custom = CompanionPosition { x: -810, y: 301 };
        assert_eq!(restore_anchor(Some(custom), &[screen]), Some(custom));
    }

    #[test]
    fn taller_views_are_explicit_and_visual_hit_bounds_follow_the_animated_island() {
        assert_eq!(requested_height(None).unwrap(), 160);
        assert_eq!(requested_height(Some(400.0)).unwrap(), 400);
        for height in [0.0, 159.0, 601.0, f64::NAN, f64::INFINITY] {
            assert!(requested_height(Some(height)).is_err());
        }
        let compact = [176.0, 0.0, 288.0, 32.0];
        assert_eq!(
            validated_hit_rect(compact, [640.0, 400.0]).unwrap(),
            compact
        );
        assert!(contains_point(compact, [200.0, 20.0]));
        assert!(!contains_point(compact, [50.0, 200.0]));
        for rect in [
            [0.0, 0.0, 0.0, 32.0],
            [0.0, -2.0, 640.0, 400.0],
            [176.0, 0.0, 500.0, 32.0],
            [f64::NAN, 0.0, 288.0, 32.0],
        ] {
            assert!(validated_hit_rect(rect, [640.0, 400.0]).is_err());
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
                ..Screen::default()
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
                        placement(Some(compact.anchor), &[screen], Presentation::Island(160))
                            .unwrap();
                    assert_eq!(compact.anchor, expanded.anchor);
                    assert_eq!(compact_origin(&expanded, scale), compact.anchor);
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
    fn island_fits_short_work_areas_and_preserves_the_compact_anchor() {
        let screen = Screen {
            x: 0,
            y: 40,
            width: 320,
            height: 440,
            scale: 1.0,
            ..Screen::default()
        };
        let expanded = placement(
            Some(CompanionPosition { x: 0, y: 40 }),
            &[screen],
            Presentation::Island(600),
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
            ..Screen::default()
        };
        let restored = placement(
            Some(CompanionPosition { x: -3000, y: 8000 }),
            &[screen],
            Presentation::Compact,
        )
        .unwrap();
        assert_eq!(restored.anchor, CompanionPosition { x: 0, y: 40 });
        assert_eq!(restored.geometry.width, COMPACT_WIDTH);
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
                ..Screen::default()
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
                assert_eq!(compact_origin(&bubble, scale), compact.anchor);
                assert!(bubble.geometry.bubble && !bubble.geometry.expanded);
                let island =
                    placement(Some(bubble.anchor), &[screen], Presentation::Island(400)).unwrap();
                assert_eq!(compact_origin(&island, scale), compact.anchor);
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
    fn legacy_vertical_positions_pin_to_the_top_and_keep_the_horizontal_anchor() {
        for scale in [1.0, 1.5, 2.0] {
            let screen = Screen {
                x: -1600,
                y: 24,
                width: 1600,
                height: 900,
                scale,
                ..Screen::default()
            };
            let right = screen.x + (screen.width - (COMPACT_WIDTH * scale).round() as u32) as i32;
            let bottom =
                screen.y + (screen.height - (COMPACT_HEIGHT * scale).round() as u32) as i32;
            for (x, y, side) in [
                (screen.x, screen.y, "left"),
                (right, screen.y, "right"),
                (screen.x, bottom, "left"),
                (right, bottom, "right"),
            ] {
                let anchor = CompanionPosition { x, y };
                for mode in [
                    Presentation::Compact,
                    Presentation::Bubble,
                    Presentation::Island(400),
                ] {
                    let surface = placement(Some(anchor), &[screen], mode).unwrap();
                    assert_eq!(
                        compact_origin(&surface, scale),
                        CompanionPosition { x, y: screen.y }
                    );
                    assert_eq!(surface.geometry.robot_side, side);
                    assert_eq!(surface.geometry.robot_vertical, "top");
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
    fn island_expands_symmetrically_at_interior_anchors_without_losing_the_compact_position() {
        let screen = Screen {
            x: 0,
            y: 40,
            width: 800,
            height: 800,
            scale: 1.0,
            ..Screen::default()
        };
        let anchor = CompanionPosition { x: 256, y: 40 };
        let island = placement(Some(anchor), &[screen], Presentation::Island(400)).unwrap();
        assert_eq!(island.geometry.width, 640.0);
        assert_eq!(island.geometry.height, 400.0);
        assert_eq!(island.position, PhysicalPosition::new(80, 40));
        assert_eq!(compact_origin(&island, 1.0), anchor);
        let compact = placement(Some(island.anchor), &[screen], Presentation::Compact).unwrap();
        assert_eq!(compact_origin(&compact, 1.0), anchor);
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
    fn horizontal_drag_uses_physical_delta_and_never_changes_the_top_edge() {
        for scale in [1.0, 1.5, 2.0] {
            let screen = Screen {
                x: -1800,
                y: 24,
                width: 1800,
                height: 1200,
                scale,
                ..Screen::default()
            };
            let origin = CompanionPosition { x: -1000, y: 800 };
            for delta in [0, 140, -200, i64::from(i32::MAX), i64::from(i32::MIN)] {
                let anchor = horizontal_anchor(origin, screen, delta);
                assert_eq!(anchor.y, screen.y);
                assert!(anchor.x >= screen.x);
                assert!(anchor.x + (COMPACT_WIDTH * scale).round() as i32 <= 0);
                if (-200..=140).contains(&delta) {
                    assert_eq!(i64::from(anchor.x), i64::from(origin.x) + delta);
                }
                let island = placement(Some(anchor), &[screen], Presentation::Island(400)).unwrap();
                assert_eq!(compact_origin(&island, scale), anchor);
                assert_eq!(island.geometry.drag_axis, "horizontal");
            }
        }
    }

    #[test]
    fn fixed_mac_island_reserves_the_real_camera_and_ignores_legacy_positions() {
        let screen = Screen {
            x: -1512,
            y: -982,
            width: 1512,
            height: 982,
            scale: 1.0,
            fixed: true,
            notch_width: 210.0,
            notch_height: 38.0,
        };
        for anchor in [
            None,
            Some(CompanionPosition { x: -800, y: -100 }),
            Some(CompanionPosition { x: 3000, y: 9000 }),
        ] {
            let compact = placement(anchor, &[screen], Presentation::Compact).unwrap();
            assert_eq!(compact.position, PhysicalPosition::new(-913, -982));
            assert_eq!(compact.geometry.compact_width, 314.0);
            assert_eq!(compact.geometry.compact_height, 38.0);
            assert_eq!(compact.geometry.notch_width, 210.0);
            assert_eq!(compact.geometry.header_height, 38.0);
            assert_eq!(compact.geometry.drag_axis, "none");
            let expanded =
                placement(Some(compact.anchor), &[screen], Presentation::Island(198)).unwrap();
            assert_eq!(expanded.position.y, screen.y);
            assert_eq!(compact_origin(&expanded, 1.0), compact.anchor);
            assert_eq!(expanded.geometry.notch_height, 38.0);
        }
        let external = Screen {
            x: 0,
            y: 0,
            width: 1920,
            height: 1080,
            scale: 1.0,
            fixed: true,
            notch_width: 0.0,
            notch_height: 0.0,
        };
        let restored = placement(
            Some(CompanionPosition { x: -913, y: -982 }),
            &[external],
            Presentation::Island(192),
        )
        .unwrap();
        assert_eq!(restored.position, PhysicalPosition::new(640, 0));
        assert_eq!(restored.geometry.header_height, 32.0);
        assert_eq!(restored.geometry.notch_width, 0.0);
        assert_eq!(restored.geometry.drag_axis, "none");
    }
}
