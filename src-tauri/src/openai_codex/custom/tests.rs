use super::*;
use crate::openai_codex::InMemorySecretStore;
use crate::persistence;

pub(crate) fn config(protocol: Protocol) -> Config {
    Config {
        base_url: "https://gateway.example/v1".into(),
        protocol,
        auth_mode: AuthMode::Bearer,
        token_field: TokenField::MaxTokens,
        replay_unsigned_thinking: false,
        models: vec![Model {
            id: "vendor/model-v4".into(),
            name: "My model".into(),
            context_window: 64_000,
            max_output_tokens: 4_000,
            supports_images: true,
            supports_tools: true,
            reasoning: Reasoning::None,
            reasoning_levels: vec![],
            default_reasoning_level: None,
            thinking_budget: None,
        }],
    }
}
#[test]
fn config_requires_explicit_limits_unique_models_and_protocol_specific_reasoning() {
    let valid = config(Protocol::OpenaiCompletions);
    assert!(valid.validate().is_ok());
    let raw = serde_json::to_value(&valid).unwrap();
    for field in [
        "contextWindow",
        "maxOutputTokens",
        "supportsTools",
        "supportsImages",
    ] {
        let mut missing = raw.clone();
        missing["models"][0].as_object_mut().unwrap().remove(field);
        assert!(
            serde_json::from_value::<Config>(missing).is_err(),
            "missing {field}"
        );
    }
    for (context, output) in [
        (0, 1),
        (4095, 1),
        (64_000, 0),
        (64_000, 64_000),
        (100_000_001, 4000),
    ] {
        let mut bad = valid.clone();
        bad.models[0].context_window = context;
        bad.models[0].max_output_tokens = output;
        assert!(bad.validate().is_err());
    }
    let mut duplicate = valid.clone();
    duplicate.models.push(duplicate.models[0].clone());
    assert!(duplicate.validate().is_err());
    let mut thinking = valid.clone();
    thinking.models[0].reasoning = Reasoning::Budget;
    thinking.models[0].reasoning_levels = vec!["on".into()];
    thinking.models[0].default_reasoning_level = Some("on".into());
    thinking.models[0].thinking_budget = Some(1024);
    assert!(thinking.validate().is_err());
    thinking.protocol = Protocol::AnthropicMessages;
    assert!(thinking.validate().is_ok());
    thinking.models[0].thinking_budget = Some(4000);
    assert!(thinking.validate().is_err());
}
#[test]
fn endpoint_keeps_gateway_paths_and_rejects_credentials_or_wrong_protocol() {
    for (protocol, suffix) in [
        (Protocol::OpenaiCompletions, "/chat/completions"),
        (Protocol::OpenaiResponses, "/responses"),
        (Protocol::AnthropicMessages, "/messages"),
    ] {
        let mut value = config(protocol);
        value.base_url = "https://openrouter.ai/api/v1/".into();
        let expected = format!("https://openrouter.ai/api/v1{suffix}");
        assert_eq!(value.endpoint().unwrap().as_str(), expected);
        value.base_url = expected.clone();
        assert_eq!(value.endpoint().unwrap().as_str(), expected);
    }
    for base in [
        "file:///tmp/key",
        "http://gateway.example/v1",
        "https://user:secret@example.com/v1",
        "https://example.com/v1?key=secret",
        "https://example.com/v1#fragment",
        "https://example.com/v1/messages",
    ] {
        let mut bad = config(Protocol::OpenaiCompletions);
        bad.base_url = base.into();
        assert!(bad.validate().is_err(), "{base}");
    }
    let mut local = config(Protocol::OpenaiCompletions);
    local.base_url = "http://127.0.0.1:8080/v1".into();
    assert!(local.validate().is_ok());
}
#[test]
fn custom_alias_is_user_owned_and_cannot_confuse_model_path_delimiters() {
    for alias in ["OpenRouter", "my.gateway-1", "openai-codex-mine", "team_2"] {
        assert!(validate_alias(alias).is_ok());
    }
    for alias in ["", "x/y", " x", "../x", "x\n"] {
        assert!(validate_alias(alias).is_err());
    }
}
#[test]
fn save_and_edit_preserve_secure_key_and_offline_catalog_without_testing_endpoint() {
    let home = tempfile::tempdir().unwrap();
    let state = AppState::default();
    let store = InMemorySecretStore::default();
    let alias = "My.OpenRouter";
    let initial = config(Protocol::OpenaiResponses);
    state
        .with_connection(home.path(), |db| {
            save(db, &store, alias, &initial, Some("secret-key"), false)
        })
        .unwrap();
    assert!(state
        .with_connection(home.path(), |db| save(
            db,
            &store,
            alias,
            &initial,
            Some("another"),
            false
        ))
        .is_err());
    let mut updated = initial.clone();
    updated.models[0].name = "Updated".into();
    state
        .with_connection(home.path(), |db| {
            save(db, &store, alias, &updated, None, true)
        })
        .unwrap();
    assert_eq!(store.load(alias).unwrap().access, "secret-key");
    assert_eq!(load(&state, home.path(), alias).unwrap(), updated);
    let record = state.list_provider_accounts(home.path()).unwrap().remove(0);
    let visible = serde_json::to_string(&account(&state, home.path(), record).unwrap()).unwrap();
    assert!(!visible.contains("secret-key"));
    assert!(!visible.contains("accountId"));
    assert!(visible.contains("Updated"));
    state
        .with_connection(home.path(), |db| {
            save(db, &store, alias, &updated, Some("rotated-key"), true)
        })
        .unwrap();
    assert_eq!(store.load(alias).unwrap().access, "rotated-key");
    state
        .with_connection(home.path(), |db| {
            crate::openai_codex::disconnect_provider_account(db, &store, alias)
        })
        .unwrap();
    assert!(load(&state, home.path(), alias).is_err());
    assert!(store.load(alias).is_err());
}
#[test]
fn secure_store_failure_leaves_previous_configuration_intact() {
    let mut db = Connection::open_in_memory().unwrap();
    persistence::initialize_database(&mut db).unwrap();
    let store = InMemorySecretStore::default();
    let initial = config(Protocol::OpenaiCompletions);
    store.fail_store(true);
    assert!(save(&mut db, &store, "new", &initial, Some("key"), false).is_err());
    assert_eq!(
        db.query_row("SELECT count(*) FROM provider_accounts", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    store.fail_store(false);
    save(&mut db, &store, "new", &initial, Some("key"), false).unwrap();
    let mut updated = initial.clone();
    updated.models[0].name = "Modified".into();
    store.fail_store(true);
    assert!(save(&mut db, &store, "new", &updated, Some("rotated"), true).is_err());
    let raw: String = db
        .query_row("SELECT config FROM custom_provider_configs", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(serde_json::from_str::<Config>(&raw).unwrap(), initial);
}

#[test]
fn custom_inference_uses_offline_catalog_and_never_refreshes_api_keys() {
    use std::sync::Arc;
    let home = tempfile::tempdir().unwrap();
    let state = AppState::default();
    let store = Arc::new(InMemorySecretStore::default());
    let initial = config(Protocol::OpenaiCompletions);
    let alias = "Gateway";
    state
        .with_connection(home.path(), |db| {
            save(db, store.as_ref(), alias, &initial, Some("key"), false)
        })
        .unwrap();
    // An API key has no OAuth expiry semantics, even if a legacy timestamp is zero.
    let mut secret = store.load(alias).unwrap();
    secret.expires = 0;
    store.store(alias, &secret).unwrap();
    let oauth = OpenAiCodexState {
        manager: Arc::new(super::super::OAuthManager::production(store.clone())),
    };
    let (secret, model) = oauth
        .inference_model(&state, home.path(), alias, &initial.models[0].id, None)
        .unwrap();
    assert_eq!(secret.access, "key");
    assert_eq!(secret.custom, Some(initial));
    assert_eq!(model.context_window, Some(64000));
    assert!(oauth
        .inference_model(&state, home.path(), alias, &model.id, Some("unsupported"))
        .is_err());
    store.fail_store(true);
    assert!(
        oauth
            .manager
            .list_accounts(&state, home.path(), None)
            .unwrap()[0]
            .models_available
    );
    state
        .with_connection(home.path(), |db| {
            db.execute("UPDATE provider_accounts SET enabled = 0", [])
                .map_err(persistence::PersistenceError::from)
        })
        .unwrap();
    assert_eq!(
        oauth
            .inference_model(&state, home.path(), alias, &model.id, None)
            .err()
            .unwrap()
            .code,
        "account_disabled"
    );
}
