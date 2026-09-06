use super::*;

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
            notifications: true
        })
        .is_err());
    assert_eq!(store.preferences, Preferences::default());
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
