use super::*;
use crate::core::{
    hooks::Hooks, root, save_manifest, ComponentId, CoreState, Installation, Manifest,
};

const RULES: &str = "---\nname: ponytail\ndescription: Coding guidance\n---\n# Ponytail\n\n## Persistence\nCLI-only activation and /ponytail commands.\n\n## The ladder\nReuse the existing helper before adding a dependency.\n\n## Rules\n- No unrequested abstractions: keep this rule verbatim.\n- Full: this unquoted rule belongs to every mode.\n\n## Intensity\n| Level | Change |\n| **lite** | Alternative |\n| **full** | Smallest correct diff |\n| **ultra** | Extreme |\n- lite: \"lite example\"\n- full: \"full example\"\n- ultra: \"ultra example\"\n\n## When NOT to be lazy\nKeep validation, accessibility, tests and user requirements.\n\n## Boundaries\nCLI-only status and stop commands.\n";
// Exact SKILL.md content from the official v4.12.0 and v5.1.0 tags (MIT).
const RULES_V4: &str = include_str!("fixtures/4.12.0.md");
const RULES_V5: &str = include_str!("fixtures/5.1.0.md");

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
        if id == ComponentId::Impeccable {
            crate::core::design::tests::prepare_fixture(&path, &[]).unwrap();
        }
        if id == ComponentId::Hyperframes {
            crate::core::hyperframes::tests::fixture(&path, "4.9.0");
        }
        if id == ComponentId::Audiovisual {
            crate::core::audiovisual::tests::fixture(&path);
        }
        if id == ComponentId::Comfyui {
            crate::core::comfyui::tests::fixture(&path);
        }
        if id == ComponentId::Graft {
            crate::core::graft::fixture(&path);
        }
        if id == ComponentId::Openmontage {
            crate::core::openmontage::tests::fixture(&path);
        }
        manifest.installations.insert(
            id,
            Installation {
                version: if id == ComponentId::Impeccable {
                    "1.2.3"
                } else if id == ComponentId::Audiovisual {
                    crate::core::audiovisual::VERSION
                } else if id == ComponentId::Comfyui {
                    crate::core::comfyui::VERSION
                } else if id == ComponentId::Graft {
                    crate::core::install::GRAFT_VERSION
                } else if id == ComponentId::Openmontage {
                    crate::core::openmontage::VERSION
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
    let example = "```md\n## Persistence\nActive for the whole session until example ends.\n- ultra: \"this is code, not an intensity example\"\n```";
    let rules = RULES.replace("## Rules\n", &format!("## Rules\n{example}\n"));
    assert!(full_rules(&rules).unwrap().contains(example));
}

#[test]
fn official_v4_and_v5_releases_keep_complete_full_guidance_without_host_controls() {
    let directory = tempfile::tempdir().unwrap();
    for (version, rules, headings) in [
        (
            "4.12.0",
            RULES_V4,
            &["The ladder", "Rules", "Output", "When NOT to be lazy"][..],
        ),
        (
            "5.1.0",
            RULES_V5,
            &["Before you write", "The smallest complete change"][..],
        ),
    ] {
        package(directory.path(), version, rules);
        let native = Ponytail::at(directory.path(), version).unwrap();
        assert!(native.prompt.contains(&format!("version=\"{version}\"")));
        for heading in headings {
            let marker = format!("## {heading}\n");
            let section = rules
                .split_once(&marker)
                .unwrap()
                .1
                .split("\n## ")
                .next()
                .unwrap()
                .trim();
            assert!(native.prompt.contains(section), "{version}: {heading}");
        }
        assert!(native.prompt.contains("| **full** |"));
        for control in [
            "| **lite** |",
            "| **ultra** |",
            "## Persistence",
            "## Boundaries",
            "Active for the whole session until",
            "Switch level:",
        ] {
            assert!(!native.prompt.contains(control), "{version}: {control}");
        }
        assert!(native.prompt.ends_with(&format!("{HOST_POLICY}\n")));
    }
}

#[test]
fn v5_rules_still_reject_missing_guidance_and_malformed_documents() {
    let directory = tempfile::tempdir().unwrap();
    for rules in [
        RULES_V5.replace("## Before you write", "## Unknown"),
        RULES_V5.replace("## The smallest complete change", "## Unknown"),
        RULES_V5.replace("## Levels", "## Unknown"),
        RULES_V5.replace("Never cut:", "Removed:"),
        RULES_V5
            .lines()
            .map(|line| {
                if line.starts_with("Never cut:") {
                    "Never cut:"
                } else {
                    line
                }
            })
            .collect::<Vec<_>>()
            .join("\n"),
        format!(
            "{}\n```md\nNever cut: only a code example\n```",
            RULES_V5.replace("Never cut:", "Removed:")
        ),
        RULES_V5.replace("| **full** |", "| **new-mode** |"),
        format!("{RULES_V5}\n```md\nUnclosed code example"),
    ] {
        package(directory.path(), "5.1.0", &rules);
        assert!(Ponytail::at(directory.path(), "5.1.0").is_err());
    }
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
    let status = CoreState::default().snapshot(home.path()).unwrap();
    assert!(
        status.ready,
        "Core fixture is not ready: {}",
        serde_json::to_string(&status).unwrap()
    );
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
    let old = manifest
        .installations
        .get_mut(&ComponentId::Ponytail)
        .unwrap();
    package(&old.path(home.path()).unwrap(), "4.12.0", RULES_V4);
    old.version = "4.12.0".into();
    save_manifest(home.path(), &manifest).unwrap();
    let hooks = Hooks::new(home.path(), home.path(), "conversation-a").unwrap();
    let base = "Jarvis: Plan mode. Manual approval required.\nProject: use Vitest and keep all tests.\nUser: preserve café.ts, answer in pt-BR and explain all three cases.\nContext-mode: use ctx_search for indexed sources.\n";
    let mut first = base.to_owned();
    hooks.before_agent(&mut first);
    assert!(first.starts_with(base));
    assert!(first.contains("version=\"4.12.0\" mode=\"full\" sha256="));
    assert!(first.ends_with(&format!("{HOST_POLICY}\n")));

    let path = root(home.path()).join("ponytail/updated");
    package(&path, "5.1.0", RULES_V5);
    manifest.installations.insert(
        ComponentId::Ponytail,
        Installation {
            version: "5.1.0".into(),
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
    assert!(next.starts_with(base));
    assert!(next.contains("version=\"5.1.0\""));
    assert!(next.contains("## The smallest complete change"));
    assert!(!next.contains("## The ladder"));
    assert!(next.ends_with(&format!("{HOST_POLICY}\n")));
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
        "Before you write",
        "The smallest complete change",
        "Levels",
    ] {
        let start = format!("## {heading}\n");
        let Some((_, body)) = upstream.split_once(&start) else {
            continue;
        };
        let section = body.split("\n## ").next().unwrap().trim();
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
