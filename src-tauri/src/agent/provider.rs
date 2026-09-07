use super::{cancelled, AgentError, ToolCall, TurnOptions, Usage};
use crate::openai_codex::{CodexCredential, OPENAI_CODEX_BASE_URL, OPENAI_CODEX_CLIENT_VERSION};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, HashSet},
    time::Duration,
};
use tokio::sync::watch;

mod antigravity;
mod custom;

pub(super) use antigravity::grounded_search;
const MAX_EVENT: usize = 4 * 1024 * 1024;
const MAX_STREAM: usize = 16 * 1024 * 1024;
pub(super) enum Delta {
    Text(String),
    Summary(String),
}
pub(super) struct Response {
    pub output: Vec<Value>,
    pub text: String,
    pub summary: String,
    pub usage: Option<Usage>,
}

#[derive(Default)]
pub(super) struct Sse {
    pending: Vec<u8>,
    data: Vec<u8>,
    done: bool,
}

#[derive(Default)]
struct StreamOutput {
    pending: HashSet<u64>,
    done: BTreeMap<u64, Value>,
}
impl StreamOutput {
    fn item(&mut self, event: &Value, finished: bool) -> Result<(), AgentError> {
        let index = event["output_index"]
            .as_u64()
            .filter(|index| *index < 128)
            .ok_or_else(protocol_error)?;
        if finished {
            let item = event
                .get("item")
                .filter(|item| item.is_object())
                .ok_or_else(protocol_error)?;
            if self
                .done
                .get(&index)
                .is_some_and(|previous| previous != item)
            {
                return Err(protocol_error());
            }
            self.done.insert(index, item.clone());
            self.pending.remove(&index);
        } else {
            self.pending.insert(index);
        }
        Ok(())
    }
    fn finish(self, mut response: Value) -> Result<Response, AgentError> {
        // Codex SSE can emit an empty terminal output array. Completed item events
        // contain the actual response, including encrypted reasoning and tool calls.
        if response["output"].as_array().is_none_or(Vec::is_empty) {
            if !self.pending.is_empty() || self.done.is_empty() {
                return Err(protocol_error());
            }
            response["output"] = Value::Array(self.done.into_values().collect());
        }
        completed(&response)
    }
}
impl Sse {
    fn push(&mut self, bytes: &[u8]) -> Result<Vec<Value>, AgentError> {
        self.push_bounded(bytes, MAX_EVENT)
    }
    pub(super) fn push_bounded(&mut self, bytes: &[u8], limit: usize) -> Result<Vec<Value>, AgentError> {
        self.pending.extend_from_slice(bytes);
        let mut consumed = 0;
        let mut events = vec![];
        while let Some(relative) = self.pending[consumed..]
            .iter()
            .position(|byte| *byte == b'\n')
        {
            let end = consumed + relative;
            let line = self.pending[consumed..end]
                .strip_suffix(b"\r")
                .unwrap_or(&self.pending[consumed..end]);
            if line.is_empty() && !self.data.is_empty() {
                if self.data != b"[DONE]\n" {
                    events.push(serde_json::from_slice(&self.data).map_err(|_| protocol_error())?);
                } else {
                    self.done = true;
                }
                self.data.clear();
            } else if let Some(data) = line.strip_prefix(b"data:") {
                self.data
                    .extend_from_slice(data.strip_prefix(b" ").unwrap_or(data));
                self.data.push(b'\n');
            }
            if self.data.len() > limit {
                return Err(protocol_error());
            }
            consumed = end + 1;
        }
        self.pending.drain(..consumed);
        if self.pending.len() > limit {
            return Err(protocol_error());
        }
        Ok(events)
    }
}
fn protocol_error() -> AgentError {
    AgentError::new("provider_protocol", "O provedor retornou uma resposta incompleta ou inválida. O progresso recebido foi preservado.")
}
fn failure(status: u16) -> AgentError {
    match status {
        401 | 403 => AgentError::new("provider_auth", "O provedor recusou o acesso. Verifique a assinatura e reconecte esta conta nas configurações."),
        429 => AgentError::new("provider_limit", "O limite da conta foi atingido. Aguarde a renovação ou selecione outra conta."),
        400 => AgentError::new("provider_request", "O provedor recusou a solicitação. Verifique o modelo e o nível de raciocínio selecionados."),
        _ => AgentError::new("provider_unavailable", "O provedor está indisponível no momento. Tente novamente em instantes."),
    }
}
fn context_overflow(value: &Value) -> bool {
    let error = value
        .get("error")
        .or_else(|| {
            value
                .get("response")
                .and_then(|response| response.get("error"))
        })
        .unwrap_or(value);
    let code = error["code"].as_str().unwrap_or_default();
    let message = error["message"].as_str().unwrap_or_default().to_lowercase();
    matches!(
        code,
        "context_length_exceeded" | "context_window_exceeded" | "max_context_length"
    ) || message.contains("maximum context length")
        || message.contains("exceeds the context window")
        || message.contains("context window exceeded")
        || message.contains("prompt is too long")
        || message.contains("input is too long")
}
fn overflow_error() -> AgentError {
    AgentError::new(
        "context_overflow",
        "A janela de contexto do modelo foi excedida.",
    )
}
fn request_body(
    options: &TurnOptions,
    instructions: &str,
    input: Vec<Value>,
    tools: Vec<Value>,
    session_id: &str,
) -> Value {
    let input: Vec<Value> = input
        .into_iter()
        .filter_map(|mut item| {
            if item["type"] == "reasoning"
                && (!item["encrypted_content"].is_string() || item.get("_custom").is_some())
            {
                return None;
            }
            if let Some(map) = item.as_object_mut() {
                map.retain(|key, _| !key.starts_with("_antigravity") && key != "_custom");
            }
            Some(item)
        })
        .collect();
    let mut body = json!({
        "model":options.model, "instructions":instructions, "input":input,
        "tools":tools, "tool_choice":"auto", "parallel_tool_calls":false,
        "stream":true, "store":false, "prompt_cache_key":session_id,
        "include":["reasoning.encrypted_content"]
    });
    if let Some(effort) = &options.reasoning {
        body["reasoning"] =
            json!({"effort": if effort == "off" { "none" } else { effort }, "summary":"auto"});
    }
    body
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn stream(
    credential: &CodexCredential,
    session_id: &str,
    options: &TurnOptions,
    instructions: &str,
    input: Vec<Value>,
    tools: Vec<Value>,
    signal: watch::Receiver<bool>,
    on_delta: impl FnMut(Delta) -> Result<(), AgentError>,
) -> Result<Response, AgentError> {
    if let Some(config) = &credential.custom {
        return custom::stream(
            credential,
            config,
            options,
            instructions,
            input,
            tools,
            signal,
            on_delta,
        )
        .await;
    }
    if credential.project_id.is_some() {
        return antigravity::stream(credential, session_id, options, instructions, input, tools, signal, on_delta).await;
    }
    let body = request_body(options, instructions, input, tools, session_id);
    let request = authenticated_request(credential, session_id, &body, Duration::from_secs(600))?;
    receive(request, signal, on_delta).await
}

pub(super) fn authenticated_request(
    credential: &CodexCredential,
    session_id: &str,
    body: &Value,
    timeout: Duration,
) -> Result<reqwest::RequestBuilder, AgentError> {
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(20))
        .timeout(timeout)
        .build()
        .map_err(|_| AgentError::internal())?;
    let request = client
        .post(format!("{OPENAI_CODEX_BASE_URL}/codex/responses"))
        .bearer_auth(&credential.access)
        .header("chatgpt-account-id", &credential.account_id)
        .header("OpenAI-Beta", "responses=experimental")
        .header("originator", "codex_cli_rs")
        .header("version", OPENAI_CODEX_CLIENT_VERSION)
        .header("session_id", session_id)
        .header(
            "x-codex-routing-hint",
            format!("model={}", body["model"].as_str().unwrap_or_default()),
        )
        .header("accept", "text/event-stream")
        .header("content-type", "application/json")
        .body(serde_json::to_vec(&body).map_err(|_| AgentError::internal())?);
    Ok(request)
}

pub(super) async fn receive(
    request: reqwest::RequestBuilder,
    mut signal: watch::Receiver<bool>,
    mut on_delta: impl FnMut(Delta) -> Result<(), AgentError>,
) -> Result<Response, AgentError> {
    let mut response = tokio::select! {
        _ = cancelled(&mut signal) => return Err(AgentError::cancelled()),
        result = request.send() => result.map_err(|_| AgentError::new("provider_network", "Não foi possível conectar ao provedor. Verifique a conexão e tente novamente."))?,
    };
    if !response.status().is_success() {
        if matches!(response.status().as_u16(), 400 | 413) {
            // Inspect only a bounded error body for the known unsupported-model
            // case. Never return upstream bodies, which can contain private data.
            let read_error = async {
                let mut bytes = vec![];
                while let Ok(Some(chunk)) = response.chunk().await {
                    if bytes.len() + chunk.len() > 64 * 1024 {
                        break;
                    }
                    bytes.extend_from_slice(&chunk);
                }
                let detail = String::from_utf8_lossy(&bytes).to_lowercase();
                (
                    detail.contains("model")
                        && (detail.contains("model is not supported")
                            || detail
                                .contains("not supported when using codex with a chatgpt account")),
                    serde_json::from_slice::<Value>(&bytes)
                        .is_ok_and(|value| context_overflow(&value)),
                )
            };
            let (unsupported, overflow) = tokio::select! {
                _ = cancelled(&mut signal) => return Err(AgentError::cancelled()),
                result = tokio::time::timeout(Duration::from_secs(5), read_error) => result.unwrap_or((false, false)),
            };
            if overflow {
                return Err(overflow_error());
            }
            if unsupported {
                return Err(AgentError::new(
                    "provider_model_unsupported",
                    "O modelo não é compatível com esta conta ChatGPT.",
                ));
            }
        }
        return Err(failure(response.status().as_u16()));
    }
    let mut parser = Sse::default();
    let mut output = StreamOutput::default();
    let mut size = 0;
    loop {
        let chunk = tokio::select! {
            _ = cancelled(&mut signal) => return Err(AgentError::cancelled()),
            result = tokio::time::timeout(Duration::from_secs(120), response.chunk()) => result.map_err(|_| AgentError::new("provider_timeout", "O provedor ficou sem responder. A execução foi interrompida."))?.map_err(|_| protocol_error())?,
        };
        let Some(chunk) = chunk else {
            return Err(protocol_error());
        };
        size += chunk.len();
        if size > MAX_STREAM {
            return Err(protocol_error());
        }
        for event in parser.push(&chunk)? {
            #[cfg(test)]
            if std::env::var_os("JARVIS_LIVE_ACCOUNT").is_some() {
                eprintln!(
                    "Codex event: type={}, status={}, output_kinds={:?}",
                    event["type"].as_str().unwrap_or("missing"),
                    event["response"]["status"].as_str().unwrap_or("missing"),
                    event["response"]["output"].as_array().map(|items| items
                        .iter()
                        .filter_map(|item| item["type"].as_str())
                        .collect::<Vec<_>>())
                );
            }
            match event["type"].as_str() {
                Some("response.output_item.added") => output.item(&event, false)?,
                Some("response.output_item.done") => output.item(&event, true)?,
                Some("response.output_text.delta" | "response.refusal.delta") => {
                    if let Some(text) = event["delta"].as_str() {
                        on_delta(Delta::Text(text.into()))?;
                    }
                }
                Some("response.reasoning_summary_text.delta") => {
                    if let Some(text) = event["delta"].as_str() {
                        on_delta(Delta::Summary(text.into()))?;
                    }
                }
                Some("response.reasoning_summary_part.done") => {
                    on_delta(Delta::Summary("\n\n".into()))?
                }
                Some("response.completed" | "response.done") => {
                    return output.finish(event["response"].clone())
                }
                Some("response.failed" | "error") if context_overflow(&event) => {
                    return Err(overflow_error())
                }
                Some("response.failed" | "error") => return Err(AgentError::new(
                    "provider_failed",
                    "O provedor não conseguiu concluir esta resposta. O progresso foi preservado.",
                )),
                Some("response.incomplete") => return Err(protocol_error()),
                _ => {}
            }
        }
    }
}

fn completed(response: &Value) -> Result<Response, AgentError> {
    if response["status"] != "completed" {
        return Err(protocol_error());
    }
    let items = response["output"].as_array().ok_or_else(protocol_error)?;
    let mut output = vec![];
    let mut text = String::new();
    let mut summary = vec![];
    for item in items {
        match item["type"].as_str() {
            Some("message") => {
                let content = item["content"].as_array().ok_or_else(protocol_error)?;
                for part in content {
                    if let Some(value) = part["text"].as_str().or_else(|| part["refusal"].as_str()) { text.push_str(value); }
                }
                output.push(json!({"type":"message", "role":"assistant", "content":content}));
            },
            Some("reasoning") => {
                if let Some(parts) = item["summary"].as_array() {
                    summary.extend(parts.iter().filter_map(|part| part["text"].as_str().map(str::to_owned)));
                }
                // Encrypted replay data stays in Rust/the private journal, never in IPC.
                if item["encrypted_content"].is_string() { output.push(item.clone()); }
            },
            Some("function_call") => output.push(json!({"type":"function_call", "call_id":item["call_id"], "name":item["name"], "arguments":item["arguments"]})),
            Some("web_search_call") => output.push(item.clone()),
            _ => return Err(protocol_error()),
        }
    }
    let usage = response
        .get("usage")
        .filter(|value| value.is_object())
        .map(|value| Usage {
            input_tokens: value["input_tokens"].as_u64().unwrap_or(0),
            output_tokens: value["output_tokens"].as_u64().unwrap_or(0),
        });
    if text.is_empty()
        && !output
            .iter()
            .any(|item| item["type"] == "function_call" || item["type"] == "web_search_call")
    {
        return Err(protocol_error());
    }
    Ok(Response {
        output,
        text,
        summary: summary.join("\n\n"),
        usage,
    })
}
pub(super) fn tool_calls(output: &[Value]) -> Result<Vec<ToolCall>, AgentError> {
    let mut calls: Vec<ToolCall> = vec![];
    for item in output.iter().filter(|item| item["type"] == "function_call") {
        let id = item["call_id"]
            .as_str()
            .filter(|id| !id.is_empty() && id.len() <= 200)
            .ok_or_else(protocol_error)?;
        if calls.len() >= 16 || calls.iter().any(|call| call.id == id) {
            return Err(protocol_error());
        }
        let name = item["name"].as_str().ok_or_else(protocol_error)?;
        let args: Value =
            serde_json::from_str(item["arguments"].as_str().ok_or_else(protocol_error)?)
                .map_err(|_| protocol_error())?;
        if !args.is_object() {
            return Err(protocol_error());
        }
        calls.push(ToolCall {
            id: id.into(),
            name: name.into(),
            args,
            status: "pending".into(),
            output: String::new(),
            duration_ms: 0,
        });
    }
    Ok(calls)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{ApprovalMode, Mode};
    #[test]
    fn codex_lean_terminal_uses_only_completed_streamed_items() {
        let mut output = StreamOutput::default();
        output
            .item(&json!({"output_index":0,"item":{"type":"message"}}), false)
            .unwrap();
        assert!(output
            .finish(json!({"status":"completed","output":[]}))
            .is_err());
        let mut output = StreamOutput::default();
        output.item(&json!({"output_index":0,"item":{"type":"message","content":[{"type":"output_text","text":"OK"}]}}), true).unwrap();
        let response = output.finish(json!({"status":"completed","output":[],"usage":{"input_tokens":5,"output_tokens":1}})).unwrap();
        assert_eq!(response.text, "OK");
        assert_eq!(response.output.len(), 1);
        assert_eq!(response.usage.unwrap().output_tokens, 1);
    }
    #[tokio::test]
    #[ignore = "Requires an explicitly selected connected Codex account; sends one live diagnostic request"]
    async fn live_codex_response() {
        let account = std::env::var("JARVIS_LIVE_ACCOUNT")
            .expect("Set JARVIS_LIVE_ACCOUNT to a connected alias");
        let home = std::path::PathBuf::from(std::env::var_os("HOME").unwrap());
        let options = TurnOptions {
            account,
            model: "gpt-5.6-luna".into(),
            reasoning: Some("low".into()),
            mode: Mode::Plan, workflow: None,
            approval_mode: ApprovalMode::Manual,
        };
        let auth_options = options.clone();
        let credential = tokio::task::spawn_blocking(move || {
            crate::openai_codex::OpenAiCodexState::default().inference_credential(
                &crate::persistence::AppState::default(),
                &home,
                &auth_options.account,
                &auth_options.model,
                auth_options.reasoning.as_deref(),
            )
        })
        .await
        .unwrap()
        .unwrap();
        let (_send, signal) = watch::channel(false);
        let result = stream(
            &credential,
            &crate::library::new_id().unwrap(),
            &options,
            "Respond briefly in Portuguese.",
            vec![json!({"role":"user","content":"Responda somente OK."})],
            vec![],
            signal,
            |_| Ok(()),
        )
        .await;
        assert!(result.is_ok(), "Live response failed: {:?}", result.err());
    }
    #[test]
    fn sse_handles_utf8_crlf_multiline_and_fragmented_frames() {
        let bytes = "event: update\r\ndata: {\"type\":\"response.output_text.delta\",\r\ndata: \"delta\":\"Olá 👋\"}\r\n\r\ndata: [DONE]\n\n".as_bytes();
        let mut parser = Sse::default();
        let mut events = vec![];
        for byte in bytes {
            events.extend(parser.push(&[*byte]).unwrap());
        }
        assert_eq!(events.len(), 1);
        assert_eq!(events[0]["delta"], "Olá 👋");
    }
    #[test]
    fn completed_preserves_calls_and_encrypted_replay_without_exposing_private_reasoning() {
        let response = completed(&json!({"status":"completed", "output":[{"type":"reasoning", "id":"r1", "encrypted_content":"ciphertext", "summary":[{"text":"Resumo disponível"}]},{"type":"message", "id":"m1", "content":[{"type":"output_text", "text":"Olá"}]},{"type":"function_call", "call_id":"c1", "name":"read", "arguments":"{\"path\":\"a.txt\"}"}], "usage":{"input_tokens":42,"output_tokens":8}})).unwrap();
        assert_eq!(response.text, "Olá");
        assert_eq!(response.summary, "Resumo disponível");
        assert_eq!(response.output[0]["encrypted_content"], "ciphertext");
        assert!(response.output[1].get("id").is_none());
        assert_eq!(
            tool_calls(&response.output).unwrap()[0].args["path"],
            "a.txt"
        );
        assert_eq!(response.usage.unwrap().input_tokens, 42);
        assert!(completed(&json!({"status":"incomplete","output":[]})).is_err());
    }
    #[test]
    fn selected_model_reasoning_and_stateless_history_are_sent() {
        let options = TurnOptions {
            account: "a".into(),
            model: "chosen-model".into(),
            reasoning: Some("high".into()),
            mode: Mode::Plan, workflow: None,
            approval_mode: ApprovalMode::Manual,
        };
        let body = request_body(
            &options,
            "instructions",
            vec![json!({"role":"user","content":"hello"})],
            vec![],
            "session",
        );
        assert_eq!(body["model"], "chosen-model");
        assert_eq!(body["reasoning"]["effort"], "high");
        assert_eq!(body["store"], false);
        assert!(body.get("account").is_none());
    }
    #[tokio::test]
    async fn context_overflow_is_classified_for_http_and_sse_without_retrying_unrelated_errors() {
        use std::io::{Read, Write};
        for (status, body, expected) in [
            (400, r#"{"error":{"code":"context_length_exceeded","message":"private"}}"#, "context_overflow"),
            (413, r#"{"error":{"message":"Maximum context length exceeded"}}"#, "context_overflow"),
            (400, r#"{"error":{"code":"invalid_request","message":"invalid tool"}}"#, "provider_request"),
            (413, r#"{"error":{"message":"request too large"}}"#, "provider_unavailable"),
            (200, "data: {\"type\":\"response.failed\",\"response\":{\"error\":{\"code\":\"context_window_exceeded\"}}}\n\n", "context_overflow"),
            (200, "data: {\"type\":\"error\",\"code\":\"invalid_request\"}\n\n", "provider_failed"),
        ] {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let request = reqwest::Client::new().get(format!("http://{}", listener.local_addr().unwrap()));
            let server = std::thread::spawn(move || {
                let (mut stream, _) = listener.accept().unwrap();
                let mut bytes = [0; 4096]; let _ = stream.read(&mut bytes);
                write!(stream, "HTTP/1.1 {status} Test\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
            });
            let (_send, signal) = watch::channel(false);
            let error = receive(request, signal, |_| Ok(())).await.err().unwrap();
            assert_eq!(error.code, expected);
            assert!(!error.message.contains("private"));
            server.join().unwrap();
        }
    }

    #[tokio::test]
    async fn cancellation_drops_an_idle_http_request_promptly() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let request =
            reqwest::Client::new().get(format!("http://{}", listener.local_addr().unwrap()));
        let (send, signal) = watch::channel(false);
        let task = tokio::spawn(async move {
            receive(request, signal, |_| Ok(()))
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
}
