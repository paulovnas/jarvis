//! Credential-free MCP drafts; user-entered values go straight to secure storage.
use super::{invalid, AgentError};
use crate::mcp::config::{self, Config};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Draft {
    pub name: String,
    pub transport: Transport,
    pub command: Option<String>,
    pub args: Vec<String>,
    pub url: Option<String>,
    pub enabled: bool,
    pub cwd: Option<String>,
    pub env_keys: Vec<String>,
    pub header_keys: Vec<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Transport {
    Stdio,
    Http,
}

// Deliberately not Debug/Serialize: these values never belong in a transcript.
#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Values {
    pub environment: BTreeMap<String, String>,
    pub headers: BTreeMap<String, String>,
}

impl Draft {
    pub fn validate(&self) -> Result<(), AgentError> {
        let values = Values {
            environment: self
                .env_keys
                .iter()
                .map(|k| (k.clone(), "value".into()))
                .collect(),
            headers: self
                .header_keys
                .iter()
                .map(|k| (k.clone(), "value".into()))
                .collect(),
        };
        self.config(&values).map(|_| ())
    }

    pub fn config(&self, values: &Values) -> Result<Config, AgentError> {
        for (keys, supplied) in [
            (&self.env_keys, &values.environment),
            (&self.header_keys, &values.headers),
        ] {
            let expected: BTreeSet<_> = keys.iter().collect();
            if keys.len() > 64
                || expected.len() != keys.len()
                || expected != supplied.keys().collect()
                || supplied
                    .values()
                    .any(|value| value.trim().is_empty() || value.len() > 16_384)
            {
                return Err(invalid(
                    "Preencha os valores solicitados no painel de aprovação do MCP.",
                ));
            }
        }
        let config = match self.transport {
            Transport::Stdio => {
                if self.url.is_some() || !self.header_keys.is_empty() {
                    return Err(invalid(
                        "Um MCP local usa comando e variáveis de ambiente, sem URL ou cabeçalhos.",
                    ));
                }
                let mut command = vec![self
                    .command
                    .clone()
                    .ok_or_else(|| invalid("Informe o programa do MCP local."))?];
                command.extend(self.args.clone());
                Config::Local {
                    command,
                    cwd: self.cwd.clone(),
                    environment: values.environment.clone(),
                    enabled: self.enabled,
                    timeout: 30_000,
                    request_timeout: 300_000,
                }
            }
            Transport::Http => {
                if self.command.is_some()
                    || !self.args.is_empty()
                    || self.cwd.is_some()
                    || !self.env_keys.is_empty()
                {
                    return Err(invalid(
                        "Um MCP HTTP usa URL e cabeçalhos, sem comando ou variáveis de ambiente.",
                    ));
                }
                if self.url.as_ref().is_some_and(|url| url.contains('?')) {
                    return Err(invalid("A proposta de MCP deve usar uma URL sem query. Solicite credenciais privadamente em headerKeys; URLs com query podem ser configuradas nas Configurações."));
                }
                Config::Remote {
                    url: self
                        .url
                        .clone()
                        .ok_or_else(|| invalid("Informe a URL do MCP HTTP."))?,
                    headers: values.headers.clone(),
                    oauth: Some(false),
                    enabled: self.enabled,
                    timeout: 30_000,
                    request_timeout: 300_000,
                }
            }
        };
        // Reuse the settings parser at the concrete configuration trust boundary.
        let (_, config) = config::parse(&config.named(&self.name)).map_err(AgentError::from)?;
        Ok(config)
    }
}

pub fn definition() -> Value {
    let nullable =
        |max| json!({"anyOf":[{"type":"null"},{"type":"string","minLength":1,"maxLength":max}]});
    let keys = json!({"type":"array","maxItems":64,"uniqueItems":true,"items":{"type":"string","minLength":1,"maxLength":200}});
    json!({"type":"function","name":"jarvis_propose_mcp","description":"Propose adding one globally configured MCP server. Always waits for explicit native user approval, including in YOLO mode. Inspect jarvis_catalog first to avoid duplicates. This only registers the server; use mcp_activate afterwards to discover its tools. Never put credentials in summary, command, arguments or URL; HTTP URL queries are unsupported here. List environment/header key names; the user supplies their values privately in the approval panel. OAuth and updates to existing servers are not supported by this tool.","parameters":{
        "type":"object","additionalProperties":false,"required":["summary","server"],"properties":{
            "summary":{"type":"string","minLength":1,"maxLength":1000},
            "server":{"type":"object","additionalProperties":false,"required":["name","transport","command","args","url","enabled","cwd","envKeys","headerKeys"],"properties":{
                "name":{"type":"string","pattern":"^[A-Za-z0-9_-]{1,48}$"},
                "transport":{"type":"string","enum":["stdio","http"]},
                "command":nullable(4000),"args":{"type":"array","maxItems":127,"items":{"type":"string","maxLength":4000}},
                "url":nullable(8000),"enabled":{"type":"boolean"},"cwd":nullable(4000),
                "envKeys":keys,"headerKeys":keys
            }}
        }
    }})
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draft() -> Draft {
        serde_json::from_value(json!({"name":"firebase","transport":"stdio","command":"npx","args":["-y","firebase-tools","mcp"],"url":null,"enabled":true,"cwd":null,"envKeys":["TOKEN"],"headerKeys":[]})).unwrap()
    }

    #[test]
    fn only_exact_private_values_create_a_valid_config_and_preview_stays_secret_free() {
        let server = draft();
        server.validate().unwrap();
        assert!(server.config(&Values::default()).is_err());
        let values = Values {
            environment: BTreeMap::from([("TOKEN".into(), "private-value".into())]),
            headers: BTreeMap::new(),
        };
        let config = server.config(&values).unwrap();
        assert!(config.named("firebase").contains("private-value"));
        assert!(!serde_json::to_string(&server)
            .unwrap()
            .contains("private-value"));
        let extra = Values {
            environment: BTreeMap::from([("OTHER".into(), "private-value".into())]),
            headers: BTreeMap::new(),
        };
        assert!(server.config(&extra).is_err());
    }

    #[test]
    fn drafts_reuse_native_validation_and_reject_cross_transport_fields_and_duplicates() {
        let mut server = draft();
        server.env_keys.push("TOKEN".into());
        assert!(server.validate().is_err());
        server.env_keys.clear();
        server.url = Some("https://example.com/mcp".into());
        assert!(server.validate().is_err());
        server.url = None;
        server.name = "../bad".into();
        assert!(server.validate().is_err());
        let bad = json!({"name":"remote","transport":"http","command":null,"args":[],"url":"https://example.com/mcp","enabled":true,"cwd":null,"envKeys":[],"headerKeys":["mcp-session-id"]});
        assert!(serde_json::from_value::<Draft>(bad)
            .unwrap()
            .validate()
            .is_err());
        let query = json!({"name":"remote","transport":"http","command":null,"args":[],"url":"https://example.com/mcp?api_key=private-value","enabled":true,"cwd":null,"envKeys":[],"headerKeys":[]});
        assert!(serde_json::from_value::<Draft>(query)
            .unwrap()
            .validate()
            .is_err());
    }
}
