//! macOS uses UserNotifications for real authorization and foreground banners.
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub(super) use macos::{authorize, show};
#[cfg(windows)]
mod windows;

pub(super) fn setup(app: &tauri::AppHandle) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    macos::setup();
    #[cfg(windows)]
    return windows::setup(app);
    #[cfg(not(windows))]
    {
        let _ = app;
        Ok(())
    }
}

#[cfg(not(target_os = "macos"))]
pub(super) async fn authorize() -> Result<(), String> {
    #[cfg(windows)]
    return windows::authorize().await;
    #[cfg(not(windows))]
    Ok(())
}
#[cfg(not(target_os = "macos"))]
pub(super) async fn show(title: &str, body: &str) -> Result<(), String> {
    #[cfg(windows)]
    windows::authorize().await?;
    let title = title.to_owned();
    let body = body.to_owned();
    tauri::async_runtime::spawn_blocking(move || {
        #[cfg(windows)]
        let _apartment = windows::Apartment::new()?;
        let mut notification = notify_rust::Notification::new();
        notification.summary(&title).body(&body).appname("Jarvis");
        #[cfg(target_os = "windows")]
        notification.app_id(windows::APP_ID);
        notification.show().map(|_| ()).map_err(|_| {
            "Não foi possível enviar a notificação. Confira as permissões do sistema.".into()
        })
    })
    .await
    .map_err(|_| "Não foi possível enviar a notificação.".to_string())?
}
