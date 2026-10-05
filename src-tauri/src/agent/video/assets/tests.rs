use super::*;

fn home() -> tempfile::TempDir {
    let home = tempfile::tempdir().unwrap();
    crate::core::brag::tests::fixture_home(home.path());
    home
}

fn project() -> tempfile::TempDir {
    tempfile::Builder::new()
        .tempdir_in(fs::canonicalize(std::env::temp_dir()).unwrap())
        .unwrap()
}

fn asset(home: &Path, category: &str) -> Value {
    list(home, &json!({"category":category})).unwrap()["assets"][0].clone()
}

#[test]
fn listing_is_bounded_filtered_and_includes_verified_music_credits() {
    let home = home();
    let first = list(home.path(), &json!({})).unwrap();
    assert_eq!(first["assets"].as_array().unwrap().len(), PAGE_SIZE);
    assert_eq!(first["total"], 265);
    assert_eq!(first["nextOffset"], PAGE_SIZE);
    let second = list(home.path(), &json!({"offset":first["nextOffset"]})).unwrap();
    assert_ne!(first["assets"][0]["path"], second["assets"][0]["path"]);
    let music = list(home.path(), &json!({"category":"music"})).unwrap();
    assert_eq!(music["total"], 5);
    assert!(music["nextOffset"].is_null());
    for track in music["assets"].as_array().unwrap() {
        assert_eq!(track["license"], "CC-BY-4.0");
        assert!(track["attribution"]
            .as_str()
            .unwrap()
            .contains("Sascha Ende"));
        assert!(track["source"]
            .as_str()
            .unwrap()
            .starts_with("https://ende.app/"));
    }
    assert!(list(home.path(), &json!({"category":"unknown"})).is_err());
    let end = list(home.path(), &json!({"offset":u64::MAX})).unwrap();
    assert!(end["assets"].as_array().unwrap().is_empty());
    assert!(end["nextOffset"].is_null());
}

#[test]
fn import_preserves_original_audio_copies_credits_and_never_overwrites() {
    let home = home();
    let root = project();
    let selected = asset(home.path(), "keyboard");
    let args = json!({"asset":selected["path"],"output":"video/assets/keyboard.wav"});
    let (command, result) = import(root.path(), home.path(), &args, None).unwrap();
    assert!(command.is_none());
    let result = result.unwrap();
    assert_eq!(result["status"], "completed");
    assert_eq!(result["asset"]["license"], "CC0-1.0");
    let output = root.path().join("video/assets/keyboard.wav");
    let bytes = fs::read(&output).unwrap();
    assert_eq!(format!("{:x}", Sha256::digest(&bytes)), selected["sha256"]);
    let credit = root.path().join(result["creditsPath"].as_str().unwrap());
    assert!(fs::read_to_string(&credit).unwrap().contains("CC0-1.0"));
    assert!(import(root.path(), home.path(), &args, None).is_err());
    assert_eq!(fs::read(output).unwrap(), bytes);
}

#[test]
fn resource_import_rejects_unknown_assets_scope_escape_and_wrong_extensions() {
    let home = home();
    let root = project();
    let selected = asset(home.path(), "keyboard");
    for args in [
        json!({"asset":"../../secret.wav","output":"video/secret.wav"}),
        json!({"asset":selected["path"],"output":"../escape.wav"}),
        json!({"asset":selected["path"],"output":"video/not-audio.exe"}),
    ] {
        assert!(
            import(root.path(), home.path(), &args, None).is_err(),
            "{args}"
        );
    }
    assert!(!root.path().join("video/not-audio.exe").exists());
    fs::create_dir_all(root.path().join("video")).unwrap();
    fs::write(
        root.path().join("video/clip.wav.credits.md"),
        "user credits",
    )
    .unwrap();
    assert!(import(
        root.path(),
        home.path(),
        &json!({"asset":selected["path"],"output":"video/clip.wav"}),
        None
    )
    .is_err());
    assert_eq!(
        fs::read_to_string(root.path().join("video/clip.wav.credits.md")).unwrap(),
        "user credits"
    );
    assert!(!root.path().join("video/clip.wav").exists());
}

#[test]
fn music_conversion_uses_managed_ffmpeg_and_publishes_pcm_only_after_success() {
    let home = home();
    let root = project();
    let selected = asset(home.path(), "music");
    let (command, result) = import(
        root.path(),
        home.path(),
        &json!({"asset":selected["path"],"output":"brag/assets/music.wav"}),
        None,
    )
    .unwrap();
    assert!(result.is_none());
    let command = command.unwrap();
    let argv: Vec<_> = command
        .process
        .as_std()
        .get_args()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect();
    assert!(argv.iter().any(|arg| arg == "pcm_s16le"));
    assert!(argv.iter().any(|arg| arg == "48000"));
    assert!(command
        .process
        .as_std()
        .get_program()
        .to_string_lossy()
        .contains("ffmpeg"));
    assert!(!root.path().join("brag/assets/music.wav").exists());
    audio::tests::write_wave(Path::new(argv.last().unwrap()), 1);
    (command.on_success.unwrap())().unwrap();
    let wave = audio::wave(&root.path().join("brag/assets/music.wav")).unwrap();
    assert!(wave.duration > 0.0);
    let credit = fs::read_to_string(root.path().join("brag/assets/music.wav.credits.md")).unwrap();
    assert!(credit.contains("Sascha Ende"));
    assert!(credit.contains("CC-BY-4.0"));
    assert!(credit.contains("Decoded to PCM16 WAV"));
}

#[test]
fn plan_catalog_allows_discovery_but_cannot_import_resources() {
    let catalog = super::super::super::tool_contract::Catalog::new(&definitions(Mode::Plan));
    assert!(catalog.capabilities("video_brag_assets").is_some());
    assert!(catalog.capabilities("video_brag_asset").is_none());
    assert!(!super::super::mutating("video_brag_assets"));
    assert!(super::super::mutating("video_brag_asset"));
}

#[test]
fn brag_documentation_is_available_to_existing_core_and_paged_safely() {
    let home = home();
    let aliased_home = home.path().join(".");
    let initial: Value =
        serde_json::from_str(&super::super::docs(&aliased_home, &json!({"topic":"brag"})).unwrap())
            .unwrap();
    assert!(initial["integration"]
        .as_str()
        .unwrap()
        .contains("never brag-slim"));
    assert!(initial["content"].as_str().unwrap().contains("Jarvis"));
    let reference: Value = serde_json::from_str(
        &super::super::docs(
            home.path(),
            &json!({"topic":"brag","file":"references/step-3-compose.md"}),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(
        reference["content"].as_str().unwrap().chars().count(),
        12_000
    );
    let next: Value = serde_json::from_str(&super::super::docs(home.path(), &json!({"topic":"brag","file":"references/step-3-compose.md","offset":reference["nextOffset"]})).unwrap()).unwrap();
    assert!(!next["content"].as_str().unwrap().is_empty());
    for domain in ["animation", "creative", "keyframes"] {
        let guidance: Value = serde_json::from_str(
            &super::super::docs(
                home.path(),
                &json!({"topic":"brag","file":format!("hyperframes/hyperframes-{domain}/SKILL.md")}),
            )
            .unwrap(),
        )
        .unwrap();
        assert!(!guidance["content"].as_str().unwrap().is_empty());
    }
    for file in ["../other.md", "/outside.md", "assets/music/not-doc.mp3"] {
        assert!(super::super::docs(home.path(), &json!({"topic":"brag","file":file})).is_err());
    }
}

#[test]
fn resource_import_resolves_managed_home_alias_without_changing_project_scope() {
    let home = home();
    let aliased_home = home.path().join(".");
    let root = project();
    let selected = asset(&aliased_home, "keyboard");
    let (_, result) = import(
        root.path(),
        &aliased_home,
        &json!({"asset":selected["path"],"output":"keyboard.wav"}),
        None,
    )
    .unwrap();
    assert_eq!(result.unwrap()["status"], "completed");
    assert!(root.path().join("keyboard.wav").is_file());
}
