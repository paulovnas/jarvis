use super::*;
use crate::agent::tests::Fixture;

pub(super) fn hub() -> (Fixture, Arc<Hub>) {
    let fixture = Fixture::new();
    let mut root = Arc::try_unwrap(crate::agent::tests::session(&fixture)).ok().unwrap();
    root.id = library::new_id().unwrap();
    let root = Arc::new(root);
    let options = TurnOptions { account: "root-account".into(), model: "root-model".into(), reasoning: Some("high".into()), mode: Mode::Build, workflow: Some(Flow::Complete), approval_mode: ApprovalMode::Manual };
    let signal = root.reserve("Implement the requested outcome".into(), options.clone()).unwrap();
    let directory = fixture.root.join("workflow"); std::fs::create_dir(&directory).unwrap();
    let manifest = Manifest { validation: None, version: 1, conversation_id: root.id.clone(), run_id: "run".into(), flow: Flow::Complete, root_status: Status::Running, updated_at: now(), revision: 1, options, profiles: BTreeMap::new(), jobs: BTreeMap::new(), messages: vec![], design_briefs: BTreeMap::new(), guidance: BTreeMap::new() };
    storage::save(&directory, &manifest).unwrap();
    let (changed, _) = watch::channel(1);
    let hub = Arc::new(Hub { root, env: Environment { processes: processes::ProcessState::default(), process_changed: Arc::new(|_| {}), state: AppState::default(), oauth: OpenAiCodexState::default(), mcp: crate::mcp::McpState::default(), home: fixture.root.clone() }, directory, manifest: Mutex::new(manifest), live: Mutex::new(HashMap::new()), changed, emit: Arc::new(|_| {}), check_lock: AsyncRwLock::new(()), root_signal: signal });
    (fixture, hub)
}
pub(super) fn job(hub: &Hub, role: Role, scope: &str) -> Job {
    Job { phase: Phase::Implementation, id: library::new_id().unwrap(), parent_id: "main".into(), run_id: "run".into(), role, title: "Assigned task".into(), prompt: "Inspect and implement only the assigned behavior".into(), acceptance: vec!["Observable outcome".into()], scope: vec![scope.into()], bead_id: None, dependencies: vec![], status: Status::Queued, created_at: now(), updated_at: now(), attempts: 1, handoff: None, error: None, options: hub.manifest.lock().unwrap().options.clone() }
}

#[test]
fn fixed_topology_has_no_standard_delegation_and_no_worker_escape() {
    let roles = [Role::Planner, Role::Investigator, Role::Writer, Role::Orchestrator, Role::Designer, Role::Builder, Role::Reviewer];
    for role in roles { assert!(!Role::Builder.spawns(Flow::Standard, role)); assert!(!Role::Reviewer.spawns(Flow::Complete, role)); }
    assert!(Role::Planner.spawns(Flow::Planned, Role::Builder));
    assert!(!Role::Planner.spawns(Flow::Planned, Role::Reviewer));
    assert!(Role::Planner.spawns(Flow::Complete, Role::Investigator));
    assert!(!Role::Planner.spawns(Flow::Complete, Role::Builder));
    assert!(Role::Orchestrator.spawns(Flow::Complete, Role::Reviewer));
}

#[test]
fn direct_designer_has_questions_and_design_tools_but_no_delegation() {
    let (_fixture, hub) = hub();
    let direct = Execution { hub, id:"main".into(), role:Role::Designer, flow:Flow::Designer, scope:vec![".".into()] };
    let mut tools = tools::definitions(Mode::Build); tools.extend(crate::core::design::definitions()); direct.filter(&mut tools);
    for name in ["ask_user", "write", "design_brief", "design_search", "design_read"] { assert!(tools.iter().any(|tool| tool["name"] == name), "missing {name}"); }
    assert!(tools.iter().all(|tool| !tool["name"].as_str().unwrap().starts_with("hub_")));
    for role in [Role::Planner, Role::Builder, Role::Designer] { assert!(!Role::Designer.spawns(Flow::Designer, role)); }
    assert_eq!(Flow::Designer.root(), Role::Designer);
    assert!(settings::validate(Flow::Designer, &BTreeMap::new()).is_ok());
}

#[test]
fn delegated_design_discovery_enforces_read_only_and_parent_questions_even_with_broad_scope() {
    let (_fixture, hub) = hub(); let mut child = job(&hub, Role::Designer, "."); child.phase = Phase::Discovery;
    hub.mutate(|state| { state.jobs.insert(child.id.clone(), child.clone()); Ok(()) }).unwrap();
    let exec = Execution { hub, id:child.id, role:Role::Designer, flow:Flow::Complete, scope:vec![".".into()] };
    assert_eq!(exec.role_mode(), Mode::Plan);
    let mut definitions = tools::definitions(Mode::Build); definitions.extend(crate::core::design::definitions()); exec.filter(&mut definitions);
    for name in ["ask_user", "write", "edit", "bash", "workflow_check", "beads_claim", "beads_update", "ctx_execute"] {
        assert!(!definitions.iter().any(|d| d["name"] == name));
        assert!(exec.preflight(&ToolCall { name:name.into(), id:"call".into(), args:json!({}), status:"pending".into(), output:String::new(), duration_ms:0 }).is_some());
    }
    for name in ["design_search", "design_read", "design_brief", "hub_request_guidance", "hub_complete"] { assert!(definitions.iter().any(|d| d["name"] == name), "missing {name}"); }
}

#[tokio::test]
async fn design_briefs_survive_reload_and_compaction_without_crossing_agent_boundaries() {
    let (_fixture, hub) = hub(); let child = job(&hub, Role::Designer, ".");
    hub.mutate(|state| { state.jobs.insert(child.id.clone(), child.clone()); Ok(()) }).unwrap();
    let direct = Execution { hub:hub.clone(), id:"main".into(), role:Role::Designer, flow:Flow::Designer, scope:vec![".".into()] };
    let worker = Execution { hub:hub.clone(), id:child.id.clone(), role:Role::Designer, flow:Flow::Complete, scope:vec![".".into()] };
    for (exec, brief) in [(&direct,"Accepted: graphite, compact dashboard"), (&worker,"Assigned: blue buttons only")] {
        exec.execute(&ToolCall { id:"brief".into(), name:"design_brief".into(), args:json!({"text":brief}), status:"pending".into(), output:String::new(), duration_ms:0 }, hub.root_signal.clone()).await.unwrap();
        assert!(exec.instructions().unwrap().contains(brief));
    }
    assert!(!worker.instructions().unwrap().contains("Accepted: graphite"));
    let loaded = storage::load(&hub.directory, &hub.root.id).unwrap().unwrap();
    assert_eq!(loaded.design_briefs["main"], "Accepted: graphite, compact dashboard");
    assert_eq!(loaded.design_briefs[&child.id], "Assigned: blue buttons only");
    // Legacy manifests did not have design state or phases.
    let mut legacy = serde_json::to_value(&loaded).unwrap(); legacy.as_object_mut().unwrap().remove("designBriefs"); legacy.as_object_mut().unwrap().remove("guidance");
    legacy["jobs"][&child.id].as_object_mut().unwrap().remove("phase");
    let decoded: Manifest = serde_json::from_value(legacy).unwrap();
    assert_eq!(decoded.jobs[&child.id].phase, Phase::Implementation);
    assert!(decoded.design_briefs.is_empty());
}

#[tokio::test]
async fn planner_reports_waiting_until_a_child_finishes_then_resumes_running() {
    let (_fixture, hub) = hub();
    let child = job(&hub, Role::Investigator, ".");
    hub.mutate(|state| { state.jobs.insert(child.id.clone(), child.clone()); Ok(()) }).unwrap();
    let exec = Execution { hub: hub.clone(), id: "main".into(), role: Role::Planner, flow: Flow::Complete, scope: vec![".".into()] };
    let signal = hub.root_signal.clone();
    let waiting = tokio::spawn(async move { exec.wait_for_children(signal).await });
    tokio::time::timeout(Duration::from_secs(2), async {
        while hub.manifest.lock().unwrap().root_status != Status::Waiting { tokio::task::yield_now().await; }
    }).await.unwrap();
    assert!(!waiting.is_finished());
    hub.mutate(|state| { state.jobs.get_mut(&child.id).unwrap().status = Status::Completed; Ok(()) }).unwrap();
    tokio::time::timeout(Duration::from_secs(2), waiting).await.unwrap().unwrap().unwrap();
    assert_eq!(hub.manifest.lock().unwrap().root_status, Status::Running);
}

#[test]
fn code_mutations_cannot_escape_read_only_roles_or_narrow_scopes() {
    for role in [Role::Planner, Role::Investigator, Role::Orchestrator, Role::Reviewer] {
        for tool in ["write", "edit", "bash"] { assert!(!role.allows(Flow::Complete, tool, true)); }
    }
    assert!(Role::Builder.allows(Flow::Planned, "write", false));
    assert!(!Role::Builder.allows(Flow::Planned, "bash", false));
    assert!(!Role::Builder.allows(Flow::Planned, "mcp_mutation", false));
    assert!(Role::Reviewer.allows(Flow::Complete, "workflow_check", true));
}

#[test]
fn legacy_options_remain_readable_and_flow_is_explicit() {
    let legacy: TurnOptions = serde_json::from_value(json!({"account":"test","model":"test","reasoning":null,"mode":"plan","approvalMode":"manual"})).unwrap();
    assert_eq!(legacy.workflow, None); assert_eq!(legacy.mode, Mode::Plan);
    let modern: TurnOptions = serde_json::from_value(json!({"account":"test","model":"test","reasoning":null,"mode":"build","approvalMode":"manual","workflow":"complete"})).unwrap();
    assert_eq!(modern.workflow, Some(Flow::Complete)); assert_eq!(modern.approval_mode, ApprovalMode::Manual);
}

#[test]
fn isolated_worker_journals_keep_role_context_and_permissions_on_recovery() {
    let (_fixture, hub) = hub();
    let first = job(&hub, Role::Investigator, "."); let second = job(&hub, Role::Writer, "docs");
    let (a, _) = storage::worker(&hub, &first, None).unwrap();
    let (b, _) = storage::worker(&hub, &second, None).unwrap();
    assert_eq!(a.snapshot().unwrap().turns[0].user, first.prompt);
    assert!(a.input().unwrap().iter().any(|item| item.to_string().contains("Implement the requested outcome")));
    a.update(true, |data| { data.turns.last_mut().unwrap().turn.steps.push(Step { text: "Private first evidence".into(), ..Step::default() }); }).unwrap();
    assert!(b.input().unwrap().iter().all(|item| !item.to_string().contains("Private first evidence")));
    assert_ne!(a.journal, b.journal);
    finish(&a, Err(AgentError::cancelled()));
    let (resumed, _) = storage::worker(&hub, &first, Some("Inspect the saved checkpoint before retrying".into())).unwrap();
    let data = resumed.data.lock().unwrap();
    assert_eq!(data.turns.len(), 2);
    assert_eq!(data.turns[0].turn.steps[0].text, "Private first evidence");
    assert_eq!(data.turns[1].turn.options.approval_mode, ApprovalMode::Yolo);
    assert!(data.turns[1].wire.iter().all(|item| item["type"] != "function_call"));
}

#[tokio::test]
async fn project_checks_exclude_mutations_without_serializing_independent_writers() {
    let (_fixture, hub) = hub();
    let exec = Execution { hub: hub.clone(), id: "main".into(), role: Role::Builder, flow: Flow::Complete, scope: vec![".".into()] };
    let tool = ToolCall { id:"write".into(), name:"write".into(), args:json!({"path":"src/a","content":"text"}), status:"pending".into(), output:String::new(), duration_ms:0 };
    let first = exec.mutation_guard(&tool, hub.root_signal.clone()).await.unwrap();
    let second = exec.mutation_guard(&tool, hub.root_signal.clone()).await.unwrap();
    assert!(hub.check_lock.try_write().is_err());
    drop(first); drop(second);
    let check = hub.check_lock.write().await;
    let (cancel, signal) = watch::channel(false);
    let pending = tokio::spawn(async move { exec.mutation_guard(&tool, signal).await.map(|_| ()) });
    tokio::task::yield_now().await; assert!(!pending.is_finished());
    cancel.send_replace(true); assert_eq!(pending.await.unwrap().unwrap_err().code, "cancelled");
    drop(check);
}

#[test]
fn persisted_manifest_marks_unfinished_work_interrupted_without_changing_completed_results() {
    let (_fixture, hub) = hub();
    let mut finished = job(&hub, Role::Investigator, "."); finished.status = Status::Completed;
    let mut pending = job(&hub, Role::Builder, "src"); pending.status = Status::Running;
    hub.mutate(|state| { state.jobs.insert(finished.id.clone(), finished.clone()); state.jobs.insert(pending.id.clone(), pending.clone()); Ok(()) }).unwrap();
    let restored = storage::load(&hub.directory, &hub.root.id).unwrap().unwrap();
    assert_eq!(restored.root_status, Status::Interrupted);
    assert_eq!(restored.jobs[&pending.id].status, Status::Interrupted);
    assert_eq!(restored.jobs[&finished.id].status, Status::Completed);
    assert_eq!(hub.job(&pending.id).unwrap().status, Status::Running);
}

#[tokio::test]
async fn child_manual_approval_cannot_be_answered_by_the_parent_or_another_worker() {
    let (_fixture, hub) = hub();
    let job = job(&hub, Role::Builder, "src");
    let (child, signal) = storage::worker(&hub, &job, None).unwrap();
    let tool = ToolCall { id:"child-write".into(), name:"write".into(), args:json!({"path":"src/example.ts","content":"text"}), status:"pending".into(), output:String::new(), duration_ms:0 };
    let turn = child.snapshot().unwrap().active_turn_id.unwrap();
    let task_child = child.clone(); let task_tool = tool.clone();
    let awaiting = tokio::spawn(async move { authorize(&task_child, &task_tool, &job.options, signal).await });
    tokio::task::yield_now().await;
    assert!(child.snapshot().unwrap().pending_approval.is_some());
    assert!(answer_approval(&hub.root, &turn, &tool.id, true).is_err());
    answer_approval(&child, &turn, &tool.id, false).unwrap();
    assert!(!awaiting.await.unwrap().unwrap());
}

#[test]
fn model_preferences_are_per_flow_and_never_change_tool_authorization() {
    let (_fixture, hub) = hub();
    let mut options = hub.manifest.lock().unwrap().options.clone();
    let mut profiles = BTreeMap::new();
    profiles.insert(settings::key(Flow::Complete, Role::Reviewer), settings::ModelChoice { account:"review-account".into(), model:"gpt-5.6-sol".into(), reasoning:Some("xhigh".into()) });
    settings::apply(&mut options, &profiles, Flow::Planned, Role::Builder);
    assert_eq!(options.model, "root-model");
    settings::apply(&mut options, &profiles, Flow::Complete, Role::Reviewer);
    assert_eq!(options.account, "review-account"); assert_eq!(options.model, "gpt-5.6-sol"); assert_eq!(options.reasoning.as_deref(), Some("xhigh"));
    assert_eq!(options.approval_mode, ApprovalMode::Manual);
    assert!(settings::validate(Flow::Complete, &profiles).is_err());
    assert!(settings::validate(Flow::Standard, &profiles).is_ok());
}

#[test]
fn complete_closure_requires_independent_approval_for_the_exact_bead() {
    let (_fixture, hub) = hub();
    let exec = Execution { hub:hub.clone(), id:"main".into(), role:Role::Orchestrator, flow:Flow::Complete, scope:vec![".".into()] };
    let tool = ToolCall { id:"close".into(), name:"beads_close".into(), args:json!({"id":"task-a","reason":"verified"}), status:"pending".into(), output:String::new(), duration_ms:0 };
    assert!(exec.preflight(&tool).is_some());
    let mut review = job(&hub, Role::Reviewer, "."); review.status = Status::Completed;
    review.handoff = Some(Handoff { verdict:Verdict::Approved, summary:"Reviewed".into(), outcomes:vec!["Outcome".into()], evidence:vec!["src/code.ts".into()], validation:vec!["Tests pass".into()], limitations:vec![], task_ids:vec!["task-a".into()] });
    hub.mutate(|state| { state.jobs.insert(review.id.clone(),review); Ok(()) }).unwrap();
    assert!(exec.preflight(&tool).is_none());
    assert!(exec.preflight(&ToolCall { args:json!({"id":"other"}), ..tool.clone() }).is_some());
    let mut rework = job(&hub, Role::Builder, "src"); rework.status = Status::Running;
    hub.mutate(|state| { state.jobs.insert(rework.id.clone(), rework.clone()); Ok(()) }).unwrap();
    assert!(exec.preflight(&tool).is_some());
    hub.mutate(|state| { let writer = state.jobs.get_mut(&rework.id).unwrap(); writer.status = Status::Completed; writer.updated_at = now() + 1000; Ok(()) }).unwrap();
    assert!(exec.preflight(&tool).is_some());
}
