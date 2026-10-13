//! X11 island presentation. CEF remains windowed, like the main interface.
use super::*;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::{cell::RefCell, sync::LazyLock};
use x11_dl::xlib;

const SHAPE_BOUNDING: i32 = 0;
const SHAPE_INPUT: i32 = 2;

#[link(name = "Xext")]
unsafe extern "C" {
    fn XShapeQueryExtension(display: *mut xlib::Display, event: *mut i32, error: *mut i32) -> i32;
    fn XShapeCombineRectangles(
        display: *mut xlib::Display,
        window: xlib::Window,
        kind: i32,
        x: i32,
        y: i32,
        rectangles: *mut xlib::XRectangle,
        count: i32,
        operation: i32,
        ordering: i32,
    );
}

static XLIB: LazyLock<Result<xlib::Xlib, String>> =
    LazyLock::new(|| xlib::Xlib::open().map_err(|error| error.to_string()));

struct Display(*mut xlib::Display);

impl Drop for Display {
    fn drop(&mut self) {
        if let Ok(xlib) = &*XLIB {
            // SAFETY: this thread owns the connection until its local storage is dropped.
            unsafe { (xlib.XCloseDisplay)(self.0) };
        }
    }
}

thread_local! {
    static DISPLAY: RefCell<Option<Display>> = const { RefCell::new(None) };
}

fn with_x11<T>(
    operation: impl FnOnce(&xlib::Xlib, *mut xlib::Display) -> Result<T, String>,
) -> Result<T, String> {
    let xlib = XLIB.as_ref().map_err(Clone::clone)?;
    DISPLAY.with(|cell| {
        let mut connection = cell.borrow_mut();
        if connection.is_none() {
            // SAFETY: null selects the session's DISPLAY; the connection stays on this thread.
            let display = unsafe { (xlib.XOpenDisplay)(std::ptr::null()) };
            if display.is_null() {
                return Err("O assistente flutuante precisa de uma sessão X11 disponível.".into());
            }
            *connection = Some(Display(display));
        }
        let display = connection.as_ref().unwrap().0;
        let result = operation(xlib, display);
        // SAFETY: all requests above use this live, thread-owned connection.
        unsafe { (xlib.XFlush)(display) };
        result
    })
}

pub(super) fn xid(window: &tauri::Window) -> Result<xlib::Window, String> {
    match window
        .window_handle()
        .map_err(|error| error.to_string())?
        .as_raw()
    {
        RawWindowHandle::Xlib(handle) => Ok(handle.window as _),
        RawWindowHandle::Xcb(handle) => Ok(handle.window.get() as _),
        _ => Err("O assistente flutuante precisa de uma janela X11.".into()),
    }
}

fn rounded_rect(rect: [f64; 4], scale: f64) -> Vec<xlib::XRectangle> {
    let [x, y, width, height] = rect.map(|value| value * scale);
    let (left, top, right, bottom) = (
        x.floor(),
        y.floor(),
        (x + width).ceil(),
        (y + height).ceil(),
    );
    let radius = (height.min(width).min(56. * scale) / 2.).max(0.5);
    (top as i32..bottom as i32)
        .map(|row| {
            let edge = (f64::from(row) + 0.5 - top).min(bottom - f64::from(row) - 0.5);
            let inset = if edge < radius {
                (radius - (radius * radius - (radius - edge).powi(2)).max(0.).sqrt() - 0.5)
                    .ceil()
                    .max(0.)
            } else {
                0.
            };
            xlib::XRectangle {
                x: (left + inset) as i16,
                y: row as i16,
                width: (right - left - inset * 2.).max(1.) as u16,
                height: 1,
            }
        })
        .collect()
}

pub(super) fn apply_hit_rect(window: &tauri::WebviewWindow, rect: [f64; 4]) -> Result<(), String> {
    let scale = window.scale_factor().map_err(|error| error.to_string())?;
    let window = xid(&window.as_ref().window())?;
    let mut rectangles = rounded_rect(rect, scale);
    with_x11(|_, display| {
        let (mut event, mut error) = (0, 0);
        // SAFETY: the extension receives live stack values and copies the rectangle buffer.
        unsafe {
            if XShapeQueryExtension(display, &mut event, &mut error) == 0 {
                return Err(
                    "O servidor gráfico não oferece o recorte necessário para a ilha.".into(),
                );
            }
            for kind in [SHAPE_BOUNDING, SHAPE_INPUT] {
                XShapeCombineRectangles(
                    display,
                    window,
                    kind,
                    0,
                    0,
                    rectangles.as_mut_ptr(),
                    rectangles.len() as i32,
                    0,
                    xlib::YXBanded,
                );
            }
        }
        Ok(())
    })
}

struct Pointer {
    x: i32,
    y: i32,
    pressed: bool,
    foreground: xlib::Window,
}

fn pointer() -> Result<Pointer, String> {
    with_x11(|xlib, display| {
        // SAFETY: read-only root-window queries, independent of disposable companion XIDs.
        unsafe {
            let root = (xlib.XDefaultRootWindow)(display);
            let (mut returned_root, mut child, mut x, mut y, mut local_x, mut local_y, mut mask) =
                (0, 0, 0, 0, 0, 0, 0);
            if (xlib.XQueryPointer)(
                display,
                root,
                &mut returned_root,
                &mut child,
                &mut x,
                &mut y,
                &mut local_x,
                &mut local_y,
                &mut mask,
            ) == 0
            {
                return Err("Posição do cursor indisponível.".into());
            }
            let atom = (xlib.XInternAtom)(display, c"_NET_ACTIVE_WINDOW".as_ptr(), 0);
            let (mut kind, mut format, mut count, mut remaining, mut data) =
                (0, 0, 0, 0, std::ptr::null_mut());
            (xlib.XGetWindowProperty)(
                display,
                root,
                atom,
                0,
                1,
                0,
                xlib::XA_WINDOW,
                &mut kind,
                &mut format,
                &mut count,
                &mut remaining,
                &mut data,
            );
            let foreground = if format == 32 && count == 1 && !data.is_null() {
                *data.cast::<xlib::Window>()
            } else {
                0
            };
            if !data.is_null() {
                (xlib.XFree)(data.cast());
            }
            Ok(Pointer {
                x,
                y,
                pressed: mask & (xlib::Button1Mask | xlib::Button2Mask | xlib::Button3Mask) != 0,
                foreground,
            })
        }
    })
}

pub(super) fn cursor_x() -> Result<i32, String> {
    Ok(pointer()?.x)
}

fn contains_shape_point(rect: [f64; 4], point: [i32; 2], scale: f64) -> bool {
    rounded_rect(rect, scale).iter().any(|row| {
        point[0] >= i32::from(row.x)
            && point[0] < i32::from(row.x) + i32::from(row.width)
            && point[1] >= i32::from(row.y)
            && point[1] < i32::from(row.y) + i32::from(row.height)
    })
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
                let mut previous = pointer().ok();
                while state.enabled.load(Ordering::SeqCst)
                    && (state.expanded.load(Ordering::SeqCst)
                        || state.dragging.load(Ordering::SeqCst))
                {
                    if let Ok(current) = pointer() {
                        if state.dragging.load(Ordering::SeqCst) && !current.pressed {
                            finish_drag(&app);
                        }
                        if let Some(previous) = previous {
                            let changed = current.foreground != 0
                                && current.foreground != previous.foreground
                                && Some(current.foreground) != xid(&window.as_ref().window()).ok();
                            let outside = current.pressed
                                && !previous.pressed
                                && window.outer_position().is_ok_and(|position| {
                                    let scale = window.scale_factor().unwrap_or(1.);
                                    !state.hit_rect.lock().ok().and_then(|hit| *hit).is_some_and(
                                        |hit| {
                                            contains_shape_point(
                                                hit,
                                                [current.x - position.x, current.y - position.y],
                                                scale,
                                            )
                                        },
                                    )
                                });
                            if changed || outside {
                                collapse_on_external_interaction(&app);
                            }
                        }
                        previous = Some(current);
                    }
                    // Match the Windows watch; no polling while compact and idle.
                    tokio::time::sleep(Duration::from_millis(25)).await;
                }
            }
        });
    }
    state.pointer_wake.notify_one();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clicking_a_transparent_corner_is_external_even_when_foreground_does_not_change() {
        assert!(!contains_shape_point([0., 0., 640., 160.], [1, 159], 1.));
        assert!(contains_shape_point([0., 0., 640., 160.], [320, 80], 1.));
        assert!(!contains_shape_point(
            [12.4, 8.2, 400.5, 160.3],
            [15, 10],
            1.25
        ));
        assert!(contains_shape_point(
            [12.4, 8.2, 400.5, 160.3],
            [300, 120],
            1.25
        ));
    }

    #[test]
    fn rounded_shape_leaves_corners_and_transparent_canvas_click_through() {
        let rows = rounded_rect([176., 0., 288., 32.], 1.);
        assert_eq!(rows.len(), 32);
        assert!(rows[0].x > 176);
        assert!(rows[0].width < 288);
        assert_eq!((rows[16].x, rows[16].width), (176, 288));
        assert!(rows
            .iter()
            .all(|row| row.x >= 176 && i32::from(row.x) + i32::from(row.width) <= 464));
        assert_eq!(rows[0].x, rows[31].x);
    }

    #[test]
    fn shape_tracks_animated_offsets_and_fractional_monitor_scale() {
        let rows = rounded_rect([12.4, 8.2, 400.5, 160.3], 1.25);
        assert_eq!(rows.first().unwrap().y, 10);
        assert_eq!(rows.last().unwrap().y, 210);
        assert_eq!((rows[100].x, rows[100].width), (15, 502));
        assert!(rows.iter().all(|row| row.height == 1 && row.width > 0));
    }
}
