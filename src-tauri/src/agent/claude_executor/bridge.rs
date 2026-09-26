//! Claude owns inference; every effect still crosses the existing Jarvis tool contracts.
use super::super::*;
use tool_contract::{Handler, Orchestrator};

pub(super) struct Bridge<'a> {
    pub session: &'a Arc<Session>,
    pub runtime: TurnRuntime<'a>,
    pub execution: Option<workflow::Execution>,
    pub options: TurnOptions,
    pub signal: watch::Receiver<bool>,
    pub context: crate::core::context::ContextMode,
    pub clients: crate::mcp::runtime::TurnClients,
    beads: Option<crate::core::beads::Beads>,
    project_beads: Option<crate::core::beads::ProjectBeads>,
    design: Option<crate::core::design::Pack>,
    lsp: lsp::Registry,
    commands: command_sessions::CommandSessions,
    instructions: instructions::Resolver,
    repeated: tool_loop::Guard,
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
    ) -> Result<Self, AgentError> {
        let owner = execution.as_ref().map_or(session, |exec| exec.root());
        let home = runtime.home;
        let publication = execution
            .as_ref()
            .is_some_and(workflow::Execution::publication);
        let restricted = execution
            .as_ref()
            .map_or(options.mode == Mode::Plan, |exec| {
                exec.role_mode() == Mode::Plan
            });
        let direct_tasks = options.direct() && owner.id == session.id;
        let intent = {
            let data = session.data.lock().map_err(|_| AgentError::internal())?;
            data.turns
                .last()
                .and_then(|turn| turn.mcp_intent.clone())
                .unwrap_or_else(|| data.inherited_mcp_intent.clone())
        };
        let clients = if publication {
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
        let context = crate::core::context::ContextMode::open(
            home,
            &session.root,
            &session.id,
            signal.clone(),
        )
        .await?;
        let beads = if direct_tasks || publication {
            None
        } else {
            Some(crate::core::beads::Beads::new(
                home,
                owner.project_id()?,
                &owner.id,
                options.mode == Mode::Plan,
            )?)
        };
        let project_beads = if direct_tasks {
            crate::core::beads::ProjectBeads::open(home, &session.root)?
        } else {
            None
        };
        let mut activities = Vec::new();
        let mut prompt = tools::instructions(&session.root, options.mode);
        prompt.push_str("\nExecution backend: Claude Code. Keep your native reasoning and conversation management. All project operations, commands, tasks, questions, workflow coordination, approvals and external integrations are exposed by the Jarvis MCP server. Use these tools rather than describing actions for the user to execute. Jarvis owns their permissions and durable results. Do not create a second task/agent system. For dynamically discovered MCP tools, load their schema then call call_mcp_tool with the exact name and arguments. The native Claude built-in tools are intentionally disabled to preserve the selected Jarvis role, project scope and approval contract.\n");
        prompt.push_str(&crate::library::repositories::prompt(
            runtime.state,
            home,
            owner.project_id()?,
        )?);
        if let Some(exec) = &execution {
            prompt.push_str(&exec.instructions()?);
        }
        prompt.push_str(context.instructions());
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
            prompt.push_str(&format!("\nBeads state (reference data):\n{snapshot}"));
        }
        if options.mode == Mode::Build {
            prompt.push_str(&publication::instructions(&publication::load(
                runtime.state,
                home,
                owner.project_id()?,
            )?));
        }
        if !publication {
            prompt.push_str(authoring::INSTRUCTIONS);
            prompt.push_str(web_search::instructions(web_search::enabled(
                runtime.state,
                home,
                &options,
            )));
            let skills = crate::skills::active(home, &session.root)
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
        context.hooks.before_agent(&mut prompt);
        tools::append_response_language(&mut prompt, crate::system::response_language(home));
        Ok(Self {
            session,
            runtime,
            execution,
            options,
            signal,
            context,
            clients,
            beads,
            project_beads,
            design,
            lsp: lsp::Registry::new(&session.root, home)?,
            commands: command_sessions::CommandSessions::default(),
            instructions: instructions::Resolver::new(&session.root)?,
            repeated: tool_loop::Guard::default(),
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
        let mut definitions = tools::definitions(self.options.mode);
        definitions.push(publication::inspection::definition());
        definitions.push(attachments::definition());
        if self.options.mode == Mode::Build {
            definitions.push(publication::definition());
        }
        if self.direct_tasks {
            definitions.push(tasks::definition());
        }
        if self.beads.is_some() {
            definitions.extend(crate::core::beads::definitions(
                self.options.mode == Mode::Plan,
            ));
        }
        if self.project_beads.is_some() {
            definitions.extend(crate::core::beads::project_definitions());
        }
        if self.design.is_some() {
            definitions.extend(crate::core::design::definitions());
        }
        definitions.extend(self.context.definitions(self.restricted));
        if !self.publication {
            definitions.extend(authoring::definitions());
            definitions.extend([
                crate::skills::definition(),
                crate::skills::search_definition(),
            ]);
            if crate::core::context7::configured(self.runtime.home) {
                definitions.extend(crate::core::context7::definitions());
            }
            if image_generation::enabled(self.runtime.state, self.runtime.home) {
                definitions.push(image_generation::definition());
            }
            definitions.push(super::native_vision::definition());
            if web_search::enabled(self.runtime.state, self.runtime.home, &self.options) {
                definitions.push(web_search::definition());
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
        }
        if let Some(exec) = &self.execution {
            exec.filter(&mut definitions);
        }
        self.clients.ensure_scope_visible(&definitions)?;
        Ok(definitions)
    }

    pub async fn call(&mut self, tool: &ToolCall) -> Result<String, AgentError> {
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
        let policy =
            execution_policy::inspect_tool(&self.session.root, tool, prepared.capabilities)?;
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
            .dispatch(tool, prepared.handler, sandbox.as_ref())
            .await;
        let _ = self.repeated.observe(
            tool,
            result.is_err(),
            result
                .as_ref()
                .map_or_else(|error| error.message.as_str(), String::as_str),
        );
        if let Some(exec) = &self.execution {
            exec.observe_recovery_inspection(
                tool,
                tool.name.starts_with("mcp_") && requires_task,
                result.is_ok(),
            )?;
        }
        result
    }

    async fn dispatch(
        &mut self,
        tool: &ToolCall,
        handler: Handler,
        sandbox: Option<&execution_sandbox::SandboxPlan>,
    ) -> Result<String, AgentError> {
        let home = self.runtime.home;
        let state = self.runtime.state;
        let signal = self.signal.clone();
        match handler {
            Handler::PublicationInspection => {
                publication::inspection::inspect(&self.session.root, &tool.args, signal).await
            }
            Handler::Workflow => {
                if tool.name == "hub_complete" {
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
                            "Revise os diagnósticos LSP antes do handoff.",
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
            Handler::Context7 => crate::core::context7::execute(
                home,
                &self.session.root,
                &tool.name,
                &tool.args,
                signal,
            )
            .await
            .map_err(AgentError::from),
            Handler::Mcp => self
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
                .await
                .map_err(AgentError::from),
            Handler::Lsp => self.lsp.execute(tool, signal).await,
            Handler::Attachment => attachments::read_tool(home, &self.owner().id, &tool.args),
            Handler::ImageGeneration => {
                image_generation::execute(
                    state,
                    self.runtime.oauth,
                    home,
                    &self.owner().id,
                    &tool.args,
                    signal,
                )
                .await
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
            Handler::SkillRead => crate::skills::read(home, &self.session.root, &tool.args)
                .await
                .map_err(|error| AgentError::new("skill_error", &error.message)),
            Handler::SkillSearch => {
                let skills = crate::skills::active(home, &self.session.root)
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

    pub async fn completion_feedback(&mut self) -> Result<Option<String>, AgentError> {
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
            return Ok(Some("Review the new LSP diagnostics before finishing. Fix errors introduced by this work or report pre-existing limitations accurately.".into()));
        }
        if self.clients.requires_explicit_attempt() {
            return Ok(Some(self.clients.explicit_reminder()));
        }
        if self.direct_tasks && self.session.has_unfinished_tasks()? {
            return Ok(Some("Update the Jarvis task list before finishing. Mark verified outcomes completed and real unresolved dependencies blocked.".into()));
        }
        if let Some(exec) = &self.execution {
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
