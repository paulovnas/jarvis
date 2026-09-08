use super::*;

pub(crate) fn example() -> Catalog {
    let agent = AgentDefinition {
        id: "a".repeat(32),
        name: "Researcher".into(),
        description: "Inspect the project".into(),
        instructions: "Read the project and return evidence.".into(),
        capability: Capability::ReadOnly,
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
fn appearance_roundtrips_and_old_catalogs_remain_readable() {
    let home = tempfile::tempdir().unwrap();
    fs::create_dir(home.path().join(".jarvis")).unwrap();
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
    legacy["flows"][0]
        .as_object_mut()
        .unwrap()
        .remove("appearance");
    let legacy: Catalog = serde_json::from_value(legacy).unwrap();
    legacy.validate().unwrap();
    assert!(legacy.agents[0].appearance.is_none());
    assert!(legacy.flows[0].appearance.is_none());
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
    fs::create_dir(home.path().join(".jarvis")).unwrap();
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
    for id in ["standard", "designer", "planned", "complete"] {
        let mut flow = example().flows[0].clone();
        flow.id = id.into();
        assert!(apply(&mut catalog, Mutation::SaveFlow { flow }).is_err());
        assert!(apply(&mut catalog, Mutation::DeleteFlow { id: id.into() }).is_err());
    }
    let home = tempfile::tempdir().unwrap();
    fs::create_dir(home.path().join(".jarvis")).unwrap();
    let file = home.path().join(".jarvis/workflow-catalog.json");
    fs::write(&file, b"invalid JSON").unwrap();
    assert!(change(home.path(), 0, |_| Ok(())).is_err());
    assert_eq!(fs::read(file).unwrap(), b"invalid JSON");
}
