use super::{error, McpError};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{collections::BTreeMap, time::Duration};

pub const TEMPLATE: &str = r#"{
  "context7": {
    "type": "local",
    "command": ["npx", "-y", "@upstash/context7-mcp", "--api-key", "YOUR_API_KEY"],
    "enabled": true
  }
}"#;

fn enabled() -> bool {
    true
}
const DEFAULT_STARTUP_TIMEOUT: u64 = 30_000;
const DEFAULT_REQUEST_TIMEOUT: u64 = 300_000;

fn startup_timeout() -> u64 {
    DEFAULT_STARTUP_TIMEOUT
}
fn request_timeout() -> u64 {
    DEFAULT_REQUEST_TIMEOUT
}
fn default_request_timeout(value: &u64) -> bool {
    *value == DEFAULT_REQUEST_TIMEOUT
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase", deny_unknown_fields)]
pub enum Config {
    Local {
        command: Vec<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cwd: Option<String>,
        #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
        environment: BTreeMap<String, String>,
        #[serde(default = "enabled")]
        enabled: bool,
        #[serde(default = "startup_timeout")]
        timeout: u64,
        #[serde(
            default = "request_timeout",
            rename = "requestTimeout",
            alias = "request_timeout",
            skip_serializing_if = "default_request_timeout"
        )]
        request_timeout: u64,
    },
    Remote {
        url: String,
        #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
        headers: BTreeMap<String, String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        oauth: Option<bool>,
        #[serde(default = "enabled")]
        enabled: bool,
        #[serde(default = "startup_timeout")]
        timeout: u64,
        #[serde(
            default = "request_timeout",
            rename = "requestTimeout",
            alias = "request_timeout",
            skip_serializing_if = "default_request_timeout"
        )]
        request_timeout: u64,
    },
}

impl Config {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Local { .. } => "local",
            Self::Remote { .. } => "remote",
        }
    }
    pub fn enabled(&self) -> bool {
        match self {
            Self::Local { enabled, .. } | Self::Remote { enabled, .. } => *enabled,
        }
    }
    pub fn set_enabled(&mut self, value: bool) {
        match self {
            Self::Local { enabled, .. } | Self::Remote { enabled, .. } => *enabled = value,
        }
    }
    pub fn startup_timeout(&self) -> Duration {
        Duration::from_millis(match self {
            Self::Local { timeout, .. } | Self::Remote { timeout, .. } => *timeout,
        })
    }
    pub fn request_timeout(&self) -> Duration {
        Duration::from_millis(match self {
            Self::Local {
                request_timeout, ..
            }
            | Self::Remote {
                request_timeout, ..
            } => *request_timeout,
        })
    }
    pub fn configured(&self) -> bool {
        !serde_json::to_string(self)
            .unwrap_or_default()
            .contains("YOUR_API_KEY")
    }
    pub fn named(&self, name: &str) -> String {
        serde_json::to_string_pretty(&json!({name: self})).unwrap_or_default()
    }
    pub fn secrets(&self) -> Vec<String> {
        let values = match self {
            Self::Local {
                command,
                environment,
                ..
            } => command
                .iter()
                .skip(1)
                .chain(environment.values())
                .cloned()
                .collect(),
            Self::Remote { url, headers, .. } => {
                let mut values: Vec<_> = headers.values().cloned().collect();
                if let Ok(url) = url::Url::parse(url) {
                    values.extend(url.query_pairs().map(|(_, v)| v.into_owned()));
                }
                values
            }
        };
        values
            .into_iter()
            .filter(|value: &String| value.len() >= 4 && !value.starts_with('-'))
            .collect()
    }
}

pub fn parse(raw: &str) -> Result<(String, Config), McpError> {
    if raw.len() > 64 * 1024 {
        return Err(error("Use uma configuração de até 64 KB."));
    }
    if raw.contains("{env:") || raw.contains("{file:") {
        return Err(error("Referências {env:...} e {file:...} ainda não são suportadas. Informe os valores diretamente na configuração."));
    }
    let value: Value = serde_json::from_str(raw)
        .map_err(|_| error("JSON inválido. Confira as aspas, vírgulas e chaves."))?;
    let object = value.as_object().filter(|v| v.len() == 1).ok_or_else(|| {
        error("Informe um objeto com exatamente um MCP nomeado, sem a chave externa mcp.")
    })?;
    let (name, value) = object
        .iter()
        .next()
        .ok_or_else(|| error("Informe o nome do MCP."))?;
    if name.is_empty()
        || name.len() > 48
        || !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
    {
        return Err(error(
            "O nome deve ter até 48 letras, números, hífens ou sublinhados.",
        ));
    }
    let config: Config = serde_json::from_value(value.clone()).map_err(|_| error("Configuração inválida. Use type local com command (lista), ou remote com url. OAuth ainda não é suportado; use headers para autenticação."))?;
    if !(1000..=120_000).contains(&(config.startup_timeout().as_millis() as u64)) {
        return Err(error(
            "timeout deve estar entre 1000 e 120000 milissegundos.",
        ));
    }
    if !(1000..=900_000).contains(&(config.request_timeout().as_millis() as u64)) {
        return Err(error(
            "requestTimeout deve estar entre 1000 e 900000 milissegundos.",
        ));
    }
    match &config {
        Config::Local {
            command,
            cwd,
            environment,
            ..
        } => {
            if command.is_empty()
                || command.len() > 128
                || command[0].trim().is_empty()
                || command.iter().any(|v| v.contains('\0'))
                || cwd
                    .as_ref()
                    .is_some_and(|v| v.trim().is_empty() || v.contains('\0'))
                || environment
                    .iter()
                    .any(|(k, v)| k.is_empty() || k.contains(['=', '\0']) || v.contains('\0'))
            {
                return Err(error("Informe um comando válido como lista e variáveis de ambiente com nomes válidos."));
            }
        }
        Config::Remote {
            url,
            headers,
            oauth,
            ..
        } => {
            let url =
                url::Url::parse(url).map_err(|_| error("Informe uma URL HTTP ou HTTPS válida."))?;
            if !matches!(url.scheme(), "https" | "http")
                || url.host_str().is_none()
                || !url.username().is_empty()
                || url.password().is_some()
                || url.fragment().is_some()
            {
                return Err(error(
                    "Use uma URL HTTP ou HTTPS sem usuário, senha ou fragmento.",
                ));
            }
            if *oauth == Some(true) {
                return Err(error(
                    "OAuth de MCPs ainda não está disponível. Use headers ou oauth: false.",
                ));
            }
            for (key, value) in headers {
                if reqwest::header::HeaderName::from_bytes(key.as_bytes()).is_err()
                    || reqwest::header::HeaderValue::from_str(value).is_err()
                    || matches!(
                        key.to_ascii_lowercase().as_str(),
                        "host" | "content-length" | "mcp-session-id" | "mcp-protocol-version"
                    )
                {
                    return Err(error(
                        "Os headers contêm um nome ou valor inválido ou reservado pelo protocolo.",
                    ));
                }
            }
        }
    }
    Ok((name.clone(), config))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn default_is_pending_and_configuration_round_trips() {
        let (name, config) = parse(TEMPLATE).unwrap();
        assert_eq!(name, "context7");
        assert!(config.enabled());
        assert!(!config.configured());
        assert_eq!(
            config.startup_timeout(),
            Duration::from_millis(DEFAULT_STARTUP_TIMEOUT)
        );
        assert_eq!(
            config.request_timeout(),
            Duration::from_millis(DEFAULT_REQUEST_TIMEOUT)
        );
        assert_eq!(parse(&config.named(&name)).unwrap().0, name);
    }

    #[test]
    fn legacy_timeout_only_controls_startup_and_request_timeout_accepts_both_spellings() {
        let (_, legacy) =
            parse(r#"{"docs":{"type":"local","command":["node"],"timeout":5000}}"#).unwrap();
        assert_eq!(legacy.startup_timeout(), Duration::from_millis(5_000));
        assert_eq!(
            legacy.request_timeout(),
            Duration::from_millis(DEFAULT_REQUEST_TIMEOUT)
        );
        for field in ["requestTimeout", "request_timeout"] {
            let raw =
                format!(r#"{{"docs":{{"type":"local","command":["node"],"{field}":180000}}}}"#);
            let (_, config) = parse(&raw).unwrap();
            assert_eq!(config.request_timeout(), Duration::from_millis(180_000));
        }
    }
    #[test]
    fn accepts_opencode_local_and_remote_and_rejects_unsupported_fields() {
        for raw in [
            r#"{"docs":{"type":"local","command":["node","server.js"],"environment":{"KEY":"secret"},"enabled":false}}"#,
            r#"{"docs":{"type":"remote","url":"https://example.test/mcp","headers":{"Authorization":"Bearer secret"},"oauth":false}}"#,
        ] {
            assert!(parse(raw).is_ok());
        }
        for raw in [
            "{}",
            "{",
            r#"{"a":{},"b":{}}"#,
            r#"{"x":{"type":"local","command":[]}}"#,
            r#"{"x":{"type":"remote","url":"file:///tmp/test"}}"#,
            r#"{"x":{"type":"remote","url":"https://example.test","oauth":true}}"#,
            r#"{"x":{"type":"local","command":["node"],"typo":true}}"#,
        ] {
            assert!(parse(raw).is_err());
        }
    }
}
