//! Opt-in executable integration probe. Never built into the normal application.
use super::*;
use std::io::{Read, Write};

const PAGE: &str = r#"<!doctype html><html><head><title>Browser probe</title><style>body{background:#181b20;color:#d7dce5;font:18px system-ui;padding:32px}button,input{font:inherit;padding:12px;border-radius:6px}h1{color:#61afef}</style></head><body><h1>Jarvis native browser</h1><label>Nome <input id="name" aria-label="Nome"></label><button id="action" onclick="document.querySelector('#result').textContent='Olá '+document.querySelector('#name').value;console.error('probe console evidence')">Testar</button><p id="result">Pronto</p><a href="/next">Próxima página</a><script>console.info('probe loaded')</script></body></html>"#;
const HOST: &str = r#"<!doctype html><html><head><title>Browser host</title><style>
html,body{margin:0;height:100%;color:#d7dce5;font:16px system-ui}
body{display:grid;grid-template-columns:100px minmax(0,1fr) 120px;grid-template-rows:80px minmax(0,1fr) 80px;background:#14171b}
header,footer{grid-column:1/-1;background:#292e37;padding:20px;box-sizing:border-box}
aside{padding:12px;box-sizing:border-box}main{background:#61afef;min-width:0;min-height:0}
</style></head><body><header>Jarvis browser integration probe</header><aside>Workspace</aside><main id="viewport"></main><aside>Inspector</aside><footer>Jarvis status bar</footer></body></html>"#;

fn server() -> (String, std::net::TcpListener) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind fixture server");
    (
        format!("http://{}", listener.local_addr().unwrap()),
        listener,
    )
}
async fn wait_page(view: &Webview, expected: &str) -> Result<Value, AgentError> {
    for _ in 0..80 {
        if let Ok(value) = page_action(view, &json!({"action":"snapshot"})).await {
            if value["title"].as_str() == Some(expected) {
                return Ok(value);
            }
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    Err(error("Probe page did not load"))
}

#[cfg(target_os = "linux")]
async fn check_linux_layout(
    app: &tauri::AppHandle,
    tab: &BrowserTab,
    view: &Webview,
    output: &std::path::Path,
) -> Result<(), AgentError> {
    use gtk::prelude::*;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };
    let unmapped = Arc::new(AtomicUsize::new(0));
    let counter = unmapped.clone();
    view.with_webview(move |native| {
        native.inner().connect_unmap(move |_| {
            counter.fetch_add(1, Ordering::SeqCst);
        });
    })
    .unwrap();
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
        let (tx, rx) = tokio::sync::oneshot::channel();
        view.with_webview(move |native| {
            let page = native.inner();
            let bounds = page.allocation();
            // GtkOverlay gives each overlay its own GdkWindow; its allocation
            // starts at (0, 0) inside that window. Check the position in the host.
            let (x, y) = page
                .translate_coordinates(&page.parent().unwrap(), 0, 0)
                .unwrap();
            let _ = tx.send([x, y, bounds.width(), bounds.height()]);
        })
        .unwrap();
        let bounds = rx.await.unwrap();
        if bounds
            != [
                rect.x as i32,
                rect.y as i32,
                rect.width as i32,
                rect.height as i32,
            ]
        {
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
        if slot != json!(bounds) {
            return Err(error(&format!(
                "Native page does not match its host slot: {slot}"
            )));
        }
    }
    if unmapped.load(Ordering::SeqCst) != 0 {
        return Err(error("Updating bounds hid the active page"));
    }
    let (tx, rx) = tokio::sync::oneshot::channel();
    view.with_webview(move |native| {
        let window = native
            .inner()
            .toplevel()
            .unwrap()
            .downcast::<gtk::Window>()
            .unwrap()
            .window()
            .unwrap();
        let pixels = window
            .pixbuf(0, 0, window.width(), window.height())
            .unwrap();
        let _ = tx.send(pixels.save_to_bufferv("png", &[]).unwrap());
    })
    .unwrap();
    std::fs::write(output.join("native-browser-host.png"), rx.await.unwrap())
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
    let snapshot = wait_page(&view, "Browser probe").await?;
    #[cfg(target_os = "linux")]
    check_linux_layout(app, &tab, &view, output).await?;
    let element = |name: &str| {
        snapshot["elements"]
            .as_array()
            .unwrap()
            .iter()
            .find(|node| node["name"] == name)
            .unwrap()["id"]
            .as_str()
            .unwrap()
            .to_string()
    };
    page_action(
        &view,
        &json!({"action":"fill","element":element("Nome"),"text":"Jarvis"}),
    )
    .await?;
    page_action(
        &view,
        &json!({"action":"click","element":element("Testar")}),
    )
    .await?;
    let result = page_action(&view, &json!({"action":"snapshot"})).await?;
    if !result["text"].as_str().unwrap_or("").contains("Olá Jarvis") {
        return Err(error("Native fill/click did not update the page"));
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
    let bytes = capture::capture(&view).await?;
    let image =
        image::load_from_memory(&bytes).map_err(|_| error("Screenshot could not be decoded"))?;
    if image.width() < 600 || image.height() < 300 {
        return Err(error("Screenshot dimensions incorrect"));
    }
    std::fs::write(output.join("native-browser.png"), &bytes).map_err(|_| AgentError::storage())?;
    set_browser_viewport(app.clone(), conversation.into(), Some(tab.id.clone()), None).await?;
    evaluate(&view, "(() => { document.querySelector('#result').textContent='Background updated'; return true; })()".into()).await?;
    // Capturing a background tab must still return pixels, not an empty image.
    let hidden = capture::capture(&view).await?;
    if image::load_from_memory(&hidden).is_err() {
        return Err(error("Hidden-tab capture failed"));
    }
    if hidden == bytes {
        return Err(error("Background capture returned stale pixels"));
    }
    std::fs::write(output.join("native-browser-background.png"), &hidden)
        .map_err(|_| AgentError::storage())?;
    view.show().unwrap();
    view.navigate(address(&format!("{base}/next"))?).unwrap();
    wait_page(&view, "Next page").await?;
    view.eval("history.back()").unwrap();
    wait_page(&view, "Browser probe").await?;
    let stale = page_action(&view, &json!({"action":"click","element":"999:999"})).await;
    if stale.is_ok() {
        return Err(error("Stale element accepted"));
    }
    view.close().unwrap();
    if app.get_webview(&label(&tab.id)).is_some() {
        return Err(error("Closed native tab remains registered"));
    }
    std::fs::write(output.join("native-report.json"), serde_json::to_vec_pretty(&json!({"passed":true,"engine":std::env::consts::OS,"checks":["native load","snapshot","fill","click","console","screenshot","hidden screenshot","app IPC denied","plugin IPC denied","navigate","back","stale element rejection","native close"],"width":image.width(),"height":image.height(),"pngBytes":bytes.len()})).unwrap()).map_err(|_| AgentError::storage())?;
    Ok(())
}

pub fn run() {
    let passed = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let completed = passed.clone();
    let (base, listener) = server();
    std::thread::spawn(move || {
        for mut stream in listener.incoming().flatten() {
            let mut request = [0; 4096];
            let count = stream.read(&mut request).unwrap_or(0);
            let request = String::from_utf8_lossy(&request[..count]);
            let page = if request.starts_with("GET /next ") {
                "<!doctype html><title>Next page</title><h1>Navigation works</h1>"
            } else if request.starts_with("GET /main ") {
                HOST
            } else {
                PAGE
            };
            let response = format!("HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{page}", page.len());
            let _ = stream.write_all(response.as_bytes());
        }
    });
    let mut context = tauri::generate_context!();
    context.config_mut().identifier = "com.foxtag.jarvis.browser-probe".into();
    context.config_mut().app.windows.clear();
    let output = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.codex/browser-review");
    tauri::Builder::default()
        .manage(BrowserState::default())
        .invoke_handler(|invoke| {
            crate::main_only(invoke, |invoke| {
                invoke.resolver.resolve("main-only probe");
                true
            })
        })
        .setup(move |app| {
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
        .run(context)
        .expect("run browser probe");
    if !passed.load(std::sync::atomic::Ordering::SeqCst) {
        std::process::exit(1);
    }
}
