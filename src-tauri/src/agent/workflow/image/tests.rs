use super::super::tests::{hub, job};
use super::*;

fn batch(ids: &[&str], sources: &[&str]) -> Value {
    json!({
        "kind":"generated_image","accountAlias":"image-account","model":"image-model",
        "images":ids.iter().map(|id| json!({"id":id,"name":format!("{id}.png"),"kind":"image","mimeType":"image/png","size":4})).collect::<Vec<_>>(),
        "sourcePaths":ids.iter().map(|id| format!("attachments/{id}/source")).collect::<Vec<_>>(),
        "exports":ids.iter().map(|id| format!("images/{id}.png")).collect::<Vec<_>>(),
        "sourceImageIds":sources,"text":"","processing":{"format":"png","images":ids.iter().enumerate().map(|(index,id)|json!({"path":format!("{id}.png"),"width":100+index,"height":200+index})).collect::<Vec<_>>()}
    })
}

fn journal(hub: &Hub, tools: Vec<ToolCall>) -> Vec<StoredTurn> {
    let mut turns = hub.root.data.lock().unwrap().turns.clone();
    turns.last_mut().unwrap().turn.steps.push(Step {
        tools,
        ..Default::default()
    });
    turns
}

fn tool(name: &str, status: &str, result: Value) -> ToolCall {
    ToolCall {
        id: name.into(),
        name: name.into(),
        args: json!({}),
        status: status.into(),
        output: result.to_string(),
        duration_ms: 0,
    }
}

#[test]
fn processing_replaces_four_originals_without_duplicate_parent_previews() {
    let (_fixture, hub) = hub();
    let mut job = job(&hub, Role::ImageGenerator, ".jarvis-image-attachments");
    job.status = Status::Completed;
    let turns = journal(
        &hub,
        vec![
            tool(
                "generate_image",
                "completed",
                batch(&["a", "b", "c", "d"], &[]),
            ),
            tool(
                "image_process",
                "completed",
                batch(&["aa", "bb", "cc", "dd"], &["a", "b", "c", "d"]),
            ),
            tool("image_process", "completed", batch(&["aaa"], &["aa"])),
        ],
    );
    let result = image_result(&turns, &job).unwrap();
    assert_eq!(result["images"].as_array().unwrap().len(), 4);
    assert!(result["images"]
        .as_array()
        .unwrap()
        .iter()
        .all(|image| !["a", "b", "c", "d", "aa"].contains(&image["id"].as_str().unwrap())));
    assert_eq!(result["sourcePaths"].as_array().unwrap().len(), 4);
    assert_eq!(result["exports"].as_array().unwrap().len(), 4);
    assert_eq!(result["agentId"], job.id);
    assert_eq!(result["accountAlias"], "image-account");
    assert_eq!(result["processing"]["images"].as_array().unwrap().len(), 4);
    for (index, image) in result["images"].as_array().unwrap().iter().enumerate() {
        assert_eq!(
            result["processing"]["images"][index]["path"],
            format!("{}.png", image["id"].as_str().unwrap())
        );
    }
}

#[test]
fn a_failed_child_preserves_paid_originals_and_partial_publication_receipts() {
    let (_fixture, hub) = hub();
    let mut job = job(&hub, Role::ImageGenerator, ".jarvis-image-attachments");
    job.status = Status::Failed;
    job.error = Some("Local processing failed".into());
    let recovery = json!({"error":{"code":"storage_error","message":"Export failed"},"sourceImageIds":["paid-original"],"producedImageIds":["confirmed-local"],"sourcePaths":["attachments/confirmed-local/source"],"exports":["images/confirmed-local.png"],"recovery":"Retry only image_process; never repeat the paid request."});
    let turns = journal(
        &hub,
        vec![tool("generate_image", "error", recovery.clone())],
    );
    let error = image_result(&turns, &job).unwrap_err();
    let receipt: Value = serde_json::from_str(error.tool_result.as_deref().unwrap()).unwrap();
    for key in [
        "sourceImageIds",
        "producedImageIds",
        "sourcePaths",
        "exports",
        "recovery",
    ] {
        assert_eq!(receipt[key], recovery[key], "{key}");
    }
    assert_eq!(receipt["agentId"], job.id);
    assert_eq!(error.message, "Local processing failed");
}

#[test]
fn a_successful_intermediate_image_does_not_hide_failed_required_processing() {
    let (_fixture, hub) = hub();
    let mut job = job(&hub, Role::ImageGenerator, ".jarvis-image-attachments");
    // Even an incorrect completed handoff cannot erase the last native error.
    job.status = Status::Completed;
    let turns = journal(
        &hub,
        vec![
            tool("generate_image", "completed", batch(&["original"], &[])),
            tool(
                "image_process",
                "error",
                json!({"sourceImageIds":["original"],"recovery":"Retry only processing"}),
            ),
        ],
    );
    let error = image_result(&turns, &job).unwrap_err();
    let receipt: Value = serde_json::from_str(error.tool_result.as_deref().unwrap()).unwrap();
    assert_eq!(receipt["sourceImageIds"], json!(["original"]));
    assert_eq!(receipt["images"][0]["id"], "original");
    assert_eq!(receipt["confirmedExports"], json!(["images/original.png"]));
}

#[test]
fn successful_processing_recovery_returns_only_verified_final_attachments() {
    let (_fixture, hub) = hub();
    let mut job = job(&hub, Role::ImageGenerator, ".jarvis-image-attachments");
    job.status = Status::Completed;
    let turns = journal(
        &hub,
        vec![
            tool(
                "generate_image",
                "error",
                json!({"sourceImageIds":["original"],"recovery":"Retry only processing"}),
            ),
            tool(
                "image_process",
                "completed",
                batch(&["final"], &["original"]),
            ),
        ],
    );
    let result = image_result(&turns, &job).unwrap();
    assert_eq!(result["images"][0]["id"], "final");
    assert_eq!(result["images"].as_array().unwrap().len(), 1);
    assert!(result.get("recovery").is_none());
}

#[test]
fn the_parent_facade_preserves_the_native_request_schema() {
    let delegated = definition(false);
    let native = definition(true);
    assert_eq!(delegated["name"], "generate_image");
    assert_eq!(delegated["parameters"], native["parameters"]);
    assert!(delegated["description"]
        .as_str()
        .unwrap()
        .contains("managed Gerador de imagens"));
}

#[test]
fn specialist_tool_catalog_is_small_and_project_knowledge_is_retrieved_only_on_demand() {
    for global in [false, true] {
        let tools = specialist_tools(global, true, None, !global);
        assert!(tools.iter().any(|tool| tool["name"] == "generate_image"));
        assert!(tools.iter().any(|tool| tool["name"] == "image_process"));
        assert_eq!(
            tools
                .iter()
                .any(|tool| tool["name"] == crate::agent::knowledge::TOOL),
            !global
        );
        assert_eq!(
            tools.iter().any(|tool| tool["name"] == "update_tasks"),
            !global
        );
        assert!(tools.iter().all(|tool| !matches!(
            tool["name"].as_str(),
            Some("bash" | "read" | "mcp_activate" | "ctx_search" | "http_send")
        )));
        assert!(tools.len() <= 6);
    }
    assert!(specialist_tools(false, false, None, false)
        .iter()
        .all(|tool| tool["name"] != "generate_image"));
}

#[test]
fn facade_retry_resumes_the_same_paid_request_and_preserves_the_specialists_model() {
    let (_fixture, hub) = hub();
    let execution = Execution {
        hub: hub.clone(),
        id: "main".into(),
        role: Role::Builder,
        flow: Flow::Standard,
        scope: vec![".".into()],
    };
    let args = json!({"prompt":"Create a robot","processing":{"format":"webp"}});
    let (mut child, _) = dispatch::image_job(&execution, &args).unwrap();
    child.options.account = "specialist-account".into();
    child.options.model = "specialist-model".into();
    child.status = Status::Failed;
    hub.manifest
        .lock()
        .unwrap()
        .jobs
        .insert(child.id.clone(), child.clone());
    let (session, _) = storage::worker(&hub, &child, None).unwrap();
    let mut png = std::io::Cursor::new(Vec::new());
    ::image::DynamicImage::new_rgba8(1, 1)
        .write_to(&mut png, ::image::ImageFormat::Png)
        .unwrap();
    let original = crate::agent::attachments::store(
        &hub.env.home,
        &hub.root.id,
        "original.png",
        png.get_ref(),
    )
    .unwrap();
    session.update(true,|data| {
        let completed_call = tool("generate_image","error",json!({"sourceImageIds":[original.id],"producedImageIds":[],"exports":[],"recovery":"Retry image_process only"}));
        let turn = data.turns.last_mut().unwrap();
        turn.wire.push(json!({"type":"function_call_output","call_id":completed_call.id,"output":completed_call.output}));
        turn.turn.steps.push(Step { tools:vec![completed_call],..Default::default() });
    }).unwrap();
    finish(&session, Err(AgentError::internal()));
    let (resumed, continuation) = resume_confirmed_image(&execution, &args).unwrap().unwrap();
    assert_eq!(resumed.id, child.id);
    assert_eq!(resumed.attempts, 2);
    assert_eq!(resumed.status, Status::Queued);
    assert_eq!(resumed.options.model, "specialist-model");
    assert_eq!(resumed.options.account, "specialist-account");
    assert_eq!(hub.manifest.lock().unwrap().jobs.len(), 1);
    assert!(continuation.contains("Do NOT call generate_image again"));
    assert!(continuation.contains("Use image_process only"));
    assert!(continuation.contains(&original.id));
    assert!(continuation.contains("webp"));
    hub.manifest
        .lock()
        .unwrap()
        .jobs
        .get_mut(&child.id)
        .unwrap()
        .status = Status::Failed;
    hub.manifest.lock().unwrap().run_id = "a-new-user-request".into();
    assert!(resume_confirmed_image(&execution, &args).unwrap().is_none());
}

#[test]
fn recovery_never_claims_an_unconfirmed_or_another_conversations_original() {
    let (_fixture, hub) = hub();
    let execution = Execution {
        hub: hub.clone(),
        id: "main".into(),
        role: Role::Builder,
        flow: Flow::Standard,
        scope: vec![".".into()],
    };
    let args = json!({"prompt":"Create a robot"});
    let (mut child, _) = dispatch::image_job(&execution, &args).unwrap();
    child.status = Status::Failed;
    hub.manifest
        .lock()
        .unwrap()
        .jobs
        .insert(child.id.clone(), child.clone());
    let (session, _) = storage::worker(&hub, &child, None).unwrap();
    session
        .update(true, |data| {
            let failed = tool(
                "generate_image",
                "error",
                json!({"error":"Provider failed before any confirmed output"}),
            );
            let turn = data.turns.last_mut().unwrap();
            turn.wire.push(
                json!({"type":"function_call_output","call_id":failed.id,"output":failed.output}),
            );
            turn.turn.steps.push(Step {
                tools: vec![failed],
                ..Default::default()
            });
        })
        .unwrap();
    finish(&session, Err(AgentError::internal()));
    assert!(resume_confirmed_image(&execution, &args).unwrap().is_none());
    session
        .update(true, |data| {
            data.turns.last_mut().unwrap().turn.steps.push(Step {
                tools: vec![tool(
                    "image_process",
                    "error",
                    json!({"sourceImageIds":["f".repeat(32)],"recovery":"Unowned receipt"}),
                )],
                ..Default::default()
            });
        })
        .unwrap();
    assert!(resume_confirmed_image(&execution, &args).is_err());
    assert_eq!(
        hub.manifest.lock().unwrap().jobs[&child.id].status,
        Status::Failed
    );
}

#[test]
fn an_uncertain_paid_generation_is_not_repeated_in_the_same_request() {
    let (_fixture, hub) = hub();
    let execution = Execution {
        hub: hub.clone(),
        id: "main".into(),
        role: Role::Builder,
        flow: Flow::Standard,
        scope: vec![".".into()],
    };
    let args = json!({"prompt":"Create a robot"});
    let (mut child, _) = dispatch::image_job(&execution, &args).unwrap();
    child.status = Status::Interrupted;
    hub.manifest
        .lock()
        .unwrap()
        .jobs
        .insert(child.id.clone(), child.clone());
    let (session, _) = storage::worker(&hub, &child, None).unwrap();
    session
        .update(true, |data| {
            data.turns.last_mut().unwrap().turn.steps.push(Step {
                tools: vec![tool("generate_image", "running", json!({}))],
                ..Default::default()
            });
        })
        .unwrap();
    finish(
        &session,
        Err(AgentError::new("interrupted", "Provider outcome unknown")),
    );
    let error = resume_confirmed_image(&execution, &args).unwrap_err();
    assert_eq!(error.code, "image_outcome_unknown");
    let receipt: Value = serde_json::from_str(error.tool_result.as_deref().unwrap()).unwrap();
    assert_eq!(receipt["agentId"], child.id);
    assert_eq!(receipt["generationRepeated"], false);
    assert_eq!(hub.manifest.lock().unwrap().jobs.len(), 1);
    assert_eq!(hub.manifest.lock().unwrap().jobs[&child.id].attempts, 1);
    hub.manifest.lock().unwrap().run_id = "a-new-explicit-user-request".into();
    assert!(resume_confirmed_image(&execution, &args).unwrap().is_none());
}

#[tokio::test]
async fn a_cancelled_parent_does_not_schedule_an_image_request() {
    let (_fixture, hub) = hub();
    let execution = Execution {
        hub: hub.clone(),
        id: "main".into(),
        role: Role::Builder,
        flow: Flow::Standard,
        scope: vec![".".into()],
    };
    let (_, signal) = watch::channel(true);
    let error = execution
        .delegate_image(&hub.root, &json!({"prompt":"Create a robot"}), signal)
        .await
        .unwrap_err();
    assert_eq!(error.code, "cancelled");
    assert!(hub.manifest.lock().unwrap().jobs.is_empty());
}
