use super::*;

#[test]
fn minimal_agent_request_uses_safe_defaults_and_draft_accepts_incomplete_url() {
    let request:Request=serde_json::from_value(serde_json::json!({"method":"POST","url":"{{base_url}}/items","body":{"type":"json","text":"{"}})).unwrap();
    assert_eq!(request.auth.kind, "none");
    assert_eq!(request.body.kind, "json");
    assert!(store::validate_request(&request).is_ok());
    assert!(store::validate_request(&Request::default()).is_ok());
}

#[test]
fn settings_reject_duplicate_variables_and_unsafe_transport_limits() {
    let variable = Variable {
        id: "one".into(),
        name: "base_url".into(),
        enabled: true,
        ..Variable::default()
    };
    let mut settings = Settings {
        variables: vec![
            variable.clone(),
            Variable {
                id: "two".into(),
                ..variable
            },
        ],
        ..Settings::default()
    };
    assert!(store::validate_settings(&settings).is_err());
    settings.variables.clear();
    settings.defaults.read_timeout_seconds = 0;
    assert!(store::validate_settings(&settings).is_err());
}

#[test]
fn templating_preserves_empty_values_and_rejects_unknown_variables() {
    let values = std::collections::BTreeMap::from([
        ("base_url".into(), "http://localhost:8000".into()),
        ("empty".into(), String::new()),
    ]);
    assert_eq!(
        transport::interpolate("{{base_url}}/path?x={{empty}}", &values, |_| panic!(
            "not a secret"
        ))
        .unwrap(),
        "http://localhost:8000/path?x="
    );
    assert!(transport::interpolate("{{missing}}", &values, |_| panic!("not a secret")).is_err());
    assert!(transport::interpolate("{{base_url", &values, |_| panic!("not a secret")).is_err());
}

#[test]
fn database_scopes_drafts_and_runs_to_their_conversation() {
    let mut db = rusqlite::Connection::open_in_memory().unwrap();
    crate::persistence::initialize_database(&mut db).unwrap();
    db.execute("INSERT INTO workspaces(id,name)VALUES('w','Workspace')", [])
        .unwrap();
    db.execute("INSERT INTO projects(id,workspace_id,name,path)VALUES('p','w','Project','/tmp/http-project')",[]).unwrap();
    db.execute("INSERT INTO conversations(id,project_id,title)VALUES('c','p','Chat'),('other','p','Other')",[]).unwrap();
    let value = Draft {
        id: "draft".into(),
        project_id: "p".into(),
        conversation_id: "c".into(),
        revision: 1,
        saved_request_id: None,
        request: Request::default(),
    };
    db.execute(
        "INSERT INTO http_drafts(id,conversation_id,payload)VALUES(?1,?2,?3)",
        params![
            value.id,
            value.conversation_id,
            store::encode(&value).unwrap()
        ],
    )
    .unwrap();
    assert_eq!(store::draft(&db, "c", "draft").unwrap().revision, 1);
    assert!(store::draft(&db, "other", "draft").is_err());
    db.execute("DELETE FROM conversations WHERE id='c'", [])
        .unwrap();
    assert!(store::draft(&db, "c", "draft").is_err());
}
