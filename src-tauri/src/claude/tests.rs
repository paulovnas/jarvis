use super::*;
use serde_json::{json, Value};
use std::{path::Path, time::Duration};
use tokio::io::BufReader;

pub(super) fn fixture(script: &str) -> ClaudeProcess {
    let mut command = crate::background::tokio_command("node");
    command.args(["-e", script]);
    transport::ClaudeProcess::spawn_command(command, vec![]).unwrap()
}

async fn event(process: &mut ClaudeProcess) -> Value {
    tokio::time::timeout(Duration::from_secs(5), process.next_event())
        .await
        .unwrap()
        .unwrap()
        .unwrap()
}

#[test]
fn executor_and_selection_keep_legacy_jarvis_defaults() {
    assert!(Executor::default().is_jarvis());
    assert_eq!(serde_json::to_value(Executor::Claude).unwrap(), "claude");
    assert!(validate_selection("claude-opus-custom", Some("max")).is_ok());
    for model in ["", "--permission-mode=bypassPermissions", "opus\n"] {
        assert!(validate_selection(model, None).is_err());
    }
    assert!(validate_selection("opus", Some("ultra")).is_err());
    let id = new_session_id().unwrap();
    assert!(transport::valid_session_id(&id));
    assert_eq!(&id[14..15], "4");
}

#[test]
fn launch_preserves_native_auth_and_uses_only_the_scoped_jarvis_tools() {
    let directory = tempfile::tempdir().unwrap();
    let mut options = RunOptions {
        cwd: directory.path().to_owned(),
        session_id: new_session_id().unwrap(),
        resume: false,
        model: "opus".into(),
        effort: Some("high".into()),
        append_system_prompt:
            "Keep the selected Designer role.\nDo not replace native instructions.".into(),
        mcp_servers: json!({"jarvis":{"type":"sdk","name":"jarvis"}}),
    };
    let (command, files) =
        transport::command_for(Path::new("/cli/claude"), &options, false).unwrap();
    let arguments: Vec<_> = command
        .as_std()
        .get_args()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect();
    assert!(arguments.windows(2).any(|pair| pair == ["--tools", ""]));
    assert!(arguments
        .windows(2)
        .any(|pair| pair == ["--permission-mode", "default"]));
    assert!(arguments
        .windows(2)
        .any(|pair| pair == ["--permission-prompt-tool", "stdio"]));
    assert!(arguments.contains(&"--strict-mcp-config".into()));
    assert!(arguments
        .windows(2)
        .any(|pair| pair == ["--settings", "{\"disableAllHooks\":true}"]));
    assert!(arguments.contains(&format!("--session-id={}", options.session_id)));
    assert!(!arguments
        .iter()
        .any(|arg| arg.contains("bypassPermissions")));
    assert!(!arguments.contains(&"--system-prompt".into()));
    assert_eq!(
        std::fs::read_to_string(files[0].path()).unwrap(),
        options.append_system_prompt
    );
    assert_eq!(
        serde_json::from_slice::<Value>(&std::fs::read(files[1].path()).unwrap()).unwrap(),
        json!({"mcpServers":options.mcp_servers})
    );
    assert!(command
        .as_std()
        .get_envs()
        .all(|(name, _)| !name.to_string_lossy().starts_with("ANTHROPIC_")));
    assert!(!directory.path().join(".mcp.json").exists());
    options.resume = true;
    let (command, _) = transport::command_for(Path::new("/cli/claude"), &options, false).unwrap();
    let arguments: Vec<_> = command
        .as_std()
        .get_args()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect();
    assert!(arguments.contains(&format!("--resume={}", options.session_id)));
    assert!(!arguments.iter().any(|arg| arg.starts_with("--session-id")));
}

#[test]
fn metadata_catalog_uses_reported_models_effort_and_safe_account_fields() {
    let models = metadata::models_from_initialize(&json!({"models":[
        {"value":"default","displayName":"Default","supportedEffortLevels":["low","medium","high","xhigh","max"]},
        {"value":"new-model","displayName":"New Model","description":"From native catalog","supportedEffortLevels":["low","high","max"],"defaultEffortLevel":"high"},
        {"value":"old-model","displayName":"Old Model"},
        {"value":"--injected","displayName":"Invalid"},
        {"value":"new-model","displayName":"Duplicate"}
    ]}));
    assert_eq!(
        models
            .iter()
            .map(|model| model.id.as_str())
            .collect::<Vec<_>>(),
        ["default", "new-model", "old-model"]
    );
    assert_eq!(models[1].reasoning_levels, ["low", "high", "max"]);
    assert_eq!(
        models[0].reasoning_levels,
        ["low", "medium", "high", "xhigh", "max"]
    );
    assert_eq!(models[1].default_reasoning.as_deref(), Some("high"));
    assert!(models[2].reasoning_levels.is_empty());
    let mut status = metadata::RuntimeStatus {
        preferences: ProviderPreferences::default(),
        installed: true,
        authenticated: false,
        version: None,
        models,
        error: None,
        auth_method: None,
        email: None,
        subscription_type: None,
    };
    metadata::account_from_status(
        &mut status,
        &json!({
            "loggedIn":true,"authMethod":"claude.ai","email":"test@example.invalid",
            "subscriptionType":"max","accessToken":"never-expose-this"
        }),
    );
    assert!(status.authenticated);
    assert_eq!(status.auth_method.as_deref(), Some("claude.ai"));
    assert!(!serde_json::to_string(&status)
        .unwrap()
        .contains("never-expose-this"));
}

#[tokio::test]
async fn initialize_is_metadata_only_and_mcp_controls_remain_bidirectional() {
    let mut process = fixture(
        r#"
        const lines = require('node:readline').createInterface({ input: process.stdin });
        const send = value => process.stdout.write(JSON.stringify(value) + '\n');
        let initialization;
        lines.on('line', line => {
            const message = JSON.parse(line);
            if (message.type === 'user') { process.exit(99); }
            if (message.type === 'control_request') {
                initialization = message.request_id;
                send({type:'control_request',request_id:'mcp-1',request:{subtype:'mcp_message',server_name:'jarvis',message:{jsonrpc:'2.0',id:1,method:'initialize',params:{protocolVersion:'2025-03-26'}}}});
            } else if (message.type === 'control_response') {
                if (message.response.request_id !== 'mcp-1' || !message.response.response.mcp_response.result) process.exit(98);
                send({type:'control_response',response:{subtype:'success',request_id:initialization,response:{models:[{value:'fixture-model',displayName:'Fixture model'}]}}});
            }
        });
        lines.on('close', () => process.exit(0));
    "#,
    );
    let control = process.control();
    let initialize = control.initialize(json!({}));
    tokio::pin!(initialize);
    let result = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            tokio::select! {
                result = &mut initialize => break result.unwrap(),
                next = process.next_event() => {
                    let next = next.unwrap().unwrap();
                    assert_eq!(next["request"]["server_name"], "jarvis");
                    control.respond_control(next["request_id"].as_str().unwrap(), Ok(json!({"mcp_response":{"jsonrpc":"2.0","id":1,"result":{}}}))).await.unwrap();
                }
            }
        }
    }).await.unwrap();
    assert_eq!(result["models"][0]["value"], "fixture-model");
    control.close().await;
    assert!(tokio::time::timeout(Duration::from_secs(5), process.wait())
        .await
        .unwrap()
        .unwrap()
        .success());
}

#[tokio::test]
async fn raw_events_preserve_sidechains_and_responses_cannot_approve_cancelled_requests() {
    let mut process = fixture(
        r#"
        const lines = require('node:readline').createInterface({ input: process.stdin });
        const send = value => process.stdout.write(JSON.stringify(value) + '\n');
        lines.on('line', line => {
            const message = JSON.parse(line);
            if (message.type === 'user') {
                send({type:'assistant',parent_tool_use_id:'child-1',message:{stop_reason:'end_turn',content:[{type:'text',text:'Child done'}]}});
                send({type:'control_request',request_id:'cancelled',request:{subtype:'can_use_tool',tool_name:'mcp__jarvis__write',input:{path:'a'}}});
                send({type:'control_cancel_request',request_id:'cancelled'});
                send({type:'control_request',request_id:'current',request:{subtype:'can_use_tool',tool_name:'mcp__jarvis__write',input:{path:'b'}}});
            } else if (message.type === 'control_response') {
                if (message.response.request_id !== 'current') process.exit(97);
                send({type:'result',subtype:'success',is_error:false,parent_tool_use_id:null,result:'Parent done',observed:message.response.response});
            }
        });
        lines.on('close', () => process.exit(0));
    "#,
    );
    let control = process.control();
    control
        .send_user(
            json!("Implement the requested change"),
            Some("user-1".into()),
        )
        .await
        .unwrap();
    let child = event(&mut process).await;
    assert_eq!(child["parent_tool_use_id"], "child-1");
    assert_eq!(event(&mut process).await["request_id"], "cancelled");
    assert_eq!(event(&mut process).await["type"], "control_cancel_request");
    assert!(
        !control.is_pending("cancelled").await,
        "a cancelled callback queued behind another operation cannot be admitted"
    );
    assert!(control
        .respond_control("cancelled", Ok(json!({"behavior":"allow"})))
        .await
        .is_err());
    assert_eq!(event(&mut process).await["request_id"], "current");
    assert!(control.is_pending("current").await);
    control
        .respond_control(
            "current",
            Ok(json!({"behavior":"allow","updatedInput":{"path":"b"}})),
        )
        .await
        .unwrap();
    assert!(!control.is_pending("current").await);
    assert!(control
        .respond_control("current", Ok(json!({"behavior":"allow"})))
        .await
        .is_err());
    let parent = event(&mut process).await;
    assert_eq!(parent["type"], "result");
    assert_eq!(parent["observed"]["updatedInput"]["path"], "b");
    control.close().await;
    assert!(process.wait().await.unwrap().success());
}

#[tokio::test]
async fn malformed_output_fails_with_bounded_error_and_eof_releases_control_waiters() {
    let mut malformed = fixture("process.stdout.write('not-json\\n');");
    assert!(malformed
        .next_event()
        .await
        .unwrap_err()
        .contains("estruturada inválida"));
    let mut process = fixture(
        r#"
        require('node:readline').createInterface({input:process.stdin}).once('line', () => process.exit(2));
    "#,
    );
    let result = tokio::time::timeout(
        Duration::from_secs(5),
        process.control().initialize(json!({})),
    )
    .await
    .unwrap();
    assert!(result.unwrap_err().contains("encerrou"));
    assert!(!process.wait().await.unwrap().success());
    let mut reader = BufReader::new(&b"{\"type\":\"result\"}\n{\"type\":\"system\"}"[..]);
    assert_eq!(
        transport::read_frame(&mut reader).await.unwrap().unwrap(),
        b"{\"type\":\"result\"}\n"
    );
    assert_eq!(
        transport::read_frame(&mut reader).await.unwrap().unwrap(),
        b"{\"type\":\"system\"}"
    );
    assert!(transport::read_frame(&mut reader).await.unwrap().is_none());
}

#[cfg(unix)]
#[tokio::test]
async fn cancellation_and_drop_stop_the_owned_process_group() {
    for cancel in [true, false] {
        let mut process = fixture(
            r#"
            const child = require('node:child_process').spawn(process.execPath, ['-e','setInterval(() => {}, 1000)'], {stdio:'ignore'});
            child.once('spawn', () => process.stdout.write(JSON.stringify({type:'fixture',pid:process.pid,child:child.pid})+'\n'));
            setInterval(() => {}, 1000);
        "#,
        );
        let pids = event(&mut process).await;
        let parent = pids["pid"].as_i64().unwrap() as libc::pid_t;
        let child = pids["child"].as_i64().unwrap() as libc::pid_t;
        assert_eq!(unsafe { libc::getpgid(parent) }, parent);
        assert_eq!(unsafe { libc::getpgid(child) }, parent);
        if cancel {
            process.cancel().await.unwrap();
        }
        drop(process);
        tokio::time::timeout(Duration::from_secs(5), async {
            while unsafe { libc::kill(parent, 0) } == 0 || unsafe { libc::kill(child, 0) } == 0 {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .expect("owned parent and descendant should terminate");
    }
}

#[tokio::test]
async fn blocked_stdin_is_bounded_and_cancellation_does_not_wait_for_the_writer() {
    let mut process = fixture(
        "process.stdout.write(JSON.stringify({type:'ready'})+'\\n'); setInterval(() => {}, 1000);",
    );
    assert_eq!(event(&mut process).await["type"], "ready");
    let control = process.control();
    let error = tokio::time::timeout(
        Duration::from_secs(2),
        control.write_with_timeout(
            json!({"type":"user","payload":"x".repeat(2 * 1024 * 1024)}),
            Duration::from_millis(100),
        ),
    )
    .await
    .unwrap()
    .unwrap_err();
    assert!(error.contains("no prazo"));
    process.cancel().await.unwrap();

    let mut process = fixture(
        "process.stdout.write(JSON.stringify({type:'ready'})+'\\n'); setInterval(() => {}, 1000);",
    );
    assert_eq!(event(&mut process).await["type"], "ready");
    let control = process.control();
    let writer_control = control.clone();
    let writer = tokio::spawn(async move {
        writer_control
            .send_user(json!("x".repeat(2 * 1024 * 1024)), None)
            .await
    });
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(!writer.is_finished(), "the synthetic CLI never reads stdin");
    let contender = control.write_with_timeout(json!({"type":"probe"}), Duration::from_millis(50));
    assert!(tokio::time::timeout(Duration::from_secs(2), contender)
        .await
        .unwrap()
        .unwrap_err()
        .contains("no prazo"));
    tokio::time::timeout(Duration::from_secs(2), process.cancel())
        .await
        .unwrap()
        .unwrap();
    assert!(tokio::time::timeout(Duration::from_secs(2), writer)
        .await
        .unwrap()
        .unwrap()
        .is_err());
}
