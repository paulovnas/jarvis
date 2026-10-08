//! Materialized plugin proposals reuse the one-shot native approval boundary.
use super::{bounded_summary, invalid, Action, PendingProposal, Target};
use crate::{
    agent::{AgentError, ToolCall},
    plugins::{self, Operation, Prepared},
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::path::Path;

impl From<plugins::PluginsError> for AgentError {
    fn from(error: plugins::PluginsError) -> Self {
        Self::new(error.code, &error.message)
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Request {
    plugins_revision: u64,
    summary: String,
    operation: Operation,
}

pub(super) fn definition() -> Value {
    json!({"type":"function","name":"jarvis_propose_plugin","description":"Prepare one Codex-compatible plugin or marketplace change for mandatory native user approval. First read jarvis_catalog view=plugins (jarvito_catalog in global chat) and use its exact pluginsRevision. Supports marketplace sources, install/update/import/create, enable/component settings and separate hook trust. Materializes and validates the package before review without executing plugin commands. Never include credentials; use plugin-scoped authentication in Settings. A successful install is not proof that external services are available. Read jarvis-plugins for requirements and native conflicts.","parameters":{
        "type":"object","additionalProperties":false,"required":["pluginsRevision","summary","operation"],
        "properties":{
            "pluginsRevision":{"type":"integer","minimum":0},
            "summary":{"type":"string","minLength":1,"maxLength":1000},
            "operation":plugins::operation_schema()
        }
    }})
}

pub(super) async fn prepare(
    home: &Path,
    tool: &ToolCall,
) -> Result<(PendingProposal, Prepared), AgentError> {
    let request: Request = serde_json::from_value(tool.args.clone())
        .map_err(|_| invalid("Proposta de plugin inválida."))?;
    let summary = bounded_summary(request.summary)?;
    plugins::validate_authoring_operation(&request.operation)?;
    let action = match &request.operation {
        Operation::Uninstall { .. } | Operation::RemoveMarketplace { .. } => Action::Delete,
        Operation::Create { .. }
        | Operation::Install { .. }
        | Operation::Import { .. }
        | Operation::AddMarketplace { .. } => Action::Create,
        _ => Action::Update,
    };
    let prepared = plugins::preview(home, request.plugins_revision, request.operation).await?;
    Ok((
        PendingProposal {
            turn_id: String::new(),
            tool_id: tool.id.clone(),
            action,
            summary,
            catalog_revision: Some(request.plugins_revision),
            target: Target::Plugin {
                preview: prepared.preview.clone(),
            },
            agent_references: vec![],
        },
        prepared,
    ))
}

#[cfg(test)]
#[path = "plugin_tests.rs"]
mod integration_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn malformed_or_stale_changes_never_create_a_pending_proposal() {
        let home = tempfile::tempdir().unwrap();
        let tool = ToolCall {
            id: "plugin-change".into(),
            name: "jarvis_propose_plugin".into(),
            args: json!({"pluginsRevision":u64::MAX,"summary":"Instalar plugin","operation":{"action":"uninstall","pluginId":"unknown"}}),
            status: "running".into(),
            output: String::new(),
            duration_ms: 0,
        };
        assert!(prepare(home.path(), &tool).await.is_err());
        assert!(plugins::catalog(home.path()).unwrap().installed.is_empty());
        let invalid = ToolCall {
            args: json!({"pluginsRevision":0,"summary":"Instalar","operation":{"action":"invented"}}),
            ..tool
        };
        assert!(prepare(home.path(), &invalid).await.is_err());
    }
}
