//! Revisioned, user-approved plugin packages and removable runtime overlays.
pub(crate) mod apps;
pub(crate) mod commands;
mod manifest;
mod presentation;
mod runtime;
mod source;
mod store;
#[cfg(test)]
mod tests;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

pub(crate) use runtime::prompt as runtime_prompt;
#[cfg(test)]
pub(crate) use store::catalog_file;
pub(crate) use store::catalog_with_icons;
pub(crate) use store::discover_builtin_catalogs;
pub(crate) use store::{apply, catalog, load_active, load_active_for_project, preview, Prepared};
pub(crate) use store::{
    frozen_component_authorized, hook_source_authorized, skill_source_authorized,
    skill_sources_authorized,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) enum ComponentKind {
    Skills,
    Mcp,
    Hooks,
    Apps,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Component {
    pub id: String,
    pub name: String,
    pub kind: ComponentKind,
    pub enabled: bool,
    pub trusted: bool,
    pub supported: bool,
    pub detail: String,
    #[serde(default)]
    pub mcp_server_id: Option<String>,
    #[serde(default)]
    pub mcp_oauth: bool,
    #[serde(default)]
    pub app_connect_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Marketplace {
    pub id: String,
    pub name: String,
    pub source: String,
    pub ref_name: Option<String>,
    pub sparse_paths: Vec<String>,
    pub refreshed: bool,
    pub built_in: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub(crate) enum PackageSource {
    Local {
        path: String,
    },
    Git {
        url: String,
        path: Option<String>,
        #[serde(rename = "refName")]
        ref_name: Option<String>,
        sha: Option<String>,
    },
    Npm {
        package: String,
        version: Option<String>,
        registry: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AvailablePlugin {
    pub id: String,
    pub name: String,
    pub marketplace_id: String,
    pub display_name: String,
    pub description: String,
    pub version: Option<String>,
    pub source: PackageSource,
    pub installable: bool,
    pub authentication: String,
    pub requirements: Vec<String>,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub short_description: Option<String>,
    #[serde(default)]
    pub icon_data_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct InstalledPlugin {
    pub id: String,
    pub name: String,
    pub marketplace_id: String,
    pub display_name: String,
    pub description: String,
    pub version: String,
    pub hash: String,
    pub root_path: String,
    pub data_path: String,
    pub enabled: bool,
    pub integrity_valid: bool,
    pub components: Vec<Component>,
    pub project_overrides: BTreeMap<String, bool>,
    pub warnings: Vec<String>,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub short_description: Option<String>,
    #[serde(default)]
    pub icon_data_url: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Catalog {
    pub revision: u64,
    pub marketplaces: Vec<Marketplace>,
    pub available: Vec<AvailablePlugin>,
    pub installed: Vec<InstalledPlugin>,
    pub issues: Vec<String>,
    pub apps_account_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Preview {
    pub title: String,
    pub description: String,
    pub source: String,
    pub hash: String,
    pub components: Vec<Component>,
    pub commands: Vec<String>,
    pub requirements: Vec<String>,
    pub warnings: Vec<String>,
    pub affected_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct DraftSkill {
    pub name: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct DraftFile {
    pub path: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Draft {
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub skills: Vec<DraftSkill>,
    #[serde(default)]
    pub mcp_servers: BTreeMap<String, Value>,
    #[serde(default)]
    pub hooks: Option<Value>,
    #[serde(default)]
    pub apps: BTreeMap<String, Value>,
    #[serde(default)]
    pub files: Vec<DraftFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "camelCase", deny_unknown_fields)]
pub(crate) enum Operation {
    AddMarketplace {
        source: String,
        #[serde(default, rename = "refName")]
        ref_name: Option<String>,
        #[serde(default, rename = "sparsePaths")]
        sparse_paths: Vec<String>,
    },
    RefreshMarketplace {
        #[serde(default, rename = "marketplaceId")]
        marketplace_id: Option<String>,
    },
    RemoveMarketplace {
        #[serde(rename = "marketplaceId")]
        marketplace_id: String,
    },
    Install {
        #[serde(rename = "pluginId")]
        plugin_id: String,
    },
    Update {
        #[serde(rename = "pluginId")]
        plugin_id: String,
    },
    Uninstall {
        #[serde(rename = "pluginId")]
        plugin_id: String,
    },
    SetEnabled {
        #[serde(rename = "pluginId")]
        plugin_id: String,
        enabled: bool,
        #[serde(default, rename = "projectPath")]
        project_path: Option<String>,
    },
    ConfigureComponent {
        #[serde(rename = "pluginId")]
        plugin_id: String,
        #[serde(rename = "componentId")]
        component_id: String,
        enabled: bool,
    },
    TrustHooks {
        #[serde(rename = "pluginId")]
        plugin_id: String,
        trusted: bool,
    },
    SetAppsAccount {
        #[serde(rename = "accountId")]
        account_id: Option<String>,
    },
    Import {
        path: String,
    },
    Create {
        draft: Draft,
    },
}

#[derive(Debug, Clone)]
pub(crate) struct SkillRoot {
    pub plugin_id: String,
    pub component_id: String,
    pub path: PathBuf,
    pub recursive: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct McpContribution {
    pub plugin_id: String,
    pub plugin_hash: String,
    pub component_id: String,
    pub name: String,
    pub definition: Value,
    pub root: PathBuf,
    pub data_path: PathBuf,
}

#[derive(Debug, Clone)]
pub(crate) struct HookSource {
    pub plugin_id: String,
    pub plugin_hash: String,
    pub component_id: String,
    pub name: String,
    pub definition: Value,
    pub root: PathBuf,
    pub data_path: PathBuf,
    pub trusted: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct AppContribution {
    pub plugin_id: String,
    pub plugin_hash: String,
    pub id: String,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct Overlay {
    pub revision: u64,
    pub capabilities: Vec<PluginCapabilities>,
    pub skill_roots: Vec<SkillRoot>,
    pub mcp_servers: Vec<McpContribution>,
    pub hook_sources: Vec<HookSource>,
    pub apps: Vec<AppContribution>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PluginCapabilities {
    pub id: String,
    pub name: String,
    pub description: String,
    pub components: Vec<String>,
    pub mcp_servers: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct PluginsError {
    pub code: &'static str,
    pub message: String,
}
impl std::fmt::Display for PluginsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.message.fmt(f)
    }
}
impl std::error::Error for PluginsError {}
pub(crate) type Result<T> = std::result::Result<T, PluginsError>;

fn error(code: &'static str, message: impl Into<String>) -> PluginsError {
    PluginsError {
        code,
        message: message.into(),
    }
}
fn io_error(_: std::io::Error) -> PluginsError {
    error(
        "plugins_storage",
        "Não foi possível acessar os arquivos dos plugins.",
    )
}
fn json_error(_: serde_json::Error) -> PluginsError {
    error("invalid_plugin", "O documento do plugin é inválido.")
}
fn plugin_home(home: &Path) -> PathBuf {
    crate::data_dir::root(home).join("plugins")
}

pub(crate) fn mcp_server_id(plugin_id: &str, name: &str) -> String {
    crate::mcp::plugin_server_id(plugin_id, &format!("mcp:{name}"))
}

pub(crate) fn validate_authoring_operation(operation: &Operation) -> Result<()> {
    fn sensitive(key: &str) -> bool {
        let key = key.to_ascii_lowercase().replace(['_', '-'], "");
        if key.ends_with("envvar") || key.ends_with("envvars") {
            return false;
        }
        [
            "apikey",
            "accesstoken",
            "refreshtoken",
            "token",
            "password",
            "secret",
            "authorization",
            "cookie",
            "clientsecret",
        ]
        .iter()
        .any(|part| key.contains(part))
    }
    fn inspect(value: &Value, parent: Option<&str>) -> bool {
        match value {
            Value::Object(map) => map.iter().any(|(key, value)| inspect(value, Some(key))),
            Value::Array(array) => array.iter().any(|value| inspect(value, parent)),
            Value::String(value) => {
                let literal = !value.trim().is_empty()
                    && !value.contains("${")
                    && !value.starts_with("$env:");
                (literal && parent.is_some_and(sensitive))
                    || url::Url::parse(value).is_ok_and(|url| {
                        url.password().is_some()
                            || (!url.username().is_empty() && url.scheme() != "ssh")
                    })
            }
            _ => false,
        }
    }
    fn command_contents<'a>(value: &'a Value, output: &mut Vec<&'a str>) {
        match value {
            Value::Object(map) => {
                for (key, value) in map {
                    if key == "command" {
                        if let Some(command) = value.as_str() {
                            output.push(command);
                        }
                    } else {
                        command_contents(value, output);
                    }
                }
            }
            Value::Array(values) => values
                .iter()
                .for_each(|value| command_contents(value, output)),
            _ => {}
        }
    }
    if let Operation::Create { draft } = operation {
        let serialized = serde_json::to_value(draft).map_err(json_error)?;
        if inspect(&serialized, None) {
            return Err(error("plugin_credentials_require_user", "Não coloque credenciais na proposta do agente. Use variáveis de ambiente ou a configuração privada do componente."));
        }
        let assignments = regex::Regex::new(r#"(?im)(?:(?:api[_-]?key|access[_-]?token|refresh[_-]?token|password|client[_-]?secret|authorization)\s*[:=]\s*|--(?:api-key|token|password)(?:\s+|=))[\"']?([^\r\n]+)"#).map_err(|_| error("plugins_storage", "Não foi possível validar a proposta."))?;
        let mut commands = Vec::new();
        command_contents(&serialized, &mut commands);
        for content in draft
            .files
            .iter()
            .map(|file| file.content.as_str())
            .chain(draft.skills.iter().map(|skill| skill.content.as_str()))
            .chain(commands)
        {
            if serde_json::from_str::<Value>(content).is_ok_and(|value| inspect(&value, None)) {
                return Err(error(
                    "plugin_credentials_require_user",
                    "Um arquivo da proposta contém credenciais literais.",
                ));
            }
            if assignments.captures_iter(content).any(|capture| {
                capture.get(1).is_some_and(|value| {
                    !value.as_str().contains("${")
                        && !value.as_str().contains("process.env")
                        && !value.as_str().contains("std::env")
                        && !value
                            .as_str()
                            .trim_start_matches([' ', '\"', '\''])
                            .starts_with('$')
                        && !value.as_str().trim_matches([' ', '\"', '\'']).is_empty()
                })
            }) {
                return Err(error(
                    "plugin_credentials_require_user",
                    "Use variáveis para credenciais nos arquivos da proposta.",
                ));
            }
        }
    }
    Ok(())
}

pub(crate) fn operation_schema() -> Value {
    use serde_json::json;
    fn variant(action: &str, fields: Vec<(&str, Value)>) -> Value {
        let mut properties = serde_json::Map::new();
        properties.insert("action".into(), json!({"type":"string","enum":[action]}));
        let mut required = vec![Value::String("action".into())];
        for (name, value) in fields {
            properties.insert(name.into(), value);
            required.push(Value::String(name.into()));
        }
        json!({"type":"object","properties":properties,"required":required,"additionalProperties":false})
    }
    let string = || json!({"type":"string"});
    let optional = || json!({"type":["string","null"]});
    let boolean = || json!({"type":"boolean"});
    json!({"anyOf":[
        variant("addMarketplace",vec![("source",string()),("refName",optional()),("sparsePaths",json!({"type":"array","items":{"type":"string"},"maxItems":64}))]),
        variant("refreshMarketplace",vec![("marketplaceId",optional())]),
        variant("removeMarketplace",vec![("marketplaceId",string())]),
        variant("install",vec![("pluginId",string())]), variant("update",vec![("pluginId",string())]), variant("uninstall",vec![("pluginId",string())]),
        variant("setEnabled",vec![("pluginId",string()),("enabled",boolean()),("projectPath",optional())]),
        variant("configureComponent",vec![("pluginId",string()),("componentId",string()),("enabled",boolean())]),
        variant("trustHooks",vec![("pluginId",string()),("trusted",boolean())]), variant("setAppsAccount",vec![("accountId",optional())]),
        variant("import",vec![("path",string())]),
        variant("create",vec![("draft",json!({"type":"object","properties":{
            "name":{"type":"string","minLength":1,"maxLength":64},"description":{"type":"string","maxLength":8192},
            "skills":{"type":"array","maxItems":64,"items":{"type":"object","properties":{"name":{"type":"string"},"content":{"type":"string","maxLength":262144}},"required":["name","content"],"additionalProperties":false}},
            "mcpServers":{"type":"object","additionalProperties":{"type":"object"}},"hooks":{"type":["object","null"]},"apps":{"type":"object","additionalProperties":{"type":"object"}},
            "files":{"type":"array","maxItems":128,"items":{"type":"object","properties":{"path":{"type":"string"},"content":{"type":"string","maxLength":1048576}},"required":["path","content"],"additionalProperties":false}}
        },"required":["name","description","skills","mcpServers","hooks","apps","files"],"additionalProperties":false}))])
    ]})
}
