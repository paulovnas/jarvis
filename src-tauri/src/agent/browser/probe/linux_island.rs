//! Native island checks use the production window builder and companion commands.
use super::*;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use tauri::Listener;
use x11_dl::{xlib, xtest};

pub(super) const PAGE: &str = r#"<!doctype html><title>Island probe</title><style>
html,body{margin:0;background:transparent;color:white;font:16px system-ui}
#island{height:100vh;background:#030303;border-radius:18px;overflow:hidden}
header{height:40px}input{margin:16px;padding:8px}
</style><div id="island"><header>Jarvito native probe</header><input id="name" aria-label="Nome"></div><script>
document.querySelector('#name').addEventListener('input',event=>{window.__probeInputTrusted=event.isTrusted===true});
</script>"#;

#[link(name = "Xext")]
unsafe extern "C" {
    fn XShapeGetRectangles(
        display: *mut xlib::Display,
        window: xlib::Window,
        kind: i32,
        count: *mut i32,
        ordering: *mut i32,
    ) -> *mut xlib::XRectangle;
}

fn xid(window: &tauri::Window) -> Result<xlib::Window, AgentError> {
    match window
        .window_handle()
        .map_err(|_| error("Island handle unavailable"))?
        .as_raw()
    {
        RawWindowHandle::Xlib(handle) => Ok(handle.window as _),
        RawWindowHandle::Xcb(handle) => Ok(handle.window.get() as _),
        _ => Err(error("Island probe requires X11")),
    }
}

fn native(window: &tauri::WebviewWindow) -> Result<Value, AgentError> {
    let window = xid(&window.as_ref().window())?;
    let xlib = xlib::Xlib::open().map_err(|_| error("Probe Xlib unavailable"))?;
    // SAFETY: all calls use this temporary probe-owned connection and free Xlib buffers.
    unsafe {
        let display = (xlib.XOpenDisplay)(std::ptr::null());
        if display.is_null() {
            return Err(error("Probe display unavailable"));
        }
        let hints = (xlib.XGetWMHints)(display, window);
        let focusable = !hints.is_null() && (*hints).input != 0;
        if !hints.is_null() {
            (xlib.XFree)(hints.cast());
        }
        let root = (xlib.XDefaultRootWindow)(display);
        let active = linux::property(&xlib, display, root, "_NET_ACTIVE_WINDOW");
        let state = (xlib.XInternAtom)(display, c"_NET_WM_STATE".as_ptr(), 0);
        let above_atom = (xlib.XInternAtom)(display, c"_NET_WM_STATE_ABOVE".as_ptr(), 0);
        let skip_atom = (xlib.XInternAtom)(display, c"_NET_WM_STATE_SKIP_TASKBAR".as_ptr(), 0);
        let (mut kind, mut format, mut count, mut remaining, mut data) =
            (0, 0, 0, 0, std::ptr::null_mut());
        (xlib.XGetWindowProperty)(
            display,
            window,
            state,
            0,
            64,
            0,
            xlib::XA_ATOM,
            &mut kind,
            &mut format,
            &mut count,
            &mut remaining,
            &mut data,
        );
        let states = if format == 32 && !data.is_null() {
            std::slice::from_raw_parts(data.cast::<xlib::Atom>(), count as usize).to_vec()
        } else {
            Vec::new()
        };
        if !data.is_null() {
            (xlib.XFree)(data.cast());
        }
        let (above, skip_taskbar) = (states.contains(&above_atom), states.contains(&skip_atom));
        let mut regions = Vec::new();
        for kind in [0, 2] {
            let (mut count, mut ordering) = (0, 0);
            let pointer = XShapeGetRectangles(display, window, kind, &mut count, &mut ordering);
            let rows: Vec<_> = if pointer.is_null() {
                Vec::new()
            } else {
                std::slice::from_raw_parts(pointer, count.max(0) as usize)
                    .iter()
                    .map(|row| {
                        [
                            i32::from(row.x),
                            i32::from(row.y),
                            i32::from(row.width),
                            i32::from(row.height),
                        ]
                    })
                    .collect()
            };
            if !pointer.is_null() {
                (xlib.XFree)(pointer.cast());
            }
            regions.push(rows);
        }
        (xlib.XCloseDisplay)(display);
        Ok(
            json!({"window":window,"active":active,"focusable":focusable,"above":above,"skipTaskbar":skip_taskbar,"bounding":regions[0],"input":regions[1]}),
        )
    }
}

fn shape_valid(value: &Value, width: u32, height: u32) -> bool {
    let inside = |point: [i32; 2]| {
        value["input"].as_array().is_some_and(|rects| {
            rects.iter().any(|rect| {
                let coordinates: Vec<_> = rect
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|n| n.as_i64().unwrap() as i32)
                    .collect();
                point[0] >= coordinates[0]
                    && point[0] < coordinates[0] + coordinates[2]
                    && point[1] >= coordinates[1]
                    && point[1] < coordinates[1] + coordinates[3]
            })
        })
    };
    value["bounding"] == value["input"]
        && !inside([0, 0])
        && !inside([0, height as i32 - 1])
        && inside([width as i32 / 2, height as i32 / 2])
}

fn move_pointer(point: [i32; 2], pressed: Option<bool>) -> Result<(), AgentError> {
    let xlib = xlib::Xlib::open().map_err(|_| error("Probe Xlib unavailable"))?;
    let xtest = xtest::Xf86vmode::open().map_err(|_| error("Probe XTest unavailable"))?;
    // SAFETY: input is sent only while one of this probe's own windows is foreground.
    unsafe {
        let display = (xlib.XOpenDisplay)(std::ptr::null());
        if display.is_null() {
            return Err(error("Probe display unavailable"));
        }
        let root = (xlib.XDefaultRootWindow)(display);
        let active = linux::property(&xlib, display, root, "_NET_ACTIVE_WINDOW");
        if active.and_then(|active| linux::property(&xlib, display, active, "_NET_WM_PID"))
            != Some(std::process::id() as _)
        {
            (xlib.XCloseDisplay)(display);
            return Err(error(
                "Island probe refused physical input outside its own windows",
            ));
        }
        (xtest.XTestFakeMotionEvent)(display, -1, point[0], point[1], 0);
        if let Some(pressed) = pressed {
            (xtest.XTestFakeButtonEvent)(display, 1, i32::from(pressed), 0);
        }
        (xlib.XSync)(display, 0);
        (xlib.XCloseDisplay)(display);
    }
    Ok(())
}

pub(super) async fn smoke_actual(app: &tauri::AppHandle) -> Result<(), AgentError> {
    let main = app
        .get_webview_window("main")
        .ok_or_else(|| error("Main window unavailable"))?;
    crate::companion::probe::show_actual(app).map_err(|cause| error(&cause))?;
    let view = app
        .get_webview("companion")
        .ok_or_else(|| error("Real companion unavailable"))?;
    let window = app.get_webview_window("companion").unwrap();
    let mut ready = false;
    for _ in 0..100 {
        if evaluate(
            &view,
            "Boolean(document.querySelector('.companion[data-visible=true] [data-renderer=rive] canvas'))".into(),
        )
        .await
        .is_ok_and(|value| value == true)
        {
            ready = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    if !ready {
        return Err(error("Real Jarvito renderer or Rive canvas did not appear"));
    }
    tokio::time::sleep(Duration::from_millis(500)).await;
    let resolution = "(() => { const c=document.querySelector('.companion-robot-placement [data-renderer=rive] canvas'); return {width:c.width,height:c.height,cssWidth:c.clientWidth,cssHeight:c.clientHeight,dpr:devicePixelRatio}; })()";
    let compact_resolution = evaluate(&view, resolution.into()).await?;
    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.codex/browser-review");
    std::fs::create_dir_all(&directory).map_err(|_| AgentError::storage())?;
    let bytes = capture::capture(&view).await?;
    let image = image::load_from_memory(&bytes)
        .map_err(|_| error("Real Jarvito screenshot invalid"))?
        .to_rgba8();
    let robot_bounds: [f64; 6] = serde_json::from_value(evaluate(&view, "(() => { const r=document.querySelector('.companion-robot-placement [data-renderer=rive] canvas').getBoundingClientRect(); return [r.x,r.y,r.width,r.height,innerWidth,innerHeight]; })()".into()).await?).map_err(|_| error("Real Jarvito paint bounds unavailable"))?;
    let [x, y, width, height, viewport_width, viewport_height] = robot_bounds;
    let (scale_x, scale_y) = (
        f64::from(image.width()) / viewport_width,
        f64::from(image.height()) / viewport_height,
    );
    let robot_pixels = image
        .enumerate_pixels()
        .filter(|(column, row, _)| {
            f64::from(*column) >= x * scale_x
                && f64::from(*column) < (x + width) * scale_x
                && f64::from(*row) >= y * scale_y
                && f64::from(*row) < (y + height) * scale_y
        })
        .map(|(_, _, pixel)| pixel)
        .filter(|pixel| pixel.0[..3].iter().any(|channel| *channel > 32))
        .count();
    if robot_pixels < 20 {
        return Err(error("Real Jarvito canvas did not paint its character"));
    }
    std::fs::write(directory.join("linux-jarvito-compact.png"), bytes)
        .map_err(|_| AgentError::storage())?;
    main.minimize()
        .map_err(|_| error("Main minimization unavailable"))?;
    tokio::time::sleep(Duration::from_millis(250)).await;
    if !window.is_visible().unwrap_or(false)
        || evaluate(&view, "document.hidden".into()).await? != false
    {
        return Err(error(
            "Jarvito disappeared when the main window was minimized",
        ));
    }
    main.unminimize()
        .and_then(|_| main.set_focus())
        .map_err(|_| error("Main focus restoration unavailable"))?;
    tokio::time::sleep(Duration::from_millis(250)).await;
    let position = window
        .outer_position()
        .map_err(|_| error("Real Jarvito position unavailable"))?;
    let size = window
        .inner_size()
        .map_err(|_| error("Real Jarvito size unavailable"))?;
    let point = [
        position.x + size.width as i32 / 2,
        position.y + size.height as i32 / 2,
    ];
    evaluate(&view, "(() => { window.__probePointer=[]; for (const type of ['pointerdown','pointerup','click']) document.addEventListener(type,event => window.__probePointer.push({type:event.type,x:event.clientX,y:event.clientY,target:event.target.className,trusted:event.isTrusted}),true); return true; })()".into()).await?;
    move_pointer(point, Some(true))?;
    tokio::time::sleep(Duration::from_millis(70)).await;
    move_pointer(point, Some(false))?;
    let mut expanded = false;
    for _ in 0..80 {
        if evaluate(
            &view,
            "Boolean(document.querySelector('.companion[data-expanded=true]'))".into(),
        )
        .await?
            == true
        {
            expanded = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    if !expanded {
        let events = evaluate(&view, "window.__probePointer".into()).await?;
        return Err(error(&format!("Physical click did not open the real Jarvito island: position={position:?}, size={size:?}, point={point:?}, events={events}")));
    }
    tokio::time::sleep(Duration::from_millis(800)).await;
    let expanded_resolution = evaluate(&view, resolution.into()).await?;
    for size in [&compact_resolution, &expanded_resolution] {
        let dpr = size["dpr"].as_f64().unwrap_or_default();
        // The installed Rive React runtime caps its drawing pixel ratio at three.
        let drawing_dpr = dpr.clamp(1., 3.);
        if dpr <= 0.
            || size["cssWidth"].as_f64().unwrap_or_default() <= 0.
            || size["cssHeight"].as_f64().unwrap_or_default() <= 0.
            || size["width"].as_f64().unwrap_or_default()
                < (size["cssWidth"].as_f64().unwrap_or_default() * drawing_dpr).floor()
            || size["height"].as_f64().unwrap_or_default()
                < (size["cssHeight"].as_f64().unwrap_or_default() * drawing_dpr).floor()
        {
            return Err(error(&format!(
                "Jarvito canvas lost drawing resolution: {size}"
            )));
        }
    }
    std::fs::write(
        directory.join("linux-jarvito-island.png"),
        capture::capture(&view).await?,
    )
    .map_err(|_| AgentError::storage())?;
    std::fs::write(directory.join("real-linux-island-report.json"), serde_json::to_vec_pretty(&json!({"passed":true,"robotPixels":robot_pixels,"compactResolution":compact_resolution,"expandedResolution":expanded_resolution,"checks":["real React companion renderer","Rive character paint","full Rive canvas resolution through expansion","main minimization preserves island","physical click opens real island"],"native":native(&window)?})).unwrap()).map_err(|_| AgentError::storage())?;
    println!("Real Linux Jarvito and island smoke passed");
    Ok(())
}

pub(super) async fn check(
    app: &tauri::AppHandle,
    base: &str,
    output: &std::path::Path,
) -> Result<(), AgentError> {
    let main = app
        .get_webview_window("main")
        .ok_or_else(|| error("Probe main unavailable"))?;
    let main_view = app.get_webview("main").unwrap();
    evaluate(
        &main_view,
        "(() => { window.__islandProbeNonce='preserved'; return true; })()".into(),
    )
    .await?;
    let main_xid = xid(&main.as_ref().window())?;
    let mut cycles = Vec::new();
    for cycle in 1..=2 {
        println!("Probe: island cycle {cycle} creation");
        let saved_anchor = app
            .state::<crate::desktop::DesktopState>()
            .companion_position();
        main.set_focus()
            .map_err(|_| error("Probe main focus unavailable"))?;
        tokio::time::sleep(Duration::from_millis(200)).await;
        crate::companion::probe::create(app, address(&format!("{base}/island"))?)
            .map_err(|cause| error(&cause))?;
        let window = app.get_webview_window("companion").unwrap();
        let view = app.get_webview("companion").unwrap();
        let collapse = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let observed = collapse.clone();
        let collapse_listener = window.listen("companion:collapse-request", move |_| {
            observed.store(true, std::sync::atomic::Ordering::SeqCst);
        });
        wait_page(&view, "Island probe").await?;
        // Match the real renderer's first geometry update after mapping.
        crate::companion::set_companion_expanded(app.clone(), false, None)
            .map_err(|cause| error(&cause))?;
        for _ in 0..60 {
            if !crate::companion::probe::geometry_pending(app) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        let compact = native(&window)?;
        if let Some(saved) = saved_anchor {
            if window
                .outer_position()
                .map_err(|_| error("Restored anchor unavailable"))?
                != tauri::PhysicalPosition::new(saved.x, saved.y)
            {
                return Err(error(
                    "Reopening the island lost the saved horizontal anchor",
                ));
            }
        }
        let size = window
            .inner_size()
            .map_err(|_| error("Island size unavailable"))?;
        if compact["active"] != json!(main_xid)
            || compact["focusable"] != false
            || compact["above"] != true
            || compact["skipTaskbar"] != true
            || !shape_valid(&compact, size.width, size.height)
        {
            return Err(error(&format!(
                "Compact island stole focus or has an invalid native shape: {compact}"
            )));
        }
        let geometry = crate::companion::set_companion_expanded(app.clone(), true, Some(160.))
            .map_err(|cause| error(&cause))?;
        let geometry =
            serde_json::to_value(geometry).map_err(|_| error("Island geometry unavailable"))?;
        crate::companion::companion_set_hit_rect(
            app.clone(),
            0.,
            0.,
            geometry["width"].as_f64().unwrap(),
            geometry["height"].as_f64().unwrap(),
        )
        .map_err(|cause| error(&cause))?;
        let scale = window
            .scale_factor()
            .map_err(|_| error("Island scale unavailable"))?;
        let expected = tauri::PhysicalSize::new(
            (geometry["width"].as_f64().unwrap() * scale).round() as u32,
            (geometry["height"].as_f64().unwrap() * scale).round() as u32,
        );
        for _ in 0..60 {
            if window.inner_size().ok() == Some(expected)
                && !crate::companion::probe::geometry_pending(app)
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        let expanded = native(&window)?;
        let size = window
            .inner_size()
            .map_err(|_| error("Island size unavailable"))?;
        if expanded["active"] != json!(main_xid) || !shape_valid(&expanded, size.width, size.height)
        {
            return Err(error(&format!("Expanded island invalid: size={size:?}, active={}, main={main_xid}, focusable={}, shapeValid={}",
                expanded["active"], expanded["focusable"], shape_valid(&expanded, size.width, size.height))));
        }
        let position = window
            .outer_position()
            .map_err(|_| error("Island position unavailable"))?;
        main.set_position(tauri::PhysicalPosition::new(position.x - 24, position.y))
            .map_err(|_| error("Probe underlay placement unavailable"))?;
        evaluate(&main_view, "(() => { window.__islandCornerClick=false; document.addEventListener('pointerdown', event => { window.__islandCornerClick=event.isTrusted; }, {once:true}); return true; })()".into()).await?;
        tokio::time::sleep(Duration::from_millis(150)).await;
        let corner = [position.x + 1, position.y + size.height as i32 - 1];
        move_pointer(corner, Some(true))?;
        tokio::time::sleep(Duration::from_millis(70)).await;
        move_pointer(corner, Some(false))?;
        tokio::time::sleep(Duration::from_millis(150)).await;
        if evaluate(&main_view, "window.__islandCornerClick".into()).await? != true
            || !collapse.load(std::sync::atomic::Ordering::SeqCst)
        {
            return Err(error("Transparent island corner blocked the underlying window or did not request collapse"));
        }
        crate::companion::set_companion_expanded(app.clone(), true, Some(160.))
            .map_err(|cause| error(&cause))?;
        collapse.store(false, std::sync::atomic::Ordering::SeqCst);
        let scale = window
            .scale_factor()
            .map_err(|_| error("Island scale unavailable"))?;
        let point = [
            position.x + size.width as i32 / 2,
            position.y + (20. * scale) as i32,
        ];
        move_pointer(point, Some(true))?;
        tokio::time::sleep(Duration::from_millis(70)).await;
        crate::companion::companion_start_drag(app.clone(), None, None)
            .map_err(|cause| error(&cause))?;
        move_pointer([point[0] + 80, point[1]], None)?;
        crate::companion::companion_move_horizontal(app.clone()).map_err(|cause| error(&cause))?;
        tokio::time::sleep(Duration::from_millis(100)).await;
        move_pointer([point[0] + 80, point[1]], Some(false))?;
        crate::companion::companion_finish_drag(app.clone());
        let moved = window
            .outer_position()
            .map_err(|_| error("Moved island position unavailable"))?;
        if moved.x != position.x + 80 || moved.y != position.y {
            return Err(error(&format!(
                "Island drag changed unexpected coordinates: from={position:?}, to={moved:?}"
            )));
        }
        for _ in 0..40 {
            if app
                .state::<crate::desktop::DesktopState>()
                .companion_position()
                != saved_anchor
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        if app
            .state::<crate::desktop::DesktopState>()
            .companion_position()
            == saved_anchor
        {
            return Err(error("Island drag did not persist its horizontal anchor"));
        }
        crate::companion::companion_set_interacting(app.clone(), true)
            .map_err(|cause| error(&cause))?;
        tokio::time::sleep(Duration::from_millis(200)).await;
        let interaction = native(&window)?;
        println!(
            "Probe: island interaction focused={:?}, focusable={}, above={}, active={}",
            window.is_focused(),
            interaction["focusable"],
            interaction["above"],
            interaction["active"]
        );
        linux::check_keyboard(&view, false).await?;
        println!("Probe: island cycle {cycle} restores main focus");
        main.set_focus()
            .map_err(|_| error("Probe focus restoration unavailable"))?;
        tokio::time::sleep(Duration::from_millis(200)).await;
        if !collapse.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(error(
                "Switching away from the island did not request collapse",
            ));
        }
        crate::companion::set_companion_expanded(app.clone(), false, None)
            .map_err(|cause| error(&cause))?;
        println!("Probe: island cycle {cycle} compact geometry restored");
        if native(&window)?["focusable"] != false {
            return Err(error("Collapsed island still accepts native focus"));
        }
        cycles.push(json!({"cycle":cycle,"compact":compact,"expanded":expanded,"dragDx":moved.x-position.x,"keyboard":"trusted Linux input"}));
        window.unlisten(collapse_listener);
        crate::companion::probe::close(app).map_err(|cause| error(&cause))?;
        // The fixture uses CDP outside the normal browser-tab lifecycle.
        cdp::forget(&app.state::<BrowserState>(), view.label());
        println!("Probe: island cycle {cycle} close requested");
        for _ in 0..80 {
            if app.get_webview_window("companion").is_none() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        if app.get_webview_window("companion").is_some() {
            return Err(error("Closed island remains registered"));
        }
        println!("Probe: island cycle {cycle} native window removed");
        if evaluate(&main_view, "window.__islandProbeNonce".into()).await? != "preserved" {
            return Err(error("Island lifecycle changed the main renderer"));
        }
        println!("Probe: island cycle {cycle} main renderer preserved");
    }
    std::fs::write(output.join("native-island-report.json"), serde_json::to_vec_pretty(&json!({"passed":true,"platform":"linux-x11","checks":["production companion builder","rounded visual and input shape","passive show and expansion","horizontal physical drag","trusted physical text input","collapse restores nonactivating window","close and reopen twice","main renderer preserved"],"cycles":cycles})).unwrap()).map_err(|_| AgentError::storage())?;
    println!("Probe: native Linux island passed");
    Ok(())
}
