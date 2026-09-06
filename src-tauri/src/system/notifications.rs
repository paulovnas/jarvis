//! macOS uses UserNotifications for real authorization and foreground banners.
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub(super) use macos::{authorize, setup, show};

#[cfg(not(target_os = "macos"))]
pub(super) fn setup() {}
#[cfg(not(target_os = "macos"))]
pub(super) async fn authorize() -> Result<(), String> {
    Ok(())
}
#[cfg(not(target_os = "macos"))]
pub(super) async fn show(title: &str, body: &str) -> Result<(), String> {
    let title = title.to_owned();
    let body = body.to_owned();
    tauri::async_runtime::spawn_blocking(move || {
        let mut notification = notify_rust::Notification::new();
        notification.summary(&title).body(&body).appname("Jarvis");
        #[cfg(target_os = "windows")]
        notification.app_id("com.foxtag.jarvis");
        notification.show().map(|_| ()).map_err(|_| {
            "Não foi possível enviar a notificação. Confira as permissões do sistema.".into()
        })
    })
    .await
    .map_err(|_| "Não foi possível enviar a notificação.".to_string())?
}
