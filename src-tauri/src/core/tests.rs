use super::*;

#[test]
fn requires_every_component_and_never_treats_newer_release_as_missing() {
    let home = tempfile::tempdir().unwrap();
    let mut manifest = Manifest::default();
    assert!(require_ready(home.path()).is_err());
    for id in ComponentId::ALL {
        let directory = format!("{}/test", id.key());
        let path = root(home.path()).join(&directory);
        fs::create_dir_all(&path).unwrap();
        fs::write(path.join("verified"), "ok").unwrap();
        if id == ComponentId::Ponytail {
            ponytail::tests::fixture_package(&path, "1.0.0");
        }
        if id == ComponentId::OpenDesign { design::tests::prepare_fixture(&path, &[]).unwrap(); }
        manifest.installations.insert(
            id,
            Installation {
                version: if id == ComponentId::OpenDesign { "1.2.3" } else { "1.0.0" }.into(),
                directory,
                files: vec!["verified".into()],
            },
        );
        save_manifest(home.path(), &manifest).unwrap();
        assert!(require_ready(home.path()).is_err());
    }
    assert!(!CoreState::default().snapshot(home.path()).unwrap().ready);
    fs::write(root(home.path()).join("context7.json"), r#"{"credential_ref":"jarvis-core-context7-test"}"#).unwrap();
    assert!(require_ready(home.path()).is_ok());
    let state = CoreState::default();
    state
        .data
        .lock()
        .unwrap()
        .latest
        .insert(ComponentId::ContextMode, "1.0.1".into());
    let snapshot = state.snapshot(home.path()).unwrap();
    assert!(snapshot.ready);
    assert!(snapshot.items[0].update_available);
    fs::remove_file(root(home.path()).join("ponytail/test/verified")).unwrap();
    assert!(!state.snapshot(home.path()).unwrap().ready);
    assert!(require_ready(home.path()).is_err());
}
#[test]
fn version_comparison_is_semantic_and_invalid_manifests_fail_closed() {
    assert!(newer("1.0.10", "1.0.9"));
    assert!(!newer("1.0.2", "1.0.10"));
    assert!(!newer("bad", "1.0.0"));
    let home = tempfile::tempdir().unwrap();
    fs::create_dir_all(root(home.path())).unwrap();
    fs::write(root(home.path()).join("manifest.json"), "{").unwrap();
    let snapshot = CoreState::default().snapshot(home.path()).unwrap();
    assert!(!snapshot.ready);
    assert!(snapshot.items.iter().all(|item| item.health_error.is_some()));
    let install = Installation {
        version: "1.0.0".into(),
        directory: "../escape".into(),
        files: vec!["file".into()],
    };
    assert!(install.path(home.path()).is_err());
}
#[test]
fn preflight_routes_http_without_blocking_regular_mutations() {
    assert!(hooks::pre_tool(
        "bash",
        &serde_json::json!({"command":"curl https://example.com"})
    )
    .is_some());
    assert!(hooks::pre_tool("bash", &serde_json::json!({"command":"git status"})).is_none());
    assert!(hooks::pre_tool("edit", &serde_json::json!({"newText":"curl url"})).is_none());
}

#[tokio::test]
#[ignore = "Downloads official Core releases into an isolated temporary home"]
async fn official_installation_smoke() {
    let home = tempfile::tempdir().unwrap();
    for id in ComponentId::ALL {
        let version = install::install(home.path(), id, |stage| eprintln!("{}: {stage}", id.key()), |_| {})
            .await
            .unwrap();
        assert!(!version.is_empty());
    }
    assert!(!CoreState::default().snapshot(home.path()).unwrap().ready);
    assert!(CoreState::default().snapshot(home.path()).unwrap().items.iter().all(|item| item.installed));
}
