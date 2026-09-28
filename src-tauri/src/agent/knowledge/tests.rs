use super::*;

fn fixture() -> (tempfile::TempDir, std::path::PathBuf) {
    let temp = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(temp.path()).unwrap();
    (temp, root)
}
fn edit(root: &Path, scope: &str, kind: Kind, content: &str, essential: &str) -> Document {
    let previous = document(
        root,
        &entry_for(root, &load_index(root).unwrap(), scope, kind),
    )
    .unwrap();
    save(
        root,
        SaveRequest {
            scope: scope.into(),
            kind,
            content: content.into(),
            essential: essential.into(),
            revision: previous.revision,
            sources: vec![],
        },
    )
    .unwrap()
}

#[test]
fn saving_reuses_existing_markdown_and_rejects_stale_editor_revisions() {
    let (_temp, root) = fixture();
    fs::write(root.join("DESIGN.md"), "# Existing\nBlue").unwrap();
    let initial = snapshot(&root, vec![]).unwrap();
    let design = initial
        .documents
        .iter()
        .find(|d| d.kind == Kind::Design)
        .unwrap();
    assert_eq!(design.path, "DESIGN.md");
    let saved = edit(&root, ".", Kind::Design, "# Existing\nGreen", "");
    assert_eq!(
        fs::read_to_string(root.join("DESIGN.md")).unwrap(),
        saved.content
    );
    fs::write(root.join("DESIGN.md"), "# User edited elsewhere").unwrap();
    let result = save(
        &root,
        SaveRequest {
            kind: Kind::Design,
            scope: ".".into(),
            content: "overwrite".into(),
            essential: String::new(),
            revision: saved.revision,
            sources: vec![],
        },
    );
    assert!(result.is_err());
    assert_eq!(
        fs::read_to_string(root.join("DESIGN.md")).unwrap(),
        "# User edited elsewhere"
    );
}

#[test]
fn scoped_search_includes_shared_facts_and_excludes_other_repositories() {
    let (_temp, root) = fixture();
    fs::create_dir(root.join("front")).unwrap();
    fs::create_dir(root.join("back")).unwrap();
    edit(
        &root,
        ".",
        Kind::Product,
        "# Product\nInvoices for businesses",
        "",
    );
    edit(
        &root,
        "front",
        Kind::Design,
        "# Buttons\nReuse InvoiceButton",
        "",
    );
    edit(
        &root,
        "back",
        Kind::Technical,
        "# Invoices\nUse PostgreSQL",
        "",
    );
    let found = retrieve(&root, &json!({"path":"front","query":"invoice"})).unwrap();
    assert!(found.contains("InvoiceButton"));
    assert!(found.contains("businesses"));
    assert!(!found.contains("PostgreSQL"));
    assert!(retrieve(&root, &json!({"query":"PostgreSQL"}))
        .unwrap()
        .contains("PostgreSQL"));
    assert!(retrieve(&root, &json!({"path":"../"})).is_err());
}

#[test]
fn reading_a_long_section_is_paged_and_source_changes_are_visible() {
    let (_temp, root) = fixture();
    fs::write(root.join("package.json"), "{}").unwrap();
    let saved = edit(
        &root,
        ".",
        Kind::Technical,
        &format!("# Stack\n{}", "á".repeat(8_000)),
        "",
    );
    save(
        &root,
        SaveRequest {
            kind: saved.kind,
            scope: saved.scope,
            content: saved.content,
            essential: saved.essential,
            revision: saved.revision,
            sources: vec![Source {
                path: "package.json".into(),
                fingerprint: fingerprint("{}"),
            }],
        },
    )
    .unwrap();
    fs::write(root.join("package.json"), "{\"new\":true}").unwrap();
    let result: Value = serde_json::from_str(&retrieve(&root, &json!({})).unwrap()).unwrap();
    let id = result["sections"][0]["id"].as_str().unwrap();
    let page: Value =
        serde_json::from_str(&retrieve(&root, &json!({"sectionId":id})).unwrap()).unwrap();
    assert_eq!(
        page["sections"][0]["content"]
            .as_str()
            .unwrap()
            .chars()
            .count(),
        6_000
    );
    assert_eq!(page["sections"][0]["nextOffset"], 6_000);
    assert_eq!(page["sections"][0]["stale"], true);
    let next: Value =
        serde_json::from_str(&retrieve(&root, &json!({"sectionId":id,"offset":6_000})).unwrap())
            .unwrap();
    assert!(next["sections"][0]["nextOffset"].is_null());
}

#[test]
fn linking_preserves_the_source_and_tracks_external_edits() {
    let (_temp, root) = fixture();
    fs::write(root.join("architecture.md"), "# Architecture\nOriginal").unwrap();
    let initial = snapshot(&root, vec![]).unwrap().documents.remove(1);
    let linked = link(
        &root,
        ".",
        Kind::Technical,
        "architecture.md",
        &initial.revision,
    )
    .unwrap();
    assert_eq!(linked.content, "# Architecture\nOriginal");
    fs::write(root.join("architecture.md"), "# Architecture\nNew").unwrap();
    assert!(retrieve(&root, &json!({"query":"New"}))
        .unwrap()
        .contains("New"));
    assert!(link(
        &root,
        ".",
        Kind::Technical,
        "../../other.md",
        &linked.revision
    )
    .is_err());
}

#[test]
fn only_essential_rules_are_automatically_loaded_for_the_affected_scope() {
    let (_temp, root) = fixture();
    fs::create_dir(root.join("front")).unwrap();
    fs::create_dir(root.join("back")).unwrap();
    edit(
        &root,
        ".",
        Kind::Rules,
        "# Detailed\nOptional details",
        "Always preserve user data.",
    );
    edit(
        &root,
        "front",
        Kind::Rules,
        "# Detailed\nLong details",
        "Use existing UI components.",
    );
    let initial = overview(&root);
    assert!(initial.contains("Always preserve user data."));
    assert!(!initial.contains("Optional details"));
    assert!(!initial.contains("Use existing UI components."));
    assert!(scoped_rules(&root, &[root.join("back")])
        .unwrap()
        .is_empty());
    let mut resolver = crate::agent::instructions::Resolver::new(&root).unwrap();
    let call = crate::agent::ToolCall {
        id: "test".into(),
        name: "read".into(),
        args: json!({"path":"front/Button.tsx"}),
        status: "pending".into(),
        output: String::new(),
        duration_ms: 0,
    };
    assert!(resolver.discover(&call).unwrap());
    assert!(!resolver.discover(&call).unwrap());
    let mut prompt = String::new();
    resolver.append_prompt(&mut prompt);
    assert!(prompt.contains("Use existing UI components."));
    edit(
        &root,
        "front",
        Kind::Rules,
        "# Detailed",
        "Use updated UI components.",
    );
    assert!(resolver.discover(&call).unwrap());
}

#[test]
fn empty_essential_rules_do_not_interrupt_tools_and_cleared_rules_are_removed() {
    let (_temp, root) = fixture();
    fs::create_dir(root.join("front")).unwrap();
    edit(&root, "front", Kind::Rules, "# Details", "");
    let mut resolver = crate::agent::instructions::Resolver::new(&root).unwrap();
    let call = crate::agent::ToolCall {
        id: "test".into(),
        name: "read".into(),
        args: json!({"path":"front/a.ts"}),
        status: "pending".into(),
        output: String::new(),
        duration_ms: 0,
    };
    assert!(!resolver.discover(&call).unwrap());
    edit(&root, "front", Kind::Rules, "# Details", "Preserve data.");
    assert!(resolver.discover(&call).unwrap());
    edit(&root, "front", Kind::Rules, "# Details", "");
    assert!(resolver.discover(&call).unwrap());
    assert!(!resolver.discover(&call).unwrap());
    let mut prompt = String::new();
    resolver.append_prompt(&mut prompt);
    assert!(!prompt.contains("Preserve data."));
}

#[test]
fn knowledge_is_available_without_mutation_permission() {
    use crate::agent::Mode;
    assert!(tools::definitions(Mode::Plan)
        .iter()
        .any(|d| d["name"] == TOOL));
    assert!(!tools::needs_approval(TOOL));
}

#[test]
fn a_search_returns_the_relevant_excerpt_even_near_the_end_of_a_long_section() {
    let (_temp, root) = fixture();
    edit(
        &root,
        ".",
        Kind::Technical,
        &format!(
            "# Details\n{}\nImportant migration convention",
            "á".repeat(12_000)
        ),
        "",
    );
    let result: Value =
        serde_json::from_str(&retrieve(&root, &json!({"query":"migration"})).unwrap()).unwrap();
    assert!(result["sections"][0]["content"]
        .as_str()
        .unwrap()
        .contains("migration convention"));
    assert!(result["sections"][0]["offset"].as_u64().unwrap() > 0);
}

#[tokio::test]
async fn native_dispatch_reads_knowledge_and_paginates_all_headings() {
    let (_temp, root) = fixture();
    let markdown = (0..10)
        .map(|i| format!("# Section {i}\nFact {i}\n"))
        .collect::<String>();
    edit(&root, ".", Kind::Product, &markdown, "");
    let (_send, signal) = tokio::sync::watch::channel(false);
    let call = crate::agent::ToolCall {
        id: "knowledge".into(),
        name: TOOL.into(),
        args: json!({"limit":6}),
        status: "pending".into(),
        output: String::new(),
        duration_ms: 0,
    };
    let result: Value = serde_json::from_str(
        &tools::execute(&root, &call, crate::agent::Mode::Plan, signal)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(result["sections"].as_array().unwrap().len(), 6);
    let page: Value = serde_json::from_str(
        &retrieve(&root, &json!({"start":result["nextStart"],"limit":6})).unwrap(),
    )
    .unwrap();
    assert_eq!(page["sections"].as_array().unwrap().len(), 4);
    assert!(page["nextStart"].is_null());
}

#[test]
fn unreadable_documents_are_isolated_and_linked_design_is_reused_by_preparation() {
    let (_temp, root) = fixture();
    let design = edit(&root, ".", Kind::Design, "# Design\nReusable identity", "");
    fs::write(root.join("identity.md"), "# Palette\nUse the brand").unwrap();
    link(&root, ".", Kind::Design, "identity.md", &design.revision).unwrap();
    assert_eq!(
        design_paths(&root, &[]),
        vec![std::path::PathBuf::from("identity.md")]
    );
    edit(&root, ".", Kind::Product, "# Product\nAvailable fact", "");
    fs::write(root.join("identity.md"), "X".repeat(MAX_DOCUMENT + 1)).unwrap();
    let state = snapshot(&root, vec![]).unwrap();
    assert!(state
        .documents
        .iter()
        .find(|doc| doc.kind == Kind::Design)
        .unwrap()
        .error
        .is_some());
    assert!(retrieve(&root, &json!({"query":"Available"}))
        .unwrap()
        .contains("Available fact"));
}

#[cfg(unix)]
#[test]
fn document_and_metadata_symlinks_cannot_escape_the_project() {
    let (_temp, root) = fixture();
    let (_outside, outside) = fixture();
    fs::write(outside.join("private.md"), "private").unwrap();
    std::os::unix::fs::symlink(outside.join("private.md"), root.join("linked.md")).unwrap();
    let current = snapshot(&root, vec![]).unwrap().documents.remove(0);
    assert!(link(&root, ".", Kind::Product, "linked.md", &current.revision).is_err());
    std::os::unix::fs::symlink(&outside, root.join(".jarvis")).unwrap();
    assert!(save(
        &root,
        SaveRequest {
            kind: Kind::Product,
            scope: ".".into(),
            content: "unsafe".into(),
            essential: String::new(),
            revision: current.revision,
            sources: vec![]
        }
    )
    .is_err());
}
