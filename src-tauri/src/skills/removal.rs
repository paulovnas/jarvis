//! Reviewed deletion of user-owned skills, including shared links without their targets.
use super::{error, find, installation::MAX_BATCH, removal_target, snapshot, store, SkillError};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    fs,
    io::Read,
    path::{Path, PathBuf},
};

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Selection {
    pub id: String,
    pub reason: String,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct Preview {
    pub id: String,
    pub name: String,
    pub description: String,
    pub origin: String,
    #[serde(serialize_with = "crate::library::serialize_display_path_buf")]
    pub path: PathBuf,
    pub linked: bool,
    pub reason: String,
    pub files: Vec<String>,
    pub digest: String,
}

#[derive(Clone)]
pub(crate) struct Prepared {
    pub previews: Vec<Preview>,
    project: Option<PathBuf>,
}

fn cancelled() -> SkillError {
    SkillError {
        code: "cancelled",
        message: "Remoção de skills cancelada.".into(),
    }
}

fn stale() -> SkillError {
    SkillError {
        code: "skill_removal_stale",
        message: "A skill ou seus arquivos mudaram durante a revisão. Prepare uma nova proposta."
            .into(),
    }
}

// Include hidden files, metadata and empty folders; never follow nested links.
fn fingerprint(
    target: &Path,
    cancelled: &impl Fn() -> bool,
) -> Result<(Vec<String>, String), SkillError> {
    fn walk(
        base: &Path,
        path: &Path,
        hash: &mut Sha256,
        files: &mut Vec<String>,
        budget: &mut (u64, usize),
        depth: usize,
        cancelled: &impl Fn() -> bool,
    ) -> Result<(), SkillError> {
        if cancelled() {
            return Err(self::cancelled());
        }
        budget.1 += 1;
        if depth > 16 || budget.1 > store::MAX_PACKAGE_FILES + 1 {
            return Err(error(
                "A skill possui arquivos ou pastas demais para revisar a remoção pelo chat.",
            ));
        }
        let meta = fs::symlink_metadata(path)?;
        let relative = path
            .strip_prefix(base)
            .map_err(|_| error("Caminho de skill inválido."))?
            .to_string_lossy()
            .into_owned();
        hash.update((relative.len() as u64).to_le_bytes());
        hash.update(relative.as_bytes());
        hash.update([u8::from(meta.permissions().readonly())]);
        if meta.is_symlink() {
            hash.update(b"link");
            hash.update(fs::read_link(path)?.as_os_str().as_encoded_bytes());
            if !relative.is_empty() {
                files.push(format!("{relative} (vínculo)"));
            }
        } else if meta.is_dir() {
            hash.update(b"dir");
            let mut entries: Vec<_> = fs::read_dir(path)?
                .take(store::MAX_PACKAGE_FILES + 1)
                .collect::<Result<_, _>>()?;
            if entries.len() > store::MAX_PACKAGE_FILES + 1 - budget.1 {
                return Err(error(
                    "A skill possui arquivos ou pastas demais para revisar a remoção pelo chat.",
                ));
            }
            entries.sort_by_key(|e| e.file_name());
            for entry in entries {
                walk(
                    base,
                    &entry.path(),
                    hash,
                    files,
                    budget,
                    depth + 1,
                    cancelled,
                )?;
            }
        } else if meta.is_file() {
            hash.update(b"file");
            let remaining = store::MAX_PACKAGE_BYTES.saturating_sub(budget.0);
            if meta.len() > remaining {
                return Err(error(
                    "A skill excede o limite de 32 MiB para revisar a remoção pelo chat.",
                ));
            }
            let mut data = Vec::new();
            fs::File::open(path)?
                .take(remaining + 1)
                .read_to_end(&mut data)?;
            budget.0 += data.len() as u64;
            if budget.0 > store::MAX_PACKAGE_BYTES {
                return Err(error("A skill mudou ou excedeu o limite de revisão."));
            }
            hash.update((data.len() as u64).to_le_bytes());
            hash.update(&data);
            files.push(relative);
        } else {
            return Err(error(
                "A skill contém um arquivo especial que não pode ser removido pelo chat.",
            ));
        }
        Ok(())
    }
    let mut hash = Sha256::new();
    let mut files = Vec::new();
    walk(
        target,
        target,
        &mut hash,
        &mut files,
        &mut (0, 0),
        0,
        cancelled,
    )?;
    Ok((files, format!("{:x}", hash.finalize())))
}

pub(crate) fn list(
    home: &Path,
    project: Option<&Path>,
    query: &str,
    offset: usize,
) -> Result<Value, SkillError> {
    if query.len() > 160 || query.chars().any(char::is_control) || offset > 580 {
        return Err(error("Busca de skills instaladas inválida."));
    }
    let query = query.to_lowercase();
    let snapshot = snapshot(home, project)?;
    let matched: Vec<_> = snapshot
        .skills
        .into_iter()
        .filter(|s| {
            format!("{} {}", s.name, s.description)
                .to_lowercase()
                .contains(&query)
        })
        .collect();
    let entries: Vec<_> = matched.iter().skip(offset).take(20).map(|s| {
        let permission = removal_target(home, project, s);
        json!({"id":s.id,"name":s.name,"description":s.description,"origin":s.origin,"path":s.removal_path,"linked":s.linked,"enabled":s.enabled,"managed":s.managed,"removable":permission.is_ok(),"removalBlockedReason":permission.err().map(|e| e.message)})
    }).collect();
    Ok(
        json!({"total":matched.len(),"skills":entries,"nextOffset":(offset + 20 < matched.len()).then_some(offset + 20),"warnings":snapshot.warnings,"next":"Use exact installed IDs and per-skill reasons in ONE jarvis_propose_skills call with action=remove. Native and plugin-owned skills cannot be removed here; manage plugin components through their plugin. Shared agents skills affect other apps too. A linked skill removes only its local link, preserving the original directory. Native approval is always required."}),
    )
}

pub(crate) fn prepare(
    home: &Path,
    project: Option<&Path>,
    selections: Vec<Selection>,
    cancelled: impl Fn() -> bool,
) -> Result<Prepared, SkillError> {
    if selections.is_empty() || selections.len() > MAX_BATCH {
        return Err(error("Selecione de 1 a 20 skills para remover."));
    }
    let mut seen = BTreeSet::new();
    for s in &selections {
        if s.id.len() != 64
            || !s
                .id
                .bytes()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
            || !seen.insert(&s.id)
            || s.reason.trim().is_empty()
            || s.reason.len() > 1000
            || s.reason.chars().any(|c| c.is_control() && c != '\n')
        {
            return Err(error(
                "Informe IDs únicos de skills instaladas e um motivo válido para cada remoção.",
            ));
        }
    }
    let _guard = super::lock()?;
    let mut previews = Vec::new();
    for selection in selections {
        if cancelled() {
            return Err(self::cancelled());
        }
        let skill = find(home, project, &selection.id)?;
        let path = removal_target(home, project, &skill)?;
        let (files, digest) = fingerprint(&path, &cancelled)?;
        previews.push(Preview {
            id: skill.id,
            name: skill.name,
            description: skill.description,
            origin: skill.origin,
            path,
            linked: skill.linked,
            reason: selection.reason.trim().into(),
            files,
            digest,
        });
    }
    Ok(Prepared {
        previews,
        project: project.map(Path::to_path_buf),
    })
}

pub(crate) fn apply(
    home: &Path,
    prepared: &Prepared,
    cancelled: impl Fn() -> bool,
) -> Result<Value, SkillError> {
    let _guard = super::lock()?;
    let mut results = Vec::new();
    let mut removed = Vec::new();
    for reviewed in &prepared.previews {
        let mut result = json!({"id":reviewed.id,"name":reviewed.name,"path":crate::library::strip_verbatim(&reviewed.path.to_string_lossy()),"linked":reviewed.linked});
        let mut deletion_started = false;
        let deletion = (|| {
            if cancelled() {
                return Err(self::cancelled());
            }
            let current =
                find(home, prepared.project.as_deref(), &reviewed.id).map_err(|_| stale())?;
            let target = removal_target(home, prepared.project.as_deref(), &current)?;
            if target != reviewed.path
                || current.origin != reviewed.origin
                || current.linked != reviewed.linked
                || fingerprint(&target, &cancelled)?.1 != reviewed.digest
            {
                return Err(stale());
            }
            if cancelled() {
                return Err(self::cancelled());
            }
            deletion_started = true;
            super::delete_skill_files(&target)
        })();
        match deletion {
            Ok(()) => {
                result["status"] = json!("removed");
                removed.push(json!({"id":reviewed.id,"name":reviewed.name}));
                match super::cleanup_removed_skill(
                    home,
                    prepared.project.as_deref(),
                    &reviewed.id,
                    reviewed.linked,
                ) {
                    Ok(remaining) => {
                        result["stillRegistered"] = json!(remaining.is_some());
                        if let Some(skill) = remaining {
                            result["remainingPath"] = json!(crate::library::strip_verbatim(
                                &skill.removal_path.to_string_lossy()
                            ));
                        }
                    }
                    Err(cause) => {
                        result["warning"] = json!({"code":cause.code,"message":cause.message});
                    }
                }
            }
            Err(cause) if deletion_started => {
                result["status"] = json!("incomplete");
                result["error"] = json!({"code":"skill_removal_incomplete","message":"A remoção foi iniciada, mas não pôde ser concluída. Alguns arquivos podem ter sido excluídos. Confira o caminho informado antes de tentar novamente.","cause":{"code":cause.code,"message":cause.message}});
            }
            Err(cause) => {
                result["status"] = json!(if cause.code == "cancelled" {
                    "cancelled"
                } else {
                    "failed"
                });
                result["error"] = json!({"code":cause.code,"message":cause.message});
            }
        }
        results.push(result);
    }
    let status = if removed.len() == prepared.previews.len() {
        "applied"
    } else if !removed.is_empty() || results.iter().any(|r| r["status"] == "incomplete") {
        "partial"
    } else if results.iter().all(|r| r["status"] == "cancelled") {
        "cancelled"
    } else {
        "failed"
    };
    Ok(
        json!({"approved":true,"status":status,"kind":"skills","action":"remove","removed":removed,"results":results,"next":"Use per-item results to determine which reviewed paths were removed; the skill catalog refreshes on the next model step in this same conversation. stillRegistered=true means another path in this conversation still exposes this skill; report remainingPath and obtain a new review before changing it. Other projects may also have links to the same skill. If catalog verification warned, inspect the refreshed catalog before claiming full removal. Already loaded text cannot be erased from history; do not use removed skill guidance. An incomplete result means deletion started and some files may already be gone; report its exact residual path and do not use that skill until inspected or repaired. Preserve confirmed removals and report each failure or cancellation. Do not automatically repeat an uncertain deletion. Stale files require a new review."}),
    )
}

#[cfg(test)]
mod tests;
