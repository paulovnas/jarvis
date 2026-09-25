use super::{cancelled, AgentError, ToolCall, TurnOptions, Usage};
use crate::openai_codex::{
    CodexCredential, ProviderModel, OPENAI_CODEX_BASE_URL, OPENAI_CODEX_CLIENT_VERSION,
};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, HashSet},
    time::Duration,
};
use tokio::sync::watch;

mod antigravity;
mod auth;
mod capabilities;
#[cfg(test)]
mod conformance;
mod custom;
mod events;
mod incremental;
pub(super) mod retry;

pub(super) use antigravity::grounded_search;
pub(super) use capabilities::ModelCapabilities;
const MAX_EVENT: usize = 4 * 1024 * 1024;
const MAX_STREAM: usize = 16 * 1024 * 1024;
pub(super) enum Delta {
    Text(String),
    Summary(String),
    Retry(Option<retry::Status>),
    Reset,
    /// Fully delimited call, never a fragment of function arguments.
    ToolReady(Box<ReadyCall>),
}
pub(super) struct ReadyCall {
    pub call: ToolCall,
    /// Exact completed provider prefix, including private replay signatures.
    pub envelope: Vec<Value>,
}
#[derive(Debug)]
pub(super) struct Response {
    pub output: Vec<Value>,
    pub text: String,
    pub summary: String,
    pub usage: Option<Usage>,
    calls: Vec<ToolCall>,
}

impl Response {
    pub(super) fn from_output(
        output: Vec<Value>,
        usage: Option<Usage>,
    ) -> Result<Self, AgentError> {
        let normalized = events::NormalizedOutput::parse(output, usage)?;
        Ok(Self {
            output: normalized.wire_output(),
            text: normalized.text,
            summary: normalized.summary,
            usage: normalized.usage,
            calls: normalized.calls,
        })
    }

    pub(super) fn tool_calls(&self) -> &[ToolCall] {
        &self.calls
    }
}

/// One transport session is retained for the complete Jarvis turn. Connection
/// pools, TLS state and provider retry identity are therefore reused across
/// model steps without leaking settings from a later turn.
pub(super) struct TurnSession {
    credential: CodexCredential,
    authentication: Option<auth::Authentication>,
    capabilities: std::sync::Arc<ModelCapabilities>,
    session_id: String,
    client: reqwest::Client,
    telemetry: super::telemetry::TraceContext,
    incremental_enabled: bool,
    incremental: tokio::sync::Mutex<incremental::Transport>,
}

impl TurnSession {
    pub(super) fn new(
        credential: CodexCredential,
        model: &ProviderModel,
        session_id: String,
        telemetry: super::telemetry::TraceContext,
    ) -> Result<Self, AgentError> {
        let capabilities = std::sync::Arc::new(ModelCapabilities::resolve(&credential, model));
        let incremental = incremental::Transport::traced(
            telemetry.clone(),
            super::telemetry::provider_kind(&credential),
        );
        Ok(Self {
            credential,
            authentication: None,
            capabilities,
            session_id,
            client: http_client()?,
            telemetry,
            incremental_enabled: false,
            incremental: tokio::sync::Mutex::new(incremental),
        })
    }

    pub(super) fn capabilities(&self) -> &std::sync::Arc<ModelCapabilities> {
        &self.capabilities
    }

    pub(super) fn set_incremental_transport(&mut self, enabled: bool) {
        self.incremental_enabled =
            enabled && self.capabilities.protocol == capabilities::WireProtocol::OpenAiResponses;
    }

    pub(super) fn set_authentication(
        &mut self,
        state: crate::persistence::AppState,
        oauth: crate::openai_codex::OpenAiCodexState,
        home: std::path::PathBuf,
        alias: String,
    ) {
        if self.credential.custom.is_none() {
            self.authentication = Some(auth::Authentication::new(
                state,
                oauth,
                home,
                alias,
                self.credential.clone(),
            ));
        }
    }

    pub(super) async fn current_credential(
        &self,
        signal: watch::Receiver<bool>,
    ) -> Result<CodexCredential, AgentError> {
        match &self.authentication {
            Some(auth) => auth.credential(false, signal).await,
            None => Ok(self.credential.clone()),
        }
    }

    pub(super) async fn stream(
        &self,
        step: &super::context_manager::StepContext,
        signal: watch::Receiver<bool>,
        mut on_delta: impl FnMut(Delta) -> Result<(), AgentError>,
    ) -> Result<Response, AgentError> {
        debug_assert!(!step.authorization().values.is_empty());
        if step.capabilities() != self.capabilities.as_ref() {
            return Err(AgentError::internal());
        }
        if !step.capabilities().tools && !step.tools().is_empty() {
            return Err(AgentError::new(
                "provider_tools_unsupported",
                "O modelo selecionado não aceita as ferramentas obrigatórias do Jarvis. Escolha um modelo com suporte a ferramentas.",
            ));
        }
        let input = provider_input(step.input().to_vec());
        if !step.capabilities().accepts_input(&input) {
            return Err(AgentError::new(
                "provider_images_unsupported",
                "O modelo selecionado não aceita imagens nesta conversa.",
            ));
        }
        if step.options().reasoning.as_deref().is_some_and(|effort| {
            !matches!(effort, "off" | "none") && !step.capabilities().reasoning.supported
        }) {
            return Err(AgentError::new(
                "provider_reasoning_unsupported",
                "O modelo selecionado não aceita configuração de raciocínio.",
            ));
        }
        let tools = ordered_tools(step.tools().to_vec());
        let credential = self.current_credential(signal.clone()).await?;
        let mut forward = |mut delta: Delta| {
            if let Delta::ToolReady(ready) = &mut delta {
                if let Some(config) = &self.credential.custom {
                    custom::scope_ready_output(config, step.options(), &mut ready.envelope);
                }
            }
            on_delta(delta)
        };
        if self.incremental_enabled {
            let request = if let Some(config) = &self.credential.custom {
                custom::incremental_request(
                    &self.client,
                    &credential,
                    config,
                    &self.session_id,
                    step.options(),
                    step.capabilities(),
                    step.instructions(),
                    input.clone(),
                    tools.clone(),
                )?
            } else {
                let body = request_body(
                    step.options(),
                    step.capabilities(),
                    step.instructions(),
                    input.clone(),
                    tools.clone(),
                    &self.session_id,
                );
                authenticated_request_with_client(
                    &self.client,
                    &credential,
                    &self.session_id,
                    &body,
                )?
                .build()
                .map_err(|_| AgentError::internal())?
            };
            let mut transport = self.incremental.lock().await;
            if let Some(mut response) = transport
                .attempt(request, signal.clone(), &mut forward)
                .await?
            {
                if let Some(config) = &self.credential.custom {
                    custom::scope_responses_output(config, step.options(), &mut response);
                }
                return Ok(response);
            }
        }
        retry::Request {
            client: self.client.clone(),
            credential: &credential,
            authentication: self.authentication.as_ref(),
            session_id: &self.session_id,
            options: step.options(),
            instructions: step.instructions(),
            capabilities: step.capabilities().clone(),
            input,
            tools,
            telemetry: self.telemetry.clone(),
        }
        .run(signal, forward, Duration::from_secs(2))
        .await
    }
}

fn http_client() -> Result<reqwest::Client, AgentError> {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(20))
        .timeout(Duration::from_secs(600))
        .build()
        .map_err(|_| AgentError::internal())
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
    emitted: u64,
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
    pub(super) fn push_bounded(
        &mut self,
        bytes: &[u8],
        limit: usize,
    ) -> Result<Vec<Value>, AgentError> {
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
        400 => AgentError::new("provider_request", "O provedor recusou a estrutura desta solicitação. O progresso foi preservado; tente continuar. Se a falha persistir, revise o modelo e o nível de raciocínio."),
        408 | 425 | 500..=599 => AgentError::new("provider_unavailable", &format!("HTTP {status} — {}. O provedor está temporariamente indisponível.", reqwest::StatusCode::from_u16(status).ok().and_then(|code| code.canonical_reason()).unwrap_or("Falha no servidor"))),
        _ => AgentError::new("provider_request", &format!("O provedor recusou a solicitação (HTTP {status}). Verifique o endpoint e a configuração do modelo.")),
    }
}
fn request_id(response: &reqwest::Response) -> Option<&str> {
    [
        "x-request-id",
        "request-id",
        "openai-request-id",
        "x-goog-request-id",
        "cf-ray",
    ]
    .into_iter()
    .find_map(|name| response.headers().get(name)?.to_str().ok())
}
fn upstream_code(value: &Value) -> Option<String> {
    let error = value
        .get("error")
        .or_else(|| {
            value
                .get("response")
                .and_then(|response| response.get("error"))
        })
        .unwrap_or(value);
    error["code"]
        .as_str()
        .or_else(|| error["type"].as_str())
        .map(str::to_owned)
        .or_else(|| error["code"].as_u64().map(|code| code.to_string()))
}
fn with_provider_metadata(
    mut error: AgentError,
    status: Option<u16>,
    upstream_code: Option<&str>,
    request_id: Option<&str>,
) -> AgentError {
    error.provider_metadata = Some(Box::new(crate::diagnostics::ProviderMetadata::new(
        status,
        upstream_code,
        request_id,
    )));
    error
}
fn with_response_request_id(mut error: AgentError, response: &reqwest::Response) -> AgentError {
    let Some(value) = request_id(response) else {
        return error;
    };
    let request_id = crate::diagnostics::ProviderMetadata::new(None, None, Some(value)).request_id;
    if let Some(metadata) = error.provider_metadata.as_mut() {
        if metadata.request_id.is_none() {
            metadata.request_id = request_id;
        }
    } else {
        error.provider_metadata = Some(Box::new(crate::diagnostics::ProviderMetadata {
            http_status: None,
            upstream_code: None,
            request_id,
        }));
    }
    error
}
fn http_failure(response: &reqwest::Response, upstream_code: Option<&str>) -> AgentError {
    let mut error = failure(response.status().as_u16());
    error.retry_after = response
        .headers()
        .get("retry-after")
        .and_then(|header| header.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|seconds| *seconds <= 86_400)
        .map(Duration::from_secs);
    with_provider_metadata(
        error,
        Some(response.status().as_u16()),
        upstream_code,
        request_id(response),
    )
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
fn event_failure(value: &Value) -> AgentError {
    let upstream_code = upstream_code(value);
    if context_overflow(value) {
        return with_provider_metadata(overflow_error(), None, upstream_code.as_deref(), None);
    }
    let error = value
        .get("error")
        .or_else(|| {
            value
                .get("response")
                .and_then(|response| response.get("error"))
        })
        .unwrap_or(value);
    if let Some(status) = error["code"]
        .as_u64()
        .and_then(|code| u16::try_from(code).ok())
    {
        return with_provider_metadata(
            failure(status),
            Some(status),
            upstream_code.as_deref(),
            None,
        );
    }
    let mapped_status = match error["code"]
        .as_str()
        .or_else(|| error["type"].as_str())
        .unwrap_or_default()
    {
        "invalid_request" | "invalid_request_error" | "invalid_argument" => Some(400),
        "authentication_error" | "invalid_api_key" => Some(401),
        "permission_error" | "permission_denied" => Some(403),
        "rate_limit_error" | "rate_limit_exceeded" => Some(429),
        "overloaded_error" | "server_error" | "internal_error" => Some(503),
        _ => None,
    };
    let failure = mapped_status.map_or_else(
        || {
            AgentError::new(
                "provider_failed",
                "O provedor não conseguiu concluir esta resposta. O progresso foi preservado.",
            )
        },
        failure,
    );
    with_provider_metadata(failure, mapped_status, upstream_code.as_deref(), None)
}
fn request_body(
    options: &TurnOptions,
    capabilities: &ModelCapabilities,
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
        "stream":true, "store":false, "prompt_cache_key":session_id,
    });
    if capabilities.replay.opaque_state {
        body["include"] = json!(["reasoning.encrypted_content"]);
    }
    if capabilities.tools && !tools.is_empty() {
        body["tools"] = Value::Array(tools);
        body["tool_choice"] = json!("auto");
        if capabilities.parallel_tool_calls {
            body["parallel_tool_calls"] = json!(true);
        }
    }
    if capabilities.reasoning.supported {
        if let Some(effort) = &options.reasoning {
            let mut reasoning = json!({"effort": if effort == "off" { "none" } else { effort }});
            if capabilities.reasoning.summaries {
                reasoning["summary"] = json!("auto");
            }
            body["reasoning"] = reasoning;
        }
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
    telemetry: &super::telemetry::TraceContext,
    signal: watch::Receiver<bool>,
    on_delta: impl FnMut(Delta) -> Result<(), AgentError>,
) -> Result<Response, AgentError> {
    let client = http_client()?;
    let capabilities = ModelCapabilities::resolve_for_options(credential, options);
    retry::Request {
        client,
        credential,
        authentication: None,
        session_id,
        options,
        capabilities,
        instructions,
        input: provider_input(input),
        tools: ordered_tools(tools),
        telemetry: telemetry.clone(),
    }
    .run(signal, on_delta, Duration::from_secs(2))
    .await
}

fn provider_input(input: Vec<Value>) -> Vec<Value> {
    input
        .into_iter()
        .map(|mut item| {
            if let Some(map) = item.as_object_mut() {
                // These fields make journal recovery and queued-message
                // deduplication durable, but provider input schemas reject
                // application-private properties on conversation items.
                map.retain(|key, _| !key.starts_with("_jarvis_"));
            }
            item
        })
        .collect()
}

fn ordered_tools(mut tools: Vec<Value>) -> Vec<Value> {
    // MCP discovery order is not contractual. Equivalent toolsets must keep the
    // same provider prefix across reconnects, turns and operating systems.
    tools.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
    tools
}

#[test]
fn equivalent_tool_catalogs_keep_the_same_cacheable_prefix() {
    let first = json!({"type":"function","name":"ctx_search","description":"Search","parameters":{"type":"object"}});
    let second = json!({"type":"function","name":"read","description":"Read","parameters":{"type":"object"}});
    assert_eq!(
        ordered_tools(vec![first.clone(), second.clone()]),
        ordered_tools(vec![second, first])
    );
}

#[allow(clippy::too_many_arguments)]
async fn stream_once(
    client: &reqwest::Client,
    credential: &CodexCredential,
    session_id: &str,
    options: &TurnOptions,
    capabilities: &ModelCapabilities,
    instructions: &str,
    input: Vec<Value>,
    tools: Vec<Value>,
    signal: watch::Receiver<bool>,
    on_delta: impl FnMut(Delta) -> Result<(), AgentError>,
) -> Result<Response, AgentError> {
    let provider = if credential.custom.is_some() {
        "custom"
    } else if credential.project_id.is_some() {
        "antigravity"
    } else {
        "openai_codex"
    };
    let result = if let Some(config) = &credential.custom {
        custom::stream_with_client(
            client,
            credential,
            config,
            session_id,
            options,
            capabilities,
            instructions,
            input,
            tools,
            signal,
            on_delta,
        )
        .await
    } else if credential.project_id.is_some() {
        antigravity::stream_with_client(
            client,
            credential,
            session_id,
            options,
            capabilities,
            instructions,
            input,
            tools,
            signal,
            on_delta,
        )
        .await
    } else {
        let body = request_body(
            options,
            capabilities,
            instructions,
            input,
            tools,
            session_id,
        );
        match authenticated_request_with_client(client, credential, session_id, &body) {
            Ok(request) => receive(request, signal, on_delta).await,
            Err(error) => Err(error),
        }
    };
    if let Err(error) = &result {
        if error.code == "context_overflow" || error.code.starts_with("provider_") {
            crate::diagnostics::record_provider_failure(
                provider,
                session_id,
                &error.code,
                error.provider_metadata.as_deref(),
            );
        }
    }
    result
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
    authenticated_request_with_client(&client, credential, session_id, body)
}

fn authenticated_request_with_client(
    client: &reqwest::Client,
    credential: &CodexCredential,
    session_id: &str,
    body: &Value,
) -> Result<reqwest::RequestBuilder, AgentError> {
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
                let value = serde_json::from_slice::<Value>(&bytes).ok();
                (
                    detail.contains("model")
                        && (detail.contains("model is not supported")
                            || detail
                                .contains("not supported when using codex with a chatgpt account")),
                    value.as_ref().is_some_and(context_overflow),
                    value.as_ref().and_then(upstream_code),
                )
            };
            let (unsupported, overflow, upstream_code) = tokio::select! {
                _ = cancelled(&mut signal) => return Err(AgentError::cancelled()),
                result = tokio::time::timeout(Duration::from_secs(5), read_error) => result.unwrap_or((false, false, None)),
            };
            if overflow {
                return Err(with_provider_metadata(
                    overflow_error(),
                    Some(response.status().as_u16()),
                    upstream_code.as_deref(),
                    request_id(&response),
                ));
            }
            if unsupported {
                return Err(with_provider_metadata(
                    AgentError::new(
                        "provider_model_unsupported",
                        "O modelo não é compatível com esta conta ChatGPT.",
                    ),
                    Some(response.status().as_u16()),
                    upstream_code.as_deref(),
                    request_id(&response),
                ));
            }
            return Err(http_failure(&response, upstream_code.as_deref()));
        }
        return Err(http_failure(&response, None));
    }
    let mut parser = Sse::default();
    let mut output = StreamOutput::default();
    let mut size = 0;
    loop {
        let chunk = tokio::select! {
            _ = cancelled(&mut signal) => return Err(AgentError::cancelled()),
            result = tokio::time::timeout(Duration::from_secs(120), response.chunk()) => result.map_err(|_| AgentError::new("provider_timeout", "O provedor ficou sem responder."))?.map_err(|_| protocol_error())?,
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
            if let Some(response_output) = stream_event(&mut output, &event, &mut on_delta)
                .map_err(|error| with_response_request_id(error, &response))?
            {
                return Ok(response_output);
            }
        }
    }
}

fn stream_event(
    output: &mut StreamOutput,
    event: &Value,
    on_delta: &mut impl FnMut(Delta) -> Result<(), AgentError>,
) -> Result<Option<Response>, AgentError> {
    match event["type"].as_str() {
        Some("response.output_item.added") => output.item(event, false)?,
        Some("response.output_item.done") => {
            output.item(event, true)?;
            while let Some(item) = output.done.get(&output.emitted) {
                let index = output.emitted;
                output.emitted += 1;
                if item["type"] != "function_call" {
                    continue;
                }
                let response = Response::from_output(vec![item.clone()], None)?;
                for call in response.calls {
                    on_delta(Delta::ToolReady(Box::new(ReadyCall {
                        call,
                        envelope: output
                            .done
                            .range(..=index)
                            .map(|(_, item)| item.clone())
                            .collect(),
                    })))?;
                }
            }
        }
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
        Some("response.reasoning_summary_part.done") => on_delta(Delta::Summary("\n\n".into()))?,
        Some("response.completed" | "response.done") => {
            return std::mem::take(output)
                .finish(event["response"].clone())
                .map(Some)
        }
        Some("response.failed" | "error") => return Err(event_failure(event)),
        Some("response.incomplete") => return Err(protocol_error()),
        _ => {}
    }
    Ok(None)
}

fn completed(response: &Value) -> Result<Response, AgentError> {
    if response["status"] != "completed" {
        return Err(protocol_error());
    }
    let items = response["output"].as_array().ok_or_else(protocol_error)?;
    let mut output = vec![];
    for item in items {
        match item["type"].as_str() {
            Some("message") => {
                let content = item["content"].as_array().ok_or_else(protocol_error)?;
                output.push(json!({"type":"message", "role":"assistant", "content":content}));
            },
            Some("reasoning") => {
                // Encrypted replay data stays in Rust/the private journal, never in IPC.
                if item["encrypted_content"].is_string() || item["summary"].is_array() { output.push(item.clone()); }
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
            cache_read_tokens: value["input_tokens_details"]["cached_tokens"].as_u64(),
            cache_write_tokens: value["input_tokens_details"]["cache_write_tokens"].as_u64(),
        });
    Response::from_output(output, usage)
}

#[test]
fn responses_cache_usage_is_a_breakdown_not_extra_input() {
    for details in [
        json!({}),
        json!({"cached_tokens":0}),
        json!({"cached_tokens":70,"cache_write_tokens":20}),
    ] {
        let response = completed(&json!({"status":"completed","output":[{"type":"message","content":[{"type":"output_text","text":"Done"}]}],"usage":{"input_tokens":100,"output_tokens":10,"input_tokens_details":details}})).unwrap();
        let usage = response.usage.unwrap();
        assert_eq!(usage.input_tokens, 100);
        assert_eq!(usage.cache_read_tokens, details["cached_tokens"].as_u64());
        assert_eq!(
            usage.cache_write_tokens,
            details["cache_write_tokens"].as_u64()
        );
    }
}
pub(super) fn tool_calls(output: &[Value]) -> Result<Vec<ToolCall>, AgentError> {
    let mut calls = Vec::new();
    let mut ids = HashSet::new();
    for item in output.iter().filter(|item| item["type"] == "function_call") {
        let call = events::parse_tool_call(item)?;
        if calls.len() >= 16 || !ids.insert(call.id.clone()) {
            return Err(protocol_error());
        }
        calls.push(call);
    }
    Ok(calls)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn out_of_order_completed_items_preserve_mutation_barriers() {
        let mut output = StreamOutput::default();
        let mut ready = Vec::new();
        stream_event(&mut output, &json!({"type":"response.output_item.done", "output_index":1, "item":{"type":"function_call", "call_id":"read", "name":"read", "arguments":"{\"path\":\"file.txt\"}"}}), &mut |delta| {
            if let Delta::ToolReady(call) = delta { ready.push(call); }
            Ok(())
        }).unwrap();
        assert!(ready.is_empty());
        stream_event(&mut output, &json!({"type":"response.output_item.done", "output_index":0, "item":{"type":"function_call", "call_id":"write", "name":"write", "arguments":"{\"path\":\"file.txt\",\"content\":\"changed\"}"}}), &mut |delta| {
            if let Delta::ToolReady(call) = delta { ready.push(call); }
            Ok(())
        }).unwrap();
        assert_eq!(
            ready
                .iter()
                .map(|item| item.call.name.as_str())
                .collect::<Vec<_>>(),
            ["write", "read"]
        );
        assert_eq!(ready[1].envelope.len(), 2);
    }

    #[test]
    fn completed_calls_arrive_before_terminal_frames_and_malformed_arguments_cannot_dispatch() {
        let mut output = StreamOutput::default();
        let mut ready = Vec::new();
        let mut emit = |delta| {
            if let Delta::ToolReady(call) = delta {
                ready.push(call);
            }
            Ok(())
        };
        stream_event(
            &mut output,
            &json!({"type":"response.output_item.added", "output_index":0}),
            &mut emit,
        )
        .unwrap();
        stream_event(
            &mut output,
            &json!({"type":"response.function_call_arguments.delta", "delta":"{\"path\":"}),
            &mut emit,
        )
        .unwrap();
        stream_event(&mut output, &json!({"type":"response.output_item.done", "output_index":0, "item":{"type":"function_call", "call_id":"read1", "name":"read", "arguments":"{\"path\":\"README.md\"}"}}), &mut emit).unwrap();
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0].call.args["path"], "README.md");
        let mut invalid = StreamOutput::default();
        let mut malformed = Vec::new();
        stream_event(
            &mut invalid,
            &json!({"type":"response.output_item.done", "output_index":0, "item":{"type":"function_call", "call_id":"bad", "name":"read", "arguments":"{"}}),
            &mut |delta| { if let Delta::ToolReady(ready) = delta { malformed.push(ready.call); } Ok(()) },
        ).unwrap();
        let runtime = crate::agent::tool_contract::Orchestrator::new(
            &crate::agent::tools::definitions(Mode::Build),
        );
        assert_eq!(malformed.len(), 1);
        assert!(runtime.preflight(&malformed[0]).is_err());
    }
    use crate::agent::{ApprovalMode, Mode};
    #[test]
    fn provider_input_strips_all_jarvis_journal_metadata() {
        let input = provider_input(vec![json!({
            "role": "user",
            "content": "Orientação adicional",
            "_jarvis_runtime": true,
            "_jarvis_auxiliary": true,
            "_jarvis_queue_id": "queued-message",
            "_custom": true,
            "_antigravity_model": "gemini-example"
        })]);

        assert_eq!(input[0]["role"], "user");
        assert_eq!(input[0]["content"], "Orientação adicional");
        assert!(input[0].get("_jarvis_runtime").is_none());
        assert!(input[0].get("_jarvis_auxiliary").is_none());
        assert!(input[0].get("_jarvis_queue_id").is_none());
        assert_eq!(input[0]["_custom"], true);
        assert_eq!(input[0]["_antigravity_model"], "gemini-example");
    }

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
            mode: Mode::Plan,
            workflow: None,
            custom_workflow_id: None,
            custom_agent_id: None,
            approval_mode: ApprovalMode::Manual,
            manual_validation: false,
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
        let trace = super::super::telemetry::trace("live-provider-test", "request");
        let result = stream(
            &credential,
            &crate::library::new_id().unwrap(),
            &options,
            "Respond briefly in Portuguese.",
            vec![json!({"role":"user","content":"Responda somente OK."})],
            vec![],
            &trace,
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
            mode: Mode::Plan,
            workflow: None,
            custom_workflow_id: None,
            custom_agent_id: None,
            approval_mode: ApprovalMode::Manual,
            manual_validation: false,
        };
        let credential = CodexCredential::new("", "", 0, "", None, None);
        let capabilities = ModelCapabilities::resolve_for_options(&credential, &options);
        let body = request_body(
            &options,
            &capabilities,
            "instructions",
            vec![json!({"role":"user","content":"hello"})],
            vec![json!({"type":"function","name":"read","parameters":{"type":"object"}})],
            "session",
        );
        assert_eq!(body["model"], "chosen-model");
        assert_eq!(body["reasoning"]["effort"], "high");
        assert_eq!(body["reasoning"]["summary"], "auto");
        assert_eq!(body["tool_choice"], "auto");
        assert_eq!(body["parallel_tool_calls"], true);
        assert_eq!(body["include"], json!(["reasoning.encrypted_content"]));
        assert_eq!(body["store"], false);
        assert!(body.get("account").is_none());
    }

    #[test]
    fn optional_codex_fields_are_omitted_when_the_step_does_not_use_them() {
        let mut options = TurnOptions {
            account: "a".into(),
            model: "text-model".into(),
            reasoning: None,
            mode: Mode::Build,
            workflow: None,
            custom_workflow_id: None,
            custom_agent_id: None,
            approval_mode: ApprovalMode::Manual,
            manual_validation: false,
        };
        let credential = CodexCredential::new("", "", 0, "", None, None);
        let capabilities = ModelCapabilities::resolve_for_options(&credential, &options);
        let body = request_body(
            &options,
            &capabilities,
            "instructions",
            vec![json!({"role":"user","content":"hello"})],
            vec![],
            "session",
        );
        for field in ["tools", "tool_choice", "parallel_tool_calls", "reasoning"] {
            assert!(body.get(field).is_none(), "unexpected field {field}");
        }
        options.reasoning = Some("none".into());
        let capabilities = ModelCapabilities::resolve_for_options(&credential, &options);
        let body = request_body(
            &options,
            &capabilities,
            "instructions",
            vec![json!({"role":"user","content":"hello"})],
            vec![],
            "session",
        );
        assert_eq!(body["reasoning"]["effort"], "none");
    }
    #[tokio::test]
    async fn context_overflow_is_classified_for_http_and_sse_without_retrying_unrelated_errors() {
        use std::io::{Read, Write};
        for (status, body, expected) in [
            (400, r#"{"error":{"code":"context_length_exceeded","message":"private"}}"#, "context_overflow"),
            (413, r#"{"error":{"message":"Maximum context length exceeded"}}"#, "context_overflow"),
            (400, r#"{"error":{"code":"invalid_request","message":"invalid tool"}}"#, "provider_request"),
            (413, r#"{"error":{"message":"request too large"}}"#, "provider_request"),
            (200, "data: {\"type\":\"response.failed\",\"response\":{\"error\":{\"code\":\"context_window_exceeded\"}}}\n\n", "context_overflow"),
            (200, "data: {\"type\":\"error\",\"code\":\"invalid_request\"}\n\n", "provider_request"),
        ] {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let request = reqwest::Client::new().get(format!("http://{}", listener.local_addr().unwrap()));
            let server = std::thread::spawn(move || {
                let (mut stream, _) = listener.accept().unwrap();
                let mut bytes = [0; 4096]; let _ = stream.read(&mut bytes);
                write!(stream, "HTTP/1.1 {status} Test\r\nContent-Length: {}\r\nX-Request-ID: req-safe-123\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
            });
            let (_send, signal) = watch::channel(false);
            let error = receive(request, signal, |_| Ok(())).await.err().unwrap();
            assert_eq!(error.code, expected);
            assert!(!error.message.contains("private"));
            let metadata = error.provider_metadata.as_deref().unwrap();
            assert_eq!(metadata.request_id.as_deref(), Some("req-safe-123"));
            if status != 200 {
                assert_eq!(metadata.http_status, Some(status));
            }
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
