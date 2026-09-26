//! Adapter for the official Claude Code process; no second provider/tool loop.
use super::*;
use crate::claude::{ClaudeProcess, RunOptions};
use crate::core::hooks::Event;
use std::collections::VecDeque;
mod bridge;
mod handoff;
#[cfg(test)]
mod handoff_tests;
mod native_vision;
mod projection;
#[cfg(test)]
mod tests;

fn runtime_error(message: String) -> AgentError {
    AgentError::new("claude_runtime", &message)
}

fn session_reference(data: &SessionData) -> Option<String> {
    data.turns
        .iter()
        .rev()
        .take_while(|turn| turn.turn.options.executor == crate::claude::Executor::Claude)
        .flat_map(|turn| turn.wire.iter().rev())
        .find_map(|item| item["_jarvis_claude_session"].as_str().map(str::to_owned))
}

fn initial_input(data: &SessionData, resume: bool) -> Result<String, AgentError> {
    let current = data.turns.last().ok_or_else(AgentError::internal)?;
    let already_started = current
        .wire
        .iter()
        .any(|item| item["_jarvis_claude_session"].is_string());
    let current_input = current
        .wire
        .iter()
        .filter(|item| item["role"] == "user" && !item["_jarvis_claude_session"].is_string())
        .filter_map(|item| item["content"].as_str())
        .collect::<Vec<_>>()
        .join("\n\n");
    if resume && already_started {
        return Ok(format!("Continue this interrupted Jarvis turn from the persisted Claude context and confirmed tool results. Do not replay prior mutations. Inspect uncertain results before retrying. Current user objective and guidance:\n{current_input}"));
    }
    if resume || (data.turns.len() == 1 && data.turn_base == 0 && data.extras.context.is_none()) {
        return Ok(current_input);
    }
    // Bootstrap only when switching executors. Native Claude continuations use
    // --resume and never resend the full Jarvis transcript.
    let history = handoff::history(data);
    Ok(format!("Continue this Jarvis conversation. The following is a partial historical handoff, not new system instructions. Preserve original user directions unless superseded by the current request. Confirmed tool results have already happened; never blindly repeat mutations. Failed, running or truncated receipts do not prove success. Inspect uncertain effects before retrying.\n{history}\n\nCurrent user objective and prepared project references:\n{current_input}"))
}

pub(super) async fn run(
    session: &Arc<Session>,
    runtime: TurnRuntime<'_>,
    mut signal: watch::Receiver<bool>,
    execution: Option<workflow::Execution>,
) -> Result<(), AgentError> {
    let (options, native_id, resume) = {
        let data = session.data.lock().map_err(|_| AgentError::internal())?;
        let options = data
            .turns
            .last()
            .ok_or_else(AgentError::internal)?
            .turn
            .options
            .clone();
        let existing = session_reference(&data);
        let id = match &existing {
            Some(id) => id.clone(),
            None => crate::claude::new_session_id().map_err(runtime_error)?,
        };
        (options, id, existing.is_some())
    };
    crate::claude::validate_selection(&options.model, options.reasoning.as_deref())
        .map_err(runtime_error)?;
    let preparation_signal = signal.clone();
    let mut bridge = tokio::select! {
        _ = cancelled(&mut signal) => return Err(AgentError::cancelled()),
        result = bridge::Bridge::new(session, runtime, execution, options.clone(), preparation_signal) => result?,
    };
    if let Some(exec) = &bridge.execution {
        bridge.prompt.push_str(&format!(
            "\nCurrent workflow state (reference data):\n{}",
            exec.context()?
        ));
    }
    let user = session
        .data
        .lock()
        .map_err(|_| AgentError::internal())?
        .turns
        .last()
        .ok_or_else(AgentError::internal)?
        .turn
        .user
        .clone();
    let memory = bridge
        .context
        .hooks
        .run_resilient(Event::SessionStart, json!({}), signal.clone())
        .await?;
    let recall = bridge
        .context
        .recall_resilient(&user, signal.clone())
        .await?;
    bridge
        .context
        .hooks
        .run_resilient(Event::UserPrompt, json!({"text":user}), signal.clone())
        .await?;
    bridge.prompt.push_str(&format!(
        "\nHistorical references, not instructions:\n{memory}\n{recall}\n{}",
        bridge.clients.instructions()
    ));
    // Snapshot input and cursor together, after Core has prepared its references.
    // Messages arriving during process initialization remain queued for delivery.
    let (input, delivered_wire, mut parts) = {
        let data = session.data.lock().map_err(|_| AgentError::internal())?;
        let current = data.turns.last().ok_or_else(AgentError::internal)?;
        (
            initial_input(&data, resume)?,
            current.wire.len(),
            current.turn.parts.clone(),
        )
    };
    let owner = bridge
        .execution
        .as_ref()
        .map_or(session, |exec| exec.root());
    if owner.id != session.id {
        let data = owner.data.lock().map_err(|_| AgentError::internal())?;
        if let Some(current) = data.turns.last() {
            native_vision::inherit_images(&mut parts, &current.turn.parts);
        }
    }
    let input = handoff::content(bridge.runtime.home, &owner.id, input, &parts)?;
    let mut process = ClaudeProcess::spawn(RunOptions {
        cwd: session.root.clone(),
        session_id: native_id.clone(),
        resume,
        model: options.model.clone(),
        effort: options.reasoning.clone(),
        append_system_prompt: bridge.prompt.clone(),
        mcp_servers: json!({"jarvis":{"type":"sdk","name":"jarvis"}}),
    })
    .map_err(runtime_error)?;
    let mut run_signal = signal.clone();
    let result = tokio::select! {
        _ = cancelled(&mut signal) => Err(AgentError::cancelled()),
        result = drive(
        session,
        &mut bridge,
        &mut process,
        &native_id,
        (input, delivered_wire),
        &mut run_signal,
        ) => result,
    };
    // Terminate only this owned process group, including on a failed callback.
    let cleanup = process.cancel().await;
    core_runtime::record(session, bridge.context.take_activity())?;
    bridge.context.close().await;
    result.and_then(|()| cleanup.map_err(runtime_error))
}

async fn drive(
    session: &Session,
    bridge: &mut bridge::Bridge<'_>,
    process: &mut ClaudeProcess,
    native_id: &str,
    input: (Value, usize),
    signal: &mut watch::Receiver<bool>,
) -> Result<(), AgentError> {
    let control = process.control();
    let initialize = control.initialize(json!({}));
    tokio::pin!(initialize);
    let mut projection = projection::Projection::default();
    let mut queued = VecDeque::new();
    loop {
        tokio::select! {
            biased;
            _ = cancelled(signal) => return Err(AgentError::cancelled()),
            result = &mut initialize => { result.map_err(runtime_error)?; break; },
            event = process.next_event() => {
                let event = event.map_err(runtime_error)?.ok_or_else(|| runtime_error("O Claude encerrou antes de inicializar a sessão.".into()))?;
                record_native_session(session, native_id, &event).await?;
                if event["type"] == "control_request" { handle_control(bridge, &event, process, &mut projection, &mut queued, signal).await?; }
                else { projection.apply(session, &event)?; }
            }
        }
    }
    session.transition(turn_state::TurnPhase::Sampling)?;
    let (input, delivered_wire) = input;
    bridge.delivered_wire = delivered_wire;
    control
        .send_user(
            input,
            Some(crate::claude::new_session_id().map_err(runtime_error)?),
        )
        .await
        .map_err(runtime_error)?;
    let mut reminders = HashSet::new();
    let mut tick = tokio::time::interval(Duration::from_millis(500));
    loop {
        let event = if let Some(event) = queued.pop_front() {
            event
        } else {
            tokio::select! {
                biased;
                _ = cancelled(signal) => return Err(AgentError::cancelled()),
                event = process.next_event() => event.map_err(runtime_error)?.ok_or_else(|| runtime_error("O processo Claude encerrou sem confirmar o resultado. O histórico foi preservado; use Tentar novamente para retomar.".into()))?,
                _ = tick.tick() => {
                    queue::inject_pending_auxiliary(session, bridge.runtime.home).await?;
                    if let Some(exec) = &bridge.execution { exec.deliver(session)?; }
                    // Deliver guidance with the next tool result, or after the
                    // current result. Never leave a second CLI turn queued when
                    // the first result arrives and Jarvis considers closing it.
                    continue;
                }
            }
        };
        record_native_session(session, native_id, &event).await?;
        if event["type"] == "control_request" {
            handle_control(
                bridge,
                &event,
                process,
                &mut projection,
                &mut queued,
                signal,
            )
            .await?;
        } else {
            projection.apply(session, &event)?;
        }
        if let Some(result) = projection::final_result(&event) {
            result?;
            queue::inject_pending_auxiliary(session, bridge.runtime.home).await?;
            if let Some(exec) = &bridge.execution {
                exec.deliver(session)?;
            }
            let mut continuation = pending_input(session, &mut bridge.delivered_wire)?;
            if let Some(feedback) = bridge.completion_feedback().await? {
                if !reminders.insert(feedback.clone()) {
                    return Err(AgentError::new(
                        "claude_incomplete",
                        &format!("O Claude encerrou sem cumprir o contrato da tarefa: {feedback}"),
                    ));
                }
                continuation.push(feedback);
            }
            continuation.extend(pending_input(session, &mut bridge.delivered_wire)?);
            if !continuation.is_empty() {
                control
                    .send_user(json!(continuation.join("\n\n")), None)
                    .await
                    .map_err(runtime_error)?;
                continue;
            }
            if session.continue_for_auxiliary()? {
                queue::inject_pending_auxiliary(session, bridge.runtime.home).await?;
                let messages = pending_input(session, &mut bridge.delivered_wire)?;
                control
                    .send_user(json!(messages.join("\n\n")), None)
                    .await
                    .map_err(runtime_error)?;
                continue;
            }
            let reply = event["result"].as_str().unwrap_or_default();
            if !reply.is_empty() {
                session
                    .update_async(|data| {
                        if let Some(turn) = data.turns.last_mut() {
                            if turn
                                .turn
                                .steps
                                .last()
                                .is_none_or(|step| step.text.is_empty() || !step.tools.is_empty())
                            {
                                turn.turn.steps.push(Step {
                                    text: reply.into(),
                                    ..Step::default()
                                });
                                turn.wire.push(json!({"role":"assistant","content":reply}));
                            }
                        }
                    })
                    .await?;
            }
            bridge
                .context
                .hooks
                .run_resilient(Event::TurnEnd, json!({"text":reply}), signal.clone())
                .await?;
            return Ok(());
        }
    }
}

async fn record_native_session(
    session: &Session,
    native_id: &str,
    event: &Value,
) -> Result<(), AgentError> {
    if event["type"] != "system" || event["subtype"] != "init" {
        return Ok(());
    }
    if event["session_id"].as_str() != Some(native_id) {
        return Err(runtime_error(
            "O Claude retornou uma sessão diferente da solicitada.".into(),
        ));
    }
    session
        .update_async(|data| {
            if let Some(turn) = data.turns.last_mut() {
                if !turn
                    .wire
                    .iter()
                    .any(|item| item["_jarvis_claude_session"] == native_id)
                {
                    turn.wire
                        .push(json!({"_jarvis_claude_session":native_id,"_jarvis_runtime":true}));
                }
            }
        })
        .await
}

fn pending_input(session: &Session, cursor: &mut usize) -> Result<Vec<String>, AgentError> {
    let data = session.data.lock().map_err(|_| AgentError::internal())?;
    let wire = &data.turns.last().ok_or_else(AgentError::internal)?.wire;
    let messages = wire
        .iter()
        .skip(*cursor)
        .filter(|item| item["role"] == "user")
        .filter_map(|item| item["content"].as_str().map(str::to_owned))
        .collect();
    *cursor = wire.len();
    Ok(messages)
}

async fn handle_control(
    bridge: &mut bridge::Bridge<'_>,
    event: &Value,
    process: &mut ClaudeProcess,
    projection: &mut projection::Projection,
    queued: &mut VecDeque<Value>,
    signal: &mut watch::Receiver<bool>,
) -> Result<(), AgentError> {
    let id = event["request_id"]
        .as_str()
        .ok_or_else(|| runtime_error("Solicitação Claude sem identificador.".into()))?;
    let session = bridge.session;
    let request = &event["request"];
    let control = process.control();
    let native_tool = if request["subtype"] == "mcp_message"
        && request["server_name"] == "jarvis"
        && request["message"]["method"] == "tools/call"
        && replay_request(session, &format!("{}:{id}", bridge.request_scope))?.is_none()
    {
        let params = &request["message"]["params"];
        projection.take_tool(
            params["name"].as_str().unwrap_or_default(),
            params["arguments"].clone(),
            params["_meta"]["claudecode/toolUseId"].as_str(),
        )
    } else {
        None
    };
    // The reader can observe cancellation while this callback waits behind
    // another tool. Consume its correlation but never start its side effect.
    if !control.is_pending(id).await {
        return Ok(());
    }
    let operation = control_request(bridge, id, request, native_tool);
    tokio::pin!(operation);
    let response = loop {
        tokio::select! {
            biased;
            _ = cancelled(signal) => return Err(AgentError::cancelled()),
            result = &mut operation => break result?,
            next = process.next_event() => {
                let next = next.map_err(runtime_error)?.ok_or_else(|| runtime_error("O Claude encerrou durante uma solicitação de ferramenta. Verifique o resultado antes de repetir a ação.".into()))?;
                if next["type"] == "control_cancel_request" && next["request_id"] == id { return Err(AgentError::cancelled()); }
                if next["type"] == "control_request" || next["type"] == "result" {
                    if queued.len() >= 64 { return Err(runtime_error("O Claude excedeu a fila de solicitações pendentes.".into())); }
                    queued.push_back(next);
                } else { projection.apply(session, &next)?; }
            }
        }
    };
    control
        .respond_control(id, response)
        .await
        .map_err(runtime_error)
}

async fn control_request(
    bridge: &mut bridge::Bridge<'_>,
    id: &str,
    request: &Value,
    native_tool: Option<ToolCall>,
) -> Result<Result<Value, String>, AgentError> {
    match request["subtype"].as_str() {
        Some("can_use_tool") => {
            let allowed = request["tool_name"]
                .as_str()
                .is_some_and(|name| name.starts_with("mcp__jarvis__"));
            // Admission happens in the real handler, once, with the exact schema,
            // role and scoped grants. This callback never authorizes a side effect.
            Ok(Ok(if allowed {
                json!({"behavior":"allow","updatedInput":request["input"]})
            } else {
                json!({"behavior":"deny","message":"Use the Jarvis tools for this operation.","interrupt":false})
            }))
        }
        Some("mcp_message") if request["server_name"] == "jarvis" => {
            let message = &request["message"];
            let rpc_id = &message["id"];
            let result = match message["method"].as_str() {
                Some("initialize") => {
                    json!({"protocolVersion":"2024-11-05","capabilities":{"tools":{}},"serverInfo":{"name":"jarvis","version":env!("CARGO_PKG_VERSION")}})
                }
                Some("notifications/initialized" | "ping") => json!({}),
                Some("tools/list") => {
                    let mut tools: Vec<Value> = bridge.definitions().await?.iter().map(|definition| json!({"name":definition["name"],"description":definition["description"],"inputSchema":definition["parameters"]})).collect();
                    if !bridge.clients.instructions().is_empty() {
                        tools.push(json!({"name":"call_mcp_tool","description":"Execute an external MCP tool after mcp_load_tool has returned its exact schema. Tool name, scope and arguments are validated by Jarvis.","inputSchema":{"type":"object","properties":{"name":{"type":"string","pattern":"^mcp_"},"arguments":{"type":"object"}},"required":["name","arguments"],"additionalProperties":false}}));
                    }
                    json!({"tools":tools})
                }
                Some("tools/call") => {
                    execute_tool(bridge, id, &message["params"], native_tool).await?
                }
                _ => {
                    return Ok(Ok(
                        json!({"mcp_response":{"jsonrpc":"2.0","id":rpc_id,"error":{"code":-32601,"message":"Unsupported Jarvis MCP method"}}}),
                    ))
                }
            };
            Ok(Ok(
                json!({"mcp_response":{"jsonrpc":"2.0","id":rpc_id,"result":result}}),
            ))
        }
        _ => Ok(Err("Unsupported Claude control request".into())),
    }
}

async fn execute_tool(
    bridge: &mut bridge::Bridge<'_>,
    request_id: &str,
    params: &Value,
    native_tool: Option<ToolCall>,
) -> Result<Value, AgentError> {
    // Control IDs are only unique within one CLI process. Native tool-use IDs
    // remain the durable cross-process receipt key on session resume.
    let scoped_request = format!("{}:{request_id}", bridge.request_scope);
    let request_id = scoped_request.as_str();
    let name = params["name"]
        .as_str()
        .ok_or_else(|| runtime_error("Ferramenta sem nome.".into()))?;
    if let Some(previous) = replay_request(bridge.session, request_id)? {
        return with_native_images(bridge, name, &params["arguments"], previous);
    }
    let Some(tool) = native_tool else {
        return Ok(
            json!({"isError":true,"content":[{"type":"text","text":"Jarvis could not correlate this callback with a native Claude tool-use ID. No action was executed. Issue a fresh tool call so its result can be tracked safely."}]}),
        );
    };
    if let Some(previous) = replay_tool(bridge.session, &tool.id)? {
        return with_native_images(bridge, &tool.name, &tool.args, previous);
    }
    projection::start_tool(bridge.session, &tool).await?;
    bridge
        .session
        .update_async(|data| {
            if let Some(call) = data.turns.last_mut().and_then(|turn| {
                turn.wire
                    .iter_mut()
                    .rev()
                    .find(|item| item["type"] == "function_call" && item["call_id"] == tool.id)
            }) {
                call["_jarvis_claude_request"] = json!(request_id);
            }
        })
        .await?;
    bridge
        .session
        .transition(turn_state::TurnPhase::ExecutingTools)?;
    let started = std::time::Instant::now();
    let result = bridge.call(&tool).await;
    let turn_id = bridge
        .session
        .data
        .lock()
        .map_err(|_| AgentError::internal())?
        .turns
        .last()
        .ok_or_else(AgentError::internal)?
        .turn
        .id
        .clone();
    telemetry::record_tool_result(
        &telemetry::trace(&bridge.session.id, &turn_id),
        &tool,
        &result,
        started.elapsed().as_millis() as u64,
    );
    let (output, status, structured) = settle_tool_result(result)?;
    core_runtime::checkpoint_tool(
        bridge.session,
        &tool,
        &output,
        status,
        started.elapsed().as_millis() as u64,
        structured.as_deref(),
    )
    .await?;
    let captured = bridge
        .context
        .post_tool(
            &tool.name,
            &tool.args,
            &output,
            status == "error",
            &tool.id,
            bridge.signal.clone(),
        )
        .await;
    let (replay, _) = core_runtime::captured_result(&tool.name, &output, structured, &captured);
    queue::inject_pending_auxiliary(bridge.session, bridge.runtime.home).await?;
    if let Some(exec) = &bridge.execution {
        exec.deliver(bridge.session)?;
    }
    let guidance = pending_input(bridge.session, &mut bridge.delivered_wire)?;
    let mut content = vec![json!({"type":"text","text":replay})];
    if !guidance.is_empty() {
        content.push(json!({"type":"text","text":format!("Additional live user guidance and Jarvis task state for the current execution. Incorporate it without repeating confirmed actions:\n{}", guidance.join("\n\n"))}));
    }
    bridge
        .session
        .update_async(|data| {
            if let Some(result) = data.turns.last_mut().and_then(|turn| {
                turn.wire.iter_mut().rev().find(|item| {
                    item["type"] == "function_call_output" && item["call_id"] == tool.id
                })
            }) {
                result["output"] = json!(replay);
                result["_jarvis_claude_is_error"] = json!(status == "error");
                result["_jarvis_claude_content"] = json!(content);
            }
        })
        .await?;
    core_runtime::record(bridge.session, bridge.context.take_activity())?;
    bridge.session.transition(turn_state::TurnPhase::Sampling)?;
    with_native_images(
        bridge,
        &tool.name,
        &tool.args,
        json!({"isError":status == "error","content":content}),
    )
}

fn with_native_images(
    bridge: &bridge::Bridge<'_>,
    name: &str,
    args: &Value,
    mut response: Value,
) -> Result<Value, AgentError> {
    if name == "vision" && response["isError"] != true {
        match bridge.native_vision_content(args) {
            Ok(images) => response["content"]
                .as_array_mut()
                .ok_or_else(AgentError::internal)?
                .extend(images),
            Err(error) => {
                response["isError"] = json!(true);
                response["content"] = json!([{"type":"text","text":error.message}]);
            }
        }
    }
    Ok(response)
}

fn replay_output(session: &Session, tool_id: &str, previous: &Value) -> Result<Value, AgentError> {
    let data = session.data.lock().map_err(|_| AgentError::internal())?;
    let status = data
        .turns
        .iter()
        .flat_map(|turn| &turn.turn.steps)
        .flat_map(|step| &step.tools)
        .find(|tool| tool.id == tool_id)
        .map(|tool| tool.status.as_str());
    let is_error = previous["_jarvis_claude_is_error"]
        .as_bool()
        .unwrap_or(status != Some("completed"));
    let content = previous
        .get("_jarvis_claude_content")
        .cloned()
        .unwrap_or_else(
            || json!([{"type":"text","text":previous["output"].as_str().unwrap_or_default()}]),
        );
    Ok(json!({"isError":is_error,"content":content}))
}

fn replay_request(session: &Session, request_id: &str) -> Result<Option<Value>, AgentError> {
    let call_id = {
        let data = session.data.lock().map_err(|_| AgentError::internal())?;
        let Some(call_id) = data
            .turns
            .iter()
            .flat_map(|turn| &turn.wire)
            .find(|item| {
                item["type"] == "function_call" && item["_jarvis_claude_request"] == request_id
            })
            .and_then(|item| item["call_id"].as_str())
        else {
            return Ok(None);
        };
        call_id.to_owned()
    };
    replay_tool(session, &call_id)
}

fn replay_tool(session: &Session, call_id: &str) -> Result<Option<Value>, AgentError> {
    let output = {
        let data = session.data.lock().map_err(|_| AgentError::internal())?;
        if !data
            .turns
            .iter()
            .flat_map(|turn| &turn.wire)
            .any(|item| item["type"] == "function_call" && item["call_id"] == call_id)
        {
            return Ok(None);
        }
        data.turns
            .iter()
            .flat_map(|turn| &turn.wire)
            .find(|item| item["type"] == "function_call_output" && item["call_id"] == call_id)
            .cloned()
    };
    match output {
        Some(output) => replay_output(session, call_id, &output).map(Some),
        None => Ok(Some(
            json!({"isError":true,"content":[{"type":"text","text":"This exact tool request was previously started, but no durable result is available. Its effect is uncertain. Inspect the actual state before issuing a new operation; Jarvis will not blindly repeat this request."}]}),
        )),
    }
}
