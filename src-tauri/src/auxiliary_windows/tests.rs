use super::*;

#[test]
fn only_fixed_local_surfaces_can_be_requested() {
    assert_eq!(
        serde_json::from_str::<AuxiliaryWindowKind>("\"settings\"").unwrap(),
        AuxiliaryWindowKind::Settings
    );
    assert_eq!(
        serde_json::from_str::<AuxiliaryWindowKind>("\"about\"").unwrap(),
        AuxiliaryWindowKind::About
    );
    for value in ["browser-1", "main", "https://example.com", "Settings"] {
        assert!(serde_json::from_value::<AuxiliaryWindowKind>(value.into()).is_err());
    }
    assert_eq!(AuxiliaryWindowKind::Settings.spec().label, "settings");
    assert_eq!(AuxiliaryWindowKind::About.spec().label, "about");
}

#[test]
fn packaged_macos_entrypoint_can_load_without_a_trailing_slash() {
    let config: tauri::utils::config::Config =
        serde_json::from_str(include_str!("../../tauri.conf.json")).unwrap();
    assert!(matches!(
        config.build.frontend_dist,
        Some(tauri::utils::config::FrontendDist::Directory(_))
    ));

    // Tauri serves this embedded frontend at its bare custom-protocol origin
    // on macOS, even when the builder requests App("index.html").
    let entrypoint = tauri::Url::parse("tauri://localhost").unwrap();
    assert!(entrypoint.path().is_empty());
    assert!(local_navigation(&entrypoint, None));
    assert!(local_navigation(&entrypoint, config.build.dev_url.as_ref()));
}

#[test]
fn empty_path_does_not_allow_foreign_origins_ports_or_credentials() {
    for value in [
        "tauri://example.com",
        "tauri://localhost.example.com",
        "tauri://127.0.0.1",
        "tauri://localhost:9999",
        "tauri://user@localhost",
        "tauri://user:password@localhost",
        "file://localhost",
        "http://tauri.localhost:9999",
        "https://user:password@tauri.localhost",
    ] {
        assert!(
            !local_navigation(&tauri::Url::parse(value).unwrap(), None),
            "{value}"
        );
    }
}

#[test]
fn windows_keep_navigation_inside_the_bundled_entrypoint() {
    for value in [
        "tauri://localhost/index.html",
        "http://tauri.localhost/index.html",
        "https://tauri.localhost/",
    ] {
        assert!(local_navigation(&tauri::Url::parse(value).unwrap(), None));
    }
    for value in [
        "https://example.com/",
        "https://tauri.localhost.example.com/index.html",
        "http://tauri.localhost:9999/index.html",
        "http://user@tauri.localhost/index.html",
        "file:///tmp/index.html",
        "tauri://localhost/remote.html",
        "http://tauri.localhost/browser-extension/options.html",
    ] {
        assert!(!local_navigation(&tauri::Url::parse(value).unwrap(), None));
    }
    let development = tauri::Url::parse("http://localhost:1420").unwrap();
    assert_eq!(
        local_navigation(
            &tauri::Url::parse("http://localhost:1420/index.html").unwrap(),
            Some(&development)
        ),
        cfg!(debug_assertions)
    );
    assert!(!local_navigation(
        &tauri::Url::parse("http://localhost:1421/index.html").unwrap(),
        Some(&development)
    ));
}

#[test]
fn settings_can_configure_the_app_but_cannot_execute_chats() {
    for command in [
        "list_provider_accounts",
        "save_system_preferences",
        "get_agent_models",
        "set_agent_model",
        "mutate_workflow_catalog",
        "configure_context7",
        "get_web_search_config",
        "set_image_generation_config",
        "refresh_remote_pairing",
        "import_settings_backup",
    ] {
        assert!(command_allowed("settings", command), "{command}");
        assert!(!command_allowed("browser-1", command), "{command}");
    }
    for command in [
        "start_agent_turn",
        "run_shell",
        "browser_command",
        "send_http_request",
        "get_chat_history",
        "subscribe_chat",
        "set_self_development_enabled",
        "capture_self_development_incident",
        "confirm_app_exit",
    ] {
        assert!(!command_allowed("settings", command), "{command}");
        assert!(!command_allowed("about", command), "{command}");
    }
}

#[test]
fn about_has_update_access_without_configuration_mutation() {
    for command in [
        "check_app_update",
        "get_app_shutdown_status",
        "install_app_update",
        "get_system_preferences",
    ] {
        assert!(command_allowed("about", command));
        assert!(!command_allowed("browser-1", command));
    }
    for command in [
        "save_system_preferences",
        "disconnect_provider_account",
        "set_agent_model",
        "install_core_component",
    ] {
        assert!(!command_allowed("about", command));
    }
}

#[test]
fn initial_windows_and_minimum_sizes_fit_the_monitor_work_area() {
    for kind in [AuxiliaryWindowKind::Settings, AuxiliaryWindowKind::About] {
        let spec = kind.spec();
        let roomy = fitted_size(spec, 1920.0, 1080.0);
        assert_eq!(
            roomy,
            (spec.width, spec.height, spec.min_width, spec.min_height)
        );
        for (work_width, work_height) in [(800.0, 600.0), (960.0, 640.0)] {
            let (width, height, min_width, min_height) = fitted_size(spec, work_width, work_height);
            assert!(width <= work_width - 32.0);
            assert!(height <= work_height - 64.0);
            assert!(min_width <= width);
            assert!(min_height <= height);
        }
    }
}

#[test]
fn plugin_capabilities_are_scoped_to_the_two_local_webviews() {
    for (source, label) in [
        (
            include_str!("../../capabilities/auxiliary-settings.json"),
            "settings",
        ),
        (
            include_str!("../../capabilities/auxiliary-about.json"),
            "about",
        ),
    ] {
        let value: serde_json::Value = serde_json::from_str(source).unwrap();
        assert_eq!(value["webviews"], serde_json::json!([label]));
        assert!(value.get("remote").is_none());
        let permissions = value["permissions"].as_array().unwrap();
        assert!(permissions.contains(&serde_json::json!("core:window:allow-close")));
        // Tauri's onCloseRequested listener finishes an accepted close with destroy().
        assert!(
            permissions.contains(&serde_json::json!("core:window:allow-destroy")),
            "{label} must be able to finish an accepted native close request"
        );
        assert!(!permissions.contains(&serde_json::json!("core:default")));
        assert!(!permissions.iter().any(|permission| permission
            .as_str()
            .is_some_and(|name| name.contains("create") || name.starts_with("shell:"))));
    }
}
