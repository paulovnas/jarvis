//! Credential-free MCP drafts; user-entered values go straight to secure storage.
use super::{invalid, AgentError};
use crate::mcp::config::{self, Config};
use crate::{
    mcp::{McpState, Server},
    persistence::AppState,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Draft {
    pub name: String,
    pub transport: Transport,
    pub command: Option<String>,
    pub args: Vec<String>,
    pub url: Option<String>,
    pub enabled: bool,
    pub cwd: Option<String>,
    pub env_keys: Vec<String>,
    pub header_keys: Vec<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Transport {
    Stdio,
    Http,
}

// Deliberately not Debug/Serialize: these values never belong in a transcript.
#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Values {
    pub environment: BTreeMap<String, String>,
    pub headers: BTreeMap<String, String>,
}

// The frozen configuration stays native; only its public draft reaches the model or UI.
#[derive(Clone)]
pub(super) struct Change {
    pub draft: Draft,
    previous: Option<(Server, Config)>,
}

impl Change {
    pub fn config(&self, supplied: &Values) -> Result<Config, AgentError> {
        let mut values = Values::default();
        let (environment, headers) = match self.previous.as_ref().map(|(_, config)| config) {
            Some(Config::Local { environment, .. }) if self.draft.transport == Transport::Stdio => {
                (Some(environment), None)
            }
            Some(Config::Remote { url, headers, .. })
                if self.draft.transport == Transport::Http
                    && self.draft.url.as_ref() == Some(url) =>
            {
                (None, Some(headers))
            }
            _ => (None, None),
        };
        for (keys, entered, stored, merged) in [
            (
                &self.draft.env_keys,
                &supplied.environment,
                environment,
                &mut values.environment,
            ),
            (
                &self.draft.header_keys,
                &supplied.headers,
                headers,
                &mut values.headers,
            ),
        ] {
            if entered.keys().any(|key| !keys.contains(key)) {
                return Err(invalid(
                    "Os valores privados não correspondem às chaves da proposta.",
                ));
            }
            for key in keys {
                let value = entered
                    .get(key)
                    .filter(|value| !value.trim().is_empty())
                    .or_else(|| stored.and_then(|stored| stored.get(key)))
                    .cloned()
                    .unwrap_or_default();
                merged.insert(key.clone(), value);
            }
        }
        let mut config = self.draft.config(&values)?;
        if let Some((_, previous)) = &self.previous {
            match &mut config {
                Config::Local {
                    timeout,
                    request_timeout,
                    ..
                }
                | Config::Remote {
                    timeout,
                    request_timeout,
                    ..
                } => {
                    *timeout = previous.startup_timeout().as_millis() as u64;
                    *request_timeout = previous.request_timeout().as_millis() as u64;
                }
            }
            if let (
                Config::Remote { url, oauth, .. },
                Config::Remote {
                    url: old_url,
                    oauth: old_oauth,
                    ..
                },
            ) = (&mut config, previous)
            {
                if url == old_url {
                    *oauth = *old_oauth;
                }
            }
        }
        Ok(config)
    }

    pub fn apply(
        &self,
        mcp: &McpState,
        state: &AppState,
        home: &Path,
        values: &Values,
    ) -> Result<Server, AgentError> {
        let config = self.config(values)?;
        Ok(match &self.previous {
            Some((expected, _)) => {
                mcp.update_config(state, home, expected, &self.draft.name, &config)?
            }
            None => mcp.add(state, home, &self.draft.name, &config)?,
        })
    }
}

impl Draft {
    fn from_config(name: &str, config: &Config) -> Self {
        match config {
            Config::Local {
                command,
                cwd,
                environment,
                enabled,
                ..
            } => Self {
                name: name.into(),
                transport: Transport::Stdio,
                command: command.first().cloned(),
                args: command.iter().skip(1).cloned().collect(),
                url: None,
                enabled: *enabled,
                cwd: cwd.clone(),
                env_keys: environment.keys().cloned().collect(),
                header_keys: vec![],
            },
            Config::Remote {
                url,
                headers,
                enabled,
                ..
            } => Self {
                name: name.into(),
                transport: Transport::Http,
                command: None,
                args: vec![],
                url: Some(url.clone()),
                enabled: *enabled,
                cwd: None,
                env_keys: vec![],
                header_keys: headers.keys().cloned().collect(),
            },
        }
    }
}

pub(super) fn catalog(
    mcp: &McpState,
    state: &AppState,
    home: &Path,
    id: &str,
) -> Result<Value, AgentError> {
    let (server, config) = mcp.inspect_config(state, home, id)?;
    let draft = public_draft(&server, &config)?;
    let check = server
        .last_check
        .as_ref()
        .map(|check| json!({"toolCount":check.tool_count,"error":check.error}));
    Ok(
        json!({"id":server.id,"revision":server.revision,"mutable":true,"server":draft,"lastCheck":check,
        "maintenance":"For registration changes use jarvis_propose_mcp with this exact id and revision. For local implementation changes, inspect the actual launcher/module and edit its source with the normal file/command tools; never rewrite Jarvis settings or weaken project file boundaries. If installed outside the project, use bash with sandboxPermissions=require_escalated and a precise justification through the existing approval policy. Read before changing, preserve unrelated code, and run the server's relevant tests. Then use jarvis_test_mcp to check a fresh process/connection and its complete catalog. This check does not execute audio effects or any other advertised tool."}),
    )
}

fn public_draft(server: &Server, config: &Config) -> Result<Draft, AgentError> {
    let draft = Draft::from_config(&server.name, config);
    let args = json!({"summary":"Inspect MCP", "server":draft});
    if super::sanitize_mcp_args("jarvis_propose_mcp", args.clone()) != args {
        return Err(invalid("Este MCP contém credenciais no comando ou na URL. Migre esses valores para variáveis de ambiente ou cabeçalhos nas Configurações antes de editar pelo chat."));
    }
    draft.validate()?;
    Ok(draft)
}

pub(super) fn prepare(
    mcp: &McpState,
    state: &AppState,
    home: &Path,
    tool: &super::ToolCall,
) -> Result<(super::PendingProposal, Change), AgentError> {
    if super::sanitize_mcp_args(&tool.name, tool.args.clone()) != tool.args {
        return Err(invalid("A proposta deve conter apenas a configuração pública. Use envKeys/headerKeys para preencher credenciais privadamente no painel de aprovação."));
    }
    let request: super::McpRequest = serde_json::from_value(tool.args.clone())
        .map_err(|_| invalid("Proposta de MCP inválida."))?;
    request.server.validate()?;
    let (previous, before) = if let Some(id) = request.id {
        let (server, config) = mcp.inspect_config(state, home, &id)?;
        if Some(server.revision) != request.revision {
            return Err(invalid(
                "Este MCP mudou. Consulte jarvis_catalog view=mcp e use a revisão atual.",
            ));
        }
        let before = public_draft(&server, &config)?;
        (Some((server, config)), Some(before))
    } else {
        if request.revision.is_some()
            || mcp
                .list(state, home)?
                .iter()
                .any(|server| server.name == request.server.name)
        {
            return Err(invalid("Para editar um MCP existente, consulte jarvis_catalog view=mcp e informe seu ID e revisão."));
        }
        (None, None)
    };
    let proposal = super::PendingProposal {
        turn_id: String::new(),
        tool_id: tool.id.clone(),
        action: if previous.is_some() {
            super::Action::Update
        } else {
            super::Action::Create
        },
        summary: super::bounded_summary(request.summary)?,
        catalog_revision: request.revision.map(|revision| revision as u64),
        target: super::Target::Mcp {
            server: request.server.clone(),
            before,
        },
        agent_references: vec![],
    };
    Ok((
        proposal,
        Change {
            draft: request.server,
            previous,
        },
    ))
}

impl Draft {
    pub fn validate(&self) -> Result<(), AgentError> {
        let values = Values {
            environment: self
                .env_keys
                .iter()
                .map(|k| (k.clone(), "value".into()))
                .collect(),
            headers: self
                .header_keys
                .iter()
                .map(|k| (k.clone(), "value".into()))
                .collect(),
        };
        self.config(&values).map(|_| ())
    }

    pub fn config(&self, values: &Values) -> Result<Config, AgentError> {
        for (keys, supplied) in [
            (&self.env_keys, &values.environment),
            (&self.header_keys, &values.headers),
        ] {
            let expected: BTreeSet<_> = keys.iter().collect();
            if keys.len() > 64
                || expected.len() != keys.len()
                || expected != supplied.keys().collect()
                || supplied
                    .values()
                    .any(|value| value.trim().is_empty() || value.len() > 16_384)
            {
                return Err(invalid(
                    "Preencha os valores solicitados no painel de aprovação do MCP.",
                ));
            }
        }
        let config = match self.transport {
            Transport::Stdio => {
                if self.url.is_some() || !self.header_keys.is_empty() {
                    return Err(invalid(
                        "Um MCP local usa comando e variáveis de ambiente, sem URL ou cabeçalhos.",
                    ));
                }
                let mut command = vec![self
                    .command
                    .clone()
                    .ok_or_else(|| invalid("Informe o programa do MCP local."))?];
                command.extend(self.args.clone());
                Config::Local {
                    command,
                    cwd: self.cwd.clone(),
                    environment: values.environment.clone(),
                    enabled: self.enabled,
                    timeout: 30_000,
                    request_timeout: 300_000,
                }
            }
            Transport::Http => {
                if self.command.is_some()
                    || !self.args.is_empty()
                    || self.cwd.is_some()
                    || !self.env_keys.is_empty()
                {
                    return Err(invalid(
                        "Um MCP HTTP usa URL e cabeçalhos, sem comando ou variáveis de ambiente.",
                    ));
                }
                if self.url.as_ref().is_some_and(|url| url.contains('?')) {
                    return Err(invalid("A proposta de MCP deve usar uma URL sem query. Solicite credenciais privadamente em headerKeys; URLs com query podem ser configuradas nas Configurações."));
                }
                Config::Remote {
                    url: self
                        .url
                        .clone()
                        .ok_or_else(|| invalid("Informe a URL do MCP HTTP."))?,
                    headers: values.headers.clone(),
                    oauth: Some(false),
                    enabled: self.enabled,
                    timeout: 30_000,
                    request_timeout: 300_000,
                }
            }
        };
        // Reuse the settings parser at the concrete configuration trust boundary.
        let (_, config) = config::parse(&config.named(&self.name)).map_err(AgentError::from)?;
        Ok(config)
    }
}

pub fn definition() -> Value {
    let nullable =
        |max| json!({"anyOf":[{"type":"null"},{"type":"string","minLength":1,"maxLength":max}]});
    let keys = json!({"type":"array","maxItems":64,"uniqueItems":true,"items":{"type":"string","minLength":1,"maxLength":200}});
    json!({"type":"function","name":"jarvis_propose_mcp","strict":false,"description":"Propose adding or updating one globally configured MCP server. Always waits for explicit native user approval, including in YOLO mode. Inspect jarvis_catalog first; for updates read view=mcp with its exact id and pass id+revision. Preserve unrelated fields and retained envKeys/headerKeys; existing private values are reused without sending them to the model. New or replaced values are entered privately in the panel. Never put credentials in summary, command, arguments or URL; HTTP URL queries are unsupported here. Existing OAuth is preserved for an unchanged endpoint; new OAuth setup uses Settings. This saves registration only; test with jarvis_test_mcp when available, then use mcp_activate afterwards to discover its tools. Local server source maintenance uses the ordinary file/command tools with their existing scope and approvals.","parameters":{
        "type":"object","additionalProperties":false,"required":["summary","server"],"properties":{
            "id":{"type":"string","minLength":1,"maxLength":200,"description":"Exact existing MCP ID from view=mcp. Omit for creation."},
            "revision":{"type":"integer","minimum":0,"description":"Exact existing MCP revision from view=mcp. Required with id; omit for creation."},
            "summary":{"type":"string","minLength":1,"maxLength":1000},
            "server":{"type":"object","additionalProperties":false,"required":["name","transport","command","args","url","enabled","cwd","envKeys","headerKeys"],"properties":{
                "name":{"type":"string","pattern":"^[A-Za-z0-9_-]{1,48}$"},
                "transport":{"type":"string","enum":["stdio","http"]},
                "command":nullable(4000),"args":{"type":"array","maxItems":127,"items":{"type":"string","maxLength":4000}},
                "url":nullable(8000),"enabled":{"type":"boolean"},"cwd":nullable(4000),
                "envKeys":keys,"headerKeys":keys
            }}
        }
    }})
}

pub(super) fn test_definition() -> Value {
    json!({"type":"function","name":"jarvis_test_mcp","strict":false,"description":"Test a registered MCP with a fresh process/connection, initialization and complete tool catalog, then close the test connection. Use exact id+revision from jarvis_catalog view=mcp or the accepted proposal. Requires command capability and the current turn's approval policy; never calls any advertised tool. Use after repairing registration or local server source; report actual connection/catalog errors, not success inferred from saving. Does not install, edit, authenticate via OAuth or run server unit tests.","parameters":{"type":"object","additionalProperties":false,"required":["id","revision"],"properties":{"id":{"type":"string","minLength":1,"maxLength":200},"revision":{"type":"integer","minimum":0}}}})
}

pub(super) async fn test(
    mcp: &McpState,
    state: &AppState,
    home: &Path,
    root: &Path,
    args: &Value,
    signal: tokio::sync::watch::Receiver<bool>,
) -> Result<String, AgentError> {
    if *signal.borrow() {
        return Err(AgentError::cancelled());
    }
    let id = args["id"]
        .as_str()
        .ok_or_else(|| invalid("Informe o ID do MCP a testar."))?;
    let (server, config) = mcp.inspect_config(state, home, id)?;
    if args["revision"].as_i64() != Some(server.revision) {
        return Err(invalid(
            "Este MCP mudou. Consulte o catálogo antes de testar.",
        ));
    }
    if !server.enabled || !server.configured {
        return Err(invalid(
            "Ative e configure este MCP antes de testar a conexão.",
        ));
    }
    let check =
        crate::mcp::runtime::check_connection(mcp, server.clone(), config, root, signal.clone())
            .await;
    if *signal.borrow() {
        return Err(AgentError::cancelled());
    }
    if !mcp.current_for_project(state, home, root, &server) {
        return Err(invalid(
            "Este MCP mudou durante o teste. Consulte o catálogo antes de testar novamente.",
        ));
    }
    mcp.record_check(state, home, &server, check.clone());
    mcp.refresh_after_check(&server)?;
    Ok(json!({"ok":check.error.is_none(),"id":server.id,"revision":server.revision,"check":{"toolCount":check.tool_count,"tools":check.tools.iter().take(16).collect::<Vec<_>>(),"error":check.error},"toolsAbbreviated":check.tools.len()>16,"scope":"initialization_and_tool_catalog","toolsExecuted":false,"next":"Use the newly offered MCP activation schema to load the checked registration and source in this same conversation. The previous process and schemas are invalidated before further calls."}).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn registered(raw: Value) -> (tempfile::TempDir, AppState, McpState, Server) {
        let home = tempfile::tempdir().unwrap();
        let state = AppState::default();
        let mcp = McpState::isolated_for_test();
        let (name, config) = config::parse(&raw.to_string()).unwrap();
        let server = mcp.add(&state, home.path(), &name, &config).unwrap();
        (home, state, mcp, server)
    }

    fn update_call(server: &Server, draft: Draft) -> crate::agent::ToolCall {
        crate::agent::ToolCall {
            id: "repair-mcp".into(),
            name: "jarvis_propose_mcp".into(),
            args: json!({"id":server.id,"revision":server.revision,"summary":"Corrigir a inicialização do MCP","server":draft}),
            status: "running".into(),
            output: String::new(),
            duration_ms: 0,
        }
    }

    #[test]
    fn mcp_updates_preserve_private_values_timeouts_and_identity_and_reject_stale_reviews() {
        let (home, state, mcp, server) = registered(
            json!({"audio":{"type":"local","command":["node","old.mjs"],"environment":{"TOKEN":"private-existing-token"},"timeout":7000,"requestTimeout":9000}}),
        );
        let detail = catalog(&mcp, &state, home.path(), &server.id).unwrap();
        assert!(!detail.to_string().contains("private-existing-token"));
        let mut draft: Draft = serde_json::from_value(detail["server"].clone()).unwrap();
        draft.args = vec!["repaired.mjs".into()];
        let call = update_call(&server, draft);
        jsonschema::validate(&definition()["parameters"], &call.args).unwrap();
        let (proposal, change) = prepare(&mcp, &state, home.path(), &call).unwrap();
        assert_eq!(proposal.action, super::super::Action::Update);
        assert!(!serde_json::to_string(&proposal)
            .unwrap()
            .contains("private-existing-token"));
        assert_eq!(
            mcp.inspect_config(&state, home.path(), &server.id)
                .unwrap()
                .0
                .revision,
            1
        );
        let saved = change
            .apply(&mcp, &state, home.path(), &Values::default())
            .unwrap();
        assert_eq!(saved.id, server.id);
        assert_eq!(saved.revision, 2);
        let (_, config) = mcp.inspect_config(&state, home.path(), &server.id).unwrap();
        assert_eq!(config.startup_timeout().as_millis(), 7000);
        assert_eq!(config.request_timeout().as_millis(), 9000);
        let Config::Local {
            command,
            environment,
            ..
        } = config
        else {
            panic!("Expected local config")
        };
        assert_eq!(command, ["node", "repaired.mjs"]);
        assert_eq!(environment["TOKEN"], "private-existing-token");
        assert_eq!(
            change
                .apply(&mcp, &state, home.path(), &Values::default())
                .err()
                .unwrap()
                .code,
            "stale_mcp_proposal"
        );
        assert!(prepare(&mcp, &state, home.path(), &call).is_err());
    }

    #[test]
    fn mcp_updates_preserve_oauth_but_never_forward_stored_headers_to_another_endpoint() {
        let (home, state, mcp, server) = registered(
            json!({"remote":{"type":"remote","url":"https://example.com/mcp","oauth":true,"headers":{"X-Api-Key":"private-existing-token"}}}),
        );
        let (_, config) = mcp.inspect_config(&state, home.path(), &server.id).unwrap();
        let mut draft = Draft::from_config(&server.name, &config);
        let original = Change {
            draft: draft.clone(),
            previous: Some((server.clone(), config.clone())),
        };
        let Config::Remote { oauth, headers, .. } = original.config(&Values::default()).unwrap()
        else {
            panic!("Expected HTTP config")
        };
        assert_eq!(oauth, Some(true));
        assert_eq!(headers["X-Api-Key"], "private-existing-token");
        draft.url = Some("https://other.example/mcp".into());
        let changed = Change {
            draft,
            previous: Some((server, config)),
        };
        assert!(changed.config(&Values::default()).is_err());
        let values = Values {
            environment: BTreeMap::new(),
            headers: BTreeMap::from([("X-Api-Key".into(), "replacement".into())]),
        };
        let Config::Remote { oauth, headers, .. } = changed.config(&values).unwrap() else {
            panic!("Expected HTTP config")
        };
        assert_eq!(oauth, Some(false));
        assert_eq!(headers["X-Api-Key"], "replacement");
    }

    #[test]
    fn mcp_details_never_disclose_inline_credentials_and_updates_cannot_replace_plugin_servers() {
        let (home, state, mcp, server) = registered(
            json!({"legacy":{"type":"local","command":["node","server.mjs","--token","private-inline-token"]}}),
        );
        let failure = catalog(&mcp, &state, home.path(), &server.id).unwrap_err();
        assert!(!failure.message.contains("private-inline-token"));
        assert!(catalog(&mcp, &state, home.path(), "plugin-mcp:owned").is_err());
    }

    #[tokio::test]
    async fn local_mcp_source_repair_is_verified_with_a_fresh_process_without_running_tools() {
        let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/mcp/fixtures/server.mjs");
        let (home, state, mcp, server) = registered(
            json!({"audio":{"type":"local","command":["node",script],"environment":{"EXTRA_TOOLS":"142"},"timeout":5000}}),
        );
        let project = home.path().join("project");
        std::fs::create_dir(&project).unwrap();
        let local = project.join("server.mjs");
        std::fs::copy(script, &local).unwrap();
        let (_, config) = mcp.inspect_config(&state, home.path(), &server.id).unwrap();
        let mut draft = Draft::from_config(&server.name, &config);
        draft.args = vec!["server.mjs".into()];
        let (_, change) = prepare(&mcp, &state, home.path(), &update_call(&server, draft)).unwrap();
        let saved = change
            .apply(&mcp, &state, home.path(), &Values::default())
            .unwrap();
        let (_sender, signal) = tokio::sync::watch::channel(false);
        let args = json!({"id":saved.id,"revision":saved.revision});
        let first: Value = serde_json::from_str(
            &test(&mcp, &state, home.path(), &project, &args, signal.clone())
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(first["check"]["toolCount"], 144);
        let edit = crate::agent::ToolCall {
            id: "repair-source".into(),
            name: "edit".into(),
            args: json!({"path":"server.mjs","oldText":"name: \"lookup\"","newText":"name: \"repaired_lookup\""}),
            status: "running".into(),
            output: String::new(),
            duration_ms: 0,
        };
        let root = std::fs::canonicalize(&project).unwrap();
        crate::agent::tools::execute(&root, &edit, crate::agent::Mode::Build, signal.clone())
            .await
            .unwrap();
        let second: Value = serde_json::from_str(
            &test(&mcp, &state, home.path(), &project, &args, signal)
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(second["ok"], true);
        assert_eq!(second["toolsExecuted"], false);
        let tools = second["check"]["tools"].as_array().unwrap();
        assert!(tools.contains(&json!("repaired_lookup")));
        assert!(!tools.contains(&json!("lookup")));
        assert!(test(
            &mcp,
            &state,
            home.path(),
            &project,
            &json!({"id":saved.id,"revision":1}),
            tokio::sync::watch::channel(false).1
        )
        .await
        .is_err());
    }

    #[tokio::test]
    async fn failed_selected_mcp_can_be_repaired_and_retested_without_an_initial_client() {
        let (home, state, mcp, server) =
            registered(json!({"audio":{"type":"local","command":["node","missing.mjs"]}}));
        let intent: crate::mcp::McpIntent = serde_json::from_value(
            json!({"mode":"explicit","servers":[{"id":server.id,"name":server.name}]}),
        )
        .unwrap();
        let (_sender, signal) = tokio::sync::watch::channel(false);
        let mut clients = crate::mcp::runtime::TurnClients::discover_for_intent(
            &mcp,
            &state,
            home.path(),
            home.path(),
            &intent,
            signal.clone(),
        )
        .await
        .unwrap();
        let mut other_chat = crate::mcp::runtime::TurnClients::discover_for_intent(
            &mcp,
            &state,
            home.path(),
            home.path(),
            &intent,
            signal.clone(),
        )
        .await
        .unwrap();
        let args = json!({"id":server.id,"revision":server.revision});
        let failed = test(
            &mcp,
            &state,
            home.path(),
            home.path(),
            &args,
            signal.clone(),
        )
        .await
        .unwrap();
        assert_eq!(serde_json::from_str::<Value>(&failed).unwrap()["ok"], false);
        clients.observe_connection_test("jarvis_test_mcp", &args, &failed);
        let (_, mut config) = mcp.inspect_config(&state, home.path(), &server.id).unwrap();
        let Config::Local { command, .. } = &mut config else {
            panic!()
        };
        command[1] = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src/mcp/fixtures/server.mjs")
            .to_string_lossy()
            .into_owned();
        let current = mcp
            .update_config(&state, home.path(), &server, &server.name, &config)
            .unwrap();
        let current_args = json!({"id":current.id,"revision":current.revision});
        let checked = test(
            &mcp,
            &state,
            home.path(),
            home.path(),
            &current_args,
            signal.clone(),
        )
        .await
        .unwrap();
        assert_eq!(serde_json::from_str::<Value>(&checked).unwrap()["ok"], true);
        for chat in [&mut clients, &mut other_chat] {
            let definitions = chat.definitions(&mcp, &state, home.path(), false).await;
            assert_eq!(
                definitions
                    .iter()
                    .find(|tool| tool["name"] == "mcp_activate")
                    .unwrap()["parameters"]["properties"]["server"]["enum"],
                json!(["audio"])
            );
            chat.execute(
                &mcp,
                &state,
                home.path(),
                "mcp_activate",
                &json!({"server":"audio"}),
                false,
                signal.clone(),
            )
            .await
            .unwrap();
            assert!(!chat.instructions().contains("could not initialize"));
        }
        assert!(
            !clients.requires_explicit_attempt(),
            "the real failed test counts as this chat's diagnostic attempt"
        );
        assert!(
            other_chat.requires_explicit_attempt(),
            "another chat's test never counts as a local attempt"
        );
        other_chat.observe_connection_test("jarvis_test_mcp", &current_args, "{}");
        other_chat.observe_connection_test(
            "jarvis_test_mcp",
            &json!({"id":"unrelated","revision":current.revision}),
            &checked,
        );
        assert!(other_chat.requires_explicit_attempt());
        assert!(test(
            &mcp,
            &state,
            home.path(),
            home.path(),
            &args,
            signal.clone()
        )
        .await
        .is_err());
        assert!(test(
            &mcp,
            &state,
            home.path(),
            home.path(),
            &current_args,
            tokio::sync::watch::channel(true).1
        )
        .await
        .is_err());
        other_chat.observe_connection_test("jarvis_test_mcp", &current_args, &checked);
        assert!(!other_chat.requires_explicit_attempt());
        drop(clients);
        drop(other_chat);
    }

    #[tokio::test]
    async fn cancelled_mcp_diagnostics_do_not_publish_a_test_receipt_or_check() {
        let (home, state, mcp, server) = registered(
            json!({"audio":{"type":"local","command":["node",std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/mcp/fixtures/server.mjs")],"environment":{"HANG_INITIALIZE":"1"}}}),
        );
        let (sender, signal) = tokio::sync::watch::channel(false);
        let cancellation = tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            sender.send(true).unwrap();
        });
        let error = test(
            &mcp,
            &state,
            home.path(),
            home.path(),
            &json!({"id":server.id,"revision":server.revision}),
            signal,
        )
        .await
        .unwrap_err();
        cancellation.await.unwrap();
        assert_eq!(error.code, "cancelled");
        assert_eq!(mcp.connection_check(&server.id), (0, 0));
        assert!(mcp.list(&state, home.path()).unwrap()[0]
            .last_check
            .is_none());
    }

    fn draft() -> Draft {
        serde_json::from_value(json!({"name":"firebase","transport":"stdio","command":"npx","args":["-y","firebase-tools","mcp"],"url":null,"enabled":true,"cwd":null,"envKeys":["TOKEN"],"headerKeys":[]})).unwrap()
    }

    #[test]
    fn only_exact_private_values_create_a_valid_config_and_preview_stays_secret_free() {
        let server = draft();
        server.validate().unwrap();
        assert!(server.config(&Values::default()).is_err());
        let values = Values {
            environment: BTreeMap::from([("TOKEN".into(), "private-value".into())]),
            headers: BTreeMap::new(),
        };
        let config = server.config(&values).unwrap();
        assert!(config.named("firebase").contains("private-value"));
        assert!(!serde_json::to_string(&server)
            .unwrap()
            .contains("private-value"));
        let extra = Values {
            environment: BTreeMap::from([("OTHER".into(), "private-value".into())]),
            headers: BTreeMap::new(),
        };
        assert!(server.config(&extra).is_err());
    }

    #[test]
    fn drafts_reuse_native_validation_and_reject_cross_transport_fields_and_duplicates() {
        let mut server = draft();
        server.env_keys.push("TOKEN".into());
        assert!(server.validate().is_err());
        server.env_keys.clear();
        server.url = Some("https://example.com/mcp".into());
        assert!(server.validate().is_err());
        server.url = None;
        server.name = "../bad".into();
        assert!(server.validate().is_err());
        let bad = json!({"name":"remote","transport":"http","command":null,"args":[],"url":"https://example.com/mcp","enabled":true,"cwd":null,"envKeys":[],"headerKeys":["mcp-session-id"]});
        assert!(serde_json::from_value::<Draft>(bad)
            .unwrap()
            .validate()
            .is_err());
        let query = json!({"name":"remote","transport":"http","command":null,"args":[],"url":"https://example.com/mcp?api_key=private-value","enabled":true,"cwd":null,"envKeys":[],"headerKeys":[]});
        assert!(serde_json::from_value::<Draft>(query)
            .unwrap()
            .validate()
            .is_err());
    }
}
