use super::{
    cancelled, context_overflow, http_failure, overflow_error, protocol_error, AgentError,
    CodexCredential, Delta, Response, Sse, TurnOptions, Usage, MAX_STREAM,
};
use crate::openai_codex::custom::{AuthMode, Config, Model, Protocol};
use serde_json::{json, Value};
use std::{collections::BTreeMap, time::Duration};
use tokio::sync::watch;

mod completions;
mod messages;
mod request;
#[cfg(test)]
mod tests;

fn failed(event: &Value) -> AgentError {
    super::event_failure(event)
}
fn output(
    text: String,
    summary: String,
    calls: Vec<Value>,
    replay: Value,
    scope: &Value,
    usage: Option<Usage>,
) -> Result<Response, AgentError> {
    if text.is_empty() && calls.is_empty() {
        return Err(protocol_error());
    }
    let mut output = vec![];
    if replay.as_object().is_some_and(|map| !map.is_empty()) {
        let mut meta = replay;
        meta["scope"] = scope.clone();
        output.push(json!({"type":"reasoning","summary":[{"type":"summary_text","text":summary}],"_custom":meta}));
    }
    if !text.is_empty() {
        output.push(json!({"type":"message","role":"assistant","content":[{"type":"output_text","text":text}]}));
    }
    output.extend(calls);
    // Reject incomplete arguments before any tool can be dispatched.
    super::tool_calls(&output)?;
    Ok(Response {
        output,
        text,
        summary,
        usage,
    })
}

fn authenticated_request(
    credential: &CodexCredential,
    config: &Config,
    body: &Value,
) -> Result<reqwest::RequestBuilder, AgentError> {
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(20))
        .timeout(Duration::from_secs(600))
        .build()
        .map_err(|_| AgentError::internal())?;
    let endpoint = config.endpoint()?;
    let openrouter = endpoint.scheme() == "https"
        && endpoint.host_str() == Some("openrouter.ai")
        && endpoint.port_or_known_default() == Some(443);
    let mut request = client
        .post(endpoint)
        .header("accept", "text/event-stream")
        .json(body);
    if openrouter {
        // App attribution is transport metadata, independent of the model or agent role.
        request = request
            .header("HTTP-Referer", "https://github.com/paulovnas/jarvis")
            .header("X-OpenRouter-Title", "Jarvis")
            .header("X-OpenRouter-App-Visibility", "hidden");
    }
    request = match config.auth_mode {
        AuthMode::Bearer => request.bearer_auth(&credential.access),
        AuthMode::XApiKey => request.header("x-api-key", &credential.access),
    };
    if config.protocol == Protocol::AnthropicMessages {
        request = request.header("anthropic-version", "2023-06-01");
    }
    Ok(request)
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn stream(
    credential: &CodexCredential,
    config: &Config,
    options: &TurnOptions,
    instructions: &str,
    input: Vec<Value>,
    tools: Vec<Value>,
    signal: watch::Receiver<bool>,
    on_delta: impl FnMut(Delta) -> Result<(), AgentError>,
) -> Result<Response, AgentError> {
    config.validate()?;
    let model = config
        .models
        .iter()
        .find(|model| model.id == options.model)
        .ok_or_else(|| {
            AgentError::new("custom_model", "Cadastre este modelo no provedor Custom.")
        })?;
    if !model.supports_images
        && input.iter().any(|item| {
            item["content"]
                .as_array()
                .is_some_and(|parts| parts.iter().any(|part| part["type"] == "input_image"))
        })
    {
        return Err(AgentError::new(
            "custom_images",
            "Este modelo não está configurado para receber imagens.",
        ));
    }
    let body = request::body(config, model, options, instructions, input, tools)?;
    let request = authenticated_request(credential, config, &body)?;
    let scope = request::scope(config, options);
    if config.protocol == Protocol::OpenaiResponses {
        let mut response = super::receive(request, signal, on_delta)
            .await
            .map_err(|error| {
                if error.code == "provider_auth" {
                    AgentError::new(
                        "provider_auth",
                        "O endpoint recusou o acesso. Verifique a chave do provedor Custom.",
                    )
                } else {
                    error
                }
            })?;
        for item in &mut response.output {
            if item["type"] == "reasoning" {
                item["_custom"] = json!({"scope":scope});
            }
        }
        super::tool_calls(&response.output)?;
        return Ok(response);
    }
    receive(request, config.protocol, &scope, signal, on_delta).await
}

async fn receive(
    request: reqwest::RequestBuilder,
    protocol: Protocol,
    scope: &Value,
    mut signal: watch::Receiver<bool>,
    mut on_delta: impl FnMut(Delta) -> Result<(), AgentError>,
) -> Result<Response, AgentError> {
    let mut response = tokio::select! {
        _ = cancelled(&mut signal) => return Err(AgentError::cancelled()),
        response = request.send() => response.map_err(|_| AgentError::new("provider_network", "Não foi possível conectar ao endpoint Custom. Confira a URL e sua conexão."))?,
    };
    if !response.status().is_success() {
        let status = response.status().as_u16();
        let read = async {
            let mut bytes = vec![];
            while let Ok(Some(chunk)) = response.chunk().await {
                if bytes.len() + chunk.len() > 64 * 1024 {
                    break;
                }
                bytes.extend_from_slice(&chunk);
            }
            serde_json::from_slice::<Value>(&bytes).is_ok_and(|error| context_overflow(&error))
        };
        let overflow = tokio::select! {
            _ = cancelled(&mut signal) => return Err(AgentError::cancelled()),
            value = tokio::time::timeout(Duration::from_secs(5), read) => value.unwrap_or(false),
        };
        if overflow {
            return Err(overflow_error());
        }
        if matches!(status, 401 | 403) {
            return Err(AgentError::new("provider_auth", "O endpoint recusou o acesso. Verifique a chave e o tipo de autenticação do provedor Custom."));
        }
        return Err(http_failure(&response));
    }
    let mut parser = Sse::default();
    let mut completions = completions::Stream::default();
    let mut messages = messages::Stream::default();
    let mut size = 0;
    loop {
        // A short drain after finish_reason collects the optional trailing usage chunk.
        let idle = if completions.finished() { 3 } else { 120 };
        let chunk = tokio::select! {
            _ = cancelled(&mut signal) => return Err(AgentError::cancelled()),
            value = tokio::time::timeout(Duration::from_secs(idle), response.chunk()) => match value {
                Err(_) if completions.finished() => return completions.finish(scope),
                Err(_) => return Err(AgentError::new("provider_timeout", "O endpoint ficou sem responder.")),
                Ok(value) => value.map_err(|_| protocol_error())?,
            },
        };
        let Some(chunk) = chunk else {
            if protocol == Protocol::OpenaiCompletions
                && parser.pending.is_empty()
                && parser.data.is_empty()
            {
                return completions.finish(scope);
            }
            return Err(protocol_error());
        };
        size += chunk.len();
        if size > MAX_STREAM {
            return Err(protocol_error());
        }
        for event in parser.push(&chunk)? {
            if event.get("error").is_some_and(|error| !error.is_null()) || event["type"] == "error"
            {
                return Err(failed(&event));
            }
            if protocol == Protocol::OpenaiCompletions {
                completions.event(&event, &mut on_delta)?;
            } else if messages.event(&event, &mut on_delta)? {
                return messages.finish(scope);
            }
        }
        if parser.done {
            return if protocol == Protocol::OpenaiCompletions {
                completions.finish(scope)
            } else {
                Err(protocol_error())
            };
        }
    }
}
