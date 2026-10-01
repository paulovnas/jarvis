use super::*;
use std::io::Write;

#[test]
fn project_images_become_owned_generation_references_without_changing_the_source() {
    let home = tempfile::tempdir().unwrap();
    let project = tempfile::tempdir().unwrap();
    let root = project.path().canonicalize().unwrap();
    let conversation = "a".repeat(32);
    let mut png = Cursor::new(Vec::new());
    image::DynamicImage::new_rgb8(8, 4)
        .write_to(&mut png, image::ImageFormat::Png)
        .unwrap();
    fs::create_dir(root.join("assets")).unwrap();
    fs::write(root.join("assets/logo.png"), png.get_ref()).unwrap();
    let args = json!({"path":"assets/logo.png","id":null,"offset":null,"limit":null});
    assert!(
        jsonschema::validator_for(&project_definition()["parameters"])
            .unwrap()
            .is_valid(&args)
    );
    assert!(!jsonschema::validator_for(&definition()["parameters"])
        .unwrap()
        .is_valid(&args));
    let output = read_tool_with_project(home.path(), &conversation, Some(&root), &args).unwrap();
    let value: Value = serde_json::from_str(&output).unwrap();
    let id = value["image_ids"][0].as_str().unwrap();
    let item = metadata(home.path(), &conversation, id).unwrap();
    assert_eq!(item.kind, "image");
    assert_eq!(item.name, "logo.png");
    assert_eq!(value["attachment"]["id"], id);
    assert_eq!(value["path"], "assets/logo.png");
    assert!(!output.contains("base64"));
    assert_eq!(
        bounded_read(
            &location(home.path(), &conversation, id)
                .unwrap()
                .join("source"),
            MAX_BYTES
        )
        .unwrap(),
        *png.get_ref()
    );
    assert_eq!(
        fs::read(root.join("assets/logo.png")).unwrap(),
        *png.get_ref()
    );
    assert!(metadata(home.path(), &"b".repeat(32), id).is_err());
    assert!(super::super::vision::input(
        home.path(),
        &conversation,
        &json!({"ids":[id],"question":"Identify this logo."})
    )
    .is_ok());
    assert!(definition()["parameters"]["properties"]
        .get("path")
        .is_none());
    assert!(project_definition()["parameters"]["properties"]
        .get("path")
        .is_some());
}

#[test]
fn project_image_reference_rejects_escapes_ambiguous_requests_and_unsupported_or_oversize_files() {
    let home = tempfile::tempdir().unwrap();
    let project = tempfile::tempdir().unwrap();
    let root = project.path().canonicalize().unwrap();
    let conversation = "a".repeat(32);
    fs::write(root.join("unsupported.bin"), [0, 1, 2]).unwrap();
    fs::write(root.join("broken.png"), b"\x89PNG\r\n\x1a\ninvalid").unwrap();
    let oversized = fs::File::create(root.join("oversized.png")).unwrap();
    oversized.set_len(MAX_BYTES as u64 + 1).unwrap();
    for args in [
        json!({}),
        json!({"id":"valid","path":"logo.png"}),
        json!({"path":"../outside.png"}),
        json!({"path":home.path().join("outside.png")}),
        json!({"path":"unsupported.bin"}),
        json!({"path":"broken.png"}),
        json!({"path":"oversized.png"}),
        json!({"path":"logo.png","limit":1}),
    ] {
        assert!(
            read_tool_with_project(home.path(), &conversation, Some(&root), &args).is_err(),
            "{args}"
        );
    }
    assert!(read_tool_with_project(
        home.path(),
        &conversation,
        None,
        &json!({"path":"logo.png"})
    )
    .is_err());
    assert!(read_tool(
        home.path(),
        &conversation,
        &json!({"id":"valid","path":"logo.png"})
    )
    .is_err());
    #[cfg(unix)]
    {
        let mut png = Cursor::new(Vec::new());
        image::DynamicImage::new_rgb8(4, 4)
            .write_to(&mut png, image::ImageFormat::Png)
            .unwrap();
        fs::write(root.join("valid.png"), png.get_ref()).unwrap();
        std::os::unix::fs::symlink(root.join("valid.png"), root.join("linked.png")).unwrap();
        std::os::unix::fs::symlink(&root, root.join("linked-dir")).unwrap();
        for path in ["linked.png", "linked-dir/valid.png"] {
            assert!(read_tool_with_project(
                home.path(),
                &conversation,
                Some(&root),
                &json!({"path":path})
            )
            .is_err());
        }
    }
    assert!(!directory(home.path(), &conversation).unwrap().exists());
}

#[test]
fn pdf_text_is_available_to_the_document_reader() {
    let stream = "BT /F1 12 Tf 20 50 Td (Jarvis PDF reference) Tj ET";
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_owned(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_owned(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 300 200] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>".to_owned(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_owned(),
        format!("<< /Length {} >>\nstream\n{stream}\nendstream", stream.len()),
    ];
    let mut pdf = "%PDF-1.4\n".to_owned();
    let mut offsets = vec![];
    for (index, object) in objects.iter().enumerate() {
        offsets.push(pdf.len());
        pdf.push_str(&format!("{} 0 obj\n{object}\nendobj\n", index + 1));
    }
    let xref = pdf.len();
    pdf.push_str("xref\n0 6\n0000000000 65535 f \n");
    for offset in offsets {
        pdf.push_str(&format!("{offset:010} 00000 n \n"));
    }
    pdf.push_str(&format!(
        "trailer\n<< /Size 6 /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF"
    ));
    assert!(document(pdf.as_bytes(), "pdf")
        .unwrap()
        .0
        .contains("Jarvis PDF reference"));
}

#[test]
fn images_are_normalized_and_documents_are_paginated_without_exposing_paths() {
    let home = tempfile::tempdir().unwrap();
    let conversation = "a".repeat(32);
    let doc = store(
        home.path(),
        &conversation,
        "../../notes.txt",
        "Olá 🦀\nDocument".as_bytes(),
    )
    .unwrap();
    assert_eq!(doc.name, "notes.txt");
    let result: Value = serde_json::from_str(
        &read_tool(
            home.path(),
            &conversation,
            &json!({"id":doc.id,"offset":4,"limit":1}),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(result["text"], "🦀");
    let mut png = Cursor::new(vec![]);
    image::DynamicImage::new_rgb8(2400, 100)
        .write_to(&mut png, image::ImageFormat::Png)
        .unwrap();
    let image = store(home.path(), &conversation, "screen.png", png.get_ref()).unwrap();
    let normalized = image::load_from_memory(
        &fs::read(
            location(home.path(), &conversation, &image.id)
                .unwrap()
                .join("content"),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(normalized.width(), 2048);
    assert!(read_tool(home.path(), &conversation, &json!({"id":image.id})).is_err());
    assert!(metadata(home.path(), &"b".repeat(32), &doc.id).is_err());
    assert!(location(home.path(), "../outside", &doc.id).is_err());
    assert!(store(home.path(), &conversation, "bad.bin", &[0, 1, 2]).is_err());
}

#[test]
fn docx_preserves_paragraphs_unicode_and_xml_entities() {
    let mut bytes = Cursor::new(Vec::new());
    {
        let mut zip = zip::ZipWriter::new(&mut bytes);
        zip.start_file(
            "word/document.xml",
            zip::write::SimpleFileOptions::default(),
        )
        .unwrap();
        zip.write_all("<w:document xmlns:w=\"urn:test\"><w:p><w:r><w:t>Olá &amp; &#x1F980;</w:t></w:r></w:p><w:p><w:r><w:t>Segundo</w:t></w:r></w:p></w:document>".as_bytes()).unwrap();
        zip.finish().unwrap();
    }
    let (text, _) = document(bytes.get_ref(), "docx").unwrap();
    assert_eq!(text, "Olá & 🦀\nSegundo\n");
}

#[tokio::test]
async fn attachment_references_survive_queue_and_journal_and_never_inline_binary_data() {
    use super::super::{
        finish, journal, skill_input,
        tests::{session, Fixture},
        ApprovalMode, Mode, TurnOptions,
    };
    let fixture = Fixture::new();
    let mut session = session(&fixture);
    std::sync::Arc::get_mut(&mut session).unwrap().id = "a".repeat(32);
    let item = store(
        &fixture.root,
        &session.id,
        "report.txt",
        b"Private reference",
    )
    .unwrap();
    let mut parts = vec![MessagePart::Attachment {
        attachment: item.clone(),
    }];
    validate_parts(&fixture.root, &session.id, &mut parts).unwrap();
    let (content, mut parts) =
        skill_input::normalize(&fixture.root, &fixture.root, String::new(), parts).unwrap();
    assert_eq!(content, "Analise os anexos.");
    let options = TurnOptions {
        executor: crate::claude::Executor::Jarvis,
        account: "test".into(),
        model: "test".into(),
        reasoning: None,
        mode: Mode::Build,
        workflow: None,
        custom_workflow_id: None,
        custom_agent_id: None,
        approval_mode: ApprovalMode::Yolo,
        manual_validation: false,
        automatic_publication: None,
    };
    session
        .submit_message("first".into(), options.clone(), vec![])
        .unwrap();
    session
        .submit_message(content, options, parts.clone())
        .unwrap();
    let (_, extras) = journal::load_all(&session.journal).unwrap();
    assert!(
        matches!(&extras.queue[0].parts[0], MessagePart::Attachment { attachment } if attachment.id == item.id)
    );
    finish(&session, Ok(()));
    session.reserve_next().unwrap().unwrap();
    skill_input::load(&session, &fixture.root).await.unwrap();
    let (stored, _) = journal::load_all(&session.journal).unwrap();
    let wire = stored.last().unwrap().wire[0].to_string();
    assert!(wire.contains(&item.id));
    assert!(!wire.contains("Private reference"));
    parts.push(parts[0].clone());
    assert!(validate_parts(&fixture.root, &session.id, &mut parts).is_err());
}
