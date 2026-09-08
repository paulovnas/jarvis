use super::*;
use serde_json::json;

fn choice(account: &str, model: &str) -> ModelChoice {
    ModelChoice {
        account: account.into(),
        model: model.into(),
        reasoning: None,
    }
}

fn fixture() -> (Connection, tempfile::TempDir) {
    let mut db = Connection::open_in_memory().unwrap();
    persistence::initialize_database(&mut db).unwrap();
    let home = tempfile::tempdir().unwrap();
    std::fs::create_dir(home.path().join(".jarvis")).unwrap();
    for name in ["old", "new", "third"] {
        persistence::insert_provider_account(&db, &format!("openai-codex-{name}"), name).unwrap();
    }
    db.execute("INSERT INTO web_search_config(id,account_alias,model,inherit_chat) VALUES(1,'openai-codex-old','old-model',0)", []).unwrap();
    std::fs::write(
        home.path().join(".jarvis/agents.json"),
        json!({"complete/planner":choice("openai-codex-old", "old-model")}).to_string(),
    )
    .unwrap();
    std::fs::write(home.path().join(".jarvis/workflow-catalog.json"), json!({"revision":2,"agents":[{"id":"a".repeat(32),"name":"Analista","description":"","instructions":"Analyze","capability":"read_only","model":choice("openai-codex-old", "old-model")}],"flows":[{"id":"b".repeat(32),"name":"Meu fluxo","description":"","entry":"c".repeat(32),"maxSteps":1,"steps":[{"id":"c".repeat(32),"agentId":"a".repeat(32),"instructions":"","position":{"x":0,"y":0},"next":null,"onRework":null}]}]}).to_string()).unwrap();
    (db, home)
}

#[test]
fn previews_all_configured_items_and_preserves_unmapped_choices_after_deletion() {
    let (mut db, home) = fixture();
    let preview = plan(&db, home.path(), "openai-codex-old").unwrap();
    assert_eq!(preview.items.len(), 3);
    assert!(preview
        .items
        .iter()
        .any(|item| item.details.contains(&"Fluxo: Meu fluxo".into())));
    let transaction = db.transaction().unwrap();
    let result = apply(
        &transaction,
        home.path(),
        &preview.alias,
        &preview.revision,
        &[],
    )
    .unwrap();
    persistence::delete_provider_account(&transaction, &preview.alias).unwrap();
    transaction.commit().unwrap();
    assert_eq!(result.unresolved.len(), 3);
    assert!(inventory(&db, home.path())
        .unwrap()
        .iter()
        .all(|item| item.choice.account == "openai-codex-old"));
    assert!(persistence::list_provider_accounts(&db)
        .unwrap()
        .iter()
        .all(|item| item.alias != "openai-codex-old"));
}

#[test]
fn vision_and_image_settings_remain_visible_when_their_provider_is_removed() {
    let (mut db, home) = fixture();
    db.execute("INSERT INTO vision_config(id,account_alias,model,inherit_chat) VALUES(1,'openai-codex-old','vision-model',0)", []).unwrap();
    db.execute(
        "INSERT INTO image_generation_config(id,account_alias) VALUES(1,'openai-codex-old')",
        [],
    )
    .unwrap();
    let preview = plan(&db, home.path(), "openai-codex-old").unwrap();
    assert!(preview.items.iter().any(|item| item.kind == Kind::Vision));
    assert!(preview
        .items
        .iter()
        .any(|item| item.kind == Kind::ImageGeneration));
    let transaction = db.transaction().unwrap();
    apply(
        &transaction,
        home.path(),
        &preview.alias,
        &preview.revision,
        &[],
    )
    .unwrap();
    persistence::delete_provider_account(&transaction, &preview.alias).unwrap();
    transaction.commit().unwrap();
    assert_eq!(inventory(&db, home.path()).unwrap().len(), 5);
    assert_eq!(
        db.query_row("SELECT count(*) FROM pragma_foreign_key_check", [], |row| {
            row.get::<_, i64>(0)
        })
        .unwrap(),
        0
    );
}

#[test]
fn destination_review_detects_account_and_custom_model_changes() {
    let (db, _) = fixture();
    let original = target_stamp(&db, "openai-codex-new").unwrap();
    db.execute(
        "UPDATE provider_accounts SET account_id='changed' WHERE alias='openai-codex-new'",
        [],
    )
    .unwrap();
    assert_ne!(target_stamp(&db, "openai-codex-new").unwrap(), original);
    let account_changed = target_stamp(&db, "openai-codex-new").unwrap();
    db.execute(
        "INSERT INTO custom_provider_configs(alias,config) VALUES('openai-codex-new','{}')",
        [],
    )
    .unwrap();
    assert_ne!(
        target_stamp(&db, "openai-codex-new").unwrap(),
        account_changed
    );
    db.execute(
        "UPDATE provider_accounts SET enabled=0 WHERE alias='openai-codex-new'",
        [],
    )
    .unwrap();
    assert!(target_stamp(&db, "openai-codex-new").is_err());
}

#[test]
fn explicit_model_edits_can_reselect_a_recreated_alias_without_stale_remapping() {
    let (db, _) = fixture();
    let original = choice("openai-codex-old", "old-model");
    let target = choice("openai-codex-new", "new-model");
    for key in [
        "builtin:complete/planner",
        "custom:agent",
        "chat:conversation",
    ] {
        model_bindings::replace(&db, key, &original, &target).unwrap();
        assert_eq!(
            model_bindings::resolve(&db, key, &original).unwrap(),
            target
        );
    }
    model_bindings::forget_item(&db, "builtin:complete/planner").unwrap();
    model_bindings::forget_item(&db, "custom:agent").unwrap();
    model_bindings::forget_choice(
        &db,
        "chat:conversation",
        &choice("openai-codex-old", "another-model"),
    )
    .unwrap();
    assert_eq!(
        model_bindings::resolve(&db, "chat:conversation", &original).unwrap(),
        target
    );
    model_bindings::forget_choice(&db, "chat:conversation", &original).unwrap();
    for key in [
        "builtin:complete/planner",
        "custom:agent",
        "chat:conversation",
    ] {
        assert_eq!(
            model_bindings::resolve(&db, key, &original).unwrap(),
            original
        );
    }
}

#[test]
fn remaps_tools_and_agents_atomically_without_rewriting_builtin_or_custom_definitions() {
    let (mut db, home) = fixture();
    let agents = std::fs::read(home.path().join(".jarvis/agents.json")).unwrap();
    let catalog = std::fs::read(home.path().join(".jarvis/workflow-catalog.json")).unwrap();
    let preview = plan(&db, home.path(), "openai-codex-old").unwrap();
    let replacements: Vec<_> = preview
        .items
        .iter()
        .map(|item| Replacement {
            id: item.id.clone(),
            choice: choice("openai-codex-new", "new-model"),
        })
        .collect();
    let transaction = db.transaction().unwrap();
    let result = apply(
        &transaction,
        home.path(),
        &preview.alias,
        &preview.revision,
        &replacements,
    )
    .unwrap();
    persistence::delete_provider_account(&transaction, &preview.alias).unwrap();
    transaction.commit().unwrap();
    assert_eq!(result.replaced, 3);
    assert!(result.unresolved.is_empty());
    assert!(inventory(&db, home.path())
        .unwrap()
        .iter()
        .all(|item| item.choice == choice("openai-codex-new", "new-model")));
    assert_eq!(
        agents,
        std::fs::read(home.path().join(".jarvis/agents.json")).unwrap()
    );
    assert_eq!(
        catalog,
        std::fs::read(home.path().join(".jarvis/workflow-catalog.json")).unwrap()
    );
    assert_eq!(
        model_bindings::resolve(
            &db,
            "builtin:complete/planner",
            &choice("openai-codex-third", "edited-model")
        )
        .unwrap(),
        choice("openai-codex-third", "edited-model")
    );
    // A second removal resolves the original saved choice to the latest explicit replacement.
    let second = plan(&db, home.path(), "openai-codex-new").unwrap();
    let next: Vec<_> = second
        .items
        .iter()
        .map(|item| Replacement {
            id: item.id.clone(),
            choice: choice("openai-codex-third", "third-model"),
        })
        .collect();
    let transaction = db.transaction().unwrap();
    apply(
        &transaction,
        home.path(),
        &second.alias,
        &second.revision,
        &next,
    )
    .unwrap();
    persistence::delete_provider_account(&transaction, &second.alias).unwrap();
    transaction.commit().unwrap();
    assert!(inventory(&db, home.path())
        .unwrap()
        .iter()
        .all(|item| item.choice.account == "openai-codex-third"));
}

#[test]
fn stale_review_and_invalid_replacements_leave_every_selection_untouched() {
    let (mut db, home) = fixture();
    let preview = plan(&db, home.path(), "openai-codex-old").unwrap();
    db.execute("UPDATE web_search_config SET model='changed'", [])
        .unwrap();
    let transaction = db.transaction().unwrap();
    assert!(apply(
        &transaction,
        home.path(),
        &preview.alias,
        &preview.revision,
        &[]
    )
    .is_err());
    drop(transaction);
    let current = plan(&db, home.path(), &preview.alias).unwrap();
    let replacement = Replacement {
        id: current.items[0].id.clone(),
        choice: choice("openai-codex-new", "new-model"),
    };
    let transaction = db.transaction().unwrap();
    assert!(apply(
        &transaction,
        home.path(),
        &preview.alias,
        &current.revision,
        &[replacement.clone(), replacement]
    )
    .is_err());
    drop(transaction);
    assert_eq!(model_bindings::revision(&db).unwrap(), 0);
    assert!(inventory(&db, home.path())
        .unwrap()
        .iter()
        .all(|item| item.choice.account == "openai-codex-old"));
}

#[test]
fn chat_replacements_change_future_choices_and_queue_metadata_without_editing_history() {
    let (mut db, home) = fixture();
    let workspace = "1".repeat(32);
    let project = "2".repeat(32);
    let conversation = "3".repeat(32);
    db.execute(
        "INSERT INTO workspaces(id,name) VALUES(?1,'Workspace')",
        [&workspace],
    )
    .unwrap();
    db.execute(
        "INSERT INTO projects(id,workspace_id,name,path) VALUES(?1,?2,'Projeto','/project')",
        params![project, workspace],
    )
    .unwrap();
    db.execute(
        "INSERT INTO conversations(id,project_id,title) VALUES(?1,?2,'Chat de teste')",
        params![conversation, project],
    )
    .unwrap();
    let path = library::session_path(home.path(), &project, &conversation, true).unwrap();
    let options: TurnOptions = serde_json::from_value(json!({"account":"openai-codex-old","model":"old-model","reasoning":null,"mode":"build","workflow":"standard","approvalMode":"yolo"})).unwrap();
    let history = format!(
        "{{}}\n{}\n{}\n",
        json!({"type":"turn_checkpoint","version":1,"data":{"turn":{"id":"turn-1","options":options}}}),
        json!({"type":"queue_checkpoint","version":1,"data":[{"id":"queued-1","options":options}]})
    );
    std::fs::write(&path, &history).unwrap();
    let preview = plan(&db, home.path(), "openai-codex-old").unwrap();
    let item = preview
        .items
        .iter()
        .find(|item| item.kind == Kind::Conversation)
        .unwrap();
    assert!(item.details.contains(&"Mensagens na fila".into()));
    let replacement = Replacement {
        id: item.id.clone(),
        choice: choice("openai-codex-new", "new-model"),
    };
    let transaction = db.transaction().unwrap();
    apply(
        &transaction,
        home.path(),
        &preview.alias,
        &preview.revision,
        &[replacement],
    )
    .unwrap();
    persistence::delete_provider_account(&transaction, &preview.alias).unwrap();
    transaction.commit().unwrap();
    let mut future = options;
    resolve_chat(&db, &conversation, &mut future).unwrap();
    assert_eq!(future.account, "openai-codex-new");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), history);
}
