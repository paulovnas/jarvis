use super::*;
use crate::agent::{
    authoring::{answer_with, execute, Mutation},
    tests::{options, session, Fixture},
    ApprovalMode, Step,
};
use crate::{openai_codex::OpenAiCodexState, persistence::AppState};

fn proposal(root: &Path, content: &str) -> ToolCall {
    ToolCall {
        id: "project-instructions".into(),
        name: "jarvis_propose_project_instructions".into(),
        args: json!({"revision":catalog(root).unwrap()["revision"],"summary":"Registrar as convenções verificadas do projeto.","content":content}),
        status: "running".into(),
        output: String::new(),
        duration_ms: 0,
    }
}

#[test]
fn missing_file_is_only_created_after_applying_a_reviewed_change() {
    let fixture = Fixture::new();
    let current = catalog(&fixture.root).unwrap();
    assert_eq!(current["revision"], "missing");
    assert_eq!(current["content"], Value::Null);
    assert_eq!(current["exists"], false);
    let call = proposal(&fixture.root, "## Validation\nRun `bun run check`.");
    jsonschema::validate(&definition()["parameters"], &call.args).unwrap();
    let (pending, change) = prepare(&fixture.root, &call).unwrap();
    assert_eq!(pending.action, Action::Create);
    assert!(!fixture.root.join(PATH).exists());
    let Target::ProjectInstructions {
        path,
        before,
        after,
    } = pending.target
    else {
        panic!("Expected the complete reviewed file")
    };
    assert_eq!(path, PATH);
    assert!(before.is_none());
    let revision = apply(&fixture.root, change).unwrap();
    assert_eq!(fs::read_to_string(fixture.root.join(PATH)).unwrap(), after);
    assert_eq!(catalog(&fixture.root).unwrap()["revision"], revision);
}

#[test]
fn appending_preserves_user_managed_and_nested_rules_byte_for_byte() {
    let fixture = Fixture::new();
    let before = "# User rules\nKeep approvals.\n<!-- BEGIN BEADS INTEGRATION -->\nUse bd.\n<!-- END BEADS INTEGRATION -->";
    fs::write(fixture.root.join(PATH), before).unwrap();
    fs::create_dir(fixture.root.join("nested")).unwrap();
    let nested = fixture.root.join("nested/AGENTS.md");
    fs::write(&nested, "Nested scope only.").unwrap();
    let (pending, change) = prepare(
        &fixture.root,
        &proposal(&fixture.root, "Use confirmed checks."),
    )
    .unwrap();
    assert_eq!(pending.action, Action::Update);
    apply(&fixture.root, change).unwrap();
    let after = fs::read_to_string(fixture.root.join(PATH)).unwrap();
    assert!(after.starts_with(&format!("{before}\n\n{BEGIN}\n")));
    assert_eq!(fs::read_to_string(nested).unwrap(), "Nested scope only.");
}

#[test]
fn updating_its_section_preserves_surrounding_text_and_line_endings() {
    let fixture = Fixture::new();
    let prefix =
        "# Keep user rules\r\n<!-- BEGIN BEADS -->\r\nUse bd.\r\n<!-- END BEADS -->\r\n\r\n";
    let suffix = "\r\n\r\n# Keep later rules\r\nNo push.\r\n";
    let before = format!("{prefix}{BEGIN}\r\nOld guidance.\r\n{END}{suffix}");
    fs::write(fixture.root.join(PATH), &before).unwrap();
    assert_eq!(
        catalog(&fixture.root).unwrap()["editableContent"],
        "Old guidance."
    );
    let (_, change) = prepare(
        &fixture.root,
        &proposal(&fixture.root, "Updated guidance.\nVerify behavior."),
    )
    .unwrap();
    apply(&fixture.root, change).unwrap();
    assert_eq!(
        fs::read_to_string(fixture.root.join(PATH)).unwrap(),
        format!("{prefix}{BEGIN}\r\nUpdated guidance.\r\nVerify behavior.\r\n{END}{suffix}")
    );
    assert!(prepare(
        &fixture.root,
        &proposal(&fixture.root, "Updated guidance.\nVerify behavior.")
    )
    .is_err());
}

#[test]
fn stale_read_review_creation_and_deletion_preserve_the_current_file() {
    let fixture = Fixture::new();
    let absent = proposal(&fixture.root, "Use confirmed commands.");
    let (_, absent_change) = prepare(&fixture.root, &absent).unwrap();
    fs::write(fixture.root.join(PATH), "Concurrent rules.").unwrap();
    assert_eq!(
        prepare(&fixture.root, &absent).unwrap_err().code,
        "stale_project_instructions"
    );
    assert_eq!(
        apply(&fixture.root, absent_change).unwrap_err().code,
        "stale_project_instructions"
    );
    let (_, existing_change) =
        prepare(&fixture.root, &proposal(&fixture.root, "Append guidance.")).unwrap();
    fs::write(fixture.root.join(PATH), "Updated by user.").unwrap();
    assert_eq!(
        apply(&fixture.root, existing_change).unwrap_err().code,
        "stale_project_instructions"
    );
    assert_eq!(
        fs::read_to_string(fixture.root.join(PATH)).unwrap(),
        "Updated by user."
    );
    let (_, deleted_change) =
        prepare(&fixture.root, &proposal(&fixture.root, "Append guidance.")).unwrap();
    fs::remove_file(fixture.root.join(PATH)).unwrap();
    assert_eq!(
        apply(&fixture.root, deleted_change).unwrap_err().code,
        "stale_project_instructions"
    );
    assert!(!fixture.root.join(PATH).exists());
}

#[test]
fn malformed_markers_invalid_drafts_and_oversized_files_never_write() {
    let fixture = Fixture::new();
    for content in [
        "",
        "\0",
        "<!-- BEGIN BEADS -->\nOther managed rules.\n<!-- END BEADS -->",
    ] {
        assert!(prepare(&fixture.root, &proposal(&fixture.root, content)).is_err());
        assert!(!fixture.root.join(PATH).exists());
    }
    let mut call = proposal(&fixture.root, "Valid guidance.");
    call.args["path"] = json!("../AGENTS.md");
    assert!(prepare(&fixture.root, &call).is_err());
    let oversized = "a".repeat(MAX_DRAFT + 1);
    assert!(prepare(&fixture.root, &proposal(&fixture.root, &oversized)).is_err());
    for content in [
        format!("{BEGIN}\nUnclosed"),
        format!("{BEGIN}\nFirst\n{END}\n{BEGIN}\nDuplicate\n{END}"),
        format!("<!-- BEGIN BEADS -->\n{BEGIN}\nNested\n{END}\n<!-- END BEADS -->"),
        format!("{BEGIN}\n<!-- BEGIN BEADS -->\nForeign rules\n<!-- END BEADS -->\n{END}"),
        "<!-- BEGIN BEADS -->\nUnclosed foreign section".into(),
    ] {
        fs::write(fixture.root.join(PATH), &content).unwrap();
        assert!(catalog(&fixture.root).is_err());
        assert_eq!(
            fs::read_to_string(fixture.root.join(PATH)).unwrap(),
            content
        );
    }
    fs::write(fixture.root.join(PATH), "a".repeat(MAX_INSTRUCTIONS)).unwrap();
    assert!(prepare(
        &fixture.root,
        &proposal(&fixture.root, "Cannot truncate prior rules.")
    )
    .is_err());
    assert_eq!(
        fs::read_to_string(fixture.root.join(PATH)).unwrap().len(),
        MAX_INSTRUCTIONS
    );
}

#[test]
fn multibyte_content_respects_the_shared_instruction_file_byte_limit() {
    let fixture = Fixture::new();
    let before = "🦀".repeat(16_000);
    fs::write(fixture.root.join(PATH), &before).unwrap();
    assert!(prepare(&fixture.root, &proposal(&fixture.root, &"🦀".repeat(1_000))).is_err());
    assert_eq!(fs::read_to_string(fixture.root.join(PATH)).unwrap(), before);
    let oversized = "🦀".repeat(17_000);
    fs::write(fixture.root.join(PATH), &oversized).unwrap();
    assert_eq!(
        catalog(&fixture.root).unwrap_err().code,
        "project_instructions_too_large"
    );
    assert_eq!(
        fs::read_to_string(fixture.root.join(PATH)).unwrap(),
        oversized
    );
}

#[cfg(unix)]
#[test]
fn linked_instruction_files_never_modify_the_external_target() {
    let fixture = Fixture::new();
    let external = Fixture::new();
    let target = external.root.join(PATH);
    fs::write(&target, "Keep external rules.").unwrap();
    std::os::unix::fs::symlink(&target, fixture.root.join(PATH)).unwrap();
    assert!(catalog(&fixture.root).is_err());
    fs::remove_file(fixture.root.join(PATH)).unwrap();
    fs::hard_link(&target, fixture.root.join(PATH)).unwrap();
    let (_, change) = prepare(
        &fixture.root,
        &proposal(&fixture.root, "No indirect edits."),
    )
    .unwrap();
    assert!(apply(&fixture.root, change).is_err());
    assert_eq!(fs::read_to_string(target).unwrap(), "Keep external rules.");
}

#[tokio::test]
async fn yolo_still_waits_for_native_approval_and_never_replays_or_loses_concurrent_edits() {
    for (approved, concurrent) in [(false, false), (true, false), (true, true)] {
        let fixture = Fixture::new();
        let session = session(&fixture);
        let signal = session
            .reserve(
                "Prepare AGENTS.md for review.".into(),
                options(ApprovalMode::Yolo),
            )
            .unwrap();
        let state = AppState::default();
        let oauth = OpenAiCodexState::default();
        let mcp = crate::mcp::McpState::default();
        let call = proposal(&fixture.root, "Use confirmed project checks.");
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
            result = &mut execution => panic!("Instructions must wait for native approval: {result:?}"),
            result = tokio::time::timeout(std::time::Duration::from_secs(5), async {
                loop {
                    if let Some(pending) = session.snapshot().unwrap().pending_authoring { break pending; }
                    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                }
            }) => result.unwrap(),
        };
        assert!(!fixture.root.join(PATH).exists());
        assert!(matches!(
            pending.target,
            Target::ProjectInstructions { before: None, .. }
        ));
        if concurrent {
            fs::write(fixture.root.join(PATH), "Concurrent user rules.").unwrap();
        }
        answer_with(
            &session,
            &pending.turn_id,
            &pending.tool_id,
            approved,
            None,
            |mutation, _, _, _| {
                let Mutation::ProjectInstructions(change) = mutation else {
                    panic!("Expected project instruction change")
                };
                let revision = apply(&fixture.root, change)?;
                Ok((
                    json!({"approved":true,"status":"applied","revision":revision}).to_string(),
                    false,
                ))
            },
        )
        .unwrap();
        let result: Value = serde_json::from_str(&execution.await.unwrap()).unwrap();
        assert_eq!(
            result["status"],
            if concurrent {
                "failed"
            } else if approved {
                "applied"
            } else {
                "rejected"
            }
        );
        if concurrent {
            assert_eq!(result["error"]["code"], "stale_project_instructions");
            assert_eq!(
                fs::read_to_string(fixture.root.join(PATH)).unwrap(),
                "Concurrent user rules."
            );
        } else {
            assert_eq!(fixture.root.join(PATH).exists(), approved);
        }
        assert!(answer_with(
            &session,
            &pending.turn_id,
            &pending.tool_id,
            true,
            None,
            |_, _, _, _| panic!("A native approval may not be replayed")
        )
        .is_err());
    }
}
