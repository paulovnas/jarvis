//! Native preferences and OS services, independent of the visible conversation.
mod notifications;
#[cfg(test)]
mod tests;
pub(crate) mod unread;

use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashSet, VecDeque},
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::{mpsc, Mutex, OnceLock},
    time::Duration,
};
use tauri::{Emitter, Manager};

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SleepMode {
    #[default]
    Off,
    Active,
    Open,
}
impl SleepMode {
    fn inhibit(self, active: bool) -> bool {
        self == Self::Open || self == Self::Active && active
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
pub(crate) enum ResponseLanguage {
    #[default]
    #[serde(rename = "pt-BR")]
    PortugueseBrazil,
    #[serde(rename = "en")]
    English,
    #[serde(rename = "es")]
    Spanish,
    #[serde(rename = "fr")]
    French,
    #[serde(rename = "de")]
    German,
    #[serde(rename = "it")]
    Italian,
    #[serde(rename = "ja")]
    Japanese,
    #[serde(rename = "zh-CN")]
    ChineseSimplified,
}

impl ResponseLanguage {
    pub(crate) fn prompt_instruction(self) -> &'static str {
        match self {
            Self::PortugueseBrazil => "Use Brazilian Portuguese (pt-BR) for user-facing prose unless the user explicitly requests another language.",
            Self::English => "Use English for user-facing prose unless the user explicitly requests another language.",
            Self::Spanish => "Use Spanish for user-facing prose unless the user explicitly requests another language.",
            Self::French => "Use French for user-facing prose unless the user explicitly requests another language.",
            Self::German => "Use German for user-facing prose unless the user explicitly requests another language.",
            Self::Italian => "Use Italian for user-facing prose unless the user explicitly requests another language.",
            Self::Japanese => "Use Japanese for user-facing prose unless the user explicitly requests another language.",
            Self::ChineseSimplified => "Use Simplified Chinese for user-facing prose unless the user explicitly requests another language.",
        }
    }
}

pub(crate) const DEFAULT_ASK_USER_TIMEOUT_SECONDS: u16 = 30;
const BUNDLED_TERMINAL_FONT: &str = "JetBrains Mono";
const TERMINAL_FONT_PRIORITY: &[&str] = &[
    "MesloLGS NF",
    "MesloLGS Nerd Font Mono",
    "NotoSansM Nerd Font Mono",
    "NotoMono Nerd Font Mono",
    "JetBrainsMono Nerd Font",
    "CaskaydiaCove Nerd Font Mono",
    "Hack Nerd Font Mono",
    "FiraCode Nerd Font",
    BUNDLED_TERMINAL_FONT,
];

fn order_terminal_fonts(fonts: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut unique = BTreeMap::new();
    for font in fonts
        .into_iter()
        .chain(std::iter::once(BUNDLED_TERMINAL_FONT.to_string()))
    {
        let font = font.trim();
        if font.is_empty() || font.len() > 160 || font.chars().any(char::is_control) {
            continue;
        }
        unique
            .entry(font.to_lowercase())
            .or_insert_with(|| font.to_string());
    }
    let mut fonts: Vec<_> = unique.into_values().collect();
    fonts.sort_by(|left, right| {
        let rank = |font: &str| {
            TERMINAL_FONT_PRIORITY
                .iter()
                .position(|candidate| candidate.eq_ignore_ascii_case(font))
                .unwrap_or_else(|| {
                    TERMINAL_FONT_PRIORITY.len()
                        + usize::from(!font.to_lowercase().contains("nerd font"))
                })
        };
        rank(left)
            .cmp(&rank(right))
            .then_with(|| left.to_lowercase().cmp(&right.to_lowercase()))
    });
    fonts.truncate(256);
    fonts
}

fn available_terminal_fonts() -> &'static [String] {
    static FONTS: OnceLock<Vec<String>> = OnceLock::new();
    FONTS.get_or_init(|| {
        let mut database = fontdb::Database::new();
        database.load_system_fonts();
        order_terminal_fonts(
            database
                .faces()
                .filter(|face| face.monospaced)
                .filter_map(|face| face.families.first().map(|(family, _)| family.clone())),
        )
    })
}

fn terminal_font_error(font: Option<&str>, fonts: &[String]) -> Option<String> {
    font.filter(|font| {
        !fonts
            .iter()
            .any(|available| available.eq_ignore_ascii_case(font))
    })
    .map(|font| {
        format!(
            "A fonte '{font}' não foi encontrada no sistema. Instale-a ou escolha uma das fontes detectadas."
        )
    })
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub(crate) struct TerminalPreferences {
    /// `None` follows the account login shell. A configured value is passed
    /// directly to the PTY and is never interpreted by another shell.
    pub shell: Option<String>,
    /// Empty uses conservative shell-specific interactive/login defaults.
    pub arguments: Vec<String>,
    /// `None` lets the UI prefer installed Nerd Fonts before its bundled mono font.
    pub font_family: Option<String>,
    pub font_size: u16,
}

impl Default for TerminalPreferences {
    fn default() -> Self {
        Self {
            shell: None,
            arguments: Vec::new(),
            font_family: None,
            font_size: 13,
        }
    }
}

impl TerminalPreferences {
    fn validate(&self) -> Result<(), String> {
        if self.shell.as_ref().is_some_and(|shell| {
            let shell = shell.trim();
            shell.is_empty() || shell.len() > 4096 || shell.contains('\0')
        }) {
            return Err("Informe um executável de shell válido.".into());
        }
        if self.arguments.len() > 16
            || self.arguments.iter().any(|argument| {
                argument.is_empty() || argument.len() > 512 || argument.contains('\0')
            })
        {
            return Err("Use até 16 argumentos de shell válidos, um por linha.".into());
        }
        if self.font_family.as_ref().is_some_and(|font| {
            let font = font.trim();
            font.is_empty() || font.len() > 160 || font.chars().any(char::is_control)
        }) {
            return Err("Informe uma família de fonte válida.".into());
        }
        if !(9..=32).contains(&self.font_size) {
            return Err("O tamanho da fonte deve ficar entre 9 e 32 pixels.".into());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct Preferences {
    pub prevent_sleep: SleepMode,
    pub notifications: bool,
    pub ask_user_timeout_seconds: u16,
    pub(crate) response_language: ResponseLanguage,
    pub(crate) terminal: TerminalPreferences,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            prevent_sleep: SleepMode::Off,
            notifications: false,
            ask_user_timeout_seconds: DEFAULT_ASK_USER_TIMEOUT_SECONDS,
            response_language: ResponseLanguage::default(),
            terminal: TerminalPreferences::default(),
        }
    }
}

impl Preferences {
    pub(crate) fn validate(&self) -> Result<(), String> {
        if !(1..=3600).contains(&self.ask_user_timeout_seconds) {
            return Err("O tempo das perguntas deve ficar entre 1 e 3.600 segundos.".into());
        }
        self.terminal.validate()?;
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    preferences: Preferences,
    sleep_inhibited: bool,
    sleep_error: Option<String>,
    notification_error: Option<String>,
    available_terminal_shells: Vec<String>,
    available_terminal_fonts: Vec<String>,
    resolved_terminal_shell: Option<String>,
    terminal_error: Option<String>,
    terminal_font_error: Option<String>,
}

struct Store {
    path: PathBuf,
    preferences: Preferences,
}
impl Store {
    fn open(path: PathBuf) -> Result<Self, String> {
        let preferences = match fs::read(&path) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map_err(|_| "Não foi possível ler as preferências do sistema.".to_string())?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Preferences::default(),
            Err(_) => return Err("Não foi possível abrir as preferências do sistema.".into()),
        };
        preferences.validate()?;
        Ok(Self { path, preferences })
    }
    fn save(&mut self, preferences: Preferences) -> Result<(), String> {
        preferences.validate()?;
        let persist = || -> Result<(), Box<dyn std::error::Error>> {
            let parent = self.path.parent().ok_or("Missing preferences directory")?;
            fs::create_dir_all(parent)?;
            let mut file = tempfile::NamedTempFile::new_in(parent)?;
            file.write_all(&serde_json::to_vec_pretty(&preferences)?)?;
            file.as_file().sync_all()?;
            file.persist(&self.path)?;
            Ok(())
        };
        persist().map_err(|_| "Não foi possível salvar as preferências do sistema.".to_string())?;
        self.preferences = preferences;
        Ok(())
    }
}

pub(crate) fn ask_user_timeout_seconds(home: &Path) -> u16 {
    Store::open(home.join(".jarvis/system.json"))
        .map(|store| store.preferences.ask_user_timeout_seconds)
        .unwrap_or(DEFAULT_ASK_USER_TIMEOUT_SECONDS)
}

pub(crate) fn response_language(home: &Path) -> ResponseLanguage {
    Store::open(home.join(".jarvis/system.json"))
        .map(|store| store.preferences.response_language)
        .unwrap_or_default()
}

#[derive(Default)]
struct Recent {
    keys: HashSet<String>,
    order: VecDeque<String>,
}
impl Recent {
    fn insert(&mut self, key: String) -> bool {
        if !self.keys.insert(key.clone()) {
            return false;
        }
        self.order.push_back(key);
        if self.order.len() > 512 {
            if let Some(old) = self.order.pop_front() {
                self.keys.remove(&old);
            }
        }
        true
    }
}

struct Power<T> {
    lease: Option<T>,
    error: Option<String>,
    retry: std::time::Instant,
}
impl<T> Default for Power<T> {
    fn default() -> Self {
        Self {
            lease: None,
            error: None,
            retry: std::time::Instant::now(),
        }
    }
}
impl<T> Power<T> {
    fn reconcile(
        &mut self,
        desired: bool,
        now: std::time::Instant,
        acquire: impl FnOnce() -> Result<T, String>,
    ) {
        if !desired {
            self.lease = None;
            self.error = None;
            self.retry = now;
        } else if self.lease.is_none() && now >= self.retry {
            match acquire() {
                Ok(lease) => {
                    self.lease = Some(lease);
                    self.error = None;
                }
                Err(error) => {
                    self.error = Some(error);
                    self.retry = now + Duration::from_secs(30);
                }
            }
        }
    }
    fn status(&self) -> (bool, Option<String>) {
        (self.lease.is_some(), self.error.clone())
    }
}

#[derive(Default)]
pub struct SystemState {
    unread: unread::UnreadState,
    store: Mutex<Option<Result<Store, String>>>,
    sleep: Mutex<(bool, Option<String>)>,
    notification_error: Mutex<Option<String>>,
    recent: Mutex<Recent>,
    worker: Mutex<Option<(mpsc::Sender<bool>, std::thread::JoinHandle<()>)>>,
    edit: tokio::sync::Mutex<()>,
}
impl SystemState {
    fn preferences(&self) -> Result<Preferences, String> {
        let store = self
            .store
            .lock()
            .map_err(|_| "Preferências indisponíveis.")?;
        Ok(store
            .as_ref()
            .ok_or("Preferências ainda não carregadas.")?
            .as_ref()
            .map_err(Clone::clone)?
            .preferences
            .clone())
    }
    fn snapshot(&self) -> Result<Snapshot, String> {
        let preferences = self.preferences()?;
        let available_terminal_fonts = available_terminal_fonts().to_vec();
        let terminal_font_error = terminal_font_error(
            preferences.terminal.font_family.as_deref(),
            &available_terminal_fonts,
        );
        let (resolved_terminal_shell, terminal_error) =
            match crate::agent::shell::interactive_shell(&preferences.terminal) {
                Ok(path) => (Some(path.to_string_lossy().into_owned()), None),
                Err(error) => (None, Some(error)),
            };
        let sleep = self
            .sleep
            .lock()
            .map_err(|_| "Estado de repouso indisponível.")?;
        Ok(Snapshot {
            preferences,
            sleep_inhibited: sleep.0,
            sleep_error: sleep.1.clone(),
            notification_error: self
                .notification_error
                .lock()
                .map_err(|_| "Notificações indisponíveis.")?
                .clone(),
            available_terminal_shells: crate::agent::shell::interactive_shells(),
            available_terminal_fonts,
            resolved_terminal_shell,
            terminal_error,
            terminal_font_error,
        })
    }
    fn changed(&self, app: &tauri::AppHandle) {
        if let Ok(snapshot) = self.snapshot() {
            let _ = app.emit("system:changed", snapshot);
        }
    }
    fn notification_result(&self, app: &tauri::AppHandle, result: &Result<(), String>) {
        if let Ok(mut error) = self.notification_error.lock() {
            *error = result.as_ref().err().cloned();
        }
        self.changed(app);
    }
    pub fn shutdown(&self) {
        if let Ok(mut worker) = self.worker.lock() {
            if let Some((stop, join)) = worker.take() {
                let _ = stop.send(true);
                let _ = join.join();
            }
        }
    }

    pub(crate) fn reload_from_disk(
        &self,
        app: &tauri::AppHandle,
        home: &Path,
    ) -> Result<(), String> {
        let store = Store::open(home.join(".jarvis/system.json"))?;
        let terminal = store.preferences.terminal.clone();
        *self
            .store
            .lock()
            .map_err(|_| "Preferências indisponíveis.")? = Some(Ok(store));
        app.state::<crate::agent::AgentState>()
            .terminals
            .set_preferences(terminal);
        if let Ok(worker) = self.worker.lock() {
            if let Some((wake, _)) = worker.as_ref() {
                let _ = wake.send(false);
            }
        }
        self.changed(app);
        Ok(())
    }
}

pub(crate) fn backup_preferences(home: &Path) -> Result<Preferences, String> {
    Store::open(home.join(".jarvis/system.json")).map(|store| store.preferences)
}

// The assertion is created and dropped on one dedicated thread (required on Windows).
pub fn setup(app: &tauri::AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let state = app.state::<SystemState>();
    let store = Store::open(app.path().home_dir()?.join(".jarvis/system.json"));
    if let Ok(store) = &store {
        app.state::<crate::agent::AgentState>()
            .terminals
            .set_preferences(store.preferences.terminal.clone());
    }
    *state
        .store
        .lock()
        .map_err(|_| "System preferences lock poisoned")? = Some(store);
    if let Err(error) = notifications::setup(app) {
        *state
            .notification_error
            .lock()
            .map_err(|_| "Notification state lock poisoned")? = Some(error);
    }
    unread::setup(app);
    let (send, receive) = mpsc::channel();
    let handle = app.clone();
    let worker = std::thread::Builder::new()
        .name("jarvis-power".into())
        .spawn(move || {
            let state = handle.state::<SystemState>();
            let mut power = Power::default();
            loop {
                let active = handle
                    .state::<crate::agent::AgentState>()
                    .has_active_chats();
                let desired = state
                    .preferences()
                    .is_ok_and(|p| p.prevent_sleep.inhibit(active));
                let previous = state.sleep.lock().ok().map(|status| status.clone());
                power.reconcile(desired, std::time::Instant::now(), || {
                    keepawake::Builder::default()
                        .idle(true)
                        .app_name("Jarvis")
                        .app_reverse_domain("com.foxtag.jarvis")
                        .reason("Jarvis: impedir repouso durante o trabalho")
                        .create()
                        .map_err(|_| "Não foi possível impedir o repouso neste sistema.".into())
                });
                let status = power.status();
                if previous.as_ref() != Some(&status) {
                    if let Ok(mut current) = state.sleep.lock() {
                        *current = status;
                    }
                    state.changed(&handle);
                }
                match receive.recv_timeout(Duration::from_millis(500)) {
                    Ok(true) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    _ => {}
                }
            }
            drop(power);
        })?;
    *state
        .worker
        .lock()
        .map_err(|_| "System worker lock poisoned")? = Some((send, worker));
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Notice {
    Completed,
    Question,
    Validation,
    Failed,
}
impl Notice {
    fn title(self) -> &'static str {
        match self {
            Self::Completed => "Trabalho concluído",
            Self::Question => "Aguardando sua resposta",
            Self::Validation => "Validação disponível",
            Self::Failed => "Conversa interrompida por erro",
        }
    }
}

pub(crate) fn notify(
    app: &tauri::AppHandle,
    conversation_id: &str,
    event_id: &str,
    notice: Notice,
) {
    let system = app.state::<SystemState>();
    // OS banners and in-app unread marks share the event source, not preferences.
    let event_key = format!("{conversation_id}/{event_id}/{notice:?}");
    let fresh = system
        .recent
        .lock()
        .is_ok_and(|mut recent| recent.insert(event_key.clone()));
    if !fresh {
        return;
    }
    let app = app.clone();
    let id = conversation_id.to_owned();
    tauri::async_runtime::spawn(async move {
        if let Err(error) = unread::notify(&app, id.clone(), event_key).await {
            let _ = app.emit("unread:error", error);
        }
        let lookup_app = app.clone();
        let body = tauri::async_runtime::spawn_blocking(move || {
            let home = lookup_app.path().home_dir().ok()?;
            crate::library::notification_names(
                &lookup_app.state::<crate::persistence::AppState>(),
                &home,
                &id,
            )
            .ok()
        })
        .await
        .ok()
        .flatten();
        // Deleted conversations cannot generate stale notifications.
        let Some((project, chat)) = body else {
            return;
        };
        let system = app.state::<SystemState>();
        if !system.preferences().is_ok_and(|p| p.notifications) {
            return;
        }
        let result = notifications::show(notice.title(), &format!("{project} · {chat}")).await;
        system.notification_result(&app, &result);
    });
}

pub(crate) fn notifications_enabled(app: &tauri::AppHandle) -> bool {
    app.state::<SystemState>()
        .preferences()
        .is_ok_and(|preferences| preferences.notifications)
}

pub(crate) fn notify_usage_limit(app: &tauri::AppHandle, title: &str, body: &str) {
    if !notifications_enabled(app) {
        return;
    }
    let app = app.clone();
    let title = title.to_owned();
    let body = body.to_owned();
    tauri::async_runtime::spawn(async move {
        let system = app.state::<SystemState>();
        let result = notifications::show(&title, &body).await;
        system.notification_result(&app, &result);
    });
}

#[tauri::command]
pub fn get_system_preferences(state: tauri::State<'_, SystemState>) -> Result<Snapshot, String> {
    state.snapshot()
}

#[tauri::command]
pub async fn save_system_preferences(
    app: tauri::AppHandle,
    state: tauri::State<'_, SystemState>,
    preferences: Preferences,
) -> Result<Snapshot, String> {
    let _edit = state.edit.lock().await;
    preferences.validate()?;
    // Fail a stale or misspelled custom executable before persisting it. Any
    // already-running PTY remains alive because only future spawns read this value.
    if preferences.terminal.shell != state.preferences()?.terminal.shell {
        crate::agent::shell::interactive_shell(&preferences.terminal)?;
    }
    if preferences.notifications && !state.preferences()?.notifications {
        let result = notifications::authorize().await;
        state.notification_result(&app, &result);
        result?;
    }
    {
        let mut store = state
            .store
            .lock()
            .map_err(|_| "Preferências indisponíveis.")?;
        store
            .as_mut()
            .ok_or("Preferências ainda não carregadas.")?
            .as_mut()
            .map_err(|error| error.clone())?
            .save(preferences.clone())?;
    }
    app.state::<crate::agent::AgentState>()
        .terminals
        .set_preferences(preferences.terminal);
    if let Ok(worker) = state.worker.lock() {
        if let Some((wake, _)) = worker.as_ref() {
            let _ = wake.send(false);
        }
    }
    state.changed(&app);
    let _ = unread::refresh(&app).await;
    state.snapshot()
}

#[tauri::command]
pub async fn test_system_notification(
    app: tauri::AppHandle,
    state: tauri::State<'_, SystemState>,
) -> Result<(), String> {
    if !state.preferences()?.notifications {
        return Err("Ative as notificações para testar.".into());
    }
    let result = async {
        notifications::authorize().await?;
        notifications::show(
            "Jarvis · Notificação de teste",
            "Você receberá avisos de conclusão, perguntas e erros das suas conversas.",
        )
        .await
    }
    .await;
    state.notification_result(&app, &result);
    result
}
