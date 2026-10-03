use super::super::{InMemorySecretStore, OAuthManager};
use super::*;
use serde_json::json;
use std::sync::Arc;

fn raw_model() -> Value {
    json!({"name":"Fixture model","limit":{"context":200_000,"output":32_000},"modalities":{"input":["text","image"]},"tool_call":true,"reasoning_options":[]})
}
fn fixtures() -> Vec<Config> {
    [
        "qwen3.8-max",
        "minimax-m2.7",
        "gpt-6-luna",
        "deepseek-v4-pro",
    ]
    .into_iter()
    .map(|id| model_config_from_metadata(id, &raw_model()).unwrap())
    .collect()
}

#[test]
fn catalog_intersects_live_ids_with_confirmed_capabilities_and_overrides_stale_protocols() {
    let mut qwen = raw_model();
    qwen["reasoning_options"] = json!([{"type":"toggle"},{"type":"budget_tokens","max":262144}]);
    let mut gpt = raw_model();
    gpt["reasoning_options"] =
        json!([{"type":"effort","values":["none","low","high","max","ultra","fabricated"]}]);
    let metadata = json!({"opencode-go":{"models":{"qwen3.8-max":qwen,"gpt-6-luna":gpt}},"opencode":{"models":{"minimax-m2.7":raw_model()}}});
    let catalog = parse_catalog(&json!({"data":[{"id":"qwen3.8-max"},{"id":"gpt-6-luna"},{"id":"minimax-m2.7"},{"id":"qwen3.8-max"},{"id":"unknown"}]}), &metadata).unwrap();
    assert_eq!(catalog.len(), 3);
    assert_eq!(catalog[0].protocol, Protocol::AnthropicMessages);
    assert_eq!(catalog[0].auth_mode, AuthMode::XApiKey);
    assert_eq!(catalog[0].models[0].reasoning, Reasoning::Budget);
    assert_eq!(catalog[0].models[0].reasoning_levels, ["off", "on"]);
    assert_eq!(catalog[1].protocol, Protocol::OpenaiResponses);
    assert_eq!(
        catalog[1].models[0].reasoning_levels,
        ["none", "low", "high", "max"]
    );
    assert_eq!(catalog[2].models[0].reasoning, Reasoning::None);
    assert!(catalog[2].replay_unsigned_thinking);
    assert!(parse_catalog(&json!({"data":[{"id":"unknown"}]}), &metadata).is_err());
}

#[test]
fn incomplete_unsupported_or_invalid_metadata_never_invents_limits_or_capabilities() {
    for field in ["limit", "modalities", "tool_call", "name"] {
        let mut raw = raw_model();
        raw.as_object_mut().unwrap().remove(field);
        if field == "modalities" {
            assert!(
                !model_config_from_metadata("fixture", &raw).unwrap().models[0].supports_images
            );
        } else {
            assert!(
                model_config_from_metadata("fixture", &raw).is_none(),
                "{field}"
            );
        }
    }
    let mut raw = raw_model();
    raw["provider"]["npm"] = json!("@ai-sdk/google");
    assert!(model_config_from_metadata("fixture", &raw).is_none());
    raw["provider"]["npm"] = json!("@ai-sdk/openai-compatible");
    raw["limit"]["context"] = json!(3000);
    assert!(model_config_from_metadata("fixture", &raw).is_none());
}

#[test]
fn catalog_total_context_equal_to_output_keeps_model_with_bounded_output() {
    let mut raw = raw_model();
    raw["limit"]["output"] = raw["limit"]["context"].clone();
    let config = model_config_from_metadata("grok-4.7", &raw).unwrap();
    assert_eq!(config.models[0].context_window, 200_000);
    assert_eq!(config.models[0].max_output_tokens, 32_000);
    config.validate().unwrap();
}

#[test]
fn deepseek_go_contracts_keep_catalog_capabilities_and_requested_efforts() {
    let mut raw = raw_model();
    raw["limit"] = json!({"context":1_000_000,"output":384_000});
    raw["reasoning_options"] = json!([{"type":"effort","values":["low","high","max"]}]);
    for (id, protocol, token_field) in [
        (
            "deepseek-v4.1-flash",
            Protocol::OpenaiCompletions,
            TokenField::MaxCompletionTokens,
        ),
        (
            "deepseek-v4-flash-vision-exp",
            Protocol::OpenaiCompletions,
            TokenField::MaxCompletionTokens,
        ),
        (
            "deepseek-v4-pro",
            Protocol::OpenaiCompletions,
            TokenField::MaxTokens,
        ),
        (
            "deepseek-v4-flash",
            Protocol::OpenaiResponses,
            TokenField::MaxTokens,
        ),
    ] {
        let config = model_config_from_metadata(id, &raw).unwrap();
        assert_eq!(config.protocol, protocol, "{id}");
        assert_eq!(config.token_field, token_field, "{id}");
        assert_eq!(config.models[0].reasoning, Reasoning::Effort);
        assert_eq!(config.models[0].reasoning_levels, ["low", "high", "max"]);
        assert_eq!(
            config.models[0].default_reasoning_level.as_deref(),
            Some("high")
        );
        assert_eq!(config.models[0].context_window, 1_000_000);
        assert_eq!(config.models[0].max_output_tokens, 32_000);
        assert!(config.models[0].supports_tools && config.models[0].supports_images);
    }
}

#[test]
fn existing_go_accounts_get_corrected_contracts_without_reconnect_or_catalog_rewrite() {
    let home = tempfile::tempdir().unwrap();
    let state = AppState::default();
    let store = InMemorySecretStore::default();
    let alias = "opencode-go-work";
    let mut raw = raw_model();
    raw["reasoning_options"] = json!([{"type":"effort","values":["low","high","max"]}]);
    let mut legacy = model_config_from_metadata("deepseek-v4.1-flash", &raw).unwrap();
    legacy.models[0].max_output_tokens = 384_000;
    legacy.models[0].context_window = 1_000_000;
    legacy.models[0].reasoning = Reasoning::Deepseek;
    legacy.token_field = TokenField::MaxTokens;
    state
        .with_connection(home.path(), |db| {
            save(db, &store, alias, "synthetic-key", false, &[legacy.clone()])
        })
        .unwrap();
    let corrected = model_config(&state, home.path(), alias, "deepseek-v4.1-flash").unwrap();
    assert_eq!(corrected.token_field, TokenField::MaxCompletionTokens);
    assert_eq!(corrected.models[0].reasoning, Reasoning::Effort);
    assert_eq!(corrected.models[0].max_output_tokens, 32_000);
    assert_eq!(
        corrected.models[0].default_reasoning_level.as_deref(),
        Some("high")
    );
    assert_eq!(store.load(alias).unwrap().access, "synthetic-key");
    state
        .with_connection(home.path(), |db| {
            let stored: String = db
                .query_row(
                    "SELECT catalog FROM opencode_go_catalogs WHERE alias=?1",
                    [alias],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(
                serde_json::from_str::<Vec<Config>>(&stored).unwrap(),
                vec![legacy]
            );
            Ok::<_, ProviderError>(())
        })
        .unwrap();
}

#[test]
fn qwen_named_efforts_are_preserved_and_minimax_toggle_uses_its_own_messages_dialect() {
    let mut qwen = raw_model();
    qwen["reasoning_options"] = json!([{"type":"toggle"},{"type":"effort","values":["low","medium","xhigh"]},{"type":"budget_tokens","max":262144}]);
    let config = model_config_from_metadata("qwen3.8-max", &qwen).unwrap();
    assert_eq!(config.models[0].reasoning, Reasoning::EnabledEffort);
    assert_eq!(
        config.models[0].reasoning_levels,
        ["off", "low", "medium", "xhigh"]
    );
    assert!(config.models[0].thinking_budget.is_none());
    let mut minimax = raw_model();
    minimax["reasoning_options"] = json!([{"type":"toggle"}]);
    let config = model_config_from_metadata("minimax-m3", &minimax).unwrap();
    assert_eq!(config.models[0].reasoning, Reasoning::Toggle);
    assert_eq!(config.models[0].reasoning_levels, ["off", "on"]);
}

#[test]
fn key_rotation_keeps_identity_and_public_catalog_while_secrets_never_cross_ipc() {
    let home = tempfile::tempdir().unwrap();
    let state = AppState::default();
    let store = Arc::new(InMemorySecretStore::default());
    let alias = "opencode-go-work";
    state
        .with_connection(home.path(), |db| {
            save(
                db,
                store.as_ref(),
                alias,
                "synthetic-key",
                false,
                &fixtures(),
            )
        })
        .unwrap();
    let original = credential(&state, home.path(), store.as_ref(), alias).unwrap();
    let records = state.list_provider_accounts(home.path()).unwrap();
    let account = account(&state, home.path(), records[0].clone()).unwrap();
    assert_eq!(account.provider_kind, "opencode-go");
    assert!(account.show_usage);
    assert!(account.custom.is_none());
    assert!(account.vision_models.contains(&"qwen3.8-max".into()));
    assert!(!serde_json::to_string(&account)
        .unwrap()
        .contains("synthetic-key"));
    state
        .with_connection(home.path(), |db| {
            save(db, store.as_ref(), alias, "rotated-key", true, &fixtures())
        })
        .unwrap();
    let rotated = credential(&state, home.path(), store.as_ref(), alias).unwrap();
    assert_eq!(rotated.account_id, original.account_id);
    assert_eq!(rotated.access, "rotated-key");
    let oauth = OpenAiCodexState {
        manager: Arc::new(OAuthManager::production(store)),
    };
    for (model, expected) in [
        ("qwen3.8-max", Protocol::AnthropicMessages),
        ("gpt-6-luna", Protocol::OpenaiResponses),
        ("deepseek-v4-pro", Protocol::OpenaiCompletions),
    ] {
        let chosen = oauth
            .inference_credential(&state, home.path(), alias, model, None)
            .unwrap();
        assert_eq!(chosen.custom.unwrap().protocol, expected);
    }
    let list = oauth
        .manager
        .list_accounts(&state, home.path(), None)
        .unwrap();
    assert_eq!(list[0].models.len(), 4);
    assert!(oauth
        .inference_credential(&state, home.path(), alias, "unknown", None)
        .is_err());
}

#[test]
fn invalid_alias_disabled_account_and_changed_identity_are_rejected() {
    for alias in [
        "",
        "opencode-go-",
        "antigravity-work",
        "opencode-go-_work",
        "opencode-go-a/b",
    ] {
        assert!(validate_alias(alias).is_err(), "{alias}");
    }
    let home = tempfile::tempdir().unwrap();
    let state = AppState::default();
    let store = InMemorySecretStore::default();
    let alias = "opencode-go-work";
    state
        .with_connection(home.path(), |db| {
            save(db, &store, alias, "synthetic", false, &fixtures())
        })
        .unwrap();
    state
        .with_connection(home.path(), |db| {
            db.execute(
                "UPDATE provider_accounts SET enabled=0 WHERE alias=?1",
                [alias],
            )
            .map_err(crate::persistence::PersistenceError::from)
        })
        .unwrap();
    assert!(credential(&state, home.path(), &store, alias).is_err());
    state.with_connection(home.path(), |db| {
        db.execute("UPDATE provider_accounts SET enabled=1,account_id='opencode-go:other' WHERE alias=?1",[alias]).map_err(crate::persistence::PersistenceError::from)
    }).unwrap();
    assert!(credential(&state, home.path(), &store, alias).is_err());
}

#[test]
fn go_catalog_survives_restart_and_disconnect_cascades_public_metadata_only() {
    let home = tempfile::tempdir().unwrap();
    let store = InMemorySecretStore::default();
    let state = AppState::default();
    state
        .with_connection(home.path(), |db| {
            save(db, &store, "opencode-go-home", "secret", false, &fixtures())
        })
        .unwrap();
    drop(state);
    let reopened = AppState::default();
    assert_eq!(
        load(&reopened, home.path(), "opencode-go-home")
            .unwrap()
            .len(),
        4
    );
    reopened
        .with_connection(home.path(), |db| {
            db.execute(
                "DELETE FROM provider_accounts WHERE alias='opencode-go-home'",
                [],
            )
            .unwrap();
            assert_eq!(
                db.query_row("SELECT count(*) FROM opencode_go_catalogs", [], |r| r
                    .get::<_, i64>(0))
                    .unwrap(),
                0
            );
            Ok::<_, ProviderError>(())
        })
        .unwrap();
}

#[test]
fn secure_store_failure_rolls_back_new_account_and_existing_catalog_rotation() {
    let mut db = Connection::open_in_memory().unwrap();
    crate::persistence::initialize_database(&mut db).unwrap();
    let store = InMemorySecretStore::default();
    let alias = "opencode-go-work";
    let original = fixtures();
    store.fail_store(true);
    assert!(save(&mut db, &store, alias, "new-key", false, &original).is_err());
    for table in ["provider_accounts", "opencode_go_catalogs"] {
        assert_eq!(
            db.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
    store.fail_store(false);
    save(&mut db, &store, alias, "old-key", false, &original).unwrap();
    let mut modified = original.clone();
    modified[0].models[0].name = "Changed".into();
    store.fail_store(true);
    assert!(save(&mut db, &store, alias, "rotated", true, &modified).is_err());
    assert_eq!(store.load(alias).unwrap().access, "old-key");
    let raw: String = db
        .query_row("SELECT catalog FROM opencode_go_catalogs", [], |r| r.get(0))
        .unwrap();
    assert_eq!(serde_json::from_str::<Vec<Config>>(&raw).unwrap(), original);
}

#[test]
#[ignore = "Read-only public Go and models.dev catalogs; no credentials or inference"]
fn live_public_go_catalog_has_only_supported_per_model_protocols() {
    let catalog = discover().unwrap();
    assert!(!catalog.is_empty());
    assert!(catalog
        .iter()
        .all(|config| config.validate().is_ok() && config.base_url == BASE_URL));
    assert!(catalog
        .iter()
        .any(|config| config.protocol == Protocol::AnthropicMessages));
    assert!(catalog
        .iter()
        .any(|config| config.protocol == Protocol::OpenaiResponses));
    assert!(catalog
        .iter()
        .any(|config| config.protocol == Protocol::OpenaiCompletions));
}
