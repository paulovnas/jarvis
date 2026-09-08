//! Native preferences and OS services, independent of the visible conversation.
mod notifications;
#[cfg(test)]
mod tests;
pub(crate) mod unread;

use serde::{Deserialize, Serialize};
use std::{
    collections::{HashSet, VecDeque},
    fs,
    io::Write,
    path::PathBuf,
    sync::{mpsc, Mutex},
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

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct Preferences {
    pub prevent_sleep: SleepMode,
    pub notifications: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    preferences: Preferences,
    sleep_inhibited: bool,
    sleep_error: Option<String>,
    notification_error: Option<String>,
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
        Ok(Self { path, preferences })
    }
    fn save(&mut self, preferences: Preferences) -> Result<(), String> {
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
}

// The assertion is created and dropped on one dedicated thread (required on Windows).
pub fn setup(app: &tauri::AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let state = app.state::<SystemState>();
    *state
        .store
        .lock()
        .map_err(|_| "System preferences lock poisoned")? = Some(Store::open(
        app.path().home_dir()?.join(".jarvis/system.json"),
    ));
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
            .save(preferences)?;
    }
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
