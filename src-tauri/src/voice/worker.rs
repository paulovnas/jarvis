//! One native owner for capture, recognition and speech, with independent cancellation.
use super::{audio, Config, Control, Phase, Session};
use serde::Serialize;
use std::{
    collections::VecDeque,
    path::{Path, PathBuf},
    sync::{atomic::Ordering, mpsc, Arc},
    time::{Duration, Instant},
};
use tauri::Emitter;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
use whisper_rs::{
    FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters, WhisperVadContext,
    WhisperVadContextParams,
};

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Transcript {
    session_id: String,
    target: String,
    sequence: u64,
    text: String,
    mode: String,
}

struct Cancellation {
    session: Arc<Session>,
    revision: u64,
}
unsafe extern "C" fn abort_transcription(data: *mut std::ffi::c_void) -> bool {
    // SAFETY: transcription keeps this immutable value alive for the synchronous
    // full() call, including its native worker threads; only atomics are read.
    let cancellation = unsafe { &*(data as *const Cancellation) };
    cancellation.session.cancelled()
        || cancellation.session.audio_revision.load(Ordering::Acquire) != cancellation.revision
}

fn transcription(
    context: &WhisperContext,
    samples: &[f32],
    session: &Arc<Session>,
) -> Result<String, String> {
    let mut state = context
        .create_state()
        .map_err(|_| "Não foi possível preparar a transcrição local.")?;
    let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
    params.set_n_threads(
        std::thread::available_parallelism().map_or(2, |n| n.get().clamp(1, 4)) as i32,
    );
    params.set_language(Some("pt"));
    params.set_translate(false);
    params.set_no_context(true);
    params.set_initial_prompt("Jarvis. Jarvito. Programação: GitHub, TypeScript, JavaScript, React, Rust, terminal, API, commit, branch, pull request.");
    params.set_print_realtime(false);
    params.set_print_progress(false);
    params.set_print_special(false);
    params.set_print_timestamps(false);
    let revision = session.audio_revision.load(Ordering::Acquire);
    let cancellation = Cancellation {
        session: session.clone(),
        revision,
    };
    // whisper-rs 0.16's safe wrapper double-boxes a trait object then casts it to
    // the original closure type. Use its documented C API with a stable lifetime.
    unsafe {
        params.set_abort_callback(Some(abort_transcription));
        params
            .set_abort_callback_user_data((&cancellation as *const Cancellation).cast_mut().cast());
    }
    let result = state.full(params, samples);
    if session.cancelled() || session.audio_revision.load(Ordering::Acquire) != revision {
        return Ok(String::new());
    }
    result.map_err(|_| "A transcrição não pôde ser concluída.")?;
    let text = state
        .as_iter()
        .filter(|segment| segment.no_speech_probability() < 0.85)
        .filter_map(|segment| segment.to_str_lossy().ok().map(|text| text.into_owned()))
        .collect::<String>();
    Ok(text.trim().to_owned())
}

/// Keep prose, omit executable code and URLs. Never vocalize tool logs or hidden reasoning.
pub(super) fn spoken_text(text: &str) -> String {
    let mut code = false;
    let mut lines = Vec::new();
    let links = regex::Regex::new(r"!?\[([^\]]*)\]\([^)]*\)").unwrap();
    let urls = regex::Regex::new(r"https?://\S+").unwrap();
    for line in text.lines() {
        if line.trim_start().starts_with("```") || line.trim_start().starts_with("~~~") {
            code = !code;
            continue;
        }
        if code {
            continue;
        }
        let line = links.replace_all(line, "$1").into_owned();
        let line = urls.replace_all(&line, "").into_owned();
        lines.push(
            line.trim()
                .trim_start_matches(['#', '>', '-', '*'])
                .replace(['`', '*', '_'], ""),
        );
    }
    lines
        .join(" ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}
fn phrases(text: &str) -> Vec<String> {
    let mut output = Vec::new();
    let mut phrase = String::new();
    for word in text.split_whitespace() {
        if word.chars().count() > 650 {
            continue;
        }
        if phrase.chars().count() + word.chars().count() > 650 && !phrase.is_empty() {
            output.push(std::mem::take(&mut phrase));
        }
        if !phrase.is_empty() {
            phrase.push(' ');
        }
        phrase.push_str(word);
        if phrase.len() > 200 && phrase.ends_with(['.', '!', '?']) {
            output.push(std::mem::take(&mut phrase));
        }
    }
    if !phrase.is_empty() {
        output.push(phrase);
    }
    output
}

struct Speech {
    child: tokio::process::Child,
    input: tokio::process::ChildStdin,
    output: tokio::io::BufReader<tokio::process::ChildStdout>,
    directory: tempfile::TempDir,
}
impl Speech {
    async fn open(home: &Path, session: &Session) -> Result<Self, String> {
        let runtime = crate::core::audiovisual::runtime(home)
            .map_err(|_| "Prepare o componente Audiovisual no Core para ouvir o Jarvis.")?;
        let directory =
            tempfile::tempdir().map_err(|_| "Não foi possível preparar a voz local.")?;
        let entry = directory.path().join("voice.py");
        std::fs::write(&entry, include_bytes!("speech.py"))
            .map_err(|_| "Não foi possível preparar a voz local.")?;
        let mut command = tokio::process::Command::new(&runtime.python);
        command
            .arg(entry)
            .arg(&runtime.models)
            .envs(
                runtime
                    .environment("narrate")
                    .map_err(|error| error.message)?,
            )
            .current_dir(runtime.package)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true);
        #[cfg(windows)]
        command.creation_flags(0x08000000);
        let mut child = command.spawn().map_err(|_| {
            "Não foi possível iniciar a voz local. Repare o componente Audiovisual no Core."
        })?;
        let input = child.stdin.take().ok_or("Canal de voz indisponível.")?;
        let output =
            tokio::io::BufReader::new(child.stdout.take().ok_or("Canal de voz indisponível.")?);
        let mut speech = Self {
            child,
            input,
            output,
            directory,
        };
        let result = speech
            .response(session, session.audio_revision.load(Ordering::Acquire))
            .await?;
        if result["ready"] != true {
            return Err("A voz local não conseguiu carregar os modelos.".into());
        }
        Ok(speech)
    }
    async fn response(
        &mut self,
        session: &Session,
        revision: u64,
    ) -> Result<serde_json::Value, String> {
        let mut line = Vec::new();
        loop {
            tokio::select! {
                result = self.output.read_until(b'\n', &mut line) => {
                    let length = result.map_err(|_| "O canal da voz local foi interrompido.")?;
                    if length == 0 || line.len() > 8192 { return Err("A voz local foi encerrada. Tente novamente ou repare Audiovisual no Core.".into()); }
                    return serde_json::from_slice(&line).map_err(|_| "A voz local retornou uma resposta inválida.".into());
                }
                _ = tokio::time::sleep(Duration::from_millis(40)) => {
                    if session.cancelled() || session.audio_revision.load(Ordering::Acquire) != revision {
                        let _ = self.child.kill().await; return Err("Fala interrompida.".into());
                    }
                }
            }
        }
    }
    async fn synthesize(
        &mut self,
        text: &str,
        config: &Config,
        session: &Session,
        revision: u64,
    ) -> Result<PathBuf, String> {
        let output = self.directory.path().join("speech.wav");
        let request = serde_json::json!({"text":text,"voice":config.voice,"speed":config.speed,"output":output});
        self.input
            .write_all(format!("{request}\n").as_bytes())
            .await
            .map_err(|_| "Não foi possível solicitar a fala local.")?;
        self.input
            .flush()
            .await
            .map_err(|_| "Canal de voz indisponível.")?;
        let response = self.response(session, revision).await?;
        if response["ok"] != true {
            return Err("Não foi possível gerar a fala. Repare Audiovisual no Core se o problema persistir.".into());
        }
        Ok(output)
    }
}

fn speak(
    app: &tauri::AppHandle,
    home: &Path,
    config: &Config,
    session: &Arc<Session>,
    speech: &mut Option<Speech>,
    text: &str,
) -> Result<(), String> {
    session.enabled.store(false, Ordering::Release);
    let revision = session.audio_revision.load(Ordering::Acquire);
    for phrase in phrases(&spoken_text(text)) {
        if session.cancelled() || session.audio_revision.load(Ordering::Acquire) != revision {
            break;
        }
        session.update(app, Phase::Synthesizing, Some(phrase.clone()), None, 0.);
        let result = tauri::async_runtime::block_on(async {
            if speech.is_none() {
                *speech = Some(Speech::open(home, session).await?);
            }
            speech
                .as_mut()
                .ok_or("Voz local indisponível.")?
                .synthesize(&phrase, config, session, revision)
                .await
        });
        if session.cancelled() || session.audio_revision.load(Ordering::Acquire) != revision {
            *speech = None;
            break;
        }
        let path = result?;
        let device = audio::device(config.speaker.as_deref(), false)?;
        let failed = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let failure = failed.clone();
        let mut sink = rodio::DeviceSinkBuilder::from_device(device)
            .map_err(|_| "Alto-falante indisponível.")?
            .with_error_callback(move |_| {
                failure.store(true, Ordering::Release);
            })
            .open_sink_or_fallback()
            .map_err(|_| "Não foi possível abrir o alto-falante.")?;
        sink.log_on_drop(false);
        let player = rodio::Player::connect_new(sink.mixer());
        let source = rodio::Decoder::try_from(
            std::fs::File::open(path).map_err(|_| "Áudio de voz indisponível.")?,
        )
        .map_err(|_| "O áudio da voz local é inválido.")?;
        let level = Arc::new(std::sync::atomic::AtomicU32::new(0));
        player.append(audio::Metered::new(source, level.clone()));
        session.update(app, Phase::Speaking, Some(phrase), None, 0.);
        while !player.empty()
            && !session.cancelled()
            && session.audio_revision.load(Ordering::Acquire) == revision
        {
            session.update(
                app,
                Phase::Speaking,
                None,
                None,
                f32::from_bits(level.load(Ordering::Acquire)),
            );
            if failed.load(Ordering::Acquire) {
                return Err("O alto-falante foi desconectado durante a fala.".into());
            }
            std::thread::sleep(Duration::from_millis(35));
        }
        player.stop();
    }
    Ok(())
}

pub(super) fn run(
    app: &tauri::AppHandle,
    home: &Path,
    config: &Config,
    paths: Option<(PathBuf, PathBuf)>,
    session: &Arc<Session>,
    controls: mpsc::Receiver<Control>,
) -> Result<(), String> {
    let mut speech = None;
    if session.mode == "test" {
        return speak(app, home, config, session, &mut speech, "Olá! Eu sou o Jarvis. Agora podemos conversar por voz, em português, no chat e com o Jarvito.");
    }
    let (model, vad_path) = paths.ok_or("Modelo de transcrição indisponível.")?;
    let parameters = WhisperContextParameters::default();
    let context = WhisperContext::new_with_params(&model, parameters)
        .or_else(|_| {
            let mut parameters = WhisperContextParameters::default();
            parameters.use_gpu(false);
            WhisperContext::new_with_params(&model, parameters)
        })
        .map_err(|_| {
            "Não foi possível carregar o Whisper local. Prepare novamente o modelo de voz."
        })?;
    if session.cancelled() {
        return Ok(());
    }
    let mut vad = WhisperVadContext::new(
        &vad_path.to_string_lossy(),
        WhisperVadContextParams::default(),
    )
    .map_err(|_| {
        "Não foi possível carregar o detector de fala. Prepare novamente o modelo de voz."
    })?;
    if session.cancelled() {
        return Ok(());
    }
    if session.mode == "call" {
        speak(
            app,
            home,
            config,
            session,
            &mut speech,
            "Oi! Estou aqui. Pode falar.",
        )?;
        std::thread::sleep(Duration::from_millis(180));
    }
    if session.cancelled() {
        return Ok(());
    }
    let capture = audio::capture(config.microphone.as_deref(), session.enabled.clone())?;
    let mut resampler = audio::Resampler::new(capture.rate);
    let mut utterance = audio::Utterance::new(config.silence_ms);
    let mut window = VecDeque::new();
    let mut since_vad = 0;
    let mut speaking = false;
    let mut sequence = 0;
    let mut meter = Instant::now();
    session.listen(app);
    while !session.cancelled() {
        if capture.failed.load(Ordering::Acquire) {
            return Err("O microfone foi desconectado ou perdeu a permissão. Verifique o dispositivo e tente novamente.".into());
        }
        if capture.dropped.swap(false, Ordering::AcqRel) {
            utterance.clear();
            window.clear();
            return Err("A captura de áudio atrasou. A fala incompleta não foi enviada; tente novamente com o modelo Tiny.".into());
        }
        let control = controls.try_recv().ok();
        let finishing = matches!(control, Some(Control::Finish));
        let mut complete = None;
        match control {
            Some(Control::Finish) => complete = utterance.finish(),
            Some(Control::Speak(text)) => {
                utterance.clear();
                window.clear();
                speak(app, home, config, session, &mut speech, &text)?;
                // Drain old capture and allow the hardware output buffer to finish.
                std::thread::sleep(Duration::from_millis(180));
                for _ in capture.audio.try_iter() {}
                since_vad = 0;
                speaking = false;
                session.listen(app);
            }
            Some(Control::Resume | Control::Interrupt) => {
                utterance.clear();
                window.clear();
                for _ in capture.audio.try_iter() {}
                session.listen(app);
            }
            Some(Control::Mute) => {
                utterance.clear();
                window.clear();
                session.update(app, Phase::Paused, None, None, 0.);
            }
            None => {}
        }
        if session.enabled.load(Ordering::Acquire) {
            if let Ok(samples) = capture.audio.recv_timeout(Duration::from_millis(30)) {
                let samples = resampler.push(samples);
                let level = audio::level(&samples);
                window.extend(samples.iter().copied());
                while window.len() > audio::RATE {
                    window.pop_front();
                }
                since_vad += samples.len();
                if window.len() >= 512 && since_vad >= audio::RATE / 10 {
                    let samples: Vec<f32> = window.iter().copied().collect();
                    vad.detect_speech(&samples)
                        .map_err(|_| "O detector de fala não conseguiu processar o microfone.")?;
                    let probabilities = vad.probabilities();
                    speaking = probabilities
                        .iter()
                        .rev()
                        .take(3)
                        .any(|probability| *probability >= 0.5)
                        && level > 0.002;
                    since_vad = 0;
                }
                if session.mode == "dictation" {
                    // Dictation stops on the user's button; silence must not discard a longer thought.
                    if let Some(chunk) = utterance.dictation(&samples, speaking) {
                        complete = Some(chunk);
                    }
                } else {
                    complete = utterance.push(&samples, speaking);
                }
                if meter.elapsed() >= Duration::from_millis(80) {
                    session.update(app, Phase::Listening, None, None, (level * 8.).min(1.));
                    meter = Instant::now();
                }
            }
        } else {
            std::thread::sleep(Duration::from_millis(20));
        }
        if let Some(samples) = complete {
            session.enabled.store(false, Ordering::Release);
            session.update(app, Phase::Transcribing, None, None, 0.);
            let text = transcription(&context, &samples, session)?;
            for _ in capture.audio.try_iter() {}
            window.clear();
            utterance.clear();
            if !text.is_empty() && !session.cancelled() {
                sequence += 1;
                let transcript = Transcript {
                    session_id: session.id.clone(),
                    target: session
                        .target
                        .lock()
                        .map_err(|_| "Destino da voz indisponível.")?
                        .clone(),
                    sequence,
                    text: text.clone(),
                    mode: session.mode.clone(),
                };
                session.update(app, Phase::Thinking, Some(text), None, 0.);
                let _ = app.emit_to(&session.owner, "voice:transcript", transcript);
                if session.mode == "dictation" {
                    return Ok(());
                }
            } else {
                session.listen(app);
            }
        } else if finishing {
            return Err(
                "Não identifiquei uma fala. Aproxime-se do microfone e tente novamente.".into(),
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn spoken_answers_preserve_prose_and_never_read_code_or_link_targets() {
        assert_eq!(spoken_text("## Olá\nAbra [o projeto](https://secret/token).\n```sh\nrm -rf /\n```\n**Concluído.**"), "Olá Abra o projeto. Concluído.");
    }
    #[test]
    fn long_answers_are_split_on_complete_words_for_the_warm_engine() {
        let text = "Uma resposta em português. ".repeat(120);
        let chunks = phrases(&text);
        assert!(chunks.len() > 1);
        assert!(chunks.iter().all(|chunk| chunk.chars().count() <= 650));
        assert_eq!(chunks.join(" "), text.trim());
        assert!(phrases(&"x".repeat(1600)).is_empty());
    }

    #[test]
    #[ignore = "requires pinned local Whisper/VAD models and synthetic PT-BR WAV; see scripts/validate-voice.py"]
    fn real_portuguese_transcription_and_voice_activity() {
        let root = PathBuf::from(
            std::env::var("JARVIS_VOICE_SMOKE_DIR").expect("Set JARVIS_VOICE_SMOKE_DIR"),
        );
        let file = std::fs::File::open(root.join("speech-16k.wav")).unwrap();
        let decoder = rodio::Decoder::try_from(file).unwrap();
        let samples: Vec<f32> = decoder.collect();
        let mut parameters = WhisperContextParameters::default();
        parameters.use_gpu(std::env::var_os("JARVIS_VOICE_SMOKE_GPU").is_some());
        let context = WhisperContext::new_with_params(
            root.join("ggml-tiny-q5_1.bin")
                .to_string_lossy()
                .into_owned(),
            parameters,
        )
        .unwrap();
        let (sender, _) = mpsc::sync_channel(1);
        let session = Arc::new(Session {
            id: "smoke".into(),
            target: std::sync::Mutex::new("voice-test".into()),
            owner: "main".into(),
            mode: "call".into(),
            stop: std::sync::atomic::AtomicBool::new(false),
            enabled: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            muted: std::sync::atomic::AtomicBool::new(false),
            audio_revision: std::sync::atomic::AtomicU64::new(0),
            sender,
        });
        let started = Instant::now();
        let text = transcription(&context, &samples, &session).unwrap();
        assert!(
            text.to_lowercase().contains("projeto"),
            "PT-BR transcription: {text}"
        );
        assert!(
            text.to_lowercase().contains("arquivos"),
            "PT-BR transcription: {text}"
        );
        let mut vad = WhisperVadContext::new(
            &root.join("ggml-silero-v6.2.0.bin").to_string_lossy(),
            WhisperVadContextParams::default(),
        )
        .unwrap();
        vad.detect_speech(&samples).unwrap();
        assert!(vad.probabilities().iter().any(|p| *p >= 0.5));
        vad.detect_speech(&vec![0.; audio::RATE]).unwrap();
        assert!(vad.probabilities().iter().all(|p| *p < 0.5));
        eprintln!(
            "Real Whisper/VAD PT-BR passed in {:.2}s: {text}",
            started.elapsed().as_secs_f32()
        );
        session.stop.store(true, Ordering::Release);
        assert_eq!(transcription(&context, &samples, &session).unwrap(), "");
    }
}
