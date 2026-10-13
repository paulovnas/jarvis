//! Physical X11 input evidence, confined to the foreground probe window.
use super::*;
use cef::{ImplBrowser, ImplBrowserHost};
use x11_dl::{xlib, xtest};

fn main_cpu_ticks() -> Result<u64, AgentError> {
    let stat = std::fs::read_to_string(format!("/proc/self/task/{}/stat", std::process::id()))
        .map_err(|_| error("Probe main-thread CPU statistics unavailable"))?;
    let (_, fields) = stat
        .rsplit_once(") ")
        .ok_or_else(|| error("Invalid probe CPU statistics"))?;
    let mut times = fields.split_whitespace().skip(11);
    let parse = |value: Option<&str>| {
        value
            .and_then(|value| value.parse::<u64>().ok())
            .ok_or_else(|| error("Invalid probe CPU time"))
    };
    Ok(parse(times.next())? + parse(times.next())?)
}

pub(super) async fn check_idle() -> Result<(), AgentError> {
    let ticks_per_second = unsafe { libc::sysconf(libc::_SC_CLK_TCK) };
    if ticks_per_second <= 0 {
        return Err(error("Probe CPU clock unavailable"));
    }
    let start = std::time::Instant::now();
    let before = main_cpu_ticks()?;
    tokio::time::sleep(Duration::from_secs(1)).await;
    let cpu = (main_cpu_ticks()? - before) as f64
        / ticks_per_second as f64
        / start.elapsed().as_secs_f64();
    println!("Probe: idle main-thread CPU {:.1}%", cpu * 100.);
    if cpu > 0.5 {
        return Err(error("Native event loop busy-polls while idle"));
    }
    Ok(())
}

pub(super) unsafe fn property(
    xlib: &xlib::Xlib,
    display: *mut xlib::Display,
    window: xlib::Window,
    name: &str,
) -> Option<libc::c_ulong> {
    let name = std::ffi::CString::new(name).ok()?;
    let atom = unsafe { (xlib.XInternAtom)(display, name.as_ptr(), 0) };
    let (mut kind, mut format, mut count, mut remaining) = (0, 0, 0, 0);
    let mut data = std::ptr::null_mut();
    let status = unsafe {
        (xlib.XGetWindowProperty)(
            display,
            window as _,
            atom,
            0,
            1,
            0,
            0,
            &mut kind,
            &mut format,
            &mut count,
            &mut remaining,
            &mut data,
        )
    };
    let value = if status == 0 && format == 32 && count == 1 && !data.is_null() {
        Some(unsafe { *data.cast::<libc::c_ulong>() })
    } else {
        None
    };
    if !data.is_null() {
        unsafe { (xlib.XFree)(data.cast()) };
    }
    value
}

pub(super) async fn check_keyboard(view: &Webview, activate: bool) -> Result<(), AgentError> {
    let point: [f64; 2] = serde_json::from_value(cdp_action(view, json!({"action":"evaluate","expression":"(() => { const input = document.querySelector('#name'); input.value = ''; window.__probeInputTrusted = false; const r = input.getBoundingClientRect(); return [r.x + r.width / 2, r.y + r.height / 2]; })()"})).await?)
        .map_err(|_| error("Physical keyboard probe input bounds unavailable"))?;
    let window = view.window();
    let scale = window
        .scale_factor()
        .map_err(|_| error("Probe window scale unavailable"))?;
    let (tx, rx) = tokio::sync::oneshot::channel();
    view.with_cef_webview(move |native| {
        let _ = tx.send(native.browser().host().map(|host| host.window_handle()));
    })
    .map_err(|_| error("Probe native window unavailable"))?;
    let xid = tokio::time::timeout(Duration::from_secs(5), rx)
        .await
        .ok()
        .and_then(Result::ok)
        .flatten()
        .filter(|xid| *xid != 0)
        .ok_or_else(|| error("Probe native window handle unavailable"))?;
    if activate {
        window
            .set_focus()
            .map_err(|_| error("Probe window focus unavailable"))?;
    }
    tokio::time::sleep(Duration::from_millis(200)).await;

    {
        let xlib =
            xlib::Xlib::open().map_err(|_| error("X11 unavailable for physical keyboard probe"))?;
        let xtest = xtest::Xf86vmode::open()
            .map_err(|_| error("XTest unavailable for physical keyboard probe"))?;
        let display = unsafe { (xlib.XOpenDisplay)(std::ptr::null()) };
        if display.is_null() {
            return Err(error("X11 display unavailable for physical keyboard probe"));
        }
        let result = (|| unsafe {
            let root = (xlib.XDefaultRootWindow)(display);
            let active = property(&xlib, display, root, "_NET_ACTIVE_WINDOW");
            if active.and_then(|window| property(&xlib, display, window, "_NET_WM_PID"))
                != Some(std::process::id() as _)
            {
                return Err(error(
                    "Physical keyboard probe refused input outside its own window",
                ));
            }
            let (mut x, mut y, mut child) = (0, 0, 0);
            // GTK's client origin can include decorations; translate the CEF child itself.
            if (xlib.XTranslateCoordinates)(
                display,
                xid as _,
                root,
                (point[0] * scale).round() as i32,
                (point[1] * scale).round() as i32,
                &mut x,
                &mut y,
                &mut child,
            ) == 0
            {
                return Err(error("Probe native input coordinates unavailable"));
            }
            let point = [x, y];
            (xtest.XTestFakeMotionEvent)(display, -1, point[0], point[1], 0);
            (xtest.XTestFakeButtonEvent)(display, 1, 1, 0);
            (xtest.XTestFakeButtonEvent)(display, 1, 0, 0);
            (xlib.XSync)(display, 0);
            std::thread::sleep(Duration::from_millis(200));
            if activate {
                window
                    .set_focus()
                    .map_err(|_| error("Probe window reactivation unavailable"))?;
                std::thread::sleep(Duration::from_millis(200));
                let other = tauri::WindowBuilder::new(view.app_handle(), "probe-focus")
                    .title("Jarvis keyboard probe")
                    .inner_size(240., 120.)
                    .build()
                    .map_err(|_| error("Probe focus-loss window unavailable"))?;
                other
                    .set_focus()
                    .map_err(|_| error("Probe focus loss unavailable"))?;
                std::thread::sleep(Duration::from_millis(200));
                window
                    .set_focus()
                    .map_err(|_| error("Probe window restore unavailable"))?;
                std::thread::sleep(Duration::from_millis(200));
                other
                    .close()
                    .map_err(|_| error("Probe focus-loss window close unavailable"))?;
                std::thread::sleep(Duration::from_millis(100));
            }
            let (mut focus, mut revert) = (0, 0);
            (xlib.XGetInputFocus)(display, &mut focus, &mut revert);
            println!(
                "Probe: physical input point {point:?}, OS focus {focus:#x}, active window {active:?}"
            );
            Ok(())
        })();
        unsafe { (xlib.XCloseDisplay)(display) };
        result?;
    }
    for character in b"linux" {
        // IME proxies can receive X11 focus outside the browser's native subtree.
        let focused = cdp_action(view, json!({"action":"evaluate","expression":"({focused:document.hasFocus(),active:document.activeElement.id})"})).await?;
        if focused["focused"] != true || focused["active"] != "name" {
            return Err(error(&format!(
                "Physical keyboard probe target lost focus: {focused}"
            )));
        }
        let xlib = xlib::Xlib::open().map_err(|_| error("Probe X11 unavailable"))?;
        let xtest = xtest::Xf86vmode::open().map_err(|_| error("Probe XTest unavailable"))?;
        let display = unsafe { (xlib.XOpenDisplay)(std::ptr::null()) };
        if display.is_null() {
            return Err(error("Probe X11 display unavailable"));
        }
        let result = (|| unsafe {
            let root = (xlib.XDefaultRootWindow)(display);
            let owner = property(&xlib, display, root, "_NET_ACTIVE_WINDOW")
                .and_then(|active| property(&xlib, display, active, "_NET_WM_PID"));
            if owner != Some(std::process::id() as _) {
                return Err(error(
                    "Physical keyboard probe refused keys outside its own window",
                ));
            }
            let key = (xlib.XKeysymToKeycode)(display, *character as _);
            if key == 0 {
                return Err(error("Physical keyboard probe key unavailable"));
            }
            (xtest.XTestFakeKeyEvent)(display, key as _, 1, 0);
            (xtest.XTestFakeKeyEvent)(display, key as _, 0, 0);
            (xlib.XSync)(display, 0);
            Ok(())
        })();
        unsafe { (xlib.XCloseDisplay)(display) };
        result?;
    }
    tokio::time::sleep(Duration::from_millis(300)).await;
    let evidence = cdp_action(view, json!({"action":"evaluate","expression":"({value:document.querySelector('#name').value,trusted:window.__probeInputTrusted,active:document.activeElement.id,pointer:window.__probePointer,key:window.__probeKey})"})).await?;
    if evidence["value"] != "linux" || evidence["trusted"] != true {
        return Err(error(&format!(
            "Physical X11 keyboard did not reach the page: {evidence}"
        )));
    }
    println!("Probe: physical X11 keyboard passed");
    Ok(())
}

pub(super) async fn check_stacking(view: &Webview) -> Result<(), AgentError> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    view.with_cef_webview(move |native| {
        let _ = tx.send(native.browser().host().map(|host| host.window_handle()));
    })
    .map_err(|_| error("Probe browser window unavailable"))?;
    let xid = tokio::time::timeout(Duration::from_secs(5), rx)
        .await
        .ok()
        .and_then(Result::ok)
        .flatten()
        .filter(|xid| *xid != 0)
        .ok_or_else(|| error("Probe browser handle unavailable"))? as xlib::Window;
    let xlib = xlib::Xlib::open().map_err(|_| error("Probe X11 unavailable"))?;
    let display = unsafe { (xlib.XOpenDisplay)(std::ptr::null()) };
    if display.is_null() {
        return Err(error("Probe X11 display unavailable"));
    }
    let result = (|| unsafe {
        let (mut root, mut parent, mut children, mut count) = (0, 0, std::ptr::null_mut(), 0);
        if (xlib.XQueryTree)(
            display,
            xid,
            &mut root,
            &mut parent,
            &mut children,
            &mut count,
        ) == 0
        {
            return Err(error("Probe browser parent unavailable"));
        }
        if !children.is_null() {
            (xlib.XFree)(children.cast());
        }
        children = std::ptr::null_mut();
        if (xlib.XQueryTree)(
            display,
            parent,
            &mut root,
            &mut parent,
            &mut children,
            &mut count,
        ) == 0
        {
            return Err(error("Probe browser stacking unavailable"));
        }
        // XQueryTree lists siblings from bottom to top; unmapped tabs cannot cover the page.
        let top = (0..count).rev().find_map(|index| {
            let window = *children.add(index as usize);
            let mut attributes = std::mem::MaybeUninit::<xlib::XWindowAttributes>::uninit();
            ((xlib.XGetWindowAttributes)(display, window, attributes.as_mut_ptr()) != 0
                && attributes.assume_init().map_state == xlib::IsViewable)
                .then_some(window)
        });
        if !children.is_null() {
            (xlib.XFree)(children.cast());
        }
        if top != Some(xid) {
            return Err(error(
                "The main renderer covered the browser after focus changed",
            ));
        }
        Ok(())
    })();
    unsafe { (xlib.XCloseDisplay)(display) };
    result?;
    println!("Probe: browser remains above the focused main renderer");
    Ok(())
}
