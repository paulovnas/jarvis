use super::super::*;
use crate::agent::{
    tests::{options, session, Fixture},
    ApprovalMode, Step,
};

fn call(draft: Value) -> ToolCall {
    ToolCall {
        id: "plugin-proposal".into(),
        name: "jarvis_propose_plugin".into(),
        args: json!({"pluginsRevision":0,"summary":"Adicionar um plugin de revisão.","operation":{"action":"create","draft":draft}}),
        status: "running".into(),
        output: String::new(),
        duration_ms: 0,
    }
}

fn draft() -> Value {
    json!({"name":"review-tools","description":"Revisão do trabalho","skills":[{"name":"review","content":"---\nname: review\ndescription: Revise o trabalho\n---\nConfira o resultado."}],"mcpServers":{},"hooks":{"hooks":{"Stop":[{"hooks":[{"type":"command","command":"echo completed"}]}]}},"apps":{},"files":[]})
}

#[tokio::test]
async fn plugin_creation_requires_yolo_review_and_applies_once_without_trusting_hooks() {
    for approved in [false, true] {
        let fixture = Fixture::new();
        let session = session(&fixture);
        let signal = session
            .reserve(
                "Crie um plugin de revisão".into(),
                options(ApprovalMode::Yolo),
            )
            .unwrap();
        let state = AppState::default();
        let oauth = OpenAiCodexState::default();
        let mcp = crate::mcp::McpState::default();
        let call = call(draft());
        jsonschema::validate(&super::definition()["parameters"], &call.args).unwrap();
        session
            .update(true, |data| {
                data.turns.last_mut().unwrap().turn.steps.push(Step {
                    tools: vec![call.clone()],
                    ..Step::default()
                });
            })
            .unwrap();
        let execution = execute(
            &session,
            &session,
            &state,
            &oauth,
            &mcp,
            &fixture.root,
            &call,
            signal,
        );
        tokio::pin!(execution);
        let pending = tokio::select! {
            result = &mut execution => panic!("Plugins must wait for native approval: {result:?}"),
            result = tokio::time::timeout(std::time::Duration::from_secs(5), async {
                loop {
                    if let Some(pending) = session.snapshot().unwrap().pending_authoring { break pending; }
                    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                }
            }) => result.unwrap(),
        };
        assert!(crate::plugins::catalog(&fixture.root)
            .unwrap()
            .installed
            .is_empty());
        let Target::Plugin { preview } = &pending.target else {
            panic!("Expected plugin review");
        };
        assert!(preview
            .commands
            .iter()
            .any(|command| command.contains("echo completed")));
        answer_with(
            &session,
            &pending.turn_id,
            &pending.tool_id,
            approved,
            None,
            |change, revision, _, _| {
                let Mutation::Plugin(prepared) = change else {
                    panic!("Expected plugin mutation");
                };
                assert_eq!(revision, Some(prepared.revision()));
                let catalog = crate::plugins::apply(&fixture.root, &prepared)?;
                Ok((
                    json!({"approved":true,"status":"applied","pluginsRevision":catalog.revision})
                        .to_string(),
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
        let catalog = crate::plugins::catalog(&fixture.root).unwrap();
        assert_eq!(catalog.installed.len(), usize::from(approved));
        if approved {
            let overlay = crate::plugins::load_active(&fixture.root).unwrap();
            assert_eq!(overlay.hook_sources.len(), 1);
            assert!(!overlay.hook_sources[0].trusted);
            assert!(!crate::plugins::hook_source_authorized(
                &fixture.root,
                None,
                &overlay.hook_sources[0]
            ));
        }
        assert!(answer_with(
            &session,
            &pending.turn_id,
            &pending.tool_id,
            true,
            None,
            |_, _, _, _| panic!("Must never replay")
        )
        .is_err());
    }
}

#[tokio::test]
async fn credential_proposals_fail_before_native_review_or_catalog_mutation() {
    for secret in [
        json!({"mcpServers":{"private":{"command":"node","env":{"API_KEY":"private-secret"}}}}),
        json!({"skills":[{"name":"review","content":"API_KEY=private-secret"}]}),
        json!({"files":[{"path":"scripts/connect.js","content":"const API_KEY = 'private-secret';"}]}),
        json!({"hooks":{"hooks":{"Stop":[{"hooks":[{"type":"command","command":"sync --token private-secret"}]}]}}}),
    ] {
        let fixture = Fixture::new();
        let session = session(&fixture);
        let signal = session
            .reserve(
                "Crie um plugin de revisão".into(),
                options(ApprovalMode::Yolo),
            )
            .unwrap();
        let mut value = draft();
        value
            .as_object_mut()
            .unwrap()
            .extend(secret.as_object().unwrap().clone());
        let call = call(value);
        let result = execute(
            &session,
            &session,
            &AppState::default(),
            &OpenAiCodexState::default(),
            &crate::mcp::McpState::default(),
            &fixture.root,
            &call,
            signal,
        )
        .await;
        assert_eq!(result.unwrap_err().code, "plugin_credentials_require_user");
        assert!(session.snapshot().unwrap().pending_authoring.is_none());
        assert!(crate::plugins::catalog(&fixture.root)
            .unwrap()
            .installed
            .is_empty());
    }
}
