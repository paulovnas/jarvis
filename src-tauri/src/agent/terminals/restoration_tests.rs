use super::*;
use std::fs;
#[cfg(unix)]
use std::time::Duration;

fn fixture() -> (tempfile::TempDir, PathBuf, AppState) {
    let home = tempfile::tempdir().unwrap();
    let root = home.path().join("project");
    fs::create_dir(&root).unwrap();
    let root = root.canonicalize().unwrap();
    let database = AppState::default();
    database
        .with_connection(home.path(), |connection| {
            connection.execute(
                "INSERT INTO workspaces(id,name) VALUES ('w','Workspace')",
                [],
            )?;
            connection.execute(
                "INSERT INTO projects(id,workspace_id,name,path) VALUES ('p','w','Project',?1)",
                [root.to_string_lossy()],
            )?;
            Ok::<_, crate::persistence::PersistenceError>(())
        })
        .unwrap();
    (home, root, database)
}

fn saved(root: &Path, command: Option<&str>, execution: tracking::Execution) -> persistence::Saved {
    persistence::Saved {
        info: ProjectTerminal {
            id: "saved-terminal".into(),
            project_id: "p".into(),
            conversation_id: Some("creator-chat".into()),
            title: "Frontend".into(),
            cwd: root.to_string_lossy().into_owned(),
            pid: 12345,
            started_at: 100,
            ended_at: None,
            exit_code: None,
            status: "running".into(),
            origin: TerminalOrigin::Agent,
            command: command.map(str::to_owned),
        },
        root: root.into(),
        cwd: root.into(),
        call_id: Some("spawn-call".into()),
        owner_id: Some("owner".into()),
        output: "retained output\r\n".into(),
        revision: 7,
        truncated: false,
        live: true,
        execution,
        sandbox: None,
        shell_program: None,
        shell_restorable: false,
        service_port: None,
    }
}

fn persist(home: &Path, database: AppState, saved: persistence::Saved) {
    let mut store = persistence::Store::default();
    store.configure(home, database).unwrap();
    store.save(vec![saved], true).unwrap();
}

#[test]
fn completed_migration_is_archived_without_replay_or_old_pid() {
    let (home, root, database) = fixture();
    let mut record = saved(&root, Some("npm run migrate"), tracking::Execution::Idle);
    record.live = false;
    record.info.status = "exited".into();
    record.info.exit_code = Some(0);
    record.info.ended_at = Some(200);
    persist(home.path(), database.clone(), record);
    let state = TerminalState::default();
    state.configure(home.path(), database).unwrap();
    state.restore(silent_events()).unwrap();
    let snapshot = state.snapshot("p", "saved-terminal").unwrap();
    assert_eq!(snapshot.terminal.pid, 0);
    assert_eq!(snapshot.terminal.exit_code, Some(0));
    assert_eq!(snapshot.output, "retained output\r\n");
    assert_eq!(snapshot.revision, 7);
    assert_eq!(
        snapshot.terminal.conversation_id.as_deref(),
        Some("creator-chat")
    );
    assert!(state.shutdown_activity().unwrap().is_empty());
    assert!(state
        .write("p", "saved-terminal", "npm run migrate\r")
        .is_err());
    state.restore(silent_events()).unwrap();
    assert_eq!(state.list("p").unwrap().len(), 1);
}

#[test]
fn deleted_projects_are_removed_but_invalid_directories_keep_archived_output() {
    for missing_project in [true, false] {
        let (home, root, database) = fixture();
        let mut record = saved(
            &root,
            Some("npm run dev"),
            tracking::Execution::Running {
                command: "npm run dev".into(),
                cwd: root.clone(),
            },
        );
        if missing_project {
            record.info.project_id = "deleted".into();
        } else {
            record.cwd = home.path().canonicalize().unwrap();
        }
        persist(home.path(), database.clone(), record);
        let state = TerminalState::default();
        state.configure(home.path(), database).unwrap();
        state.restore(silent_events()).unwrap();
        if missing_project {
            assert!(state.project_activity().unwrap().is_empty());
        } else {
            let snapshot = state.snapshot("p", "saved-terminal").unwrap();
            assert_eq!(snapshot.terminal.pid, 0);
            assert_eq!(snapshot.terminal.status, "exited");
            assert_eq!(snapshot.terminal.cwd, root.to_string_lossy());
            assert!(snapshot.output.starts_with("retained output\r\n"));
            assert!(snapshot.output.contains("O histórico foi preservado"));
        }
    }
}

#[test]
fn a_moved_project_keeps_terminal_history_without_running_the_old_service() {
    let (home, root, database) = fixture();
    let record = saved(
        &root,
        Some("npm run dev"),
        tracking::Execution::Running {
            command: "npm run dev".into(),
            cwd: root.clone(),
        },
    );
    persist(home.path(), database.clone(), record);
    let current = home.path().join("moved-project");
    fs::create_dir(&current).unwrap();
    let current = current.canonicalize().unwrap();
    database
        .with_connection(home.path(), |connection| {
            connection.execute(
                "UPDATE projects SET path=?1 WHERE id='p'",
                [current.to_string_lossy()],
            )?;
            Ok::<_, crate::persistence::PersistenceError>(())
        })
        .unwrap();
    let state = TerminalState::default();
    state.configure(home.path(), database).unwrap();
    state.restore(silent_events()).unwrap();
    let snapshot = state.snapshot("p", "saved-terminal").unwrap();
    assert_eq!(snapshot.terminal.pid, 0);
    assert_eq!(snapshot.terminal.cwd, current.to_string_lossy());
    assert!(snapshot.output.starts_with("retained output\r\n"));
    assert!(!state.has_running());
}

#[test]
fn an_unavailable_project_keeps_unresolved_history_until_its_directory_returns() {
    let (home, root, database) = fixture();
    let mut record = saved(&root, Some("npm run migrate"), tracking::Execution::Idle);
    record.live = false;
    record.info.status = "exited".into();
    persist(home.path(), database.clone(), record);
    let unavailable = home.path().join("temporarily-unavailable");
    fs::rename(&root, &unavailable).unwrap();
    let state = TerminalState::default();
    state.configure(home.path(), database.clone()).unwrap();
    state.restore(silent_events()).unwrap();
    assert!(state.list("p").unwrap().is_empty());
    state.checkpoint().unwrap();
    state.shutdown().unwrap();
    let path = crate::data_dir::root(home.path()).join("terminals/state.json");
    let checkpoint: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert_eq!(checkpoint["terminals"][0]["output"], "retained output\r\n");
    fs::rename(unavailable, &root).unwrap();
    let next = TerminalState::default();
    next.configure(home.path(), database).unwrap();
    next.restore(silent_events()).unwrap();
    let snapshot = next.snapshot("p", "saved-terminal").unwrap();
    assert_eq!(snapshot.terminal.pid, 0);
    assert_eq!(snapshot.output, "retained output\r\n");
}

#[test]
fn deleting_a_project_discards_unresolved_terminal_state_as_well() {
    let (home, root, database) = fixture();
    let record = saved(&root, None, tracking::Execution::Idle);
    persist(home.path(), database.clone(), record);
    fs::remove_dir(&root).unwrap();
    let state = TerminalState::default();
    state.configure(home.path(), database).unwrap();
    state.restore(silent_events()).unwrap();
    state.stop_project("p");
    let checkpoint: Value = serde_json::from_slice(
        &fs::read(crate::data_dir::root(home.path()).join("terminals/state.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(checkpoint["terminals"], json!([]));
}

#[test]
fn a_service_with_an_occupied_saved_port_is_archived_without_choosing_a_different_port() {
    let (home, root, database) = fixture();
    let socket = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = socket.local_addr().unwrap().port();
    let mut record = saved(
        &root,
        Some("npm run dev"),
        tracking::Execution::Running {
            command: "npm run dev".into(),
            cwd: root.clone(),
        },
    );
    record.service_port = Some(port);
    record.sandbox = Some(serde_json::from_value(json!({
        "report": {"backend":"native", "availability":"unavailable", "filesystemIsolated":false, "network":"native", "processTreeIsolated":true, "reason":null},
        "launcher": "native"
    })).unwrap());
    persist(home.path(), database.clone(), record);
    let state = TerminalState::default();
    state.configure(home.path(), database).unwrap();
    state.restore(silent_events()).unwrap();
    let snapshot = state.snapshot("p", "saved-terminal").unwrap();
    assert_eq!(snapshot.terminal.pid, 0);
    assert_eq!(snapshot.terminal.status, "failed");
    assert!(snapshot.output.contains("já está ocupada"));
    assert!(!state.has_running());
}

#[test]
fn corrupt_and_future_version_snapshots_are_preserved_on_load_failure() {
    let (home, _, database) = fixture();
    let directory = crate::data_dir::root(home.path()).join("terminals");
    fs::create_dir(&directory).unwrap();
    let path = directory.join("state.json");
    for contents in ["broken JSON", "{\"version\":999,\"terminals\":[]}"] {
        fs::write(&path, contents).unwrap();
        let state = TerminalState::default();
        state.configure(home.path(), database.clone()).unwrap();
        assert!(!path.exists());
        assert!(fs::read_dir(&directory)
            .unwrap()
            .filter_map(Result::ok)
            .any(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with("state.recovery.")
                    && fs::read_to_string(entry.path()).unwrap() == contents
            }));
    }
}

#[cfg(unix)]
#[test]
fn redirected_snapshot_files_are_rejected_without_changing_the_target() {
    let (home, _, database) = fixture();
    let directory = crate::data_dir::root(home.path()).join("terminals");
    fs::create_dir(&directory).unwrap();
    let target = home.path().join("private-file");
    fs::write(&target, "preserved").unwrap();
    std::os::unix::fs::symlink(&target, directory.join("state.json")).unwrap();
    let state = TerminalState::default();
    assert!(state.configure(home.path(), database).is_err());
    assert_eq!(fs::read_to_string(target).unwrap(), "preserved");
}

#[test]
fn an_agent_without_its_original_sandbox_is_archived_instead_of_elevated() {
    let (home, root, database) = fixture();
    let record = saved(
        &root,
        Some("npm run dev"),
        tracking::Execution::Running {
            command: "npm run dev".into(),
            cwd: root.clone(),
        },
    );
    persist(home.path(), database.clone(), record);
    let state = TerminalState::default();
    state.configure(home.path(), database).unwrap();
    state.restore(silent_events()).unwrap();
    let snapshot = state.snapshot("p", "saved-terminal").unwrap();
    assert_eq!(snapshot.terminal.pid, 0);
    assert_eq!(snapshot.terminal.status, "exited");
    assert_eq!(snapshot.output, "retained output\r\n");
}

#[test]
fn an_untracked_custom_shell_never_reexecutes_its_launch_script() {
    let (home, root, database) = fixture();
    let mut record = saved(&root, None, tracking::Execution::Unknown);
    record.info.origin = TerminalOrigin::User;
    record.shell_program = Some(root.join("custom-shell"));
    persist(home.path(), database.clone(), record);
    let state = TerminalState::default();
    state.set_preferences(crate::system::TerminalPreferences {
        shell: Some(root.join("missing-shell").to_string_lossy().into_owned()),
        arguments: vec!["-c".into(), "npm run migrate".into()],
        ..Default::default()
    });
    state.configure(home.path(), database).unwrap();
    state.restore(silent_events()).unwrap();
    let snapshot = state.snapshot("p", "saved-terminal").unwrap();
    assert_eq!(snapshot.terminal.pid, 0);
    assert_eq!(snapshot.terminal.status, "exited");
    assert_eq!(snapshot.output, "retained output\r\n");
}

#[cfg(unix)]
#[test]
fn a_scriptless_shell_still_initializing_reopens_without_inventing_a_command() {
    let (home, root, database) = fixture();
    let mut record = saved(&root, None, tracking::Execution::Unknown);
    record.info.origin = TerminalOrigin::User;
    record.shell_program = Some(PathBuf::from("/bin/bash"));
    record.shell_restorable = true;
    persist(home.path(), database.clone(), record);
    let state = TerminalState::default();
    state.configure(home.path(), database).unwrap();
    state.restore(silent_events()).unwrap();
    wait_execution(&state, "saved-terminal", false);
    let snapshot = state.snapshot("p", "saved-terminal").unwrap();
    assert!(snapshot.terminal.running());
    assert!(snapshot.output.starts_with("retained output\r\n"));
    assert!(snapshot.terminal.command.is_none());
    state.shutdown().unwrap();
}

#[test]
fn an_unavailable_saved_sandbox_never_falls_back_to_native_execution() {
    let (home, root, database) = fixture();
    let mut record = saved(
        &root,
        Some("npm run dev"),
        tracking::Execution::Running {
            command: "npm run dev".into(),
            cwd: root.clone(),
        },
    );
    record.sandbox = Some(serde_json::from_value(json!({
        "report": {"backend":"macosSeatbelt", "availability":"full", "filesystemIsolated":true, "network":"allowed", "processTreeIsolated":true, "reason":null},
        "launcher": {"seatbelt": {"executable":root.join("missing-sandbox"), "profile":"(version 1)\n(deny default)"}}
    })).unwrap());
    persist(home.path(), database.clone(), record);
    let state = TerminalState::default();
    state.configure(home.path(), database).unwrap();
    state.restore(silent_events()).unwrap();
    let snapshot = state.snapshot("p", "saved-terminal").unwrap();
    assert_eq!(snapshot.terminal.pid, 0);
    assert_eq!(snapshot.terminal.status, "failed");
    assert!(snapshot.output.starts_with("retained output\r\n"));
    assert!(!state.has_running());
}

#[cfg(unix)]
fn shell_preferences() -> crate::system::TerminalPreferences {
    crate::system::TerminalPreferences {
        shell: Some("/bin/bash".into()),
        arguments: vec!["--noprofile".into(), "-i".into()],
        ..Default::default()
    }
}

#[cfg(unix)]
fn open(state: &TerminalState, root: &Path) -> ProjectTerminal {
    state
        .spawn(
            Spawn {
                project: "p",
                conversation: None,
                root,
                title: Some("User terminal"),
                origin: TerminalOrigin::User,
                call_id: None,
                owner_id: None,
                initial_input: None,
                service: None,
            },
            silent_events(),
        )
        .unwrap()
}

#[cfg(unix)]
fn wait_execution(state: &TerminalState, id: &str, active: bool) {
    for _ in 0..300 {
        if state
            .0
            .lock()
            .unwrap()
            .get(id)
            .is_some_and(|entry| match &entry.execution {
                tracking::Execution::Running { .. } => active,
                tracking::Execution::Idle => !active,
                _ => false,
            })
        {
            return;
        }
        thread::sleep(Duration::from_millis(20));
    }
    panic!("shell did not confirm its execution state");
}

#[cfg(unix)]
#[test]
fn a_failed_installer_unfreezes_the_existing_shell_and_pending_input_prevents_shutdown() {
    let (home, root, database) = fixture();
    let state = TerminalState::default();
    state.configure(home.path(), database).unwrap();
    state.set_preferences(shell_preferences());
    let terminal = open(&state, &root);
    wait_execution(&state, &terminal.id, false);
    let guard = state.prepare_update_shutdown().unwrap();
    assert!(state.write("p", &terminal.id, "echo forbidden\r").is_err());
    assert!(state
        .snapshot("p", &terminal.id)
        .unwrap()
        .terminal
        .running());
    drop(guard);
    state
        .write("p", &terminal.id, "printf 'installer-%s\\n' recovered\r")
        .unwrap();
    super::tests::wait_for_text(&state, "p", &terminal.id, "installer-recovered\r\n");
    let runtime = state
        .0
        .lock()
        .unwrap()
        .get(&terminal.id)
        .unwrap()
        .runtime
        .clone()
        .unwrap();
    runtime.pending_writes.store(1, Ordering::SeqCst);
    assert!(state.shutdown().is_err());
    assert!(runtime.alive.load(Ordering::SeqCst));
    runtime.pending_writes.store(0, Ordering::SeqCst);
    state.shutdown().unwrap();
}

#[cfg(unix)]
#[test]
fn a_snapshot_write_failure_preserves_the_confirmed_spawn_receipt_without_repeating_the_command() {
    let (home, root, database) = fixture();
    let state = TerminalState::default();
    state.configure(home.path(), database).unwrap();
    let directory = state.3.lock().unwrap().directory.clone();
    let blocker = root.join("not-a-directory");
    fs::write(&blocker, "blocking file").unwrap();
    state.3.lock().unwrap().directory = Some(blocker);
    let command = "printf 'once\\n' >> run-count; printf 'confirmed-%s\\n' spawn";
    let request = || Spawn {
        project: "p",
        conversation: Some("chat"),
        root: &root,
        title: Some("Confirmed spawn"),
        origin: TerminalOrigin::Agent,
        call_id: Some("once"),
        owner_id: Some("owner"),
        initial_input: None,
        service: Some((command, None)),
    };
    let first = state
        .spawn_sandboxed(request(), None, silent_events())
        .unwrap();
    super::tests::wait_for_text(&state, "p", &first.id, "confirmed-spawn\r\n");
    assert!(state
        .snapshot("p", &first.id)
        .unwrap()
        .output
        .contains("não foi possível salvar sua sessão"));
    let retry = state
        .spawn_sandboxed(request(), None, silent_events())
        .unwrap();
    assert_eq!(retry.id, first.id);
    assert_eq!(
        fs::read_to_string(root.join("run-count"))
            .unwrap()
            .lines()
            .count(),
        1
    );
    state.3.lock().unwrap().directory = directory;
    state.shutdown().unwrap();
}

#[cfg(unix)]
#[test]
fn idle_shell_restores_output_native_directory_and_identity_without_replaying_a_migration() {
    let (home, root, database) = fixture();
    let cwd = root.join("ação front end");
    fs::create_dir(&cwd).unwrap();
    let state = TerminalState::default();
    state.configure(home.path(), database.clone()).unwrap();
    state.set_preferences(shell_preferences());
    let terminal = open(&state, &root);
    wait_execution(&state, &terminal.id, false);
    state
        .write("p", &terminal.id, "cd 'ação front end'\r")
        .unwrap();
    for _ in 0..200 {
        if state.snapshot("p", &terminal.id).unwrap().terminal.cwd == cwd.to_string_lossy() {
            break;
        }
        thread::sleep(Duration::from_millis(20));
    }
    state
        .write(
            "p",
            &terminal.id,
            "printf 'migration executed\\n' >> migration-count; printf 'migration-%s\\n' done\r",
        )
        .unwrap();
    super::tests::wait_for_text(&state, "p", &terminal.id, "migration-done\r\n");
    wait_execution(&state, &terminal.id, false);
    assert!(state
        .shutdown_activity()
        .unwrap()
        .iter()
        .all(|activity| !activity.active));
    state
        .rename("p", &terminal.id, "Persistent shell", &silent_events())
        .unwrap();
    state.shutdown().unwrap();
    let checkpoint =
        fs::read(crate::data_dir::root(home.path()).join("terminals/state.json")).unwrap();
    state.shutdown().unwrap();
    state.checkpoint().unwrap();
    assert_eq!(
        fs::read(crate::data_dir::root(home.path()).join("terminals/state.json")).unwrap(),
        checkpoint
    );
    assert!(state.write("p", &terminal.id, "echo forbidden\r").is_err());
    assert!(state
        .spawn(
            Spawn {
                project: "p",
                conversation: None,
                root: &root,
                title: None,
                origin: TerminalOrigin::User,
                call_id: None,
                owner_id: None,
                initial_input: None,
                service: None
            },
            silent_events()
        )
        .is_err());
    let restored = TerminalState::default();
    restored.set_preferences(crate::system::TerminalPreferences {
        shell: Some(root.join("missing-shell").to_string_lossy().into_owned()),
        arguments: vec!["-c".into(), "npm run migrate".into()],
        ..Default::default()
    });
    restored.configure(home.path(), database).unwrap();
    restored.restore(silent_events()).unwrap();
    wait_execution(&restored, &terminal.id, false);
    let snapshot = restored.snapshot("p", &terminal.id).unwrap();
    assert_eq!(snapshot.terminal.title, "Persistent shell");
    assert_eq!(snapshot.terminal.cwd, cwd.to_string_lossy());
    assert_eq!(snapshot.terminal.started_at, terminal.started_at);
    assert!(snapshot.output.contains("migration-done"));
    assert!(!snapshot.output.contains("777;jarvis"));
    assert_eq!(
        fs::read_to_string(cwd.join("migration-count"))
            .unwrap()
            .lines()
            .count(),
        1
    );
    restored.shutdown().unwrap();
}

#[cfg(unix)]
#[test]
fn manually_started_npm_development_service_restarts_once_and_keeps_its_retained_output() {
    let (home, root, database) = fixture();
    fs::write(
        root.join("package.json"),
        serde_json::json!({"scripts":{"dev":"node server.cjs"}}).to_string(),
    )
    .unwrap();
    fs::write(root.join("server.cjs"), "require('fs').appendFileSync('run-count','run\\n'); console.log('actual-service-ready'); setInterval(()=>{},1000);\n").unwrap();
    let state = TerminalState::default();
    state.configure(home.path(), database.clone()).unwrap();
    state.set_preferences(shell_preferences());
    let terminal = open(&state, &root);
    wait_execution(&state, &terminal.id, false);
    state.write("p", &terminal.id, "npm run dev\r").unwrap();
    super::tests::wait_for_text(&state, "p", &terminal.id, "actual-service-ready\r\n");
    wait_execution(&state, &terminal.id, true);
    let activity = state.shutdown_activity().unwrap();
    assert!(activity
        .iter()
        .any(|activity| activity.active && activity.restartable));
    state.shutdown().unwrap();
    // The exit generated by shutdown cannot turn a restart recipe into a finished command.
    for _ in 0..100 {
        if !state
            .snapshot("p", &terminal.id)
            .unwrap()
            .terminal
            .running()
        {
            break;
        }
        thread::sleep(Duration::from_millis(20));
    }
    let restored = TerminalState::default();
    restored.set_preferences(shell_preferences());
    restored.configure(home.path(), database).unwrap();
    restored.restore(silent_events()).unwrap();
    super::tests::wait_for_text_occurrences(
        &restored,
        "p",
        &terminal.id,
        "actual-service-ready\r\n",
        2,
    );
    wait_execution(&restored, &terminal.id, true);
    restored.restore(silent_events()).unwrap();
    assert_eq!(restored.list("p").unwrap().len(), 1);
    assert_eq!(
        fs::read_to_string(root.join("run-count"))
            .unwrap()
            .lines()
            .count(),
        2
    );
    assert_eq!(
        restored.snapshot("p", &terminal.id).unwrap().terminal.id,
        terminal.id
    );
    restored.shutdown().unwrap();
}
