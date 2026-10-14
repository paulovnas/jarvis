//! Evidence attached to individual interrupted calls, not a global "some read happened" flag.
use super::*;
use sha2::{Digest, Sha256};

pub(super) const TOOL: &str = "recovery_resolve";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Call {
    id: String,
    name: String,
    signature: String,
    #[serde(default)]
    server: Option<String>,
    paths: Vec<String>,
    #[serde(default)]
    repositories: Vec<String>,
    #[serde(default)]
    remote_effect: bool,
    selectors: BTreeMap<String, Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    live_args: Option<Vec<String>>,
    expected_hash: Option<String>,
    #[serde(default)]
    evidence: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    outcome: Option<Outcome>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Outcome {
    Applied,
    NotApplied,
    Unavailable,
}

fn uncertain_input(tool: &ToolCall, output: &str) -> bool {
    super::super::computer::mutating(&tool.name)
        && serde_json::from_str::<Value>(output).is_ok_and(|receipt| {
            receipt["code"] == "computer_action_uncertain"
                && receipt["phase"] == "uncertain"
                && receipt["inputSent"] == true
                && receipt
                    .get("targetId")
                    .is_some_and(|id| id.is_string() && Some(id) == tool.args.get("targetId"))
        })
}

fn observed_input(tool: &ToolCall, output: &str) -> Option<(String, String)> {
    if !super::super::computer::mutating(&tool.name) {
        return None;
    }
    let receipt: Value = serde_json::from_str(output).ok()?;
    if receipt["phase"] != "observed" || receipt["inputSent"] != true {
        return None;
    }
    let target = receipt["targetId"].as_str()?;
    let operation = receipt["operationId"].as_str()?;
    (tool.args["targetId"].as_str() == Some(target)
        && !operation.is_empty()
        && operation.len() <= 160
        && !operation.contains('\0'))
    .then(|| (target.to_owned(), operation.to_owned()))
}

fn unavailable_target(call: &Call, output: &Value) -> bool {
    super::super::computer::mutating(&call.name)
        && output["code"] == "computer_target_unavailable"
        && output["phase"] == "observed"
        && output["available"] == false
        && output["inputSent"] == false
        && output
            .get("targetId")
            .is_some_and(|id| id.is_string() && Some(id) == call.selectors.get("targetId"))
}

fn empty_browser_catalog(output: &Value) -> bool {
    output["backend"] == "embedded"
        && output.get("activeId").is_some_and(Value::is_null)
        && output["tabs"].as_array().is_some_and(Vec::is_empty)
}

fn inactive_impeccable_live(
    output: &Value,
    conversation: &str,
    project: &Path,
    args: &[String],
) -> bool {
    let Ok(target) = crate::core::impeccable_live::requested_target(project, args) else {
        return false;
    };
    output.get("liveServer").is_some_and(Value::is_null)
        && output["activeSessions"]
            .as_array()
            .is_some_and(Vec::is_empty)
        && output["hostStatus"]["conversationId"] == conversation
        && output["hostStatus"]["state"] == "off"
        && output["hostStatus"]["phase"] == "idle"
        && output["hostInspection"]["confirmedInactive"] == true
        && output["hostInspection"]["conversationId"] == conversation
        && output["hostInspection"]["projectRoot"] == project.to_string_lossy().as_ref()
        && output["hostInspection"].get("target") == Some(&json!(target))
}

fn hash(value: &[u8]) -> String {
    format!("{:x}", Sha256::digest(value))
}

fn signature(tool: &ToolCall) -> String {
    hash(format!("{}\0{}", tool.name, tool.args).as_bytes())
}

fn resource_path(root: &Path, path: &str) -> std::path::PathBuf {
    let path = root.join(path);
    if let Ok(path) = path.canonicalize() {
        return path;
    }
    let mut normalized = std::path::PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                normalized.pop();
            }
            part => normalized.push(part.as_os_str()),
        }
    }
    for ancestor in normalized.ancestors() {
        if let Ok(existing) = ancestor.canonicalize() {
            if let Ok(suffix) = normalized.strip_prefix(ancestor) {
                return existing.join(suffix);
            }
        }
    }
    normalized
}

fn command_has(tool: &ToolCall, predicate: impl Fn(&[&str]) -> bool) -> bool {
    if tool.name != "bash" {
        return false;
    }
    let Some(command) = tool.args["command"].as_str() else {
        return false;
    };
    let Ok(plan) = super::super::execution_policy::parse_command(command) else {
        return false;
    };
    !plan.dynamic
        && plan.invocations.iter().any(|invocation| {
            let mut words: Vec<_> = invocation.argv.iter().map(String::as_str).collect();
            if words.starts_with(&["git", "-C"]) && words.len() > 3 {
                words.drain(1..3);
            }
            predicate(&words)
        })
}

// Only confirmed checks may run again. An interrupted check still requires
// reconciliation, since project scripts can have side effects.
fn repeatable_check(tool: &ToolCall) -> bool {
    let Some(command) = tool.args["command"].as_str() else {
        return false;
    };
    if tool.name != "bash" {
        return false;
    }
    let Ok(plan) = super::super::execution_policy::parse_command(command) else {
        return false;
    };
    !plan.dynamic
        && !plan.invocations.is_empty()
        && plan.invocations.iter().all(|invocation| {
            let words: Vec<_> = invocation.argv.iter().map(String::as_str).collect();
            matches!(
                words.as_slice(),
                ["cd", _]
                    | ["cargo", "test" | "check" | "clippy", ..]
                    | ["npm" | "bun" | "pnpm" | "yarn", "test", ..]
                    | [
                        "npm" | "bun" | "pnpm" | "yarn",
                        "run",
                        "test" | "check" | "lint" | "typecheck" | "build",
                        ..
                    ]
            )
        })
}

fn repositories(tool: &ToolCall) -> Vec<String> {
    if tool.name == "jarvis_propose_publication" {
        return tool.args["repositories"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|repo| repo["path"].as_str().map(str::to_owned))
            .collect();
    }
    let mut cwd = std::path::PathBuf::from(tool.args["workdir"].as_str().unwrap_or("."));
    let Some(command) = tool.args["command"].as_str() else {
        return vec![];
    };
    let Ok(plan) = super::super::execution_policy::parse_command(command) else {
        return vec![];
    };
    if plan.dynamic {
        return vec![];
    }
    let mut roots = vec![];
    for invocation in plan.invocations {
        let words: Vec<_> = invocation.argv.iter().map(String::as_str).collect();
        let root = match words.as_slice() {
            ["cd", path] => {
                cwd = cwd.join(path);
                continue;
            }
            ["git", "-C", path, ..] => cwd.join(path),
            ["git" | "gh", ..] => cwd.clone(),
            _ => continue,
        };
        let path: std::path::PathBuf = root
            .components()
            .filter(|part| !matches!(part, std::path::Component::CurDir))
            .collect();
        let path = path.to_string_lossy().into_owned();
        let path = if path.is_empty() { ".".into() } else { path };
        if !roots.contains(&path) {
            roots.push(path);
        }
    }
    roots
}

pub(super) fn read_only(tool: &ToolCall, mcp_mutating: bool) -> bool {
    if tool.name == "browser_screenshot" && tool.args["savePath"].is_string() {
        return false;
    }
    (tool.name == "impeccable"
        && super::super::impeccable::capabilities(&tool.args).effect
            == super::super::tool_contract::Effect::ReadOnly)
        || (tool.name == "bash" && !super::super::tasks::requires_active_task_for(tool))
        || tool.name == "jarvis_inspect_publication"
        || super::recovery_inspection_tool(&tool.name, mcp_mutating)
}

pub(super) fn collect(turn: &StoredTurn) -> Vec<Call> {
    let outputs: HashMap<_, _> = turn
        .wire
        .iter()
        .filter(|item| {
            item["type"] == "function_call_output"
                && item["output"].as_str() != Some(journal::UNKNOWN_TOOL_OUTPUT)
        })
        .filter_map(|item| item["call_id"].as_str().map(|id| (id, &item["output"])))
        .collect();
    let tools = recorded_tools(turn);
    let resolutions: HashMap<_, _> = tools
        .iter()
        .filter(|tool| tool.name == TOOL)
        .filter_map(|tool| {
            let result = outputs
                .get(tool.id.as_str())?
                .as_str()
                .and_then(|output| serde_json::from_str::<Value>(output).ok())?;
            if result["callId"] != tool.args["callId"]
                || result["evidenceCallId"] != tool.args["evidenceCallId"]
                || result["outcome"] != tool.args["outcome"]
            {
                return None;
            }
            let outcome = match result["outcome"].as_str()? {
                "applied" => Some(Outcome::Applied),
                "not_applied" => Some(Outcome::NotApplied),
                "unavailable" => Some(Outcome::Unavailable),
                "unknown" => None,
                _ => return None,
            };
            Some((result["callId"].as_str()?.to_owned(), outcome))
        })
        .collect();
    tools
        .iter()
        .filter(|tool| {
            outputs
                .get(tool.id.as_str())
                .is_none_or(|output| uncertain_input(tool, output.as_str().unwrap_or_default()))
        })
        .filter(|tool| {
            !read_only(tool, true)
                && !matches!(tool.name.as_str(), "ask_user" | "update_tasks" | TOOL)
        })
        .map(|tool| {
            let mut call = Call::new(tool);
            call.outcome = resolutions.get(&call.id).copied().flatten();
            call
        })
        .collect()
}

fn recorded_tools(turn: &StoredTurn) -> Vec<ToolCall> {
    let mut tools: Vec<ToolCall> = turn
        .turn
        .steps
        .iter()
        .flat_map(|step| &step.tools)
        .cloned()
        .collect();
    for item in &turn.wire {
        if item["type"] != "function_call" {
            continue;
        }
        let (Some(id), Some(name), Some(args)) = (
            item["call_id"].as_str(),
            item["name"].as_str(),
            item["arguments"].as_str(),
        ) else {
            continue;
        };
        if tools.iter().any(|tool| tool.id == id) {
            continue;
        }
        if let Ok(args) = serde_json::from_str(args) {
            tools.push(ToolCall {
                id: id.into(),
                name: name.into(),
                args,
                status: "error".into(),
                output: String::new(),
                duration_ms: 0,
            });
        }
    }
    tools
}

impl Call {
    fn new(tool: &ToolCall) -> Self {
        let paths = if tool.name == "apply_patch" {
            super::super::patch::target_paths(&tool.args).unwrap_or_default()
        } else if matches!(tool.name.as_str(), "write" | "edit") {
            tool.args["path"]
                .as_str()
                .map(str::to_owned)
                .into_iter()
                .collect()
        } else {
            vec![]
        };
        let mut selectors: BTreeMap<String, Value> = tool
            .args
            .as_object()
            .into_iter()
            .flatten()
            .filter(|(key, value)| {
                let key = key.to_ascii_lowercase();
                (key.ends_with("id")
                    || matches!(key.as_str(), "path" | "url" | "key" | "name" | "workdir"))
                    && (value.is_string() || value.is_number())
            })
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect();
        if tool.name == "impeccable" && tool.args["command"] == "live" {
            selectors.insert("command".into(), json!("live"));
        }
        Self {
            id: tool.id.clone(),
            name: tool.name.clone(),
            signature: signature(tool),
            server: None,
            paths,
            repositories: repositories(tool),
            remote_effect: (tool.name == "jarvis_propose_publication"
                && tool.args["repositories"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .any(|repo| {
                        repo["push"].as_str().is_some_and(|push| push != "none")
                            || repo["pullRequest"].is_object()
                    }))
                || command_has(tool, |words| {
                    matches!(words, ["git", "push", ..] | ["gh", ..])
                }),
            selectors,
            live_args: (tool.name == "impeccable" && tool.args["command"] == "live")
                .then(|| serde_json::from_value(tool.args["args"].clone()).ok())
                .flatten(),
            expected_hash: (tool.name == "write")
                .then(|| {
                    tool.args["content"]
                        .as_str()
                        .map(|text| hash(text.as_bytes()))
                })
                .flatten(),
            evidence: BTreeMap::new(),
            outcome: None,
        }
    }

    fn related(&self, tool: &ToolCall, same_mcp: bool, root: &Path, conversation: &str) -> bool {
        // An interrupted open has no tab ID; an empty owned catalog can confirm its absence.
        if self.name == "browser_open" {
            return tool.name == "browser_list"
                && serde_json::from_str::<Value>(&tool.output)
                    .is_ok_and(|output| empty_browser_catalog(&output));
        }
        if self.name.starts_with("browser_") {
            return matches!(
                tool.name.as_str(),
                "browser_snapshot" | "browser_wait" | "browser_screenshot"
            ) && self.same_ui_target(tool);
        }
        if self.name == "impeccable"
            && self.selectors.get("command").and_then(Value::as_str) == Some("live")
        {
            return tool.name == "impeccable"
                && tool.args["command"] == "live-status"
                && self.live_args.as_ref().is_some_and(|args| {
                    serde_json::from_str::<Value>(&tool.output).is_ok_and(|output| {
                        inactive_impeccable_live(&output, conversation, root, args)
                    })
                });
        }
        if super::super::computer::mutating(&self.name) {
            return tool.name == "computer_observe" && self.same_ui_target(tool);
        }
        if !self.paths.is_empty() {
            return tool.args["path"].as_str().is_some_and(|path| {
                self.paths.iter().any(|target| {
                    (tool.name == "read"
                        && resource_path(root, target) == resource_path(root, path))
                        || (tool.name == "list"
                            && resource_path(root, target).parent()
                                == Some(resource_path(root, path).as_path()))
                })
            });
        }
        if self.name.starts_with("mcp_") {
            return same_mcp
                && !self.selectors.is_empty()
                && self
                    .selectors
                    .iter()
                    .all(|(key, value)| tool.args.get(key) == Some(value));
        }
        if matches!(self.name.as_str(), "bash" | "jarvis_propose_publication") {
            if tool.name == "jarvis_inspect_publication" && !self.remote_effect {
                return !self.repositories.is_empty()
                    && self.repositories.iter().all(|repo| {
                        tool.args["paths"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .filter_map(Value::as_str)
                            .any(|path| resource_path(root, path) == resource_path(root, repo))
                    });
            }
            let observed = repositories(tool);
            return tool.name == "bash"
                && !super::super::tasks::requires_active_task_for(tool)
                && !self.repositories.is_empty()
                && self.repositories.iter().all(|repo| {
                    observed
                        .iter()
                        .any(|path| resource_path(root, path) == resource_path(root, repo))
                })
                && (!self.remote_effect
                    || command_has(tool, |words| {
                        matches!(
                            words,
                            ["git", "ls-remote", ..] | ["gh", "pr", "view" | "list" | "status", ..]
                        )
                    }));
        }
        if self.name.starts_with("beads_") || self.name.starts_with("project_beads_") {
            return tool.name.ends_with("_show")
                && self
                    .selectors
                    .iter()
                    .any(|(key, value)| tool.args.get(key) == Some(value));
        }
        if self.name.starts_with("process_") || self.name.starts_with("terminal_") {
            return (tool.name.ends_with("_output")
                || tool.name.ends_with("_list")
                || tool.name.ends_with("_check_port"))
                && !self.selectors.is_empty()
                && self
                    .selectors
                    .iter()
                    .any(|(key, value)| tool.args.get(key) == Some(value));
        }
        false
    }

    fn same_ui_target(&self, tool: &ToolCall) -> bool {
        let key = if self.name.starts_with("browser_") && tool.name.starts_with("browser_") {
            "id"
        } else if self.name.starts_with("computer_") && tool.name.starts_with("computer_") {
            "targetId"
        } else {
            return false;
        };
        self.selectors
            .get(key)
            .filter(|value| value.as_str().is_some_and(|id| !id.is_empty()))
            .is_some_and(|value| tool.args.get(key) == Some(value))
    }
}

pub(super) fn definition() -> Value {
    tools::definition(TOOL,
        "Resolve one uncertain operation from a successful, relevant inspection in this turn. Use its saved callId and an actual inspection call ID from its evidence keys, never the tool name. Select applied only when evidence confirms this operation's effect; not_applied only with definitive non-application evidence. An unchanged button, absent toast or loading screen immediately after a click does not prove not_applied: asynchronous remote effects may arrive later, so use unknown and inspect the same resource again. Distinguish the current resource state from attribution to an earlier call; a later changed state does not justify rewriting a confirmed operation outcome. An interrupted browser_open without a tab ID requires browser_list's empty integrated catalog and not_applied; a nonempty catalog remains ambiguous. An interrupted impeccable command=live requires live-status for the same --target, with an empty helper catalog, idle off hostStatus for this conversation, hostInspection.confirmedInactive for this project and target, and not_applied. Use unavailable only for computer_observe's authoritative same-target computer_target_unavailable receipt; this retires the old target without claiming the input effect was known. An unknown outcome remains unresolved. Continue independent inspections and preserve the user's remaining intent. Never repeat an uncertain mutation blindly or replay it on a replacement target. This records evidence, not user approval.",
        json!({"callId":{"type":"string","minLength":1},"evidenceCallId":{"type":"string","minLength":1},"outcome":{"type":"string","enum":["applied","not_applied","unavailable","unknown"]}}),
        &["callId","evidenceCallId","outcome"])
}

impl Execution {
    /// Both executors call this only after their function result is durable.
    pub(in crate::agent) fn record_computer_result(
        &self,
        tool: &ToolCall,
        output: &str,
    ) -> Result<(), AgentError> {
        self.record_uncertain_input(tool, output)?;
        self.confirm_computer_result(tool, output)
    }

    fn confirm_computer_result(&self, tool: &ToolCall, output: &str) -> Result<(), AgentError> {
        self.confirm_computer_result_with(tool, output, |conversation, call, target, operation| {
            if let Some(app) = &self.hub.env.browser_app {
                use tauri::Manager;
                app.state::<super::super::computer::ComputerState>()
                    .confirmed(conversation, call, target, operation)?;
            }
            Ok(())
        })
    }

    fn confirm_computer_result_with(
        &self,
        tool: &ToolCall,
        output: &str,
        confirmed: impl FnOnce(&str, &str, &str, &str) -> Result<(), AgentError>,
    ) -> Result<(), AgentError> {
        let Some((target, operation)) = observed_input(tool, output) else {
            return Ok(());
        };
        let session = self.recovery_session()?.ok_or_else(AgentError::internal)?;
        let preserved = {
            let data = session.data.lock().map_err(|_| AgentError::internal())?;
            data.turns.iter().any(|turn| {
                turn.wire.iter().any(|item| {
                    item["type"] == "function_call_output"
                        && item["call_id"] == tool.id
                        && item["output"].as_str() == Some(output)
                })
            })
        };
        if !preserved {
            return Err(AgentError::new(
                "computer_receipt_not_preserved",
                "O resultado da entrada nativa ainda não foi preservado. O alvo continua bloqueado.",
            ));
        }
        session.flush()?;
        confirmed(
            &self.hub.root.id,
            &self.computer_call_id(&tool.id),
            &target,
            &operation,
        )
    }

    fn confirm_preserved_computer_results(&self, session: &Session) -> Result<(), AgentError> {
        self.confirm_preserved_computer_results_with(session, |tool, output| {
            self.confirm_computer_result(tool, output)
        })
    }

    fn confirm_preserved_computer_results_with(
        &self,
        session: &Session,
        mut confirmed: impl FnMut(&ToolCall, &str) -> Result<(), AgentError>,
    ) -> Result<(), AgentError> {
        let results: Vec<_> = {
            let data = session.data.lock().map_err(|_| AgentError::internal())?;
            data.turns
                .last()
                .into_iter()
                .flat_map(|turn| {
                    recorded_tools(turn).into_iter().filter_map(move |tool| {
                        let output = turn.wire.iter().rev().find(|item| {
                            item["type"] == "function_call_output" && item["call_id"] == tool.id
                        })?["output"]
                            .as_str()?;
                        observed_input(&tool, output).map(|_| (tool, output.to_owned()))
                    })
                })
                .collect()
        };
        session.flush()?;
        for (tool, output) in results {
            // A lost ACK may follow a durable success. The controller accepts
            // only the exact call/operation marker, so old ACKs are harmless.
            confirmed(&tool, &output)?;
        }
        Ok(())
    }

    pub(in crate::agent) fn record_uncertain_input(
        &self,
        tool: &ToolCall,
        output: &str,
    ) -> Result<(), AgentError> {
        if !uncertain_input(tool, output) {
            return Ok(());
        }
        self.initialize_recovery()?;
        self.hub.mutate(|state| {
            let checkpoint = if self.id == "main" {
                state
                    .root_recovery
                    .get_or_insert_with(|| RecoveryCheckpoint::new(vec![]))
            } else {
                state
                    .jobs
                    .get_mut(&self.id)
                    .ok_or_else(AgentError::internal)?
                    .recovery
                    .get_or_insert_with(|| RecoveryCheckpoint::new(vec![]))
            };
            checkpoint.loaded = true;
            if !checkpoint.calls.iter().any(|call| call.id == tool.id) {
                checkpoint.calls.push(Call::new(tool));
                if !checkpoint.uncertain_tools.contains(&tool.name) {
                    checkpoint.uncertain_tools.push(tool.name.clone());
                }
                checkpoint.inspected = false;
            }
            Ok(())
        })
    }
    pub(in crate::agent) fn refresh_recovery_catalog(
        &self,
        read_only_mcp: impl Fn(&str) -> bool,
        server: impl Fn(&str) -> Option<String>,
    ) -> Result<(), AgentError> {
        self.initialize_recovery()?;
        let prune = {
            let state = self
                .hub
                .manifest
                .lock()
                .map_err(|_| AgentError::internal())?;
            self.checkpoint(&state).is_some_and(|checkpoint| {
                checkpoint.calls.iter().any(|call| {
                    call.name.starts_with("mcp_")
                        && (read_only_mcp(&call.name) || call.server != server(&call.name))
                })
            })
        };
        if !prune {
            return Ok(());
        }
        self.hub.mutate(|state| {
            if let Some(checkpoint) = self.checkpoint_mut(state) {
                checkpoint
                    .calls
                    .retain(|call| !call.name.starts_with("mcp_") || !read_only_mcp(&call.name));
                for call in &mut checkpoint.calls {
                    if call.name.starts_with("mcp_") {
                        call.server = server(&call.name);
                    }
                }
                checkpoint.inspected = checkpoint.calls.iter().all(|call| call.outcome.is_some());
            }
            Ok(())
        })
    }

    pub(in crate::agent) fn recovery_mcp_preflight(
        &self,
        tool: &ToolCall,
        mutating: bool,
        server: Option<&str>,
    ) -> Result<(), AgentError> {
        if !mutating || !tool.name.starts_with("mcp_") || server.is_none() {
            return Ok(());
        }
        self.initialize_recovery()?;
        let state = self
            .hub
            .manifest
            .lock()
            .map_err(|_| AgentError::internal())?;
        let conflicting = self.checkpoint(&state).is_some_and(|checkpoint| {
            checkpoint.calls.iter().any(|call| {
                call.outcome.is_none()
                    && call.server.as_deref() == server
                    && !call.selectors.is_empty()
                    && call
                        .selectors
                        .iter()
                        .all(|(key, value)| tool.args.get(key) == Some(value))
            })
        });
        if conflicting {
            return Err(AgentError::new("recovery_inspection_required", "Este recurso possui uma operação incerta no mesmo MCP. Inspecione o estado e registre recovery_resolve antes de alterá-lo novamente."));
        }
        Ok(())
    }

    fn recovery_session(&self) -> Result<Option<Arc<Session>>, AgentError> {
        if self.id == "main" {
            return Ok(Some(self.hub.root.clone()));
        }
        Ok(self
            .hub
            .live
            .lock()
            .map_err(|_| AgentError::internal())?
            .get(&self.id)
            .cloned())
    }

    pub(super) fn initialize_recovery(&self) -> Result<(), AgentError> {
        let needs_loading = {
            let state = self
                .hub
                .manifest
                .lock()
                .map_err(|_| AgentError::internal())?;
            self.checkpoint(&state)
                .is_some_and(|checkpoint| !checkpoint.loaded)
        };
        if !needs_loading {
            return Ok(());
        }
        let Some(session) = self.recovery_session()? else {
            return Ok(());
        };
        self.confirm_preserved_computer_results(&session)?;
        let calls = {
            let data = session.data.lock().map_err(|_| AgentError::internal())?;
            data.turns.last().map(collect).unwrap_or_default()
        };
        self.hub.mutate(|state| {
            if let Some(checkpoint) = self.checkpoint_mut(state) {
                if !checkpoint.loaded {
                    checkpoint.calls = calls;
                    checkpoint.loaded = true;
                    checkpoint.inspected =
                        checkpoint.calls.iter().all(|call| call.outcome.is_some());
                }
            }
            Ok(())
        })
    }

    fn checkpoint<'a>(&self, state: &'a Manifest) -> Option<&'a RecoveryCheckpoint> {
        if self.id == "main" {
            state.root_recovery.as_ref()
        } else {
            state.jobs.get(&self.id)?.recovery.as_ref()
        }
    }
    fn checkpoint_mut<'a>(&self, state: &'a mut Manifest) -> Option<&'a mut RecoveryCheckpoint> {
        if self.id == "main" {
            state.root_recovery.as_mut()
        } else {
            state.jobs.get_mut(&self.id)?.recovery.as_mut()
        }
    }

    pub(super) fn recovery_preflight(
        &self,
        tool: &ToolCall,
        mcp_mutating: bool,
    ) -> Result<(), AgentError> {
        self.initialize_recovery()?;
        if tool.name == TOOL
            || read_only(tool, mcp_mutating)
            || tool.name.starts_with("hub_")
            || matches!(
                tool.name.as_str(),
                "ask_user" | "update_tasks" | "bash_wait" | "bash_cancel"
            )
        {
            return Ok(());
        }
        let state = self
            .hub
            .manifest
            .lock()
            .map_err(|_| AgentError::internal())?;
        let Some(checkpoint) = self.checkpoint(&state) else {
            return Ok(());
        };
        if checkpoint
            .calls
            .iter()
            .any(|call| call.outcome == Some(Outcome::Unavailable) && call.same_ui_target(tool))
        {
            return Err(AgentError::new("recovery_target_unavailable", "Esta janela foi confirmada como indisponível. Preserve o resultado incerto e selecione um novo alvo; não repita a entrada antiga."));
        }
        if checkpoint
            .calls
            .iter()
            .any(|call| call.outcome == Some(Outcome::Applied) && call.signature == signature(tool))
            && !repeatable_check(tool)
        {
            return Err(AgentError::new("recovery_already_applied", "Esta operação já foi confirmada pela inspeção. Use o resultado preservado e continue sem repeti-la."));
        }
        let next = Call::new(tool);
        if checkpoint.calls.iter().any(|call| {
            call.outcome.is_none()
                && (call.signature == signature(tool)
                    || call.same_ui_target(tool)
                    || call.paths.iter().any(|target| {
                        next.paths.iter().any(|path| {
                            resource_path(&self.hub.root.root, target)
                                == resource_path(&self.hub.root.root, path)
                        })
                    })
                    || call.repositories.iter().any(|target| {
                        next.repositories.iter().any(|path| {
                            resource_path(&self.hub.root.root, target)
                                == resource_path(&self.hub.root.root, path)
                        })
                    })
                    || (call.name == tool.name
                        && !call.selectors.is_empty()
                        && call
                            .selectors
                            .iter()
                            .all(|(key, value)| tool.args.get(key) == Some(value))))
        }) {
            return Err(AgentError::new("recovery_inspection_required", "Confira o alvo das operações incertas listadas no checkpoint e registre cada resultado com recovery_resolve. Uma leitura sem relação não confirma o efeito; não repita a mutação."));
        }
        Ok(())
    }

    pub(in crate::agent) fn observe_recovery_result(
        &self,
        tool: &ToolCall,
        mcp_mutating: bool,
        completed: bool,
        server: impl Fn(&str) -> Option<String>,
    ) -> Result<(), AgentError> {
        if !completed || !read_only(tool, mcp_mutating) {
            return Ok(());
        }
        self.initialize_recovery()?;
        if !self.recovery_inspection_pending()? {
            return Ok(());
        }
        // Executors may still hold the pending callback after its result is saved.
        let preserved = if let Some(session) = self.recovery_session()? {
            let data = session.data.lock().map_err(|_| AgentError::internal())?;
            data.turns.last().and_then(|turn| {
                turn.turn
                    .steps
                    .iter()
                    .flat_map(|step| &step.tools)
                    .find(|saved| saved.id == tool.id)
                    .cloned()
            })
        } else {
            None
        };
        if preserved
            .as_ref()
            .is_some_and(|saved| saved.status != "completed")
        {
            return Ok(());
        }
        let tool = preserved.as_ref().unwrap_or(tool);
        let relevant = |call: &Call| {
            let same_mcp = server(&call.name)
                .zip(server(&tool.name))
                .is_some_and(|(a, b)| a == b);
            call.outcome.is_none()
                && !call.evidence.contains_key(&tool.id)
                && call.related(tool, same_mcp, &self.hub.root.root, &self.hub.root.id)
        };
        let has_evidence = {
            let state = self
                .hub
                .manifest
                .lock()
                .map_err(|_| AgentError::internal())?;
            self.checkpoint(&state)
                .is_some_and(|checkpoint| checkpoint.calls.iter().any(&relevant))
        };
        if !has_evidence {
            return Ok(());
        }
        self.hub.mutate(|state| {
            if let Some(checkpoint) = self.checkpoint_mut(state) {
                for call in &mut checkpoint.calls {
                    if relevant(call) {
                        let mut inspected_paths = vec![];
                        for target in &call.paths {
                            let inspected = tool.name == "read"
                                && tool.args["path"].as_str().is_some_and(|path| {
                                    resource_path(&self.hub.root.root, path)
                                        == resource_path(&self.hub.root.root, target)
                                });
                            let missing = tool.name == "list"
                                && tool.args["path"].as_str().is_some_and(|path| {
                                    resource_path(&self.hub.root.root, target).parent()
                                        == Some(resource_path(&self.hub.root.root, path).as_path())
                                })
                                && missing_file(&self.hub.root.root, target);
                            if inspected || missing {
                                inspected_paths.push(target.clone());
                            }
                        }
                        call.evidence.insert(tool.id.clone(), inspected_paths);
                    }
                }
            }
            Ok(())
        })
    }

    pub(super) fn resolve_recovery(&self, args: &Value) -> Result<String, AgentError> {
        self.resolve_recovery_with(args, |conversation, target, call| {
            if let Some(app) = &self.hub.env.browser_app {
                use tauri::Manager;
                app.state::<super::super::computer::ComputerState>()
                    .reconciled(conversation, target, call)?;
            }
            Ok(())
        })
    }

    fn resolve_recovery_with(
        &self,
        args: &Value,
        reconciled: impl FnOnce(&str, &str, &str) -> Result<(), AgentError>,
    ) -> Result<String, AgentError> {
        let id = args["callId"].as_str().ok_or_else(AgentError::internal)?;
        let evidence = args["evidenceCallId"]
            .as_str()
            .ok_or_else(AgentError::internal)?;
        let outcome = match args["outcome"].as_str() {
            Some("applied") => Some(Outcome::Applied),
            Some("not_applied") => Some(Outcome::NotApplied),
            Some("unavailable") => Some(Outcome::Unavailable),
            Some("unknown") => None,
            _ => return Err(invalid("Resultado de recuperação inválido.")),
        };
        self.initialize_recovery()?;
        let call = {
            let state = self
                .hub
                .manifest
                .lock()
                .map_err(|_| AgentError::internal())?;
            self.checkpoint(&state)
                .and_then(|checkpoint| checkpoint.calls.iter().find(|call| call.id == id))
                .cloned()
                .ok_or_else(|| invalid("Chamada incerta não encontrada."))?
        };
        if call.outcome.is_some() && call.outcome != outcome {
            let mut error = AgentError::new("recovery_already_resolved", "Esta operação já possui resultado confirmado. Preserve a resolução registrada e continue sem reclassificá-la.");
            error.tool_result = Some(json!({"code":error.code,"message":error.message,"callId":id,"outcome":call.outcome,"alreadyResolved":true,"pending":self.recovery_inspection_pending()?,"instructions":"Preserve this operation outcome. A later resource state is a new observation, not proof of attribution to this historical call. Continue from the current verified resource state and the user's remaining intent without reclassifying or replaying the operation."}).to_string());
            return Err(error);
        }
        if call.outcome.is_some() {
            return Ok(json!({"callId":id,"outcome":args["outcome"],"alreadyResolved":true,"pending":self.recovery_inspection_pending()?}).to_string());
        }
        if outcome.is_some() {
            if !call.evidence.contains_key(evidence) {
                return Err(invalid("A evidência deve ser uma inspeção bem-sucedida do alvo desta operação, no mesmo serviço."));
            }
            let session = self.recovery_session()?.ok_or_else(AgentError::internal)?;
            let (durable, unavailable) = {
                let data = session.data.lock().map_err(|_| AgentError::internal())?;
                let mut durable = HashSet::new();
                let mut unavailable = false;
                for item in data
                    .turns
                    .last()
                    .into_iter()
                    .flat_map(|turn| &turn.wire)
                    .filter(|item| {
                        item["type"] == "function_call_output"
                            && item["output"].as_str() != Some(journal::UNKNOWN_TOOL_OUTPUT)
                    })
                {
                    if let Some(id) = item["call_id"].as_str() {
                        let saved_tool = data.turns.last().and_then(|turn| {
                            turn.turn
                                .steps
                                .iter()
                                .flat_map(|step| &step.tools)
                                .find(|tool| tool.id == id)
                        });
                        if saved_tool.is_some_and(|tool| tool.status != "completed") {
                            continue;
                        }
                        durable.insert(id.to_owned());
                        if id == evidence {
                            // Model compaction changes wire replay, not the preserved native result.
                            let receipt = saved_tool
                                .map(|tool| tool.output.as_str())
                                .or_else(|| item["output"].as_str())
                                .and_then(|output| serde_json::from_str::<Value>(output).ok());
                            if call.name == "browser_open"
                                && (outcome != Some(Outcome::NotApplied)
                                    || !receipt.as_ref().is_some_and(empty_browser_catalog))
                            {
                                return Err(invalid("A abertura interrompida exige uma lista integrada vazia e o resultado not_applied. Uma aba existente não comprova o resultado desta chamada."));
                            }
                            if call.name == "impeccable"
                                && call.selectors.get("command").and_then(Value::as_str)
                                    == Some("live")
                                && (outcome != Some(Outcome::NotApplied)
                                    || !call.live_args.as_ref().is_some_and(|args| {
                                        receipt.as_ref().is_some_and(|output| {
                                            inactive_impeccable_live(
                                                output,
                                                &self.hub.root.id,
                                                &self.hub.root.root,
                                                args,
                                            )
                                        })
                                    }))
                            {
                                return Err(invalid("A ativação Live interrompida exige live-status sem helper ou sessão ativa, com o gerenciador inativo nesta conversa, e o resultado not_applied."));
                            }
                            unavailable =
                                receipt.is_some_and(|receipt| unavailable_target(&call, &receipt));
                        }
                    }
                }
                (durable, unavailable)
            };
            if !durable.contains(evidence) {
                return Err(invalid("A inspeção ainda não possui resultado preservado. Inspecione novamente o alvo antes de resolver."));
            }
            if outcome == Some(Outcome::Unavailable) && !unavailable {
                return Err(invalid("A ausência do alvo exige um recibo nativo autoritativo da mesma janela; falhas genéricas não são evidência."));
            }
            if unavailable && outcome != Some(Outcome::Unavailable) {
                return Err(invalid("O alvo não está mais disponível. Registre unavailable sem afirmar se a entrada incerta foi aplicada."));
            }
            if !call.paths.iter().all(|path| {
                call.evidence
                    .iter()
                    .any(|(id, paths)| durable.contains(id) && paths.contains(path))
            }) {
                return Err(invalid("Inspecione todos os arquivos afetados antes de concluir a recuperação desta operação."));
            }
            session.flush()?;
            // A full-file write has an exact observable postcondition. Do not
            // accept a model's contrary claim or a partial read as proof of it.
            if let Some(expected) = &call.expected_hash {
                let path = call.paths.first().ok_or_else(AgentError::internal)?;
                let actual = if missing_file(&self.hub.root.root, path) {
                    None
                } else {
                    let path = tools::scoped(&self.hub.root.root, path, false)?;
                    let mut file = std::fs::File::open(path).map_err(|_| {
                        invalid("Não foi possível confirmar o conteúdo atual do arquivo.")
                    })?;
                    let mut digest = Sha256::new();
                    let mut buffer = [0u8; 64 * 1024];
                    loop {
                        let count = std::io::Read::read(&mut file, &mut buffer).map_err(|_| {
                            invalid("Não foi possível confirmar o conteúdo atual do arquivo.")
                        })?;
                        if count == 0 {
                            break;
                        }
                        digest.update(&buffer[..count]);
                    }
                    Some(format!("{:x}", digest.finalize()))
                };
                if outcome == Some(Outcome::Applied) && actual.as_ref() != Some(expected) {
                    return Err(invalid("O arquivo atual não corresponde à escrita interrompida. Preserve o resultado como desconhecido ou examine as diferenças."));
                }
                if outcome == Some(Outcome::NotApplied) && actual.as_ref() == Some(expected) {
                    return Err(invalid(
                        "A escrita já está presente no arquivo. Registre applied e não a repita.",
                    ));
                }
            }
        }
        let output = self.hub.mutate(|state| {
            let checkpoint = self.checkpoint_mut(state).ok_or_else(|| invalid("Não há recuperação pendente."))?;
            let call = checkpoint.calls.iter_mut().find(|call| call.id == id).ok_or_else(|| invalid("Chamada incerta não encontrada."))?;
            call.outcome = outcome;
            checkpoint.inspected = checkpoint.calls.iter().all(|call| call.outcome.is_some());
            Ok(json!({"callId":id,"outcome":args["outcome"],"evidenceCallId":evidence,"pending":!checkpoint.inspected}).to_string())
        })?;
        if outcome.is_some() && super::super::computer::mutating(&call.name) {
            let target = call
                .selectors
                .get("targetId")
                .and_then(Value::as_str)
                .ok_or_else(AgentError::internal)?;
            reconciled(&self.hub.root.id, target, &self.computer_call_id(&call.id))?;
        }
        Ok(output)
    }
}

// Verify the existing parent with the same scope/symlink rules as file tools.
// A failed read for any other reason is not evidence that the file is absent.
fn missing_file(root: &Path, target: &str) -> bool {
    let target = Path::new(target);
    let Some(name) = target.file_name() else {
        return false;
    };
    let parent = target
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let Some(parent) = parent
        .to_str()
        .and_then(|parent| tools::scoped(root, parent, false).ok())
    else {
        return false;
    };
    std::fs::symlink_metadata(parent.join(name))
        .is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tool(id: &str, name: &str, args: Value) -> ToolCall {
        ToolCall {
            id: id.into(),
            name: name.into(),
            args,
            status: "completed".into(),
            output: "observed state".into(),
            duration_ms: 1,
        }
    }

    fn recovering(calls: &[ToolCall]) -> (crate::agent::tests::Fixture, Execution) {
        let (fixture, hub) = super::super::tests::hub();
        hub.root.update(true, |data| {
            let turn = data.turns.last_mut().unwrap();
            for call in calls {
                // Sparse replay after interruption, without UI Step metadata.
                turn.wire.push(json!({"type":"function_call","call_id":call.id,"name":call.name,"arguments":call.args.to_string()}));
                turn.wire.push(json!({"type":"function_call_output","call_id":call.id,"output":journal::UNKNOWN_TOOL_OUTPUT}));
            }
        }).unwrap();
        hub.mutate(|state| {
            state.root_recovery = Some(RecoveryCheckpoint::new(
                calls.iter().map(|call| call.name.clone()).collect(),
            ));
            Ok(())
        })
        .unwrap();
        let exec = Execution {
            hub,
            id: "main".into(),
            role: Role::Builder,
            flow: Flow::Standard,
            scope: vec![".".into()],
        };
        exec.initialize_recovery().unwrap();
        (fixture, exec)
    }

    fn observe(exec: &Execution, call: &ToolCall, server: impl Fn(&str) -> Option<String>) {
        exec.recovery_session()
            .unwrap()
            .unwrap()
            .update(true, |data| {
                data.turns.last_mut().unwrap().wire.push(
                    json!({"type":"function_call_output","call_id":call.id,"output":call.output}),
                );
            })
            .unwrap();
        exec.observe_recovery_result(call, false, true, server)
            .unwrap();
    }

    fn resolve(
        exec: &Execution,
        call: &ToolCall,
        evidence: &ToolCall,
        outcome: &str,
    ) -> Result<String, AgentError> {
        exec.resolve_recovery(
            &json!({"callId":call.id,"evidenceCallId":evidence.id,"outcome":outcome}),
        )
    }

    #[test]
    fn ga4_later_resource_state_does_not_reclassify_a_confirmed_operation() {
        let click = tool(
            "delayed-click",
            "browser_click",
            json!({"id":"cloud-tab","element":"enable"}),
        );
        let (_fixture, exec) = recovering(std::slice::from_ref(&click));
        let first = tool(
            "initial-state",
            "browser_snapshot",
            json!({"id":"cloud-tab"}),
        );
        observe(&exec, &first, |_| None);
        let unresolved: Value =
            serde_json::from_str(&resolve(&exec, &click, &first, "unknown").unwrap()).unwrap();
        assert_eq!(unresolved["pending"], true);
        assert!(exec.recovery_preflight(&click, false).is_err());
        resolve(&exec, &click, &first, "not_applied").unwrap();
        let later = tool("later-state", "browser_snapshot", json!({"id":"cloud-tab"}));
        observe(&exec, &later, |_| None);
        let error = resolve(&exec, &click, &later, "applied").unwrap_err();
        let receipt: Value = serde_json::from_str(error.tool_result.as_ref().unwrap()).unwrap();
        assert_eq!(receipt["code"], "recovery_already_resolved");
        assert_eq!(receipt["callId"], click.id);
        assert_eq!(receipt["outcome"], "not_applied");
        assert_eq!(receipt["alreadyResolved"], true);
        assert!(receipt["instructions"]
            .as_str()
            .unwrap()
            .contains("attribution"));
        let preserved: Value =
            serde_json::from_str(&resolve(&exec, &click, &first, "not_applied").unwrap()).unwrap();
        assert_eq!(preserved["outcome"], "not_applied");
    }

    #[test]
    fn browser_recovery_requires_a_durable_observation_of_the_same_tab() {
        let click = tool(
            "lost-click",
            "browser_click",
            json!({"id":"tab-one","element":"expired"}),
        );
        let (_fixture, exec) = recovering(std::slice::from_ref(&click));
        let other = tool("other-tab", "browser_snapshot", json!({"id":"tab-two"}));
        observe(&exec, &other, |_| None);
        assert!(resolve(&exec, &click, &other, "applied").is_err());
        let independent = tool(
            "independent",
            "browser_fill",
            json!({"id":"tab-two","element":"field","text":"ok"}),
        );
        assert!(exec.recovery_preflight(&independent, false).is_ok());
        let same = tool(
            "same-tab",
            "browser_fill",
            json!({"id":"tab-one","element":"new-field","text":"ok"}),
        );
        assert!(exec.recovery_preflight(&same, false).is_err());
        let state = tool(
            "same-tab-state",
            "browser_snapshot",
            json!({"id":"tab-one"}),
        );
        assert!(exec.recovery_preflight(&state, false).is_ok());
        exec.observe_recovery_result(&state, false, true, |_| None)
            .unwrap();
        assert!(resolve(&exec, &click, &state, "applied").is_err());
        observe(&exec, &state, |_| None);
        resolve(&exec, &click, &state, "applied").unwrap();
        assert!(exec.recovery_preflight(&click, false).is_err());
        assert!(exec.recovery_preflight(&same, false).is_ok());
    }

    #[test]
    fn interrupted_browser_open_can_recover_from_an_empty_owned_catalog() {
        let open = tool(
            "lost-open",
            "browser_open",
            json!({"url":"http://127.0.0.1:5173/"}),
        );
        let (_fixture, exec) = recovering(std::slice::from_ref(&open));
        let mut catalog = tool("owned-tabs", "browser_list", json!({}));
        catalog.output = json!({"backend":"embedded","activeId":null,"tabs":[]}).to_string();
        exec.observe_recovery_result(&catalog, false, true, |_| None)
            .unwrap();
        assert!(resolve(&exec, &open, &catalog, "not_applied").is_err());
        observe(&exec, &catalog, |_| None);
        assert!(resolve(&exec, &open, &catalog, "applied").is_err());
        assert!(exec
            .resolve_recovery(
                &json!({"callId":open.id,"evidenceCallId":"browser_list","outcome":"not_applied"})
            )
            .is_err());
        resolve(&exec, &open, &catalog, "not_applied").unwrap();
        assert!(exec.recovery_preflight(&open, false).is_ok());
        assert!(!exec.recovery_inspection_pending().unwrap());
    }

    #[test]
    fn interrupted_browser_open_rejects_ambiguous_or_invalid_catalogs() {
        let open = tool(
            "lost-open",
            "browser_open",
            json!({"url":"http://127.0.0.1:5173/"}),
        );
        let (_fixture, exec) = recovering(std::slice::from_ref(&open));
        for (index, output) in [
            json!({"backend":"embedded","activeId":"tab","tabs":[{"id":"tab","url":"http://127.0.0.1:5173/"}]}),
            json!({"backend":"extension","activeId":null,"tabs":[]}),
            json!({"backend":"embedded","activeId":"tab","tabs":[]}),
            json!({"backend":"embedded","activeId":null}),
            json!({"tabs":[]}),
        ].into_iter().enumerate() {
            let mut catalog = tool(&format!("catalog-{index}"), "browser_list", json!({}));
            catalog.output = output.to_string();
            observe(&exec, &catalog, |_| None);
            for outcome in ["applied", "not_applied", "unavailable"] {
                assert!(resolve(&exec, &open, &catalog, outcome).is_err());
            }
        }
        assert!(exec.recovery_preflight(&open, false).is_err());
    }

    fn inactive_live_status(exec: &Execution) -> Value {
        json!({
            "liveServer":null,"activeSessions":[],
            "hostStatus":{"conversationId":exec.hub.root.id,"state":"off","phase":"idle"},
            "hostInspection":{"confirmedInactive":true,"projectRoot":exec.hub.root.root,"conversationId":exec.hub.root.id,"target":null}
        })
    }

    #[test]
    fn live_inspections_are_read_only_but_start_and_repairs_are_not() {
        for command in ["live-status", "live-resume"] {
            assert!(read_only(
                &tool(
                    "inspect",
                    "impeccable",
                    json!({"command":command,"args":[]})
                ),
                false
            ));
        }
        for args in [
            json!({"command":"live","args":[]}),
            json!({"command":"doctor","args":["--fix"]}),
        ] {
            assert!(!read_only(&tool("mutate", "impeccable", args), false));
        }
    }

    #[test]
    fn interrupted_live_recovers_from_durable_owned_inactive_status() {
        let start = tool(
            "lost-live",
            "impeccable",
            json!({"command":"live","args":["--dev-url","http://127.0.0.1:5173/"]}),
        );
        let (_fixture, exec) = recovering(std::slice::from_ref(&start));
        assert!(exec.recovery_preflight(&start, false).is_err());
        let mut status = tool(
            "owned-live-status",
            "impeccable",
            json!({"command":"live-status","args":[]}),
        );
        status.output = inactive_live_status(&exec).to_string();
        assert!(exec.recovery_preflight(&status, false).is_ok());
        exec.observe_recovery_result(&status, false, true, |_| None)
            .unwrap();
        assert!(resolve(&exec, &start, &status, "not_applied").is_err());
        observe(&exec, &status, |_| None);
        assert!(exec.context().unwrap().contains("owned-live-status"));
        assert!(exec
            .resolve_recovery(
                &json!({"callId":start.id,"evidenceCallId":"live-status","outcome":"not_applied"})
            )
            .is_err());
        for outcome in ["applied", "unavailable"] {
            assert!(resolve(&exec, &start, &status, outcome).is_err());
        }
        resolve(&exec, &start, &status, "not_applied").unwrap();
        assert!(exec.recovery_preflight(&start, false).is_ok());
        assert!(!exec.recovery_inspection_pending().unwrap());
    }

    #[test]
    fn interrupted_live_rejects_unowned_active_or_incomplete_status() {
        let start = tool(
            "lost-live",
            "impeccable",
            json!({"command":"live","args":[]}),
        );
        let (_fixture, exec) = recovering(std::slice::from_ref(&start));
        let inactive = inactive_live_status(&exec);
        let mut foreign = inactive.clone();
        foreign["hostStatus"]["conversationId"] = json!("another-chat");
        let mut preparing = inactive.clone();
        preparing["hostStatus"]["state"] = json!("preparing");
        let mut helper = inactive.clone();
        helper["liveServer"] = json!({"port":4173});
        let mut active = inactive.clone();
        active["activeSessions"] = json!([{"id":"active-session"}]);
        let mut missing_sessions = inactive.clone();
        missing_sessions
            .as_object_mut()
            .unwrap()
            .remove("activeSessions");
        let mut failed_inspection = inactive.clone();
        failed_inspection["hostInspection"]["confirmedInactive"] = json!(false);
        let mut missing_inspection = inactive.clone();
        missing_inspection
            .as_object_mut()
            .unwrap()
            .remove("hostInspection");
        let mut foreign_project = inactive.clone();
        foreign_project["hostInspection"]["projectRoot"] = json!("/another/project");
        let mut foreign_inspection = inactive.clone();
        foreign_inspection["hostInspection"]["conversationId"] = json!("another-chat");
        let mut foreign_target = inactive.clone();
        foreign_target["hostInspection"]["target"] = json!(exec.hub.root.root.join("another-app"));
        for (index, output) in [
            foreign,
            preparing,
            helper,
            active,
            missing_sessions,
            failed_inspection,
            missing_inspection,
            foreign_project,
            foreign_inspection,
            foreign_target,
            json!({"liveServer":null,"activeSessions":[]}),
            json!({"active":false}),
        ]
        .into_iter()
        .enumerate()
        {
            let mut status = tool(
                &format!("live-inspection-{index}"),
                "impeccable",
                json!({"command":"live-status","args":[]}),
            );
            status.output = output.to_string();
            observe(&exec, &status, |_| None);
            assert!(resolve(&exec, &start, &status, "not_applied").is_err());
        }
        let mut resume = tool(
            "live-resume",
            "impeccable",
            json!({"command":"live-resume","args":[]}),
        );
        resume.output = inactive.to_string();
        observe(&exec, &resume, |_| None);
        assert!(resolve(&exec, &start, &resume, "not_applied").is_err());
        assert!(exec.recovery_preflight(&start, false).is_err());
    }

    #[test]
    fn inactive_live_status_does_not_resolve_other_impeccable_mutations() {
        let mutation = tool(
            "lost-accept",
            "impeccable",
            json!({"command":"live-accept","args":["--id","request"]}),
        );
        let (_fixture, exec) = recovering(std::slice::from_ref(&mutation));
        let mut status = tool(
            "live-status",
            "impeccable",
            json!({"command":"live-status","args":[]}),
        );
        status.output = inactive_live_status(&exec).to_string();
        observe(&exec, &status, |_| None);
        assert!(resolve(&exec, &mutation, &status, "not_applied").is_err());
        assert!(exec.recovery_preflight(&mutation, false).is_err());
    }

    #[test]
    fn interrupted_live_requires_the_same_requested_app_target() {
        let start = tool(
            "lost-live",
            "impeccable",
            json!({"command":"live","args":["--target","app"]}),
        );
        let (_fixture, exec) = recovering(std::slice::from_ref(&start));
        std::fs::create_dir_all(exec.hub.root.root.join("app")).unwrap();
        let mut status = tool(
            "target-live-status",
            "impeccable",
            json!({"command":"live-status","args":["--target=app"]}),
        );
        status.output = inactive_live_status(&exec).to_string();
        observe(&exec, &status, |_| None);
        assert!(resolve(&exec, &start, &status, "not_applied").is_err());
        status.id = "scoped-live-status".into();
        let mut scoped = inactive_live_status(&exec);
        scoped["hostInspection"]["target"] =
            json!(exec.hub.root.root.join("app").canonicalize().unwrap());
        status.output = scoped.to_string();
        observe(&exec, &status, |_| None);
        resolve(&exec, &start, &status, "not_applied").unwrap();
        assert!(exec.recovery_preflight(&start, false).is_ok());
    }

    #[tokio::test]
    async fn cli_inspection_is_observed_only_after_its_result_is_preserved() {
        let write = tool(
            "lost-write",
            "write",
            json!({"path":"state.txt","content":"planned"}),
        );
        let (_fixture, exec) = recovering(std::slice::from_ref(&write));
        std::fs::write(exec.hub.root.root.join("state.txt"), "saved state").unwrap();
        let (_sender, signal) = watch::channel(false);
        let env = &exec.hub.env;
        let mut bridge = crate::agent::claude_executor::bridge::tests::fixture_bridge(
            &exec.hub.root,
            crate::agent::TurnRuntime {
                grants: &env.grants,
                state: &env.state,
                oauth: &env.oauth,
                mcp: &env.mcp,
                home: &env.home,
            },
            crate::agent::tests::options(crate::agent::ApprovalMode::Yolo),
            signal,
        );
        bridge.execution = Some(exec.clone());
        let mut read = tool("cli-read", "read", json!({"path":"state.txt"}));
        read.status = "pending".into();
        read.output.clear();
        let output = bridge.call(&read).await.unwrap();
        assert!(output.contains("saved state"));
        assert!(!exec.context().unwrap().contains("cli-read"));
        let result = crate::agent::claude_executor::execute_tool(
            &mut bridge,
            "inspect",
            &json!({"name":"read","arguments":read.args}),
            Some(read.clone()),
        )
        .await
        .unwrap();
        assert_eq!(result["isError"], false);
        assert!(exec.context().unwrap().contains("cli-read"));
        let model_content = result["content"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|block| block["text"].as_str())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(model_content.contains("Execution recovery checkpoint"));
        assert!(model_content.contains("cli-read"));
        let resolution = tool(
            "cli-resolve",
            TOOL,
            json!({"callId":write.id,"evidenceCallId":read.id,"outcome":"not_applied"}),
        );
        let resolved = crate::agent::claude_executor::execute_tool(
            &mut bridge,
            "resolve",
            &json!({"name":TOOL,"arguments":resolution.args}),
            Some(resolution.clone()),
        )
        .await
        .unwrap();
        assert_eq!(resolved["isError"], false);
        let returned = resolved["content"].as_array().unwrap();
        assert!(returned.iter().any(|block| {
            block["text"]
                .as_str()
                .and_then(|text| serde_json::from_str::<Value>(text).ok())
                .is_some_and(|receipt| receipt["pending"] == false)
        }));
        assert!(!returned.iter().any(|block| block["text"]
            .as_str()
            .is_some_and(|text| text.contains("Execution recovery checkpoint"))));
        assert!(exec.recovery_preflight(&write, false).is_ok());
    }

    #[test]
    fn compact_model_replay_does_not_replace_the_native_recovery_receipt() {
        let start = tool(
            "lost-live",
            "impeccable",
            json!({"command":"live","args":[]}),
        );
        let (_fixture, exec) = recovering(std::slice::from_ref(&start));
        let mut status = tool(
            "live-status-receipt",
            "impeccable",
            json!({"command":"live-status","args":[]}),
        );
        status.output = inactive_live_status(&exec).to_string();
        exec.hub.root.update(true, |data| {
            let turn = data.turns.last_mut().unwrap();
            turn.turn.steps.push(Step {tools:vec![status.clone()], ..Step::default()});
            turn.wire.push(json!({"type":"function_call_output","call_id":status.id,"output":"The complete status was indexed; use the compact model preview."}));
        }).unwrap();
        exec.observe_recovery_result(&status, false, true, |_| None)
            .unwrap();
        resolve(&exec, &start, &status, "not_applied").unwrap();
        assert!(exec.recovery_preflight(&start, false).is_ok());
    }

    #[test]
    fn pending_callback_uses_only_a_completed_preserved_inspection() {
        for saved_status in ["pending", "error", "completed"] {
            let start = tool(
                "lost-live",
                "impeccable",
                json!({"command":"live","args":[]}),
            );
            let (_fixture, exec) = recovering(std::slice::from_ref(&start));
            let mut status = tool(
                "live-status-receipt",
                "impeccable",
                json!({"command":"live-status","args":[]}),
            );
            status.output = inactive_live_status(&exec).to_string();
            status.status = saved_status.into();
            exec.hub.root.update(true, |data| {
                let turn = data.turns.last_mut().unwrap();
                turn.turn.steps.push(Step {tools:vec![status.clone()], ..Step::default()});
                turn.wire.push(json!({"type":"function_call_output","call_id":status.id,"output":status.output}));
            }).unwrap();
            let mut callback = status.clone();
            callback.status = "pending".into();
            callback.output.clear();
            exec.observe_recovery_result(&callback, false, true, |_| None)
                .unwrap();
            assert_eq!(
                exec.context().unwrap().contains(&status.id),
                saved_status == "completed"
            );
            if saved_status == "completed" {
                resolve(&exec, &start, &status, "not_applied").unwrap();
                assert!(exec.recovery_preflight(&start, false).is_ok());
            } else {
                assert!(resolve(&exec, &start, &status, "not_applied").is_err());
            }
        }
    }

    #[test]
    fn invalid_native_recovery_receipts_never_fall_back_to_model_replay() {
        for (saved_status, malformed) in [("pending", false), ("error", false), ("completed", true)]
        {
            let start = tool(
                "lost-live",
                "impeccable",
                json!({"command":"live","args":[]}),
            );
            let (_fixture, exec) = recovering(std::slice::from_ref(&start));
            let mut status = tool(
                "live-status-receipt",
                "impeccable",
                json!({"command":"live-status","args":[]}),
            );
            status.output = inactive_live_status(&exec).to_string();
            let mut raw = status.clone();
            raw.status = saved_status.into();
            if malformed {
                raw.output = "Incomplete native receipt".into();
            }
            exec.hub
                .root
                .update(true, |data| {
                    data.turns.last_mut().unwrap().turn.steps.push(Step {
                        tools: vec![raw],
                        ..Step::default()
                    });
                })
                .unwrap();
            observe(&exec, &status, |_| None);
            assert!(resolve(&exec, &start, &status, "not_applied").is_err());
            assert!(exec.recovery_preflight(&start, false).is_err());
        }
    }

    #[test]
    fn computer_recovery_ignores_expired_observation_ids_but_never_accepts_another_target() {
        let click = tool(
            "lost-native",
            "computer_click",
            json!({"targetId":"window-one","observationId":"old","element":"old-button"}),
        );
        let (_fixture, exec) = recovering(std::slice::from_ref(&click));
        let other = tool(
            "other-window",
            "computer_observe",
            json!({"targetId":"window-two"}),
        );
        observe(&exec, &other, |_| None);
        assert!(resolve(&exec, &click, &other, "not_applied").is_err());
        let input = tool(
            "different-native-action",
            "computer_type",
            json!({"targetId":"window-one","observationId":"new","text":"hello"}),
        );
        assert!(exec.recovery_preflight(&input, false).is_err());
        let seen = tool(
            "fresh-native",
            "computer_observe",
            json!({"targetId":"window-one"}),
        );
        observe(&exec, &seen, |_| None);
        resolve(&exec, &click, &seen, "unknown").unwrap();
        assert!(exec.recovery_preflight(&input, false).is_err());
        resolve(&exec, &click, &seen, "not_applied").unwrap();
        assert!(exec.recovery_preflight(&input, false).is_ok());
    }

    #[test]
    fn a_durable_uncertain_input_opens_live_recovery_and_survives_collection() {
        let click = tool(
            "live-native",
            "computer_click",
            json!({"targetId":"window","observationId":"seen","element":"button"}),
        );
        let (_fixture, exec) = recovering(&[]);
        exec.hub
            .mutate(|state| {
                state.root_recovery = None;
                Ok(())
            })
            .unwrap();
        let mut initial = tools::definitions(Mode::Build);
        exec.filter(&mut initial);
        assert!(
            initial.iter().any(|definition| definition["name"] == TOOL),
            "native input catalogs must include recovery before a CLI caches tools/list"
        );
        let receipt = json!({"targetId":"window","inputSent":true,"code":"computer_action_uncertain","phase":"uncertain"}).to_string();
        exec.hub.root.update(true, |data| {
            let turn = data.turns.last_mut().unwrap();
            turn.wire.push(json!({"type":"function_call","call_id":click.id,"name":click.name,"arguments":click.args.to_string()}));
            turn.wire.push(json!({"type":"function_call_output","call_id":click.id,"output":receipt}));
        }).unwrap();
        exec.record_uncertain_input(&click, &receipt).unwrap();
        assert!(exec.recovery_inspection_pending().unwrap());
        let input = tool(
            "next-native",
            "computer_press",
            json!({"targetId":"window","observationId":"new","key":"Enter"}),
        );
        assert!(exec.recovery_preflight(&input, false).is_err());
        let mut definitions = tools::definitions(Mode::Build);
        exec.filter(&mut definitions);
        assert!(definitions
            .iter()
            .any(|definition| definition["name"] == TOOL));
        let data = exec.hub.root.data.lock().unwrap();
        assert_eq!(collect(data.turns.last().unwrap()).len(), 1);
        drop(data);
        let observed = tool(
            "after-native",
            "computer_observe",
            json!({"targetId":"window"}),
        );
        observe(&exec, &observed, |_| None);
        resolve(&exec, &click, &observed, "not_applied").unwrap();
        assert!(exec.recovery_preflight(&input, false).is_ok());
    }

    #[test]
    fn native_pre_dispatch_errors_do_not_open_uncertain_recovery() {
        let click = tool(
            "native-refused",
            "computer_click",
            json!({"targetId":"window","observationId":"seen","element":"button"}),
        );
        let (_fixture, exec) = recovering(&[]);
        exec.hub
            .mutate(|state| {
                state.root_recovery = None;
                Ok(())
            })
            .unwrap();
        for receipt in [
            json!({"targetId":"window","inputSent":false,"code":"computer_action_uncertain","phase":"uncertain"}),
            json!({"targetId":"other","inputSent":true,"code":"computer_action_uncertain","phase":"uncertain"}),
            json!({"targetId":"window","inputSent":true,"code":"generic_error","phase":"error"}),
        ] {
            exec.record_uncertain_input(&click, &receipt.to_string())
                .unwrap();
        }
        assert!(!exec.recovery_inspection_pending().unwrap());
    }

    #[test]
    fn durable_resolution_survives_checkpoint_reconstruction_and_cannot_be_downgraded() {
        let click = tool(
            "resolved-native",
            "computer_click",
            json!({"targetId":"window","observationId":"seen","element":"button"}),
        );
        let (_fixture, exec) = recovering(std::slice::from_ref(&click));
        let observed = tool(
            "proof-native",
            "computer_observe",
            json!({"targetId":"window"}),
        );
        observe(&exec, &observed, |_| None);
        let output = resolve(&exec, &click, &observed, "applied").unwrap();
        assert!(resolve(&exec, &click, &observed, "unknown").is_err());
        let resolution = tool(
            "save-native-resolution",
            TOOL,
            json!({"callId":click.id,"evidenceCallId":observed.id,"outcome":"applied"}),
        );
        exec.hub.root.update(true, |data| {
            let turn = data.turns.last_mut().unwrap();
            turn.wire.push(json!({"type":"function_call","call_id":resolution.id,"name":resolution.name,"arguments":resolution.args.to_string()}));
            turn.wire.push(json!({"type":"function_call_output","call_id":resolution.id,"output":output}));
        }).unwrap();
        exec.hub
            .mutate(|state| {
                state.root_recovery = Some(RecoveryCheckpoint::new(vec![click.name.clone()]));
                Ok(())
            })
            .unwrap();
        exec.initialize_recovery().unwrap();
        assert!(!exec.recovery_inspection_pending().unwrap());
        assert!(exec.recovery_preflight(&click, false).is_err());
    }

    #[test]
    fn native_resume_replays_only_preserved_observed_success_receipts() {
        let click = tool(
            "lost-ack-native",
            "computer_click",
            json!({"targetId":"window","observationId":"seen","element":"button"}),
        );
        let lost = tool(
            "lost-receipt-native",
            "computer_press",
            json!({"targetId":"window","observationId":"seen","key":"Enter"}),
        );
        let (_fixture, exec) = recovering(std::slice::from_ref(&lost));
        let receipt = json!({"phase":"observed","inputSent":true,"targetId":"window","operationId":"preserved-operation"}).to_string();
        exec.hub.root.update(true, |data| {
            let turn = data.turns.last_mut().unwrap();
            turn.wire.push(json!({"type":"function_call","call_id":click.id,"name":click.name,"arguments":click.args.to_string()}));
            turn.wire.push(json!({"type":"function_call_output","call_id":click.id,"output":receipt}));
        }).unwrap();
        let mut acknowledged = vec![];
        exec.confirm_preserved_computer_results_with(&exec.hub.root, |tool, output| {
            let (turns, _) = journal::read_only(&exec.hub.root.journal)?;
            assert!(turns.last().unwrap().wire.iter().any(|item| {
                item["type"] == "function_call_output"
                    && item["call_id"] == tool.id
                    && item["output"] == output
            }));
            acknowledged.push((tool.id.clone(), observed_input(tool, output).unwrap()));
            Ok(())
        })
        .unwrap();
        assert_eq!(
            acknowledged,
            vec![(click.id, ("window".into(), "preserved-operation".into()))]
        );
        let data = exec.hub.root.data.lock().unwrap();
        let uncertain = collect(data.turns.last().unwrap());
        assert_eq!(uncertain.len(), 1);
        assert_eq!(uncertain[0].id, lost.id);
    }

    #[test]
    fn native_success_is_acknowledged_only_with_preserved_receipt_and_host_operation() {
        let click = tool(
            "ack-native",
            "computer_click",
            json!({"targetId":"window","observationId":"seen","element":"button"}),
        );
        let (_fixture, exec) = recovering(&[]);
        let receipt = json!({"phase":"observed","inputSent":true,"targetId":"window","operationId":"native-operation","conversationId":"forged","callId":"forged"}).to_string();
        assert!(exec
            .confirm_computer_result_with(&click, &receipt, |_, _, _, _| {
                panic!("an unpreserved receipt must not unlock input")
            })
            .is_err());
        for refused in [
            json!({"phase":"uncertain","inputSent":true,"targetId":"window","operationId":"native-operation"}),
            json!({"phase":"observed","inputSent":false,"targetId":"window","operationId":"native-operation"}),
            json!({"phase":"observed","inputSent":true,"targetId":"other","operationId":"native-operation"}),
            json!({"phase":"observed","inputSent":true,"targetId":"window","operationId":""}),
        ] {
            exec.confirm_computer_result_with(&click, &refused.to_string(), |_, _, _, _| {
                panic!("an invalid receipt must not unlock input")
            })
            .unwrap();
        }
        exec.hub.root.update(true, |data| {
            let turn = data.turns.last_mut().unwrap();
            turn.wire.push(json!({"type":"function_call","call_id":click.id,"name":click.name,"arguments":click.args.to_string()}));
            turn.wire.push(json!({"type":"function_call_output","call_id":click.id,"output":receipt}));
        }).unwrap();
        let acknowledged = std::cell::Cell::new(false);
        exec.confirm_computer_result_with(
            &click,
            &receipt,
            |conversation, call, target, operation| {
                assert_eq!(conversation, exec.hub.root.id);
                assert_eq!(call, format!("main:{}", click.id));
                assert_eq!(target, "window");
                assert_eq!(operation, "native-operation");
                let (turns, _) = journal::read_only(&exec.hub.root.journal)?;
                assert!(turns.last().unwrap().wire.iter().any(|item| {
                    item["type"] == "function_call_output"
                        && item["call_id"] == click.id
                        && item["output"] == receipt
                }));
                acknowledged.set(true);
                Ok(())
            },
        )
        .unwrap();
        assert!(acknowledged.get());
        let mut stale = click.clone();
        stale.id = "older-call".into();
        assert!(exec
            .confirm_computer_result_with(&stale, &receipt, |_, _, _, _| {
                panic!("the preserved receipt belongs to a different call")
            })
            .is_err());
        let other_operation = receipt.replace("native-operation", "another-operation");
        assert!(exec
            .confirm_computer_result_with(&click, &other_operation, |_, _, _, _| {
                panic!("the preserved receipt belongs to a different operation")
            })
            .is_err());
    }

    #[test]
    fn equal_provider_ids_between_workers_do_not_release_other_input_and_survive_resume() {
        let (_fixture, hub) = super::super::tests::hub();
        let call = tool(
            "same-provider-id",
            "computer_type",
            json!({"targetId":"window","observationId":"seen","text":"draft"}),
        );
        let prepare = |output: Option<&str>| {
            let mut job = super::super::tests::job(&hub, Role::Builder, ".");
            job.recovery = Some(RecoveryCheckpoint::new(vec![call.name.clone()]));
            hub.mutate(|state| {
                state.jobs.insert(job.id.clone(), job.clone());
                Ok(())
            })
            .unwrap();
            let (session, _) = storage::worker(&hub, &job, None).unwrap();
            session.update(true, |data| {
                let turn = data.turns.last_mut().unwrap();
                turn.wire.push(json!({"type":"function_call","call_id":call.id,"name":call.name,"arguments":call.args.to_string()}));
                if let Some(output) = output {
                    turn.wire.push(json!({"type":"function_call_output","call_id":call.id,"output":output}));
                }
            }).unwrap();
            hub.live
                .lock()
                .unwrap()
                .insert(job.id.clone(), session.clone());
            let exec = Execution {
                hub: hub.clone(),
                id: job.id.clone(),
                role: job.role,
                flow: Flow::Complete,
                scope: job.scope.clone(),
            };
            exec.initialize_recovery().unwrap();
            (job, session, exec)
        };
        // B stopped before dispatch; its journal has no native result/nonce.
        let (_job_b, session_b, exec_b) = prepare(None);
        let receipt = json!({"targetId":"window","phase":"uncertain","inputSent":true,"code":"computer_action_uncertain","operationId":"operation-a"}).to_string();
        let (mut job_a, session_a, exec_a) = prepare(Some(&receipt));
        let internal_a = exec_a.computer_call_id(&call.id);
        let internal_b = exec_b.computer_call_id(&call.id);
        assert_ne!(internal_a, internal_b);
        let pending = std::cell::RefCell::new(Some(internal_a.clone()));
        let released_keys = std::cell::RefCell::new(vec![]);
        let release = |conversation: &str, target: &str, key: &str| {
            assert_eq!(conversation, hub.root.id);
            assert_eq!(target, "window");
            released_keys.borrow_mut().push(key.to_owned());
            let mut pending = pending.borrow_mut();
            if pending.as_deref() == Some(key) {
                *pending = None;
            }
            Ok(())
        };
        let observed_b = tool(
            "worker-b-proof",
            "computer_observe",
            json!({"targetId":"window"}),
        );
        observe(&exec_b, &observed_b, |_| None);
        exec_b
            .resolve_recovery_with(
                &json!({"callId":call.id,"evidenceCallId":observed_b.id,"outcome":"not_applied"}),
                release,
            )
            .unwrap();
        assert_eq!(released_keys.borrow().as_slice(), &[internal_b]);
        assert_eq!(pending.borrow().as_deref(), Some(internal_a.as_str()));
        assert!(exec_a.recovery_inspection_pending().unwrap());
        for session in [&session_a, &session_b] {
            let (turns, _) = journal::read_only(&session.journal).unwrap();
            let calls: Vec<_> = turns
                .last()
                .unwrap()
                .wire
                .iter()
                .filter(|item| item["type"] == "function_call")
                .map(|item| item["call_id"].as_str().unwrap())
                .collect();
            assert_eq!(calls, vec![call.id.as_str()]);
        }
        let previous_owner = exec_a.terminal_owner_id().unwrap();
        crate::agent::finish(
            &session_a,
            Err(AgentError::new(
                "interrupted",
                "Interrupção após a entrada nativa.",
            )),
        );
        hub.live.lock().unwrap().remove(&exec_a.id);
        drop(session_a);
        job_a.recovery = Some(RecoveryCheckpoint::new(vec![call.name.clone()]));
        job_a.status = Status::Interrupted;
        job_a.run_id = "resumed-run".into();
        hub.mutate(|state| {
            state.run_id = job_a.run_id.clone();
            state.jobs.insert(job_a.id.clone(), job_a.clone());
            Ok(())
        })
        .unwrap();
        let (resumed, _) =
            storage::worker(&hub, &job_a, Some("Continue the same assignment".into())).unwrap();
        hub.live
            .lock()
            .unwrap()
            .insert(job_a.id.clone(), resumed.clone());
        let resumed_exec = Execution {
            hub: hub.clone(),
            id: job_a.id.clone(),
            role: job_a.role,
            flow: Flow::Complete,
            scope: job_a.scope.clone(),
        };
        assert_ne!(previous_owner, resumed_exec.terminal_owner_id().unwrap());
        assert_eq!(resumed_exec.computer_call_id(&call.id), internal_a);
        resumed_exec.initialize_recovery().unwrap();
        let observed_a = tool(
            "resumed-proof",
            "computer_observe",
            json!({"targetId":"window"}),
        );
        observe(&resumed_exec, &observed_a, |_| None);
        resumed_exec
            .resolve_recovery_with(
                &json!({"callId":call.id,"evidenceCallId":observed_a.id,"outcome":"not_applied"}),
                release,
            )
            .unwrap();
        assert!(pending.borrow().is_none());
        assert_eq!(released_keys.borrow().last(), Some(&internal_a));
        assert!(!resumed_exec.recovery_inspection_pending().unwrap());
        let (turns, _) = journal::read_only(&resumed.journal).unwrap();
        assert!(turns
            .last()
            .unwrap()
            .wire
            .iter()
            .any(|item| { item["type"] == "function_call" && item["call_id"] == call.id }));
    }

    #[test]
    fn native_control_is_reconciled_only_after_durable_proof_with_host_target_and_conversation() {
        let click = tool(
            "controlled-native",
            "computer_click",
            json!({"targetId":"window","observationId":"seen","element":"button"}),
        );
        let (_fixture, exec) = recovering(std::slice::from_ref(&click));
        let observed = tool(
            "control-proof",
            "computer_observe",
            json!({"targetId":"window"}),
        );
        let args = json!({"callId":click.id,"evidenceCallId":observed.id,"outcome":"not_applied","conversationId":"forged","targetId":"forged"});
        assert!(exec
            .resolve_recovery_with(&args, |_, _, _| panic!(
                "no evidence must not clear control uncertainty"
            ))
            .is_err());
        observe(&exec, &observed, |_| None);
        let cleared = std::cell::Cell::new(false);
        let mut unknown = args.clone();
        unknown["outcome"] = json!("unknown");
        exec.resolve_recovery_with(&unknown, |_, _, _| {
            cleared.set(true);
            Ok(())
        })
        .unwrap();
        assert!(!cleared.get());
        exec.resolve_recovery_with(&args, |conversation, target, call| {
            assert_eq!(conversation, exec.hub.root.id);
            assert_eq!(target, "window");
            assert_eq!(call, format!("main:{}", click.id));
            let saved: Value = serde_json::from_slice(
                &std::fs::read(exec.hub.directory.join("state.json")).unwrap(),
            )
            .unwrap();
            assert_eq!(saved["rootRecovery"]["calls"][0]["outcome"], "not_applied");
            cleared.set(true);
            Ok(())
        })
        .unwrap();
        assert!(cleared.get());
        cleared.set(false);
        let later = tool(
            "later-native-incident",
            "computer_press",
            json!({"targetId":"window","observationId":"fresh","key":"Enter"}),
        );
        let receipt = json!({"targetId":"window","code":"computer_action_uncertain","phase":"uncertain","inputSent":true}).to_string();
        exec.record_uncertain_input(&later, &receipt).unwrap();
        exec.resolve_recovery_with(&args, |_, _, _| {
            cleared.set(true);
            Ok(())
        })
        .unwrap();
        assert!(
            !cleared.get(),
            "replaying an older resolution must not clear a later target incident"
        );
        assert!(exec.recovery_inspection_pending().unwrap());
    }

    #[test]
    fn unavailable_native_targets_require_authoritative_same_target_evidence() {
        let click = tool(
            "closed-native",
            "computer_click",
            json!({"targetId":"window","observationId":"seen","element":"button"}),
        );
        let (_fixture, exec) = recovering(std::slice::from_ref(&click));
        let mut observed = tool(
            "generic-failure",
            "computer_observe",
            json!({"targetId":"window"}),
        );
        observed.output = json!({"targetId":"window","available":false,"inputSent":false,"code":"computer_backend_failed","phase":"observed"}).to_string();
        observe(&exec, &observed, |_| None);
        assert!(resolve(&exec, &click, &observed, "unavailable").is_err());
        let mut missing = tool(
            "closed-window",
            "computer_observe",
            json!({"targetId":"window"}),
        );
        missing.output = json!({"targetId":"window","available":false,"inputSent":false,"code":"computer_target_unavailable","phase":"observed"}).to_string();
        observe(&exec, &missing, |_| None);
        assert!(resolve(&exec, &click, &missing, "applied").is_err());
        assert!(resolve(&exec, &click, &missing, "not_applied").is_err());
        let mut lost_binding = tool(
            "lost-binding",
            "computer_observe",
            json!({"targetId":"window"}),
        );
        lost_binding.output = json!({"targetId":"window","available":false,"inputSent":false,"code":"computer_target_unavailable","reason":"host_binding_missing","effect":"unknown","phase":"observed"}).to_string();
        observe(&exec, &lost_binding, |_| None);
        assert!(resolve(&exec, &click, &lost_binding, "applied").is_err());
        assert!(resolve(&exec, &click, &lost_binding, "not_applied").is_err());
        resolve(&exec, &click, &lost_binding, "unavailable").unwrap();
        resolve(&exec, &click, &missing, "unavailable").unwrap();
        assert!(!exec.recovery_inspection_pending().unwrap());
        assert!(exec.recovery_preflight(&click, false).is_err());
        let replacement = tool(
            "new-native",
            "computer_click",
            json!({"targetId":"replacement","observationId":"fresh","element":"button"}),
        );
        assert!(exec.recovery_preflight(&replacement, false).is_ok());
    }

    #[test]
    fn interrupted_structural_reads_never_become_uncertain_project_mutations() {
        let calls = [
            tool(
                "lost-graft-read",
                "graft_file_api",
                json!({"file":"result.ts"}),
            ),
            tool(
                "lost-graft-search",
                "graft_find_code",
                json!({"query":"authentication"}),
            ),
            tool(
                "lost-write",
                "write",
                json!({"path":"result.ts","content":"confirmed"}),
            ),
        ];
        let (_fixture, exec) = recovering(&calls);
        let data = exec.hub.root.data.lock().unwrap();
        let uncertain = collect(data.turns.last().unwrap());
        assert_eq!(uncertain.len(), 1);
        assert_eq!(uncertain[0].name, "write");
        drop(data);
        let graph = tool(
            "current-graft",
            "graft_file_api",
            json!({"file":"result.ts"}),
        );
        observe(&exec, &graph, |_| None);
        assert!(
            resolve(&exec, &calls[2], &graph, "applied").is_err(),
            "signatures and graph coverage do not prove a lost source mutation was applied"
        );
    }

    #[test]
    fn lost_write_receipt_requires_exact_durable_evidence_and_survives_checkpoint_reload() {
        let write = tool(
            "lost-write",
            "write",
            json!({"path":"result.txt","content":"confirmed content"}),
        );
        let (_fixture, exec) = recovering(std::slice::from_ref(&write));
        std::fs::write(exec.hub.root.root.join("result.txt"), "confirmed content").unwrap();
        let unrelated = tool("unrelated", "ctx_stats", json!({}));
        observe(&exec, &unrelated, |_| None);
        assert!(resolve(&exec, &write, &unrelated, "applied").is_err());
        let read = tool("read-result", "read", json!({"path":"./result.txt"}));
        exec.observe_recovery_result(&read, false, true, |_| None)
            .unwrap();
        assert!(
            resolve(&exec, &write, &read, "applied").is_err(),
            "unpersisted evidence cannot release a mutation"
        );
        let read = tool("fresh-read-result", "read", json!({"path":"result.txt"}));
        observe(&exec, &read, |_| None);
        assert!(
            resolve(&exec, &write, &read, "not_applied").is_err(),
            "existing exact content must not be overwritten blindly"
        );
        resolve(&exec, &write, &read, "applied").unwrap();
        let stored = serde_json::to_vec(&*exec.hub.manifest.lock().unwrap()).unwrap();
        *exec.hub.manifest.lock().unwrap() = serde_json::from_slice(&stored).unwrap();
        assert_eq!(
            exec.recovery_preflight(&write, false).unwrap_err().code,
            "recovery_already_applied"
        );
        let next = tool("next", "write", json!({"path":"next.txt","content":"next"}));
        assert!(exec.recovery_preflight(&next, false).is_ok());
    }

    #[test]
    fn multifile_patch_requires_every_target_and_an_unrelated_call_stays_uncertain() {
        let patch = tool(
            "patch",
            "apply_patch",
            json!({"patchText":"*** Begin Patch\n*** Add File: a.txt\n+one\n*** Add File: b.txt\n+two\n*** End Patch"}),
        );
        let other = tool("other", "write", json!({"path":"c.txt","content":"three"}));
        let (_fixture, exec) = recovering(&[patch.clone(), other.clone()]);
        let first = tool("read-a", "read", json!({"path":"a.txt"}));
        observe(&exec, &first, |_| None);
        assert!(resolve(&exec, &patch, &first, "applied").is_err());
        assert!(resolve(&exec, &other, &first, "applied").is_err());
        let overwrite = tool(
            "overwrite",
            "write",
            json!({"path":"b.txt","content":"changed"}),
        );
        assert!(exec.recovery_preflight(&overwrite, false).is_err());
        let second = tool("read-b", "read", json!({"path":"b.txt"}));
        observe(&exec, &second, |_| None);
        resolve(&exec, &patch, &second, "applied").unwrap();
        assert!(exec.recovery_inspection_pending().unwrap());
        assert!(exec.recovery_preflight(&other, false).is_err());
    }

    #[test]
    fn absent_file_can_be_reconciled_without_creating_it() {
        let write = tool(
            "new-file",
            "write",
            json!({"path":"absent.txt","content":"new"}),
        );
        let (_fixture, exec) = recovering(std::slice::from_ref(&write));
        let list = tool("list-parent", "list", json!({"path":"."}));
        observe(&exec, &list, |_| None);
        resolve(&exec, &write, &list, "not_applied").unwrap();
        assert!(!exec.hub.root.root.join("absent.txt").exists());
        assert!(exec.recovery_preflight(&write, false).is_ok());
    }

    #[test]
    #[cfg(unix)]
    fn missing_file_under_a_symlink_is_the_same_recovery_target() {
        let write = tool(
            "file",
            "write",
            json!({"path":"real/new.txt","content":"new"}),
        );
        let (_fixture, exec) = recovering(std::slice::from_ref(&write));
        std::fs::create_dir(exec.hub.root.root.join("real")).unwrap();
        std::os::unix::fs::symlink("real", exec.hub.root.root.join("alias")).unwrap();
        let through_alias = tool(
            "alias-write",
            "write",
            json!({"path":"alias/new.txt","content":"changed"}),
        );
        assert!(exec.recovery_preflight(&through_alias, false).is_err());
        let list = tool("list-alias", "list", json!({"path":"alias"}));
        observe(&exec, &list, |_| None);
        resolve(&exec, &write, &list, "not_applied").unwrap();
    }

    #[test]
    fn listing_one_parent_does_not_confirm_missing_files_in_another() {
        let patch = tool(
            "patch",
            "apply_patch",
            json!({"patchText":"*** Begin Patch\n*** Add File: a/a.txt\n+one\n*** Add File: b/b.txt\n+two\n*** End Patch"}),
        );
        let (_fixture, exec) = recovering(std::slice::from_ref(&patch));
        for directory in ["a", "b"] {
            std::fs::create_dir(exec.hub.root.root.join(directory)).unwrap();
        }
        let first = tool("list-a", "list", json!({"path":"a"}));
        observe(&exec, &first, |_| None);
        assert!(resolve(&exec, &patch, &first, "not_applied").is_err());
        let second = tool("list-b", "list", json!({"path":"b"}));
        observe(&exec, &second, |_| None);
        resolve(&exec, &patch, &second, "not_applied").unwrap();
    }

    #[test]
    fn publication_reconciliation_protects_the_repository_when_the_proposal_changes() {
        let publication = tool(
            "publication",
            "jarvis_propose_publication",
            json!({"repositories":[{"path":"backend","push":"normal"}]}),
        );
        let (_fixture, exec) = recovering(std::slice::from_ref(&publication));
        let changed = tool(
            "changed",
            "jarvis_propose_publication",
            json!({"summary":"new summary","repositories":[{"path":"./backend","push":"normal"}]}),
        );
        assert!(exec.recovery_preflight(&changed, false).is_err());
        let independent = tool(
            "other",
            "jarvis_propose_publication",
            json!({"repositories":[{"path":"frontend","push":"normal"}]}),
        );
        assert!(exec.recovery_preflight(&independent, false).is_ok());
        let local = tool(
            "local",
            "jarvis_inspect_publication",
            json!({"paths":["backend"]}),
        );
        observe(&exec, &local, |_| None);
        assert!(resolve(&exec, &publication, &local, "applied").is_err());
        let remote = tool(
            "remote",
            "bash",
            json!({"command":"git -C backend ls-remote origin"}),
        );
        observe(&exec, &remote, |_| None);
        resolve(&exec, &publication, &remote, "applied").unwrap();
    }

    #[test]
    fn git_push_requires_same_repository_and_remote_inspection() {
        let push = tool(
            "push",
            "bash",
            json!({"command":"git -C backend push origin HEAD"}),
        );
        let (_fixture, exec) = recovering(std::slice::from_ref(&push));
        for (id, command) in [
            ("wrong-repo", "git -C frontend ls-remote origin"),
            ("local-only", "git -C backend status --short"),
        ] {
            let read = tool(id, "bash", json!({"command":command}));
            observe(&exec, &read, |_| None);
            assert!(resolve(&exec, &push, &read, "applied").is_err());
        }
        let remote = tool(
            "remote-ref",
            "bash",
            json!({"command":"git -C backend ls-remote origin"}),
        );
        observe(&exec, &remote, |_| None);
        resolve(&exec, &push, &remote, "applied").unwrap();
    }

    #[test]
    fn mcp_evidence_matches_server_and_all_resource_selectors() {
        let write = tool(
            "mcp-write",
            "mcp_write_digest",
            json!({"notebookId":"one","id":"two"}),
        );
        let read_only = tool("lost-read", "mcp_known_read", json!({"id":"read"}));
        let (_fixture, exec) = recovering(&[write.clone(), read_only]);
        exec.refresh_recovery_catalog(
            |name| name == "mcp_known_read",
            |_| Some("notebooks".into()),
        )
        .unwrap();
        let another_tool = tool("mcp-delete", "mcp_delete_digest", write.args.clone());
        assert!(exec
            .recovery_mcp_preflight(&another_tool, true, Some("notebooks"))
            .is_err());
        assert!(exec
            .recovery_mcp_preflight(&another_tool, true, Some("database"))
            .is_ok());
        assert_eq!(
            exec.hub
                .manifest
                .lock()
                .unwrap()
                .root_recovery
                .as_ref()
                .unwrap()
                .calls
                .len(),
            1
        );
        let servers = |name: &str| {
            Some(
                if name == "mcp_wrong_service" {
                    "database"
                } else {
                    "notebooks"
                }
                .to_owned(),
            )
        };
        for (id, name, notebook) in [
            ("wrong-service", "mcp_wrong_service", "one"),
            ("wrong-target", "mcp_read_digest", "other"),
        ] {
            let read = tool(id, name, json!({"notebookId":notebook,"id":"two"}));
            observe(&exec, &read, servers);
            assert!(resolve(&exec, &write, &read, "applied").is_err());
        }
        let read = tool(
            "right-target",
            "mcp_read_digest",
            json!({"notebookId":"one","id":"two"}),
        );
        observe(&exec, &read, servers);
        resolve(&exec, &write, &read, "applied").unwrap();
    }

    #[test]
    fn unsupported_effect_does_not_stop_independent_work_or_require_user_permission() {
        let effect = tool("create", "mcp_create", json!({"payload":{"text":"hello"}}));
        let (_fixture, exec) = recovering(std::slice::from_ref(&effect));
        resolve(&exec, &effect, &tool("none", "read", json!({})), "unknown").unwrap();
        assert!(exec.recovery_preflight(&effect, true).is_err());
        assert!(exec
            .recovery_preflight(
                &tool(
                    "file",
                    "write",
                    json!({"path":"independent.txt","content":"work"})
                ),
                false
            )
            .is_ok());
        assert!(exec.recovery_inspection_pending().unwrap());
        let mut definitions = vec![];
        exec.filter(&mut definitions);
        let catalog = super::super::super::tool_contract::Catalog::new(&definitions);
        let capability = catalog.capabilities(TOOL).unwrap();
        assert!(!capability.parallel_safe);
        assert_eq!(
            capability.approval,
            super::super::super::tool_contract::ApprovalPolicy::Never
        );
    }

    #[test]
    fn confirmed_verification_can_run_again_but_unknown_checks_and_pushes_cannot() {
        let check = tool("check", "bash", json!({"command":"npm run check"}));
        let (_fixture, exec) = recovering(std::slice::from_ref(&check));
        assert!(exec.recovery_preflight(&check, false).is_err());
        exec.hub
            .manifest
            .lock()
            .unwrap()
            .root_recovery
            .as_mut()
            .unwrap()
            .calls[0]
            .outcome = Some(Outcome::Applied);
        assert!(exec.recovery_preflight(&check, false).is_ok());
        assert!(!repeatable_check(&tool(
            "push",
            "bash",
            json!({"command":"npm test && git push"})
        )));
        assert!(!read_only(
            &tool("bead", "project_beads_update", json!({"id":"x"})),
            false
        ));
    }
}
