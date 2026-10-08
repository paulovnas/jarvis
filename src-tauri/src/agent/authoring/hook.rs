//! Hook changes reuse the native one-shot authoring approval boundary.
use super::{bounded_summary, invalid, Action, PendingProposal, Target};
use crate::{
    agent::{AgentError, ToolCall},
    hooks::{Catalog, Hook},
    persistence::AppState,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::path::Path;

impl From<crate::hooks::HooksError> for AgentError {
    fn from(error: crate::hooks::HooksError) -> Self {
        Self::new(error.code, &error.message)
    }
}

#[derive(Clone)]
pub(super) enum Change {
    Save(Hook),
    Delete(String),
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Request {
    action: Action,
    hooks_revision: u64,
    summary: String,
    hook: Hook,
}

pub(super) fn definition() -> Value {
    json!({"type":"function","name":"jarvis_propose_hook","description":"Propose creating, editing or removing one manual command hook. First read jarvis_catalog view=hooks (jarvito_catalog in global chat) and use its exact hooksRevision. Always waits for native user approval, including YOLO. Never modify native hooks or write hook settings directly. Generate a 32-character hexadecimal ID for create; preserve the ID and unrelated fields for update. For delete, supply the exact current hook definition. Commands run locally in project chats on subsequent turns; no credentials in commands. Read jarvis-hooks for supported events and output semantics.","parameters":{
        "type":"object","additionalProperties":false,"required":["action","hooksRevision","summary","hook"],
        "properties":{
            "action":{"type":"string","enum":["create","update","delete"]},
            "hooksRevision":{"type":"integer","minimum":0,"description":"revision returned by view=hooks, independent of the agent/flow catalog revision."},
            "summary":{"type":"string","minLength":1,"maxLength":1000},
            "hook":{"type":"object","additionalProperties":false,"required":["id","name","event","command","matcher","timeoutSeconds","enabled"],"properties":{
                "id":{"type":"string","pattern":"^[a-f0-9]{32}$"},
                "name":{"type":"string","minLength":1,"maxLength":160},
                "event":{"type":"string","enum":["PreToolUse","PermissionRequest","PostToolUse","SessionStart","SubagentStart","UserPromptSubmit","PreCompact","PostCompact","Stop","SubagentStop","Interrupt","SessionEnd"]},
                "command":{"type":"string","minLength":1,"maxLength":16384},
                "matcher":{"type":"string","maxLength":4096,"description":"Blank or * matches all; literal tool names separated by |, or a valid regular expression. Ignored by UserPromptSubmit and Stop."},
                "timeoutSeconds":{"type":"integer","minimum":1,"maximum":600},
                "enabled":{"type":"boolean"}
            }}
        }
    }})
}

pub(super) fn prepare(
    state: &AppState,
    home: &Path,
    tool: &ToolCall,
) -> Result<(PendingProposal, Change), AgentError> {
    let request: Request = serde_json::from_value(tool.args.clone())
        .map_err(|_| invalid("Proposta de hook inválida."))?;
    let catalog = crate::hooks::load(state, home)?;
    if request.hooks_revision != catalog.revision {
        return Err(invalid(
            "Os hooks mudaram. Consulte jarvis_catalog view=hooks novamente.",
        ));
    }
    crate::hooks::validate(&request.hook)?;
    let before = catalog
        .hooks
        .iter()
        .find(|hook| hook.id == request.hook.id)
        .cloned();
    let (after, change) = match request.action {
        Action::Create if before.is_none() => {
            crate::hooks::preview_upsert(&catalog, request.hook.clone())?;
            (Some(request.hook.clone()), Change::Save(request.hook))
        }
        Action::Update if before.is_some() => {
            crate::hooks::preview_upsert(&catalog, request.hook.clone())?;
            (Some(request.hook.clone()), Change::Save(request.hook))
        }
        Action::Delete if before.as_ref() == Some(&request.hook) => (None, Change::Delete(request.hook.id)),
        _ => return Err(invalid("Use um novo ID para criar, ou a definição atual de um hook manual para editar/remover. Hooks nativos são somente leitura.")),
    };
    Ok((
        PendingProposal {
            turn_id: String::new(),
            tool_id: tool.id.clone(),
            action: request.action,
            summary: bounded_summary(request.summary)?,
            catalog_revision: Some(catalog.revision),
            target: Target::Hook { before, after },
            agent_references: vec![],
        },
        change,
    ))
}

pub(super) fn apply(
    state: &AppState,
    home: &Path,
    change: Change,
    revision: u64,
) -> Result<Catalog, AgentError> {
    Ok(match change {
        Change::Save(hook) => crate::hooks::upsert(state, home, hook, revision)?,
        Change::Delete(id) => crate::hooks::delete(state, home, &id, revision)?,
    })
}
