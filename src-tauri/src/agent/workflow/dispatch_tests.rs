use super::super::tests::{hub, job};
use super::*;

#[test]
fn retry_keeps_the_same_worker_turn_and_confirmed_results() {
    let (_fixture, hub) = hub();
    let mut worker = job(&hub, Role::Builder, ".");
    let (session, _) = storage::worker(&hub, &worker, None).unwrap();
    let original = session.snapshot().unwrap().turns[0].id.clone();
    session.update(true, |data| {
        let turn = data.turns.last_mut().unwrap();
        turn.wire.push(json!({"type":"function_call_output","call_id":"confirmed","output":"Already applied"}));
        turn.turn.steps.push(Step { text: "Verified existing result".into(), ..Step::default() });
    }).unwrap();
    finish(&session, Err(AgentError::internal()));
    drop(session);
    worker.recovery = Some(RecoveryCheckpoint::new(vec![]));
    worker.options.model = "updated-model".into();
    hub.manifest
        .lock()
        .unwrap()
        .jobs
        .insert(worker.id.clone(), worker.clone());
    let (resumed, _) =
        storage::worker(&hub, &worker, Some("Finish the remaining test".into())).unwrap();
    let data = resumed.data.lock().unwrap();
    assert_eq!(data.turns.len(), 1);
    let turn = &data.turns[0];
    assert_eq!(turn.turn.id, original);
    assert_eq!(turn.turn.user, worker.prompt);
    assert_eq!(turn.turn.options.model, "updated-model");
    assert_eq!(
        turn.mcp_intent,
        Some(hub.manifest.lock().unwrap().mcp_intent.clone())
    );
    assert_eq!(turn.turn.steps[0].text, "Verified existing result");
    assert_eq!(
        turn.wire
            .iter()
            .filter(|item| item["call_id"] == "confirmed")
            .count(),
        1
    );
    assert!(turn.wire.iter().any(|item| item["_jarvis_runtime"] == true
        && item["content"]
            .as_str()
            .is_some_and(|s| s.contains("Finish the remaining test"))));
}

#[test]
fn dependency_failure_does_not_count_queued_hours_as_work() {
    let (_fixture, hub) = hub();
    let worker = job(&hub, Role::Designer, ".");
    hub.manifest
        .lock()
        .unwrap()
        .jobs
        .insert(worker.id.clone(), worker.clone());
    let (session, _) = storage::worker(&hub, &worker, None).unwrap();
    session
        .data
        .lock()
        .unwrap()
        .turns
        .last_mut()
        .unwrap()
        .turn
        .created_at = now().saturating_sub(9 * 3_600_000);
    let result = Err(invalid("Uma dependência não foi concluída com sucesso."));
    finish(&session, result.clone());
    let duration = session.snapshot().unwrap().turns[0].duration_ms;
    settle(&hub, &worker, &result, Some(duration)).unwrap();
    assert_eq!(duration, 0);
    assert_eq!(hub.job(&worker.id).unwrap().duration_ms, 0);
}

#[test]
fn coordinator_clock_runs_only_while_a_descendant_is_doing_work() {
    use crate::agent::turn_state::TurnPhase;
    let (_fixture, hub) = hub();
    let worker = job(&hub, Role::Builder, ".");
    hub.manifest
        .lock()
        .unwrap()
        .jobs
        .insert(worker.id.clone(), worker.clone());
    let (session, _) = storage::worker(&hub, &worker, None).unwrap();
    hub.live
        .lock()
        .unwrap()
        .insert(worker.id.clone(), session.clone());
    hub.root.transition(TurnPhase::WaitingForAgents).unwrap();
    hub.refresh_waiting_clocks();
    assert!(hub.root.snapshot().unwrap().turns[0].active_since.is_none());
    session.transition(TurnPhase::Sampling).unwrap();
    assert!(hub.root.snapshot().unwrap().turns[0].active_since.is_some());
    session.transition(TurnPhase::WaitingForApproval).unwrap();
    assert!(hub.root.snapshot().unwrap().turns[0].active_since.is_none());
    session.transition(TurnPhase::ExecutingTools).unwrap();
    assert!(hub.root.snapshot().unwrap().turns[0].active_since.is_some());
    finish(&session, Err(AgentError::cancelled()));
    assert!(hub.root.snapshot().unwrap().turns[0].active_since.is_none());
}

fn planned_flow_evaluation_case() -> Value {
    serde_json::from_str(include_str!(
        "../fixtures/evaluations/movarte-planned-flow.json"
    ))
    .unwrap()
}

fn spawn_roles(flow: Flow, role: Role) -> Vec<String> {
    definitions(flow, role)
        .into_iter()
        .find(|definition| definition["name"] == "hub_spawn")
        .unwrap()["parameters"]["properties"]["role"]["enum"]
        .as_array()
        .unwrap()
        .iter()
        .map(|value| value.as_str().unwrap().to_owned())
        .collect()
}

#[test]
fn harness_evaluation_spawn_schema_only_advertises_allowed_roles() {
    let case = planned_flow_evaluation_case();
    let expected: Vec<String> = case["expectations"]["allowedPlannerSpawnRoles"]
        .as_array()
        .unwrap()
        .iter()
        .map(|role| role.as_str().unwrap().to_owned())
        .collect();
    assert_eq!(spawn_roles(Flow::Planned, Role::Planner), expected);
    assert_eq!(
        spawn_roles(Flow::Complete, Role::Planner),
        vec!["investigator", "writer", "orchestrator"]
    );
    assert_eq!(
        spawn_roles(Flow::Complete, Role::Orchestrator),
        vec!["planner", "designer", "builder", "reviewer"]
    );
}

#[test]
fn workflow_check_rejects_missing_bun_scripts_and_lists_real_options() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(
        directory.path().join("package.json"),
        r#"{"scripts":{"test":"vitest","build":"vite build"}}"#,
    )
    .unwrap();
    let error = workflow_command(directory.path(), "bun_typecheck").unwrap_err();
    assert!(error.message.contains("typecheck"));
    assert!(error.message.contains("build"));
    assert!(error.message.contains("test"));
    assert_eq!(
        workflow_command(directory.path(), "bun_test").unwrap(),
        "bun run test"
    );
}

#[test]
fn workflow_check_uses_the_existing_type_check_script() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(
        directory.path().join("package.json"),
        r#"{"scripts":{"type-check":"tsc --noEmit"}}"#,
    )
    .unwrap();
    assert_eq!(
        workflow_command(directory.path(), "bun_typecheck").unwrap(),
        "bun run type-check"
    );
}

#[test]
fn designer_is_always_an_implementation_role_and_complete_routes_it_through_orchestrator() {
    assert!(validate_phase(
        Flow::Complete,
        Role::Planner,
        Role::Designer,
        Phase::Discovery
    )
    .is_err());
    assert!(validate_phase(
        Flow::Complete,
        Role::Planner,
        Role::Designer,
        Phase::Implementation
    )
    .is_err());
    assert!(validate_phase(
        Flow::Complete,
        Role::Orchestrator,
        Role::Designer,
        Phase::Implementation
    )
    .is_ok());
    assert!(validate_phase(
        Flow::Planned,
        Role::Planner,
        Role::Designer,
        Phase::Implementation
    )
    .is_ok());
    assert!(validate_phase(
        Flow::Planned,
        Role::Planner,
        Role::Builder,
        Phase::Discovery
    )
    .is_err());
    let phases = definitions(Flow::Planned, Role::Planner)
        .into_iter()
        .find(|definition| definition["name"] == "hub_spawn")
        .unwrap()["parameters"]["properties"]["phase"]["enum"]
        .as_array()
        .unwrap()
        .clone();
    assert_eq!(phases, vec![json!("implementation")]);
}

#[test]
fn dependent_work_waits_and_failed_dependencies_block_instead_of_running() {
    let (_fixture, hub) = hub();
    let mut dependency = job(&hub, Role::Builder, "src/a");
    dependency.status = Status::Running;
    let mut next = job(&hub, Role::Reviewer, ".");
    next.dependencies = vec![dependency.id.clone()];
    let mut state = hub.manifest.lock().unwrap();
    state.jobs.insert(dependency.id.clone(), dependency.clone());
    assert!(!admitted(&state, &next).unwrap());
    state.jobs.get_mut(&dependency.id).unwrap().status = Status::Failed;
    assert!(admitted(&state, &next).is_err());
    state.jobs.get_mut(&dependency.id).unwrap().status = Status::Completed;
    assert!(admitted(&state, &next).unwrap());
}

#[test]
fn independent_scopes_run_in_parallel_but_overlap_and_leaf_limit_queue() {
    let (_fixture, hub) = hub();
    let mut first = job(&hub, Role::Builder, "src/a");
    first.status = Status::Running;
    let same = job(&hub, Role::Designer, "src/a/nested");
    let independent = job(&hub, Role::Builder, "src/b");
    let mut state = hub.manifest.lock().unwrap();
    state.jobs.insert(first.id.clone(), first);
    assert!(!admitted(&state, &same).unwrap());
    assert!(admitted(&state, &independent).unwrap());
    for _ in 0..3 {
        let mut other = independent.clone();
        other.id = library::new_id().unwrap();
        other.role = Role::Investigator;
        other.status = Status::Running;
        state.jobs.insert(other.id.clone(), other);
    }
    assert!(!admitted(&state, &independent).unwrap());
    let mut coordinator = independent.clone();
    coordinator.role = Role::Orchestrator;
    assert!(admitted(&state, &coordinator).unwrap());
}

#[tokio::test]
async fn completion_wakes_parent_once_even_when_it_arrives_before_wait_registration() {
    let (_fixture, hub) = hub();
    let mut child = job(&hub, Role::Investigator, ".");
    child.status = Status::Running;
    child.handoff = Some(Handoff {
        verdict: Verdict::Completed,
        summary: "Evidence collected".into(),
        outcomes: vec!["Located handler".into()],
        evidence: vec!["src/handler.ts:14".into()],
        validation: vec![],
        limitations: vec![],
        task_ids: vec![],
    });
    hub.mutate(|state| {
        state.jobs.insert(child.id.clone(), child.clone());
        Ok(())
    })
    .unwrap();
    settle(&hub, &child, &Ok(()), Some(1200)).unwrap();
    let messages = tokio::time::timeout(
        Duration::from_secs(1),
        hub.wait("main", hub.root_signal.clone()),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(messages.len(), 1);
    assert!(messages[0].text.contains("Evidence collected"));
    assert!(hub
        .wait("main", hub.root_signal.clone())
        .await
        .unwrap()
        .is_empty());
}

#[tokio::test]
async fn waiting_is_cancellable_and_never_busy_polls() {
    let (_fixture, hub) = hub();
    let mut child = job(&hub, Role::Investigator, ".");
    child.status = Status::Running;
    hub.mutate(|state| {
        state.jobs.insert(child.id.clone(), child);
        Ok(())
    })
    .unwrap();
    let revision = hub.manifest.lock().unwrap().revision;
    let (cancel, signal) = watch::channel(false);
    let h = hub.clone();
    let wait = tokio::spawn(async move { h.wait("main", signal).await });
    tokio::task::yield_now().await;
    assert_eq!(hub.manifest.lock().unwrap().revision, revision);
    assert!(!wait.is_finished());
    cancel.send_replace(true);
    assert_eq!(wait.await.unwrap().unwrap_err().code, "cancelled");
}

#[test]
fn writer_scope_rejects_product_code_absolute_escapes_and_traversal() {
    let root = Path::new("/project");
    let scope = vec!["docs".into()];
    assert!(path_allowed(
        root,
        &json!({"path":"docs/PLAN-feature.md"}),
        &scope,
        Role::Writer
    ));
    for path in [
        "src/app.ts",
        "docs/README.md",
        "docs/../src/app.ts",
        "/etc/secret",
        "docs/PLAN-x.ts",
    ] {
        assert!(!path_allowed(
            root,
            &json!({"path":path}),
            &scope,
            Role::Writer
        ));
    }
}

#[test]
fn beads_dispatch_checks_real_status_and_blocking_dependencies() {
    assert!(validate_bead(
        &json!([{"status":"open","dependencies":[{"dependency_type":"blocks","status":"closed"}]}]),
        &[]
    )
    .is_ok());
    assert!(validate_bead(
        &json!([{"status":"open","dependencies":[{"dependency_type":"blocks","status":"open"}]}]),
        &[]
    )
    .is_err());
    for status in ["closed", "blocked", "deferred"] {
        assert!(validate_bead(&json!({"status":status}), &[]).is_err());
    }
    assert!(validate_bead(&json!([]), &[]).is_err());
}

fn completed_handoff(id: &str) -> Handoff {
    Handoff {
        verdict: Verdict::Completed,
        summary: "Implementation verified".into(),
        outcomes: vec!["Acceptance criteria met".into()],
        evidence: vec!["Focused regression passed".into()],
        validation: vec![],
        limitations: vec![],
        task_ids: vec![id.into()],
    }
}

#[test]
fn approved_closed_epic_can_finish_without_becoming_executable_again() {
    let (_fixture, hub) = hub();
    let mut coordinator = job(&hub, Role::Orchestrator, ".");
    coordinator.bead_id = Some("epic".into());
    let mut reviewer = job(&hub, Role::Reviewer, ".");
    reviewer.status = Status::Completed;
    reviewer.handoff = Some(Handoff {
        verdict: Verdict::Approved,
        ..completed_handoff("epic")
    });
    let mut state = hub.manifest.lock().unwrap();
    state.flow = Flow::Complete;
    state.options.manual_validation = false;
    state.jobs.insert(reviewer.id.clone(), reviewer.clone());
    let task = json!({"id":"epic","issue_type":"epic","status":"closed",
        "assignee":format!("jarvis-{}", state.conversation_id),"comments":[]});
    let handoff = completed_handoff("epic");
    assert!(validate_completion_bead(&state, &coordinator, &task, &handoff).is_ok());
    assert!(validate_bead(&task, &[]).is_err());
    for status in ["blocked", "deferred"] {
        let mut blocked = task.clone();
        blocked["status"] = json!(status);
        assert!(validate_completion_bead(&state, &coordinator, &blocked, &handoff).is_err());
    }
    let mut foreign = task.clone();
    foreign["assignee"] = json!("another-conversation");
    assert!(validate_completion_bead(&state, &coordinator, &foreign, &handoff).is_err());
    state.options.manual_validation = true;
    assert!(validate_completion_bead(&state, &coordinator, &task, &handoff).is_err());
    state.validation = Some(validation::Batch {
        id: "acceptance".into(),
        flow: Flow::Complete,
        run_id: state.run_id.clone(),
        epic_ids: vec!["epic".into()],
        submitted: true,
        stale: false,
        created_at: now(),
        items: vec![validation::Item {
            id: "criterion".into(),
            title: "Acceptance".into(),
            steps: vec!["Verify the outcome".into()],
            expected: "Works".into(),
            decision: validation::Decision::Approved,
            reason: None,
        }],
    });
    assert!(validate_completion_bead(&state, &coordinator, &task, &handoff).is_ok());
    state.validation.as_mut().unwrap().stale = true;
    assert!(validate_completion_bead(&state, &coordinator, &task, &handoff).is_err());
    state.options.manual_validation = false;
    state.jobs.remove(&reviewer.id);
    assert!(validate_completion_bead(&state, &coordinator, &task, &handoff).is_err());
}

#[tokio::test]
async fn completion_retries_preserve_the_accepted_handoff() {
    let (_fixture, hub) = hub();
    let child = job(&hub, Role::Investigator, ".");
    hub.mutate(|state| {
        state.jobs.insert(child.id.clone(), child.clone());
        Ok(())
    })
    .unwrap();
    let exec = Execution {
        hub: hub.clone(),
        id: child.id.clone(),
        role: child.role,
        flow: Flow::Complete,
        scope: child.scope.clone(),
    };
    let handoff = completed_handoff("evidence");
    let (_, signal) = watch::channel(false);
    complete(&exec, handoff.clone(), signal.clone())
        .await
        .unwrap();
    complete(&exec, handoff.clone(), signal.clone())
        .await
        .unwrap();
    let mut replacement = handoff.clone();
    replacement.summary = "Different outcome".into();
    assert!(complete(&exec, replacement, signal).await.is_err());
    assert_eq!(hub.job(&child.id).unwrap().handoff, Some(handoff));
}

#[test]
fn verified_progress_allows_recovery_but_a_failed_child_does_not() {
    let (_fixture, hub) = hub();
    let mut parent = job(&hub, Role::Orchestrator, ".");
    parent.status = Status::Failed;
    parent.recovery_attempts = 2;
    let mut child = job(&hub, Role::Builder, "src");
    child.parent_id = parent.id.clone();
    child.handoff = Some(completed_handoff("task"));
    hub.mutate(|state| {
        state.jobs.insert(parent.id.clone(), parent.clone());
        state.jobs.insert(child.id.clone(), child.clone());
        Ok(())
    })
    .unwrap();
    settle(&hub, &child, &Err(AgentError::internal()), None).unwrap();
    assert_eq!(hub.job(&parent.id).unwrap().recovery_attempts, 2);
    settle(&hub, &child, &Ok(()), None).unwrap();
    let mut state = hub.manifest.lock().unwrap();
    let recovered = prepare_retry(
        &mut state,
        "main",
        Flow::Complete,
        Role::Planner,
        &parent.id,
        None,
    )
    .unwrap();
    assert_eq!(recovered.recovery_attempts, 1);
    drop(state);
    let exec = Execution {
        hub: hub.clone(),
        id: parent.id.clone(),
        role: parent.role,
        flow: Flow::Complete,
        scope: parent.scope,
    };
    exec.record_confirmed_progress().unwrap();
    assert_eq!(hub.job(&parent.id).unwrap().recovery_attempts, 0);
    hub.mutate(|state| {
        let parent = state.jobs.get_mut(&parent.id).unwrap();
        parent.run_id = "next-run".into();
        parent.recovery_attempts = 2;
        Ok(())
    })
    .unwrap();
    settle(&hub, &child, &Ok(()), None).unwrap();
    assert_eq!(hub.job(&parent.id).unwrap().recovery_attempts, 2);
}

#[test]
fn harness_evaluation_resumed_repair_gets_the_entire_finding_contract() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../fixtures/evaluations/movarte-resumed-review.json"
    ))
    .unwrap();
    let handoff: Handoff = serde_json::from_value(fixture["handoff"].clone()).unwrap();
    assert!(handoff.summary.len() > 300);
    for role in [Role::Builder, Role::Designer, Role::Custom] {
        let (_fixture, hub) = hub();
        let mut reviewer = job(
            &hub,
            if role == Role::Custom {
                Role::Custom
            } else {
                Role::Reviewer
            },
            "src/integration",
        );
        reviewer.status = Status::Blocked;
        reviewer.handoff = Some(handoff.clone());
        let mut worker = job(&hub, role, "src/integration");
        worker.dependencies = vec![reviewer.id.clone()];
        let unrelated = job(&hub, role, "unrelated");
        hub.mutate(|state| {
            for job in [&worker, &reviewer, &unrelated] {
                state.jobs.insert(job.id.clone(), job.clone());
            }
            Ok(())
        })
        .unwrap();
        let exec = Execution {
            hub: hub.clone(),
            id: worker.id,
            role,
            flow: Flow::Complete,
            scope: worker.scope,
        };
        let context = exec.context().unwrap();
        assert!(
            context.contains(&json!(handoff).to_string()),
            "repair contract was truncated"
        );
        let other = Execution {
            id: unrelated.id,
            scope: unrelated.scope,
            ..exec
        };
        assert!(!other.context().unwrap().contains("reviewFindings"));
        assert!(
            !technically_approved(&hub.manifest.lock().unwrap(), "integration-report"),
            "a repair contract is not independent approval"
        );
    }
}

#[test]
fn bead_checkpoint_tracks_requirements_and_comments_without_progress_noise() {
    let source = json!([{
        "id":"jproject-task",
        "title":"Implement feature",
        "description":"Keep the existing contract",
        "status":"in_progress",
        "assignee":"jarvis-conversation",
        "notes":"Started",
        "updated_at":"2026-09-11T10:00:00Z",
        "dependencies":[],
        "comments":[{"id":1,"author":"Você","text":"Preserve the API"}]
    }]);
    let initial = bead_checkpoint(&source).unwrap();
    let mut progress = source.clone();
    progress[0]["notes"] = json!("Validation passed");
    progress[0]["updated_at"] = json!("2026-09-11T11:00:00Z");
    assert_eq!(
        initial.fingerprint,
        bead_checkpoint(&progress).unwrap().fingerprint
    );

    let mut commented = progress.clone();
    commented[0]["comments"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":2,"author":"Você","text":"Also cover the empty state"}));
    let refreshed = bead_checkpoint(&commented).unwrap();
    assert_ne!(initial.fingerprint, refreshed.fingerprint);
    let error = bead_changed_error(&refreshed);
    assert_eq!(error.code, "beads_changed");
    assert!(error
        .tool_result
        .unwrap()
        .contains("Also cover the empty state"));

    let mut changed_requirement = source;
    changed_requirement[0]["description"] = json!("Keep the API and add pagination");
    assert_ne!(
        initial.fingerprint,
        bead_checkpoint(&changed_requirement).unwrap().fingerprint
    );
}

#[test]
fn assigned_bead_comments_are_injected_before_worker_execution() {
    let (_fixture, hub) = hub();
    let child = job(&hub, Role::Builder, "src");
    let (session, _) = storage::worker(&hub, &child, None).unwrap();
    let checkpoint = bead_checkpoint(&json!([{
        "id":"jproject-task",
        "title":"Implement feature",
        "status":"open",
        "comments":[{"id":3,"author":"Você","text":"Use the shared component"}]
    }]))
    .unwrap();
    inject_bead_checkpoint(&session, "jproject-task", &checkpoint).unwrap();
    let input = session.input().unwrap();
    let content = input
        .iter()
        .filter_map(|item| item["content"].as_str())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(content.contains("Assigned Beads task snapshot"));
    assert!(content.contains("Use the shared component"));
    assert!(content.contains("re-read this task and its comments"));
}

#[test]
fn review_admits_completed_implementation_without_removing_its_beads_dependency() {
    let (_fixture, hub) = hub();
    let mut builder = job(&hub, Role::Builder, "src");
    builder.bead_id = Some("implementation".into());
    builder.status = Status::Completed;
    builder.handoff = Some(Handoff {
        verdict: Verdict::Completed,
        summary: "Implemented".into(),
        outcomes: vec!["Works".into()],
        evidence: vec![],
        validation: vec![],
        limitations: vec![],
        task_ids: vec!["implementation".into()],
    });
    let mut reviewer = job(&hub, Role::Reviewer, ".");
    reviewer.dependencies = vec![builder.id.clone()];
    let mut state = hub.manifest.lock().unwrap();
    state.jobs.insert(builder.id.clone(), builder.clone());
    let task = json!({"status":"open","dependencies":[{"id":"implementation","dependency_type":"blocks","status":"in_progress"}]});
    assert!(validate_bead(&task, &review_dependencies(&state, &reviewer)).is_ok());
    assert!(validate_bead(&task, &review_dependencies(&state, &builder)).is_err());
    for role in [Role::Builder, Role::Designer] {
        let mut dependent = reviewer.clone();
        dependent.role = role;
        dependent.dependencies = vec![builder.id.clone()];
        assert!(validate_bead(&task, &review_dependencies(&state, &dependent)).is_ok());
    }
    for status in [Status::Running, Status::Blocked, Status::Failed] {
        state.jobs.get_mut(&builder.id).unwrap().status = status;
        assert!(validate_bead(&task, &review_dependencies(&state, &reviewer)).is_err());
    }
    state.jobs.get_mut(&builder.id).unwrap().status = Status::Completed;
    state.jobs.get_mut(&builder.id).unwrap().run_id = "previous".into();
    assert!(validate_bead(&task, &review_dependencies(&state, &reviewer)).is_err());
    let blocked = json!({"status":"open","dependencies":[{"id":"implementation","dependency_type":"blocks","status":"blocked"}]});
    assert!(validate_bead(&blocked, &["implementation".into()]).is_err());
}

#[test]
fn repair_can_depend_on_a_review_with_findings_but_not_a_failed_review() {
    let (_fixture, hub) = hub();
    let mut review = job(&hub, Role::Reviewer, "backend");
    review.status = Status::Blocked;
    review.handoff = Some(Handoff {
        verdict: Verdict::Rework,
        summary: "Fix the missing validation".into(),
        outcomes: vec![],
        evidence: vec![],
        validation: vec![],
        limitations: vec![],
        task_ids: vec![],
    });
    let mut repair = job(&hub, Role::Builder, "backend");
    repair.dependencies = vec![review.id.clone()];
    let mut state = hub.manifest.lock().unwrap();
    state.jobs.insert(review.id.clone(), review.clone());
    assert!(admitted(&state, &repair).unwrap());
    repair.role = Role::Reviewer;
    assert!(admitted(&state, &repair).is_err());
    repair.role = Role::Builder;
    state
        .jobs
        .get_mut(&review.id)
        .unwrap()
        .handoff
        .as_mut()
        .unwrap()
        .verdict = Verdict::Blocked;
    assert!(admitted(&state, &repair).is_err());
    state.jobs.get_mut(&review.id).unwrap().status = Status::Failed;
    assert!(admitted(&state, &repair).is_err());
}

#[test]
fn reviewers_and_overlapping_writers_never_run_together() {
    let (_fixture, hub) = hub();
    let mut builder = job(&hub, Role::Builder, "src");
    builder.status = Status::Running;
    let mut reviewer = job(&hub, Role::Reviewer, ".");
    let mut state = hub.manifest.lock().unwrap();
    state.jobs.insert(builder.id.clone(), builder.clone());
    assert!(!admitted(&state, &reviewer).unwrap());
    state.jobs.clear();
    reviewer.status = Status::Running;
    state.jobs.insert(reviewer.id.clone(), reviewer);
    assert!(!admitted(&state, &builder).unwrap());
}

#[tokio::test]
async fn failed_coordinator_waits_for_children_to_settle_before_retry_can_start() {
    let (_fixture, hub) = hub();
    let mut child = job(&hub, Role::Builder, "src");
    child.status = Status::Running;
    hub.mutate(|state| {
        state.jobs.insert(child.id.clone(), child.clone());
        Ok(())
    })
    .unwrap();
    let task_hub = hub.clone();
    let waiting = tokio::spawn(async move { await_children_settled(&task_hub, "main").await });
    tokio::task::yield_now().await;
    assert!(!waiting.is_finished());
    settle(&hub, &child, &Err(AgentError::cancelled()), Some(800)).unwrap();
    tokio::time::timeout(Duration::from_secs(1), waiting)
        .await
        .unwrap()
        .unwrap();
}

#[tokio::test]
async fn cancellation_reaches_nested_workers_and_failure_recovery_limit_is_enforced() {
    let (_fixture, hub) = hub();
    let mut parent = job(&hub, Role::Orchestrator, ".");
    parent.status = Status::Running;
    let mut child = job(&hub, Role::Builder, "src");
    child.parent_id = parent.id.clone();
    let (session, signal) = storage::worker(&hub, &child, None).unwrap();
    hub.live.lock().unwrap().insert(child.id.clone(), session);
    hub.mutate(|state| {
        state.jobs.insert(parent.id.clone(), parent.clone());
        state.jobs.insert(child.id.clone(), child.clone());
        Ok(())
    })
    .unwrap();
    cancel_tree(&hub, &parent.id).unwrap();
    assert!(*signal.borrow());
    hub.mutate(|state| {
        let job = state.jobs.get_mut(&parent.id).unwrap();
        job.status = Status::Failed;
        job.attempts = 3;
        job.recovery_attempts = 2;
        Ok(())
    })
    .unwrap();
    let exec = Execution {
        hub,
        id: "main".into(),
        role: Role::Planner,
        flow: Flow::Complete,
        scope: vec![".".into()],
    };
    assert!(retry(
        &exec,
        &parent.id,
        "Try after inspecting the checkpoint",
        None
    )
    .is_err());
    let mut state = exec.hub.manifest.lock().unwrap();
    state.run_id = "new-user-turn-after-provider-repair".into();
    let recovered = prepare_retry(
        &mut state,
        "main",
        Flow::Complete,
        Role::Planner,
        &parent.id,
        None,
    )
    .unwrap();
    assert_eq!(recovered.recovery_attempts, 1);
    assert_eq!(recovered.id, parent.id);
}

fn dispatch_for(job: &Job) -> Dispatch {
    Dispatch {
        role: job.role,
        phase: job.phase,
        title: job.title.clone(),
        prompt: job.prompt.clone(),
        acceptance: job.acceptance.clone(),
        scope: job.scope.clone(),
        bead_id: job.bead_id.clone(),
        dependencies: job.dependencies.clone(),
    }
}

#[test]
fn harness_evaluation_duplicate_active_task_returns_its_worker_without_discarding_new_instructions_silently(
) {
    let (_fixture, hub) = hub();
    let mut worker = job(&hub, Role::Builder, "backend");
    worker.bead_id = Some("task-api".into());
    worker.status = Status::Running;
    hub.mutate(|state| {
        state.jobs.insert(worker.id.clone(), worker.clone());
        Ok(())
    })
    .unwrap();
    let exec = Execution {
        hub: hub.clone(),
        id: "main".into(),
        role: Role::Planner,
        flow: Flow::Planned,
        scope: vec![".".into()],
    };
    let mut input = dispatch_for(&worker);
    input.prompt = "Also test the empty request".into();
    input.acceptance.push("Reject empty requests".into());
    let result: Value = serde_json::from_str(&spawn(&exec, input).unwrap()).unwrap();
    assert_eq!(result["existingAgentId"], worker.id);
    assert_eq!(result["instructionScheduled"], false);
    assert!(result["nextAction"].as_str().unwrap().contains("hub_send"));
    let state = hub.manifest.lock().unwrap();
    assert_eq!(state.jobs.len(), 1);
    assert!(state.messages.is_empty());
    assert_eq!(state.jobs[&worker.id].prompt, worker.prompt);
    assert_eq!(state.jobs[&worker.id].acceptance, worker.acceptance);
    assert!(hub.live.lock().unwrap().is_empty());
}

#[test]
fn duplicate_detection_preserves_independent_tasks_roles_scopes_and_runs() {
    let (_fixture, hub) = hub();
    let mut worker = job(&hub, Role::Builder, "backend");
    worker.bead_id = Some("task-api".into());
    let mut state = hub.manifest.lock().unwrap();
    state.jobs.insert(worker.id.clone(), worker.clone());
    let mut input = dispatch_for(&worker);
    assert!(active_duplicate(&state, "main", &input).is_some());
    input.scope = vec!["frontend".into()];
    assert!(active_duplicate(&state, "main", &input).is_none());
    input = dispatch_for(&worker);
    input.bead_id = Some("other-task".into());
    assert!(active_duplicate(&state, "main", &input).is_none());
    input = dispatch_for(&worker);
    input.role = Role::Reviewer;
    assert!(active_duplicate(&state, "main", &input).is_none());
    input = dispatch_for(&worker);
    assert!(active_duplicate(&state, "other-parent", &input).is_none());
    state.jobs.get_mut(&worker.id).unwrap().run_id = "old-run".into();
    assert!(active_duplicate(&state, "main", &input).is_none());
    let stored = state.jobs.get_mut(&worker.id).unwrap();
    stored.run_id = "run".into();
    stored.status = Status::Completed;
    assert!(active_duplicate(&state, "main", &input).is_none());
    let stored = state.jobs.get_mut(&worker.id).unwrap();
    stored.status = Status::Running;
    stored.bead_id = None;
    stored.role = Role::Investigator;
    input = dispatch_for(stored);
    input.prompt = "Investigate a different part of the feature".into();
    assert!(active_duplicate(&state, "main", &input).is_none());
}

#[test]
fn harness_evaluation_successful_followups_keep_worker_context_and_models_beyond_two_rounds() {
    let (_fixture, hub) = hub();
    let mut worker = job(&hub, Role::Builder, "backend");
    worker.status = Status::Completed;
    worker.options.account = "worker-account".into();
    worker.options.model = "worker-model".into();
    worker.recovery_attempts = 2;
    let mut state = hub.manifest.lock().unwrap();
    state.jobs.insert(worker.id.clone(), worker.clone());
    for round in 2..=9 {
        let continued = prepare_retry(
            &mut state,
            "main",
            Flow::Planned,
            Role::Planner,
            &worker.id,
            None,
        )
        .unwrap();
        assert_eq!(continued.id, worker.id);
        assert_eq!(continued.attempts, round);
        assert_eq!(continued.recovery_attempts, 0);
        assert_eq!(continued.options.account, "worker-account");
        assert_eq!(continued.options.model, "worker-model");
        state.jobs.get_mut(&worker.id).unwrap().status = Status::Completed;
    }
    assert_eq!(state.jobs.len(), 1);
}

#[test]
fn harness_evaluation_rework_reuses_builder_and_reviewer_with_completed_prior_round_dependencies() {
    let (_fixture, hub) = hub();
    let mut builder = job(&hub, Role::Builder, "backend");
    builder.status = Status::Completed;
    let mut reviewer = job(&hub, Role::Reviewer, "backend");
    reviewer.status = Status::Blocked;
    reviewer.recovery_attempts = 2;
    reviewer.handoff = Some(Handoff {
        verdict: Verdict::Rework,
        summary: "Add the missing validation".into(),
        outcomes: vec![],
        evidence: vec!["backend/handler.ts".into()],
        validation: vec![],
        limitations: vec![],
        task_ids: vec![],
    });
    reviewer.dependencies = vec![builder.id.clone()];
    let mut state = hub.manifest.lock().unwrap();
    state.jobs.insert(builder.id.clone(), builder.clone());
    state.jobs.insert(reviewer.id.clone(), reviewer.clone());
    let repairing = prepare_retry(
        &mut state,
        "main",
        Flow::Complete,
        Role::Orchestrator,
        &builder.id,
        Some(vec![reviewer.id.clone()]),
    )
    .unwrap();
    assert!(admitted(&state, &repairing).unwrap());
    state.jobs.get_mut(&builder.id).unwrap().status = Status::Completed;
    let rechecking = prepare_retry(
        &mut state,
        "main",
        Flow::Complete,
        Role::Orchestrator,
        &reviewer.id,
        Some(vec![builder.id.clone()]),
    )
    .unwrap();
    assert_eq!(rechecking.recovery_attempts, 0);
    assert!(admitted(&state, &rechecking).unwrap());
    assert_eq!(state.jobs.len(), 2);
}

#[test]
fn invalid_retry_dependencies_leave_the_checkpoint_unchanged() {
    let (_fixture, hub) = hub();
    let mut builder = job(&hub, Role::Builder, "backend");
    builder.status = Status::Completed;
    let mut waiting = job(&hub, Role::Reviewer, "backend");
    waiting.dependencies = vec![builder.id.clone()];
    let mut stranger = waiting.clone();
    stranger.id = "stranger".into();
    stranger.parent_id = "another-parent".into();
    let mut state = hub.manifest.lock().unwrap();
    state.jobs.insert(builder.id.clone(), builder.clone());
    state.jobs.insert(waiting.id.clone(), waiting.clone());
    state.jobs.insert(stranger.id.clone(), stranger.clone());
    let before = serde_json::to_value(&*state).unwrap();
    for dependency in [
        builder.id.as_str(),
        waiting.id.as_str(),
        stranger.id.as_str(),
        "missing",
    ] {
        assert!(prepare_retry(
            &mut state,
            "main",
            Flow::Planned,
            Role::Planner,
            &builder.id,
            Some(vec![dependency.to_owned()])
        )
        .is_err());
        assert_eq!(serde_json::to_value(&*state).unwrap(), before);
    }
}

#[tokio::test]
async fn failure_retry_preserves_uncertain_effects_until_a_successful_inspection() {
    let (_fixture, hub) = hub();
    let mut worker = job(&hub, Role::Builder, "backend");
    worker.status = Status::Interrupted;
    worker.recovery = Some(RecoveryCheckpoint::new(
        vec!["apply_patch:uncertain".into()],
    ));
    worker.recovery.as_mut().unwrap().inspected = true;
    hub.mutate(|state| {
        state.jobs.insert(worker.id.clone(), worker.clone());
        prepare_retry(
            state,
            "main",
            Flow::Planned,
            Role::Planner,
            &worker.id,
            None,
        )
    })
    .unwrap();
    let exec = Execution {
        hub: hub.clone(),
        id: worker.id.clone(),
        role: Role::Builder,
        flow: Flow::Planned,
        scope: worker.scope.clone(),
    };
    assert!(exec.context().unwrap().contains("apply_patch:uncertain"));
    let mutation = ToolCall {
        id: "write".into(),
        name: "write".into(),
        args: json!({"path":"backend/task.ts","content":"changed"}),
        status: "pending".into(),
        output: String::new(),
        duration_ms: 0,
    };
    assert!(exec
        .mutation_guard(&mutation, false, hub.root_signal.clone())
        .await
        .is_err());
    let read = ToolCall {
        name: "read".into(),
        ..mutation.clone()
    };
    exec.observe_recovery_inspection(&read, false, false)
        .unwrap();
    assert!(exec
        .mutation_guard(&mutation, false, hub.root_signal.clone())
        .await
        .is_err());
    exec.observe_recovery_inspection(&read, false, true)
        .unwrap();
    assert!(exec
        .mutation_guard(&mutation, false, hub.root_signal.clone())
        .await
        .is_ok());
    hub.mutate(|state| {
        state.jobs.get_mut(&worker.id).unwrap().status = Status::Completed;
        let followup = prepare_retry(
            state,
            "main",
            Flow::Planned,
            Role::Planner,
            &worker.id,
            None,
        )?;
        assert!(followup.recovery.is_none());
        Ok(())
    })
    .unwrap();
}

#[test]
fn narrow_write_scope_allows_project_reads_but_rejects_out_of_scope_mutations() {
    let (_fixture, hub) = hub();
    let exec = Execution {
        hub,
        id: "worker".into(),
        role: Role::Builder,
        flow: Flow::Planned,
        scope: vec!["frontend".into()],
    };
    let call = ToolCall {
        id: "read-contract".into(),
        name: "read".into(),
        args: json!({"path":"backend/contracts.ts"}),
        status: "pending".into(),
        output: String::new(),
        duration_ms: 0,
    };
    assert!(exec.preflight(&call).is_none());
    assert!(exec
        .preflight(&ToolCall {
            name: "write".into(),
            ..call
        })
        .is_some());
}
