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
    write_config(home, &Config { disabled: [first.id.clone()].into_iter().collect(), ..Config::default() }).unwrap();
    let after = remove(home, None, &first.id).unwrap();
    assert!(!read_config(home).unwrap().disabled.contains(&first.id));
    assert!(!own.exists());
    assert!(other.join("SKILL.md").is_file());
    assert_eq!(after.skills.len(), 1);
    assert!(remove(home, None, "../two").is_err());
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
    write_config(home, &Config { include_agents: true, ..Config::default() }).unwrap();
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
