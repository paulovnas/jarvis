use tauri::{
    menu::{Menu, MenuItem, MenuItemKind},
    Emitter, Manager,
};

const ABOUT_ID: &str = "jarvis.about";
const SETTINGS_ID: &str = "jarvis.settings";

pub(crate) fn install(app: &tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    // Keep the native editing shortcuts, Services and window-management roles.
    let menu = Menu::default(app.handle())?;
    let application = menu
        .items()?
        .into_iter()
        .next()
        .and_then(|item| match item {
            MenuItemKind::Submenu(submenu) => Some(submenu),
            _ => None,
        })
        .ok_or("Missing macOS application menu")?;
    let about = MenuItem::with_id(app, ABOUT_ID, "Sobre o Jarvis", true, None::<&str>)?;
    // Tauri's first application item is its native About panel.
    application.remove_at(0)?;
    application.insert(&about, 0)?;
    let settings = MenuItem::with_id(
        app,
        SETTINGS_ID,
        "Configurações…",
        true,
        Some("CmdOrCtrl+,"),
    )?;
    application.insert(&settings, 1)?;
    app.set_menu(menu)?;
    app.on_menu_event(|app, event| {
        let target = match event.id().as_ref() {
            ABOUT_ID => Some("app:about"),
            SETTINGS_ID => Some("app:settings"),
            _ => None,
        };
        if let Some(target) = target {
            if let Some(window) = app.get_window("main") {
                let _ = window.unminimize();
                let _ = window.show();
                let _ = window.set_focus();
                let _ = window.emit(target, ());
            }
        }
    });
    Ok(())
}
