use super::*;
use crate::openai_codex::InMemorySecretStore;
use crate::persistence;

#[test]
fn legacy_models_remain_chat_models_without_implicitly_enabling_generation() {
    let mut raw = serde_json::to_value(config(Protocol::OpenaiCompletions)).unwrap();
    let model = raw["models"][0].as_object_mut().unwrap();
    model.remove("supportsChat");
    model.remove("supportsImageGeneration");
    let restored: Config = serde_json::from_value(raw).unwrap();
    restored.validate().unwrap();
    assert!(restored.models[0].supports_chat);
    assert!(!restored.models[0].supports_image_generation);
}

#[test]
fn explicit_image_models_persist_separately_from_chat_and_refresh_on_edit() {
    let home = tempfile::tempdir().unwrap();
    let state = AppState::default();
    let store = InMemorySecretStore::default();
    let mut value = config(Protocol::OpenaiCompletions);
    value.base_url = "https://openrouter.ai/api/v1".into();
    value.models.push(Model {
        id: "bytedance-seed/seedream-5-0-flash".into(),
        name: "Meu Seedream".into(),
        context_window: 0,
        max_output_tokens: 0,
        supports_chat: false,
        supports_image_generation: true,
        supports_tools: false,
        ..value.models[0].clone()
    });
    value.validate().unwrap();
    assert_eq!(value.catalog().len(), 1);
    assert!(
        crate::openai_codex::selected_model(&value.catalog(), &value.models[1].id, None).is_err()
    );
    state
        .with_connection(home.path(), |db| {
            save(db, &store, "router", &value, Some("test-key"), false, None)
        })
        .unwrap();
    assert_eq!(load(&state, home.path(), "router").unwrap(), value);
    let accounts = crate::openai_codex::catalog_cache::list(&state, home.path()).unwrap();
    assert_eq!(accounts[0].models.len(), 1);
    assert_eq!(accounts[0].image_models.len(), 1);
    assert_eq!(accounts[0].image_models[0].id, value.models[1].id);
    assert_eq!(accounts[0].image_models[0].name, "Meu Seedream");
    assert_eq!(accounts[0].image_models[0].context_window, None);
    assert!(accounts[0].image_models[0].reasoning_levels.is_empty());
    value.models[1].supports_image_generation = false;
    state
        .with_connection(home.path(), |db| {
            save(db, &store, "router", &value, None, true, None)
        })
        .unwrap();
    let accounts = crate::openai_codex::catalog_cache::list(&state, home.path()).unwrap();
    assert!(accounts[0].image_models.is_empty());
    assert_eq!(accounts[0].models.len(), 1);
    assert_eq!(store.load("router").unwrap().access, "test-key");
}

#[test]
fn literal_model_ids_round_trip_for_all_protocols_without_sanitizing() {
    for protocol in [
        Protocol::OpenaiCompletions,
        Protocol::OpenaiResponses,
        Protocol::AnthropicMessages,
    ] {
        let mut value = config(protocol);
        value.models[0].id = "Opus 5.5".into();
        value.models.push(Model {
            id: "opus 5.5".into(),
            ..value.models[0].clone()
        });
        assert!(value.validate().is_ok());
        let decoded: Config =
            serde_json::from_value(serde_json::to_value(&value).unwrap()).unwrap();
        assert_eq!(decoded.catalog()[0].id, "Opus 5.5");
        for id in ["", "   ", "model\nname", "model\tname", "model\0name"] {
            value.models[0].id = id.into();
            assert!(value.validate().is_err());
        }
    }
}

#[test]
fn saved_bare_inference_roots_keep_their_original_routes_for_all_protocols() {
    for (protocol, route) in [
        (Protocol::OpenaiCompletions, "chat/completions"),
        (Protocol::OpenaiResponses, "responses"),
        (Protocol::AnthropicMessages, "messages"),
    ] {
        let mut value = config(protocol);
        value.base_url = "https://gateway.example/".into();
        assert_eq!(
            value.endpoint().unwrap().as_str(),
            format!("https://gateway.example/{route}")
        );
    }
}

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
            supports_chat: true,
            supports_image_generation: false,
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
        (64_000, 10_000_001),
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
fn independent_input_and_output_limits_are_preserved_for_all_protocols() {
    for protocol in [
        Protocol::OpenaiCompletions,
        Protocol::OpenaiResponses,
        Protocol::AnthropicMessages,
    ] {
        for output in [512_000, 600_000] {
            let mut value = config(protocol);
            value.models[0].context_window = 512_000;
            value.models[0].max_output_tokens = output;
            value.validate().unwrap();
            let decoded: Config =
                serde_json::from_value(serde_json::to_value(&value).unwrap()).unwrap();
            assert_eq!(decoded.models[0].max_output_tokens, output);
        }
    }
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
            save(
                db,
                &store,
                alias,
                &initial,
                Some("secret-key"),
                false,
                Some("Meu endpoint"),
            )
        })
        .unwrap();
    assert!(state
        .with_connection(home.path(), |db| save(
            db,
            &store,
            alias,
            &initial,
            Some("another"),
            false,
            None
        ))
        .is_err());
    let mut updated = initial.clone();
    updated.models[0].name = "Updated".into();
    state
        .with_connection(home.path(), |db| {
            save(db, &store, alias, &updated, None, true, None)
        })
        .unwrap();
    assert_eq!(store.load(alias).unwrap().access, "secret-key");
    assert_eq!(load(&state, home.path(), alias).unwrap(), updated);
    let record = state.list_provider_accounts(home.path()).unwrap().remove(0);
    assert_eq!(record.alias, alias);
    assert_eq!(record.display_name.as_deref(), Some("Meu endpoint"));
    let visible = serde_json::to_string(&account(&state, home.path(), record).unwrap()).unwrap();
    assert!(!visible.contains("secret-key"));
    assert!(!visible.contains("accountId"));
    assert!(visible.contains("Updated"));
    assert!(visible.contains("Meu endpoint"));
    state
        .with_connection(home.path(), |db| {
            save(db, &store, alias, &updated, Some("rotated-key"), true, None)
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
    assert!(save(&mut db, &store, "new", &initial, Some("key"), false, None).is_err());
    assert_eq!(
        db.query_row("SELECT count(*) FROM provider_accounts", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    store.fail_store(false);
    save(&mut db, &store, "new", &initial, Some("key"), false, None).unwrap();
    let mut updated = initial.clone();
    updated.models[0].name = "Modified".into();
    store.fail_store(true);
    assert!(save(
        &mut db,
        &store,
        "new",
        &updated,
        Some("rotated"),
        true,
        None
    )
    .is_err());
    let raw: String = db
        .query_row("SELECT config FROM custom_provider_configs", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(serde_json::from_str::<Config>(&raw).unwrap(), initial);
}

#[test]
fn inaccessible_wallet_preserves_custom_config_and_key_until_recovery() {
    let mut db = Connection::open_in_memory().unwrap();
    persistence::initialize_database(&mut db).unwrap();
    let store = InMemorySecretStore::default();
    let initial = config(Protocol::OpenaiCompletions);
    save(
        &mut db,
        &store,
        "gateway",
        &initial,
        Some("original"),
        false,
        None,
    )
    .unwrap();
    let mut updated = initial.clone();
    updated.models[0].name = "Updated".into();
    store.fail_load(true);
    for key in [None, Some("replacement")] {
        let error = save(&mut db, &store, "gateway", &updated, key, true, None).unwrap_err();
        assert_eq!(error.code, "secret_store");
        #[cfg(target_os = "linux")]
        assert!(error.message.contains("desbloqueie"));
    }
    let raw: String = db
        .query_row(
            "SELECT config FROM custom_provider_configs WHERE alias = 'gateway'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(serde_json::from_str::<Config>(&raw).unwrap(), initial);
    store.fail_load(false);
    assert_eq!(store.load("gateway").unwrap().access, "original");
    save(&mut db, &store, "gateway", &updated, None, true, None).unwrap();
}

#[test]
fn custom_inference_uses_offline_catalog_and_never_refreshes_api_keys() {
    use std::sync::Arc;
    let home = tempfile::tempdir().unwrap();
    let state = AppState::default();
    let store = Arc::new(InMemorySecretStore::default());
    let mut initial = config(Protocol::OpenaiCompletions);
    initial.models[0].id = "Opus 5.5".into();
    let alias = "Gateway";
    state
        .with_connection(home.path(), |db| {
            save(
                db,
                store.as_ref(),
                alias,
                &initial,
                Some("key"),
                false,
                None,
            )
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
    super::super::update_provider_model_enabled(&state, home.path(), alias, &model.id, false)
        .unwrap();
    assert_eq!(
        oauth
            .inference_model(&state, home.path(), alias, &model.id, None)
            .err()
            .expect("disabled model error")
            .code,
        "invalid_model"
    );
    super::super::update_provider_model_enabled(&state, home.path(), alias, &model.id, true)
        .unwrap();
    assert!(oauth
        .inference_model(&state, home.path(), alias, &model.id, None)
        .is_ok());
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
