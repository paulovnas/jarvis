use super::*;

const PROJECT: &str = "0123456789abcdef0123456789abcdef";
const SESSION: &str = "fedcba9876543210fedcba9876543210";

fn fixture(home: &Path) -> Beads {
    Beads {
        package: home.join("package"),
        home: home.into(),
        project: PROJECT.into(),
        session: SESSION.into(),
        plan: false,
    }
}
fn store(beads: &Beads) {
    fs::create_dir_all(beads.workspace().join(".beads/embeddeddolt")).unwrap();
    fs::create_dir_all(beads.workspace().join("host")).unwrap();
    fs::write(beads.workspace().join(".beads/metadata.json"), json!({"database":"dolt", "backend":"dolt", "dolt_mode":"embedded", "dolt_database":beads.prefix()}).to_string()).unwrap();
}

#[test]
fn strict_tools_reject_cross_project_ids_invalid_fields_and_plan_writes() {
    let home = tempfile::tempdir().unwrap();
    let beads = fixture(home.path());
    let prefix = beads.prefix();
    let parse = |name, args: Value, plan| tools::parse(name, &args, plan, &prefix, SESSION, "call");
    assert_eq!(definitions(false).len(), 8);
    assert_eq!(definitions(true).len(), 3);
    for name in [
        "beads_create",
        "beads_update",
        "beads_claim",
        "beads_close",
        "beads_dependency",
    ] {
        assert!(needs_approval(name));
        assert!(parse(name, json!({}), true).is_err());
    }
    for args in [
        json!({"id":"external-task"}),
        json!({"id":format!("{prefix}-../../x")}),
        json!({"id":format!("{prefix}-a"), "command":"rm"}),
    ] {
        assert!(parse("beads_show", args, false).is_err());
    }
    for args in [
        json!({"limit":0}),
        json!({"limit":51}),
        json!({"status":"bogus"}),
        json!({"query":"\u{0000}"}),
    ] {
        assert!(parse("beads_list", args, false).is_err());
    }
    let id = format!("{prefix}-abc");
    assert!(parse("beads_update", json!({"id":id}), false).is_err());
    assert!(parse("beads_close", json!({"id":id,"reason":" "}), false).is_err());
    assert!(parse(
        "beads_dependency",
        json!({"id":id,"depends_on":id,"action":"add"}),
        false
    )
    .is_err());
    assert!(!parse("beads_ready", json!({}), true).unwrap().write);
}

#[test]
fn providers_can_omit_or_null_unused_fields_without_inventing_parent_ids() {
    assert!(definitions(false)
        .iter()
        .all(|tool| tool["strict"] == false));
    let args = json!({"parent":null,"query":null,"limit":null,"status":null});
    let call = tools::parse("beads_list", &args, false, "jproject", SESSION, "query").unwrap();
    assert!(!call.args.iter().any(|arg| arg.starts_with("--parent=")));
    let create =
        json!({"title":"Task","description":"Test","parent":null,"priority":null,"type":null});
    assert!(tools::parse(
        "beads_create",
        &create,
        false,
        "jproject",
        SESSION,
        "create"
    )
    .is_ok());
    assert!(tools::parse(
        "beads_show",
        &json!({"id":null}),
        false,
        "jproject",
        SESSION,
        "show"
    )
    .is_err());
}

#[test]
fn create_is_retry_stable_and_text_is_one_argument() {
    let args = json!({"title":"--help $(touch file)","description":"line one\nline two"});
    let first = tools::parse("beads_create", &args, false, "jproject", SESSION, "call").unwrap();
    let same = tools::parse("beads_create", &args, false, "jproject", SESSION, "call").unwrap();
    let next = tools::parse("beads_create", &args, false, "jproject", SESSION, "next").unwrap();
    assert_eq!(first.created_id, same.created_id);
    assert_ne!(first.created_id, next.created_id);
    assert!(first.args.contains(&"--title=--help $(touch file)".into()));
    assert!(first
        .args
        .contains(&"--description=line one\nline two".into()));
}

#[test]
fn child_environment_is_private_and_cannot_inherit_tracker_routing() {
    let workspace = Path::new("/private/store");
    let command = process::command(Path::new("/private/package"), workspace, SESSION);
    let env: std::collections::HashMap<_, _> = command
        .as_std()
        .get_envs()
        .map(|(k, v)| {
            (
                k.to_string_lossy().into_owned(),
                v.unwrap().to_string_lossy().into_owned(),
            )
        })
        .collect();
    // Build the expected paths the same way the product does: Path::join uses
    // the native separator, so this stays correct on Windows and Unix alike.
    assert_eq!(env["BEADS_DIR"], workspace.join(".beads").to_string_lossy());
    assert_eq!(env["HOME"], workspace.join("host").to_string_lossy());
    assert_eq!(env["BD_DOLT_MODE"], "embedded");
    assert_eq!(env["BEADS_DOLT_AUTO_START"], "0");
    assert!(!env.contains_key("BEADS_DOLT_SERVER_HOST"));
    assert!(!env.contains_key("GIT_DIR"));
}

#[test]
fn malformed_or_redirected_store_is_preserved_and_rejected() {
    let home = tempfile::tempdir().unwrap();
    let beads = fixture(home.path());
    assert!(!beads.validate_store().unwrap());
    store(&beads);
    assert!(beads.validate_store().unwrap());
    let metadata = beads.workspace().join(".beads/metadata.json");
    fs::write(&metadata, "{").unwrap();
    assert!(beads.validate_store().is_err());
    assert_eq!(fs::read_to_string(&metadata).unwrap(), "{");
    store(&beads);
    fs::write(beads.workspace().join(".beads/redirect"), "/external").unwrap();
    assert!(beads.validate_store().is_err());
}

#[cfg(unix)]
#[test]
fn broken_store_symlink_is_not_treated_as_an_empty_database() {
    let home = tempfile::tempdir().unwrap();
    let beads = fixture(home.path());
    fs::create_dir_all(storage(home.path(), PROJECT)).unwrap();
    std::os::unix::fs::symlink(home.path().join("missing"), beads.workspace()).unwrap();
    assert!(beads.validate_store().is_err());
}

#[cfg(unix)]
#[tokio::test]
async fn process_cancellation_reaps_child_and_oversized_output_is_an_error() {
    let directory = tempfile::tempdir().unwrap();
    let pid_file = directory.path().join("child.pid");
    let mut command = tokio::process::Command::new("/bin/sh");
    command
        .args(["-c", "echo $$ > \"$1\"; exec sleep 30", "beads-test"])
        .arg(&pid_file);
    let (cancel, signal) = watch::channel(false);
    let running = tokio::spawn(process::run(command, signal));
    let pid = tokio::time::timeout(std::time::Duration::from_secs(3), async {
        loop {
            // File creation precedes echo writing its PID; wait for the complete
            // readiness signal instead of racing an empty file under load.
            if let Some(pid) = fs::read_to_string(&pid_file)
                .ok()
                .and_then(|contents| contents.trim().parse::<i32>().ok())
            {
                break pid;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    cancel.send(true).unwrap();
    assert!(
        tokio::time::timeout(std::time::Duration::from_secs(3), running)
            .await
            .unwrap()
            .unwrap()
            .is_err()
    );
    // SAFETY: signal 0 probes the recorded child without sending a signal.
    assert_eq!(unsafe { libc::kill(pid, 0) }, -1);
    let mut output = tokio::process::Command::new("/bin/sh");
    output.args(["-c", "head -c 2097153 /dev/zero"]);
    let (_cancel, signal) = watch::channel(false);
    assert!(process::run(output, signal)
        .await
        .unwrap_err()
        .message
        .contains("excedeu o limite"));
}

#[tokio::test]
async fn empty_reads_are_lazy_and_deleted_projects_cannot_be_resurrected() {
    let home = tempfile::tempdir().unwrap();
    let beads = fixture(home.path());
    let (_cancel, signal) = watch::channel(false);
    let value = beads
        .execute("beads_ready", &json!({}), "read", signal.clone(), || Ok(()))
        .await
        .unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&value).unwrap()["initialized"],
        false
    );
    assert!(!beads.workspace().exists());
    let err = beads
        .execute(
            "beads_create",
            &json!({"title":"Task","description":"Description"}),
            "create",
            signal,
            || Err(failure("deleted")),
        )
        .await
        .unwrap_err();
    assert_eq!(err.message, "deleted");
    assert!(!beads.workspace().exists());
}

#[tokio::test]
async fn cancellation_and_deletion_respect_the_shared_project_lock() {
    let home = tempfile::tempdir().unwrap();
    let beads = fixture(home.path());
    store(&beads);
    let (cancel, signal) = watch::channel(false);
    let lock = beads.lock(signal.clone()).await.unwrap();
    assert!(cleanup_project(home.path(), PROJECT).is_err());
    assert!(beads.workspace().exists());
    cancel.send(true).unwrap();
    assert!(beads.lock(signal).await.is_err());
    drop(lock);
    cleanup_project(home.path(), PROJECT).unwrap();
    assert!(!storage(home.path(), PROJECT).exists());
}

// Two conversations in one project share the tracker lock. A contended try-lock
// must wait and then succeed — not fail fast — so the plan sidebar never shows a
// spurious "could not lock" error that only clears by luck on a later attempt.
#[tokio::test]
async fn a_contended_project_lock_waits_for_release_then_succeeds() {
    let home = tempfile::tempdir().unwrap();
    store(&fixture(home.path()));
    let (_cancel, signal) = watch::channel(false);
    let held = fixture(home.path()).lock(signal.clone()).await.unwrap();

    // The second lock contends the same file and must stay pending, not error.
    let waiter = fixture(home.path());
    let pending = tokio::spawn(async move { waiter.lock(signal).await });
    tokio::time::sleep(std::time::Duration::from_millis(120)).await;
    assert!(
        !pending.is_finished(),
        "a contended lock must wait, not resolve early"
    );

    drop(held);
    let second = tokio::time::timeout(std::time::Duration::from_secs(5), pending)
        .await
        .expect("lock released within timeout")
        .expect("waiter task joined");
    assert!(
        second.is_ok(),
        "the waiting lock must succeed once released"
    );
}

async fn call(beads: &Beads, name: &str, args: Value, id: &str) -> Value {
    let (_cancel, signal) = watch::channel(false);
    let result = beads
        .execute(name, &args, id, signal, || Ok(()))
        .await
        .unwrap_or_else(|e| panic!("{name}: {}", e.message));
    serde_json::from_str(&result).unwrap()
}
fn task(value: &Value) -> &Value {
    value
        .as_array()
        .and_then(|rows| rows.first())
        .unwrap_or(value)
}

#[tokio::test]
#[ignore = "Uses the installed private Beads binary against an isolated temporary home"]
async fn installed_beads_lifecycle_smoke() {
    let home = tempfile::tempdir().unwrap();
    let host = PathBuf::from(
        std::env::var_os("HOME")
            .or_else(|| std::env::var_os("USERPROFILE"))
            .unwrap(),
    );
    let package = super::super::installed(&host, ComponentId::Beads)
        .unwrap()
        .path(&host)
        .unwrap();
    let mut beads = fixture(home.path());
    beads.package = package;
    let epic = call(
        &beads,
        "beads_create",
        json!({"title":"Synthetic epic","description":"No project files involved","type":"epic"}),
        "epic",
    )
    .await;
    let epic_id = task(&epic)["id"].as_str().unwrap();
    let first = call(
        &beads,
        "beads_create",
        json!({"title":"First","description":"First synthetic task","parent":epic_id}),
        "first",
    )
    .await;
    let a = task(&first)["id"].as_str().unwrap();
    let again = call(
        &beads,
        "beads_create",
        json!({"title":"First","description":"First synthetic task","parent":epic_id}),
        "first",
    )
    .await;
    assert_eq!(task(&again)["id"], a);
    let second = call(
        &beads,
        "beads_create",
        json!({"title":"Second","description":"Second synthetic task","parent":epic_id}),
        "second",
    )
    .await;
    let b = task(&second)["id"].as_str().unwrap();
    call(
        &beads,
        "beads_dependency",
        json!({"id":b,"depends_on":a,"action":"add"}),
        "dep",
    )
    .await;
    let ready = call(&beads, "beads_ready", json!({"parent":epic_id}), "ready").await;
    assert!(ready["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .any(|v| v["id"] == a));
    assert!(!ready["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .any(|v| v["id"] == b));
    call(&beads, "beads_claim", json!({"id":a}), "claim").await;
    let mut other_conversation = fixture(home.path());
    other_conversation.package = beads.package.clone();
    other_conversation.session = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".into();
    let (_cancel, signal) = watch::channel(false);
    assert!(other_conversation
        .execute("beads_claim", &json!({"id":a}), "conflict", signal, || Ok(
            ()
        ))
        .await
        .is_err());
    let (_cancel, signal) = watch::channel(false);
    assert!(beads
        .execute(
            "beads_close",
            &json!({"id":b,"reason":"Must not override blocking work"}),
            "blocked-close",
            signal,
            || Ok(())
        )
        .await
        .is_err());
    call(
        &beads,
        "beads_update",
        json!({"id":a,"notes":"Validated synthetic progress"}),
        "update",
    )
    .await;
    let shown = call(&beads, "beads_show", json!({"id":a}), "show").await;
    assert_eq!(task(&shown)["status"], "in_progress");
    assert_eq!(task(&shown)["notes"], "Validated synthetic progress");
    call(
        &beads,
        "beads_close",
        json!({"id":a,"reason":"Synthetic behavior checked"}),
        "close",
    )
    .await;
    let ready = call(&beads, "beads_ready", json!({"parent":epic_id}), "ready2").await;
    assert!(ready["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .any(|v| v["id"] == b));
    let mut resumed = fixture(home.path());
    resumed.package = beads.package.clone();
    resumed.session = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into();
    let (_cancel, signal) = watch::channel(false);
    assert!(resumed.resume(signal, || Ok(())).await.unwrap().contains(b));
    assert_eq!(
        task(&call(&resumed, "beads_show", json!({"id":a}), "reopen").await)["status"],
        "closed"
    );
    resumed.project = "0123456789abcdef0123456789abcdee".into();
    assert!(
        call(&resumed, "beads_list", json!({}), "other").await["tasks"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let (_cancel, signal) = watch::channel(false);
    assert!(resumed
        .execute("beads_show", &json!({"id":a}), "foreign", signal, || Ok(()))
        .await
        .is_err());
    cleanup_project(home.path(), PROJECT).unwrap();
    assert!(!beads.workspace().exists());
}
