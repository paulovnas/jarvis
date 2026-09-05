use super::*;
use crate::openai_codex::{
    commit_provider_account, disconnect_provider_account, InMemorySecretStore, SecretStore,
};

fn server(responses: Vec<Value>) -> (String, std::thread::JoinHandle<Vec<String>>) {
    use std::io::Write;
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = format!("http://{}", listener.local_addr().unwrap());
    let task = std::thread::spawn(move || {
        let mut requests = vec![];
        for value in responses {
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut bytes = vec![];
            let mut buffer = [0_u8; 1024];
            loop {
                let read = socket.read(&mut buffer).unwrap();
                if read == 0 {
                    break;
                }
                bytes.extend_from_slice(&buffer[..read]);
                if let Some(end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                    let header = String::from_utf8_lossy(&bytes[..end]);
                    let length = header
                        .lines()
                        .find_map(|line| {
                            line.to_lowercase()
                                .strip_prefix("content-length:")
                                .and_then(|n| n.trim().parse::<usize>().ok())
                        })
                        .unwrap_or(0);
                    if bytes.len() >= end + 4 + length {
                        break;
                    }
                }
            }
            requests.push(String::from_utf8(bytes).unwrap());
            let body = value.to_string();
            write!(socket,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",body.len(),body).unwrap();
        }
        requests
    });
    (address, task)
}

#[test]
fn google_login_uses_pkce_offline_consent_and_fixed_loopback_callback() {
    let url = url::Url::parse(
        &authorization_url(
            "http://127.0.0.1:51121/oauth-callback",
            "challenge",
            "state",
        )
        .unwrap(),
    )
    .unwrap();
    let query: std::collections::HashMap<_, _> = url.query_pairs().collect();
    assert_eq!(url.host_str(), Some("accounts.google.com"));
    assert_eq!(query["code_challenge_method"], "S256");
    assert_eq!(query["code_challenge"], "challenge");
    assert_eq!(query["state"], "state");
    assert_eq!(query["access_type"], "offline");
    assert_eq!(query["prompt"], "consent");
    assert_eq!(
        query["redirect_uri"],
        "http://127.0.0.1:51121/oauth-callback"
    );
    assert!(!query.contains_key("client_secret"));
}

#[test]
fn google_callback_rejects_wrong_state_and_accepts_its_own_route() {
    use std::io::Write;
    for (state, valid) in [("wrong", false), ("expected", true)] {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let task = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            crate::openai_codex::process_callback_route(&mut stream, "expected", "/oauth-callback")
        });
        let mut client = std::net::TcpStream::connect(address).unwrap();
        write!(
            client,
            "GET /oauth-callback?state={state}&code=test-code HTTP/1.1\r\nHost: localhost\r\n\r\n"
        )
        .unwrap();
        let mut response = String::new();
        client.read_to_string(&mut response).unwrap();
        let event = task.join().unwrap();
        if valid {
            assert!(
                matches!(event,crate::openai_codex::CallbackEvent::Code(code) if code=="test-code")
            );
        } else {
            assert!(matches!(
                event,
                crate::openai_codex::CallbackEvent::StateMismatch
            ));
        }
    }
}

#[test]
fn exchange_discovers_google_identity_and_project_then_commits_and_disconnects() {
    let project = json!({"currentTier":{"id":"free-tier"},"paidTier":{"id":"paid"},"cloudaicompanionProject":"project-123"});
    let (url, task) = server(vec![
        json!({"access_token":"test-access","refresh_token":"test-refresh","expires_in":3600}),
        json!({"id":"123","email":"USER@EXAMPLE.COM"}),
        project.clone(),
        project,
    ]);
    let credential = exchange_with(
        &build_codex_client().unwrap(),
        &format!("{url}/token"),
        &format!("{url}/userinfo"),
        &url,
        "test-code",
        "verifier",
        "http://127.0.0.1:51121/oauth-callback",
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(credential.account_id, "google:123");
    assert_eq!(credential.email.as_deref(), Some("user@example.com"));
    assert_eq!(credential.project_id.as_deref(), Some("project-123"));
    let requests = task.join().unwrap();
    assert!(requests[0].contains("code_verifier=verifier"));
    assert!(requests[0].contains("grant_type=authorization_code"));
    assert!(requests[2].contains("\"ideType\":\"ANTIGRAVITY\""));
    let mut connection = rusqlite::Connection::open_in_memory().unwrap();
    crate::persistence::initialize_database(&mut connection).unwrap();
    let secrets = InMemorySecretStore::default();
    let account =
        commit_provider_account(&connection, &secrets, "antigravity-pessoal", &credential).unwrap();
    assert_eq!(account.provider_kind, "antigravity");
    let ipc = serde_json::to_string(&account).unwrap();
    assert!(!ipc.contains("test-access"));
    assert!(!ipc.contains("project-123"));
    assert!(
        commit_provider_account(&connection, &secrets, "antigravity-outra", &credential).is_err()
    );
    disconnect_provider_account(&connection, &secrets, "antigravity-pessoal").unwrap();
    assert!(secrets.load("antigravity-pessoal").is_err());
}

#[test]
fn refresh_preserves_project_and_identity_without_exposing_runtime_catalog() {
    let mut old = CodexCredential::new(
        "old",
        "refresh",
        1,
        "google:1",
        Some("a@example.com".into()),
        None,
    );
    old.project_id = Some("project".into());
    old.antigravity_models
        .insert("model".into(), json!({"supportsThinking":true}));
    let next = apply_token(&json!({"access_token":"new","expires_in":3600}), Some(&old)).unwrap();
    assert_eq!(next.refresh, "refresh");
    assert_eq!(next.project_id, old.project_id);
    assert_eq!(next.account_id, old.account_id);
    assert!(!serde_json::to_string(&next)
        .unwrap()
        .contains("supportsThinking"));
    assert!(apply_token(&json!({"access_token":"x","expires_in":3600}), None).is_err());
    assert!(apply_token(
        &json!({"access_token":"x","refresh_token":"r","expires_in":-1}),
        None
    )
    .is_err());
}

#[test]
fn cancelled_login_never_contacts_the_issuer() {
    assert_eq!(
        exchange_with(
            &build_codex_client().unwrap(),
            "http://127.0.0.1:1",
            "http://127.0.0.1:1",
            "http://127.0.0.1:1",
            "code",
            "verifier",
            "redirect",
            &AtomicBool::new(true)
        )
        .err()
        .unwrap()
        .code,
        "cancelled"
    );
}

#[test]
fn ineligible_account_is_not_provisioned() {
    let (url, task) = server(vec![json!({"ineligibleTiers":[{"tierId":"free-tier"}]})]);
    assert_eq!(
        discover_project(
            &build_codex_client().unwrap(),
            &url,
            "test-token",
            &AtomicBool::new(false)
        )
        .unwrap_err()
        .code,
        "antigravity_ineligible"
    );
    assert_eq!(task.join().unwrap().len(), 1);
}

#[test]
fn catalog_filters_internal_models_and_offers_model_specific_reasoning() {
    let models=normalize_models(&json!({"models":{
        "internal":{"isInternal":true},"chat_20706":{},"gemini-2.5-pro":{},
        "gemini-3-pro":{"displayName":"Gemini Pro","supportsThinking":true,"maxTokens":1048576},
        "claude-sonnet":{"displayName":"Claude","supportsThinking":false},
        "claude-opus-thinking":{"displayName":"Claude Thinking","supportsThinking":true}
    }})).unwrap();
    assert_eq!(models.len(), 3);
    assert!(models[0].reasoning_levels.is_empty());
    assert_eq!(models[1].reasoning_levels, ["low", "medium", "high"]);
    assert_eq!(models[2].reasoning_levels, ["low", "high"]);
    assert_eq!(models[2].context_window, Some(1048576));
    assert_eq!(normalize_models(&json!({"models":{}})), Some(vec![]));
    assert!(normalize_models(&json!({"error":"no"})).is_none());
}
