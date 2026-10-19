use super::*;
use crate::openai_codex::{InMemorySecretStore, OAuthManager};
use std::sync::Arc;

const ALIAS: &str = "manual-router";

fn config() -> custom::Config {
    custom::Config {
        base_url: BASE_URL.into(),
        protocol: custom::Protocol::OpenaiCompletions,
        auth_mode: custom::AuthMode::Bearer,
        token_field: custom::TokenField::MaxTokens,
        replay_unsigned_thinking: false,
        models: vec![custom::Model {
            id: "vendor/manual-model".into(),
            name: "Meu modelo manual".into(),
            context_window: 128_000,
            max_output_tokens: 8192,
            supports_chat: true,
            supports_image_generation: false,
            supports_images: true,
            supports_tools: true,
            reasoning: custom::Reasoning::None,
            reasoning_levels: vec![],
            default_reasoning_level: None,
            thinking_budget: None,
        }],
    }
}

fn save(
    state: &AppState,
    home: &Path,
    store: &dyn SecretStore,
    config: &custom::Config,
    api_key: Option<&str>,
    editing: bool,
    balance_key: Option<Option<&str>>,
) {
    state
        .with_connection(home, |db| {
            custom::save_provider_with_balance(
                db,
                store,
                ALIAS,
                config,
                api_key,
                editing,
                None,
                "openrouter",
                balance_key,
            )
        })
        .unwrap();
}

#[test]
fn management_key_is_optional_preserved_on_edit_and_removed_without_changing_identity_or_models() {
    let home = tempfile::tempdir().unwrap();
    let state = AppState::default();
    let store = InMemorySecretStore::default();
    let mut config = config();
    save(
        &state,
        home.path(),
        &store,
        &config,
        Some("private-inference-key"),
        false,
        None,
    );
    let initial = store.load(ALIAS).unwrap();
    assert!(initial.openrouter_balance_key.is_none());

    save(
        &state,
        home.path(),
        &store,
        &config,
        None,
        true,
        Some(Some("private-management-key")),
    );
    config.models[0].name = "Nome atualizado".into();
    save(&state, home.path(), &store, &config, None, true, None);
    let retained = store.load(ALIAS).unwrap();
    assert_eq!(retained.access, "private-inference-key");
    assert_eq!(
        retained.openrouter_balance_key.as_deref(),
        Some("private-management-key")
    );
    assert_eq!(retained.account_id, initial.account_id);

    save(
        &state,
        home.path(),
        &store,
        &config,
        Some("private-inference-key"),
        true,
        None,
    );
    assert_eq!(
        store.load(ALIAS).unwrap().openrouter_balance_key.as_deref(),
        Some("private-management-key")
    );

    save(
        &state,
        home.path(),
        &store,
        &config,
        Some("rotated-inference-key"),
        true,
        None,
    );
    let rotated = store.load(ALIAS).unwrap();
    assert_eq!(rotated.access, "rotated-inference-key");
    assert!(rotated.openrouter_balance_key.is_none());
    save(
        &state,
        home.path(),
        &store,
        &config,
        None,
        true,
        Some(Some("new-management-key")),
    );
    save(&state, home.path(), &store, &config, None, true, Some(None));
    let removed = store.load(ALIAS).unwrap();
    assert!(removed.openrouter_balance_key.is_none());
    assert_eq!(removed.access, "rotated-inference-key");
    assert_eq!(removed.account_id, initial.account_id);
    assert_eq!(custom::load(&state, home.path(), ALIAS).unwrap(), config);
    let record = state.list_provider_accounts(home.path()).unwrap().remove(0);
    assert_eq!(record.provider_kind, "openrouter");
    assert_eq!(record.account_id, initial.account_id);
}

#[test]
fn inference_strips_management_key_without_removing_it_from_secure_storage_or_exposing_it_in_accounts(
) {
    let home = tempfile::tempdir().unwrap();
    let state = AppState::default();
    let store = Arc::new(InMemorySecretStore::default());
    let config = config();
    save(
        &state,
        home.path(),
        store.as_ref(),
        &config,
        Some("private-inference-key"),
        false,
        Some(Some("private-management-key")),
    );
    let oauth = OpenAiCodexState {
        manager: Arc::new(OAuthManager::production(store.clone())),
    };
    let (inference, models) = oauth
        .credential_and_models(&state, home.path(), ALIAS)
        .unwrap();
    assert_eq!(inference.access, "private-inference-key");
    assert!(inference.openrouter_balance_key.is_none());
    assert_eq!(inference.custom.as_ref(), Some(&config));
    assert_eq!(models.len(), 1);
    assert_eq!(models[0].id, "vendor/manual-model");
    assert_eq!(models[0].name, "Meu modelo manual");
    assert_eq!(
        store.load(ALIAS).unwrap().openrouter_balance_key.as_deref(),
        Some("private-management-key")
    );
    let record = state.list_provider_accounts(home.path()).unwrap().remove(0);
    let public = serde_json::to_string(&account(&state, home.path(), record).unwrap()).unwrap();
    assert!(public.contains("vendor/manual-model"));
    assert!(public.contains("Meu modelo manual"));
    assert!(!public.contains("private-inference-key"));
    assert!(!public.contains("private-management-key"));
    assert!(!public.contains("openrouterBalanceKey"));
}

#[test]
fn native_configuration_rejects_nonofficial_endpoints_and_unsupported_auth_or_protocols() {
    let mut value = config();
    validate_config(&value).unwrap();
    value.base_url = "https://openrouter.ai.evil.example/api/v1".into();
    assert!(validate_config(&value).is_err());
    value = config();
    value.protocol = custom::Protocol::OpenaiResponses;
    assert!(validate_config(&value).is_err());
    value = config();
    value.auth_mode = custom::AuthMode::XApiKey;
    assert!(validate_config(&value).is_err());
}

#[test]
fn budget_probes_keep_management_auth_separate_and_do_not_treat_a_key_cap_as_account_credit() {
    use std::{
        io::{BufRead, BufReader, Write},
        net::TcpListener,
        time::Duration,
    };

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let worker = std::thread::spawn(move || {
        for (path, key, status, body) in [
            (
                "key",
                "private-inference-key",
                200,
                r#"{"data":{"limit_remaining":20.0}}"#,
            ),
            (
                "credits",
                "private-management-key",
                200,
                r#"{"data":{"total_credits":100.5,"total_usage":25.75,"private":"private-management-key"}}"#,
            ),
            (
                "key",
                "private-inference-key",
                200,
                r#"{"data":{"limit_remaining":15.0}}"#,
            ),
            (
                "credits",
                "private-inference-key",
                403,
                r#"{"error":"private-management-key"}"#,
            ),
        ] {
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let mut lines = BufReader::new(&socket).lines();
            assert_eq!(
                lines.next().unwrap().unwrap(),
                format!("GET /{path} HTTP/1.1")
            );
            let headers: Vec<_> = lines
                .map_while(Result::ok)
                .take_while(|line| !line.is_empty())
                .map(|line| line.to_ascii_lowercase())
                .collect();
            assert!(headers.contains(&format!("authorization: bearer {key}")));
            write!(socket, "HTTP/1.1 {status} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
        }
    });
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(3))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    let mut credential = CodexCredential::new(
        "private-inference-key",
        "",
        i64::MAX,
        "openrouter:fixture",
        None,
        None,
    );
    credential.openrouter_balance_key = Some("private-management-key".into());
    let full = fetch(&client, &base, &credential).unwrap();
    assert_eq!(full.account_balance, Some(74.75));
    assert_eq!(full.key_limit_remaining, Some(20.0));
    assert_eq!(full.available(), Some(20.0));
    assert!(!serde_json::to_string(&full)
        .unwrap()
        .contains("private-management-key"));
    credential.openrouter_balance_key = None;
    let limited = fetch(&client, &base, &credential).unwrap();
    assert_eq!(limited.key_limit_remaining, Some(15.0));
    assert!(limited.account_balance.is_none());
    assert!(limited.available().is_none());
    assert!(limited.note.is_some());
    worker.join().unwrap();
}
