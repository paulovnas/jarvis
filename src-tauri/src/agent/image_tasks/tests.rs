use super::*;
fn fixture(staging: &Path, width: u32, height: u32) {
    let mut bytes = std::io::Cursor::new(vec![]);
    image::DynamicImage::new_rgba8(width, height)
        .write_to(&mut bytes, image::ImageFormat::Png)
        .unwrap();
    fs::write(staging.join("image-01.png"), bytes.into_inner()).unwrap();
    fs::write(
        staging.join("workflow.json"),
        b"{\"1\":{\"class_type\":\"LoadImage\"}}",
    )
    .unwrap();
    fs::write(staging.join("generation-report.json"), json!({"engine":"comfyui","version":"0.3.8","operations":["export"],"images":[{"path":"image-01.png","width":width,"height":height}]}).to_string()).unwrap();
}
fn request() -> Request {
    Request {
        image_ids: vec!["a".repeat(32)],
        processing: default_processing(),
    }
}
#[test]
fn closed_processing_contract_rejects_models_workflows_and_invalid_alpha_exports() {
    let catalog = super::super::tool_contract::Catalog::new(&definitions());
    let valid = call(
        "image_process",
        json!({"image_ids":["a".repeat(32)],"processing":{"width":128,"format":"webp","remove_background":true}}),
    );
    assert!(catalog.validate(&valid).is_ok());
    assert_eq!(
        super::super::tool_contract::Orchestrator::new(&definitions())
            .preflight(&valid)
            .unwrap()
            .handler,
        super::super::tool_contract::Handler::Native
    );
    for processing in [
        json!({"width":4097}),
        json!({"height":0}),
        json!({"workflow":"custom.py"}),
        json!({"format":"gif"}),
    ] {
        assert!(catalog
            .validate(&call(
                "image_process",
                json!({"image_ids":["a".repeat(32)],"processing":processing})
            ))
            .is_err());
    }
    let transparent: Processing =
        serde_json::from_value(json!({"format":"jpg","remove_background":true})).unwrap();
    assert!(transparent.validate().is_err());
    assert!(serde_json::from_value::<Processing>(json!({"model":"arbitrary"})).is_err());
}
#[test]
fn owned_input_scope_is_checked_before_processing() {
    let home = tempfile::tempdir().unwrap();
    let staging = tempfile::tempdir().unwrap();
    fixture(staging.path(), 8, 6);
    let bytes = fs::read(staging.path().join("image-01.png")).unwrap();
    let owner = "a".repeat(32);
    let image = attachments::store(home.path(), &owner, "input.png", &bytes).unwrap();
    assert_eq!(
        inputs(home.path(), &owner, std::slice::from_ref(&image.id))
            .unwrap()
            .len(),
        1
    );
    assert!(inputs(home.path(), &"b".repeat(32), &[image.id]).is_err());
    assert!(inputs(home.path(), &owner, &[]).is_err());
}
#[test]
fn final_preview_and_unique_exports_preserve_sources_and_record_workflow() {
    let home = tempfile::tempdir().unwrap();
    let project = tempfile::tempdir().unwrap();
    let staging = tempfile::tempdir().unwrap();
    fixture(staging.path(), 8, 6);
    let root = project.path().canonicalize().unwrap();
    let mut request = request();
    request.processing.output_directory = Some("assets/images".into());
    let first: Value = serde_json::from_str(
        &publish(
            home.path(),
            &"a".repeat(32),
            &root,
            staging.path(),
            &request,
        )
        .unwrap(),
    )
    .unwrap();
    let second: Value = serde_json::from_str(
        &publish(
            home.path(),
            &"a".repeat(32),
            &root,
            staging.path(),
            &request,
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(first["kind"], "generated_image");
    assert_eq!(first["accountAlias"], "local");
    assert_ne!(first["exports"], second["exports"]);
    let path = root.join(first["exports"][0].as_str().unwrap());
    assert_eq!(
        fs::read(&path).unwrap(),
        fs::read(staging.path().join("image-01.png")).unwrap()
    );
    let source = Path::new(first["sourcePaths"][0].as_str().unwrap());
    assert!(source.parent().unwrap().join("workflow.json").is_file());
    assert!(source
        .parent()
        .unwrap()
        .join("generation-report.json")
        .is_file());
    assert_eq!(first["sourceImageIds"], json!(request.image_ids));
}
#[test]
fn untrusted_output_paths_dimensions_and_formats_never_publish() {
    let home = tempfile::tempdir().unwrap();
    let project = tempfile::tempdir().unwrap();
    let staging = tempfile::tempdir().unwrap();
    let root = project.path().canonicalize().unwrap();
    fixture(staging.path(), 8, 6);
    fs::write(
        staging.path().join("generation-report.json"),
        json!({"images":[{"path":"../escape.png","width":8,"height":6}]}).to_string(),
    )
    .unwrap();
    assert!(publish(
        home.path(),
        &"a".repeat(32),
        &root,
        staging.path(),
        &request()
    )
    .is_err());
    fixture(staging.path(), 8, 6);
    fs::write(
        staging.path().join("generation-report.json"),
        json!({"images":[{"path":"image-01.png","width":99,"height":6}]}).to_string(),
    )
    .unwrap();
    assert!(publish(
        home.path(),
        &"a".repeat(32),
        &root,
        staging.path(),
        &request()
    )
    .is_err());
    fixture(staging.path(), 8, 6);
    let mut bad = request();
    bad.processing.output_directory = Some("../outside".into());
    assert!(publish(home.path(), &"a".repeat(32), &root, staging.path(), &bad).is_err());
}
#[test]
fn failed_processing_returns_retry_ids_without_regenerating_images() {
    let ids = vec!["a".repeat(32)];
    let cause = preserve(error("Processing failed"), &ids);
    let result: Value = serde_json::from_str(cause.tool_result.as_deref().unwrap()).unwrap();
    assert_eq!(result["sourceImageIds"], json!(ids));
    assert!(result["recovery"]
        .as_str()
        .unwrap()
        .contains("image_process"));
}
#[test]
fn existing_file_cannot_be_an_export_directory_and_partial_receipts_survive() {
    let project = tempfile::tempdir().unwrap();
    let root = project.path().canonicalize().unwrap();
    fs::write(root.join("README.md"), "original").unwrap();
    assert!(validate_export(&root, Some("README.md")).is_err());
    assert_eq!(
        fs::read_to_string(root.join("README.md")).unwrap(),
        "original"
    );
    let mut cause = error("export failed");
    cause.tool_result = Some(
        json!({"producedImageIds":["b".repeat(32)],"exports":["assets/result.png"]}).to_string(),
    );
    let cause = preserve(cause, &["a".repeat(32)]);
    let result: Value = serde_json::from_str(cause.tool_result.as_deref().unwrap()).unwrap();
    assert_eq!(result["producedImageIds"], json!(["b".repeat(32)]));
    assert_eq!(result["exports"], json!(["assets/result.png"]));
}
#[test]
fn whole_batch_is_validated_before_publishing_first_image() {
    let home = tempfile::tempdir().unwrap();
    let project = tempfile::tempdir().unwrap();
    let staging = tempfile::tempdir().unwrap();
    fixture(staging.path(), 8, 6);
    let owner = "a".repeat(32);
    let mut request = request();
    request.image_ids.push("b".repeat(32));
    fs::write(staging.path().join("generation-report.json"), json!({"images":[{"path":"image-01.png","width":8,"height":6},{"path":"../invalid.png","width":8,"height":6}]}).to_string()).unwrap();
    assert!(publish(
        home.path(),
        &owner,
        &project.path().canonicalize().unwrap(),
        staging.path(),
        &request
    )
    .is_err());
    assert!(!attachments::directory(home.path(), &owner)
        .unwrap()
        .exists());
}
#[cfg(unix)]
#[test]
fn symlink_outputs_are_rejected() {
    let home = tempfile::tempdir().unwrap();
    let project = tempfile::tempdir().unwrap();
    let staging = tempfile::tempdir().unwrap();
    fixture(staging.path(), 8, 6);
    fs::remove_file(staging.path().join("image-01.png")).unwrap();
    std::os::unix::fs::symlink(
        project.path().join("file.png"),
        staging.path().join("image-01.png"),
    )
    .unwrap();
    assert!(publish(
        home.path(),
        &"a".repeat(32),
        &project.path().canonicalize().unwrap(),
        staging.path(),
        &request()
    )
    .is_err());
}

#[tokio::test]
#[ignore = "Installs private ComfyUI and runs real offline graphs through native sandboxed tools"]
async fn native_comfyui_processing_pipeline() {
    let task_home = std::env::var_os("JARVIS_TEST_COMFYUI_HOME")
        .expect("Set JARVIS_TEST_COMFYUI_HOME to an isolated test home");
    let home = Path::new(&task_home);
    assert!(home.is_absolute());
    fs::create_dir_all(home).unwrap();
    if crate::core::comfyui::runtime(home).is_err() {
        crate::core::audiovisual::tests::install_smoke_component(
            home,
            crate::core::ComponentId::Comfyui,
        )
        .await;
    }
    let runtime = crate::core::comfyui::runtime(home).unwrap();
    assert_eq!(
        fs::read_to_string(&runtime.entry).unwrap(),
        include_str!("../../core/comfyui/runner.py")
    );
    let fixture = super::super::tests::Fixture::new();
    let session = super::super::tests::session_with_id(&fixture, &"a".repeat(32));
    let mut buffer = std::io::Cursor::new(vec![]);
    let mut image = image::RgbaImage::from_pixel(64, 64, image::Rgba([240, 240, 240, 255]));
    for y in 16..48 {
        for x in 20..44 {
            image.put_pixel(x, y, image::Rgba([20, 70, 210, 255]));
        }
    }
    image::DynamicImage::ImageRgba8(image)
        .write_to(&mut buffer, image::ImageFormat::Png)
        .unwrap();
    let source = attachments::store(home, &session.id, "source.png", &buffer.into_inner()).unwrap();
    let mut commands = CommandSessions::default();
    let (_sender, signal) = watch::channel(false);
    for (format, remove_background) in [("png", false), ("webp", true), ("jpg", false)] {
        let raw = execute(&AppState::default(), &OpenAiCodexState::default(), home, &session.id, &session, &mut commands, None,
            &json!({"image_ids":[source.id],"processing":{"width":128,"height":72,"format":format,"upscale":true,"remove_background":remove_background,"output_directory":"generated-images"}}), signal.clone()).await.unwrap();
        let result: Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(result["kind"], "generated_image");
        assert_eq!(result["images"].as_array().unwrap().len(), 1);
        let output = Path::new(result["sourcePaths"][0].as_str().unwrap());
        let decoded = image::load_from_memory(&fs::read(output).unwrap()).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (128, 72));
        if remove_background {
            let pixels = decoded.to_rgba8();
            assert!(pixels.pixels().any(|pixel| pixel.0[3] < 200));
            assert!(pixels.pixels().any(|pixel| pixel.0[3] > 200));
        }
        let export = fixture.root.join(result["exports"][0].as_str().unwrap());
        assert!(export.is_file());
        assert_eq!(fs::read(&export).unwrap(), fs::read(output).unwrap());
        assert!(output.parent().unwrap().join("workflow.json").is_file());
        eprintln!("Native ComfyUI {format}: {}", result["processing"]);
    }
    assert!(commands.running_ids().is_empty());
    assert!(attachments::metadata(home, &session.id, &source.id).is_ok());
}
