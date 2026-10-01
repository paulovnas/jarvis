//! Local audiovisual inference, owned by the existing command session lifecycle.
use super::*;
use sha2::{Digest, Sha256};

pub(super) fn definition() -> Value {
    super::super::tools::definition(
        "video_audio",
        "Generate one reusable PT-BR narration scene or instrumental music with Jarvis's required private Audiovisual Core. No downloads or package installation during inference. Narration voices: pf_dora, pm_alex (default), pm_santa. Narration preserves natural speech edges and embeds 250ms of lead-in plus 200ms of tail-out; video_presentation uses the complete actual WAV duration, including these pauses. Do not trim syllables, overlap scene narration or shorten audio to force a target duration. Use a new project-relative WAV output; exact confirmed requests reuse their existing receipt, changed requests never overwrite. MusicGen weights are CC-BY-NC-4.0: music requires nonCommercial=true ONLY when the user confirmed noncommercial use; for commercial work use a licensed supplied soundtrack instead. Music creates a short seed and crossfade-loops it to duration. Heavy generation is serialized across chats, may take time on CPU, and has no total timeout. Continue with video_wait/video_cancel using the returned sessionId.",
        json!({"action":{"type":"string","enum":["narrate","music"]},"output":{"type":"string","minLength":1,"maxLength":4096},"text":{"type":"string","minLength":1,"maxLength":4000},"prompt":{"type":"string","minLength":1,"maxLength":1000},"voice":{"type":"string","enum":["pf_dora","pm_alex","pm_santa"]},"speed":{"type":"number","minimum":0.5,"maximum":2},"duration":{"type":"number","minimum":1,"maximum":600},"seed":{"type":"integer","minimum":0,"maximum":2147483647},"nonCommercial":{"type":"boolean"},"yieldTimeMs":{"type":"integer","minimum":1,"maximum":30000}}),
        &["action","output"],
    )
}

fn text(args: &Value, key: &str, maximum: usize) -> Result<String, AgentError> {
    args[key]
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty() && value.len() <= maximum && !value.contains('\0'))
        .map(str::to_owned)
        .ok_or_else(|| error(&format!("Informe {key} válido para gerar o áudio.")))
}

fn request(args: &Value) -> Result<Value, AgentError> {
    match args["action"].as_str() {
        Some("narrate") => {
            let voice = choice(
                args,
                "voice",
                "pm_alex",
                &["pf_dora", "pm_alex", "pm_santa"],
            )?;
            let speed = args["speed"].as_f64().unwrap_or(1.0);
            if !(0.5..=2.0).contains(&speed) {
                return Err(error("Velocidade da voz inválida."));
            }
            Ok(
                json!({"action":"narrate","text":text(args,"text",4000)?,"voice":voice,"speed":speed,"narrationPolicy":2}),
            )
        }
        Some("music") => {
            if args["nonCommercial"] != true {
                return Err(error("MusicGen usa pesos CC-BY-NC-4.0. Confirme com o usuário o uso não comercial antes de gerar música; em apresentações comerciais, utilize uma trilha licenciada fornecida pelo usuário."));
            }
            let duration = args["duration"]
                .as_f64()
                .filter(|v| (1.0..=600.0).contains(v))
                .ok_or_else(|| error("Informe a duração da trilha entre 1 e 600 segundos."))?;
            let seed = args["seed"].as_u64().unwrap_or(0);
            if seed > 2_147_483_647 {
                return Err(error("Seed musical inválida."));
            }
            Ok(
                json!({"action":"music","prompt":text(args,"prompt",1000)?,"duration":duration,"seed":seed,"nonCommercial":true}),
            )
        }
        _ => Err(error("Selecione narrate ou music.")),
    }
}

fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn hash_file(path: &Path) -> Result<String, AgentError> {
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    let mut file = options
        .open(path)
        .map_err(|_| error("Não foi possível verificar o áudio."))?;
    let metadata = file.metadata().map_err(|_| error("Áudio inacessível."))?;
    if !metadata.is_file() || metadata.len() > 256 * 1024 * 1024 {
        return Err(error("Use um WAV regular de até 256 MB."));
    }
    let mut digest = Sha256::new();
    let mut bytes = [0; 64 * 1024];
    loop {
        let length = file
            .read(&mut bytes)
            .map_err(|_| error("Falha ao verificar o áudio."))?;
        if length == 0 {
            break;
        }
        digest.update(&bytes[..length]);
    }
    Ok(format!("{:x}", digest.finalize()))
}

pub(super) fn valid_receipt(record: &Value, path: &Path) -> bool {
    record["version"] == 1
        && record["requestHash"] == hash(record["request"].to_string().as_bytes())
        && hash_file(path).is_ok_and(|digest| record["audioHash"] == digest)
}

pub(super) fn receipt_path(output: &Path) -> PathBuf {
    output.with_extension("wav.jarvis.json")
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct WaveInfo {
    pub duration: f64,
    pub sample_rate: u32,
    pub frames: u64,
}

// The engines publish PCM16. Validate chunk boundaries rather than trusting an extension.
pub(super) fn wave(path: &Path) -> Result<WaveInfo, AgentError> {
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    let mut file = options
        .open(path)
        .map_err(|_| error("Áudio WAV inacessível."))?;
    let metadata = file.metadata().map_err(|_| error("Áudio inacessível."))?;
    if !metadata.is_file() || metadata.len() > 256 * 1024 * 1024 {
        return Err(error("Use um WAV regular de até 256 MB."));
    }
    let size = metadata.len();
    let mut header = [0; 12];
    file.read_exact(&mut header)
        .map_err(|_| error("Áudio WAV incompleto."))?;
    if &header[..4] != b"RIFF"
        || &header[8..] != b"WAVE"
        || u32::from_le_bytes(header[4..8].try_into().unwrap()) as u64 + 8 != size
    {
        return Err(error("A geração não produziu um WAV PCM válido."));
    }
    let mut position = 12;
    let mut format = None;
    let mut data = None;
    use std::io::{Seek, SeekFrom};
    while position + 8 <= size {
        let mut chunk = [0; 8];
        file.read_exact(&mut chunk)
            .map_err(|_| error("WAV incompleto."))?;
        let length = u32::from_le_bytes(chunk[4..].try_into().unwrap()) as u64;
        position += 8;
        if position + length > size {
            return Err(error("WAV truncado."));
        }
        if &chunk[..4] == b"fmt " && length >= 16 {
            let mut bytes = [0; 16];
            file.read_exact(&mut bytes)
                .map_err(|_| error("WAV incompleto."))?;
            let codec = u16::from_le_bytes(bytes[0..2].try_into().unwrap());
            let channels = u16::from_le_bytes(bytes[2..4].try_into().unwrap());
            let rate = u32::from_le_bytes(bytes[4..8].try_into().unwrap());
            let byte_rate = u32::from_le_bytes(bytes[8..12].try_into().unwrap());
            let block = u16::from_le_bytes(bytes[12..14].try_into().unwrap());
            let bits = u16::from_le_bytes(bytes[14..16].try_into().unwrap());
            if codec != 1
                || !(1..=2).contains(&channels)
                || bits != 16
                || !(8_000..=96_000).contains(&rate)
                || block != channels * 2
                || byte_rate != rate * u32::from(block)
            {
                return Err(error(
                    "Use áudio WAV PCM16 mono ou estéreo, entre 8 e 96 kHz.",
                ));
            }
            format = Some((rate, block));
        } else if &chunk[..4] == b"data" {
            data = Some(length);
        }
        position += length + length % 2;
        file.seek(SeekFrom::Start(position))
            .map_err(|_| error("WAV inválido."))?;
    }
    let (sample_rate, block) = format.ok_or_else(|| error("WAV sem formato PCM."))?;
    let bytes = data
        .filter(|v| *v > 0 && *v % u64::from(block) == 0)
        .ok_or_else(|| error("WAV sem amostras completas."))?;
    let frames = bytes / u64::from(block);
    Ok(WaveInfo {
        duration: frames as f64 / f64::from(sample_rate),
        sample_rate,
        frames,
    })
}

pub(super) fn generate(
    session: &super::super::Session,
    home: &Path,
    args: &Value,
    sandbox: Option<&SandboxPlan>,
) -> Result<(Option<PreparedCommand>, Option<String>), AgentError> {
    let request = request(args)?;
    let digest = hash(request.to_string().as_bytes());
    let relative = text(args, "output", 4096)?;
    let output = super::super::tools::scoped(&session.root, &relative, true)?;
    if output.extension().and_then(|v| v.to_str()) != Some("wav") {
        return Err(error("Escolha um arquivo .wav no projeto."));
    }
    let receipt = receipt_path(&output);
    let receipt_relative = receipt
        .strip_prefix(&session.root)
        .map_err(|_| AgentError::internal())?
        .to_string_lossy()
        .to_string();
    super::super::tools::scoped(&session.root, &receipt_relative, true)?;
    if output.exists() {
        let verified = super::super::tools::scoped(&session.root, &relative, false)?;
        let previous = super::super::tools::scoped(&session.root, &receipt_relative, false)
            .and_then(|path| super::super::tools::read_text(&path))
            .ok()
            .and_then(|text| serde_json::from_str::<Value>(&text).ok());
        if previous
            .as_ref()
            .is_some_and(|v| v["requestHash"] == digest && valid_receipt(v, &verified))
        {
            let info = wave(&verified)?;
            return Ok((None,Some(json!({"resource":"hyperframes","action":request["action"],"status":"completed","exitCode":0,"path":relative,"reused":true,"audio":info,"receipt":previous}).to_string())));
        }
        return Err(error("O áudio existente foi preservado. Para uma narração/trilha alterada, escolha outro arquivo e atualize apenas a cena correspondente no presentation.json."));
    }
    if receipt.exists() {
        return Err(error(
            "Já existe um recibo neste destino. Escolha um novo arquivo WAV.",
        ));
    }
    let runtime = crate::core::audiovisual::runtime(home).map_err(AgentError::from)?;
    let lock = runtime.lock.clone();
    let mut options = fs::OpenOptions::new();
    options.write(true).create(true).truncate(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW);
        options.mode(0o600);
    }
    let lock_file = options
        .open(&lock)
        .map_err(|_| error("Não foi possível preparar a fila de geração audiovisual."))?;
    if !lock_file.metadata().is_ok_and(|meta| meta.is_file()) {
        return Err(error("Lock audiovisual inválido."));
    }
    let lock = lock
        .canonicalize()
        .map_err(|_| error("Lock audiovisual inacessível."))?;
    let sandbox = sandbox.map(|plan| plan.with_host_writable_file(&lock));
    let staged = tempfile::Builder::new()
        .prefix(".jarvis-audio-")
        .suffix(".wav")
        .tempfile_in(output.parent().ok_or_else(AgentError::internal)?)
        .map_err(|_| error("Não foi possível preparar o áudio."))?
        .into_temp_path();
    let action = request["action"].as_str().unwrap();
    let mut payload = request.clone();
    payload["models"] = json!(runtime.models);
    payload["output"] = json!(staged.to_string_lossy());
    payload["device"] = json!("auto");
    let argv = vec![
        runtime.entry.as_os_str().to_owned(),
        OsString::from("--request"),
        OsString::from(payload.to_string()),
    ];
    let (program, argv) = sandbox.as_ref().map_or_else(
        || (runtime.python.clone(), argv.clone()),
        |sandbox| sandbox.wrap(&runtime.python, argv.clone()),
    );
    let mut process = crate::background::tokio_command(program);
    process
        .args(argv)
        .envs(runtime.environment(action).map_err(AgentError::from)?)
        .current_dir(&runtime.package);
    let root = session.root.clone();
    let metadata =
        json!({"resource":"hyperframes","action":action,"path":relative,"requestHash":digest});
    let on_success = Box::new(move || {
        let info = wave(&staged)?;
        let audio_hash = hash_file(&staged)?;
        super::super::tools::scoped(&root, &relative, true)?;
        super::super::tools::scoped(&root, &receipt_relative, true)?;
        staged
            .persist_noclobber(&output)
            .map_err(|failure| {
                let unpublished=failure.path.keep().ok().and_then(|path|path.strip_prefix(&root).ok().map(|relative|relative.to_string_lossy().into_owned()));
                let mut cause=error("O destino do áudio mudou. O arquivo existente e o áudio não publicado foram preservados; inspecione-os antes de repetir.");
                cause.tool_result=Some(json!({"error":{"code":cause.code,"message":cause.message},"unpublishedPath":unpublished}).to_string());
                cause
            })?;
        let record = json!({"version":1,"requestHash":digest,"request":request,"audioHash":audio_hash,"audio":info});
        let mut file=fs::OpenOptions::new().write(true).create_new(true).open(&receipt)
            .map_err(|_|error("O áudio foi salvo, mas não foi possível registrar seu recibo. Inspecione o WAV antes de repetir a geração."))?;
        use std::io::Write;
        file.write_all(record.to_string().as_bytes())
            .map_err(|_| error("Falha ao salvar o recibo. O áudio concluído foi preservado."))?;
        file.sync_all()
            .map_err(|_| error("Falha ao persistir o recibo do áudio."))?;
        Ok(())
    });
    Ok((
        Some(PreparedCommand {
            process,
            metadata,
            on_success: Some(on_success),
        }),
        None,
    ))
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    pub(crate) fn write_wave(path: &Path, seconds: u32) {
        let length = 24_000 * 2 * seconds;
        let mut bytes = Vec::new();
        bytes.extend(b"RIFF");
        bytes.extend((length + 36).to_le_bytes());
        bytes.extend(b"WAVEfmt ");
        bytes.extend(16u32.to_le_bytes());
        bytes.extend(1u16.to_le_bytes());
        bytes.extend(1u16.to_le_bytes());
        bytes.extend(24_000u32.to_le_bytes());
        bytes.extend(48_000u32.to_le_bytes());
        bytes.extend(2u16.to_le_bytes());
        bytes.extend(16u16.to_le_bytes());
        bytes.extend(b"data");
        bytes.extend(length.to_le_bytes());
        bytes.resize(44 + length as usize, 0);
        fs::write(path, bytes).unwrap();
    }
    #[test]
    fn validates_pt_br_requests_and_music_license_before_execution() {
        assert_eq!(
            request(&json!({"action":"narrate","text":"Olá, mundo"})).unwrap()["voice"],
            "pm_alex"
        );
        assert!(
            request(&json!({"action":"music","prompt":"synth","duration":5}))
                .unwrap_err()
                .message
                .contains("CC-BY-NC")
        );
        assert!(request(
            &json!({"action":"music","prompt":"synth","duration":5,"nonCommercial":true})
        )
        .is_ok());
    }

    #[test]
    fn new_narration_timing_does_not_misidentify_old_receipts_as_new_generation() {
        let fixture = super::super::super::tests::Fixture::new();
        let session = super::super::super::tests::session(&fixture);
        let path = fixture.root.join("voice.wav");
        write_wave(&path, 2);
        let previous = json!({"action":"narrate","text":"Olá","voice":"pm_alex","speed":1.0});
        let record = json!({"version":1,"request":previous,"requestHash":hash(previous.to_string().as_bytes()),"audioHash":hash_file(&path).unwrap()});
        let confirmed = fs::read(&path).unwrap();
        fs::write(receipt_path(&path), record.to_string()).unwrap();
        assert!(valid_receipt(&record, &path));
        let next = request(&json!({"action":"narrate","text":"Olá"})).unwrap();
        assert_eq!(next["narrationPolicy"], 2);
        assert_ne!(record["requestHash"], hash(next.to_string().as_bytes()));
        let failure = generate(
            &session,
            &fixture.root,
            &json!({"action":"narrate","text":"Olá","output":"voice.wav"}),
            None,
        )
        .err()
        .unwrap();
        assert!(failure.message.contains("preservado"));
        assert_eq!(fs::read(&path).unwrap(), confirmed);
        assert_eq!(
            fs::read_to_string(receipt_path(&path)).unwrap(),
            record.to_string()
        );
        fs::write(
            fixture.root.join("presentation.json"),
            json!({"scenes":[{"id":"intro","narration":"Olá","audio":"voice.wav"}]}).to_string(),
        )
        .unwrap();
        assert_eq!(
            super::super::presentation::inspect(&fixture.root, &json!({"path":"."})).unwrap()
                ["ready"],
            true
        );
        assert_eq!(wave(&path).unwrap().duration, 2.0);
    }
    #[test]
    fn duration_comes_from_samples_and_truncation_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("voice.wav");
        write_wave(&path, 2);
        assert_eq!(wave(&path).unwrap().duration, 2.0);
        let mut bytes = fs::read(&path).unwrap();
        bytes.pop();
        fs::write(&path, bytes).unwrap();
        assert!(wave(&path).is_err());
    }
}
