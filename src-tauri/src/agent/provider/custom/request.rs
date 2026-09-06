use super::*;
use crate::openai_codex::custom::{Reasoning, TokenField};

pub(super) fn scope(config: &Config, options: &TurnOptions) -> Value {
    json!({"account":options.account,"model":options.model,"endpoint":config.base_url,"protocol":config.protocol})
}
fn metadata<'a>(item: &'a Value, scope: &Value) -> Option<&'a Value> {
    item.get("_custom").filter(|data| &data["scope"] == scope)
}
fn text(item: &Value) -> String {
    item["content"]
        .as_str()
        .map(str::to_owned)
        .unwrap_or_else(|| {
            item["content"]
                .as_array()
                .map_or_else(String::new, |parts| {
                    parts
                        .iter()
                        .filter_map(|p| p["text"].as_str().or_else(|| p["refusal"].as_str()))
                        .collect::<Vec<_>>()
                        .join("\n")
                })
        })
}
fn user_content(item: &Value, anthropic: bool, images: bool) -> Result<Value, AgentError> {
    let Some(parts) = item["content"].as_array() else {
        return Ok(item["content"].clone());
    };
    let mut result = vec![];
    for part in parts {
        if part["type"] == "input_image" {
            if !images {
                return Err(AgentError::new("custom_images", "Ative o suporte a imagens no cadastro deste modelo ou escolha outro modelo Vision."));
            }
            let url = part["image_url"].as_str().ok_or_else(protocol_error)?;
            if anthropic {
                let source = if let Some(data) = url.strip_prefix("data:") {
                    let (mime, data) = data.split_once(";base64,").ok_or_else(protocol_error)?;
                    json!({"type":"base64","media_type":mime,"data":data})
                } else {
                    json!({"type":"url","url":url})
                };
                result.push(json!({"type":"image","source":source}));
            } else {
                result.push(json!({"type":"image_url","image_url":{"url":url}}));
            }
        } else if let Some(text) = part["text"].as_str() {
            result.push(json!({"type":"text","text":text}));
        }
    }
    Ok(Value::Array(result))
}
fn assistant(messages: &mut Vec<Value>, anthropic: bool) -> &mut Value {
    if messages.last().is_none_or(|m| m["role"] != "assistant") {
        messages.push(if anthropic {
            json!({"role":"assistant","content":[]})
        } else {
            json!({"role":"assistant","content":""})
        });
    }
    messages.last_mut().expect("assistant was inserted")
}
fn blocks(message: &mut Value) -> &mut Vec<Value> {
    message["content"]
        .as_array_mut()
        .expect("constructed content array")
}

fn messages(
    input: &[Value],
    scope: &Value,
    model: &Model,
    anthropic: bool,
    replay_unsigned: bool,
) -> Result<Vec<Value>, AgentError> {
    let mut result = vec![];
    for item in input {
        match item["type"].as_str() {
            Some("reasoning") => {
                if let Some(meta) = metadata(item, scope) {
                    if anthropic {
                        if let Some(thinking) = meta["blocks"].as_array() {
                            for block in thinking {
                                if block["type"] != "thinking"
                                    || block["signature"].as_str().is_some_and(|s| !s.is_empty())
                                    || replay_unsigned
                                {
                                    let mut block = block.clone();
                                    if block["type"] == "thinking"
                                        && !block["signature"].is_string()
                                    {
                                        block["signature"] = json!("");
                                    }
                                    blocks(assistant(&mut result, true)).push(block);
                                }
                            }
                        }
                    } else {
                        let message = assistant(&mut result, false);
                        if meta["reasoning_content"].is_string() {
                            message["reasoning_content"] = meta["reasoning_content"].clone();
                        }
                        if meta["reasoning_details"].is_array() {
                            message["reasoning_details"] = meta["reasoning_details"].clone();
                        }
                    }
                }
            }
            Some("function_call") => {
                let message = assistant(&mut result, anthropic);
                if anthropic {
                    let arguments: Value = serde_json::from_str(
                        item["arguments"].as_str().ok_or_else(protocol_error)?,
                    )
                    .map_err(|_| protocol_error())?;
                    blocks(message).push(json!({"type":"tool_use","id":item["call_id"],"name":item["name"],"input":arguments}));
                } else {
                    if !message["tool_calls"].is_array() {
                        message["tool_calls"] = json!([]);
                    }
                    message["tool_calls"].as_array_mut().expect("constructed calls").push(json!({"id":item["call_id"],"type":"function","function":{"name":item["name"],"arguments":item["arguments"]}}));
                }
            }
            Some("function_call_output") => {
                if anthropic {
                    let block = json!({"type":"tool_result","tool_use_id":item["call_id"],"content":item["output"]});
                    if let Some(last) = result
                        .last_mut()
                        .filter(|m| m["role"] == "user" && m["content"].is_array())
                    {
                        blocks(last).push(block);
                    } else {
                        result.push(json!({"role":"user","content":[block]}));
                    }
                } else {
                    result.push(json!({"role":"tool","tool_call_id":item["call_id"],"content":item["output"]}));
                }
            }
            _ if item["role"] == "assistant" => {
                let content = text(item);
                if !content.is_empty() {
                    let message = assistant(&mut result, anthropic);
                    if anthropic {
                        blocks(message).push(json!({"type":"text","text":content}));
                    } else {
                        let previous = message["content"].as_str().unwrap_or_default();
                        message["content"] = json!(format!("{previous}{content}"));
                    }
                }
            }
            _ if item["role"] == "user"
                || item["role"] == "system"
                || item["role"] == "developer" =>
            {
                // Canonical history contains user data; instructions are a separate trusted argument.
                result.push(json!({"role":"user","content":user_content(item, anthropic, model.supports_images)?}));
            }
            _ => {}
        }
    }
    Ok(result)
}

pub(super) fn body(
    config: &Config,
    model: &Model,
    options: &TurnOptions,
    instructions: &str,
    input: Vec<Value>,
    tools: Vec<Value>,
) -> Result<Value, AgentError> {
    let scope = scope(config, options);
    let tools = if model.supports_tools { tools } else { vec![] };
    let mut body = match config.protocol {
        Protocol::OpenaiResponses => {
            let input: Vec<_> = input
                .into_iter()
                .filter_map(|mut item| {
                    if item["type"] == "reasoning"
                        && (!item["encrypted_content"].is_string()
                            || metadata(&item, &scope).is_none())
                    {
                        return None;
                    }
                    if item["type"] == "web_search_call" {
                        return None;
                    }
                    if let Some(map) = item.as_object_mut() {
                        map.retain(|key, _| key != "_custom" && !key.starts_with("_antigravity"));
                    }
                    Some(item)
                })
                .collect();
            // No Codex account, cache, beta or encrypted-reasoning headers are sent to gateways.
            json!({"model":model.id,"instructions":instructions,"input":input,"stream":true,"store":false,"max_output_tokens":model.max_output_tokens})
        }
        Protocol::OpenaiCompletions => {
            let mut messages = messages(&input, &scope, model, false, false)?;
            messages.insert(0, json!({"role":"system","content":instructions}));
            let mut body = json!({"model":model.id,"messages":messages,"stream":true,"stream_options":{"include_usage":true}});
            body[match config.token_field {
                TokenField::MaxTokens => "max_tokens",
                TokenField::MaxCompletionTokens => "max_completion_tokens",
            }] = json!(model.max_output_tokens);
            body
        }
        Protocol::AnthropicMessages => {
            json!({"model":model.id,"system":instructions,"messages":messages(&input, &scope, model, true, config.replay_unsigned_thinking)?,"stream":true,"max_tokens":model.max_output_tokens})
        }
    };
    if !tools.is_empty() {
        body["tools"] = Value::Array(tools.into_iter().map(|tool| match config.protocol {
            Protocol::OpenaiResponses => tool,
            Protocol::OpenaiCompletions => json!({"type":"function","function":{"name":tool["name"],"description":tool["description"],"parameters":tool["parameters"]}}),
            Protocol::AnthropicMessages => json!({"name":tool["name"],"description":tool["description"],"input_schema":tool["parameters"]}),
        }).collect());
    }
    let effort = options
        .reasoning
        .as_deref()
        .or(model.default_reasoning_level.as_deref());
    if let Some(effort) = effort {
        if !model.reasoning_levels.iter().any(|level| level == effort) {
            return Err(AgentError::new(
                "custom_reasoning",
                "O nível de raciocínio não está configurado para este modelo.",
            ));
        }
        let off = matches!(effort, "off" | "none");
        match model.reasoning {
            Reasoning::None => {}
            Reasoning::Effort if config.protocol == Protocol::OpenaiResponses => {
                body["reasoning"] = json!({"effort":if off { "none" } else { effort }});
            }
            Reasoning::Effort => {
                body["reasoning_effort"] = json!(if off { "none" } else { effort });
            }
            Reasoning::Openrouter => {
                body["reasoning"] = if off {
                    json!({"enabled":false})
                } else {
                    json!({"effort":effort})
                };
            }
            Reasoning::Deepseek => {
                body["thinking"] = json!({"type":if off { "disabled" } else { "enabled" }});
                if !off {
                    body["reasoning_effort"] = json!(effort);
                }
            }
            Reasoning::Budget => {
                body["thinking"] = if off {
                    json!({"type":"disabled"})
                } else {
                    json!({"type":"enabled","budget_tokens":model.thinking_budget})
                };
            }
            Reasoning::Adaptive => {
                body["thinking"] = json!({"type":if off { "disabled" } else { "adaptive" }});
                if !off {
                    body["output_config"] = json!({"effort":effort});
                }
            }
        }
    }
    Ok(body)
}
