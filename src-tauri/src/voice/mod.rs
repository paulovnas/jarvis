//! Voice is an input/output adapter for existing conversations, never a second agent.
mod announcements;
mod audio;
pub(crate) mod models;
mod worker;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::Write,
    path::Path,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc, Arc, Mutex,
    },
};
use tauri::{Emitter, Manager};

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Config {
    pub enabled: bool,
    pub microphone: Option<String>,
    pub speaker: Option<String>,
    pub model: String,
    pub voice: String,
    pub speed: f32,
    pub silence_ms: u32,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            enabled: false,
            microphone: None,
            speaker: None,
            model: "small".into(),
            voice: "pm_alex".into(),
            speed: 1.,
            silence_ms: 650,
        }
    }
}
impl Config {
    fn validate(&self) -> Result<(), String> {
        if !models::MODELS.iter().any(|model| model.id == self.model)
            || !["pf_dora", "pm_alex", "pm_santa"].contains(&self.voice.as_str())
            || !self.speed.is_finite()
            || !(0.75..=1.5).contains(&self.speed)
            || !(400..=1800).contains(&self.silence_ms)
            || [&self.microphone, &self.speaker]
                .into_iter()
                .flatten()
                .any(|id| id.is_empty() || id.len() > 1024 || id.contains('\0'))
        {
            return Err("Configuração de voz inválida.".into());
        }
        Ok(())
    }
}
fn config(home: &Path) -> Result<Config, String> {
    let path = crate::data_dir::root(home).join("voice.json");
    match fs::read(path) {
        Ok(bytes) => {
            let config: Config = serde_json::from_slice(&bytes)
                .map_err(|_| "As configurações de voz estão inválidas.")?;
            config.validate()?;
            Ok(config)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Config::default()),
        Err(_) => Err("Não foi possível ler as configurações de voz.".into()),
    }
}

#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SessionView {
    pub started_at: Option<u64>,
    pub speaker: Option<String>,
    pub id: Option<String>,
    pub target: Option<String>,
    pub owner: Option<String>,
    pub mode: Option<String>,
    pub phase: Phase,
    pub level: f32,
    pub transcript: String,
    pub error: Option<String>,
    pub revision: u64,
}
#[derive(Clone, Copy, Debug, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) enum Phase {
    #[default]
    Idle,
    Preparing,
    Listening,
    Transcribing,
    Synthesizing,
    Speaking,
    Closing,
    Error,
}

#[derive(Default)]
pub(crate) struct VoiceState {
    active: Mutex<Option<Arc<Session>>>,
    view: Mutex<SessionView>,
    speech: Mutex<Option<worker::Speech>>,
    pub(super) downloading: AtomicBool,
    pub(super) cancel_download: AtomicBool,
    pub(super) download: Mutex<Option<models::Download>>,
}
pub(super) struct Session {
    id: String,
    target: String,
    owner: String,
    mode: String,
    stop: AtomicBool,
    enabled: Arc<AtomicBool>,
    audio_revision: AtomicU64,
    speech_revision: AtomicU64,
    sender: mpsc::SyncSender<Control>,
}
pub(super) enum Control {
    Finish,
}

impl VoiceState {
    pub(crate) fn silence_speech(&self, app: &tauri::AppHandle) {
        if let Ok(active) = self.active.lock() {
            if let Some(session) = active
                .as_ref()
                .filter(|session| session.mode == "announcement")
            {
                session.stop_for_owner(None);
                session.update(app, Phase::Closing, None, None, 0.);
            }
        }
    }
    pub(crate) fn shutdown(&self, app: &tauri::AppHandle, owner: Option<&str>) {
        if let Ok(active) = self.active.lock() {
            if let Some(session) = active.as_ref() {
                if session.stop_for_owner(owner) {
                    session.update(app, Phase::Closing, None, None, 0.);
                }
            }
        }
        if owner.is_none() {
            self.cancel_download.store(true, Ordering::Release);
        }
    }
}
impl Session {
    fn stop_for_owner(&self, owner: Option<&str>) -> bool {
        if owner.is_some_and(|owner| owner != self.owner) {
            return false;
        }
        self.stop.store(true, Ordering::Release);
        self.enabled.store(false, Ordering::Release);
        self.audio_revision.fetch_add(1, Ordering::AcqRel);
        self.speech_revision.fetch_add(1, Ordering::AcqRel);
        true
    }
    fn cancelled(&self) -> bool {
        self.stop.load(Ordering::Acquire)
    }
    fn current_speech(&self, revision: u64) -> bool {
        !self.cancelled() && self.speech_revision.load(Ordering::Acquire) == revision
    }
    fn update(
        &self,
        app: &tauri::AppHandle,
        phase: Phase,
        transcript: Option<String>,
        error: Option<String>,
        level: f32,
    ) {
        if self.cancelled() && !matches!(phase, Phase::Idle | Phase::Closing) {
            return;
        }
        let state = app.state::<VoiceState>();
        if let Ok(mut view) = state.view.lock() {
            if view.id.as_deref() != Some(&self.id) {
                return;
            }
            view.phase = phase;
            view.level = level.clamp(0., 1.);
            view.error = error;
            if let Some(transcript) = transcript {
                view.transcript = transcript;
                view.speaker = Some(
                    if self.mode == "dictation" {
                        "user"
                    } else {
                        "jarvis"
                    }
                    .into(),
                );
            }
            view.revision += 1;
            let _ = app.emit("voice:state", view.clone());
        };
    }
    fn listen(&self, app: &tauri::AppHandle) {
        self.enabled.store(!self.cancelled(), Ordering::Release);
        self.update(app, Phase::Listening, None, None, 0.);
    }
    fn finish(&self, app: &tauri::AppHandle, error: Option<String>) {
        self.enabled.store(false, Ordering::Release);
        let state = app.state::<VoiceState>();
        if let Ok(mut active) = state.active.lock() {
            if active.as_ref().is_some_and(|session| session.id == self.id) {
                *active = None;
            }
        }
        self.update(
            app,
            if error.is_some() {
                Phase::Error
            } else {
                Phase::Idle
            },
            None,
            error,
            0.,
        );
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Settings {
    config: Config,
    microphones: Vec<audio::Device>,
    speakers: Vec<audio::Device>,
    models: Vec<models::Model>,
    speech_ready: bool,
    speech_error: Option<String>,
    announcement_clips: Vec<String>,
    download: Option<models::Download>,
    session: SessionView,
}
fn settings(app: tauri::AppHandle) -> Result<Settings, String> {
    let home = app
        .path()
        .home_dir()
        .map_err(|_| "Diretório de dados indisponível.")?;
    let state = app.state::<VoiceState>();
    let speech = crate::core::audiovisual::runtime(&home);
    let download = state
        .download
        .lock()
        .map_err(|_| "Estado da voz indisponível.")?
        .clone();
    let session = state
        .view
        .lock()
        .map_err(|_| "Estado da voz indisponível.")?
        .clone();
    Ok(Settings {
        config: config(&home)?,
        microphones: audio::devices(true)?,
        speakers: audio::devices(false)?,
        models: models::list(&home),
        speech_ready: speech.is_ok(),
        speech_error: speech.err().map(|error| error.message),
        announcement_clips: announcements::available(&app),
        download,
        session,
    })
}

#[tauri::command]
pub(crate) async fn get_voice_settings(app: tauri::AppHandle) -> Result<Settings, String> {
    tokio::task::spawn_blocking(move || settings(app))
        .await
        .map_err(|_| "Não foi possível consultar os dispositivos de voz.".to_owned())?
}

#[tauri::command]
pub(crate) async fn save_voice_settings(
    app: tauri::AppHandle,
    config: Config,
) -> Result<Settings, String> {
    config.validate()?;
    let home = app
        .path()
        .home_dir()
        .map_err(|_| "Diretório de dados indisponível.")?;
    let root = crate::data_dir::root(&home);
    fs::create_dir_all(&root).map_err(|_| "Diretório de dados indisponível.")?;
    let mut staged = tempfile::NamedTempFile::new_in(&root)
        .map_err(|_| "Não foi possível salvar as configurações de voz.")?;
    staged
        .write_all(&serde_json::to_vec(&config).map_err(|_| "Configuração de voz inválida.")?)
        .map_err(|_| "Não foi possível salvar as configurações de voz.")?;
    staged
        .as_file()
        .sync_all()
        .map_err(|_| "Não foi possível salvar as configurações de voz.")?;
    staged
        .persist(root.join("voice.json"))
        .map_err(|_| "Não foi possível salvar as configurações de voz.")?;
    if !config.enabled {
        app.state::<VoiceState>().shutdown(&app, None);
    }
    let _ = app.emit("voice:changed", ());
    get_voice_settings(app).await
}

#[tauri::command]
pub(crate) fn get_voice_session(
    state: tauri::State<'_, VoiceState>,
) -> Result<SessionView, String> {
    state
        .view
        .lock()
        .map(|view| view.clone())
        .map_err(|_| "Estado da voz indisponível.".into())
}
fn valid_target(target: &str) -> bool {
    target
        .strip_prefix("chat:")
        .is_some_and(|id| id.len() == 32 && id.bytes().all(|c| c.is_ascii_hexdigit()))
        || target == "voice-test"
}
fn valid_session(target: &str, mode: &str, owner: &str) -> bool {
    match mode {
        "dictation" => owner == "main" && target.starts_with("chat:") && valid_target(target),
        "test" => owner == "main" && target == "voice-test",
        "announcement" => owner == "companion" && target == "companion-notice",
        _ => false,
    }
}
fn recognition_required(mode: &str) -> bool {
    mode == "dictation"
}
fn announcement_text(mode: &str, text: Option<String>) -> Result<Option<String>, String> {
    match (mode, text) {
        ("announcement", Some(text))
            if !text.trim().is_empty() && text.chars().count() <= 600 && !text.contains('\0') =>
        {
            Ok(Some(text))
        }
        ("announcement", _) | (_, Some(_)) => Err("Texto de aviso inválido.".into()),
        (_, None) => Ok(None),
    }
}
fn announcement_clip(mode: &str, clip: Option<String>) -> Result<Option<String>, String> {
    match (mode, clip) {
        ("announcement", Some(clip)) if announcements::valid_id(&clip) => Ok(Some(clip)),
        (_, Some(_)) => Err("Áudio de aviso inválido.".into()),
        (_, None) => Ok(None),
    }
}
#[tauri::command]
pub(crate) fn start_voice_session(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
    target: String,
    mode: String,
    text: Option<String>,
    clip: Option<String>,
) -> Result<SessionView, String> {
    if !valid_session(&target, &mode, window.label()) {
        return Err("Destino da conversa por voz inválido.".into());
    }
    let text = announcement_text(&mode, text)?;
    let clip = announcement_clip(&mode, clip)?;
    let clip_path = clip
        .as_deref()
        .and_then(|id| announcements::resolve(&app, id));
    let home = app
        .path()
        .home_dir()
        .map_err(|_| "Diretório de dados indisponível.")?;
    let config = config(&home)?;
    if !config.enabled && clip_path.is_none() {
        return Err("Ative o Jarvis Voice nas configurações de voz.".into());
    }
    let paths = if !recognition_required(&mode) {
        None
    } else {
        Some(models::paths(&home, &config)?)
    };
    if mode != "dictation" && clip_path.is_none() {
        crate::core::audiovisual::runtime(&home)
            .map_err(|_| "Prepare o componente Audiovisual no Core para ouvir o Jarvis.")?;
    }
    let state = app.state::<VoiceState>();
    let mut active = state
        .active
        .lock()
        .map_err(|_| "Estado da voz indisponível.")?;
    if let Some(previous) = active.as_ref() {
        if previous.mode == "announcement" && mode == "dictation" {
            previous.stop_for_owner(None);
        } else {
            return Err(
                "Já existe uma sessão de voz ativa. Encerre-a antes de iniciar outra.".into(),
            );
        }
    }
    let mut bytes = [0; 16];
    getrandom::fill(&mut bytes).map_err(|_| "Não foi possível iniciar a sessão de voz.")?;
    let id = bytes
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let (sender, receiver) = mpsc::sync_channel(16);
    let session = Arc::new(Session {
        id: id.clone(),
        target: target.clone(),
        owner: window.label().into(),
        mode: mode.clone(),
        stop: AtomicBool::new(false),
        enabled: Arc::new(AtomicBool::new(false)),
        audio_revision: AtomicU64::new(0),
        speech_revision: AtomicU64::new(0),
        sender,
    });
    let mut view = SessionView {
        started_at: Some(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
        ),
        id: Some(id),
        target: Some(target),
        owner: Some(session.owner.clone()),
        mode: Some(mode),
        phase: Phase::Preparing,
        ..Default::default()
    };
    {
        let mut current = state
            .view
            .lock()
            .map_err(|_| "Estado da voz indisponível.")?;
        view.revision = current.revision + 1;
        *current = view.clone();
    }
    *active = Some(session.clone());
    drop(active);
    let _ = app.emit("voice:state", view.clone());
    let worker_app = app.clone();
    let worker_session = session.clone();
    std::thread::Builder::new()
        .name("jarvis-voice".into())
        .spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                worker::run(
                    &worker_app,
                    &home,
                    &config,
                    paths,
                    &worker_session,
                    receiver,
                    worker::SpeechRequest {
                        text: text.as_deref(),
                        clip: clip_path.as_deref(),
                    },
                )
            }))
            .unwrap_or_else(|_| {
                Err("O serviço de voz foi interrompido. Inicie uma nova sessão.".into())
            });
            worker_session.finish(
                &worker_app,
                if worker_session.cancelled() {
                    None
                } else {
                    result.err()
                },
            );
        })
        .map_err(|_| {
            session.finish(
                &app,
                Some("Não foi possível iniciar o serviço de voz.".into()),
            );
            "Não foi possível iniciar o serviço de voz.".to_owned()
        })?;
    Ok(view)
}

#[tauri::command]
pub(crate) fn control_voice_session(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
    session_id: String,
    action: String,
) -> Result<(), String> {
    let state = app.state::<VoiceState>();
    let active = state
        .active
        .lock()
        .map_err(|_| "Estado da voz indisponível.")?;
    let session = active
        .as_ref()
        .filter(|session| session.id == session_id && session.owner == window.label())
        .ok_or("A sessão de voz já foi encerrada ou pertence a outra janela.")?;
    let control = match action.as_str() {
        "end" => {
            session.stop_for_owner(Some(window.label()));
            session.update(&app, Phase::Closing, None, None, 0.);
            return Ok(());
        }
        "finish" => {
            if session.mode != "dictation" {
                return Err("Somente o ditado pode finalizar uma transcrição.".into());
            }
            session.enabled.store(false, Ordering::Release);
            Control::Finish
        }
        _ => return Err("Controle de voz inválido.".into()),
    };
    session
        .sender
        .try_send(control)
        .map_err(|_| "O serviço de voz está ocupado. Tente novamente.".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn voice_requires_explicit_activation_and_does_not_restore_an_open_microphone() {
        let home = tempfile::tempdir().unwrap();
        assert!(!config(home.path()).unwrap().enabled);
        assert_eq!(SessionView::default().phase, Phase::Idle);
    }
    #[test]
    fn native_settings_and_destinations_are_validated() {
        let mut config = Config::default();
        assert!(config.validate().is_ok());
        config.speed = f32::NAN;
        assert!(config.validate().is_err());
        assert!(valid_target("chat:0123456789abcdef0123456789abcdef"));
        assert!(!valid_target("companion:0123456789abcdef0123456789abcdef"));
        assert!(!valid_target("chat:../../another-project"));
    }
    #[test]
    fn dictation_only_starts_in_the_main_chat_and_calls_are_retired() {
        let chat = "chat:0123456789abcdef0123456789abcdef";
        let companion = "companion:0123456789abcdef0123456789abcdef";
        assert!(valid_session(chat, "dictation", "main"));
        assert!(!valid_session(companion, "dictation", "companion"));
        assert!(!valid_session(chat, "dictation", "companion"));
        assert!(!valid_session(companion, "dictation", "main"));
        assert!(!valid_session(companion, "call", "companion"));
        assert!(!valid_session(chat, "call", "main"));
        assert!(!valid_session(companion, "call", "main"));
        assert!(!valid_session(chat, "call", "companion"));
        assert!(valid_session("voice-test", "test", "main"));
        assert!(!valid_session("voice-test", "test", "companion"));
        assert!(!valid_session(chat, "test", "main"));
        assert!(!valid_session("voice-test", "dictation", "main"));
        assert!(valid_session(
            "companion-notice",
            "announcement",
            "companion"
        ));
        assert!(!valid_session("companion-notice", "announcement", "main"));
        assert!(!valid_session(companion, "announcement", "companion"));
        assert!(!recognition_required("announcement"));
        assert!(!recognition_required("test"));
        assert!(recognition_required("dictation"));
        assert!(
            announcement_text("announcement", Some("Pronto, terminei sua tarefa.".into())).is_ok()
        );
        assert!(announcement_text("announcement", Some(" ".into())).is_err());
        assert!(announcement_text("announcement", Some("x".repeat(601))).is_err());
        assert!(announcement_text("call", Some("Não pode injetar uma saudação.".into())).is_err());
    }
    #[test]
    fn destroying_the_owner_releases_capture_and_invalidates_pending_speech() {
        let (sender, _) = mpsc::sync_channel(1);
        let session = Session {
            id: "dictation".into(),
            target: "chat:0123456789abcdef0123456789abcdef".into(),
            owner: "main".into(),
            mode: "dictation".into(),
            stop: AtomicBool::new(false),
            enabled: Arc::new(AtomicBool::new(true)),
            audio_revision: AtomicU64::new(0),
            speech_revision: AtomicU64::new(0),
            sender,
        };
        assert!(!session.stop_for_owner(Some("companion")));
        assert!(session.enabled.load(Ordering::Acquire));
        assert!(!session.cancelled());
        let revision = session.speech_revision.load(Ordering::Acquire);
        assert!(session.current_speech(revision));
        assert!(session.stop_for_owner(Some("main")));
        assert!(!session.enabled.load(Ordering::Acquire));
        assert!(session.cancelled());
        assert_eq!(session.audio_revision.load(Ordering::Acquire), 1);
        assert!(!session.current_speech(revision));
    }
    #[test]
    fn prerecorded_clips_are_only_accepted_for_known_announcement_ids() {
        assert_eq!(
            announcement_clip("announcement", Some("completed-1".into())).unwrap(),
            Some("completed-1".into())
        );
        assert!(announcement_clip("announcement", Some("../../voice.wav".into())).is_err());
        assert!(announcement_clip("dictation", Some("completed-1".into())).is_err());
        assert!(announcement_clip("test", Some("completed-1".into())).is_err());
    }
    #[test]
    fn announcements_and_voice_tests_do_not_prepare_microphone_recognition() {
        for mode in ["announcement", "test"] {
            assert!(!recognition_required(mode), "{mode}");
        }
        assert!(recognition_required("dictation"));
    }
}
