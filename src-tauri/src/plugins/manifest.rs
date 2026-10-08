use super::*;
use std::{fs, io::Read, path::Component as PathComponent};

const PLUGIN_SCHEMA: &str = "https://agent-plugins.org/schemas/1.0.0/plugin.schema.json";
const MCP_SCHEMA: &str = "https://agent-plugins.org/schemas/1.0.0/mcp.schema.json";
const MANIFESTS: [&str; 3] = [
    ".codex-plugin/plugin.json",
    ".claude-plugin/plugin.json",
    ".cursor-plugin/plugin.json",
];
pub(super) const MARKETPLACES: [&str; 4] = [
    ".agents/plugins/marketplace.json",
    ".agents/plugins/api_marketplace.json",
    ".claude-plugin/marketplace.json",
    ".cursor-plugin/marketplace.json",
];
const MAX_DOCUMENT: u64 = 4 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct NamedDocument {
    pub name: String,
    pub definition: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct Parsed {
    pub name: String,
    pub display_name: String,
    pub description: String,
    pub version: String,
    pub portable: bool,
    pub components: Vec<Component>,
    pub skills: Vec<String>,
    pub mcp: BTreeMap<String, Value>,
    pub hooks: Vec<NamedDocument>,
    pub apps: BTreeMap<String, Value>,
    pub commands: Vec<String>,
    pub warnings: Vec<String>,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub short_description: Option<String>,
}

pub(super) fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && !name.starts_with('.')
        && !name.ends_with('.')
        && !name.contains("..")
        && name
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"._-".contains(&c))
}

pub(super) fn relative(path: &str, require_dot: bool) -> Result<PathBuf> {
    if path.is_empty()
        || path.contains('\\')
        || path.contains(':')
        || path.chars().any(char::is_control)
        || (require_dot && !path.starts_with("./"))
    {
        return Err(error(
            "invalid_plugin_path",
            "Use um caminho relativo ao pacote, sem subir de pasta.",
        ));
    }
    let path = Path::new(path);
    if path.is_absolute()
        || path
            .components()
            .any(|p| !matches!(p, PathComponent::Normal(_) | PathComponent::CurDir))
    {
        return Err(error(
            "invalid_plugin_path",
            "O caminho sai da pasta do plugin.",
        ));
    }
    let relative: PathBuf = path
        .components()
        .filter_map(|p| match p {
            PathComponent::Normal(p) => Some(p),
            _ => None,
        })
        .collect();
    if require_dot && relative.as_os_str().is_empty() {
        return Err(error(
            "invalid_plugin_path",
            "O caminho do componente não pode ser a raiz do pacote.",
        ));
    }
    Ok(relative)
}

pub(super) fn document(path: &Path) -> Result<Value> {
    let metadata = fs::symlink_metadata(path).map_err(io_error)?;
    if !metadata.is_file() || metadata.len() > MAX_DOCUMENT {
        return Err(error(
            "invalid_plugin",
            "O documento não é um arquivo regular ou excede 4 MiB.",
        ));
    }
    let mut bytes = Vec::new();
    fs::File::open(path)
        .map_err(io_error)?
        .take(MAX_DOCUMENT + 1)
        .read_to_end(&mut bytes)
        .map_err(io_error)?;
    if bytes.len() as u64 > MAX_DOCUMENT {
        return Err(error("invalid_plugin", "O documento excede 4 MiB."));
    }
    serde_json::from_slice(&bytes).map_err(json_error)
}

fn string(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_owned()
}
fn paths(value: Option<&Value>) -> Result<Vec<String>> {
    match value {
        None => Ok(Vec::new()),
        Some(Value::String(s)) => Ok(vec![s.clone()]),
        Some(Value::Array(values)) => values
            .iter()
            .map(|v| {
                v.as_str().map(str::to_owned).ok_or_else(|| {
                    error(
                        "invalid_plugin",
                        "Os caminhos dos componentes devem ser textos.",
                    )
                })
            })
            .collect(),
        _ => Err(error(
            "invalid_plugin",
            "Os caminhos dos componentes são inválidos.",
        )),
    }
}

fn component(id: &str, name: &str, kind: ComponentKind, detail: &str) -> Component {
    Component {
        id: id.into(),
        name: name.into(),
        kind,
        enabled: true,
        trusted: false,
        supported: true,
        detail: detail.into(),
        mcp_server_id: None,
        mcp_oauth: false,
        app_connect_url: None,
    }
}

fn contained(root: &Path, path: &str) -> Result<PathBuf> {
    let path = root.join(relative(path, true)?);
    if path.exists()
        && !fs::canonicalize(&path)
            .map_err(io_error)?
            .starts_with(fs::canonicalize(root).map_err(io_error)?)
    {
        return Err(error(
            "invalid_plugin_path",
            "O componente sai da pasta do plugin.",
        ));
    }
    Ok(path)
}

pub(super) fn plugin_document(root: &Path) -> Result<(Value, bool)> {
    let portable_path = root.join("plugin.json");
    let mut portable = false;
    let mut selected = None;
    if fs::symlink_metadata(&portable_path).is_ok() {
        let doc = contained_document(root, &portable_path)?;
        if let Some(schema) = doc.get("$schema").and_then(Value::as_str) {
            if schema == PLUGIN_SCHEMA {
                portable = true;
                selected = Some(doc);
            } else if schema.starts_with("https://agent-plugins.org/") {
                return Err(error(
                    "unsupported_plugin_schema",
                    "Esta versão do formato portátil ainda não é suportada.",
                ));
            }
        }
    }
    if selected.is_none() {
        for path in MANIFESTS {
            let path = root.join(path);
            if path.exists() {
                selected = Some(contained_document(root, &path)?);
                break;
            }
        }
    }
    let manifest = selected.ok_or_else(|| {
        error(
            "missing_plugin_manifest",
            "Não foi encontrado um manifest de plugin compatível com Codex.",
        )
    })?;
    if !manifest.is_object() {
        return Err(error(
            "invalid_plugin",
            "O manifest deve ser um objeto JSON.",
        ));
    }
    Ok((manifest, portable))
}

fn contained_document(root: &Path, path: &Path) -> Result<Value> {
    if !fs::canonicalize(path)
        .map_err(io_error)?
        .starts_with(fs::canonicalize(root).map_err(io_error)?)
    {
        return Err(error(
            "invalid_plugin_path",
            "O manifest sai da pasta do pacote.",
        ));
    }
    document(path)
}

pub(super) fn parse(root: &Path) -> Result<Parsed> {
    let (manifest, portable) = plugin_document(root)?;
    if portable {
        for key in [
            "name",
            "version",
            "description",
            "homepage",
            "repository",
            "license",
        ] {
            if manifest.get(key).is_some_and(|value| !value.is_string()) {
                return Err(error(
                    "invalid_plugin",
                    format!("O campo portátil {key} precisa ser um texto."),
                ));
            }
        }
    }
    let mut name = string(&manifest, "name");
    if name.is_empty() && !portable {
        name = root
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .to_owned();
    }
    if portable
        && (!name
            .as_bytes()
            .first()
            .is_some_and(u8::is_ascii_alphanumeric)
            || !name
                .as_bytes()
                .last()
                .is_some_and(u8::is_ascii_alphanumeric))
    {
        return Err(error(
            "invalid_plugin_name",
            "O nome portátil deve começar e terminar com letra ou número.",
        ));
    }
    if !valid_name(&name)
        || (portable
            && (name != name.to_ascii_lowercase() || name.contains("--") || name.contains('_')))
    {
        return Err(error("invalid_plugin_name", "O nome do plugin é inválido."));
    }
    let mut version = string(&manifest, "version");
    if version.is_empty() {
        version = if portable { "1.0.0" } else { "local" }.into();
    }
    let metadata = super::presentation::fields(&manifest);
    let display_name = manifest
        .pointer("/interface/displayName")
        .and_then(Value::as_str)
        .filter(|v| !v.trim().is_empty())
        .unwrap_or(&name)
        .to_owned();
    let mut out = Parsed {
        name,
        display_name,
        description: metadata.description.unwrap_or_default(),
        version,
        portable,
        components: Vec::new(),
        skills: Vec::new(),
        mcp: BTreeMap::new(),
        hooks: Vec::new(),
        apps: BTreeMap::new(),
        commands: Vec::new(),
        warnings: Vec::new(),
        category: metadata.category,
        short_description: metadata.short_description,
    };
    let skill_paths = if portable {
        vec!["./skills".into()]
    } else {
        let explicit = paths(manifest.get("skills"))?;
        if explicit.is_empty() {
            vec!["./skills".into()]
        } else {
            explicit
        }
    };
    for path in skill_paths {
        let resolved = contained(root, &path)?;
        if resolved.exists() {
            let id = format!("skills:{path}");
            out.components
                .push(component(&id, "Skills", ComponentKind::Skills, &path));
            out.skills.push(path);
        }
    }
    if !portable && root.join(".jarvis-command-skills").is_dir() {
        out.skills.push("./.jarvis-command-skills".into());
        out.components.push(component(
            "skills:./.jarvis-command-skills",
            "Comandos",
            ComponentKind::Skills,
            "Comandos convertidos em skills",
        ));
    }
    let mcp_value = if portable {
        let path = root.join("mcp.json");
        if path.exists() {
            let doc = contained_document(root, &path)?;
            if doc.get("$schema").and_then(Value::as_str) != Some(MCP_SCHEMA) {
                return Err(error(
                    "unsupported_mcp_schema",
                    "O mcp.json portátil precisa declarar seu schema suportado.",
                ));
            }
            Some(doc)
        } else {
            None
        }
    } else {
        match manifest.get("mcpServers") {
            Some(Value::String(path)) => Some(document(&contained(root, path)?)?),
            Some(Value::Object(map)) => Some(Value::Object(map.clone())),
            Some(_) => return Err(error("invalid_plugin", "A configuração MCP é inválida.")),
            None => {
                let path = root.join(".mcp.json");
                if path.exists() {
                    Some(document(&path)?)
                } else {
                    None
                }
            }
        }
    };
    if let Some(doc) = mcp_value {
        let servers = doc
            .get("mcpServers")
            .unwrap_or(&doc)
            .as_object()
            .ok_or_else(|| error("invalid_plugin", "A lista de servidores MCP é inválida."))?;
        for (name, config) in servers {
            if name == "$schema" {
                continue;
            }
            if let Err(err) = validate_mcp(name, config, portable) {
                out.warnings.push(format!("MCP {name}: {}", err.message));
                continue;
            }
            let id = format!("mcp:{name}");
            let mut entry = component(
                &id,
                name,
                ComponentKind::Mcp,
                if config.get("command").is_some() {
                    "Processo local"
                } else {
                    "Servidor HTTP"
                },
            );
            entry.mcp_oauth =
                config.get("url").is_some() && config.get("bearer_token_env_var").is_none();
            out.components.push(entry);
            if let Some(command) = config.get("command").and_then(Value::as_str) {
                let args = config
                    .get("args")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .map(|arg| serde_json::to_string(arg).unwrap_or_default())
                    .collect::<Vec<_>>()
                    .join(" ");
                out.commands.push(format!(
                    "MCP {name}: {}",
                    redacted_command(&format!("{command} {args}"))
                ));
            }
            out.mcp.insert(name.clone(), config.clone());
        }
    }
    if portable {
        if manifest.get("hooks").is_some()
            || manifest.get("apps").is_some()
            || manifest.pointer("/extensions/com.openai/hooks").is_some()
            || manifest.pointer("/extensions/com.openai/apps").is_some()
            || root.join("hooks/hooks.json").exists()
            || root.join(".app.json").exists()
            || root.join(".codex-plugin/plugin.json").exists()
        {
            out.warnings.push("O formato portátil do Codex não ativa hooks ou apps; use o formato legado para esses componentes.".into());
        }
    } else {
        let hooks = match manifest.get("hooks") {
            Some(Value::String(path)) => vec![NamedDocument {
                name: path.clone(),
                definition: document(&contained(root, path)?)?,
            }],
            Some(Value::Array(items)) => items
                .iter()
                .enumerate()
                .map(|(i, value)| match value {
                    Value::String(path) => Ok(NamedDocument {
                        name: path.clone(),
                        definition: document(&contained(root, path)?)?,
                    }),
                    Value::Object(_) => Ok(NamedDocument {
                        name: format!("inline-{i}"),
                        definition: value.clone(),
                    }),
                    _ => Err(error("invalid_plugin", "A lista de hooks é inválida.")),
                })
                .collect::<Result<Vec<_>>>()?,
            Some(Value::Object(_)) => vec![NamedDocument {
                name: "inline".into(),
                definition: manifest["hooks"].clone(),
            }],
            Some(_) => {
                return Err(error(
                    "invalid_plugin",
                    "A configuração de hooks é inválida.",
                ))
            }
            None => {
                let path = root.join("hooks/hooks.json");
                if path.exists() {
                    vec![NamedDocument {
                        name: "./hooks/hooks.json".into(),
                        definition: document(&path)?,
                    }]
                } else {
                    Vec::new()
                }
            }
        };
        for hook in hooks {
            let normalized =
                validate_hooks(&hook.definition, &mut out.commands, &mut out.warnings)?;
            let mut entry = component(
                &format!("hooks:{}", hook.name),
                "Hooks",
                ComponentKind::Hooks,
                &hook.name,
            );
            entry.supported = normalized
                .get("hooks")
                .and_then(Value::as_object)
                .is_some_and(|events| !events.is_empty());
            entry.enabled = entry.supported;
            out.components.push(entry);
            out.hooks.push(NamedDocument {
                name: hook.name,
                definition: normalized,
            });
        }
        let apps_path = match manifest.get("apps") {
            Some(Value::String(path)) => Some(contained(root, path)?),
            Some(_) => return Err(error("invalid_plugin", "O caminho de apps é inválido.")),
            None => root
                .join(".app.json")
                .exists()
                .then(|| root.join(".app.json")),
        };
        if let Some(path) = apps_path {
            let apps = document(&path)?;
            let apps = apps
                .get("apps")
                .and_then(Value::as_object)
                .ok_or_else(|| error("invalid_plugin", "A lista de apps é inválida."))?;
            for (name, app) in apps {
                let id = string(app, "id");
                if id.is_empty() {
                    out.warnings
                        .push(format!("App {name}: identificador ausente."));
                    continue;
                }
                let Some(connect_url) = super::apps::connect_url(name, &id) else {
                    out.warnings
                        .push(format!("App {name}: identificador de conector inválido."));
                    continue;
                };
                let mut app_component = component(
                    &format!("apps:{name}"),
                    name,
                    ComponentKind::Apps,
                    "Requer conta ChatGPT e autorização do conector",
                );
                app_component.app_connect_url = Some(connect_url);
                out.components.push(app_component);
                out.apps.insert(name.clone(), app.clone());
            }
        }
    }
    if out.components.len() > 256 {
        return Err(error(
            "plugin_too_large",
            "O plugin excede 256 componentes.",
        ));
    }
    for (field, warning) in [
        (
            "lspServers",
            "Servidores LSP deste pacote não são ativados pelo Jarvis.",
        ),
        (
            "agents",
            "Agentes declarados neste pacote não são importados como agentes Jarvis.",
        ),
    ] {
        if manifest.get(field).is_some_and(|value| !value.is_null()) {
            out.warnings.push(warning.into());
        }
    }
    Ok(out)
}

fn validate_mcp(name: &str, config: &Value, portable: bool) -> Result<()> {
    if !valid_name(name) || !config.is_object() {
        return Err(error("invalid_mcp", "Nome ou configuração inválida."));
    }
    if config.get("args").is_some_and(|args| {
        !args
            .as_array()
            .is_some_and(|args| args.iter().all(Value::is_string))
    }) {
        return Err(error(
            "invalid_mcp",
            "Os argumentos MCP devem ser uma lista de textos.",
        ));
    }
    for key in ["env", "http_headers", "headers"] {
        if config.get(key).is_some_and(|values| {
            !values
                .as_object()
                .is_some_and(|values| values.values().all(Value::is_string))
        }) {
            return Err(error(
                "invalid_mcp",
                "As variáveis e os headers MCP devem conter textos.",
            ));
        }
    }
    if let Some(url) = config.get("url").and_then(Value::as_str) {
        let url = url::Url::parse(url).map_err(|_| error("invalid_mcp", "URL inválida."))?;
        if !url.username().is_empty() || url.password().is_some() || url.fragment().is_some() {
            return Err(error(
                "invalid_mcp",
                "A URL não pode conter credenciais ou fragmentos.",
            ));
        }
        let loopback = matches!(
            url.host_str(),
            Some("localhost" | "127.0.0.1" | "[::1]" | "::1")
        );
        if url.scheme() != "https" && !(url.scheme() == "http" && loopback) {
            return Err(error("invalid_mcp", "Use HTTPS ou HTTP local."));
        }
        if portable && config.get("type").and_then(Value::as_str) != Some("streamable-http") {
            return Err(error(
                "unsupported_mcp",
                "O transporte portátil precisa ser streamable-http.",
            ));
        }
    } else {
        let command = config
            .get("command")
            .and_then(Value::as_str)
            .filter(|s| !s.trim().is_empty())
            .ok_or_else(|| error("invalid_mcp", "Comando ausente."))?;
        if portable && config.get("type").and_then(Value::as_str) != Some("stdio") {
            return Err(error(
                "unsupported_mcp",
                "O transporte portátil precisa ser stdio.",
            ));
        }
        if portable && (command.contains('/') || command.contains('\\') || command.contains(':')) {
            relative(command, true)?;
        }
    }
    if let Some(env) = config.get("env").and_then(Value::as_object) {
        for key in env.keys() {
            if matches!(
                key.to_ascii_uppercase().as_str(),
                "PLUGIN_ROOT" | "PLUGIN_DATA"
            ) {
                return Err(error(
                    "invalid_mcp",
                    "O ambiente não pode substituir PLUGIN_ROOT ou PLUGIN_DATA.",
                ));
            }
        }
    }
    Ok(())
}

fn validate_hooks(
    doc: &Value,
    commands: &mut Vec<String>,
    warnings: &mut Vec<String>,
) -> Result<Value> {
    let events = doc.get("hooks").and_then(Value::as_object).ok_or_else(|| {
        error(
            "invalid_hooks",
            "O documento deve conter hooks agrupados por evento.",
        )
    })?;
    let supported = [
        "PreToolUse",
        "PermissionRequest",
        "PostToolUse",
        "PreCompact",
        "PostCompact",
        "SessionStart",
        "SessionEnd",
        "UserPromptSubmit",
        "SubagentStart",
        "SubagentStop",
        "Stop",
        "Interrupt",
    ];
    let mut normalized = serde_json::Map::new();
    for (event, groups) in events {
        if !supported.contains(&event.as_str()) {
            warnings.push(format!("Evento de hook não suportado: {event}."));
            continue;
        }
        let groups = groups
            .as_array()
            .ok_or_else(|| error("invalid_hooks", "Os grupos de hooks devem ser listas."))?;
        let mut normalized_groups = Vec::new();
        for group in groups {
            if let Some(matcher) = group.get("matcher").and_then(Value::as_str) {
                if !matcher.is_empty() && matcher != "*" {
                    regex::Regex::new(matcher)
                        .map_err(|_| error("invalid_hooks", "O matcher do hook é inválido."))?;
                }
            }
            let handlers = group
                .get("hooks")
                .and_then(Value::as_array)
                .ok_or_else(|| {
                    error("invalid_hooks", "Cada grupo precisa de uma lista de hooks.")
                })?;
            let mut normalized_handlers = Vec::new();
            for handler in handlers {
                if handler
                    .get("timeout")
                    .is_some_and(|timeout| timeout.as_u64().is_none())
                {
                    return Err(error(
                        "invalid_hooks",
                        "O tempo limite do hook precisa ser um número positivo.",
                    ));
                }
                match handler.get("type").and_then(Value::as_str) {
                    Some("command") => {
                        let command = handler
                            .get("command")
                            .and_then(Value::as_str)
                            .filter(|s| !s.trim().is_empty())
                            .ok_or_else(|| error("invalid_hooks", "Comando de hook ausente."))?;
                        commands.push(format!("{event}: {}", redacted_command(command)));
                        if let Some(command) = handler
                            .get("commandWindows")
                            .or_else(|| handler.get("command_windows"))
                            .and_then(Value::as_str)
                        {
                            commands
                                .push(format!("{event} (Windows): {}", redacted_command(command)));
                        }
                        normalized_handlers.push(handler.clone());
                    }
                    Some("mcp_tool") if event != "SessionEnd" => {
                        if handler
                            .get("server")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .trim()
                            .is_empty()
                            || handler
                                .get("tool")
                                .and_then(Value::as_str)
                                .unwrap_or_default()
                                .trim()
                                .is_empty()
                            || handler.get("input").is_some_and(|input| !input.is_object())
                        {
                            return Err(error(
                                "invalid_hooks",
                                "O hook MCP precisa de servidor, ferramenta e argumentos válidos.",
                            ));
                        }
                        normalized_handlers.push(handler.clone());
                    }
                    Some(kind) => warnings.push(format!(
                        "Hook {event} do tipo {kind} será ignorado, como no host Codex."
                    )),
                    None => return Err(error("invalid_hooks", "Tipo do hook ausente.")),
                }
            }
            if !normalized_handlers.is_empty() {
                let mut group = group.clone();
                group["hooks"] = Value::Array(normalized_handlers);
                normalized_groups.push(group);
            }
        }
        if !normalized_groups.is_empty() {
            normalized.insert(event.clone(), Value::Array(normalized_groups));
        }
    }
    Ok(serde_json::json!({"hooks":normalized}))
}

fn redacted_command(command: &str) -> String {
    let lowercase = command.to_ascii_lowercase();
    let sensitive = [
        "token=",
        "password=",
        "api_key=",
        "apikey=",
        "--api-key",
        "--token",
        "--password",
        "authorization:",
        "bearer ",
        "sk-proj-",
        "sk-ant-",
    ];
    if command.len() > 2048 {
        "[comando longo; revise o arquivo do pacote]".into()
    } else if sensitive.iter().any(|key| lowercase.contains(key)) {
        "[comando contém configuração sensível; revise o arquivo do pacote]".into()
    } else {
        command.into()
    }
}

pub(super) fn migrate_commands(root: &Path) -> Result<()> {
    // Migration happens only inside the private owned staging package.
    if root.join("plugin.json").exists()
        && document(&root.join("plugin.json"))?
            .get("$schema")
            .and_then(Value::as_str)
            == Some(PLUGIN_SCHEMA)
    {
        return Ok(());
    }
    let mut manifest = None;
    for path in MANIFESTS {
        if root.join(path).exists() {
            manifest = Some(document(&root.join(path))?);
            break;
        }
    }
    let command_paths = paths(manifest.as_ref().and_then(|m| m.get("commands")))?;
    let command_paths = if command_paths.is_empty() {
        vec!["./commands".into()]
    } else {
        command_paths
    };
    for path in command_paths {
        let directory = contained(root, &path)?;
        if !directory.is_dir() {
            continue;
        }
        for entry in fs::read_dir(directory).map_err(io_error)? {
            let path = entry.map_err(io_error)?.path();
            if path.extension().and_then(|e| e.to_str()) != Some("md")
                || !fs::symlink_metadata(&path).map_err(io_error)?.is_file()
            {
                continue;
            }
            let name = path
                .file_stem()
                .and_then(|n| n.to_str())
                .unwrap_or_default();
            if !valid_name(name) {
                continue;
            }
            let content = fs::read_to_string(&path).map_err(io_error)?;
            if content.len() > 64 * 1024
                || content.contains("$ARGUMENTS")
                || content.contains("$1")
                || content.contains("!`")
            {
                continue;
            }
            let dir = root
                .join(".jarvis-command-skills")
                .join(format!("command-{name}"));
            fs::create_dir_all(&dir).map_err(io_error)?;
            let (description, body) = content
                .strip_prefix("---\n")
                .and_then(|text| text.split_once("\n---\n"))
                .map_or(
                    ("Comando importado do plugin".to_owned(), content.as_str()),
                    |(frontmatter, body)| {
                        let description =
                            serde_yaml_ng::from_str::<serde_yaml_ng::Value>(frontmatter)
                                .ok()
                                .and_then(|value| {
                                    value
                                        .get("description")
                                        .and_then(serde_yaml_ng::Value::as_str)
                                        .map(str::to_owned)
                                })
                                .unwrap_or_else(|| "Comando importado do plugin".into());
                        (description, body)
                    },
                );
            let description = serde_json::to_string(&description).map_err(json_error)?;
            let skill =
                format!("---\nname: command-{name}\ndescription: {description}\n---\n\n{body}");
            fs::write(dir.join("SKILL.md"), skill).map_err(io_error)?;
        }
    }
    Ok(())
}

pub(super) fn marketplace_document(root: &Path) -> Result<(Value, bool)> {
    let selected = MARKETPLACES
        .iter()
        .find_map(|path| root.join(path).is_file().then(|| root.join(path)))
        .ok_or_else(|| {
            error(
                "missing_marketplace",
                "Não foi encontrado um marketplace compatível.",
            )
        })?;
    let cursor = selected.ends_with(".cursor-plugin/marketplace.json");
    let doc = contained_document(root, &selected)?;
    Ok((doc, cursor))
}

pub(super) fn marketplace(
    root: &Path,
    id: &str,
) -> Result<(String, Vec<AvailablePlugin>, Vec<String>)> {
    let (doc, cursor) = marketplace_document(root)?;
    let name = string(&doc, "name");
    if !valid_name(&name) || name.contains('.') {
        return Err(error(
            "invalid_marketplace",
            "O nome do marketplace é inválido.",
        ));
    }
    let entries = doc
        .get("plugins")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            error(
                "invalid_marketplace",
                "A lista de plugins do marketplace é inválida.",
            )
        })?;
    if entries.len() > 2048 {
        return Err(error(
            "marketplace_too_large",
            "O marketplace excede 2.048 plugins.",
        ));
    }
    let mut out = Vec::new();
    let mut warnings = Vec::new();
    for entry in entries {
        let plugin = string(entry, "name");
        if !valid_name(&plugin) || out.iter().any(|p: &AvailablePlugin| p.name == plugin) {
            warnings.push("Entrada de plugin inválida ou duplicada ignorada.".into());
            continue;
        }
        let source = match source::marketplace_package(entry.get("source"), root, cursor) {
            Ok(source) => source,
            Err(err) => {
                warnings.push(format!("{plugin}: {}", err.message));
                continue;
            }
        };
        let installation = entry
            .pointer("/policy/installation")
            .and_then(Value::as_str)
            .unwrap_or("AVAILABLE");
        let authentication = entry
            .pointer("/policy/authentication")
            .and_then(Value::as_str)
            .unwrap_or("ON_INSTALL")
            .to_owned();
        let products: Vec<_> = entry
            .pointer("/policy/products")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .collect();
        let mut requirements = Vec::new();
        let product_allowed = products.is_empty()
            || products.iter().any(|product| {
                product.eq_ignore_ascii_case("codex") || product.eq_ignore_ascii_case("jarvis")
            });
        if !product_allowed {
            requirements.push(format!("Restrito aos produtos: {}", products.join(", ")));
        }
        let mut available = AvailablePlugin {
            id: format!("{plugin}@{id}"),
            name: plugin.clone(),
            marketplace_id: id.into(),
            display_name: entry
                .pointer("/interface/displayName")
                .and_then(Value::as_str)
                .unwrap_or(&plugin)
                .into(),
            description: string(entry, "description"),
            version: entry
                .get("version")
                .and_then(Value::as_str)
                .map(str::to_owned),
            source,
            installable: installation != "NOT_AVAILABLE" && product_allowed,
            authentication,
            requirements,
            category: None,
            short_description: None,
            icon_data_url: None,
        };
        super::presentation::enrich(&mut available, entry, false);
        out.push(available);
    }
    Ok((name, out, warnings))
}
