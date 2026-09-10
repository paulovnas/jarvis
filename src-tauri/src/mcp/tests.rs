use super::*;
use serde_json::{json, Value};
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
    loads: AtomicU64,
}
impl Secrets for MemorySecrets {
    fn load(&self, key: &str) -> Result<String, McpError> {
        self.loads.fetch_add(1, Ordering::Relaxed);
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
        self.local_with_request_timeout(name, 2000)
    }
    fn local_with_request_timeout(&self, name: &str, request_timeout: u64) -> Server {
        self.local_with_tools(name, request_timeout, 0)
    }
    fn local_with_tools(&self, name: &str, request_timeout: u64, extra_tools: usize) -> Server {
        let raw = json!({name: {"type":"local", "command":["node", fixture_script()], "environment":{"TEST_SECRET":"fixture-sensitive-value", "CALLS_FILE": self.home.join("calls"), "STARTS_FILE": self.home.join("starts"), "SERVER_NAME": name, "PID_FILE": self.home.join(format!("{name}-pid")), "EXTRA_TOOLS":extra_tools.to_string()}, "timeout":2000, "requestTimeout":request_timeout}}).to_string();
        self.mcp
            .save(&self.state, &self.home, None, &raw)
            .unwrap()
            .into_iter()
            .find(|s| s.name == name)
            .unwrap()
    }
    fn local_with_dynamic_tools(
        &self,
        name: &str,
        request_timeout: u64,
        extra_tools: usize,
    ) -> (Server, PathBuf) {
        let tools_file = self.home.join(format!("{name}-tools"));
        fs::write(&tools_file, extra_tools.to_string()).unwrap();
        let raw = json!({name: {"type":"local", "command":["node", fixture_script()], "environment":{"TEST_SECRET":"fixture-sensitive-value", "CALLS_FILE": self.home.join("calls"), "STARTS_FILE": self.home.join("starts"), "SERVER_NAME": name, "PID_FILE": self.home.join(format!("{name}-pid")), "EXTRA_TOOLS_FILE":&tools_file}, "timeout":2000, "requestTimeout":request_timeout}}).to_string();
        let server = self
            .mcp
            .save(&self.state, &self.home, None, &raw)
            .unwrap()
            .into_iter()
            .find(|server| server.name == name)
            .unwrap();
        (server, tools_file)
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

fn explicit_mcp_evaluation_case() -> Value {
    serde_json::from_str(include_str!(
        "../agent/fixtures/evaluations/movarte-explicit-mcp.json"
    ))
    .unwrap()
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
fn context7_core_does_not_create_a_user_mcp_registration() {
    let f = Fixture::new();
    let servers = f.mcp.list(&f.state, &f.home).unwrap();
    assert!(servers.is_empty());
    assert!(f.mcp.active_configs(&f.state, &f.home).unwrap().is_empty());
    assert!(f.secrets.values.lock().unwrap().is_empty());
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
fn backup_configs_replace_the_catalog_and_keep_secrets_in_secure_storage() {
    let f = Fixture::new();
    let first = f.local("docs");
    f.mcp
        .set_enabled(&f.state, &f.home, &first.id, false)
        .unwrap();
    let exported = f.mcp.backup_configs(&f.state, &f.home).unwrap();
    assert_eq!(exported.len(), 1);
    assert!(exported[0].contains("fixture-sensitive-value"));
    assert!(exported[0].contains("\"enabled\": false"));

    f.mcp
        .replace_from_backup(
            &f.state,
            &f.home,
            &[json!({"other": {"type":"local", "command":["node", "server.js"], "environment":{"TOKEN":"restored-secret"}, "enabled":true}}).to_string()],
            &["builtin:planned/planner".into()],
        )
        .unwrap();

    let servers = f.mcp.list(&f.state, &f.home).unwrap();
    assert_eq!(servers.len(), 1);
    assert_eq!(servers[0].name, "other");
    assert!(f
        .mcp
        .edit(&f.state, &f.home, &servers[0].id)
        .unwrap()
        .contains("restored-secret"));
    assert_eq!(f.secrets.values.lock().unwrap().len(), 1);
}

#[test]
fn invalid_backup_mcp_set_preserves_the_current_catalog() {
    let f = Fixture::new();
    let server = f.local("docs");
    let duplicate = f.mcp.edit(&f.state, &f.home, &server.id).unwrap();

    assert!(f
        .mcp
        .replace_from_backup(&f.state, &f.home, &[duplicate.clone(), duplicate], &[])
        .is_err());
    assert_eq!(f.mcp.list(&f.state, &f.home).unwrap().len(), 1);
    assert_eq!(f.mcp.list(&f.state, &f.home).unwrap()[0].id, server.id);
}

#[test]
fn duplicate_names_and_failed_secret_updates_preserve_existing_configuration() {
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

#[test]
fn keychain_cleanup_failure_does_not_block_mcp_removal() {
    let f = Fixture::new();
    let server = f.local("context7");
    f.secrets.fail.store(true, Ordering::Relaxed);

    assert!(f.mcp.remove(&f.state, &f.home, &server.id).is_ok());
    assert!(f.mcp.list(&f.state, &f.home).unwrap().is_empty());
    // The inaccessible old Keychain item can remain orphaned, but it no longer
    // has a database registration and therefore cannot be loaded or executed.
    assert_eq!(f.secrets.values.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn harness_evaluation_explicit_mcp_connects_only_the_named_server() {
    let f = Fixture::new();
    let case = explicit_mcp_evaluation_case();
    let required = case["expectations"]["requiredMcp"].as_str().unwrap();
    let forbidden = case["expectations"]["forbiddenMcps"][0].as_str().unwrap();
    let notebook = f.local(required);
    let database = f.local(forbidden);
    f.secrets.loads.store(0, Ordering::Relaxed);
    let (_sender, signal) = watch::channel(false);
    let mut clients = runtime::TurnClients::discover_for_user(
        &f.mcp,
        &f.state,
        &f.home,
        &f.home,
        case["sanitizedInput"].as_str().unwrap(),
        signal.clone(),
    )
    .await
    .unwrap();
    assert_eq!(f.secrets.loads.load(Ordering::Relaxed), 1);
    let definitions = clients.definitions(&f.mcp, &f.state, &f.home, false).await;
    assert_eq!(definitions.len(), 2);
    assert!(definitions.iter().all(|definition| {
        definition["description"]
            .as_str()
            .unwrap()
            .contains(required)
    }));
    assert!(clients.instructions().contains("outcomeUncertain"));
    assert!(clients.requires_explicit_attempt());

    let unrelated = runtime::wire_name(&database, "lookup");
    let error = clients
        .execute(
            &f.mcp,
            &f.state,
            &f.home,
            &unrelated,
            &json!({"query":"status"}),
            false,
            signal.clone(),
        )
        .await
        .unwrap_err();
    assert_eq!(error.code, "mcp_scope_violation");
    assert!(clients.requires_explicit_attempt());

    let lookup = runtime::wire_name(&notebook, "lookup");
    let output = clients
        .execute(
            &f.mcp,
            &f.state,
            &f.home,
            &lookup,
            &json!({"query":"status"}),
            false,
            signal,
        )
        .await
        .unwrap();
    assert!(output.contains("status"));
    assert!(!clients.requires_explicit_attempt());
    assert_eq!(
        fs::read_to_string(f.home.join("calls")).unwrap(),
        "lookup\n"
    );
}

#[tokio::test]
async fn harness_evaluation_large_mcp_catalog_loads_individual_tools_with_bounded_growth() {
    let f = Fixture::new();
    let server = f.local_with_tools("large-catalog", 2000, 48);
    let (_sender, signal) = watch::channel(false);
    let mut clients = runtime::TurnClients::discover_for_user(
        &f.mcp,
        &f.state,
        &f.home,
        &f.home,
        "Use o MCP large catalog para consultar o arquivo.",
        signal.clone(),
    )
    .await
    .unwrap();

    let initial = clients.definitions(&f.mcp, &f.state, &f.home, false).await;
    assert_eq!(
        initial
            .iter()
            .map(|definition| definition["name"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["mcp_search_tools", "mcp_load_tool"]
    );
    clients.ensure_scope_visible(&initial).unwrap();
    assert!(clients.instructions().contains("catalog is deferred"));
    assert!(clients.requires_explicit_attempt());
    assert!(!clients.requires_active_task("mcp_search_tools"));
    assert!(!clients.requires_active_task("mcp_load_tool"));

    let (catalog_tools, catalog_bytes) = clients.complete_catalog_metrics();
    let initial_bytes = initial
        .iter()
        .map(|definition| definition.to_string().len())
        .sum::<usize>();
    let reduction_basis_points =
        10_000usize.saturating_sub(initial_bytes.saturating_mul(10_000) / catalog_bytes);
    assert_eq!(catalog_tools, 50);
    assert!(reduction_basis_points >= 8_000);
    println!(
        "HARNESS_EVAL case=mcp-individual-tool-demand catalogTools={catalog_tools} fullSchemaBytes={catalog_bytes} initialSchemaBytes={initial_bytes} reductionBasisPoints={reduction_basis_points}"
    );

    let selected = runtime::wire_name(&server, "catalog_tool_37");
    let hidden = clients
        .execute(
            &f.mcp,
            &f.state,
            &f.home,
            &selected,
            &json!({"query":"archive"}),
            false,
            signal.clone(),
        )
        .await
        .unwrap_err();
    assert_eq!(hidden.code, "mcp_scope_violation");

    let search: Value = serde_json::from_str(
        &clients
            .execute(
                &f.mcp,
                &f.state,
                &f.home,
                "mcp_search_tools",
                &json!({"query":"catalog tool 37", "server":"large-catalog", "limit":3}),
                false,
                signal.clone(),
            )
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(search["matches"][0]["tool"], selected);
    assert_eq!(search["matches"][0]["name"], "catalog_tool_37");
    clients
        .execute(
            &f.mcp,
            &f.state,
            &f.home,
            "mcp_load_tool",
            &json!({"tool":selected}),
            false,
            signal.clone(),
        )
        .await
        .unwrap();
    assert!(clients.requires_explicit_attempt());
    let loaded = clients.definitions(&f.mcp, &f.state, &f.home, false).await;
    assert_eq!(loaded.len(), 3);
    assert_eq!(loaded[2]["name"], selected);
    let output = clients
        .execute(
            &f.mcp,
            &f.state,
            &f.home,
            &selected,
            &json!({"query":"archive"}),
            false,
            signal.clone(),
        )
        .await
        .unwrap();
    assert!(output.contains("archive"));
    assert!(!clients.requires_explicit_attempt());

    for index in 0..10 {
        let search: Value = serde_json::from_str(
            &clients
                .execute(
                    &f.mcp,
                    &f.state,
                    &f.home,
                    "mcp_search_tools",
                    &json!({"query":format!("catalog tool {index}"), "limit":1}),
                    false,
                    signal.clone(),
                )
                .await
                .unwrap(),
        )
        .unwrap();
        let tool = search["matches"][0]["tool"].as_str().unwrap();
        clients
            .execute(
                &f.mcp,
                &f.state,
                &f.home,
                "mcp_load_tool",
                &json!({"tool":tool}),
                false,
                signal.clone(),
            )
            .await
            .unwrap();
    }
    let bounded = clients.definitions(&f.mcp, &f.state, &f.home, false).await;
    assert_eq!(
        bounded
            .iter()
            .filter(|definition| {
                !matches!(
                    definition["name"].as_str(),
                    Some("mcp_search_tools" | "mcp_load_tool")
                )
            })
            .count(),
        8
    );
    let evicted = clients
        .execute(
            &f.mcp,
            &f.state,
            &f.home,
            &selected,
            &json!({"query":"archive again"}),
            false,
            signal.clone(),
        )
        .await
        .unwrap_err();
    assert_eq!(evicted.code, "mcp_scope_violation");

    let denied = runtime::wire_name(&server, "catalog_tool_42");
    clients
        .definitions_with(&f.mcp, &f.state, &f.home, false, |name| name != denied)
        .await;
    let search: Value = serde_json::from_str(
        &clients
            .execute(
                &f.mcp,
                &f.state,
                &f.home,
                "mcp_search_tools",
                &json!({"query":"catalog tool 42", "limit":8}),
                false,
                signal.clone(),
            )
            .await
            .unwrap(),
    )
    .unwrap();
    assert!(search["matches"]
        .as_array()
        .unwrap()
        .iter()
        .all(|item| item["tool"] != denied));
    let denied = clients
        .execute(
            &f.mcp,
            &f.state,
            &f.home,
            "mcp_load_tool",
            &json!({"tool":denied}),
            false,
            signal,
        )
        .await
        .unwrap_err();
    assert_eq!(denied.code, "mcp_scope_violation");
    assert_eq!(
        fs::read_to_string(f.home.join("calls")).unwrap(),
        "catalog_tool_37\n"
    );
}

#[tokio::test]
async fn deferred_catalog_controls_fail_closed_and_preserve_plan_read_only_scope() {
    let f = Fixture::new();
    let server = f.local_with_tools("large-catalog", 2000, 48);
    let (_sender, signal) = watch::channel(false);
    let mut clients = runtime::TurnClients::discover_for_user(
        &f.mcp,
        &f.state,
        &f.home,
        &f.home,
        "Use o MCP large catalog apenas para leitura.",
        signal.clone(),
    )
    .await
    .unwrap();

    let plan = clients.definitions(&f.mcp, &f.state, &f.home, true).await;
    assert_eq!(plan.len(), 2);
    assert_eq!(plan[0]["name"], "mcp_search_tools");
    assert_eq!(plan[1]["name"], "mcp_load_tool");

    let invalid_search = clients
        .execute(
            &f.mcp,
            &f.state,
            &f.home,
            "mcp_search_tools",
            &json!({"query":"x"}),
            true,
            signal.clone(),
        )
        .await
        .unwrap_err();
    assert_eq!(invalid_search.code, "mcp_invalid_arguments");
    assert_eq!(
        invalid_search.metadata.validation_errors,
        vec![McpValidationIssue {
            path: "$.query".into(),
            keyword: "catalog".into(),
            message: "informe de 2 a 160 caracteres".into(),
        }]
    );

    let invalid_load = clients
        .execute(
            &f.mcp,
            &f.state,
            &f.home,
            "mcp_load_tool",
            &json!({"tool":42}),
            true,
            signal.clone(),
        )
        .await
        .unwrap_err();
    assert_eq!(invalid_load.code, "mcp_invalid_arguments");
    assert_eq!(invalid_load.metadata.validation_errors[0].path, "$.tool");

    let search: Value = serde_json::from_str(
        &clients
            .execute(
                &f.mcp,
                &f.state,
                &f.home,
                "mcp_search_tools",
                &json!({"query":"mutate unknown side effect"}),
                true,
                signal.clone(),
            )
            .await
            .unwrap(),
    )
    .unwrap();
    assert!(search["matches"].as_array().unwrap().is_empty());

    let mutation = runtime::wire_name(&server, "mutate");
    let hidden_mutation = clients
        .execute(
            &f.mcp,
            &f.state,
            &f.home,
            "mcp_load_tool",
            &json!({"tool":mutation}),
            true,
            signal,
        )
        .await
        .unwrap_err();
    assert_eq!(hidden_mutation.code, "mcp_scope_violation");
    assert!(!f.home.join("calls").exists());
}

#[tokio::test]
async fn activated_large_catalog_waits_for_the_next_provider_step() {
    let f = Fixture::new();
    let server = f.local_with_tools("large-catalog", 2000, 48);
    let (_sender, signal) = watch::channel(false);
    let mut clients = runtime::TurnClients::discover_for_user(
        &f.mcp,
        &f.state,
        &f.home,
        &f.home,
        "Consulte uma integração se for necessário.",
        signal.clone(),
    )
    .await
    .unwrap();
    assert_eq!(
        clients.definitions(&f.mcp, &f.state, &f.home, false).await[0]["name"],
        "mcp_activate"
    );
    clients
        .execute(
            &f.mcp,
            &f.state,
            &f.home,
            "mcp_activate",
            &json!({"server":"large-catalog"}),
            false,
            signal.clone(),
        )
        .await
        .unwrap();

    let guessed = clients
        .execute(
            &f.mcp,
            &f.state,
            &f.home,
            &runtime::wire_name(&server, "catalog_tool_12"),
            &json!({"query":"archive"}),
            false,
            signal.clone(),
        )
        .await
        .unwrap_err();
    assert_eq!(guessed.code, "mcp_scope_violation");
    let premature_search = clients
        .execute(
            &f.mcp,
            &f.state,
            &f.home,
            "mcp_search_tools",
            &json!({"query":"catalog tool 12"}),
            false,
            signal,
        )
        .await
        .unwrap_err();
    assert_eq!(premature_search.code, "mcp_scope_violation");

    let next_step = clients.definitions(&f.mcp, &f.state, &f.home, false).await;
    assert_eq!(next_step[0]["name"], "mcp_search_tools");
    assert_eq!(next_step[1]["name"], "mcp_load_tool");
    assert_eq!(next_step.len(), 2);
    assert!(!f.home.join("calls").exists());
}

#[tokio::test]
async fn loaded_deferred_tool_survives_reconnection_when_its_schema_still_exists() {
    let f = Fixture::new();
    let server = f.local_with_tools("large-catalog", 1000, 48);
    let (_sender, signal) = watch::channel(false);
    let mut clients = runtime::TurnClients::discover_for_user(
        &f.mcp,
        &f.state,
        &f.home,
        &f.home,
        "Use o MCP large catalog para ler a documentação.",
        signal.clone(),
    )
    .await
    .unwrap();
    clients.definitions(&f.mcp, &f.state, &f.home, false).await;
    let lookup = runtime::wire_name(&server, "lookup");
    let search: Value = serde_json::from_str(
        &clients
            .execute(
                &f.mcp,
                &f.state,
                &f.home,
                "mcp_search_tools",
                &json!({"query":"read documentation", "limit":1}),
                false,
                signal.clone(),
            )
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(search["matches"][0]["tool"], lookup);
    clients
        .execute(
            &f.mcp,
            &f.state,
            &f.home,
            "mcp_load_tool",
            &json!({"tool":lookup}),
            false,
            signal.clone(),
        )
        .await
        .unwrap();
    clients.definitions(&f.mcp, &f.state, &f.home, false).await;

    let timeout = clients
        .execute(
            &f.mcp,
            &f.state,
            &f.home,
            &lookup,
            &json!({"query":"hang"}),
            false,
            signal.clone(),
        )
        .await
        .unwrap_err();
    assert_eq!(timeout.code, "mcp_request_timeout");
    assert_eq!(timeout.metadata.connection_recovered, Some(true));

    let recovered = clients.definitions(&f.mcp, &f.state, &f.home, false).await;
    assert!(recovered.iter().any(|tool| tool["name"] == lookup));
    let output = clients
        .execute(
            &f.mcp,
            &f.state,
            &f.home,
            &lookup,
            &json!({"query":"status"}),
            false,
            signal,
        )
        .await
        .unwrap();
    assert!(output.contains("status"));
    assert_eq!(
        fs::read_to_string(f.home.join("starts")).unwrap(),
        "large-catalog\nlarge-catalog\n"
    );
    assert_eq!(
        fs::read_to_string(f.home.join("calls")).unwrap(),
        "lookup\nlookup\n"
    );
}

#[tokio::test]
async fn catalog_refresh_evicts_loaded_tools_that_the_server_removed() {
    let f = Fixture::new();
    let (server, tools_file) = f.local_with_dynamic_tools("large-catalog", 2000, 48);
    let (_sender, signal) = watch::channel(false);
    let mut clients = runtime::TurnClients::discover_for_user(
        &f.mcp,
        &f.state,
        &f.home,
        &f.home,
        "Use o MCP large catalog para consultar o arquivo.",
        signal.clone(),
    )
    .await
    .unwrap();
    clients.definitions(&f.mcp, &f.state, &f.home, false).await;
    let removed = runtime::wire_name(&server, "catalog_tool_37");
    clients
        .execute(
            &f.mcp,
            &f.state,
            &f.home,
            "mcp_search_tools",
            &json!({"query":"catalog tool 37", "limit":1}),
            false,
            signal.clone(),
        )
        .await
        .unwrap();
    clients
        .execute(
            &f.mcp,
            &f.state,
            &f.home,
            "mcp_load_tool",
            &json!({"tool":removed}),
            false,
            signal.clone(),
        )
        .await
        .unwrap();
    assert!(clients
        .definitions(&f.mcp, &f.state, &f.home, false)
        .await
        .iter()
        .any(|tool| tool["name"] == removed));

    fs::write(tools_file, "10").unwrap();
    clients.notify_catalog_changed_for_test();
    let refreshed = clients.definitions(&f.mcp, &f.state, &f.home, false).await;
    assert!(!refreshed.iter().any(|tool| tool["name"] == removed));
    let stale = clients
        .execute(
            &f.mcp,
            &f.state,
            &f.home,
            &removed,
            &json!({"query":"archive"}),
            false,
            signal,
        )
        .await
        .unwrap_err();
    assert_eq!(stale.code, "mcp_scope_violation");
    assert!(!f.home.join("calls").exists());
}

#[tokio::test]
async fn harness_evaluation_mcp_intent_survives_continuations_and_accepts_user_changes() {
    let f = Fixture::new();
    let notebook = f.local("gemini-notebook-mcp");
    let database = f.local("database");
    let initial = runtime::resolve_user_intent(
        &f.mcp,
        &f.state,
        &f.home,
        &McpIntent::default(),
        &["Use o MCP Gemini Notebook nesta tarefa.".into()],
    )
    .await
    .unwrap();
    assert_eq!(initial.mode, McpIntentMode::Explicit);
    assert_eq!(initial.servers.len(), 1);
    assert_eq!(initial.servers[0].id, notebook.id);

    let continued = runtime::resolve_user_intent(
        &f.mcp,
        &f.state,
        &f.home,
        &initial,
        &["Continue a análise e confirme o resultado.".into()],
    )
    .await
    .unwrap();
    assert_eq!(continued, initial);

    let mentioned = runtime::resolve_user_intent(
        &f.mcp,
        &f.state,
        &f.home,
        &continued,
        &["Por que o MCP database foi usado antes? Continue a análise.".into()],
    )
    .await
    .unwrap();
    assert_eq!(mentioned, initial);
    let (_sender, signal) = watch::channel(false);
    let mut clients = runtime::TurnClients::discover_for_intent(
        &f.mcp, &f.state, &f.home, &f.home, &continued, signal,
    )
    .await
    .unwrap();
    let definitions = clients.definitions(&f.mcp, &f.state, &f.home, false).await;
    assert_eq!(definitions.len(), 2);
    assert!(definitions
        .iter()
        .all(|definition| definition["description"]
            .as_str()
            .unwrap()
            .contains("gemini-notebook-mcp")));

    let changed = runtime::resolve_user_intent(
        &f.mcp,
        &f.state,
        &f.home,
        &continued,
        &["Agora use o database para esta consulta.".into()],
    )
    .await
    .unwrap();
    assert_eq!(changed.mode, McpIntentMode::Explicit);
    assert_eq!(changed.servers.len(), 1);
    assert_eq!(changed.servers[0].id, database.id);

    let switched = runtime::resolve_user_intent(
        &f.mcp,
        &f.state,
        &f.home,
        &initial,
        &["Troque do MCP Gemini Notebook para o MCP database.".into()],
    )
    .await
    .unwrap();
    assert_eq!(switched.mode, McpIntentMode::Explicit);
    assert_eq!(switched.servers.len(), 1);
    assert_eq!(switched.servers[0].id, database.id);

    let excluded = runtime::resolve_user_intent(
        &f.mcp,
        &f.state,
        &f.home,
        &initial,
        &["Não use mais o MCP Gemini Notebook; escolha outro se necessário.".into()],
    )
    .await
    .unwrap();
    assert_eq!(excluded.mode, McpIntentMode::OnDemand);
    assert!(excluded.servers.is_empty());
    assert_eq!(excluded.excluded_servers.len(), 1);
    assert_eq!(excluded.excluded_servers[0].id, notebook.id);
    let (_sender, signal) = watch::channel(false);
    let mut excluded_clients = runtime::TurnClients::discover_for_intent(
        &f.mcp, &f.state, &f.home, &f.home, &excluded, signal,
    )
    .await
    .unwrap();
    assert!(excluded_clients
        .instructions()
        .contains("available on demand"));
    let definitions = excluded_clients
        .definitions(&f.mcp, &f.state, &f.home, false)
        .await;
    let choices = definitions[0]["parameters"]["properties"]["server"]["enum"]
        .as_array()
        .unwrap();
    assert_eq!(choices, &[json!("database")]);

    let on_demand = runtime::resolve_user_intent(
        &f.mcp,
        &f.state,
        &f.home,
        &changed,
        &["Pode usar outros MCPs conforme necessário.".into()],
    )
    .await
    .unwrap();
    assert_eq!(on_demand, McpIntent::default());
    let disabled = runtime::resolve_user_intent(
        &f.mcp,
        &f.state,
        &f.home,
        &continued,
        &["Continue sem nenhum MCP.".into()],
    )
    .await
    .unwrap();
    assert_eq!(disabled.mode, McpIntentMode::Disabled);
    assert!(disabled.servers.is_empty());
}

#[tokio::test]
async fn request_timeout_is_independent_from_the_short_startup_timeout() {
    let f = Fixture::new();
    let raw = json!({
        "slow": {
            "type": "local",
            "command": ["node", fixture_script()],
            "environment": {"CALLS_FILE": f.home.join("calls")},
            "timeout": 1000,
            "requestTimeout": 2500
        }
    })
    .to_string();
    let server = f
        .mcp
        .save(&f.state, &f.home, None, &raw)
        .unwrap()
        .into_iter()
        .find(|server| server.name == "slow")
        .unwrap();
    let (_sender, signal) = watch::channel(false);
    let mut clients = runtime::TurnClients::discover_for_user(
        &f.mcp,
        &f.state,
        &f.home,
        &f.home,
        "Use o MCP slow para consultar a documentação.",
        signal.clone(),
    )
    .await
    .unwrap();
    let lookup = runtime::wire_name(&server, "lookup");
    let output = clients
        .execute(
            &f.mcp,
            &f.state,
            &f.home,
            &lookup,
            &json!({"query":"status", "delayMs":1200}),
            false,
            signal,
        )
        .await
        .unwrap();
    assert!(output.contains("status"));
    assert_eq!(
        fs::read_to_string(f.home.join("calls")).unwrap(),
        "lookup\n"
    );
}

#[tokio::test]
async fn harness_evaluation_timeout_reconnects_only_the_requested_mcp_without_replaying() {
    let f = Fixture::new();
    let notebook = f.local_with_request_timeout("gemini-notebook-mcp", 1000);
    f.local("database");
    let (_sender, signal) = watch::channel(false);
    let mut clients = runtime::TurnClients::discover_for_user(
        &f.mcp,
        &f.state,
        &f.home,
        &f.home,
        "Use o MCP Gemini Notebook para consultar as informações.",
        signal.clone(),
    )
    .await
    .unwrap();
    let lookup = runtime::wire_name(&notebook, "lookup");

    let timeout = clients
        .execute(
            &f.mcp,
            &f.state,
            &f.home,
            &lookup,
            &json!({"query":"hang"}),
            false,
            signal.clone(),
        )
        .await
        .unwrap_err();

    assert_eq!(timeout.code, "mcp_request_timeout");
    assert!(timeout.metadata.retryable);
    assert!(!timeout.metadata.outcome_uncertain);
    assert_eq!(timeout.metadata.connection_recovered, Some(true));
    assert_eq!(
        timeout.metadata.server.as_deref(),
        Some("gemini-notebook-mcp")
    );
    assert_eq!(timeout.metadata.tool.as_deref(), Some("lookup"));
    assert_eq!(
        fs::read_to_string(f.home.join("calls")).unwrap(),
        "lookup\n"
    );
    assert_eq!(
        fs::read_to_string(f.home.join("starts")).unwrap(),
        "gemini-notebook-mcp\ngemini-notebook-mcp\n"
    );

    let definitions = clients.definitions(&f.mcp, &f.state, &f.home, false).await;
    assert_eq!(definitions.len(), 2);
    assert!(definitions
        .iter()
        .all(|definition| definition["description"]
            .as_str()
            .unwrap()
            .contains("gemini-notebook-mcp")));
    let output = clients
        .execute(
            &f.mcp,
            &f.state,
            &f.home,
            &lookup,
            &json!({"query":"status"}),
            false,
            signal,
        )
        .await
        .unwrap();
    assert!(output.contains("status"));
    assert_eq!(
        fs::read_to_string(f.home.join("calls")).unwrap(),
        "lookup\nlookup\n"
    );
}

#[tokio::test]
async fn harness_evaluation_preserves_structured_mcp_server_errors_without_reconnecting() {
    let f = Fixture::new();
    let server = f.local("gemini-notebook-mcp");
    let (_sender, signal) = watch::channel(false);
    let mut clients = runtime::TurnClients::discover_for_user(
        &f.mcp,
        &f.state,
        &f.home,
        &f.home,
        "Use o MCP Gemini Notebook para consultar as informações.",
        signal.clone(),
    )
    .await
    .unwrap();
    let lookup = runtime::wire_name(&server, "lookup");

    let failure = clients
        .execute(
            &f.mcp,
            &f.state,
            &f.home,
            &lookup,
            &json!({"query":"rpc-error"}),
            false,
            signal,
        )
        .await
        .unwrap_err();

    assert_eq!(failure.code, "mcp_server_error");
    assert_eq!(failure.metadata.server_error_code, Some(-32602));
    assert_eq!(
        failure.metadata.server_error_data,
        Some(json!({"path":"$.notebook_id", "expected":"string", "diagnostic":"[redigido]"}))
    );
    assert!(failure.metadata.retryable);
    assert!(!failure.metadata.outcome_uncertain);
    assert_eq!(failure.metadata.connection_recovered, None);
    assert_eq!(
        fs::read_to_string(f.home.join("starts")).unwrap(),
        "gemini-notebook-mcp\n"
    );
    assert_eq!(
        fs::read_to_string(f.home.join("calls")).unwrap(),
        "lookup\n"
    );
    let wire: Value = serde_json::from_str(&failure.tool_result()).unwrap();
    assert_eq!(wire["error"]["serverErrorCode"], -32602);
    assert_eq!(wire["error"]["serverErrorData"]["path"], "$.notebook_id");
}

#[tokio::test]
async fn mutating_timeout_is_uncertain_and_is_never_replayed_during_reconnection() {
    let f = Fixture::new();
    let server = f.local_with_request_timeout("writer", 1000);
    let (_sender, signal) = watch::channel(false);
    let mut clients = runtime::TurnClients::discover_for_user(
        &f.mcp,
        &f.state,
        &f.home,
        &f.home,
        "Use o MCP writer para realizar a operação.",
        signal,
    )
    .await
    .unwrap();
    let mutation = runtime::wire_name(&server, "mutate");

    let timeout = clients
        .execute(
            &f.mcp,
            &f.state,
            &f.home,
            &mutation,
            &json!({"hang":true}),
            false,
            watch::channel(false).1,
        )
        .await
        .unwrap_err();

    assert_eq!(timeout.code, "mcp_request_timeout");
    assert!(!timeout.metadata.retryable);
    assert!(timeout.metadata.outcome_uncertain);
    assert_eq!(timeout.metadata.connection_recovered, Some(true));
    assert_eq!(
        fs::read_to_string(f.home.join("calls")).unwrap(),
        "mutate\n"
    );
    assert_eq!(
        fs::read_to_string(f.home.join("starts")).unwrap(),
        "writer\nwriter\n"
    );
}

#[tokio::test]
async fn unavailable_explicit_mcp_fails_before_any_alternative_is_connected() {
    let f = Fixture::new();
    let notebook = f.local("gemini-notebook-mcp");
    f.local("database");
    f.mcp
        .set_enabled(&f.state, &f.home, &notebook.id, false)
        .unwrap();
    let (_sender, signal) = watch::channel(false);
    let error = runtime::TurnClients::discover_for_user(
        &f.mcp,
        &f.state,
        &f.home,
        &f.home,
        "Preciso que use o MCP Gemini Notebook.",
        signal,
    )
    .await
    .err()
    .unwrap();
    assert_eq!(error.code, "mcp_requested_unavailable");
    assert!(error.message.contains("gemini-notebook-mcp"));
    assert!(!f.home.join("calls").exists());
}

#[tokio::test]
async fn generic_turn_exposes_a_small_selector_then_only_the_activated_server_tools() {
    let f = Fixture::new();
    f.local("docs");
    f.local("database");
    f.secrets.loads.store(0, Ordering::Relaxed);
    let (_sender, signal) = watch::channel(false);
    let mut clients = runtime::TurnClients::discover_for_user(
        &f.mcp,
        &f.state,
        &f.home,
        &f.home,
        "Consulte a documentação se isso for útil.",
        signal.clone(),
    )
    .await
    .unwrap();
    let initial = clients.definitions(&f.mcp, &f.state, &f.home, false).await;
    assert_eq!(f.secrets.loads.load(Ordering::Relaxed), 0);
    assert_eq!(initial.len(), 1);
    assert_eq!(initial[0]["name"], "mcp_activate");
    assert_eq!(
        initial[0]["parameters"]["properties"]["server"]["enum"],
        json!(["database", "docs"])
    );
    assert!(!clients.requires_active_task("mcp_activate"));
    clients
        .execute(
            &f.mcp,
            &f.state,
            &f.home,
            "mcp_activate",
            &json!({"server":"docs"}),
            false,
            signal,
        )
        .await
        .unwrap();
    assert_eq!(f.secrets.loads.load(Ordering::Relaxed), 1);
    let active = clients.definitions(&f.mcp, &f.state, &f.home, false).await;
    assert_eq!(active.len(), 3);
    let lookup = active
        .iter()
        .find(|definition| {
            definition["description"]
                .as_str()
                .is_some_and(|description| description.contains("docs / lookup"))
        })
        .unwrap()["name"]
        .as_str()
        .unwrap();
    let mutation = active
        .iter()
        .find(|definition| {
            definition["description"]
                .as_str()
                .is_some_and(|description| description.contains("docs / mutate"))
        })
        .unwrap()["name"]
        .as_str()
        .unwrap();
    assert!(!clients.requires_active_task(lookup));
    assert!(clients.requires_active_task(mutation));
    assert!(active
        .iter()
        .filter(|definition| definition["name"] != "mcp_activate")
        .all(|definition| definition["description"]
            .as_str()
            .unwrap()
            .contains("MCP docs /")));
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
    let invalid = clients
        .execute(
            &f.mcp,
            &f.state,
            &f.home,
            lookup,
            &json!({"query":42, "unexpected":true}),
            false,
            signal.clone(),
        )
        .await
        .unwrap_err();
    assert_eq!(invalid.code, "mcp_invalid_arguments");
    assert_eq!(invalid.metadata.server.as_deref(), Some("docs"));
    assert_eq!(invalid.metadata.tool.as_deref(), Some("lookup"));
    assert!(invalid.metadata.retryable);
    assert!(!invalid.metadata.outcome_uncertain);
    assert_eq!(
        invalid.metadata.validation_errors,
        vec![
            McpValidationIssue {
                path: "$.query".into(),
                keyword: "type".into(),
                message: "tipo inválido; esperado texto".into(),
            },
            McpValidationIssue {
                path: "$.unexpected".into(),
                keyword: "additionalProperties".into(),
                message: "campo não permitido pelo schema".into(),
            },
        ]
    );
    let wire: Value = serde_json::from_str(&invalid.tool_result()).unwrap();
    assert_eq!(wire["ok"], false);
    assert_eq!(wire["error"]["code"], "mcp_invalid_arguments");
    assert_eq!(wire["error"]["validationErrors"][0]["path"], "$.query");
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
    #[cfg(unix)]
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
    let server = fixture
        .mcp
        .save(
            &fixture.state,
            &fixture.home,
            Some("builtin-context7"),
            &raw,
        )
        .unwrap()
        .into_iter()
        .find(|server| server.name == "context7")
        .unwrap();
    let mut config = fixture.mcp.config(&server).unwrap();
    let home = PathBuf::from(std::env::var_os("HOME").unwrap());
    if let Config::Local { environment, .. } = &mut config {
        let path = executable::search_path(
            std::ffi::OsStr::new("/usr/bin:/bin:/usr/sbin:/sbin"),
            &home,
            &[Path::new("/opt/homebrew"), Path::new("/usr/local")],
        );
        environment.insert("PATH".into(), path.to_string_lossy().into_owned());
    }
    let (_send, signal) = watch::channel(false);
    let mut client = runtime::connect(server, config, &fixture.home, signal)
        .await
        .expect("Context7 discovery failed");
    assert!(client.tool_count() > 0);
    println!("Context7 discovery passed: {} tools", client.tool_count());
    client.close().await;
}
