//! Small subscription-only sessions; the official host owns credentials and hosted search.
use super::transport::{MuseControl, MuseProcess, MuseSpawnConfig};
use base64::Engine;
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    ffi::OsString,
    path::Path,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::sync::watch;

const MAX_INPUT_BYTES: usize = 8 * 1024 * 1024;
const MAX_ANSWER_BYTES: usize = 256 * 1024;
const RUN_TIMEOUT: Duration = Duration::from_secs(80);
const CANCELLED: &str = "A operação do Muse Code foi cancelada.";

pub(crate) async fn initialize_tools(process: &mut MuseProcess) -> Result<Value, String> {
    let info = process
        .initialize(&json!({"capabilities":{
            "experimentalApi":true,"userInputDialogs":false,"requestedCapabilities":["sessionMcp"]
        }}))
        .await?;
    if info["experimentalApi"] != true
        || !info["grantedCapabilities"]
            .as_array()
            .is_some_and(|grants| grants.iter().any(|grant| grant == "sessionMcp"))
    {
        return Err("O Muse Code não autorizou a ponte de ferramentas. Atualize o CLI oficial e tente novamente.".into());
    }
    Ok(info)
}

pub(crate) fn configure(
    config: &mut MuseSpawnConfig,
    home: &Path,
    toolset: &[String],
) -> Result<(), String> {
    if !home.is_absolute()
        || !home.is_dir()
        || toolset
            .iter()
            .any(|tool| !matches!(tool.as_str(), "web_search" | "mcp__jarvis__*"))
        || (toolset.iter().any(|tool| tool == "web_search") && toolset.len() != 1)
        || config.args.first().is_none_or(|arg| arg != "serve")
        || config
            .args
            .iter()
            .any(|arg| arg == "--trust-workspace" || arg == "--disable-sandbox")
    {
        return Err("Configuração isolada do Muse Code inválida.".into());
    }
    let provider = config.args.iter().position(|arg| arg == "--provider");
    if provider.is_some_and(|index| {
        config
            .args
            .get(index + 1)
            .is_none_or(|value| value != "meta")
    }) {
        return Err("O perfil do Jarvis exige o provedor de assinatura do Muse Code.".into());
    }
    let base = crate::data_dir::root(home).join("muse");
    for path in [
        base.clone(),
        base.join("config"),
        base.join("config/muse"),
        base.join("data"),
        base.join("state"),
    ] {
        if std::fs::symlink_metadata(&path).is_ok_and(|metadata| metadata.file_type().is_symlink())
        {
            return Err("O perfil privado do Muse Code não pode redirecionar pastas.".into());
        }
        std::fs::create_dir_all(&path)
            .map_err(|_| "Não foi possível preparar o perfil privado do Muse Code.")?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700))
                .map_err(|_| "Não foi possível proteger o perfil privado do Muse Code.")?;
        }
    }
    let managed = [
        "XDG_CONFIG_HOME",
        "XDG_DATA_HOME",
        "XDG_STATE_HOME",
        "MUSE_AUTH_PATH",
    ];
    config.env.retain(|(key, _)| {
        !managed
            .iter()
            .any(|name| key.to_string_lossy().eq_ignore_ascii_case(name))
    });
    config.env.extend([
        (
            OsString::from("XDG_CONFIG_HOME"),
            base.join("config").into_os_string(),
        ),
        (
            OsString::from("XDG_DATA_HOME"),
            base.join("data").into_os_string(),
        ),
        (
            OsString::from("XDG_STATE_HOME"),
            base.join("state").into_os_string(),
        ),
        // Current hosts resolve auth through XDG_CONFIG_HOME; future override-aware hosts use the same file.
        (
            OsString::from("MUSE_AUTH_PATH"),
            base.join("config/muse/auth.json").into_os_string(),
        ),
    ]);
    if provider.is_none() {
        config.args.extend(["--provider".into(), "meta".into()]);
    }
    // These flags add global read-only developer instructions, including for MCP tools.
    if !toolset.iter().any(|tool| tool == "mcp__jarvis__*") {
        config
            .args
            .extend(["--disable-write".into(), "--disable-shell".into()]);
    }
    config.args.push("--disable-request-user-input".into());
    let web = if toolset.iter().any(|tool| tool == "web_search") {
        "hosted"
    } else {
        "off"
    };
    for setting in [
        format!("run.toolset={}", json!(toolset)),
        "run.workflow_trigger_mode=\"off\"".into(),
        "run.reminder_roster=[]".into(),
        "run.parallel_tool_calls=false".into(),
        "context.foreign_personal_rules=false".into(),
        "context.foreign_personal_skills=false".into(),
        "hooks={}".into(),
        format!("tools.web_search.mode=\"{web}\""),
    ] {
        config.args.extend(["-c".into(), setting.into()]);
    }
    Ok(())
}

pub(crate) fn command_id() -> Result<String, String> {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| "Não foi possível identificar a operação do Muse Code.")?
        .as_millis();
    if millis >= 1_u128 << 48 {
        return Err("O relógio não permite identificar a operação do Muse Code.".into());
    }
    let mut bytes = [0_u8; 16];
    getrandom::fill(&mut bytes)
        .map_err(|_| "Não foi possível identificar a operação do Muse Code.")?;
    bytes[..6].copy_from_slice(&(millis as u64).to_be_bytes()[2..]);
    bytes[6] = (bytes[6] & 0x0f) | 0x70;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let hex: String = bytes.iter().map(|byte| format!("{byte:02x}")).collect();
    Ok(format!(
        "{}-{}-{}-{}-{}",
        &hex[..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..]
    ))
}

pub(crate) fn turn_input(prompt: &str, input: &Value) -> Result<Value, String> {
    let input = input.as_array().ok_or("Entrada do Muse Code inválida.")?;
    if input.len() > 64 || prompt.len() > MAX_ANSWER_BYTES {
        return Err("A entrada do Muse Code excede o limite permitido.".into());
    }
    let mut parts = Vec::new();
    let mut size = prompt.len();
    if !prompt.trim().is_empty() {
        parts.push(json!({"type":"text", "text":prompt}));
    }
    for part in input {
        let value = match part["type"].as_str() {
            Some("text") => {
                let text = part["text"]
                    .as_str()
                    .ok_or("Parte de texto do Muse inválida.")?;
                size = size.saturating_add(text.len());
                if size > MAX_INPUT_BYTES {
                    return Err("A entrada do Muse Code excede o limite permitido.".into());
                }
                json!({"type":"text", "text":text})
            }
            Some("image") => {
                let data = part["base64Data"]
                    .as_str()
                    .ok_or("Imagem do Muse inválida.")?;
                let media = part["mediaType"]
                    .as_str()
                    .ok_or("Imagem do Muse inválida.")?;
                size = size.saturating_add(data.len());
                if size > MAX_INPUT_BYTES
                    || data.is_empty()
                    || !matches!(
                        media,
                        "image/png" | "image/jpeg" | "image/gif" | "image/webp"
                    )
                    || base64::engine::general_purpose::STANDARD
                        .decode(data)
                        .is_err()
                {
                    return Err("Imagem do Muse inválida ou acima do limite permitido.".into());
                }
                let mut image = json!({"type":"image", "base64Data":data, "mediaType":media});
                if part.get("width").is_some() || part.get("height").is_some() {
                    let width = part["width"]
                        .as_u64()
                        .filter(|value| (1..=32768).contains(value))
                        .ok_or("Dimensões da imagem inválidas.")?;
                    let height = part["height"]
                        .as_u64()
                        .filter(|value| (1..=32768).contains(value))
                        .ok_or("Dimensões da imagem inválidas.")?;
                    image["width"] = json!(width);
                    image["height"] = json!(height);
                }
                image
            }
            _ => return Err("O Muse aceita apenas texto e imagens nesta operação.".into()),
        };
        parts.push(value);
    }
    if parts.is_empty() || parts.len() > 64 {
        return Err("Envie uma entrada de texto ou imagem para o Muse Code.".into());
    }
    let value = Value::Array(parts);
    if serde_json::to_vec(&value)
        .map_err(|_| "Entrada do Muse Code inválida.")?
        .len()
        > MAX_INPUT_BYTES
    {
        return Err("A entrada do Muse Code excede o limite permitido.".into());
    }
    Ok(value)
}

pub(crate) fn validate_account(account: &Value) -> Result<(), String> {
    match account["state"].as_str() {
        Some("accountLogin") => Ok(()),
        Some("apiKey" | "envKey") => Err("O Muse Code precisa usar a assinatura; cobrança por chave de API não é permitida.".into()),
        _ => Err("Conecte a assinatura do Muse Code no perfil privado do Jarvis em Configurações → Provedores.".into()),
    }
}

pub(crate) fn validate_model(
    catalog: &Value,
    model: &str,
    reasoning: Option<&str>,
) -> Result<(), String> {
    if model.trim().is_empty()
        || model.len() > 256
        || model.starts_with('-')
        || model.chars().any(char::is_control)
        || catalog["providerId"] != "meta"
    {
        return Err("Modelo de assinatura do Muse Code inválido.".into());
    }
    let rows = catalog["models"]
        .as_array()
        .filter(|rows| rows.len() <= 256)
        .ok_or("O Muse Code não retornou um catálogo de modelos válido.")?;
    let mut matches = rows
        .iter()
        .filter(|row| row["providerId"] == "meta" && row["modelId"] == model);
    let row = matches
        .next()
        .ok_or("O modelo selecionado não está no catálogo atual do Muse Code.")?;
    if matches.next().is_some()
        || reasoning.is_some_and(|effort| {
            !row["variants"]
                .as_array()
                .is_some_and(|variants| variants.iter().any(|variant| variant == effort))
        })
    {
        return Err(
            "O esforço selecionado não está disponível para este modelo do Muse Code.".into(),
        );
    }
    Ok(())
}

fn valid_uuid(value: &str) -> bool {
    value.len() == 36
        && value.bytes().enumerate().all(|(index, byte)| {
            if matches!(index, 8 | 13 | 18 | 23) {
                byte == b'-'
            } else {
                byte.is_ascii_hexdigit()
            }
        })
}

pub(crate) fn session_id(
    result: &Value,
    root: &Path,
    model: &str,
    expected_id: Option<&str>,
) -> Result<String, String> {
    let session = &result["session"];
    let id = session["sessionId"]
        .as_str()
        .filter(|id| valid_uuid(id))
        .ok_or("O Muse Code retornou uma sessão inválida.")?;
    let workspace = session["workspaceRoot"]
        .as_str()
        .ok_or("A sessão Muse não confirmou a pasta de trabalho.")?;
    let expected = root
        .canonicalize()
        .map_err(|_| "A pasta de trabalho do Muse não está disponível.")?;
    if expected_id.is_some_and(|expected| expected != id)
        || !Path::new(workspace).is_absolute()
        || Path::new(workspace).canonicalize().ok().as_ref() != Some(&expected)
        || session["providerId"] != "meta"
        || session["modelId"] != model
        || result["viewCursor"]
            .as_str()
            .is_none_or(|cursor| cursor.is_empty() || cursor.len() > 4096)
    {
        return Err("O Muse Code abriu uma sessão diferente da solicitada.".into());
    }
    Ok(id.into())
}

pub(crate) fn turn_id(ack: &Value, command: &str) -> Result<String, String> {
    let id = ack["turnId"].as_str().filter(|id| valid_uuid(id));
    if ack["commandId"] != command
        || ack["status"] != "accepted"
        || ack["disposition"] != "started"
        || ack["startedNewTurn"] != true
    {
        return Err(
            "O Muse Code não iniciou o turno solicitado. Confira o estado antes de continuar."
                .into(),
        );
    }
    id.map(str::to_owned)
        .ok_or_else(|| "O Muse Code não confirmou o turno.".into())
}

fn ephemeral_config(
    home: &Path,
    root: &Path,
    toolset: &[String],
) -> Result<MuseSpawnConfig, String> {
    let executable = super::executable()
        .ok_or("Instale o CLI oficial do Muse Code e conecte a assinatura no Jarvis.")?;
    let mut config = MuseSpawnConfig::new(executable, root.into());
    configure(&mut config, home, toolset)?;
    config.args.push("--no-session-log".into());
    Ok(config)
}

pub(crate) async fn text(
    home: &Path,
    root: &Path,
    model: &str,
    reasoning: Option<&str>,
    prompt: &str,
    input: &Value,
    signal: watch::Receiver<bool>,
) -> Result<String, String> {
    let config = ephemeral_config(home, root, &[])?;
    run_config(home, config, model, reasoning, prompt, input, signal, None)
        .await
        .map(|out| out.answer)
}

pub(crate) async fn search(
    home: &Path,
    model: &str,
    query: &str,
    limit: usize,
    response_language: crate::system::ResponseLanguage,
    signal: watch::Receiver<bool>,
) -> Result<Value, String> {
    if query.trim().is_empty() || query.len() > 2000 || !(1..=10).contains(&limit) {
        return Err("Informe uma busca de até 2.000 bytes e entre 1 e 10 fontes.".into());
    }
    let directory =
        tempfile::tempdir().map_err(|_| "Não foi possível preparar a busca do Muse Code.")?;
    let config = ephemeral_config(home, directory.path(), &["web_search".into()])?;
    search_with_config(home, config, model, query, limit, response_language, signal).await
}

async fn search_with_config(
    home: &Path,
    config: MuseSpawnConfig,
    model: &str,
    query: &str,
    limit: usize,
    response_language: crate::system::ResponseLanguage,
    signal: watch::Receiver<bool>,
) -> Result<Value, String> {
    let prompt = format!(
        "Use the hosted web_search tool for the user's query. Base the answer only on observed search results. Return a concise answer in the requested language. Do not access workspace or personal context. Requested language: {}. Requested source limit: {limit}.",
        json!(response_language.prompt_instruction()),
    );
    let output = run_config(
        home,
        config,
        model,
        None,
        &prompt,
        &json!([{"type":"text","text":query}]),
        signal,
        Some(limit),
    )
    .await?;
    Ok(
        json!({"accountAlias":"Muse Code", "model":model, "answer":output.answer, "sources":output.sources}),
    )
}

struct Output {
    answer: String,
    sources: Vec<Value>,
}

struct Collector {
    session: String,
    turn: String,
    search: bool,
    limit: usize,
    messages: HashMap<String, String>,
    order: Vec<String>,
    sources: Vec<Value>,
    searched: bool,
    completed: bool,
    events: usize,
}

impl Collector {
    fn new(session: &str, turn: &str, search: bool, limit: usize) -> Self {
        Self {
            session: session.into(),
            turn: turn.into(),
            search,
            limit,
            messages: HashMap::new(),
            order: Vec::new(),
            sources: Vec::new(),
            searched: false,
            completed: false,
            events: 0,
        }
    }

    fn apply(&mut self, event: &Value) -> Result<bool, String> {
        self.events += 1;
        if self.events > 4096 {
            return Err("O Muse Code excedeu o limite de eventos desta operação.".into());
        }
        let params = &event["params"];
        if params["sessionId"] != self.session {
            return Ok(false);
        }
        match event["method"].as_str() {
            Some("turn/completed") if params["turnId"] == self.turn => {
                if params["terminal"] != "completed" {
                    return Err("O Muse Code não concluiu a operação solicitada.".into());
                }
                self.completed = true;
                return Ok(true);
            }
            Some("item/completed") => {
                let item = &params["item"];
                if item["turnId"] != self.turn {
                    return Ok(false);
                }
                if item["kind"] == "agentMessage"
                    && item["status"] == "completed"
                    && item
                        .get("messagePhase")
                        .is_none_or(|phase| phase == "finalAnswer")
                {
                    let id = item["itemId"]
                        .as_str()
                        .filter(|id| !id.is_empty() && id.len() <= 128)
                        .ok_or("O Muse Code retornou uma mensagem sem identidade.")?;
                    let text = item["text"]
                        .as_str()
                        .ok_or("O Muse Code retornou uma resposta inválida.")?;
                    let previous = self.messages.get(id).map_or(0, String::len);
                    let total: usize = self.messages.values().map(String::len).sum();
                    if text.len() > MAX_ANSWER_BYTES
                        || total - previous + text.len() > MAX_ANSWER_BYTES
                        || (!self.messages.contains_key(id) && self.messages.len() >= 128)
                    {
                        return Err("A resposta do Muse Code excedeu o limite permitido.".into());
                    }
                    if !self.messages.contains_key(id) {
                        self.order.push(id.into());
                    }
                    self.messages.insert(id.into(), text.into());
                }
                if item["kind"] == "webSearch" {
                    if !self.search {
                        return Err(
                            "O Muse Code tentou buscar na web durante uma síntese sem ferramentas."
                                .into(),
                        );
                    }
                    if item["status"] == "completed"
                        && item["providerStatus"] == "completed"
                        && item["action"] == "search"
                    {
                        self.searched = true;
                        for hit in item["results"].as_array().into_iter().flatten().take(256) {
                            if self.sources.len() >= self.limit {
                                break;
                            }
                            let Some(url) = hit["url"].as_str().and_then(safe_url) else {
                                continue;
                            };
                            let Some(title) = hit["title"].as_str() else {
                                continue;
                            };
                            if self.sources.iter().any(|source| source["url"] == url) {
                                continue;
                            }
                            let title: String = title
                                .chars()
                                .filter(|ch| !ch.is_control())
                                .take(512)
                                .collect();
                            self.sources.push(json!({"title":title, "url":url}));
                        }
                    }
                }
            }
            _ => {}
        }
        Ok(false)
    }

    fn finish(self) -> Result<Output, String> {
        let answer = self
            .order
            .iter()
            .filter_map(|id| self.messages.get(id))
            .map(String::as_str)
            .collect::<Vec<_>>()
            .join("\n\n");
        if !self.completed || answer.trim().is_empty() || answer.len() > MAX_ANSWER_BYTES {
            return Err("O Muse Code não confirmou uma resposta completa.".into());
        }
        if self.search && (!self.searched || self.sources.is_empty()) {
            return Err("O Muse Code não confirmou uma busca com fontes verificáveis.".into());
        }
        Ok(Output {
            answer,
            sources: self.sources,
        })
    }
}

fn safe_url(value: &str) -> Option<String> {
    if value.len() > 4096 || value.trim() != value || value.chars().any(char::is_control) {
        return None;
    }
    let url = reqwest::Url::parse(value).ok()?;
    (matches!(url.scheme(), "http" | "https")
        && url.host_str().is_some()
        && url.username().is_empty()
        && url.password().is_none())
    .then(|| url.to_string())
}

async fn cancelled(signal: &mut watch::Receiver<bool>) {
    loop {
        if *signal.borrow() {
            return;
        }
        if signal.changed().await.is_err() {
            std::future::pending::<()>().await;
        }
    }
}

async fn interrupt(control: &MuseControl, session: Option<&str>, turn: Option<&str>) {
    if let (Some(session), Ok(command)) = (session, command_id()) {
        let mut params = json!({"commandId":command, "sessionId":session});
        if let Some(turn) = turn {
            params["turnId"] = json!(turn);
        }
        let _ = tokio::time::timeout(
            Duration::from_secs(2),
            control.request("turn/interrupt", &params),
        )
        .await;
    }
}

#[allow(clippy::too_many_arguments)]
async fn run_config(
    home: &Path,
    mut config: MuseSpawnConfig,
    model: &str,
    reasoning: Option<&str>,
    prompt: &str,
    input: &Value,
    mut signal: watch::Receiver<bool>,
    search_limit: Option<usize>,
) -> Result<Output, String> {
    if *signal.borrow() {
        return Err(CANCELLED.into());
    }
    let input = turn_input(prompt, input)?;
    let root = config
        .cwd
        .canonicalize()
        .map_err(|_| "A pasta de execução do Muse não está disponível.")?;
    config.cwd = root.clone();
    let mut process = MuseProcess::spawn(config)?;
    let control = process.control();
    let mut session = None;
    let mut turn = None;
    let work = async {
        let info = process
            .initialize(&json!({"capabilities":{"experimentalApi":true,"userInputDialogs":false}}))
            .await?;
        if info["experimentalApi"] != true || info["sessionDurability"] != "ephemeral" {
            return Err(
                "O host Muse não confirmou a sessão temporária e os contratos necessários.".into(),
            );
        }
        let account = control.request("account/read", &json!({})).await?;
        validate_account(&account)?;
        let _ = super::usage::observed(home, &account);
        let catalog = control.request("model/list", &json!({})).await?;
        validate_model(&catalog, model, reasoning)?;
        let effective_reasoning = reasoning.or_else(|| {
            catalog["models"]
                .as_array()?
                .iter()
                .find(|row| row["providerId"] == "meta" && row["modelId"] == model)?
                ["defaultReasoningEffort"]
                .as_str()
        });
        validate_model(&catalog, model, effective_reasoning)?;
        let started = control
            .request(
                "session/start",
                &json!({
                    "commandId":command_id()?, "providerId":"meta", "modelId":model,
                    "workspaceRoot":crate::library::strip_verbatim(&root.to_string_lossy()), "approvalMode":"denyUnmatched",
                    "config":{"mcpServers":{}}
                }),
            )
            .await?;
        let id = session_id(&started, &root, model, None)?;
        session = Some(id.clone());
        let command = command_id()?;
        let mut params = json!({"commandId":command, "sessionId":id, "input":input});
        if let Some(effort) = effective_reasoning {
            params["reasoningEffort"] = json!(effort);
        }
        let ack = control.request("turn/start", &params).await?;
        let turn_id = turn_id(&ack, &command)?;
        turn = Some(turn_id.clone());
        let mut collector = Collector::new(
            &id,
            &turn_id,
            search_limit.is_some(),
            search_limit.unwrap_or(0),
        );
        while let Some(event) = process.next_event().await? {
            if event["method"] == "usage/changed" {
                let _ = super::usage::observe(home, &account, &event["params"]);
                continue;
            }
            if event["method"] == "session/modelChanged"
                && event["params"]["sessionId"] == id
                && (event["params"]["providerId"] != "meta" || event["params"]["modelId"] != model)
            {
                return Err("O Muse Code alterou o modelo durante a operação.".into());
            }
            if let Some(id) = event.get("id") {
                control
                    .reject(id, -32601, "Esta operação não aceita ações adicionais.")
                    .await?;
                return Err("O Muse Code solicitou uma ação fora do escopo desta operação.".into());
            }
            if event["method"] == "approval/requested" || event["method"] == "admission/requested" {
                return Err(
                    "O Muse Code solicitou uma aprovação fora do escopo desta operação.".into(),
                );
            }
            if collector.apply(&event)? {
                if let Ok(Ok(payload)) = tokio::time::timeout(
                    Duration::from_secs(2),
                    control.request("usage/read", &json!({})),
                )
                .await
                {
                    let _ = super::usage::observe(home, &account, &payload);
                }
                return collector.finish();
            }
        }
        Err("O host Muse encerrou antes de confirmar a resposta.".into())
    };
    let result = tokio::select! {
        biased;
        _ = cancelled(&mut signal) => Err(CANCELLED.into()),
        _ = tokio::time::sleep(RUN_TIMEOUT) => Err("O Muse Code excedeu o prazo desta operação; confirme o estado antes de tentar novamente.".into()),
        result = work => result,
    };
    if result.is_err() {
        interrupt(&control, session.as_deref(), turn.as_deref()).await;
    }
    let cleanup = process.cancel().await;
    match result {
        Ok(output) => {
            if cleanup.is_err() {
                // Preserve the confirmed answer; owned-process Drop still enforces termination.
                eprintln!("Muse Code: encerramento do host não confirmado após resposta completa.");
            }
            Ok(output)
        }
        Err(error) => Err(error),
    }
}

#[cfg(test)]
#[path = "session/tests.rs"]
mod tests;
