use super::*;
use crate::mcp::{McpError, Secrets};
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Default)]
struct Vault {
    values: Mutex<HashMap<String, String>>,
    fail_store: AtomicBool,
}
impl Secrets for Vault {
    fn load(&self, key: &str) -> Result<String, McpError> {
        self.values
            .lock()
            .unwrap()
            .get(key)
            .cloned()
            .ok_or_else(|| crate::mcp::error("missing"))
    }
    fn store(&self, key: &str, value: &str) -> Result<(), McpError> {
        self.values.lock().unwrap().insert(key.into(), value.into());
        if self.fail_store.load(Ordering::Relaxed) {
            Err(crate::mcp::error("write failed"))
        } else {
            Ok(())
        }
    }
    fn delete(&self, key: &str) -> Result<(), McpError> {
        self.values.lock().unwrap().remove(key);
        Ok(())
    }
}
fn database() -> rusqlite::Connection {
    let db = rusqlite::Connection::open_in_memory().unwrap();
    db.execute_batch("CREATE TABLE projects(id TEXT PRIMARY KEY); INSERT INTO projects VALUES('project'); CREATE TABLE http_settings(project_id TEXT PRIMARY KEY,payload TEXT NOT NULL); CREATE TABLE related(value TEXT NOT NULL);").unwrap();
    db
}
fn secret_settings() -> Settings {
    Settings {
        project_id: "project".into(),
        variables: vec![Variable {
            id: "token".into(),
            name: "token".into(),
            value: "original-secret".into(),
            enabled: true,
            secret: true,
            ..Variable::default()
        }],
        ..Settings::default()
    }
}

#[test]
fn failed_save_or_import_preserves_working_credentials_and_database() {
    let mut db = database();
    let vault = Vault::default();
    let saved =
        store::save_settings_using(&mut db, "project", secret_settings(), &vault, |_| Ok(()))
            .unwrap();
    let old_key = store::secret_key("project", saved.variables[0].secret_ref.as_deref().unwrap());
    let mut changed = store::masked_settings(saved.clone());
    changed.variables[0].value = "replacement-secret".into();
    let failure = store::save_settings_using(&mut db, "project", changed, &vault, |tx| {
        tx.execute("INSERT INTO related VALUES('must roll back')", [])?;
        Err(invalid("import failed"))
    });
    assert!(failure.is_err());
    assert_eq!(store::settings(&db, "project").unwrap().revision, 1);
    assert_eq!(vault.load(&old_key).unwrap(), "original-secret");
    assert_eq!(vault.values.lock().unwrap().len(), 1);
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM related", [], |row| row
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    let mut cleared = store::masked_settings(saved);
    cleared.variables[0].configured = false;
    assert!(
        store::save_settings_using(&mut db, "project", cleared.clone(), &vault, |_| Err(
            invalid("rollback")
        ))
        .is_err()
    );
    assert_eq!(vault.load(&old_key).unwrap(), "original-secret");
    store::save_settings_using(&mut db, "project", cleared, &vault, |_| Ok(())).unwrap();
    assert!(vault.values.lock().unwrap().is_empty());
}

#[test]
fn vault_write_failure_and_forged_reference_cannot_replace_a_saved_secret() {
    let mut db = database();
    let vault = Vault::default();
    let saved =
        store::save_settings_using(&mut db, "project", secret_settings(), &vault, |_| Ok(()))
            .unwrap();
    let mut changed = store::masked_settings(saved.clone());
    changed.variables[0].value = "replacement".into();
    vault.fail_store.store(true, Ordering::Relaxed);
    assert!(store::save_settings_using(&mut db, "project", changed, &vault, |_| Ok(())).is_err());
    assert_eq!(vault.values.lock().unwrap().len(), 1);
    assert_eq!(store::settings(&db, "project").unwrap().revision, 1);
    vault.fail_store.store(false, Ordering::Relaxed);
    let mut forged = store::masked_settings(saved.clone());
    forged.variables[0].secret_ref = Some("var:other-credential".into());
    let updated =
        store::save_settings_using(&mut db, "project", forged, &vault, |_| Ok(())).unwrap();
    assert_eq!(
        updated.variables[0].secret_ref,
        saved.variables[0].secret_ref
    );
    let exposed = store::masked_settings(updated);
    assert!(exposed.variables[0].secret_ref.is_none());
    assert!(exposed.variables[0].value.is_empty());
}

#[test]
fn pasted_url_credentials_and_form_json_secrets_use_vault_references() {
    let vault = Vault::default();
    let mut request = Request {
        url: "{{base_url}}/items?tag=a&access_token=a%26b&tag=b#part".into(),
        body: Body {
            kind: "json".into(),
            text: r#"{"password":"quoted\"secret","data":"public"}"#.into(),
            ..Body::default()
        },
        ..Request::default()
    };
    store::protect_request_using("project", &mut request, &vault).unwrap();
    assert_eq!(request.url, "{{base_url}}/items#part");
    assert_eq!(
        request
            .params
            .iter()
            .map(|pair| pair.name.as_str())
            .collect::<Vec<_>>(),
        vec!["tag", "access_token", "tag"]
    );
    assert!(request.params[1].value.starts_with("{{secret:inline:"));
    assert!(!request.body.text.contains("quoted"));
    let stored = vault.values.lock().unwrap();
    assert!(stored.values().any(|value| value == "a&b"));
    assert!(stored.values().any(|value| value == "quoted\"secret"));
    drop(stored);
    let mut form = Request {
        body: Body {
            kind: "urlencoded".into(),
            fields: vec![BodyField {
                name: "password".into(),
                value: "form-secret".into(),
                enabled: true,
                ..BodyField::default()
            }],
            ..Body::default()
        },
        ..Request::default()
    };
    store::protect_request_using("project", &mut form, &vault).unwrap();
    assert!(form.body.fields[0].value.starts_with("{{secret:inline:"));
}

#[test]
fn portable_request_preserves_named_variables_but_excludes_literals_and_private_refs() {
    let mut request=Request {url:"{{base_url}}/items?access_token=literal-token&tag=one&tag=two".into(),auth:Auth{kind:"bearer".into(),token:"{{token}}".into(),..Auth::default()},headers:vec![Pair{name:"Authorization".into(),value:"Bearer {{secret:inline:abc}}".into(),enabled:true}],body:Body{kind:"json".into(),text:r#"{"password":"literal-password","nested":{"token":"{{token}}"},"reference":"{{secret:inline:def}}"}"#.into(),..Body::default()},..Request::default()};
    strip_portable(&mut request);
    let encoded = store::encode(&request).unwrap();
    assert!(!encoded.contains("literal-token"));
    assert!(!encoded.contains("literal-password"));
    assert!(!encoded.contains("secret:inline"));
    assert_eq!(request.auth.token, "{{token}}");
    assert_eq!(request.url, "{{base_url}}/items");
    assert_eq!(request.params.len(), 3);
    assert_eq!(request.params[0].value, "");
}

fn sample_run() -> Run {
    Run {
        id: "run".into(),
        project_id: "project".into(),
        conversation_id: "conversation".into(),
        draft_id: "draft".into(),
        draft_revision: 1,
        environment_id: None,
        environment_revision: 1,
        request: Request::default(),
        status: "completed".into(),
        http_status: Some(200),
        error: None,
        outcome_uncertain: false,
        started_at: 0,
        finished_at: Some(1),
        elapsed_ms: 1,
        received_bytes: 0,
        stored_bytes: 0,
        mime: "text/plain".into(),
        url: "http://localhost".into(),
        headers: vec![],
        redirects: vec![],
        truncated: false,
        body_expired: false,
        preview: String::new(),
    }
}

fn cleanup_database() -> rusqlite::Connection {
    let db = rusqlite::Connection::open_in_memory().unwrap();
    db.execute_batch("CREATE TABLE projects(id TEXT PRIMARY KEY); CREATE TABLE conversations(id TEXT PRIMARY KEY); CREATE TABLE http_runs(id TEXT PRIMARY KEY,conversation_id TEXT,payload TEXT); INSERT INTO projects VALUES('project'); INSERT INTO conversations VALUES('conversation');").unwrap();
    db
}

#[test]
fn cleanup_refreshes_live_runs_and_waits_for_file_preparation() {
    use std::sync::{mpsc, Arc};
    let home = tempfile::tempdir().unwrap();
    let db = Arc::new(Mutex::new(cleanup_database()));
    // Before a cancellation wait there were no runs. A different conversation
    // can finish a new request before cleanup resumes.
    assert!(stored_ids(&db.lock().unwrap(), "http_runs")
        .unwrap()
        .is_empty());
    let runtime = Arc::new(HttpState::default());
    let preparation = runtime.preparation.lock().unwrap();
    let body = run_dir(home.path(), "project", "new-run")
        .unwrap()
        .join("body");
    std::fs::create_dir_all(body.parent().unwrap()).unwrap();
    std::fs::write(&body, "new response").unwrap();
    let orphan = run_dir(home.path(), "project", "deleted-run").unwrap();
    std::fs::create_dir_all(&orphan).unwrap();
    let upload = directory(home.path(), "project")
        .unwrap()
        .join("files")
        .join("upload");
    std::fs::create_dir_all(&upload).unwrap();
    std::fs::write(upload.join("body"), "upload data").unwrap();
    let (started_tx, started_rx) = mpsc::channel();
    let (read_tx, read_rx) = mpsc::channel();
    let cleaner_runtime = Arc::clone(&runtime);
    let cleaner_db = Arc::clone(&db);
    let cleaner_home = home.path().to_owned();
    let cleaner = std::thread::spawn(move || {
        started_tx.send(()).unwrap();
        prune_files(&cleaner_home, &cleaner_runtime, || {
            read_tx.send(()).unwrap();
            let db = cleaner_db.lock().unwrap();
            Ok((
                stored_ids(&db, "projects")?,
                stored_ids(&db, "conversations")?,
                stored_ids(&db, "http_runs")?,
            ))
        })
        .unwrap();
    });
    started_rx.recv().unwrap();
    assert!(read_rx.recv_timeout(Duration::from_millis(50)).is_err());
    db.lock()
        .unwrap()
        .execute(
            "INSERT INTO http_runs VALUES('new-run','conversation','{}')",
            [],
        )
        .unwrap();
    drop(preparation);
    cleaner.join().unwrap();
    assert_eq!(std::fs::read_to_string(body).unwrap(), "new response");
    assert_eq!(
        std::fs::read_to_string(upload.join("body")).unwrap(),
        "upload data"
    );
    assert!(!orphan.exists());
}

#[test]
fn terminal_fallback_reconciles_before_a_new_explicit_send_and_survives_write_failure() {
    let db = cleanup_database();
    let mut run = sample_run();
    run.status = "running".into();
    db.execute(
        "INSERT INTO http_runs VALUES(?1,?2,?3)",
        params![run.id, run.conversation_id, store::encode(&run).unwrap()],
    )
    .unwrap();
    run.status = "completed".into();
    run.http_status = Some(200);
    run.finished_at = Some(10);
    let mut terminal = HashMap::from([(run.id.clone(), run.clone())]);
    db.execute_batch("CREATE TRIGGER fail_http_update BEFORE UPDATE ON http_runs BEGIN SELECT RAISE(ABORT, 'storage temporarily unavailable'); END;").unwrap();
    assert!(reconcile_terminal(&db, &mut terminal).is_err());
    assert_eq!(terminal[&run.id].http_status, Some(200));
    assert_eq!(
        store::run(&db, &run.conversation_id, &run.id)
            .unwrap()
            .status,
        "running"
    );
    db.execute_batch("DROP TRIGGER fail_http_update").unwrap();
    reconcile_terminal(&db, &mut terminal).unwrap();
    assert!(terminal.is_empty());
    assert_eq!(
        store::run(&db, &run.conversation_id, &run.id)
            .unwrap()
            .status,
        "completed"
    );
    assert_eq!(
        db.query_row(
            "SELECT COUNT(*) FROM http_runs WHERE json_extract(payload,'$.status')='running'",
            [],
            |row| row.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
}

#[test]
fn response_pages_cannot_reveal_raw_encoded_or_json_escaped_secret_fragments() {
    let home = tempfile::tempdir().unwrap();
    let run = sample_run();
    let directory = run_dir(home.path(), &run.project_id, &run.id).unwrap();
    std::fs::create_dir_all(&directory).unwrap();
    let secrets = vec!["complex token+&/".to_owned(), "a\"b\nc".to_owned()];
    let body = b"begin complex token+&/ | complex%20token%2B%26%2F | a\\\"b\\nc end";
    std::fs::write(directory.join("body"), body).unwrap();
    let mut expected = body.to_vec();
    store::redact_bytes(&mut expected, &secrets);
    assert!(!String::from_utf8_lossy(&expected).contains("complex"));
    for offset in 0..body.len() {
        for limit in [1, 3, 9] {
            let page = transport::read_result_with_secrets(
                home.path(),
                run.clone(),
                offset as u64,
                limit,
                &secrets,
            )
            .unwrap();
            assert_eq!(
                page.text.as_bytes(),
                &expected[offset..expected.len().min(offset + limit)]
            );
            assert_eq!(page.offset, offset as u64);
        }
    }
}

#[test]
fn truncated_response_tail_does_not_expose_a_partial_secret() {
    let home = tempfile::tempdir().unwrap();
    let mut run = sample_run();
    run.truncated = true;
    let directory = run_dir(home.path(), &run.project_id, &run.id).unwrap();
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(directory.join("body"), b"ok abc").unwrap();
    let page =
        transport::read_result_with_secrets(home.path(), run, 0, 100, &["abcdef".into()]).unwrap();
    assert_eq!(page.text, "ok ***");
}
