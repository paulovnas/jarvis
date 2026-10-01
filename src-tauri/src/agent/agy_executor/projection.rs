use super::super::claude_executor::projection::mapped_call;
use super::super::*;

pub(super) type McpRoutes =
    std::collections::BTreeMap<String, std::collections::BTreeMap<String, String>>;

#[derive(Default)]
pub(super) struct Projection {
    pub routes: McpRoutes,
    native_session: Option<String>,
    last_response: Option<String>,
    calls: Vec<ToolCall>,
    associated_calls: HashSet<String>,
    started: HashMap<String, std::time::Instant>,
    cycle_steps: HashSet<String>,
    cycle: u64,
    last_result: Option<Value>,
    model: Option<String>,
}

pub(super) fn session_id(event: &Value) -> Option<&str> {
    event["conversation_id"]
        .as_str()
        .or_else(|| event[event["event"].as_str().unwrap_or_default()]["conversation_id"].as_str())
        .filter(|id| !id.is_empty())
}

pub(in crate::agent) fn reply(event: &Value) -> &str {
    event["result"]["response"].as_str().unwrap_or_default()
}

pub(in crate::agent) fn final_result(event: &Value) -> Option<Result<(), AgentError>> {
    if event["event"] != "result" {
        return None;
    }
    let result = &event["result"];
    Some(match result["status"].as_str() {
        Some("SUCCESS")
            if result["is_error"] != true
                && event["is_error"] != true
                && (result["error"].is_null() || result["error"] == "") =>
        {
            Ok(())
        }
        Some("CANCELED" | "CANCELLED" | "INTERRUPTED") => Err(AgentError::cancelled()),
        _ => {
            let detail = result["error"]
                .as_str()
                .or_else(|| result["error"]["message"].as_str())
                .filter(|detail| !detail.is_empty())
                .unwrap_or("O Antigravity CLI encerrou sem confirmar sucesso. Retome pelo chat com o histórico preservado.");
            let message = detail.trim().to_ascii_lowercase();
            let exhausted = result["error"]["code"] == "provider_retry_exhausted"
                || result["error_code"] == "provider_retry_exhausted"
                || message == "provider_retry_exhausted"
                || message.starts_with("provider retries exhausted")
                || message.starts_with("model provider retries exhausted");
            Err(AgentError::new(
                if exhausted {
                    "provider_retry_exhausted"
                } else {
                    "agy_execution"
                },
                detail,
            ))
        }
    })
}

fn step_for<'a>(turn: &'a mut StoredTurn, key: &str) -> &'a mut Step {
    let index = turn
        .turn
        .steps
        .iter()
        .position(|step| step.context_id.as_deref() == Some(key))
        .unwrap_or_else(|| {
            turn.turn.steps.push(Step {
                context_id: Some(key.into()),
                ..Step::default()
            });
            turn.turn.steps.len() - 1
        });
    &mut turn.turn.steps[index]
}

fn wire_text(turn: &mut StoredTurn, key: &str, text: &str) {
    if text.is_empty() {
        return;
    }
    if let Some(existing) = turn
        .wire
        .iter_mut()
        .find(|item| item["_jarvis_agy_step"] == key)
    {
        existing["content"] = json!(text);
    } else {
        turn.wire
            .push(json!({"role":"assistant", "content":text, "_jarvis_agy_step":key}));
    }
}

fn usage(value: &Value, previous: Option<&Usage>) -> Option<Usage> {
    let input = value["input_tokens"].as_u64();
    let output = value["output_tokens"].as_u64();
    let cache = value["cache_read_tokens"].as_u64();
    if input.is_none() && output.is_none() && cache.is_none() {
        return None;
    }
    let previous_cache = previous.and_then(|usage| usage.cache_read_tokens);
    let cache = cache.or(previous_cache);
    Some(Usage {
        input_tokens: input
            .unwrap_or_else(|| {
                previous.map_or(0, |usage| {
                    usage
                        .input_tokens
                        .saturating_sub(previous_cache.unwrap_or(0))
                })
            })
            .saturating_add(cache.unwrap_or(0)),
        output_tokens: output.unwrap_or_else(|| previous.map_or(0, |usage| usage.output_tokens)),
        cache_read_tokens: cache,
        cache_write_tokens: None,
    })
}

pub(super) fn routed_name(routes: &McpRoutes, server: &str, name: &str) -> Option<String> {
    routes.get(server)?.get(name).cloned()
}

fn native_tool(step: &Value, id: &str, routes: &McpRoutes) -> Option<ToolCall> {
    let parameters = &step["tool_info"]["parameters"];
    if step["step_type"] != "tool" {
        return None;
    }
    let server = parameters["ServerName"].as_str()?;
    let name = parameters["ToolName"]
        .as_str()
        .filter(|name| !name.is_empty())?;
    let args = match &parameters["Arguments"] {
        Value::String(text) => serde_json::from_str(text).ok()?,
        value => value.clone(),
    };
    if !args.is_object() {
        return None;
    }
    let (name, args) = if server == "jarvis" {
        mapped_call(name, args)
    } else {
        (routed_name(routes, server, name)?, args)
    };
    Some(ToolCall {
        id: id.into(),
        name,
        args,
        status: "pending".into(),
        output: String::new(),
        duration_ms: 0,
    })
}

impl Projection {
    pub(super) fn take_tool(
        &mut self,
        name: &str,
        args: Value,
        native_id: Option<&str>,
    ) -> Option<ToolCall> {
        let (name, args) = mapped_call(name, args);
        let tool = self
            .calls
            .iter()
            .find(|tool| {
                tool.name == name
                    && tool.args == args
                    && native_id.map_or_else(
                        || !self.associated_calls.contains(&tool.id),
                        |id| id == tool.id,
                    )
            })?
            .clone();
        self.associated_calls.insert(tool.id.clone());
        Some(tool)
    }

    pub(super) fn apply(&mut self, session: &Session, event: &Value) -> Result<(), AgentError> {
        if let Some(id) = session_id(event) {
            if self.native_session.as_deref() != Some(id) {
                self.last_response = None;
            }
            self.native_session = Some(id.into());
        }
        if event["event"] == "init" {
            self.model = event["init"]["model"].as_str().map(str::to_owned);
            return Ok(());
        }
        let Some(native_session) = self.native_session.as_deref() else {
            return Ok(());
        };
        if event["event"] == "step_update" {
            let native = &event["step_update"];
            let Some(index) = native["step_index"].as_u64() else {
                return Ok(());
            };
            let key = format!("agy:{native_session}:{index}");
            let started = self
                .started
                .entry(key.clone())
                .or_insert_with(std::time::Instant::now);
            let duration_ms = started.elapsed().as_millis() as u64;
            let tool = native_tool(native, &key, &self.routes);
            let dispatched =
                if let Some(tool) = &tool {
                    self.associated_calls.contains(&tool.id) || {
                        let data = session.data.lock().map_err(|_| AgentError::internal())?;
                        data.turns.iter().any(|turn| {
                            turn.wire.iter().any(|item| {
                                item["type"] == "function_call" && item["call_id"] == tool.id
                            }) || turn.turn.steps.iter().flat_map(|step| &step.tools).any(
                                |existing| existing.id == tool.id && existing.status != "pending",
                            )
                        })
                    }
                } else {
                    false
                };
            if let Some(tool) = &tool {
                if let Some(existing) = self
                    .calls
                    .iter_mut()
                    .find(|existing| existing.id == tool.id)
                {
                    // AGY can publish a parameter preview before the complete
                    // request. Only unassociated previews may change identity
                    // data; an executed callback keeps its original receipt.
                    if !dispatched {
                        existing.name.clone_from(&tool.name);
                        existing.args.clone_from(&tool.args);
                    }
                } else {
                    self.calls.push(tool.clone());
                }
            }
            let is_response = native["step_type"] == "agent_response";
            if is_response {
                self.last_response = Some(key.clone());
                self.cycle_steps.insert(key.clone());
            }
            if !is_response && tool.is_none() {
                return Ok(());
            }
            session.update(native["state"] == "DONE" || tool.is_some(), |data| {
                let Some(turn) = data.turns.last_mut() else {
                    return;
                };
                // The MCP bridge owns tool effects and confirmed outputs. Native
                // notifications only locate the durable receipt in the journal.
                if let Some(tool) = &tool {
                    if let Some(existing) = turn
                        .turn
                        .steps
                        .iter_mut()
                        .flat_map(|step| &mut step.tools)
                        .find(|existing| existing.id == tool.id)
                    {
                        if !dispatched && existing.status == "pending" {
                            existing.name.clone_from(&tool.name);
                            existing.args.clone_from(&tool.args);
                        }
                    } else {
                        step_for(turn, &key).tools.push(tool.clone());
                    }
                }
                let step = step_for(turn, &key);
                if is_response {
                    if let Some(text) = native["text_delta"].as_str() {
                        step.text.push_str(text);
                    }
                    // Native thinking is an optional cumulative snapshot, never
                    // inferred from token usage or local-harness protobuf deltas.
                    if let Some(thinking) = native["thinking"].as_str() {
                        step.summary = thinking.into();
                    }
                    step.duration_ms = duration_ms;
                    if let Some(usage) = usage(&native["usage"], step.usage.as_ref()) {
                        step.usage = Some(usage);
                    }
                    if native["state"] == "DONE" {
                        let text = step.text.clone();
                        wire_text(turn, &key, &text);
                    }
                }
            })?;
        } else if event["event"] == "result" {
            if self.cycle_steps.is_empty() && self.last_result.as_ref() == Some(event) {
                return Ok(());
            }
            self.last_result = Some(event.clone());
            self.cycle = self.cycle.saturating_add(1);
            let final_text = reply(event);
            let aggregate = usage(&event["result"]["usage"], None);
            let prefix = format!("agy:{native_session}:");
            let last_response = self.last_response.take();
            let cycle_steps = std::mem::take(&mut self.cycle_steps);
            session.update(true, |data| {
                let Some(turn) = data.turns.last_mut() else {
                    return;
                };
                let last = last_response.as_deref().and_then(|key| {
                    turn.turn
                        .steps
                        .iter()
                        .find(|step| step.context_id.as_deref() == Some(key))
                });
                let key = last
                    .filter(|step| {
                        final_text.is_empty()
                            || final_text.starts_with(&step.text)
                            || step.text.starts_with(final_text)
                    })
                    .and_then(|step| step.context_id.clone())
                    .unwrap_or_else(|| format!("{prefix}result:{}", self.cycle));
                // Final usage covers the native run. Replacing provisional step
                // counts avoids adding that same usage twice in the public totals.
                if aggregate.is_some() {
                    for step in &mut turn.turn.steps {
                        if step
                            .context_id
                            .as_ref()
                            .is_some_and(|id| cycle_steps.contains(id))
                        {
                            step.usage = None;
                        }
                    }
                }
                let step = step_for(turn, &key);
                if !final_text.is_empty() && !step.text.starts_with(final_text) {
                    step.text = final_text.into();
                }
                if let Some(usage) = aggregate.clone() {
                    step.usage = Some(usage);
                }
                let text = step.text.clone();
                wire_text(turn, &key, &text);
            })?;
            let data = session.data.lock().map_err(|_| AgentError::internal())?;
            let turn = data.turns.last().ok_or_else(AgentError::internal)?;
            let result = final_result(event).unwrap_or(Ok(()));
            // The CLI exposes aggregate usage, not HTTP request/retry counts.
            // Record its observed result without inventing ProviderRequest events.
            telemetry::record(
                &telemetry::trace(&session.id, &turn.turn.id),
                telemetry::Event::ProviderResponse {
                    provider: telemetry::ProviderKind::AntigravityCli,
                    model_id: telemetry::model_id(
                        self.model.as_deref().unwrap_or(&turn.turn.options.model),
                    ),
                    attempt: 1,
                    outcome: match &result {
                        Ok(()) => telemetry::Outcome::Succeeded,
                        Err(error) if error.code == "cancelled" => telemetry::Outcome::Cancelled,
                        Err(_) => telemetry::Outcome::Failed,
                    },
                    duration_ms: event["result"]["duration_seconds"]
                        .as_f64()
                        .filter(|seconds| seconds.is_finite() && *seconds >= 0.0)
                        .map_or(0, |seconds| (seconds * 1000.0) as u64),
                    first_event_ms: None,
                    input_tokens: aggregate.as_ref().map(|usage| usage.input_tokens),
                    output_tokens: aggregate.as_ref().map(|usage| usage.output_tokens),
                    cache_read_tokens: aggregate.as_ref().and_then(|usage| usage.cache_read_tokens),
                    cache_write_tokens: None,
                    failure: result.as_ref().err().map(telemetry::failure_class),
                },
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::tests::{options, session, Fixture};

    fn init(id: &str) -> Value {
        json!({"event":"init","conversation_id":id,"init":{"model":"native-model"}})
    }
    fn tool(session: &str, index: u64, args: Value) -> Value {
        json!({"event":"step_update","step_update":{"conversation_id":session,"step_index":index,"state":"ACTIVE","step_type":"tool","tool_name":"call_mcp_tool","tool_info":{"name":"call_mcp_tool","parameters":{"ServerName":"jarvis","ToolName":"read","Arguments":args}}}})
    }

    #[test]
    fn original_mcp_names_correlate_to_scoped_jarvis_receipts_without_native_tool_access() {
        let fixture = Fixture::new();
        let session = session(&fixture);
        session
            .reserve("List notebooks".into(), options(ApprovalMode::Yolo))
            .unwrap();
        let routes = McpRoutes::from([
            (
                "gemini-notebook-mcp".into(),
                std::collections::BTreeMap::from([(
                    "list_notebooks".into(),
                    "mcp_notebook_list_hash".into(),
                )]),
            ),
            (
                "other".into(),
                std::collections::BTreeMap::from([(
                    "list_notebooks".into(),
                    "mcp_other_list_hash".into(),
                )]),
            ),
        ]);
        let mut projection = Projection {
            routes,
            ..Projection::default()
        };
        projection.apply(&session, &init("native")).unwrap();
        for (index, server, canonical) in [
            (32, "gemini-notebook-mcp", "mcp_notebook_list_hash"),
            (33, "other", "mcp_other_list_hash"),
        ] {
            let event = json!({"event":"step_update","step_update":{"conversation_id":"native","step_index":index,"state":"ACTIVE","step_type":"tool","tool_name":"call_mcp_tool","tool_info":{"parameters":{"ServerName":server,"ToolName":"list_notebooks","Arguments":{}}}}});
            projection.apply(&session, &event).unwrap();
            let receipt = projection.take_tool(canonical, json!({}), None).unwrap();
            assert_eq!(receipt.id, format!("agy:native:{index}"));
            assert!(projection.take_tool(canonical, json!({}), None).is_none());
        }
        assert!(routed_name(&projection.routes, "gemini-notebook-mcp", "bash").is_none());
        assert!(routed_name(&projection.routes, "unknown", "list_notebooks").is_none());
        let (name, args) = mapped_call(
            "execute_mcp_tool",
            json!({"name":"mcp_notebook_list_hash","arguments":{}}),
        );
        assert_eq!((name.as_str(), args), ("mcp_notebook_list_hash", json!({})));
        assert_eq!(
            mapped_call("execute_mcp_tool", json!({"name":"bash","arguments":{}})).0,
            "execute_mcp_tool"
        );
    }

    #[test]
    fn streamed_text_and_native_thinking_survive_without_duplicate_final_text() {
        let fixture = Fixture::new();
        let session = session(&fixture);
        session
            .reserve("Inspect files".into(), options(ApprovalMode::Yolo))
            .unwrap();
        let mut projection = Projection::default();
        for event in [
            init("native"),
            json!({"event":"step_update","step_update":{"conversation_id":"native","step_index":1,"state":"ACTIVE","step_type":"agent_response","text_delta":"Done","thinking":"Inspecting"}}),
            json!({"event":"step_update","step_update":{"step_index":1,"state":"DONE","step_type":"agent_response","text_delta":".","thinking":"Inspecting files"}}),
            json!({"event":"result","result":{"conversation_id":"native","status":"SUCCESS","response":"Done."}}),
        ] {
            projection.apply(&session, &event).unwrap();
        }
        let snapshot = session.snapshot().unwrap();
        assert_eq!(snapshot.turns[0].steps.len(), 1);
        assert_eq!(snapshot.turns[0].steps[0].text, "Done.");
        assert_eq!(snapshot.turns[0].steps[0].summary, "Inspecting files");
        assert_eq!(
            session.data.lock().unwrap().turns[0]
                .wire
                .iter()
                .filter(|item| item["role"] == "assistant")
                .count(),
            1
        );
    }

    #[test]
    fn final_usage_replaces_provisional_totals_and_missing_stream_suffix() {
        let fixture = Fixture::new();
        let session = session(&fixture);
        session
            .reserve("Inspect files".into(), options(ApprovalMode::Yolo))
            .unwrap();
        let mut projection = Projection::default();
        projection.apply(&session, &init("native")).unwrap();
        projection.apply(&session, &json!({"event":"step_update","step_update":{"step_index":0,"step_type":"agent_response","text_delta":"Done","usage":{"input_tokens":2,"output_tokens":1}}})).unwrap();
        let final_event = json!({"event":"result","result":{"status":"SUCCESS","response":"Done completely.","usage":{"input_tokens":20,"cache_read_tokens":80,"output_tokens":7,"thinking_tokens":3}}});
        projection.apply(&session, &final_event).unwrap();
        projection.apply(&session, &final_event).unwrap();
        let snapshot = session.snapshot().unwrap();
        let step = &snapshot.turns[0].steps[0];
        assert_eq!(step.text, "Done completely.");
        assert_eq!(step.usage.as_ref().unwrap().input_tokens, 100);
        assert_eq!(step.usage.as_ref().unwrap().output_tokens, 7);
    }

    #[test]
    fn unstreamed_final_reply_preserves_prior_intermediate_text() {
        let fixture = Fixture::new();
        let session = session(&fixture);
        session
            .reserve("Inspect files".into(), options(ApprovalMode::Yolo))
            .unwrap();
        let mut projection = Projection::default();
        for event in [
            init("native"),
            json!({"event":"step_update","step_update":{"step_index":0,"step_type":"agent_response","text_delta":"Inspecting files."}}),
            json!({"event":"result","result":{"status":"SUCCESS","response":"Finished."}}),
        ] {
            projection.apply(&session, &event).unwrap();
        }
        assert_eq!(
            session.snapshot().unwrap().turns[0]
                .steps
                .iter()
                .map(|step| step.text.as_str())
                .collect::<Vec<_>>(),
            ["Inspecting files.", "Finished."]
        );
    }

    #[test]
    fn tool_correlation_requires_observed_native_identity_and_object_arguments() {
        let fixture = Fixture::new();
        let session = session(&fixture);
        session
            .reserve("Inspect files".into(), options(ApprovalMode::Yolo))
            .unwrap();
        let mut projection = Projection::default();
        let args = json!({"path":"README.md"});
        assert!(projection
            .take_tool("read", args.clone(), Some("unobserved"))
            .is_none());
        projection
            .apply(&session, &tool("native", 2, json!(args.to_string())))
            .unwrap();
        projection
            .apply(&session, &tool("native", 3, args.clone()))
            .unwrap();
        assert_eq!(
            projection.take_tool("read", args.clone(), None).unwrap().id,
            "agy:native:2"
        );
        assert_eq!(
            projection.take_tool("read", args.clone(), None).unwrap().id,
            "agy:native:3"
        );
        assert!(projection.take_tool("read", args.clone(), None).is_none());
        assert_eq!(
            projection
                .take_tool("read", args, Some("agy:native:2"))
                .unwrap()
                .id,
            "agy:native:2"
        );
        projection
            .apply(&session, &tool("native", 4, json!("malformed")))
            .unwrap();
        assert!(projection.take_tool("read", Value::Null, None).is_none());
    }

    #[test]
    fn tool_parameter_previews_update_before_callback_without_rewriting_its_receipt() {
        let fixture = Fixture::new();
        let session = session(&fixture);
        session
            .reserve("Inspect files".into(), options(ApprovalMode::Yolo))
            .unwrap();
        let mut projection = Projection::default();
        projection
            .apply(&session, &tool("native", 2, json!({})))
            .unwrap();
        let args = json!({"path":"README.md"});
        projection
            .apply(&session, &tool("native", 2, json!(args.to_string())))
            .unwrap();
        let snapshot = session.snapshot().unwrap();
        assert_eq!(snapshot.turns[0].steps[0].tools.len(), 1);
        assert_eq!(snapshot.turns[0].steps[0].tools[0].args, args);
        assert_eq!(
            projection.take_tool("read", args.clone(), None).unwrap().id,
            "agy:native:2"
        );
        session.update(true, |data| {
            let turn = &mut data.turns[0];
            let tool = &mut turn.turn.steps[0].tools[0];
            tool.status = "completed".into();
            tool.output = "Confirmed output".into();
            tool.duration_ms = 42;
            turn.wire.push(json!({"type":"function_call","call_id":"agy:native:2","name":"read","arguments":args.to_string()}));
        }).unwrap();
        let different = json!({"path":"other.md"});
        projection
            .apply(&session, &tool("native", 2, different.clone()))
            .unwrap();
        assert!(projection.take_tool("read", different, None).is_none());
        let receipt = &session.snapshot().unwrap().turns[0].steps[0].tools[0];
        assert_eq!(receipt.args, args);
        assert_eq!(receipt.output, "Confirmed output");
        assert_eq!(receipt.duration_ms, 42);
        assert_eq!(
            projection
                .take_tool("read", args, Some("agy:native:2"))
                .unwrap()
                .id,
            "agy:native:2"
        );
    }

    #[test]
    fn native_tool_ids_resume_stably_and_do_not_overwrite_confirmed_receipts() {
        let fixture = Fixture::new();
        let session = session(&fixture);
        session
            .reserve("Inspect files".into(), options(ApprovalMode::Yolo))
            .unwrap();
        let args = json!({"path":"README.md"});
        let event = tool("native", 1, args.clone());
        let mut first = Projection::default();
        first.apply(&session, &event).unwrap();
        session
            .update(true, |data| {
                let tool = &mut data.turns[0].turn.steps[0].tools[0];
                tool.status = "completed".into();
                tool.output = "Confirmed output".into();
                tool.duration_ms = 42;
            })
            .unwrap();
        let mut resumed = Projection::default();
        resumed.apply(&session, &event).unwrap();
        assert_eq!(
            resumed.take_tool("read", args.clone(), None).unwrap().id,
            "agy:native:1"
        );
        let snapshot = session.snapshot().unwrap();
        let receipt = &snapshot.turns[0].steps[0].tools[0];
        assert_eq!(
            (&receipt.status, &receipt.output, receipt.duration_ms),
            (&"completed".into(), &"Confirmed output".into(), 42)
        );
        resumed
            .apply(&session, &tool("other-native", 1, args.clone()))
            .unwrap();
        assert_eq!(
            resumed.take_tool("read", args, None).unwrap().id,
            "agy:other-native:1"
        );
    }

    #[test]
    fn terminal_tool_errors_do_not_trigger_provider_fallback() {
        assert!(final_result(&json!({"event":"step_update"})).is_none());
        assert!(
            final_result(&json!({"event":"result","result":{"status":"SUCCESS"}}))
                .unwrap()
                .is_ok()
        );
        assert_eq!(final_result(&json!({"event":"result","result":{"status":"ERROR","error":"Tool execution failed"}})).unwrap().unwrap_err().code, "agy_execution");
        assert_eq!(final_result(&json!({"event":"result","result":{"status":"ERROR","error":"Provider retries exhausted: rate limit"}})).unwrap().unwrap_err().code, "provider_retry_exhausted");
        for status in ["CANCELED", "CANCELLED", "INTERRUPTED"] {
            assert_eq!(
                final_result(&json!({"event":"result","result":{"status":status}}))
                    .unwrap()
                    .unwrap_err()
                    .code,
                "cancelled"
            );
        }
    }

    #[test]
    fn success_with_a_native_error_never_confirms_completion() {
        for result in [
            json!({"status":"SUCCESS","error":"Tool failed"}),
            json!({"status":"SUCCESS","is_error":true}),
            json!({"status":"ERROR","error":"rate limit"}),
        ] {
            assert_eq!(
                final_result(&json!({"event":"result","result":result}))
                    .unwrap()
                    .unwrap_err()
                    .code,
                "agy_execution"
            );
        }
    }

    #[test]
    fn continuation_results_keep_prior_text_and_token_totals() {
        let fixture = Fixture::new();
        let session = session(&fixture);
        session
            .reserve("Inspect files".into(), options(ApprovalMode::Yolo))
            .unwrap();
        let mut projection = Projection::default();
        for event in [
            init("native"),
            json!({"event":"step_update","step_update":{"step_index":0,"step_type":"agent_response","text_delta":"First partial","usage":{"input_tokens":1,"output_tokens":1}}}),
            json!({"event":"result","result":{"status":"SUCCESS","response":"First final","usage":{"input_tokens":10,"output_tokens":2}}}),
            json!({"event":"step_update","step_update":{"step_index":1,"step_type":"agent_response","text_delta":"Second partial","usage":{"input_tokens":3,"output_tokens":1}}}),
            json!({"event":"result","result":{"status":"SUCCESS","response":"Second final","usage":{"input_tokens":20,"output_tokens":4}}}),
        ] {
            projection.apply(&session, &event).unwrap();
        }
        let snapshot = session.snapshot().unwrap();
        let steps = &snapshot.turns[0].steps;
        assert_eq!(
            steps
                .iter()
                .map(|step| step.text.as_str())
                .collect::<Vec<_>>(),
            [
                "First partial",
                "First final",
                "Second partial",
                "Second final"
            ]
        );
        assert_eq!(
            steps
                .iter()
                .filter_map(|step| step.usage.as_ref())
                .map(|usage| usage.input_tokens)
                .sum::<u64>(),
            30
        );
        assert_eq!(
            steps
                .iter()
                .filter_map(|step| step.usage.as_ref())
                .map(|usage| usage.output_tokens)
                .sum::<u64>(),
            6
        );
    }

    #[test]
    fn partial_native_usage_retains_previously_observed_input() {
        let fixture = Fixture::new();
        let session = session(&fixture);
        session
            .reserve("Inspect files".into(), options(ApprovalMode::Yolo))
            .unwrap();
        let mut projection = Projection::default();
        for event in [
            init("native"),
            json!({"event":"step_update","step_update":{"step_index":0,"step_type":"agent_response","text_delta":"Done","usage":{"input_tokens":20,"cache_read_tokens":80}}}),
            json!({"event":"step_update","step_update":{"step_index":0,"step_type":"agent_response","usage":{"output_tokens":7}}}),
        ] {
            projection.apply(&session, &event).unwrap();
        }
        let snapshot = session.snapshot().unwrap();
        let usage = snapshot.turns[0].steps[0].usage.as_ref().unwrap();
        assert_eq!(
            (
                usage.input_tokens,
                usage.output_tokens,
                usage.cache_read_tokens
            ),
            (100, 7, Some(80))
        );
    }
}
