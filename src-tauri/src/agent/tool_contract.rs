//! The exact model-visible catalog is also the dispatch contract for this step.
use super::{AgentError, ToolCall};
use jsonschema::error::ValidationErrorKind;
use serde_json::{json, Value};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Effect {
    ReadOnly,
    Mutating,
    Interactive,
    Stateful,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ApprovalPolicy {
    Never,
    AccordingToTurn,
    /// Also prompts in Plan mode; YOLO still preauthorizes execution.
    Always,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Handler {
    Progress,
    PublicationInspection,
    Workflow,
    Design,
    DirectTasks,
    Beads,
    ProjectBeads,
    JarvisAuthoring,
    Context7,
    Lsp,
    Patch,
    ContextMode,
    AskUser,
    Mcp,
    WebSearch,
    Attachment,
    Vision,
    ImageGeneration,
    SkillRead,
    SkillSearch,
    Native,
}

impl Handler {
    fn native(name: &str) -> Self {
        if name == super::progress::TOOL_NAME {
            Self::Progress
        } else if name == "jarvis_inspect_publication" {
            Self::PublicationInspection
        } else if name.starts_with("hub_")
            || name.starts_with("process_")
            || name.starts_with("terminal_")
            || name.starts_with("browser_")
            || matches!(
                name,
                "workflow_check" | "design_brief" | "validation_publish" | "recovery_resolve"
            )
        {
            Self::Workflow
        } else if matches!(name, "design_search" | "design_read") {
            Self::Design
        } else if name == "update_tasks" {
            Self::DirectTasks
        } else if name.starts_with("project_beads_") {
            Self::ProjectBeads
        } else if name.starts_with("beads_") {
            Self::Beads
        } else if name.starts_with("jarvis_") {
            Self::JarvisAuthoring
        } else if name.starts_with("context7_") {
            Self::Context7
        } else if name.starts_with("lsp_") {
            Self::Lsp
        } else if name == "apply_patch" {
            Self::Patch
        } else if name.starts_with("ctx_") {
            Self::ContextMode
        } else if name == "ask_user" {
            Self::AskUser
        } else if name.starts_with("mcp_") {
            Self::Mcp
        } else if name == "web_search" {
            Self::WebSearch
        } else if name == "read_attachment" {
            Self::Attachment
        } else if name == "vision" {
            Self::Vision
        } else if name == "generate_image" {
            Self::ImageGeneration
        } else if name == "read_skill" {
            Self::SkillRead
        } else if name == "find_skills" {
            Self::SkillSearch
        } else {
            Self::Native
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Capabilities {
    pub effect: Effect,
    pub approval: ApprovalPolicy,
    pub parallel_safe: bool,
}

impl Capabilities {
    fn native(name: &str) -> Self {
        let read_only = matches!(
            name,
            "read"
                | "search"
                | "list"
                | "read_attachment"
                | "read_skill"
                | "find_skills"
                | "web_search"
                | "vision"
                | "design_search"
                | "design_read"
                | "jarvis_inspect_publication"
                | "jarvis_catalog"
        ) || (name.starts_with("ctx_")
            && !crate::core::context::needs_approval(name))
            || name.starts_with("context7_")
            || name.starts_with("lsp_")
            || matches!(
                name,
                "project_beads_list" | "project_beads_ready" | "project_beads_show"
            )
            || matches!(name, "beads_list" | "beads_ready" | "beads_show");
        let interactive = name == "ask_user" || name.starts_with("jarvis_propose_");
        let stateful = name.starts_with("process_")
            || name.starts_with("terminal_")
            || name.starts_with("browser_")
            || name.starts_with("hub_")
            || matches!(
                name,
                super::progress::TOOL_NAME | "update_tasks" | "validation_publish"
            );
        let approval = if super::tools::needs_approval(name)
            || name == "workflow_check"
            || crate::core::context::needs_approval(name)
            || crate::core::beads::needs_approval(name)
        {
            ApprovalPolicy::AccordingToTurn
        } else {
            ApprovalPolicy::Never
        };
        let effect = if read_only {
            Effect::ReadOnly
        } else if interactive {
            Effect::Interactive
        } else if stateful {
            Effect::Stateful
        } else if approval == ApprovalPolicy::AccordingToTurn {
            Effect::Mutating
        } else {
            Effect::Stateful
        };
        Self {
            effect,
            approval,
            parallel_safe: effect == Effect::ReadOnly,
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct ToolSpec {
    schema: Value,
    pub capabilities: Capabilities,
    pub handler: Handler,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct PreparedTool {
    pub capabilities: Capabilities,
    pub handler: Handler,
}

pub(super) struct Catalog {
    specs: BTreeMap<String, ToolSpec>,
}

impl Catalog {
    pub(super) fn new(definitions: &[Value]) -> Self {
        Self {
            specs: definitions
                .iter()
                .filter_map(|definition| {
                    let name = definition["name"].as_str()?.to_owned();
                    Some((
                        name.clone(),
                        ToolSpec {
                            schema: definition["parameters"].clone(),
                            capabilities: Capabilities::native(&name),
                            handler: Handler::native(&name),
                        },
                    ))
                })
                .collect(),
        }
    }

    pub(super) fn register_external(&mut self, name: &str, mutating: bool) {
        if let Some(spec) = self.specs.get_mut(name) {
            spec.capabilities = Capabilities {
                effect: if mutating {
                    Effect::Mutating
                } else {
                    Effect::ReadOnly
                },
                approval: if mutating {
                    ApprovalPolicy::AccordingToTurn
                } else {
                    ApprovalPolicy::Never
                },
                parallel_safe: !mutating,
            };
            spec.handler = Handler::Mcp;
        }
    }

    pub(super) fn capabilities(&self, name: &str) -> Option<Capabilities> {
        self.specs.get(name).map(|spec| spec.capabilities)
    }

    pub(super) fn parallel_safe(&self, calls: &[ToolCall]) -> bool {
        calls.len() > 1
            && calls.iter().all(|call| {
                self.capabilities(&call.name)
                    .is_some_and(|capabilities| capabilities.parallel_safe)
            })
    }

    pub(super) fn validate(&self, tool: &ToolCall) -> Result<(), AgentError> {
        if tool.status == "error" {
            return Err(recoverable(
                "tool_arguments_invalid",
                &tool.name,
                &tool.output,
                vec![],
            ));
        }
        let Some(spec) = self.specs.get(&tool.name) else {
            return Err(recoverable("tool_unavailable", &tool.name,
                "Esta ferramenta não está disponível nesta etapa. Use o catálogo atual; para MCPs, descubra a ferramenta antes de chamá-la.", vec![]));
        };
        // MCP clients already compile and retain their validators, including redacted
        // structured errors. Preserve that contract without recompiling large schemas.
        if tool.name.starts_with("mcp_") {
            return Ok(());
        }
        let schema = &spec.schema;
        let validator = jsonschema::validator_for(schema).map_err(|_| {
            recoverable("tool_schema_invalid", &tool.name,
                "O schema desta ferramenta é inválido. Escolha outra ação disponível e relate esta falha de configuração.", vec![])
        })?;
        let issues: Vec<Value> = validator.iter_errors(&tool.args).take(8).map(|error| {
            let mut path = error.instance_path().to_string();
            let rule = error.kind().keyword();
            let message = match error.kind() {
                ValidationErrorKind::Required { property } => {
                    path.push('/');
                    path.push_str(property.as_str().unwrap_or("?"));
                    "campo obrigatório ausente".to_owned()
                }
                ValidationErrorKind::AdditionalProperties { unexpected } => {
                    format!("campos não permitidos: {}", unexpected.join(", "))
                }
                _ => {
                    let expected = schema.pointer(&error.schema_path().to_string()).cloned().unwrap_or(Value::Null);
                    format!("não atende à regra {rule}; esperado: {}", expected.to_string().chars().take(240).collect::<String>())
                }
            };
            json!({"path":if path.is_empty() { "/" } else { &path }, "rule":rule, "message":message})
        }).collect();
        if issues.is_empty() {
            return Ok(());
        }
        let detail = issues
            .iter()
            .map(|issue| {
                format!(
                    "{}: {}",
                    issue["path"].as_str().unwrap_or("/"),
                    issue["message"].as_str().unwrap_or("")
                )
            })
            .collect::<Vec<_>>()
            .join("; ");
        Err(recoverable("tool_arguments_invalid", &tool.name,
            &format!("Argumentos inválidos. {detail}. Corrija apenas os campos indicados; a ferramenta não foi executada."), issues))
    }
}

/// The finalized model-visible plan is also the only executable tool plan for
/// a provider step. Callers may add runtime metadata for external tools before
/// any invocation is admitted.
pub(super) struct Orchestrator {
    catalog: Catalog,
}

impl Orchestrator {
    pub(super) fn new(definitions: &[Value]) -> Self {
        Self {
            catalog: Catalog::new(definitions),
        }
    }

    pub(super) fn register_external(&mut self, name: &str, mutating: bool) {
        self.catalog.register_external(name, mutating);
    }

    pub(super) fn preflight(&self, tool: &ToolCall) -> Result<PreparedTool, AgentError> {
        self.catalog.validate(tool)?;
        self.catalog
            .specs
            .get(&tool.name)
            .map(|spec| PreparedTool {
                capabilities: spec.capabilities,
                handler: spec.handler,
            })
            .ok_or_else(|| {
                recoverable(
                    "tool_unavailable",
                    &tool.name,
                    "Ferramenta indisponível nesta etapa.",
                    vec![],
                )
            })
    }

    pub(super) fn parallel_safe(&self, calls: &[ToolCall]) -> bool {
        self.catalog.parallel_safe(calls)
    }
}

fn recoverable(code: &str, tool: &str, message: &str, issues: Vec<Value>) -> AgentError {
    let mut error = AgentError::new(code, message);
    error.tool_result = Some(
        json!({
            "error":{"code":code,"tool":tool,"message":message,"issues":issues},
            "executed":false,
            "recoverable":true
        })
        .to_string(),
    );
    error
}

#[cfg(test)]
mod tests {
    use super::*;
    fn call(name: &str, args: Value) -> ToolCall {
        ToolCall {
            id: "contract-test".into(),
            name: name.into(),
            args,
            status: "pending".into(),
            output: String::new(),
            duration_ms: 0,
        }
    }

    #[test]
    fn native_validation_identifies_missing_wrong_type_and_unknown_fields_before_dispatch() {
        let catalog = Catalog::new(&super::super::tools::definitions(super::super::Mode::Build));
        let error = catalog
            .validate(&call("read", json!({"offset":"wrong", "extra":true})))
            .unwrap_err();
        assert_eq!(error.code, "tool_arguments_invalid");
        assert!(error.message.contains("/path"));
        assert!(error.message.contains("/offset"));
        assert!(error.message.contains("extra"));
        let result: Value = serde_json::from_str(error.tool_result.as_deref().unwrap()).unwrap();
        assert_eq!(result["executed"], false);
        assert!(catalog
            .validate(&call(
                "read",
                json!({"path":"src/main.rs", "offset":1, "limit":100})
            ))
            .is_ok());
    }

    #[test]
    fn hidden_tools_and_invalid_bounds_are_recoverable() {
        let catalog = Catalog::new(&super::super::tools::definitions(super::super::Mode::Plan));
        assert_eq!(
            catalog
                .validate(&call("bash", json!({"command":"pwd"})))
                .unwrap_err()
                .code,
            "tool_unavailable"
        );
        assert!(catalog
            .validate(&call("read", json!({"path":"a", "limit":501})))
            .is_err());
        assert!(catalog.validate(&call("invented", json!({}))).is_err());
    }

    #[test]
    fn malformed_provider_json_is_a_tool_error_and_accepts_the_corrected_call() {
        let catalog = Catalog::new(&super::super::tools::definitions(super::super::Mode::Build));
        for arguments in ["{broken json", "[1,2]"] {
            let calls = super::super::provider::tool_calls(&[
                json!({"type":"function_call","call_id":"a","name":"read","arguments":arguments}),
            ])
            .unwrap();
            let error = catalog.validate(&calls[0]).unwrap_err();
            assert_eq!(error.code, "tool_arguments_invalid");
            assert!(error
                .tool_result
                .as_deref()
                .unwrap()
                .contains("\"executed\":false"));
        }
        let calls = super::super::provider::tool_calls(&[json!({"type":"function_call","call_id":"b","name":"read","arguments":"{\"path\":\"README.md\"}"})]).unwrap();
        assert!(catalog.validate(&calls[0]).is_ok());
    }

    #[test]
    fn registry_declares_approval_effect_handler_and_parallel_safety() {
        let mut definitions = super::super::tools::definitions(super::super::Mode::Build);
        definitions.extend([
            json!({"type":"function","name":"mcp_docs_lookup","parameters":{"type":"object","additionalProperties":false}}),
            json!({"type":"function","name":"mcp_database_write","parameters":{"type":"object","additionalProperties":false}}),
        ]);
        let mut orchestrator = Orchestrator::new(&definitions);
        orchestrator.register_external("mcp_docs_lookup", false);
        orchestrator.register_external("mcp_database_write", true);
        let read = orchestrator
            .preflight(&call("read", json!({"path":"README.md"})))
            .unwrap();
        assert_eq!(read.handler, Handler::Native);
        assert_eq!(read.capabilities.effect, Effect::ReadOnly);
        assert_eq!(read.capabilities.approval, ApprovalPolicy::Never);
        assert!(orchestrator.parallel_safe(&[
            call("read", json!({"path":"README.md"})),
            call("list", json!({"path":"."})),
        ]));

        let write = orchestrator
            .preflight(&call("write", json!({"path":"a.txt", "content":"content"})))
            .unwrap();
        assert_eq!(write.capabilities.effect, Effect::Mutating);
        assert_eq!(write.capabilities.approval, ApprovalPolicy::AccordingToTurn);
        assert!(!orchestrator.parallel_safe(&[
            call("read", json!({"path":"README.md"})),
            call("write", json!({"path":"a.txt", "content":"content"})),
        ]));

        let lookup = orchestrator
            .preflight(&call("mcp_docs_lookup", json!({})))
            .unwrap();
        assert_eq!(lookup.handler, Handler::Mcp);
        assert_eq!(lookup.capabilities.effect, Effect::ReadOnly);
        assert_eq!(lookup.capabilities.approval, ApprovalPolicy::Never);
        let write = orchestrator
            .preflight(&call("mcp_database_write", json!({})))
            .unwrap();
        assert_eq!(write.handler, Handler::Mcp);
        assert_eq!(write.capabilities.effect, Effect::Mutating);
        assert_eq!(write.capabilities.approval, ApprovalPolicy::AccordingToTurn);
        assert!(!orchestrator.parallel_safe(&[
            call("mcp_docs_lookup", json!({})),
            call("mcp_database_write", json!({})),
        ]));
    }
}
