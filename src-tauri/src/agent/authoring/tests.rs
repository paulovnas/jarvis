use super::*;
use crate::agent::{
    tests::{session, Fixture},
    ApprovalMode, Mode, Step, TurnOptions,
};
use serde_json::json;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

fn tool(name: &str, args: Value) -> ToolCall {
    ToolCall {
        id: "proposal-1".into(),
        name: name.into(),
        args,
        status: "running".into(),
        output: String::new(),
        duration_ms: 0,
    }
}

fn agent_request(action: &str, revision: u64, agent: &workflow::catalog::AgentDefinition) -> Value {
    json!({
        "action":action,
        "catalogRevision":revision,
        "summary":"Criar um especialista que revise acessibilidade.",
        "agent":agent,
    })
}

fn reserve() -> (Fixture, Arc<Session>, watch::Receiver<bool>) {
    let fixture = Fixture::new();
    let session = session(&fixture);
    let signal = session
        .reserve(
            "Crie um agente de acessibilidade".into(),
            TurnOptions {
                account: "account".into(),
                model: "model".into(),
                reasoning: None,
                mode: Mode::Build,
                workflow: None,
                custom_workflow_id: None,
                custom_agent_id: None,
                approval_mode: ApprovalMode::Yolo,
            },
        )
        .unwrap();
    (fixture, session, signal)
}

#[test]
fn catalog_tools_expose_overview_and_typed_proposals() {
    let definitions = definitions();
    assert_eq!(definitions.len(), 3);
    let names: Vec<_> = definitions
        .iter()
        .filter_map(|definition| definition["name"].as_str())
        .collect();
    assert_eq!(
        names,
        [
            "jarvis_catalog",
            "jarvis_propose_agent",
            "jarvis_propose_flow"
        ]
    );
    assert!(definitions[1]["parameters"]["properties"]["agent"].is_object());
    assert!(definitions[2]["parameters"]["properties"]["flow"].is_object());
    let native_agent_ids = definitions[2]["parameters"]["properties"]["flow"]["properties"]
        ["steps"]["items"]["properties"]["agentId"]["anyOf"][1]["enum"]
        .as_array()
        .unwrap();
    assert!(native_agent_ids.contains(&json!("builtin:designer")));

    let catalog = workflow::catalog::tests::example();
    let overview: Value =
        serde_json::from_str(&catalog_output(&catalog, &json!({"view":"overview"})).unwrap())
            .unwrap();
    assert_eq!(overview["revision"], 0);
    assert_eq!(overview["builtIn"]["mutable"], false);
    assert!(overview["builtIn"]["agents"]
        .as_array()
        .unwrap()
        .iter()
        .any(|agent| agent["id"] == "builtin:designer" && agent["capability"] == "commands"));
    assert!(overview["builtIn"]["flows"]
        .as_array()
        .unwrap()
        .iter()
        .any(|flow| flow["id"] == "complete" && flow["connections"].is_array()));
    assert_eq!(overview["custom"]["agents"][0]["name"], "Researcher");
    assert!(overview["toolCatalog"]
        .as_array()
        .unwrap()
        .iter()
        .any(|tool| tool["id"] == "jarvis_propose_agent"));
    let bash = overview["toolCatalog"]
        .as_array()
        .unwrap()
        .iter()
        .find(|tool| tool["id"] == "bash")
        .unwrap();
    assert_eq!(bash["capabilities"], json!(["commands"]));
    assert!(!overview
        .to_string()
        .contains("Read the project and return evidence"));
    assert!(INSTRUCTIONS.contains("Jarvis product capabilities"));
    assert!(INSTRUCTIONS.contains("explicitly approves"));

    let detail: Value = serde_json::from_str(
        &catalog_output(&catalog, &json!({"view":"agent","id":catalog.agents[0].id})).unwrap(),
    )
    .unwrap();
    assert_eq!(
        detail["agent"]["instructions"],
        catalog.agents[0].instructions
    );
    let native: Value = serde_json::from_str(
        &catalog_output(&catalog, &json!({"view":"agent","id":"builtin:designer"})).unwrap(),
    )
    .unwrap();
    assert_eq!(native["mutable"], false);
    assert_eq!(native["agent"]["role"], "designer");
    assert!(native["agent"]["instructions"]
        .as_str()
        .unwrap()
        .contains("Implement only assigned visual scope"));
}

#[test]
fn assisted_flow_proposals_can_reference_native_agents() {
    let catalog = workflow::catalog::tests::example();
    let mut flow = catalog.flows[0].clone();
    flow.id = "e".repeat(32);
    for step in &mut flow.steps {
        step.agent_id = "builtin:designer".into();
    }
    let call = tool(
        "jarvis_propose_flow",
        json!({
            "action":"create",
            "catalogRevision":catalog.revision,
            "summary":"Usar o Designer Jarvis para executar o frontend.",
            "flow":flow,
        }),
    );
    let (proposal, _) = prepare(&catalog, &call).unwrap();
    assert_eq!(proposal.agent_references.len(), 1);
    assert_eq!(proposal.agent_references[0].id, "builtin:designer");
    assert_eq!(proposal.agent_references[0].name, "Designer");
}

#[test]
fn proposals_require_the_latest_revision_and_never_target_builtins() {
    let catalog = workflow::catalog::tests::example();
    let mut created = catalog.agents[0].clone();
    created.id = "e".repeat(32);
    created.name = "Accessibility reviewer".into();
    let valid = tool(
        "jarvis_propose_agent",
        agent_request("create", catalog.revision, &created),
    );
    let (proposal, _) = prepare(&catalog, &valid).unwrap();
    assert_eq!(proposal.action, Action::Create);
    assert!(matches!(
        proposal.target,
        Target::Agent { before: None, .. }
    ));

    let stale = tool(
        "jarvis_propose_agent",
        agent_request("create", catalog.revision + 1, &created),
    );
    assert!(prepare(&catalog, &stale)
        .unwrap_err()
        .message
        .contains("catálogo mudou"));

    let mut native = created.clone();
    native.id = "builder".into();
    let immutable = tool(
        "jarvis_propose_agent",
        agent_request("update", catalog.revision, &native),
    );
    assert!(prepare(&catalog, &immutable)
        .unwrap_err()
        .message
        .contains("nativos são imutáveis"));

    let overwrite = tool(
        "jarvis_propose_agent",
        agent_request("create", catalog.revision, &catalog.agents[0]),
    );
    assert!(prepare(&catalog, &overwrite)
        .unwrap_err()
        .message
        .contains("já pertence"));
}

#[tokio::test]
async fn approval_is_correlated_durable_and_cannot_be_replayed() {
    let (_fixture, session, _signal) = reserve();
    let catalog = workflow::catalog::tests::example();
    let mut created = catalog.agents[0].clone();
    created.id = "e".repeat(32);
    let call = tool(
        "jarvis_propose_agent",
        agent_request("create", catalog.revision, &created),
    );
    let (mut request, mutation) = prepare(&catalog, &call).unwrap();
    let (reply, received) = oneshot::channel();
    session
        .update(true, |data| {
            let active = data.active.as_mut().unwrap();
            request.turn_id.clone_from(&active.id);
            active.authoring = Some(Pending {
                request,
                mutation,
                started: std::time::Instant::now(),
                reply,
            });
            let current = data.turns.last_mut().unwrap();
            current.turn.steps.push(Step {
                tools: vec![call.clone()],
                ..Step::default()
            });
            current.wire.push(json!({"type":"function_call","call_id":call.id,"name":call.name,"arguments":call.args.to_string()}));
        })
        .unwrap();
    let pending = session.snapshot().unwrap().pending_authoring.unwrap();
    assert!(answer_with(
        &session,
        "wrong-turn",
        &pending.tool_id,
        true,
        None,
        |_, _| Ok(1)
    )
    .is_err());
    let applied = Arc::new(AtomicBool::new(false));
    let marker = applied.clone();
    let (snapshot, changed) = answer_with(
        &session,
        &pending.turn_id,
        &pending.tool_id,
        true,
        Some("Aprovado para este catálogo".into()),
        move |revision, mutation| {
            assert_eq!(revision, 0);
            assert!(matches!(
                mutation,
                workflow::catalog::Mutation::SaveAgent { .. }
            ));
            marker.store(true, Ordering::SeqCst);
            Ok(1)
        },
    )
    .unwrap();
    assert!(changed);
    assert!(applied.load(Ordering::SeqCst));
    assert!(snapshot.pending_authoring.is_none());
    let output = received.await.unwrap();
    let parsed: Value = serde_json::from_str(&output).unwrap();
    assert_eq!(parsed["approved"], true);
    assert_eq!(parsed["catalogRevision"], 1);
    let (stored, _) = journal::load_all(&session.journal).unwrap();
    assert_eq!(stored[0].turn.steps[0].tools[0].status, "completed");
    assert_eq!(stored[0].turn.steps[0].tools[0].output, output);
    assert_eq!(
        stored[0]
            .wire
            .iter()
            .filter(|item| item["type"] == "function_call_output")
            .count(),
        1
    );
    assert!(answer_with(
        &session,
        &pending.turn_id,
        &pending.tool_id,
        true,
        None,
        |_, _| Ok(2)
    )
    .unwrap_err()
    .message
    .contains("não está mais"));
}

#[tokio::test]
async fn rejection_never_applies_the_catalog_mutation() {
    let (_fixture, session, _signal) = reserve();
    let catalog = workflow::catalog::tests::example();
    let mut created = catalog.agents[0].clone();
    created.id = "e".repeat(32);
    let call = tool(
        "jarvis_propose_agent",
        agent_request("create", catalog.revision, &created),
    );
    let (mut request, mutation) = prepare(&catalog, &call).unwrap();
    let (reply, received) = oneshot::channel();
    session
        .update(true, |data| {
            let active = data.active.as_mut().unwrap();
            request.turn_id.clone_from(&active.id);
            active.authoring = Some(Pending {
                request,
                mutation,
                started: std::time::Instant::now(),
                reply,
            });
            data.turns.last_mut().unwrap().turn.steps.push(Step {
                tools: vec![call],
                ..Step::default()
            });
        })
        .unwrap();
    let pending = session.snapshot().unwrap().pending_authoring.unwrap();
    let (snapshot, changed) = answer_with(
        &session,
        &pending.turn_id,
        &pending.tool_id,
        false,
        Some("Prefiro um nome mais curto".into()),
        |_, _| panic!("a rejected proposal must not mutate the catalog"),
    )
    .unwrap();
    assert!(!changed);
    assert!(snapshot.pending_authoring.is_none());
    let output: Value = serde_json::from_str(&received.await.unwrap()).unwrap();
    assert_eq!(output["status"], "rejected");
    assert_eq!(output["note"], "Prefiro um nome mais curto");
}
