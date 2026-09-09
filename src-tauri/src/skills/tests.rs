use super::*;
use serde_json::json;

fn skill(dir: &Path, name: &str, body: &str) {
    fs::create_dir_all(dir.join("references")).unwrap();
    fs::write(
        dir.join("SKILL.md"),
        format!(
            "---\nname: {name}\ndescription: >-\n  A useful workflow\n  for testing.\n---\n{body}"
        ),
    )
    .unwrap();
    fs::write(dir.join("references/guide.md"), "Reference instructions").unwrap();
}

#[cfg(unix)]
#[test]
fn shared_discovery_reads_two_links_deduplicates_cycles_and_ignores_other_roots() {
    use std::os::unix::fs::symlink;
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    let agents = home.join(".agents/skills");
    fs::create_dir_all(&agents).unwrap();
    for name in ["nextjs-developer", "nextjs-best-practices"] {
        let target = home.join("managed").join(name);
        skill(&target, name, "Linked instructions");
        symlink(target, agents.join(name)).unwrap();
    }
    symlink(
        home.join("managed/nextjs-developer"),
        agents.join("duplicate"),
    )
    .unwrap();
    symlink(&agents, agents.join("cycle")).unwrap();
    symlink(home.join("missing"), agents.join("broken")).unwrap();
    skill(
        &home.join(".codex/skills/excluded"),
        "excluded",
        "Do not discover",
    );
    skill(
        &home.join(".skills-manager/skills/unlinked"),
        "unlinked",
        "Do not discover",
    );
    assert!(snapshot(home, None).unwrap().skills.is_empty());
    write_config(
        home,
        &Config {
            include_agents: true,
            ..Config::default()
        },
    )
    .unwrap();
    let found = snapshot(home, None).unwrap().skills;
    assert_eq!(found.len(), 2);
    assert!(found
        .iter()
        .all(|skill| skill.linked && skill.origin == "agents"));
    assert!(found
        .iter()
        .all(|skill| catalog::resource(skill, "SKILL.md")
            .unwrap()
            .contains("Linked instructions")));
}

#[test]
#[ignore = "Read-only check of user-provided links in the host .agents/skills"]
fn live_shared_skill_links() {
    let home = PathBuf::from(std::env::var_os("HOME").unwrap());
    let config = Config {
        include_agents: true,
        ..Config::default()
    };
    let (found, _) = catalog::discover(&home, None, &config).unwrap();
    for name in ["nextjs-developer", "nextjs-best-practices"] {
        let entry = found
            .iter()
            .find(|skill| skill.name == name && skill.origin == "agents")
            .expect("Expected user-provided global link");
        assert!(entry.linked);
        assert!(!catalog::resource(entry, "SKILL.md").unwrap().is_empty());
    }
}

#[test]
fn deletion_removes_only_selected_skill_and_preserves_neighbors() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    let own = root(home).join("skills/one");
    let other = root(home).join("skills/two");
    skill(&own, "one", "Instructions");
    skill(&other, "two", "Keep");
    let found = snapshot(home, None).unwrap().skills;
    let first = found.iter().find(|s| s.name == "one").unwrap();
    write_config(
        home,
        &Config {
            disabled: [first.id.clone()].into_iter().collect(),
            ..Config::default()
        },
    )
    .unwrap();
    let after = remove(home, None, &first.id).unwrap();
    assert!(!read_config(home).unwrap().disabled.contains(&first.id));
    assert!(!own.exists());
    assert!(other.join("SKILL.md").is_file());
    assert_eq!(after.skills.len(), 1);
    assert!(remove(home, None, "../two").is_err());
}

#[test]
fn portable_backup_keeps_installed_skill_state_across_different_home_paths() {
    let source = tempfile::tempdir().unwrap();
    let target = tempfile::tempdir().unwrap();
    for home in [source.path(), target.path()] {
        skill(
            &root(home).join("skills/review"),
            "review",
            "Review instructions",
        );
    }
    let source_skill = snapshot(source.path(), None).unwrap().skills.remove(0);
    write_config(
        source.path(),
        &Config {
            include_agents: true,
            disabled: [source_skill.id].into_iter().collect(),
        },
    )
    .unwrap();

    let portable = backup_config(source.path()).unwrap();
    assert!(portable.include_agents);
    assert_eq!(
        portable.disabled_skills,
        ["review".into()].into_iter().collect()
    );
    write_config(
        target.path(),
        &restore_config(target.path(), &portable).unwrap(),
    )
    .unwrap();

    let restored = snapshot(target.path(), None).unwrap();
    assert!(!restored.skills[0].enabled);
    assert!(restored.include_agents);
}

#[cfg(unix)]
#[test]
fn deleting_shared_link_unlinks_without_following_target_and_rejects_linked_container() {
    use std::os::unix::fs::symlink;
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    let target = home.join("external/workflow");
    let linked = home.join(".agents/skills/shared");
    skill(&target, "shared", "Keep target");
    fs::create_dir_all(linked.parent().unwrap()).unwrap();
    symlink(&target, &linked).unwrap();
    write_config(
        home,
        &Config {
            include_agents: true,
            ..Config::default()
        },
    )
    .unwrap();
    let found = snapshot(home, None).unwrap().skills.pop().unwrap();
    assert!(found.linked);
    remove(home, None, &found.id).unwrap();
    assert!(target.join("SKILL.md").is_file());
    assert!(fs::symlink_metadata(&linked).is_err());
    symlink(home.join("external"), &linked).unwrap();
    let found = snapshot(home, None).unwrap().skills.pop().unwrap();
    assert!(remove(home, None, &found.id).is_err());
    assert!(target.join("SKILL.md").is_file());
}
#[test]
fn discovery_defaults_to_jarvis_and_agents_switch_includes_global_and_project() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    let project = home.join("project");
    skill(&root(home).join("skills/own"), "own", "Jarvis body");
    skill(&home.join(".agents/skills/shared"), "shared", "Global body");
    skill(
        &project.join(".agents/skills/local"),
        "local",
        "Project body",
    );
    let initial = snapshot(home, Some(&project)).unwrap();
    assert!(!initial.include_agents);
    assert_eq!(initial.skills.len(), 1);
    write_config(
        home,
        &Config {
            include_agents: true,
            ..Config::default()
        },
    )
    .unwrap();
    let found = snapshot(home, Some(&project)).unwrap();
    assert_eq!(found.skills.len(), 3);
    assert!(found.skills.iter().any(|s| s.origin == "project"));
    assert_eq!(
        found.skills[0].description,
        "A useful workflow for testing."
    );
    assert!(read_config(home).unwrap().include_agents);
}

#[test]
fn configuration_changes_do_not_wait_for_marketplace_catalog_work() {
    use std::{sync::mpsc, time::Duration};

    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    let disabled_id = "skill-being-toggled".to_string();
    let catalog_guard = CATALOG_LOCK.lock().unwrap();
    let (sender, receiver) = mpsc::channel();

    std::thread::scope(|scope| {
        scope.spawn(|| {
            let result = update_config(home, |config| {
                config.disabled.insert(disabled_id.clone());
            });
            sender.send(result).unwrap();
        });

        let result = receiver
            .recv_timeout(Duration::from_secs(2))
            .expect("skills.json should remain writable during a remote catalog check");
        drop(catalog_guard);
        result.unwrap();
    });

    assert!(read_config(home).unwrap().disabled.contains(&disabled_id));
}

#[tokio::test]
async fn progressive_loading_reads_only_enabled_skills_and_confines_references() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    let project = home.join("project");
    fs::create_dir_all(&project).unwrap();
    skill(
        &root(home).join("skills/own"),
        "own",
        "Private instructions only on demand",
    );
    let skills = active(home, &project).await.unwrap();
    let id = skills[0].id.clone();
    let listed = prompt(&skills);
    assert!(listed.contains("A useful workflow"));
    assert!(!listed.contains("Private instructions"));
    assert!(read(home, &project, &json!({"id":id}))
        .await
        .unwrap()
        .contains("Private instructions"));
    assert!(read(home, &project, &json!({"id":id,"path":"  "}))
        .await
        .unwrap()
        .contains("Private instructions"));
    assert!(read(
        home,
        &project,
        &json!({"id":id,"path":"references/guide.md"})
    )
    .await
    .unwrap()
    .contains("Reference instructions"));
    assert!(
        read(home, &project, &json!({"id":id,"path":"../../skills.json"}))
            .await
            .is_err()
    );
    assert!(read(home, &project, &json!({"id":id,"path":"/etc/passwd"}))
        .await
        .is_err());
    fs::write(
        root(home).join("skills/own/references/large.md"),
        vec![b'x'; MAX_TEXT + 1],
    )
    .unwrap();
    let oversized = read(
        home,
        &project,
        &json!({"id":id,"path":"references/large.md"}),
    )
    .await
    .unwrap_err();
    assert!(oversized.message.contains("own"));
    assert!(oversized.message.contains("1 MiB"));
    write_config(
        home,
        &Config {
            disabled: BTreeSet::from([id.clone()]),
            ..Config::default()
        },
    )
    .unwrap();
    assert!(active(home, &project).await.unwrap().is_empty());
    assert!(read(home, &project, &json!({"id":id}))
        .await
        .unwrap_err()
        .message
        .contains("desativada"));
}
#[test]
fn invalid_skills_are_reported_and_manual_skills_are_not_automatically_advertised() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    let base = root(home).join("skills");
    skill(&base.join("manual"), "manual", "Manual instructions");
    fs::write(base.join("manual/SKILL.md"),"---\nname: manual\ndescription: Only on request\ndisable-model-invocation: true\n---\nInstructions").unwrap();
    fs::create_dir_all(base.join("invalid")).unwrap();
    fs::write(base.join("invalid/SKILL.md"), "No metadata").unwrap();
    let found = snapshot(home, None).unwrap();
    assert_eq!(found.skills.len(), 1);
    assert_eq!(found.warnings.len(), 1);
    assert!(prompt(&found.skills).is_empty());
    assert_eq!(
        catalog::escape("<skill> & \"x\""),
        "&lt;skill&gt; &amp; &quot;x&quot;"
    );
}
#[cfg(unix)]
#[test]
fn shared_skill_links_work_but_nested_reference_escapes_are_rejected() {
    use std::os::unix::fs::symlink;
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    let dir = home.join("external/safe");
    skill(&dir, "linked", "Linked body");
    fs::create_dir_all(home.join(".agents/skills")).unwrap();
    symlink(&dir, home.join(".agents/skills/linked")).unwrap();
    symlink(home, dir.join("escape")).unwrap();
    write_config(
        home,
        &Config {
            include_agents: true,
            ..Config::default()
        },
    )
    .unwrap();
    let found = snapshot(home, None).unwrap();
    assert_eq!(found.skills.len(), 1);
    assert!(catalog::resource(&found.skills[0], "escape/.jarvis/skills.json").is_err());
}
#[test]
fn marketplace_parses_rankings_searches_and_rejects_unsafe_sources() {
    let plain = r#"{"source":"vercel-labs/skills","skillId":"find-skills","name":"find-skills","installs":500}"#;
    let rsc = format!(
        "<script>self.__next_f.push([1,{}])</script>",
        serde_json::to_string(&format!("[{plain}]")).unwrap()
    );
    assert_eq!(marketplace::parse_board(&rsc).unwrap()[0].installs, 500);
    let entries = marketplace::parse_search(&format!(r#"{{"skills":[{plain},{plain}]}}"#)).unwrap();
    assert_eq!(entries.len(), 1);
    assert!(marketplace::parse_board("<html>Rate limited</html>").is_err());
    assert!(marketplace::parse_search("{}").is_err());
    for (source, id) in [
        ("../../bad", "skill"),
        ("https://evil.test/repo", "skill"),
        ("owner/repo", "../escape"),
        ("owner/repo;whoami", "skill"),
    ] {
        assert!(store::validate(source, id).is_err());
    }
}

#[test]
fn marketplace_resolution_ignores_materialized_compatibility_aliases() {
    let temp = tempfile::tempdir().unwrap();
    let canonical = temp.path().join("engineering-team/skills/senior-backend");
    skill(&canonical, "senior-backend", "Canonical package");
    let alias = temp.path().join(".gemini/skills/senior-backend");
    fs::create_dir_all(&alias).unwrap();
    fs::write(
        alias.join("SKILL.md"),
        "../../../engineering-team/skills/senior-backend/SKILL.md",
    )
    .unwrap();

    assert_eq!(
        store::resolve(temp.path(), "senior-backend").unwrap(),
        canonical
    );
}

#[cfg(unix)]
#[test]
fn marketplace_resolution_ignores_symlinked_compatibility_aliases() {
    let temp = tempfile::tempdir().unwrap();
    let canonical = temp.path().join("engineering-team/skills/senior-backend");
    skill(&canonical, "senior-backend", "Canonical package");
    let alias = temp.path().join(".gemini/skills/senior-backend");
    fs::create_dir_all(&alias).unwrap();
    std::os::unix::fs::symlink(
        "../../../engineering-team/skills/senior-backend/SKILL.md",
        alias.join("SKILL.md"),
    )
    .unwrap();

    assert_eq!(
        store::resolve(temp.path(), "senior-backend").unwrap(),
        canonical
    );
}

#[test]
fn marketplace_resolution_prefers_standard_authoring_roots_over_generated_plugin_copies() {
    let temp = tempfile::tempdir().unwrap();
    let authoring = temp.path().join(".claude/skills/tauri-v2");
    let generated = temp.path().join("plugins/cce-tauri/skills/tauri-v2");
    skill(&authoring, "tauri-v2", "Authoring package");
    skill(&generated, "tauri-v2", "Generated plugin package");

    assert_eq!(store::resolve(temp.path(), "tauri-v2").unwrap(), authoring);
}

#[test]
fn marketplace_resolution_prefers_unified_agents_root_over_provider_variants() {
    let temp = tempfile::tempdir().unwrap();
    let unified = temp.path().join(".agents/skills/example");
    let provider = temp.path().join(".claude/skills/example");
    skill(&unified, "example", "Unified package");
    skill(&provider, "example", "Provider package");

    assert_eq!(store::resolve(temp.path(), "example").unwrap(), unified);
}

#[test]
fn marketplace_resolution_keeps_genuinely_nested_duplicates_ambiguous() {
    let temp = tempfile::tempdir().unwrap();
    skill(
        &temp.path().join("category-a/example"),
        "example",
        "First package",
    );
    skill(
        &temp.path().join("category-b/example"),
        "example",
        "Second package",
    );

    assert!(store::resolve(temp.path(), "example")
        .unwrap_err()
        .message
        .contains("mais de uma skill"));
}
#[test]
fn installs_complete_packages_and_recovers_interrupted_updates() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    let remote = home.join("remote");
    skill(&remote, "example", "Version one");
    let destination = store::target(home, "owner/repo", "example");
    let meta = store::Metadata {
        source: "owner/repo".into(),
        skill_id: "example".into(),
        subpath: "skills/example".into(),
        digest: store::digest(&remote).unwrap(),
        update_available: false,
        update_error: None,
    };
    store::replace(home, &remote, &destination, &meta).unwrap();
    assert_eq!(
        fs::read_to_string(destination.join("references/guide.md")).unwrap(),
        "Reference instructions"
    );
    let snapshot = snapshot(home, None).unwrap();
    assert_eq!(
        snapshot.skills[0].marketplace_id.as_deref(),
        Some("owner/repo/example")
    );
    let transaction = root(home).join("skill-transactions/interrupted");
    fs::create_dir_all(&transaction).unwrap();
    fs::write(
        transaction.join("target.json"),
        serde_json::to_string(destination.file_name().unwrap().to_str().unwrap()).unwrap(),
    )
    .unwrap();
    fs::rename(&destination, transaction.join("previous")).unwrap();
    store::recover(home).unwrap();
    assert!(destination.join("SKILL.md").exists());
    assert!(!transaction.exists());
    skill(&remote, "example", "Version two");
    assert_ne!(store::digest(&remote).unwrap(), meta.digest);
    let mut newer = meta.clone();
    newer.digest = store::digest(&remote).unwrap();
    store::replace(home, &remote, &destination, &newer).unwrap();
    assert!(fs::read_to_string(destination.join("SKILL.md"))
        .unwrap()
        .contains("Version two"));
    assert_eq!(store::digest(&destination).unwrap(), newer.digest);
}
#[test]
fn updating_refuses_to_overwrite_local_modifications() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    let remote = home.join("remote");
    skill(&remote, "example", "Original");
    let destination = store::target(home, "owner/repo", "example");
    let meta = store::Metadata {
        source: "owner/repo".into(),
        skill_id: "example".into(),
        subpath: PathBuf::new(),
        digest: store::digest(&remote).unwrap(),
        update_available: true,
        update_error: None,
    };
    store::replace(home, &remote, &destination, &meta).unwrap();
    fs::write(
        destination.join("references/guide.md"),
        "Local modification",
    )
    .unwrap();
    let installed = snapshot(home, None).unwrap().skills.remove(0);
    assert!(store::update(home, &installed)
        .unwrap_err()
        .message
        .contains("alterações locais"));
    assert_eq!(
        fs::read_to_string(destination.join("references/guide.md")).unwrap(),
        "Local modification"
    );
}
#[cfg(unix)]
#[test]
fn unsafe_packages_do_not_replace_an_existing_installation() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    let remote = home.join("remote");
    skill(&remote, "example", "Original");
    let destination = store::target(home, "owner/repo", "example");
    let meta = store::Metadata {
        source: "owner/repo".into(),
        skill_id: "example".into(),
        subpath: PathBuf::new(),
        digest: store::digest(&remote).unwrap(),
        update_available: false,
        update_error: None,
    };
    store::replace(home, &remote, &destination, &meta).unwrap();
    std::os::unix::fs::symlink(home, remote.join("escape")).unwrap();
    assert!(store::replace(home, &remote, &destination, &meta).is_err());
    assert!(destination.join("SKILL.md").exists());
}
#[test]
#[ignore = "Opt-in public marketplace and isolated package download diagnostic"]
fn live_marketplace_and_install() {
    let temp = tempfile::tempdir().unwrap();
    for ranking in ["alltime", "trending", "hot"] {
        let results = marketplace::browse("", ranking, 60).unwrap();
        assert!(!results.is_empty());
        println!("{ranking}: {} entries", results.len());
    }
    let results = marketplace::browse("find-skills", "alltime", 60).unwrap();
    assert!(results
        .iter()
        .any(|s| s.source == "vercel-labs/skills" && s.skill_id == "find-skills"));
    let detail = store::preview(temp.path(), "vercel-labs/skills", "find-skills").unwrap();
    assert!(!detail.content.is_empty());
    store::install(temp.path(), "vercel-labs/skills", "find-skills").unwrap();
    let found = snapshot(temp.path(), None).unwrap();
    assert_eq!(found.skills.len(), 1);
    assert!(found.skills[0].enabled);
    store::check(temp.path()).unwrap();
    assert!(!snapshot(temp.path(), None).unwrap().skills[0].update_available);
    store::update(temp.path(), &found.skills[0]).unwrap();
    println!("Public search, preview, isolated installation, check and update passed");
}

#[test]
fn large_catalog_is_bounded_and_search_reaches_skills_omitted_from_prompt() {
    let skills: Vec<_> = (0..200)
        .map(|index| Skill {
            id: format!("skill-{index}"),
            name: format!("workflow-{index}"),
            description: "Useful instructions ".repeat(50),
            origin: "jarvis".into(),
            path: PathBuf::new(),
            removal_path: PathBuf::new(),
            linked: false,
            managed: false,
            file: PathBuf::new(),
            enabled: true,
            automatic: true,
            source: None,
            marketplace_id: None,
            update_available: false,
            update_error: None,
        })
        .collect();
    let instructions = prompt(&skills);
    assert!(instructions.len() < 35_000);
    assert!(instructions.contains("find_skills"));
    assert!(!instructions.contains("workflow-199"));
    let found: serde_json::Value =
        serde_json::from_str(&search(&skills, &json!({"query":"workflow-199"})).unwrap()).unwrap();
    assert_eq!(found["skills"][0]["id"], "skill-199");
    assert_eq!(found["total"], 1);
}

#[cfg(unix)]
#[test]
fn skill_document_links_cannot_read_outside_the_package() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    let outside = home.join("outside.md");
    fs::write(
        &outside,
        "---\nname: secret\ndescription: private\n---\nDo not expose",
    )
    .unwrap();
    let package = root(home).join("skills/bad");
    fs::create_dir_all(&package).unwrap();
    std::os::unix::fs::symlink(&outside, package.join("SKILL.md")).unwrap();
    let snapshot = snapshot(home, None).unwrap();
    assert!(snapshot.skills.is_empty());
    assert_eq!(snapshot.warnings.len(), 1);
}

#[test]
fn skill_paths_serialize_without_the_windows_verbatim_prefix() {
    // Skills work on Windows through filesystem discovery, so SkillDetailsDialog
    // and the delete dialog would otherwise show \\?\C:\ to the user.
    let stored = if cfg!(windows) {
        PathBuf::from(r"\\?\C:\Users\me\.jarvis\skills\demo")
    } else {
        PathBuf::from("/home/me/.jarvis/skills/demo")
    };
    let skill = Skill {
        id: "demo".into(),
        name: "Demo".into(),
        description: "d".into(),
        origin: "jarvis".into(),
        path: stored.clone(),
        removal_path: stored.clone(),
        linked: false,
        managed: false,
        file: stored.clone(),
        enabled: true,
        automatic: false,
        source: None,
        marketplace_id: None,
        update_available: false,
        update_error: None,
    };
    let value = serde_json::to_value(&skill).unwrap();
    assert_eq!(
        value["path"],
        crate::library::strip_verbatim(&stored.to_string_lossy()).as_ref()
    );
    assert_eq!(value["removalPath"], value["path"]);
    // The Rust value keeps its canonical form for the delete containment checks.
    assert_eq!(skill.path, stored);

    let detail = Detail {
        name: "Demo".into(),
        description: "d".into(),
        content: "body".into(),
        path: Some(stored.clone()),
        source: None,
        files: vec![],
    };
    let value = serde_json::to_value(&detail).unwrap();
    assert_eq!(
        value["path"],
        crate::library::strip_verbatim(&stored.to_string_lossy()).as_ref()
    );
}
