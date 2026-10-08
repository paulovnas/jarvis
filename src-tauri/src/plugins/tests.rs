use super::*;
use serde_json::json;
use std::fs;

fn draft(name: &str) -> Draft {
    Draft {
        name: name.into(),
        description: "Fixture de plugin".into(),
        skills: vec![DraftSkill {
            name: "review".into(),
            content: "---\nname: review\ndescription: Revise o trabalho\n---\nLeia o pedido."
                .into(),
        }],
        mcp_servers: BTreeMap::new(),
        hooks: None,
        apps: BTreeMap::new(),
        files: Vec::new(),
    }
}

async fn create(home: &Path, draft: Draft) -> Catalog {
    let prepared = preview(
        home,
        catalog(home).unwrap().revision,
        Operation::Create { draft },
    )
    .await
    .unwrap();
    apply(home, &prepared).unwrap()
}

#[test]
fn default_marketplace_is_lazy_and_does_not_create_storage() {
    let home = tempfile::tempdir().unwrap();
    let catalog = catalog(home.path()).unwrap();
    assert!(
        catalog.marketplaces[0].built_in
            && !catalog.marketplaces[0].refreshed
            && !plugin_home(home.path()).exists()
    );
}

#[tokio::test]
async fn approval_installs_owned_snapshot_without_running_commands() {
    let home = tempfile::tempdir().unwrap();
    let marker = home.path().join("should-not-exist");
    let mut draft = draft("example");
    draft.hooks = Some(
        json!({"hooks":{"PreToolUse":[{"hooks":[{"type":"command","command":format!("touch {}",marker.display())}]}]}}),
    );
    let installed = create(home.path(), draft).await;
    let overlay = load_active(home.path()).unwrap();
    assert!(
        installed.installed[0].integrity_valid
            && !overlay.hook_sources[0].trusted
            && !marker.exists()
    );
}

#[tokio::test]
async fn repeated_approval_receipt_cannot_replay_mutation() {
    let home = tempfile::tempdir().unwrap();
    let prepared = preview(
        home.path(),
        0,
        Operation::Create {
            draft: draft("once"),
        },
    )
    .await
    .unwrap();
    apply(home.path(), &prepared).unwrap();
    assert_eq!(
        apply(home.path(), &prepared).unwrap_err().code,
        "plugin_revision_conflict"
    );
}

#[tokio::test]
async fn external_catalog_edit_invalidates_pending_approval_even_without_revision_change() {
    let home = tempfile::tempdir().unwrap();
    create(home.path(), draft("first")).await;
    let prepared = preview(
        home.path(),
        1,
        Operation::SetEnabled {
            plugin_id: "first@local".into(),
            enabled: false,
            project_path: None,
        },
    )
    .await
    .unwrap();
    let path = catalog_file(home.path());
    let mut stored: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    stored["apps_account_id"] = json!("changed");
    fs::write(path, serde_json::to_vec(&stored).unwrap()).unwrap();
    assert_eq!(
        apply(home.path(), &prepared).unwrap_err().code,
        "plugin_revision_conflict"
    );
}

#[tokio::test]
async fn package_update_revokes_hook_trust_and_retains_previous_frozen_root() {
    let home = tempfile::tempdir().unwrap();
    let mut original = draft("versioned");
    original.hooks =
        Some(json!({"hooks":{"Stop":[{"hooks":[{"type":"command","command":"echo original"}]}]}}));
    let installed = create(home.path(), original.clone()).await;
    let old_root = installed.installed[0].root_path.clone();
    let approval = preview(
        home.path(),
        1,
        Operation::TrustHooks {
            plugin_id: "versioned@local".into(),
            trusted: true,
        },
    )
    .await
    .unwrap();
    apply(home.path(), &approval).unwrap();
    assert!(load_active(home.path()).unwrap().hook_sources[0].trusted);
    original.hooks =
        Some(json!({"hooks":{"Stop":[{"hooks":[{"type":"command","command":"echo changed"}]}]}}));
    create(home.path(), original).await;
    assert!(
        !load_active(home.path()).unwrap().hook_sources[0].trusted && Path::new(&old_root).exists()
    );
}

#[tokio::test]
async fn frozen_approved_hook_survives_update_but_explicit_revocation_stops_it() {
    let home = tempfile::tempdir().unwrap();
    let mut original = draft("frozen");
    original.hooks =
        Some(json!({"hooks":{"Stop":[{"hooks":[{"type":"command","command":"echo approved"}]}]}}));
    create(home.path(), original.clone()).await;
    let approved = preview(
        home.path(),
        1,
        Operation::TrustHooks {
            plugin_id: "frozen@local".into(),
            trusted: true,
        },
    )
    .await
    .unwrap();
    apply(home.path(), &approved).unwrap();
    let frozen = load_active(home.path()).unwrap().hook_sources.remove(0);
    original.hooks =
        Some(json!({"hooks":{"Stop":[{"hooks":[{"type":"command","command":"echo changed"}]}]}}));
    create(home.path(), original).await;
    assert!(
        hook_source_authorized(home.path(), None, &frozen)
            && !load_active(home.path()).unwrap().hook_sources[0].trusted
    );
    let revoke = preview(
        home.path(),
        3,
        Operation::TrustHooks {
            plugin_id: "frozen@local".into(),
            trusted: false,
        },
    )
    .await
    .unwrap();
    apply(home.path(), &revoke).unwrap();
    assert!(!hook_source_authorized(home.path(), None, &frozen));
}

#[tokio::test]
async fn explicit_component_disable_revokes_frozen_hook() {
    let home = tempfile::tempdir().unwrap();
    let mut draft = draft("disabled");
    draft.hooks =
        Some(json!({"hooks":{"Stop":[{"hooks":[{"type":"command","command":"echo approved"}]}]}}));
    create(home.path(), draft).await;
    let trust = preview(
        home.path(),
        1,
        Operation::TrustHooks {
            plugin_id: "disabled@local".into(),
            trusted: true,
        },
    )
    .await
    .unwrap();
    apply(home.path(), &trust).unwrap();
    let frozen = load_active(home.path()).unwrap().hook_sources.remove(0);
    let disable = preview(
        home.path(),
        2,
        Operation::ConfigureComponent {
            plugin_id: "disabled@local".into(),
            component_id: frozen.component_id.clone(),
            enabled: false,
        },
    )
    .await
    .unwrap();
    apply(home.path(), &disable).unwrap();
    assert!(!hook_source_authorized(home.path(), None, &frozen));
}

#[tokio::test]
async fn frozen_skill_and_mcp_integrity_survives_update_but_rejects_retired_version_changes() {
    let home = tempfile::tempdir().unwrap();
    let mut original = draft("frozen-components");
    original.mcp_servers.insert(
        "local".into(),
        json!({"command":"node","args":["server.js"]}),
    );
    original.files.push(DraftFile {
        path: "server.js".into(),
        content: "console.log('original');".into(),
    });
    create(home.path(), original.clone()).await;
    let frozen = load_active(home.path()).unwrap();
    let skill = &frozen.skill_roots[0];
    let skill_path = skill.path.join("review");
    let mcp = &frozen.mcp_servers[0];
    original.description = "Updated package".into();
    create(home.path(), original).await;
    assert!(skill_source_authorized(
        home.path(),
        None,
        &skill.plugin_id,
        &skill_path
    ));
    assert!(frozen_component_authorized(
        home.path(),
        None,
        &mcp.plugin_id,
        &mcp.component_id,
        &mcp.plugin_hash,
        &mcp.root
    ));
    fs::write(mcp.root.join("server.js"), "changed externally").unwrap();
    assert!(!skill_source_authorized(
        home.path(),
        None,
        &skill.plugin_id,
        &skill_path
    ));
    assert!(!frozen_component_authorized(
        home.path(),
        None,
        &mcp.plugin_id,
        &mcp.component_id,
        &mcp.plugin_hash,
        &mcp.root
    ));
    assert!(catalog(home.path()).unwrap().installed[0].integrity_valid);
}

#[tokio::test]
async fn explicit_component_disable_and_uninstall_revoke_frozen_skill_access() {
    let home = tempfile::tempdir().unwrap();
    create(home.path(), draft("frozen-skill")).await;
    let frozen = load_active(home.path()).unwrap().skill_roots.remove(0);
    let skill_path = frozen.path.join("review");
    let disable = preview(
        home.path(),
        1,
        Operation::ConfigureComponent {
            plugin_id: frozen.plugin_id.clone(),
            component_id: frozen.component_id.clone(),
            enabled: false,
        },
    )
    .await
    .unwrap();
    apply(home.path(), &disable).unwrap();
    assert!(!skill_source_authorized(
        home.path(),
        None,
        &frozen.plugin_id,
        &skill_path
    ));
    let enable = preview(
        home.path(),
        2,
        Operation::ConfigureComponent {
            plugin_id: frozen.plugin_id.clone(),
            component_id: frozen.component_id.clone(),
            enabled: true,
        },
    )
    .await
    .unwrap();
    apply(home.path(), &enable).unwrap();
    assert!(skill_source_authorized(
        home.path(),
        None,
        &frozen.plugin_id,
        &skill_path
    ));
    let uninstall = preview(
        home.path(),
        3,
        Operation::Uninstall {
            plugin_id: frozen.plugin_id.clone(),
        },
    )
    .await
    .unwrap();
    apply(home.path(), &uninstall).unwrap();
    assert!(!skill_source_authorized(
        home.path(),
        None,
        &frozen.plugin_id,
        &skill_path
    ));
}

#[tokio::test]
async fn grouped_skill_checks_preserve_order_and_recheck_changes_on_later_calls() {
    let home = tempfile::tempdir().unwrap();
    let mut shared = draft("grouped");
    shared.skills.push(DraftSkill {
        name: "second".into(),
        content: "---\nname: second\ndescription: Another skill\n---\nReview.".into(),
    });
    create(home.path(), shared).await;
    create(home.path(), draft("separate")).await;
    let active = load_active(home.path()).unwrap();
    let grouped = active
        .skill_roots
        .iter()
        .find(|root| root.plugin_id == "grouped@local")
        .unwrap();
    let separate = active
        .skill_roots
        .iter()
        .find(|root| root.plugin_id == "separate@local")
        .unwrap();
    let first = grouped.path.join("review");
    let second = grouped.path.join("second");
    let third = separate.path.join("review");
    let sources = [
        (grouped.plugin_id.as_str(), first.as_path()),
        (separate.plugin_id.as_str(), third.as_path()),
        (grouped.plugin_id.as_str(), second.as_path()),
        ("unknown@local", first.as_path()),
    ];
    assert_eq!(
        skill_sources_authorized(home.path(), None, &sources),
        vec![true, true, true, false]
    );
    fs::write(first.join("SKILL.md"), "changed externally").unwrap();
    assert_eq!(
        skill_sources_authorized(home.path(), None, &sources),
        vec![false, true, false, false]
    );
}

#[cfg(unix)]
#[tokio::test]
async fn canonical_skill_paths_preserve_owned_cache_containment_across_home_aliases() {
    let actual = tempfile::tempdir().unwrap();
    let aliases = tempfile::tempdir().unwrap();
    let alias = aliases.path().join("home-alias");
    std::os::unix::fs::symlink(actual.path(), &alias).unwrap();
    create(&alias, draft("aliased-home")).await;
    let frozen = load_active(&alias).unwrap().skill_roots.remove(0);
    let canonical_skill = fs::canonicalize(frozen.path.join("review")).unwrap();
    assert!(skill_source_authorized(
        &alias,
        None,
        &frozen.plugin_id,
        &canonical_skill
    ));
    assert!(skill_source_authorized(
        actual.path(),
        None,
        &frozen.plugin_id,
        &canonical_skill
    ));
    assert!(catalog(actual.path()).unwrap().installed[0].integrity_valid);
    let outside = tempfile::tempdir().unwrap();
    assert!(!skill_source_authorized(
        &alias,
        None,
        &frozen.plugin_id,
        outside.path()
    ));
}

#[tokio::test]
async fn update_does_not_install_an_uninstalled_marketplace_package() {
    let home = tempfile::tempdir().unwrap();
    let marketplace = tempfile::tempdir().unwrap();
    source::write_json(
        &marketplace.path().join(".agents/plugins/marketplace.json"),
        &json!({"name":"updates","plugins":[{"name":"example","source":"./package"}]}),
    )
    .unwrap();
    source::write_json(
        &marketplace.path().join("package/.codex-plugin/plugin.json"),
        &json!({"name":"example"}),
    )
    .unwrap();
    let register = preview(
        home.path(),
        0,
        Operation::AddMarketplace {
            source: marketplace.path().to_string_lossy().into_owned(),
            ref_name: None,
            sparse_paths: vec![],
        },
    )
    .await
    .unwrap();
    apply(home.path(), &register).unwrap();
    let error = preview(
        home.path(),
        1,
        Operation::Update {
            plugin_id: "example@updates".into(),
        },
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "plugin_not_found");
    let unchanged = catalog(home.path()).unwrap();
    assert_eq!(unchanged.revision, 1);
    assert!(unchanged.installed.is_empty());
}

#[tokio::test]
async fn updates_preserve_component_disabling_when_a_component_disappears_then_returns() {
    let home = tempfile::tempdir().unwrap();
    let marketplace = tempfile::tempdir().unwrap();
    source::write_json(
        &marketplace.path().join(".agents/plugins/marketplace.json"),
        &json!({"name":"component-updates","plugins":[{"name":"example","source":"./package"}]}),
    )
    .unwrap();
    let package = marketplace.path().join("package");
    source::write_json(
        &package.join(".codex-plugin/plugin.json"),
        &json!({"name":"example"}),
    )
    .unwrap();
    let apps = package.join(".app.json");
    source::write_json(&apps, &json!({"apps":{"mail":{"id":"connector-one"}}})).unwrap();
    let register = preview(
        home.path(),
        0,
        Operation::AddMarketplace {
            source: marketplace.path().to_string_lossy().into_owned(),
            ref_name: None,
            sparse_paths: vec![],
        },
    )
    .await
    .unwrap();
    apply(home.path(), &register).unwrap();
    let plugin_id = "example@component-updates";
    let install = preview(
        home.path(),
        1,
        Operation::Install {
            plugin_id: plugin_id.into(),
        },
    )
    .await
    .unwrap();
    apply(home.path(), &install).unwrap();
    let disable = preview(
        home.path(),
        2,
        Operation::ConfigureComponent {
            plugin_id: plugin_id.into(),
            component_id: "apps:mail".into(),
            enabled: false,
        },
    )
    .await
    .unwrap();
    apply(home.path(), &disable).unwrap();
    fs::remove_file(&apps).unwrap();
    let remove_component = preview(
        home.path(),
        3,
        Operation::Update {
            plugin_id: plugin_id.into(),
        },
    )
    .await
    .unwrap();
    apply(home.path(), &remove_component).unwrap();
    assert!(catalog(home.path()).unwrap().installed[0]
        .components
        .is_empty());
    source::write_json(&apps, &json!({"apps":{"mail":{"id":"connector-one"}}})).unwrap();
    let return_component = preview(
        home.path(),
        4,
        Operation::Update {
            plugin_id: plugin_id.into(),
        },
    )
    .await
    .unwrap();
    assert!(!return_component.preview.components[0].enabled);
    let updated = apply(home.path(), &return_component).unwrap();
    assert!(!updated.installed[0].components[0].enabled);
    assert_eq!(
        return_component.preview.components[0].enabled,
        updated.installed[0].components[0].enabled
    );
    assert!(load_active(home.path()).unwrap().apps.is_empty());
}

#[tokio::test]
async fn modifying_installed_script_quarantines_all_contributions() {
    let home = tempfile::tempdir().unwrap();
    let mut draft = draft("integrity");
    draft.files.push(DraftFile {
        path: "scripts/helper.sh".into(),
        content: "echo ok".into(),
    });
    let installed = create(home.path(), draft).await;
    fs::write(
        Path::new(&installed.installed[0].root_path).join("scripts/helper.sh"),
        "echo modified",
    )
    .unwrap();
    let catalog = catalog(home.path()).unwrap();
    assert!(
        !catalog.installed[0].integrity_valid
            && load_active(home.path()).unwrap().skill_roots.is_empty()
    );
}

#[tokio::test]
async fn disable_component_preserves_other_plugin_components() {
    let home = tempfile::tempdir().unwrap();
    let mut draft = draft("components");
    draft.mcp_servers.insert(
        "docs".into(),
        json!({"command":"node","args":["${PLUGIN_ROOT}/scripts/server.js"]}),
    );
    create(home.path(), draft).await;
    let prepared = preview(
        home.path(),
        1,
        Operation::ConfigureComponent {
            plugin_id: "components@local".into(),
            component_id: "mcp:docs".into(),
            enabled: false,
        },
    )
    .await
    .unwrap();
    apply(home.path(), &prepared).unwrap();
    let active = load_active(home.path()).unwrap();
    assert!(active.mcp_servers.is_empty() && active.skill_roots.len() == 1);
}

#[tokio::test]
async fn per_project_override_does_not_leak_to_other_projects() {
    let home = tempfile::tempdir().unwrap();
    let project = tempfile::tempdir().unwrap();
    let other = tempfile::tempdir().unwrap();
    create(home.path(), draft("scope")).await;
    let prepared = preview(
        home.path(),
        1,
        Operation::SetEnabled {
            plugin_id: "scope@local".into(),
            enabled: false,
            project_path: Some(project.path().to_string_lossy().into_owned()),
        },
    )
    .await
    .unwrap();
    apply(home.path(), &prepared).unwrap();
    assert!(
        load_active_for_project(home.path(), Some(project.path()))
            .unwrap()
            .skill_roots
            .is_empty()
            && load_active_for_project(home.path(), Some(other.path()))
                .unwrap()
                .skill_roots
                .len()
                == 1
    );
}

#[tokio::test]
async fn uninstall_preserves_data_and_in_use_version() {
    let home = tempfile::tempdir().unwrap();
    let installed = create(home.path(), draft("retained")).await;
    let entry = &installed.installed[0];
    fs::write(Path::new(&entry.data_path).join("state.txt"), "saved").unwrap();
    let prepared = preview(
        home.path(),
        1,
        Operation::Uninstall {
            plugin_id: entry.id.clone(),
        },
    )
    .await
    .unwrap();
    apply(home.path(), &prepared).unwrap();
    assert!(
        load_active(home.path()).unwrap().skill_roots.is_empty()
            && Path::new(&entry.root_path).exists()
            && Path::new(&entry.data_path).join("state.txt").exists()
    );
}

#[tokio::test]
async fn portable_package_uses_fixed_skills_mcp_and_does_not_activate_legacy_hooks_apps() {
    let home = tempfile::tempdir().unwrap();
    let package = tempfile::tempdir().unwrap();
    source::write_json(&package.path().join("plugin.json"), &json!({"$schema":"https://agent-plugins.org/schemas/1.0.0/plugin.schema.json","name":"portable"})).unwrap();
    source::write_json(&package.path().join("mcp.json"), &json!({"$schema":"https://agent-plugins.org/schemas/1.0.0/mcp.schema.json","mcpServers":{"docs":{"type":"stdio","command":"node","args":[]}}})).unwrap();
    source::write_json(
        &package.path().join("hooks/hooks.json"),
        &json!({"hooks":{"Stop":[]}}),
    )
    .unwrap();
    source::write_json(
        &package.path().join(".app.json"),
        &json!({"apps":{"mail":{"id":"gmail"}}}),
    )
    .unwrap();
    let prepared = preview(
        home.path(),
        0,
        Operation::Import {
            path: package.path().to_string_lossy().into_owned(),
        },
    )
    .await
    .unwrap();
    apply(home.path(), &prepared).unwrap();
    let active = load_active(home.path()).unwrap();
    assert!(
        active.mcp_servers.len() == 1
            && active.hook_sources.is_empty()
            && active.apps.is_empty()
            && !active.warnings.is_empty()
    );
}

#[tokio::test]
async fn apps_metadata_exposes_fixed_authorization_link_and_actual_grouped_gateway() {
    let home = tempfile::tempdir().unwrap();
    let mut draft = draft("app-metadata");
    draft
        .apps
        .insert("Google Drive".into(), json!({"id":"connector-one"}));
    draft
        .apps
        .insert("Unsafe".into(), json!({"id":"../../steal"}));
    let installed = create(home.path(), draft).await;
    let plugin = &installed.installed[0];
    let app = plugin
        .components
        .iter()
        .find(|component| component.kind == ComponentKind::Apps)
        .unwrap();
    assert_eq!(
        app.app_connect_url.as_deref(),
        Some("https://chatgpt.com/apps/google-drive/connector-one")
    );
    assert_eq!(
        app.mcp_server_id.as_deref(),
        Some(super::apps::server_id(&plugin.id).as_str())
    );
    assert!(plugin
        .warnings
        .iter()
        .any(|warning| warning.contains("conector inválido")));
    let active = load_active(home.path()).unwrap();
    assert_eq!(active.apps.len(), 1);
    assert_eq!(active.apps[0].id, "connector-one");
    assert!(installed.apps_account_id.is_none());
}

#[tokio::test]
async fn local_marketplace_sources_resolve_from_repository_root_and_survive_removal() {
    let home = tempfile::tempdir().unwrap();
    let marketplace = tempfile::tempdir().unwrap();
    source::write_json(&marketplace.path().join(".agents/plugins/marketplace.json"), &json!({"name":"fixtures","plugins":[{"name":"local-example","source":"./packages/example"}]})).unwrap();
    source::write_json(
        &marketplace
            .path()
            .join("packages/example/.codex-plugin/plugin.json"),
        &json!({"name":"local-example"}),
    )
    .unwrap();
    let prepared = preview(
        home.path(),
        0,
        Operation::AddMarketplace {
            source: marketplace.path().to_string_lossy().into_owned(),
            ref_name: None,
            sparse_paths: Vec::new(),
        },
    )
    .await
    .unwrap();
    let snapshot = apply(home.path(), &prepared).unwrap();
    assert_eq!(snapshot.available[0].id, "local-example@fixtures");
    let installed = preview(
        home.path(),
        1,
        Operation::Install {
            plugin_id: snapshot.available[0].id.clone(),
        },
    )
    .await
    .unwrap();
    apply(home.path(), &installed).unwrap();
    let removed = preview(
        home.path(),
        2,
        Operation::RemoveMarketplace {
            marketplace_id: "fixtures".into(),
        },
    )
    .await
    .unwrap();
    apply(home.path(), &removed).unwrap();
    assert!(
        marketplace
            .path()
            .join("packages/example/.codex-plugin/plugin.json")
            .exists()
            && catalog(home.path()).unwrap().installed.len() == 1
    );
}

#[test]
fn marketplace_rejects_traversal_and_preserves_valid_siblings() {
    let marketplace = tempfile::tempdir().unwrap();
    source::write_json(&marketplace.path().join(".claude-plugin/marketplace.json"), &json!({"name":"mixed","plugins":[{"name":"unsafe","source":"../outside"},{"name":"good","source":"./good"}]})).unwrap();
    let (_, plugins, warnings) = manifest::marketplace(marketplace.path(), "mixed").unwrap();
    assert!(plugins.len() == 1 && plugins[0].name == "good" && warnings.len() == 1);
}

#[test]
fn marketplace_supports_git_subdir_pinned_sha_and_npm_ignore_scripts_source() {
    let marketplace = tempfile::tempdir().unwrap();
    let source = source::marketplace_package(Some(&json!({"source":"git-subdir","url":"owner/repo","path":"plugins/tool","ref":"release","sha":"0123456789abcdef0123456789abcdef01234567"})),marketplace.path(),false).unwrap();
    assert!(matches!(
        source,
        PackageSource::Git {
            path: Some(_),
            sha: Some(_),
            ..
        }
    ));
    let npm = source::marketplace_package(Some(&json!({"source":"npm","package":"@owner/plugin","version":"1.2.3","registry":"https://registry.npmjs.org"})),marketplace.path(),false).unwrap();
    assert!(matches!(npm, PackageSource::Npm { .. }));
}

#[test]
fn authoring_proposals_cannot_embed_literal_credentials() {
    let mut draft = draft("credentials");
    draft.mcp_servers.insert("private".into(),json!({"url":"https://example.com/mcp","http_headers":{"Authorization":"Bearer private-value"}}));
    assert_eq!(
        validate_authoring_operation(&Operation::Create { draft })
            .unwrap_err()
            .code,
        "plugin_credentials_require_user"
    );
}

#[test]
fn authoring_allows_environment_credentials_and_bounded_package_files() {
    let mut draft = draft("environment");
    draft.mcp_servers.insert(
        "private".into(),
        json!({"command":"node","env":{"API_KEY":"${SERVICE_API_KEY}"}}),
    );
    draft.files.push(DraftFile {
        path: "scripts/server.js".into(),
        content: "const apiKey = process.env.SERVICE_API_KEY;".into(),
    });
    assert!(validate_authoring_operation(&Operation::Create { draft }).is_ok());
}

#[test]
fn authoring_accepts_public_bearer_environment_variable_names() {
    let mut draft = draft("variable-names");
    draft.mcp_servers.insert("remote".into(), json!({"url":"https://mcp.example.test","bearer_token_env_var":"API_TOKEN","env_vars":["API_TOKEN"]}));
    assert!(validate_authoring_operation(&Operation::Create { draft }).is_ok());
}

#[test]
fn hook_parser_filters_unsupported_handlers_without_dropping_valid_commands() {
    let package = tempfile::tempdir().unwrap();
    source::write_json(&package.path().join(".codex-plugin/plugin.json"), &json!({"name":"hooks","hooks":{"hooks":{"Stop":[{"hooks":[{"type":"prompt"},{"type":"command","command":"echo checked"}]}],"FutureEvent":[]}}})).unwrap();
    let parsed = manifest::parse(package.path()).unwrap();
    assert!(
        parsed.hooks[0].definition["hooks"]["Stop"][0]["hooks"]
            .as_array()
            .unwrap()
            .len()
            == 1
            && parsed.warnings.len() == 2
    );
}

#[tokio::test]
async fn zip_import_accepts_single_wrapped_package_without_running_code() {
    use std::io::Write;
    let home = tempfile::tempdir().unwrap();
    let archive = home.path().join("plugin.zip");
    let mut zip = zip::ZipWriter::new(fs::File::create(&archive).unwrap());
    zip.start_file(
        "wrapped/.codex-plugin/plugin.json",
        zip::write::SimpleFileOptions::default(),
    )
    .unwrap();
    zip.write_all(br#"{"name":"archived","version":"2.0.0"}"#)
        .unwrap();
    zip.finish().unwrap();
    let prepared = preview(
        home.path(),
        0,
        Operation::Import {
            path: archive.to_string_lossy().into_owned(),
        },
    )
    .await
    .unwrap();
    let installed = apply(home.path(), &prepared).unwrap();
    assert_eq!(installed.installed[0].version, "2.0.0");
}

#[test]
fn archive_traversal_is_rejected_before_writing_outside_destination() {
    use std::io::Write;
    let home = tempfile::tempdir().unwrap();
    let archive = home.path().join("unsafe.zip");
    let mut zip = zip::ZipWriter::new(fs::File::create(&archive).unwrap());
    zip.start_file("../escaped.txt", zip::write::SimpleFileOptions::default())
        .unwrap();
    zip.write_all(b"outside").unwrap();
    zip.finish().unwrap();
    let destination = home.path().join("unpacked");
    fs::create_dir(&destination).unwrap();
    assert!(
        source::unpack(&archive, &destination).is_err()
            && !home.path().join("escaped.txt").exists()
    );
}

#[test]
fn unsupported_portable_schema_does_not_fall_back_to_legacy_manifest() {
    let package = tempfile::tempdir().unwrap();
    source::write_json(&package.path().join("plugin.json"),&json!({"$schema":"https://agent-plugins.org/schemas/2.0.0/plugin.schema.json","name":"future"})).unwrap();
    source::write_json(
        &package.path().join(".codex-plugin/plugin.json"),
        &json!({"name":"legacy"}),
    )
    .unwrap();
    assert_eq!(
        manifest::parse(package.path()).unwrap_err().code,
        "unsupported_plugin_schema"
    );
}

#[tokio::test]
async fn mutable_catalog_metadata_cannot_replace_verified_hook_command() {
    let home = tempfile::tempdir().unwrap();
    let mut draft = draft("derived");
    draft.hooks =
        Some(json!({"hooks":{"Stop":[{"hooks":[{"type":"command","command":"echo approved"}]}]}}));
    create(home.path(), draft).await;
    let approved = preview(
        home.path(),
        1,
        Operation::TrustHooks {
            plugin_id: "derived@local".into(),
            trusted: true,
        },
    )
    .await
    .unwrap();
    apply(home.path(), &approved).unwrap();
    let path = catalog_file(home.path());
    let mut stored: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    stored["installed"][0]["parsed"]["hooks"][0]["definition"]["hooks"]["Stop"][0]["hooks"][0]
        ["command"] = json!("echo unapproved");
    fs::write(path, serde_json::to_vec(&stored).unwrap()).unwrap();
    assert_eq!(
        load_active(home.path()).unwrap().hook_sources[0].definition["hooks"]["Stop"][0]["hooks"]
            [0]["command"],
        "echo approved"
    );
}

#[tokio::test]
async fn staged_package_changes_are_rejected_before_activation() {
    let home = tempfile::tempdir().unwrap();
    let prepared = preview(
        home.path(),
        0,
        Operation::Create {
            draft: draft("staged"),
        },
    )
    .await
    .unwrap();
    let staging = plugin_home(home.path()).join("staging");
    let stage = fs::read_dir(staging)
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    fs::write(stage.join("staged/skills/review/SKILL.md"), "changed").unwrap();
    assert_eq!(
        apply(home.path(), &prepared).unwrap_err().code,
        "plugin_integrity"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn unexpected_symlink_in_installed_package_revokes_integrity() {
    let home = tempfile::tempdir().unwrap();
    let installed = create(home.path(), draft("symlink")).await;
    std::os::unix::fs::symlink(
        home.path(),
        Path::new(&installed.installed[0].root_path).join("unexpected"),
    )
    .unwrap();
    assert!(!catalog(home.path()).unwrap().installed[0].integrity_valid);
}

fn presentation_fixture(root: &Path, name: &str) {
    source::write_json(&root.join(".agents/plugins/marketplace.json"), &json!({"name":"presentation","plugins":[{"name":name,"source":"./package","category":"Productivity","policy":{"products":["CODEX"],"installation":"AVAILABLE"}}]})).unwrap();
    source::write_json(&root.join("package/.codex-plugin/plugin.json"), &json!({"name":name,"version":"2.0.0","description":"Technical connector description","interface":{"displayName":"Real Product","shortDescription":"Useful short description","longDescription":"Human product description","category":"Other","composerIcon":"./assets/logo.svg"},"skills":"./skills"})).unwrap();
    fs::create_dir_all(root.join("package/skills")).unwrap();
    fs::create_dir_all(root.join("package/assets")).unwrap();
    fs::write(root.join("package/assets/logo.svg"), r##"<svg xmlns="http://www.w3.org/2000/svg" width="32" height="32"><defs><clipPath id="clip"><rect width="32" height="32"/></clipPath></defs><path clip-path="url(#clip)" fill="#ff9100" d="M0 0h32v32H0z"/></svg>"##).unwrap();
}

#[tokio::test]
async fn cached_store_hydrates_real_metadata_and_policy_without_changing_approved_source() {
    let home = tempfile::tempdir().unwrap();
    let market = tempfile::tempdir().unwrap();
    presentation_fixture(market.path(), "real-product");
    let prepared = preview(
        home.path(),
        0,
        Operation::AddMarketplace {
            source: market.path().to_string_lossy().into_owned(),
            ref_name: None,
            sparse_paths: Vec::new(),
        },
    )
    .await
    .unwrap();
    apply(home.path(), &prepared).unwrap();
    let path = catalog_file(home.path());
    let mut old: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    let available = old["marketplaces"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|record| record["entry"]["id"] == "presentation")
        .unwrap()["available"]
        .as_array_mut()
        .unwrap();
    available[0]["displayName"] = json!("real-product");
    available[0]["description"] = json!("");
    available[0]["installable"] = json!(false);
    available[0]["requirements"] = json!(["Restrito aos produtos: CODEX"]);
    for key in ["category", "shortDescription", "iconDataUrl"] {
        available[0].as_object_mut().unwrap().remove(key);
    }
    let approved_source = available[0]["source"].clone();
    let old_bytes = serde_json::to_vec(&old).unwrap();
    fs::write(&path, &old_bytes).unwrap();
    let plain = catalog(home.path()).unwrap();
    let plugin = &plain.available[0];
    assert_eq!(plugin.display_name, "Real Product");
    assert_eq!(plugin.description, "Human product description");
    assert_eq!(
        plugin.short_description.as_deref(),
        Some("Useful short description")
    );
    assert_eq!(plugin.category.as_deref(), Some("Productivity"));
    assert!(plugin.installable && plugin.requirements.is_empty() && plugin.icon_data_url.is_none());
    assert_eq!(
        serde_json::to_value(&plugin.source).unwrap(),
        approved_source
    );
    assert_eq!(plain.revision, 1);
    assert_eq!(fs::read(&path).unwrap(), old_bytes);
    let ui = catalog_with_icons(home.path()).unwrap();
    assert!(ui.available[0]
        .icon_data_url
        .as_ref()
        .unwrap()
        .starts_with("data:image/svg+xml;base64,"));
    assert!(!serde_json::to_string(&plain)
        .unwrap()
        .contains("data:image"));
    let install = preview(
        home.path(),
        1,
        Operation::Install {
            plugin_id: plugin.id.clone(),
        },
    )
    .await
    .unwrap();
    let installed = apply(home.path(), &install).unwrap();
    assert_eq!(
        installed.installed[0].category.as_deref(),
        Some("Productivity")
    );
    assert!(installed.installed[0].icon_data_url.is_none());
    assert!(catalog_with_icons(home.path()).unwrap().installed[0]
        .icon_data_url
        .is_some());
    assert!(!fs::read_to_string(path).unwrap().contains("data:image"));
}

#[tokio::test]
async fn recommended_marketplaces_migrate_once_and_removed_sources_stay_removed() {
    let home = tempfile::tempdir().unwrap();
    let fresh = catalog(home.path()).unwrap();
    assert_eq!(fresh.marketplaces.len(), 6);
    assert_eq!(
        fresh
            .marketplaces
            .iter()
            .filter(|market| market.built_in)
            .count(),
        1
    );
    let path = catalog_file(home.path());
    let original_openai =
        json!({"entry":fresh.marketplaces[0],"root":null,"hash":null,"available":[],"issues":[]});
    source::write_json(
        &path,
        &json!({"revision":7,"marketplaces":[original_openai],"installed":[],"apps_account_id":null}),
    )
    .unwrap();
    let migrated = catalog(home.path()).unwrap();
    assert_eq!(migrated.revision, 7);
    assert_eq!(migrated.marketplaces.len(), 6);
    assert!(migrated
        .marketplaces
        .iter()
        .any(|market| market.source == "https://github.com/firebase/agent-skills.git"));
    let removed = preview(
        home.path(),
        7,
        Operation::RemoveMarketplace {
            marketplace_id: "firebase".into(),
        },
    )
    .await
    .unwrap();
    let result = apply(home.path(), &removed).unwrap();
    assert_eq!(result.marketplaces.len(), 5);
    assert!(!catalog(home.path())
        .unwrap()
        .marketplaces
        .iter()
        .any(|market| market.id == "firebase"));
    assert_eq!(
        serde_json::from_slice::<Value>(&fs::read(path).unwrap()).unwrap()["builtins_version"],
        1
    );
}

#[test]
fn existing_recommended_marketplace_alias_keeps_its_user_selected_source_and_ref() {
    let home = tempfile::tempdir().unwrap();
    let existing = json!({"entry":{"id":"claude-plugins-official","name":"Anthropic personalizado","source":"git@github.com:anthropics/claude-plugins-official.git","refName":"approved-release","sparsePaths":["plugins"],"refreshed":false,"builtIn":false},"root":null,"hash":null,"available":[],"issues":[]});
    source::write_json(
        &catalog_file(home.path()),
        &json!({"revision":3,"marketplaces":[existing],"installed":[],"apps_account_id":null}),
    )
    .unwrap();
    let migrated = catalog(home.path()).unwrap();
    let entries: Vec<_> = migrated
        .marketplaces
        .iter()
        .filter(|market| market.id == "claude-plugins-official")
        .collect();
    assert_eq!(entries.len(), 1);
    assert_eq!(
        entries[0].source,
        "git@github.com:anthropics/claude-plugins-official.git"
    );
    assert_eq!(entries[0].ref_name.as_deref(), Some("approved-release"));
    assert_eq!(entries[0].sparse_paths, ["plugins"]);
    assert_eq!(migrated.revision, 3);
    assert!(!plugin_home(home.path()).join("staging").exists());
}

#[tokio::test]
async fn automatic_discovery_only_reads_selected_catalogs_and_does_not_retry_failed_sources() {
    let home = tempfile::tempdir().unwrap();
    let market = tempfile::tempdir().unwrap();
    presentation_fixture(market.path(), "discovery-product");
    let missing = fs::canonicalize(home.path())
        .unwrap()
        .join("missing-marketplace");
    let good = fs::canonicalize(market.path())
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let bad = missing.to_string_lossy().into_owned();
    let entry = |id: &str, source: &str| json!({"entry":{"id":id,"name":id,"source":source,"refName":null,"sparsePaths":[],"refreshed":false,"builtIn":false},"root":null,"hash":null,"available":[],"issues":[]});
    source::write_json(&catalog_file(home.path()), &json!({"revision":0,"marketplaces":[entry("good", &good),entry("bad", &bad)],"installed":[],"apps_account_id":null,"builtins_version":1})).unwrap();
    let failures = store::discover_catalogs(home.path(), &[&good, &bad]).await;
    assert_eq!(failures.len(), 1);
    assert!(failures[0].contains("Atualizar loja"));
    let initial = catalog(home.path()).unwrap();
    assert_eq!(initial.available.len(), 1);
    assert!(
        initial.installed.is_empty() && load_active(home.path()).unwrap().skill_roots.is_empty()
    );
    presentation_fixture(&missing, "later-product");
    assert_eq!(
        store::discover_catalogs(home.path(), &[&good, &bad]).await,
        failures
    );
    assert_eq!(catalog(home.path()).unwrap().revision, initial.revision);
    // A manual retry remains possible and removes the visible failure after success.
    let retry = preview(
        home.path(),
        initial.revision,
        Operation::RefreshMarketplace {
            marketplace_id: Some("bad".into()),
        },
    )
    .await
    .unwrap_err();
    assert_eq!(retry.code, "marketplace_conflict");
    source::write_json(
        &missing.join(".agents/plugins/marketplace.json"),
        &json!({"name":"recovered","plugins":[{"name":"later-product","source":"./package"}]}),
    )
    .unwrap();
    let retry = preview(
        home.path(),
        initial.revision,
        Operation::RefreshMarketplace {
            marketplace_id: Some("bad".into()),
        },
    )
    .await
    .unwrap();
    apply(home.path(), &retry).unwrap();
    assert!(store::discover_catalogs(home.path(), &[&good, &bad])
        .await
        .is_empty());
    assert!(catalog(home.path()).unwrap().installed.is_empty());
}

#[test]
fn self_url_marketplace_source_is_owned_local_and_unsupported_entries_are_explicit() {
    let market = tempfile::tempdir().unwrap();
    presentation_fixture(market.path(), "present");
    fs::create_dir_all(market.path().join("only-lsp")).unwrap();
    source::write_json(&market.path().join(".agents/plugins/marketplace.json"), &json!({"name":"self","plugins":[{"name":"self","source":{"source":"url","url":"./"}},{"name":"only-lsp","source":"./only-lsp","strict":false,"lspServers":{"rust":{"command":"rust-analyzer"}}}]})).unwrap();
    source::write_json(
        &market.path().join(".codex-plugin/plugin.json"),
        &json!({"name":"self"}),
    )
    .unwrap();
    let (_, available, _) = manifest::marketplace(market.path(), "self").unwrap();
    assert!(
        matches!(&available[0].source, PackageSource::Local { path } if Path::new(path) == market.path())
    );
    assert!(!available[1].installable);
    assert!(available[1]
        .requirements
        .iter()
        .any(|requirement| requirement.contains("LSP")));
    assert!(available[1]
        .requirements
        .iter()
        .any(|requirement| requirement.contains("manifest compatível")));
}

#[tokio::test]
#[ignore = "Read-only network smoke for the real recommended marketplaces"]
async fn live_recommended_marketplaces_and_firebase_preview() {
    let home = tempfile::tempdir().unwrap();
    let failures = discover_builtin_catalogs(home.path()).await;
    let catalog = catalog_with_icons(home.path()).unwrap();
    for market in &catalog.marketplaces {
        let available: Vec<_> = catalog
            .available
            .iter()
            .filter(|plugin| plugin.marketplace_id == market.id)
            .collect();
        eprintln!(
            "Marketplace {}: refreshed={}, plugins={}, icons={}, categories={}, unavailable={}",
            market.id,
            market.refreshed,
            available.len(),
            available
                .iter()
                .filter(|plugin| plugin.icon_data_url.is_some())
                .count(),
            available
                .iter()
                .filter(|plugin| plugin.category.is_some())
                .count(),
            available
                .iter()
                .filter(|plugin| !plugin.installable)
                .count()
        );
    }
    assert!(failures.is_empty(), "{failures:?}");
    assert!(catalog.marketplaces.iter().all(|market| market.refreshed));
    let firebase = catalog
        .available
        .iter()
        .find(|plugin| plugin.name == "firebase")
        .unwrap();
    assert!(firebase.installable && firebase.icon_data_url.is_some());
    let prepared = preview(
        home.path(),
        catalog.revision,
        Operation::Install {
            plugin_id: firebase.id.clone(),
        },
    )
    .await
    .unwrap();
    eprintln!(
        "Firebase preview: {} components; commands={:?}",
        prepared.preview.components.len(),
        prepared.preview.commands
    );
    assert!(prepared
        .preview
        .commands
        .iter()
        .any(|command| command.contains("firebase-tools")));
    assert!(self::catalog(home.path()).unwrap().installed.is_empty());
}
