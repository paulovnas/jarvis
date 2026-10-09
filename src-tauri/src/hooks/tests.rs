use super::*;

fn hook() -> Hook {
    Hook {
        id: "a".repeat(32),
        name: "Verificar comando".into(),
        event: Event::PreToolUse,
        command: "printf '{}'".into(),
        matcher: "bash|apply_patch".into(),
        timeout_seconds: 30,
        enabled: true,
    }
}

#[test]
fn manual_edits_are_persisted_and_native_hooks_remain_read_only() {
    let home = tempfile::tempdir().unwrap();
    let state = AppState::default();
    let initial = load(&state, home.path()).unwrap();
    assert_eq!(initial.revision, 0);
    assert!(initial.hooks.is_empty());
    assert_eq!(initial.native_hooks.len(), 12);
    assert!(initial
        .native_hooks
        .iter()
        .any(|hook| hook.id == "native-impeccable-edit" && hook.event == Event::PostToolUse));
    let created = upsert(&state, home.path(), hook(), 0).unwrap();
    assert_eq!(created.revision, 1);
    assert_eq!(read(home.path()).unwrap(), created);
    let mut edited = hook();
    edited.name = " Verificar antes de executar ".into();
    edited.enabled = false;
    let updated = upsert(&state, home.path(), edited.clone(), 1).unwrap();
    assert_eq!(updated.hooks.len(), 1);
    assert_eq!(updated.hooks[0].name, "Verificar antes de executar");
    assert!(!updated.hooks[0].enabled);
    assert_eq!(updated.native_hooks, initial.native_hooks);
    assert_eq!(
        upsert(&state, home.path(), edited, 1).unwrap_err().code,
        "hooks_conflict"
    );
    assert_eq!(read(home.path()).unwrap(), updated);
    assert!(delete(&state, home.path(), &initial.native_hooks[0].id, 2).is_err());
    let mut native = hook();
    native.id = initial.native_hooks[0].id.clone();
    assert!(upsert(&state, home.path(), native, 2).is_err());
    assert!(delete(&state, home.path(), "missing", 2).is_err());
    let removed = delete(&state, home.path(), &hook().id, 2).unwrap();
    assert_eq!(removed.revision, 3);
    assert!(removed.hooks.is_empty());
    assert_eq!(read(home.path()).unwrap(), removed);
    let stored: serde_json::Value = serde_json::from_slice(
        &fs::read(crate::data_dir::root(home.path()).join("hooks.json")).unwrap(),
    )
    .unwrap();
    assert!(stored.get("nativeHooks").is_none());
    assert!(created.untrusted_ids.is_empty());
}

#[test]
fn unreviewed_edits_remain_visible_but_cannot_execute_until_that_hook_is_saved() {
    let home = tempfile::tempdir().unwrap();
    let state = AppState::default();
    let first = upsert(&state, home.path(), hook(), 0).unwrap();
    let mut other = hook();
    other.id = "b".repeat(32);
    let approved = upsert(&state, home.path(), other.clone(), first.revision).unwrap();
    assert_eq!(approved.trusted_hooks().count(), 2);

    let path = crate::data_dir::root(home.path()).join("hooks.json");
    let mut external: Stored = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    external.hooks[0].command = "printf 'unreviewed'".into();
    fs::write(&path, serde_json::to_vec(&external).unwrap()).unwrap();
    let changed = load(&state, home.path()).unwrap();
    assert_eq!(changed.hooks[0].command, "printf 'unreviewed'");
    assert_eq!(changed.untrusted_ids, [hook().id]);
    assert_eq!(
        changed
            .trusted_hooks()
            .map(|hook| &hook.id)
            .collect::<Vec<_>>(),
        [&other.id]
    );
    assert_eq!(changed.native_hooks, approved.native_hooks);
    let serialized = serde_json::to_value(&changed).unwrap();
    assert_eq!(serialized["untrustedIds"], serde_json::json!([hook().id]));
    assert!(serialized.get("trustedHashes").is_none());

    other.enabled = false;
    let unrelated = upsert(&state, home.path(), other, changed.revision).unwrap();
    assert_eq!(read(home.path()).unwrap().untrusted_ids, [hook().id]);
    let mut reviewed = unrelated.hooks[0].clone();
    reviewed.enabled = false;
    let saved = upsert(&state, home.path(), reviewed, unrelated.revision).unwrap();
    assert!(saved.untrusted_ids.is_empty());
    assert_eq!(saved.trusted_hooks().count(), 2);
    assert!(!saved.hooks[0].matches(Event::PreToolUse, Some("bash")));
    assert_eq!(read(home.path()).unwrap(), saved);
}

#[test]
fn hooks_without_saved_trust_are_reviewable_and_deletion_preserves_other_trust() {
    let home = tempfile::tempdir().unwrap();
    let directory = crate::data_dir::root(home.path());
    fs::create_dir_all(&directory).unwrap();
    let path = directory.join("hooks.json");
    fs::write(
        &path,
        serde_json::to_vec(&serde_json::json!({"revision":0,"hooks":[hook()]})).unwrap(),
    )
    .unwrap();
    let unreviewed = read(home.path()).unwrap();
    assert_eq!(unreviewed.untrusted_ids, [hook().id]);
    assert_eq!(unreviewed.trusted_hooks().count(), 0);
    let state = AppState::default();
    let mut other = hook();
    other.id = "b".repeat(32);
    let saved = upsert(&state, home.path(), other, unreviewed.revision).unwrap();
    assert_eq!(saved.untrusted_ids, [hook().id]);
    let removed = delete(&state, home.path(), &hook().id, saved.revision).unwrap();
    assert!(removed.untrusted_ids.is_empty());
    assert_eq!(removed.trusted_hooks().count(), 1);
    assert_eq!(read(home.path()).unwrap(), removed);
}

#[test]
fn matcher_uses_codex_exact_alternatives_and_regex_and_respects_enabled_state() {
    let mut hook = hook();
    assert!(hook.matches(Event::PreToolUse, Some("bash")));
    assert!(hook.matches(Event::PreToolUse, Some("apply_patch")));
    assert!(!hook.matches(Event::PreToolUse, Some("bash_extra")));
    assert!(!hook.matches(Event::PostToolUse, Some("bash")));
    assert!(!hook.matches(Event::PreToolUse, None));
    hook.matcher = "^mcp__.*".into();
    assert!(hook.matches(Event::PreToolUse, Some("mcp__context__execute")));
    assert!(!hook.matches(Event::PreToolUse, Some("bash")));
    for matcher in ["", "*"] {
        hook.matcher = matcher.into();
        assert!(hook.matches(Event::PreToolUse, None));
    }
    hook.enabled = false;
    assert!(!hook.matches(Event::PreToolUse, Some("bash")));
    hook.enabled = true;
    hook.event = Event::Stop;
    hook.matcher = "bash".into();
    assert!(hook.matches(Event::Stop, None));
    hook.event = Event::UserPromptSubmit;
    assert!(hook.matches(Event::UserPromptSubmit, None));
}

#[test]
fn invalid_manual_settings_do_not_mutate_the_catalog() {
    let initial = catalog(Stored::default()).unwrap();
    let mut invalid_hooks = Vec::new();
    let mut invalid = hook();
    invalid.event = Event::BeforeAgent;
    invalid_hooks.push(invalid);
    let mut invalid = hook();
    invalid.timeout_seconds = 0;
    invalid_hooks.push(invalid);
    let mut invalid = hook();
    invalid.timeout_seconds = 601;
    invalid_hooks.push(invalid);
    let mut invalid = hook();
    invalid.matcher = "[".into();
    invalid_hooks.push(invalid);
    let mut invalid = hook();
    invalid.command = " \n".into();
    invalid_hooks.push(invalid);
    let mut invalid = hook();
    invalid.command = "echo\0ok".into();
    invalid_hooks.push(invalid);
    let mut invalid = hook();
    invalid.name = " ".into();
    invalid_hooks.push(invalid);
    let mut invalid = hook();
    invalid.id = "../../arbitrary-file".into();
    invalid_hooks.push(invalid);
    for invalid in invalid_hooks {
        assert_eq!(
            preview_upsert(&initial, invalid).unwrap_err().code,
            "invalid_hook"
        );
    }
    assert!(initial.hooks.is_empty());
    let mut unknown = serde_json::to_value(hook()).unwrap();
    unknown["native"] = true.into();
    assert!(serde_json::from_value::<Hook>(unknown).is_err());
}

#[test]
fn stored_unknown_events_duplicates_and_native_overrides_are_rejected() {
    let home = tempfile::tempdir().unwrap();
    let directory = crate::data_dir::root(home.path());
    fs::create_dir_all(&directory).unwrap();
    let path = directory.join("hooks.json");
    fs::write(
        &path,
        serde_json::to_vec(&serde_json::json!({"revision": 0, "hooks": [], "nativeHooks": []}))
            .unwrap(),
    )
    .unwrap();
    assert!(read(home.path()).is_err());
    let duplicate = Stored {
        revision: 1,
        hooks: vec![hook(), hook()],
        ..Stored::default()
    };
    fs::write(&path, serde_json::to_vec(&duplicate).unwrap()).unwrap();
    assert!(read(home.path()).is_err());
    let mut unknown = serde_json::to_value(hook()).unwrap();
    unknown["event"] = "UnsupportedEvent".into();
    fs::write(
        &path,
        serde_json::to_vec(&serde_json::json!({"revision":0,"hooks":[unknown]})).unwrap(),
    )
    .unwrap();
    assert!(read(home.path()).is_err());
}

#[test]
fn hook_count_and_serialized_size_are_bounded_without_losing_previous_settings() {
    let home = tempfile::tempdir().unwrap();
    let state = AppState::default();
    let initial = upsert(&state, home.path(), hook(), 0).unwrap();
    let hooks = (0..MAX_HOOKS)
        .map(|index| {
            let mut hook = hook();
            hook.id = format!("{index:032x}");
            hook.command = "\n".repeat(16_380);
            hook.command.push_str("true");
            hook
        })
        .collect::<Vec<_>>();
    let mut oversized = catalog(Stored {
        revision: 2,
        hooks,
        ..Stored::default()
    })
    .unwrap();
    assert!(write(home.path(), &oversized).is_err());
    assert_eq!(read(home.path()).unwrap(), initial);
    let mut extra = hook();
    extra.id = "f".repeat(32);
    assert!(preview_upsert(&oversized, extra).is_err());
    oversized.hooks.pop();
    assert!(catalog(Stored {
        revision: 0,
        hooks: oversized.hooks,
        ..Stored::default()
    })
    .is_ok());
}
