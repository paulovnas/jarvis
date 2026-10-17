use super::*;
use crate::openai_codex::{InMemorySecretStore, OAuthManager};
use serde_json::json;
use std::sync::Arc;

fn published_catalog() -> Value {
    json!({"object":"list","data":[
        {"id":"minimax-m3","display_name":"MiniMax M3","context_window":512000,"max_output_tokens":512000,
         "capabilities":{"vision":true},"reasoning":{"effort_levels":[{"value":"low","display":"Low"},{"value":"medium","display":"Medium"},{"value":"high","display":"High"}],"default_effort_level":"medium"}},
        {"id":"prism","display_name":"Prism","context_window":1000000,"max_output_tokens":16000,"capabilities":{"vision":true}}
    ]})
}

#[test]
fn published_catalog_preserves_independent_limits_vision_and_exact_reasoning() {
    let config = catalog(&published_catalog()).unwrap();
    assert_eq!(config.base_url, BASE_URL);
    assert_eq!(config.protocol, Protocol::OpenaiCompletions);
    assert_eq!(config.token_field, TokenField::MaxTokens);
    let model = &config.models[0];
    assert_eq!(model.id, "minimax-m3");
    assert_eq!(model.name, "MiniMax M3");
    assert_eq!(model.context_window, 512000);
    assert_eq!(model.max_output_tokens, 512000);
    assert!(model.supports_images && model.supports_tools);
    assert_eq!(model.reasoning, Reasoning::Effort);
    assert_eq!(model.reasoning_levels, ["low", "medium", "high"]);
    assert_eq!(model.default_reasoning_level.as_deref(), Some("medium"));
    assert_eq!(config.models[1].reasoning, Reasoning::None);
    assert!(config.models[1].reasoning_levels.is_empty());
}

#[test]
fn incomplete_or_invalid_native_limits_and_defaults_are_not_invented() {
    for value in [
        Value::Null,
        json!(0),
        json!(-1),
        json!(512000.5),
        json!(100000001),
    ] {
        let mut data = published_catalog();
        data["data"][0]["context_window"] = value;
        assert!(catalog(&data).is_err());
    }
    for value in [Value::Null, json!(0), json!(-1), json!(10000001)] {
        let mut data = published_catalog();
        data["data"][0]["max_output_tokens"] = value;
        assert!(catalog(&data).is_err());
    }
    let mut data = published_catalog();
    data["data"][0]["reasoning"]["default_effort_level"] = json!("ultra");
    assert!(catalog(&data).is_err());
    assert!(catalog(&json!({"data":[]})).is_err());
}

#[test]
fn credits_never_imply_subscription_quota_or_convert_usd_to_hypercredits() {
    assert_eq!(credit_balance(&json!({"balance":173})).unwrap(), 173.0);
    assert_eq!(credit_balance(&json!({"balance":0})).unwrap(), 0.0);
    for data in [
        json!({"balance":-1}),
        json!({"balance":"173"}),
        json!({"balance_usd":1.73}),
        json!({}),
    ] {
        assert!(credit_balance(&data).is_err());
    }
}

#[test]
fn native_accounts_keep_their_kind_and_use_offline_models_without_oauth() {
    let home = tempfile::tempdir().unwrap();
    let state = AppState::default();
    let store = Arc::new(InMemorySecretStore::default());
    let alias = "charm-hyper-personal";
    let config = catalog(&published_catalog()).unwrap();
    state
        .with_connection(home.path(), |db| {
            custom::save_provider(
                db,
                store.as_ref(),
                alias,
                &config,
                Some("fixture-key"),
                false,
                Some("My Hyper"),
                KIND,
            )
        })
        .unwrap();
    let mut secret = store.load(alias).unwrap();
    assert!(secret.account_id.starts_with("charm-hyper:"));
    secret.expires = 0;
    store.store(alias, &secret).unwrap();
    let oauth = OpenAiCodexState {
        manager: Arc::new(OAuthManager::production(store.clone())),
    };
    assert!(
        matches!(oauth.manager.begin(&state, home.path(), "charm-hyper-other"), Err(error) if error.code == "invalid_alias")
    );
    let accounts = oauth
        .manager
        .list_accounts(&state, home.path(), None)
        .unwrap();
    assert_eq!(accounts[0].provider_kind, KIND);
    assert_eq!(accounts[0].display_name.as_deref(), Some("My Hyper"));
    assert!(accounts[0].show_usage && accounts[0].models_available);
    assert!(accounts[0].custom.is_none());
    assert_eq!(accounts[0].vision_models, ["minimax-m3", "prism"]);
    let (credential, model) = oauth
        .inference_model(&state, home.path(), alias, "minimax-m3", Some("high"))
        .unwrap();
    assert_eq!(credential.access, "fixture-key");
    assert_eq!(credential.custom, Some(config.clone()));
    assert!(!credential.needs_refresh().unwrap());
    assert_eq!(model.reasoning_levels, ["low", "medium", "high"]);
    assert!(oauth
        .inference_model(&state, home.path(), alias, "minimax-m3", Some("ultra"))
        .is_err());
    super::super::update_provider_model_enabled(&state, home.path(), alias, "prism", false)
        .unwrap();
    assert!(oauth
        .inference_model(&state, home.path(), alias, "prism", None)
        .is_err());
    // A custom provider with a native-looking alias cannot be edited into a native account.
    assert!(state
        .with_connection(home.path(), |db| custom::save_provider(
            db,
            store.as_ref(),
            alias,
            &config,
            None,
            true,
            None,
            "custom"
        ))
        .is_err());
    let mut changed = config;
    changed.base_url = "https://other.example/v1".into();
    state
        .with_connection(home.path(), |db| {
            db.execute(
                "UPDATE custom_provider_configs SET config=?1 WHERE alias=?2",
                rusqlite::params![serde_json::to_string(&changed).unwrap(), alias],
            )
            .unwrap();
            Ok::<_, ProviderError>(())
        })
        .unwrap();
    assert!(oauth
        .inference_model(&state, home.path(), alias, "minimax-m3", None)
        .is_err());
}

#[test]
fn secret_store_failure_does_not_create_or_replace_native_accounts() {
    let home = tempfile::tempdir().unwrap();
    let state = AppState::default();
    let store = InMemorySecretStore::default();
    let alias = "charm-hyper-personal";
    let config = catalog(&published_catalog()).unwrap();
    store.fail_store(true);
    assert!(state
        .with_connection(home.path(), |db| custom::save_provider(
            db,
            &store,
            alias,
            &config,
            Some("fixture"),
            false,
            None,
            KIND
        ))
        .is_err());
    assert!(state
        .list_provider_accounts(home.path())
        .unwrap()
        .is_empty());
    store.fail_store(false);
    state
        .with_connection(home.path(), |db| {
            custom::save_provider(
                db,
                &store,
                alias,
                &config,
                Some("original"),
                false,
                None,
                KIND,
            )
        })
        .unwrap();
    store.fail_store(true);
    assert!(state
        .with_connection(home.path(), |db| custom::save_provider(
            db,
            &store,
            alias,
            &config,
            Some("replacement"),
            true,
            Some("Changed"),
            KIND
        ))
        .is_err());
    assert_eq!(store.load(alias).unwrap().access, "original");
    assert_ne!(
        state.list_provider_accounts(home.path()).unwrap()[0]
            .display_name
            .as_deref(),
        Some("Changed")
    );
}

#[test]
fn credits_http_authentication_and_errors_do_not_expose_credentials() {
    use std::{
        io::{Read, Write},
        net::TcpListener,
    };
    for (status, body) in [
        (200, r#"{"balance":173}"#),
        (401, r#"{"error":{"message":"private-token"}}"#),
    ] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut buffer = [0; 4096];
            let length = socket.read(&mut buffer).unwrap();
            let request = String::from_utf8_lossy(&buffer[..length]).to_ascii_lowercase();
            assert!(request.starts_with("get /credits "));
            assert!(request.contains("authorization: bearer private-token"));
            write!(
                socket,
                "HTTP/1.1 {status} Test\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
            .unwrap();
        });
        let result = credits(&client().unwrap(), &base, "private-token");
        if status == 200 {
            assert_eq!(result.unwrap(), 173.0);
        } else {
            let error = result.unwrap_err();
            assert_eq!(error.code, "unauthorized");
            assert!(!error.message.contains("private-token"));
        }
        server.join().unwrap();
    }
}
