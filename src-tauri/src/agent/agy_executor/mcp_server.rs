//! Authenticated loopback Streamable HTTP transport. The turn owns tool execution.
use axum::{
    extract::{DefaultBodyLimit, Path as RoutePath, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::post,
    Json, Router,
};
use serde_json::{json, Value};
use std::{
    collections::HashSet,
    io::Read,
    path::{Path, PathBuf},
};
use tokio::sync::{mpsc, oneshot};

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Connection {
    port: u16,
    token: String,
    profile: Option<String>,
}

fn connection(path: &Path) -> Result<Option<Connection>, String> {
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW);
    }
    if let Ok(metadata) = std::fs::symlink_metadata(path) {
        if !metadata.is_file() || metadata.is_symlink() {
            return Err("A conexão privada AGY precisa ser um arquivo regular.".into());
        }
    }
    let file = match options.open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err("Não foi possível ler a conexão privada AGY.".into()),
    };
    let metadata = file
        .metadata()
        .map_err(|_| "Conexão privada AGY inválida.")?;
    if !metadata.is_file() || metadata.len() > 512 {
        return Err("Conexão privada AGY inválida.".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        if metadata.nlink() != 1 {
            return Err("A conexão privada AGY não aceita hard links.".into());
        }
        file.set_permissions(std::fs::Permissions::from_mode(0o600))
            .map_err(|_| "Não foi possível proteger a conexão privada AGY.")?;
    }
    let mut bytes = Vec::new();
    file.take(513)
        .read_to_end(&mut bytes)
        .map_err(|_| "Não foi possível ler a conexão privada AGY.")?;
    let value: Connection =
        serde_json::from_slice(&bytes).map_err(|_| "Conexão privada AGY inválida.")?;
    if value.port == 0
        || value.token.len() != 36
        || !value.token.bytes().enumerate().all(|(index, byte)| {
            if matches!(index, 8 | 13 | 18 | 23) {
                byte == b'-'
            } else {
                byte.is_ascii_hexdigit()
            }
        })
        || value.profile.as_ref().is_some_and(|profile| {
            profile.len() != 64 || !profile.bytes().all(|byte| byte.is_ascii_hexdigit())
        })
    {
        return Err("Conexão privada AGY inválida.".into());
    }
    Ok(Some(value))
}

pub(super) struct Request {
    pub server: Option<String>,
    pub message: Value,
    pub response: oneshot::Sender<Value>,
}

#[derive(Clone)]
struct Endpoint {
    token: String,
    host: String,
    requests: mpsc::Sender<Request>,
    aliases: HashSet<String>,
}

pub(super) struct Server {
    pub url: String,
    pub token: String,
    pub resume_compatible: bool,
    pub requests: mpsc::Receiver<Request>,
    connection: Connection,
    path: PathBuf,
    task: tokio::task::JoinHandle<()>,
}

impl Drop for Server {
    fn drop(&mut self) {
        self.task.abort();
    }
}

impl Server {
    #[cfg(test)]
    pub async fn open_for(workspace: &Path, profile: &str) -> Result<Self, String> {
        Self::open_with_aliases(workspace, profile, &[]).await
    }

    pub async fn open_with_aliases(
        workspace: &Path,
        profile: &str,
        aliases: &[String],
    ) -> Result<Self, String> {
        crate::agy::validate_mcp_aliases(aliases)?;
        crate::agy::prepare_workspace(workspace)?;
        let path = workspace.join(".bridge.json");
        let previous = connection(&path)?;
        // AGY snapshots the MCP URL and headers in its native conversation.
        let reused = if let Some(previous) = &previous {
            match tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, previous.port))
                .await
            {
                Ok(listener) => Some(listener),
                Err(error) if error.kind() == std::io::ErrorKind::AddrInUse => None,
                Err(_) => return Err("Não foi possível reabrir a ponte de ferramentas AGY.".into()),
            }
        } else {
            None
        };
        let resume_compatible = reused.is_some()
            && previous
                .as_ref()
                .is_some_and(|old| old.profile.as_deref() == Some(profile));
        let token = if reused.is_some() {
            previous.as_ref().expect("reused connection").token.clone()
        } else {
            crate::claude::new_session_id()?
        };
        let listener = match reused {
            Some(listener) => listener,
            None => tokio::net::TcpListener::bind("127.0.0.1:0")
                .await
                .map_err(|_| "Não foi possível abrir a ponte de ferramentas do Antigravity CLI.")?,
        };
        let connection = Connection {
            port: listener
                .local_addr()
                .map_err(|error| error.to_string())?
                .port(),
            token: token.clone(),
            // Commit the new profile only after the journal's resume boundary is durable.
            profile: resume_compatible.then(|| profile.to_owned()),
        };
        super::super::tools::write_atomic(
            &path,
            &serde_json::to_string(&connection).map_err(|_| "Conexão privada AGY inválida.")?,
        )
        .map_err(|_| "Não foi possível salvar a conexão privada AGY.")?;
        let host = listener
            .local_addr()
            .map_err(|error| error.to_string())?
            .to_string();
        let (send, requests) = mpsc::channel(64);
        let endpoint = Endpoint {
            token: token.clone(),
            host: host.clone(),
            requests: send,
            aliases: aliases.iter().cloned().collect(),
        };
        let app = Router::new()
            .route("/mcp", post(handle))
            .route("/mcp/{alias}", post(handle_alias))
            .layer(DefaultBodyLimit::max(4 * 1024 * 1024))
            .with_state(endpoint);
        let task = tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        Ok(Self {
            url: format!("http://{host}/mcp"),
            token,
            resume_compatible,
            requests,
            connection,
            path,
            task,
        })
    }

    pub async fn close(mut self) {
        self.task.abort();
        let _ = (&mut self.task).await;
    }

    pub fn commit_profile(&self, profile: &str) -> Result<(), String> {
        let connection = Connection {
            port: self.connection.port,
            token: self.token.clone(),
            profile: Some(profile.into()),
        };
        super::super::tools::write_atomic(
            &self.path,
            &serde_json::to_string(&connection).map_err(|_| "Conexão privada AGY inválida.")?,
        )
        .map_err(|_| "Não foi possível confirmar a conexão privada AGY.")?;
        Ok(())
    }
}

fn rpc_error(id: Value, code: i32, message: &str) -> Value {
    json!({"jsonrpc":"2.0","id":id,"error":{"code":code,"message":message}})
}

async fn handle(
    State(endpoint): State<Endpoint>,
    headers: HeaderMap,
    Json(message): Json<Value>,
) -> Response {
    handle_request(endpoint, None, headers, message).await
}

async fn handle_alias(
    State(endpoint): State<Endpoint>,
    RoutePath(alias): RoutePath<String>,
    headers: HeaderMap,
    Json(message): Json<Value>,
) -> Response {
    handle_request(endpoint, Some(alias), headers, message).await
}

async fn handle_request(
    endpoint: Endpoint,
    server: Option<String>,
    headers: HeaderMap,
    message: Value,
) -> Response {
    if headers
        .get("authorization")
        .and_then(|value| value.to_str().ok())
        != Some(format!("Bearer {}", endpoint.token).as_str())
    {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    if headers.contains_key("origin")
        || headers.get("host").and_then(|value| value.to_str().ok()) != Some(endpoint.host.as_str())
    {
        return StatusCode::FORBIDDEN.into_response();
    }
    if server
        .as_ref()
        .is_some_and(|server| !endpoint.aliases.contains(server))
    {
        return StatusCode::NOT_FOUND.into_response();
    }
    let id = message.get("id").cloned();
    if message["jsonrpc"] != "2.0"
        || !message["method"].is_string()
        || id
            .as_ref()
            .is_some_and(|id| !id.is_string() && !id.is_number())
    {
        return Json(rpc_error(Value::Null, -32600, "Invalid JSON-RPC request")).into_response();
    }
    let method = message["method"].as_str().unwrap_or_default();
    let Some(id) = id else {
        return StatusCode::ACCEPTED.into_response();
    };
    let result = match method {
        "initialize" => {
            let requested = message["params"]["protocolVersion"]
                .as_str()
                .unwrap_or_default();
            let version = if matches!(
                requested,
                "2024-11-05" | "2025-03-26" | "2025-06-18" | "2025-11-25"
            ) {
                requested
            } else {
                "2025-03-26"
            };
            json!({"jsonrpc":"2.0","id":id,"result":{"protocolVersion":version,"capabilities":{"tools":{}},"serverInfo":{"name":server.as_deref().unwrap_or("jarvis"),"version":env!("CARGO_PKG_VERSION")}}})
        }
        "ping" => json!({"jsonrpc":"2.0","id":id,"result":{}}),
        "tools/list" | "tools/call" => {
            let (send, recv) = oneshot::channel();
            if endpoint
                .requests
                .send(Request {
                    server,
                    message,
                    response: send,
                })
                .await
                .is_err()
            {
                rpc_error(id, -32603, "Jarvis turn ended")
            } else {
                recv.await
                    .unwrap_or_else(|_| rpc_error(id, -32603, "Jarvis turn ended"))
            }
        }
        _ => rpc_error(id, -32601, "Unsupported Jarvis MCP method"),
    };
    Json(result).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn loopback_requires_auth_and_origin_is_rejected_before_tool_execution() {
        let workspace = tempfile::tempdir().unwrap();
        let mut server = Server::open_for(workspace.path(), &"a".repeat(64))
            .await
            .unwrap();
        let client = reqwest::Client::new();
        let message = json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26"}});
        assert_eq!(
            client
                .post(&server.url)
                .json(&message)
                .send()
                .await
                .unwrap()
                .status(),
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            client
                .post(&server.url)
                .bearer_auth(&server.token)
                .header("Origin", "https://example.com")
                .json(&message)
                .send()
                .await
                .unwrap()
                .status(),
            StatusCode::FORBIDDEN
        );
        let initialized: Value = client
            .post(&server.url)
            .bearer_auth(&server.token)
            .json(&message)
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(initialized["result"]["protocolVersion"], "2025-03-26");
        let future = client.post(&server.url).bearer_auth(&server.token).json(&json!({"jsonrpc":"2.0","id":"tool-1","method":"tools/call","params":{"name":"tasks","arguments":{}}})).send();
        let (result, ()) = tokio::join!(future, async {
            let request = server.requests.recv().await.unwrap();
            assert!(request.server.is_none());
            assert_eq!(request.message["params"]["name"], "tasks");
            request.response.send(json!({"jsonrpc":"2.0","id":"tool-1","result":{"content":[{"type":"text","text":"done"}]}})).unwrap();
        });
        let result: Value = result.unwrap().json().await.unwrap();
        assert_eq!(result["result"]["content"][0]["text"], "done");
    }

    #[tokio::test]
    async fn authenticated_aliases_keep_the_server_identity_and_reject_unconfigured_routes() {
        let workspace = tempfile::tempdir().unwrap();
        let mut server = Server::open_with_aliases(
            workspace.path(),
            &"a".repeat(64),
            &["gemini-notebook-mcp".into()],
        )
        .await
        .unwrap();
        let client = reqwest::Client::new();
        let alias_url = format!("{}/gemini-notebook-mcp", server.url);
        let initialize = json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26"}});
        assert_eq!(
            client
                .post(&alias_url)
                .json(&initialize)
                .send()
                .await
                .unwrap()
                .status(),
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            client
                .post(&alias_url)
                .bearer_auth(&server.token)
                .header("Origin", "https://example.com")
                .json(&initialize)
                .send()
                .await
                .unwrap()
                .status(),
            StatusCode::FORBIDDEN
        );
        let initialized: Value = client
            .post(&alias_url)
            .bearer_auth(&server.token)
            .json(&initialize)
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(
            initialized["result"]["serverInfo"]["name"],
            "gemini-notebook-mcp"
        );
        for path in [
            "unknown",
            "jarvis",
            "gemini-notebook-mcp%2Fother",
            "%2F",
            "gemini-notebook-mcp/other",
        ] {
            assert_eq!(
                client
                    .post(format!("{}/{path}", server.url))
                    .bearer_auth(&server.token)
                    .json(&initialize)
                    .send()
                    .await
                    .unwrap()
                    .status(),
                StatusCode::NOT_FOUND
            );
        }
        assert!(server.requests.try_recv().is_err());
        for message in [
            json!({"jsonrpc":"2.0","id":1,"method":"tools/list"}),
            json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"list_notebooks","arguments":{}}}),
        ] {
            let future = client
                .post(&alias_url)
                .bearer_auth(&server.token)
                .json(&message)
                .send();
            let (result, ()) = tokio::join!(future, async {
                let request = server.requests.recv().await.unwrap();
                assert_eq!(request.server.as_deref(), Some("gemini-notebook-mcp"));
                assert_eq!(request.message, message);
                request
                    .response
                    .send(json!({"jsonrpc":"2.0","id":1,"result":{}}))
                    .unwrap();
            });
            assert!(result.unwrap().status().is_success());
        }
        assert!(Server::open_with_aliases(
            workspace.path(),
            &"a".repeat(64),
            &["../outside".into()]
        )
        .await
        .is_err());
    }

    #[tokio::test]
    async fn resume_reuses_private_endpoint_but_changed_profile_or_busy_port_requires_handoff() {
        let workspace = tempfile::tempdir().unwrap();
        let profile = "a".repeat(64);
        let first = Server::open_for(workspace.path(), &profile).await.unwrap();
        assert!(!first.resume_compatible);
        let url = first.url.clone();
        let token = first.token.clone();
        first.close().await;
        // A failed journal update must not make the stale native agent resumable.
        let pending = Server::open_for(workspace.path(), &profile).await.unwrap();
        assert!(!pending.resume_compatible);
        assert_eq!(pending.url, url);
        pending.commit_profile(&profile).unwrap();
        pending.close().await;
        let resumed = Server::open_for(workspace.path(), &profile).await.unwrap();
        assert!(resumed.resume_compatible);
        assert_eq!(resumed.url, url);
        assert_eq!(resumed.token, token);
        resumed.close().await;
        let edited = Server::open_for(workspace.path(), &"b".repeat(64))
            .await
            .unwrap();
        assert!(!edited.resume_compatible);
        assert_eq!(edited.url, url);
        let replacement = Server::open_for(workspace.path(), &"b".repeat(64))
            .await
            .unwrap();
        assert!(!replacement.resume_compatible);
        assert_ne!(replacement.url, url);
        assert_ne!(replacement.token, token);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(workspace.path().join(".bridge.json"))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn private_endpoint_rejects_symlinks_without_touching_target() {
        let workspace = tempfile::tempdir().unwrap();
        let target = workspace.path().join("secret");
        std::fs::write(&target, "untouched").unwrap();
        std::os::unix::fs::symlink(&target, workspace.path().join(".bridge.json")).unwrap();
        assert!(Server::open_for(workspace.path(), &"a".repeat(64))
            .await
            .is_err());
        assert_eq!(std::fs::read_to_string(target).unwrap(), "untouched");
    }

    #[tokio::test]
    #[ignore = "Uses the installed authenticated AGY CLI for two small tool calls and resume; set JARVIS_AGY_SMOKE_MODEL"]
    async fn native_cli_completes_a_scoped_mcp_tool_and_reports_its_durable_identity() {
        use crate::agy::{AgyProcess, RunOptions};
        let root = tempfile::tempdir().unwrap();
        let project = root.path().join("project");
        std::fs::create_dir(&project).unwrap();
        let mut resume: Option<String> = None;
        let mut previous_tool_ids = std::collections::HashSet::new();
        let aliases = vec!["gemini-notebook-mcp".into()];
        for (value, target, tool_name) in [
            ("agy-bridge-verified", "jarvis", "jarvis_echo"),
            (
                "agy-resume-verified",
                "gemini-notebook-mcp",
                "list_notebooks",
            ),
        ] {
            let mut server = Server::open_with_aliases(
                &root.path().join("workspace"),
                &"a".repeat(64),
                &aliases,
            )
            .await
            .unwrap();
            assert_eq!(server.resume_compatible, resume.is_some());
            server.commit_profile(&"a".repeat(64)).unwrap();
            let mut process = AgyProcess::spawn(RunOptions {
            cwd: project.clone(), workspace_dir: root.path().join("workspace"), session_id: resume.clone(),
            model: std::env::var("JARVIS_AGY_SMOKE_MODEL").expect("Set JARVIS_AGY_SMOKE_MODEL to a model in agy models"),
            effort: Some("low".into()), prompt: "You are testing a Jarvis MCP bridge with an external-server alias that also routes to Jarvis. Use only the configured MCP tools. Call the exact requested server and tool and wait for the actual tool result before answering. No project files should be read or changed.".into(),
            mcp_url: server.url.clone(), mcp_token: server.token.clone(), mcp_aliases: aliases.clone(),
        }).unwrap();
            process.send_user(&format!("Call the {target} MCP server tool {tool_name} with value {value} and then answer with the tool result. Do not answer without calling it.")).await.unwrap();
            let mut calls = 0;
            let mut native_calls = 0;
            let mut initialized = false;
            let mut observed_session: Option<String> = None;
            let mut tool_ids = std::collections::HashSet::new();
            tokio::time::timeout(std::time::Duration::from_secs(180), async {
            loop {
                tokio::select! {
                    request = server.requests.recv() => {
                        let request = request.unwrap();
                        let result = match request.message["method"].as_str() {
                            Some("tools/list") => {
                                let name = if request.server.is_some() { "list_notebooks" } else { "jarvis_echo" };
                                json!({"tools":[{"name":name,"description":"Return the provided value to test the Jarvis bridge.","inputSchema":{"type":"object","properties":{"value":{"type":"string"}},"required":["value"],"additionalProperties":false}}]})
                            }
                            Some("tools/call") => {
                                assert_eq!(request.server.as_deref().unwrap_or("jarvis"), target);
                                assert_eq!(request.message["params"]["name"], tool_name);
                                assert_eq!(request.message["params"]["arguments"]["value"], value);
                                calls += 1;
                                json!({"content":[{"type":"text","text":value}],"isError":false})
                            }
                            _ => panic!("unexpected bridge request"),
                        };
                        request.response.send(json!({"jsonrpc":"2.0","id":request.message["id"],"result":result})).unwrap();
                    }
                    event = process.next_event() => {
                        let event = event.unwrap_or_else(|error| panic!("Native round {value}: {error}")).expect("CLI exited before a terminal result");
                        if event["event"] == "init" {
                            assert_eq!(event["init"]["agent"], "jarvis-runtime");
                            observed_session = super::super::projection::session_id(&event).map(str::to_owned);
                            assert!(observed_session.is_some());
                            if let Some(resume) = &resume { assert_eq!(observed_session.as_ref(), Some(resume)); }
                            let tools = event["init"]["tools"].as_array().expect("native tool inventory");
                            assert!(tools.iter().any(|tool| tool == "call_mcp_tool"));
                            initialized = true;
                        }
                        if event["step_update"]["step_type"] == "tool" && event["step_update"]["tool_info"]["parameters"]["ServerName"].is_string() {
                            assert!(super::super::projection::session_id(&event).is_some());
                            assert!(event["step_update"]["step_index"].is_u64());
                            assert_eq!(event["step_update"]["tool_info"]["parameters"]["ServerName"], target);
                            let key = format!("{}:{}", super::super::projection::session_id(&event).unwrap(), event["step_update"]["step_index"]);
                            assert!(!previous_tool_ids.contains(&key), "Native step identity must remain unique after restart");
                            tool_ids.insert(key);
                            native_calls += 1;
                        }
                        if let Some(result) = super::super::projection::final_result(&event) {
                            result.unwrap();
                            assert!(super::super::projection::reply(&event).contains(value));
                            break;
                        }
                    }
                }
            }
        }).await.expect("native MCP smoke timed out");
            process.cancel().await.unwrap();
            server.close().await;
            assert!(initialized);
            assert_eq!(calls, 1);
            assert!(native_calls > 0);
            previous_tool_ids.extend(tool_ids);
            resume = observed_session;
            eprintln!("Native MCP round {value} completed and process ended.");
        }
        assert_eq!(std::fs::read_dir(project).unwrap().count(), 0);
    }
}
