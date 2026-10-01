//! Antigravity image generation is independent of the conversational model.
//! Binary output lives in attachment storage; only metadata enters the journal.
use super::{attachments, cancelled, provider::Sse, web_search::Config, AgentError};
use crate::{
    openai_codex::{
        antigravity::{user_agent, ENDPOINTS},
        CodexCredential, OpenAiCodexState,
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

pub(super) const MODEL: &str = "gemini-3.1-flash-image";
const EVENT_LIMIT: usize = 28 * 1024 * 1024;
const STREAM_LIMIT: usize = 64 * 1024 * 1024;
fn invalid(message: &str) -> AgentError {
    AgentError::new("image_generation", message)
}

fn compatible(state: &AppState, home: &Path, alias: &str) -> bool {
    state.list_provider_accounts(home).is_ok_and(|accounts| {
        accounts.iter().any(|account| {
            account.alias == alias && account.enabled && account.provider_kind == "antigravity"
        })
    })
}
fn load(state: &AppState, home: &Path) -> Result<Config, AgentError> {
    state.with_connection(home, |db| {
        let alias: Option<String> = db
            .query_row(
                "SELECT account_alias FROM image_generation_config WHERE id=1",
                [],
                |row| row.get(0),
            )
            .optional()
            .map_err(|_| AgentError::storage())?
            .flatten();
        Ok(Config {
            inherit_chat: false,
            model: alias.as_ref().map(|_| MODEL.into()),
            account_alias: alias,
        })
    })
}
fn save(state: &AppState, home: &Path, alias: Option<String>) -> Result<Config, AgentError> {
    if alias
        .as_deref()
        .is_some_and(|alias| !compatible(state, home, alias))
    {
        return Err(invalid("Selecione uma conta Antigravity ativa."));
    }
    state.with_connection(home, |db| db.execute("INSERT INTO image_generation_config(id,account_alias) VALUES(1,?1) ON CONFLICT(id) DO UPDATE SET account_alias=excluded.account_alias", [&alias]).map_err(|_| AgentError::storage()))?;
    load(state, home)
}
pub(super) fn enabled(state: &AppState, home: &Path) -> bool {
    load(state, home).is_ok_and(|config| {
        config
            .account_alias
            .as_deref()
            .is_some_and(|alias| compatible(state, home, alias))
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
) -> Result<Config, AgentError> {
    let home = app.path().home_dir().map_err(|_| AgentError::storage())?;
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || save(&state, &home, account_alias))
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

#[derive(Default)]
struct Output {
    images: Vec<Vec<u8>>,
    text: String,
    finished: bool,
    bytes: usize,
}
impl Output {
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
                    let bytes = STANDARD
                        .decode(encoded)
                        .map_err(|_| invalid("O provedor retornou uma imagem inválida."))?;
                    if bytes.is_empty() || image::guess_format(&bytes).is_err() {
                        return Err(invalid("O provedor retornou uma imagem inválida."));
                    }
                    // Some streams repeat the final candidate; avoid duplicate attachments.
                    if self.images.contains(&bytes) {
                        continue;
                    }
                    self.bytes += bytes.len();
                    if bytes.len() > attachments::MAX_BYTES
                        || self.bytes > 40 * 1024 * 1024
                        || self.images.len() >= 4
                    {
                        return Err(invalid("As imagens geradas excederam o limite de tamanho."));
                    }
                    self.images.push(bytes);
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
    Ok(json!({"kind":"generated_image","accountAlias":alias,"model":MODEL,"images":images,"sourcePaths":paths,"text":output.text}).to_string())
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
        let alias = config.account_alias.filter(|alias| compatible(state, home, alias)).ok_or_else(|| invalid("Geração de imagens desligada. Selecione uma conta Antigravity nas configurações."))?;
        let (auth_state, auth_oauth, auth_home, auth_alias) = (
            state.clone(),
            oauth.clone(),
            home.to_path_buf(),
            alias.clone(),
        );
        let (credential, _) = tauri::async_runtime::spawn_blocking(move || {
            auth_oauth.inference_model(&auth_state, &auth_home, &auth_alias, MODEL, None)
        })
        .await
        .map_err(|_| AgentError::internal())??;
        let (body_home, body_conversation) = (home.to_path_buf(), conversation.to_owned());
        let body_credential = credential.clone();
        let body = tauri::async_runtime::spawn_blocking(move || {
            request_body(&body_credential, &body_home, &body_conversation, &args)
        })
        .await
        .map_err(|_| AgentError::internal())??;
        if load(state, home)?.account_alias.as_deref() != Some(&alias)
            || !compatible(state, home, &alias)
        {
            return Err(invalid("A configuração de imagens mudou. Tente novamente."));
        }
        let endpoint = credential
            .antigravity_endpoint
            .as_deref()
            .filter(|endpoint| ENDPOINTS.contains(endpoint))
            .unwrap_or(ENDPOINTS[0]);
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(20))
            .timeout(Duration::from_secs(300))
            .build()
            .map_err(|_| AgentError::internal())?;
        let response = client
            .post(format!(
                "{endpoint}/v1internal:streamGenerateContent?alt=sse"
            ))
            .bearer_auth(&credential.access)
            .header("user-agent", user_agent())
            .header("accept", "text/event-stream")
            .json(&body)
            .send()
            .await
            .map_err(|_| invalid("Não foi possível conectar ao Antigravity."))?;
        receive(response, signal.clone())
            .await
            .map(|output| (alias, output))
    };
    let (alias, output) = tokio::select! { biased; _ = cancelled(&mut cancel_signal) => return Err(AgentError::cancelled()), result = tokio::time::timeout(Duration::from_secs(300), operation) => result.map_err(|_| invalid("A geração excedeu 5 minutos. Tente novamente."))?? };
    if *signal.borrow_and_update() {
        return Err(AgentError::cancelled());
    }
    // Await the atomic storage operation so cancellation cannot orphan a running writer.
    let (home, conversation) = (home.to_path_buf(), conversation.to_owned());
    tauri::async_runtime::spawn_blocking(move || persist(&home, &conversation, &alias, output))
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
    commands: &mut super::command_sessions::CommandSessions,
    sandbox: Option<&super::execution_sandbox::SandboxPlan>,
    args: &Value,
    signal: watch::Receiver<bool>,
) -> Result<String, AgentError> {
    let request = arguments(args)?;
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
