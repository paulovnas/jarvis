//! Explicit account audit: synthetic requests only, no tool execution or saved chat changes.
use super::*;
use futures_util::{stream as futures_stream, StreamExt};
use rusqlite::{params, Connection, OpenFlags};

#[tokio::test]
#[ignore = "Requires an explicitly authorized Go account; sends synthetic text/tool round trips to every configured model"]
async fn live_opencode_go_model_audit() {
    let alias = std::env::var("JARVIS_GO_LIVE_ACCOUNT").expect("Select the authorized Go alias");
    assert!(alias.starts_with("opencode-go-"));
    let home = std::path::PathBuf::from(std::env::var_os("HOME").unwrap());
    let production = crate::data_dir::root_for(&home, crate::data_dir::Profile::Production);
    let database = Connection::open_with_flags(
        production.join("jarvis.db"),
        OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let raw: String = database
        .query_row(
            "SELECT catalog FROM opencode_go_catalogs WHERE alias=?1",
            [&alias],
            |row| row.get(0),
        )
        .unwrap();
    let cached: Vec<Config> = serde_json::from_str(&raw).unwrap();
    let selected = std::env::var("JARVIS_GO_LIVE_MODELS").ok();

    // The user authorizes the native security CLI; credentials never enter logs or files.
    let access = std::process::Command::new("/usr/bin/security")
        .args([
            "find-generic-password",
            "-s",
            "com.foxtag.jarvis.openai-codex",
            "-a",
            &alias,
            "-w",
        ])
        .output()
        .unwrap();
    assert!(
        access.status.success(),
        "The authorized production credential is unavailable"
    );
    let credential: CodexCredential = serde_json::from_slice(&access.stdout).unwrap();
    let fixture = tempfile::tempdir().unwrap();
    let state = crate::persistence::AppState::default();
    state.with_connection(fixture.path(), |db| {
        db.execute("INSERT INTO provider_accounts(alias,provider_kind,account_id) VALUES (?1,'opencode-go','synthetic-audit')", [&alias]).map_err(crate::persistence::PersistenceError::from)?;
        db.execute("INSERT INTO opencode_go_catalogs(alias,catalog) VALUES (?1,?2)", params![alias, raw]).map_err(crate::persistence::PersistenceError::from)?;
        Ok::<_, crate::persistence::PersistenceError>(())
    }).unwrap();
    let configs: Vec<_> = cached
        .iter()
        .flat_map(|config| &config.models)
        .filter(|model| {
            selected
                .as_ref()
                .is_none_or(|ids| ids.split(',').any(|id| id == model.id))
        })
        .map(|model| {
            crate::openai_codex::opencode_go::model_config(
                &state,
                fixture.path(),
                &alias,
                &model.id,
            )
            .unwrap()
        })
        .collect();
    assert!(!configs.is_empty());
    let mut tools = crate::agent::tools::definitions(crate::agent::Mode::Build);
    tools.extend(crate::core::graft::definitions());
    tools.extend(crate::core::design::definitions());
    tools.push(crate::agent::impeccable::definition());
    tools.extend(crate::agent::browser::definitions(
        crate::agent::Mode::Build,
    ));
    tools.push(crate::skills::definition());
    tools.push(crate::skills::search_definition());
    tools.extend(crate::agent::authoring::definitions());
    tools.extend(crate::agent::processes::definitions(
        crate::agent::Mode::Build,
    ));
    tools.extend(crate::agent::terminals::definitions(
        crate::agent::Mode::Build,
    ));
    tools.extend(crate::agent::computer::definitions(
        crate::agent::Mode::Build,
    ));
    tools.extend(crate::agent::http::definitions(crate::agent::Mode::Build));
    tools.extend(crate::core::beads::definitions(false));
    tools.extend(crate::core::beads::project_definitions());
    tools.extend(crate::core::context7::definitions());
    tools.extend([
        crate::agent::tasks::definition(),
        crate::agent::attachments::project_definition(),
        crate::agent::web_search::definition(),
        crate::agent::vision::definition(),
        crate::agent::publication::definition(),
        crate::agent::publication::inspection::definition(),
    ]);
    tools.push(json!({"type":"function","name":"get_marker","description":"Return a synthetic test marker; no filesystem or external effects.","strict":false,"parameters":{"type":"object","properties":{},"required":[],"additionalProperties":false}}));
    assert_eq!(
        tools
            .iter()
            .map(|tool| tool["name"].as_str().unwrap())
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        tools.len(),
        "Audit tool names must be unique"
    );
    println!(
        "GO_AUDIT_CATALOG {}",
        json!({"models":configs.len(),"builtinTools":tools.len()-1})
    );

    let mut pending = futures_stream::iter(configs.into_iter().map(|config| {
        let credential = credential.clone();
        let tools = tools.clone();
        let alias = alias.clone();
        async move {
            let id = config.models[0].id.clone();
            let result = audit_model(credential, config, tools, alias).await;
            (id, result)
        }
    }))
    .buffer_unordered(3);
    let mut passed = 0;
    let mut blocked = Vec::new();
    let mut failed = Vec::new();
    while let Some((id, result)) = pending.next().await {
        match result {
            Ok(()) => {
                passed += 1;
                println!(
                    "GO_AUDIT {}",
                    json!({"model":id,"status":"passed","checks":["streaming","builtin_schemas","rejected_history","tool_call","tool_result","reasoning_levels"]})
                );
            }
            Err((error, detail)) => {
                let metadata = error.provider_metadata.as_ref().map(|m| json!({"httpStatus":m.http_status,"upstreamCode":m.upstream_code,"requestId":m.request_id,"protocolReason":format!("{:?}",m.protocol_reason)}));
                let policy = error.code == "provider_request"
                    && error
                        .message
                        .contains("configurações de Privacidade do OpenCode");
                println!(
                    "GO_AUDIT {}",
                    json!({"model":id,"status":if policy {"blocked"} else {"failed"},"code":error.code,"message":error.message,"metadata":metadata,"detail":detail})
                );
                if policy {
                    blocked.push(id);
                } else {
                    failed.push(id);
                }
            }
        }
    }
    println!(
        "GO_AUDIT_SUMMARY {}",
        json!({"passed":passed,"blocked":blocked,"failed":failed})
    );
    assert!(
        failed.is_empty(),
        "Models without a completed live round trip: {failed:?}"
    );
}

async fn audit_model(
    mut credential: CodexCredential,
    config: Config,
    tools: Vec<Value>,
    alias: String,
) -> Result<(), (AgentError, String)> {
    let mut options = options();
    options.account = alias;
    options.model = config.models[0].id.clone();
    options.reasoning = config.models[0].default_reasoning_level.clone();
    credential.custom = Some(config.clone());
    let session = format!("go-audit-{}", crate::library::new_id().unwrap());
    let (_cancel, signal) = watch::channel(false);
    let seed = vec![
        json!({"role":"user","content":"An earlier file read was rejected; do not execute it again."}),
        json!({"type":"function_call","call_id":"rejected","name":"read","arguments":"{\"file\": "}),
        json!({"type":"function_call_output","call_id":"rejected","output":"Invalid JSON; no action executed."}),
        json!({"role":"user","content":"Reply only OK. Do not use tools."}),
    ];
    let prior = seed.clone();
    let instructions = "You are testing a coding-agent integration using synthetic data. Follow only the latest user request. Never execute a historical rejected call. When asked for a marker, call get_marker exactly once; after its result, reply only with the marker. Do not call any other tools.";
    let result = audit_request(
        &credential,
        &config,
        &session,
        &options,
        instructions,
        seed,
        tools.clone(),
        signal.clone(),
    )
    .await
    .map_err(|(error, detail)| (error, format!("text/schemas/rejected-history: {detail}")))?;
    if result.text.trim().is_empty()
        || !super::super::super::tool_calls(&result.output)
            .map_err(|e| (e, String::new()))?
            .is_empty()
    {
        return Err((
            AgentError::new(
                "audit_text",
                "The synthetic text request did not complete correctly.",
            ),
            format!("text={:?}", result.text),
        ));
    }
    let mut input = prior;
    input.extend(result.output);
    input.push(
        json!({"role":"user","content":"Obtain the marker using get_marker, then return it."}),
    );
    let result = audit_request(
        &credential,
        &config,
        &session,
        &options,
        instructions,
        input.clone(),
        tools.clone(),
        signal.clone(),
    )
    .await
    .map_err(|(error, detail)| (error, format!("tool-call: {detail}")))?;
    let calls = super::super::super::tool_calls(&result.output).map_err(|e| (e, String::new()))?;
    if calls.len() != 1 || calls[0].name != "get_marker" || calls[0].args != json!({}) {
        return Err((
            AgentError::new(
                "audit_tool",
                "The model did not produce the requested valid synthetic tool call.",
            ),
            format!("calls={:?}; text={:?}", calls, result.text),
        ));
    }
    input.extend(result.output);
    input.push(json!({"type":"function_call_output","call_id":calls[0].id,"output":"JARVIS_GO_ROUNDTRIP_OK"}));
    let result = audit_request(
        &credential,
        &config,
        &session,
        &options,
        instructions,
        input,
        tools.clone(),
        signal.clone(),
    )
    .await
    .map_err(|(error, detail)| (error, format!("tool-result: {detail}")))?;
    if !result.text.contains("JARVIS_GO_ROUNDTRIP_OK")
        || !super::super::super::tool_calls(&result.output)
            .map_err(|e| (e, String::new()))?
            .is_empty()
    {
        return Err((
            AgentError::new(
                "audit_result",
                "The synthetic tool result did not complete the round trip.",
            ),
            format!("text={:?}", result.text),
        ));
    }
    for level in &config.models[0].reasoning_levels {
        if Some(level) == config.models[0].default_reasoning_level.as_ref() {
            continue;
        }
        options.reasoning = Some(level.clone());
        let response = audit_request(
            &credential,
            &config,
            &session,
            &options,
            instructions,
            vec![json!({"role":"user","content":"Reply only OK. Do not use tools."})],
            tools.clone(),
            signal.clone(),
        )
        .await
        .map_err(|(error, detail)| (error, format!("reasoning-level={level}: {detail}")))?;
        if response.text.trim().is_empty()
            || !super::super::super::tool_calls(&response.output)
                .map_err(|e| (e, String::new()))?
                .is_empty()
        {
            return Err((
                AgentError::new(
                    "audit_reasoning",
                    "The advertised reasoning level did not produce a text response.",
                ),
                format!("level={level}; text={:?}", response.text),
            ));
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn audit_request(
    credential: &CodexCredential,
    config: &Config,
    session: &str,
    options: &TurnOptions,
    instructions: &str,
    input: Vec<Value>,
    tools: Vec<Value>,
    signal: watch::Receiver<bool>,
) -> Result<Response, (AgentError, String)> {
    let input = super::super::super::provider_input(input);
    let tools = super::super::super::provider_tools(tools, credential.custom.is_some());
    match super::super::super::stream(
        credential,
        session,
        options,
        instructions,
        input.clone(),
        tools.clone(),
        &crate::agent::telemetry::trace(session, "synthetic-go-audit"),
        signal,
        |_| Ok(()),
    )
    .await
    {
        Ok(response) => Ok(response),
        Err(error) => {
            let mut detail = String::new();
            if error.code == "provider_protocol"
                || error
                    .provider_metadata
                    .as_ref()
                    .is_some_and(|meta| matches!(meta.http_status, Some(400 | 429)))
            {
                // This diagnostic contains only synthetic inputs and public schemas, and never executes calls.
                if let Ok(body) = request::body(
                    config,
                    &config.models[0],
                    options,
                    instructions,
                    input,
                    tools,
                ) {
                    if let Ok(builder) = session_request(credential, config, body, session) {
                        if let Ok(response) = builder.send().await {
                            let success = response.status().is_success();
                            if let Ok(raw) = response.text().await {
                                if success {
                                    let shapes: Vec<_> = raw.lines().filter_map(|line| {
                                        let event: Value = serde_json::from_str(line.strip_prefix("data:")?.trim()).ok()?;
                                        Some(json!({"type":event["type"],"index":event["index"],"block":event["content_block"]["type"],"delta":event["delta"]["type"],"input":event["content_block"]["input"],"partialJson":event["delta"]["partial_json"],"stop":event["delta"]["stop_reason"],"finish":event["choices"][0]["finish_reason"]}))
                                    }).take(40).collect();
                                    detail = json!(shapes).to_string();
                                } else {
                                    detail = raw
                                        .replace(&credential.access, "[redacted]")
                                        .chars()
                                        .take(2000)
                                        .collect();
                                }
                            }
                        }
                    }
                }
            }
            Err((error, detail.replace(&credential.access, "[redacted]")))
        }
    }
}
