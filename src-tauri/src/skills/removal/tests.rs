use super::*;
use crate::skills::{active, authorized_snapshot, builtin, root, snapshot, update_config, Skill};

fn skill(path: &Path, name: &str) {
    fs::create_dir_all(path).unwrap();
    fs::write(
        path.join("SKILL.md"),
        format!("---\nname: {name}\ndescription: Useful local guidance\n---\nInstructions\n"),
    )
    .unwrap();
}

fn select(skill: &Skill) -> Selection {
    Selection {
        id: skill.id.clone(),
        reason: "Não é mais usada.".into(),
    }
}

#[cfg(windows)]
fn junction(link: &Path, target: &Path) {
    use std::os::windows::process::CommandExt;
    // cmd's builtin does not accept the mixed separators supported by Rust paths.
    let result = std::process::Command::new("cmd.exe")
        .args(["/c", "mklink", "/J"])
        .arg(link.to_string_lossy().replace('/', "\\"))
        .arg(target.to_string_lossy().replace('/', "\\"))
        .creation_flags(0x0800_0000)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
}

#[tokio::test]
async fn removes_only_reviewed_global_shared_and_project_skills_including_disabled_entries() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    let project = home.join("project");
    for (path, name) in [
        (root(home).join("skills/global"), "global"),
        (home.join(".agents/skills/shared"), "shared"),
        (project.join(".agents/skills/local"), "local"),
    ] {
        skill(&path, name);
    }
    update_config(home, |config| config.include_agents = true).unwrap();
    let available = snapshot(home, Some(&project)).unwrap().skills;
    let disabled = available.iter().find(|s| s.name == "global").unwrap();
    update_config(home, |config| {
        config.disabled.insert(disabled.id.clone());
    })
    .unwrap();
    let listing = list(home, Some(&project), "", 0).unwrap();
    assert_eq!(listing["total"], 3);
    assert!(listing["skills"]
        .as_array()
        .unwrap()
        .iter()
        .any(|s| s["id"] == disabled.id && s["enabled"] == false && s["removable"] == true));
    let frozen = active(home, &project).await.unwrap();
    let prepared = prepare(
        home,
        Some(&project),
        available.iter().map(select).collect(),
        || false,
    )
    .unwrap();
    assert!(available.iter().all(|s| s.path.exists()));
    let result = apply(home, &prepared, || false).unwrap();
    assert_eq!(result["status"], "applied");
    assert_eq!(result["removed"].as_array().unwrap().len(), 3);
    assert!(available.iter().all(|s| !s.path.exists()));
    assert!(snapshot(home, Some(&project)).unwrap().skills.is_empty());
    assert!(authorized_snapshot(home, &project, &frozen)
        .await
        .unwrap()
        .is_empty());
    assert!(crate::skills::read_config(home)
        .unwrap()
        .disabled
        .is_empty());
}

#[test]
fn stale_files_hidden_metadata_and_empty_directories_require_a_new_review() {
    for changed in [
        "references/note.md",
        ".jarvis-source.json",
        ".git/info/exclude",
        "empty/",
    ] {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path();
        let a = root(home).join("skills/a");
        let b = root(home).join("skills/b");
        skill(&a, "first");
        skill(&b, "second");
        let available = snapshot(home, None).unwrap().skills;
        let prepared =
            prepare(home, None, available.iter().map(select).collect(), || false).unwrap();
        let file = b.join(changed);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        if changed.ends_with('/') {
            fs::create_dir_all(&file).unwrap();
        } else if changed == ".jarvis-source.json" {
            fs::write(&file, serde_json::to_vec(&json!({"source":"studio/local", "skillId":"second", "subpath":".", "digest":"changed"})).unwrap()).unwrap();
        } else {
            fs::write(&file, "User changed this after review").unwrap();
        }
        let result = apply(home, &prepared, || false).unwrap();
        assert_eq!(result["status"], "partial", "{changed}");
        assert_eq!(result["removed"].as_array().unwrap().len(), 1);
        assert!(!a.exists());
        assert!(b.exists());
        assert!(file.exists());
        assert_eq!(result["results"][1]["error"]["code"], "skill_removal_stale");
    }
}

#[test]
fn native_plugin_root_and_external_targets_cannot_be_removed() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    skill(&builtin::root(home).join("protected"), "protected");
    let native = snapshot(home, None).unwrap().skills.remove(0);
    assert!(prepare(home, None, vec![select(&native)], || false).is_err());
    assert!(native.path.exists());
    let mut candidate = native.clone();
    candidate.managed = false;
    candidate.origin = "plugin".into();
    assert!(super::super::removal_target(home, None, &candidate).is_err());
    candidate.origin = "jarvis".into();
    candidate.removal_path = root(home).join("skills");
    fs::create_dir_all(&candidate.removal_path).unwrap();
    assert!(super::super::removal_target(home, None, &candidate).is_err());
    candidate.removal_path = home.join("outside");
    skill(&candidate.removal_path, "outside");
    assert!(super::super::removal_target(home, None, &candidate).is_err());
    assert!(candidate.removal_path.exists());
}

#[test]
fn malformed_duplicate_unknown_and_cancelled_batches_do_not_delete_files() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    skill(&root(home).join("skills/kept"), "kept");
    let installed = snapshot(home, None).unwrap().skills.remove(0);
    for selections in [
        vec![],
        vec![select(&installed); 21],
        vec![select(&installed), select(&installed)],
        vec![Selection {
            id: "../escape".into(),
            reason: "x".into(),
        }],
        vec![Selection {
            id: "a".repeat(64),
            reason: "x".into(),
        }],
        vec![Selection {
            id: installed.id.clone(),
            reason: " ".into(),
        }],
    ] {
        assert!(prepare(home, None, selections, || false).is_err());
        assert!(installed.path.exists());
    }
    assert!(prepare(home, None, vec![select(&installed)], || true).is_err());
    let prepared = prepare(home, None, vec![select(&installed)], || false).unwrap();
    let result = apply(home, &prepared, || true).unwrap();
    assert_eq!(result["results"][0]["status"], "cancelled");
    assert!(installed.path.exists());
}

#[test]
fn cancellation_after_one_removal_keeps_its_receipt_and_preserves_the_remaining_skill() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    let first = root(home).join("skills/a");
    let second = root(home).join("skills/b");
    skill(&first, "first");
    skill(&second, "second");
    let available = snapshot(home, None).unwrap().skills;
    let prepared = prepare(home, None, available.iter().map(select).collect(), || false).unwrap();
    let result = apply(home, &prepared, || !first.exists()).unwrap();
    assert_eq!(result["status"], "partial");
    assert_eq!(result["results"][0]["status"], "removed");
    assert_eq!(result["results"][1]["status"], "cancelled");
    assert!(!first.exists());
    assert!(second.exists());
}

#[cfg(windows)]
#[test]
fn windows_locked_resource_reports_incomplete_deletion_and_keeps_the_residual_path() {
    use std::os::windows::fs::OpenOptionsExt;
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    let target = root(home).join("skills/locked");
    skill(&target, "locked");
    let resource = target.join("zz-resource.txt");
    fs::write(&resource, "User resource").unwrap();
    let installed = snapshot(home, None).unwrap().skills.remove(0);
    let prepared = prepare(home, None, vec![select(&installed)], || false).unwrap();
    let _locked = fs::OpenOptions::new()
        .read(true)
        .share_mode(3)
        .open(&resource)
        .unwrap();
    let result = apply(home, &prepared, || false).unwrap();
    assert_eq!(result["status"], "partial");
    assert_eq!(result["removed"].as_array().unwrap().len(), 0);
    assert_eq!(result["results"][0]["status"], "incomplete");
    assert_eq!(
        result["results"][0]["error"]["code"],
        "skill_removal_incomplete"
    );
    assert!(result["results"][0]["path"]
        .as_str()
        .unwrap()
        .ends_with("skills\\locked"));
    assert!(resource.exists());
}

#[cfg(windows)]
#[test]
fn windows_directory_junction_removal_preserves_the_original_skill_files() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    let external = home.join("external");
    skill(&external, "linked");
    let agents = home.join(".agents/skills");
    fs::create_dir_all(&agents).unwrap();
    let link = agents.join("linked");
    junction(&link, &external);
    let other = agents.join("other");
    junction(&other, &external);
    update_config(home, |config| config.include_agents = true).unwrap();
    let installed = snapshot(home, None).unwrap().skills.remove(0);
    update_config(home, |c| {
        c.disabled.insert(installed.id.clone());
    })
    .unwrap();
    let prepared = prepare(home, None, vec![select(&installed)], || false).unwrap();
    assert!(prepared.previews[0].linked);
    let result = apply(home, &prepared, || false).unwrap();
    assert_eq!(result["status"], "applied");
    assert_eq!(result["results"][0]["stillRegistered"], true);
    assert!(!snapshot(home, None).unwrap().skills.remove(0).enabled);
    assert!(other.exists());
    assert!(!link.exists());
    assert!(external.join("SKILL.md").exists());
}

#[cfg(unix)]
#[test]
fn unlinking_a_shared_skill_preserves_its_external_directory_and_nested_links() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    let external = home.join("external");
    skill(&external, "linked");
    let agents = home.join(".agents/skills");
    fs::create_dir_all(&agents).unwrap();
    std::os::unix::fs::symlink(&external, agents.join("linked")).unwrap();
    update_config(home, |config| config.include_agents = true).unwrap();
    let installed = snapshot(home, None).unwrap().skills.remove(0);
    let prepared = prepare(home, None, vec![select(&installed)], || false).unwrap();
    assert!(prepared.previews[0].linked);
    let result = apply(home, &prepared, || false).unwrap();
    assert_eq!(result["status"], "applied");
    assert!(external.join("SKILL.md").exists());
    assert!(!agents.join("linked").exists());
    let local = root(home).join("skills/local");
    skill(&local, "local");
    std::os::unix::fs::symlink(&external, local.join("reference-link")).unwrap();
    let installed = snapshot(home, None).unwrap().skills.remove(0);
    let prepared = prepare(home, None, vec![select(&installed)], || false).unwrap();
    assert_eq!(
        apply(home, &prepared, || false).unwrap()["status"],
        "applied"
    );
    assert!(external.join("SKILL.md").exists());
}

#[cfg(windows)]
#[test]
fn windows_unlinking_one_project_does_not_reenable_the_same_skill_in_another_project() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    let external = home.join("external");
    skill(&external, "shared");
    let first = home.join("project-a");
    let second = home.join("project-b");
    let mut links = Vec::new();
    for project in [&first, &second] {
        let link = project.join(".agents/skills/shared");
        fs::create_dir_all(link.parent().unwrap()).unwrap();
        junction(&link, &external);
        links.push(link);
    }
    update_config(home, |c| c.include_agents = true).unwrap();
    let installed = snapshot(home, Some(&first)).unwrap().skills.remove(0);
    update_config(home, |c| {
        c.disabled.insert(installed.id.clone());
    })
    .unwrap();
    let prepared = prepare(home, Some(&first), vec![select(&installed)], || false).unwrap();
    let result = apply(home, &prepared, || false).unwrap();
    assert_eq!(result["status"], "applied");
    assert_eq!(result["results"][0]["stillRegistered"], false);
    assert!(
        !snapshot(home, Some(&second))
            .unwrap()
            .skills
            .remove(0)
            .enabled
    );
    assert!(!links[0].exists());
    assert!(links[1].exists());
    assert!(external.join("SKILL.md").exists());
}
