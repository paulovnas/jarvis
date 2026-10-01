use super::*;

#[test]
fn retired_cli_settings_do_not_prevent_loading_or_reappear_in_saved_preferences() {
    let home = tempfile::tempdir().unwrap();
    let path = crate::data_dir::root(home.path()).join("system.json");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(
        &path,
        r#"{
        "notifications":true,"askUserTimeoutSeconds":75,
        "claude":{"enabled":true,"showUsage":false,"disabledModels":["opus"]},
        "agy":{"enabled":true,"showUsage":true,"disabledModels":["gemini-3-flash"]}
    }"#,
    )
    .unwrap();
    let mut store = Store::open(path.clone()).unwrap();
    assert!(store.preferences.notifications);
    assert_eq!(store.preferences.ask_user_timeout_seconds, 75);
    assert_eq!(store.preferences.claude.disabled_models, ["opus"]);
    assert!(!store.preferences.claude.show_usage);
    store.save(store.preferences.clone()).unwrap();
    let saved: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert!(saved.get("agy").is_none());
    assert!(Store::open(path).unwrap().preferences.notifications);
    assert!(serde_json::from_str::<Preferences>(r#"{"unexpectedSetting":true}"#).is_err());
}

#[test]
fn companion_is_opt_in_and_survives_restart_and_backup() {
    let home = tempfile::tempdir().unwrap();
    let legacy: Preferences = serde_json::from_str(r#"{"notifications":false}"#).unwrap();
    assert!(!legacy.companion_enabled);
    let path = crate::data_dir::root(home.path()).join("system.json");
    let mut store = Store::open(path.clone()).unwrap();
    store
        .save(Preferences {
            companion_enabled: true,
            ..legacy
        })
        .unwrap();
    assert!(Store::open(path).unwrap().preferences.companion_enabled);
    assert!(backup_preferences(home.path()).unwrap().companion_enabled);
}

#[test]
fn dedicated_chat_title_model_defaults_to_automatic_and_survives_restart() {
    let home = tempfile::tempdir().unwrap();
    let path = crate::data_dir::root(home.path()).join("system.json");
    let legacy: Preferences = serde_json::from_str(r#"{"notifications":false}"#).unwrap();
    assert!(legacy.chat_title_model.is_none());
    let choice: crate::agent::workflow::settings::ModelChoice = serde_json::from_value(
        serde_json::json!({"account":"cheap-provider","model":"cheap-model","reasoning":null}),
    )
    .unwrap();
    let mut store = Store::open(path.clone()).unwrap();
    let mut preferences = legacy;
    preferences.chat_title_model = Some(choice.clone());
    store.save(preferences.clone()).unwrap();
    assert_eq!(
        Store::open(path.clone())
            .unwrap()
            .preferences
            .chat_title_model,
        Some(choice.clone())
    );
    assert_eq!(
        backup_preferences(home.path()).unwrap().chat_title_model,
        Some(choice.clone())
    );
    assert_eq!(
        serde_json::to_value(&preferences).unwrap()["chatTitleModel"]["model"],
        "cheap-model"
    );

    let mut invalid = choice.clone();
    invalid.executor = crate::claude::Executor::Claude;
    invalid.account.clear();
    invalid.model = "sonnet".into();
    assert!(validate_chat_title_model(&invalid).is_err());
    invalid = choice.clone();
    invalid.fallback = Some(Box::new(crate::agent::workflow::settings::ModelChoice {
        model: "different-model".into(),
        ..choice.clone()
    }));
    assert!(validate_chat_title_model(&invalid).is_err());
    preferences.chat_title_model = Some(invalid);
    assert!(store.save(preferences).is_err());
    assert_eq!(
        Store::open(path.clone())
            .unwrap()
            .preferences
            .chat_title_model,
        Some(choice)
    );
    store.save(Preferences::default()).unwrap();
    assert!(Store::open(path)
        .unwrap()
        .preferences
        .chat_title_model
        .is_none());
}

#[test]
fn browser_preferences_default_to_embedded_and_persist_without_pairing_credentials() {
    use crate::agent::browser::{BrowserApplication, BrowserMode};
    let legacy: Preferences = serde_json::from_str(r#"{"notifications":false}"#).unwrap();
    assert_eq!(legacy.browser.mode, BrowserMode::Embedded);
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("system.json");
    let mut store = Store::open(path.clone()).unwrap();
    let mut preferences = legacy;
    preferences.browser.mode = BrowserMode::Extension;
    preferences.browser.application = BrowserApplication::Brave;
    store.save(preferences.clone()).unwrap();
    assert_eq!(
        Store::open(path).unwrap().preferences.browser,
        preferences.browser
    );
    assert_eq!(
        serde_json::to_value(preferences).unwrap()["browser"],
        serde_json::json!({"mode":"extension","application":"brave"})
    );
    assert!(serde_json::from_str::<Preferences>(
        r#"{"browser":{"mode":"extension","token":"secret"}}"#
    )
    .is_err());
}

#[test]
fn claude_provider_preferences_survive_restart_and_preserve_legacy_choices() {
    let home = tempfile::tempdir().unwrap();
    let path = crate::data_dir::root(home.path()).join("system.json");
    let legacy: Preferences = serde_json::from_str(r#"{"notifications":false}"#).unwrap();
    assert!(legacy.claude.allows("sonnet"));
    assert!(legacy.claude.show_usage);
    let mut store = Store::open(path.clone()).unwrap();
    let mut preferences = legacy;
    preferences.claude.disabled_models = vec!["opus".into()];
    preferences.claude.show_usage = false;
    store.save(preferences.clone()).unwrap();
    assert_eq!(
        Store::open(path.clone()).unwrap().preferences.claude,
        preferences.claude
    );
    assert!(crate::claude::validate_available_model(home.path(), "sonnet").is_ok());
    assert!(crate::claude::validate_available_model(home.path(), "opus")
        .unwrap_err()
        .contains("Disponibilize"));
    preferences.claude.enabled = false;
    store.save(preferences.clone()).unwrap();
    assert!(
        crate::claude::validate_available_model(home.path(), "sonnet")
            .unwrap_err()
            .contains("Ative")
    );
    preferences.claude.disabled_models = vec!["--invalid".into()];
    assert!(store.save(preferences).is_err());
    assert_eq!(
        Store::open(path)
            .unwrap()
            .preferences
            .claude
            .disabled_models,
        ["opus"]
    );
    assert!(
        serde_json::from_str::<crate::claude::ProviderPreferences>(r#"{"alias":"second"}"#)
            .is_err()
    );
}

#[test]
fn terminal_fonts_prioritize_nerd_fonts_and_report_missing_configurations() {
    let fonts = order_terminal_fonts([
        "Menlo".to_string(),
        "NotoSansM Nerd Font Mono".to_string(),
        "menlo".to_string(),
    ]);
    assert_eq!(fonts[0], "NotoSansM Nerd Font Mono");
    assert!(fonts.iter().any(|font| font == BUNDLED_TERMINAL_FONT));
    assert_eq!(
        fonts
            .iter()
            .filter(|font| font.eq_ignore_ascii_case("Menlo"))
            .count(),
        1
    );
    assert!(terminal_font_error(Some("NotoSansM Nerd Font Mono"), &fonts).is_none());
    assert!(terminal_font_error(Some("MesloLGS NF"), &fonts)
        .is_some_and(|error| error.contains("não foi encontrada")));
}

#[test]
fn application_exit_waits_for_the_power_worker_and_is_repeatable() {
    let system = SystemState::default();
    let agent = crate::agent::AgentState::default();
    let (stop, receive) = mpsc::channel::<bool>();
    let (released, confirmation) = mpsc::channel();
    let worker = std::thread::spawn(move || {
        assert!(receive.recv().unwrap());
        released.send(()).unwrap();
    });
    *system.worker.lock().unwrap() = Some((stop, worker));
    crate::shutdown_services(&system, &agent).unwrap();
    confirmation
        .try_recv()
        .expect("The OS power worker must finish before the updater exits");
    crate::shutdown_services(&system, &agent).unwrap();
    assert!(system.worker.lock().unwrap().is_none());
}

#[cfg(target_os = "macos")]
#[test]
#[ignore = "Requires the macOS power service; run explicitly on the host"]
fn native_power_assertion_is_visible_and_released() {
    let reason = format!("jarvis-power-test-{}", std::process::id());
    let assertions = || {
        String::from_utf8(
            std::process::Command::new("/usr/bin/pmset")
                .args(["-g", "assertions"])
                .output()
                .unwrap()
                .stdout,
        )
        .unwrap()
    };
    assert!(!assertions().contains(&reason));
    let awake = keepawake::Builder::default()
        .idle(true)
        .reason(reason.clone())
        .create()
        .unwrap();
    assert!(assertions().contains(&reason));
    drop(awake);
    assert!(!assertions().contains(&reason));
}

#[test]
fn preferences_restore_all_modes_without_touching_layout() {
    let home = tempfile::tempdir().unwrap();
    let path = crate::data_dir::root(home.path()).join("system.json");
    let mut store = Store::open(path.clone()).unwrap();
    assert_eq!(store.preferences, Preferences::default());
    for mode in [SleepMode::Active, SleepMode::Open, SleepMode::Off] {
        let preferences = Preferences {
            prevent_sleep: mode,
            notifications: mode != SleepMode::Off,
            companion_enabled: false,
            ask_user_timeout_seconds: 45,
            response_language: if mode == SleepMode::Active {
                ResponseLanguage::English
            } else {
                ResponseLanguage::PortugueseBrazil
            },
            terminal: TerminalPreferences::default(),
            claude: crate::claude::ProviderPreferences::default(),
            _retired_cli_preferences: (),
            browser: crate::agent::browser::BrowserPreferences::default(),
            chat_title_model: None,
        };
        store.save(preferences.clone()).unwrap();
        assert_eq!(Store::open(path.clone()).unwrap().preferences, preferences);
    }
    assert!(!crate::data_dir::root(home.path())
        .join("desktop.json")
        .exists());
}

#[test]
fn unreadable_preferences_are_preserved_and_failed_saves_do_not_change_runtime() {
    let home = tempfile::tempdir().unwrap();
    let path = home.path().join("system.json");
    fs::write(&path, r#"{"preventSleep":"future-mode"}"#).unwrap();
    assert!(Store::open(path.clone()).is_err());
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        r#"{"preventSleep":"future-mode"}"#
    );
    let mut store = Store {
        path: path.join("cannot-write"),
        preferences: Preferences::default(),
    };
    assert!(store
        .save(Preferences {
            prevent_sleep: SleepMode::Open,
            notifications: true,
            companion_enabled: false,
            ask_user_timeout_seconds: 30,
            response_language: ResponseLanguage::Spanish,
            terminal: TerminalPreferences::default(),
            claude: crate::claude::ProviderPreferences::default(),
            _retired_cli_preferences: (),
            browser: crate::agent::browser::BrowserPreferences::default(),
            chat_title_model: None,
        })
        .is_err());
    assert_eq!(store.preferences, Preferences::default());
}

#[test]
fn question_timeout_defaults_for_existing_installs_and_rejects_invalid_values() {
    let home = tempfile::tempdir().unwrap();
    let directory = crate::data_dir::root(home.path());
    fs::create_dir_all(&directory).unwrap();
    let path = directory.join("system.json");
    fs::write(&path, r#"{"preventSleep":"off","notifications":true}"#).unwrap();
    assert_eq!(ask_user_timeout_seconds(home.path()), 30);
    let mut store = Store::open(path).unwrap();
    assert_eq!(store.preferences.ask_user_timeout_seconds, 30);
    assert_eq!(
        store.preferences.response_language,
        ResponseLanguage::PortugueseBrazil
    );
    assert_eq!(
        response_language(home.path()),
        ResponseLanguage::PortugueseBrazil
    );
    assert_eq!(store.preferences.terminal, TerminalPreferences::default());
    assert!(store
        .save(Preferences {
            ask_user_timeout_seconds: 0,
            ..Preferences::default()
        })
        .is_err());
    assert_eq!(store.preferences.ask_user_timeout_seconds, 30);
    fs::write(
        crate::data_dir::root(home.path()).join("system.json"),
        r#"{"preventSleep":"off","notifications":true,"askUserTimeoutSeconds":0}"#,
    )
    .unwrap();
    assert_eq!(ask_user_timeout_seconds(home.path()), 30);
}

#[test]
fn imported_preferences_reset_only_nonportable_terminal_values() {
    let mut preferences = Preferences {
        terminal: TerminalPreferences {
            shell: Some("/bin/zsh".into()),
            arguments: vec!["-il".into()],
            font_family: Some("Jarvis Test Font That Is Not Installed".into()),
            font_size: 15,
        },
        ..Preferences::default()
    };
    let source = if std::env::consts::OS == "windows" {
        "macos"
    } else {
        "windows"
    };
    let normalization = normalize_imported_preferences(&mut preferences, Some(source));

    assert!(normalization.terminal_shell_reset);
    assert!(normalization.terminal_font_reset);
    assert_eq!(preferences.terminal.shell, None);
    assert!(preferences.terminal.arguments.is_empty());
    assert_eq!(preferences.terminal.font_family, None);
    assert_eq!(preferences.terminal.font_size, 15);
}

#[test]
fn sleep_policy_follows_activity_and_open_mode() {
    assert!(!SleepMode::Off.inhibit(false));
    assert!(!SleepMode::Off.inhibit(true));
    assert!(!SleepMode::Active.inhibit(false));
    assert!(SleepMode::Active.inhibit(true));
    assert!(SleepMode::Open.inhibit(false));
    assert!(SleepMode::Open.inhibit(true));
}

#[test]
fn power_assertion_is_not_duplicated_and_releases_on_idle_setting_change_and_exit() {
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };
    struct Lease(Arc<AtomicUsize>);
    impl Drop for Lease {
        fn drop(&mut self) {
            self.0.fetch_sub(1, Ordering::SeqCst);
        }
    }
    let count = Arc::new(AtomicUsize::new(0));
    let acquire = || {
        count.fetch_add(1, Ordering::SeqCst);
        Ok(Lease(count.clone()))
    };
    let mut power = Power::default();
    let now = std::time::Instant::now();
    power.reconcile(true, now, acquire);
    power.reconcile(true, now, acquire);
    assert_eq!(count.load(Ordering::SeqCst), 1);
    power.reconcile(false, now, acquire);
    assert_eq!(count.load(Ordering::SeqCst), 0);
    power.reconcile(true, now, acquire);
    drop(power);
    assert_eq!(count.load(Ordering::SeqCst), 0);
}

#[test]
fn failed_inhibition_reports_failure_and_retries_without_busy_loop() {
    let mut power = Power::<()>::default();
    let now = std::time::Instant::now();
    power.reconcile(true, now, || Err("unavailable".into()));
    assert_eq!(power.status(), (false, Some("unavailable".into())));
    power.reconcile(true, now + Duration::from_secs(1), || {
        panic!("must back off")
    });
    power.reconcile(true, now + Duration::from_secs(31), || Ok(()));
    assert_eq!(power.status(), (true, None));
    power.reconcile(false, now, || panic!("must release"));
    assert_eq!(power.status(), (false, None));
}

#[test]
fn repeated_questions_and_terminals_are_deduplicated_per_conversation_with_bounded_memory() {
    let mut recent = Recent::default();
    assert!(recent.insert("chat-a/agent/turn/question".into()));
    assert!(!recent.insert("chat-a/agent/turn/question".into()));
    assert!(recent.insert("chat-b/agent/turn/question".into()));
    assert!(recent.insert("chat-a/agent/turn/next-question".into()));
    for i in 0..1024 {
        recent.insert(format!("chat-a/{i}/Completed"));
    }
    assert_eq!(recent.keys.len(), 512);
    assert_eq!(recent.order.len(), 512);
    assert!(!recent.insert("chat-a/1023/Completed".into()));
}
