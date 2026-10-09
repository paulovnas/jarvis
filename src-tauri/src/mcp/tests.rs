use super::*;
use crate::agent::evaluation::{assert_runtime_report, RuntimeReport};
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
pub(super) struct MemorySecrets {
    values: Mutex<HashMap<String, String>>,
    pub(super) fail: AtomicBool,
    loads: AtomicU64,
}
impl Secrets for MemorySecrets {
    fn load_optional(&self, key: &str) -> Result<Option<String>, McpError> {
        if self.fail.load(Ordering::Relaxed) {
            return Err(storage_error());
        }
        Ok(self.values.lock().unwrap().get(key).cloned())
    }

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
                apps_context: Mutex::new(None),
            })),
            secrets,
        }
    }
    fn local(&self, name: &str) -> Server {
        self.local_with_request_timeout(name, 2000)
    }
    fn single_tool(&self, name: &str, original: &str) -> Server {
        let script = self.home.join("single-tool.mjs");
        fs::write(
            &script,
            r#"import { createInterface } from 'node:readline';
import { appendFileSync } from 'node:fs';
const tool = { name: process.env.TOOL_NAME, description: 'Inspect a production board',
  inputSchema: { type: 'object', properties: { action: { type: 'string', enum: ['inspect'] } },
    required: ['action'], additionalProperties: false }, annotations: { readOnlyHint: true } };
createInterface({ input: process.stdin }).on('line', line => {
  const request = JSON.parse(line);
  if (!Object.hasOwn(request, 'id')) return;
  let result;
  if (request.method === 'initialize') result = { protocolVersion: '2024-11-05',
    capabilities: { tools: {} }, serverInfo: { name: 'single-tool-fixture', version: '1' } };
  else if (request.method === 'tools/list') result = { tools: [tool] };
  else if (request.method === 'tools/call') {
    appendFileSync(process.env.CALLS_FILE, `${request.params.name}\n`);
    result = { content: [{ type: 'text', text: 'Production board inspected.' }] };
  } else result = {};
  process.stdout.write(`${JSON.stringify({ jsonrpc: '2.0', id: request.id, result })}\n`);
});
"#,
        )
        .unwrap();
        let raw = json!({name:{"type":"local", "command":["node",script],
            "environment":{"TOOL_NAME":original,"CALLS_FILE":self.home.join("calls")},
            "timeout":2000,"requestTimeout":2000}})
        .to_string();
        self.mcp
            .save(&self.state, &self.home, None, &raw)
            .unwrap()
            .into_iter()
            .find(|server| server.name == name)
            .unwrap()
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

#[tokio::test]
async fn hook_mcp_core_call_preserves_structured_content_and_redacts_before_bounding() {
    let fixture = Fixture::new();
    let server = fixture.local("hook-docs");
    let config = fixture.mcp.config(&fixture.home, &server).unwrap();
    let (_sender, signal) = watch::channel(false);
    let mut client =
        runtime::connect_with_state(&fixture.mcp, server, config, &fixture.home, signal.clone())
            .await
            .unwrap();
    let output = client
        .core_call("lookup", &json!({"query":"hook brief"}), signal.clone())
        .await
        .unwrap();
    assert!(output.contains("Documentation: hook brief"));
    assert!(output.contains("\"source\":\"fixture\""));
    assert!(output.contains(&fixture.home.to_string_lossy().to_string()));
    assert!(!output.contains("fixture-sensitive-value"));
    let long = client
        .core_call(
            "lookup",
            &json!({"query":"mirrored-long-description"}),
            signal,
        )
        .await
        .unwrap();
    assert_eq!(long.matches("fixture-task").count(), 1);
    assert!(long.ends_with("[Resultado abreviado pelo Jarvis]"));
    assert!(long.chars().count() <= 48_050);
    client.close().await;
}

#[tokio::test]
async fn new_registrations_refresh_the_current_turn_without_connecting_or_restarting_clients() {
    let f = Fixture::new();
    let database = f.local("database");
    let (_sender, signal) = watch::channel(false);
    let mut clients = runtime::TurnClients::discover_for_intent(
        &f.mcp,
        &f.state,
        &f.home,
        &f.home,
        &McpIntent::default(),
        signal.clone(),
    )
    .await
    .unwrap();
    clients.definitions(&f.mcp, &f.state, &f.home, false).await;
    clients
        .execute(
            &f.mcp,
            &f.state,
            &f.home,
            "mcp_activate",
            &json!({"server":"database"}),
            false,
            signal,
        )
        .await
        .unwrap();
    let starts = fs::read_to_string(f.home.join("starts")).unwrap();
    let secret_loads = f.secrets.loads.load(Ordering::Relaxed);
    f.local("later");

    let refreshed = clients.definitions(&f.mcp, &f.state, &f.home, false).await;
    assert!(refreshed
        .iter()
        .any(|definition| definition["name"] == runtime::wire_name(&database, "lookup")));
    let receipt = clients.discovery_schemas("jarvis_propose_mcp", &json!({}), "", &refreshed);
    let activate = receipt
        .iter()
        .find(|tool| tool["name"] == "mcp_activate")
        .unwrap();
    let names = activate["inputSchema"]["properties"]["server"]["enum"]
        .as_array()
        .unwrap();
    assert!(names.contains(&json!("later")));
    assert!(!names.contains(&json!("database")));
    assert_eq!(fs::read_to_string(f.home.join("starts")).unwrap(), starts);
    assert_eq!(f.secrets.loads.load(Ordering::Relaxed), secret_loads);
    assert!(!f.home.join("later-pid").exists());
}

#[tokio::test]
async fn newly_installed_plugin_mcp_refreshes_without_starting_or_replacing_frozen_versions() {
    let f = Fixture::new();
    let (_sender, signal) = watch::channel(false);
    let mut clients = runtime::TurnClients::discover_for_intent(
        &f.mcp,
        &f.state,
        &f.home,
        &f.home,
        &McpIntent::default(),
        signal.clone(),
    )
    .await
    .unwrap();
    assert!(clients
        .definitions(&f.mcp, &f.state, &f.home, false)
        .await
        .is_empty());
    let create = |version: &str| {
        crate::plugins::Operation::Create {
        draft: serde_json::from_value(json!({
            "name":"firebase", "description":"Firebase fixture",
            "mcpServers":{"firebase":{"command":"node","args":[fixture_script()],"env":{"STARTS_FILE":f.home.join("plugin-starts"),"SERVER_NAME":version}}}
        })).unwrap(),
    }
    };
    let prepared = crate::plugins::preview(&f.home, 0, create("frozen"))
        .await
        .unwrap();
    let installed = crate::plugins::apply(&f.home, &prepared).unwrap();
    let definitions = clients.definitions(&f.mcp, &f.state, &f.home, false).await;
    assert_eq!(
        definitions[0]["parameters"]["properties"]["server"]["enum"],
        json!(["firebase@local: firebase"])
    );
    assert!(!f.home.join("plugin-starts").exists());
    let error = clients
        .execute(
            &f.mcp,
            &f.state,
            &f.home,
            "mcp_activate",
            &json!({"server":"firebase"}),
            false,
            signal.clone(),
        )
        .await
        .unwrap_err();
    assert!(error.message.contains("firebase@local: firebase"));
    assert!(!f.home.join("plugin-starts").exists());
    let prepared = crate::plugins::preview(&f.home, installed.revision, create("updated"))
        .await
        .unwrap();
    let updated = crate::plugins::apply(&f.home, &prepared).unwrap();
    clients.definitions(&f.mcp, &f.state, &f.home, false).await;
    clients
        .execute(
            &f.mcp,
            &f.state,
            &f.home,
            "mcp_activate",
            &json!({"server":"firebase@local: firebase"}),
            false,
            signal,
        )
        .await
        .unwrap();
    assert_eq!(
        fs::read_to_string(f.home.join("plugin-starts")).unwrap(),
        "frozen\n"
    );
    let prepared = crate::plugins::preview(
        &f.home,
        updated.revision,
        crate::plugins::Operation::SetEnabled {
            plugin_id: "firebase@local".into(),
            enabled: false,
            project_path: Some(f.home.to_string_lossy().into_owned()),
        },
    )
    .await
    .unwrap();
    crate::plugins::apply(&f.home, &prepared).unwrap();
    assert!(clients
        .definitions(&f.mcp, &f.state, &f.home, false)
        .await
        .is_empty());
    assert!(f
        .mcp
        .list_for_project(&f.state, &f.home, None)
        .unwrap()
        .iter()
        .any(|server| server.name == "firebase@local: firebase"));
    drop(clients);
}

#[tokio::test]
async fn plugin_mcp_activity_tracks_real_calls_errors_parallelism_and_cancellation() {
    let f = Fixture::new();
    let prepared = crate::plugins::preview(&f.home, 0, crate::plugins::Operation::Create {
        draft: serde_json::from_value(json!({"name":"usage","description":"Usage fixture","mcpServers":{"docs":{"command":"node","args":[fixture_script()],"env":{"CALLS_FILE":f.home.join("plugin-calls"),"TEST_SECRET":"receipt-secret"}}}})).unwrap(),
    }).await.unwrap();
    crate::plugins::apply(&f.home, &prepared).unwrap();
    let (sender, signal) = watch::channel(false);
    let mut clients = runtime::TurnClients::discover_for_intent(
        &f.mcp,
        &f.state,
        &f.home,
        &f.home,
        &McpIntent::default(),
        signal.clone(),
    )
    .await
    .unwrap();
    clients.definitions(&f.mcp, &f.state, &f.home, false).await;
    assert!(clients.take_activity().is_empty());
    clients
        .execute(
            &f.mcp,
            &f.state,
            &f.home,
            "mcp_activate",
            &json!({"server":"usage@local: docs"}),
            false,
            signal.clone(),
        )
        .await
        .unwrap();
    assert!(clients.take_activity().is_empty());
    let server = f
        .mcp
        .list_for_project(&f.state, &f.home, Some(&f.home))
        .unwrap()
        .into_iter()
        .find(|server| server.name == "usage@local: docs")
        .unwrap();
    let tool = runtime::wire_name(&server, "lookup");
    clients.definitions(&f.mcp, &f.state, &f.home, false).await;
    assert!(clients
        .execute(
            &f.mcp,
            &f.state,
            &f.home,
            &tool,
            &json!({}),
            false,
            signal.clone()
        )
        .await
        .is_err());
    assert!(clients.take_activity().is_empty());
    assert!(!f.home.join("plugin-calls").exists());
    clients
        .execute(
            &f.mcp,
            &f.state,
            &f.home,
            &tool,
            &json!({"query":"normal"}),
            false,
            signal.clone(),
        )
        .await
        .unwrap();
    assert!(clients
        .execute(
            &f.mcp,
            &f.state,
            &f.home,
            &tool,
            &json!({"query":"fail"}),
            false,
            signal.clone()
        )
        .await
        .is_err());
    let calls = clients.take_activity();
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0].status, crate::core::activity::Status::Applied);
    assert_eq!(calls[1].status, crate::core::activity::Status::Issues);
    assert!(calls.iter().all(
        |activity| activity.plugin_id.as_deref() == Some("usage@local")
            && activity.resource_id.as_deref() == Some(server.id.as_str())
            && activity.resource_name.as_deref() == Some(server.name.as_str())
    ));
    assert!(!serde_json::to_string(&calls)
        .unwrap()
        .contains("receipt-secret"));
    let left_args = json!({"query":"barrier:left"});
    let right_args = json!({"query":"barrier:right"});
    let (left, right) = tokio::join!(
        clients.execute_parallel_read(&f.mcp, &f.state, &f.home, &tool, &left_args, signal.clone()),
        clients.execute_parallel_read(
            &f.mcp,
            &f.state,
            &f.home,
            &tool,
            &right_args,
            signal.clone()
        ),
    );
    left.unwrap();
    right.unwrap();
    assert_eq!(clients.take_activity().len(), 2);
    let (_cancelled_sender, cancelled_signal) = watch::channel(true);
    let calls_before = fs::read_to_string(f.home.join("plugin-calls")).unwrap();
    assert!(clients
        .execute_parallel_read(
            &f.mcp,
            &f.state,
            &f.home,
            &tool,
            &json!({"query":"not-sent"}),
            cancelled_signal
        )
        .await
        .is_err());
    assert!(clients.take_activity().is_empty());
    assert_eq!(
        fs::read_to_string(f.home.join("plugin-calls")).unwrap(),
        calls_before
    );
    // Closing a cancelled peer refreshes schemas before a later call is allowed.
    assert!(clients
        .execute(
            &f.mcp,
            &f.state,
            &f.home,
            &tool,
            &json!({"query":"after-reconnect"}),
            false,
            signal.clone()
        )
        .await
        .is_err());
    assert!(clients.take_activity().is_empty());
    clients.definitions(&f.mcp, &f.state, &f.home, false).await;
    clients
        .execute(
            &f.mcp,
            &f.state,
            &f.home,
            &tool,
            &json!({"query":"confirmed"}),
            false,
            signal.clone(),
        )
        .await
        .unwrap();
    let cancel = async {
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
        sender.send(true).unwrap();
    };
    let args = json!({"query":"hang"});
    let (interrupted, _) = tokio::join!(
        clients.execute_parallel_read(&f.mcp, &f.state, &f.home, &tool, &args, signal),
        cancel
    );
    assert!(interrupted.is_err());
    let calls = clients.take_activity();
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0].status, crate::core::activity::Status::Applied);
    assert_eq!(calls[1].status, crate::core::activity::Status::Issues);
    assert!(clients.take_activity().is_empty());
    let context =
        crate::hooks::mcp_dispatch::Context::new(f.mcp.clone(), f.state.clone(), &f.home, &f.home)
            .unwrap();
    assert!(context.take_activity().is_empty());
    let (_hook_sender, hook_signal) = watch::channel(false);
    crate::hooks::mcp_dispatch::execute(
        &context,
        &server.id,
        "lookup",
        &json!({"query":"hook-call"}),
        std::time::Duration::from_secs(2),
        hook_signal.clone(),
    )
    .await
    .unwrap_or_else(|_| panic!("unexpected cancellation"))
    .unwrap();
    let hooks = context.take_activity();
    assert_eq!(hooks.len(), 1);
    assert_eq!(hooks[0].plugin_id.as_deref(), Some("usage@local"));
    assert_eq!(hooks[0].status, crate::core::activity::Status::Applied);
    assert!(crate::hooks::mcp_dispatch::execute(
        &context,
        &server.id,
        "lookup",
        &json!({}),
        std::time::Duration::from_secs(2),
        hook_signal
    )
    .await
    .unwrap_or_else(|_| panic!("unexpected cancellation"))
    .is_err());
    assert!(context.take_activity().is_empty());
}

#[tokio::test]
async fn registration_refresh_preserves_disabled_explicit_and_excluded_mcp_scopes() {
    for mode in [
        McpIntentMode::Disabled,
        McpIntentMode::Explicit,
        McpIntentMode::OnDemand,
    ] {
        let f = Fixture::new();
        let selected = f.local("selected");
        let identity = McpIntentServer {
            id: selected.id.clone(),
            name: selected.name.clone(),
        };
        let intent = McpIntent {
            mode,
            servers: if mode == McpIntentMode::Explicit {
                vec![identity.clone()]
            } else {
                vec![]
            },
            excluded_servers: if mode == McpIntentMode::OnDemand {
                vec![identity]
            } else {
                vec![]
            },
        };
        let (_sender, signal) = watch::channel(false);
        let mut clients = runtime::TurnClients::discover_for_intent(
            &f.mcp, &f.state, &f.home, &f.home, &intent, signal,
        )
        .await
        .unwrap();
        f.local("later");
        let prepared = crate::plugins::preview(&f.home, 0, crate::plugins::Operation::Create {
            draft: serde_json::from_value(json!({"name":"scope-fixture","description":"MCP scope fixture","mcpServers":{"later":{"command":"node","args":[fixture_script()]}}})).unwrap(),
        }).await.unwrap();
        crate::plugins::apply(&f.home, &prepared).unwrap();
        let definitions = clients.definitions(&f.mcp, &f.state, &f.home, false).await;
        let activate = definitions
            .iter()
            .find(|tool| tool["name"] == "mcp_activate");
        if mode == McpIntentMode::OnDemand {
            let names = activate.unwrap()["parameters"]["properties"]["server"]["enum"]
                .as_array()
                .unwrap();
            assert!(names.contains(&json!("later")));
            assert!(names.contains(&json!("scope-fixture@local: later")));
            assert!(!names.contains(&json!("selected")));
            assert!(!f.home.join("starts").exists());
        } else {
            assert!(activate.is_none());
            assert!(!definitions
                .iter()
                .any(|tool| tool.to_string().contains("later")));
            if mode == McpIntentMode::Disabled {
                assert!(definitions.is_empty());
                assert!(!f.home.join("starts").exists());
            }
        }
    }
}

#[tokio::test]
async fn stable_gateway_receives_activated_schemas_without_relisting_its_initial_catalog() {
    let f = Fixture::new();
    let database = f.local("database");
    f.local("unrelated");
    let (_sender, signal) = watch::channel(false);
    let mut clients = runtime::TurnClients::discover_for_intent(
        &f.mcp,
        &f.state,
        &f.home,
        &f.home,
        &McpIntent::default(),
        signal.clone(),
    )
    .await
    .unwrap();
    // Claude reads tools/list once. The bridge must provide the new schemas in
    // the activation receipt even though that initial catalog cannot change.
    let initial = clients.definitions(&f.mcp, &f.state, &f.home, true).await;
    assert_eq!(initial.len(), 1);
    assert_eq!(initial[0]["name"], "mcp_activate");
    let args = json!({"server":"database"});
    let output = clients
        .execute(
            &f.mcp,
            &f.state,
            &f.home,
            "mcp_activate",
            &args,
            true,
            signal.clone(),
        )
        .await
        .unwrap();
    let available = clients.definitions(&f.mcp, &f.state, &f.home, true).await;
    let receipt = clients.discovery_schemas("mcp_activate", &args, &output, &available);
    let lookup = runtime::wire_name(&database, "lookup");
    let schema = receipt.iter().find(|tool| tool["name"] == lookup).unwrap();
    assert_eq!(schema["inputSchema"]["required"], json!(["query"]));
    assert!(!receipt
        .iter()
        .any(|tool| tool["name"] == runtime::wire_name(&database, "mutate")));
    let result = clients
        .execute(
            &f.mcp,
            &f.state,
            &f.home,
            schema["name"].as_str().unwrap(),
            &json!({"query":"schema receipt"}),
            true,
            signal.clone(),
        )
        .await
        .unwrap();
    assert!(result.contains("schema receipt"));
    assert_eq!(
        fs::read_to_string(f.home.join("calls")).unwrap(),
        "lookup\n"
    );

    // A resumed on-demand turn can safely reactivate through current controls,
    // rather than guessing a cached external name or asking for a new message.
    let mut resumed = runtime::TurnClients::discover_for_intent(
        &f.mcp,
        &f.state,
        &f.home,
        &f.home,
        &McpIntent::default(),
        signal,
    )
    .await
    .unwrap();
    let current = resumed.definitions(&f.mcp, &f.state, &f.home, true).await;
    let recovery = resumed.discovery_schemas("", &json!({}), "", &current);
    assert_eq!(recovery.len(), 1);
    assert_eq!(recovery[0]["name"], "mcp_activate");
}

#[tokio::test]
async fn stable_gateway_receipts_include_only_selected_and_permitted_deferred_schemas() {
    let f = Fixture::new();
    let database = f.local_with_tools("database", 2000, 48);
    let (_sender, signal) = watch::channel(false);
    let mut clients = runtime::TurnClients::discover_for_intent(
        &f.mcp,
        &f.state,
        &f.home,
        &f.home,
        &McpIntent::default(),
        signal.clone(),
    )
    .await
    .unwrap();
    clients.definitions(&f.mcp, &f.state, &f.home, false).await;
    let activate_args = json!({"server":"database"});
    let activated = clients
        .execute(
            &f.mcp,
            &f.state,
            &f.home,
            "mcp_activate",
            &activate_args,
            false,
            signal.clone(),
        )
        .await
        .unwrap();
    let available = clients.definitions(&f.mcp, &f.state, &f.home, false).await;
    let controls =
        clients.discovery_schemas("mcp_activate", &activate_args, &activated, &available);
    assert_eq!(
        controls
            .iter()
            .map(|tool| tool["name"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["mcp_search_tools", "mcp_load_tool"]
    );
    let args = json!({"query":"catalog tool 37", "limit":1});
    let output = clients
        .execute(
            &f.mcp,
            &f.state,
            &f.home,
            "mcp_search_tools",
            &args,
            false,
            signal.clone(),
        )
        .await
        .unwrap();
    let selected = runtime::wire_name(&database, "catalog_tool_37");
    let available = clients.definitions(&f.mcp, &f.state, &f.home, false).await;
    let receipt = clients.discovery_schemas("mcp_search_tools", &args, &output, &available);
    assert_eq!(receipt.len(), 3);
    let schema = receipt
        .iter()
        .find(|tool| tool["name"] == selected)
        .unwrap();
    assert_eq!(schema["inputSchema"]["required"], json!(["query"]));
    assert!(clients
        .execute(
            &f.mcp,
            &f.state,
            &f.home,
            schema["name"].as_str().unwrap(),
            &json!({"query":"exact schema"}),
            false,
            signal.clone(),
        )
        .await
        .unwrap()
        .contains("exact schema"));

    let load_args = json!({"tool":selected});
    let loaded = clients
        .execute(
            &f.mcp,
            &f.state,
            &f.home,
            "mcp_load_tool",
            &load_args,
            false,
            signal,
        )
        .await
        .unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&loaded).unwrap()["alreadyLoaded"],
        true
    );
    assert!(clients
        .discovery_schemas("mcp_load_tool", &load_args, &loaded, &available)
        .iter()
        .any(|tool| tool["name"] == selected));

    let restricted = clients
        .definitions_with(&f.mcp, &f.state, &f.home, false, |name| name != selected)
        .await;
    assert!(!clients
        .discovery_schemas("mcp_load_tool", &load_args, &loaded, &restricted)
        .iter()
        .any(|tool| tool["name"] == selected));
}

#[tokio::test]
async fn small_mcp_receipts_recover_original_names_with_current_schemas_in_the_same_turn() {
    let f = Fixture::new();
    let server = f.single_tool("creative-production", "creative_production_board");
    let (_sender, signal) = watch::channel(false);
    let mut clients = runtime::TurnClients::discover_for_intent(
        &f.mcp,
        &f.state,
        &f.home,
        &f.home,
        &McpIntent::default(),
        signal.clone(),
    )
    .await
    .unwrap();
    clients.definitions(&f.mcp, &f.state, &f.home, false).await;
    let activate = json!({"server":"creative-production"});
    let output = clients
        .execute(
            &f.mcp,
            &f.state,
            &f.home,
            "mcp_activate",
            &activate,
            false,
            signal.clone(),
        )
        .await
        .unwrap();
    assert!(output.contains("mesma mensagem"));
    let definitions = clients.definitions(&f.mcp, &f.state, &f.home, false).await;
    let receipt: Value = serde_json::from_str(&clients.discovery_output(
        "mcp_activate",
        &activate,
        &output,
        &definitions,
    ))
    .unwrap();
    let canonical = runtime::wire_name(&server, "creative_production_board");
    assert_eq!(receipt["schemas"].as_array().unwrap().len(), 1);
    assert_eq!(receipt["schemas"][0]["name"], canonical);
    assert_eq!(
        receipt["schemas"][0]["inputSchema"]["required"],
        json!(["action"])
    );
    assert!(receipt["next"]
        .as_str()
        .unwrap()
        .contains("same user message"));
    assert!(!receipt.to_string().contains("mcp_search_tools"));
    assert!(!clients.requires_explicit_attempt());
    assert!(clients
        .unavailable_tool_feedback("unrelated", &json!({}), &definitions)
        .is_none());
    assert!(clients
        .unavailable_tool_feedback("mcp_invented", &json!({}), &[])
        .is_none());

    let feedback = clients
        .unavailable_tool_feedback(
            "mcp_search_tools",
            &json!({"name":"creative_production_board"}),
            &definitions,
        )
        .unwrap();
    assert_eq!(
        feedback.tool_names.as_slice(),
        std::slice::from_ref(&canonical)
    );
    assert_eq!(
        feedback.schemas,
        receipt["schemas"].as_array().unwrap().clone()
    );
    let error: Value = serde_json::from_str(&feedback.output).unwrap();
    assert_eq!(error["error"]["code"], "tool_unavailable");
    assert_eq!(error["executed"], false);
    assert_eq!(error["recoverable"], true);
    assert!(clients
        .unavailable_tool_feedback("creative_production_board", &json!({}), &definitions)
        .is_some());
    assert!(clients
        .unavailable_tool_feedback(
            "invented_gateway",
            &json!({"name":"creative_production_board"}),
            &definitions,
        )
        .is_some());
    assert!(!f.home.join("calls").exists());
    let output = clients
        .execute(
            &f.mcp,
            &f.state,
            &f.home,
            &feedback.tool_names[0],
            &json!({"action":"inspect"}),
            false,
            signal,
        )
        .await
        .unwrap();
    assert!(output.contains("Production board inspected."));
    assert_eq!(
        fs::read_to_string(f.home.join("calls")).unwrap(),
        "creative_production_board\n"
    );
}

#[tokio::test]
async fn mcp_catalog_feedback_never_exposes_hidden_or_revoked_schemas() {
    let f = Fixture::new();
    let server = f.local_with_tools("large-catalog", 2000, 48);
    let (_sender, signal) = watch::channel(false);
    let mut clients = runtime::TurnClients::discover_for_user(
        &f.mcp,
        &f.state,
        &f.home,
        &f.home,
        "Use o MCP large catalog.",
        signal.clone(),
    )
    .await
    .unwrap();
    let definitions = clients.definitions(&f.mcp, &f.state, &f.home, false).await;
    assert!(clients
        .unavailable_tool_feedback("mcp_invented", &json!({}), &definitions)
        .is_none());
    assert!(clients
        .unavailable_tool_feedback("catalog_tool_37", &json!({}), &definitions)
        .is_none());
    let args = json!({"query":"catalog tool 37", "server":"large-catalog", "limit":1});
    let output = clients
        .execute(
            &f.mcp,
            &f.state,
            &f.home,
            "mcp_search_tools",
            &args,
            false,
            signal,
        )
        .await
        .unwrap();
    let definitions = clients.definitions(&f.mcp, &f.state, &f.home, false).await;
    let receipt: Value = serde_json::from_str(&clients.discovery_output(
        "mcp_search_tools",
        &args,
        &output,
        &definitions,
    ))
    .unwrap();
    let canonical = runtime::wire_name(&server, "catalog_tool_37");
    assert_eq!(receipt["autoLoaded"], json!([canonical.clone()]));
    assert!(receipt["schemas"]
        .as_array()
        .unwrap()
        .iter()
        .any(|schema| schema["name"] == canonical));
    let feedback = clients
        .unavailable_tool_feedback("mcp_invented", &json!({}), &definitions)
        .unwrap();
    assert_eq!(
        feedback.tool_names.as_slice(),
        std::slice::from_ref(&canonical)
    );
    assert_eq!(feedback.schemas.len(), 3);
    assert!(feedback
        .schemas
        .iter()
        .any(|schema| schema["name"] == "mcp_search_tools"));
    assert!(feedback
        .schemas
        .iter()
        .any(|schema| schema["name"] == "mcp_load_tool"));
    let restricted = clients
        .definitions_with(&f.mcp, &f.state, &f.home, false, |name| name != canonical)
        .await;
    assert!(clients
        .unavailable_tool_feedback("catalog_tool_37", &json!({}), &restricted)
        .is_none());
    assert!(clients
        .unavailable_tool_feedback("mcp_invented", &json!({}), &restricted)
        .is_none());
    let restricted_receipt: Value = serde_json::from_str(&clients.discovery_output(
        "mcp_search_tools",
        &args,
        &output,
        &restricted,
    ))
    .unwrap();
    assert!(!restricted_receipt["schemas"]
        .to_string()
        .contains("catalog operation 37"));
    assert!(!f.home.join("calls").exists());
}

#[tokio::test]
async fn mcp_original_name_feedback_keeps_ambiguity_without_selecting_or_executing() {
    let f = Fixture::new();
    let first = f.single_tool("first", "creative_production_board");
    let second = f.single_tool("second", "creative_production_board");
    let (_sender, signal) = watch::channel(false);
    let mut clients = runtime::TurnClients::discover_for_intent(
        &f.mcp,
        &f.state,
        &f.home,
        &f.home,
        &McpIntent::default(),
        signal.clone(),
    )
    .await
    .unwrap();
    for server in ["first", "second"] {
        clients.definitions(&f.mcp, &f.state, &f.home, false).await;
        clients
            .execute(
                &f.mcp,
                &f.state,
                &f.home,
                "mcp_activate",
                &json!({"server":server}),
                false,
                signal.clone(),
            )
            .await
            .unwrap();
    }
    let definitions = clients.definitions(&f.mcp, &f.state, &f.home, false).await;
    let feedback = clients
        .unavailable_tool_feedback("creative_production_board", &json!({}), &definitions)
        .unwrap();
    assert_eq!(
        feedback.tool_names,
        [
            runtime::wire_name(&first, "creative_production_board"),
            runtime::wire_name(&second, "creative_production_board"),
        ]
    );
    assert_eq!(feedback.schemas.len(), 2);
    assert_eq!(
        serde_json::from_str::<Value>(&feedback.output).unwrap()["ambiguous"],
        true
    );
    assert!(!f.home.join("calls").exists());
    assert!(clients
        .unavailable_tool_feedback("creative_production_bord", &json!({}), &definitions,)
        .is_none());
}

#[test]
fn mcp_discovery_receipts_preserve_uncertain_errors_and_non_discovery_outputs() {
    let clients = runtime::TurnClients::default();
    let error =
        json!({"ok":false,"error":{"code":"mcp_connection_closed","outcomeUncertain":true}})
            .to_string();
    assert_eq!(
        clients.discovery_output("mcp_activate", &json!({}), &error, &[]),
        error
    );
    assert_eq!(
        clients.discovery_output("mcp_operation", &json!({}), "confirmed", &[]),
        "confirmed"
    );
}

#[tokio::test]
async fn read_only_calls_overlap_on_the_same_mcp_peer_without_exposing_mutations() {
    let f = Fixture::new();
    let server = f.local("parallel-docs");
    let (_cancel, signal) = watch::channel(false);
    let mut clients = runtime::TurnClients::discover_for_user(
        &f.mcp,
        &f.state,
        &f.home,
        &f.home,
        "Use o MCP parallel-docs para consultar documentação.",
        signal.clone(),
    )
    .await
    .unwrap();
    clients.definitions(&f.mcp, &f.state, &f.home, false).await;
    let name = runtime::wire_name(&server, "lookup");
    assert!(clients.parallel_ready(&name));
    let mutation = runtime::wire_name(&server, "mutate");
    assert!(!clients.parallel_ready(&mutation));
    assert!(!clients.parallel_ready("mcp_invented"));
    let first_args = json!({"query":"barrier:first"});
    let second_args = json!({"query":"barrier:second"});
    let (first, second) = tokio::join!(
        clients.execute_parallel_read(
            &f.mcp,
            &f.state,
            &f.home,
            &name,
            &first_args,
            signal.clone()
        ),
        clients.execute_parallel_read(
            &f.mcp,
            &f.state,
            &f.home,
            &name,
            &second_args,
            signal.clone()
        ),
    );
    let first = first.unwrap();
    let second = second.unwrap();
    assert!(first.contains("barrier:first"));
    assert!(!first.contains("barrier:second"));
    assert!(second.contains("barrier:second"));
    assert!(!second.contains("fixture-sensitive-value"));
    assert!(!clients.requires_explicit_attempt());
    assert!(clients
        .execute_parallel_read(&f.mcp, &f.state, &f.home, &mutation, &json!({}), signal)
        .await
        .is_err());
    assert_eq!(
        fs::read_to_string(f.home.join("calls")).unwrap(),
        "lookup\nlookup\n"
    );
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
        apps_context: Mutex::new(None),
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
fn adding_a_server_returns_metadata_without_loading_or_exposing_secrets() {
    let f = Fixture::new();
    let (_, config) = config::parse(
        r#"{"docs":{"type":"remote","url":"https://example.test/mcp","headers":{"Authorization":"Bearer fixture-sensitive-value"}}}"#,
    )
    .unwrap();

    let server = f.mcp.add(&f.state, &f.home, "docs", &config).unwrap();

    assert_eq!(server.name, "docs");
    assert_eq!(server.kind, "remote");
    assert_eq!(server.revision, 1);
    assert!(server.enabled && server.configured);
    assert_eq!(f.secrets.loads.load(Ordering::Relaxed), 0);
    let metadata = serde_json::to_string(&server).unwrap();
    assert!(!metadata.contains("fixture-sensitive-value"));
    assert!(!metadata.contains("Authorization"));
    assert!(f
        .mcp
        .edit(&f.state, &f.home, &server.id)
        .unwrap()
        .contains("fixture-sensitive-value"));
}

#[test]
fn adding_a_duplicate_cannot_update_the_existing_server() {
    let f = Fixture::new();
    let server = f.local("docs");
    let original = f.mcp.edit(&f.state, &f.home, &server.id).unwrap();
    let (_, replacement) =
        config::parse(r#"{"docs":{"type":"remote","url":"https://replacement.test/mcp"}}"#)
            .unwrap();

    assert!(f.mcp.add(&f.state, &f.home, "docs", &replacement).is_err());

    let catalog = f.mcp.list(&f.state, &f.home).unwrap();
    assert_eq!(catalog.len(), 1);
    assert_eq!(catalog[0].id, server.id);
    assert_eq!(catalog[0].revision, 1);
    assert_eq!(f.mcp.edit(&f.state, &f.home, &server.id).unwrap(), original);
    assert_eq!(f.secrets.values.lock().unwrap().len(), 1);
}

#[test]
fn adding_an_invalid_typed_config_does_not_register_or_store_it() {
    let f = Fixture::new();
    let config: Config = serde_json::from_value(json!({
        "type": "remote",
        "url": "file:///tmp/server"
    }))
    .unwrap();
    assert!(f.mcp.add(&f.state, &f.home, "docs", &config).is_err());
    assert!(f.mcp.list(&f.state, &f.home).unwrap().is_empty());
    assert!(f.secrets.values.lock().unwrap().is_empty());
}

#[test]
fn adding_a_server_with_unavailable_secret_storage_leaves_no_registration() {
    let f = Fixture::new();
    let (_, config) = config::parse(config::TEMPLATE).unwrap();
    f.secrets.fail.store(true, Ordering::Relaxed);

    assert!(f.mcp.add(&f.state, &f.home, "docs", &config).is_err());
    assert!(f.mcp.list(&f.state, &f.home).unwrap().is_empty());
    assert!(f.secrets.values.lock().unwrap().is_empty());
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
    assert_runtime_report(
        "mcp-explicit-selection",
        RuntimeReport::new("completed", [("errors", 1), ("steps", 2), ("toolCalls", 2)]),
    );
}

#[tokio::test]
async fn live_mcp_selection_preserves_the_peer_loaded_schema_and_confirmed_effects() {
    let f = Fixture::new();
    let voice = f.local_with_tools("voicestudio", 2000, 6);
    let other = f.local("unrelated");
    let (_sender, signal) = watch::channel(false);
    let mut clients = runtime::TurnClients::discover_for_intent(
        &f.mcp,
        &f.state,
        &f.home,
        &f.home,
        &McpIntent::default(),
        signal.clone(),
    )
    .await
    .unwrap();
    clients.definitions(&f.mcp, &f.state, &f.home, false).await;
    clients
        .execute(
            &f.mcp,
            &f.state,
            &f.home,
            "mcp_activate",
            &json!({"server":"voicestudio"}),
            false,
            signal.clone(),
        )
        .await
        .unwrap();
    clients.definitions(&f.mcp, &f.state, &f.home, false).await;
    let lookup = runtime::wire_name(&voice, "lookup");
    clients
        .execute(
            &f.mcp,
            &f.state,
            &f.home,
            "mcp_search_tools",
            &json!({"query":"lookup"}),
            false,
            signal.clone(),
        )
        .await
        .unwrap();
    let before = clients.definitions(&f.mcp, &f.state, &f.home, false).await;
    assert!(before.iter().any(|definition| definition["name"] == lookup));
    let starts = fs::read_to_string(f.home.join("starts")).unwrap();
    let intent = runtime::resolve_user_intent(&f.mcp, &f.state, &f.home, &McpIntent::default(), &["Se puder usar a narração usando mcp do voicestudio por favor, la o audio vem melhor e mais bonito".into()]).await.unwrap();
    assert_eq!(intent.mode, McpIntentMode::Explicit);
    clients
        .refresh_intent(&f.mcp, &f.state, &f.home, &intent, signal.clone())
        .await
        .unwrap();
    let after = clients.definitions(&f.mcp, &f.state, &f.home, false).await;
    assert!(after.iter().any(|definition| definition["name"] == lookup));
    assert!(!after
        .iter()
        .any(|definition| definition["name"] == "mcp_activate"));
    assert!(clients.requires_explicit_attempt());
    assert_eq!(fs::read_to_string(f.home.join("starts")).unwrap(), starts);
    clients
        .execute(
            &f.mcp,
            &f.state,
            &f.home,
            &lookup,
            &json!({"query":"narration"}),
            false,
            signal.clone(),
        )
        .await
        .unwrap();
    assert!(!clients.requires_explicit_attempt());
    assert_eq!(
        fs::read_to_string(f.home.join("calls")).unwrap(),
        "lookup\n"
    );
    let excluded = McpIntent {
        excluded_servers: vec![McpIntentServer {
            id: voice.id.clone(),
            name: voice.name.clone(),
        }],
        ..McpIntent::default()
    };
    clients
        .refresh_intent(&f.mcp, &f.state, &f.home, &excluded, signal.clone())
        .await
        .unwrap();
    let choices = clients.definitions(&f.mcp, &f.state, &f.home, false).await;
    assert!(!choices
        .iter()
        .any(|definition| definition["name"] == lookup));
    assert_eq!(
        choices[0]["parameters"]["properties"]["server"]["enum"],
        json!([other.name])
    );
    let disabled = McpIntent {
        mode: McpIntentMode::Disabled,
        ..McpIntent::default()
    };
    clients
        .refresh_intent(&f.mcp, &f.state, &f.home, &disabled, signal.clone())
        .await
        .unwrap();
    assert!(clients
        .definitions(&f.mcp, &f.state, &f.home, false)
        .await
        .is_empty());
    clients
        .refresh_intent(&f.mcp, &f.state, &f.home, &intent, signal)
        .await
        .unwrap();
    assert!(clients.requires_explicit_attempt());
    assert_eq!(
        fs::read_to_string(f.home.join("calls")).unwrap(),
        "lookup\n"
    );
}

#[tokio::test]
async fn harness_evaluation_invalid_mcp_arguments_are_precise_and_recoverable() {
    let f = Fixture::new();
    let server = f.local("docs");
    let (_sender, signal) = watch::channel(false);
    let mut clients = runtime::TurnClients::discover_for_user(
        &f.mcp,
        &f.state,
        &f.home,
        &f.home,
        "Use o MCP docs para consultar a documentação.",
        signal.clone(),
    )
    .await
    .unwrap();
    let lookup = runtime::wire_name(&server, "lookup");

    let invalid = clients
        .execute(
            &f.mcp,
            &f.state,
            &f.home,
            &lookup,
            &json!({"query":42, "unexpected":true}),
            false,
            signal.clone(),
        )
        .await
        .unwrap_err();
    assert_eq!(invalid.code, "mcp_invalid_arguments");
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
    assert!(!f.home.join("calls").exists());

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
        "lookup\n"
    );
    assert_runtime_report(
        "mcp-invalid-arguments",
        RuntimeReport::new(
            "completed",
            [
                ("errors", 1),
                ("serverDispatches", 1),
                ("steps", 2),
                ("toolCalls", 2),
            ],
        ),
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
    assert!(clients
        .instructions()
        .contains("one focused mcp_search_tools call"));
    assert!(clients.instructions().contains("Call it directly"));
    assert!(clients.instructions().contains("when no result matches"));
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
    assert_eq!(search["matches"][0]["loaded"], true);
    // Searching alone advertises the tool on the next inference; no load-only
    // provider round trip is required. Explicit load remains compatible.
    let discovered = clients.definitions(&f.mcp, &f.state, &f.home, false).await;
    assert!(discovered
        .iter()
        .any(|definition| definition["name"] == selected));
    let search_only = clients
        .definitions_with(&f.mcp, &f.state, &f.home, false, |name| {
            name != "mcp_load_tool"
        })
        .await;
    clients.ensure_scope_visible(&search_only).unwrap();
    assert!(search_only.iter().any(|tool| tool["name"] == selected));
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
async fn each_user_turn_has_a_fresh_mcp_catalog_and_explicit_activation_guidance() {
    let f = Fixture::new();
    let server = f.local("documents");
    let (_sender, signal) = watch::channel(false);
    let mut first = runtime::TurnClients::discover_for_user(
        &f.mcp,
        &f.state,
        &f.home,
        &f.home,
        "Consulte uma integração se precisar.",
        signal.clone(),
    )
    .await
    .unwrap();
    let initial = first.definitions(&f.mcp, &f.state, &f.home, false).await;
    assert_eq!(initial.len(), 1);
    assert_eq!(initial[0]["name"], "mcp_activate");
    first
        .execute(
            &f.mcp,
            &f.state,
            &f.home,
            "mcp_activate",
            &json!({"server":"documents"}),
            false,
            signal.clone(),
        )
        .await
        .unwrap();
    let lookup = runtime::wire_name(&server, "lookup");
    assert!(first
        .definitions(&f.mcp, &f.state, &f.home, false)
        .await
        .iter()
        .any(|definition| definition["name"] == lookup));
    drop(first);

    let mut next = runtime::TurnClients::discover_for_user(
        &f.mcp,
        &f.state,
        &f.home,
        &f.home,
        "Continue a análise anterior.",
        signal.clone(),
    )
    .await
    .unwrap();
    let definitions = next.definitions(&f.mcp, &f.state, &f.home, false).await;
    assert_eq!(definitions.len(), 1);
    assert_eq!(definitions[0]["name"], "mcp_activate");
    let guidance = next.instructions();
    assert!(guidance.contains("current user turn"));
    assert!(guidance.contains("current tool catalog is authoritative"));
    assert!(guidance.contains("historical tool name does not mean its server is active"));
    assert!(guidance.contains("mcp_search_tools and mcp_load_tool only when offered"));
    let stale = next
        .execute(
            &f.mcp,
            &f.state,
            &f.home,
            &lookup,
            &json!({"query":"old task"}),
            false,
            signal,
        )
        .await
        .unwrap_err();
    assert_eq!(stale.code, "mcp_scope_violation");
    assert!(!f.home.join("calls").exists());
    drop(next);
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
    assert_runtime_report(
        "mcp-intent-continuation",
        RuntimeReport::new(
            "completed",
            [("intentChanges", 4), ("intentResolutions", 8), ("steps", 8)],
        ),
    );
}

#[tokio::test]
async fn long_mirrored_mcp_descriptions_remain_complete_and_redacted_for_indexing() {
    let f = Fixture::new();
    let server = f.local("monday-fixture");
    let (_sender, signal) = watch::channel(false);
    let mut clients = runtime::TurnClients::discover_for_user(
        &f.mcp,
        &f.state,
        &f.home,
        &f.home,
        "Use o MCP monday fixture para consultar a descrição.",
        signal.clone(),
    )
    .await
    .unwrap();
    let output = clients
        .execute(
            &f.mcp,
            &f.state,
            &f.home,
            &runtime::wire_name(&server, "lookup"),
            &json!({"query":"mirrored-long-description"}),
            false,
            signal,
        )
        .await
        .unwrap();
    assert!(output.len() > 48_000);
    let result: Value = serde_json::from_str(&output).unwrap();
    let description = result["items"][0]["description"].as_str().unwrap();
    assert!(description.contains("Middle acceptance criterion: copper lighthouse."));
    assert!(description.ends_with("Final acceptance criterion: amber harbor."));
    assert!(description.contains("[redigido]"));
    assert!(!output.contains("fixture-sensitive-value"));
    assert_eq!(output.matches("fixture-task").count(), 1);
    assert_eq!(
        fs::read_to_string(f.home.join("calls")).unwrap(),
        "lookup\n"
    );
    drop(clients);
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
    assert_runtime_report(
        "mcp-timeout-reconnect",
        RuntimeReport::new(
            "completed",
            [
                ("errors", 1),
                ("logicalTimeMs", 1000),
                ("recoveries", 1),
                ("serverDispatches", 2),
                ("steps", 2),
                ("toolCalls", 2),
            ],
        ),
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
    let docs = f.local("docs");
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
    let lookup = runtime::wire_name(&docs, "lookup");
    // Failed registrations remain selectable for recovery, but contribute no
    // connected tool. Discovery controls do not change that isolation.
    assert_eq!(
        definitions
            .iter()
            .filter(|definition| definition["name"] != "mcp_activate")
            .map(|definition| definition["name"].as_str().unwrap())
            .collect::<Vec<_>>(),
        [lookup.as_str()]
    );
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
    let cancel = async {
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        sender.send(true).unwrap();
    };
    let args = json!({"query":"hang"});
    let (result, _) = tokio::join!(
        clients.execute(&f.mcp, &f.state, &f.home, &lookup, &args, false, signal),
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
    let mut config = fixture.mcp.config(&fixture.home, &server).unwrap();
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

#[tokio::test]
async fn plugin_relative_entrypoint_runs_in_package_cwd_with_canonical_project_environment() {
    let fixture = Fixture::new();
    let project = fixture.home.join("actual-project");
    fs::create_dir_all(&project).unwrap();
    let peer = r#"import { createInterface } from 'node:readline';
const input = createInterface({ input:process.stdin });
input.on('line', line => {
  const request = JSON.parse(line);
  if (!Object.hasOwn(request,'id')) return;
  let result;
  if (request.method === 'initialize') result = { protocolVersion:'2024-11-05',capabilities:{tools:{}},serverInfo:{name:'relative-plugin',version:'1'} };
  else if (request.method === 'tools/list') result = { tools:[{ name:'where',inputSchema:{type:'object',properties:{},additionalProperties:false} }] };
  else if (request.method === 'tools/call') result = { content:[],structuredContent:{ cwd:process.cwd(),contextProject:process.env.CONTEXT_MODE_PROJECT_DIR,claudeProject:process.env.CLAUDE_PROJECT_DIR,codexProject:process.env.CODEX_PROJECT_DIR,codexHome:process.env.CODEX_HOME,tenant:process.env.TENANT } };
  else result = {};
  process.stdout.write(JSON.stringify({jsonrpc:'2.0',id:request.id,result})+'\n');
});
input.on('close',()=>process.exit(0));"#;
    let draft = serde_json::from_value(json!({"name":"relative-context","description":"Context-mode manifest shape fixture","mcpServers":{"context-mode":{"command":"node","args":["./start.mjs"],"cwd":".","env":{"CLAUDE_PROJECT_DIR":"wrong-workspace","TENANT":"private-tenant-fixture"}}},"files":[{"path":"start.mjs","content":peer}]})).unwrap();
    let prepared = crate::plugins::preview(
        &fixture.home,
        0,
        crate::plugins::Operation::Create { draft },
    )
    .await
    .unwrap();
    crate::plugins::apply(&fixture.home, &prepared).unwrap();
    let configs = fixture.mcp.plugin_configs(&fixture.home, &project).unwrap();
    let (server, config) = configs.into_values().next().unwrap();
    let Config::Local { environment, .. } = &config else {
        panic!("local plugin")
    };
    let package_root = PathBuf::from(&environment["CODEX_PLUGIN_ROOT"])
        .canonicalize()
        .unwrap();
    let codex_home = environment["CODEX_HOME"].clone();
    let (_sender, signal) = watch::channel(false);
    let mut client = runtime::connect_with_state(
        &fixture.mcp,
        server,
        config,
        &project.join("."),
        signal.clone(),
    )
    .await
    .unwrap();
    let result: Value =
        serde_json::from_str(&client.core_call("where", &json!({}), signal).await.unwrap())
            .unwrap();
    let canonical_project = project
        .canonicalize()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    assert_eq!(
        Path::new(result["cwd"].as_str().unwrap())
            .canonicalize()
            .unwrap(),
        package_root
    );
    for key in ["contextProject", "claudeProject", "codexProject"] {
        assert_eq!(result[key], canonical_project);
    }
    assert_ne!(result["cwd"], result["contextProject"]);
    assert_eq!(result["codexHome"], codex_home);
    assert_eq!(result["tenant"], "[redigido]");
    client.close().await;
}

#[tokio::test]
async fn plugin_mcp_overlay_preserves_manual_configs_and_frozen_versions() {
    let fixture = Fixture::new();
    let manual = fixture.local("documentation");
    let initial_raw = fixture
        .mcp
        .config(&fixture.home, &manual)
        .unwrap()
        .named(&manual.name);
    let create = |argument: &str| {
        crate::plugins::Operation::Create { draft: serde_json::from_value(json!({
        "name":"plugin-fixture", "description":"Runtime fixture", "mcpServers":{"documentation":{"command":"node","args":[argument]}}, "skills":[], "files":[], "apps":{}
    })).unwrap() }
    };
    let prepared = crate::plugins::preview(&fixture.home, 0, create("first.js"))
        .await
        .unwrap();
    let installed = crate::plugins::apply(&fixture.home, &prepared).unwrap();
    let pinned = fixture
        .mcp
        .plugin_configs(&fixture.home, &fixture.home)
        .unwrap();
    assert_eq!(pinned.len(), 1);
    let (id, (old, config)) = pinned.iter().next().unwrap();
    assert!(id.starts_with("plugin-mcp:"));
    assert!((0..=(1_i64 << 53) - 1).contains(&old.revision));
    assert_ne!(old.name, manual.name);
    assert!(config.named("plugin").contains("first.js"));
    assert_eq!(
        fixture
            .mcp
            .config(&fixture.home, &manual)
            .unwrap()
            .named(&manual.name),
        initial_raw
    );
    assert_eq!(
        fixture
            .mcp
            .edit(&fixture.state, &fixture.home, id)
            .unwrap_err()
            .code,
        "plugin_owned_mcp"
    );
    let prepared = crate::plugins::preview(&fixture.home, installed.revision, create("second.js"))
        .await
        .unwrap();
    let updated = crate::plugins::apply(&fixture.home, &prepared).unwrap();
    let fresh = fixture
        .mcp
        .plugin_configs(&fixture.home, &fixture.home)
        .unwrap();
    assert_eq!(fresh.keys().next(), Some(id));
    assert_ne!(fresh[id].0.revision, old.revision);
    assert!((0..=(1_i64 << 53) - 1).contains(&fresh[id].0.revision));
    assert!(config.named("plugin").contains("first.js"));
    assert!(fixture
        .mcp
        .current_for_project(&fixture.state, &fixture.home, &fixture.home, old));
    assert!(fixture.mcp.frozen_config_current(
        &fixture.state,
        &fixture.home,
        &fixture.home,
        old,
        config
    ));
    let Config::Local { environment, .. } = config else {
        panic!("local fixture")
    };
    let frozen_root = Path::new(environment.get("CODEX_PLUGIN_ROOT").unwrap());
    fs::write(frozen_root.join("tampered.txt"), "unexpected script").unwrap();
    assert!(!fixture.mcp.frozen_config_current(
        &fixture.state,
        &fixture.home,
        &fixture.home,
        old,
        config
    ));
    let prepared = crate::plugins::preview(
        &fixture.home,
        updated.revision,
        crate::plugins::Operation::SetEnabled {
            plugin_id: updated.installed[0].id.clone(),
            enabled: false,
            project_path: None,
        },
    )
    .await
    .unwrap();
    crate::plugins::apply(&fixture.home, &prepared).unwrap();
    assert!(!fixture
        .mcp
        .current_for_project(&fixture.state, &fixture.home, &fixture.home, old));
    assert!(fixture.mcp.current(&fixture.state, &fixture.home, &manual));
}
