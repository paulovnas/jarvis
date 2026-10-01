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
fn catalog_groups_native_effort_variants_without_inventing_capabilities() {
    let models = metadata::parse_models("Fetching available models...\ngemini-3.8-flash-high\tGemini 3.8 Flash (High)\nclaude-opus-4-6-thinking\tClaude Opus 4.6 (Thinking)\ngemini-3.8-flash-medium\tGemini 3.8 Flash (Medium)\ngemini-3.8-flash-low\tGemini 3.8 Flash (Low)\ngemini-3.1-pro-high\tGemini 3.1 Pro (High)\ngemini-3.1-pro-low\tGemini 3.1 Pro (Low)\ngpt-oss-120b-medium\tGPT-OSS 120B (Medium)\ngemini-3.8-flash-high\tDuplicate\n--invalid\tInvalid\n\tempty\n");
    assert_eq!(models.len(), 4);
    assert_eq!(models[0].id, "gemini-3.8-flash");
    assert_eq!(models[0].name, "Gemini 3.8 Flash");
    assert_eq!(models[1].id, "claude-opus-4-6-thinking");
    assert_eq!(models[0].reasoning_levels, ["low", "medium", "high"]);
    assert_eq!(models[0].default_reasoning.as_deref(), Some("high"));
    assert!(models[1].reasoning_levels.is_empty());
    assert!(models[1].default_reasoning.is_none());
    assert_eq!(models[2].reasoning_levels, ["low", "high"]);
    assert_eq!(models[3].id, "gpt-oss-120b");
    assert_eq!(models[3].reasoning_levels, ["medium"]);
}

#[test]
fn grouped_catalog_preserves_legacy_visibility_and_blocks_hidden_base_variants() {
    let models = metadata::parse_models(
        "gemini-high\tGemini (High)\ngemini-medium\tGemini (Medium)\ngemini-low\tGemini (Low)\n",
    );
    let preferences = ProviderPreferences {
        enabled: true,
        disabled_models: vec![
            "gemini-medium".into(),
            "gemini-low".into(),
            "retired-low".into(),
        ],
        ..Default::default()
    };
    let grouped = preferences.clone().for_models(&models);
    assert_eq!(grouped.disabled_models, ["retired-low"]);
    assert!(grouped.allows("gemini"));
    let mut hidden = preferences;
    hidden.disabled_models.push("gemini-high".into());
    let grouped = hidden.for_models(&models);
    assert_eq!(grouped.disabled_models, ["retired-low", "gemini"]);
    assert!(!grouped.allows("gemini"));
    assert!(!grouped.allows("gemini-high"));
    assert_eq!(grouped.clone().for_models(&models), grouped);
}

#[test]
fn transport_uses_base_and_one_effort_for_grouped_and_legacy_selections() {
    let root = tempfile::tempdir().unwrap();
    let mut options = options(root.path());
    for (model, effort, expected) in [
        ("gemini-3.8-flash-high", Some("medium"), Some("medium")),
        ("gemini-3.8-flash-high", None, Some("high")),
        ("gemini-3.8-flash", Some("low"), Some("low")),
        ("claude-opus-4-6-thinking", None, None),
    ] {
        options.model = model.into();
        options.effort = effort.map(str::to_owned);
        let (command, _) = transport::command_for(std::path::Path::new("agy"), &options).unwrap();
        let args: Vec<_> = command
            .as_std()
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect();
        let base = if model.starts_with("gemini-") {
            "gemini-3.8-flash"
        } else {
            model
        };
        assert!(args.contains(&format!("--model={base}")));
        let efforts: Vec<_> = args
            .iter()
            .filter(|arg| arg.starts_with("--effort="))
            .collect();
        assert_eq!(
            efforts,
            expected
                .map(|level| format!("--effort={level}"))
                .iter()
                .collect::<Vec<_>>()
        );
    }
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
        mcp_aliases: vec![],
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
fn mcp_aliases_preserve_native_names_and_only_use_the_authenticated_loopback_bridge() {
    let root = tempfile::tempdir().unwrap();
    let mut options = options(root.path());
    options.mcp_aliases = vec!["gemini-notebook-mcp".into(), "project_docs".into()];
    let (_, path) = transport::command_for(std::path::Path::new("agy"), &options).unwrap();
    let document = std::fs::read_to_string(path).unwrap();
    let header: Value = serde_yaml_ng::from_str(document.split("---\n").nth(1).unwrap()).unwrap();
    let servers = header["mcpServers"].as_array().unwrap();
    assert_eq!(servers.len(), 3);
    for (server, alias) in servers[1..].iter().zip(&options.mcp_aliases) {
        assert_eq!(server["name"], *alias);
        let url = url::Url::parse(server["serverUrl"].as_str().unwrap()).unwrap();
        assert_eq!(url.host_str(), Some("127.0.0.1"));
        assert_eq!(url.port(), Some(43210));
        assert_eq!(url.path(), format!("/mcp/{alias}"));
        assert!(url.query().is_none());
        assert!(url.fragment().is_none());
        assert_eq!(
            server["headers"]["Authorization"],
            "Bearer private-run-token"
        );
    }
    for aliases in [
        vec!["jarvis".into()],
        vec!["../outside".into()],
        vec!["https://example.com".into()],
        vec!["alias?server=other".into()],
        vec!["alias\nheader".into()],
        vec!["same".into(), "same".into()],
    ] {
        options.mcp_aliases = aliases;
        assert!(transport::command_for(std::path::Path::new("agy"), &options).is_err());
    }
    options.mcp_aliases = vec!["gemini-notebook-mcp".into()];
    options.mcp_url.clear();
    options.mcp_token.clear();
    assert!(transport::command_for(std::path::Path::new("agy"), &options).is_err());
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
