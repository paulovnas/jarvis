//! Project guidance changes preserve the file outside one reviewed Jarvis section.
use super::{bounded_summary, invalid, Action, PendingProposal, Target};
use crate::agent::{instructions::MAX_INSTRUCTION_FILE, tools, AgentError, ToolCall};
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{fs, io::Write, ops::Range, path::Path, sync::Mutex};

const PATH: &str = "AGENTS.md";
const BEGIN: &str = "<!-- BEGIN JARVIS PROJECT INSTRUCTIONS -->";
const END: &str = "<!-- END JARVIS PROJECT INSTRUCTIONS -->";
const MAX_DRAFT: usize = 10_000;
const MAX_INSTRUCTIONS: usize = 24_000;
// ponytail: serialize rare native saves; per-project locks if approval throughput matters.
static SAVES: Mutex<()> = Mutex::new(());

#[derive(Debug, Clone)]
pub(super) struct Change {
    revision: String,
    before: Option<String>,
    after: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Request {
    revision: String,
    summary: String,
    content: String,
}

pub(super) fn definition() -> Value {
    json!({"type":"function","name":"jarvis_propose_project_instructions","description":"Propose creating or updating only Jarvis's section in the current project's root AGENTS.md. First read jarvis_catalog view=project_instructions and use its exact revision. Inspect project conventions and real validation commands; draft short grounded Markdown rather than copying generic rules. Supply only the section content, without managed markers. All user text, other managed sections and nested/global files remain untouched. Always waits for explicit native approval, including YOLO. Stale file changes are rejected; re-read instead of repeating an uncertain save. Read jarvis-authoring for guidance.","parameters":{
        "type":"object","additionalProperties":false,"required":["revision","summary","content"],
        "properties":{
            "revision":{"type":"string","minLength":1,"maxLength":64,"description":"Exact SHA256 revision or missing returned by view=project_instructions."},
            "summary":{"type":"string","minLength":1,"maxLength":1000},
            "content":{"type":"string","minLength":1,"maxLength":MAX_DRAFT,"description":"Only the proposed Jarvis section, grounded in inspected project files. Preserve existing rules; no managed markers or credentials."}
        }
    }})
}

fn source(root: &Path) -> Result<Option<String>, AgentError> {
    let path = tools::scoped(root, PATH, true)?;
    match fs::symlink_metadata(&path) {
        Ok(metadata) if metadata.len() > MAX_INSTRUCTION_FILE => Err(source_too_large()),
        Ok(_) => {
            let text = tools::read_text(&path)?;
            if text.len() as u64 > MAX_INSTRUCTION_FILE {
                return Err(source_too_large());
            }
            Ok(Some(text))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(_) => Err(AgentError::storage()),
    }
}

fn source_too_large() -> AgentError {
    AgentError::new("project_instructions_too_large", "AGENTS.md excede o limite de 64 KiB por arquivo de instruções do Jarvis. Preserve o conteúdo e divida as regras por escopo antes de propor uma atualização.")
}

fn revision(content: Option<&str>) -> String {
    content.map_or_else(
        || "missing".into(),
        |text| format!("{:x}", Sha256::digest(text.as_bytes())),
    )
}

fn managed_depth(text: &str) -> usize {
    text.lines().fold(0usize, |depth, line| {
        let line = line.trim();
        if line.starts_with("<!-- BEGIN") {
            depth + 1
        } else if line.starts_with("<!-- END") {
            depth.saturating_sub(1)
        } else {
            depth
        }
    })
}

fn section(text: &str) -> Result<Option<Range<usize>>, AgentError> {
    let begins: Vec<_> = text.match_indices(BEGIN).map(|(index, _)| index).collect();
    let ends: Vec<_> = text.match_indices(END).map(|(index, _)| index).collect();
    let (start, end) = match (begins.as_slice(), ends.as_slice()) {
        ([], []) => {
            if managed_depth(text) != 0 {
                return Err(invalid("Há uma seção gerenciada sem fechamento em AGENTS.md. Preserve o arquivo e corrija os marcadores antes de propor uma inclusão."));
            }
            return Ok(None);
        }
        ([start], [end]) if end > start => (*start, *end),
        _ => return Err(invalid("Os marcadores da seção Jarvis estão duplicados ou incompletos. Preserve o arquivo e revise os marcadores antes de tentar novamente.")),
    };
    let finish = end + END.len();
    let standalone = (start == 0 || text[..start].ends_with('\n'))
        && text[start + BEGIN.len()..].starts_with(['\r', '\n'])
        && text[..end].ends_with('\n')
        && (finish == text.len() || text[finish..].starts_with(['\r', '\n']));
    let body = &text[start + BEGIN.len()..end];
    if !standalone
        || managed_depth(&text[..start]) != 0
        || body.contains("<!-- BEGIN")
        || body.contains("<!-- END")
    {
        return Err(invalid("A seção Jarvis não pode estar dentro de outra seção gerenciada nem conter seus marcadores. Preserve as regras existentes."));
    }
    Ok(Some(start..finish))
}

pub(super) fn catalog(root: &Path) -> Result<Value, AgentError> {
    let content = source(root)?;
    let range = section(content.as_deref().unwrap_or_default())?;
    let editable = range.map(|range| {
        content.as_deref().unwrap_or_default()[range.start + BEGIN.len()..range.end - END.len()]
            .trim()
            .to_owned()
    });
    Ok(json!({
        "path":PATH,"exists":content.is_some(),"revision":revision(content.as_deref()),
        "content":content,"editableContent":editable,"requiresNativeApproval":true,
        "authoringTool":"jarvis_propose_project_instructions",
        "rules":["Only Jarvis's own section may change; preserve all other text.","Inspect project conventions and confirmed validation commands before drafting.","Nested and global instruction files are outside this proposal.","Explicit native approval is required, including YOLO."]
    }))
}

pub(super) fn prepare(
    root: &Path,
    tool: &ToolCall,
) -> Result<(PendingProposal, Change), AgentError> {
    let request: Request = serde_json::from_value(tool.args.clone())
        .map_err(|_| invalid("Proposta de instruções do projeto inválida."))?;
    let summary = bounded_summary(request.summary)?;
    let draft = request.content.trim();
    if draft.is_empty()
        || draft.chars().count() > MAX_DRAFT
        || draft.contains('\0')
        || draft.contains("<!-- BEGIN")
        || draft.contains("<!-- END")
    {
        return Err(invalid(
            "Escreva uma seção com até 10.000 caracteres, sem marcadores de seções gerenciadas.",
        ));
    }
    let before = source(root)?;
    if request.revision != revision(before.as_deref()) {
        return Err(stale());
    }
    let text = before.as_deref().unwrap_or_default();
    let range = section(text)?;
    let newline = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let draft = draft.replace("\r\n", "\n").replace('\n', newline);
    let block = format!("{BEGIN}{newline}{draft}{newline}{END}");
    let after = if let Some(range) = range {
        format!("{}{block}{}", &text[..range.start], &text[range.end..])
    } else {
        let separator = if text.is_empty() {
            String::new()
        } else if text.ends_with('\n') {
            newline.to_owned()
        } else {
            newline.repeat(2)
        };
        format!("{text}{separator}{block}{newline}")
    };
    if after.chars().count() > MAX_INSTRUCTIONS || after.len() as u64 > MAX_INSTRUCTION_FILE {
        return Err(invalid("A proposta deve manter AGENTS.md em até 24.000 caracteres e 64 KiB. Nenhuma regra existente será removida para abrir espaço."));
    }
    if before.as_deref() == Some(after.as_str()) {
        return Err(invalid(
            "A proposta não altera as instruções atuais do projeto.",
        ));
    }
    Ok((
        PendingProposal {
            turn_id: String::new(),
            tool_id: tool.id.clone(),
            action: if before.is_some() {
                Action::Update
            } else {
                Action::Create
            },
            summary,
            catalog_revision: None,
            target: Target::ProjectInstructions {
                path: PATH.into(),
                before: before.clone(),
                after: after.clone(),
            },
            agent_references: vec![],
        },
        Change {
            revision: request.revision,
            before,
            after,
        },
    ))
}

fn stale() -> AgentError {
    AgentError::new("stale_project_instructions", "AGENTS.md mudou desde a leitura ou revisão. Consulte jarvis_catalog view=project_instructions novamente e prepare uma nova proposta; o conteúdo atual foi preservado.")
}

pub(super) fn apply(root: &Path, change: Change) -> Result<String, AgentError> {
    let _save = SAVES.lock().map_err(|_| AgentError::internal())?;
    if revision(source(root)?.as_deref()) != change.revision {
        return Err(stale());
    }
    let path = tools::scoped(root, PATH, true)?;
    if change.before.is_some() {
        tools::write_atomic(&path, &change.after)?;
    } else {
        let mut file = tempfile::NamedTempFile::new_in(root).map_err(|_| AgentError::storage())?;
        file.write_all(change.after.as_bytes())
            .and_then(|()| file.as_file().sync_all())
            .map_err(|_| AgentError::storage())?;
        file.persist_noclobber(&path).map_err(|error| {
            if error.error.kind() == std::io::ErrorKind::AlreadyExists {
                stale()
            } else {
                AgentError::storage()
            }
        })?;
        #[cfg(unix)]
        fs::File::open(root).and_then(|directory| directory.sync_all()).map_err(|_| AgentError::new("project_instructions_sync_failed", "AGENTS.md foi salvo, mas a pasta não pôde ser sincronizada. Confira o estado atual antes de repetir a operação."))?;
    }
    Ok(revision(Some(&change.after)))
}

#[cfg(test)]
#[path = "project_instructions_tests.rs"]
mod tests;
