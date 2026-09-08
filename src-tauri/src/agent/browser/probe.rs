//! Opt-in executable integration probe. Never built into the normal application.
use super::*;
use std::io::{Read, Write};

const PAGE: &str = r#"<!doctype html><html><head><title>Browser probe</title><style>body{background:#181b20;color:#d7dce5;font:18px system-ui;padding:32px}button,input{font:inherit;padding:12px;border-radius:6px}h1{color:#61afef}</style></head><body><h1>Jarvis native browser</h1><label>Nome <input id="name" aria-label="Nome"></label><button id="action" onclick="document.querySelector('#result').textContent='Olá '+document.querySelector('#name').value;console.error('probe console evidence')">Testar</button><p id="result">Pronto</p><a href="/next">Próxima página</a><script>console.info('probe loaded')</script></body></html>"#;

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
async fn check(
    app: &tauri::AppHandle,
    base: &str,
    output: &std::path::Path,
) -> Result<(), AgentError> {
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
    let view = ensure_view(app, &tab)?;
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
        return Err(error("Main webview did not resize with its window"));
    }
    let snapshot = wait_page(&view, "Browser probe").await?;
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
    std::fs::create_dir_all(output).map_err(|_| AgentError::storage())?;
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
            let next = String::from_utf8_lossy(&request[..count]).starts_with("GET /next ");
            let page = if next {
                "<!doctype html><title>Next page</title><h1>Navigation works</h1>"
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
