//! The exact model-visible catalog is also the dispatch contract for this step.
use super::{AgentError, ToolCall};
use jsonschema::error::ValidationErrorKind;
use serde_json::{json, Value};
use std::collections::BTreeMap;

pub(super) struct Catalog {
    schemas: BTreeMap<String, Value>,
}

impl Catalog {
    pub(super) fn new(definitions: &[Value]) -> Self {
        Self {
            schemas: definitions
                .iter()
                .filter_map(|definition| {
                    Some((
                        definition["name"].as_str()?.to_owned(),
                        definition["parameters"].clone(),
                    ))
                })
                .collect(),
        }
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
        let Some(schema) = self.schemas.get(&tool.name) else {
            return Err(recoverable("tool_unavailable", &tool.name,
                "Esta ferramenta não está disponível nesta etapa. Use o catálogo atual; para MCPs, descubra a ferramenta antes de chamá-la.", vec![]));
        };
        // MCP clients already compile and retain their validators, including redacted
        // structured errors. Preserve that contract without recompiling large schemas.
        if tool.name.starts_with("mcp_") {
            return Ok(());
        }
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
}
