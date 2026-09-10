use super::*;

fn custom_agent(
    model: Option<workflow::settings::ModelChoice>,
) -> workflow::catalog::AgentDefinition {
    workflow::catalog::AgentDefinition {
        id: "0123456789abcdef0123456789abcdef".into(),
        name: "Especialista".into(),
        description: "Analisa o projeto".into(),
        instructions: "Leia o contexto antes de agir.".into(),
        native_role: None,
        usage: workflow::catalog::AgentUsage::Mixed,
        capability: workflow::catalog::Capability::ReadOnly,
        denied_tools: vec!["write_file".into()],
        model,
        appearance: None,
    }
}

fn payload() -> SettingsPayload {
    let catalog = workflow::catalog::Catalog {
        revision: 0,
        agents: vec![custom_agent(None)],
        flows: vec![],
    };
    SettingsPayload {
        system: system::Preferences::default(),
        model_targets: model_targets(&BTreeMap::new(), &catalog).unwrap(),
        catalog,
        skills: skills::PortableConfig::default(),
        mcps: vec![
            r#"{"docs":{"type":"remote","url":"https://example.test/mcp","headers":{"Authorization":"Bearer mcp-secret"},"enabled":true,"timeout":5000}}"#.into(),
        ],
    }
}

#[test]
fn round_trip_preserves_portable_settings_and_skill_payload() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("backup.zip");
    let settings = payload();
    let files = vec![SkillFile {
        path: PathBuf::from("review/SKILL.md"),
        bytes: b"# Review\nCheck the diff.".to_vec(),
        mode: 0o644,
    }];

    fs::write(&path, b"previous backup").unwrap();
    let written = write_archive(&path, &settings, &files).unwrap();
    let loaded = read_archive(&path).unwrap();

    assert_eq!(written, fs::metadata(&path).unwrap().len());
    assert_eq!(loaded.manifest.format, FORMAT);
    assert_eq!(loaded.manifest.version, FORMAT_VERSION);
    assert_eq!(loaded.manifest.summary.skills, 1);
    assert_eq!(loaded.manifest.summary.mcps, 1);
    assert_eq!(loaded.payload.catalog.agents[0].name, "Especialista");
    assert_eq!(loaded.payload.catalog.agents[0].model, None);
    assert_eq!(loaded.skill_files[0].path, Path::new("review/SKILL.md"));
    assert_eq!(loaded.skill_files[0].bytes, files[0].bytes);
    assert!(loaded.payload.mcps[0].contains("mcp-secret"));
    assert_eq!(preview(&loaded).model_targets.len(), 1);
}

#[test]
fn sanitized_catalog_and_targets_never_serialize_provider_assignments() {
    let choice = workflow::settings::ModelChoice {
        account: "openai-codex-private".into(),
        model: "private-model".into(),
        reasoning: Some("high".into()),
    };
    let catalog = clean_catalog(workflow::catalog::Catalog {
        revision: 42,
        agents: vec![custom_agent(Some(choice.clone()))],
        flows: vec![],
    });
    let native = BTreeMap::from([("planned/planner".into(), choice)]);
    let targets = model_targets(&native, &catalog).unwrap();
    let serialized = serde_json::to_string(&(catalog, targets)).unwrap();

    assert!(!serialized.contains("openai-codex-private"));
    assert!(!serialized.contains("private-model"));
    assert!(serialized.contains("builtin:planned/planner"));
    assert!(serialized.contains("custom:0123456789abcdef0123456789abcdef"));
}

#[test]
fn validation_rejects_embedded_agent_models_and_invalid_paths() {
    let mut invalid = payload();
    invalid.catalog.agents[0].model = Some(workflow::settings::ModelChoice {
        account: "provider".into(),
        model: "model".into(),
        reasoning: None,
    });

    assert!(validate_payload(&invalid).is_err());
    for path in ["../outside", "/absolute", "skills\\escape", ""] {
        assert!(safe_archive_path(path).is_err(), "accepted {path}");
    }
    for path in ["con/SKILL.md", "demo/file?.md", "demo/trailing. "] {
        assert!(
            validate_skill_path(Path::new(path)).is_err(),
            "accepted {path}"
        );
    }
}

#[test]
fn inspection_rejects_unknown_entries_and_future_versions() {
    fn archive(path: &Path, version: u16, extra: Option<&str>) {
        let settings = serde_json::to_vec_pretty(&payload()).unwrap();
        let manifest = Manifest {
            format: FORMAT.into(),
            version,
            created_at: 1,
            app_version: "test".into(),
            settings_sha256: digest(&settings),
            summary: summary(&payload(), &[]),
        };
        let file = fs::File::create(path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        zip.start_file(MANIFEST_NAME, zip_options(0o600)).unwrap();
        zip.write_all(&serde_json::to_vec(&manifest).unwrap())
            .unwrap();
        zip.start_file(SETTINGS_NAME, zip_options(0o600)).unwrap();
        zip.write_all(&settings).unwrap();
        if let Some(name) = extra {
            zip.start_file(name, zip_options(0o600)).unwrap();
            zip.write_all(b"unexpected").unwrap();
        }
        zip.finish().unwrap();
    }

    let directory = tempfile::tempdir().unwrap();
    let future = directory.path().join("future.zip");
    archive(&future, FORMAT_VERSION + 1, None);
    assert!(read_archive(&future)
        .unwrap_err()
        .message
        .contains("versão mais nova"));

    let unknown = directory.path().join("unknown.zip");
    archive(&unknown, FORMAT_VERSION, Some("providers.json"));
    assert!(read_archive(&unknown).is_err());
}

#[test]
fn file_swap_can_be_rolled_back_without_losing_previous_settings() {
    let directory = tempfile::tempdir().unwrap();
    let live = directory.path().join("live");
    let staged = directory.path().join("staged");
    let saved = directory.path().join("saved");
    for root in [&live, &staged] {
        fs::create_dir_all(root.join("skills")).unwrap();
        for name in [
            "system.json",
            "agents.json",
            "workflow-catalog.json",
            "skills.json",
        ] {
            fs::write(root.join(name), root.to_string_lossy().as_bytes()).unwrap();
        }
        fs::write(
            root.join("skills/SKILL.md"),
            root.to_string_lossy().as_bytes(),
        )
        .unwrap();
    }

    let swaps = swap_staged(&live, &staged, &saved).unwrap();
    assert_eq!(
        fs::read_to_string(live.join("system.json")).unwrap(),
        staged.to_string_lossy()
    );
    assert!(rollback_swaps(&live, &saved, &swaps));
    assert_eq!(
        fs::read_to_string(live.join("system.json")).unwrap(),
        live.to_string_lossy()
    );
    assert_eq!(
        fs::read_to_string(live.join("skills/SKILL.md")).unwrap(),
        live.to_string_lossy()
    );
}
