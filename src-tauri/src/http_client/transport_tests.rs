use super::*;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};

fn run() -> Run {
    Run {
        id: "run".into(),
        project_id: "project".into(),
        conversation_id: "conversation".into(),
        draft_id: "draft".into(),
        draft_revision: 1,
        environment_id: None,
        environment_revision: 1,
        request: Request::default(),
        status: "running".into(),
        http_status: None,
        error: None,
        outcome_uncertain: false,
        started_at: 1,
        finished_at: None,
        elapsed_ms: 0,
        received_bytes: 0,
        stored_bytes: 0,
        mime: String::new(),
        url: String::new(),
        headers: Vec::new(),
        redirects: Vec::new(),
        truncated: false,
        body_expired: false,
        preview: String::new(),
    }
}
fn prepared(home: &Path, url: Url) -> Prepared {
    let run = run();
    std::fs::create_dir_all(run_dir(home, &run.project_id, &run.id).unwrap()).unwrap();
    Prepared {
        run,
        client: reqwest::Client::builder()
            .no_proxy()
            .retry(reqwest::retry::never())
            .redirect(reqwest::redirect::Policy::none())
            .read_timeout(Duration::from_secs(1))
            .build()
            .unwrap(),
        method: Method::GET,
        url,
        headers: HeaderMap::new(),
        body: Payload::None,
        secrets: Vec::new(),
        defaults: Defaults::default(),
        home: home.into(),
    }
}
async fn server(response: Vec<u8>) -> (Url, tokio::task::JoinHandle<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = Url::parse(&format!("http://{}/", listener.local_addr().unwrap())).unwrap();
    let task = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut bytes = vec![0; 16384];
        let count = socket.read(&mut bytes).await.unwrap();
        socket.write_all(&response).await.unwrap();
        String::from_utf8_lossy(&bytes[..count]).into_owned()
    });
    (url, task)
}

#[tokio::test]
async fn preserves_error_responses_duplicate_query_and_headers_without_retry() {
    let temp = tempfile::tempdir().unwrap();
    let(url,server)=server(b"HTTP/1.1 422 Unprocessable Entity\r\nContent-Type: application/json\r\nContent-Length: 16\r\nX-Debug: one\r\nX-Debug: two\r\nConnection: close\r\n\r\n{\"error\":\"bad!\"}".to_vec()).await;
    let mut p = prepared(temp.path(), url);
    p.url
        .query_pairs_mut()
        .append_pair("item", "a")
        .append_pair("item", "b");
    add_header(&mut p.headers, "X-Test", "one").unwrap();
    add_header(&mut p.headers, "X-Test", "two").unwrap();
    let mut run = p.run.clone();
    perform(&p, &mut run).await.unwrap();
    let request = server.await.unwrap();
    assert!(request.contains("item=a&item=b"));
    assert!(request
        .to_ascii_lowercase()
        .contains("x-test: one\r\nx-test: two"));
    assert_eq!(run.http_status, Some(422));
    assert_eq!(
        run.headers.iter().filter(|h| h.name == "x-debug").count(),
        2
    );
    let page = read_result(temp.path(), run, 0, 16000).unwrap();
    assert_eq!(page.text, "{\"error\":\"bad!\"}");
}
#[tokio::test]
async fn supports_empty_and_bounded_binary_bodies() {
    let temp = tempfile::tempdir().unwrap();
    let (url, empty_server) =
        server(b"HTTP/1.1 204 No Content\r\nConnection: close\r\n\r\n".to_vec()).await;
    let p = prepared(temp.path(), url);
    let mut run = p.run.clone();
    perform(&p, &mut run).await.unwrap();
    empty_server.await.unwrap();
    assert_eq!(run.http_status, Some(204));
    assert_eq!(run.stored_bytes, 0);
    assert_eq!(read_result(temp.path(), run, 0, 100).unwrap().text, "");
    let mut response=b"HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Length: 4096\r\nConnection: close\r\n\r\n".to_vec();
    response.extend(vec![0u8; 4096]);
    let (url, server) = server(response).await;
    let mut p = prepared(temp.path(), url);
    p.defaults.max_response_bytes = 1024;
    let mut run = p.run.clone();
    perform(&p, &mut run).await.unwrap();
    server.await.unwrap();
    assert_eq!(run.stored_bytes, 1024);
    assert!(run.truncated);
    assert!(read_result(temp.path(), run, 0, 100).unwrap().binary);
}
#[tokio::test]
async fn cross_origin_redirect_strips_auth_and_secret_derived_headers() {
    let temp = tempfile::tempdir().unwrap();
    let (target, target_task) =
        server(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok".to_vec())
            .await;
    let response=format!("HTTP/1.1 302 Found\r\nLocation: {target}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
    let (url, first) = server(response.into_bytes()).await;
    let mut p = prepared(temp.path(), url);
    p.defaults.follow_redirects = true;
    p.secrets = vec!["private-token".into()];
    add_header(&mut p.headers, "Authorization", "Bearer private-token").unwrap();
    add_header(&mut p.headers, "X-Custom", "private-token").unwrap();
    let mut run = p.run.clone();
    perform(&p, &mut run).await.unwrap();
    first.await.unwrap();
    let request = target_task.await.unwrap();
    assert!(!request.contains("private-token"));
    assert!(!request.to_ascii_lowercase().contains("authorization"));
    assert_eq!(run.redirects.len(), 1);
}
#[tokio::test]
async fn stalled_response_is_bounded_by_read_deadline() {
    let temp = tempfile::tempdir().unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = Url::parse(&format!("http://{}/", listener.local_addr().unwrap())).unwrap();
    let server = tokio::spawn(async move {
        let (_socket, _) = listener.accept().await.unwrap();
        tokio::time::sleep(Duration::from_secs(2)).await;
    });
    let p = prepared(temp.path(), url);
    let mut run = p.run.clone();
    assert_eq!(
        perform(&p, &mut run).await.unwrap_err().code,
        "http_timeout"
    );
    server.abort();
}
#[test]
fn unused_missing_secret_does_not_block_and_environment_override_wins() {
    let missing = Variable {
        id: "one".into(),
        name: "token".into(),
        secret: true,
        configured: false,
        enabled: true,
        ..Variable::default()
    };
    let common = Variable {
        id: "two".into(),
        name: "base_url".into(),
        value: "https://prod.test".into(),
        enabled: true,
        ..Variable::default()
    };
    let environment = Variable {
        id: "three".into(),
        name: "base_url".into(),
        value: "http://localhost".into(),
        enabled: true,
        ..Variable::default()
    };
    let definitions = BTreeMap::from([
        ("token".into(), &missing),
        ("base_url".into(), &common),
        ("base_url".into(), &environment),
    ]);
    let mut values = BTreeMap::new();
    let mut secrets = Vec::new();
    assert_eq!(
        resolve_template(
            "{{base_url}}/api",
            &definitions,
            &mut values,
            &mut secrets,
            "project"
        )
        .unwrap(),
        "http://localhost/api"
    );
    assert!(resolve_template(
        "{{token}}",
        &definitions,
        &mut values,
        &mut secrets,
        "project"
    )
    .is_err());
}

#[test]
fn json_interpolation_escapes_quotes_and_newlines_in_string_values() {
    let text = resolve_json(
        r#"{"password":"{{token}}","items":["{{token}}"]}"#,
        &mut |value| Ok(value.replace("{{token}}", "a\"b\n")),
    )
    .unwrap();
    let value: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(value["password"], "a\"b\n");
    assert_eq!(value["items"][0], "a\"b\n");
}

#[tokio::test]
async fn cancelling_a_dispatched_request_does_not_retry_server_mutation() {
    let temp = tempfile::tempdir().unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = Url::parse(&format!(
        "http://{}/mutation",
        listener.local_addr().unwrap()
    ))
    .unwrap();
    let (mutated, mut received) = tokio::sync::mpsc::channel(1);
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut request = [0; 1024];
        let size = socket.read(&mut request).await.unwrap();
        assert!(String::from_utf8_lossy(&request[..size]).starts_with("POST "));
        mutated.send(()).await.unwrap();
        assert!(
            tokio::time::timeout(Duration::from_millis(150), listener.accept())
                .await
                .is_err()
        );
    });
    let mut p = prepared(temp.path(), url);
    p.method = Method::POST;
    let mut run = p.run.clone();
    {
        let operation = perform(&p, &mut run);
        tokio::pin!(operation);
        tokio::select! {result=&mut operation=>panic!("unexpected completion: {result:?}"),_=received.recv()=>{}}
    }
    server.await.unwrap();
}

#[test]
fn file_reference_cannot_bypass_agent_project_scope() {
    let temp = tempfile::tempdir().unwrap();
    let dir = directory(temp.path(), "project")
        .unwrap()
        .join("files")
        .join("file");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("metadata.json"),
        br#"{"id":"file","name":"credential","size":6,"source":"/outside/credential"}"#,
    )
    .unwrap();
    std::fs::write(dir.join("body"), b"secret").unwrap();
    assert_eq!(
        file(temp.path(), "project", "file", Some(Path::new("/project")))
            .unwrap_err()
            .code,
        "http_file_scope"
    );
    assert_eq!(
        file(temp.path(), "project", "file", None).unwrap().1,
        b"secret"
    );
}
