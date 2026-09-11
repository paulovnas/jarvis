use super::*;

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
    crate::shutdown_services(&system, &agent);
    confirmation
        .try_recv()
        .expect("The OS power worker must finish before the updater exits");
    crate::shutdown_services(&system, &agent);
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
    let path = home.path().join(".jarvis/system.json");
    let mut store = Store::open(path.clone()).unwrap();
    assert_eq!(store.preferences, Preferences::default());
    for mode in [SleepMode::Active, SleepMode::Open, SleepMode::Off] {
        let preferences = Preferences {
            prevent_sleep: mode,
            notifications: mode != SleepMode::Off,
            ask_user_timeout_seconds: 45,
            response_language: if mode == SleepMode::Active {
                ResponseLanguage::English
            } else {
                ResponseLanguage::PortugueseBrazil
            },
            terminal: TerminalPreferences::default(),
        };
        store.save(preferences.clone()).unwrap();
        assert_eq!(Store::open(path.clone()).unwrap().preferences, preferences);
    }
    assert!(!home.path().join(".jarvis/desktop.json").exists());
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
            ask_user_timeout_seconds: 30,
            response_language: ResponseLanguage::Spanish,
            terminal: TerminalPreferences::default(),
        })
        .is_err());
    assert_eq!(store.preferences, Preferences::default());
}

#[test]
fn question_timeout_defaults_for_existing_installs_and_rejects_invalid_values() {
    let home = tempfile::tempdir().unwrap();
    let directory = home.path().join(".jarvis");
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
        home.path().join(".jarvis/system.json"),
        r#"{"preventSleep":"off","notifications":true,"askUserTimeoutSeconds":0}"#,
    )
    .unwrap();
    assert_eq!(ask_user_timeout_seconds(home.path()), 30);
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
