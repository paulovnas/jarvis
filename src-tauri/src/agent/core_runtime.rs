//! Host-owned Core work. Receipts are durable; auxiliary failures are observations.
use super::{lsp, AgentError, Session, ToolCall};
use crate::core::{
    activity::{Activity, Status},
    design::Prepared,
    ComponentId,
};
use serde_json::json;
use tokio::sync::watch;

pub(super) fn beads_activity() -> Activity {
    Activity::new(
        ComponentId::Beads,
        "workflow_snapshot",
        "Estado do plano recuperado automaticamente para orientar o fluxo",
    )
}

pub(super) fn record(session: &Session, activities: Vec<Activity>) -> Result<(), AgentError> {
    if activities.is_empty() {
        return Ok(());
    }
    session.update(true, |data| {
        if let Some(step) = data
            .turns
            .last_mut()
            .and_then(|turn| turn.turn.steps.last_mut())
        {
            step.core_activities.extend(activities);
        }
    })
}

/// Compare to history as well as this inference loop. References are refreshed
/// from local files before inference and explicitly replayed after compaction.
pub(super) fn prepare_design(
    session: &Session,
    mut prepared: Prepared,
    fingerprint: &mut Option<String>,
    replay: bool,
) -> Result<Option<Activity>, AgentError> {
    let unchanged = fingerprint.as_ref() == prepared.activity.fingerprint.as_ref();
    if unchanged && !replay {
        return Ok(None);
    }
    let was_used = unchanged
        || session
            .data
            .lock()
            .map_err(|_| AgentError::internal())?
            .turns
            .iter()
            .flat_map(|turn| &turn.turn.steps)
            .flat_map(|step| &step.core_activities)
            .any(|activity| {
                activity.component == ComponentId::OpenDesign
                    && activity.fingerprint == prepared.activity.fingerprint
            });
    if was_used {
        prepared.activity.status = Status::Reused;
        prepared.activity.summary =
            "Referências de design revalidadas e reutilizadas no contexto".into();
    }
    session.update(true, |data| {
        data.turns.last_mut().unwrap().wire.push(json!({
            "role":"user", "_jarvis_runtime":true, "_jarvis_core_design":true,
            "content": prepared.prompt,
        }));
    })?;
    fingerprint.clone_from(&prepared.activity.fingerprint);
    Ok(Some(prepared.activity))
}

pub(super) async fn diagnose(
    session: &Session,
    registry: &mut lsp::Registry,
    paths: &mut Vec<String>,
    signal: watch::Receiver<bool>,
) -> Result<bool, AgentError> {
    if paths.is_empty() {
        return Ok(false);
    }
    let report = registry
        .diagnostics_after_changes(&std::mem::take(paths), signal)
        .await?;
    let Some(report) = report else {
        return Ok(false);
    };
    record(session, report.activities)?;
    session.update(true, |data| {
        data.turns.last_mut().unwrap().wire.push(json!({
            "role":"user", "_jarvis_runtime":true,
            "content":report.observation,
        }));
    })?;
    Ok(report.new_errors)
}

/// A failed auxiliary capture never erases a completed action, invents an index,
/// or replaces a structured tool error. Cancellation is handled after persistence.
pub(super) fn captured_result(
    name: &str,
    output: &str,
    structured: Option<String>,
    captured: &Result<Option<String>, crate::core::CoreError>,
) -> (String, bool) {
    if let Some(error) = structured {
        return (error, false);
    }
    match captured {
        Ok(Some(compact)) => (compact.clone(), true),
        Ok(None) => (output.to_owned(), false),
        Err(_) => (crate::core::context::fallback_result(name, output), false),
    }
}

/// Checkpoint the accepted result before auxiliary subprocesses can fail, stall
/// or be cancelled. A bounded replay is sufficient until indexing completes.
pub(super) async fn checkpoint_tool(
    session: &Session,
    tool: &ToolCall,
    output: &str,
    status: &str,
    duration_ms: u64,
    structured: Option<&str>,
) -> Result<(), AgentError> {
    let replay = structured
        .map(str::to_owned)
        .unwrap_or_else(|| crate::core::context::fallback_result(&tool.name, output));
    session
        .update_async(|data| {
            let current = data.turns.last_mut().unwrap();
            if !current
                .wire
                .iter()
                .any(|item| item["type"] == "function_call_output" && item["call_id"] == tool.id)
            {
                current.wire.push(
                    json!({"type":"function_call_output", "call_id":tool.id, "output":replay}),
                );
            }
            if let Some(item) = current
                .turn
                .steps
                .last_mut()
                .and_then(|step| step.tools.iter_mut().find(|item| item.id == tool.id))
            {
                item.status = status.into();
                item.output = output.into();
                item.duration_ms = duration_ms;
            }
        })
        .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{
        journal,
        tests::{options, session, Fixture},
        ApprovalMode, Step,
    };

    #[tokio::test]
    async fn completed_action_and_original_output_survive_auxiliary_failure_and_reload() {
        let fixture = Fixture::new();
        let session = session(&fixture);
        session
            .reserve("Implementar".into(), options(ApprovalMode::Yolo))
            .unwrap();
        let tool = ToolCall {
            id: "write-1".into(),
            name: "write".into(),
            args: json!({"path":"file.ts"}),
            status: "running".into(),
            output: String::new(),
            duration_ms: 0,
        };
        session.update(true, |data| {
            let current = data.turns.last_mut().unwrap();
            current.turn.steps.push(Step { tools: vec![tool.clone()], ..Step::default() });
            current.wire.push(json!({"type":"function_call", "call_id":tool.id, "name":tool.name, "arguments":tool.args.to_string()}));
        }).unwrap();
        let output = format!("Arquivo salvo.\n{}", "á".repeat(12_000));
        checkpoint_tool(&session, &tool, &output, "completed", 12, None)
            .await
            .unwrap();
        let captured = Err(crate::core::error("Auxiliary subprocess failed"));
        let (replay, indexed) = captured_result("write", &output, None, &captured);
        assert!(!indexed);
        assert!(replay.len() < 8_000);
        assert!(replay.contains("NOT searchable"));
        record(
            &session,
            vec![Activity::unavailable(
                ComponentId::ContextMode,
                "result_indexing",
                "Índice indisponível",
            )],
        )
        .unwrap();
        session.flush().unwrap();

        let (mut loaded, _) = journal::read_only(&session.journal).unwrap();
        journal::interrupt_tools(&mut loaded[0]);
        let step = &loaded[0].turn.steps[0];
        assert_eq!(step.tools[0].status, "completed");
        assert_eq!(step.tools[0].output, output);
        assert_eq!(step.core_activities[0].status, Status::Unavailable);
        assert_eq!(
            loaded[0]
                .wire
                .iter()
                .filter(
                    |item| item["call_id"] == "write-1" && item["type"] == "function_call_output"
                )
                .count(),
            1
        );
        assert!(journal::uncertain_tool_names(&loaded[0]).is_empty());
    }

    #[test]
    fn structured_tool_errors_are_never_replaced_by_auxiliary_errors() {
        let structured =
            json!({"ok":false,"error":{"code":"mcp_invalid_arguments","path":"$.query"}})
                .to_string();
        let (replay, indexed) = captured_result(
            "mcp_docs",
            "original tool failure",
            Some(structured.clone()),
            &Err(crate::core::error("memory failed")),
        );
        assert_eq!(replay, structured);
        assert!(!indexed);
    }

    #[test]
    fn design_context_replays_after_compaction_and_only_changes_when_references_change() {
        let fixture = Fixture::new();
        let session = session(&fixture);
        session
            .reserve("Preserve my brand".into(), options(ApprovalMode::Yolo))
            .unwrap();
        session
            .update(true, |data| {
                data.turns
                    .last_mut()
                    .unwrap()
                    .turn
                    .steps
                    .push(Step::default())
            })
            .unwrap();
        let prepared = |digest: &str| {
            let mut activity = Activity::new(
                ComponentId::OpenDesign,
                "design_preparation",
                "Referências preparadas",
            );
            activity.fingerprint = Some(digest.into());
            Prepared {
                activity,
                prompt: format!("Untrusted design reference: {digest}"),
            }
        };
        let mut fingerprint = None;
        let first = prepare_design(&session, prepared("one"), &mut fingerprint, false)
            .unwrap()
            .unwrap();
        assert_eq!(first.status, Status::Applied);
        record(&session, vec![first]).unwrap();
        assert!(
            prepare_design(&session, prepared("one"), &mut fingerprint, false)
                .unwrap()
                .is_none()
        );
        let replayed = prepare_design(&session, prepared("one"), &mut fingerprint, true)
            .unwrap()
            .unwrap();
        assert_eq!(replayed.status, Status::Reused);
        let refreshed = prepare_design(&session, prepared("two"), &mut fingerprint, false)
            .unwrap()
            .unwrap();
        assert_eq!(refreshed.status, Status::Applied);
        let data = session.data.lock().unwrap();
        assert_eq!(data.turns[0].turn.user, "Preserve my brand");
        let references: Vec<_> = data.turns[0]
            .wire
            .iter()
            .filter(|item| item["_jarvis_core_design"] == true)
            .collect();
        assert_eq!(references.len(), 3);
        assert!(references
            .iter()
            .all(|item| item["_jarvis_runtime"] == true));
        assert!(references[2]["content"].as_str().unwrap().contains("two"));
    }
}
