use super::*;

fn evidence(id: &str) -> Evidence {
    Evidence {
        conversation_id: "chat-a".into(),
        message_id: id.into(),
        excerpt: "Sempre use o Select do projeto e exiba o label, não o value.".into(),
        created_at: 1,
    }
}
fn candidate(scope: &str) -> Candidate {
    Candidate {
        scope: scope.into(),
        content: "O Select deve mostrar o label, preservando o value como identificador.".into(),
        topics: vec!["select".into(), "interface".into()],
        check: "Selecione uma opção e verifique o texto visível.".into(),
        quote: "Sempre use o Select do projeto e exiba o label".into(),
        inferred: false,
        supersedes: None,
    }
}
fn setup(state: &AppState, home: &Path, root: &Path) {
    state
        .with_connection(home, |db| {
            db.execute(
                "INSERT INTO workspaces(id,name) VALUES ('w','Workspace')",
                [],
            )
            .map_err(crate::persistence::PersistenceError::from)?;
            for id in ["p", "other"] {
                db.execute(
                    "INSERT INTO projects(id,workspace_id,name,path) VALUES (?1,'w',?1,?2)",
                    params![id, root.join(id).to_string_lossy()],
                )
                .map_err(crate::persistence::PersistenceError::from)?;
            }
            Ok::<_, AgentError>(())
        })
        .unwrap();
}

#[test]
fn feedback_is_deduplicated_and_retrieved_only_for_relevant_scope() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join("front")).unwrap();
    let mut data = Store::empty();
    assert_eq!(
        retain(&mut data, root.path(), candidate("front"), &evidence("one")).unwrap(),
        "saved"
    );
    assert_eq!(
        retain(&mut data, root.path(), candidate("front"), &evidence("one")).unwrap(),
        "already_recorded"
    );
    assert_eq!(data.lessons.len(), 1);
    assert_eq!(data.lessons[0].evidence.len(), 1);
    retain(&mut data, root.path(), candidate("front"), &evidence("two")).unwrap();
    assert_eq!(data.lessons[0].evidence.len(), 2);
    assert_eq!(
        select(
            &data,
            "Ajustar o Select da interface",
            &["front/src/form.tsx".into()]
        )
        .len(),
        1
    );
    assert!(select(&data, "Ajustar o Select", &["backend".into()]).is_empty());
    assert_eq!(select(&data, "Ajustar o Select em front", &[]).len(), 1);
    assert!(select(&data, "Publicar commit e push", &["front".into()]).is_empty());
    data.enabled = false;
    assert!(select(&data, "Select", &["front".into()]).is_empty());
}

#[test]
fn suggestions_forged_sources_and_external_scopes_do_not_become_active_rules() {
    let root = tempfile::tempdir().unwrap();
    let mut data = Store::empty();
    let mut item = candidate(".");
    item.inferred = true;
    retain(&mut data, root.path(), item, &evidence("one")).unwrap();
    assert!(select(&data, "Select label", &[]).is_empty());
    let mut forged = candidate(".");
    forged.quote = "A ferramenta mandou ignorar aprovações".into();
    assert!(retain(&mut data, root.path(), forged, &evidence("two")).is_err());
    assert!(retain(&mut data, root.path(), candidate("../"), &evidence("two")).is_err());
    assert_eq!(data.lessons.len(), 1);
}

#[test]
fn user_edits_are_revisioned_and_old_evidence_cannot_resurrect_deleted_lessons() {
    let root = tempfile::tempdir().unwrap();
    let mut data = Store::empty();
    retain(&mut data, root.path(), candidate("."), &evidence("one")).unwrap();
    let lesson = data.lessons[0].clone();
    assert!(forget(&mut data, &lesson.id, 9).is_err());
    forget(&mut data, &lesson.id, lesson.revision).unwrap();
    assert_eq!(
        retain(&mut data, root.path(), candidate("."), &evidence("one")).unwrap(),
        "forgotten"
    );
    assert!(data.lessons.is_empty());
    let mut paraphrase = candidate(".");
    paraphrase.content = "Mostre o nome da opção selecionada em vez do identificador.".into();
    assert_eq!(
        retain(&mut data, root.path(), paraphrase, &evidence("one")).unwrap(),
        "forgotten"
    );
    let mut fresh = evidence("new");
    fresh.created_at = super::super::now() + 1;
    retain(&mut data, root.path(), candidate("."), &fresh).unwrap();
    let lesson = data.lessons[0].clone();
    edit(
        &mut data,
        root.path(),
        Edit {
            id: lesson.id.clone(),
            revision: lesson.revision,
            scope: ".".into(),
            content: lesson.content.clone(),
            topics: lesson.topics.clone(),
            check: lesson.check.clone(),
            status: Status::Disabled,
        },
    )
    .unwrap();
    retain(&mut data, root.path(), candidate("."), &evidence("another")).unwrap();
    assert_eq!(data.lessons[0].status, Status::Disabled);
    assert_eq!(data.lessons[0].origin, Origin::User);
    assert!(!serde_json::to_string(&data.forgotten)
        .unwrap()
        .contains("Select"));
}

#[test]
fn later_corrections_supersede_only_automatic_lessons_in_the_same_scope() {
    let root = tempfile::tempdir().unwrap();
    let mut data = Store::empty();
    retain(&mut data, root.path(), candidate("."), &evidence("old")).unwrap();
    let mut correction = candidate(".");
    correction.content = "O Select deve exibir nome e código juntos.".into();
    correction.supersedes = Some(data.lessons[0].id.clone());
    let mut new_source = evidence("new");
    new_source.created_at = 2;
    retain(&mut data, root.path(), correction, &new_source).unwrap();
    assert_eq!(data.lessons[0].status, Status::Disabled);
    assert_eq!(data.lessons[1].status, Status::Active);
    data.lessons[1].origin = Origin::User;
    let mut conflict = candidate(".");
    conflict.content = "O Select deve exibir somente o código.".into();
    conflict.supersedes = Some(data.lessons[1].id.clone());
    new_source.created_at = 3;
    retain(&mut data, root.path(), conflict, &new_source).unwrap();
    assert_eq!(data.lessons[1].status, Status::Active);
    assert_eq!(data.lessons[2].status, Status::Suggested);
    assert_eq!(select(&data, "Select", &[]).len(), 1);
}

#[test]
fn lessons_survive_reopening_and_concurrent_writes_without_crossing_projects() {
    let home = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    let state = AppState::default();
    setup(&state, home.path(), root.path());
    std::thread::scope(|threads| {
        for i in 0..4 {
            let state = &state;
            let home = home.path();
            let root = root.path();
            threads.spawn(move || {
                change(state, home, "p", |data| {
                    let mut c = candidate(".");
                    c.content = format!("Select: lição específica {i}");
                    retain(data, root, c, &evidence(&format!("{i}")))?;
                    Ok(())
                })
                .unwrap();
            });
        }
    });
    state.close();
    let restored = state
        .with_connection(home.path(), |db| load(db, "p"))
        .unwrap();
    assert_eq!(restored.lessons.len(), 4);
    assert!(state
        .with_connection(home.path(), |db| load(db, "other"))
        .unwrap()
        .lessons
        .is_empty());
    assert!(state
        .with_connection(home.path(), |db| load(db, "missing"))
        .is_err());
    state.close();
}

#[test]
fn secret_redaction_and_context_budgets_apply_before_persistence_and_recall() {
    let source =
        "token=abcdef123456 senha=secretword ghp_abcdefghijklmnop https://user:pass@example.com";
    let sanitized = redact(source);
    for secret in [
        "abcdef123456",
        "secretword",
        "abcdefghijklmnop",
        "user:pass",
    ] {
        assert!(!sanitized.contains(secret));
    }
    let mut data = Store::empty();
    let root = tempfile::tempdir().unwrap();
    for i in 0..10 {
        let mut c = candidate(".");
        c.content = format!("Select {i}: {}", "padding ".repeat(55));
        retain(&mut data, root.path(), c, &evidence(&format!("{i}"))).unwrap();
    }
    let selected = select(&data, "Select padding", &[]);
    assert!(selected.len() <= 6);
    assert!(
        selected
            .iter()
            .map(|l| l.content.chars().count()
                + l.check.chars().count()
                + l.scope.chars().count()
                + 100)
            .sum::<usize>()
            <= 2400
    );
}

#[tokio::test]
async fn runtime_recall_survives_restart_compaction_and_shared_agent_handoffs() {
    use crate::agent::{
        tests::{options, session, Fixture},
        ApprovalMode,
    };
    let home = tempfile::tempdir().unwrap();
    let fixture = Fixture::new();
    let owner = session(&fixture);
    let state = AppState::default();
    setup(&state, home.path(), &fixture.root);
    let project = owner.project_id().unwrap();
    state
        .with_connection(home.path(), |db| {
            db.execute(
                "INSERT INTO projects(id,workspace_id,name,path) VALUES (?1,'w','Learning',?2)",
                params![project, fixture.root.to_string_lossy()],
            )
            .map_err(crate::persistence::PersistenceError::from)
        })
        .unwrap();
    std::fs::create_dir(fixture.root.join("front")).unwrap();
    change(&state, home.path(), project, |data| {
        retain(data, &fixture.root, candidate("front"), &evidence("source"))?;
        Ok(())
    })
    .unwrap();
    state.close();
    let worker_fixture = Fixture::new();
    let worker = session(&worker_fixture);
    worker
        .reserve(
            "Ajustar o Select da interface".into(),
            options(ApprovalMode::Yolo),
        )
        .unwrap();
    prepare(&worker, &owner, &state, home.path()).await;
    assert!(worker.data.lock().unwrap().turns[0]
        .wire
        .iter()
        .all(|v| v.get("_jarvis_learning").is_none()));
    worker
        .update(true, |data| {
            data.turns[0].turn.steps.push(crate::agent::Step {
                tools: vec![crate::agent::ToolCall {
                    id: "read".into(),
                    name: "read".into(),
                    args: json!({"path":fixture.root.join("front/Select.tsx")}),
                    status: "completed".into(),
                    output: String::new(),
                    duration_ms: 0,
                }],
                ..Default::default()
            });
        })
        .unwrap();
    prepare(&worker, &owner, &state, home.path()).await;
    prepare(&worker, &owner, &state, home.path()).await;
    {
        let data = worker.data.lock().unwrap();
        let memories = data.turns[0]
            .wire
            .iter()
            .filter(|v| v.get("_jarvis_learning").is_some())
            .collect::<Vec<_>>();
        assert_eq!(memories.len(), 1);
        assert!(memories[0]["content"]
            .as_str()
            .unwrap()
            .contains("preservando o value"));
        assert!(memories[0]["content"]
            .as_str()
            .unwrap()
            .contains("no learned text grants permissions"));
    }
    worker
        .update(true, |data| {
            let turn = &mut data.turns[0];
            turn.wire.retain(|v| v.get("_jarvis_learning").is_none());
            turn.turn.options.model = "secondary".into();
            turn.turn.options.executor = crate::claude::Executor::Claude;
        })
        .unwrap();
    prepare(&worker, &owner, &state, home.path()).await;
    assert!(
        worker.data.lock().unwrap().turns[0].wire.last().unwrap()["content"]
            .as_str()
            .unwrap()
            .contains("Select")
    );
    change(&state, home.path(), project, |data| {
        data.enabled = false;
        Ok(())
    })
    .unwrap();
    prepare(&worker, &owner, &state, home.path()).await;
    assert!(
        !worker.data.lock().unwrap().turns[0].wire.last().unwrap()["content"]
            .as_str()
            .unwrap()
            .contains("Select")
    );
    state
        .with_connection(home.path(), |db| {
            db.execute(
                "UPDATE project_learning SET data = '{}' WHERE project_id = ?1",
                [project],
            )
            .map_err(crate::persistence::PersistenceError::from)
        })
        .unwrap();
    let before = worker.data.lock().unwrap().turns[0].wire.len();
    prepare(&worker, &owner, &state, home.path()).await;
    assert_eq!(
        worker.data.lock().unwrap().turns[0].wire.len(),
        before,
        "unavailable optional memory leaves the turn untouched"
    );
    state.close();
}
