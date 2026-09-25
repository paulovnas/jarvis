use super::*;
use crate::agent::journal;
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
                manual_validation: false,
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
            active.wait_for_authoring(Pending {
                request,
                mutation: Mutation::Catalog(mutation),
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
        |_, _, _| Ok((String::new(), false))
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
        move |mutation, revision, note| {
            assert_eq!(revision, Some(0));
            assert_eq!(note, Some("Aprovado para este catálogo"));
            assert!(matches!(
                mutation,
                Mutation::Catalog(workflow::catalog::Mutation::SaveAgent { .. })
            ));
            marker.store(true, Ordering::SeqCst);
            Ok((
                json!({"approved":true,"status":"applied","note":note,"catalogRevision":1})
                    .to_string(),
                true,
            ))
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
        |_, _, _| Ok((String::new(), false))
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
            active.wait_for_authoring(Pending {
                request,
                mutation: Mutation::Catalog(mutation),
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
        |_, _, _| panic!("a rejected proposal must not mutate the catalog"),
    )
    .unwrap();
    assert!(!changed);
    assert!(snapshot.pending_authoring.is_none());
    let output: Value = serde_json::from_str(&received.await.unwrap()).unwrap();
    assert_eq!(output["status"], "rejected");
    assert_eq!(output["note"], "Prefiro um nome mais curto");
}

#[tokio::test]
async fn publication_without_preview_confirmation_opens_native_review() {
    let fixture = Fixture::new();
    let session = session(&fixture);
    let state = AppState::default();
    let oauth = OpenAiCodexState::default();
    let root = fixture.root.join("repository");
    std::fs::create_dir(&root).unwrap();
    let git = |args: &[&str]| {
        let output = crate::background::command("git")
            .args(args)
            .current_dir(&root)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap()
    };
    git(&["init", "--initial-branch=publication-test"]);
    git(&["config", "user.name", "Jarvis Test"]);
    git(&["config", "user.email", "jarvis@example.test"]);
    git(&["commit", "--allow-empty", "--no-gpg-sign", "-m", "initial"]);
    git(&["branch", "hml"]);
    git(&[
        "remote",
        "add",
        "origin",
        fixture.root.join("remote.git").to_str().unwrap(),
    ]);
    std::fs::write(root.join("app.txt"), "pending change\n").unwrap();
    let head = git(&["rev-parse", "HEAD"]);
    let status = git(&["status", "--porcelain=v1"]);
    state
        .with_connection(&fixture.root, |db| -> Result<(), AgentError> {
            db.execute(
                "INSERT INTO workspaces (id, name) VALUES ('w1', 'Test')",
                [],
            )
            .unwrap();
            db.execute(
                "INSERT INTO projects (id, workspace_id, name, path) VALUES (?1, 'w1', 'Test', ?2)",
                rusqlite::params![
                    session.project_id().unwrap(),
                    fixture.root.to_str().unwrap()
                ],
            )
            .unwrap();
            Ok(())
        })
        .unwrap();

    let cases = [
        ("Commit, push, pr e merge na hml", false),
        ("Suba tudo o que estiverpendente, no front e no Back, crie a PR para hml e faça o merge por favor", false),
        ("Corrija por favor, não era pra ter conflito se só tem nós trabalhando nessa branch", true),
    ];
    for (user, sync_only) in cases {
        for (confirmation, force_review) in [
            (Some(json!("")), false),
            (Some(json!(" \t")), false),
            (Some(Value::Null), false),
            (None, false),
            (None, true),
        ] {
            let signal = session
                .reserve(
                    user.into(),
                    crate::agent::tests::options(ApprovalMode::Yolo),
                )
                .unwrap();
            let mut args = json!({
                "summary":"Publicar alterações solicitadas",
                "authorization":{"mode":"explicit_request","evidence":user},
                "previewOnly":force_review,
                "repositories":[{
                    "path":"repository","reset":null,"files":["app.txt"],"branch":null,
                    "commitMessage":"fix: approved change","sync":"none","push":"normal","pullRequest":null
                }]
            });
            if let Some(value) = confirmation {
                args["confirmedProposalId"] = value;
            }
            if sync_only {
                args["authorization"] = Value::Null;
                args["repositories"][0] = json!({
                    "path":"repository","reset":null,"files":[],"branch":"hml",
                    "commitMessage":null,"sync":"ff_only","push":"none","pullRequest":null
                });
            }
            jsonschema::validate(&publication::definition()["parameters"], &args).unwrap();
            let call = tool("jarvis_propose_publication", args);
            session.update(true, |data| {
                let turn = data.turns.last_mut().unwrap();
                if sync_only && !force_review {
                    turn.turn.options.approval_mode = ApprovalMode::Manual;
                }
                turn.turn.steps.push(Step { tools: vec![call.clone()], ..Step::default() });
                turn.wire.push(json!({"type":"function_call","call_id":call.id,"name":call.name,"arguments":call.args.to_string()}));
            }).unwrap();
            let execution = execute(
                &session,
                &session,
                &state,
                &oauth,
                &fixture.root,
                &call,
                signal,
            );
            tokio::pin!(execution);
            let pending = tokio::select! {
                result = &mut execution => panic!("publication returned before review: {result:?}"),
                pending = tokio::time::timeout(std::time::Duration::from_secs(5), async {
                    loop {
                        if let Some(pending) = session.snapshot().unwrap().pending_authoring {
                            break pending;
                        }
                        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                    }
                }) => pending.expect("native publication review must be visible"),
            };
            assert_eq!(pending.action, Action::Publish);
            assert!(
                matches!(pending.target, Target::Publication { ref after } if !publication::executes_without_review(after))
            );
            assert_eq!(git(&["rev-parse", "HEAD"]), head);
            assert_eq!(git(&["status", "--porcelain=v1"]), status);
            answer_with(
                &session,
                &pending.turn_id,
                &pending.tool_id,
                false,
                None,
                |_, _, _| panic!("rejection must not publish"),
            )
            .unwrap();
            let output: Value = serde_json::from_str(&execution.await.unwrap()).unwrap();
            assert_eq!(output["status"], "rejected");
            crate::agent::finish(&session, Ok(()));
        }
    }

    let signal = session
        .reserve(
            "Volte à branch hml para preparar a correção".into(),
            crate::agent::tests::options(ApprovalMode::Yolo),
        )
        .unwrap();
    let call = tool(
        "jarvis_propose_publication",
        json!({
            "summary":"Selecionar a branch local",
            "authorization":null,
            "previewOnly":false,
            "repositories":[{
                "path":"repository","reset":null,"files":[],"branch":"hml",
                "commitMessage":null,"sync":"none","push":"none","pullRequest":null
            }]
        }),
    );
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        execute(
            &session,
            &session,
            &state,
            &oauth,
            &fixture.root,
            &call,
            signal,
        ),
    )
    .await
    .expect("local preparation must not wait for publication review")
    .unwrap();
    let output: Value = serde_json::from_str(&result).unwrap();
    assert_eq!(output["status"], "published");
    assert_eq!(git(&["branch", "--show-current"]).trim(), "hml");
    assert_eq!(git(&["rev-parse", "HEAD"]), head);
    assert_eq!(git(&["status", "--porcelain=v1"]), status);
    assert!(session.snapshot().unwrap().pending_authoring.is_none());
    crate::agent::finish(&session, Ok(()));
}

#[test]
fn natural_publication_reply_is_guidance_for_the_agent_without_another_text_confirmation() {
    for note in [
        "Resolva isso de uma vez",
        "PODE FAZER, MAS OBEDEÇA O QUE ESTOU PEDINDO",
        "Pode fazer, mas exclua app.txt",
        "Não publique",
    ] {
        let output: Value = serde_json::from_str(&publication_revision_output(note)).unwrap();
        assert_eq!(output["status"], "revision_requested");
        assert_eq!(output["approved"], false);
        assert_eq!(output["note"], note);
        let guidance = output["guidance"].as_str().unwrap();
        assert!(guidance.contains("previewOnly=false and confirmedProposalId=null"));
        assert!(guidance.contains("stop if the user withdraws"));
        assert!(guidance.contains("Do not create another conversational preview"));
        assert!(guidance.contains("exact confirmation phrase"));
    }
}

#[tokio::test]
async fn publication_approval_with_a_note_requests_revision_without_applying() {
    let (_fixture, session, _signal) = reserve();
    let proposal: publication::Proposal = serde_json::from_value(json!({
        "summary":"Publicar somente os arquivos aprovados.",
        "repositories":[{
            "path":".",
            "reset":null,
            "files":["src/App.tsx","docs/picpay.ofx"],
            "branch":null,
            "commitMessage":"fix: adjust publication",
            "push":"normal",
            "pullRequest":null
        }]
    }))
    .unwrap();
    let call = tool(
        "jarvis_propose_publication",
        serde_json::to_value(&proposal).unwrap(),
    );
    let (reply, received) = oneshot::channel();
    session
        .update(true, |data| {
            let active = data.active.as_mut().unwrap();
            let turn_id = active.id.clone();
            active.wait_for_authoring(Pending {
                request: PendingProposal {
                    turn_id,
                    tool_id: call.id.clone(),
                    action: Action::Publish,
                    summary: proposal.summary.clone(),
                    catalog_revision: None,
                    target: Target::Publication {
                        after: proposal.clone(),
                    },
                    agent_references: vec![],
                },
                mutation: Mutation::Publication(proposal),
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
    let applied = Arc::new(AtomicBool::new(false));
    let marker = applied.clone();

    let (snapshot, changed) = answer_with(
        &session,
        &pending.turn_id,
        &pending.tool_id,
        true,
        Some("Ignore docs/picpay.ofx".into()),
        move |_, _, _| {
            marker.store(true, Ordering::SeqCst);
            Ok((String::new(), false))
        },
    )
    .unwrap();

    assert!(!changed);
    assert!(!applied.load(Ordering::SeqCst));
    assert!(snapshot.pending_authoring.is_none());
    let output: Value = serde_json::from_str(&received.await.unwrap()).unwrap();
    assert_eq!(output["approved"], false);
    assert_eq!(output["status"], "revision_requested");
    assert_eq!(output["note"], "Ignore docs/picpay.ofx");
    assert!(output["guidance"]
        .as_str()
        .unwrap()
        .contains("submit a revised jarvis_propose_publication proposal"));
}
