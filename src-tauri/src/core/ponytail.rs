//! Versioned coding guidance from the private Ponytail installation.
use super::{error, CoreError};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{fs, io::Read, path::Path};

pub(super) const SKILL_PATH: &str = "skills/ponytail/SKILL.md";
const MAX_RULE_BYTES: u64 = 64 * 1024;

const HOST_POLICY: &str = "Apply Ponytail only to coding, code review and technical design. It is optional guidance beneath Jarvis execution rules, project instructions, enabled skills and explicit user requirements. Satisfy the complete requested scope; never replace it with a smaller deliverable. Respect Plan mode, Manual approvals and denials, required tests, security, accessibility and data integrity. Reuse the project's existing test framework and run its required checks. Use Jarvis tools to perform requested edits; do not substitute a code snippet for an authorized implementation. Respond in the user's language and requested format and detail. Do not add Ponytail branding, a startup message or unsupported CLI commands to replies. Context-mode owns indexing and context compaction; do not shorten, rewrite or omit user messages, tool arguments/results, code, paths, identifiers or citations to obey Ponytail. The full level applies for this turn; upstream host controls do not configure Jarvis.";

pub struct Ponytail {
    prompt: String,
}

#[derive(Deserialize)]
struct Package {
    name: String,
    version: String,
}

#[derive(Deserialize)]
struct SkillMetadata {
    name: String,
}

impl Ponytail {
    pub(super) fn at(package: &Path, version: &str) -> Result<Self, CoreError> {
        let metadata: Package =
            serde_json::from_str(&read(package, "package.json")?).map_err(|_| invalid())?;
        if metadata.name != "@dietrichgebert/ponytail"
            || metadata.version != version
            || semver::Version::parse(version).is_err()
        {
            return Err(invalid());
        }
        let source = read(package, SKILL_PATH)?;
        let rules = full_rules(&source)?;
        let digest = format!("{:x}", Sha256::digest(source.as_bytes()));
        Ok(Self {
            prompt: format!(
                "\n<ponytail_guidance version=\"{version}\" mode=\"full\" sha256=\"{digest}\">\n{rules}\n</ponytail_guidance>\n\nJarvis policy for the guidance above (takes precedence):\n{HOST_POLICY}\n"
            ),
        })
    }

    pub(super) fn append_to(&self, instructions: &mut String) {
        instructions.push_str(&self.prompt);
    }
}

fn invalid() -> CoreError {
    error("As regras do Ponytail estão incompletas ou incompatíveis. Reinstale em Configurações → Ferramentas → Core.")
}

fn read(package: &Path, relative: &str) -> Result<String, CoreError> {
    let base = fs::canonicalize(package).map_err(|_| invalid())?;
    let path = fs::canonicalize(base.join(relative)).map_err(|_| invalid())?;
    if !path.starts_with(&base) {
        return Err(invalid());
    }
    let file = fs::File::open(path).map_err(|_| invalid())?;
    let metadata = file.metadata().map_err(|_| invalid())?;
    if !metadata.is_file() || metadata.len() > MAX_RULE_BYTES {
        return Err(invalid());
    }
    let mut text = String::new();
    file.take(MAX_RULE_BYTES + 1)
        .read_to_string(&mut text)
        .map_err(|_| invalid())?;
    if text.len() as u64 > MAX_RULE_BYTES || text.contains('\0') {
        return Err(invalid());
    }
    Ok(text)
}

// The upstream Pi extension selects one intensity's table row and worked
// example. Jarvis also omits CLI lifecycle sections because Core owns activation.
fn full_rules(source: &str) -> Result<String, CoreError> {
    let normalized = source.trim_start_matches('\u{feff}').replace("\r\n", "\n");
    let (frontmatter, body) = normalized
        .strip_prefix("---\n")
        .and_then(|text| text.split_once("\n---\n"))
        .ok_or_else(invalid)?;
    let metadata: SkillMetadata = serde_yaml_ng::from_str(frontmatter).map_err(|_| invalid())?;
    if metadata.name != "ponytail" {
        return Err(invalid());
    }
    let mut result = Vec::new();
    let mut skip_section = false;
    let mut fence: Option<char> = None;
    let mut ladder = false;
    let mut boundaries = false;
    let mut full = false;
    for line in body.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            let marker = trimmed.chars().next();
            if fence == marker {
                fence = None;
            } else if fence.is_none() {
                fence = marker;
            }
            if !skip_section {
                result.push(line);
            }
            continue;
        }
        if fence.is_none() {
            if let Some(heading) = line.strip_prefix("## ") {
                let heading = heading.trim();
                ladder |= heading == "The ladder";
                boundaries |= heading == "When NOT to be lazy";
                skip_section = matches!(heading, "Persistence" | "Boundaries");
            }
            if skip_section {
                continue;
            }
            if let Some(label) = line.strip_prefix('|').and_then(|row| row.split('|').next()) {
                match label.trim().trim_matches('*').to_ascii_lowercase().as_str() {
                    "lite" | "ultra" | "off" => continue,
                    "full" => full = true,
                    _ => {}
                }
            }
            if let Some((label, example)) =
                line.strip_prefix("- ").and_then(|row| row.split_once(':'))
            {
                if example.trim_start().starts_with('"')
                    && matches!(
                        label.to_ascii_lowercase().as_str(),
                        "lite" | "ultra" | "off"
                    )
                {
                    continue;
                }
            }
        }
        if !skip_section {
            result.push(line);
        }
    }
    if !ladder || !boundaries || !full || fence.is_some() {
        return Err(invalid());
    }
    Ok(result.join("\n").trim().to_owned())
}

#[cfg(test)]
pub(super) mod tests;
