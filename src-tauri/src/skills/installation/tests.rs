use super::*;
fn prepare(home: &Path, selections: Vec<Selection>) -> Result<Prepared, SkillError> {
    super::prepare(home, selections, || false)
}
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

pub(crate) fn repository(home: &Path) -> PathBuf {
    let source = "studio/blender-skills";
    let key = format!("{:x}", Sha256::digest(source.as_bytes()));
    let cached = crate::data_dir::root(home)
        .join("cache/skills/repositories")
        .join(key);
    let checkout = cached.join("checkout");
    fs::create_dir_all(checkout.join(".git")).unwrap();
    for (id, description) in [
        ("modeling", "Validate existing Blender geometry"),
        ("materials", "Review scene materials"),
    ] {
        let dir = checkout.join(id);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("SKILL.md"), format!("---\nname: {id}\ndescription: {description}\n---\nInspect the current scene before making changes.")).unwrap();
        fs::write(
            dir.join("script.ps1"),
            "throw 'This script must never execute during installation'",
        )
        .unwrap();
    }
    fs::write(cached.join(".jarvis-repository.json"), serde_json::to_vec(&json!({"source":source,"checkedAt":SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs()})).unwrap()).unwrap();
    checkout
}

fn selection(id: &str) -> Selection {
    Selection {
        source: "studio/blender-skills".into(),
        skill_id: id.into(),
        reason: "Help the requested 3D designer work with the project scene.".into(),
    }
}

#[test]
fn reviewed_batch_installs_real_packages_without_executing_scripts() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    repository(home);
    let prepared = prepare(home, vec![selection("modeling"), selection("materials")]).unwrap();
    assert_eq!(prepared.previews.len(), 2);
    assert_eq!(
        prepared.previews[0].description,
        "Validate existing Blender geometry"
    );
    assert!(!store::target(home, "studio/blender-skills", "modeling").exists());
    let detail = inspect(home, "studio/blender-skills", "modeling").unwrap();
    assert_eq!(detail["description"], prepared.previews[0].description);
    assert!(detail["content"]
        .as_str()
        .unwrap()
        .contains("Inspect the current scene"));
    let result = apply(home, &prepared, || false).unwrap();
    assert_eq!(result["status"], "applied");
    assert_eq!(result["installed"].as_array().unwrap().len(), 2);
    let (available, _) = catalog::discover(home, None, &read_config(home).unwrap()).unwrap();
    for skill in &available {
        assert!(skill.enabled);
        assert_eq!(skill.source.as_deref(), Some("studio/blender-skills"));
        assert!(result["installed"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry["id"] == skill.id));
        assert!(skill.path.join("script.ps1").is_file());
    }
    assert_eq!(available.len(), 2);
}

#[test]
fn changed_package_fails_individually_and_preserves_successes_and_existing_files() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    let checkout = repository(home);
    let prepared = prepare(home, vec![selection("modeling"), selection("materials")]).unwrap();
    fs::write(
        checkout.join("materials/script.ps1"),
        "Changed after review",
    )
    .unwrap();
    let result = apply(home, &prepared, || false).unwrap();
    assert_eq!(result["status"], "partial");
    assert_eq!(result["results"][0]["status"], "installed");
    assert_eq!(result["results"][1]["status"], "failed");
    assert_eq!(result["installed"].as_array().unwrap().len(), 1);
    let target = store::target(home, "studio/blender-skills", "modeling");
    assert!(target.join("SKILL.md").is_file());
    assert!(!store::target(home, "studio/blender-skills", "materials").exists());
    assert!(prepare(home, vec![selection("modeling")]).is_err());
    fs::write(target.join("user.txt"), "preserve").unwrap();
    let replay = apply(home, &prepared, || false).unwrap();
    assert_eq!(replay["status"], "failed");
    assert_eq!(
        fs::read_to_string(target.join("user.txt")).unwrap(),
        "preserve"
    );
}

#[test]
fn cancellation_before_or_between_installations_never_claims_the_remaining_packages() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    repository(home);
    let prepared = prepare(home, vec![selection("modeling"), selection("materials")]).unwrap();
    let result = apply(home, &prepared, || true).unwrap();
    assert!(result["installed"].as_array().unwrap().is_empty());
    assert_eq!(result["results"][0]["status"], "cancelled");
    let result = apply(home, &prepared, || {
        store::target(home, "studio/blender-skills", "modeling").exists()
    })
    .unwrap();
    assert_eq!(result["status"], "partial");
    assert_eq!(result["results"][1]["status"], "cancelled");
    assert!(!store::target(home, "studio/blender-skills", "materials").exists());
}

#[test]
fn preparation_checks_stop_before_fetching_each_package() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    repository(home);
    let calls = std::cell::Cell::new(0);
    let result = super::prepare(
        home,
        vec![selection("modeling"), selection("materials")],
        || {
            calls.set(calls.get() + 1);
            calls.get() == 2
        },
    );
    assert_eq!(result.err().unwrap().code, "cancelled");
    assert_eq!(calls.get(), 2);
    assert!(!store::target(home, "studio/blender-skills", "modeling").exists());
}

#[test]
fn batches_reject_empty_duplicate_unsafe_and_unexplained_selections_before_network_access() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    assert!(prepare(home, vec![]).is_err());
    assert!(prepare(home, vec![selection("modeling"); MAX_BATCH + 1]).is_err());
    assert!(prepare(home, vec![selection("modeling"), selection("modeling")]).is_err());
    assert!(prepare(
        home,
        vec![Selection {
            source: "../escape".into(),
            ..selection("modeling")
        }]
    )
    .is_err());
    assert!(prepare(
        home,
        vec![Selection {
            reason: " ".into(),
            ..selection("modeling")
        }]
    )
    .is_err());
    for (query, offset, limit) in [
        ("", 0, 10),
        ("blender\nexecute", 0, 10),
        ("blender", 0, 21),
        ("blender", 581, 10),
    ] {
        assert!(search(home, query, offset, limit).is_err());
    }
    assert!(!crate::data_dir::root(home).join("skills").exists());
}

#[test]
fn a_concurrent_install_cannot_be_replaced_by_an_old_proposal() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    repository(home);
    let prepared = prepare(home, vec![selection("modeling")]).unwrap();
    let target = store::target(home, "studio/blender-skills", "modeling");
    fs::create_dir_all(&target).unwrap();
    fs::write(target.join("SKILL.md"), "New user's version").unwrap();
    let result = apply(home, &prepared, || false).unwrap();
    assert_eq!(result["status"], "failed");
    assert_eq!(
        fs::read_to_string(target.join("SKILL.md")).unwrap(),
        "New user's version"
    );
}

#[test]
fn installation_receipts_use_canonical_ids_and_review_lists_every_installed_file() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join(".");
    let checkout = repository(&home);
    let package = checkout.join("modeling");
    fs::create_dir_all(package.join(".references/deep/a/b/c/d/e")).unwrap();
    fs::write(
        package.join(".references/deep/a/b/c/d/e/hidden.md"),
        "Hidden guidance",
    )
    .unwrap();
    for index in 0..105 {
        fs::write(package.join(format!("reference-{index}.md")), "Guidance").unwrap();
    }
    let prepared = prepare(&home, vec![selection("modeling")]).unwrap();
    assert_eq!(prepared.previews[0].files.len(), 108);
    assert!(prepared.previews[0]
        .files
        .iter()
        .any(|path| path == ".references/deep/a/b/c/d/e/hidden.md"));
    let inspected = inspect(&home, "studio/blender-skills", "modeling").unwrap();
    assert_eq!(inspected["fileCount"], 108);
    let result = apply(&home, &prepared, || false).unwrap();
    let (available, _) = catalog::discover(&home, None, &read_config(&home).unwrap()).unwrap();
    assert_eq!(result["installed"][0]["id"], available[0].id);
    assert_eq!(result["status"], "applied");
}

#[test]
fn marketplace_results_keep_real_ids_pagination_and_installed_status_without_inventing_descriptions(
) {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    repository(home);
    let prepared = prepare(home, vec![selection("modeling")]).unwrap();
    apply(home, &prepared, || false).unwrap();
    let raw = json!({"skills":[
        {"source":"studio/blender-skills","skillId":"modeling","name":"Blender modeling","installs":20},
        {"source":"studio/blender-skills","skillId":"materials","name":"Blender materials","installs":10},
        {"source":"another/repository","skillId":"rendering","name":"Render a scene","installs":2}
    ]}).to_string();
    marketplace::cache_search("blender-pagination-fixture", 24, &raw);
    let result = search(home, "blender-pagination-fixture", 0, 2).unwrap();
    assert_eq!(result["skills"][0]["source"], "studio/blender-skills");
    assert_eq!(result["skills"][0]["skillId"], "modeling");
    assert_eq!(result["skills"][0]["installed"], true);
    assert_eq!(result["skills"][1]["installed"], false);
    assert!(result["skills"][0].get("description").is_none());
    assert_eq!(result["nextOffset"], 2);
    let last = search(home, "blender-pagination-fixture", 2, 2).unwrap();
    assert_eq!(last["skills"].as_array().unwrap().len(), 1);
    assert!(last["nextOffset"].is_null());
}

#[test]
#[ignore = "Downloads a real marketplace package into an isolated temporary profile"]
fn live_marketplace_skill_can_be_inspected_installed_and_discovered() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    let result = search(home, "find-skills", 0, 10).unwrap();
    assert!(result["skills"]
        .as_array()
        .unwrap()
        .iter()
        .any(|s| s["source"] == "vercel-labs/skills" && s["skillId"] == "find-skills"));
    let detail = inspect(home, "vercel-labs/skills", "find-skills").unwrap();
    assert!(!detail["description"].as_str().unwrap().trim().is_empty());
    let prepared = prepare(
        home,
        vec![Selection {
            source: "vercel-labs/skills".into(),
            skill_id: "find-skills".into(),
            reason: "Exercise the native installer in an isolated test profile.".into(),
        }],
    )
    .unwrap();
    let installed = apply(home, &prepared, || false).unwrap();
    assert_eq!(installed["status"], "applied");
    let (available, _) = catalog::discover(home, None, &read_config(home).unwrap()).unwrap();
    assert_eq!(available.len(), 1);
    assert_eq!(available[0].name, "find-skills");
    eprintln!("Native marketplace search, inspection, pinned installation and discovery passed in an isolated profile.");
}
