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
        if id == ComponentId::Impeccable {
            design::tests::prepare_fixture(&path, &[]).unwrap();
        }
        if id == ComponentId::Hyperframes {
            hyperframes::tests::fixture(&path, "1.0.0");
        }
        if id == ComponentId::Audiovisual {
            audiovisual::tests::fixture(&path);
        }
        if id == ComponentId::Comfyui {
            comfyui::tests::fixture(&path);
        }
        if id == ComponentId::Graft {
            graft::fixture(&path);
        }
        if id == ComponentId::Openmontage {
            openmontage::tests::fixture(&path);
        }
        let record = Installation {
            version: if id == ComponentId::Impeccable {
                "1.2.3"
            } else if id == ComponentId::Graft {
                install::GRAFT_VERSION
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

#[tokio::test]
async fn diagnosis_reports_every_component_without_revalidating_all_packs_for_progress() {
    let home = tempfile::tempdir().unwrap();
    let core = CoreState::default();
    let mut stages = Vec::new();
    let snapshot = diagnose_components(&core, home.path(), |snapshot| {
        for item in &snapshot.items {
            if item.stage.is_some() {
                stages.push(item.id);
            }
        }
    })
    .await
    .unwrap();
    assert_eq!(stages, ComponentId::ALL);
    assert_eq!(
        core.snapshot_reads
            .load(std::sync::atomic::Ordering::Relaxed),
        2
    );
    assert!(!snapshot.ready);
    assert!(snapshot.items.iter().all(|item| {
        item.stage.is_none() && !item.diagnostics.is_empty() && item.health_error.is_some()
    }));
    assert!(core.data.lock().unwrap().stages.is_empty());
}

#[tokio::test]
async fn diagnosis_progress_preserves_concurrent_update_discovery() {
    let home = tempfile::tempdir().unwrap();
    let core = CoreState::default();
    core.data.lock().unwrap().checking = true;
    let mut initial = true;
    let mut progress = 0;
    diagnose_components(&core, home.path(), |snapshot| {
        if initial {
            assert!(snapshot.checking);
            let mut data = core.data.lock().unwrap();
            data.checking = false;
            data.latest.insert(ComponentId::Ponytail, "5.1.0".into());
            initial = false;
        } else {
            assert!(!snapshot.checking);
            assert_eq!(
                snapshot
                    .items
                    .iter()
                    .find(|item| item.id == ComponentId::Ponytail)
                    .unwrap()
                    .latest_version
                    .as_deref(),
                Some("5.1.0")
            );
            progress += 1;
        }
    })
    .await
    .unwrap();
    assert_eq!(progress, ComponentId::ALL.len() * 2 + 1);
}

#[test]
fn diagnostic_progress_keeps_failures_visible_and_final_status_rechecks_integrity() {
    let home = tempfile::tempdir().unwrap();
    let manifest = fixture(home.path());
    let core = CoreState::default();
    let mut snapshot = core.snapshot(home.path()).unwrap();
    assert!(snapshot.ready);
    diagnostic_result(
        &mut snapshot,
        ComponentId::Beads,
        vec![
            Check::result("Arquivos e recursos", Ok(()), "Pronto"),
            Check::result("Execução local", Err(error("Não inicia")), ""),
        ],
    );
    assert!(!snapshot.ready);
    let beads = snapshot
        .items
        .iter()
        .find(|item| item.id == ComponentId::Beads)
        .unwrap();
    assert!(beads.installed);
    assert_eq!(beads.health_error.as_deref(), Some("Não inicia"));
    assert!(beads.stage.is_none());

    // No persistent validity cache: the next complete status detects changed installation files.
    let path = root(home.path())
        .join(&manifest.installations[&ComponentId::Ponytail].directory)
        .join("verified");
    fs::remove_file(path).unwrap();
    diagnostic_result(
        &mut snapshot,
        ComponentId::Beads,
        vec![
            Check::result("Arquivos e recursos", Ok(()), "Pronto"),
            Check::result("Execução local", Ok(()), "Pronto"),
        ],
    );
    assert!(snapshot.ready);
    let final_status = core.snapshot(home.path()).unwrap();
    assert!(!final_status.ready);
    assert!(final_status
        .items
        .iter()
        .any(|item| item.id == ComponentId::Ponytail && !item.installed));
    assert!(core.require_ready(home.path()).is_err());
}

#[test]
fn recovers_damaged_manifest_from_receipts_without_erasing_data() {
    let home = tempfile::tempdir().unwrap();
    fixture(home.path());
    let config = fs::read(root(home.path()).join("context7.json")).unwrap();
    let journal = crate::data_dir::root(home.path()).join("conversations");
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
