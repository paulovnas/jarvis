use super::*;

#[test]
fn registers_identity_idempotently_without_changing_existing_values_or_preferences() {
    let id = format!("{APP_ID}.test.{}", crate::library::new_id().unwrap());
    let path = format!(r"Software\Classes\AppUserModelId\{id}");
    struct Registration(String);
    impl Drop for Registration {
        fn drop(&mut self) {
            let _ = CURRENT_USER.remove_tree(&self.0);
        }
    }
    let _registration = Registration(path.clone());
    let temp = tempfile::tempdir().unwrap();
    let icon = temp.path().join("ação com espaços/icon.png");
    let key = CURRENT_USER.create(&path).unwrap();
    key.set_u32("UserPreferenceFixture", 0).unwrap();
    register(&id, &icon).unwrap();
    let modified = fs::metadata(&icon).unwrap().modified().unwrap();
    register(&id, &icon).unwrap();
    assert_eq!(key.get_string("DisplayName").unwrap(), "Jarvis");
    assert_eq!(Path::new(&key.get_string("IconUri").unwrap()), icon);
    assert_eq!(key.get_u32("UserPreferenceFixture").unwrap(), 0);
    assert_eq!(fs::read(&icon).unwrap(), ICON);
    assert_eq!(fs::metadata(&icon).unwrap().modified().unwrap(), modified);
}

#[test]
fn blocked_notification_settings_return_actionable_errors_without_requesting_permission() {
    assert!(ensure_allowed(NotificationSetting::Enabled).is_ok());
    for (setting, message) in [
        (NotificationSetting::DisabledForApplication, "Ative Jarvis"),
        (
            NotificationSetting::DisabledForUser,
            "desativadas no Windows",
        ),
        (
            NotificationSetting::DisabledByGroupPolicy,
            "política do Windows",
        ),
        (
            NotificationSetting::DisabledByManifest,
            "identidade do Jarvis",
        ),
        (NotificationSetting(99), "determinar a permissão"),
    ] {
        assert!(ensure_allowed(setting).unwrap_err().contains(message));
    }
}

#[tokio::test]
#[ignore = "Registers Jarvis for this Windows user and sends one visible native test notification"]
async fn native_jarvis_notification_reaches_windows_history() {
    unsafe { SetCurrentProcessExplicitAppUserModelID(&HSTRING::from(APP_ID)).unwrap() };
    let icon = PathBuf::from(std::env::var_os("LOCALAPPDATA").unwrap())
        .join(APP_ID)
        .join("notifications/icon.png");
    ICON_PATH.get_or_init(|| icon);
    {
        let _apartment = Apartment::new().unwrap();
        let executable = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/release/jarvis.exe");
        shortcut::register(&executable).unwrap();
    }
    let marker = format!("Teste Windows {}", crate::library::new_id().unwrap());
    super::super::show("Jarvis · Notificação de teste", &marker)
        .await
        .unwrap();
    let _apartment = Apartment::new().unwrap();
    let history = ToastNotificationManager::History().unwrap();
    for _ in 0..50 {
        let notifications = history.GetHistoryWithId(&HSTRING::from(APP_ID)).unwrap();
        for notification in notifications {
            if notification
                .Content()
                .unwrap()
                .GetXml()
                .unwrap()
                .to_string()
                .contains(&marker)
            {
                // Keep the sender alive while Windows presents the banner, as the app is.
                std::thread::sleep(std::time::Duration::from_secs(10));
                return;
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    panic!("Windows did not retain the Jarvis test notification in notification history");
}
