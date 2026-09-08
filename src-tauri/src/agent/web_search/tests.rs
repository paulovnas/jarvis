use super::*;
use crate::openai_codex::CodexCredential;
use std::io::{Read, Write};

fn options(account: &str, model: &str) -> TurnOptions {
    TurnOptions {
        account: account.into(),
        model: model.into(),
        reasoning: None,
        mode: super::super::Mode::Plan,
        workflow: None,
        custom_workflow_id: None,
        approval_mode: super::super::ApprovalMode::Yolo,
    }
}

#[test]
fn inheritance_uses_each_executing_agents_model_and_explicit_choices_stay_fixed() {
    let config = Config::default();
    let root = options("openai-codex-chat", "gpt-5.6-sol");
    let child = options("antigravity-designer", "gemini-3.8-flash");
    assert_eq!(config.resolve(&root).model.as_deref(), Some("gpt-5.6-sol"));
    assert_eq!(
        config.resolve(&child).account_alias.as_deref(),
        Some("antigravity-designer")
    );
    assert_eq!(
        config.resolve(&child).model.as_deref(),
        Some("gemini-3.8-flash")
    );
    let explicit = Config {
        inherit_chat: false,
        account_alias: Some("search".into()),
        model: Some("gpt-5.6-luna".into()),
    };
    assert_eq!(explicit.resolve(&root), explicit.resolve(&child));
    assert_eq!(
        Config {
            inherit_chat: false,
            account_alias: None,
            model: None
        }
        .resolve(&child)
        .account_alias,
        None
    );
    assert!(supports("antigravity", "gemini-3.8-flash"));
    assert!(!supports("antigravity", "claude-opus"));
}

fn database() -> Connection {
    let mut connection = Connection::open_in_memory().unwrap();
    crate::persistence::initialize_database(&mut connection).unwrap();
    for alias in ["openai-codex-chat", "openai-codex-search"] {
        connection.execute("INSERT INTO provider_accounts (alias, provider_kind, account_id) VALUES (?1, 'openai-codex', ?1)", [alias]).unwrap();
    }
    connection
}

#[test]
fn default_inherited_selection_and_disconnect_are_persistent() {
    let mut db = database();
    assert_eq!(read_config(&db).unwrap(), Config::default());
    save_config(
        &mut db,
        Some("openai-codex-search".into()),
        Some("gpt-5.6-luna".into()),
        false,
    )
    .unwrap();
    crate::persistence::initialize_database(&mut db).unwrap();
    assert_eq!(
        read_config(&db).unwrap().account_alias.as_deref(),
        Some("openai-codex-search")
    );
    db.execute(
        "DELETE FROM provider_accounts WHERE alias = 'openai-codex-chat'",
        [],
    )
    .unwrap();
    assert_eq!(
        read_config(&db).unwrap().account_alias.as_deref(),
        Some("openai-codex-search")
    );
    db.execute(
        "DELETE FROM provider_accounts WHERE alias = 'openai-codex-search'",
        [],
    )
    .unwrap();
    assert_eq!(
        read_config(&db).unwrap(),
        Config {
            inherit_chat: false,
            account_alias: Some("openai-codex-search".into()),
            model: Some("gpt-5.6-luna".into())
        }
    );
}

#[test]
fn invalid_accounts_do_not_replace_selection_and_off_is_explicit() {
    let mut db = database();
    save_config(
        &mut db,
        Some("openai-codex-search".into()),
        Some("gpt-5.6-luna".into()),
        false,
    )
    .unwrap();
    assert!(save_config(
        &mut db,
        Some("missing' OR 1 = 1 --".into()),
        Some("gpt-5.6-luna".into()),
        false
    )
    .is_err());
    assert_eq!(
        read_config(&db).unwrap().account_alias.as_deref(),
        Some("openai-codex-search")
    );
    save_config(&mut db, None, None, false).unwrap();
    assert_eq!(
        read_config(&db).unwrap(),
        Config {
            inherit_chat: false,
            account_alias: None,
            model: None
        }
    );
}

#[test]
fn migration_preserves_existing_accounts_and_defaults_search_to_inherit() {
    let mut db = Connection::open_in_memory().unwrap();
    for sql in [
        include_str!("../../../../drizzle/0000_heavy_tomas.sql"),
        include_str!("../../../../drizzle/0001_nervous_nighthawk.sql"),
        include_str!("../../../../drizzle/0002_silky_meltdown.sql"),
        include_str!("../../../../drizzle/0003_amazing_sasquatch.sql"),
    ] {
        db.execute_batch(sql).unwrap();
    }
    db.pragma_update(None, "user_version", 4).unwrap();
    db.execute("INSERT INTO provider_accounts (alias, provider_kind, account_id) VALUES ('openai-codex-existing', 'openai-codex', 'account')", []).unwrap();
    crate::persistence::initialize_database(&mut db).unwrap();
    assert_eq!(read_config(&db).unwrap(), Config::default());
    assert_eq!(
        db.query_row("SELECT count(*) FROM provider_accounts", [], |row| row
            .get::<_, i64>(0))
            .unwrap(),
        1
    );
}

#[test]
fn query_validation_rejects_empty_unbounded_and_routing_overrides() {
    for value in [
        json!({"query":" "}),
        json!({"query":"x".repeat(2001)}),
        json!({"query":"valid", "limit":0}),
        json!({"query":"valid", "limit":11}),
        json!({"query":"valid", "account":"openai-codex-chat"}),
    ] {
        assert!(arguments(&value).is_err());
    }
    assert_eq!(
        arguments(&json!({"query":"  Tauri docs  "})).unwrap().query,
        "Tauri docs"
    );
    assert!(!super::super::tools::needs_approval("web_search"));
}

#[tokio::test]
async fn disabled_search_never_resolves_credentials_or_uses_the_chat_account() {
    let home = std::env::temp_dir().join(format!(
        "jarvis-search-{}",
        crate::library::new_id().unwrap()
    ));
    let state = AppState::default();
    state
        .with_connection(&home, |db| save_config(db, None, None, false))
        .unwrap();
    let options = options("chat", "gpt-5.6-sol");
    let (_send, signal) = watch::channel(false);
    let result = execute(
        &AppState::default(),
        &OpenAiCodexState::default(),
        &home,
        &options,
        &json!({"query":"Tauri"}),
        signal,
    )
    .await;
    assert_eq!(result.unwrap_err().code, "web_search_disabled");
    state.close();
    std::fs::remove_dir_all(home).unwrap();
}

fn search_items() -> Vec<Value> {
    vec![
        json!({"type":"web_search_call", "status":"completed", "action":{"sources":[{"url":"https://v2.tauri.app/?utm_source=openai"}, {"url":"javascript:alert(1)"}, {"url":"https://user:secret@example.org/"}]}}),
        json!({"type":"message", "content":[{"type":"output_text", "text":"Fontes consultadas", "annotations":[{"type":"url_citation", "title":"Tauri", "url":"https://v2.tauri.app/"},{"type":"url_citation", "title":"Rust", "url":"https://www.rust-lang.org/"}]}]}),
    ]
}

#[test]
fn sources_are_merged_sanitized_bounded_and_include_the_search_account() {
    let response = provider::Response {
        output: search_items(),
        text: "á".repeat(13000),
        summary: "not exposed".into(),
        usage: None,
    };
    let result: Value = serde_json::from_str(
        &format_result(&response, "openai-codex-search", "gpt-5.6-luna", 1).unwrap(),
    )
    .unwrap();
    assert_eq!(result["accountAlias"], "openai-codex-search");
    assert_eq!(result["model"], "gpt-5.6-luna");
    assert_eq!(
        result["sources"],
        json!([{"title":"Tauri", "url":"https://v2.tauri.app/"}])
    );
    assert_eq!(result["answer"].as_str().unwrap().chars().count(), 12000);
    assert!(result.get("summary").is_none());
}

#[test]
fn plain_completions_failed_searches_and_missing_sources_are_not_search_results() {
    let mut response = provider::Response {
        output: vec![],
        text: "A plausible answer".into(),
        summary: String::new(),
        usage: None,
    };
    assert_eq!(
        format_result(&response, "a", "gpt-5.6-luna", 8)
            .unwrap_err()
            .code,
        "web_search_not_invoked"
    );
    response.output = vec![json!({"type":"web_search_call", "status":"failed"})];
    assert!(format_result(&response, "a", "gpt-5.6-luna", 8).is_err());
    response.output[0]["status"] = json!("completed");
    assert_eq!(
        format_result(&response, "a", "gpt-5.6-luna", 8)
            .unwrap_err()
            .code,
        "web_search_no_sources"
    );
}

#[test]
fn search_requests_the_configured_model_independently_of_the_chat() {
    let catalog = ["gpt-5.5", "gpt-5.6-luna"].map(|id| ProviderModel {
        id: id.into(),
        name: id.into(),
        context_window: None,
        reasoning_levels: vec![],
        default_reasoning_level: None,
    });
    require_search_model(&catalog, "gpt-5.5").unwrap();
    let body = search_body("Tauri docs", "gpt-5.5");
    assert_eq!(body["model"], "gpt-5.5");
    assert_eq!(body["input"].as_array().unwrap().len(), 1);
    assert_eq!(body["input"][0]["content"][0]["text"], "Tauri docs");
    assert_eq!(body["tool_choice"], json!({"type":"web_search"}));
    assert_eq!(body["store"], false);
}

#[test]
fn missing_selected_model_is_an_error_instead_of_silent_fallback() {
    let catalog = [ProviderModel {
        id: "gpt-5.5".into(),
        name: "GPT-5.5".into(),
        context_window: None,
        reasoning_levels: vec![],
        default_reasoning_level: None,
    }];
    let error = require_search_model(&catalog, "gpt-5.6-luna").unwrap_err();
    assert_eq!(error.code, "web_search_model");
    assert!(error.message.contains("modelo selecionado"));
    assert!(require_search_model(&[], "gpt-5.6-luna").is_err());
}

fn mock_request(
    body: &str,
    status: u16,
) -> (reqwest::RequestBuilder, std::thread::JoinHandle<String>) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let response = format!("HTTP/1.1 {status} Test\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        let mut bytes = vec![];
        let mut buffer = [0; 4096];
        loop {
            let count = stream.read(&mut buffer).unwrap();
            bytes.extend_from_slice(&buffer[..count]);
            let text = String::from_utf8_lossy(&bytes);
            if let Some(end) = text.find("\r\n\r\n") {
                let content_length = text[..end]
                    .lines()
                    .find_map(|line| {
                        line.to_lowercase()
                            .strip_prefix("content-length: ")
                            .map(|value| value.parse::<usize>().unwrap())
                    })
                    .unwrap_or(0);
                if bytes.len() >= end + 4 + content_length {
                    break;
                }
            }
            assert!(count > 0 && bytes.len() < 16000);
        }
        // Split frames and UTF-8 across chunks to exercise the actual transport.
        for chunk in response.as_bytes().chunks(47) {
            stream.write_all(chunk).unwrap();
        }
        String::from_utf8(bytes).unwrap()
    });
    let credential = CodexCredential::new(
        "search-fake-token",
        "fake-refresh",
        i64::MAX,
        "search-account-id",
        None,
        None,
    );
    let client = reqwest::Client::new();
    let mut request = provider::authenticated_request(
        &credential,
        "search-session",
        &search_body("Tauri docs", "gpt-5.6-luna"),
        TIMEOUT,
    )
    .unwrap()
    .build()
    .unwrap();
    *request.url_mut() = format!("http://{address}").parse().unwrap();
    (reqwest::RequestBuilder::from_parts(client, request), server)
}

#[tokio::test]
async fn hosted_search_sse_uses_its_own_auth_and_preserves_sources_with_lean_completion() {
    let mut events: Vec<_> = search_items().into_iter().enumerate().map(|(index, item)| json!({"type":"response.output_item.done", "output_index":index, "item":item})).collect();
    events
        .push(json!({"type":"response.completed", "response":{"status":"completed", "output":[]}}));
    let sse: String = events
        .iter()
        .map(|event| format!("data: {event}\n\n"))
        .collect();
    let (request, server) = mock_request(&sse, 200);
    let (_send, signal) = watch::channel(false);
    let response = provider::receive(request, signal, |_| Ok(()))
        .await
        .unwrap();
    let result = format_result(&response, "openai-codex-search", "gpt-5.6-luna", 8).unwrap();
    assert!(result.contains("https://v2.tauri.app/"));
    let wire = server.join().unwrap();
    assert!(wire.contains("authorization: Bearer search-fake-token\r\n"));
    assert!(wire.contains("chatgpt-account-id: search-account-id\r\n"));
    let request: Value = serde_json::from_str(wire.split_once("\r\n\r\n").unwrap().1).unwrap();
    assert_eq!(request["model"], "gpt-5.6-luna");
    assert!(!result.contains("search-fake-token"));
}

#[tokio::test]
async fn http_and_incomplete_stream_errors_never_expose_upstream_secrets() {
    for (status, sse) in [(429, "private backend error secret"), (200, "data: {\"type\":\"response.failed\",\"response\":{\"error\":{\"message\":\"secret\"}}}\n\n"), (200, "data: {\"type\":\"response.created\"}\n\n")] {
        let (request, server) = mock_request(sse, status);
        let (_send, signal) = watch::channel(false);
        let error = provider::receive(request, signal, |_| Ok(())).await.err().unwrap();
        assert!(!error.message.contains("secret"));
        server.join().unwrap();
    }
}

#[tokio::test]
async fn cancellation_interrupts_a_stalled_search_transport() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let request = reqwest::Client::new().get(format!("http://{}", listener.local_addr().unwrap()));
    let (send, signal) = watch::channel(false);
    let task = tokio::spawn(async move {
        provider::receive(request, signal, |_| Ok(()))
            .await
            .err()
            .unwrap()
            .code
    });
    tokio::time::sleep(Duration::from_millis(20)).await;
    send.send(true).unwrap();
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(1), task)
            .await
            .unwrap()
            .unwrap(),
        "cancelled"
    );
}
