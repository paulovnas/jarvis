use super::super::*;
use std::collections::BTreeMap;

#[derive(Default)]
struct Preview {
    kind: String,
    text: String,
    confirmed: bool,
}

/// The public journal remains the only UI history. A full assistant envelope
/// replaces its streamed preview rather than appending a second copy.
#[derive(Default)]
pub(super) struct Projection {
    message: Option<String>,
    started: Option<std::time::Instant>,
    previews: HashMap<String, BTreeMap<u64, Preview>>,
    envelopes: HashSet<String>,
    calls: Vec<ToolCall>,
    associated_calls: HashSet<String>,
    metrics: MessageMetrics,
}

#[derive(Default)]
struct TokenCounters {
    input: Option<u64>,
    output: Option<u64>,
    cache_read: Option<u64>,
    cache_write: Option<u64>,
}

impl TokenCounters {
    fn merge(&mut self, usage: &Value) {
        for (target, key) in [
            (&mut self.input, "input_tokens"),
            (&mut self.output, "output_tokens"),
            (&mut self.cache_read, "cache_read_input_tokens"),
            (&mut self.cache_write, "cache_creation_input_tokens"),
        ] {
            if let Some(value) = usage[key].as_u64() {
                *target = Some(value);
            }
        }
    }

    fn public_usage(&self) -> Option<Usage> {
        if self.input.is_none()
            && self.output.is_none()
            && self.cache_read.is_none()
            && self.cache_write.is_none()
        {
            return None;
        }
        Some(Usage {
            input_tokens: self
                .input
                .unwrap_or(0)
                .saturating_add(self.cache_read.unwrap_or(0))
                .saturating_add(self.cache_write.unwrap_or(0)),
            output_tokens: self.output.unwrap_or(0),
            cache_read_tokens: self.cache_read,
            cache_write_tokens: self.cache_write,
        })
    }
}

struct MeasuredMessage {
    id: String,
    model: String,
    started: Option<std::time::Instant>,
    duration_ms: u64,
    confirmed: bool,
    failure: Option<telemetry::FailureClass>,
    usage: TokenCounters,
}

#[derive(Default)]
struct MessageMetrics {
    trace: Option<telemetry::TraceContext>,
    pending: Option<MeasuredMessage>,
    recorded: HashSet<String>,
    last_model: Option<String>,
}

impl MessageMetrics {
    fn observe(&mut self, event: &Value, fallback_model: &str) {
        let stream = &event["event"];
        let message = if event["type"] == "assistant" {
            Some(&event["message"])
        } else if event["type"] == "stream_event" && stream["type"] == "message_start" {
            Some(&stream["message"])
        } else {
            None
        };
        if let Some(message) = message {
            if let Some(id) = message["id"].as_str() {
                if self
                    .pending
                    .as_ref()
                    .is_some_and(|pending| pending.id != id)
                {
                    self.flush();
                }
                if self.recorded.contains(id) {
                    return;
                }
                let pending = self.pending.get_or_insert_with(|| MeasuredMessage {
                    id: id.into(),
                    model: fallback_model.into(),
                    started: None,
                    duration_ms: 0,
                    confirmed: false,
                    failure: None,
                    usage: TokenCounters::default(),
                });
                if let Some(model) = message["model"].as_str().filter(|model| !model.is_empty()) {
                    pending.model = model.into();
                    self.last_model = Some(model.into());
                }
                if event["type"] == "stream_event" {
                    pending.started = Some(std::time::Instant::now());
                } else {
                    pending.confirmed = true;
                    if event.get("error").is_some_and(|error| !error.is_null()) {
                        pending.failure = Some(match event["error"].as_str() {
                            Some("authentication_failed") => {
                                telemetry::FailureClass::Authentication
                            }
                            Some("rate_limit") => telemetry::FailureClass::RateLimit,
                            Some("invalid_request") => telemetry::FailureClass::InvalidRequest,
                            Some("server_error") => telemetry::FailureClass::Unavailable,
                            _ => telemetry::FailureClass::Unknown,
                        });
                    }
                }
                pending.usage.merge(&message["usage"]);
                pending.duration_ms = pending
                    .started
                    .map_or(0, |started| started.elapsed().as_millis() as u64);
            }
        }
        if event["type"] == "stream_event" {
            if let Some(pending) = &mut self.pending {
                if stream["type"] == "message_delta" {
                    pending.usage.merge(&stream["usage"]);
                }
                if stream["type"] == "message_stop" {
                    pending.duration_ms = pending
                        .started
                        .map_or(0, |started| started.elapsed().as_millis() as u64);
                }
            }
        }
        if event["type"] == "result" {
            self.flush();
        }
    }

    fn take_events(&mut self) -> Vec<telemetry::Event> {
        let Some(message) = self.pending.take().filter(|message| message.confirmed) else {
            return vec![];
        };
        if !self.recorded.insert(message.id) {
            return vec![];
        }
        let model_id = telemetry::model_id(&message.model);
        // A CLI message is one observed logical generation. Native HTTP retries,
        // request bytes, advertised tools and time-to-first-token are not exposed.
        // Never infer them from envelopes or count split content blocks as requests.
        vec![
            telemetry::Event::ProviderRequest {
                provider: telemetry::ProviderKind::ClaudeCode,
                model_id: model_id.clone(),
                attempt: 1,
                input_items: 0,
                input_bytes: 0,
                advertised_tools: 0,
            },
            telemetry::Event::ProviderResponse {
                provider: telemetry::ProviderKind::ClaudeCode,
                model_id,
                attempt: 1,
                outcome: if message.failure.is_some() {
                    telemetry::Outcome::Failed
                } else {
                    telemetry::Outcome::Succeeded
                },
                duration_ms: message.duration_ms,
                first_event_ms: None,
                input_tokens: message.usage.input.map(|input| {
                    input
                        .saturating_add(message.usage.cache_read.unwrap_or(0))
                        .saturating_add(message.usage.cache_write.unwrap_or(0))
                }),
                output_tokens: message.usage.output,
                cache_read_tokens: message.usage.cache_read,
                cache_write_tokens: message.usage.cache_write,
                failure: message.failure,
            },
        ]
    }

    fn flush(&mut self) {
        let events = self.take_events();
        if let Some(trace) = &self.trace {
            for event in events {
                telemetry::record(trace, event);
            }
        }
    }
}

impl Drop for MessageMetrics {
    fn drop(&mut self) {
        // Preserve already confirmed generation metrics when the process ends
        // unexpectedly or the user cancels before the final result frame.
        self.flush();
    }
}

fn child(event: &Value) -> bool {
    event
        .get("parent_tool_use_id")
        .is_some_and(|id| !id.is_null())
}

pub(super) fn final_result(event: &Value) -> Option<Result<(), AgentError>> {
    if event["type"] != "result" || child(event) {
        return None;
    }
    Some(
        if event["is_error"] == true || event["subtype"] != "success" {
            let detail = event["errors"].as_array().map(|errors| errors.iter().filter_map(Value::as_str).collect::<Vec<_>>().join("\n"))
            .filter(|text| !text.is_empty()).or_else(|| event["result"].as_str().map(str::to_owned))
            .unwrap_or_else(|| "O Claude encerrou a execução sem confirmar sucesso. Retome pelo chat para continuar com o histórico preservado.".into());
            Err(AgentError::new("claude_execution", &detail))
        } else {
            Ok(())
        },
    )
}

fn step_for<'a>(turn: &'a mut StoredTurn, message: &str) -> &'a mut Step {
    let key = format!("claude:{message}");
    let index = turn
        .turn
        .steps
        .iter()
        .position(|step| step.context_id.as_deref() == Some(&key))
        .unwrap_or_else(|| {
            turn.turn.steps.push(Step {
                context_id: Some(key),
                ..Step::default()
            });
            turn.turn.steps.len() - 1
        });
    &mut turn.turn.steps[index]
}

pub(super) fn mapped_call(name: &str, args: Value) -> (String, Value) {
    let name = name.strip_prefix("mcp__jarvis__").unwrap_or(name);
    if name == "call_mcp_tool" {
        if let Some(external) = args["name"]
            .as_str()
            .filter(|name| name.starts_with("mcp_"))
        {
            return (external.into(), args["arguments"].clone());
        }
    }
    (name.into(), args)
}

impl Projection {
    pub(super) fn take_tool(
        &mut self,
        name: &str,
        args: Value,
        native_id: Option<&str>,
    ) -> Option<ToolCall> {
        let (name, args) = mapped_call(name, args);
        if let Some(id) = native_id.filter(|id| !id.is_empty()) {
            self.associated_calls.insert(id.into());
            self.calls.retain(|tool| tool.id != id);
            return Some(ToolCall {
                id: id.into(),
                name,
                args,
                status: "pending".into(),
                output: String::new(),
                duration_ms: 0,
            });
        }
        let index = self
            .calls
            .iter()
            .position(|tool| tool.name == name && tool.args == args)?;
        let tool = self.calls.remove(index);
        self.associated_calls.insert(tool.id.clone());
        Some(tool)
    }

    pub fn apply(&mut self, session: &Session, event: &Value) -> Result<(), AgentError> {
        if child(event) {
            return Ok(());
        }
        if event["type"] == "assistant" {
            let envelope = event["uuid"]
                .as_str()
                .map(str::to_owned)
                .unwrap_or_else(|| event["message"].to_string());
            if !self.envelopes.insert(envelope) {
                return Ok(());
            }
        }
        {
            let data = session.data.lock().map_err(|_| AgentError::internal())?;
            let turn = data.turns.last().ok_or_else(AgentError::internal)?;
            self.metrics
                .trace
                .get_or_insert_with(|| telemetry::trace(&session.id, &turn.turn.id));
            self.metrics.observe(event, &turn.turn.options.model);
        }
        if event["type"] == "stream_event" {
            let event = &event["event"];
            if event["type"] == "message_start" {
                self.message = event["message"]["id"].as_str().map(str::to_owned);
                self.started = Some(std::time::Instant::now());
            }
            if let Some(id) = &self.message {
                if event["type"] == "message_delta" {
                    if let Some(usage) = self
                        .metrics
                        .pending
                        .as_ref()
                        .and_then(|pending| pending.usage.public_usage())
                    {
                        session.update(false, |data| {
                            if let Some(turn) = data.turns.last_mut() {
                                step_for(turn, id).usage = Some(usage);
                            }
                        })?;
                    }
                }
                if event["type"] == "content_block_delta" {
                    let previews = self.previews.entry(id.clone()).or_default();
                    for kind in ["text", "thinking"] {
                        if let Some(text) = event["delta"][kind].as_str() {
                            let preview = previews
                                .entry(event["index"].as_u64().unwrap_or(0))
                                .or_default();
                            preview.kind = kind.into();
                            preview.text.push_str(text);
                        }
                    }
                    session.update(false, |data| {
                        if let Some(turn) = data.turns.last_mut() {
                            let step = step_for(turn, id);
                            render_previews(step, previews);
                            step.duration_ms = self
                                .started
                                .map_or(0, |started| started.elapsed().as_millis() as u64);
                        }
                    })?;
                }
            }
        } else if event["type"] == "assistant" {
            let message = &event["message"];
            let Some(id) = message["id"].as_str() else {
                return Ok(());
            };
            let blocks = message["content"].as_array().cloned().unwrap_or_default();
            let previews = self.previews.entry(id.into()).or_default();
            for (index, block) in blocks.iter().enumerate() {
                let Some(kind @ ("text" | "thinking")) = block["type"].as_str() else {
                    continue;
                };
                let Some(text) = block[kind].as_str() else {
                    continue;
                };
                // Claude emits one assistant envelope per completed block, often
                // sharing a message ID. Reconcile the matching streamed block,
                // retaining later blocks and authoritative unstreamed suffixes.
                let index = if blocks.len() > 1 {
                    index as u64
                } else {
                    previews
                        .iter()
                        .find(|(_, preview)| !preview.confirmed && preview.kind == kind)
                        .map(|(index, _)| *index)
                        .unwrap_or_else(|| {
                            previews
                                .last_key_value()
                                .map_or(0, |(index, _)| index.saturating_add(1))
                        })
                };
                previews.insert(
                    index,
                    Preview {
                        kind: kind.into(),
                        text: text.into(),
                        confirmed: true,
                    },
                );
            }
            session.update(true, |data| {
                if let Some(turn) = data.turns.last_mut() {
                    // A native MCP callback may arrive before its assistant
                    // envelope. Keep its already checkpointed tool in this step.
                    let mut existing_tools = Vec::new();
                    for block in blocks.iter().filter(|block| block["type"] == "tool_use") {
                        for step in &mut turn.turn.steps {
                            if let Some(index) =
                                step.tools.iter().position(|tool| block["id"] == tool.id)
                            {
                                existing_tools.push(step.tools.remove(index));
                                break;
                            }
                        }
                    }
                    let step = step_for(turn, id);
                    step.tools.extend(existing_tools);
                    render_previews(step, previews);
                    if let Some(usage) = self
                        .metrics
                        .pending
                        .as_ref()
                        .filter(|pending| pending.id == id)
                        .and_then(|pending| pending.usage.public_usage())
                    {
                        step.usage = Some(usage);
                    }
                    step.duration_ms = self
                        .started
                        .map_or(0, |started| started.elapsed().as_millis() as u64);
                    for block in blocks.iter().filter(|block| block["type"] == "tool_use") {
                        let (Some(id), Some(name)) = (block["id"].as_str(), block["name"].as_str())
                        else {
                            continue;
                        };
                        if step.tools.iter().any(|tool| tool.id == id) {
                            continue;
                        }
                        let (name, args) = mapped_call(name, block["input"].clone());
                        step.tools.push(ToolCall {
                            id: id.into(),
                            name,
                            args,
                            status: "pending".into(),
                            output: String::new(),
                            duration_ms: 0,
                        });
                    }
                    let text = step.text.clone();
                    if let Some(existing) = turn
                        .wire
                        .iter_mut()
                        .find(|item| item["_jarvis_claude_message"] == id)
                    {
                        existing["content"] = json!(text);
                    } else {
                        turn.wire.push(
                            json!({"role":"assistant","content":text,"_jarvis_claude_message":id}),
                        );
                    }
                }
            })?;
            for block in blocks.iter().filter(|block| block["type"] == "tool_use") {
                let (Some(id), Some(name)) = (block["id"].as_str(), block["name"].as_str()) else {
                    continue;
                };
                if self.associated_calls.contains(id) || self.calls.iter().any(|tool| tool.id == id)
                {
                    continue;
                }
                let (name, args) = mapped_call(name, block["input"].clone());
                self.calls.push(ToolCall {
                    id: id.into(),
                    name,
                    args,
                    status: "pending".into(),
                    output: String::new(),
                    duration_ms: 0,
                });
            }
        } else if event["type"] == "result" {
            if let Some(window) = context_window(event, self.metrics.last_model.as_deref()) {
                session.update(true, |data| {
                    if let Some(turn) = data.turns.last_mut() {
                        turn.turn.context_window = Some(window);
                    }
                })?;
            }
        }
        // User envelopes are transport acknowledgements/tool results, never a
        // replacement for the user message already persisted by Jarvis.
        Ok(())
    }
}

fn context_window(event: &Value, model: Option<&str>) -> Option<u64> {
    let models = event["modelUsage"].as_object()?;
    let usage = model.and_then(|model| models.get(model)).or_else(|| {
        (models.len() == 1)
            .then(|| models.values().next())
            .flatten()
    })?;
    usage["contextWindow"].as_u64().filter(|window| *window > 0)
}

fn render_previews(step: &mut Step, previews: &BTreeMap<u64, Preview>) {
    let text = |kind| {
        previews
            .values()
            .filter(|preview| preview.kind == kind)
            .map(|preview| preview.text.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    };
    step.text = text("text");
    step.summary = text("thinking");
}

pub(super) async fn start_tool(session: &Session, tool: &ToolCall) -> Result<(), AgentError> {
    session.update_async(|data| {
        let Some(turn) = data.turns.last_mut() else { return; };
        if let Some(existing) = turn.turn.steps.iter_mut().flat_map(|step| &mut step.tools).find(|existing| existing.id == tool.id) {
            existing.status = "running".into();
        } else {
            if turn.turn.steps.is_empty() { turn.turn.steps.push(Step::default()); }
            if let Some(step) = turn.turn.steps.last_mut() {
                let mut tool = tool.clone(); tool.status = "running".into(); step.tools.push(tool);
            }
        }
        turn.wire.push(json!({"type":"function_call", "call_id":tool.id,"name":tool.name,"arguments":tool.args.to_string()}));
    }).await
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_message_metrics_use_final_usage_once_across_split_envelopes() {
        let mut metrics = MessageMetrics::default();
        for event in [
            json!({"type":"stream_event","event":{"type":"message_start","message":{"id":"one","model":"private-model","usage":{"input_tokens":20,"cache_read_input_tokens":80,"cache_creation_input_tokens":5}}}}),
            json!({"type":"assistant","message":{"id":"one","usage":{"output_tokens":7},"content":[{"type":"text","text":"Private text"}]}}),
            json!({"type":"assistant","message":{"id":"one","usage":{"output_tokens":12},"content":[{"type":"tool_use","name":"write"}]}}),
            json!({"type":"stream_event","event":{"type":"message_delta","usage":{"output_tokens":19}}}),
        ] {
            metrics.observe(&event, "default");
        }
        let events = metrics.take_events();
        assert_eq!(events.len(), 2);
        assert!(matches!(
            events[0],
            telemetry::Event::ProviderRequest {
                provider: telemetry::ProviderKind::ClaudeCode,
                attempt: 1,
                ..
            }
        ));
        assert!(matches!(
            events[1],
            telemetry::Event::ProviderResponse {
                input_tokens: Some(105),
                output_tokens: Some(19),
                cache_read_tokens: Some(80),
                cache_write_tokens: Some(5),
                first_event_ms: None,
                outcome: telemetry::Outcome::Succeeded,
                ..
            }
        ));
        let serialized = serde_json::to_string(&events).unwrap();
        assert!(!serialized.contains("Private text"));
        assert!(!serialized.contains("private-model"));
        metrics.observe(
            &json!({"type":"assistant","message":{"id":"one","usage":{"output_tokens":7}}}),
            "default",
        );
        assert!(metrics.take_events().is_empty());
    }

    #[test]
    fn an_unconfirmed_native_stream_does_not_claim_a_successful_generation() {
        let mut metrics = MessageMetrics::default();
        metrics.observe(&json!({"type":"stream_event","event":{"type":"message_start","message":{"id":"partial","usage":{"input_tokens":3}}}}), "default");
        assert!(metrics.take_events().is_empty());
    }

    #[test]
    fn a_native_assistant_error_is_not_reported_as_provider_success() {
        let mut metrics = MessageMetrics::default();
        metrics.observe(&json!({"type":"assistant","error":"authentication_failed","message":{"id":"error","content":[{"type":"text","text":"Private provider error"}]}}), "default");
        assert!(matches!(
            metrics.take_events()[1],
            telemetry::Event::ProviderResponse {
                outcome: telemetry::Outcome::Failed,
                failure: Some(telemetry::FailureClass::Authentication),
                input_tokens: None,
                output_tokens: None,
                ..
            }
        ));
    }

    #[test]
    fn final_native_usage_and_context_window_are_visible_without_recounting_envelopes() {
        use crate::agent::tests::{options, session, Fixture};
        let fixture = Fixture::new();
        let session = session(&fixture);
        session
            .reserve("Check the files".into(), options(ApprovalMode::Yolo))
            .unwrap();
        let mut projection = Projection::default();
        for event in [
            json!({"type":"stream_event","event":{"type":"message_start","message":{"id":"one","model":"native-model","usage":{"input_tokens":20,"cache_read_input_tokens":80}}}}),
            json!({"type":"assistant","uuid":"block","message":{"id":"one","model":"native-model","content":[{"type":"text","text":"Done"}],"usage":{"output_tokens":4}}}),
            json!({"type":"stream_event","event":{"type":"message_delta","usage":{"output_tokens":10}}}),
            json!({"type":"assistant","uuid":"block","message":{"id":"one","usage":{"output_tokens":4}}}),
            json!({"type":"result","subtype":"success","modelUsage":{"native-model":{"contextWindow":200000}}}),
        ] {
            projection.apply(&session, &event).unwrap();
        }
        let snapshot = session.snapshot().unwrap();
        let turn = &snapshot.turns[0];
        assert_eq!(turn.steps.len(), 1);
        assert_eq!(turn.steps[0].usage.as_ref().unwrap().input_tokens, 100);
        assert_eq!(turn.steps[0].usage.as_ref().unwrap().output_tokens, 10);
        assert_eq!(turn.context_window, Some(200000));
        assert_eq!(projection.metrics.recorded.len(), 1);
        assert!(projection.metrics.take_events().is_empty());
    }

    #[test]
    fn native_context_window_uses_the_reported_model_without_guessing_between_models() {
        let result =
            json!({"modelUsage":{"small":{"contextWindow":1000},"large":{"contextWindow":2000}}});
        assert_eq!(context_window(&result, Some("large")), Some(2000));
        assert_eq!(context_window(&result, Some("alias")), None);
        assert_eq!(
            context_window(
                &json!({"modelUsage":{"actual":{"contextWindow":4000}}}),
                Some("alias")
            ),
            Some(4000)
        );
        assert_eq!(
            context_window(&json!({"modelUsage":{"actual":{"contextWindow":0}}}), None),
            None
        );
    }

    #[test]
    fn child_results_never_finish_or_fail_the_parent() {
        assert!(final_result(
            &json!({"type":"result","subtype":"success","parent_tool_use_id":"child"})
        )
        .is_none());
        assert!(final_result(
            &json!({"type":"result","is_error":true,"parent_tool_use_id":"child"})
        )
        .is_none());
        assert!(final_result(&json!({"type":"result","subtype":"success"}))
            .unwrap()
            .is_ok());
        assert!(final_result(
            &json!({"type":"result","subtype":"error_during_execution","errors":["failed"]})
        )
        .unwrap()
        .is_err());
    }
    #[test]
    fn dynamic_mcp_dispatch_cannot_address_a_native_publication_tool() {
        assert_eq!(
            mapped_call(
                "mcp__jarvis__call_mcp_tool",
                json!({"name":"mcp_notebook_read","arguments":{"id":1}})
            ),
            ("mcp_notebook_read".into(), json!({"id":1}))
        );
        assert_eq!(
            mapped_call("call_mcp_tool", json!({"name":"bash","arguments":{}})).0,
            "call_mcp_tool"
        );
    }
}
