use super::*;
use rusqlite::params;

const PROJECT: &str = "11111111111111111111111111111111";
const OTHER: &str = "22222222222222222222222222222222";
const CONVERSATION: &str = "33333333333333333333333333333333";

struct Fixture {
    _temp: tempfile::TempDir,
    home: PathBuf,
    root: PathBuf,
    state: AppState,
    agent: AgentState,
}

fn git_ok(root: &Path, args: &[&str]) {
    let output = crate::background::command("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "Git fixture failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn create_repo(root: &Path, origin: &str) {
    fs::create_dir_all(root.join("src-tauri")).unwrap();
    fs::write(
        root.join("package.json"),
        r#"{"name":"jarvis","version":"9.9.9"}"#,
    )
    .unwrap();
    fs::write(
        root.join("src-tauri/tauri.conf.json"),
        r#"{"identifier":"com.foxtag.jarvis","productName":"Renamed"}"#,
    )
    .unwrap();
    fs::write(
        root.join("src-tauri/Cargo.toml"),
        "[package]\nname = \"jarvis\"\nversion = \"9.9.9\"\n",
    )
    .unwrap();
    git_ok(root, &["init", "--quiet"]);
    git_ok(root, &["remote", "add", "origin", origin]);
}

impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join("home");
        let root = temp.path().join("checkout-with-any-name");
        fs::create_dir(&home).unwrap();
        create_repo(&root, "git@github.com:paulovnas/jarvis.git");
        let root = fs::canonicalize(root).unwrap();
        let home = fs::canonicalize(home).unwrap();
        let state = AppState::default();
        state.with_connection(&home, |db| {
            db.execute("INSERT INTO workspaces(id,name) VALUES ('workspace','Workspace')", [])?;
            db.execute("INSERT INTO projects(id,workspace_id,name,path) VALUES (?1,'workspace','Jarvis',?2)", params![PROJECT, root.to_string_lossy()])?;
            Ok::<_, SelfDevelopmentError>(())
        }).unwrap();
        Self {
            _temp: temp,
            home,
            root,
            state,
            agent: AgentState::default(),
        }
    }

    fn add_project(&self, root: &Path, id: &str) {
        self.state.with_connection(&self.home, |db| {
            db.execute("INSERT INTO projects(id,workspace_id,name,path) VALUES (?1,'workspace','Jarvis',?2)", params![id, root.to_string_lossy()])?;
            Ok::<_, SelfDevelopmentError>(())
        }).unwrap();
    }

    fn enable(&self) {
        assert!(
            set_enabled(&self.state, &self.home, PROJECT, true)
                .unwrap()
                .enabled
        );
    }

    fn conversation(&self, project: &str, root: &Path, id: &str) -> PathBuf {
        self.state.with_connection(&self.home, |db| {
            db.execute("INSERT INTO conversations(id,project_id,title,created_at) VALUES (?1,?2,'Incident source',7)", params![id,project])?;
            Ok::<_, SelfDevelopmentError>(())
        }).unwrap();
        let journal = crate::library::session_path(&self.home, project, id, true).unwrap();
        let header = json!({"type":"session","version":1,"id":id,"projectId":project,"cwd":root,"title":"Incident source","createdAt":7});
        let turn = json!({
            "id":"private-turn-secret", "createdAt":8, "durationMs":40,"activeSince":null,
            "user":"RAW_CHAT_SECRET", "parts":[], "status":"completed", "tasks":[],
            "options":{"account":"RAW_ACCOUNT_SECRET","model":"private-model-secret","reasoning":"high","mode":"build","approvalMode":"manual"},
            "steps":[{"text":"RAW_ASSISTANT_SECRET","summary":"RAW_SUMMARY_SECRET","tools":[{
                "id":"private-tool-secret","name":"run_shell","args":{"command":"dangerous publication RAW_ARGUMENT_SECRET","token":"sk-private-credential-value"},
                "status":"completed","output":"RAW_TERMINAL_SECRET","durationMs":10
            }],"usage":null}],"error":null
        });
        let record = json!({"type":"turn_checkpoint","version":1,"data":{"turn":turn,"wire":[{"content":"RAW_PROVIDER_SECRET"}]}});
        fs::write(&journal, format!("{header}\n{record}\n")).unwrap();
        journal
    }
}

#[test]
fn default_off_requires_owner_opt_in_and_survives_restart() {
    let fixture = Fixture::new();
    let initial = status(&fixture.state, &fixture.home, PROJECT).unwrap();
    assert!(initial.eligible);
    assert!(!initial.enabled);
    assert!(!enabled_for_root(&fixture.home, &fixture.root, PROJECT));
    assert!(sources(&fixture.state, &fixture.agent, &fixture.home, PROJECT).is_err());
    fixture.enable();
    assert!(!enabled_for_root(&fixture.home, &fixture.root, OTHER));
    let reopened = AppState::default();
    assert!(status(&reopened, &fixture.home, PROJECT).unwrap().enabled);
    assert!(enabled_for_root(&fixture.home, &fixture.root, PROJECT));
    assert!(!fixture.root.join("self-development").exists());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let authorization = require_for_root(&fixture.home, &fixture.root, PROJECT).unwrap();
        let path = directory(&fixture.home, false).unwrap().join(format!(
            "{}.authorization.json",
            binding_key(&authorization).unwrap()
        ));
        assert_eq!(
            fs::metadata(path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
}

#[test]
fn enabling_a_relocated_registration_replaces_its_old_consent_and_incidents() {
    let fixture = Fixture::new();
    fixture.enable();
    fixture.conversation(PROJECT, &fixture.root, CONVERSATION);
    let summary = capture(
        &fixture.state,
        &fixture.agent,
        &fixture.home,
        PROJECT,
        CONVERSATION,
        None,
    )
    .unwrap();
    let previous = require_for_root(&fixture.home, &fixture.root, PROJECT).unwrap();
    let previous_incident = incident_path(&fixture.home, &previous, &summary.id).unwrap();
    let moved = fixture._temp.path().join("moved-checkout");
    create_repo(&moved, "https://github.com/paulovnas/jarvis.git");
    let moved = fs::canonicalize(moved).unwrap();
    fixture
        .state
        .with_connection(&fixture.home, |db| {
            db.execute(
                "UPDATE projects SET path=?1 WHERE id=?2",
                params![moved.to_string_lossy(), PROJECT],
            )?;
            Ok::<_, SelfDevelopmentError>(())
        })
        .unwrap();
    assert!(
        set_enabled(&fixture.state, &fixture.home, PROJECT, true)
            .unwrap()
            .enabled
    );
    assert!(require_for_root(&fixture.home, &moved, PROJECT).is_ok());
    assert!(require_for_root(&fixture.home, &fixture.root, PROJECT).is_err());
    assert_eq!(authorizations(&fixture.home).unwrap().len(), 1);
    assert!(!previous_incident.exists());
    assert!(list_approved_incidents(&fixture.home, &moved, PROJECT)
        .unwrap()
        .is_empty());
}

#[test]
fn same_name_and_spoofed_origins_never_grant_access() {
    let fixture = Fixture::new();
    let other = fixture.home.join("Jarvis");
    create_repo(&other, "https://github.com/attacker/jarvis.git");
    fixture.add_project(&other, OTHER);
    assert!(
        !status(&fixture.state, &fixture.home, OTHER)
            .unwrap()
            .eligible
    );
    assert!(set_enabled(&fixture.state, &fixture.home, OTHER, true).is_err());
    fixture.enable();
    for origin in [
        "https://github.com/paulovnas/jarvis.git.evil",
        "https://github.com.evil/paulovnas/jarvis.git",
        "https://github.com/paulovnas/jarvis.git?query=1",
        "https://attacker@github.com/paulovnas/jarvis.git",
        "git@github.com:paulovnas/jarvis.git/other",
        "file:///tmp/jarvis",
    ] {
        assert!(!official_origin(origin));
        git_ok(&fixture.root, &["remote", "set-url", "origin", origin]);
        assert!(!enabled_for_root(&fixture.home, &fixture.root, PROJECT));
    }
    set_enabled(&fixture.state, &fixture.home, PROJECT, false).unwrap();
}

#[test]
fn official_transport_aliases_and_display_version_changes_preserve_identity() {
    let fixture = Fixture::new();
    fixture.enable();
    for origin in [
        "https://github.com/paulovnas/jarvis",
        "ssh://git@github.com/paulovnas/jarvis.git",
    ] {
        git_ok(&fixture.root, &["remote", "set-url", "origin", origin]);
        assert!(enabled_for_root(&fixture.home, &fixture.root, PROJECT));
    }
    fs::write(
        fixture.root.join("package.json"),
        r#"{"name":"jarvis","version":"100.1.2"}"#,
    )
    .unwrap();
    fs::write(
        fixture.root.join("src-tauri/tauri.conf.json"),
        r#"{"identifier":"com.foxtag.jarvis","productName":"Jarvis Dev","version":"100.1.2"}"#,
    )
    .unwrap();
    assert!(enabled_for_root(&fixture.home, &fixture.root, PROJECT));
    fs::write(
        fixture.root.join("src-tauri/Cargo.toml"),
        "[package]\nname = \"lookalike\"\n",
    )
    .unwrap();
    assert!(!enabled_for_root(&fixture.home, &fixture.root, PROJECT));
}

#[test]
fn exact_roots_and_separately_enabled_worktrees_are_required() {
    let fixture = Fixture::new();
    fixture.enable();
    assert!(!enabled_for_root(
        &fixture.home,
        &fixture.root.join("src-tauri"),
        PROJECT
    ));
    git_ok(
        &fixture.root,
        &[
            "add",
            "package.json",
            "src-tauri/Cargo.toml",
            "src-tauri/tauri.conf.json",
        ],
    );
    git_ok(
        &fixture.root,
        &[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.test",
            "commit",
            "--quiet",
            "-m",
            "fixture",
        ],
    );
    let worktree = fixture.home.join("worktree");
    git_ok(
        &fixture.root,
        &[
            "worktree",
            "add",
            "--quiet",
            "--detach",
            worktree.to_str().unwrap(),
        ],
    );
    let worktree = fs::canonicalize(worktree).unwrap();
    fixture.add_project(&worktree, OTHER);
    assert!(
        status(&fixture.state, &fixture.home, OTHER)
            .unwrap()
            .eligible
    );
    assert!(!enabled_for_root(&fixture.home, &worktree, OTHER));
    set_enabled(&fixture.state, &fixture.home, OTHER, true).unwrap();
    assert!(enabled_for_root(&fixture.home, &worktree, OTHER));
    set_enabled(&fixture.state, &fixture.home, PROJECT, false).unwrap();
    assert!(!enabled_for_root(&fixture.home, &fixture.root, PROJECT));
    assert!(enabled_for_root(&fixture.home, &worktree, OTHER));
    assert!(!enabled_for_root(&fixture.home, &worktree, PROJECT));
    assert!(!enabled_for_root(&fixture.home, &fixture.root, OTHER));
}

#[test]
fn symlink_alias_resolves_to_same_root_but_redirected_manifest_is_ineligible() {
    #[cfg(unix)]
    {
        let fixture = Fixture::new();
        fixture.enable();
        let alias = fixture.home.join("alias");
        std::os::unix::fs::symlink(&fixture.root, &alias).unwrap();
        assert!(enabled_for_root(&fixture.home, &alias, PROJECT));
        let manifest = fixture.root.join("package.json");
        fs::remove_file(&manifest).unwrap();
        let elsewhere = fixture.home.join("package.json");
        fs::write(&elsewhere, r#"{"name":"jarvis"}"#).unwrap();
        std::os::unix::fs::symlink(elsewhere, manifest).unwrap();
        assert!(!enabled_for_root(&fixture.home, &fixture.root, PROJECT));
    }
}

#[test]
fn explicit_capture_is_sanitized_bounded_and_does_not_mutate_source() {
    let fixture = Fixture::new();
    fixture.enable();
    let other = fixture.home.join("ordinary-project");
    fs::create_dir(&other).unwrap();
    let other = fs::canonicalize(other).unwrap();
    fixture.add_project(&other, OTHER);
    let path = fixture.conversation(OTHER, &other, CONVERSATION);
    let before = fs::read(&path).unwrap();
    let modified = fs::metadata(&path).unwrap().modified().unwrap();
    let listed = sources(&fixture.state, &fixture.agent, &fixture.home, PROJECT).unwrap();
    assert_eq!(listed[0].id, CONVERSATION);
    let summary = capture(
        &fixture.state,
        &fixture.agent,
        &fixture.home,
        PROJECT,
        CONVERSATION,
        Some("Erro token=sk-private-token contact@example.test https://secret.example/test"),
    )
    .unwrap();
    assert_eq!(summary.event_count, 2);
    assert_eq!(summary.source_project_name, "Jarvis");
    assert_eq!(summary.source_status, "completed");
    let page =
        read_approved_incident(&fixture.home, &fixture.root, PROJECT, &summary.id, 0, 1).unwrap();
    assert_eq!(page.next_start, Some(1));
    assert_eq!(page.total, 2);
    let serialization = serde_json::to_string(
        &read_approved_incident(&fixture.home, &fixture.root, PROJECT, &summary.id, 0, 50).unwrap(),
    )
    .unwrap();
    for secret in [
        "RAW_CHAT_SECRET",
        "RAW_ASSISTANT_SECRET",
        "RAW_SUMMARY_SECRET",
        "RAW_ARGUMENT_SECRET",
        "RAW_TERMINAL_SECRET",
        "RAW_PROVIDER_SECRET",
        "RAW_ACCOUNT_SECRET",
        "sk-private",
        "contact@example",
        "secret.example",
        "private-model-secret",
        "private-turn-secret",
    ] {
        assert!(!serialization.contains(secret), "leaked {secret}");
    }
    assert_eq!(fs::read(&path).unwrap(), before);
    assert_eq!(fs::metadata(&path).unwrap().modified().unwrap(), modified);
    assert_eq!(
        fs::read_dir(path.parent().unwrap()).unwrap().count(),
        1,
        "capture wrote source sidecars"
    );
    fixture
        .state
        .with_connection(&fixture.home, |db| {
            let title: String = db.query_row(
                "SELECT title FROM conversations WHERE id=?1",
                [CONVERSATION],
                |row| row.get(0),
            )?;
            assert_eq!(title, "Incident source");
            Ok::<_, SelfDevelopmentError>(())
        })
        .unwrap();
}

#[test]
fn incidents_are_destination_bound_and_ids_are_never_conversations_or_paths() {
    let fixture = Fixture::new();
    fixture.enable();
    fixture.conversation(PROJECT, &fixture.root, CONVERSATION);
    let summary = capture(
        &fixture.state,
        &fixture.agent,
        &fixture.home,
        PROJECT,
        CONVERSATION,
        None,
    )
    .unwrap();
    let other = fixture.home.join("second-official-checkout");
    create_repo(&other, "git@github.com:paulovnas/jarvis.git");
    let other = fs::canonicalize(other).unwrap();
    fixture.add_project(&other, OTHER);
    set_enabled(&fixture.state, &fixture.home, OTHER, true).unwrap();
    assert!(read_approved_incident(&fixture.home, &other, OTHER, &summary.id, 0, 50).is_err());
    for id in [
        CONVERSATION,
        "arbitrary",
        "../authorization.json",
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    ] {
        assert!(read_approved_incident(&fixture.home, &fixture.root, PROJECT, id, 0, 50).is_err());
    }
    assert!(
        read_approved_incident(&fixture.home, &fixture.root, PROJECT, &summary.id, 0, 51).is_err()
    );
    assert!(
        read_approved_incident(&fixture.home, &fixture.root, PROJECT, &summary.id, 3, 1).is_err()
    );
}

#[test]
fn disabling_revokes_and_deletes_incidents_including_after_identity_change() {
    let fixture = Fixture::new();
    fixture.enable();
    fixture.conversation(PROJECT, &fixture.root, CONVERSATION);
    let summary = capture(
        &fixture.state,
        &fixture.agent,
        &fixture.home,
        PROJECT,
        CONVERSATION,
        None,
    )
    .unwrap();
    assert_eq!(
        list_approved_incidents(&fixture.home, &fixture.root, PROJECT)
            .unwrap()
            .len(),
        1
    );
    git_ok(
        &fixture.root,
        &[
            "remote",
            "set-url",
            "origin",
            "https://github.com/attacker/jarvis",
        ],
    );
    assert!(
        !set_enabled(&fixture.state, &fixture.home, PROJECT, false)
            .unwrap()
            .enabled
    );
    assert_eq!(
        fs::read_dir(directory(&fixture.home, false).unwrap())
            .unwrap()
            .count(),
        0
    );
    git_ok(
        &fixture.root,
        &[
            "remote",
            "set-url",
            "origin",
            "git@github.com:paulovnas/jarvis.git",
        ],
    );
    fixture.enable();
    assert!(
        read_approved_incident(&fixture.home, &fixture.root, PROJECT, &summary.id, 0, 50).is_err()
    );
}

#[test]
fn deleted_incident_is_immediately_unreadable() {
    let fixture = Fixture::new();
    fixture.enable();
    fixture.conversation(PROJECT, &fixture.root, CONVERSATION);
    let summary = capture(
        &fixture.state,
        &fixture.agent,
        &fixture.home,
        PROJECT,
        CONVERSATION,
        None,
    )
    .unwrap();
    let authorization = require_for_root(&fixture.home, &fixture.root, PROJECT).unwrap();
    fs::remove_file(incident_path(&fixture.home, &authorization, &summary.id).unwrap()).unwrap();
    assert!(
        read_approved_incident(&fixture.home, &fixture.root, PROJECT, &summary.id, 0, 50).is_err()
    );
}

#[test]
fn capture_limits_events_and_recursively_exports_only_shapes() {
    let mut tools = vec![];
    for index in 0..300 {
        tools.push(json!({"id":index.to_string(),"name":"mcp_private_secret","args":{"private_secret":"RAW_SECRET"},"status":"error","output":"{\"private\":\"RAW_SECRET\"}","durationMs":10}));
    }
    let snapshot = json!({"turns":[{"id":"turn","status":"error","durationMs":0,"options":{"model":"private"},"steps":[{"tools":tools}]}],"history":{"total":50}});
    let (events, truncated, status) = events(&snapshot);
    assert_eq!(events.len(), MAX_EVENTS);
    assert!(truncated);
    assert_eq!(status, "error");
    let serialized = serde_json::to_string(&events).unwrap();
    assert!(!serialized.contains("RAW_SECRET"));
    assert!(!serialized.contains("private_secret"));
    assert!(serialized.contains("external"));
}

#[test]
fn expired_incidents_are_not_returned() {
    let fixture = Fixture::new();
    fixture.enable();
    fixture.conversation(PROJECT, &fixture.root, CONVERSATION);
    let summary = capture(
        &fixture.state,
        &fixture.agent,
        &fixture.home,
        PROJECT,
        CONVERSATION,
        None,
    )
    .unwrap();
    let authorization = require_for_root(&fixture.home, &fixture.root, PROJECT).unwrap();
    let path = incident_path(&fixture.home, &authorization, &summary.id).unwrap();
    let mut incident: Incident = read_json(&path).unwrap();
    incident.summary.captured_at = now() - RETENTION_MS - 1;
    save_json(&path, &incident).unwrap();
    assert!(
        list_approved_incidents(&fixture.home, &fixture.root, PROJECT)
            .unwrap()
            .is_empty()
    );
    assert!(
        read_approved_incident(&fixture.home, &fixture.root, PROJECT, &summary.id, 0, 50).is_err()
    );
}

#[test]
fn retention_bounds_the_private_snapshot_count_and_removes_expired_data() {
    let fixture = Fixture::new();
    fixture.enable();
    fixture.conversation(PROJECT, &fixture.root, CONVERSATION);
    let summary = capture(
        &fixture.state,
        &fixture.agent,
        &fixture.home,
        PROJECT,
        CONVERSATION,
        None,
    )
    .unwrap();
    let authorization = require_for_root(&fixture.home, &fixture.root, PROJECT).unwrap();
    let mut incident: Incident =
        read_json(&incident_path(&fixture.home, &authorization, &summary.id).unwrap()).unwrap();
    for index in 0..25 {
        incident.summary.id = format!("{index:032x}");
        incident.summary.captured_at = if index == 0 {
            now() - RETENTION_MS - 1
        } else {
            now()
        };
        save_json(
            &incident_path(&fixture.home, &authorization, &incident.summary.id).unwrap(),
            &incident,
        )
        .unwrap();
    }
    capture(
        &fixture.state,
        &fixture.agent,
        &fixture.home,
        PROJECT,
        CONVERSATION,
        None,
    )
    .unwrap();
    assert_eq!(
        incident_paths(&fixture.home, &authorization).unwrap().len(),
        MAX_INCIDENTS
    );
    assert!(
        !incident_path(&fixture.home, &authorization, &"0".repeat(32))
            .unwrap()
            .exists()
    );
}

#[test]
fn structurally_summarizes_subagents_without_titles_prompts_or_error_messages() {
    let snapshot = json!({"turns":[],"subagents":[{
        "id":crate::agent::telemetry::context_id("worker"),
        "parentId":crate::agent::telemetry::context_id("root"),
        "status":"waiting","durationMs":123,
        "title":"RAW_AGENT_TITLE", "prompt":"RAW_AGENT_PROMPT", "error":"RAW_AGENT_ERROR",
        "options":{"executor":"claude","model":"private_worker_model","reasoning":"high"}
    }]});
    let (events, _, _) = events(&snapshot);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].status.label(), "waiting");
    let serialized = serde_json::to_string(&events).unwrap();
    assert!(!serialized.contains("RAW_AGENT"));
    assert!(!serialized.contains("private_worker_model"));
    assert!(serialized.contains("subagent"));
    assert!(serialized.contains("parentId"));
}

#[test]
fn private_snapshot_reader_rejects_symlinks_and_unknown_fields() {
    let fixture = Fixture::new();
    fixture.enable();
    fixture.conversation(PROJECT, &fixture.root, CONVERSATION);
    let summary = capture(
        &fixture.state,
        &fixture.agent,
        &fixture.home,
        PROJECT,
        CONVERSATION,
        None,
    )
    .unwrap();
    let authorization = require_for_root(&fixture.home, &fixture.root, PROJECT).unwrap();
    let path = incident_path(&fixture.home, &authorization, &summary.id).unwrap();
    let mut data: Value = read_json(&path).unwrap();
    data["rawContent"] = json!("RAW_INJECTED_CHAT_SECRET");
    save_json(&path, &data).unwrap();
    assert!(
        read_approved_incident(&fixture.home, &fixture.root, PROJECT, &summary.id, 0, 50).is_err()
    );
    #[cfg(unix)]
    {
        let actual = fixture.home.join("unrelated-private-file.json");
        fs::rename(&path, &actual).unwrap();
        std::os::unix::fs::symlink(actual, &path).unwrap();
        assert!(
            read_approved_incident(&fixture.home, &fixture.root, PROJECT, &summary.id, 0, 50)
                .is_err()
        );
    }
}
