use super::*;
use serde_json::json;
use std::io::{Read, Write};

fn mock_request(
    body: &str,
    status: u16,
    length: Option<usize>,
) -> (RequestBuilder, std::thread::JoinHandle<String>) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let redirect = if status == 302 {
        "Location: http://127.0.0.1:0/\r\n"
    } else {
        ""
    };
    let response = format!("HTTP/1.1 {status} Test\r\nContent-Type: application/json\r\n{redirect}Content-Length: {}\r\nConnection: close\r\n\r\n{body}", length.unwrap_or(body.len()));
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(3)))
            .unwrap();
        let mut bytes = vec![];
        loop {
            let mut buffer = [0; 2048];
            let count = stream.read(&mut buffer).unwrap();
            assert!(count > 0 && bytes.len() < 16000);
            bytes.extend_from_slice(&buffer[..count]);
            if bytes.windows(4).any(|part| part == b"\r\n\r\n") {
                break;
            }
        }
        let _ = stream.write_all(response.as_bytes());
        String::from_utf8(bytes).unwrap()
    });
    let (client, built) = request("brave-private-test-key", "Tauri & Rust documentação", 3)
        .unwrap()
        .build_split();
    let mut built = built.unwrap();
    *built.url_mut() = format!(
        "http://{address}{}?{}",
        built.url().path(),
        built.url().query().unwrap()
    )
    .parse()
    .unwrap();
    (RequestBuilder::from_parts(client, built), server)
}

#[test]
fn requests_encode_the_query_use_the_subscription_header_and_enforce_brave_limits() {
    let built = request("test-key", "Tauri & Rust documentação", 3)
        .unwrap()
        .build()
        .unwrap();
    assert_eq!(built.method(), reqwest::Method::GET);
    assert_eq!(
        built.url().origin().ascii_serialization(),
        "https://api.search.brave.com"
    );
    assert_eq!(built.url().path(), "/res/v1/web/search");
    let query: std::collections::HashMap<_, _> = built.url().query_pairs().collect();
    assert_eq!(query["q"], "Tauri & Rust documentação");
    assert_eq!(query["count"], "3");
    assert_eq!(query["result_filter"], "web");
    assert_eq!(query["text_decorations"], "false");
    assert_eq!(built.headers()["X-Subscription-Token"], "test-key");
    assert!(built.headers()["X-Subscription-Token"].is_sensitive());
    assert!(!built.url().as_str().contains("test-key"));
    assert!(request("key", &"á".repeat(600), 10).is_ok());
    for query in ["".to_owned(), "x".repeat(601), vec!["word"; 76].join(" ")] {
        assert!(request("key", &query, 3).is_err());
    }
    assert!(request("key", "valid", 0).is_err());
    assert!(request("bad\nheader", "valid", 1).is_err());
}

#[test]
fn search_results_preserve_snippets_filter_unsafe_urls_and_do_not_invent_a_model() {
    let data = json!({"type":"search", "web":{"results":[
        {"title":"Unsafe", "url":"javascript:alert(1)", "description":"Skip"},
        {"title":"Credential URL", "url":"https://secret@example.test/"},
        {"title":"Tauri", "url":"https://tauri.app/", "description":"Official documentation"},
        {"title":"Duplicate", "url":"https://tauri.app/", "description":"Repeated"},
        {"title":"Rust", "url":"https://www.rust-lang.org/", "description":"Rust language"}
    ]}});
    let result: Value =
        serde_json::from_str(&format_result(&data, "brave-search-work", 1).unwrap()).unwrap();
    assert_eq!(result["accountAlias"], "brave-search-work");
    assert_eq!(result["provider"], "Brave Search");
    assert!(result["model"].is_null());
    assert_eq!(result["sources"].as_array().unwrap().len(), 1);
    assert_eq!(result["sources"][0]["url"], "https://tauri.app/");
    assert!(result["answer"]
        .as_str()
        .unwrap()
        .contains("Official documentation"));
    assert!(!result["answer"].as_str().unwrap().contains("Repeated"));
    assert_eq!(
        serde_json::from_str::<Value>(&format_result(&data, "work", 10).unwrap()).unwrap()
            ["sources"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    for data in [
        json!({}),
        json!({"type":"search", "web":{}}),
        json!({"type":"search", "web":{"results":[{"url":"file:///private"}]}}),
    ] {
        assert!(format_result(&data, "work", 3).is_err());
    }
    let empty: Value = serde_json::from_str(
        &format_result(&json!({"type":"search", "web":{"results":[]}}), "work", 3).unwrap(),
    )
    .unwrap();
    assert_eq!(empty["sources"], json!([]));
    assert!(empty["answer"]
        .as_str()
        .unwrap()
        .contains("Nenhum resultado"));
}

#[tokio::test]
async fn real_http_transport_returns_sources_and_never_exposes_the_search_key() {
    let (request, server) = mock_request(
        r#"{"type":"search","web":{"results":[{"title":"Tauri","url":"https://tauri.app/","description":"Official docs"}]}}"#,
        200,
        None,
    );
    let result = receive(request, "brave-search-work", 3).await.unwrap();
    let sent = server.join().unwrap();
    assert!(sent.starts_with("GET /res/v1/web/search?"));
    assert!(sent
        .to_ascii_lowercase()
        .contains("x-subscription-token: brave-private-test-key"));
    assert!(result.contains("Official docs"));
    assert!(!result.contains("brave-private-test-key"));
}

#[tokio::test]
async fn http_errors_are_actionable_sanitized_and_do_not_follow_redirects() {
    for (status, code) in [
        (401, "web_search_brave_auth"),
        (403, "web_search_brave_auth"),
        (429, "web_search_brave_limit"),
        (422, "web_search_arguments"),
        (500, "web_search_brave_provider"),
        (302, "web_search_brave_provider"),
    ] {
        let (request, server) = mock_request(
            "brave-private-test-key internal provider detail",
            status,
            None,
        );
        let error = receive(request, "work", 3).await.unwrap_err();
        server.join().unwrap();
        assert_eq!(error.code, code);
        assert!(!error.message.contains("brave-private-test-key"));
        assert!(!error.message.contains("internal provider detail"));
    }
}

#[tokio::test]
async fn invalid_and_oversized_responses_fail_without_returning_raw_provider_content() {
    for (body, length) in [
        ("brave-private-test-key", None),
        ("{}", Some(MAX_BYTES + 1)),
    ] {
        let (request, server) = mock_request(body, 200, length);
        let error = receive(request, "work", 3).await.unwrap_err();
        server.join().unwrap();
        assert_eq!(error.code, "web_search_brave_response");
        assert!(!error.message.contains("brave-private-test-key"));
    }
}
