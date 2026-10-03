use super::*;
use crate::agent::tests::{options, Fixture};
use crate::agent::{journal, ApprovalMode};

const CHAT_A: &str = "11111111111111111111111111111111";
const CHAT_B: &str = "22222222222222222222222222222222";

#[test]
fn accepted_queued_choice_preserves_primary_and_secondary_after_a_saved_swap_and_restart() {
    let fixture = Fixture::new();
    let state = state(&fixture.root);
    let profile = key(Flow::Standard, Role::Builder);
    let mut accepted = choice("accepted-primary");
    accepted.fallback = Some(Box::new(choice("accepted-secondary")));
    let mut submitted = options(ApprovalMode::Yolo);
    submitted.workflow = Some(Flow::Standard);
    accepted.apply(&mut submitted);
    // A supplied snapshot is replaced by the native persisted choice at admission.
    submitted.model_selection = Some(choice("renderer-supplied"));
    state
        .with_connection(&fixture.root, |db| {
            write(db, CHAT_A, &profile, &accepted)?;
            apply_saved(db, CHAT_A, &mut submitted)?;
            capture(db, &fixture.root, CHAT_A, &mut submitted)
        })
        .unwrap();
    assert_eq!(submitted.model_selection, Some(accepted.clone()));
    let session = crate::agent::tests::session(&fixture);
    let mut executing = options(ApprovalMode::Yolo);
    executing.workflow = Some(Flow::Standard);
    session
        .reserve("Running request".into(), executing)
        .unwrap();
    session
        .submit("Accepted queued request".into(), submitted)
        .unwrap();
    assert_eq!(session.snapshot().unwrap().turns[0].options.model, "model");
    let mut swapped = choice("accepted-secondary");
    swapped.fallback = Some(Box::new(choice("accepted-primary")));
    state
        .with_connection(&fixture.root, |db| write(db, CHAT_A, &profile, &swapped))
        .unwrap();
    let queue = journal::load_all(&session.journal).unwrap().1.queue;
    assert_eq!(queue[0].options.model_selection, Some(accepted.clone()));
    let frozen = profiles(&state, &fixture.root, CHAT_A, &queue[0].options).unwrap();
    assert_eq!(frozen[&profile], accepted);
    assert_eq!(
        selected(&state, &fixture.root, CHAT_A, &queue[0].options).unwrap(),
        Some(swapped)
    );
    let mut resumed = queue[0].options.clone();
    choice("accepted-secondary").apply(&mut resumed);
    let resumed: TurnOptions =
        serde_json::from_value(serde_json::to_value(resumed).unwrap()).unwrap();
    assert_eq!(resumed.model_selection, Some(accepted));
    assert_eq!(
        profiles(&state, &fixture.root, CHAT_A, &resumed).unwrap()[&profile],
        choice("accepted-secondary")
    );
}

#[test]
fn queued_replacement_resolves_accepted_source_without_overwriting_newer_chat_preferences() {
    let fixture = Fixture::new();
    let state = state(&fixture.root);
    let profile = key(Flow::Standard, Role::Builder);
    let mut accepted = choice("queued-primary");
    accepted.fallback = Some(Box::new(choice("queued-secondary")));
    let mut submitted = options(ApprovalMode::Yolo);
    submitted.workflow = Some(Flow::Standard);
    accepted.apply(&mut submitted);
    submitted.model_selection = Some(accepted.clone());
    let next = choice("next-chat-model");
    state
        .with_connection(&fixture.root, |db| {
            write(db, CHAT_A, &profile, &next)?;
            crate::model_bindings::replace(
                db,
                &format!("chat:{CHAT_A}"),
                &accepted,
                &choice("queued-replacement"),
            )?;
            crate::agent::provider_links::resolve_chat(db, CHAT_A, &mut submitted)?;
            assert_eq!(submitted.model, "queued-replacement");
            assert_eq!(
                submitted.model_selection.as_ref().unwrap().fallback,
                accepted.fallback
            );
            assert_eq!(read(db, CHAT_A)?[&profile], next);
            // A fresh selection is not allowed to inherit a stale queue replacement.
            let mut fresh = options(ApprovalMode::Yolo);
            fresh.workflow = Some(Flow::Standard);
            accepted.apply(&mut fresh);
            crate::agent::provider_links::resolve_chat(db, CHAT_A, &mut fresh)?;
            assert_eq!(fresh.model, "queued-primary");
            Ok::<_, AgentError>(())
        })
        .unwrap();
}

#[test]
fn local_publication_choice_is_snapshotted_without_changing_global_or_other_worker_profiles() {
    let (fixture, hub) = super::super::super::tests::hub();
    let state = state(&fixture.root);
    let github = key(Flow::Publication, Role::Github);
    let builder = key(Flow::Standard, Role::Builder);
    fs::write(
        crate::data_dir::root(&fixture.root).join("agents.json"),
        serde_json::to_vec(&ModelSettings::from([
            (github.clone(), choice("global-github")),
            (builder.clone(), choice("global-builder")),
        ]))
        .unwrap(),
    )
    .unwrap();
    let local = choice("chat-github");
    state
        .with_connection(&fixture.root, |db| {
            write(db, CHAT_A, "agent:builtin:github", &local)
        })
        .unwrap();
    let admitted = defaults(&state, &fixture.root, CHAT_A).unwrap();
    assert_eq!(admitted[&github], local);
    assert_eq!(admitted[&builder], choice("global-builder"));
    hub.manifest.lock().unwrap().profiles = admitted;
    state
        .with_connection(&fixture.root, |db| {
            write(
                db,
                CHAT_A,
                "agent:builtin:github",
                &choice("next-publication"),
            )
        })
        .unwrap();
    let frozen = worker_choice(&hub, Flow::Publication, Role::Github)
        .unwrap()
        .unwrap();
    let mut publication = options(ApprovalMode::Yolo);
    frozen.apply(&mut publication);
    assert_eq!(publication.model, "chat-github");
    assert_eq!(
        super::super::load(&state, &fixture.root).unwrap()[&github],
        choice("global-github")
    );
}

#[test]
fn saved_choices_override_stale_admission_and_idle_views_without_changing_running_options() {
    let fixture = Fixture::new();
    let state = state(&fixture.root);
    let profile = key(Flow::Standard, Role::Builder);
    let saved = choice("new-chat-model");
    state
        .with_connection(&fixture.root, |db| write(db, CHAT_A, &profile, &saved))
        .unwrap();
    let mut incoming = options(ApprovalMode::Yolo);
    incoming.workflow = Some(Flow::Standard);
    choice("previous-model").apply(&mut incoming);
    let mut snapshot = crate::agent::tests::session(&fixture).snapshot().unwrap();
    snapshot.conversation_id = CHAT_A.into();
    snapshot.active_turn_id = Some("running".into());
    let mut displayed = Some(incoming.clone());
    hydrate_idle(&state, &fixture.root, &snapshot, &mut displayed).unwrap();
    assert_eq!(displayed.unwrap().model, "previous-model");
    snapshot.active_turn_id = None;
    let mut displayed = Some(incoming.clone());
    hydrate_idle(&state, &fixture.root, &snapshot, &mut displayed).unwrap();
    let displayed = displayed.unwrap();
    assert_eq!(displayed.model, "new-chat-model");
    assert_eq!(displayed.model_selection, Some(saved));
    state
        .with_connection(&fixture.root, |db| apply_saved(db, CHAT_A, &mut incoming))
        .unwrap();
    assert_eq!(incoming.model, "new-chat-model");
    let edit = choice("edited-during-validation");
    state
        .with_connection(&fixture.root, |db| write(db, CHAT_A, &profile, &edit))
        .unwrap();
    remember(&state, &fixture.root, CHAT_A, &incoming).unwrap();
    state
        .with_connection(&fixture.root, |db| {
            assert_eq!(read(db, CHAT_A)?[&profile], edit);
            Ok::<_, AgentError>(())
        })
        .unwrap();
}

fn choice(model: &str) -> ModelChoice {
    ModelChoice {
        executor: crate::claude::Executor::Jarvis,
        account: "provider".into(),
        model: model.into(),
        reasoning: Some("high".into()),
        fallback: None,
    }
}

fn state(home: &Path) -> AppState {
    let state = AppState::default();
    state.with_connection(home, |db| {
        db.execute_batch("INSERT INTO workspaces(id,name) VALUES('w','Workspace'); INSERT INTO projects(id,workspace_id,name,path) VALUES('p1','w','Project A','/tmp/project-a'),('p2','w','Project B','/tmp/project-b'); INSERT INTO conversations(id,project_id,title) VALUES('11111111111111111111111111111111','p1','A'),('22222222222222222222222222222222','p2','B');").map_err(|_| AgentError::storage())?;
        Ok::<_, AgentError>(())
    }).unwrap();
    state
}

#[test]
fn chat_choices_are_isolated_durable_and_do_not_mutate_defaults_or_a_running_manifest() {
    let fixture = Fixture::new();
    let state = state(&fixture.root);
    let profile = key(Flow::Standard, Role::Builder);
    let directory = crate::data_dir::root(&fixture.root);
    fs::write(
        directory.join("agents.json"),
        serde_json::to_vec(&ModelSettings::from([(profile.clone(), choice("default"))])).unwrap(),
    )
    .unwrap();
    let mut chat_a = ModelChoice {
        executor: crate::claude::Executor::Claude,
        account: String::new(),
        model: "sonnet".into(),
        reasoning: Some("high".into()),
        fallback: None,
    };
    chat_a.fallback = Some(Box::new(choice("backup")));
    state
        .with_connection(&fixture.root, |db| {
            write(db, CHAT_A, &profile, &chat_a)?;
            write(db, CHAT_B, &profile, &choice("sol-primary"))?;
            Ok::<_, AgentError>(())
        })
        .unwrap();
    let mut submitted = options(ApprovalMode::Yolo);
    submitted.workflow = Some(Flow::Standard);
    chat_a.apply(&mut submitted);
    let frozen = profiles(&state, &fixture.root, CHAT_A, &submitted).unwrap();
    state
        .with_connection(&fixture.root, |db| {
            write(db, CHAT_B, &profile, &choice("sol-updated"))
        })
        .unwrap();
    assert_eq!(frozen[&profile], chat_a);
    assert_eq!(
        super::super::load(&state, &fixture.root).unwrap()[&profile],
        choice("default")
    );
    drop(state);
    let restarted = AppState::default();
    restarted
        .with_connection(&fixture.root, |db| {
            assert_eq!(read(db, CHAT_A)?[&profile], chat_a);
            assert_eq!(read(db, CHAT_B)?[&profile], choice("sol-updated"));
            db.execute("DELETE FROM conversations WHERE id=?1", [CHAT_A])
                .unwrap();
            assert!(read(db, CHAT_A)?.is_empty());
            assert_eq!(read(db, CHAT_B)?.len(), 1);
            Ok::<_, AgentError>(())
        })
        .unwrap();
}

#[test]
fn history_seeds_the_intended_primary_and_keeps_agent_and_flow_choices_separate() {
    let fixture = Fixture::new();
    let state = state(&fixture.root);
    let history = fixture.root.join("models.jsonl");
    fs::write(&history, "{}\n").unwrap();
    let mut submitted = options(ApprovalMode::Yolo);
    submitted.workflow = Some(Flow::Standard);
    choice("requested-primary").apply(&mut submitted);
    journal::append_event(
        &history,
        "turn_checkpoint",
        &json!({"turn":{"id":"turn-1","options":submitted}}),
    )
    .unwrap();
    choice("runtime-fallback").apply(&mut submitted);
    journal::append_event(
        &history,
        "turn_checkpoint",
        &json!({"turn":{"id":"turn-1","options":submitted}}),
    )
    .unwrap();
    submitted.workflow = Some(Flow::Custom);
    submitted.custom_agent_id = Some("builtin:github".into());
    choice("github-chat-primary").apply(&mut submitted);
    journal::append_event(
        &history,
        "turn_checkpoint",
        &json!({"turn":{"id":"turn-2","options":submitted}}),
    )
    .unwrap();
    submitted.workflow = Some(Flow::Designer);
    submitted.custom_agent_id = None;
    choice("designer-fallback").apply(&mut submitted);
    journal::append_event(
        &history,
        "turn_checkpoint",
        &json!({"turn":{"id":"vacuumed-turn","options":submitted},"wire":[{"_jarvis_model_fallback":{"from":choice("designer-primary")}}]}),
    ).unwrap();
    state
        .with_connection(&fixture.root, |db| {
            seed_history(db, &fixture.root, CHAT_A, &history)?;
            let models = read(db, CHAT_A)?;
            assert_eq!(models["standard/builder"].model, "requested-primary");
            assert_eq!(models["agent:builtin:github"].model, "github-chat-primary");
            assert_eq!(models["designer/designer"], choice("designer-primary"));
            write(db, CHAT_A, "standard/builder", &choice("explicit-choice"))?;
            seed_history(db, &fixture.root, CHAT_A, &history)?;
            assert_eq!(
                read(db, CHAT_A)?["standard/builder"].model,
                "explicit-choice"
            );
            Ok::<_, AgentError>(())
        })
        .unwrap();
    assert!(validate_key(&fixture.root, "standard/builder").is_ok());
    assert!(validate_key(&fixture.root, "agent:builtin:github").is_ok());
    assert!(validate_key(&fixture.root, "../agents.json").is_err());
    assert!(validate_key(&fixture.root, "standard/designer").is_err());
}

#[test]
fn removed_models_remain_selected_until_explicit_replacement_and_stale_remaps_cannot_override_an_edit(
) {
    let fixture = Fixture::new();
    let state = state(&fixture.root);
    let profile = key(Flow::Standard, Role::Builder);
    let old = choice("removed-model");
    state
        .with_connection(&fixture.root, |db| write(db, CHAT_A, &profile, &old))
        .unwrap();
    assert!(validate_choice(&state, &OpenAiCodexState::default(), &fixture.root, &old).is_err());
    let mut submitted = options(ApprovalMode::Yolo);
    submitted.workflow = Some(Flow::Standard);
    old.apply(&mut submitted);
    state
        .with_connection(&fixture.root, |db| {
            assert_eq!(read(db, CHAT_A)?[&profile], old);
            let binding = binding_key(CHAT_A, &profile);
            crate::model_bindings::replace(db, &binding, &old, &choice("explicit-replacement"))?;
            resolve_options(db, CHAT_A, &mut submitted)?;
            assert_eq!(submitted.model, "explicit-replacement");
            let edit = choice("user-edited");
            write(db, CHAT_A, &profile, &edit)?;
            assert_eq!(read(db, CHAT_A)?[&profile], edit);
            old.apply(&mut submitted);
            resolve_options(db, CHAT_A, &mut submitted)?;
            assert_eq!(submitted.model, "removed-model");
            Ok::<_, AgentError>(())
        })
        .unwrap();
}
