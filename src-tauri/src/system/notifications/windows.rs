use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::OnceLock,
};
use tauri::Manager;
use windows::{
    core::HSTRING,
    Win32::System::WinRT::{RoInitialize, RoUninitialize, RO_INIT_MULTITHREADED},
    Win32::UI::Shell::SetCurrentProcessExplicitAppUserModelID,
    UI::Notifications::{NotificationSetting, ToastNotificationManager},
};
use windows_registry::CURRENT_USER;

mod shortcut;

pub(super) const APP_ID: &str = crate::data_dir::PRODUCTION_IDENTIFIER;
const DEVELOPMENT_APP_ID: &str = crate::data_dir::DEVELOPMENT_IDENTIFIER;
const ICON: &[u8] = include_bytes!("../../../icons/128x128.png");
static ICON_PATH: OnceLock<PathBuf> = OnceLock::new();
static ACTIVE_APP_ID: OnceLock<String> = OnceLock::new();

fn valid_app_id(app_id: &str) -> bool {
    matches!(app_id, APP_ID | DEVELOPMENT_APP_ID)
}

// Keep the apartment on the same blocking thread as the WinRT calls.
pub(super) struct Apartment;
impl Apartment {
    pub(super) fn new() -> Result<Self, String> {
        unsafe { RoInitialize(RO_INIT_MULTITHREADED) }
            .map_err(|error| native_error("iniciar as notificações", error.code().0))?;
        Ok(Self)
    }
}
impl Drop for Apartment {
    fn drop(&mut self) {
        unsafe { RoUninitialize() };
    }
}

fn native_error(action: &str, code: i32) -> String {
    format!(
        "Não foi possível {action} no Windows (0x{:08X}).",
        code as u32
    )
}

fn register(app_id: &str, icon: &Path) -> Result<(), String> {
    let save_icon = || -> std::io::Result<()> {
        if fs::read(icon).is_ok_and(|current| current == ICON) {
            return Ok(());
        }
        let parent = icon
            .parent()
            .ok_or_else(|| std::io::Error::other("Missing icon directory"))?;
        fs::create_dir_all(parent)?;
        let mut file = tempfile::NamedTempFile::new_in(parent)?;
        file.write_all(ICON)?;
        file.as_file().sync_all()?;
        file.persist(icon).map_err(|error| error.error)?;
        Ok(())
    };
    save_icon()
        .map_err(|_| "Não foi possível preparar o ícone das notificações do Jarvis.".to_string())?;
    // Unpackaged Win32 apps need a registered AUMID even when .show() succeeds.
    // This is app identity, not the separate registry key containing user permissions.
    let key = CURRENT_USER
        .create(format!(r"Software\Classes\AppUserModelId\{app_id}"))
        .map_err(|error| native_error("registrar o Jarvis para notificações", error.code().0))?;
    key.set_string("DisplayName", "Jarvis")
        .and_then(|()| key.set_string("IconBackgroundColor", "0"))
        .and_then(|()| {
            key.set_string(
                "IconUri",
                crate::library::strip_verbatim(&icon.to_string_lossy()),
            )
        })
        .map_err(|error| native_error("registrar a identidade das notificações", error.code().0))
}

fn ensure_allowed(setting: NotificationSetting) -> Result<(), String> {
    let message = match setting {
        NotificationSetting::Enabled => return Ok(()),
        NotificationSetting::DisabledForApplication => "Notificações do Jarvis bloqueadas. Ative Jarvis em Configurações do Windows → Sistema → Notificações.",
        NotificationSetting::DisabledForUser => "As notificações estão desativadas no Windows. Confira Configurações → Sistema → Notificações.",
        NotificationSetting::DisabledByGroupPolicy => "As notificações estão bloqueadas por uma política do Windows. Consulte o administrador do computador.",
        NotificationSetting::DisabledByManifest => "O Windows não reconheceu a identidade do Jarvis para notificações. Reabra ou reinstale o aplicativo.",
        _ => "Não foi possível determinar a permissão de notificações do Jarvis no Windows.",
    };
    Err(message.into())
}

fn authorize_identity(app_id: &str, icon: &Path) -> Result<(), String> {
    register(app_id, icon)?;
    let _apartment = Apartment::new()?;
    let executable = std::env::current_exe()
        .map_err(|_| "Não foi possível localizar o executável do Jarvis.".to_string())?;
    shortcut::register(&executable, app_id)?;
    let notifier = ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from(app_id))
        .map_err(|error| native_error("consultar as notificações", error.code().0))?;
    ensure_allowed(
        notifier.Setting().map_err(|error| {
            native_error("consultar as permissões de notificação", error.code().0)
        })?,
    )
}

pub(super) fn setup(app: &tauri::AppHandle) -> Result<(), String> {
    let app_id = app.config().identifier.clone();
    if !valid_app_id(&app_id) {
        return Err(
            "O identificador do aplicativo não corresponde às notificações do Jarvis.".into(),
        );
    }
    let _ = ACTIVE_APP_ID.set(app_id.clone());
    unsafe { SetCurrentProcessExplicitAppUserModelID(&HSTRING::from(app_id.as_str())) }
        .map_err(|error| native_error("identificar o aplicativo Jarvis", error.code().0))?;
    let icon = app
        .path()
        .app_local_data_dir()
        .map_err(|_| "Não foi possível localizar os dados locais do Jarvis.".to_string())?
        .join("notifications/icon.png");
    let icon = ICON_PATH.get_or_init(|| icon);
    let executable = std::env::current_exe()
        .map_err(|_| "Não foi possível localizar o executável do Jarvis.".to_string())?;
    // Tauri owns the UI thread's COM apartment. Register shell objects separately.
    std::thread::spawn(move || {
        register(&app_id, icon)?;
        let _apartment = Apartment::new()?;
        shortcut::register(&executable, &app_id)
    })
    .join()
    .map_err(|_| "Não foi possível preparar as notificações do Windows.".to_string())?
}

pub(super) async fn authorize() -> Result<(), String> {
    let icon = ICON_PATH
        .get()
        .cloned()
        .ok_or("A identidade das notificações não foi inicializada. Reabra o Jarvis.")?;
    let app_id = ACTIVE_APP_ID
        .get()
        .cloned()
        .ok_or("A identidade das notificações não foi inicializada. Reabra o Jarvis.")?;
    tauri::async_runtime::spawn_blocking(move || authorize_identity(&app_id, &icon))
        .await
        .map_err(|_| "Não foi possível consultar as notificações do Windows.".to_string())?
}

#[cfg(test)]
mod tests;
