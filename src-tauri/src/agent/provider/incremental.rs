//! Optional Responses WebSocket transport. Local replay stays authoritative.
use super::{
    cancelled, protocol_error, stream_event, AgentError, Delta, Response, StreamOutput, MAX_EVENT,
    MAX_STREAM,
};
use crate::agent::telemetry;
use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use std::time::{Duration, Instant};
use tokio::{net::TcpStream, sync::watch};
use tokio_tungstenite::{
    connect_async_with_config,
    tungstenite::{client::IntoClientRequest, protocol::WebSocketConfig, Message},
    MaybeTlsStream, WebSocketStream,
};

type Socket = WebSocketStream<MaybeTlsStream<TcpStream>>;

struct Previous {
    request: Value,
    expected_input: Vec<Value>,
    response_id: String,
}

impl Previous {
    fn delta(&self, request: &Value) -> Option<Vec<Value>> {
        let input = request["input"].as_array()?;
        let mut previous = self.request.clone();
        let mut next = request.clone();
        previous.as_object_mut()?.remove("input");
        next.as_object_mut()?.remove("input");
        if previous != next || !input.starts_with(&self.expected_input) {
            return None;
        }
        Some(input[self.expected_input.len()..].to_vec())
    }
}

#[derive(Default)]
pub(super) struct Transport {
    socket: Option<Socket>,
    previous: Option<Previous>,
    disabled: bool,
    telemetry: Option<(telemetry::TraceContext, telemetry::ProviderKind)>,
}

fn interrupted() -> AgentError {
    AgentError::new("provider_transport_interrupted", "A conexão incremental foi interrompida após o envio. O conteúdo recebido foi preservado; a solicitação não foi repetida automaticamente. Use Tentar novamente para continuar.")
}

impl Transport {
    pub(super) fn traced(
        trace: telemetry::TraceContext,
        provider: telemetry::ProviderKind,
    ) -> Self {
        Self {
            telemetry: Some((trace, provider)),
            ..Self::default()
        }
    }

    fn reset(&mut self) {
        self.socket = None;
        self.previous = None;
    }

    /// None means no inference was accepted: the caller can safely use HTTP.
    pub(super) async fn attempt(
        &mut self,
        request: reqwest::Request,
        mut signal: watch::Receiver<bool>,
        on_delta: &mut impl FnMut(Delta) -> Result<(), AgentError>,
    ) -> Result<Option<Response>, AgentError> {
        if self.disabled {
            return Ok(None);
        }
        let body: Value = serde_json::from_slice(
            request
                .body()
                .and_then(|body| body.as_bytes())
                .ok_or_else(protocol_error)?,
        )
        .map_err(|_| protocol_error())?;
        let delta = self
            .previous
            .as_ref()
            .and_then(|previous| previous.delta(&body));
        if self.previous.is_some() && delta.is_none() {
            self.reset();
        }
        if self.socket.is_none() {
            let mut url = request.url().clone();
            let scheme = if url.scheme() == "https" { "wss" } else { "ws" };
            url.set_scheme(scheme).map_err(|_| protocol_error())?;
            let mut upgrade = url
                .as_str()
                .into_client_request()
                .map_err(|_| protocol_error())?;
            for (name, value) in request.headers() {
                if !matches!(
                    name.as_str(),
                    "content-length"
                        | "content-type"
                        | "accept"
                        | "host"
                        | "connection"
                        | "upgrade"
                ) {
                    upgrade.headers_mut().insert(name.clone(), value.clone());
                }
            }
            upgrade.headers_mut().insert(
                "OpenAI-Beta",
                "responses_websockets=2026-02-06"
                    .parse()
                    .map_err(|_| protocol_error())?,
            );
            let config = WebSocketConfig::default()
                .max_message_size(Some(MAX_EVENT))
                .max_frame_size(Some(MAX_EVENT));
            let connected = tokio::select! {
                _ = cancelled(&mut signal) => return Err(AgentError::cancelled()),
                result = tokio::time::timeout(Duration::from_secs(8), connect_async_with_config(upgrade, Some(config), true)) => result,
            };
            match connected {
                Ok(Ok((socket, _))) => self.socket = Some(socket),
                _ => {
                    self.disabled = true;
                    self.reset();
                    return Ok(None);
                }
            }
        }
        let mut payload = body.clone();
        payload["type"] = "response.create".into();
        // Responses WebSockets v2 uses the same create fields, including store=false.
        if let (Some(previous), Some(delta)) = (&self.previous, delta) {
            payload["previous_response_id"] = previous.response_id.clone().into();
            payload["input"] = delta.into();
        }
        let model_id = telemetry::model_id(payload["model"].as_str().unwrap_or_default());
        let payload_text = payload.to_string();
        if let Some((trace, provider)) = &self.telemetry {
            telemetry::record(
                trace,
                telemetry::Event::ProviderRequest {
                    provider: *provider,
                    model_id: model_id.clone(),
                    attempt: 1,
                    input_items: payload["input"]
                        .as_array()
                        .map_or(0, |input| input.len() as u64),
                    input_bytes: payload_text.len() as u64,
                    advertised_tools: payload["tools"]
                        .as_array()
                        .map_or(0, |tools| tools.len() as u64),
                },
            );
        }
        let started = Instant::now();
        let mut first_event_ms = None;
        let socket = self.socket.as_mut().ok_or_else(protocol_error)?;
        let sent = tokio::select! {
            _ = cancelled(&mut signal) => Err(AgentError::cancelled()),
            result = tokio::time::timeout(Duration::from_secs(15), socket.send(Message::Text(payload_text.into()))) => result.map_err(|_| interrupted()).and_then(|result| result.map_err(|_| interrupted())),
        };
        let result = match sent {
            Ok(()) => {
                self.receive(body, &mut signal, &mut |delta| {
                    if first_event_ms.is_none()
                        && matches!(delta, Delta::Text(_) | Delta::Summary(_))
                    {
                        first_event_ms = Some(started.elapsed().as_millis() as u64);
                    }
                    on_delta(delta)
                })
                .await
            }
            Err(error) => Err(error),
        };
        if let Some((trace, provider)) = &self.telemetry {
            let failure = result.as_ref().err();
            let usage = result
                .as_ref()
                .ok()
                .and_then(Option::as_ref)
                .and_then(|response| response.usage.as_ref());
            telemetry::record(
                trace,
                telemetry::Event::ProviderResponse {
                    provider: *provider,
                    model_id,
                    attempt: 1,
                    outcome: telemetry::outcome(failure, matches!(result, Ok(None))),
                    duration_ms: started.elapsed().as_millis() as u64,
                    first_event_ms,
                    input_tokens: usage.map(|usage| usage.input_tokens),
                    output_tokens: usage.map(|usage| usage.output_tokens),
                    cache_read_tokens: usage.and_then(|usage| usage.cache_read_tokens),
                    cache_write_tokens: usage.and_then(|usage| usage.cache_write_tokens),
                    failure: failure.map(telemetry::failure_class),
                },
            );
        }
        if result.is_err() || matches!(result, Ok(None)) {
            self.disabled = true;
            self.reset();
        }
        result
    }

    async fn receive(
        &mut self,
        request: Value,
        signal: &mut watch::Receiver<bool>,
        on_delta: &mut impl FnMut(Delta) -> Result<(), AgentError>,
    ) -> Result<Option<Response>, AgentError> {
        let socket = self.socket.as_mut().ok_or_else(protocol_error)?;
        let mut output = StreamOutput::default();
        let mut accepted = false;
        let mut size = 0;
        loop {
            let frame = tokio::select! {
                _ = cancelled(signal) => return Err(AgentError::cancelled()),
                result = tokio::time::timeout(Duration::from_secs(120), socket.next()) => result.map_err(|_| interrupted())?.ok_or_else(interrupted)?.map_err(|_| interrupted())?,
            };
            let text = match frame {
                Message::Text(text) => text,
                Message::Ping(_) | Message::Pong(_) => {
                    tokio::select! {
                        _ = cancelled(signal) => return Err(AgentError::cancelled()),
                        result = tokio::time::timeout(Duration::from_secs(15), socket.flush()) => {
                            result.map_err(|_| interrupted())?.map_err(|_| interrupted())?;
                        }
                    }
                    continue;
                }
                Message::Close(_) => return Err(interrupted()),
                _ => return Err(protocol_error()),
            };
            size += text.len();
            if size > MAX_STREAM {
                return Err(protocol_error());
            }
            let event: Value = serde_json::from_str(&text).map_err(|_| protocol_error())?;
            if !accepted
                && event["type"] == "error"
                && super::upstream_code(&event).is_some_and(|code| {
                    matches!(
                        code.as_str(),
                        "previous_response_not_found"
                            | "websocket_not_supported"
                            | "unsupported_websocket"
                    )
                })
            {
                return Ok(None);
            }
            accepted = true;
            if let Some(response) = stream_event(&mut output, &event, on_delta)? {
                self.previous = event["response"]["id"]
                    .as_str()
                    .filter(|id| !id.is_empty())
                    .map(|id| {
                        let mut expected_input =
                            request["input"].as_array().cloned().unwrap_or_default();
                        expected_input.extend(response.output.clone());
                        Previous {
                            request,
                            expected_input,
                            response_id: id.to_owned(),
                        }
                    });
                return Ok(Some(response));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tokio::net::TcpListener;
    use tokio_tungstenite::accept_async;

    fn body() -> Value {
        json!({"model":"test","instructions":"Be precise","input":[{"role":"user","content":"x".repeat(4000)}],"store":false,"stream":true,"tools":[]})
    }
    fn request(port: u16, body: &Value) -> reqwest::Request {
        reqwest::Client::new()
            .post(format!("http://127.0.0.1:{port}/responses"))
            .json(body)
            .build()
            .unwrap()
    }
    fn done(id: &str) -> Value {
        json!({"type":"response.completed","response":{"id":id,"status":"completed","output":[{"type":"message","role":"assistant","content":[{"type":"output_text","text":"OK"}]}]}})
    }

    #[tokio::test]
    async fn reuses_connection_and_only_sends_new_input_after_a_verified_prefix() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = tokio::spawn(async move {
            let (tcp, _) = listener.accept().await.unwrap();
            let mut socket = accept_async(tcp).await.unwrap();
            let first = socket.next().await.unwrap().unwrap().into_text().unwrap();
            socket
                .send(Message::Text(done("r1").to_string().into()))
                .await
                .unwrap();
            let second = socket.next().await.unwrap().unwrap().into_text().unwrap();
            socket
                .send(Message::Text(done("r2").to_string().into()))
                .await
                .unwrap();
            (first, second)
        });
        let (_tx, rx) = watch::channel(false);
        let mut transport = Transport::default();
        let mut next = body();
        let first = transport
            .attempt(request(port, &next), rx.clone(), &mut |_| Ok(()))
            .await
            .unwrap()
            .unwrap();
        next["input"].as_array_mut().unwrap().extend(first.output);
        next["input"]
            .as_array_mut()
            .unwrap()
            .push(json!({"role":"user","content":"continue"}));
        transport
            .attempt(request(port, &next), rx, &mut |_| Ok(()))
            .await
            .unwrap()
            .unwrap();
        let (first, second) = server.await.unwrap();
        let value: Value = serde_json::from_str(&second).unwrap();
        assert_eq!(value["previous_response_id"], "r1");
        assert_eq!(value["input"].as_array().unwrap().len(), 1);
        assert_eq!(value["store"], false);
        assert!(second.len() < first.len() / 4);
    }

    #[test]
    fn compaction_model_and_catalog_changes_invalidate_incremental_state() {
        let original = body();
        let previous = Previous {
            request: original.clone(),
            expected_input: original["input"].as_array().unwrap().clone(),
            response_id: "id".into(),
        };
        for (field, value) in [
            ("model", json!("other")),
            ("tools", json!([{"name":"new_tool"}])),
            ("instructions", json!("new instructions")),
            ("input", json!([])),
        ] {
            let mut changed = original.clone();
            changed[field] = value;
            assert!(previous.delta(&changed).is_none(), "{field}");
        }
    }

    #[tokio::test]
    async fn secure_handshake_failure_returns_to_http_without_panicking() {
        use tokio::io::AsyncWriteExt;
        crate::initialize_tls();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = tokio::spawn(async move {
            let (mut tcp, _) = listener.accept().await.unwrap();
            let _ = tcp.write_all(b"not a TLS server").await;
        });
        let mut request = request(port, &body());
        request.url_mut().set_scheme("https").unwrap();
        let (_tx, signal) = watch::channel(false);
        let mut transport = Transport::default();
        assert!(transport
            .attempt(request, signal, &mut |_| Ok(()))
            .await
            .unwrap()
            .is_none());
        assert!(transport.disabled);
        server.await.unwrap();
    }

    #[tokio::test]
    async fn cancellation_interrupts_a_stalled_secure_handshake() {
        use tokio::io::AsyncReadExt;
        crate::initialize_tls();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let (cancel, signal) = watch::channel(false);
        let server = tokio::spawn(async move {
            let (mut tcp, _) = listener.accept().await.unwrap();
            let mut hello = [0; 4096];
            assert!(tcp.read(&mut hello).await.unwrap() > 0);
            cancel.send_replace(true);
            // Keep the peer connected so only cancellation can end the handshake.
            cancel.closed().await;
        });
        let mut request = request(port, &body());
        request.url_mut().set_scheme("https").unwrap();
        let mut transport = Transport::default();
        let result = tokio::time::timeout(
            Duration::from_secs(1),
            transport.attempt(request, signal, &mut |_| Ok(())),
        )
        .await
        .expect("stop must not wait for the handshake timeout");
        assert_eq!(result.unwrap_err().code, "cancelled");
        server.await.unwrap();
    }

    #[tokio::test]
    async fn handshake_rejection_falls_back_without_submitting_an_inference() {
        use tokio::io::AsyncWriteExt;
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = tokio::spawn(async move {
            let (mut tcp, _) = listener.accept().await.unwrap();
            tcp.write_all(
                b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            )
            .await
            .unwrap();
        });
        let (_tx, rx) = watch::channel(false);
        let mut transport = Transport::default();
        assert!(transport
            .attempt(request(port, &body()), rx, &mut |_| Ok(()))
            .await
            .unwrap()
            .is_none());
        assert!(transport.disabled);
        server.await.unwrap();
    }

    #[tokio::test]
    async fn turn_session_uses_http_after_upgrade_rejection_and_keeps_full_replay() {
        use crate::agent::{context_manager::StepContext, tests, ApprovalMode};
        use crate::openai_codex::{custom::Config, CodexCredential, ProviderModel};
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = tokio::spawn(async move {
            let mut methods = Vec::new();
            for index in 0..3 {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut bytes = Vec::new();
                let header_end = loop {
                    let mut chunk = [0; 4096];
                    let count = socket.read(&mut chunk).await.unwrap();
                    assert!(count > 0);
                    bytes.extend_from_slice(&chunk[..count]);
                    if let Some(end) = bytes.windows(4).position(|window| window == b"\r\n\r\n") {
                        break end + 4;
                    }
                };
                let headers = String::from_utf8(bytes[..header_end].to_vec()).unwrap();
                let method = headers.split_whitespace().next().unwrap().to_owned();
                methods.push(method.clone());
                if index == 0 {
                    assert_eq!(method, "GET");
                    socket.write_all(b"HTTP/1.1 426 Upgrade Required\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").await.unwrap();
                } else {
                    assert_eq!(method, "POST");
                    let length: usize = headers
                        .lines()
                        .find_map(|line| {
                            line.to_lowercase()
                                .strip_prefix("content-length:")
                                .map(|value| value.trim().parse().unwrap())
                        })
                        .unwrap();
                    while bytes.len() < header_end + length {
                        let mut chunk = [0; 4096];
                        let count = socket.read(&mut chunk).await.unwrap();
                        assert!(count > 0);
                        bytes.extend_from_slice(&chunk[..count]);
                    }
                    let body: Value =
                        serde_json::from_slice(&bytes[header_end..header_end + length]).unwrap();
                    assert_eq!(body["store"], false);
                    assert!(body.get("previous_response_id").is_none());
                    assert!(body["input"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|item| item["role"] == "user"));
                    let sse = format!("data: {}\n\n", done("http-response"));
                    socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{sse}", sse.len()).as_bytes()).await.unwrap();
                }
            }
            methods
        });
        let config: Config = serde_json::from_value(json!({
            "baseUrl":format!("http://127.0.0.1:{port}"),"protocol":"openai-responses","authMode":"bearer","tokenField":"max_tokens",
            "models":[{"id":"model","name":"Model","contextWindow":64000,"maxOutputTokens":4000,"supportsImages":false,"supportsTools":true,"reasoning":"none","reasoningLevels":[],"defaultReasoningLevel":null,"thinkingBudget":null}]
        })).unwrap();
        let mut credential = CodexCredential::new("synthetic-token", "", 0, "account", None, None);
        credential.custom = Some(config);
        let model = ProviderModel {
            id: "model".into(),
            name: "Model".into(),
            reasoning_levels: vec![],
            default_reasoning_level: None,
            context_window: Some(64000),
        };
        let mut provider = super::super::TurnSession::new(
            credential,
            &model,
            "session".into(),
            telemetry::TraceContext::new("fixture", "turn"),
        )
        .unwrap();
        provider.set_incremental_transport(true);
        let fixture = tests::Fixture::new();
        let session = tests::session(&fixture);
        let options = tests::options(ApprovalMode::Yolo);
        session
            .reserve("Return OK".into(), options.clone())
            .unwrap();
        let step = StepContext::capture(
            &session,
            &options,
            "Be concise",
            &[],
            provider.capabilities(),
        )
        .unwrap();
        let (_cancel, signal) = watch::channel(false);
        for _ in 0..2 {
            let result = provider
                .stream(&step, signal.clone(), |_| Ok(()))
                .await
                .unwrap();
            assert_eq!(result.text, "OK");
        }
        assert_eq!(server.await.unwrap(), ["GET", "POST", "POST"]);
    }

    #[tokio::test]
    async fn interrupted_stream_preserves_deltas_and_never_replays_automatically() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = tokio::spawn(async move {
            let (tcp, _) = listener.accept().await.unwrap();
            let mut socket = accept_async(tcp).await.unwrap();
            let _ = socket.next().await;
            socket
                .send(Message::Text(
                    json!({"type":"response.output_text.delta","delta":"partial"})
                        .to_string()
                        .into(),
                ))
                .await
                .unwrap();
            socket.close(None).await.unwrap();
        });
        let (_tx, rx) = watch::channel(false);
        let mut transport = Transport::default();
        let mut text = String::new();
        let error = transport
            .attempt(request(port, &body()), rx, &mut |delta| {
                if let Delta::Text(part) = delta {
                    text.push_str(&part);
                }
                Ok(())
            })
            .await
            .unwrap_err();
        assert_eq!(error.code, "provider_transport_interrupted");
        assert_eq!(text, "partial");
        assert!(transport.previous.is_none());
        server.await.unwrap();
    }
}
