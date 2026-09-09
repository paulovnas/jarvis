use super::*;
use std::collections::HashSet;

#[derive(Default)]
pub(super) struct Stream {
    blocks: BTreeMap<u64, Value>,
    pending: HashSet<u64>,
    arguments: BTreeMap<u64, String>,
    started: bool,
    stop: Option<String>,
    usage: Option<Usage>,
}
impl Stream {
    pub fn event(
        &mut self,
        event: &Value,
        on_delta: &mut impl FnMut(Delta) -> Result<(), AgentError>,
    ) -> Result<bool, AgentError> {
        let index = || {
            event["index"]
                .as_u64()
                .filter(|n| *n < 128)
                .ok_or_else(protocol_error)
        };
        match event["type"].as_str() {
            Some("ping") => {}
            Some("message_start") => {
                if self.started {
                    return Err(protocol_error());
                }
                self.started = true;
                let usage = &event["message"]["usage"];
                self.usage = Some(Usage {
                    input_tokens: usage["input_tokens"]
                        .as_u64()
                        .unwrap_or(0)
                        .saturating_add(usage["cache_read_input_tokens"].as_u64().unwrap_or(0))
                        .saturating_add(usage["cache_creation_input_tokens"].as_u64().unwrap_or(0)),
                    output_tokens: usage["output_tokens"].as_u64().unwrap_or(0),
                    cache_read_tokens: usage["cache_read_input_tokens"].as_u64(),
                    cache_write_tokens: usage["cache_creation_input_tokens"].as_u64(),
                });
            }
            Some("content_block_start") => {
                let index = index()?;
                if !self.started || self.stop.is_some() || self.blocks.contains_key(&index) {
                    return Err(protocol_error());
                }
                let block = &event["content_block"];
                match block["type"].as_str() {
                    Some("text") => {
                        if let Some(text) = block["text"].as_str() {
                            on_delta(Delta::Text(text.into()))?;
                        }
                    }
                    Some("thinking") => {
                        if let Some(text) = block["thinking"].as_str() {
                            on_delta(Delta::Summary(text.into()))?;
                        }
                    }
                    Some("tool_use" | "redacted_thinking") => {}
                    _ => return Err(protocol_error()),
                }
                self.blocks.insert(index, block.clone());
                self.pending.insert(index);
            }
            Some("content_block_delta") => {
                let index = index()?;
                if !self.pending.contains(&index) {
                    return Err(protocol_error());
                }
                let block = self.blocks.get_mut(&index).ok_or_else(protocol_error)?;
                let delta = &event["delta"];
                let (kind, key) = match delta["type"].as_str() {
                    Some("text_delta") => ("text", "text"),
                    Some("thinking_delta") => ("thinking", "thinking"),
                    Some("signature_delta") => ("thinking", "signature"),
                    Some("input_json_delta") => ("tool_use", "partial_json"),
                    _ => return Err(protocol_error()),
                };
                if block["type"] != kind {
                    return Err(protocol_error());
                }
                let fragment = delta[key].as_str().ok_or_else(protocol_error)?;
                if key == "partial_json" {
                    self.arguments.entry(index).or_default().push_str(fragment);
                } else {
                    let previous = block[key].as_str().unwrap_or_default();
                    block[key] = json!(format!("{previous}{fragment}"));
                    if key == "text" {
                        on_delta(Delta::Text(fragment.into()))?;
                    }
                    if key == "thinking" {
                        on_delta(Delta::Summary(fragment.into()))?;
                    }
                }
            }
            Some("content_block_stop") => {
                let index = index()?;
                if !self.pending.remove(&index) {
                    return Err(protocol_error());
                }
                if let Some(arguments) = self.arguments.remove(&index) {
                    self.blocks.get_mut(&index).ok_or_else(protocol_error)?["input"] =
                        serde_json::from_str(&arguments).map_err(|_| protocol_error())?;
                }
            }
            Some("message_delta") => {
                if !self.started || !self.pending.is_empty() {
                    return Err(protocol_error());
                }
                if let Some(reason) = event["delta"]["stop_reason"].as_str() {
                    self.stop = Some(reason.into());
                }
                if let Some(usage) = &mut self.usage {
                    // Streaming counters are cumulative. Gateways may report final cache
                    // counters only in message_delta; replace, never sum snapshots.
                    let delta = &event["usage"];
                    let uncached = usage
                        .input_tokens
                        .saturating_sub(usage.cache_read_tokens.unwrap_or(0))
                        .saturating_sub(usage.cache_write_tokens.unwrap_or(0));
                    usage.cache_read_tokens = delta["cache_read_input_tokens"]
                        .as_u64()
                        .or(usage.cache_read_tokens);
                    usage.cache_write_tokens = delta["cache_creation_input_tokens"]
                        .as_u64()
                        .or(usage.cache_write_tokens);
                    usage.input_tokens = delta["input_tokens"]
                        .as_u64()
                        .unwrap_or(uncached)
                        .saturating_add(usage.cache_read_tokens.unwrap_or(0))
                        .saturating_add(usage.cache_write_tokens.unwrap_or(0));
                    if let Some(tokens) = event["usage"]["output_tokens"].as_u64() {
                        usage.output_tokens = tokens;
                    }
                }
            }
            Some("message_stop") => return Ok(true),
            _ => {}
        }
        Ok(false)
    }
    pub fn finish(self, scope: &Value) -> Result<Response, AgentError> {
        if !self.started
            || !self.pending.is_empty()
            || !matches!(
                self.stop.as_deref(),
                Some("end_turn" | "stop_sequence" | "tool_use")
            )
        {
            return Err(protocol_error());
        }
        let mut text = String::new();
        let mut summary = vec![];
        let mut calls = vec![];
        let mut thinking = vec![];
        for block in self.blocks.into_values() {
            match block["type"].as_str() {
                Some("text") => text.push_str(block["text"].as_str().ok_or_else(protocol_error)?),
                Some("thinking") => {
                    summary.push(block["thinking"].as_str().unwrap_or_default().to_owned());
                    // Retain unsigned gateway thinking privately; request shaping decides
                    // whether this endpoint permits replaying it.
                    thinking.push(block);
                }
                Some("redacted_thinking") => thinking.push(block),
                Some("tool_use") => calls.push(json!({"type":"function_call","call_id":block["id"],"name":block["name"],"arguments":block["input"].to_string()})),
                _ => return Err(protocol_error()),
            }
        }
        if (self.stop.as_deref() == Some("tool_use")) != !calls.is_empty() {
            return Err(protocol_error());
        }
        output(
            text,
            summary.join("\n\n"),
            calls,
            if thinking.is_empty() {
                json!({})
            } else {
                json!({"blocks":thinking})
            },
            scope,
            self.usage,
        )
    }
}
