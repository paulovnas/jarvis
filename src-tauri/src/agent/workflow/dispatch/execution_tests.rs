use super::super::tests::{hub, job};
use super::*;

#[tokio::test]
async fn launched_native_image_canvas_step_accepts_its_declared_completion_route() {
    let (_fixture, hub) = hub();
    let mut catalog = catalog::tests::example();
    let target = catalog.flows[0].steps[1].id.clone();
    let step = &mut catalog.flows[0].steps[0];
    step.agent_id = "builtin:image_generator".into();
    step.next = None;
    step.routes = vec![catalog::Route {
        id: "review".into(),
        label: "Revisar imagem".into(),
        target: Some(target),
    }];
    let definition = catalog.resolve(&catalog.flows[0].id).unwrap();
    let native = definition
        .agents
        .iter()
        .find(|agent| agent.id == "builtin:image_generator")
        .unwrap()
        .clone();
    let mut worker = job(&hub, Role::ImageGenerator, ".");
    worker.custom_agent = Some(native);
    worker.custom_step_id = Some(definition.flow.entry.clone());
    worker.options.workflow = Some(Flow::Custom);
    hub.mutate(|state| {
        state.flow = Flow::Custom;
        state.custom_definition = Some(definition);
        state.jobs.insert(worker.id.clone(), worker.clone());
        Ok(())
    })
    .unwrap();

    let execution = execution_for_job(hub.clone(), &worker).unwrap();
    assert_eq!(execution.flow, Flow::Custom);
    assert!(execution.image_generator());
    let mut handoff = Handoff {
        verdict: Verdict::Completed,
        summary: "Image candidate ready for review".into(),
        outcomes: vec!["The configured image stage delivered its candidate".into()],
        evidence: vec![],
        validation: vec![],
        limitations: vec![],
        task_ids: vec![],
        route: Some("unknown".into()),
    };
    assert!(
        complete(&execution, handoff.clone(), hub.root_signal.clone())
            .await
            .is_err()
    );
    assert!(hub.job(&worker.id).unwrap().handoff.is_none());
    handoff.route = Some("review".into());
    complete(&execution, handoff.clone(), hub.root_signal.clone())
        .await
        .unwrap();
    assert_eq!(hub.job(&worker.id).unwrap().handoff, Some(handoff));
}

#[test]
fn custom_canvas_image_calls_cannot_create_agents_outside_the_graph() {
    for native in [false, true] {
        let (_fixture, hub) = hub();
        let mut agent = if native {
            catalog::Catalog::default()
                .resolve_agent("builtin:image_generator")
                .unwrap()
        } else {
            catalog::tests::example().agents.remove(0)
        };
        agent.capability = catalog::Capability::WriteFiles;
        let role = agent.native_role.unwrap_or(Role::Custom);
        let mut worker = job(&hub, role, ".");
        worker.custom_agent = Some(agent);
        worker.custom_step_id = Some("b".repeat(32));
        hub.mutate(|state| {
            state.flow = Flow::Custom;
            state.jobs.insert(worker.id.clone(), worker.clone());
            Ok(())
        })
        .unwrap();
        let execution = execution_for_job(hub.clone(), &worker).unwrap();
        assert!(execution.allowed("generate_image"));
        let before = hub.manifest.lock().unwrap().jobs.len();
        assert!(image_job(&execution, &json!({"prompt":"Generate a sprite"})).is_err());
        assert!(prepare_spawn(
            &execution,
            Dispatch {
                role: Role::ImageGenerator,
                phase: Phase::Implementation,
                title: "Generate a sprite".into(),
                prompt: "Create the requested image candidate".into(),
                acceptance: vec!["Deliver the generated image".into()],
                scope: vec![".".into()],
                bead_id: None,
                dependencies: vec![],
            },
        )
        .is_err());
        assert_eq!(hub.manifest.lock().unwrap().jobs.len(), before);
    }
}

#[test]
fn managed_image_children_keep_their_image_flow_and_retry_identity() {
    for flow in [Flow::Standard, Flow::Custom] {
        let (_fixture, hub) = hub();
        hub.mutate(|state| {
            state.flow = flow;
            if flow == Flow::Custom {
                let mut agent = catalog::tests::example().agents.remove(0);
                agent.capability = catalog::Capability::WriteFiles;
                state.custom_agent = Some(agent);
            }
            Ok(())
        })
        .unwrap();
        let parent = Execution {
            hub: hub.clone(),
            id: "main".into(),
            role: if flow == Flow::Custom {
                Role::Custom
            } else {
                Role::Builder
            },
            flow,
            scope: vec![".".into()],
        };
        let (child, created) = image_job(&parent, &json!({"prompt":"Generate a sprite"})).unwrap();
        assert!(created);
        assert!(child.custom_agent.is_none());
        assert!(child.custom_step_id.is_none());
        assert_eq!(child.options.workflow, Some(Flow::ImageGenerator));
        assert_eq!(
            execution_for_job(hub.clone(), &child).unwrap().flow,
            Flow::ImageGenerator
        );
        hub.mutate(|state| {
            state.jobs.get_mut(&child.id).unwrap().status = Status::Failed;
            Ok(())
        })
        .unwrap();
        let (retry, _) =
            prepare_image_retry(&parent, &child.id, "Resume the same request").unwrap();
        assert_eq!(retry.id, child.id);
        assert_eq!(retry.attempts, 2);
        assert_eq!(hub.manifest.lock().unwrap().jobs.len(), 1);
        assert_eq!(
            execution_for_job(hub, &retry).unwrap().flow,
            Flow::ImageGenerator
        );
    }
}
