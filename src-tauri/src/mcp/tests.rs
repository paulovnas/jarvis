use super::*;
use serde_json::json;
use std::{
    collections::HashMap,
    fs,
    path::PathBuf,
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
};
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    sync::watch,
};

#[derive(Default)]
struct MemorySecrets {
    values: Mutex<HashMap<String, String>>,
    fail: AtomicBool,
}
impl Secrets for MemorySecrets {
    fn load(&self, key: &str) -> Result<String, McpError> {
        self.values
            .lock()
            .unwrap()
            .get(key)
            .cloned()
            .ok_or_else(storage_error)
    }
    fn store(&self, key: &str, value: &str) -> Result<(), McpError> {
        if self.fail.load(Ordering::Relaxed) {
            return Err(storage_error());
        }
        self.values.lock().unwrap().insert(key.into(), value.into());
        Ok(())
    }
    fn delete(&self, key: &str) -> Result<(), McpError> {
        if self.fail.load(Ordering::Relaxed) {
            return Err(storage_error());
        }
        self.values.lock().unwrap().remove(key);
        Ok(())
    }
}
struct Fixture {
    home: PathBuf,
    state: AppState,
    mcp: McpState,
    secrets: Arc<MemorySecrets>,
}
impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let home = std::env::temp_dir().join(format!(
            "jarvis-mcp-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&home).unwrap();
        let secrets = Arc::new(MemorySecrets::default());
        Self {
            home,
            state: AppState::default(),
            mcp: McpState(Arc::new(Manager {
                guard: Mutex::new(()),
                secrets: secrets.clone(),
            })),
            secrets,
        }
    }
    fn local(&self, name: &str) -> Server {
        let raw = json!({name: {"type":"local", "command":["node", fixture_script()], "environment":{"TEST_SECRET":"fixture-sensitive-value", "CALLS_FILE": self.home.join("calls"), "PID_FILE": self.home.join(format!("{name}-pid"))}, "timeout":2000}}).to_string();
        self.mcp
            .save(&self.state, &self.home, None, &raw)
            .unwrap()
            .into_iter()
            .find(|s| s.name == name)
            .unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.home);
    }
}
fn fixture_script() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/mcp/fixtures/server.mjs")
}

#[tokio::test]
async fn discovery_names_survive_restart_and_stale_checks_do_not_overwrite_new_configuration() {
    let f = Fixture::new();
    let server = f.local("docs");
    let (_sender, signal) = watch::channel(false);
    let _clients = runtime::TurnClients::discover(&f.mcp, &f.state, &f.home, &f.home, signal)
        .await
        .unwrap();
    let reopened = McpState(Arc::new(Manager {
        guard: Mutex::new(()),
        secrets: f.secrets.clone(),
    }));
    let check = reopened
        .list(&AppState::default(), &f.home)
        .unwrap()
        .into_iter()
        .find(|item| item.id == server.id)
        .unwrap()
        .last_check
        .unwrap();
    assert_eq!(check.tool_count, check.tools.len());
    assert_eq!(check.tools.len(), 2);
    assert!(check
        .tools
        .iter()
        .all(|name| !name.contains("fixture-sensitive-value")));
    let raw = f.mcp.edit(&f.state, &f.home, &server.id).unwrap();
    f.mcp
        .save(&f.state, &f.home, Some(&server.id), &raw)
        .unwrap();
    f.mcp.record_check(&f.state, &f.home, &server, check);
    assert!(f
        .mcp
        .list(&f.state, &f.home)
        .unwrap()
        .into_iter()
        .find(|item| item.id == server.id)
        .unwrap()
        .last_check
        .is_none());
}

#[test]
fn seeded_template_never_connects_and_deleting_it_is_persistent() {
    let f = Fixture::new();
    let servers = f.mcp.list(&f.state, &f.home).unwrap();
    assert_eq!(servers.len(), 1);
    assert!(servers[0].enabled);
    assert!(!servers[0].configured);
    assert!(f.mcp.active_configs(&f.state, &f.home).unwrap().is_empty());
    assert!(f.secrets.values.lock().unwrap().is_empty());
    f.mcp.remove(&f.state, &f.home, &servers[0].id).unwrap();
    let new_state = AppState::default();
    assert!(f.mcp.list(&new_state, &f.home).unwrap().is_empty());
}

#[test]
fn secure_configuration_survives_toggle_and_restart_and_is_removed_on_delete() {
    let f = Fixture::new();
    let server = f.local("docs");
    let original = f.mcp.edit(&f.state, &f.home, &server.id).unwrap();
    f.mcp
        .set_enabled(&f.state, &f.home, &server.id, false)
        .unwrap();
    assert!(f.mcp.active_configs(&f.state, &f.home).unwrap().is_empty());
    let reopened = AppState::default();
    let raw = f.mcp.edit(&reopened, &f.home, &server.id).unwrap();
    assert!(!config::parse(&raw).unwrap().1.enabled());
    assert!(raw.contains("fixture-sensitive-value"));
    assert!(!String::from_utf8_lossy(
        &fs::read(crate::persistence::database_path(&f.home)).unwrap()
    )
    .contains("fixture-sensitive-value"));
    assert!(
        !serde_json::to_string(&f.mcp.list(&reopened, &f.home).unwrap())
            .unwrap()
            .contains("fixture-sensitive-value")
    );
    f.mcp
        .set_enabled(&reopened, &f.home, &server.id, true)
        .unwrap();
    assert_eq!(
        f.mcp.edit(&reopened, &f.home, &server.id).unwrap(),
        original
    );
    f.mcp.remove(&reopened, &f.home, &server.id).unwrap();
    assert!(f.secrets.values.lock().unwrap().is_empty());
}

#[test]
fn duplicate_names_and_keychain_failures_preserve_existing_configuration() {
    let f = Fixture::new();
    let server = f.local("docs");
    let original = f.mcp.edit(&f.state, &f.home, &server.id).unwrap();
    assert!(f.mcp.save(&f.state, &f.home, None, &original).is_err());
    f.secrets.fail.store(true, Ordering::Relaxed);
    assert!(f
        .mcp
        .save(
            &f.state,
            &f.home,
            Some(&server.id),
            &original.replace("docs", "new_name")
        )
        .is_err());
    assert!(f.mcp.remove(&f.state, &f.home, &server.id).is_err());
    assert_eq!(f.mcp.edit(&f.state, &f.home, &server.id).unwrap(), original);
    f.secrets.fail.store(false, Ordering::Relaxed);
    let updated = f
        .mcp
        .save(
            &f.state,
            &f.home,
            Some(&server.id),
            &original.replace("docs", "renamed"),
        )
        .unwrap();
    assert!(updated
        .iter()
        .any(|s| s.id == server.id && s.name == "renamed" && s.revision == 2));
    assert_eq!(f.secrets.values.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn stdio_discovery_dispatch_policy_redaction_and_stale_config() {
    let f = Fixture::new();
    let server = f.local("docs");
    let (_sender, signal) = watch::channel(false);
    let mut clients =
        runtime::TurnClients::discover(&f.mcp, &f.state, &f.home, &f.home, signal.clone())
            .await
            .unwrap();
    let plan = clients.definitions(&f.mcp, &f.state, &f.home, true).await;
    assert_eq!(plan.len(), 1);
    let all = clients.definitions(&f.mcp, &f.state, &f.home, false).await;
    assert_eq!(all.len(), 2);
    let lookup = plan[0]["name"].as_str().unwrap();
    let mutation = all.iter().find(|t| t["name"] != lookup).unwrap()["name"]
        .as_str()
        .unwrap();
    assert!(clients
        .execute(
            &f.mcp,
            &f.state,
            &f.home,
            mutation,
            &json!({}),
            true,
            signal.clone()
        )
        .await
        .is_err());
    assert!(clients
        .execute(
            &f.mcp,
            &f.state,
            &f.home,
            lookup,
            &json!({"query":42}),
            false,
            signal.clone()
        )
        .await
        .is_err());
    assert!(!f.home.join("calls").exists());
    let text = clients
        .execute(
            &f.mcp,
            &f.state,
            &f.home,
            lookup,
            &json!({"query":"React"}),
            true,
            signal.clone(),
        )
        .await
        .unwrap();
    assert!(text.contains("React") && text.contains("fixture"));
    assert!(!text.contains("fixture-sensitive-value"));
    f.mcp
        .set_enabled(&f.state, &f.home, &server.id, false)
        .unwrap();
    assert!(clients
        .execute(
            &f.mcp,
            &f.state,
            &f.home,
            lookup,
            &json!({"query":"React"}),
            true,
            signal.clone()
        )
        .await
        .is_err());
    assert_eq!(
        fs::read_to_string(f.home.join("calls"))
            .unwrap()
            .lines()
            .count(),
        1
    );
    assert!(clients
        .definitions(&f.mcp, &f.state, &f.home, false)
        .await
        .is_empty());
}

#[tokio::test]
async fn failed_server_is_isolated_and_cancellation_stops_stdio_process() {
    let f = Fixture::new();
    f.local("docs");
    f.mcp
        .save(
            &f.state,
            &f.home,
            None,
            r#"{"broken":{"type":"local","command":["/nonexistent-jarvis-mcp"]}}"#,
        )
        .unwrap();
    let (sender, signal) = watch::channel(false);
    let mut clients =
        runtime::TurnClients::discover(&f.mcp, &f.state, &f.home, &f.home, signal.clone())
            .await
            .unwrap();
    let definitions = clients.definitions(&f.mcp, &f.state, &f.home, true).await;
    assert_eq!(definitions.len(), 1);
    assert!(f
        .mcp
        .list(&f.state, &f.home)
        .unwrap()
        .iter()
        .find(|s| s.name == "broken")
        .unwrap()
        .last_check
        .as_ref()
        .unwrap()
        .error
        .is_some());
    let lookup = definitions[0]["name"].as_str().unwrap();
    let cancel = async {
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        sender.send(true).unwrap();
    };
    let args = json!({"query":"hang"});
    let (result, _) = tokio::join!(
        clients.execute(&f.mcp, &f.state, &f.home, lookup, &args, false, signal),
        cancel
    );
    assert!(result.unwrap_err().message.contains("interrompida"));
    let pid: i32 = fs::read_to_string(f.home.join("docs-pid"))
        .unwrap()
        .parse()
        .unwrap();
    drop(clients);
    #[cfg(unix)]
    tokio::time::timeout(std::time::Duration::from_secs(8), async {
        // Test-only probe for the process ID published by our disposable fixture.
        while unsafe { libc::kill(pid, 0) } == 0 {
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn streamable_http_supports_headers_and_real_protocol_dispatch() {
    let f = Fixture::new();
    let mut process = tokio::process::Command::new("node")
        .arg(fixture_script())
        .arg("--http")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let mut port = String::new();
    BufReader::new(process.stdout.take().unwrap())
        .read_line(&mut port)
        .await
        .unwrap();
    let port: u16 = port.trim().parse().unwrap();
    let raw = json!({"remote":{"type":"remote", "url":format!("http://127.0.0.1:{port}/mcp"),"headers":{"Authorization":"Bearer fixture-token"},"oauth":false}}).to_string();
    f.mcp.save(&f.state, &f.home, None, &raw).unwrap();
    let (_sender, signal) = watch::channel(false);
    let mut clients =
        runtime::TurnClients::discover(&f.mcp, &f.state, &f.home, &f.home, signal.clone())
            .await
            .unwrap();
    let definitions = clients.definitions(&f.mcp, &f.state, &f.home, true).await;
    assert_eq!(definitions.len(), 1);
    let output = clients
        .execute(
            &f.mcp,
            &f.state,
            &f.home,
            definitions[0]["name"].as_str().unwrap(),
            &json!({"query":"HTTP"}),
            true,
            signal,
        )
        .await
        .unwrap();
    assert!(output.contains("HTTP"));
    drop(clients);
    process.kill().await.unwrap();
}

#[tokio::test]
#[ignore = "Starts the public Context7 package with the host runtime; tools/list only, no stored credentials"]
async fn live_context7_with_gui_runtime_path() {
    let fixture = Fixture::new();
    let raw = json!({"context7": {"type":"local", "command":["npx", "-y", "@upstash/context7-mcp"], "timeout":60000}}).to_string();
    let server = fixture.mcp.save(&fixture.state, &fixture.home, Some("builtin-context7"), &raw).unwrap().into_iter().find(|server| server.name == "context7").unwrap();
    let mut config = fixture.mcp.config(&server).unwrap();
    let home = PathBuf::from(std::env::var_os("HOME").unwrap());
    if let Config::Local { environment, .. } = &mut config {
        let path = executable::search_path(std::ffi::OsStr::new("/usr/bin:/bin:/usr/sbin:/sbin"), &home, &[Path::new("/opt/homebrew"), Path::new("/usr/local")]);
        environment.insert("PATH".into(), path.to_string_lossy().into_owned());
    }
    let (_send, signal) = watch::channel(false);
    let mut client = runtime::connect(server, config, &fixture.home, signal).await.expect("Context7 discovery failed");
    assert!(client.tool_count() > 0);
    println!("Context7 discovery passed: {} tools", client.tool_count());
    client.close().await;
}
