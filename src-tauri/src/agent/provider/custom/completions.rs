use super::*;

#[derive(Default)]
pub(super) struct Stream {
    text: String,
    reasoning: String,
    details: BTreeMap<u64, Value>,
    calls: BTreeMap<u64, Value>,
    stop: Option<String>,
    usage: Option<Usage>,
}

impl Stream {
    pub fn finished(&self) -> bool {
        self.stop.is_some()
    }
    pub fn event(
        &mut self,
        event: &Value,
        on_delta: &mut impl FnMut(Delta) -> Result<(), AgentError>,
    ) -> Result<(), AgentError> {
        if let Some(usage) = event.get("usage").filter(|v| v.is_object()) {
            self.usage = Some(Usage {
                input_tokens: usage["prompt_tokens"].as_u64().unwrap_or(0),
                output_tokens: usage["completion_tokens"].as_u64().unwrap_or(0),
                cache_read_tokens: usage["prompt_tokens_details"]["cached_tokens"]
                    .as_u64()
                    .or_else(|| usage["prompt_cache_hit_tokens"].as_u64()),
                cache_write_tokens: usage["prompt_tokens_details"]["cache_write_tokens"].as_u64(),
            });
        }
        let Some(choices) = event["choices"].as_array() else {
            return if event["choices"].is_null() && event["usage"].is_object() {
                Ok(())
            } else {
                Err(protocol_error())
            };
        };
        for choice in choices {
            if choice["index"].as_u64().unwrap_or(0) != 0 {
                return Err(protocol_error());
            }
            let delta = &choice["delta"];
            if let Some(stopped) = &self.stop {
                // OpenRouter repeats the terminal choice in its trailing usage
                // envelope, including role:"assistant" and content:"".
                let changed_stop = !choice["finish_reason"].is_null()
                    && choice["finish_reason"].as_str() != Some(stopped.as_str());
                let payload = !delta.is_null()
                    && delta.as_object().is_none_or(|map| {
                        map.iter().any(|(key, value)| {
                            if key == "role" {
                                return !value.is_null() && value != "assistant";
                            }
                            match value {
                                Value::Null => false,
                                Value::String(s) => !s.is_empty(),
                                Value::Array(items) => !items.is_empty(),
                                _ => true,
                            }
                        })
                    });
                if changed_stop || payload {
                    return Err(protocol_error());
                }
                continue;
            }
            if let Some(content) = delta["content"]
                .as_str()
                .or_else(|| delta["refusal"].as_str())
            {
                self.text.push_str(content);
                on_delta(Delta::Text(content.into()))?;
            }
            if let Some(reasoning) = delta["reasoning_content"]
                .as_str()
                .or_else(|| delta["reasoning"].as_str())
            {
                self.reasoning.push_str(reasoning);
                on_delta(Delta::Summary(reasoning.into()))?;
            }
            if let Some(details) = delta["reasoning_details"].as_array() {
                for (position, detail) in details.iter().enumerate() {
                    let index = detail["index"].as_u64().unwrap_or(position as u64);
                    if index >= 128 {
                        return Err(protocol_error());
                    }
                    let stored = self.details.entry(index).or_insert_with(|| json!({}));
                    for (key, value) in detail.as_object().ok_or_else(protocol_error)? {
                        if matches!(key.as_str(), "text" | "summary" | "data") {
                            let fragment = value.as_str().ok_or_else(protocol_error)?;
                            let previous = stored[key].as_str().unwrap_or_default();
                            stored[key] = json!(format!("{previous}{fragment}"));
                        } else {
                            stored[key] = value.clone();
                        }
                    }
                }
            }
            if let Some(calls) = delta["tool_calls"].as_array() {
                for call in calls {
                    let index = call["index"]
                        .as_u64()
                        .filter(|n| *n < 16)
                        .ok_or_else(protocol_error)?;
                    let stored = self.calls.entry(index).or_insert_with(
                        || json!({"type":"function_call","call_id":"","name":"","arguments":""}),
                    );
                    for (field, value) in [
                        ("call_id", &call["id"]),
                        ("name", &call["function"]["name"]),
                    ] {
                        if let Some(value) = value.as_str().filter(|value| !value.is_empty()) {
                            let previous = stored[field].as_str().unwrap_or_default();
                            if !previous.is_empty() && previous != value {
                                return Err(protocol_error());
                            }
                            // Compatible gateways may repeat metadata on every argument delta.
                            stored[field] = json!(value);
                        }
                    }
                    if let Some(fragment) = call["function"]["arguments"].as_str() {
                        let previous = stored["arguments"].as_str().unwrap_or_default();
                        stored["arguments"] = json!(format!("{previous}{fragment}"));
                    }
                }
            }
            if let Some(reason) = choice["finish_reason"].as_str() {
                if self.stop.is_some() {
                    return Err(protocol_error());
                }
                if !matches!(reason, "stop" | "tool_calls") {
                    return Err(AgentError::new("provider_incomplete", "A resposta foi interrompida pelo provedor. Revise o limite de saída do modelo."));
                }
                self.stop = Some(reason.into());
            }
        }
        Ok(())
    }
    pub fn finish(self, scope: &Value) -> Result<Response, AgentError> {
        if self.stop.is_none()
            || (self.stop.as_deref() == Some("tool_calls")) != !self.calls.is_empty()
        {
            return Err(protocol_error());
        }
        let mut replay = json!({});
        if !self.reasoning.is_empty() {
            replay["reasoning_content"] = json!(self.reasoning);
        }
        if !self.details.is_empty() {
            replay["reasoning_details"] = json!(self.details.into_values().collect::<Vec<_>>());
        }
        output(
            self.text,
            self.reasoning,
            self.calls.into_values().collect(),
            replay,
            scope,
            self.usage,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(stream: &mut Stream, calls: Value) -> Result<(), AgentError> {
        stream.event(
            &json!({"choices":[{"index":0,"delta":{"tool_calls":calls}}]}),
            &mut |delta| {
                assert!(!matches!(delta, Delta::ToolReady(_)));
                Ok(())
            },
        )
    }

    fn finish(mut stream: Stream) -> Response {
        stream
            .event(
                &json!({"choices":[{"index":0,"delta":{},"finish_reason":"tool_calls"}]}),
                &mut |_| Ok(()),
            )
            .unwrap();
        stream.finish(&json!({})).unwrap()
    }

    #[test]
    fn repeated_tool_metadata_preserves_names_ids_and_interleaved_argument_fragments() {
        let mut stream = Stream::default();
        event(
            &mut stream,
            json!([
                {"index":0,"id":"call-a","function":{"name":"read_attachment","arguments":"{\"path\":\"assets/"}},
                {"index":1,"id":"call-b","function":{"name":"read_attachment","arguments":"{\"path\":\"assets/"}}
            ]),
        )
        .unwrap();
        event(
            &mut stream,
            json!([
                {"index":1,"id":"call-b","function":{"name":"read_attachment","arguments":"shot-3.png\"}"}},
                {"index":0,"id":"call-a","function":{"name":"read_attachment","arguments":"shot-0.png\"}"}}
            ]),
        )
        .unwrap();
        let response = finish(stream);
        let calls = response.tool_calls();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].id, "call-a");
        assert_eq!(calls[1].id, "call-b");
        for call in calls {
            assert_eq!(call.name, "read_attachment");
            assert_eq!(call.status, "pending");
        }
        assert_eq!(calls[0].args["path"], "assets/shot-0.png");
        assert_eq!(calls[1].args["path"], "assets/shot-3.png");
    }

    #[test]
    fn repeated_argument_fragments_are_preserved_verbatim() {
        let mut stream = Stream::default();
        for fragment in ["{\"content\":\"", "ha ", "ha ", "\"}"] {
            event(
                &mut stream,
                json!([{"index":0,"id":"call-a","function":{"name":"write","arguments":fragment}}]),
            )
            .unwrap();
        }
        assert_eq!(finish(stream).tool_calls()[0].args["content"], "ha ha ");
    }

    #[test]
    fn conflicting_tool_id_or_name_at_the_same_index_is_not_merged_or_dispatched() {
        for conflicting in [
            json!({"index":0,"id":"call-b","function":{"name":"read_attachment","arguments":"\"other.png\"}"}}),
            json!({"index":0,"id":"call-a","function":{"name":"write","arguments":"\"other.png\"}"}}),
        ] {
            let mut stream = Stream::default();
            event(
                &mut stream,
                json!([{"index":0,"id":"call-a","function":{"name":"read_attachment","arguments":"{\"path\":"}}]),
            )
            .unwrap();
            assert!(event(&mut stream, json!([conflicting])).is_err());
            assert!(stream.finish(&json!({})).is_err());
        }
    }
}
