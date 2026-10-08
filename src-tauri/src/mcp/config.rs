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
        self.secret_values(false)
    }
    pub(crate) fn plugin_secrets(&self) -> Vec<String> {
        self.secret_values(true)
    }
    fn secret_values(&self, plugin_owned: bool) -> Vec<String> {
        let values = match self {
            Self::Local {
                command,
                environment,
                ..
            } => command
                .iter()
                .skip(1)
                .chain(environment.iter().filter_map(|(name, value)| {
                    // These six paths are overwritten by the native plugin
                    // adapter. All user-supplied values still redact, regardless
                    // of whether their names resemble credentials.
                    let native_path = matches!(
                        name.as_str(),
                        "CODEX_PLUGIN_ROOT"
                            | "CLAUDE_PLUGIN_ROOT"
                            | "CODEX_PLUGIN_DATA"
                            | "CLAUDE_PLUGIN_DATA"
                            | "CODEX_HOME"
                            | "CLAUDE_CONFIG_DIR"
                    );
                    (!plugin_owned || !native_path).then_some(value)
                }))
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
    let config: Config = serde_json::from_value(value.clone()).map_err(|_| error("Configuração inválida. Use type local com command (lista), ou remote com url e OAuth ou headers para autenticação."))?;
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
            if *oauth == Some(true)
                && headers
                    .keys()
                    .any(|key| key.eq_ignore_ascii_case("authorization"))
            {
                return Err(error(
                    "Remova o header Authorization para usar OAuth neste MCP.",
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
            r#"{"docs":{"type":"remote","url":"https://example.test/mcp","oauth":true}}"#,
        ] {
            assert!(parse(raw).is_ok());
        }
        for raw in [
            "{}",
            "{",
            r#"{"a":{},"b":{}}"#,
            r#"{"x":{"type":"local","command":[]}}"#,
            r#"{"x":{"type":"remote","url":"file:///tmp/test"}}"#,
            r#"{"x":{"type":"remote","url":"https://example.test","oauth":true,"headers":{"Authorization":"Bearer secret"}}}"#,
            r#"{"x":{"type":"local","command":["node"],"typo":true}}"#,
        ] {
            assert!(parse(raw).is_err());
        }
    }

    #[test]
    fn plugin_redaction_preserves_native_paths_and_masks_every_private_value() {
        let paths = [
            ("CODEX_PLUGIN_ROOT", "/native/plugin-root"),
            ("CLAUDE_PLUGIN_ROOT", "/native/plugin-root"),
            ("CODEX_PLUGIN_DATA", "/native/plugin-data"),
            ("CLAUDE_PLUGIN_DATA", "/native/plugin-data"),
            ("CODEX_HOME", "/native/plugin-data/codex"),
            ("CLAUDE_CONFIG_DIR", "/native/plugin-data/claude"),
        ];
        let raw = json!({"docs":{"type":"local","command":["node","server.mjs","--api-key","command-secret"],"environment":paths.into_iter().chain([("DOCS_TOKEN","private-token"),("TENANT","private-tenant"),("CUSTOM_PATH","/private/account")]).collect::<BTreeMap<_,_>>()}}).to_string();
        let (_, config) = parse(&raw).unwrap();
        let plugin = config.plugin_secrets();
        let ordinary = config.secrets();
        for (_, path) in paths {
            assert!(!plugin.iter().any(|value| value == path));
            assert!(ordinary.iter().any(|value| value == path));
        }
        for secret in [
            "command-secret",
            "private-token",
            "private-tenant",
            "/private/account",
        ] {
            assert!(plugin.iter().any(|value| value == secret));
            assert!(ordinary.iter().any(|value| value == secret));
        }
    }
}

/// Adapt plugin-owned Codex/Claude declarations without storing ordinary MCP settings.
#[cfg(test)]
pub(crate) fn from_plugin(source: &crate::plugins::McpContribution) -> Result<Config, McpError> {
    from_plugin_with_values(source, &BTreeMap::new())
}
pub(crate) fn from_plugin_with_values(
    source: &crate::plugins::McpContribution,
    values: &BTreeMap<String, String>,
) -> Result<Config, McpError> {
    fn expand(
        raw: &str,
        source: &crate::plugins::McpContribution,
        values: &BTreeMap<String, String>,
    ) -> Result<String, McpError> {
        let mut result = raw.to_owned();
        for (name, path) in [
            ("CODEX_PLUGIN_ROOT", &source.root),
            ("CLAUDE_PLUGIN_ROOT", &source.root),
            ("CODEX_PLUGIN_DATA", &source.data_path),
            ("CLAUDE_PLUGIN_DATA", &source.data_path),
        ] {
            result = result.replace(&format!("${{{name}}}"), &path.to_string_lossy());
        }
        for (name, value) in values {
            result = result
                .replace(&format!("${{{name}}}"), value)
                .replace(&format!("${{env:{name}}}"), value);
        }
        if result.contains("${") {
            return Err(super::coded_error("plugin_mcp_configuration", "O MCP do plugin contém uma variável não configurada. Configure-a antes de conectar."));
        }
        Ok(result)
    }
    let definition = source
        .definition
        .as_object()
        .ok_or_else(|| error("Configuração MCP inválida no plugin."))?;
    let enabled = true;
    let config = if let Some(url) = definition.get("url").and_then(Value::as_str) {
        let mut headers = BTreeMap::new();
        if let Some(entries) = definition
            .get("http_headers")
            .or_else(|| definition.get("headers"))
        {
            for (key, value) in entries
                .as_object()
                .ok_or_else(|| error("Headers MCP inválidos no plugin."))?
            {
                headers.insert(
                    key.clone(),
                    expand(
                        value
                            .as_str()
                            .ok_or_else(|| error("Header MCP inválido no plugin."))?,
                        source,
                        values,
                    )?,
                );
            }
        }
        if let Some(name) = definition
            .get("bearer_token_env_var")
            .and_then(Value::as_str)
        {
            let value = values
                .get(name)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| {
                    super::coded_error(
                        "plugin_mcp_configuration",
                        &format!("Configure a variável privada {name} deste MCP."),
                    )
                })?;
            headers.insert("Authorization".into(), format!("Bearer {value}"));
        }
        if let Some(names) = definition
            .get("env_http_headers")
            .and_then(Value::as_object)
        {
            for (header, name) in names {
                let name = name
                    .as_str()
                    .ok_or_else(|| error("Variável de header inválida."))?;
                headers.insert(
                    header.clone(),
                    values
                        .get(name)
                        .filter(|value| !value.is_empty())
                        .cloned()
                        .ok_or_else(|| {
                            super::coded_error(
                                "plugin_mcp_configuration",
                                &format!("Configure a variável privada {name} deste MCP."),
                            )
                        })?,
                );
            }
        }
        let oauth = Some(
            definition.get("oauth").and_then(Value::as_bool).unwrap_or(
                !headers
                    .keys()
                    .any(|key| key.eq_ignore_ascii_case("authorization")),
            ),
        );
        Config::Remote {
            url: expand(url, source, values)?,
            headers,
            oauth,
            enabled,
            timeout: startup_timeout(),
            request_timeout: request_timeout(),
        }
    } else {
        let mut command = match definition.get("command") {
            Some(Value::String(program)) => vec![expand(program, source, values)?],
            Some(Value::Array(parts)) => parts
                .iter()
                .map(|value| {
                    value
                        .as_str()
                        .ok_or_else(|| error("Comando MCP inválido no plugin."))
                        .and_then(|value| expand(value, source, values))
                })
                .collect::<Result<Vec<_>, _>>()?,
            _ => return Err(error("O MCP do plugin não contém um programa válido.")),
        };
        if let Some(arguments) = definition.get("args") {
            command.extend(
                arguments
                    .as_array()
                    .ok_or_else(|| error("Argumentos MCP inválidos no plugin."))?
                    .iter()
                    .map(|value| {
                        value
                            .as_str()
                            .ok_or_else(|| error("Argumento MCP inválido no plugin."))
                            .and_then(|value| expand(value, source, values))
                    })
                    .collect::<Result<Vec<_>, _>>()?,
            );
        }
        let mut environment = BTreeMap::new();
        if let Some(entries) = definition
            .get("env")
            .or_else(|| definition.get("environment"))
        {
            for (key, value) in entries
                .as_object()
                .ok_or_else(|| error("Ambiente MCP inválido no plugin."))?
            {
                environment.insert(
                    key.clone(),
                    expand(
                        value
                            .as_str()
                            .ok_or_else(|| error("Variável MCP inválida no plugin."))?,
                        source,
                        values,
                    )?,
                );
            }
        }
        if let Some(names) = definition.get("env_vars").and_then(Value::as_array) {
            for value in names {
                let name = value
                    .as_str()
                    .or_else(|| value.get("name").and_then(Value::as_str))
                    .ok_or_else(|| error("Variável MCP inválida."))?;
                environment.insert(
                    name.into(),
                    values
                        .get(name)
                        .filter(|value| !value.is_empty())
                        .cloned()
                        .ok_or_else(|| {
                            super::coded_error(
                                "plugin_mcp_configuration",
                                &format!("Configure a variável privada {name} deste MCP."),
                            )
                        })?,
                );
            }
        }
        for (name, path) in [
            ("CODEX_PLUGIN_ROOT", &source.root),
            ("CLAUDE_PLUGIN_ROOT", &source.root),
            ("CODEX_PLUGIN_DATA", &source.data_path),
            ("CLAUDE_PLUGIN_DATA", &source.data_path),
        ] {
            environment.insert(name.into(), path.to_string_lossy().into_owned());
        }
        environment.insert(
            "CODEX_HOME".into(),
            source
                .data_path
                .join("codex")
                .to_string_lossy()
                .into_owned(),
        );
        environment.insert(
            "CLAUDE_CONFIG_DIR".into(),
            source
                .data_path
                .join("claude")
                .to_string_lossy()
                .into_owned(),
        );
        let cwd = definition
            .get("cwd")
            .and_then(Value::as_str)
            .map(|value| {
                let expanded = expand(value, source, values)?;
                let path = std::path::Path::new(&expanded);
                Ok::<_, McpError>(if path.is_absolute() {
                    expanded
                } else {
                    source.root.join(path).to_string_lossy().into_owned()
                })
            })
            .transpose()?;
        Config::Local {
            command,
            cwd,
            environment,
            enabled,
            timeout: startup_timeout(),
            request_timeout: request_timeout(),
        }
    };
    Ok(parse(&config.named("plugin"))?.1)
}

pub(crate) fn plugin_fields(source: &crate::plugins::McpContribution) -> Vec<String> {
    let mut fields = std::collections::BTreeSet::new();
    let expression =
        regex::Regex::new(r"\$\{(?:env:)?([A-Za-z_][A-Za-z0-9_]*)\}").expect("constant expression");
    for capture in expression.captures_iter(&source.definition.to_string()) {
        if ![
            "CODEX_PLUGIN_ROOT",
            "CLAUDE_PLUGIN_ROOT",
            "CODEX_PLUGIN_DATA",
            "CLAUDE_PLUGIN_DATA",
        ]
        .contains(&&capture[1])
        {
            fields.insert(capture[1].to_owned());
        }
    }
    if let Some(name) = source.definition["bearer_token_env_var"].as_str() {
        fields.insert(name.into());
    }
    for value in source.definition["env_http_headers"]
        .as_object()
        .into_iter()
        .flatten()
        .map(|(_, value)| value)
    {
        if let Some(name) = value.as_str() {
            fields.insert(name.into());
        }
    }
    for value in source.definition["env_vars"]
        .as_array()
        .into_iter()
        .flatten()
    {
        if let Some(name) = value.as_str().or_else(|| value["name"].as_str()) {
            fields.insert(name.into());
        }
    }
    fields.into_iter().collect()
}

#[cfg(test)]
mod plugin_tests {
    use super::*;
    #[test]
    fn explicit_plugin_cwd_is_package_relative_and_default_remains_project_scoped() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().join("bundle");
        let absolute = temporary
            .path()
            .join("explicit-folder")
            .to_string_lossy()
            .into_owned();
        let mut source = crate::plugins::McpContribution {
            plugin_id: "context-mode@local".into(),
            plugin_hash: "reviewed".into(),
            component_id: "mcp:context-mode".into(),
            name: "context-mode".into(),
            definition: json!({"command":"node","args":["./start.mjs"],"cwd":"."}),
            root: root.clone(),
            data_path: temporary.path().join("private"),
        };
        for (raw, expected) in [
            (
                Some("."),
                Some(root.join(".").to_string_lossy().into_owned()),
            ),
            (
                Some("runtime"),
                Some(root.join("runtime").to_string_lossy().into_owned()),
            ),
            (
                Some("${CODEX_PLUGIN_ROOT}"),
                Some(root.to_string_lossy().into_owned()),
            ),
            (Some(absolute.as_str()), Some(absolute.clone())),
            (None, None),
        ] {
            match raw {
                Some(cwd) => source.definition["cwd"] = json!(cwd),
                None => {
                    source.definition.as_object_mut().unwrap().remove("cwd");
                }
            }
            let Config::Local { cwd, command, .. } = from_plugin(&source).unwrap() else {
                panic!("local fixture")
            };
            assert_eq!(cwd, expected);
            assert_eq!(command, vec!["node", "./start.mjs"]);
        }
    }

    #[test]
    fn private_plugin_variables_are_declared_scoped_and_redacted() {
        let source = crate::plugins::McpContribution {
            plugin_id: "docs@local".into(),
            plugin_hash: "reviewed".into(),
            component_id: "mcp:docs".into(),
            name: "docs".into(),
            definition: json!({"url":"https://example.test/mcp","bearer_token_env_var":"DOCS_TOKEN","env_http_headers":{"X-Tenant":"TENANT"}}),
            root: "/bundle/docs".into(),
            data_path: "/private/docs".into(),
        };
        assert_eq!(plugin_fields(&source), vec!["DOCS_TOKEN", "TENANT"]);
        assert!(from_plugin(&source).is_err());
        let config = from_plugin_with_values(
            &source,
            &BTreeMap::from([
                ("DOCS_TOKEN".into(), "private-test-token".into()),
                ("TENANT".into(), "account".into()),
            ]),
        )
        .unwrap();
        match &config {
            Config::Remote { headers, oauth, .. } => {
                assert_eq!(headers["Authorization"], "Bearer private-test-token");
                assert_eq!(*oauth, Some(false));
            }
            _ => panic!("wrong transport"),
        }
        assert!(config
            .secrets()
            .iter()
            .any(|value| value.contains("private-test-token")));
        let local = crate::plugins::McpContribution {
            definition: json!({"command":"node","args":["${CODEX_PLUGIN_ROOT}/server.js"],"env_vars":["DOCS_TOKEN"],"env":{"PROJECT":"${env:TENANT}"}}),
            ..source
        };
        let config = from_plugin_with_values(
            &local,
            &BTreeMap::from([
                ("DOCS_TOKEN".into(), "private-test-token".into()),
                ("TENANT".into(), "account".into()),
            ]),
        )
        .unwrap();
        match config {
            Config::Local {
                command,
                environment,
                ..
            } => {
                assert_eq!(command[1], "/bundle/docs/server.js");
                assert_eq!(environment["DOCS_TOKEN"], "private-test-token");
                assert_eq!(environment["PROJECT"], "account");
                assert_eq!(environment["CODEX_HOME"], "/private/docs/codex");
                assert_eq!(environment["CLAUDE_CONFIG_DIR"], "/private/docs/claude");
            }
            _ => panic!("wrong transport"),
        }
    }
}
