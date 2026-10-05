use super::*;

#[test]
fn image_generator_is_a_mixed_native_agent_available_solo_and_in_custom_flows() {
    let native = builtin_agent(Role::ImageGenerator).unwrap();
    assert_eq!(native.id, "builtin:image_generator");
    assert_eq!(native.name, "Gerador de imagens");
    assert_eq!(native.usage, AgentUsage::Mixed);
    assert_eq!(native.capability, Capability::WriteFiles);
    assert_eq!(native.appearance.icon, appearance::Icon::Sparkles);
    let solo = Catalog::default().resolve_agent(&native.id).unwrap();
    assert_eq!(solo.native_role, Some(Role::ImageGenerator));
    assert!(solo.instructions.contains("ComfyUI"));
    assert!(custom::allowed(&solo, "generate_image"));
    assert!(custom::allowed(&solo, "image_process"));
    for name in [
        "read",
        "list",
        "search",
        "browser_open",
        "browser_screenshot",
        "mcp_activate",
        "beads_show",
        "project_beads_show",
        "ctx_search",
        "graft_find_code",
    ] {
        assert!(custom::allowed(&solo, name), "{name}");
    }
    for name in ["bash", "http_send", "write", "video_audio", "beads_update"] {
        assert!(!custom::allowed(&solo, name), "{name}");
    }
    let mut catalog = example();
    for step in &mut catalog.flows[0].steps {
        step.agent_id = native.id.clone();
    }
    catalog.validate().unwrap();
    let run = catalog.resolve(&catalog.flows[0].id).unwrap();
    assert_eq!(run.agents[0].native_role, Some(Role::ImageGenerator));
    let native_flow = builtin_flows()
        .into_iter()
        .find(|flow| flow.id == Flow::ImageGenerator)
        .unwrap();
    assert_eq!(native_flow.steps.len(), 1);
    assert_eq!(native_flow.steps[0].agent_id, native.id);
    assert!(native_flow.connections.is_empty());
    for flow in [Flow::Planned, Flow::Complete] {
        assert!(!flow.roster().contains(&Role::ImageGenerator));
    }
}

#[test]
fn catalog_round_trip_preserves_claude_for_custom_agents_and_flow_steps() {
    let mut catalog = example();
    let choice = settings::ModelChoice {
        executor: crate::claude::Executor::Claude,
        account: String::new(),
        model: "sonnet".into(),
        reasoning: Some("high".into()),
        fallback: Some(Box::new(settings::ModelChoice {
            executor: crate::claude::Executor::Claude,
            account: String::new(),
            model: "opus".into(),
            reasoning: Some("max".into()),
            fallback: None,
        })),
    };
    catalog.agents[0].model = Some(choice.clone());
    catalog.validate().unwrap();
    let decoded: Catalog = serde_json::from_slice(&serde_json::to_vec(&catalog).unwrap()).unwrap();
    let resolved = decoded.resolve(&catalog.flows[0].id).unwrap();
    assert_eq!(resolved.agents[0].model, Some(choice));
    let mut options = crate::agent::tests::options(ApprovalMode::Manual);
    custom::apply_model(&mut options, &resolved.agents[0]);
    assert_eq!(options.executor, crate::claude::Executor::Claude);
    assert!(options.account.is_empty());
    catalog.agents[0].model.as_mut().unwrap().account = "fake-claude".into();
    assert!(catalog.validate().is_err());
}

pub(crate) fn example() -> Catalog {
    let agent = AgentDefinition {
        id: "a".repeat(32),
        name: "Researcher".into(),
        description: "Inspect the project".into(),
        instructions: "Read the project and return evidence.".into(),
        native_role: None,
        usage: AgentUsage::Mixed,
        capability: Capability::ReadOnly,
        denied_tools: vec![],
        model: None,
        appearance: None,
    };
    let entry = "b".repeat(32);
    let end = "c".repeat(32);
    Catalog {
        revision: 0,
        agents: vec![agent.clone()],
        flows: vec![FlowDefinition {
            appearance: None,
            id: "d".repeat(32),
            name: "Research and review".into(),
            description: "".into(),
            entry: entry.clone(),
            max_steps: 6,
            steps: vec![
                Step {
                    id: entry.clone(),
                    agent_id: agent.id.clone(),
                    instructions: "First".into(),
                    position: Position { x: 10.0, y: 20.0 },
                    next: Some(end.clone()),
                    on_rework: None,
                },
                Step {
                    id: end,
                    agent_id: agent.id.clone(),
                    instructions: "Review".into(),
                    position: Position { x: 320.0, y: 20.0 },
                    next: None,
                    on_rework: Some(entry),
                },
            ],
        }],
    }
}

#[test]
fn validates_connected_graph_and_rejects_missing_links_cycles_and_unreachable_steps() {
    let valid = example();
    valid.validate().unwrap();
    for modify in [
        |c: &mut Catalog| c.flows[0].steps[1].next = Some("e".repeat(32)),
        |c: &mut Catalog| c.flows[0].steps[1].next = Some(c.flows[0].entry.clone()),
        |c: &mut Catalog| c.flows[0].steps[0].next = None,
        |c: &mut Catalog| c.flows[0].entry = "f".repeat(32),
        |c: &mut Catalog| c.flows[0].steps[0].agent_id = "f".repeat(32),
        |c: &mut Catalog| c.flows[0].steps[0].position.x = f64::NAN,
        |c: &mut Catalog| c.flows[0].max_steps = 1,
        |c: &mut Catalog| c.flows[0].steps[1].id = c.flows[0].entry.clone(),
    ] {
        let mut invalid = valid.clone();
        modify(&mut invalid);
        assert!(invalid.validate().is_err());
    }
}

#[test]
fn custom_flows_accept_immutable_native_agents_and_freeze_their_runtime_contract() {
    let mut catalog = example();
    for step in &mut catalog.flows[0].steps {
        step.agent_id = "builtin:designer".into();
    }
    catalog.validate().unwrap();
    let run = catalog.resolve(&catalog.flows[0].id).unwrap();
    assert_eq!(run.agents.len(), 1);
    assert_eq!(run.agents[0].native_role, Some(Role::Designer));
    assert_eq!(run.agents[0].capability, Capability::Commands);
    assert!(run.agents[0]
        .instructions
        .contains("Deliver the requested frontend/design outcome within the authorized scope"));

    catalog.flows[0].steps[0].agent_id = "builtin:unknown".into();
    assert!(catalog.validate().is_err());
}

#[test]
fn native_canvas_topology_is_derived_from_the_real_delegation_contract() {
    let agents = builtin_agents();
    assert_eq!(agents.len(), 10);
    assert!(agents.iter().all(|agent| agent.immutable));
    assert_eq!(
        agents
            .iter()
            .find(|agent| agent.role == Role::Designer)
            .unwrap()
            .capability,
        Capability::Commands
    );
    let github = agents
        .iter()
        .find(|agent| agent.role == Role::Github)
        .unwrap();
    assert_eq!(github.name, "GitHub");
    assert_eq!(github.usage, AgentUsage::Mixed);
    let video = builtin_agent(Role::Video).unwrap();
    assert_eq!(video.name, "Gerador de vídeos");
    assert_eq!(video.usage, AgentUsage::Mixed);
    assert_eq!(video.capability, Capability::Commands);
    assert_eq!(video.appearance.icon, appearance::Icon::Film);

    let flows = builtin_flows();
    assert_eq!(flows.len(), 6);
    let video = flows.iter().find(|flow| flow.id == Flow::Video).unwrap();
    assert_eq!(video.steps.len(), 1);
    assert_eq!(video.steps[0].agent_id, "builtin:video");
    assert!(video.connections.is_empty());
    for flow in flows
        .iter()
        .filter(|flow| matches!(flow.id, Flow::Planned | Flow::Complete))
    {
        assert!(flow
            .steps
            .iter()
            .all(|step| step.agent_id != "builtin:video"));
    }
    let complete = flows.iter().find(|flow| flow.id == Flow::Complete).unwrap();
    let pairs: Vec<_> = complete
        .connections
        .iter()
        .map(|connection| (connection.source.as_str(), connection.target.as_str()))
        .collect();
    assert!(pairs.contains(&("builtin:complete:orchestrator", "builtin:complete:designer")));
    assert!(!pairs.contains(&("builtin:complete:planner", "builtin:complete:designer")));
    for flow in &flows {
        for connection in &flow.connections {
            let source = flow
                .steps
                .iter()
                .find(|step| step.id == connection.source)
                .and_then(|step| Role::from_builtin_id(&step.agent_id))
                .unwrap();
            let target = flow
                .steps
                .iter()
                .find(|step| step.id == connection.target)
                .and_then(|step| Role::from_builtin_id(&step.agent_id))
                .unwrap();
            assert!(source.spawns(flow.id, target));
        }
    }
}

#[test]
fn catalog_view_keeps_user_definitions_separate_from_immutable_native_graphs() {
    let value = serde_json::to_value(CatalogView::from(example())).unwrap();
    assert_eq!(value["agents"].as_array().unwrap().len(), 1);
    assert_eq!(value["flows"].as_array().unwrap().len(), 1);
    assert_eq!(value["builtinAgents"].as_array().unwrap().len(), 10);
    assert_eq!(value["builtinFlows"].as_array().unwrap().len(), 6);
    assert_eq!(value["builtinFlows"][2]["id"], "video");
    let complete = value["builtinFlows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|flow| flow["id"] == "complete")
        .unwrap();
    assert_eq!(complete["immutable"], true);
}

#[test]
fn appearance_roundtrips_and_old_catalogs_remain_readable() {
    let home = tempfile::tempdir().unwrap();
    fs::create_dir(crate::data_dir::root(home.path())).unwrap();
    let mut catalog = example();
    let appearance: Appearance =
        serde_json::from_value(serde_json::json!({ "icon": "shield", "color": "purple" })).unwrap();
    catalog.agents[0].appearance = Some(appearance);
    catalog.flows[0].appearance = Some(appearance);
    change(home.path(), 0, |current| {
        *current = catalog;
        Ok(())
    })
    .unwrap();
    let restored = read(home.path()).unwrap();
    assert_eq!(restored.agents[0].appearance, Some(appearance));
    assert_eq!(restored.flows[0].appearance, Some(appearance));
    let frozen = restored.resolve(&restored.flows[0].id).unwrap();
    assert_eq!(frozen.agents[0].appearance, Some(appearance));
    let mut legacy = serde_json::to_value(restored).unwrap();
    legacy["agents"][0]
        .as_object_mut()
        .unwrap()
        .remove("appearance");
    legacy["agents"][0].as_object_mut().unwrap().remove("usage");
    legacy["flows"][0]
        .as_object_mut()
        .unwrap()
        .remove("appearance");
    let legacy: Catalog = serde_json::from_value(legacy).unwrap();
    legacy.validate().unwrap();
    assert!(legacy.agents[0].appearance.is_none());
    assert_eq!(legacy.agents[0].usage, AgentUsage::FlowOnly);
    assert!(legacy.flows[0].appearance.is_none());
}

#[test]
fn agent_usage_controls_direct_selection_and_flow_membership() {
    let mut catalog = example();
    let agent_id = catalog.agents[0].id.clone();
    assert!(catalog.resolve_agent(&agent_id).is_ok());

    catalog.agents[0].usage = AgentUsage::Solo;
    assert!(catalog.resolve_agent(&agent_id).is_ok());
    assert!(catalog.validate().is_err());

    catalog.flows.clear();
    catalog.validate().unwrap();
    catalog.agents[0].usage = AgentUsage::FlowOnly;
    assert!(catalog.resolve_agent(&agent_id).is_err());

    let github = catalog.resolve_agent("builtin:github").unwrap();
    assert_eq!(github.native_role, Some(Role::Github));
    assert_eq!(github.usage, AgentUsage::Mixed);
    let video = builtin_agent(Role::Video).unwrap();
    assert_eq!(video.name, "Gerador de vídeos");
    assert_eq!(video.usage, AgentUsage::Mixed);
    assert_eq!(video.capability, Capability::Commands);
    assert_eq!(video.appearance.icon, appearance::Icon::Film);
    assert!(catalog.resolve_agent("builtin:planner").is_err());
}

#[test]
fn video_generator_labels_preserve_native_identity_and_user_owned_names() {
    let mut catalog = example();
    // A user-owned name matching the former built-in label must stay unchanged.
    catalog.agents[0].name = "Criador de vídeos".into();
    catalog.agents[0].description = "Minha apresentação personalizada".into();
    let custom_id = catalog.agents[0].id.clone();
    let encoded = serde_json::to_vec(&catalog).unwrap();
    let restored: Catalog = serde_json::from_slice(&encoded).unwrap();
    let custom = restored.resolve_agent(&custom_id).unwrap();
    assert_eq!(custom.name, "Criador de vídeos");
    assert_eq!(custom.description, "Minha apresentação personalizada");
    assert_eq!(custom.instructions, catalog.agents[0].instructions);

    let video = restored.resolve_agent("builtin:video").unwrap();
    assert_eq!(video.id, "builtin:video");
    assert_eq!(video.name, "Gerador de vídeos");
    assert_eq!(video.native_role, Some(Role::Video));
    assert!(video.description.contains("narração e música"));
    let flow = builtin_flows()
        .into_iter()
        .find(|flow| flow.id == Flow::Video)
        .unwrap();
    assert_eq!(flow.steps[0].agent_id, "builtin:video");
    assert!(flow.description.contains("narração, música"));
}

#[test]
fn video_director_contract_reaches_native_selection_and_existing_custom_flows() {
    let mut catalog = example();
    for step in &mut catalog.flows[0].steps {
        step.agent_id = "builtin:video".into();
    }
    catalog.validate().unwrap();
    let restored: Catalog = serde_json::from_slice(&serde_json::to_vec(&catalog).unwrap()).unwrap();
    let direct = restored.resolve_agent("builtin:video").unwrap();
    let run = restored.resolve(&restored.flows[0].id).unwrap();
    let prompts = [
        contracts::prompt(Flow::Video, Role::Video, "builtin:video"),
        custom::direct_instructions(&direct),
        custom::instructions(&run.agents[0]),
    ];
    for prompt in prompts {
        for requirement in [
            "creative marketing director and video producer",
            "project_knowledge product/design/learning",
            "real navigation and clicks",
            "Group at most three concise questions",
            "script/storyboard",
            "without redundant approvals",
            "Native voice generation supports PT-BR only",
            "New padding does not alter existing WAVs",
            "first/last syllables and natural pauses",
            "reuse confirmed music without asking the same question again",
            "enabled only when the user explicitly requests /brag",
            "evidence ledger in brag-plan.md",
            "returned attribution, source URL and license link into share-copy.txt",
            "static checks alone do not prove that speech sounds natural",
        ] {
            assert!(prompt.contains(requirement), "Missing: {requirement}");
        }
    }
    for tool in [
        "ask_user",
        "read",
        "list",
        "search",
        "project_knowledge",
        "browser_open",
        "browser_navigate",
        "browser_snapshot",
        "browser_click",
        "browser_screenshot",
        "video_audio",
    ] {
        assert!(custom::allowed(&direct, tool), "{tool}");
        assert!(custom::allowed(&run.agents[0], tool), "{tool}");
    }
}

#[test]
fn video_briefing_guards_survive_native_selection_and_serialized_custom_nodes() {
    let mut catalog = example();
    for step in &mut catalog.flows[0].steps {
        step.agent_id = "builtin:video".into();
    }
    let restored: Catalog = serde_json::from_slice(&serde_json::to_vec(&catalog).unwrap()).unwrap();
    let direct = restored.resolve_agent("builtin:video").unwrap();
    let run = restored.resolve(&restored.flows[0].id).unwrap();
    for instructions in [
        contracts::prompt(Flow::Video, Role::Video, "builtin:video"),
        custom::direct_instructions(&direct),
        custom::instructions(&run.agents[0]),
    ] {
        let research = instructions.find("Research autonomously").unwrap();
        let briefing = instructions
            .find("For an underspecified new video")
            .unwrap();
        let storyboard = instructions
            .find("Before production, show a compact script/storyboard")
            .unwrap();
        let production = instructions
            .find("Direct the visual production deliberately")
            .unwrap();
        assert!(research < briefing && briefing < storyboard && storyboard < production);
        assert!(instructions
            .contains("one collaborative creative process for every new video, including Brag"));
        assert!(instructions.contains(
            "For an underspecified new video, including a bare \"faça um brag\", use ask_user"
        ));
        assert!(instructions.contains("Ask only material unanswered choices"));
        assert!(instructions.contains("dismissal or cancellation is not acceptance"));
        assert!(instructions.contains("Reuse answers and an accepted brief on follow-up turns"));
        assert!(instructions.contains("an explicit request to decide and execute is sufficient"));
        assert!(instructions.contains("do not produce a final film based on unanswered questions"));
        assert!(instructions.contains("Never claim a live screen was observed"));
        assert!(instructions.contains("compact claim-to-source and scene mapping"));
        assert!(instructions.contains("choreograph interactions, callouts and camera movement"));
        assert!(instructions.contains("retain the actual product's identity"));
    }
}

#[test]
fn designer_direction_and_explicit_improvement_choices_reach_native_and_custom_execution() {
    let mut catalog = example();
    for step in &mut catalog.flows[0].steps {
        step.agent_id = "builtin:designer".into();
    }
    catalog.validate().unwrap();
    let restored: Catalog = serde_json::from_slice(&serde_json::to_vec(&catalog).unwrap()).unwrap();
    assert!(restored.resolve_agent("builtin:designer").is_err());
    let run = restored.resolve(&restored.flows[0].id).unwrap();
    let custom_node = custom::instructions(&run.agents[0]);
    for instructions in [
        contracts::prompt(Flow::Designer, Role::Designer, "builtin:designer"),
        contracts::prompt(Flow::Planned, Role::Designer, "delegated"),
        contracts::prompt(Flow::Complete, Role::Designer, "delegated"),
        custom_node.clone(),
    ] {
        let stages = [
            "1. Establish the actual design problem",
            "2. Notice valuable improvements",
            "3. Resolve relevant Open Design contracts",
            "4. Implement and perform a real design review",
            "5. Preserve direction and hand off evidence",
        ];
        let positions: Vec<_> = stages
            .iter()
            .map(|stage| instructions.find(stage).unwrap())
            .collect();
        assert!(positions.windows(2).all(|pair| pair[0] < pair[1]));
        for requirement in [
            "senior product UI/UX designer and art director",
            "offer two concrete directions",
            "requireExplicitAnswer=true for optional scope expansion",
            "No reply, timeout, a dismissed question or a recommended default is authorization",
            "preserve the original scope",
            "Broad improvement authority already given by the user is sufficient",
            "An already-specified small correction does not need a new briefing exercise",
            "relevant USAGE.md, DESIGN.md, token contract and component manifest/example",
            "a short prose excerpt is not the complete component contract",
            "affected loading, empty, error, populated and edge states",
            "Fix concrete defects found by this review",
            "a keyboard event is only synthetic",
            "a self-assigned quality score is not evidence",
            "a suggested improvement remains pending until accepted",
        ] {
            assert!(instructions.contains(requirement), "Missing: {requirement}");
        }
    }
    assert!(custom_node.contains("current Designer execution only"));
    assert!(custom_node.contains("Use ask_user directly"));
    assert!(!custom_node.contains("hub_request_guidance"));
    for tool in [
        "ask_user",
        "design_brief",
        "design_search",
        "design_read",
        "write",
        "edit",
        "project_knowledge",
        "browser_snapshot",
        "browser_screenshot",
    ] {
        assert!(custom::allowed(&run.agents[0], tool), "{tool}");
    }
}

#[test]
fn coordinated_designer_improvements_require_the_real_user_choice_and_preserve_scope() {
    for (flow, role) in [
        (Flow::Planned, Role::Planner),
        (Flow::Complete, Role::Orchestrator),
    ] {
        let instructions = contracts::prompt(flow, role, "main");
        for requirement in [
            "ask_user with requireExplicitAnswer=true",
            "An existing broad user authorization is sufficient",
            "a recommendation, timeout or dismissed question is not consent",
            "Preserve the original requested work and relay only the actual decision",
            "without granting unrelated browser or filesystem access",
        ] {
            assert!(instructions.contains(requirement), "Missing: {requirement}");
        }
    }
}

#[test]
fn appearance_rejects_unregistered_icons_colors_and_extra_fields() {
    for value in [
        serde_json::json!({ "icon": "https://example.com/icon.svg", "color": "purple" }),
        serde_json::json!({ "icon": "bot", "color": "url(evil)" }),
        serde_json::json!({ "icon": "bot", "color": "blue", "script": "run" }),
    ] {
        assert!(serde_json::from_value::<Appearance>(value).is_err());
    }
}

#[test]
fn catalog_crud_is_atomic_revisioned_and_preserves_referenced_agents() {
    let home = tempfile::tempdir().unwrap();
    fs::create_dir(crate::data_dir::root(home.path())).unwrap();
    let initial = example();
    let agent = initial.agents[0].clone();
    let flow = initial.flows[0].clone();
    let saved = change(home.path(), 0, |c| {
        apply(
            c,
            Mutation::SaveAgent {
                agent: agent.clone(),
            },
        )
    })
    .unwrap();
    assert_eq!(saved.revision, 1);
    change(home.path(), 1, |c| {
        apply(c, Mutation::SaveFlow { flow: flow.clone() })
    })
    .unwrap();
    let frozen = read(home.path()).unwrap().resolve(&flow.id).unwrap();
    assert!(change(home.path(), 1, |c| apply(
        c,
        Mutation::DeleteFlow {
            id: flow.id.clone()
        }
    ))
    .is_err());
    assert!(change(home.path(), 2, |c| apply(
        c,
        Mutation::DeleteAgent {
            id: agent.id.clone()
        }
    ))
    .is_err());
    let mut changed_agent = agent.clone();
    changed_agent.instructions = "New instructions".into();
    change(home.path(), 2, |c| {
        apply(
            c,
            Mutation::SaveAgent {
                agent: changed_agent,
            },
        )
    })
    .unwrap();
    assert_ne!(
        frozen.agents[0].instructions,
        read(home.path()).unwrap().agents[0].instructions
    );
    change(home.path(), 3, |c| {
        apply(
            c,
            Mutation::DeleteFlow {
                id: flow.id.clone(),
            },
        )
    })
    .unwrap();
    change(home.path(), 4, |c| {
        apply(
            c,
            Mutation::DeleteAgent {
                id: agent.id.clone(),
            },
        )
    })
    .unwrap();
    let empty = read(home.path()).unwrap();
    assert!(empty.agents.is_empty() && empty.flows.is_empty());
    assert_eq!(frozen.flow.steps[0].position.x, 10.0);
}

#[test]
fn builtins_cannot_be_overridden_or_deleted_and_corruption_is_not_overwritten() {
    let mut catalog = Catalog::default();
    let mut built_in = example().agents[0].clone();
    built_in.id = "builder".into();
    assert!(apply(&mut catalog, Mutation::SaveAgent { agent: built_in }).is_err());
    for id in ["standard", "designer", "video", "planned", "complete"] {
        let mut flow = example().flows[0].clone();
        flow.id = id.into();
        assert!(apply(&mut catalog, Mutation::SaveFlow { flow }).is_err());
        assert!(apply(&mut catalog, Mutation::DeleteFlow { id: id.into() }).is_err());
    }
    let home = tempfile::tempdir().unwrap();
    fs::create_dir(crate::data_dir::root(home.path())).unwrap();
    let file = crate::data_dir::root(home.path()).join("workflow-catalog.json");
    fs::write(&file, b"invalid JSON").unwrap();
    assert!(change(home.path(), 0, |_| Ok(())).is_err());
    assert_eq!(fs::read(file).unwrap(), b"invalid JSON");
}
