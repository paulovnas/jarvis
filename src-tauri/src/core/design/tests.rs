use super::*;

const ENGINE: &str = "#!/bin/sh\nif [ \"$1\" = engine-probe ]; then printf 'impeccable-engine 0.1.14\\n'; elif [ \"$1\" = context ]; then printf 'RESOLVED_CONTEXT:\\n{\"projectRoot\":\"fixture\"}\\n'; else printf '4.0.0\\n'; fi\n";

pub(crate) fn pack_fixture(directory: &Path) -> Pack {
    prepare_fixture(directory, &[]).unwrap();
    Pack::at(directory, "1.2.3").unwrap()
}

pub(in crate::core) fn prepare_fixture(
    directory: &Path,
    extra: &[(&str, &str)],
) -> Result<(), CoreError> {
    for (relative, content) in [
        (".agents/skills/impeccable/SKILL.md", "---\nname: impeccable\ndescription: Design direction, UX and accessible interfaces\nmetadata:\n  version: 1.2.3\n---\n# Impeccable\nUse the actual project identity."),
        (".agents/skills/impeccable/scripts/VERSION", "0.1.12\n"),
        (".agents/skills/impeccable/scripts/impeccable", "#!/bin/sh\nexec \"$IMPECCABLE_BIN\" \"$@\"\n"),
        (".agents/skills/impeccable/scripts/impeccable.cmd", "@echo off\r\n\"%IMPECCABLE_BIN%\" %*\r\n"),
        (".agents/skills/impeccable/scripts/live-browser.js", "export const live = true;"),
        (".agents/skills/impeccable/reference/craft-floor.md", "# Craft floor\nPreserve user intent and verify interaction."),
        (".agents/skills/impeccable/reference/new-work.md", "# New work\nExplore the actual product and choose a coherent visual direction."),
        (".agents/skills/impeccable/reference/forms.md", "# Accessible forms\nAccessible form controls and buttons."),
        (".agents/skills/impeccable/reference/color.md", "# Color contrast\nReadable foreground and background color contrast."),
        (".agents/skills/impeccable/agents/reviewer.toml", "description = \"Design reviewer\"\n"),
    ].into_iter().chain(extra.iter().copied()) {
        let path = directory.join(relative);
        fs::create_dir_all(path.parent().ok_or_else(invalid)?)?;
        let mut file = fs::OpenOptions::new().write(true).create_new(true).open(path)?;
        use std::io::Write;
        file.write_all(content.as_bytes())?;
    }
    let binary = directory.join(executable_relative());
    fs::create_dir_all(binary.parent().ok_or_else(invalid)?)?;
    fs::write(&binary, ENGINE)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(binary, fs::Permissions::from_mode(0o755))?;
    }
    prepare(
        directory,
        "1.2.3",
        &format!("{:x}", Sha256::digest(ENGINE.as_bytes())),
    )?;
    Ok(())
}
#[test]
fn keeps_complete_skill_runtime_assets_and_separate_version_receipts() {
    let directory = tempfile::tempdir().unwrap();
    prepare_fixture(directory.path(), &[]).unwrap();
    let pack = Pack::at(directory.path(), "1.2.3").unwrap();
    assert_eq!(pack.index.version, "1.2.3");
    assert_eq!(pack.index.engine_version, ENGINE_VERSION);
    assert_eq!(pack.index.cli_version, CLI_VERSION);
    assert_eq!(pack.index.bundle_engine_version, "0.1.12");
    assert!(directory
        .path()
        .join(".agents/skills/impeccable/scripts/live-browser.js")
        .is_file());
    assert!(directory
        .path()
        .join(".agents/skills/impeccable/agents/reviewer.toml")
        .is_file());
    assert!(fs::read_to_string(directory.path().join("LICENSE"))
        .unwrap()
        .contains("Apache License"));
    assert!(!directory.path().join(".codex/hooks.json").exists());
    assert!(Pack::at(directory.path(), "9.0.0").is_err());
}
#[test]
fn reads_full_references_in_bounded_unicode_pages_and_rejects_path_escape() {
    let directory = tempfile::tempdir().unwrap();
    let long = format!("# Long reference\n{}", "á".repeat(13000));
    prepare_fixture(
        directory.path(),
        &[(".agents/skills/impeccable/reference/long.md", &long)],
    )
    .unwrap();
    let pack = Pack::at(directory.path(), "1.2.3").unwrap();
    let listing: Value = serde_json::from_str(
        &pack
            .execute("design_read", &json!({"id":"impeccable/long","file":null}))
            .unwrap(),
    )
    .unwrap();
    let file = listing["files"][0].as_str().unwrap();
    let first: Value = serde_json::from_str(
        &pack
            .execute("design_read", &json!({"id":"impeccable/long","file":file}))
            .unwrap(),
    )
    .unwrap();
    assert_eq!(first["content"].as_str().unwrap().chars().count(), 12000);
    assert_eq!(first["hostInstructions"], HOST_ADAPTATION);
    let second: Value = serde_json::from_str(
        &pack
            .execute(
                "design_read",
                &json!({"id":"impeccable/long","file":file,"offset":12000}),
            )
            .unwrap(),
    )
    .unwrap();
    assert!(second["nextOffset"].is_null());
    assert_eq!(
        first["content"].as_str().unwrap().chars().count()
            + second["content"].as_str().unwrap().chars().count(),
        long.chars().count()
    );
    assert!(pack
        .execute(
            "design_read",
            &json!({"id":"impeccable/long","file":"../../secret"})
        )
        .is_err());
    assert!(pack
        .execute("design_search", &json!({"query":"","offset":-1}))
        .is_err());
}
#[test]
fn reads_new_work_and_init_by_resource_id_without_project_paths() {
    let directory = tempfile::tempdir().unwrap();
    let project = tempfile::tempdir().unwrap();
    prepare_fixture(
        directory.path(),
        &[(
            ".agents/skills/impeccable/reference/init.md",
            "# Init flow\nCapture confirmed product truth before new visual work.",
        )],
    )
    .unwrap();
    let pack = Pack::at(directory.path(), "1.2.3").unwrap();
    for id in ["impeccable/new-work", "impeccable/init"] {
        let page: Value =
            serde_json::from_str(&pack.execute("design_read", &json!({"id": id})).unwrap())
                .unwrap();
        assert!(page["content"].as_str().unwrap().starts_with('#'));
        assert_eq!(page["offset"], 0);
        assert!(page["nextOffset"].is_null());
        assert_eq!(page["hostInstructions"], HOST_ADAPTATION);
        assert!(!project.path().join(page["file"].as_str().unwrap()).exists());
        let listing: Value = serde_json::from_str(
            &pack
                .execute("design_read", &json!({"id": id, "file": null}))
                .unwrap(),
        )
        .unwrap();
        assert_eq!(listing["files"], json!([page["file"]]));
        assert_eq!(listing["hostInstructions"], HOST_ADAPTATION);
        assert!(listing.get("content").is_none());
    }
    let skill: Value = serde_json::from_str(
        &pack
            .execute("design_read", &json!({"id": "impeccable/skill"}))
            .unwrap(),
    )
    .unwrap();
    assert!(skill["files"].as_array().unwrap().len() > 1);
    assert!(skill.get("content").is_none());
}
#[test]
fn searches_portuguese_intents_and_pages_the_impeccable_playbooks() {
    let directory = tempfile::tempdir().unwrap();
    let paths: Vec<_> = (0..14)
        .map(|index| format!(".agents/skills/impeccable/reference/form-{index:02}.md"))
        .collect();
    let extras: Vec<_> = paths
        .iter()
        .map(|path| {
            (
                path.as_str(),
                "# Form\nAccessible form controls and buttons.",
            )
        })
        .collect();
    prepare_fixture(directory.path(), &extras).unwrap();
    let pack = Pack::at(directory.path(), "1.2.3").unwrap();
    let search = |query: &str, kind: &str, offset: usize| -> Value {
        serde_json::from_str(
            &pack
                .execute(
                    "design_search",
                    &json!({"query":query,"kind":kind,"offset":offset}),
                )
                .unwrap(),
        )
        .unwrap()
    };
    let first = search("acessibilidade formulários botões", "craft", 0);
    assert_eq!(first["resources"].as_array().unwrap().len(), 12);
    assert_eq!(first["nextOffset"], 12);
    assert_eq!(
        search("acessibilidade formulários botões", "craft", 12)["resources"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
    assert_eq!(search("", "system", 0)["total"], 0);
    assert_eq!(search("nadaequivalente", "all", 0)["total"], 0);
    assert_eq!(
        search("contraste de cores", "craft", 0)["resources"][0]["id"],
        "impeccable/color"
    );
}
#[test]
fn refuses_racing_skill_versions_new_engine_requirements_and_modified_executables() {
    let directory = tempfile::tempdir().unwrap();
    prepare_fixture(directory.path(), &[]).unwrap();
    let digest = format!("{:x}", Sha256::digest(ENGINE.as_bytes()));
    assert!(prepare(directory.path(), "1.2.4", &digest).is_err());
    fs::write(
        directory
            .path()
            .join(".agents/skills/impeccable/scripts/VERSION"),
        "0.2.0",
    )
    .unwrap();
    assert!(prepare(directory.path(), "1.2.3", &digest).is_err());
    fs::write(
        directory
            .path()
            .join(".agents/skills/impeccable/scripts/VERSION"),
        "0.1.12",
    )
    .unwrap();
    fs::write(
        directory.path().join(executable_relative()),
        "changed binary",
    )
    .unwrap();
    assert!(Pack::at(directory.path(), "1.2.3").is_err());
}
#[test]
fn private_preparation_keeps_signed_assets_without_redundant_launcher_engine() {
    let directory = tempfile::tempdir().unwrap();
    prepare_fixture(directory.path(), &[]).unwrap();
    let fallback = directory
        .path()
        .join(".agents/skills/impeccable/scripts/bin/platform/impeccable");
    fs::create_dir_all(fallback.parent().unwrap()).unwrap();
    fs::write(&fallback, [0u8; 16]).unwrap();
    let files = prepare(
        directory.path(),
        "1.2.3",
        &format!("{:x}", Sha256::digest(ENGINE.as_bytes())),
    )
    .unwrap();
    assert!(!fallback.exists());
    assert!(!files.iter().any(|file| file.contains("scripts/bin/")));
    assert!(files
        .iter()
        .any(|file| file.ends_with("scripts/live-browser.js")));
    assert!(files.contains(&executable_relative()));
    assert!(Pack::at(directory.path(), "1.2.3").is_ok());
}
#[test]
fn managed_commands_use_project_cwd_and_private_upstream_paths() {
    let directory = tempfile::tempdir().unwrap();
    let project = tempfile::tempdir().unwrap();
    prepare_fixture(directory.path(), &[]).unwrap();
    let command = command_at(directory.path(), project.path()).unwrap();
    let command = command.as_std();
    let project_path = fs::canonicalize(project.path()).unwrap();
    let expected =
        PathBuf::from(crate::library::strip_verbatim(&project_path.to_string_lossy()).as_ref());
    assert_eq!(command.get_current_dir(), Some(expected.as_path()));
    let env = |key: &str| {
        command
            .get_envs()
            .find(|(name, _)| *name == key)
            .and_then(|(_, value)| value)
            .map(|value| value.to_string_lossy().into_owned())
    };
    assert_eq!(env("IMPECCABLE_PROVIDER_ID").as_deref(), Some("codex"));
    assert_eq!(env("IMPECCABLE_LIVE_COPY_AGENT").as_deref(), Some("chat"));
    assert_eq!(
        env("IMPECCABLE_BIN").as_deref(),
        command.get_program().to_str()
    );
    assert!(Path::new(&env("IMPECCABLE_SKILL_DIR").unwrap()).ends_with(SKILL_DIRECTORY));
    assert!(Path::new(&env("IMPECCABLE_HOME").unwrap()).ends_with("cache"));
    assert!(command
        .get_envs()
        .any(|(name, value)| name == "IMPECCABLE_CONTEXT_DIR" && value.is_none()));
    #[cfg(windows)]
    {
        assert!(!command.get_program().to_string_lossy().starts_with(r"\\?\"));
        assert!(!env("IMPECCABLE_SKILL_DIR").unwrap().starts_with(r"\\?\"));
    }
    assert!(!project.path().join(".impeccable").exists());
}

#[test]
fn managed_context_detect_and_live_receive_the_canonical_knowledge_design() {
    let directory = tempfile::tempdir().unwrap();
    let project = tempfile::tempdir().unwrap();
    prepare_fixture(directory.path(), &[]).unwrap();
    fs::create_dir_all(project.path().join(".jarvis/knowledge/shared")).unwrap();
    let design = ".jarvis/knowledge/shared/design.md";
    fs::write(
        project.path().join(design),
        "---\nname: Identity\n---\nCanonical",
    )
    .unwrap();
    fs::write(project.path().join("DESIGN.md"), "Competing root").unwrap();
    fs::write(
        project.path().join(".jarvis/knowledge/index.json"),
        json!({"entries":[{"kind":"design","scope":".","path":design}]}).to_string(),
    )
    .unwrap();
    let command = command_at(directory.path(), project.path()).unwrap();
    let env = |key: &str| {
        command
            .as_std()
            .get_envs()
            .find(|(name, _)| *name == key)
            .and_then(|(_, value)| value)
            .map(PathBuf::from)
    };
    let root = fs::canonicalize(project.path()).unwrap();
    let root = PathBuf::from(crate::library::strip_verbatim(&root.to_string_lossy()).as_ref());
    assert_eq!(env("IMPECCABLE_DESIGN_ROOT"), Some(root.clone()));
    assert_eq!(env("IMPECCABLE_DESIGN_PATH"), Some(root.join(design)));
    assert_eq!(
        fs::read_to_string(project.path().join("DESIGN.md")).unwrap(),
        "Competing root"
    );
}
#[cfg(unix)]
#[test]
fn managed_product_binding_is_scoped_and_cannot_inherit_another_project() {
    let directory = tempfile::tempdir().unwrap();
    let project = tempfile::tempdir().unwrap();
    prepare_fixture(directory.path(), &[]).unwrap();
    fs::create_dir_all(project.path().join("app")).unwrap();
    fs::create_dir_all(project.path().join(".jarvis/knowledge")).unwrap();
    fs::write(project.path().join(".jarvis/knowledge/prd.md"), "# Shared").unwrap();
    fs::write(project.path().join(".jarvis/knowledge/app-prd.md"), "# App").unwrap();
    fs::write(project.path().join("PRODUCT.md"), "# Competing").unwrap();
    fs::write(
        project.path().join(".jarvis/knowledge/index.json"),
        json!({"entries":[
            {"kind":"product","scope":".","path":".jarvis/knowledge/prd.md"},
            {"kind":"product","scope":"app","path":".jarvis/knowledge/app-prd.md"}
        ]})
        .to_string(),
    )
    .unwrap();
    let mut command = command_at_for(directory.path(), project.path(), Some("app")).unwrap();
    let root = fs::canonicalize(project.path()).unwrap();
    let env = |command: &tokio::process::Command, key: &str| {
        command
            .as_std()
            .get_envs()
            .find(|(name, _)| *name == key)
            .and_then(|(_, value)| value)
            .map(PathBuf::from)
    };
    assert_eq!(env(&command, "IMPECCABLE_PRODUCT_ROOT"), Some(root.clone()));
    assert_eq!(
        env(&command, "IMPECCABLE_PRODUCT_PATH"),
        Some(root.join(".jarvis/knowledge/app-prd.md"))
    );
    let other = tempfile::tempdir().unwrap();
    set_product_path(&mut command, &fs::canonicalize(other.path()).unwrap(), None).unwrap();
    assert!(env(&command, "IMPECCABLE_PRODUCT_PATH").is_none());
    assert!(env(&command, "IMPECCABLE_PRODUCT_ROOT").is_none());
    fs::remove_file(root.join(".jarvis/knowledge/prd.md")).unwrap();
    assert!(command_at(directory.path(), project.path()).is_err());
    assert_eq!(
        fs::read_to_string(root.join("PRODUCT.md")).unwrap(),
        "# Competing"
    );
}

#[cfg(unix)]
#[test]
fn rejects_pack_and_executable_symlinks_outside_managed_generation() {
    let directory = tempfile::tempdir().unwrap();
    let external = tempfile::NamedTempFile::new().unwrap();
    prepare_fixture(directory.path(), &[]).unwrap();
    let pack = Pack::at(directory.path(), "1.2.3").unwrap();
    let reference = ".agents/skills/impeccable/reference/color.md";
    fs::remove_file(directory.path().join(reference)).unwrap();
    std::os::unix::fs::symlink(external.path(), directory.path().join(reference)).unwrap();
    assert!(pack
        .execute(
            "design_read",
            &json!({"id":"impeccable/color","file":reference})
        )
        .is_err());
    assert!(prepare(
        directory.path(),
        "1.2.3",
        &format!("{:x}", Sha256::digest(ENGINE.as_bytes()))
    )
    .is_err());
    fs::remove_file(directory.path().join(executable_relative())).unwrap();
    std::os::unix::fs::symlink(
        external.path(),
        directory.path().join(executable_relative()),
    )
    .unwrap();
    assert!(command_at(directory.path(), directory.path()).is_err());
}
#[cfg(unix)]
#[tokio::test]
async fn verifies_both_managed_engine_identity_and_cli_version() {
    let directory = tempfile::tempdir().unwrap();
    prepare_fixture(directory.path(), &[]).unwrap();
    verify(directory.path(), "1.2.3").await.unwrap();
}
