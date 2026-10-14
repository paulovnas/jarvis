//! Opt-in smoke checks of the real React island and its production Win32 window.
use super::*;
use windows_sys::Win32::{
    Foundation::RECT,
    Graphics::Gdi::GetWindowRgnBox,
    UI::WindowsAndMessaging::{
        GetWindowLongPtrW, GetWindowRect, GWL_EXSTYLE, GWL_STYLE, WS_CAPTION, WS_EX_TOPMOST,
        WS_MAXIMIZEBOX, WS_MINIMIZEBOX, WS_SYSMENU, WS_THICKFRAME,
    },
};

const RESOLUTION: &str = "(() => { const c=document.querySelector('.companion-robot-placement [data-renderer=rive] canvas'); return {width:c.width,height:c.height,cssWidth:c.clientWidth,cssHeight:c.clientHeight,dpr:devicePixelRatio}; })()";

fn full_resolution(value: &Value) -> bool {
    let dpr = value["dpr"].as_f64().unwrap_or_default();
    let width = value["cssWidth"].as_f64().unwrap_or_default();
    let height = value["cssHeight"].as_f64().unwrap_or_default();
    // Rive caps its drawing pixel ratio at three, independently of CSS transforms.
    dpr > 0.
        && width >= 80.
        && height >= 80.
        && value["width"].as_f64().unwrap_or_default() >= (width * dpr.clamp(1., 3.)).floor()
        && value["height"].as_f64().unwrap_or_default() >= (height * dpr.clamp(1., 3.)).floor()
}

fn native(window: &tauri::WebviewWindow) -> Result<Value, AgentError> {
    let handle = window
        .hwnd()
        .map_err(|_| error("Island HWND unavailable"))?;
    // SAFETY: the smoke owns this live window and only reads its native styles.
    let (style, extended) = unsafe {
        (
            GetWindowLongPtrW(handle.0 as _, GWL_STYLE) as u32,
            GetWindowLongPtrW(handle.0 as _, GWL_EXSTYLE) as u32,
        )
    };
    let frame = WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX | WS_MAXIMIZEBOX | WS_THICKFRAME;
    if style & frame != 0 || extended & WS_EX_TOPMOST == 0 {
        return Err(error(&format!(
            "Island native chrome reappeared: style={style:#x}, extended={extended:#x}"
        )));
    }
    let (mut bounds, mut region) = (RECT::default(), RECT::default());
    // SAFETY: both APIs only write to these live stack rectangles.
    if unsafe { GetWindowRect(handle.0 as _, &mut bounds) } == 0
        || unsafe { GetWindowRgnBox(handle.0 as _, &mut region) } == 0
    {
        return Err(error("Island native bounds or clipping region unavailable"));
    }
    let size = [bounds.right - bounds.left, bounds.bottom - bounds.top];
    if region.right - region.left >= size[0] && region.bottom - region.top >= size[1] {
        return Err(error("Island exposed its entire canvas during transition"));
    }
    Ok(json!({"style":style,"extendedStyle":extended,
        "bounds":[bounds.left,bounds.top,size[0],size[1]],
        "region":[region.left,region.top,region.right-region.left,region.bottom-region.top]}))
}

pub(super) async fn smoke_actual(app: &tauri::AppHandle) -> Result<(), AgentError> {
    crate::companion::probe::show_actual(app).map_err(|cause| error(&cause))?;
    let view = app
        .get_webview("companion")
        .ok_or_else(|| error("Real companion unavailable"))?;
    let window = app.get_webview_window("companion").unwrap();
    let mut ready = false;
    for _ in 0..100 {
        if evaluate(&view, "Boolean(document.querySelector('.companion[data-visible=true] [data-renderer=rive] canvas'))".into())
            .await.is_ok_and(|value| value == true)
        {
            ready = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    if !ready {
        return Err(error("Real Jarvito Rive canvas did not appear"));
    }
    let main = app
        .get_webview_window("main")
        .ok_or_else(|| error("Main window unavailable"))?;
    for _ in 0..100 {
        if main.is_visible().unwrap_or(false) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    main.set_focus()
        .map_err(|_| error("Main focus unavailable"))?;
    tokio::time::sleep(Duration::from_millis(500)).await;
    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.codex/browser-review");
    std::fs::create_dir_all(&directory).map_err(|_| AgentError::storage())?;
    let compact = evaluate(&view, RESOLUTION.into()).await?;
    if !full_resolution(&compact) {
        return Err(error(&format!(
            "Compact Jarvito lost drawing resolution: {compact}"
        )));
    }
    std::fs::write(
        directory.join("windows-jarvito-compact.png"),
        capture::capture(&view).await?,
    )
    .map_err(|_| AgentError::storage())?;
    let initial_native = native(&window)?;
    let mut transitions = Vec::new();
    for expanded in [true, false, true, false] {
        println!("Real Windows island: expanded={expanded}");
        crate::companion::set_companion_expanded(app.clone(), expanded, None)
            .map_err(|cause| error(&cause))?;
        // Sample during the animation as well as after it settles.
        for _ in 0..50 {
            let observed = native(&window)?;
            if observed["bounds"] != initial_native["bounds"] {
                return Err(error(
                    "Island moved or resized its native canvas during animation",
                ));
            }
            tokio::time::sleep(Duration::from_millis(16)).await;
        }
        let resolution = evaluate(&view, RESOLUTION.into()).await?;
        if !full_resolution(&resolution) {
            return Err(error(&format!(
                "Jarvito lost drawing resolution in transition: {resolution}"
            )));
        }
        let rendered = evaluate(
            &view,
            "Boolean(document.querySelector('.companion[data-expanded=true]'))".into(),
        )
        .await?;
        if rendered != expanded {
            return Err(error(&format!("React island did not follow its native geometry: expanded={expanded}, rendered={rendered}, mainFocused={:?}, companionFocused={:?}, native={}", main.is_focused(),window.is_focused(),native(&window)?)));
        }
        transitions
            .push(json!({"expanded":expanded,"resolution":resolution,"native":native(&window)?}));
        if expanded {
            std::fs::write(
                directory.join("windows-jarvito-island.png"),
                capture::capture(&view).await?,
            )
            .map_err(|_| AgentError::storage())?;
        }
    }
    std::fs::write(directory.join("real-windows-island-report.json"), serde_json::to_vec_pretty(&json!({
        "passed":true,"compactResolution":compact,"native":initial_native,"transitions":transitions,
        "checks":["real React companion renderer","full Rive canvas resolution through expansion", "native chrome absent throughout repeated transitions", "stable native canvas and clipping throughout animation", "always-on-top preserved"]
    })).map_err(|_| AgentError::storage())?).map_err(|_| AgentError::storage())?;
    println!("Real Windows Jarvito and island smoke passed");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drawing_resolution_rejects_a_bitmap_shrunk_to_the_compact_transform() {
        for dpr in [1., 1.25, 2., 4.] {
            let full = 80. * f64::clamp(dpr, 1., 3.);
            assert!(full_resolution(
                &json!({"width":full,"height":full,"cssWidth":80.,"cssHeight":80.,"dpr":dpr})
            ));
            assert!(!full_resolution(
                &json!({"width":28. * dpr,"height":28. * dpr,"cssWidth":80.,"cssHeight":80.,"dpr":dpr})
            ));
        }
        assert!(!full_resolution(
            &json!({"width":80.,"height":80.,"cssWidth":0.,"cssHeight":0.,"dpr":1.})
        ));
    }
}
