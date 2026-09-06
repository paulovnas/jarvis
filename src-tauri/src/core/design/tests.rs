use super::*;
use flate2::{write::GzEncoder, Compression};

fn archive(extra: &[(&str, &str)]) -> Vec<u8> {
    let mut archive = tar::Builder::new(GzEncoder::new(Vec::new(), Compression::fast()));
    for (path, content) in [
        ("package.json", r#"{"name":"open-design","version":"1.2.3"}"#),
        ("LICENSE", "Apache-2.0"),
        ("design-systems/test/manifest.json", r#"{"name":"Test System","description":"Developer tools"}"#),
        ("design-systems/test/DESIGN.md", "Graphite and blue"),
        ("design-templates/landing/SKILL.md", "---\nname: Landing\ndescription: Marketing page\n---\n# Landing"),
        ("skills/design-brief/SKILL.md", "---\nname: Brief\ndescription: Design decisions\n---\n# Brief"),
        ("craft/color.md", "# Color contrast"),
    ].into_iter().chain(extra.iter().copied()) {
        let mut header = tar::Header::new_gnu(); header.set_size(content.len() as u64); header.set_mode(0o644); header.set_cksum();
        archive.append_data(&mut header, format!("source/{path}"), content.as_bytes()).unwrap();
    }
    archive.into_inner().unwrap().finish().unwrap()
}
pub(in crate::core) fn prepare_fixture(directory: &Path, extra: &[(&str, &str)]) -> Result<(), CoreError> {
    prepare(std::io::Cursor::new(archive(extra)), directory, "1.2.3", &"a".repeat(40), &"b".repeat(64))
}
#[test]
fn extracts_only_portable_resources_and_preserves_license_and_provenance() {
    let dir = tempfile::tempdir().unwrap();
    prepare_fixture(dir.path(), &[("apps/daemon/server.ts", "never executed"), ("skills/ui-ux-pro-max/SKILL.md", "catalogue pointer"), ("skills/design-brief/scripts/install.js", "unsafe host install"), ("design-systems/_schema/schema.json", "{}"), ("design-systems/README.md", "readme")]).unwrap();
    assert!(!dir.path().join("apps").exists());
    assert!(!dir.path().join("skills/ui-ux-pro-max").exists());
    assert!(!dir.path().join("skills/design-brief/scripts/install.js").exists());
    assert_eq!(fs::read_to_string(dir.path().join("LICENSE")).unwrap(), "Apache-2.0");
    let pack = Pack::at(dir.path(), "1.2.3").unwrap();
    assert_eq!(pack.index.resources.len(), 4);
    assert_eq!(pack.index.commit, "a".repeat(40));
    assert!(Pack::at(dir.path(), "9.0.0").is_err());
}
#[test]
fn searches_metadata_and_reads_unicode_in_bounded_pages_without_path_escape() {
    let dir = tempfile::tempdir().unwrap(); let long = "á".repeat(13000);
    prepare_fixture(dir.path(), &[("design-systems/test/tokens.css", &long)]).unwrap();
    let pack = Pack::at(dir.path(), "1.2.3").unwrap();
    let search: Value = serde_json::from_str(&pack.execute("design_search", &json!({"query":"developer","kind":"system"})).unwrap()).unwrap();
    assert_eq!(search["total"], 1); assert!(search["resources"][0].get("files").is_none());
    let listing: Value = serde_json::from_str(&pack.execute("design_read", &json!({"id":"design-systems/test"})).unwrap()).unwrap();
    assert!(listing["files"].as_array().unwrap().contains(&json!("design-systems/test/tokens.css")));
    for file in [Value::Null, json!(""), json!(".")] {
        let listing: Value = serde_json::from_str(&pack.execute("design_read", &json!({"id":"design-systems/test","file":file,"offset":0})).unwrap()).unwrap();
        assert!(listing["files"].is_array());
    }
    let schema = definitions().into_iter().find(|d| d["name"] == "design_read").unwrap();
    assert_eq!(schema["strict"], false);
    let validator = jsonschema::validator_for(&schema["parameters"]).unwrap();
    assert!(validator.is_valid(&json!({"id":"design-systems/test","file":null})));
    assert!(validator.is_valid(&json!({"id":"design-systems/test"})));
    let args = json!({"id":"design-systems/test","file":"design-systems/test/tokens.css"});
    let read: Value = serde_json::from_str(&pack.execute("design_read", &args).unwrap()).unwrap();
    assert_eq!(read["content"].as_str().unwrap().chars().count(), 12000); assert_eq!(read["nextOffset"], 12000);
    let mut next = args.clone(); next["offset"] = json!(12000);
    let page: Value = serde_json::from_str(&pack.execute("design_read", &next).unwrap()).unwrap();
    assert_eq!(page["content"].as_str().unwrap().chars().count(), 1000);
    assert!(pack.execute("design_read", &json!({"id":"design-systems/test","file":"../../secret"})).is_err());
    assert!(pack.execute("design_search", &json!({"query":"","offset":-1})).is_err());
}
#[test]
fn incomplete_or_duplicate_packages_never_validate() {
    let dir = tempfile::tempdir().unwrap();
    assert!(prepare_fixture(dir.path(), &[("LICENSE", "duplicate")]).is_err());
    let dir = tempfile::tempdir().unwrap();
    assert!(prepare(std::io::Cursor::new(archive(&[])), dir.path(), "2.0.0", &"a".repeat(40), &"b".repeat(64)).is_err());
}
#[cfg(unix)]
#[test]
fn resource_symlinks_cannot_escape_the_active_package() {
    let dir = tempfile::tempdir().unwrap(); prepare_fixture(dir.path(), &[]).unwrap();
    let outside = tempfile::NamedTempFile::new().unwrap(); fs::write(outside.path(), "secret").unwrap();
    let file = dir.path().join("design-systems/test/DESIGN.md"); fs::remove_file(&file).unwrap();
    std::os::unix::fs::symlink(outside.path(), file).unwrap();
    let pack = Pack::at(dir.path(), "1.2.3").unwrap();
    assert!(pack.execute("design_read", &json!({"id":"design-systems/test","file":"design-systems/test/DESIGN.md"})).is_err());
}
