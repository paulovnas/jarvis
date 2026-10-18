use super::*;
use crate::agent::{
    authoring::{answer_with, execute, Mutation},
    tests::{options, session, Fixture},
    ApprovalMode, Step,
};

fn call() -> ToolCall {
    ToolCall {
        id: "skill-batch".into(),
        name: "jarvis_propose_skills".into(),
        args: json!({"summary":"Skills úteis para o designer 3D deste projeto", "skills":[
            {"source":"studio/blender-skills","skillId":"modeling","reason":"Validar a geometria da cena existente."},
            {"source":"studio/blender-skills","skillId":"materials","reason":"Revisar os materiais da cena."}
        ]}),
        status: "running".into(),
        output: String::new(),
        duration_ms: 0,
    }
}

#[tokio::test]
async fn batch_requires_one_yolo_review_and_approved_skills_are_readable_in_the_same_turn() {
    for approved in [false, true] {
        let fixture = Fixture::new();
        crate::skills::installation::tests::repository(&fixture.root);
        let frozen = crate::skills::active(&fixture.root, &fixture.root)
            .await
            .unwrap();
        let session = session(&fixture);
        let signal = session
            .reserve(
                "Procure skills úteis para nosso designer 3D".into(),
                options(ApprovalMode::Yolo),
            )
            .unwrap();
        let tool = call();
        jsonschema::validate(&definition()["parameters"], &tool.args).unwrap();
        session
            .update(true, |data| {
                data.turns.last_mut().unwrap().turn.steps.push(Step {
                    tools: vec![tool.clone()],
                    ..Step::default()
                });
            })
            .unwrap();
        let state = crate::persistence::AppState::default();
        let oauth = crate::openai_codex::OpenAiCodexState::default();
        let mcp = crate::mcp::McpState::default();
        let execution = execute(
            &session,
            &session,
            &state,
            &oauth,
            &mcp,
            &fixture.root,
            &tool,
            signal,
        );
        tokio::pin!(execution);
        let pending = tokio::select! {
            result = &mut execution => panic!("Batch must wait for native approval: {result:?}"),
            result = tokio::time::timeout(std::time::Duration::from_secs(5), async {
                loop {
                    if let Some(pending) = session.snapshot().unwrap().pending_authoring { break pending; }
                    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                }
            }) => result.unwrap(),
        };
        let Target::Skills { skills } = &pending.target else {
            panic!("Expected skill batch")
        };
        assert_eq!(skills.len(), 2);
        assert_eq!(skills[0].description, "Validate existing Blender geometry");
        assert!(crate::skills::active(&fixture.root, &fixture.root)
            .await
            .unwrap()
            .is_empty());
        answer_with(
            &session,
            &pending.turn_id,
            &pending.tool_id,
            approved,
            Some("Only this set".into()),
            |mutation, revision, _, (signal, cleanup)| {
                assert!(revision.is_none());
                let Mutation::Skills(prepared) = mutation else {
                    panic!("Expected skill mutation")
                };
                Ok((
                    apply(&fixture.root, &prepared, || {
                        *signal.borrow() || *cleanup.borrow()
                    })?,
                    false,
                ))
            },
        )
        .unwrap();
        let result: Value = serde_json::from_str(&execution.await.unwrap()).unwrap();
        assert_eq!(
            result["status"],
            if approved { "applied" } else { "rejected" }
        );
        let refreshed = crate::skills::refresh_snapshot(&fixture.root, &fixture.root, &frozen)
            .await
            .unwrap();
        assert_eq!(refreshed.len(), if approved { 2 } else { 0 });
        if approved {
            let skill = refreshed
                .iter()
                .find(|skill| skill.name == "modeling")
                .unwrap();
            let output = crate::agent::core_runtime::read_skill(
                &session,
                &fixture.root,
                &json!({"id":skill.id}),
                &refreshed,
            )
            .await
            .unwrap();
            assert!(output.contains("Inspect the current scene"));
        }
        assert!(answer_with(
            &session,
            &pending.turn_id,
            &pending.tool_id,
            true,
            None,
            |_, _, _, _| panic!("Approval must never replay")
        )
        .is_err());
    }
}

#[tokio::test]
async fn malformed_marketplace_or_installation_calls_fail_before_network_or_review() {
    let fixture = Fixture::new();
    let (_stop, signal) = tokio::sync::watch::channel(false);
    for args in [
        json!({"action":"search","query":"blender","command":"execute"}),
        json!({"action":"install","source":"studio/blender-skills","skillId":"modeling"}),
    ] {
        assert!(jsonschema::validate(&marketplace_definition()["parameters"], &args).is_err());
        assert!(marketplace(&fixture.root, &args).await.is_err());
    }
    let mut tool = call();
    tool.args["skills"][0]["command"] = json!("execute");
    assert!(prepare(&fixture.root, &tool, signal.clone()).await.is_err());
    tool.args = json!({"summary":"Useful skills","skills":[]});
    assert!(prepare(&fixture.root, &tool, signal).await.is_err());
    let plan = crate::agent::authoring_tools_for_turn(crate::agent::Mode::Plan, false);
    assert!(plan
        .iter()
        .any(|tool| tool["name"] == "jarvis_skill_marketplace"));
    assert!(!plan
        .iter()
        .any(|tool| tool["name"] == "jarvis_propose_skills"));
}

#[tokio::test]
async fn stopping_preparation_does_not_open_an_approval_or_wait_for_downloads() {
    let fixture = Fixture::new();
    let session = session(&fixture);
    session
        .reserve("Find relevant skills".into(), options(ApprovalMode::Yolo))
        .unwrap();
    let (_stop, signal) = tokio::sync::watch::channel(true);
    let state = crate::persistence::AppState::default();
    let oauth = crate::openai_codex::OpenAiCodexState::default();
    let mcp = crate::mcp::McpState::default();
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        execute(
            &session,
            &session,
            &state,
            &oauth,
            &mcp,
            &fixture.root,
            &call(),
            signal,
        ),
    )
    .await
    .unwrap();
    assert_eq!(result.unwrap_err().code, "cancelled");
    assert!(session.snapshot().unwrap().pending_authoring.is_none());
}
