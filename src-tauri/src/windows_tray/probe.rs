//! Opt-in Windows/macOS smoke test: `JARVIS_TRAY_PROBE=1 cargo run --features browser-probe --example browser-probe`.
//! All preference and library writes belong to a temporary profile.
use super::*;
use std::{sync::Arc, time::Duration};

#[cfg(target_os = "windows")]
fn native_menu_labels(menu: &Menu<tauri::DynRuntime>) -> Result<Vec<String>, String> {
    use tauri::menu::ContextMenu;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetMenuItemCount, GetMenuStringW, MF_BYPOSITION,
    };
    let handle = menu.hpopupmenu().map_err(|e| e.to_string())? as *mut std::ffi::c_void;
    // Muda's text getter removes the tab-separated right column; inspect the actual Win32 labels.
    let count = unsafe { GetMenuItemCount(handle) };
    if count < 0 {
        return Err("Could not inspect the native menu".into());
    }
    Ok((0..count)
        .map(|i| {
            let mut text = [0u16; 128];
            let length = unsafe {
                GetMenuStringW(
                    handle,
                    i as u32,
                    text.as_mut_ptr(),
                    text.len() as i32,
                    MF_BYPOSITION,
                )
            };
            if length == 0 {
                "---".into()
            } else {
                String::from_utf16_lossy(&text[..length as usize])
            }
        })
        .collect())
}

#[cfg(target_os = "macos")]
fn native_menu_labels(menu: &Menu<tauri::DynRuntime>) -> Result<Vec<String>, String> {
    menu.items()
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|item| match item {
            tauri::menu::MenuItemKind::MenuItem(item) => item.text().map_err(|e| e.to_string()),
            tauri::menu::MenuItemKind::Predefined(_) => Ok("---".into()),
            _ => Err("Unexpected native menu item".into()),
        })
        .collect()
}

#[cfg(target_os = "macos")]
async fn check_native_popup(app: &tauri::AppHandle) -> Result<(), String> {
    use objc2_foundation::{MainThreadMarker, NSRunLoop, NSRunLoopCommonModes, NSTimer};
    let opened = Arc::new(AtomicBool::new(false));
    let observed = opened.clone();
    let handle = app.clone();
    app.run_on_main_thread(move || {
        let block = block2::RcBlock::new(move |timer: std::ptr::NonNull<NSTimer>| {
            let visible = handle.tray_by_id(TRAY_ID).is_some_and(|tray| {
                tray.with_inner_tray_icon(|tray| {
                    tray.ns_status_item()
                        .and_then(|item| item.menu(MainThreadMarker::new().unwrap()))
                        .is_some_and(|menu| {
                            menu.cancelTracking();
                            true
                        })
                })
                .unwrap_or(false)
            });
            if visible {
                observed.store(true, Ordering::Release);
                // NSTimer supplies its live object to the callback on the main run loop.
                unsafe { timer.as_ref().invalidate() };
            }
        });
        // A native timer runs inside AppKit menu tracking, while winit tasks cannot.
        let timer = unsafe { NSTimer::timerWithTimeInterval_repeats_block(0.1, true, &block) };
        unsafe {
            NSRunLoop::currentRunLoop().addTimer_forMode(&timer, NSRunLoopCommonModes);
            NSRunLoop::currentRunLoop()
                .addTimer_forMode(&timer, objc2_app_kit::NSEventTrackingRunLoopMode);
        }
    })
    .map_err(|e| e.to_string())?;
    act(app, TrayAction::Menu);
    for _ in 0..50 {
        if opened.load(Ordering::Acquire)
            && !app.state::<TrayState>().opening.load(Ordering::Acquire)
        {
            println!("macOS native tray popup opened and dismissed");
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    Err("Opening and dismissing the native tray popup did not complete".into())
}

async fn check(app: &tauri::AppHandle, preferences: &std::path::Path) -> Result<(), String> {
    let main = app
        .get_webview_window("main")
        .ok_or("Missing probe window")?;
    let companion = app
        .get_webview_window("companion")
        .ok_or("Missing companion fixture")?;
    let tray = app.tray_by_id(TRAY_ID).ok_or("Missing native tray icon")?;
    if tray.rect().map_err(|e| e.to_string())?.is_none() {
        return Err("The operating system did not register the tray icon".into());
    }
    let menu = build_menu(
        app,
        &Chats {
            running: vec![Chat {
                id: "running".into(),
                title: "Uma conversa muito longa para caber aqui".into(),
                project: "Um projeto muito comprido".into(),
            }],
            recent: vec![Chat {
                id: "recent".into(),
                title: "Conversa recente".into(),
                project: "Global".into(),
            }],
        },
    )
    .map_err(|e| e.to_string())?;
    let (sender, receiver) = tokio::sync::oneshot::channel();
    let inspected = menu.clone();
    app.run_on_main_thread(move || {
        let _ = sender.send(native_menu_labels(&inspected));
    })
    .map_err(|e| e.to_string())?;
    let labels = receiver.await.map_err(|e| e.to_string())??;
    let expected = [
        "Rodando",
        "Uma conversa muito longa ...\tUm projeto muit...",
        "---",
        "Recentes",
        "Conversa recente\tGlobal",
        "---",
        "Abrir Jarvis",
        "---",
        "Configurações",
        "Sobre",
        "---",
        "Encerrar",
    ];
    if labels != expected {
        return Err(format!("Unexpected native menu labels: {labels:?}"));
    }
    tray.set_menu(Some(menu)).map_err(|e| e.to_string())?;
    #[cfg(target_os = "macos")]
    check_native_popup(app).await?;
    main.close().map_err(|e| e.to_string())?;
    for _ in 0..50 {
        if !main.is_visible().map_err(|e| e.to_string())? {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    if main.is_visible().map_err(|e| e.to_string())?
        || app.get_webview_window("main").is_none()
        || crate::app_exit::prepared(app)
    {
        return Err("Closing did not keep the main window alive in the tray".into());
    }
    if !companion.is_visible().map_err(|e| e.to_string())? {
        return Err("Closing hid the companion".into());
    }
    tokio::time::sleep(Duration::from_millis(200)).await;
    act(app, TrayAction::Open);
    if !main.is_visible().map_err(|e| e.to_string())? {
        return Err("Open Jarvis did not restore the window".into());
    }
    for (action, label) in [
        (TrayAction::Settings, "settings"),
        (TrayAction::About, "about"),
    ] {
        act(app, action);
        for _ in 0..50 {
            if app.get_webview_window(label).is_some() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        let window = app
            .get_webview_window(label)
            .ok_or_else(|| format!("Missing {label} window"))?;
        window.destroy().map_err(|e| e.to_string())?;
    }
    std::fs::write(preferences, r#"{"closeBehavior":"exit"}"#).map_err(|e| e.to_string())?;
    app.state::<crate::system::SystemState>()
        .initialize_tray_probe(preferences.to_path_buf())?;
    if hide_on_close(&app.get_window("main").ok_or("Missing native main window")?) {
        return Err("Exit preference was intercepted as hide".into());
    }
    std::fs::write(preferences, r#"{"closeBehavior":"tray"}"#).map_err(|e| e.to_string())?;
    app.state::<crate::system::SystemState>()
        .initialize_tray_probe(preferences.to_path_buf())?;
    Ok(())
}

pub(crate) fn run() {
    let passed = Arc::new(AtomicBool::new(false));
    let completed = passed.clone();
    let mut context = crate::application_context();
    context.config_mut().identifier = "com.foxtag.jarvis.tray-probe".into();
    context.config_mut().app.windows.clear();
    tauri::Builder::default()
        .runtime(tauri_runtime_cef::Cef::default())
        .manage(crate::desktop::DesktopState::default())
        .manage(crate::system::SystemState::default())
        .manage(crate::persistence::AppState::default())
        .manage(crate::agent::AgentState::default())
        .manage(crate::app_exit::ExitState::default())
        .setup(move |app| {
            let profile = tempfile::tempdir()?;
            let preferences = profile.path().join("system.json");
            app.state::<crate::system::SystemState>()
                .initialize_tray_probe(preferences.clone())?;
            app.state::<crate::persistence::AppState>()
                .with_connection(profile.path(), |db| {
                    crate::persistence::initialize_database(db)
                })?;
            app.manage(profile);
            for label in ["main", "companion"] {
                tauri::WebviewWindowBuilder::new(
                    app,
                    label,
                    tauri::WebviewUrl::External("about:blank".parse()?),
                )
                .title(format!("Jarvis tray probe · {label}"))
                .inner_size(400., 180.)
                .visible(true)
                .build()?;
            }
            install(app.handle())?;
            let app = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                match check(&app, &preferences).await {
                    Ok(()) => {
                        // Confirm exit without running service cleanup against the real home directory.
                        crate::app_exit::allow(&app);
                        completed.store(true, Ordering::Release);
                        act(&app, TrayAction::Quit);
                    }
                    Err(error) => {
                        eprintln!("System tray probe failed: {error}");
                        app.exit(1);
                    }
                }
            });
            Ok(())
        })
        .on_window_event(crate::desktop::on_window_event)
        .run(context)
        .expect("Run system tray probe");
    if !passed.load(Ordering::Acquire) {
        std::process::exit(1);
    }
    println!("System tray probe passed: native icon, native menu, hide/restore, companion continuity, settings, about, exit preference, explicit Quit");
}
