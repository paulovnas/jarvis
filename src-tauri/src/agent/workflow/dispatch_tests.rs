use super::super::tests::{hub, job};
use super::*;

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
async fn cancellation_reaches_nested_workers_and_rework_limit_is_enforced() {
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
    assert!(retry(&exec, &parent.id, "Try after inspecting the checkpoint").is_err());
}
