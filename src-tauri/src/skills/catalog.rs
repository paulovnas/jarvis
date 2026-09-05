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
}
fn walk(
    path: &Path,
    origin: &str,
    depth: usize,
    config: &Config,
    scan: &mut Scan,
) -> Result<(), SkillError> {
    if depth > 6 || scan.visited >= 6000 || scan.skills.len() >= 512 {
        return Ok(());
    }
    scan.visited += 1;
    let canonical = match path.canonicalize() {
        Ok(path) => path,
        Err(_) => return Ok(()),
    };
    if !canonical.is_dir() || !scan.seen.insert(canonical.clone()) {
        return Ok(());
    }
    if let Some(file) = skill_file(&canonical) {
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
        walk(&entry.path(), origin, depth + 1, config, scan)?;
    }
    Ok(())
}
pub(super) fn discover(
    home: &Path,
    project: Option<&Path>,
    config: &Config,
) -> Result<(Vec<Skill>, Vec<String>), SkillError> {
    let own = root(home).join("skills");
    fs::create_dir_all(&own)?;
    let mut scan = Scan {
        seen: BTreeSet::new(),
        visited: 0,
        skills: Vec::new(),
        warnings: Vec::new(),
    };
    walk(&own, "jarvis", 0, config, &mut scan)?;
    if config.include_agents {
        if let Some(project) = project {
            walk(
                &project.join(".agents/skills"),
                "project",
                0,
                config,
                &mut scan,
            )?;
        }
        walk(&home.join(".agents/skills"), "agents", 0, config, &mut scan)?;
    }
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
    Ok((scan.skills, scan.warnings))
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
