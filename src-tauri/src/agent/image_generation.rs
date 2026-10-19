//! Image generation uses its own account and model, independently of the conversation.
//! Binary output lives in attachment storage; only metadata enters the journal.
use super::{attachments, cancelled, provider::Sse, web_search::Config, AgentError};
use crate::{
    openai_codex::{
        antigravity::{user_agent, ENDPOINTS},
        image_models, CodexCredential, OpenAiCodexState,
    },
    persistence::AppState,
};
use base64::{engine::general_purpose::STANDARD, Engine};
use rusqlite::OptionalExtension;
use serde::Deserialize;
use serde_json::{json, Value};
use std::{path::Path, time::Duration};
use tauri::Manager;
use tokio::sync::watch;

pub(super) const MODEL: &str = image_models::ANTIGRAVITY_MODEL;
const EVENT_LIMIT: usize = 28 * 1024 * 1024;
const STREAM_LIMIT: usize = 64 * 1024 * 1024;
fn invalid(message: &str) -> AgentError {
    AgentError::new("image_generation", message)
}
pub(super) fn uncertain(message: &str) -> AgentError {
    let mut error = AgentError::new("image_outcome_unknown", message);
    error.tool_result = Some(json!({"error":{"code":error.code,"message":error.message},"generationRepeated":false,"recovery":"The provider operation has an unknown outcome. Inspect its history. Only a new explicit user request may authorize another paid generation; do not repeat automatically."}).to_string());
    error
}
pub(super) fn tool_receipt(turn: &super::StoredTurn, tool: &super::ToolCall) -> Option<Value> {
    // UI errors are readable messages; durable wire outputs retain recovery receipts.
    serde_json::from_str::<Value>(&tool.output)
        .ok()
        .filter(Value::is_object)
        .or_else(|| {
            turn.wire
                .iter()
                .rev()
                .filter(|item| item["type"] == "function_call_output" && item["call_id"] == tool.id)
                .filter_map(|item| item["output"].as_str())
                .find_map(|output| {
                    serde_json::from_str::<Value>(output)
                        .ok()
                        .filter(Value::is_object)
                })
        })
}

pub(super) fn compatible(state: &AppState, home: &Path, alias: &str, model: &str) -> bool {
    image_models::available(state, home, alias)
        .is_ok_and(|models| models.iter().any(|item| item.id == model))
}
fn load(state: &AppState, home: &Path) -> Result<Config, AgentError> {
    state.with_connection(home, |db| {
        let selected: Option<(Option<String>, Option<String>)> = db
            .query_row(
                "SELECT account_alias, model FROM image_generation_config WHERE id=1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(|_| AgentError::storage())?;
        let (alias, model) = selected.unwrap_or_default();
        Ok(Config {
            executor: crate::claude::Executor::Jarvis,
            inherit_chat: false,
            model: alias
                .as_ref()
                .map(|_| model.unwrap_or_else(|| MODEL.into())),
            account_alias: alias,
        })
    })
}
#[cfg(test)]
fn save(state: &AppState, home: &Path, alias: Option<String>) -> Result<Config, AgentError> {
    save_selection(state, home, alias, Some(MODEL.into()))
}
fn save_selection(
    state: &AppState,
    home: &Path,
    alias: Option<String>,
    model: Option<String>,
) -> Result<Config, AgentError> {
    let model = alias
        .as_ref()
        .map(|_| model.unwrap_or_else(|| MODEL.into()));
    if alias
        .as_deref()
        .is_some_and(|alias| !compatible(state, home, alias, model.as_deref().unwrap_or_default()))
    {
        return Err(invalid(
            "Selecione uma conta ativa e um modelo disponível para geração de imagens.",
        ));
    }
    state.with_connection(home, |db| db.execute("INSERT INTO image_generation_config(id,account_alias,model) VALUES(1,?1,?2) ON CONFLICT(id) DO UPDATE SET account_alias=excluded.account_alias,model=excluded.model", rusqlite::params![alias,model]).map_err(|_| AgentError::storage()))?;
    load(state, home)
}
pub(super) fn enabled(state: &AppState, home: &Path) -> bool {
    load(state, home).is_ok_and(|config| {
        config.account_alias.as_deref().is_some_and(|alias| {
            compatible(
                state,
                home,
                alias,
                config.model.as_deref().unwrap_or_default(),
            )
        })
    })
}
#[tauri::command]
pub async fn get_image_generation_config(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<Config, AgentError> {
    let home = app.path().home_dir().map_err(|_| AgentError::storage())?;
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || load(&state, &home))
        .await
        .map_err(|_| AgentError::internal())?
}
#[tauri::command]
pub async fn set_image_generation_config(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    account_alias: Option<String>,
    model: Option<String>,
) -> Result<Config, AgentError> {
    let home = app.path().home_dir().map_err(|_| AgentError::storage())?;
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        save_selection(&state, &home, account_alias, model)
    })
    .await
    .map_err(|_| AgentError::internal())?
}
pub(super) fn definition() -> Value {
    json!({"type":"function","name":"generate_image","description":"Create or edit images using the independently configured image account/model and required ComfyUI workflow. Optional image_ids reference owned attachments; variants requests up to four independent alternatives. Processing controls resize, background removal, interpolation upscale and verified exports. UI displays final images automatically. Returns metadata and paths, never base64. Only use for requested visual creation/editing; preserve successful sourceImageIds on errors and retry image_process instead of generating again.","parameters":{"type":"object","properties":{"prompt":{"type":"string","minLength":1,"maxLength":8000},"aspect_ratio":{"type":"string","enum":["1:1","2:3","3:2","3:4","4:3","4:5","5:4","9:16","16:9","21:9"]},"image_ids":{"type":"array","items":{"type":"string"},"maxItems":4},"variants":{"type":"integer","minimum":1,"maximum":4,"default":1},"processing":super::image_tasks::processing_schema()},"required":["prompt"],"additionalProperties":false}})
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Arguments {
    prompt: String,
    aspect_ratio: Option<String>,
    #[serde(default)]
    image_ids: Vec<String>,
    #[serde(default = "one")]
    variants: u8,
    #[serde(default = "super::image_tasks::default_processing")]
    processing: super::image_tasks::Processing,
}
fn one() -> u8 {
    1
}
fn arguments(args: &Value) -> Result<Arguments, AgentError> {
    let args: Arguments = serde_json::from_value(args.clone())
        .map_err(|_| invalid("Informe uma descrição válida para a imagem."))?;
    if args.prompt.trim().is_empty()
        || args.prompt.len() > 8000
        || args.image_ids.len() > 4
        || !(1..=4).contains(&args.variants)
        || args.aspect_ratio.as_deref().is_some_and(|ratio| {
            ![
                "1:1", "2:3", "3:2", "3:4", "4:3", "4:5", "5:4", "9:16", "16:9", "21:9",
            ]
            .contains(&ratio)
        })
    {
        return Err(invalid(
            "Revise a descrição, proporção e imagens de referência.",
        ));
    }
    args.processing.validate()?;
    Ok(args)
}
fn request_body(
    credential: &CodexCredential,
    home: &Path,
    conversation: &str,
    args: &Arguments,
) -> Result<Value, AgentError> {
    let project = credential
        .project_id
        .as_ref()
        .ok_or_else(|| invalid("Reconecte a conta Antigravity."))?;
    let mut parts = vec![];
    for id in &args.image_ids {
        let item = attachments::metadata(home, conversation, id)?;
        if item.kind != "image" {
            return Err(invalid("A referência precisa ser uma imagem."));
        }
        let bytes = attachments::bounded_read(
            &attachments::location(home, conversation, id)?.join("content"),
            attachments::MAX_BYTES,
        )?;
        parts.push(json!({"inlineData":{"mimeType":"image/png","data":STANDARD.encode(bytes)}}));
    }
    parts.push(json!({"text":args.prompt}));
    let mut config = json!({"responseModalities":["IMAGE"],"candidateCount":1});
    if let Some(ratio) = &args.aspect_ratio {
        config["imageConfig"] = json!({"aspectRatio":ratio});
    }
    Ok(
        json!({"project":project,"model":MODEL,"userAgent":"antigravity","requestType":"agent","requestId":format!("agent-{}",crate::library::new_id()?),"request":{"contents":[{"role":"user","parts":parts}],"generationConfig":config}}),
    )
}

fn api_request(
    client: &reqwest::Client,
    credential: &CodexCredential,
    model: &str,
    home: &Path,
    conversation: &str,
    args: &Arguments,
) -> Result<reqwest::RequestBuilder, AgentError> {
    let mut references = vec![];
    for id in &args.image_ids {
        if attachments::metadata(home, conversation, id)?.kind != "image" {
            return Err(invalid("A referência precisa ser uma imagem."));
        }
        let bytes = attachments::bounded_read(
            &attachments::location(home, conversation, id)?.join("content"),
            attachments::MAX_BYTES,
        )?;
        references.push(format!("data:image/png;base64,{}", STANDARD.encode(bytes)));
    }
    if let Some(config) = &credential.custom {
        if !image_models::is_openrouter(config) {
            return Err(invalid("Selecione uma conta OpenRouter compatível."));
        }
        let mut body =
            json!({"model":model,"prompt":args.prompt,"n":1,"response_format":"b64_json"});
        if let Some(ratio) = &args.aspect_ratio {
            body["aspect_ratio"] = json!(ratio);
        }
        if !references.is_empty() {
            body["input_references"] = json!(references
                .iter()
                .map(|url| json!({"type":"image_url","image_url":{"url":url}}))
                .collect::<Vec<_>>());
        }
        return Ok(client
            .post("https://openrouter.ai/api/v1/images")
            .bearer_auth(&credential.access)
            .json(&body));
    }
    if credential.project_id.is_some() || model != image_models::CODEX_MODEL {
        return Err(invalid(
            "Selecione um modelo de imagens compatível com a assinatura Codex.",
        ));
    }
    let size = match args.aspect_ratio.as_deref() {
        Some("1:1") => "1024x1024",
        Some("2:3") => "1024x1536",
        Some("3:2") => "1536x1024",
        Some("3:4") => "1152x1536",
        Some("4:3") => "1536x1152",
        Some("4:5") => "1024x1280",
        Some("5:4") => "1280x1024",
        Some("9:16") => "864x1536",
        Some("16:9") => "1536x864",
        Some("21:9") => "1792x768",
        _ => "auto",
    };
    let mut body = json!({"model":model,"prompt":args.prompt,"n":1,"quality":"auto","size":size});
    let operation = if references.is_empty() {
        "generations"
    } else {
        body["images"] = json!(references
            .iter()
            .map(|url| json!({"image_url":url}))
            .collect::<Vec<_>>());
        "edits"
    };
    Ok(client
        .post(format!(
            "{}/codex/images/{operation}",
            crate::openai_codex::OPENAI_CODEX_BASE_URL
        ))
        .bearer_auth(&credential.access)
        .header("chatgpt-account-id", &credential.account_id)
        .header("originator", "codex_cli_rs")
        .header("version", crate::openai_codex::OPENAI_CODEX_CLIENT_VERSION)
        .header("x-codex-image-turn-id", crate::library::new_id()?)
        .json(&body))
}

#[derive(Default)]
struct Output {
    images: Vec<Vec<u8>>,
    text: String,
    finished: bool,
    bytes: usize,
}
impl Output {
    fn image(&mut self, encoded: &str) -> Result<(), AgentError> {
        if encoded.len() > EVENT_LIMIT {
            return Err(invalid("Imagem inválida ou muito grande."));
        }
        let bytes = STANDARD
            .decode(encoded)
            .map_err(|_| invalid("O provedor retornou uma imagem inválida."))?;
        if bytes.is_empty()
            || !matches!(
                image::guess_format(&bytes),
                Ok(image::ImageFormat::Png | image::ImageFormat::Jpeg | image::ImageFormat::WebP)
            )
        {
            return Err(invalid(
                "O provedor retornou um formato de imagem inválido.",
            ));
        }
        if self.images.contains(&bytes) {
            return Ok(());
        }
        if bytes.len() > attachments::MAX_BYTES
            || self.bytes + bytes.len() > 40 * 1024 * 1024
            || self.images.len() >= 4
        {
            return Err(invalid("As imagens geradas excederam o limite de tamanho."));
        }
        self.bytes += bytes.len();
        self.images.push(bytes);
        Ok(())
    }
    fn event(&mut self, event: &Value) -> Result<(), AgentError> {
        let response = event.get("response").unwrap_or(event);
        if !event["error"].is_null() || !response["error"].is_null() {
            return Err(invalid("O Antigravity não conseguiu gerar a imagem."));
        }
        if response["promptFeedback"]["blockReason"].is_string() {
            return Err(invalid(
                "O provedor bloqueou a geração desta imagem. Revise o pedido.",
            ));
        }
        let candidate = &response["candidates"][0];
        if let Some(reason) = candidate["finishReason"].as_str() {
            if reason != "STOP" {
                return Err(invalid(
                    "O provedor interrompeu a geração. Revise o pedido e tente novamente.",
                ));
            }
            self.finished = true;
        }
        if let Some(parts) = candidate["content"]["parts"].as_array() {
            for part in parts {
                // Thought parts and signatures are not generated artifacts.
                if part["thought"] == true {
                    continue;
                }
                if let Some(text) = part["text"].as_str() {
                    self.text.extend(
                        text.chars()
                            .take(4000usize.saturating_sub(self.text.chars().count())),
                    );
                }
                if let Some(inline) = part.get("inlineData").or_else(|| part.get("inline_data")) {
                    let mime = inline["mimeType"]
                        .as_str()
                        .or_else(|| inline["mime_type"].as_str())
                        .unwrap_or("");
                    if !["image/png", "image/jpeg", "image/webp"].contains(&mime) {
                        return Err(invalid(
                            "O provedor retornou um formato de imagem não suportado.",
                        ));
                    }
                    let encoded = inline["data"]
                        .as_str()
                        .filter(|data| data.len() <= EVENT_LIMIT)
                        .ok_or_else(|| invalid("Imagem inválida ou muito grande."))?;
                    self.image(encoded)?;
                }
            }
        }
        Ok(())
    }
    fn validate(&self) -> Result<(), AgentError> {
        if !self.finished {
            return Err(invalid(
                "A geração foi interrompida antes de concluir. Tente novamente.",
            ));
        }
        if self.images.is_empty() {
            return Err(invalid(
                "O modelo não retornou uma imagem. Revise o pedido e tente novamente.",
            ));
        }
        Ok(())
    }
}
async fn receive_api(
    mut response: reqwest::Response,
    mut signal: watch::Receiver<bool>,
) -> Result<Output, AgentError> {
    if !response.status().is_success() {
        if response.status().is_server_error() {
            return Err(uncertain("O provedor não confirmou a geração. Confira o histórico antes de solicitar uma nova imagem."));
        }
        return Err(invalid(match response.status().as_u16() {
            401 | 403 => "O provedor recusou a geração de imagens. Confira a conexão, o modelo e as permissões da conta.",
            402 => "O saldo do provedor de imagens é insuficiente.",
            404 => "O modelo de imagens não está disponível nesta conta. Atualize o catálogo e revise a seleção.",
            429 => "O limite de geração de imagens foi atingido. Tente mais tarde.",
            _ => "O provedor não confirmou a geração. Confira o histórico antes de solicitar uma nova imagem.",
        }));
    }
    let mut bytes = vec![];
    loop {
        let chunk = tokio::select! { biased; _ = cancelled(&mut signal) => return Err(AgentError::cancelled()), chunk = response.chunk() => chunk.map_err(|_| uncertain("A conexão foi interrompida. Confira o histórico do provedor antes de gerar novamente."))? };
        let Some(chunk) = chunk else {
            break;
        };
        if bytes.len() + chunk.len() > STREAM_LIMIT {
            return Err(uncertain("A resposta da imagem excedeu o limite de tamanho. Confira o histórico antes de gerar novamente."));
        }
        bytes.extend_from_slice(&chunk);
    }
    let value: Value = serde_json::from_slice(&bytes).map_err(|_| uncertain("O provedor retornou uma resposta de imagem inválida. Confira o histórico antes de gerar novamente."))?;
    if !value["error"].is_null() {
        return Err(uncertain(
            "O provedor não concluiu a geração. Confira o histórico antes de gerar novamente.",
        ));
    }
    let images = value["data"].as_array().filter(|items| !items.is_empty() && items.len() <= 4)
        .ok_or_else(|| uncertain("O provedor não retornou imagens válidas. Confira o histórico antes de gerar novamente."))?;
    let mut output = Output {
        finished: true,
        ..Output::default()
    };
    for image in images {
        output.image(image["b64_json"].as_str().ok_or_else(|| uncertain("O provedor não retornou a imagem em base64. Confira o histórico antes de gerar novamente."))?).map_err(|cause| uncertain(&cause.message))?;
    }
    output.validate()?;
    Ok(output)
}
async fn receive(
    mut response: reqwest::Response,
    mut signal: watch::Receiver<bool>,
) -> Result<Output, AgentError> {
    if !response.status().is_success() {
        return Err(invalid(match response.status().as_u16() {
            401 | 403 => {
                "O Antigravity recusou o acesso. Reconecte a conta e confira o acesso ao modelo."
            }
            404 => "Gemini 3.1 Flash Image não está disponível nesta conta.",
            429 => "O limite de geração de imagens foi atingido. Tente mais tarde.",
            _ => "Não foi possível gerar a imagem no Antigravity. Tente novamente.",
        }));
    }
    let mut parser = Sse::default();
    let mut output = Output::default();
    let mut size = 0;
    loop {
        let chunk = tokio::select! { biased; _ = cancelled(&mut signal) => return Err(AgentError::cancelled()), chunk = response.chunk() => chunk.map_err(|_| invalid("A conexão foi interrompida durante a geração."))? };
        let eof = chunk.is_none();
        let bytes = chunk.as_deref().unwrap_or(b"\n\n");
        size += bytes.len();
        if size > STREAM_LIMIT {
            return Err(invalid("A resposta da imagem excedeu o limite de tamanho."));
        }
        for event in parser.push_bounded(bytes, EVENT_LIMIT)? {
            output.event(&event)?;
        }
        if eof {
            output.validate()?;
            return Ok(output);
        }
    }
}
fn persist(
    home: &Path,
    conversation: &str,
    alias: &str,
    model: &str,
    output: Output,
) -> Result<String, AgentError> {
    output.validate()?;
    let mut images: Vec<attachments::Attachment> = vec![];
    for (index, bytes) in output.images.iter().enumerate() {
        let extension = match image::guess_format(bytes) {
            Ok(image::ImageFormat::Jpeg) => "jpg",
            Ok(image::ImageFormat::WebP) => "webp",
            _ => "png",
        };
        match attachments::store(
            home,
            conversation,
            &format!("imagem-{}.{}", index + 1, extension),
            bytes,
        ) {
            Ok(item) => images.push(item),
            Err(error) => {
                return Err(preserve_sources(
                    error,
                    &images
                        .iter()
                        .map(|item| item.id.clone())
                        .collect::<Vec<_>>(),
                ));
            }
        }
    }
    let paths: Vec<_> = images
        .iter()
        .map(|item| {
            attachments::location(home, conversation, &item.id).map(|path| path.join("source"))
        })
        .collect::<Result<_, _>>()?;
    Ok(json!({"kind":"generated_image","accountAlias":alias,"model":model,"images":images,"sourcePaths":paths,"text":output.text}).to_string())
}
pub(super) async fn execute(
    state: &AppState,
    oauth: &OpenAiCodexState,
    home: &Path,
    conversation: &str,
    args: &Value,
    mut signal: watch::Receiver<bool>,
) -> Result<String, AgentError> {
    let mut cancel_signal = signal.clone();
    let operation = async {
        let args = arguments(args)?;
        let config = load(state, home)?;
        let model = config.model.clone().unwrap_or_default();
        let alias = config.account_alias.filter(|alias| compatible(state, home, alias, &model)).ok_or_else(|| invalid("Geração de imagens desligada ou indisponível. Selecione uma conta e um modelo de imagens nas configurações."))?;
        let record = state
            .list_provider_accounts(home)
            .map_err(|_| AgentError::storage())?
            .into_iter()
            .find(|record| record.alias == alias)
            .ok_or_else(AgentError::storage)?;
        if matches!(record.provider_kind.as_str(), "custom" | "openrouter") {
            image_models::validate_request(
                home,
                &model,
                args.aspect_ratio.as_deref(),
                args.image_ids.len(),
            )?;
        }
        let (auth_state, auth_oauth, auth_home, auth_alias) = (
            state.clone(),
            oauth.clone(),
            home.to_path_buf(),
            alias.clone(),
        );
        let credential = tauri::async_runtime::spawn_blocking(move || {
            if record.provider_kind == "openai-codex" {
                auth_oauth.app_credential(&auth_state, &auth_home, &auth_alias)
            } else {
                auth_oauth
                    .credential_and_models(&auth_state, &auth_home, &auth_alias)
                    .map(|(credential, _)| credential)
            }
        })
        .await
        .map_err(|_| AgentError::internal())??;
        let endpoint = credential
            .antigravity_endpoint
            .as_deref()
            .filter(|endpoint| ENDPOINTS.contains(endpoint))
            .unwrap_or(ENDPOINTS[0])
            .to_owned();
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(20))
            .timeout(Duration::from_secs(300))
            .build()
            .map_err(|_| AgentError::internal())?;
        let antigravity = credential.project_id.is_some();
        let (body_home, body_conversation, body_model) =
            (home.to_path_buf(), conversation.to_owned(), model.clone());
        let request = tauri::async_runtime::spawn_blocking(move || {
            if credential.project_id.is_some() {
                let body = request_body(&credential, &body_home, &body_conversation, &args)?;
                Ok(client
                    .post(format!(
                        "{endpoint}/v1internal:streamGenerateContent?alt=sse"
                    ))
                    .bearer_auth(&credential.access)
                    .header("user-agent", user_agent())
                    .header("accept", "text/event-stream")
                    .json(&body))
            } else {
                api_request(
                    &client,
                    &credential,
                    &body_model,
                    &body_home,
                    &body_conversation,
                    &args,
                )
            }
        })
        .await
        .map_err(|_| AgentError::internal())??;
        let latest = load(state, home)?;
        if latest.account_alias.as_deref() != Some(&alias)
            || latest.model.as_deref() != Some(&model)
            || !compatible(state, home, &alias, &model)
        {
            return Err(invalid("A configuração de imagens mudou. Tente novamente."));
        }
        let response = request.send().await.map_err(|_| uncertain("Não foi possível confirmar a geração. Confira o histórico do provedor antes de gerar novamente."))?;
        let output = if antigravity {
            receive(response, signal.clone()).await
        } else {
            receive_api(response, signal.clone()).await
        }?;
        Ok::<_, AgentError>((alias, model, output))
    };
    let (alias, model, output) = tokio::select! { biased; _ = cancelled(&mut cancel_signal) => return Err(AgentError::cancelled()), result = tokio::time::timeout(Duration::from_secs(300), operation) => result.map_err(|_| uncertain("A geração excedeu 5 minutos. Confira o histórico do provedor antes de gerar novamente."))?? };
    if *signal.borrow_and_update() {
        return Err(AgentError::cancelled());
    }
    // Await the atomic storage operation so cancellation cannot orphan a running writer.
    let (home, conversation) = (home.to_path_buf(), conversation.to_owned());
    tauri::async_runtime::spawn_blocking(move || {
        persist(&home, &conversation, &alias, &model, output)
    })
    .await
    .map_err(|_| AgentError::internal())?
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn execute_pipeline(
    state: &AppState,
    oauth: &OpenAiCodexState,
    home: &Path,
    owner: &str,
    session: &super::Session,
    execution: Option<&super::workflow::Execution>,
    commands: &mut super::command_sessions::CommandSessions,
    sandbox: Option<&super::execution_sandbox::SandboxPlan>,
    args: &Value,
    signal: watch::Receiver<bool>,
) -> Result<String, AgentError> {
    let request = arguments(args)?;
    {
        let data = session.data.lock().map_err(|_| AgentError::internal())?;
        if let Some(receipt) = data
            .turns
            .last()
            .into_iter()
            .flat_map(|turn| {
                turn.turn
                    .steps
                    .iter()
                    .flat_map(|step| &step.tools)
                    .map(move |tool| (turn, tool))
            })
            .filter(|(_, tool)| tool.name == "generate_image" && tool.status == "error")
            .find_map(|(turn, tool)| {
                tool_receipt(turn, tool)
                    .filter(|value| value["error"]["code"] == "image_outcome_unknown")
            })
        {
            let mut error = uncertain("Uma geração deste pedido não foi confirmada. Confira o histórico do provedor antes de solicitar outra geração.");
            error.tool_result = Some(receipt.to_string());
            return Err(error);
        }
    }
    if let Some(execution) = execution {
        execution.ensure_image_generation_ready()?;
    }
    // Missing local dependencies or invalid exports must fail before a paid call.
    crate::core::comfyui::runtime(home).map_err(AgentError::from)?;
    super::image_tasks::validate_export(
        &session.root,
        request.processing.output_directory.as_deref(),
    )?;
    let mut ids = Vec::new();
    let mut original = json!({});
    for _ in 0..request.variants {
        match execute(state, oauth, home, owner, args, signal.clone()).await {
            Ok(output) => {
                original = serde_json::from_str(&output).map_err(|_| AgentError::internal())?;
                ids.extend(
                    original["images"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(|image| image["id"].as_str().map(str::to_owned)),
                );
                if ids.len() > 4 {
                    return Err(preserve_sources(invalid("O provedor entregou mais de quatro imagens. As originais foram preservadas; processe-as em lotes de até quatro."), &ids));
                }
            }
            Err(cause) => return Err(preserve_sources(cause, &ids)),
        }
    }
    let processed = super::image_tasks::execute(
        state,
        oauth,
        home,
        owner,
        session,
        commands,
        sandbox,
        &json!({"image_ids":ids,"processing":request.processing}),
        signal,
    )
    .await?;
    let mut result: Value = serde_json::from_str(&processed).map_err(|_| AgentError::internal())?;
    result["accountAlias"] = original["accountAlias"].clone();
    result["model"] = original["model"].clone();
    result["text"] = original["text"].clone();
    Ok(result.to_string())
}
fn preserve_sources(mut cause: AgentError, ids: &[String]) -> AgentError {
    if !ids.is_empty() {
        let mut result = cause
            .tool_result
            .as_deref()
            .and_then(|text| serde_json::from_str::<Value>(text).ok())
            .filter(Value::is_object)
            .unwrap_or_else(|| json!({}));
        let mut sources = ids.to_vec();
        for id in result["sourceImageIds"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
        {
            if !sources.iter().any(|source| source == id) {
                sources.push(id.to_owned());
            }
        }
        result["error"] = json!({"code":cause.code,"message":cause.message});
        result["sourceImageIds"] = json!(sources);
        result["recovery"] = json!("Preserved confirmed generations. Retry only image_process with these IDs, never repeat the paid provider request.");
        cause.tool_result = Some(result.to_string());
    }
    cause
}

#[cfg(test)]
mod tests;
