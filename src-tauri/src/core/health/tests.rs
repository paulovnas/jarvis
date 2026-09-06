use super::*;

fn fixture(home: &Path) -> Manifest {
    let mut manifest = Manifest::default();
    for id in ComponentId::ALL {
        let directory = format!("{}/original", id.key());
        let path = root(home).join(&directory);
        fs::create_dir_all(&path).unwrap();
        fs::write(path.join("verified"), "preserve").unwrap();
        if id == ComponentId::Ponytail {
            ponytail::tests::fixture_package(&path, "1.0.0");
        }
        if id == ComponentId::OpenDesign {
            design::tests::prepare_fixture(&path, &[]).unwrap();
        }
        let record = Installation {
            version: if id == ComponentId::OpenDesign {
                "1.2.3"
            } else {
                "1.0.0"
            }
            .into(),
            directory,
            files: vec!["verified".into()],
        };
        save_receipt(home, id, &record).unwrap();
        manifest.installations.insert(id, record);
    }
    save_manifest(home, &manifest).unwrap();
    fs::write(
        root(home).join("context7.json"),
        r#"{"credential_ref":"jarvis-core-context7-fixture"}"#,
    )
    .unwrap();
    manifest
}

#[test]
fn update_network_errors_do_not_block_but_failed_local_checks_do() {
    let home = tempfile::tempdir().unwrap();
    fixture(home.path());
    let core = CoreState::default();
    core.data
        .lock()
        .unwrap()
        .errors
        .insert(ComponentId::Beads, "GitHub offline".into());
    assert!(core.snapshot(home.path()).unwrap().ready);
    assert!(core.require_ready(home.path()).is_ok());
    core.data.lock().unwrap().diagnostics.insert(
        ComponentId::Beads,
        vec![Check::result(
            "Execução local",
            Err(error("Não inicia")),
            "",
        )],
    );
    assert!(!core.snapshot(home.path()).unwrap().ready);
    assert!(core.require_ready(home.path()).is_err());
    core.data.lock().unwrap().diagnostics.insert(
        ComponentId::Beads,
        vec![Check::result("Execução local", Ok(()), "Pronto")],
    );
    assert!(core.require_ready(home.path()).is_ok());
}

#[test]
fn recovers_damaged_manifest_from_receipts_without_erasing_data() {
    let home = tempfile::tempdir().unwrap();
    fixture(home.path());
    let config = fs::read(root(home.path()).join("context7.json")).unwrap();
    let journal = home.path().join(".jarvis/conversations");
    fs::create_dir_all(&journal).unwrap();
    fs::write(journal.join("history.jsonl"), "user data").unwrap();
    fs::write(root(home.path()).join("manifest.json"), "{broken").unwrap();
    repair_local(home.path(), ComponentId::Ponytail).unwrap();
    assert!(require_ready(home.path()).is_ok());
    assert_eq!(
        fs::read(root(home.path()).join("context7.json")).unwrap(),
        config
    );
    assert_eq!(
        fs::read_to_string(journal.join("history.jsonl")).unwrap(),
        "user data"
    );
    let backups: Vec<_> = fs::read_dir(root(home.path()))
        .unwrap()
        .flatten()
        .filter(|e| {
            e.file_name()
                .to_string_lossy()
                .starts_with("manifest-damaged-")
        })
        .collect();
    assert_eq!(backups.len(), 1);
    assert_eq!(fs::read_to_string(backups[0].path()).unwrap(), "{broken");
}

#[test]
fn retire_only_deletes_the_replaced_generation_after_valid_publication() {
    let home = tempfile::tempdir().unwrap();
    let mut manifest = fixture(home.path());
    let old = manifest.installations[&ComponentId::Context7].clone();
    assert!(retire_previous(home.path(), ComponentId::Context7, &old).is_err());
    let mut new = old.clone();
    new.directory = "context7/replacement".into();
    fs::create_dir_all(root(home.path()).join(&new.directory)).unwrap();
    manifest
        .installations
        .insert(ComponentId::Context7, new.clone());
    save_manifest(home.path(), &manifest).unwrap();
    assert!(retire_previous(home.path(), ComponentId::Context7, &old).is_err());
    fs::write(
        root(home.path()).join(&new.directory).join("verified"),
        "new",
    )
    .unwrap();
    retire_previous(home.path(), ComponentId::Context7, &old).unwrap();
    assert!(!root(home.path()).join(&old.directory).exists());
    assert!(installed(home.path(), ComponentId::Context7).is_ok());
    assert!(installed(home.path(), ComponentId::Ponytail).is_ok());
    assert!(context7::configured(home.path()));
}

#[test]
fn recovers_a_missing_registry_using_the_highest_verified_version() {
    let home = tempfile::tempdir().unwrap();
    let manifest = fixture(home.path());
    for version in ["9.0.0", "10.0.0"] {
        let mut record = manifest.installations[&ComponentId::Context7].clone();
        record.directory = format!("context7/{version}");
        record.version = version.into();
        let path = root(home.path()).join(&record.directory);
        fs::create_dir_all(&path).unwrap();
        fs::write(path.join("verified"), "ok").unwrap();
        save_receipt(home.path(), ComponentId::Context7, &record).unwrap();
    }
    fs::remove_file(root(home.path()).join("manifest.json")).unwrap();
    recover_manifest(home.path()).unwrap();
    assert_eq!(
        installed(home.path(), ComponentId::Context7)
            .unwrap()
            .version,
        "10.0.0"
    );
    assert!(require_ready(home.path()).is_ok());
}

#[cfg(unix)]
#[tokio::test]
async fn detects_and_repairs_executable_permissions() {
    use std::os::unix::fs::PermissionsExt;
    let home = tempfile::tempdir().unwrap();
    let manifest = fixture(home.path());
    let path = root(home.path()).join("beads/original");
    fs::create_dir_all(path.join("dolt/bin")).unwrap();
    for file in [path.join("bd"), path.join("dolt/bin/dolt")] {
        fs::write(&file, "#!/bin/sh\nprintf 'version 1.0.0\\n'\n").unwrap();
        fs::set_permissions(file, fs::Permissions::from_mode(0o600)).unwrap();
    }
    assert!(runtime(
        home.path(),
        ComponentId::Beads,
        &manifest.installations[&ComponentId::Beads]
    )
    .await
    .is_err());
    repair_local(home.path(), ComponentId::Beads).unwrap();
    assert!(inspect(home.path(), ComponentId::Beads)
        .await
        .iter()
        .all(|c| c.passed));
}

#[cfg(unix)]
#[test]
fn repair_refuses_redirected_component_directories_and_preserves_external_files() {
    let home = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let mut manifest = fixture(home.path());
    fs::write(outside.path().join("important"), "untouched").unwrap();
    std::os::unix::fs::symlink(outside.path(), root(home.path()).join("context7/redirect"))
        .unwrap();
    let record = manifest
        .installations
        .get_mut(&ComponentId::Context7)
        .unwrap();
    record.directory = "context7/redirect".into();
    record.files = vec!["important".into()];
    save_manifest(home.path(), &manifest).unwrap();
    assert!(repair_local(home.path(), ComponentId::Context7).is_err());
    assert_eq!(
        fs::read_to_string(outside.path().join("important")).unwrap(),
        "untouched"
    );
}
