//! Licensed Brag resources, imported through the same project scope as native video jobs.
use super::{audio, error, AgentError, Mode, PreparedCommand, SandboxPlan};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{ffi::OsString, fs, io::Write, path::Path};

const PAGE_SIZE: usize = 30;
const FAMILIES: &[&str] = &["music", "ui", "interface", "keyboard", "impact", "casino"];

pub(super) fn definitions(mode: Mode) -> Vec<Value> {
    let mut tools = vec![super::super::tools::definition(
        "video_brag_assets",
        "List the bundled licensed Brag sound effects and ready-made soundtracks, with provenance and credits. Read-only, paginated (30 per page). category is music/ui/interface/keyboard/impact/casino; omit or null lists all. Use nextOffset to continue. No downloads or generation. Import selected assets with video_brag_asset; never invent a resource path.",
        json!({"category":{"type":["string","null"],"enum":["music","ui","interface","keyboard","impact","casino",null]},"offset":{"type":"integer","minimum":0}}),
        &[],
    )];
    if mode == Mode::Build {
        tools.push(super::super::tools::definition(
            "video_brag_asset",
            "Import ONE exact asset returned by video_brag_assets into a new project-relative output path, preserving existing files. Keep its extension for an exact copy, or select .wav to decode MP3/OGG to PCM16 WAV with managed FFmpeg for video_presentation. Copies a .credits.md sidecar and returns source/license/attribution: preserve these in share-copy and delivered credits; mention edits/cuts. Conversion returns a managed session: wait with video_wait until completed. No remote downloads, audio generation or MusicGen license permission is needed for bundled tracks.",
            json!({"asset":{"type":"string","minLength":1,"maxLength":512},"output":{"type":"string","minLength":1,"maxLength":4096},"yieldTimeMs":{"type":"integer","minimum":1,"maximum":30000}}),
            &["asset", "output"],
        ));
    }
    tools
}

fn catalog(directory: &Path) -> Result<Vec<Value>, AgentError> {
    let value: Value = serde_json::from_slice(
        &fs::read(directory.join("asset-catalog.json")).map_err(|_| {
            error("O catálogo do Brag está indisponível. Repare HyperFrames no Core.")
        })?,
    )
    .map_err(|_| error("O catálogo do Brag é inválido."))?;
    value["assets"]
        .as_array()
        .cloned()
        .ok_or_else(|| error("O catálogo do Brag está incompleto."))
}

pub(super) fn list(home: &Path, args: &Value) -> Result<Value, AgentError> {
    let directory = crate::core::brag::directory(home).map_err(AgentError::from)?;
    let category = args["category"].as_str();
    if category.is_some_and(|category| !FAMILIES.contains(&category)) {
        return Err(error(
            "Selecione music, ui, interface, keyboard, impact ou casino.",
        ));
    }
    let assets: Vec<_> = catalog(&directory)?
        .into_iter()
        .filter(|asset| category.is_none_or(|category| asset["family"] == category))
        .collect();
    let offset = args["offset"].as_u64().unwrap_or(0);
    let offset = usize::try_from(offset).unwrap_or(usize::MAX);
    let total = assets.len();
    let page: Vec<_> = assets.into_iter().skip(offset).take(PAGE_SIZE).collect();
    let next = offset.saturating_add(page.len());
    Ok(
        json!({"resource":"brag","version":crate::core::brag::VERSION,"assets":page,"total":total,"nextOffset":(next < total).then_some(next)}),
    )
}

fn credits(asset: &Value, converted: bool) -> String {
    format!(
        "# Audio credits\n\nAsset: {}\nAuthor / attribution: {}\nSource: {}\nLicense: {}\nLicense URL: {}\nChanges: {}. Any additional trim, mix or edit must be credited in the final delivery.\n",
        asset["path"].as_str().unwrap_or_default(),
        asset["attribution"].as_str().unwrap_or("See source and license"),
        asset["source"].as_str().unwrap_or_default(),
        asset["license"].as_str().unwrap_or_default(),
        asset["licenseUrl"].as_str().unwrap_or_default(),
        if converted { "Decoded to PCM16 WAV; original track preserved" } else { "Unmodified copy" },
    )
}

fn publish(
    root: &Path,
    relative: &str,
    staged: tempfile::TempPath,
    credit: &str,
) -> Result<(), AgentError> {
    let output = super::super::tools::scoped(root, relative, true)?;
    let credit_path = super::super::tools::scoped(root, &format!("{relative}.credits.md"), true)?;
    if output.exists() || credit_path.exists() {
        return Err(error("O áudio ou seus créditos já existem. Escolha outro destino; os arquivos foram preservados."));
    }
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&credit_path)
        .map_err(|_| error("Não foi possível salvar os créditos do áudio. Os arquivos existentes foram preservados."))?;
    file.write_all(credit.as_bytes())
        .and_then(|()| file.sync_all())
        .map_err(|_| {
            error("Falha ao salvar os créditos do áudio. Inspecione o destino antes de repetir.")
        })?;
    staged.persist_noclobber(output).map_err(|_| {
        error("O destino mudou durante a importação. O áudio existente e os créditos foram preservados; inspecione-os antes de repetir.")
    })?;
    Ok(())
}

pub(super) fn import(
    root: &Path,
    home: &Path,
    args: &Value,
    sandbox: Option<&SandboxPlan>,
) -> Result<(Option<PreparedCommand>, Option<Value>), AgentError> {
    let directory = crate::core::brag::directory(home).map_err(AgentError::from)?;
    let name = args["asset"]
        .as_str()
        .ok_or_else(|| error("Escolha um asset do catálogo Brag."))?;
    let asset = catalog(&directory)?
        .into_iter()
        .find(|asset| asset["path"] == name)
        .ok_or_else(|| error("O asset não está no catálogo Brag. Consulte video_brag_assets."))?;
    let source_root = fs::canonicalize(directory.join("assets"))
        .map_err(|_| error("Os recursos do Brag estão indisponíveis."))?;
    let source = super::super::tools::scoped(&source_root, name, false)?;
    let bytes = fs::read(&source).map_err(|_| error("O asset Brag está indisponível."))?;
    if asset["bytes"].as_u64() != Some(bytes.len() as u64)
        || asset["sha256"] != format!("{:x}", Sha256::digest(&bytes))
    {
        return Err(error(
            "O asset Brag foi alterado. Repare HyperFrames no Core.",
        ));
    }
    let relative = args["output"]
        .as_str()
        .ok_or_else(|| error("Informe um novo destino de áudio dentro do projeto."))?;
    let output = super::super::tools::scoped(root, relative, true)?;
    let credit_relative = format!("{relative}.credits.md");
    let credit_path = super::super::tools::scoped(root, &credit_relative, true)?;
    if output.exists() || credit_path.exists() {
        return Err(error("O áudio ou seus créditos já existem. Escolha outro destino; os arquivos foram preservados."));
    }
    let extension = output.extension().and_then(|extension| extension.to_str());
    let same = extension.is_some_and(|extension| {
        source
            .extension()
            .is_some_and(|original| original.eq_ignore_ascii_case(extension))
    });
    let converted =
        !same && extension.is_some_and(|extension| extension.eq_ignore_ascii_case("wav"));
    if !same && !converted {
        return Err(error(
            "Mantenha a extensão original ou use .wav para importar áudio PCM16.",
        ));
    }
    let staged = tempfile::Builder::new()
        .prefix(".jarvis-brag-")
        .suffix(if converted { ".wav" } else { ".audio" })
        .tempfile_in(output.parent().ok_or_else(AgentError::internal)?)
        .map_err(|_| error("Não foi possível preparar o áudio do Brag."))?
        .into_temp_path();
    let metadata = json!({"resource":"hyperframes","action":"brag_asset","path":relative,"asset":asset,"creditsPath":credit_relative,"converted":converted});
    let credit = credits(&asset, converted);
    if !converted {
        fs::write(&staged, bytes).map_err(|_| error("Não foi possível copiar o áudio do Brag."))?;
        fs::File::open(&staged)
            .and_then(|file| file.sync_all())
            .map_err(|_| error("Não foi possível persistir o áudio do Brag."))?;
        publish(root, relative, staged, &credit)?;
        let mut result = metadata;
        result["status"] = "completed".into();
        return Ok((None, Some(result)));
    }
    let runtime = crate::core::hyperframes::runtime(home).map_err(AgentError::from)?;
    let ffmpeg = std::path::PathBuf::from(&runtime.environment["HYPERFRAMES_FFMPEG_PATH"]);
    let argv: Vec<OsString> = ["-nostdin", "-v", "error", "-i"]
        .into_iter()
        .map(OsString::from)
        .chain([source.as_os_str().to_owned()])
        .chain(
            [
                "-map",
                "0:a:0",
                "-ac",
                "2",
                "-ar",
                "48000",
                "-c:a",
                "pcm_s16le",
                "-f",
                "wav",
                "-y",
            ]
            .into_iter()
            .map(OsString::from),
        )
        .chain([staged.as_os_str().to_owned()])
        .collect();
    let (program, argv) = sandbox.map_or_else(
        || (ffmpeg.clone(), argv.clone()),
        |sandbox| sandbox.wrap(&ffmpeg, argv.clone()),
    );
    let mut process = crate::background::tokio_command(program);
    process
        .args(argv)
        .envs(runtime.environment)
        .current_dir(root);
    let root = root.to_path_buf();
    let relative = relative.to_owned();
    Ok((
        Some(PreparedCommand {
            process,
            metadata,
            on_success: Some(Box::new(move || {
                audio::wave(&staged)?;
                fs::File::open(&staged)
                    .and_then(|file| file.sync_all())
                    .map_err(|_| error("Não foi possível persistir o WAV importado."))?;
                publish(&root, &relative, staged, &credit)
            })),
        }),
        None,
    ))
}

#[cfg(test)]
mod tests;
