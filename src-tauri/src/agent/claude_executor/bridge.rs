//! Native executors own inference; effects cross the existing Jarvis tool contracts.
use super::super::*;
use tool_contract::{Handler, Orchestrator};

pub(in crate::agent) struct Bridge<'a> {
    pub session: &'a Arc<Session>,
    pub runtime: TurnRuntime<'a>,
    pub execution: Option<workflow::Execution>,
    pub options: TurnOptions,
    pub signal: watch::Receiver<bool>,
    pub context: crate::core::context::ContextMode,
    pub manual_hooks: Arc<crate::hooks::runtime::Runtime>,
    frozen_skills: Arc<[crate::skills::Skill]>,
    pub graft: crate::core::graft::Graft,
    pub clients: crate::mcp::runtime::TurnClients,
    mcp_intent: crate::mcp::McpIntent,
    beads: Option<crate::core::beads::Beads>,
    project_beads: Option<crate::core::beads::ProjectBeads>,
    design: Option<crate::core::design::Pack>,
    lsp: lsp::Registry,
    commands: command_sessions::CommandSessions,
    video_jobs: video::Jobs,
    instructions: instructions::Resolver,
    repeated: tool_loop::Guard,
    mcp_name_recovery: Option<(ToolCall, Vec<String>)>,
    mcp_name_reminded: bool,
    diagnostics: Vec<String>,
    direct_tasks: bool,
    restricted: bool,
    publication: bool,
    pub prompt: String,
    pub delivered_wire: usize,
    pub request_scope: String,
}

impl<'a> Bridge<'a> {
    pub async fn new(
        session: &'a Arc<Session>,
        runtime: TurnRuntime<'a>,
        execution: Option<workflow::Execution>,
        options: TurnOptions,
        signal: watch::Receiver<bool>,
        frozen_skills: Arc<[crate::skills::Skill]>,
    ) -> Result<Self, AgentError> {
        let owner = execution.as_ref().map_or(session, |exec| exec.root());
        let global_companion = companion_chat::is_global_session(&owner.id);
        let image_specialist = execution
            .as_ref()
            .is_some_and(workflow::Execution::image_generator);
        let home = runtime.home;
        let publication = execution
            .as_ref()
            .is_some_and(workflow::Execution::publication);
        let restricted = execution
            .as_ref()
            .map_or(options.mode == Mode::Plan, |exec| {
                exec.role_mode() == Mode::Plan
            });
        let direct_tasks = !global_companion && options.direct() && owner.id == session.id;
        let intent = {
            let data = session.data.lock().map_err(|_| AgentError::internal())?;
            data.turns
                .last()
                .and_then(|turn| turn.mcp_intent.clone())
                .unwrap_or_else(|| data.inherited_mcp_intent.clone())
        };
        let clients = if global_companion {
            crate::mcp::runtime::TurnClients::default()
        } else {
            crate::mcp::runtime::TurnClients::discover_for_intent(
                runtime.mcp,
                runtime.state,
                home,
                &session.root,
                &intent,
                signal.clone(),
            )
            .await?
        };
        let context = if global_companion {
            crate::core::context::ContextMode::without_project(&session.root, &session.id)
        } else {
            crate::core::context::ContextMode::open(
                home,
                &session.root,
                &session.id,
                signal.clone(),
            )
            .await?
        };
        let graft = if global_companion
            || execution
                .as_ref()
                .is_some_and(workflow::Execution::video_specialist)
        {
            crate::core::graft::Graft::inactive()
        } else {
            crate::core::graft::Graft::open(home, &session.root, signal.clone()).await?
        };
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
        if !image_specialist {
            core_runtime::prepare_graft(session, &graft, &user, signal.clone()).await?;
        }
        let beads = if (direct_tasks && !image_specialist) || publication || global_companion {
            None
        } else {
            Some(crate::core::beads::Beads::new(
                home,
                owner.project_id()?,
                &owner.id,
                image_specialist || options.mode == Mode::Plan,
            )?)
        };
        let project_beads = if !global_companion && (direct_tasks || image_specialist) {
            crate::core::beads::ProjectBeads::open(home, &session.root)?
        } else {
            None
        };
        let mut activities = graft.take_activity();
        let mut prompt = tools::instructions(&session.root, options.mode, options.approval_mode);
        if !global_companion
            && !publication
            && self_development::available(runtime.state, home, &session.root, owner.project_id()?)
        {
            prompt.push_str(self_development::INSTRUCTIONS);
        }
        let backend = "Claude Code";
        let mcp_dispatcher = "call_mcp_tool";
        prompt.push_str(&format!("\nExecution backend: {backend}. Keep your native reasoning and conversation management. All project operations, commands, tasks, questions, workflow coordination, approvals and external integrations are exposed by the Jarvis MCP server. Use these tools rather than describing actions for the user to execute. Jarvis owns their permissions and durable results. Do not create a second task/agent system. MCP discovery results include availableTools with exact schemas. Execute newly available tools and discovery controls through {mcp_dispatcher} using their exact name and arguments; no new user message or tools/list refresh is required. Search results marked loaded:true already include their schema; do not load them again. Native built-in tools are intentionally disabled to preserve the selected Jarvis role, project scope and approval contract.\n"));
        prompt.push_str(&crate::library::repositories::prompt(
            runtime.state,
            home,
            owner.project_id()?,
        )?);
        if let Some(exec) = &execution {
            exec.refresh_recovery_catalog(
                |name| !clients.requires_active_task(name),
                |name| {
                    clients
                        .tool_metadata(name)
                        .map(|(server, _, _)| server.to_owned())
                },
            )?;
            prompt.push_str(&exec.instructions()?);
            if !global_companion || image_specialist {
                prompt.push_str(&exec.context()?);
            }
        }
        prompt.push_str(context.instructions());
        prompt.push_str(graft.instructions());
        if direct_tasks {
            prompt.push_str(tasks::INSTRUCTIONS);
            prompt.push_str(&session.task_context()?);
        }
        if project_beads.is_some() {
            prompt.push_str(crate::core::beads::PROJECT_INSTRUCTIONS);
        }
        if let Some(beads) = &beads {
            activities.push(core_runtime::beads_activity());
            prompt.push_str(crate::core::beads::INSTRUCTIONS);
            let snapshot = beads
                .resume(signal.clone(), || {
                    library::agent_location(runtime.state, home, &owner.id)
                        .map(|_| ())
                        .map_err(|_| crate::core::error("Projeto indisponível."))
                })
                .await?;
            let snapshot = format!("\nBeads state (reference data):\n{snapshot}");
            prompt.push_str(&snapshot);
        }
        if options.mode == Mode::Build {
            prompt.push_str(&publication::instructions(&publication::load(
                runtime.state,
                home,
                owner.project_id()?,
            )?));
        }
        if publication && !global_companion {
            prompt.push_str(MCP_REGISTRATION_INSTRUCTIONS);
        }
        if !publication && !global_companion {
            prompt.push_str(authoring::INSTRUCTIONS);
            prompt.push_str(
                &crate::plugins::runtime_prompt(home, &session.root)
                    .map_err(|cause| AgentError::new(cause.code, &cause.message))?,
            );
            prompt.push_str(web_search::instructions(web_search::enabled(
                runtime.state,
                home,
                &options,
            )));
            let skills = crate::skills::authorized_snapshot(home, &session.root, &frozen_skills)
                .await
                .map_err(|error| AgentError::new("skill_error", &error.message))?;
            prompt.push_str(&crate::skills::prompt(&skills));
            if crate::core::context7::configured(home) {
                prompt.push_str(crate::core::context7::INSTRUCTIONS);
            }
        }
        let design = if execution
            .as_ref()
            .is_some_and(workflow::Execution::design_resources)
        {
            match crate::core::design::Pack::open(home) {
                Ok(pack) => Some(pack),
                Err(error) => {
                    activities.push(crate::core::activity::Activity::unavailable(
                        crate::core::ComponentId::OpenDesign,
                        "design_preparation",
                        &error.message,
                    ));
                    None
                }
            }
        } else {
            None
        };
        if let (Some(pack), Some(exec)) = (&design, &execution) {
            let (mut scope, brief) = exec.design_inputs()?;
            if exec.direct() {
                scope.extend(crate::library::repositories::configured_paths(
                    runtime.state,
                    home,
                    owner.project_id()?,
                )?);
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
            if let Some(activity) = core_runtime::prepare_design(
                session,
                pack.prepare_context(&session.root, &user, &scope, &brief),
                &mut None,
                false,
            )? {
                activities.push(activity);
            }
        }
        if !activities.is_empty() {
            session
                .update_async(|data| {
                    if let Some(turn) = data.turns.last_mut() {
                        turn.turn.steps.push(Step {
                            core_activities: activities,
                            ..Step::default()
                        });
                    }
                })
                .await?;
        }
        if global_companion {
            let instructions = if execution
                .as_ref()
                .is_some_and(workflow::Execution::image_generator)
            {
                execution
                    .as_ref()
                    .ok_or_else(AgentError::internal)?
                    .instructions()?
            } else {
                companion_chat::global_prompt().into()
            };
            prompt = format!("{instructions}\nExecution backend: {backend}. Use only the Jarvis MCP tools advertised for this conversation. Native built-in tools and project filesystem operations are disabled until the user confirms a project in the Jarvito interface. Managed image attachments may be generated without granting project access.\n");
            if image_specialist {
                prompt = format!("{instructions}\nExecution backend: {backend}. Use only the native image, attachment, question, knowledge and handoff tools advertised by Jarvis. Built-in Claude tools and alternate image engines are disabled. Use the exact native generation request; its provider and local ComfyUI processing are managed by Jarvis.\n");
                let data = owner.data.lock().map_err(|_| AgentError::internal())?;
                if let Some(turn) = data.turns.last() {
                    prompt.push_str(&attachments::prompt(&turn.turn.parts));
                }
                if direct_tasks {
                    prompt.push_str(tasks::INSTRUCTIONS);
                    prompt.push_str(&session.task_context()?);
                }
            }
        }
        context.hooks.before_agent(&mut prompt);
        tools::append_response_language(&mut prompt, crate::system::response_language(home));
        Ok(Self {
            session,
            runtime,
            execution,
            options,
            signal,
            context,
            manual_hooks: Arc::new(crate::hooks::runtime::Runtime::inactive()),
            frozen_skills,
            graft,
            clients,
            mcp_intent: intent,
            beads,
            project_beads,
            design,
            lsp: lsp::Registry::new(&session.root, home)?,
            commands: command_sessions::CommandSessions::default(),
            video_jobs: video::Jobs::default(),
            instructions: instructions::Resolver::new(&session.root)?,
            repeated: tool_loop::Guard::default(),
            mcp_name_recovery: None,
            mcp_name_reminded: false,
            diagnostics: vec![],
            direct_tasks,
            restricted,
            publication,
            prompt,
            delivered_wire: 0,
            request_scope: crate::claude::new_session_id().map_err(super::runtime_error)?,
        })
    }

    fn owner(&self) -> &Arc<Session> {
        self.execution
            .as_ref()
            .map_or(self.session, |exec| exec.root())
    }

    pub async fn definitions(&mut self) -> Result<Vec<Value>, AgentError> {
        self.refresh_mcp_intent().await?;
        let image_specialist = self
            .execution
            .as_ref()
            .is_some_and(workflow::Execution::image_generator);
        if image_specialist && companion_chat::is_global_session(&self.owner().id) {
            let mut definitions = workflow::image_tools(
                image_generation::enabled(self.runtime.state, self.runtime.home),
                Some(super::native_vision::definition()),
            );
            if let Some(exec) = &self.execution {
                exec.filter(&mut definitions);
            }
            return Ok(definitions);
        }
        if companion_chat::is_global_session(&self.owner().id) {
            let mut definitions = if image_specialist {
                vec![attachments::definition()]
            } else {
                companion_chat::tools()
            };
            definitions.push(questions::definition());
            if !image_specialist
                && web_search::enabled(self.runtime.state, self.runtime.home, &self.options)
            {
                definitions.push(web_search::definition());
            }
            if image_generation::enabled(self.runtime.state, self.runtime.home) {
                definitions.push(workflow::image_definition(image_specialist));
            }
            if image_specialist {
                definitions.extend(image_tasks::definitions());
            }
            if let Some(exec) = &self.execution {
                exec.filter(&mut definitions);
            }
            return Ok(definitions);
        }
        let mut definitions = tools::definitions(self.options.mode);
        definitions.push(publication::inspection::definition());
        definitions.push(attachments::project_definition());
        if self.options.mode == Mode::Build {
            definitions.push(publication::definition());
        }
        if self.direct_tasks {
            definitions.push(tasks::definition());
        }
        if self.beads.is_some() {
            definitions.extend(crate::core::beads::definitions(
                image_specialist || self.options.mode == Mode::Plan,
            ));
        }
        if self.project_beads.is_some() {
            definitions.extend(crate::core::beads::project_definitions());
        }
        if self.design.is_some() {
            definitions.extend(crate::core::design::definitions());
        }
        definitions.extend(self.context.definitions(self.restricted));
        definitions.extend(self.graft.definitions());
        definitions.extend(authoring_tools_for_turn(
            if self.restricted {
                Mode::Plan
            } else {
                self.options.mode
            },
            self.publication,
        ));
        if !self.publication {
            self.frozen_skills = crate::skills::refresh_snapshot(
                self.runtime.home,
                &self.session.root,
                &self.frozen_skills,
            )
            .await
            .map_err(|cause| AgentError::new("skill_error", &cause.message))?;
            if self_development::available(
                self.runtime.state,
                self.runtime.home,
                &self.session.root,
                self.owner().project_id()?,
            ) {
                definitions.extend(self_development::definitions());
            }
            definitions.extend([
                crate::skills::definition(),
                crate::skills::search_definition(),
            ]);
            if crate::core::context7::configured(self.runtime.home) {
                definitions.extend(crate::core::context7::definitions());
            }
            definitions.push(super::native_vision::definition());
            if web_search::enabled(self.runtime.state, self.runtime.home, &self.options) {
                definitions.push(web_search::definition());
            }
        }
        definitions.extend(
            self.clients
                .definitions_with(
                    self.runtime.mcp,
                    self.runtime.state,
                    self.runtime.home,
                    self.restricted,
                    |name| {
                        self.execution
                            .as_ref()
                            .is_none_or(|exec| exec.allowed(name))
                    },
                )
                .await,
        );
        if self.execution.is_some()
            && image_generation::enabled(self.runtime.state, self.runtime.home)
        {
            definitions.push(workflow::image_definition(image_specialist));
        }
        if image_specialist {
            definitions.extend(image_tasks::definitions());
        }
        if let Some(exec) = &self.execution {
            exec.filter(&mut definitions);
        }
        self.clients.ensure_scope_visible(&definitions)?;
        Ok(definitions)
    }

    pub async fn call(&mut self, tool: &ToolCall) -> Result<String, AgentError> {
        let mut result = self.call_inner(tool).await;
        if result
            .as_ref()
            .is_err_and(|error| error.code == "tool_unavailable")
        {
            let definitions = self.definitions().await?;
            if let Some(feedback) =
                self.clients
                    .unavailable_tool_feedback(&tool.name, &tool.args, &definitions)
            {
                self.mcp_name_recovery = Some((tool.clone(), feedback.tool_names));
                if let Err(error) = &mut result {
                    error.tool_result = Some(feedback.output);
                }
            }
        } else if self
            .mcp_name_recovery
            .as_ref()
            .is_some_and(|(_, candidates)| candidates.contains(&tool.name))
        {
            // An actual canonical result owns further recovery, including
            // validation, authorization and uncertain-effect errors.
            self.mcp_name_recovery = None;
        }
        result
    }

    async fn call_inner(&mut self, tool: &ToolCall) -> Result<String, AgentError> {
        if *self.signal.borrow() {
            return Err(AgentError::cancelled());
        }
        let definitions = self.definitions().await?;
        let mut contract = Orchestrator::new(&definitions);
        for name in definitions
            .iter()
            .filter_map(|definition| definition["name"].as_str())
            .filter(|name| name.starts_with("mcp_"))
        {
            contract.register_external(name, self.clients.requires_active_task(name));
        }
        let prepared = contract.preflight(tool)?;
        self.repeated.before_call(tool)?;
        if let Some(reason) = self
            .execution
            .as_ref()
            .and_then(|exec| exec.preflight(tool))
            .or_else(|| publication::blocks_unsupervised_tool(tool))
            .or_else(|| {
                self.context
                    .pre_tool(&tool.name, &tool.args)
                    .map(str::to_owned)
            })
            .or_else(|| {
                self.clients
                    .tool_metadata(&tool.name)
                    .and_then(|(server, name, description)| {
                        publication::blocks_unsupervised_mcp(server, name, description)
                    })
            })
        {
            return Err(AgentError::new("tool_preflight", &reason));
        }
        if self.instructions.discover(tool)? {
            let mut prompt = String::new();
            self.instructions.append_prompt(&mut prompt);
            return Err(AgentError::new("project_instructions", &format!("New applicable project instructions loaded. Review them and retry the intended operation:\n{prompt}")));
        }
        let requires_task = if tool.name.starts_with("mcp_") {
            self.clients.requires_active_task(&tool.name)
        } else {
            tasks::requires_active_task_for(tool)
        };
        if self.direct_tasks && requires_task && !self.session.has_active_task()? {
            return Err(AgentError::new(
                "task_required",
                "Use update_tasks para manter uma tarefa em andamento antes de alterar o projeto.",
            ));
        }
        let mut policy =
            execution_policy::inspect_tool(&self.session.root, tool, prepared.capabilities)?;
        let video_preflight =
            video::approval_preflight(self.runtime.home, &self.session.root, tool).await?;
        let explicit_video_approval = video_preflight.required;
        if let Some(policy) = &mut policy {
            execution_policy::apply_video_preflight(policy, tool, &video_preflight);
        }
        let sandbox = policy.as_ref().and_then(execution_sandbox::prepare);
        let project_id = self.owner().project_id()?.to_owned();
        let mut policy_ask = false;
        let mut sandbox_ask = false;
        if let Some(policy) = &policy {
            if policy.outcome.decision == execution_policy::ExecutionDecision::Deny {
                return Err(AgentError::new("execution_denied", &policy.outcome.reason));
            }
            let granted = self
                .runtime
                .grants
                .authorize(
                    &policy.outcome,
                    &tool.name,
                    &tool.args,
                    execution_grants::GrantContext {
                        conversation_id: &self.session.id,
                        project_id: &project_id,
                        working_directory: &policy.working_directory,
                    },
                    now(),
                )
                .map_err(|message| AgentError::new("execution_grant", &message))?
                .is_some();
            policy_ask = policy.outcome.decision == execution_policy::ExecutionDecision::Ask
                && !granted
                && tool.name != "jarvis_propose_publication";
            sandbox_ask = !granted
                && (policy.outcome.native_working_directory.is_some()
                    || sandbox.as_ref().is_some_and(|plan| {
                        plan.requires_informed_approval(&policy.outcome.effects)
                    }));
        }
        if let Some(reason) = run_manual_hook(
            self.session,
            &self.manual_hooks,
            crate::hooks::Event::PreToolUse,
            json!({"tool_name":tool.name,"tool_input":tool.args,"tool_use_id":tool.id}),
            self.signal.clone(),
        )
        .await?
        {
            return Err(AgentError::new("hook_denied", &reason));
        }
        let terminal_ask = self
            .execution
            .as_ref()
            .map(|exec| exec.terminal_close_requires_approval(tool))
            .transpose()?
            .unwrap_or(false);
        if !authorize_prepared(
            ApprovalRequest {
                session: self.session,
                tool,
                options: &self.options,
                policy,
                sandbox: sandbox.as_ref(),
                project_id: Some(&project_id),
                manual_hooks: Some(&self.manual_hooks),
                explicit_video_approval,
                signal: self.signal.clone(),
            },
            prepared,
            terminal_ask,
            policy_ask,
            sandbox_ask,
        )
        .await?
        {
            return Err(AgentError::new(
                "denied",
                "A execução foi recusada pelo usuário.",
            ));
        }
        let execution = self.execution.clone();
        let _guard = match &execution {
            Some(exec) => {
                exec.recovery_mcp_preflight(
                    tool,
                    requires_task,
                    self.clients
                        .tool_metadata(&tool.name)
                        .map(|(server, _, _)| server),
                )?;
                exec.mutation_guard(
                    tool,
                    tool.name.starts_with("mcp_") && requires_task,
                    self.signal.clone(),
                )
                .await?
            }
            None => None,
        };
        let result = self
            .dispatch(
                tool,
                prepared.handler,
                sandbox.as_ref(),
                explicit_video_approval,
            )
            .await;
        let _ = self.repeated.observe(
            tool,
            result.as_ref().err(),
            result
                .as_ref()
                .map_or_else(|error| error.message.as_str(), String::as_str),
        );
        if let Some(exec) = &self.execution {
            exec.observe_recovery_result(
                tool,
                tool.name.starts_with("mcp_") && requires_task,
                result.is_ok(),
                |name| {
                    self.clients
                        .tool_metadata(name)
                        .map(|(server, _, _)| server.to_owned())
                },
            )?;
        }
        result
    }

    pub async fn discovery_content(
        &mut self,
        tool: &ToolCall,
        output: &str,
        status: &str,
    ) -> Result<Option<Value>, AgentError> {
        let registered = matches!(
            tool.name.as_str(),
            "jarvis_propose_mcp" | "jarvis_propose_plugin"
        ) && status == "completed"
            && serde_json::from_str::<Value>(output)
                .is_ok_and(|result| result["approved"] == true && result["status"] == "applied");
        let discovery = matches!(
            tool.name.as_str(),
            "mcp_activate" | "mcp_search_tools" | "mcp_load_tool"
        );
        let unavailable = status == "error"
            && serde_json::from_str::<Value>(output).is_ok_and(|result| {
                matches!(
                    result["error"]["code"].as_str(),
                    Some("tool_unavailable" | "mcp_scope_violation")
                )
            });
        if !(discovery && status != "error" || unavailable || registered) {
            return Ok(None);
        }
        let definitions = self.definitions().await?;
        let schemas = if unavailable {
            self.clients
                .unavailable_tool_feedback(&tool.name, &tool.args, &definitions)
                .map_or_else(Vec::new, |feedback| feedback.schemas)
        } else {
            self.clients
                .discovery_schemas(&tool.name, &tool.args, output, &definitions)
        };
        if !tool.name.starts_with("mcp_") && !registered && schemas.is_empty() {
            return Ok(None);
        }
        let capability_context = if (registered || unavailable)
            && !self.publication
            && !companion_chat::is_global_session(&self.owner().id)
        {
            let mut context = crate::plugins::runtime_prompt(self.runtime.home, &self.session.root)
                .map_err(|cause| AgentError::new(cause.code, &cause.message))?;
            let skills = crate::skills::authorized_snapshot(
                self.runtime.home,
                &self.session.root,
                &self.frozen_skills,
            )
            .await
            .map_err(|cause| AgentError::new("skill_error", &cause.message))?;
            context.push_str(&crate::skills::prompt(&skills));
            context
        } else {
            String::new()
        };
        if schemas.is_empty() && capability_context.is_empty() {
            return Ok(None);
        }
        let call_with = json!("mcp__jarvis__call_mcp_tool");
        Ok(Some(json!({
            "type":"text",
            "text":json!({
                "availableTools":schemas,
                "callWith":call_with,
                "catalogChanged":true,
                "capabilityContext":capability_context,
                "next":"Continue the current request in this same execution using the exact advertised schemas through callWith. No new user message or tools/list refresh is needed. Do not replay confirmed actions.",
            }).to_string(),
        })))
    }

    async fn dispatch(
        &mut self,
        tool: &ToolCall,
        handler: Handler,
        sandbox: Option<&execution_sandbox::SandboxPlan>,
        explicit_video_approval: bool,
    ) -> Result<String, AgentError> {
        let home = self.runtime.home;
        let state = self.runtime.state;
        let signal = self.signal.clone();
        let owner = self
            .execution
            .as_ref()
            .map_or(self.session, |exec| exec.root());
        if !companion_chat::is_global_session(&self.session.id) {
            learning::prepare(self.session, owner, state, home).await;
        }
        match handler {
            Handler::Companion => {
                let app = self
                    .execution
                    .as_ref()
                    .and_then(workflow::Execution::native_app)
                    .ok_or_else(AgentError::internal)?;
                companion_chat::execute(app, state, home, self.session, tool, signal)
                    .await
                    .map(|value| value.to_string())
            }
            Handler::Knowledge => learning::retrieve(state, home, owner, &tool.args),
            Handler::Learning => learning::remember(state, home, owner, &tool.args),
            Handler::PublicationInspection => {
                publication::inspection::inspect(&self.session.root, &tool.args, signal).await
            }
            Handler::SelfDevelopment => {
                self_development::execute(
                    state,
                    home,
                    &self.session.root,
                    owner.project_id()?,
                    tool,
                )
                .await
            }
            Handler::Workflow => {
                if tool.name == "hub_complete" {
                    let definitions = self.definitions().await?;
                    if let Some(feedback) = mcp_tool_recovery_feedback(
                        &self.clients,
                        &definitions,
                        &mut self.mcp_name_recovery,
                        &mut self.mcp_name_reminded,
                    )? {
                        let mut error = tool_contract::recoverable(
                            "mcp_tool_name_recovery",
                            &tool.name,
                            "O handoff não foi registrado. Continue com o nome exato da ferramenta MCP disponível nesta execução.",
                            vec![],
                        );
                        let mut feedback: Value =
                            serde_json::from_str(&feedback).map_err(|_| AgentError::internal())?;
                        feedback["error"] = json!({
                            "code":error.code, "tool":tool.name, "message":error.message,
                        });
                        error.tool_result = Some(feedback.to_string());
                        return Err(error);
                    }
                    if !self.commands.running_ids().is_empty() {
                        return Err(AgentError::new(
                            "commands_running",
                            "Aguarde ou cancele os comandos ativos antes do handoff.",
                        ));
                    }
                    if core_runtime::diagnose(
                        self.session,
                        &mut self.lsp,
                        &mut self.diagnostics,
                        signal.clone(),
                    )
                    .await?
                    {
                        return Err(AgentError::new(
                            "diagnostics_feedback",
                            "Revise os novos problemas de código antes do handoff; corrija o que foi introduzido ou descreva o impacto e a ação necessária para uma pendência real.",
                        ));
                    }
                }
                match &self.execution {
                    Some(exec) => exec.execute_sandboxed(tool, sandbox, signal).await,
                    None => Err(AgentError::new(
                        "workflow_error",
                        "Coordenação indisponível neste modo.",
                    )),
                }
            }
            Handler::DirectTasks => tasks::execute(self.session, &tool.args),
            Handler::AskUser => {
                questions::execute(
                    self.session,
                    tool,
                    signal,
                    crate::system::ask_user_timeout_seconds(home),
                )
                .await
            }
            Handler::JarvisAuthoring => {
                authoring::execute(
                    self.session,
                    self.owner(),
                    state,
                    self.runtime.oauth,
                    self.runtime.mcp,
                    home,
                    tool,
                    signal,
                )
                .await
            }
            Handler::Beads => {
                if let Some(exec) = &self.execution {
                    workflow::validation::closure(exec, tool, signal.clone()).await?;
                }
                let beads = self.beads.as_ref().ok_or_else(AgentError::internal)?;
                let owner = self.owner();
                beads
                    .execute(
                        &tool.name,
                        &tool.args,
                        &format!("{}:{}", self.session.id, tool.id),
                        signal,
                        || {
                            library::agent_location(state, home, &owner.id)
                                .map(|_| ())
                                .map_err(|_| crate::core::error("Projeto indisponível."))
                        },
                    )
                    .await
                    .map_err(AgentError::from)
            }
            Handler::ProjectBeads => self
                .project_beads
                .as_ref()
                .ok_or_else(AgentError::internal)?
                .execute(&tool.name, &tool.args, signal)
                .await
                .map_err(AgentError::from),
            Handler::Design => self
                .design
                .as_ref()
                .ok_or_else(AgentError::internal)?
                .execute(&tool.name, &tool.args)
                .map_err(AgentError::from),
            Handler::ContextMode => self
                .context
                .execute(&tool.name, &tool.args, self.restricted, signal)
                .await
                .map_err(AgentError::from),
            Handler::Graft => {
                core_runtime::execute_graft(&self.graft, &tool.name, &tool.args, signal).await
            }
            Handler::Context7 => crate::core::context7::execute(
                home,
                &self.session.root,
                &tool.name,
                &tool.args,
                signal,
            )
            .await
            .map_err(AgentError::from),
            Handler::Mcp => {
                let result = self
                    .clients
                    .execute(
                        self.runtime.mcp,
                        state,
                        home,
                        &tool.name,
                        &tool.args,
                        self.restricted,
                        signal,
                    )
                    .await;
                core_runtime::record_async(self.session, self.clients.take_activity()).await?;
                result.map_err(AgentError::from)
            }
            Handler::Lsp => self.lsp.execute(tool, signal).await,
            Handler::Attachment => attachments::read_tool_with_project(
                home,
                &self.owner().id,
                (!companion_chat::is_global_session(&self.owner().id))
                    .then_some(self.session.root.as_path()),
                &tool.args,
            ),
            Handler::ImageGeneration => {
                let exec = self.execution.as_ref().ok_or_else(AgentError::internal)?;
                if exec.image_generator() {
                    let owner = self.owner().clone();
                    image_generation::execute_pipeline(
                        state,
                        self.runtime.oauth,
                        home,
                        &owner.id,
                        &owner,
                        &mut self.commands,
                        sandbox,
                        &tool.args,
                        signal,
                    )
                    .await
                } else {
                    exec.delegate_image(self.session, &tool.args, signal).await
                }
            }
            Handler::Vision => {
                self.native_vision_content(&tool.args)?;
                Ok(super::native_vision::receipt(&tool.args))
            }
            Handler::WebSearch => {
                web_search::execute(
                    state,
                    self.runtime.oauth,
                    home,
                    &self.options,
                    crate::system::response_language(home),
                    &tool.args,
                    signal,
                )
                .await
            }
            Handler::SkillRead => {
                core_runtime::read_skill(self.session, home, &tool.args, &self.frozen_skills).await
            }
            Handler::SkillSearch => {
                let skills = crate::skills::authorized_snapshot(
                    home,
                    &self.session.root,
                    &self.frozen_skills,
                )
                .await
                .map_err(|error| AgentError::new("skill_error", &error.message))?;
                crate::skills::search(&skills, &tool.args)
                    .map_err(|error| AgentError::new("skill_error", &error.message))
            }
            Handler::Patch => {
                let outcome =
                    patch::execute(&self.session.root, &tool.args, self.options.mode, signal)
                        .await?;
                for revision in outcome.revisions {
                    diffs::record(self.owner(), revision).await?;
                }
                self.diagnostics.extend(outcome.changed_paths);
                self.diagnostics.extend(outcome.diagnostic_paths);
                if let Some(exec) = &self.execution {
                    exec.record_confirmed_progress()?;
                }
                Ok(outcome.output)
            }
            Handler::Native => {
                if tool.name == "image_process"
                    && self
                        .execution
                        .as_ref()
                        .is_some_and(workflow::Execution::image_generator)
                {
                    let owner = self.owner().clone();
                    return image_tasks::execute(
                        state,
                        self.runtime.oauth,
                        home,
                        &owner.id,
                        &owner,
                        &mut self.commands,
                        sandbox,
                        &tool.args,
                        signal,
                    )
                    .await;
                }
                if tool.name.starts_with("video_") {
                    let owner = self.owner().clone();
                    return self
                        .video_jobs
                        .execute(
                            &mut self.commands,
                            video::Context {
                                session: &owner,
                                home,
                                approved: self.options.approval_mode == ApprovalMode::Yolo
                                    || explicit_video_approval,
                                app: self
                                    .execution
                                    .as_ref()
                                    .and_then(workflow::Execution::native_app),
                            },
                            tool,
                            sandbox,
                            signal,
                        )
                        .await;
                }
                if command_sessions::CommandSessions::handles(&tool.name) {
                    return self
                        .commands
                        .execute(&self.session.root, tool, sandbox, signal)
                        .await;
                }
                let result = tools::execute_with_revision_sandboxed(
                    &self.session.root,
                    tool,
                    self.options.mode,
                    sandbox,
                    signal,
                )
                .await?;
                if let Some(revision) = result.revision {
                    self.diagnostics.push(revision.path.clone());
                    diffs::record(self.owner(), revision).await?;
                    if let Some(exec) = &self.execution {
                        exec.record_confirmed_progress()?;
                    }
                }
                Ok(result.output)
            }
            // These API-specific tools are absent from this executor's catalog.
            Handler::Progress => Err(AgentError::new(
                "tool_unavailable",
                "Ferramenta indisponível para este executor.",
            )),
        }
    }

    pub fn native_vision_content(&self, args: &Value) -> Result<Vec<Value>, AgentError> {
        if self.publication
            || self
                .execution
                .as_ref()
                .is_some_and(|exec| !exec.allowed("vision"))
        {
            return Err(AgentError::new(
                "tool_unavailable",
                "Vision não está disponível para este papel.",
            ));
        }
        super::native_vision::content(self.runtime.home, &self.owner().id, args)
    }

    pub async fn refresh_mcp_intent(&mut self) -> Result<(), AgentError> {
        if !companion_chat::is_global_session(&self.owner().id) {
            let changed = refresh_user_mcp_intent(
                self.session,
                self.runtime,
                &mut self.clients,
                &mut self.mcp_intent,
                self.execution.as_ref(),
                self.signal.clone(),
            )
            .await?;
            if changed {
                let definitions = self
                    .clients
                    .definitions_with(
                        self.runtime.mcp,
                        self.runtime.state,
                        self.runtime.home,
                        self.restricted,
                        |name| {
                            self.execution
                                .as_ref()
                                .is_none_or(|exec| exec.allowed(name))
                        },
                    )
                    .await;
                self.clients.ensure_scope_visible(&definitions)?;
                let instructions = self.clients.instructions();
                let content = format!(
                    "The latest user guidance updated the MCP scope for this execution. {instructions}\nCurrent MCP schemas, callable through the stable Jarvis gateway:\n{}",
                    json!({"callWith":"mcp__jarvis__call_mcp_tool", "availableTools":definitions}),
                );
                self.session
                    .update_async(|data| {
                        data.turns.last_mut().unwrap().wire.push(json!({
                            "role":"user", "_jarvis_runtime":true, "content":content,
                        }));
                    })
                    .await?;
            }
            if let Some(exec) = &self.execution {
                exec.deliver(self.session)?;
            }
        }
        Ok(())
    }

    pub async fn completion_feedback(&mut self) -> Result<Option<String>, AgentError> {
        self.refresh_mcp_intent().await?;
        let definitions = self.definitions().await?;
        if let Some(feedback) = mcp_tool_recovery_feedback(
            &self.clients,
            &definitions,
            &mut self.mcp_name_recovery,
            &mut self.mcp_name_reminded,
        )? {
            return Ok(Some(feedback));
        }
        if !self.commands.running_ids().is_empty() {
            return Ok(Some("Commands are still running. Use bash_wait to obtain their results or bash_cancel before finishing.".into()));
        }
        if core_runtime::diagnose(
            self.session,
            &mut self.lsp,
            &mut self.diagnostics,
            self.signal.clone(),
        )
        .await?
        {
            return Ok(Some(core_runtime::DIAGNOSTIC_REVIEW.into()));
        }
        if self.clients.requires_explicit_attempt() {
            return Ok(Some(self.clients.explicit_reminder()));
        }
        if self.direct_tasks && self.session.has_unfinished_tasks()? {
            return Ok(Some("Update the Jarvis task list before finishing. Mark verified outcomes completed and real unresolved dependencies blocked.".into()));
        }
        if let Some(exec) = &self.execution {
            if companion_chat::is_global_session(&self.owner().id) && !exec.image_generator() {
                return Ok(None);
            }
            if exec.barrier(self.session, self.signal.clone()).await? {
                return Ok(Some(exec.context()?));
            }
            if !exec.has_handoff()? {
                return Ok(Some("Deliver your structured result with hub_complete, including evidence, validation and limitations. Do not claim success without evidence.".into()));
            }
        }
        Ok(None)
    }
}

#[cfg(test)]
pub(in crate::agent) mod tests {
    use super::*;

    pub(in crate::agent) fn fixture_bridge<'a>(
        session: &'a Arc<Session>,
        runtime: TurnRuntime<'a>,
        options: TurnOptions,
        signal: watch::Receiver<bool>,
    ) -> Bridge<'a> {
        let direct_tasks = options.direct();
        Bridge {
            session,
            runtime,
            execution: None,
            options,
            signal,
            context: crate::core::context::ContextMode::without_project(&session.root, &session.id),
            manual_hooks: Arc::new(crate::hooks::runtime::Runtime::inactive()),
            frozen_skills: Arc::from([]),
            graft: crate::core::graft::Graft::inactive(),
            clients: crate::mcp::runtime::TurnClients::default(),
            mcp_intent: crate::mcp::McpIntent::default(),
            beads: None,
            project_beads: None,
            design: None,
            lsp: lsp::Registry::new(runtime.home, &session.root).unwrap(),
            commands: command_sessions::CommandSessions::default(),
            video_jobs: video::Jobs::default(),
            instructions: instructions::Resolver::new(&session.root).unwrap(),
            repeated: tool_loop::Guard::default(),
            mcp_name_recovery: None,
            mcp_name_reminded: false,
            diagnostics: vec![],
            direct_tasks,
            restricted: false,
            publication: false,
            prompt: String::new(),
            delivered_wire: 0,
            request_scope: "gateway-test".into(),
        }
    }

    #[tokio::test]
    async fn project_instruction_gateway_routes_native_review_only_in_build_mode() {
        let fixture = crate::agent::tests::Fixture::new();
        let session = crate::agent::tests::session(&fixture);
        let state = AppState::default();
        let oauth = OpenAiCodexState::default();
        let mcp = crate::mcp::McpState::default();
        let grants = execution_grants::GrantStore::default();
        let options = crate::agent::tests::options(ApprovalMode::Yolo);
        let signal = session
            .reserve("Preparar AGENTS.md".into(), options.clone())
            .unwrap();
        let mut bridge = fixture_bridge(
            &session,
            TurnRuntime {
                grants: &grants,
                state: &state,
                oauth: &oauth,
                mcp: &mcp,
                home: &fixture.root,
            },
            options,
            signal,
        );
        for mode in [Mode::Build, Mode::Plan] {
            bridge.options.mode = mode;
            bridge.restricted = mode == Mode::Plan;
            let definitions = bridge.definitions().await.unwrap();
            assert_eq!(
                definitions
                    .iter()
                    .any(|definition| definition["name"] == "jarvis_propose_project_instructions"),
                mode == Mode::Build
            );
            if mode == Mode::Build {
                let contract = Orchestrator::new(&definitions);
                let proposed = ToolCall {
                    id: "instructions".into(),
                    name: "jarvis_propose_project_instructions".into(),
                    args: json!({"revision":"missing","summary":"Convenções verificadas","content":"Use confirmed project checks."}),
                    status: "pending".into(),
                    output: String::new(),
                    duration_ms: 0,
                };
                assert_eq!(
                    contract.preflight(&proposed).unwrap().handler,
                    Handler::JarvisAuthoring
                );
            }
        }
    }

    #[tokio::test]
    async fn live_mcp_scope_change_reaches_the_stable_cli_gateway_without_replaying_tools() {
        for publication in [false, true] {
            let fixture = crate::agent::tests::Fixture::new();
            let session = crate::agent::tests::session(&fixture);
            let state = AppState::default();
            let oauth = OpenAiCodexState::default();
            let mcp = crate::mcp::McpState::default();
            let grants = execution_grants::GrantStore::default();
            let options = crate::agent::tests::options(ApprovalMode::Yolo);
            let signal = session
                .reserve("Continuar".into(), options.clone())
                .unwrap();
            session
                .update(true, |data| {
                    data.turns.last_mut().unwrap().mcp_intent = Some(crate::mcp::McpIntent {
                        mode: crate::mcp::McpIntentMode::Disabled,
                        ..crate::mcp::McpIntent::default()
                    });
                })
                .unwrap();
            let mut bridge = fixture_bridge(
                &session,
                TurnRuntime {
                    grants: &grants,
                    state: &state,
                    oauth: &oauth,
                    mcp: &mcp,
                    home: &fixture.root,
                },
                options,
                signal,
            );
            bridge.publication = publication;
            bridge.refresh_mcp_intent().await.unwrap();
            {
                let data = session.data.lock().unwrap();
                let wire = &data.turns.last().unwrap().wire;
                assert_eq!(wire.len(), 2, "publication={publication}");
                assert!(wire[1]["content"]
                    .as_str()
                    .unwrap()
                    .contains("\"availableTools\":[]"));
            }
            assert_eq!(bridge.delivered_wire, 0);
            bridge.refresh_mcp_intent().await.unwrap();
            assert_eq!(
                session
                    .data
                    .lock()
                    .unwrap()
                    .turns
                    .last()
                    .unwrap()
                    .wire
                    .len(),
                2
            );
        }
    }

    #[tokio::test]
    async fn publication_mcp_gateway_tracks_live_scope_without_connecting() {
        let fixture = crate::agent::tests::Fixture::new();
        let session = crate::agent::tests::session(&fixture);
        let state = AppState::default();
        let oauth = OpenAiCodexState::default();
        let mcp = crate::mcp::McpState::default();
        let grants = execution_grants::GrantStore::default();
        let options = crate::agent::tests::options(ApprovalMode::Yolo);
        let signal = session.reserve("Publicar".into(), options.clone()).unwrap();
        session
            .update(true, |data| {
                data.turns.last_mut().unwrap().mcp_intent = Some(crate::mcp::McpIntent::default());
            })
            .unwrap();
        // Public metadata exposes on-demand discovery without credentials or a transport.
        state.with_connection(&fixture.root, |db| {
            db.execute("INSERT INTO mcp_servers (id,name,kind,enabled,configured,revision) VALUES ('fake-github','Github','local',1,1,1)", [])?;
            Ok::<_, crate::mcp::McpError>(())
        }).unwrap();
        let mut bridge = fixture_bridge(
            &session,
            TurnRuntime {
                grants: &grants,
                state: &state,
                oauth: &oauth,
                mcp: &mcp,
                home: &fixture.root,
            },
            options,
            signal,
        );
        bridge.publication = true;
        let definitions = bridge.definitions().await.unwrap();
        let activate = definitions
            .iter()
            .find(|tool| tool["name"] == "mcp_activate")
            .expect("publication must expose enabled MCP discovery");
        assert!(activate["parameters"]["properties"]["server"]["enum"]
            .as_array()
            .unwrap()
            .contains(&json!("Github")));
        assert!(definitions
            .iter()
            .any(|tool| tool["name"] == "jarvis_propose_publication"));

        for (mode, enabled) in [
            (crate::mcp::McpIntentMode::Disabled, false),
            (crate::mcp::McpIntentMode::OnDemand, true),
        ] {
            session
                .update(true, |data| {
                    data.turns.last_mut().unwrap().mcp_intent = Some(crate::mcp::McpIntent {
                        mode,
                        ..crate::mcp::McpIntent::default()
                    });
                })
                .unwrap();
            let definitions = bridge.definitions().await.unwrap();
            assert_eq!(
                definitions
                    .iter()
                    .any(|tool| tool["name"] == "mcp_activate"),
                enabled
            );
        }
        assert!(bridge.clients.tool_metadata("mcp_Github_lookup").is_none());
        assert_eq!(bridge.delivered_wire, 0);
        let wire_count = session
            .data
            .lock()
            .unwrap()
            .turns
            .last()
            .unwrap()
            .wire
            .len();
        assert_eq!(wire_count, 3);
        bridge.definitions().await.unwrap();
        assert_eq!(
            session
                .data
                .lock()
                .unwrap()
                .turns
                .last()
                .unwrap()
                .wire
                .len(),
            wire_count
        );
    }

    #[tokio::test]
    async fn skill_gateway_admits_new_plugins_keeps_turn_versions_and_honors_revocation() {
        let fixture = crate::agent::tests::Fixture::new();
        let session = crate::agent::tests::session(&fixture);
        let home = fixture.root.as_path();
        let create = |name: &str, body: &str| {
            crate::plugins::Operation::Create {
            draft: serde_json::from_value(json!({"name":name,"description":"Gateway fixture","skills":[{"name":"guidance","content":format!("---\nname: guidance\ndescription: {body}\n---\n{body}")}]})).unwrap(),
        }
        };
        let prepared = crate::plugins::preview(home, 0, create("original", "Original guidance"))
            .await
            .unwrap();
        let catalog = crate::plugins::apply(home, &prepared).unwrap();
        let frozen = crate::skills::active(home, &session.root).await.unwrap();
        let original = frozen
            .iter()
            .find(|skill| skill.origin == "plugin")
            .unwrap()
            .id
            .clone();
        let plugin_id = catalog.installed[0].id.clone();
        let options = crate::agent::tests::options(ApprovalMode::Yolo);
        let signal = session
            .reserve("Use a orientação original".into(), options.clone())
            .unwrap();
        let state = AppState::default();
        let oauth = OpenAiCodexState::default();
        let mcp = crate::mcp::McpState::default();
        let grants = execution_grants::GrantStore::default();
        let mut bridge = fixture_bridge(
            &session,
            TurnRuntime {
                grants: &grants,
                state: &state,
                oauth: &oauth,
                mcp: &mcp,
                home,
            },
            options,
            signal,
        );
        bridge.frozen_skills = frozen;
        let prepared = crate::plugins::preview(
            home,
            catalog.revision,
            create("original", "Updated guidance"),
        )
        .await
        .unwrap();
        let catalog = crate::plugins::apply(home, &prepared).unwrap();
        let prepared =
            crate::plugins::preview(home, catalog.revision, create("late", "Late guidance"))
                .await
                .unwrap();
        let catalog = crate::plugins::apply(home, &prepared).unwrap();
        let late = crate::skills::active(home, &session.root)
            .await
            .unwrap()
            .iter()
            .find(|skill| skill.description == "Late guidance")
            .unwrap()
            .id
            .clone();
        let local = crate::data_dir::root(home).join("skills/late-local");
        std::fs::create_dir_all(&local).unwrap();
        std::fs::write(
            local.join("SKILL.md"),
            "---\nname: late-local\ndescription: Local guidance\n---\nLocal guidance",
        )
        .unwrap();
        bridge.definitions().await.unwrap();
        let tool = |name: &str, args| ToolCall {
            id: "skill-test".into(),
            name: name.into(),
            args,
            status: "pending".into(),
            output: String::new(),
            duration_ms: 0,
        };
        let found = bridge
            .dispatch(
                &tool("find_skills", json!({"query":"Late"})),
                Handler::SkillSearch,
                None,
                false,
            )
            .await
            .unwrap();
        assert_eq!(serde_json::from_str::<Value>(&found).unwrap()["total"], 1);
        assert!(session.snapshot().unwrap().turns[0]
            .steps
            .iter()
            .all(|step| step.core_activities.is_empty()));
        let original_text = bridge
            .dispatch(
                &tool("read_skill", json!({"id":original})),
                Handler::SkillRead,
                None,
                false,
            )
            .await
            .unwrap();
        assert!(original_text.contains("Original guidance"));
        assert!(!original_text.contains("Updated guidance"));
        let added = bridge
            .dispatch(
                &tool("read_skill", json!({"id":late})),
                Handler::SkillRead,
                None,
                false,
            )
            .await
            .unwrap();
        assert!(added.contains("Late guidance"));
        let activities: Vec<_> = session.snapshot().unwrap().turns[0]
            .steps
            .iter()
            .flat_map(|step| &step.core_activities)
            .cloned()
            .collect();
        assert_eq!(activities.len(), 2);
        assert_eq!(activities[0].plugin_id.as_deref(), Some("original@local"));
        assert_eq!(activities[1].plugin_id.as_deref(), Some("late@local"));
        assert!(activities
            .iter()
            .all(|activity| activity.action == "skill_loaded"));
        let local = bridge
            .dispatch(
                &tool("find_skills", json!({"query":"Local guidance"})),
                Handler::SkillSearch,
                None,
                false,
            )
            .await
            .unwrap();
        assert_eq!(serde_json::from_str::<Value>(&local).unwrap()["total"], 0);
        let content = bridge
            .discovery_content(
                &tool("jarvis_propose_plugin", json!({})),
                &json!({"approved":true,"status":"applied"}).to_string(),
                "completed",
            )
            .await
            .unwrap()
            .unwrap();
        let receipt: Value = serde_json::from_str(content["text"].as_str().unwrap()).unwrap();
        assert!(receipt["capabilityContext"]
            .as_str()
            .unwrap()
            .contains("late@local"));
        assert!(receipt["capabilityContext"]
            .as_str()
            .unwrap()
            .contains("Original guidance"));
        assert!(!receipt["capabilityContext"]
            .as_str()
            .unwrap()
            .contains("Updated guidance"));
        let prepared = crate::plugins::preview(
            home,
            catalog.revision,
            crate::plugins::Operation::SetEnabled {
                plugin_id,
                enabled: false,
                project_path: None,
            },
        )
        .await
        .unwrap();
        crate::plugins::apply(home, &prepared).unwrap();
        assert!(bridge
            .dispatch(
                &tool("read_skill", json!({"id":original})),
                Handler::SkillRead,
                None,
                false,
            )
            .await
            .is_err());
        let found = bridge
            .dispatch(
                &tool("find_skills", json!({"query":"Original"})),
                Handler::SkillSearch,
                None,
                false,
            )
            .await
            .unwrap();
        assert_eq!(serde_json::from_str::<Value>(&found).unwrap()["total"], 0);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn large_unread_event_denial_prevents_native_file_action() {
        let fixture = crate::agent::tests::Fixture::new();
        let session = crate::agent::tests::session(&fixture);
        let options = crate::agent::tests::options(ApprovalMode::Yolo);
        let signal = session
            .reserve("Salvar um arquivo".into(), options.clone())
            .unwrap();
        let hooks = crate::agent::tests::manual_hook_runtime(
            &fixture,
            &session,
            crate::hooks::Event::PreToolUse,
            "printf '%s' 'file action blocked' >&2; exit 2",
            "write",
        );
        let state = AppState::default();
        let oauth = OpenAiCodexState::default();
        let mcp = crate::mcp::McpState::default();
        let grants = execution_grants::GrantStore::default();
        let mut bridge = fixture_bridge(
            &session,
            TurnRuntime {
                grants: &grants,
                state: &state,
                oauth: &oauth,
                mcp: &mcp,
                home: &fixture.root,
            },
            options,
            signal,
        );
        bridge.manual_hooks = Arc::new(hooks);
        bridge.direct_tasks = false;
        let tool = ToolCall {
            id: "write-blocked".into(),
            name: "write".into(),
            args: json!({"path":"forbidden.txt", "content":"x".repeat(512 * 1024)}),
            status: "pending".into(),
            output: String::new(),
            duration_ms: 0,
        };
        let error = bridge.call(&tool).await.unwrap_err();
        assert_eq!(error.code, "hook_denied");
        assert_eq!(error.message, "file action blocked");
        assert!(!fixture.root.join("forbidden.txt").exists());
        assert!(session.snapshot().unwrap().pending_approval.is_none());
    }

    #[tokio::test]
    async fn native_mcp_gateway_recovers_unknown_names_in_the_same_run_without_replaying_effects() {
        for outcome in ["listed_only", "lookup", "alias", "invalid", "revoked"] {
            let fixture = crate::agent::tests::Fixture::new();
            let session = crate::agent::tests::session(&fixture);
            let options = crate::agent::tests::options(ApprovalMode::Yolo);
            let signal = session
                .reserve("Consulte a documentação.".into(), options.clone())
                .unwrap();
            let state = AppState::default();
            let oauth = OpenAiCodexState::default();
            let mcp = crate::mcp::McpState::default();
            let grants = execution_grants::GrantStore::default();
            let calls = fixture.root.join("mcp-calls");
            let prepared = crate::plugins::preview(&fixture.root, 0, crate::plugins::Operation::Create {
                draft: serde_json::from_value(json!({
                    "name":"same-turn", "description":"Offline native MCP fixture",
                    "mcpServers":{"documents":{
                        "command":"node",
                        "args":[PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/mcp/fixtures/server.mjs")],
                        "env":{"CALLS_FILE":calls}
                    },"other":{
                        "command":"node",
                        "args":[PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/mcp/fixtures/server.mjs")],
                        "env":{"CALLS_FILE":fixture.root.join("other-mcp-calls")}
                    }}
                })).unwrap(),
            }).await.unwrap();
            crate::plugins::apply(&fixture.root, &prepared).unwrap();
            let mut bridge = fixture_bridge(
                &session,
                TurnRuntime {
                    grants: &grants,
                    state: &state,
                    oauth: &oauth,
                    mcp: &mcp,
                    home: &fixture.root,
                },
                options,
                signal,
            );
            bridge.direct_tasks = false;
            bridge.clients = crate::mcp::runtime::TurnClients::discover_for_intent(
                &mcp,
                &state,
                &fixture.root,
                &session.root,
                &crate::mcp::McpIntent::default(),
                bridge.signal.clone(),
            )
            .await
            .unwrap();
            let tool = |id: &str, name: &str, args: Value| ToolCall {
                id: id.into(),
                name: name.into(),
                args,
                status: "pending".into(),
                output: String::new(),
                duration_ms: 0,
            };
            let activate = tool(
                "activate",
                "mcp_activate",
                json!({"server":"same-turn@local: documents"}),
            );
            let activated = super::super::execute_tool(
                &mut bridge,
                "activate-request",
                &json!({"name":activate.name,"arguments":activate.args}),
                Some(activate),
            )
            .await
            .unwrap();
            assert_eq!(activated["isError"], false);
            let receipt = activated["content"]
                .as_array()
                .unwrap()
                .iter()
                .find_map(|item| {
                    serde_json::from_str::<Value>(item["text"].as_str()?)
                        .ok()
                        .filter(|receipt| receipt["availableTools"].is_array())
                })
                .unwrap();
            assert!(receipt["next"]
                .as_str()
                .unwrap()
                .contains("No new user message"));
            let lookup = receipt["availableTools"]
                .as_array()
                .unwrap()
                .iter()
                .find(|tool| {
                    tool["description"]
                        .as_str()
                        .is_some_and(|description| description.contains("Read documentation"))
                })
                .unwrap();
            let canonical = lookup["name"].as_str().unwrap().to_owned();
            assert_eq!(lookup["inputSchema"]["required"], json!(["query"]));
            assert!(!calls.exists());
            if outcome == "listed_only" {
                assert!(bridge.completion_feedback().await.unwrap().is_none());
                continue;
            }
            let unknown_name = if outcome == "alias" {
                "lookup"
            } else {
                "mcp_documentation_lookup"
            };
            let unknown = tool("unknown", unknown_name, json!({"query":"archive evidence"}));
            let rejected = super::super::execute_tool(
                &mut bridge,
                "unknown-request",
                &json!({"name":unknown.name,"arguments":unknown.args}),
                Some(unknown),
            )
            .await
            .unwrap();
            assert_eq!(rejected["isError"], true);
            let error: Value =
                serde_json::from_str(rejected["content"][0]["text"].as_str().unwrap()).unwrap();
            assert_eq!(error["error"]["code"], "tool_unavailable");
            assert_eq!(error["executed"], false);
            assert!(error["schemas"]
                .as_array()
                .unwrap()
                .iter()
                .any(|tool| tool["name"] == canonical));
            assert_eq!(
                super::super::replay_tool(&session, "unknown")
                    .unwrap()
                    .unwrap(),
                rejected
            );
            assert!(!calls.exists());
            if outcome == "revoked" {
                let other = tool(
                    "activate-other",
                    "mcp_activate",
                    json!({"server":"same-turn@local: other"}),
                );
                bridge.call(&other).await.unwrap();
                let selected = mcp
                    .list_for_project(&state, &fixture.root, Some(&session.root))
                    .unwrap()
                    .into_iter()
                    .find(|server| server.name == "same-turn@local: documents")
                    .unwrap();
                session
                    .update(true, |data| {
                        data.turns.last_mut().unwrap().mcp_intent = Some(crate::mcp::McpIntent {
                            excluded_servers: vec![crate::mcp::McpIntentServer {
                                id: selected.id.clone(),
                                name: selected.name.clone(),
                            }],
                            ..crate::mcp::McpIntent::default()
                        });
                    })
                    .unwrap();
                assert!(bridge.completion_feedback().await.unwrap().is_none());
                let definitions = bridge.definitions().await.unwrap();
                assert!(!definitions.iter().any(|tool| tool["name"] == canonical));
                assert!(definitions
                    .iter()
                    .any(|tool| tool["name"].as_str().is_some_and(|name| bridge
                        .clients
                        .tool_metadata(name)
                        .is_some_and(|(server, _, _)| server == "same-turn@local: other"))));
                assert!(!calls.exists());
                assert!(!fixture.root.join("other-mcp-calls").exists());
                continue;
            }
            if outcome == "lookup" {
                let feedback = bridge.completion_feedback().await.unwrap().unwrap();
                assert!(feedback.contains(&canonical));
                assert_eq!(
                    bridge.completion_feedback().await.unwrap_err().code,
                    "mcp_tool_unavailable"
                );
                assert!(!calls.exists(), "completion recovery never executes a tool");
            }
            let args = if outcome == "invalid" {
                json!({"notQuery":true})
            } else {
                json!({"query":"archive evidence"})
            };
            let selected = tool("canonical", &canonical, args);
            let params = json!({"name":selected.name,"arguments":selected.args});
            let result = super::super::execute_tool(
                &mut bridge,
                "canonical-request",
                &params,
                Some(selected.clone()),
            )
            .await
            .unwrap();
            assert_eq!(result["isError"], outcome == "invalid");
            assert!(
                bridge.completion_feedback().await.unwrap().is_none(),
                "the canonical outcome owns further recovery"
            );
            if outcome == "invalid" {
                assert!(!calls.exists());
            } else {
                assert!(result
                    .to_string()
                    .contains("Documentation: archive evidence"));
                let replayed = super::super::execute_tool(
                    &mut bridge,
                    "canonical-request",
                    &params,
                    Some(selected),
                )
                .await
                .unwrap();
                assert_eq!(replayed, result);
                assert_eq!(std::fs::read_to_string(&calls).unwrap(), "lookup\n");
            }
        }
    }

    #[tokio::test]
    async fn approved_mcp_registration_refreshes_the_stable_gateway_without_connecting() {
        let fixture = crate::agent::tests::Fixture::new();
        let session = crate::agent::tests::session(&fixture);
        let options = crate::agent::tests::options(ApprovalMode::Yolo);
        let signal = session
            .reserve("Adicionar um MCP".into(), options.clone())
            .unwrap();
        let state = AppState::default();
        let oauth = OpenAiCodexState::default();
        let mcp = crate::mcp::McpState::default();
        let grants = execution_grants::GrantStore::default();
        let mut bridge = fixture_bridge(
            &session,
            TurnRuntime {
                grants: &grants,
                state: &state,
                oauth: &oauth,
                mcp: &mcp,
                home: &fixture.root,
            },
            options,
            signal,
        );
        bridge.definitions().await.unwrap();
        // Only public metadata is needed to refresh discovery; credentials and
        // transport startup must remain untouched until mcp_activate.
        state.with_connection(&fixture.root, |db| {
            db.execute("INSERT INTO mcp_servers (id,name,kind,enabled,configured,revision) VALUES ('new-server','added-server','local',1,1,1)", [])?;
            Ok::<_, crate::mcp::McpError>(())
        }).unwrap();
        let tool = ToolCall {
            id: "proposal".into(),
            name: "jarvis_propose_mcp".into(),
            args: json!({}),
            status: "pending".into(),
            output: String::new(),
            duration_ms: 0,
        };
        for (output, status) in [
            (json!({"approved":false,"status":"rejected"}), "completed"),
            (json!({"approved":true,"status":"failed"}), "completed"),
            (json!({"approved":false,"status":"applied"}), "completed"),
            (json!({"approved":true,"status":"applied"}), "error"),
        ] {
            assert!(bridge
                .discovery_content(&tool, &output.to_string(), status)
                .await
                .unwrap()
                .is_none());
        }
        let output = json!({"approved":true,"status":"applied"}).to_string();
        let content = bridge
            .discovery_content(&tool, &output, "completed")
            .await
            .unwrap()
            .unwrap();
        let receipt: Value = serde_json::from_str(content["text"].as_str().unwrap()).unwrap();
        assert_eq!(receipt["callWith"], "mcp__jarvis__call_mcp_tool");
        let activate = receipt["availableTools"]
            .as_array()
            .unwrap()
            .iter()
            .find(|tool| tool["name"] == "mcp_activate")
            .unwrap();
        assert!(activate["inputSchema"]["properties"]["server"]["enum"]
            .as_array()
            .unwrap()
            .contains(&json!("added-server")));
        assert!(bridge
            .clients
            .tool_metadata("mcp_added-server_lookup")
            .is_none());
    }
}
