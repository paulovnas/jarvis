use super::*;
use serde_json::Value;

#[test]
fn agy_is_optional_and_selections_use_the_native_effort_contract() {
    let preferences = ProviderPreferences::default();
    assert!(!preferences.enabled);
    assert!(preferences.show_usage);
    assert!(!preferences.allows("gemini-3.8-flash-high"));
    let enabled = ProviderPreferences {
        enabled: true,
        disabled_models: vec!["hidden".into()],
        ..preferences
    };
    assert!(enabled.allows("gemini-3.8-flash-high"));
    assert!(!enabled.allows("hidden"));
    assert!(enabled.validate().is_ok());
    for model in ["", "--model=unsafe", "gemini\nextra"] {
        assert!(validate_selection(model, None).is_err());
    }
    assert!(validate_selection("gemini-3.8-flash-high", Some("max")).is_ok());
    assert!(validate_selection("gemini-3.8-flash-high", Some("xhigh")).is_err());
}

#[test]
fn catalog_uses_cli_slugs_labels_and_ignores_noise_duplicates_and_invalid_rows() {
    let models = metadata::parse_models("Fetching available models...\ngemini-3.8-flash-high\tGemini 3.8 Flash (High)\nclaude-opus-4-6-thinking\tClaude Opus 4.6 (Thinking)\ngemini-3.8-flash-high\tDuplicate\n--invalid\tInvalid\n\tempty\n");
    assert_eq!(models.len(), 2);
    assert_eq!(models[0].id, "gemini-3.8-flash-high");
    assert_eq!(models[0].name, "Gemini 3.8 Flash (High)");
    assert_eq!(models[1].id, "claude-opus-4-6-thinking");
    assert_eq!(models[0].reasoning_levels, ["low", "medium", "high", "max"]);
    assert!(models[0].default_reasoning.is_none());
}

fn options(root: &std::path::Path) -> RunOptions {
    let cwd = root.join("project");
    std::fs::create_dir(&cwd).unwrap();
    RunOptions {
        cwd,
        workspace_dir: root.join("runtime"),
        session_id: Some("abc123-session".into()),
        model: "gemini-3.8-flash-high".into(),
        effort: Some("high".into()),
        prompt: "Keep user instructions and use Jarvis tools.".into(),
        mcp_url: "http://127.0.0.1:43210/mcp".into(),
        mcp_token: "private-run-token".into(),
    }
}

#[test]
fn transport_isolates_agent_mcp_and_keeps_native_login_and_project_untouched() {
    let root = tempfile::tempdir().unwrap();
    let options = options(root.path());
    let (command, path) = transport::command_for(std::path::Path::new("agy"), &options).unwrap();
    let command = command.as_std();
    assert_eq!(
        command.get_current_dir(),
        Some(options.workspace_dir.as_path())
    );
    let args: Vec<_> = command
        .get_args()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect();
    assert!(args.contains(&"--dangerously-skip-permissions".into()));
    assert!(args.contains(&"--conversation=abc123-session".into()));
    assert!(args.contains(&"--add-dir".into()));
    assert!(args.contains(&options.cwd.to_string_lossy().into_owned()));
    assert!(!command
        .get_envs()
        .any(|(key, _)| matches!(key.to_str(), Some("HOME" | "USERPROFILE"))));
    assert_eq!(std::fs::read_dir(&options.cwd).unwrap().count(), 0);
    let document = std::fs::read_to_string(&path).unwrap();
    let header: Value = serde_yaml_ng::from_str(document.split("---\n").nth(1).unwrap()).unwrap();
    assert_eq!(header["excludeDefaultComponents"], true);
    assert_eq!(header["inheritMcp"], false);
    // The MCP dispatcher is injected by mcpServers, not a registry component.
    assert!(header.get("tools").is_none());
    assert_eq!(header["mcpServers"].as_array().unwrap().len(), 1);
    assert_eq!(header["mcpServers"][0]["serverUrl"], options.mcp_url);
    assert_eq!(
        header["mcpServers"][0]["headers"]["Authorization"],
        "Bearer private-run-token"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(
            std::fs::metadata(&options.workspace_dir)
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
    }
}

#[test]
fn transport_rejects_external_bridges_and_argument_or_header_injection() {
    let root = tempfile::tempdir().unwrap();
    let mut options = options(root.path());
    options.mcp_url = "https://example.com/mcp".into();
    assert!(transport::command_for(std::path::Path::new("agy"), &options).is_err());
    options.mcp_url = "http://127.0.0.1:43210/mcp".into();
    options.mcp_token = "token\nAnother-Header: unsafe".into();
    assert!(transport::command_for(std::path::Path::new("agy"), &options).is_err());
    options.mcp_token = "valid".into();
    options.session_id = Some("--other-flag".into());
    // Slug-like IDs may contain hyphens but remain one --conversation= argument.
    let (command, _) = transport::command_for(std::path::Path::new("agy"), &options).unwrap();
    assert!(command
        .as_std()
        .get_args()
        .any(|arg| arg == "--conversation=--other-flag"));
    options.workspace_dir = options.cwd.join(".agents");
    assert!(transport::command_for(std::path::Path::new("agy"), &options).is_err());
}

#[test]
fn tool_free_generation_excludes_native_tools_and_rejects_partial_bridge_configuration() {
    let root = tempfile::tempdir().unwrap();
    let mut options = options(root.path());
    options.mcp_url.clear();
    options.mcp_token.clear();
    let (_, path) = transport::command_for(std::path::Path::new("agy"), &options).unwrap();
    let document = std::fs::read_to_string(path).unwrap();
    let header: Value = serde_yaml_ng::from_str(document.split("---\n").nth(1).unwrap()).unwrap();
    assert_eq!(header["excludeDefaultComponents"], true);
    assert_eq!(header["inheritMcp"], false);
    assert_eq!(header["mcpServers"], serde_json::json!([]));
    assert_eq!(header["tools"], serde_json::json!([]));
    options.mcp_token = "private-run-token".into();
    assert!(transport::command_for(std::path::Path::new("agy"), &options).is_err());
    options.mcp_url = "http://127.0.0.1:43210/mcp".into();
    options.mcp_token.clear();
    assert!(transport::command_for(std::path::Path::new("agy"), &options).is_err());
}

#[cfg(unix)]
#[test]
fn generated_configuration_rejects_symlinked_subdirectories() {
    let root = tempfile::tempdir().unwrap();
    let options = options(root.path());
    std::fs::create_dir(&options.workspace_dir).unwrap();
    std::os::unix::fs::symlink(&options.cwd, options.workspace_dir.join(".agents")).unwrap();
    assert!(transport::command_for(std::path::Path::new("agy"), &options).is_err());
    assert_eq!(std::fs::read_dir(&options.cwd).unwrap().count(), 0);
}

#[cfg(unix)]
#[test]
fn private_workspace_alias_cannot_write_configuration_inside_the_project() {
    let root = tempfile::tempdir().unwrap();
    let mut options = options(root.path());
    let alias = root.path().join("runtime-parent");
    std::os::unix::fs::symlink(&options.cwd, &alias).unwrap();
    options.workspace_dir = alias.join("new-runtime");
    assert!(transport::command_for(std::path::Path::new("agy"), &options).is_err());
    assert_eq!(std::fs::read_dir(&options.cwd).unwrap().count(), 0);
}

#[tokio::test]
#[ignore = "Requires an installed AGY CLI; only enumerates custom agents, no inference"]
async fn native_cli_accepts_isolated_remote_mcp_agent_contract() {
    let executable = metadata::executable().unwrap();
    let root = tempfile::tempdir().unwrap();
    let options = options(root.path());
    let (_, _) = transport::command_for(&executable, &options).unwrap();
    // The bare agents subcommand omits workspace customizations in AGY 1.2.13.
    // The native /agents command initializes the workspace and reports zero model turns.
    let result = metadata::probe(
        &executable,
        &options.workspace_dir,
        &[
            "-p=/agents",
            "--output-format",
            "json",
            "--agent",
            transport::AGENT_NAME,
        ],
    )
    .await
    .unwrap();
    let result: Value = serde_json::from_str(result.trim()).unwrap();
    assert_eq!(result["status"], "SUCCESS");
    assert_eq!(result["num_turns"], 0);
    assert_eq!(result["command"]["name"], "agents");
    assert!(
        result["command"]["data"]["agents"]
            .as_array()
            .unwrap()
            .iter()
            .any(|agent| agent == transport::AGENT_NAME),
        "The CLI did not load the generated custom agent"
    );
}
