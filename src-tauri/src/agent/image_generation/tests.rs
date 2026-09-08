use super::*;
use std::io::{Read, Write};

fn png() -> Vec<u8> {
    let mut bytes = std::io::Cursor::new(vec![]);
    image::DynamicImage::new_rgb8(8, 6)
        .write_to(&mut bytes, image::ImageFormat::Png)
        .unwrap();
    bytes.into_inner()
}
fn event() -> Value {
    json!({"response":{"candidates":[{"content":{"parts":[{"inlineData":{"mimeType":"image/png","data":STANDARD.encode(png())}}]},"finishReason":"STOP"}]}})
}
fn account(state: &AppState, home: &Path, alias: &str, kind: &str) {
    state
        .with_connection(home, |db| {
            db.execute(
                "INSERT INTO provider_accounts(alias,provider_kind,account_id) VALUES(?1,?2,?1)",
                [alias, kind],
            )
            .map_err(|_| AgentError::storage())
        })
        .unwrap();
}

#[test]
fn settings_default_to_off_and_only_enable_active_antigravity() {
    let home = tempfile::tempdir().unwrap();
    let state = AppState::default();
    assert!(!enabled(&state, home.path()));
    assert!(!load(&state, home.path()).unwrap().inherit_chat);
    account(&state, home.path(), "codex", "openai-codex");
    account(&state, home.path(), "google", "antigravity");
    assert!(save(&state, home.path(), Some("codex".into())).is_err());
    assert!(save(&state, home.path(), Some("missing".into())).is_err());
    assert_eq!(
        save(&state, home.path(), Some("google".into()))
            .unwrap()
            .model
            .as_deref(),
        Some(MODEL)
    );
    assert!(enabled(&state, home.path()));
    state
        .with_connection(home.path(), |db| {
            db.execute(
                "UPDATE provider_accounts SET enabled=0 WHERE alias='google'",
                [],
            )
            .map_err(|_| AgentError::storage())
        })
        .unwrap();
    assert!(!enabled(&state, home.path()));
    assert!(save(&state, home.path(), Some("google".into())).is_err());
    state
        .with_connection(home.path(), |db| {
            db.execute("DELETE FROM provider_accounts WHERE alias='google'", [])
                .map_err(|_| AgentError::storage())
        })
        .unwrap();
    assert!(load(&state, home.path()).unwrap().account_alias.is_none());
}

#[tokio::test]
async fn disabled_generation_fails_without_credentials_or_network() {
    let home = tempfile::tempdir().unwrap();
    let (_send, signal) = watch::channel(false);
    let result = execute(
        &AppState::default(),
        &OpenAiCodexState::default(),
        home.path(),
        &"a".repeat(32),
        &json!({"prompt":"Uma árvore"}),
        signal,
    )
    .await;
    assert_eq!(result.unwrap_err().code, "image_generation");
}

#[test]
fn request_uses_fixed_model_and_owned_references_only() {
    let credential: CodexCredential = serde_json::from_value(json!({"version":1,"access":"test","refresh":"test","expires":9999999999999u64,"accountId":"test","projectId":"project"})).unwrap();
    let home = tempfile::tempdir().unwrap();
    let conversation = "a".repeat(32);
    let item = attachments::store(home.path(), &conversation, "reference.png", &png()).unwrap();
    let args =
        arguments(&json!({"prompt":"Recolorir","aspect_ratio":"16:9","image_ids":[item.id]}))
            .unwrap();
    let body = request_body(&credential, home.path(), &conversation, &args).unwrap();
    assert_eq!(body["model"], MODEL);
    assert_eq!(body["project"], "project");
    assert_eq!(
        body["request"]["generationConfig"]["responseModalities"],
        json!(["IMAGE"])
    );
    assert!(body["request"]["contents"][0]["parts"][0]["inlineData"].is_object());
    assert!(request_body(&credential, home.path(), &"b".repeat(32), &args).is_err());
    assert!(arguments(&json!({"prompt":"x", "model":"another-model"})).is_err());
    assert!(arguments(&json!({"prompt":" "})).is_err());
    assert!(arguments(&json!({"prompt":"x", "aspect_ratio":"invalid"})).is_err());
}

#[test]
fn output_persists_images_without_binary_payloads_or_private_thoughts() {
    let mut output = Output::default();
    output.event(&json!({"response":{"candidates":[{"content":{"parts":[{"text":"private thought","thought":true,"thoughtSignature":"secret"}]}}]}})).unwrap();
    output.event(&event()).unwrap();
    output.event(&event()).unwrap();
    assert_eq!(output.images.len(), 1);
    let home = tempfile::tempdir().unwrap();
    let conversation = "a".repeat(32);
    let raw = persist(home.path(), &conversation, "google", output).unwrap();
    assert!(!raw.contains("base64") && !raw.contains("private thought") && !raw.contains("secret"));
    let result: Value = serde_json::from_str(&raw).unwrap();
    let id = result["images"][0]["id"].as_str().unwrap();
    assert_eq!(
        attachments::metadata(home.path(), &conversation, id)
            .unwrap()
            .kind,
        "image"
    );
    assert_eq!(
        attachments::bounded_read(
            &attachments::location(home.path(), &conversation, id)
                .unwrap()
                .join("source"),
            attachments::MAX_BYTES
        )
        .unwrap(),
        png()
    );
}

#[test]
fn malformed_blocked_empty_and_incomplete_results_fail() {
    assert!(Output::default()
        .event(&json!({"response":{"promptFeedback":{"blockReason":"SAFETY"}}}))
        .is_err());
    assert!(Output::default()
        .event(&json!({"error":{"message":"sensitive detail"}}))
        .unwrap_err()
        .message
        .find("sensitive")
        .is_none());
    let mut empty = Output::default();
    empty
        .event(&json!({"response":{"candidates":[{"finishReason":"STOP"}]}}))
        .unwrap();
    assert!(empty.validate().is_err());
    let mut broken = event();
    broken["response"]["candidates"][0]["content"]["parts"][0]["inlineData"]["data"] =
        json!("broken");
    assert!(Output::default().event(&broken).is_err());
    let mut incomplete = event();
    incomplete["response"]["candidates"][0]
        .as_object_mut()
        .unwrap()
        .remove("finishReason");
    let mut output = Output::default();
    output.event(&incomplete).unwrap();
    assert!(output.validate().is_err());
}

async fn fixture_response(body: String, status: u16) -> reqwest::Response {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut request = [0; 1024];
        let _ = socket.read(&mut request);
        let header = format!("HTTP/1.1 {status} OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",body.len());
        socket.write_all(header.as_bytes()).unwrap();
        for chunk in body.as_bytes().chunks(19) {
            if socket.write_all(chunk).is_err() {
                break;
            }
        }
    });
    reqwest::Client::new()
        .get(format!("http://{address}"))
        .send()
        .await
        .unwrap()
}
#[tokio::test]
async fn streaming_images_handle_fragmented_sse_eof_and_cancellation() {
    let body = format!("data: {}\r\n\r\ndata: [DONE]", event());
    let (_send, signal) = watch::channel(false);
    let output = receive(fixture_response(body.clone(), 200).await, signal)
        .await
        .unwrap();
    assert_eq!(output.images, vec![png()]);
    let (_send, signal) = watch::channel(true);
    assert_eq!(
        receive(fixture_response(body, 200).await, signal)
            .await
            .err()
            .unwrap()
            .code,
        "cancelled"
    );
    let (_send, signal) = watch::channel(false);
    let error = receive(
        fixture_response("sensitive server response".into(), 429).await,
        signal,
    )
    .await
    .err()
    .unwrap();
    assert!(error.message.contains("limite") && !error.message.contains("sensitive"));
}
