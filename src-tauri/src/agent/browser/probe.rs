//! Opt-in executable integration probe. Never built into the normal application.
use super::*;
use std::io::{Read, Write};
use tauri_runtime_cef::WebviewCefExt;

#[cfg(target_os = "macos")]
mod island;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub(crate) mod linux_agent;
#[cfg(target_os = "linux")]
mod linux_island;
#[cfg(target_os = "windows")]
mod windows_island;

#[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
pub(crate) fn observe_companion(app: &tauri::AppHandle) {
    #[cfg(any(target_os = "linux", target_os = "windows"))]
    if std::env::var("JARVIS_COMPANION_SMOKE").is_ok_and(|value| value == "1") {
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            #[cfg(target_os = "linux")]
            let result = linux_island::smoke_actual(&app).await;
            #[cfg(target_os = "windows")]
            let result = windows_island::smoke_actual(&app).await;
            // This probe owns a full app; granting exit also marks it prepared.
            // Run the production cleanup before requesting native teardown.
            if let Err(cause) = crate::prepare_exit(&app) {
                eprintln!("Real companion shutdown failed: {cause}");
                app.exit(1);
                return;
            }
            match result {
                Ok(()) => app.exit(0),
                Err(cause) => {
                    eprintln!("Real companion failed: {}", cause.message);
                    app.exit(1);
                }
            }
        });
    }
    #[cfg(target_os = "macos")]
    if std::env::var("JARVIS_COMPANION_OBSERVE").is_ok_and(|value| value == "1") {
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            if let Err(cause) = island::observe_companion(&app).await {
                eprintln!("Companion observation failed: {}", cause.message);
            }
        });
    }
}

const PAGE: &str = r#"<!doctype html><html><head><title>Browser probe</title><style>body{background:#181b20;color:#d7dce5;font:18px system-ui;padding:32px}button,input{font:inherit;padding:12px;border-radius:6px}h1{color:#61afef}</style></head><body><h1>Jarvis native browser</h1><label>Nome <input id="name" aria-label="Nome"></label><button id="action">Testar</button><p id="result">Pronto</p><a href="/next">Próxima página</a><script>
window.__probeInputTrusted=false;
window.__probeClickTrusted=false;
document.addEventListener('pointerdown',event=>{window.__probePointer={target:event.target.id,x:event.clientX,y:event.clientY,trusted:event.isTrusted}},true);
document.addEventListener('keydown',event=>{window.__probeKey={key:event.key,target:event.target.id,trusted:event.isTrusted}},true);
document.querySelector('#name').addEventListener('input',event=>{
  window.__probeInputTrusted=event.isTrusted===true;
});
document.querySelector('#action').addEventListener('click',async event=>{
  window.__probeClickTrusted=event.isTrusted===true;
  if(!window.__probeClickTrusted||!window.__probeInputTrusted){
    document.querySelector('#result').textContent='Entrada não confiável';
    return;
  }
  document.querySelector('#result').textContent='Olá '+document.querySelector('#name').value;
  console.error('probe console evidence');
  try{
    const response=await fetch('/api/probe');
    window.__probeFetch=await response.json();
  }catch(cause){window.__probeFetchError=String(cause);}
});
console.info('probe loaded');
</script></body></html>"#;
const NETWORK_BODY: &str = r#"{"ok":true,"evidence":"jarvis-probe-network"}"#;
const INPUT_EVIDENCE: &str = "({inputTrusted:window.__probeInputTrusted,clickTrusted:window.__probeClickTrusted,value:document.querySelector('#name').value,fetch:window.__probeFetch,fetchError:window.__probeFetchError})";
const HOST: &str = r#"<!doctype html><html><head><title>Browser host</title><style>
html,body{margin:0;height:100%;color:#d7dce5;font:16px system-ui}
body{display:grid;grid-template-columns:100px minmax(0,1fr) 120px;grid-template-rows:80px minmax(0,1fr) 80px;background:#14171b}
header,footer{grid-column:1/-1;background:#292e37;padding:20px;box-sizing:border-box}
aside{padding:12px;box-sizing:border-box}main{background:#61afef;min-width:0;min-height:0}
</style></head><body><header>Jarvis browser integration probe</header><aside>Workspace</aside><main id="viewport"></main><aside>Inspector</aside><footer><label>Nome <input id="name"></label></footer><script>
document.querySelector('#name').addEventListener('input',event=>{window.__probeInputTrusted=event.isTrusted===true});
</script></body></html>"#;

fn server() -> (String, std::net::TcpListener) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind fixture server");
    (
        format!("http://{}", listener.local_addr().unwrap()),
        listener,
    )
}

fn fixture_response(request: &str) -> (&'static str, &'static str) {
    #[cfg(target_os = "linux")]
    if request.starts_with("GET /island ") {
        return ("text/html; charset=utf-8", linux_island::PAGE);
    }
    #[cfg(target_os = "macos")]
    if request.starts_with("GET /island ") {
        return ("text/html; charset=utf-8", island::PAGE);
    }
    if request.starts_with("GET /api/probe ") {
        ("application/json; charset=utf-8", NETWORK_BODY)
    } else {
        let page = if request.starts_with("GET /next ") {
            "<!doctype html><title>Next page</title><h1>Navigation works</h1>"
        } else if request.starts_with("GET /main ") {
            HOST
        } else {
            PAGE
        };
        ("text/html; charset=utf-8", page)
    }
}

async fn cdp_action(view: &Webview, value: Value) -> Result<Value, AgentError> {
    let request: BrowserRequest =
        serde_json::from_value(value).map_err(|_| error("Invalid probe Chromium request"))?;
    cdp::command(view, &request).await
}

async fn wait_page(view: &Webview, expected: &str) -> Result<Value, AgentError> {
    for _ in 0..80 {
        if let Ok(value) = cdp_action(view, json!({"action":"snapshot"})).await {
            if value["title"].as_str() == Some(expected) {
                return Ok(value);
            }
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    Err(error("Probe page did not load"))
}

fn native_bounds_match(bounds: [f64; 4], rect: &Viewport) -> bool {
    // Native bounds may round to physical pixels at fractional display scales.
    bounds
        .into_iter()
        .zip([rect.x, rect.y, rect.width, rect.height])
        .all(|(observed, expected)| (observed - expected).abs() < 1.)
}

fn page_bounds_match(bounds: &Value, rect: &Viewport) -> bool {
    serde_json::from_value::<[f64; 4]>(bounds.clone())
        .is_ok_and(|bounds| native_bounds_match(bounds, rect))
}

async fn native_snapshot(view: &Webview) -> Result<tauri_runtime_cef::WebviewSnapshot, AgentError> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    view.with_cef_webview(move |native| {
        let _ = tx.send(native.snapshot().clone());
    })
    .map_err(|_| error("Probe could not access the CEF browser"))?;
    tokio::time::timeout(Duration::from_secs(5), rx)
        .await
        .map_err(|_| error("CEF native observation timed out"))?
        .map_err(|_| error("CEF browser closed during native observation"))
}

#[cfg(windows)]
async fn check_capture_restored(view: &Webview, bounds: tauri::Rect) -> Result<(), AgentError> {
    let native = native_snapshot(view).await?;
    let restored = native
        .bounds
        .ok_or_else(|| error("Capture lost native bounds"))?;
    if native.visible != Some(false)
        || restored.position.to_physical::<i32>(1.) != bounds.position.to_physical::<i32>(1.)
        || restored.size.to_physical::<u32>(1.) != bounds.size.to_physical::<u32>(1.)
    {
        return Err(error("Hidden capture did not restore its native state"));
    }
    Ok(())
}

#[cfg(windows)]
async fn check_cancelled_capture(
    app: &tauri::AppHandle,
    tab: &BrowserTab,
    view: &Webview,
    main: &Webview,
    bounds: tauri::Rect,
) -> Result<(), AgentError> {
    let state = app.state::<BrowserState>();
    let _operation = state.operations.lock().await;
    let before = capture::capture(main).await?;
    let (ready, received) = tokio::sync::oneshot::channel();
    let child = view.clone();
    let task = tokio::spawn(async move {
        let _surface = capture::CaptureSurface::prepare(&child).await?;
        let _ = ready.send(());
        std::future::pending::<()>().await;
        Ok::<(), AgentError>(())
    });
    let during = async {
        tokio::time::timeout(Duration::from_secs(10), received)
            .await
            .map_err(|_| error("Cancelled capture preparation timed out"))?
            .map_err(|_| error("Cancelled capture could not prepare the surface"))?;
        tokio::time::timeout(
            Duration::from_secs(5),
            set_browser_viewport(
                app.clone(),
                tab.conversation_id.clone(),
                Some(tab.id.clone()),
                None,
            ),
        )
        .await
        .map_err(|_| error("Frontend hide waited for the pending capture"))??;
        if native_snapshot(view).await?.visible != Some(true) {
            return Err(error("Frontend hide stopped the capture surface"));
        }
        capture::capture(main).await
    }
    .await;
    task.abort();
    let _ = task.await;
    check_capture_restored(view, bounds).await?;
    let before = image::load_from_memory(&before).map_err(|_| error("Host capture invalid"))?;
    let during = image::load_from_memory(&during?).map_err(|_| error("Host capture invalid"))?;
    if before.to_rgba8() != during.to_rgba8() {
        return Err(error("Hidden capture changed the visible host pixels"));
    }
    Ok(())
}

#[cfg(windows)]
async fn check_visible_capture_hide(
    app: &tauri::AppHandle,
    tab: &BrowserTab,
    view: &Webview,
    bounds: tauri::Rect,
) -> Result<(), AgentError> {
    let state = app.state::<BrowserState>();
    let _operation = state.operations.lock().await;
    view.show()
        .map_err(|_| error("Visible capture probe could not show the tab"))?;
    let surface = capture::CaptureSurface::prepare(view).await?;
    let hidden = async {
        tokio::time::timeout(
            Duration::from_secs(5),
            set_browser_viewport(
                app.clone(),
                tab.conversation_id.clone(),
                Some(tab.id.clone()),
                None,
            ),
        )
        .await
        .map_err(|_| error("Frontend hide waited for the visible capture"))??;
        let native = native_snapshot(view).await?;
        let bounds = native
            .bounds
            .ok_or_else(|| error("Capture surface lost its bounds"))?;
        let position = bounds.position.to_physical::<i32>(1.);
        let size = bounds.size.to_physical::<u32>(1.);
        if native.visible != Some(true)
            || i64::from(position.x) + i64::from(size.width) > 0
            || i64::from(position.y) + i64::from(size.height) > 0
        {
            return Err(error(
                "Frontend hide did not keep a rendering surface outside the host",
            ));
        }
        Ok(())
    }
    .await;
    drop(surface);
    check_capture_restored(view, bounds).await?;
    hidden
}

#[cfg(windows)]
async fn check_pending_capture_hide(
    app: &tauri::AppHandle,
    tab: &BrowserTab,
    view: &Webview,
    bounds: tauri::Rect,
) -> Result<(), AgentError> {
    let state = app.state::<BrowserState>();
    let _operation = state.operations.lock().await;
    view.show()
        .map_err(|_| error("Pending capture probe could not show the tab"))?;
    let pending = capture::CaptureSurface::reserve(view)?;
    let hidden = async {
        tokio::time::timeout(
            Duration::from_secs(5),
            set_browser_viewport(
                app.clone(),
                tab.conversation_id.clone(),
                Some(tab.id.clone()),
                None,
            ),
        )
        .await
        .map_err(|_| error("Frontend hide waited for capture preparation"))??;
        if native_snapshot(view).await?.visible != Some(false) {
            return Err(error(
                "Pending capture left the tab visible after frontend hide",
            ));
        }
        Ok(())
    }
    .await;
    drop(pending);
    check_capture_restored(view, bounds).await?;
    hidden
}

async fn check_layout(
    app: &tauri::AppHandle,
    tab: &BrowserTab,
    view: &Webview,
    output: &std::path::Path,
) -> Result<(), AgentError> {
    let window = app.get_window("main").unwrap();
    for (width, height) in [(1100., 760.), (820., 620.), (1000., 700.)] {
        window
            .set_size(tauri::LogicalSize::new(width, height))
            .unwrap();
        let rect = Viewport {
            x: 100.,
            y: 80.,
            width: width - 220.,
            height: height - 160.,
        };
        set_browser_viewport(
            app.clone(),
            tab.conversation_id.clone(),
            Some(tab.id.clone()),
            Some(rect.clone()),
        )
        .await?;
        // Content grows after the viewport is assigned, as in a real application.
        evaluate(view, format!("(() => {{ document.body.style.minWidth='{}px'; document.body.style.minHeight='{}px'; return true; }})()", width * 2., height * 3.)).await?;
        tokio::time::sleep(Duration::from_millis(100)).await;
        let native = native_snapshot(view).await?;
        if native.parent_matches != Some(true) || native.visible != Some(true) {
            return Err(error("CEF child is not visibly embedded in its host"));
        }
        let native_bounds = native
            .bounds
            .ok_or_else(|| error("CEF native bounds are unavailable"))?;
        let scale = window
            .scale_factor()
            .map_err(|_| error("Probe could not read the display scale"))?;
        let position = native_bounds.position.to_logical::<f64>(scale);
        let size = native_bounds.size.to_logical::<f64>(scale);
        let bounds = [position.x, position.y, size.width, size.height];
        if !native_bounds_match(bounds, &rect) {
            return Err(error(&format!("Browser escaped its viewport: {bounds:?}")));
        }
        if app.get_webview("main").unwrap().size().unwrap() != window.inner_size().unwrap() {
            return Err(error("Child allocation resized the main interface"));
        }
        let viewport = evaluate(view, "[innerWidth, innerHeight]".into()).await?;
        if viewport != json!([rect.width as i32, rect.height as i32]) {
            return Err(error(&format!("Page escaped its viewport: {viewport}")));
        }
        let slot = evaluate(&app.get_webview("main").unwrap(), "(() => { const r=document.querySelector('#viewport').getBoundingClientRect(); return [r.x,r.y,r.width,r.height]; })()".into()).await?;
        if !page_bounds_match(&slot, &rect) {
            return Err(error(&format!(
                "Native page does not match its host slot: {slot}"
            )));
        }
    }
    let bytes = capture::capture(&app.get_webview("main").unwrap()).await?;
    std::fs::write(output.join("native-browser-host.png"), bytes)
        .map_err(|_| AgentError::storage())?;
    Ok(())
}
async fn check(
    app: &tauri::AppHandle,
    base: &str,
    output: &std::path::Path,
) -> Result<(), AgentError> {
    std::fs::create_dir_all(output).map_err(|_| AgentError::storage())?;
    // The production UI opens tabs only after its main webview has loaded.
    wait_page(&app.get_webview("main").unwrap(), "Browser host").await?;
    #[cfg(target_os = "linux")]
    linux::check_idle().await?;
    #[cfg(target_os = "linux")]
    linux::check_keyboard(&app.get_webview("main").unwrap(), true).await?;
    #[cfg(target_os = "macos")]
    {
        let window = app.get_webview_window("main").unwrap();
        let (tx, rx) = tokio::sync::oneshot::channel();
        // AppKit callbacks run outside Winit dispatch, like the Jarvito panel.
        dispatch2::DispatchQueue::main().exec_async(move || {
            let handles = window.ns_window().is_ok() && window.ns_view().is_ok();
            let scale = window.scale_factor().is_ok_and(|value| value > 0.);
            let _ = tx.send(handles && scale);
        });
        if !tokio::time::timeout(Duration::from_secs(5), rx)
            .await
            .is_ok_and(|result| result.unwrap_or(false))
        {
            return Err(error("Native main-thread window getters failed"));
        }
    }
    let conversation = "browser-probe";
    let tab = BrowserTab {
        id: "probe-one".into(),
        conversation_id: conversation.into(),
        url: format!("{base}/"),
        title: "Probe".into(),
        loading: false,
    };
    app.state::<BrowserState>().access(app, true, |catalog| {
        catalog.conversations.insert(
            conversation.into(),
            Snapshot {
                tabs: vec![tab.clone()],
                active_id: Some(tab.id.clone()),
                ..Snapshot::default()
            },
        );
        Ok(())
    })?;
    *app.state::<BrowserState>().catalog.lock().unwrap() = None;
    let restored = app.state::<BrowserState>().snapshot(app, conversation)?;
    if restored.active_id.as_deref() != Some(&tab.id) || restored.tabs.len() != 1 {
        return Err(error("Tab metadata was not restored"));
    }
    if app
        .state::<BrowserState>()
        .tab(app, "another-conversation", &tab.id)
        .is_ok()
    {
        return Err(error("Conversation ownership was not enforced"));
    }
    let view = ensure_view(app, &tab).await?;
    set_browser_viewport(
        app.clone(),
        conversation.into(),
        Some(tab.id.clone()),
        Some(Viewport {
            x: 20.,
            y: 60.,
            width: 900.,
            height: 520.,
        }),
    )
    .await?;
    let main_window = app
        .get_window("main")
        .ok_or_else(|| error("Main window missing after child creation"))?;
    main_window
        .set_size(tauri::LogicalSize::new(1000., 700.))
        .unwrap();
    for _ in 0..20 {
        if app.get_webview("main").unwrap().size().unwrap() == main_window.inner_size().unwrap() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    if app.get_webview("main").unwrap().size().unwrap() != main_window.inner_size().unwrap() {
        return Err(error(&format!(
            "Main webview did not resize with its window: page={:?}, window={:?}",
            app.get_webview("main").unwrap().size().unwrap(),
            main_window.inner_size().unwrap()
        )));
    }
    wait_page(&view, "Browser probe").await?;
    check_layout(app, &tab, &view, output).await?;
    #[cfg(target_os = "linux")]
    linux::check_keyboard(&view, false).await?;
    #[cfg(target_os = "linux")]
    linux::check_keyboard(&app.get_webview("main").unwrap(), true).await?;
    #[cfg(target_os = "linux")]
    linux::check_stacking(&view).await?;
    let snapshot = cdp_action(&view, json!({"action":"snapshot"})).await?;
    let element = |name: &str| {
        snapshot["elements"]
            .as_array()
            .and_then(|nodes| nodes.iter().find(|node| node["name"] == name))
            .and_then(|node| node["id"].as_str())
            .map(str::to_owned)
            .ok_or_else(|| error(&format!("Probe element missing from CDP snapshot: {name}")))
    };
    let button = element("Testar")?;
    let fill = cdp_action(
        &view,
        json!({"action":"fill","element":element("Nome")?,"text":"Jarvis"}),
    )
    .await?;
    let click = cdp_action(&view, json!({"action":"click","element":button})).await?;
    if fill["dispatched"] != true || click["dispatched"] != true {
        return Err(error("CDP did not confirm native input dispatch"));
    }
    let mut input = Value::Null;
    for _ in 0..80 {
        input = cdp_action(
            &view,
            json!({"action":"evaluate","expression":INPUT_EVIDENCE}),
        )
        .await?;
        if input["fetchError"].is_string() {
            return Err(error(&format!(
                "Probe fetch failed: {}",
                input["fetchError"]
            )));
        }
        if input["fetch"].is_object() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let expected_body: Value = serde_json::from_str(NETWORK_BODY).unwrap();
    if input["inputTrusted"] != true
        || input["clickTrusted"] != true
        || input["value"] != "Jarvis"
        || input["fetch"] != expected_body
    {
        return Err(error(&format!(
            "Trusted CDP input or fetch evidence missing: {input}"
        )));
    }
    let result = cdp_action(&view, json!({"action":"snapshot"})).await?;
    if !result["text"].as_str().unwrap_or("").contains("Olá Jarvis") {
        return Err(error("Native fill/click did not update the page"));
    }
    let network = cdp_action(&view, json!({"action":"network","filter":"/api/probe"})).await?;
    let request = network["requests"]
        .as_array()
        .and_then(|requests| {
            requests.iter().find(|request| {
                request["url"] == format!("{base}/api/probe")
                    && request["method"] == "GET"
                    && request["status"] == 200
            })
        })
        .ok_or_else(|| error("CDP did not observe the fixture HTTP response"))?;
    let request_id = request["id"]
        .as_str()
        .ok_or_else(|| error("CDP network evidence has no request ID"))?;
    let body = cdp_action(
        &view,
        json!({"action":"response_body","requestId":request_id}),
    )
    .await?;
    let decoded_body: Value = serde_json::from_str(body["body"].as_str().unwrap_or_default())
        .map_err(|_| error("CDP fixture response body is not JSON"))?;
    if body["base64Encoded"] != false || body["truncated"] != false || decoded_body != expected_body
    {
        return Err(error("CDP returned an incorrect fixture response body"));
    }
    let devtools = cdp_action(
        &view,
        json!({"action":"devtools","method":"Runtime.evaluate","params":{"expression":INPUT_EVIDENCE,"returnByValue":true}}),
    )
    .await?;
    if devtools["result"]["value"] != input {
        return Err(error("DevTools evaluation disagrees with browser_evaluate"));
    }
    let logs = page_action(&view, &json!({"action":"console"})).await?;
    if !logs.to_string().contains("probe console evidence") {
        return Err(error("Console evidence missing"));
    }
    view.eval("window.__probeIpc='pending';window.__TAURI_INTERNALS__.invoke('probe_ping').then(()=>window.__probeIpc='ALLOWED').catch(()=>window.__probeIpc='denied');window.__probePlugin='pending';window.__TAURI_INTERNALS__.invoke('plugin:window|get_all_windows').then(()=>window.__probePlugin='ALLOWED').catch(()=>window.__probePlugin='denied');").map_err(|_| error("Probe IPC evaluation failed"))?;
    for _ in 0..40 {
        let value = evaluate(
            &view,
            "({app:window.__probeIpc,plugin:window.__probePlugin})".into(),
        )
        .await?;
        if value["app"] == "denied" && value["plugin"] == "denied" {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let ipc = evaluate(
        &view,
        "({app:window.__probeIpc,plugin:window.__probePlugin})".into(),
    )
    .await?;
    if ipc["app"] != "denied" || ipc["plugin"] != "denied" {
        return Err(error(&format!("Browser IPC was not denied: {ipc}")));
    }
    println!("Probe: capturing visible child");
    println!(
        "Probe: capture owner {}, visible {:?}, minimized {:?}",
        view.window().label(),
        view.window().is_visible(),
        view.window().is_minimized()
    );
    let bytes = capture::capture(&view).await?;
    let image =
        image::load_from_memory(&bytes).map_err(|_| error("Screenshot could not be decoded"))?;
    if image.width() < 600 || image.height() < 300 {
        return Err(error("Screenshot dimensions incorrect"));
    }
    std::fs::write(output.join("native-browser.png"), &bytes).map_err(|_| AgentError::storage())?;
    set_browser_viewport(app.clone(), conversation.into(), Some(tab.id.clone()), None).await?;
    evaluate(&view, "(() => { document.querySelector('#result').textContent='Background updated'; return true; })()".into()).await?;
    #[cfg(windows)]
    let hidden_bounds = native_snapshot(&view)
        .await?
        .bounds
        .ok_or_else(|| error("Hidden capture bounds unavailable"))?;
    #[cfg(windows)]
    let foreground =
        unsafe { windows_sys::Win32::UI::WindowsAndMessaging::GetForegroundWindow() } as usize;
    #[cfg(windows)]
    check_cancelled_capture(
        app,
        &tab,
        &view,
        &app.get_webview("main").unwrap(),
        hidden_bounds,
    )
    .await?;
    // Capturing a background tab must still return pixels, not an empty image.
    let hidden = capture::capture(&view).await?;
    #[cfg(windows)]
    {
        check_capture_restored(&view, hidden_bounds).await?;
        if app
            .state::<BrowserState>()
            .snapshot(app, conversation)?
            .active_id
            .as_deref()
            != Some(&tab.id)
            || unsafe { windows_sys::Win32::UI::WindowsAndMessaging::GetForegroundWindow() }
                as usize
                != foreground
        {
            return Err(error(
                "Hidden capture changed tab selection or foreground focus",
            ));
        }
    }
    if image::load_from_memory(&hidden).is_err() {
        return Err(error("Hidden-tab capture failed"));
    }
    if hidden == bytes {
        return Err(error("Background capture returned stale pixels"));
    }
    std::fs::write(output.join("native-browser-background.png"), &hidden)
        .map_err(|_| AgentError::storage())?;
    #[cfg(windows)]
    check_visible_capture_hide(app, &tab, &view, hidden_bounds).await?;
    #[cfg(windows)]
    check_pending_capture_hide(app, &tab, &view, hidden_bounds).await?;
    view.show().unwrap();
    view.navigate(address(&format!("{base}/next"))?).unwrap();
    wait_page(&view, "Next page").await?;
    view.eval("history.back()").unwrap();
    wait_page(&view, "Browser probe").await?;
    let stale = cdp_action(&view, json!({"action":"click","element":button})).await;
    if stale.is_ok() {
        return Err(error("Stale element accepted"));
    }
    view.close().unwrap();
    for _ in 0..80 {
        if app.get_webview(&label(&tab.id)).is_none() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    if app.get_webview(&label(&tab.id)).is_some() {
        return Err(error("Closed native tab remains registered"));
    }
    #[cfg(target_os = "linux")]
    linux::check_keyboard(&app.get_webview("main").unwrap(), false).await?;
    #[cfg(target_os = "macos")]
    island::check(app, base, output).await?;
    #[cfg(target_os = "linux")]
    linux_island::check(app, base, output).await?;
    std::fs::write(output.join("native-report.json"), serde_json::to_vec_pretty(&json!({"passed":true,"engine":"CEF/Chromium","platform":std::env::consts::OS,"checks":["native load","native main-thread getters","CEF child parent and visibility","native viewport bounds","page viewport bounds","host viewport slot","main interface resize","CDP snapshot","trusted CDP fill","trusted CDP click","CDP evaluate","CDP network","CDP response body","CDP devtools","console","screenshot","hidden screenshot","app IPC denied","plugin IPC denied","navigate","back","stale element rejection","native close"],"cdp":{"fill":fill,"click":click,"input":input,"network":network,"responseBody":body,"devtools":devtools},"width":image.width(),"height":image.height(),"pngBytes":bytes.len()})).unwrap()).map_err(|_| AgentError::storage())?;
    Ok(())
}

fn probe_context() -> tauri::Context {
    let mut context = crate::application_context();
    context.config_mut().identifier = "com.foxtag.jarvis.browser-probe".into();
    context.config_mut().app.windows.clear();
    context
}

pub fn run() {
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    if std::env::var("JARVIS_TRAY_PROBE").is_ok_and(|value| value == "1") {
        return crate::windows_tray::probe::run();
    }
    let passed = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let completed = passed.clone();
    let context = probe_context();
    let output = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.codex/browser-review");
    tauri::Builder::default()
        .runtime(tauri_runtime_cef::Cef::default())
        .manage(BrowserState::default())
        .manage(crate::companion::CompanionState::default())
        .manage(crate::desktop::DesktopState::default())
        .invoke_handler(|invoke| {
            crate::main_only(invoke, |invoke| {
                invoke.resolver.resolve("main-only probe");
                true
            })
        })
        .setup(move |app| {
            #[cfg(target_os = "linux")]
            {
                crate::initialize_native_async_runtime();
                if std::env::var("JARVIS_AGENT_PIPELINE_PROBE").is_ok_and(|value| value == "1") {
                    app.manage(AppState::default());
                    app.manage(crate::agent::AgentState::default());
                    app.manage(crate::agent::computer::ComputerState::default());
                    app.manage(crate::core::CoreState::default());
                    app.manage(crate::core::impeccable_live::ImpeccableLiveState::default());
                    app.manage(extension::ExtensionState::default());
                    app.manage(crate::mcp::McpState::default());
                    linux_agent::initialize(app)
                        .map_err(|cause| std::io::Error::other(cause.message))?;
                }
                let preferences = tempfile::tempdir()?;
                app.state::<crate::desktop::DesktopState>()
                    .initialize_probe(preferences.path().join("desktop.json"))
                    .map_err(std::io::Error::other)?;
                app.manage(preferences);
            }
            // CEF subprocesses must enter Chromium before starting fixture threads.
            let (base, listener) = server();
            std::thread::spawn(move || {
                for mut stream in listener.incoming().flatten() {
                    let mut request = [0; 4096];
                    let count = stream.read(&mut request).unwrap_or(0);
                    let request = String::from_utf8_lossy(&request[..count]);
                    let (content_type, body) = fixture_response(&request);
                    #[cfg(target_os = "linux")]
                    let dynamic = request.starts_with("GET /agent ")
                        .then(linux_agent::page)
                        .flatten();
                    #[cfg(target_os = "linux")]
                    let body = dynamic.as_deref().unwrap_or(body);
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    let _ = stream.write_all(response.as_bytes());
                }
            });
            tauri::WebviewWindowBuilder::new(
                app,
                "main",
                WebviewUrl::External(address(&format!("{base}/main")).unwrap()),
            )
            .title("Jarvis browser integration probe")
            .initialization_script(include_str!("page.js"))
            .inner_size(960., 640.)
            .visible(true)
            .build()?;
            let app = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                #[cfg(target_os = "linux")]
                let result = if std::env::var("JARVIS_AGENT_PIPELINE_PROBE").is_ok_and(|value| value == "1") {
                    linux_agent::check(&app, &base, &output).await
                } else {
                    check(&app, &base, &output).await
                };
                #[cfg(not(target_os = "linux"))]
                let result = check(&app, &base, &output).await;
                match result {
                    Ok(()) => {
                        completed.store(true, std::sync::atomic::Ordering::SeqCst);
                        println!("Native browser probe passed: {}", output.display());
                        app.exit(0);
                    }
                    Err(cause) => {
                        eprintln!("Native browser probe failed: {}", cause.message);
                        app.exit(1);
                    }
                }
            });
            Ok(())
        })
        .on_window_event(crate::companion::on_window_event)
        .run(context)
        .expect("run browser probe");
    if !passed.load(std::sync::atomic::Ordering::SeqCst) {
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pinned_macos_runtime_keeps_the_framework_loaded_through_shutdown() {
        // CEF/AppKit needs a bundled main-thread process, so the unit fixture
        // guards the vendored ownership contract; the native probe covers use.
        let source = include_str!("../../../vendor/tauri-runtime-cef/src/runtime.rs");
        let runtime = source
            .split_once("pub struct CefRuntime<T:")
            .unwrap()
            .1
            .split_once("impl<T: UserEvent> fmt::Debug")
            .unwrap()
            .0;
        assert!(runtime.contains("_library_loader: cef::library_loader::LibraryLoader"));
        assert!(source.contains("_library_loader: _loader,"));

        let run = source
            .split_once("fn run<F: FnMut(RunEvent<T>) + 'static>(self, callback: F)")
            .unwrap()
            .1
            .split_once("#[cfg(test)]")
            .unwrap()
            .0;
        let retained = run
            .find("let library_loader = self._library_loader;")
            .unwrap();
        let main_loop = run.find("self.event_loop.run_app(app)").unwrap();
        let shutdown = run.find("cef::shutdown();").unwrap();
        let delegate_released = run.find("drop(app_delegate);").unwrap();
        let unloaded = run.find("drop(library_loader);").unwrap();
        assert!(
            retained < main_loop
                && main_loop < shutdown
                && shutdown < delegate_released
                && delegate_released < unloaded
        );

        let manifest = include_str!("../../../Cargo.toml");
        assert!(manifest.contains("[patch.\"https://github.com/tauri-apps/tauri\"]"));
        assert!(manifest.contains("tauri-runtime-cef = { path = \"vendor/tauri-runtime-cef\" }"));
    }

    #[test]
    fn probe_uses_an_isolated_context_without_changing_the_normal_app_configuration() {
        let original = crate::application_context();
        let identifier = original.config().identifier.clone();
        let window_count = original.config().app.windows.len();
        let probe = probe_context();
        assert_eq!(probe.config().identifier, "com.foxtag.jarvis.browser-probe");
        assert!(probe.config().app.windows.is_empty());
        let regular = crate::application_context();
        assert_eq!(regular.config().identifier, identifier);
        assert_eq!(regular.config().app.windows.len(), window_count);
    }

    #[test]
    fn fixture_serves_json_evidence_separately_from_browser_pages() {
        let (content_type, body) = fixture_response("GET /api/probe HTTP/1.1\r\n");
        assert_eq!(content_type, "application/json; charset=utf-8");
        assert_eq!(
            serde_json::from_str::<Value>(body).unwrap(),
            json!({"ok":true,"evidence":"jarvis-probe-network"})
        );
        for request in ["GET / HTTP/1.1", "GET /main HTTP/1.1", "GET /next HTTP/1.1"] {
            let (content_type, body) = fixture_response(request);
            assert_eq!(content_type, "text/html; charset=utf-8");
            assert!(body.starts_with("<!doctype html>"));
        }
    }

    #[test]
    fn native_bounds_accept_pixel_rounding_but_reject_an_escaped_viewport() {
        let viewport = Viewport {
            x: 100.,
            y: 80.,
            width: 880.,
            height: 600.,
        };
        assert!(native_bounds_match([100., 80., 880., 600.], &viewport));
        assert!(native_bounds_match([100.4, 79.6, 880.4, 599.6], &viewport));
        assert!(page_bounds_match(&json!([100, 80, 880, 600]), &viewport));
        assert!(page_bounds_match(
            &json!([100.4, 79.6, 880.4, 599.6]),
            &viewport
        ));
        assert!(!page_bounds_match(&json!([100, 80, 1760, 600]), &viewport));
        assert!(!page_bounds_match(&json!([100, 80, 880]), &viewport));
        for bounds in [
            [101., 80., 880., 600.],
            [100., 81., 880., 600.],
            [100., 80., 1760., 600.],
            [100., 80., 880., 1800.],
            [f64::NAN, 80., 880., 600.],
        ] {
            assert!(!native_bounds_match(bounds, &viewport));
        }
    }
}
