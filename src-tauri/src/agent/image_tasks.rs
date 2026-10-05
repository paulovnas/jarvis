//! Closed ComfyUI workflows over owned attachments. No arbitrary Python/nodes.
use super::{
    attachments,
    command_sessions::{CommandSessions, PreparedCommand},
    execution_policy,
    execution_sandbox::{self, SandboxPlan},
    tool_contract::{ApprovalPolicy, Capabilities, Effect},
    AgentError, Session, ToolCall,
};
use crate::{openai_codex::OpenAiCodexState, persistence::AppState};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{ffi::OsString, fs, io::Write, path::Path, sync::Arc};
use tokio::sync::watch;

fn error(message: &str) -> AgentError {
    AgentError::new("image_processing", message)
}
fn call(name: &str, args: Value) -> ToolCall {
    ToolCall {
        id: String::new(),
        name: name.into(),
        args,
        status: "running".into(),
        output: String::new(),
        duration_ms: 0,
    }
}

#[derive(Default, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Processing {
    pub width: Option<u32>,
    pub height: Option<u32>,
    #[serde(default = "png")]
    pub format: String,
    #[serde(default)]
    pub upscale: bool,
    #[serde(default)]
    pub remove_background: bool,
    pub output_directory: Option<String>,
}
fn png() -> String {
    "png".into()
}
impl Processing {
    pub(super) fn validate(&self) -> Result<(), AgentError> {
        if self.width.is_some_and(|v| !(1..=4096).contains(&v))
            || self.height.is_some_and(|v| !(1..=4096).contains(&v))
            || !["png", "jpg", "webp"].contains(&self.format.as_str())
            || (self.remove_background && self.format == "jpg")
            || self
                .output_directory
                .as_ref()
                .is_some_and(|v| v.trim().is_empty() || v.len() > 4096)
        {
            return Err(error("Use dimensões de 1 a 4096 e formato PNG, JPG ou WEBP. Remoção de fundo requer PNG ou WEBP."));
        }
        Ok(())
    }
}
pub(super) fn processing_schema() -> Value {
    json!({"type":"object","properties":{
        "width":{"type":"integer","minimum":1,"maximum":4096},
        "height":{"type":"integer","minimum":1,"maximum":4096},
        "format":{"type":"string","enum":["png","jpg","webp"],"default":"png"},
        "upscale":{"type":"boolean","description":"Required when enlarging beyond the source dimensions. Resize with interpolation; does not invent neural detail."},
        "remove_background":{"type":"boolean","description":"Remove the background locally; requires PNG or WEBP."},
        "output_directory":{"type":"string","minLength":1,"maxLength":4096,"description":"Optional project directory for uniquely named exports; never overwrite existing files."}
    },"additionalProperties":false})
}
pub(super) fn definitions() -> Vec<Value> {
    let mut definitions = vec![
        json!({"type":"function","name":"image_process","description":"Process existing image attachments through required ComfyUI, without calling or charging the image provider again. Resize, interpolate upscale, remove background or export PNG/JPG/WEBP. Returns final displayed image attachments, verified workflow/report paths and optional project exports.","parameters":{"type":"object","properties":{"image_ids":{"type":"array","items":{"type":"string"},"minItems":1,"maxItems":4},"processing":processing_schema()},"required":["image_ids"],"additionalProperties":false}}),
    ];
    definitions
        .iter_mut()
        .for_each(super::execution_sandbox::add_permission_parameters);
    definitions
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    image_ids: Vec<String>,
    #[serde(default = "default_processing")]
    processing: Processing,
}
pub(super) fn default_processing() -> Processing {
    Processing {
        format: png(),
        ..Processing::default()
    }
}

fn inputs(home: &Path, owner: &str, ids: &[String]) -> Result<Vec<std::path::PathBuf>, AgentError> {
    if ids.is_empty() || ids.len() > 4 {
        return Err(error("Selecione de uma a quatro imagens desta conversa."));
    }
    ids.iter()
        .map(|id| {
            if attachments::metadata(home, owner, id)?.kind != "image" {
                return Err(error("O anexo selecionado não é uma imagem."));
            }
            let path = attachments::location(home, owner, id)?.join("source");
            attachments::bounded_read(&path, attachments::MAX_BYTES)?;
            path.canonicalize()
                .map_err(|_| error("Imagem de origem inacessível."))
        })
        .collect()
}
fn preserve(mut cause: AgentError, ids: &[String]) -> AgentError {
    let mut result = cause
        .tool_result
        .as_deref()
        .and_then(|text| serde_json::from_str::<Value>(text).ok())
        .filter(Value::is_object)
        .unwrap_or_else(|| json!({}));
    result["error"] = json!({"code":cause.code,"message":cause.message});
    result["sourceImageIds"] = json!(ids);
    let preservation = json!("Original and produced images are preserved. Inspect producedImageIds/exports first. Retry only missing image_process operations; never generate or charge for the same images again.");
    if result["recovery"].is_object() {
        result["recovery"]["preservation"] = preservation;
    } else {
        result["recovery"] = preservation;
    }
    cause.tool_result = Some(result.to_string());
    cause
}
pub(super) fn validate_export(root: &Path, directory: Option<&str>) -> Result<(), AgentError> {
    if let Some(directory) = directory {
        let path = super::tools::scoped(root, directory, true)?;
        if path.exists() && !path.is_dir() {
            return Err(error(
                "O destino de exportação precisa ser uma pasta do projeto.",
            ));
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn execute(
    _state: &AppState,
    _oauth: &OpenAiCodexState,
    home: &Path,
    owner: &str,
    session: &Session,
    commands: &mut CommandSessions,
    sandbox: Option<&SandboxPlan>,
    args: &Value,
    signal: watch::Receiver<bool>,
) -> Result<String, AgentError> {
    let request: Request =
        serde_json::from_value(super::execution_sandbox::command_arguments(args))
            .map_err(|_| error("Informe imagens e opções válidas para o processamento."))?;
    let ids = request.image_ids.clone();
    process(home, owner, session, commands, sandbox, &request, signal)
        .await
        .map_err(|cause| preserve(cause, &ids))
}

#[allow(clippy::too_many_arguments)]
async fn process(
    home: &Path,
    owner: &str,
    session: &Session,
    commands: &mut CommandSessions,
    sandbox: Option<&SandboxPlan>,
    request: &Request,
    signal: watch::Receiver<bool>,
) -> Result<String, AgentError> {
    request.processing.validate()?;
    let sources = inputs(home, owner, &request.image_ids)?;
    // Validate destination before starting expensive processing.
    validate_export(
        &session.root,
        request.processing.output_directory.as_deref(),
    )?;
    let runtime = crate::core::comfyui::runtime(home).map_err(AgentError::from)?;
    let parent = attachments::directory(home, owner)?;
    fs::create_dir_all(&parent).map_err(|_| AgentError::storage())?;
    let staged = Arc::new(
        tempfile::Builder::new()
            .prefix(".image-task-")
            .tempdir_in(parent)
            .map_err(|_| AgentError::storage())?,
    );
    let output = staged
        .path()
        .canonicalize()
        .map_err(|_| AgentError::storage())?;
    let mut payload =
        serde_json::to_value(&request.processing).map_err(|_| AgentError::internal())?;
    payload
        .as_object_mut()
        .ok_or_else(AgentError::internal)?
        .remove("output_directory");
    payload["inputs"] = json!(sources);
    payload["output"] = json!(output);
    let native = call("image_process", json!({}));
    let derived = execution_policy::inspect_tool(
        &session.root,
        &native,
        Capabilities {
            effect: Effect::Stateful,
            approval: ApprovalPolicy::Never,
            parallel_safe: false,
        },
    )?
    .and_then(|policy| execution_sandbox::prepare(&policy));
    let sandbox = sandbox
        .or(derived.as_ref())
        .map(|plan| plan.with_host_writable_directory(&output));
    let argv = vec![
        runtime.entry.as_os_str().to_owned(),
        OsString::from("--request"),
        OsString::from(payload.to_string()),
    ];
    let (program, argv) = sandbox.as_ref().map_or_else(
        || (runtime.python.clone(), argv.clone()),
        |plan| plan.wrap(&runtime.python, argv.clone()),
    );
    let mut process = crate::background::tokio_command(program);
    process
        .args(argv)
        .envs(runtime.environment(&output).map_err(AgentError::from)?)
        .current_dir(&runtime.package);
    let retained = staged.clone();
    let command = PreparedCommand {
        process,
        metadata: json!({"resource":"comfyui"}),
        on_success: Some(Box::new(move || {
            drop(retained);
            Ok(())
        })),
    };
    let mut call = call(
        "bash",
        json!({"command":"ComfyUI image processing","yieldTimeMs":1000,"nativeTool":"image_process","nativeArguments":{"image_ids":request.image_ids,"processing":request.processing}}),
    );
    let mut result: Value = serde_json::from_str(
        &commands
            .execute_prepared(
                &session.root,
                &call,
                sandbox.as_ref(),
                signal.clone(),
                Some(command),
            )
            .await?,
    )
    .map_err(|_| AgentError::internal())?;
    while result["status"] == "running" {
        call.name = "bash_wait".into();
        call.args =
            json!({"sessionId":result["sessionId"],"cursor":result["cursor"],"yieldTimeMs":10000});
        result = serde_json::from_str(
            &commands
                .execute_prepared(&session.root, &call, sandbox.as_ref(), signal.clone(), None)
                .await?,
        )
        .map_err(|_| AgentError::internal())?;
    }
    if *signal.borrow() || result["status"] == "cancelled" {
        return Err(AgentError::cancelled());
    }
    if result["exitCode"] != 0 {
        return Err(error(
            "O ComfyUI não concluiu o processamento. As imagens originais foram preservadas.",
        ));
    }
    publish(home, owner, &session.root, &output, request)
}

fn publish(
    home: &Path,
    owner: &str,
    root: &Path,
    staging: &Path,
    request: &Request,
) -> Result<String, AgentError> {
    validate_export(root, request.processing.output_directory.as_deref())?;
    let report_bytes =
        attachments::bounded_read(&staging.join("generation-report.json"), 64 * 1024)?;
    let workflow = attachments::bounded_read(&staging.join("workflow.json"), 64 * 1024)?;
    let report: Value =
        serde_json::from_slice(&report_bytes).map_err(|_| error("Relatório ComfyUI inválido."))?;
    let files = report["images"]
        .as_array()
        .filter(|v| v.len() == request.image_ids.len())
        .ok_or_else(|| error("O ComfyUI não entregou todas as imagens solicitadas."))?;
    let _: serde_json::Map<String, Value> =
        serde_json::from_slice(&workflow).map_err(|_| error("Fluxo ComfyUI inválido."))?;
    let mut outputs = vec![];
    for (index, record) in files.iter().enumerate() {
        let name = format!("image-{:02}.{}", index + 1, request.processing.format);
        if record["path"] != name {
            return Err(error("Caminho de saída ComfyUI inválido."));
        }
        let bytes = attachments::bounded_read(&staging.join(&name), attachments::MAX_BYTES)?;
        let expected = match request.processing.format.as_str() {
            "jpg" => image::ImageFormat::Jpeg,
            "webp" => image::ImageFormat::WebP,
            _ => image::ImageFormat::Png,
        };
        if image::guess_format(&bytes).ok() != Some(expected) {
            return Err(error("O ComfyUI retornou um formato inesperado."));
        }
        let mut reader = image::ImageReader::new(std::io::Cursor::new(&bytes))
            .with_guessed_format()
            .map_err(|_| error("Imagem de saída inválida."))?;
        let mut limits = image::Limits::default();
        limits.max_image_width = Some(4096);
        limits.max_image_height = Some(4096);
        limits.max_alloc = Some(128 * 1024 * 1024);
        reader.limits(limits);
        let image = reader
            .decode()
            .map_err(|_| error("Imagem de saída inválida ou maior que 4096 pixels."))?;
        if record["width"] != image.width() || record["height"] != image.height() {
            return Err(error("As dimensões não correspondem ao relatório ComfyUI."));
        }
        outputs.push((name, bytes));
    }
    let mut images = vec![];
    let mut paths = vec![];
    let mut exports = vec![];
    let publication = (|| -> Result<(), AgentError> {
        for (name, bytes) in outputs {
            let item = attachments::store(home, owner, &name, &bytes)?;
            let location = attachments::location(home, owner, &item.id)?;
            paths.push(location.join("source"));
            images.push(item.clone());
            fs::write(location.join("workflow.json"), &workflow)
                .map_err(|_| AgentError::storage())?;
            fs::write(location.join("generation-report.json"), &report_bytes)
                .map_err(|_| AgentError::storage())?;
            if let Some(directory) = &request.processing.output_directory {
                let relative = Path::new(directory)
                    .join(format!("image-{}.{}", item.id, request.processing.format));
                let output = super::tools::scoped(root, &relative.to_string_lossy(), true)?;
                let mut file = tempfile::NamedTempFile::new_in(
                    output.parent().ok_or_else(AgentError::internal)?,
                )
                .map_err(|_| AgentError::storage())?;
                file.write_all(&bytes)
                    .and_then(|_| file.as_file().sync_all())
                    .map_err(|_| AgentError::storage())?;
                file.persist_noclobber(output).map_err(|_| error("O arquivo existente foi preservado. Consulte o anexo antes de repetir a exportação."))?;
                exports.push(relative.to_string_lossy().replace('\\', "/"));
            }
        }
        Ok(())
    })();
    if let Err(mut cause) = publication {
        cause.tool_result = Some(json!({"producedImageIds":images.iter().map(|image| &image.id).collect::<Vec<_>>(),"sourcePaths":paths,"exports":exports}).to_string());
        return Err(cause);
    }
    Ok(json!({"kind":"generated_image","accountAlias":"local","model":"ComfyUI","images":images,"sourcePaths":paths,"exports":exports,"processing":report,"sourceImageIds":request.image_ids,"text":""}).to_string())
}

#[cfg(test)]
mod tests;
