use super::*;
use std::fs;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModelChoice {
    #[serde(default, skip_serializing_if = "crate::claude::Executor::is_jarvis")]
    pub executor: crate::claude::Executor,
    pub account: String,
    pub model: String,
    pub reasoning: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fallback: Option<Box<ModelChoice>>,
}
pub type ModelSettings = BTreeMap<String, ModelChoice>;

impl ModelChoice {
    pub(crate) fn validate_shape(&self) -> Result<(), AgentError> {
        let valid = |value: &str, max: usize| {
            !value.trim().is_empty() && value.len() <= max && !value.chars().any(char::is_control)
        };
        if !valid(&self.model, 200)
            || self
                .reasoning
                .as_ref()
                .is_some_and(|value| !valid(value, 40))
            || match self.executor {
                crate::claude::Executor::Jarvis => !valid(&self.account, 200),
                crate::claude::Executor::Claude => !self.account.is_empty(),
            }
        {
            return Err(invalid("Escolha um executor e modelo válidos. Claude usa sua própria autenticação, sem provedor Jarvis."));
        }
        if self.executor == crate::claude::Executor::Claude {
            crate::claude::validate_selection(&self.model, self.reasoning.as_deref())
                .map_err(|message| invalid(&message))?;
        }
        if let Some(fallback) = &self.fallback {
            if fallback.fallback.is_some()
                || (self.executor == fallback.executor
                    && self.account == fallback.account
                    && self.model == fallback.model)
            {
                return Err(invalid(
                    "Escolha um único modelo secundário diferente do principal.",
                ));
            }
            fallback.validate_shape()?;
        }
        Ok(())
    }

    pub(crate) fn apply(&self, options: &mut TurnOptions) {
        options.executor = self.executor;
        options.account.clone_from(&self.account);
        options.model.clone_from(&self.model);
        options.reasoning.clone_from(&self.reasoning);
    }
}

pub(crate) fn validate_choice(
    state: &AppState,
    oauth: &OpenAiCodexState,
    home: &Path,
    choice: &ModelChoice,
) -> Result<(), AgentError> {
    choice.validate_shape()?;
    for selection in std::iter::once(choice).chain(choice.fallback.as_deref()) {
        if selection.executor == crate::claude::Executor::Claude {
            crate::claude::validate_available_model(home, &selection.model)
                .map_err(|message| AgentError::new("claude_provider", &message))?;
        } else {
            oauth.inference_model(
                state,
                home,
                &selection.account,
                &selection.model,
                selection.reasoning.as_deref(),
            )?;
        }
    }
    Ok(())
}

pub(in crate::agent) fn key(flow: Flow, role: Role) -> String {
    format!(
        "{}/{}",
        serde_json::to_value(flow).unwrap().as_str().unwrap(),
        serde_json::to_value(role).unwrap().as_str().unwrap()
    )
}
pub(super) fn roster(flow: Flow) -> &'static [Role] {
    flow.roster()
}
pub(crate) fn read(home: &Path) -> Result<ModelSettings, AgentError> {
    let path = crate::data_dir::root(home).join("agents.json");
    let meta = match fs::symlink_metadata(&path) {
        Ok(meta) => meta,
        Err(cause) if cause.kind() == std::io::ErrorKind::NotFound => return Ok(BTreeMap::new()),
        Err(_) => return Err(AgentError::storage()),
    };
    if !meta.is_file() || meta.is_symlink() || meta.len() > 64 * 1024 {
        return Err(AgentError::storage());
    }
    let settings: ModelSettings =
        serde_json::from_slice(&fs::read(path).map_err(|_| AgentError::storage())?)
            .map_err(|_| AgentError::storage())?;
    if settings.len() > 13
        || settings
            .values()
            .any(|choice| choice.validate_shape().is_err())
        || settings.keys().any(|name| {
            ![
                Flow::Standard,
                Flow::Designer,
                Flow::Planned,
                Flow::Complete,
                Flow::Publication,
            ]
            .iter()
            .any(|flow| roster(*flow).iter().any(|role| key(*flow, *role) == *name))
        })
    {
        return Err(invalid("Configuração de agentes inválida."));
    }
    Ok(settings)
}
pub(in crate::agent) fn load(state: &AppState, home: &Path) -> Result<ModelSettings, AgentError> {
    state.with_connection(home, |db| configured(db, home))
}
fn configured(db: &rusqlite::Connection, home: &Path) -> Result<ModelSettings, AgentError> {
    read(home)?
        .into_iter()
        .map(|(key, choice)| {
            let choice = crate::model_bindings::resolve(db, &format!("builtin:{key}"), &choice)?;
            Ok((key, choice))
        })
        .collect()
}
pub(in crate::agent) fn validate(flow: Flow, profiles: &ModelSettings) -> Result<(), AgentError> {
    if flow.direct() || flow == Flow::Publication {
        return Ok(());
    }
    let missing: Vec<_> = roster(flow)
        .iter()
        .filter(|role| !profiles.contains_key(&key(flow, **role)))
        .map(|role| role.label())
        .collect();
    if missing.is_empty() {
        Ok(())
    } else {
        Err(invalid(&format!(
            "Escolha os modelos em Configurações > Workflow: {}.",
            missing.join(", ")
        )))
    }
}
pub(super) fn apply(options: &mut TurnOptions, profiles: &ModelSettings, flow: Flow, role: Role) {
    if let Some(choice) = profiles.get(&key(flow, role)) {
        choice.apply(options);
    }
}
#[tauri::command]
pub fn get_agent_instructions(
    flow: Flow,
    role: Role,
) -> Result<Vec<contracts::InstructionSection>, AgentError> {
    if !roster(flow).contains(&role) {
        return Err(invalid("Este agente não pertence ao fluxo."));
    }
    Ok(contracts::instruction_sections(flow, role))
}

#[tauri::command]
pub async fn get_agent_models(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<ModelSettings, AgentError> {
    let state = state.inner().clone();
    let home = app.path().home_dir().map_err(|_| AgentError::storage())?;
    tauri::async_runtime::spawn_blocking(move || load(&state, &home))
        .await
        .map_err(|_| AgentError::internal())?
}
#[tauri::command]
pub async fn set_agent_model(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    oauth: tauri::State<'_, OpenAiCodexState>,
    flow: Flow,
    role: Role,
    choice: ModelChoice,
) -> Result<ModelSettings, AgentError> {
    if !roster(flow).contains(&role) {
        return Err(invalid("Este agente não pertence ao fluxo."));
    }
    let state = state.inner().clone();
    let oauth = oauth.inner().clone();
    let home = app.path().home_dir().map_err(|_| AgentError::storage())?;
    let result = tauri::async_runtime::spawn_blocking(move || {
        validate_choice(&state, &oauth, &home, &choice)?;
        state.with_connection(&home, |db| {
            let accounts = crate::persistence::list_provider_accounts(db)?;
            for selection in std::iter::once(&choice).chain(choice.fallback.as_deref()) {
                if selection.executor == crate::claude::Executor::Jarvis
                    && !accounts
                        .iter()
                        .any(|account| account.alias == selection.account && account.enabled)
                {
                    return Err(invalid(
                        "O provedor foi removido ou desativado. Escolha outro modelo.",
                    ));
                }
            }
            let mut config = configured(db, &home)?;
            config.insert(key(flow, role), choice);
            let directory = crate::data_dir::root(&home);
            let mut file =
                tempfile::NamedTempFile::new_in(&directory).map_err(|_| AgentError::storage())?;
            serde_json::to_writer(file.as_file_mut(), &config)
                .map_err(|_| AgentError::storage())?;
            file.as_file_mut()
                .sync_all()
                .map_err(|_| AgentError::storage())?;
            file.persist(directory.join("agents.json"))
                .map_err(|_| AgentError::storage())?;
            crate::model_bindings::forget_item(db, &format!("builtin:{}", key(flow, role)))?;
            #[cfg(unix)]
            fs::File::open(directory)
                .and_then(|file| file.sync_all())
                .map_err(|_| AgentError::storage())?;
            Ok::<_, AgentError>(config)
        })
    })
    .await
    .map_err(|_| AgentError::internal())??;
    let _ = app.emit("agent-models:changed", ());
    Ok(result)
}

#[cfg(test)]
mod instruction_tests {
    use super::*;

    #[test]
    fn executor_choices_default_legacy_records_and_apply_without_a_fake_provider() {
        let legacy = json!({"account":"existing","model":"existing-model","reasoning":null});
        let native: ModelChoice = serde_json::from_value(legacy.clone()).unwrap();
        assert_eq!(native.executor, crate::claude::Executor::Jarvis);
        assert!(native.fallback.is_none());
        assert_eq!(serde_json::to_value(&native).unwrap(), legacy);
        let choice = ModelChoice {
            executor: crate::claude::Executor::Claude,
            account: String::new(),
            model: "sonnet".into(),
            reasoning: Some("high".into()),
            fallback: None,
        };
        let home = tempfile::tempdir().unwrap();
        validate_choice(
            &AppState::default(),
            &OpenAiCodexState::default(),
            home.path(),
            &choice,
        )
        .unwrap();
        let mut options = crate::agent::tests::options(ApprovalMode::Manual);
        apply(
            &mut options,
            &BTreeMap::from([(key(Flow::Designer, Role::Designer), choice.clone())]),
            Flow::Designer,
            Role::Designer,
        );
        assert_eq!(options.executor, crate::claude::Executor::Claude);
        assert!(options.account.is_empty());
        assert_eq!(options.model, "sonnet");
        assert_eq!(options.reasoning.as_deref(), Some("high"));
        assert_eq!(
            serde_json::from_value::<ModelChoice>(json!(choice)).unwrap(),
            choice
        );
        let mut invalid_choice = choice;
        invalid_choice.account = "fabricated-provider".into();
        assert!(invalid_choice.validate_shape().is_err());
        invalid_choice.account.clear();
        invalid_choice.reasoning = Some("invalid-effort".into());
        assert!(invalid_choice.validate_shape().is_err());
    }

    #[test]
    fn secondary_model_round_trips_and_rejects_duplicate_or_nested_targets() {
        let mut choice: ModelChoice = serde_json::from_value(json!({
            "account":"primary","model":"model-a","reasoning":null,
            "fallback":{"account":"secondary","model":"model-b","reasoning":"high"}
        }))
        .unwrap();
        choice.validate_shape().unwrap();
        let home = tempfile::tempdir().unwrap();
        fs::create_dir(crate::data_dir::root(home.path())).unwrap();
        fs::write(
            crate::data_dir::root(home.path()).join("agents.json"),
            json!({"standard/builder":choice}).to_string(),
        )
        .unwrap();
        assert_eq!(
            read(home.path()).unwrap().get("standard/builder"),
            Some(&choice)
        );

        let mut duplicate = choice.clone();
        duplicate.fallback = None;
        duplicate.reasoning = Some("low".into());
        choice.fallback = Some(Box::new(duplicate));
        assert!(choice.validate_shape().is_err());
        choice.fallback.as_mut().unwrap().account = "secondary".into();
        choice.validate_shape().unwrap();
        choice.fallback.as_mut().unwrap().fallback = Some(Box::new(
            serde_json::from_value(json!({
                "account":"third","model":"model-c","reasoning":null
            }))
            .unwrap(),
        ));
        assert!(choice.validate_shape().is_err());
    }

    #[test]
    fn secondary_model_requires_an_enabled_account() {
        let home = tempfile::tempdir().unwrap();
        let state = AppState::default();
        state
            .with_connection(home.path(), |db| {
                crate::persistence::insert_provider_account(
                    db,
                    "openai-codex-disabled",
                    "disabled",
                )?;
                db.execute("UPDATE provider_accounts SET enabled=0", [])?;
                Ok::<_, crate::persistence::PersistenceError>(())
            })
            .unwrap();
        let choice: ModelChoice = serde_json::from_value(json!({
            "executor":"claude","account":"","model":"sonnet","reasoning":null,
            "fallback":{"account":"openai-codex-disabled","model":"model-b","reasoning":null}
        }))
        .unwrap();
        let error = validate_choice(&state, &OpenAiCodexState::default(), home.path(), &choice)
            .unwrap_err();
        assert!(error.message.contains("Ative a conta"));
    }

    #[test]
    fn all_visible_instructions_are_the_actual_fixed_runtime_contracts() {
        for flow in [
            Flow::Standard,
            Flow::Designer,
            Flow::Planned,
            Flow::Complete,
            Flow::Publication,
        ] {
            for role in roster(flow) {
                let sections = get_agent_instructions(flow, *role).unwrap();
                let runtime = contracts::prompt(flow, *role, "runtime-agent-id");
                assert_eq!(sections[0].content, role.contract());
                assert!(sections
                    .iter()
                    .any(|section| section.content == include_str!("common.md")));
                for section in &sections {
                    assert!(!section.title.is_empty());
                    assert!(!section.content.is_empty());
                    assert!(runtime.contains(section.content));
                    assert!(!section.content.contains("runtime-agent-id"));
                }
                assert_eq!(
                    sections
                        .iter()
                        .any(|section| section.title == "Open Design"),
                    *role == Role::Designer
                );
                assert_eq!(
                    sections
                        .iter()
                        .any(|section| section.title == "Designer direto"),
                    flow == Flow::Designer
                );
                assert_eq!(
                    sections
                        .iter()
                        .any(|section| section.title == "Coordenação de design"),
                    role.coordinator()
                );
            }
        }
    }

    #[test]
    fn instructions_reject_agents_outside_the_selected_flow() {
        assert!(get_agent_instructions(Flow::Standard, Role::Reviewer).is_err());
        assert!(get_agent_instructions(Flow::Planned, Role::Orchestrator).is_err());
        assert!(get_agent_instructions(Flow::Designer, Role::Builder).is_err());
        assert!(get_agent_instructions(Flow::Publication, Role::Github).is_ok());
    }

    #[test]
    fn built_in_roles_preserve_objectives_and_proportionate_follow_ups() {
        for (flow, role) in [
            (Flow::Standard, Role::Builder),
            (Flow::Planned, Role::Planner),
            (Flow::Planned, Role::Builder),
            (Flow::Complete, Role::Planner),
            (Flow::Complete, Role::Builder),
        ] {
            let prompt = contracts::prompt(flow, role, "agent");
            assert!(prompt.contains("Preserve the unresolved user objective across turns"));
            assert!(prompt.contains("Corrections, clarifications and status questions steer"));
            assert!(prompt.contains("proving or rejecting one hypothesis is progress"));
            assert!(prompt.contains("read-only backend, HTTP and database diagnostics"));
            if !flow.direct() {
                assert!(prompt.contains("Workers do not inherit the full conversation"));
            }
            if role != Role::Planner && !flow.direct() {
                assert!(prompt.contains("request guidance from its coordinator"));
            }
            assert!(!prompt.contains("latest user request as the active objective"));
        }

        let planner = contracts::prompt(Flow::Planned, Role::Planner, "planner");
        assert!(planner.contains("narrow operational follow-up"));
        assert!(planner.contains("dispatch exactly one appropriate worker"));
        assert!(planner.contains("Do not duplicate that worker's source/runbook investigation"));
        assert!(planner.contains("partial diagnosis or unsearched configuration"));

        let builder = contracts::prompt(Flow::Planned, Role::Builder, "builder");
        assert!(builder.contains("actual failing request through active configuration"));
        assert!(
            builder.contains("Locate existing authorized credentials/integration configuration")
        );
        assert!(builder.contains("failed probe or plausible diagnosis does not finish"));
        assert!(builder.contains("bounded preflight/action/postcondition sequence"));
        assert!(builder.contains("skip code-quality gates"));

        let designer = contracts::prompt(Flow::Planned, Role::Designer, "designer");
        assert!(designer.contains("Reuse valid evidence"));
        assert!(designer.contains("Run checks proportional to the changed surface"));

        assert!(!planner.contains("maximum number of steps"));
        assert!(!builder.contains("maximum number of steps"));
    }

    #[test]
    fn specialist_contracts_define_proportionate_work_and_evidence_based_outcomes() {
        let investigator = contracts::prompt(Flow::Complete, Role::Investigator, "research");
        assert!(investigator.contains("specific unresolved questions"));
        assert!(investigator.contains("accessible evidence is exhausted"));
        let writer = contracts::prompt(Flow::Complete, Role::Writer, "plan");
        assert!(writer.contains("smallest executable specification"));
        assert!(writer.contains("dependency edges only when a task needs another's result"));
        assert!(writer.contains("A saved plan is not implementation or approval"));
        let reviewer = contracts::prompt(Flow::Complete, Role::Reviewer, "review");
        assert!(
            reviewer.contains("Speculative risks or stylistic preferences do not justify rework")
        );
        assert!(reviewer.contains("reuse recorded results only while their inputs are unchanged"));
        assert!(reviewer.contains("rework for concrete repairable failures"));
        assert!(reviewer.contains(
            "blocked only when missing evidence or a prerequisite actually prevents assessment"
        ));
        let orchestrator = contracts::prompt(Flow::Complete, Role::Orchestrator, "coordination");
        assert!(orchestrator.contains("Read only the epic and dependency-ready tasks"));
        assert!(orchestrator.contains("independent technical assessment"));
        assert!(orchestrator.contains("do not repeat workers' discovery or verification"));
        let designer = contracts::prompt(Flow::Designer, Role::Designer, "design");
        assert!(designer.contains("When a task is assigned"));
        assert!(designer.contains("without restarting discovery or redesigning unrelated areas"));
        assert!(designer.contains("only when those decisions materially change"));
        assert!(!designer.contains("native beads_"));
        let github = contracts::prompt(Flow::Publication, Role::Github, "publish");
        assert!(github.contains("Group compatible authorized operations"));
        assert!(github.contains("required preparation separately"));
        assert!(github.contains("without repeating confirmed results"));
        assert!(github.contains("hub_spawn"));
        assert!(github.contains("hub_respond_guidance"));
        assert!(!github.contains("through hub_request_guidance"));
    }

    #[test]
    fn direct_contracts_omit_native_coordination_but_keep_scope_and_acceptance() {
        for flow in [Flow::Standard, Flow::Designer] {
            let sections = get_agent_instructions(flow, flow.root()).unwrap();
            let prompt = contracts::prompt(flow, flow.root(), "main");
            assert!(!sections
                .iter()
                .any(|section| section.title == "Execução coordenada"));
            for unavailable in [
                "hub_spawn",
                "hub_retry",
                "hub_complete",
                "validation_publish",
            ] {
                assert!(!prompt.contains(unavailable), "{flow:?}: {unavailable}");
            }
            assert!(prompt.contains("Preserve the unresolved user objective"));
            assert!(prompt.contains("fixed role capabilities"));
            assert!(prompt.contains("USER performs final functional and visual acceptance"));
        }
    }

    #[test]
    fn native_tool_guidance_reaches_all_runtime_agents_without_rewriting_custom_prompts() {
        let mut prompts = Vec::new();
        for flow in [
            Flow::Standard,
            Flow::Designer,
            Flow::Planned,
            Flow::Complete,
            Flow::Publication,
        ] {
            for role in roster(flow) {
                let sections = get_agent_instructions(flow, *role).unwrap();
                let runtime = contracts::prompt(flow, *role, "runtime");
                assert!(sections.iter().any(|section| {
                    section.title == "Diretrizes comuns" && runtime.contains(section.content)
                }));
                prompts.push(runtime);
            }
        }
        let mut agent = catalog::AgentDefinition {
            id: "custom-agent".into(),
            name: "Custom agent".into(),
            description: String::new(),
            instructions: "Keep my project's API conventions.".into(),
            native_role: None,
            usage: catalog::AgentUsage::Mixed,
            capability: catalog::Capability::Commands,
            denied_tools: vec![],
            model: None,
            appearance: None,
        };
        for role in [None, Some(Role::Builder), Some(Role::Designer)] {
            agent.native_role = role;
            for prompt in [
                custom::instructions(&agent),
                custom::direct_instructions(&agent),
            ] {
                if role.is_none() {
                    assert!(prompt.contains(&agent.instructions));
                }
                prompts.push(prompt);
            }
        }
        for prompt in prompts {
            for guidance in [
                "current turn's advertised catalog",
                "Prefer native HTTP tools over ad-hoc curl/Python",
                "HTTP requests do not inherit browser cookies",
                "exact revision",
                "read that runId without preparing or sending another request",
                "configured embedded or Chromium-extension backend",
                "unique semantic locator",
                "browser_wait for uncertain readiness",
                "browser_outcome_unknown",
                "never automatically replay the mutation",
                "USER performs final functional and visual acceptance",
            ] {
                assert!(prompt.contains(guidance), "Missing guidance: {guidance}");
            }
            assert_eq!(
                prompt.matches("current turn's advertised catalog").count(),
                1
            );
        }
        assert_eq!(agent.instructions, "Keep my project's API conventions.");
    }
}
