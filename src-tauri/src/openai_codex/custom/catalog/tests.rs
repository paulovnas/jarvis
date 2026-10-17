use super::*;
use crate::openai_codex::{InMemorySecretStore, SecretStore};
use serde_json::json;
use std::io::{Read, Write};

fn target(protocol: Protocol, auth_mode: AuthMode, base_url: &str) -> Target {
    Target {
        protocol,
        auth_mode,
        base_url: base_url.into(),
    }
}

#[test]
fn catalog_routes_match_inference_base_without_duplicate_versions_or_endpoint_suffixes() {
    for (protocol, suffix) in [
        (Protocol::OpenaiCompletions, "chat/completions"),
        (Protocol::OpenaiResponses, "responses"),
        (Protocol::AnthropicMessages, "messages"),
    ] {
        for (base, expected) in [
            (
                "https://gateway.example".to_owned(),
                "https://gateway.example/v1/models",
            ),
            (
                "https://gateway.example/v1/".to_owned(),
                "https://gateway.example/v1/models",
            ),
            (
                format!("https://gateway.example/api/v1/{suffix}"),
                "https://gateway.example/api/v1/models",
            ),
            (
                "http://127.0.0.1:8080/custom/v2".to_owned(),
                "http://127.0.0.1:8080/custom/v2/models",
            ),
        ] {
            assert_eq!(
                target(protocol, AuthMode::Bearer, &base)
                    .url()
                    .unwrap()
                    .as_str(),
                expected
            );
        }
    }
    for base in [
        "http://remote.example/v1",
        "https://user:secret@example.com/v1",
        "https://example.com/v1?key=secret",
        "https://example.com/v1#secret",
    ] {
        assert!(target(Protocol::OpenaiCompletions, AuthMode::Bearer, base)
            .url()
            .is_err());
    }
}

#[test]
fn standard_catalog_preserves_case_and_spaces_deduplicates_exact_ids_and_leaves_unknown_metadata_absent(
) {
    let result = parse(&json!({"object":"list","data":[{"id":"Opus 5.5"},{"id":"opus 5.5"},{"id":"GPT-5.6"},{"id":"Opus 5.5"}]})).unwrap();
    assert_eq!(
        result.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(),
        ["Opus 5.5", "opus 5.5", "GPT-5.6"]
    );
    assert!(result.iter().all(|m| m.context_window.is_none()
        && m.max_output_tokens.is_none()
        && m.supports_images.is_none()
        && m.supports_tools.is_none()
        && m.reasoning_levels.is_empty()
        && m.default_reasoning_level.is_none()));
    assert_eq!(result[0].name, "Opus 5.5");
    assert!(parse(&json!({"data":[{"id":"bad\nmodel"}]})).is_err());
    assert!(parse(&json!({"models":[]})).is_err());
    let result = parse(&json!({"data":[{"id":"Known", "context_length":64000,"top_provider":{"context_length":32000,"max_completion_tokens":2000},"supported_parameters":["tools"],"architecture":{"input_modalities":["text","image"]}}]})).unwrap();
    assert_eq!(result[0].context_window, Some(32000.0));
    assert_eq!(result[0].max_output_tokens, Some(2000.0));
    assert_eq!(result[0].supports_tools, Some(true));
    assert_eq!(result[0].supports_images, Some(true));
}

#[test]
fn documented_catalog_metadata_keeps_independent_limits_vision_and_effort_labels() {
    let result = parse(&json!({"data":[{
        "id":"minimax-m3", "display_name":"MiniMax M3",
        "context_window":512000, "max_output_tokens":512000,
        "capabilities":{"vision":true},
        "reasoning":{"effort_levels":[{"value":"low","display":"Low"},{"value":"medium","display":"Medium"},{"value":"high","display":"High"}],"default_effort_level":"medium"}
    }]})).unwrap();
    let model = &result[0];
    assert_eq!(model.name, "MiniMax M3");
    assert_eq!(model.context_window, Some(512000.0));
    assert_eq!(model.max_output_tokens, Some(512000.0));
    assert_eq!(model.supports_images, Some(true));
    assert_eq!(model.supports_tools, None);
    assert_eq!(
        serde_json::to_value(model).unwrap()["reasoningLevels"],
        json!([{"value":"low","display":"Low"},{"value":"medium","display":"Medium"},{"value":"high","display":"High"}])
    );
    assert_eq!(model.default_reasoning_level.as_deref(), Some("medium"));
}

#[test]
fn invalid_numeric_limits_stay_visible_for_correction_instead_of_hiding_the_catalog() {
    let result = parse(&json!({"data":[
        {"id":"too-small","context_window":1024,"max_output_tokens":0},
        {"id":"too-large","context_window":100000001,"max_output_tokens":10000001},
        {"id":"not-integers","context_window":-1,"max_output_tokens":1.5}
    ]}))
    .unwrap();
    assert_eq!(result[0].context_window, Some(1024.0));
    assert_eq!(result[0].max_output_tokens, Some(0.0));
    assert_eq!(result[1].context_window, Some(100000001.0));
    assert_eq!(result[1].max_output_tokens, Some(10000001.0));
    assert_eq!(result[2].context_window, Some(-1.0));
    assert_eq!(result[2].max_output_tokens, Some(1.5));
}

#[test]
fn reasoning_metadata_retains_only_advertised_valid_unique_efforts_and_never_invents_a_default() {
    let result = parse(&json!({"data":[
        {"id":"efforts","reasoning":{"effort_levels":[{"value":"high","display":"High"},{"value":"high","display":"Duplicate"},{"value":"xhigh"},{"value":"Bad Value"},{}],"default_effort_level":"unknown"}},
        {"id":"no-reasoning","capabilities":{"vision":false}}
    ]})).unwrap();
    assert_eq!(
        serde_json::to_value(&result[0]).unwrap()["reasoningLevels"],
        json!([{"value":"high","display":"High"},{"value":"xhigh","display":"xhigh"}])
    );
    assert_eq!(result[0].default_reasoning_level, None);
    assert!(result[1].reasoning_levels.is_empty());
    assert_eq!(result[1].supports_images, Some(false));
}

#[test]
fn unicode_catalog_names_follow_the_same_length_contract_as_saved_models() {
    for name in ["模型测试-".repeat(30), "🧪".repeat(100)] {
        let result = parse(&json!({"data":[{"id":"Opus 5.5","display_name":name}]})).unwrap();
        let mut config = super::super::tests::config(Protocol::OpenaiCompletions);
        config.models[0].name = result[0].name.clone();
        assert!(config.models[0].name.encode_utf16().count() <= 120);
        config.validate().unwrap();
    }
    let id = "模型测试-".repeat(10);
    assert_eq!(parse(&json!({"data":[{"id":id}]})).unwrap()[0].name, id);
}

#[test]
fn saved_credentials_are_only_reused_for_the_saved_catalog_url() {
    let home = tempfile::tempdir().unwrap();
    let state = AppState::default();
    let store = InMemorySecretStore::default();
    let config = super::super::tests::config(Protocol::OpenaiCompletions);
    state
        .with_connection(home.path(), |db| {
            super::super::save(
                db,
                &store,
                "gateway",
                &config,
                Some("saved-key"),
                false,
                None,
            )
        })
        .unwrap();
    let initial = target(config.protocol, config.auth_mode, &config.base_url);
    assert_eq!(
        credential(
            &initial,
            None,
            Some("gateway".into()),
            &state,
            home.path(),
            &store
        )
        .unwrap(),
        "saved-key"
    );
    let other = target(
        config.protocol,
        config.auth_mode,
        "https://other.example/v1",
    );
    assert!(credential(
        &other,
        None,
        Some("gateway".into()),
        &state,
        home.path(),
        &store
    )
    .is_err());
    assert_eq!(
        credential(
            &other,
            Some("new-key".into()),
            Some("gateway".into()),
            &state,
            home.path(),
            &store
        )
        .unwrap(),
        "new-key"
    );
    assert_eq!(store.load("gateway").unwrap().access, "saved-key");
    assert!(credential(
        &other,
        Some("bad\nkey".into()),
        None,
        &state,
        home.path(),
        &store
    )
    .is_err());
}

fn server(replies: Vec<(u16, String, Value)>) -> (String, std::thread::JoinHandle<Vec<String>>) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let address = format!("http://{}", listener.local_addr().unwrap());
    let worker = std::thread::spawn(move || {
        let mut requests = vec![];
        for (status, headers, body) in replies {
            let started = std::time::Instant::now();
            let mut socket = loop {
                match listener.accept() {
                    Ok((socket, _)) => break socket,
                    Err(e)
                        if e.kind() == std::io::ErrorKind::WouldBlock
                            && started.elapsed() < Duration::from_secs(10) =>
                    {
                        std::thread::sleep(Duration::from_millis(5))
                    }
                    Err(e) => panic!("fixture connection: {e}"),
                }
            };
            socket
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut bytes = vec![];
            let mut buffer = [0; 1024];
            while !bytes.windows(4).any(|part| part == b"\r\n\r\n") {
                let size = socket.read(&mut buffer).unwrap();
                assert_ne!(size, 0);
                bytes.extend_from_slice(&buffer[..size]);
            }
            requests.push(String::from_utf8(bytes).unwrap());
            let body = body.to_string();
            write!(socket, "HTTP/1.1 {status} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n{headers}\r\n{body}", body.len()).unwrap();
        }
        requests
    });
    (address, worker)
}

#[tokio::test]
async fn all_protocols_fetch_authenticated_paginated_catalogs_with_literal_ids() {
    for (protocol, auth) in [
        (Protocol::OpenaiCompletions, AuthMode::Bearer),
        (Protocol::OpenaiResponses, AuthMode::Bearer),
        (Protocol::AnthropicMessages, AuthMode::Bearer),
        (Protocol::AnthropicMessages, AuthMode::XApiKey),
    ] {
        let (base, server) = server(vec![
            (
                200,
                String::new(),
                json!({"data":[{"id":"Opus 5.5"}],"has_more":true,"last_id":"Opus 5.5"}),
            ),
            (
                200,
                String::new(),
                json!({"data":[{"id":"GPT-5.6"}],"has_more":false}),
            ),
        ]);
        let result = fetch(&target(protocol, auth, &base), "synthetic-secret")
            .await
            .unwrap();
        assert_eq!(result.models[0].id, "Opus 5.5");
        assert_eq!(result.models[1].id, "GPT-5.6");
        let requests = server.join().unwrap();
        assert!(requests[0].starts_with("GET /v1/models HTTP/1.1"));
        assert!(requests[1].starts_with("GET /v1/models?after_id=Opus+5.5 HTTP/1.1"));
        for request in requests {
            let request = request.to_lowercase();
            assert!(request.contains(if auth == AuthMode::Bearer {
                "authorization: bearer synthetic-secret"
            } else {
                "x-api-key: synthetic-secret"
            }));
            if protocol == Protocol::AnthropicMessages {
                assert!(request.contains("anthropic-version: 2023-06-01"));
            }
        }
    }
}

#[tokio::test]
async fn http_failures_and_redirects_are_sanitized_and_never_followed() {
    for status in [302, 401, 403, 404, 429, 500] {
        let (base, server) = server(vec![(
            status,
            "Location: https://other.example/models\r\n".into(),
            json!({"error":"synthetic-secret"}),
        )]);
        let error = fetch(
            &target(Protocol::OpenaiResponses, AuthMode::Bearer, &base),
            "synthetic-secret",
        )
        .await
        .unwrap_err();
        let serialized = serde_json::to_string(&error).unwrap();
        assert!(!serialized.contains("synthetic-secret"));
        assert_eq!(server.join().unwrap().len(), 1);
    }
}

#[tokio::test]
#[ignore = "Uses an explicitly supplied custom gateway test key for catalog reads only"]
async fn live_custom_catalog() {
    let key = std::env::var("JARVIS_CUSTOM_TEST_KEY").expect("Provide an explicit test key");
    let base = std::env::var("JARVIS_CUSTOM_TEST_URL").expect("Provide the test gateway URL");
    for protocol in [
        Protocol::OpenaiCompletions,
        Protocol::OpenaiResponses,
        Protocol::AnthropicMessages,
    ] {
        let result = fetch(&target(protocol, AuthMode::Bearer, &base), &key)
            .await
            .unwrap();
        assert!(!result.models.is_empty());
        println!(
            "{protocol:?}: {} catalog IDs read without inference",
            result.models.len()
        );
    }
}
