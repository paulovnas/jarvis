//! Native messages/NDJSON projected onto the same validated tool-response contract.
use super::*;

pub(super) fn request(mut body: Value, model: &Model) -> Result<Value, AgentError> {
    let mut names = BTreeMap::new();
    for message in body["messages"].as_array_mut().ok_or_else(protocol_error)? {
        if let Some(parts) = message["content"].as_array() {
            let mut text = vec![];
            let mut images = vec![];
            for part in parts {
                if let Some(content) = part["text"].as_str() {
                    text.push(content);
                }
                if part["type"] == "image_url" {
                    let url = part["image_url"]["url"]
                        .as_str()
                        .ok_or_else(protocol_error)?;
                    let (header, data) = url.split_once(";base64,").filter(|(header, _)| header.starts_with("data:image/")).ok_or_else(|| AgentError::new("ollama_images", "O Ollama aceita imagens anexadas em base64. Anexe a imagem ao chat em vez de enviar uma URL externa."))?;
                    let _ = header;
                    images.push(data);
                }
            }
            let text = text.join("\n");
            let images = json!(images);
            message["content"] = json!(text);
            if images.as_array().is_some_and(|images| !images.is_empty()) {
                message["images"] = images;
            }
        }
        let message = message.as_object_mut().ok_or_else(protocol_error)?;
        if let Some(thinking) = message.remove("reasoning_content") {
            message.insert("thinking".into(), thinking);
        }
        message.remove("reasoning_details");
        if let Some(calls) = message.get_mut("tool_calls").and_then(Value::as_array_mut) {
            for call in calls {
                let name = call["function"]["name"]
                    .as_str()
                    .ok_or_else(protocol_error)?
                    .to_owned();
                let id = call["id"].as_str().ok_or_else(protocol_error)?.to_owned();
                let arguments: Value = serde_json::from_str(
                    call["function"]["arguments"]
                        .as_str()
                        .ok_or_else(protocol_error)?,
                )
                .map_err(|_| protocol_error())?;
                if !arguments.is_object() {
                    return Err(protocol_error());
                }
                names.insert(id, name.clone());
                *call = json!({"function":{"name":name,"arguments":arguments}});
            }
        }
        if message.get("role").is_some_and(|role| role == "tool") {
            let id = message.remove("tool_call_id").ok_or_else(protocol_error)?;
            let name = id
                .as_str()
                .and_then(|id| names.get(id))
                .ok_or_else(protocol_error)?;
            message.insert("tool_name".into(), json!(name));
        }
    }
    let limit = body["max_tokens"].clone();
    body.as_object_mut()
        .ok_or_else(protocol_error)?
        .retain(|key, _| {
            matches!(
                key.as_str(),
                "model" | "messages" | "stream" | "tools" | "think"
            )
        });
    body["options"] = json!({"num_ctx":model.context_window,"num_predict":limit});
    Ok(body)
}

#[derive(Default)]
pub(super) struct Stream {
    completion: completions::Stream,
    pending: Vec<u8>,
    calls: u64,
    pub received_event: bool,
}

impl Stream {
    pub fn push(
        &mut self,
        chunk: &[u8],
        on_delta: &mut impl FnMut(Delta) -> Result<(), AgentError>,
    ) -> Result<bool, AgentError> {
        self.pending.extend_from_slice(chunk);
        while let Some(end) = self.pending.iter().position(|byte| *byte == b'\n') {
            if end > super::super::MAX_EVENT {
                return Err(protocol_error());
            }
            let line: Vec<_> = self.pending.drain(..=end).collect();
            if line.iter().all(u8::is_ascii_whitespace) {
                continue;
            }
            let event = serde_json::from_slice(&line).map_err(|_| protocol_error())?;
            self.received_event = true;
            if self.event(&event, on_delta)? {
                return Ok(true);
            }
        }
        if self.pending.len() > super::super::MAX_EVENT {
            return Err(protocol_error());
        }
        Ok(false)
    }

    pub fn event(
        &mut self,
        event: &Value,
        on_delta: &mut impl FnMut(Delta) -> Result<(), AgentError>,
    ) -> Result<bool, AgentError> {
        if event.get("error").is_some_and(|error| !error.is_null()) {
            return Err(AgentError::new("provider_request", "O Ollama interrompeu a geração. Confira o modelo e os recursos de memória do servidor; o progresso foi preservado."));
        }
        if event["done"].as_bool().is_none()
            || event
                .get("message")
                .is_some_and(|message| !message.is_object() || message["role"] != "assistant")
        {
            return Err(protocol_error());
        }
        let mut delta = json!({});
        for (native, compatible) in [("content", "content"), ("thinking", "reasoning_content")] {
            if let Some(value) = event["message"].get(native) {
                if !value.is_string() {
                    return Err(protocol_error());
                }
                delta[compatible] = value.clone();
            }
        }
        if let Some(calls) = event["message"].get("tool_calls") {
            let calls = calls.as_array().ok_or_else(protocol_error)?;
            let mut projected = vec![];
            for call in calls {
                if self.calls >= 16 || !call["function"]["arguments"].is_object() {
                    return Err(protocol_error());
                }
                let name = call["function"]["name"]
                    .as_str()
                    .filter(|name| !name.is_empty())
                    .ok_or_else(protocol_error)?;
                let id = crate::library::new_id().map_err(|_| AgentError::internal())?;
                projected.push(json!({"index":self.calls,"id":id,"function":{"name":name,"arguments":call["function"]["arguments"].to_string()}}));
                self.calls += 1;
            }
            delta["tool_calls"] = json!(projected);
        }
        let done = event["done"] == true;
        let finish = if done {
            match event["done_reason"].as_str() {
                Some("stop") => Some(if self.calls > 0 { "tool_calls" } else { "stop" }),
                Some("length") => return Err(AgentError::new("provider_incomplete", "O Ollama atingiu o limite de saída antes de concluir. Ajuste o contexto ou a saída máxima deste modelo nas configurações.")),
                Some("load") => return Err(AgentError::new("provider_protocol", "O Ollama carregou o modelo sem gerar uma resposta. O progresso foi preservado; tente enviar uma nova mensagem.")),
                _ => return Err(protocol_error()),
            }
        } else {
            None
        };
        let mut compatible = json!({"choices":[{"index":0,"delta":delta,"finish_reason":finish}]});
        if done {
            compatible["usage"] = json!({"prompt_tokens":event["prompt_eval_count"],"completion_tokens":event["eval_count"],"prompt_tokens_details":{"cached_tokens":event["prompt_eval_cached_count"]}});
        }
        self.completion.event(&compatible, on_delta)?;
        Ok(done)
    }

    pub fn finish(self, scope: &Value) -> Result<Response, AgentError> {
        self.completion.finish(scope)
    }
}
