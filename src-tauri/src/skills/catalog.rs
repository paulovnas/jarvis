use super::{error, root, store, Config, Detail, Skill, SkillError, MAX_TEXT};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    fs,
    io::Read,
    path::{Component, Path, PathBuf},
};

#[derive(Deserialize)]
struct Frontmatter {
    name: Option<String>,
    description: Option<String>,
    #[serde(default, rename = "disable-model-invocation")]
    manual: bool,
}
pub(super) fn text(path: &Path) -> Result<String, SkillError> {
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW);
    }
    let file = options.open(path)?;
    if !file.metadata()?.is_file() || file.metadata()?.len() > MAX_TEXT as u64 {
        return Err(error(
            "Arquivo de skill excede 1 MiB ou não é um arquivo comum.",
        ));
    }
    let mut bytes = Vec::new();
    file.take(MAX_TEXT as u64 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > MAX_TEXT {
        return Err(error("Arquivo de skill excede 1 MiB."));
    }
    String::from_utf8(bytes).map_err(|_| error("O arquivo da skill precisa estar em UTF-8."))
}
pub(super) fn parse(path: &Path) -> Result<(String, String, bool), SkillError> {
    let raw = text(path)?;
    let raw = raw.trim_start_matches('\u{feff}');
    let mut lines = raw.lines();
    if lines.next().map(str::trim) != Some("---") {
        return Err(error("SKILL.md sem metadados YAML."));
    }
    let mut header = Vec::new();
    let mut closed = false;
    for line in lines {
        if matches!(line.trim(), "---" | "...") {
            closed = true;
            break;
        }
        header.push(line);
    }
    if !closed {
        return Err(error("Metadados YAML incompletos."));
    }
    let meta: Frontmatter = serde_yaml_ng::from_str(&header.join("\n"))
        .map_err(|_| error("Metadados YAML inválidos."))?;
    let name = meta
        .name
        .unwrap_or_else(|| {
            path.parent()
                .and_then(Path::file_name)
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned()
        })
        .trim()
        .to_string();
    let description = meta
        .description
        .unwrap_or_default()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if name.is_empty()
        || name.len() > 128
        || name.chars().any(char::is_control)
        || description.is_empty()
    {
        return Err(error("A skill precisa de nome e descrição válidos."));
    }
    Ok((name, description.chars().take(1024).collect(), !meta.manual))
}
pub(super) fn id(path: &Path) -> String {
    format!("{:x}", Sha256::digest(path.to_string_lossy().as_bytes()))
}
pub(super) fn skill_file(dir: &Path) -> Option<PathBuf> {
    ["SKILL.md", "skill.md"]
        .into_iter()
        .map(|name| dir.join(name))
        .find(|p| p.is_file())
}

struct Scan {
    seen: BTreeSet<PathBuf>,
    visited: usize,
    skills: Vec<Skill>,
    warnings: Vec<String>,
    watched: Vec<super::runtime_cache::Stamp>,
}
fn walk(
    path: &Path,
    origin: &str,
    managed: bool,
    depth: usize,
    config: &Config,
    scan: &mut Scan,
) -> Result<(), SkillError> {
    if depth > 6 || scan.visited >= 6000 || scan.skills.len() >= 512 {
        return Ok(());
    }
    scan.visited += 1;
    scan.watched.push(super::runtime_cache::Stamp::read(path));
    let canonical = match path.canonicalize() {
        Ok(path) => path,
        Err(_) => return Ok(()),
    };
    if !canonical.is_dir() || !scan.seen.insert(canonical.clone()) {
        return Ok(());
    }
    if let Some(file) = skill_file(&canonical) {
        scan.watched.push(super::runtime_cache::Stamp::read(&file));
        scan.watched.push(super::runtime_cache::Stamp::read(
            &canonical.join(".jarvis-source.json"),
        ));
        match parse(&file) {
            Ok((name, description, automatic)) => {
                let file = file.canonicalize()?;
                if !file.starts_with(&canonical) {
                    return Ok(());
                }
                let id = id(&canonical);
                let metadata = if origin == "jarvis" {
                    store::metadata(&canonical)?
                } else {
                    None
                };
                scan.skills.push(Skill {
                    enabled: !config.disabled.contains(&id),
                    id,
                    name,
                    description,
                    origin: origin.into(),
                    removal_path: path.to_path_buf(),
                    linked: fs::symlink_metadata(path)?.file_type().is_symlink(),
                    managed,
                    path: canonical,
                    file,
                    automatic,
                    source: metadata.as_ref().map(|m| m.source.clone()),
                    marketplace_id: metadata
                        .as_ref()
                        .map(|m| format!("{}/{}", m.source, m.skill_id)),
                    update_available: metadata.as_ref().is_some_and(|m| m.update_available),
                    update_error: metadata.and_then(|m| m.update_error),
                });
            }
            Err(cause) => {
                if scan.warnings.len() < 12 {
                    scan.warnings.push(format!(
                        "{}: {}",
                        path.file_name().unwrap_or_default().to_string_lossy(),
                        cause.message
                    ));
                }
            }
        }
        return Ok(());
    }
    let mut entries: Vec<_> = fs::read_dir(path)?.filter_map(Result::ok).collect();
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with('.') || matches!(name.as_ref(), "node_modules" | "target" | "vendor") {
            continue;
        }
        walk(&entry.path(), origin, managed, depth + 1, config, scan)?;
    }
    Ok(())
}
pub(super) fn discover(
    home: &Path,
    project: Option<&Path>,
    config: &Config,
) -> Result<(Vec<Skill>, Vec<String>), SkillError> {
    let (skills, warnings, _) = discover_watched(home, project, config)?;
    Ok((skills, warnings))
}

type Discovery = (Vec<Skill>, Vec<String>, Vec<super::runtime_cache::Stamp>);

pub(super) fn discover_watched(
    home: &Path,
    project: Option<&Path>,
    config: &Config,
) -> Result<Discovery, SkillError> {
    let own = root(home).join("skills");
    fs::create_dir_all(&own)?;
    let mut scan = Scan {
        seen: BTreeSet::new(),
        visited: 0,
        skills: Vec::new(),
        warnings: Vec::new(),
        watched: Vec::new(),
    };
    walk(&own, "jarvis", false, 0, config, &mut scan)?;
    let builtin = super::builtin::root(home);
    walk(&builtin, "jarvis", true, 0, config, &mut scan)?;
    scan.watched.push(super::runtime_cache::Stamp::read(
        &crate::core::root(home).join("manifest.json"),
    ));
    if let Ok(directory) = crate::core::design::skill_directory(home) {
        native_impeccable(&directory, config, &mut scan)?;
    }
    let overlay = crate::plugins::load_active_for_project(home, project)
        .map_err(|cause| error(cause.message))?;
    scan.watched.push(super::runtime_cache::Stamp::read(
        &root(home).join("plugins"),
    ));
    scan.watched.push(super::runtime_cache::Stamp::read(
        &root(home).join("plugins/catalog.json"),
    ));
    scan.warnings.extend(overlay.warnings);
    for source in overlay.skill_roots {
        let source_root = fs::canonicalize(&source.path)?;
        let start = scan.skills.len();
        if source.recursive || skill_file(&source.path).is_some() {
            walk(
                &source.path,
                "plugin",
                true,
                0,
                &Config::default(),
                &mut scan,
            )?;
        } else if let Ok(entries) = fs::read_dir(&source.path) {
            scan.watched
                .push(super::runtime_cache::Stamp::read(&source.path));
            let mut children: Vec<_> = entries
                .filter_map(Result::ok)
                .map(|entry| entry.path())
                .filter(|path| skill_file(path).is_some())
                .collect();
            children.sort();
            for path in children {
                walk(&path, "plugin", true, 0, &Config::default(), &mut scan)?;
            }
        }
        for skill in &mut scan.skills[start..] {
            let relative = skill
                .path
                .strip_prefix(&source_root)
                .map_err(|_| error("A skill está fora do componente do plugin."))?;
            skill.id = format!(
                "{:x}",
                Sha256::digest(
                    format!(
                        "{}\0{}\0{}",
                        source.plugin_id,
                        source.component_id,
                        relative.display()
                    )
                    .as_bytes()
                )
            );
            skill.name = format!("{}:{}", source.plugin_id, skill.name);
            skill.source = Some(source.plugin_id.clone());
            skill.marketplace_id = Some(source.component_id.clone());
        }
    }
    if config.include_agents {
        if let Some(project) = project {
            walk(
                &project.join(".agents/skills"),
                "project",
                false,
                0,
                config,
                &mut scan,
            )?;
        }
        walk(
            &home.join(".agents/skills"),
            "agents",
            false,
            0,
            config,
            &mut scan,
        )?;
    }
    prefer_native_impeccable_skills(&mut scan.skills);
    if scan.visited >= 6000 || scan.skills.len() >= 512 {
        scan.warnings
            .push("Limite de descoberta atingido (512 skills).".into());
    }
    scan.skills.sort_by(|a, b| {
        a.name
            .to_lowercase()
            .cmp(&b.name.to_lowercase())
            .then(a.id.cmp(&b.id))
    });
    Ok((scan.skills, scan.warnings, scan.watched))
}

fn native_impeccable(directory: &Path, config: &Config, scan: &mut Scan) -> Result<(), SkillError> {
    let start = scan.skills.len();
    walk(directory, "jarvis", true, 0, config, scan)?;
    for skill in &mut scan.skills[start..] {
        skill.source = Some("impeccable-core".into());
    }
    Ok(())
}

fn prefer_native_impeccable_skills(skills: &mut Vec<Skill>) {
    if skills
        .iter()
        .any(|skill| skill.source.as_deref() == Some("impeccable-core") && skill.enabled)
    {
        skills.retain(|skill| skill.origin != "plugin" || !skill.name.ends_with(":impeccable"));
    }
}
pub(super) fn resource(skill: &Skill, relative: &str) -> Result<String, SkillError> {
    let path = Path::new(relative);
    if relative.contains('\\')
        || path.is_absolute()
        || path
            .components()
            .any(|c| !matches!(c, Component::Normal(_) | Component::CurDir))
    {
        return Err(error("Referência fora da pasta da skill."));
    }
    let target = if relative == "SKILL.md" {
        skill.file.clone()
    } else {
        skill.path.join(path)
    };
    let target = target.canonicalize()?;
    if !target.starts_with(&skill.path) {
        return Err(error("Referência fora da pasta da skill."));
    }
    text(&target)
}
pub(super) fn files(root: &Path) -> Result<Vec<String>, SkillError> {
    fn visit(
        root: &Path,
        dir: &Path,
        result: &mut Vec<String>,
        depth: usize,
    ) -> Result<(), SkillError> {
        if depth > 5 || result.len() >= 100 {
            return Ok(());
        }
        let mut entries: Vec<_> = fs::read_dir(dir)?.filter_map(Result::ok).collect();
        entries.sort_by_key(|e| e.file_name());
        for entry in entries {
            if result.len() >= 100 {
                break;
            }
            if entry.file_name().to_string_lossy().starts_with('.') {
                continue;
            }
            let meta = entry.file_type()?;
            if meta.is_symlink() {
                continue;
            }
            if meta.is_dir() {
                visit(root, &entry.path(), result, depth + 1)?;
            } else if meta.is_file() {
                result.push(
                    entry
                        .path()
                        .strip_prefix(root)
                        .map_err(|_| error("Caminho inválido."))?
                        .to_string_lossy()
                        .into_owned(),
                );
            }
        }
        Ok(())
    }
    let mut result = Vec::new();
    visit(root, root, &mut result, 0)?;
    Ok(result)
}
pub(super) fn detail(skill: &Skill) -> Result<Detail, SkillError> {
    Ok(Detail {
        name: skill.name.clone(),
        description: skill.description.clone(),
        content: resource(skill, "SKILL.md")?,
        path: Some(skill.path.clone()),
        source: skill.source.clone(),
        files: files(&skill.path)?,
    })
}
pub(super) fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod native_design_tests {
    use super::*;

    #[test]
    fn managed_native_skill_keeps_full_resources_and_deduplicates_only_equivalent_plugins() {
        let directory = tempfile::tempdir().unwrap();
        fs::create_dir_all(directory.path().join("reference")).unwrap();
        fs::write(directory.path().join("SKILL.md"), "---\nname: impeccable\ndescription: Professional product UI and design workflow\n---\nUse the full workflow").unwrap();
        fs::write(
            directory.path().join("reference/craft-floor.md"),
            "The complete edit quality floor",
        )
        .unwrap();
        let mut scan = Scan {
            seen: Default::default(),
            visited: 0,
            skills: vec![],
            warnings: vec![],
            watched: vec![],
        };
        native_impeccable(directory.path(), &Config::default(), &mut scan).unwrap();
        let native = scan.skills[0].clone();
        assert!(native.managed && native.enabled && native.automatic);
        assert_eq!(native.source.as_deref(), Some("impeccable-core"));
        assert_eq!(
            resource(&native, "reference/craft-floor.md").unwrap(),
            "The complete edit quality floor"
        );
        assert!(files(&native.path)
            .unwrap()
            .contains(&"reference/craft-floor.md".into()));
        let mut equivalent = native.clone();
        equivalent.origin = "plugin".into();
        equivalent.name = "impeccable:impeccable".into();
        equivalent.source = Some("impeccable@local".into());
        let mut other = equivalent.clone();
        other.name = "impeccable:local-helper".into();
        let mut personal = equivalent.clone();
        personal.origin = "agents".into();
        let mut skills = vec![native.clone(), equivalent.clone(), other, personal];
        prefer_native_impeccable_skills(&mut skills);
        assert_eq!(skills.len(), 3);
        assert!(skills
            .iter()
            .any(|skill| skill.name == "impeccable:local-helper"));
        assert!(skills.iter().any(|skill| skill.origin == "agents"));
        let mut disabled = native;
        disabled.enabled = false;
        let mut skills = vec![disabled, equivalent];
        prefer_native_impeccable_skills(&mut skills);
        assert_eq!(skills.len(), 2);
        let config = Config {
            disabled: [id(&directory.path().canonicalize().unwrap())].into(),
            ..Config::default()
        };
        let mut scan = Scan {
            seen: Default::default(),
            visited: 0,
            skills: vec![],
            warnings: vec![],
            watched: vec![],
        };
        native_impeccable(directory.path(), &config, &mut scan).unwrap();
        assert!(!scan.skills[0].enabled);
    }
}
