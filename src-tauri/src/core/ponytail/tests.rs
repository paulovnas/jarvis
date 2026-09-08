use super::*;
use crate::core::{
    hooks::Hooks, root, save_manifest, ComponentId, CoreState, Installation, Manifest,
};

const RULES: &str = "---\nname: ponytail\ndescription: Coding guidance\n---\n# Ponytail\n\n## Persistence\nCLI-only activation and /ponytail commands.\n\n## The ladder\nReuse the existing helper before adding a dependency.\n\n## Rules\n- No unrequested abstractions: keep this rule verbatim.\n- Full: this unquoted rule belongs to every mode.\n\n## Intensity\n| Level | Change |\n| **lite** | Alternative |\n| **full** | Smallest correct diff |\n| **ultra** | Extreme |\n- lite: \"lite example\"\n- full: \"full example\"\n- ultra: \"ultra example\"\n\n## When NOT to be lazy\nKeep validation, accessibility, tests and user requirements.\n\n## Boundaries\nCLI-only status and stop commands.\n";

fn package(path: &Path, version: &str, rules: &str) {
    fs::create_dir_all(path.join("skills/ponytail")).unwrap();
    fs::write(path.join(SKILL_PATH), rules).unwrap();
    fs::write(
        path.join("package.json"),
        serde_json::json!({
            "name":"@dietrichgebert/ponytail", "version":version
        })
        .to_string(),
    )
    .unwrap();
}

pub(in crate::core) fn fixture_package(path: &Path, version: &str) {
    package(path, version, RULES);
}

fn install_fixture(home: &Path) -> Manifest {
    let mut manifest = Manifest::default();
    for id in ComponentId::ALL {
        let directory = format!("{}/test", id.key());
        let path = root(home).join(&directory);
        fs::create_dir_all(&path).unwrap();
        fs::write(path.join("verified"), "ok").unwrap();
        if id == ComponentId::Ponytail {
            fixture_package(&path, "4.9.0");
        }
        if id == ComponentId::OpenDesign {
            crate::core::design::tests::prepare_fixture(&path, &[]).unwrap();
        }
        manifest.installations.insert(
            id,
            Installation {
                version: if id == ComponentId::OpenDesign {
                    "1.2.3"
                } else {
                    "4.9.0"
                }
                .into(),
                directory,
                files: vec!["verified".into()],
            },
        );
    }
    save_manifest(home, &manifest).unwrap();
    manifest
}

#[test]
fn full_guidance_keeps_coding_rules_and_excludes_other_levels_and_cli_controls() {
    let body = full_rules(&RULES.replace('\n', "\r\n")).unwrap();
    for expected in [
        "Reuse the existing helper",
        "No unrequested abstractions:",
        "Full: this unquoted rule",
        "| **full** |",
        "- full: \"full example\"",
        "Keep validation, accessibility, tests and user requirements.",
    ] {
        assert!(body.contains(expected), "Missing {expected}");
    }
    for excluded in [
        "description:",
        "CLI-only",
        "## Persistence",
        "## Boundaries",
        "| **lite**",
        "| **ultra**",
        "- lite:",
        "- ultra:",
    ] {
        assert!(!body.contains(excluded), "Unexpected {excluded}");
    }
}

#[test]
fn code_examples_are_preserved_exactly_even_when_they_look_like_host_sections() {
    let example = "```md\n## Persistence\n- ultra: \"this is code, not an intensity example\"\n```";
    let rules = RULES.replace("## Rules\n", &format!("## Rules\n{example}\n"));
    assert!(full_rules(&rules).unwrap().contains(example));
}

#[test]
fn invalid_or_incompatible_rules_are_rejected_before_enabling_core() {
    let home = tempfile::tempdir().unwrap();
    install_fixture(home.path());
    fs::write(
        root(home.path()).join("context7.json"),
        r#"{"credential_ref":"jarvis-core-context7-test"}"#,
    )
    .unwrap();
    let path = root(home.path()).join("ponytail/test");
    for rules in [
        RULES.replace("name: ponytail", "name: other"),
        RULES.replace("## The ladder", "## Unknown"),
        RULES.replace("| **full**", "| **new-mode**"),
        RULES.replace("## When NOT to be lazy", "## Unknown"),
        "x".repeat(MAX_RULE_BYTES as usize + 1),
    ] {
        fs::write(path.join(SKILL_PATH), rules).unwrap();
        assert!(crate::core::require_ready(home.path()).is_err());
        let status = CoreState::default().snapshot(home.path()).unwrap();
        assert!(!status.ready);
        let item = status
            .items
            .iter()
            .find(|item| item.id == ComponentId::Ponytail)
            .unwrap();
        assert!(!item.installed);
        assert!(item.error.as_deref().unwrap().contains("Reinstale"));
    }
    fixture_package(&path, "4.9.0");
    assert!(CoreState::default().snapshot(home.path()).unwrap().ready);
    fs::remove_file(path.join(SKILL_PATH)).unwrap();
    assert!(Hooks::new(home.path(), home.path(), "session").is_err());
}

#[test]
fn package_identity_and_version_must_match_the_active_installation() {
    let dir = tempfile::tempdir().unwrap();
    package(dir.path(), "4.9.0", RULES);
    assert!(Ponytail::at(dir.path(), "4.9.1").is_err());
    fs::write(
        dir.path().join("package.json"),
        r#"{"name":"other","version":"4.9.0"}"#,
    )
    .unwrap();
    assert!(Ponytail::at(dir.path(), "4.9.0").is_err());
}

#[cfg(unix)]
#[test]
fn rules_cannot_escape_the_private_installation_through_a_symlink() {
    let dir = tempfile::tempdir().unwrap();
    let external = tempfile::tempdir().unwrap();
    package(dir.path(), "4.9.0", RULES);
    fs::write(external.path().join("SKILL.md"), RULES).unwrap();
    fs::remove_file(dir.path().join(SKILL_PATH)).unwrap();
    std::os::unix::fs::symlink(
        external.path().join("SKILL.md"),
        dir.path().join(SKILL_PATH),
    )
    .unwrap();
    assert!(Ponytail::at(dir.path(), "4.9.0").is_err());
}

#[test]
fn before_agent_preserves_host_requirements_and_freezes_rules_until_the_next_turn() {
    let home = tempfile::tempdir().unwrap();
    let mut manifest = install_fixture(home.path());
    let hooks = Hooks::new(home.path(), home.path(), "conversation-a").unwrap();
    let base = "Jarvis: Plan mode. Manual approval required.\nProject: use Vitest and keep all tests.\nUser: preserve café.ts, answer in pt-BR and explain all three cases.\nContext-mode: use ctx_search for indexed sources.\n";
    let mut first = base.to_owned();
    hooks.before_agent(&mut first);
    assert!(first.starts_with(base));
    assert!(first.contains("version=\"4.9.0\" mode=\"full\" sha256="));
    assert!(first.ends_with(&format!("{HOST_POLICY}\n")));

    let path = root(home.path()).join("ponytail/updated");
    package(
        &path,
        "4.9.1",
        &RULES.replace("Reuse the existing helper", "New release guidance"),
    );
    manifest.installations.insert(
        ComponentId::Ponytail,
        Installation {
            version: "4.9.1".into(),
            directory: "ponytail/updated".into(),
            files: vec!["package.json".into(), SKILL_PATH.into()],
        },
    );
    save_manifest(home.path(), &manifest).unwrap();
    // A retry or a post-compaction model request retains the turn's policy.
    let mut resumed = base.to_owned();
    hooks.before_agent(&mut resumed);
    assert_eq!(resumed, first);
    let mut next = base.to_owned();
    Hooks::new(home.path(), home.path(), "conversation-b")
        .unwrap()
        .before_agent(&mut next);
    assert!(next.contains("version=\"4.9.1\""));
    assert!(next.contains("New release guidance"));
    assert!(!next.contains("Reuse the existing helper"));
}

#[test]
#[ignore = "Reads the privately installed Core and compares against the official Ponytail builder"]
fn installed_rules_match_upstream_full_coding_guidance() {
    let home = std::path::PathBuf::from(std::env::var_os("HOME").unwrap());
    let record = crate::core::installed(&home, ComponentId::Ponytail).unwrap();
    let path = record.path(&home).unwrap();
    let context = crate::core::installed(&home, ComponentId::ContextMode)
        .unwrap()
        .path(&home)
        .unwrap();
    let output = std::process::Command::new(crate::core::install::node_path(&context))
        .args([
            "-e",
            "process.stdout.write(require(process.argv[1]).getPonytailInstructions('full'))",
        ])
        .arg(path.join("hooks/ponytail-instructions.js"))
        .env_remove("NODE_OPTIONS")
        .output()
        .unwrap();
    assert!(output.status.success());
    let upstream = String::from_utf8(output.stdout).unwrap();
    let native = Ponytail::at(&path, &record.version).unwrap();
    // Compare entire substantive sections, using the upstream implementation as oracle.
    for heading in [
        "The ladder",
        "Rules",
        "Output",
        "Intensity",
        "When NOT to be lazy",
    ] {
        let start = format!("## {heading}\n");
        let section = upstream
            .split_once(&start)
            .unwrap()
            .1
            .split("\n## ")
            .next()
            .unwrap()
            .trim();
        assert!(
            native.prompt.contains(section),
            "Changed upstream section: {heading}"
        );
    }
    eprintln!(
        "Ponytail {}: upstream {} bytes; Jarvis policy {} bytes. No message/result compression.",
        record.version,
        upstream.len(),
        native.prompt.len()
    );
}
