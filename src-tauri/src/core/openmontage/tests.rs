use super::*;

pub(crate) fn fixture(generation: &Path) {
    fs::create_dir_all(generation).unwrap();
    hyperframes::tests::fixture(generation, HYPERFRAMES_VERSION);
    for file in SOURCE_FILES {
        let path = generation.join(REPOSITORY).join(file);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, format!("source fixture {file}")).unwrap();
    }
    for path in [
        python_path(generation),
        base_python(generation),
        generation.join("venv/pyvenv.cfg"),
        generation.join("python-requirements.lock.txt"),
        generation
            .join(REPOSITORY)
            .join("remotion-composer/package-lock.json"),
        generation
            .join(REPOSITORY)
            .join("remotion-composer/node_modules/@remotion/cli/package.json"),
    ] {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, "fixture").unwrap();
    }
    repair_bridge(generation).unwrap();
    aliases(generation).unwrap();
    let sources = SOURCE_FILES
        .iter()
        .map(|file| {
            (
                (*file).to_string(),
                format!(
                    "{:x}",
                    Sha256::digest(fs::read(generation.join(REPOSITORY).join(file)).unwrap())
                ),
            )
        })
        .collect();
    fs::write(
        generation.join(RECEIPT),
        serde_json::to_vec(&Receipt {
            version: VERSION.into(),
            revision: REVISION.into(),
            source_sha256: SOURCE_SHA256.into(),
            python: audiovisual::PYTHON_VERSION.into(),
            renderer: HYPERFRAMES_VERSION.into(),
            sources,
        })
        .unwrap(),
    )
    .unwrap();
}

#[tokio::test]
async fn python_archive_launches_regular_interpreter_without_symlink_aliases() {
    let generation = tempfile::tempdir().unwrap();
    let encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    let mut archive = tar::Builder::new(encoder);
    let payload = b"#!/bin/sh\nprintf 'standalone interpreter'\n";
    let mut header = tar::Header::new_gnu();
    header.set_size(payload.len() as u64);
    header.set_mode(0o755);
    header.set_cksum();
    archive
        .append_data(
            &mut header,
            if cfg!(windows) {
                "python/python.exe"
            } else {
                "python/bin/python3.11"
            },
            &payload[..],
        )
        .unwrap();
    #[cfg(unix)]
    {
        let mut header = tar::Header::new_gnu();
        header.set_entry_type(tar::EntryType::Symlink);
        header.set_size(0);
        header.set_mode(0o777);
        header.set_link_name("python3.11").unwrap();
        header.set_cksum();
        archive
            .append_data(&mut header, "python/bin/python3", std::io::empty())
            .unwrap();
    }
    let bytes = archive.into_inner().unwrap().finish().unwrap();
    install::unpack(bytes, &generation.path().join("python"), false, true).unwrap();
    assert!(base_python(generation.path()).is_file());
    #[cfg(unix)]
    {
        assert!(!generation.path().join("python/bin/python3").exists());
        let command = tokio::process::Command::new(base_python(generation.path()));
        assert_eq!(
            install::command(command, 5).await.unwrap(),
            "standalone interpreter"
        );
    }
}

#[test]
fn full_production_runtime_is_ready_without_cloud_credentials_or_optional_models() {
    let package = tempfile::tempdir().unwrap();
    fixture(package.path());
    validate(package.path(), VERSION).unwrap();
    let runtime = Runtime::at(package.path()).unwrap();
    assert_eq!(runtime.package, package.path().join(REPOSITORY));
    assert!(runtime.bridge.is_file());
    assert_eq!(runtime.environment["OPENAI_API_KEY"], "");
    assert_eq!(runtime.environment["JARVIS_OPENMONTAGE_ALLOW_PAID"], "0");
    assert_eq!(runtime.environment["HF_HUB_OFFLINE"], "1");
    assert!(runtime.environment["PATH"].contains(package.path().join("bin").to_str().unwrap()));
    assert!(!package.path().join("models").exists());
}

#[test]
fn optional_animation_keeps_main_python_and_uses_its_own_manim_runtime() {
    let package = tempfile::tempdir().unwrap();
    fixture(package.path());
    animation::tests::fixture(package.path());
    let animation = animation::runtime(package.path()).unwrap().unwrap();
    let runtime = Runtime::at(package.path()).unwrap();
    let paths: Vec<_> = std::env::split_paths(&runtime.environment["PATH"]).collect();
    assert_eq!(paths[0], python_path(package.path()).parent().unwrap());
    assert_eq!(paths[1], animation.bin);
    assert_eq!(
        runtime.environment["JARVIS_OPENMONTAGE_MANIM_PYTHON"],
        animation.python.to_string_lossy()
    );
    assert_eq!(
        runtime.environment["JARVIS_OPENMONTAGE_MANIM_PREFIX"],
        animation.prefix.to_string_lossy()
    );
    assert_eq!(
        runtime.environment["JARVIS_OPENMONTAGE_MANIM_MANAGER"],
        animation.manager.to_string_lossy()
    );
}

#[test]
fn damaged_source_or_bridge_is_detected_and_only_bridge_is_repaired() {
    let package = tempfile::tempdir().unwrap();
    fixture(package.path());
    fs::write(package.path().join(REPOSITORY).join(BRIDGE), "changed").unwrap();
    assert!(validate(package.path(), VERSION).is_err());
    repair_bridge(package.path()).unwrap();
    validate(package.path(), VERSION).unwrap();
    fs::write(
        package
            .path()
            .join(REPOSITORY)
            .join("tools/tool_registry.py"),
        "changed source",
    )
    .unwrap();
    assert!(validate(package.path(), VERSION).is_err());
    assert_eq!(
        fs::read_to_string(
            package
                .path()
                .join(REPOSITORY)
                .join("tools/tool_registry.py")
        )
        .unwrap(),
        "changed source"
    );
}

#[test]
fn generation_publish_relocates_python_without_referencing_removed_staging() {
    let package = tempfile::tempdir().unwrap();
    let from = package.path().join("staging");
    let to = package.path().join("published");
    fs::create_dir_all(from.join("venv/bin")).unwrap();
    fs::write(
        from.join("venv/pyvenv.cfg"),
        format!("home = {}/python/bin\n", from.display()),
    )
    .unwrap();
    #[cfg(unix)]
    fs::write(
        from.join("venv/bin/piper"),
        format!("#!{}/venv/bin/python\nprint('piper')\n", from.display()),
    )
    .unwrap();
    relocate_venv(&from, &to).unwrap();
    let config = fs::read_to_string(from.join("venv/pyvenv.cfg")).unwrap();
    assert!(config.contains(to.to_str().unwrap()));
    assert!(!config.contains(from.to_str().unwrap()));
    #[cfg(unix)]
    assert!(fs::read_to_string(from.join("venv/bin/piper"))
        .unwrap()
        .contains(to.to_str().unwrap()));
}

#[tokio::test]
async fn package_release_is_pinned_and_not_dependent_on_network_discovery() {
    assert_eq!(
        install::component_release(ComponentId::Openmontage)
            .await
            .unwrap()
            .version(),
        VERSION
    );
}

#[test]
fn legacy_manifest_records_remain_readable_and_are_not_required() {
    assert!(!ComponentId::ALL.contains(&ComponentId::Hyperframes));
    assert!(!ComponentId::ALL.contains(&ComponentId::Audiovisual));
    assert!(ComponentId::ALL.contains(&ComponentId::Openmontage));
    let record: super::super::Manifest = serde_json::from_str(r#"{"installations":{"hyperframes":{"version":"0.8.99","directory":"hyperframes/old","files":["file"]},"audiovisual":{"version":"1.0.0","directory":"audiovisual/old","files":["file"]}}}"#).unwrap();
    assert_eq!(record.installations.len(), 2);
}

#[tokio::test]
#[ignore = "Downloads the complete pinned production package and managed runtimes"]
async fn official_openmontage_install() {
    let home = tempfile::tempdir().unwrap();
    install::install(
        home.path(),
        ComponentId::Openmontage,
        |stage| eprintln!("{stage}"),
        |_| {},
    )
    .await
    .unwrap();
    let runtime = runtime(home.path()).unwrap();
    let request = tempfile::NamedTempFile::new_in(home.path()).unwrap();
    let mut offset = 0;
    let mut names = std::collections::BTreeSet::new();
    loop {
        fs::write(
            request.path(),
            serde_json::to_vec(&serde_json::json!({
                "action":"tools","package":runtime.package,"root":home.path(),
                "directory":home.path(),"nativeApproved":false,"arguments":{},"offset":offset
            }))
            .unwrap(),
        )
        .unwrap();
        let mut command = tokio::process::Command::new(&runtime.python);
        command
            .arg(&runtime.bridge)
            .arg("--request")
            .arg(request.path())
            .env_clear()
            .envs(&runtime.environment)
            .current_dir(&runtime.package);
        let result = install::command(command, 90).await.unwrap();
        let value: serde_json::Value = result
            .lines()
            .rev()
            .find_map(|line| serde_json::from_str(line).ok())
            .unwrap();
        assert_eq!(value["success"], true, "{value}");
        for tool in value["tools"].as_array().unwrap() {
            names.insert(tool["name"].as_str().unwrap().to_string());
        }
        let Some(next) = value["nextOffset"].as_u64() else {
            assert_eq!(names.len() as u64, value["total"].as_u64().unwrap());
            break;
        };
        assert!(next > offset);
        offset = next;
    }
    for name in [
        "video_compose",
        "hyperframes_compose",
        "piper_tts",
        "suno_music",
    ] {
        assert!(
            names.contains(name),
            "Missing upstream production tool: {name}"
        );
    }
    super::board::tests::smoke(home.path()).await;
    crate::agent::video::tests::smoke_render(home.path()).await;
}
