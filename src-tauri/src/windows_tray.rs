//! Windows notification-area menu. Refresh on demand without polling or loading journals.
use crate::{
    auxiliary_windows::{self, AuxiliaryWindowKind},
    system::CloseBehavior,
};
use std::{
    collections::HashSet,
    sync::atomic::{AtomicBool, Ordering},
};
use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Emitter, Manager,
};

const TRAY_ID: &str = "jarvis";
const CHAT_LIMIT: usize = 4;

#[cfg(feature = "browser-probe")]
pub(crate) mod probe;

#[derive(Clone, Debug, PartialEq, Eq)]
struct Chat {
    id: String,
    title: String,
    project: String,
}

#[derive(Debug, Default)]
struct Chats {
    running: Vec<Chat>,
    recent: Vec<Chat>,
}

fn read_chats(
    db: &rusqlite::Connection,
    active: &HashSet<String>,
) -> Result<Chats, crate::library::LibraryError> {
    let read = |ids: Option<String>| -> Result<Vec<Chat>, crate::library::LibraryError> {
        let mut query = db.prepare(
            "SELECT c.id, COALESCE(c.display_title,c.title), p.name
            FROM conversations c JOIN projects p ON p.id=c.project_id
            WHERE (?1 IS NULL OR c.id IN (SELECT value FROM json_each(?1)))
            ORDER BY COALESCE(c.last_activity_at,c.created_at) DESC, c.rowid DESC LIMIT 4",
        )?;
        let chats = query
            .query_map([ids], |row| {
                Ok(Chat {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    project: row.get(2)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(chats)
    };
    Ok(Chats {
        running: read(Some(serde_json::json!(active).to_string()))?,
        recent: read(None)?,
    })
}

fn short_name(value: &str, maximum: usize) -> String {
    let value: String = value
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    if value.chars().count() <= maximum {
        value
    } else {
        format!("{}...", value.chars().take(maximum - 3).collect::<String>())
    }
}

#[derive(Debug)]
enum MenuEntry {
    Item {
        id: String,
        label: String,
        enabled: bool,
    },
    Separator,
}

fn menu_entries(chats: &Chats) -> Vec<MenuEntry> {
    let item = |id: &str, label: &str, enabled| MenuEntry::Item {
        id: id.into(),
        label: label.into(),
        enabled,
    };
    let mut entries = Vec::new();
    for (section, label, chats) in [
        ("running", "Rodando", &chats.running),
        ("recent", "Recentes", &chats.recent),
    ] {
        entries.push(item(&format!("tray.{section}"), label, false));
        entries.extend(chats.iter().take(CHAT_LIMIT).map(|chat| {
            item(
                &format!("tray.{section}.{}", chat.id),
                &format!(
                    "{}\t{}",
                    short_name(&chat.title, 28),
                    short_name(&chat.project, 18)
                )
                .replace('&', "&&"),
                true,
            )
        }));
        entries.push(MenuEntry::Separator);
    }
    entries.extend([
        item("tray.open", "Abrir Jarvis", true),
        MenuEntry::Separator,
        item("tray.settings", "Configurações", true),
        item("tray.about", "Sobre", true),
        MenuEntry::Separator,
        item("tray.quit", "Encerrar", true),
    ]);
    entries
}

fn build_menu(app: &tauri::AppHandle, chats: &Chats) -> tauri::Result<Menu<tauri::DynRuntime>> {
    let menu = Menu::new(app)?;
    for entry in menu_entries(chats) {
        match entry {
            MenuEntry::Item { id, label, enabled } => {
                menu.append(&MenuItem::with_id(app, id, label, enabled, None::<&str>)?)?
            }
            MenuEntry::Separator => menu.append(&PredefinedMenuItem::separator(app)?)?,
        }
    }
    Ok(menu)
}

#[derive(Debug, PartialEq, Eq)]
enum TrayAction {
    Open,
    Settings,
    About,
    Quit,
    Chat(String),
}

impl TrayAction {
    fn from_id(id: &str) -> Option<Self> {
        match id {
            "tray.open" => Some(Self::Open),
            "tray.settings" => Some(Self::Settings),
            "tray.about" => Some(Self::About),
            "tray.quit" => Some(Self::Quit),
            _ => id
                .strip_prefix("tray.running.")
                .or_else(|| id.strip_prefix("tray.recent."))
                .filter(|id| !id.is_empty())
                .map(|id| Self::Chat(id.into())),
        }
    }
}

fn show_main(app: &tauri::AppHandle) -> Result<(), String> {
    let window = app
        .get_webview_window("main")
        .ok_or("A janela do Jarvis não está disponível.")?;
    window
        .show()
        .and_then(|_| window.unminimize())
        .and_then(|_| window.set_focus())
        .map_err(|error| error.to_string())
}

fn report(app: &tauri::AppHandle, error: impl std::fmt::Display) {
    eprintln!("Windows tray: {error}");
    let _ = app.emit_to("main", "app:auxiliary-error", error.to_string());
}

fn act(app: &tauri::AppHandle, action: TrayAction) {
    match action {
        TrayAction::Open => {
            if let Err(error) = show_main(app) {
                report(app, error);
            }
        }
        TrayAction::Settings => {
            auxiliary_windows::open_from_menu(app, AuxiliaryWindowKind::Settings)
        }
        TrayAction::About => auxiliary_windows::open_from_menu(app, AuxiliaryWindowKind::About),
        TrayAction::Quit => {
            if crate::app_exit::request(app) {
                if let Err(error) = show_main(app) {
                    report(app, error);
                }
            } else {
                app.exit(0);
            }
        }
        TrayAction::Chat(id) => {
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                if let Err(error) =
                    crate::companion::companion_open_conversation(app.clone(), id).await
                {
                    report(&app, error.message());
                }
            });
        }
    }
}

#[derive(Default)]
struct TrayState {
    opening: AtomicBool,
}

fn open_menu(app: &tauri::AppHandle) {
    if app
        .state::<TrayState>()
        .opening
        .swap(true, Ordering::AcqRel)
    {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let handle = app.clone();
        let chats = tauri::async_runtime::spawn_blocking(move || -> Result<Chats, String> {
            let active = handle
                .state::<crate::agent::AgentState>()
                .tray_running_ids()
                .map_err(|e| e.message().to_owned())?;
            let home = handle.path().home_dir().map_err(|e| e.to_string())?;
            handle
                .state::<crate::persistence::AppState>()
                .with_connection(&home, |db| read_chats(db, &active))
                .map_err(|e| crate::agent::AgentError::from(e).message().to_owned())
        })
        .await;
        let chats = match chats {
            Ok(Ok(chats)) => chats,
            error => {
                eprintln!("Windows tray conversations: {error:?}");
                report(&app, "Não foi possível listar as conversas da bandeja.");
                Chats::default()
            }
        };
        let handle = app.clone();
        if let Err(error) = app.run_on_main_thread(move || {
            let result = (|| -> tauri::Result<()> {
                if let Some(tray) = handle.tray_by_id(TRAY_ID) {
                    tray.set_menu(Some(build_menu(&handle, &chats)?))?;
                    tray.with_inner_tray_icon(|tray| tray.show_menu())?;
                }
                Ok(())
            })();
            handle
                .state::<TrayState>()
                .opening
                .store(false, Ordering::Release);
            if let Err(error) = result {
                report(&handle, error);
            }
        }) {
            app.state::<TrayState>()
                .opening
                .store(false, Ordering::Release);
            report(&app, error);
        }
    });
}

pub(crate) fn install(app: &tauri::AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    app.manage(TrayState::default());
    let image =
        image::load_from_memory(include_bytes!("../../public/v2/png/mark-32.png"))?.into_rgba8();
    let (width, height) = image.dimensions();
    let tray = TrayIconBuilder::with_id(TRAY_ID)
        .icon(tauri::image::Image::new_owned(
            image.into_raw(),
            width,
            height,
        ))
        .tooltip("Jarvis")
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| {
            if let Some(action) = TrayAction::from_id(event.id().as_ref()) {
                act(app, action);
            }
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                match button {
                    MouseButton::Right => open_menu(tray.app_handle()),
                    MouseButton::Left => act(tray.app_handle(), TrayAction::Open),
                    _ => {}
                }
            }
        })
        .build(app)?;
    // Tauri is revision-pinned. Disable the automatic popup so the first menu
    // already includes the latest runtime and library state, read off the UI thread.
    tray.with_inner_tray_icon(|tray| tray.set_show_menu_on_right_click(false))?;
    Ok(())
}

fn should_hide(behavior: CloseBehavior, prepared: bool, tray_available: bool) -> bool {
    behavior == CloseBehavior::Tray && !prepared && tray_available
}

pub(crate) fn hide_on_close(window: &tauri::Window) -> bool {
    let app = window.app_handle();
    if crate::app_exit::prepared(app) {
        return false;
    }
    let preferences = match app.state::<crate::system::SystemState>().preferences() {
        Ok(preferences) => preferences,
        Err(error) => {
            report(app, error);
            return true;
        }
    };
    if !should_hide(
        preferences.close_behavior,
        crate::app_exit::prepared(app),
        app.tray_by_id(TRAY_ID).is_some(),
    ) {
        return false;
    }
    crate::desktop::flush(app);
    if let Err(error) = window.hide() {
        report(app, error);
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn database() -> rusqlite::Connection {
        let mut db = rusqlite::Connection::open_in_memory().unwrap();
        crate::persistence::initialize_database(&mut db).unwrap();
        db.execute_batch("INSERT INTO workspaces(id,name) VALUES('workspace','Pessoal');
            INSERT INTO projects(id,workspace_id,name,path) VALUES('project','workspace','Projeto','C:/fixture');
            INSERT INTO projects(id,workspace_id,name,path) VALUES('global','workspace','Global','C:/global');").unwrap();
        for i in 0..7 {
            db.execute("INSERT INTO conversations(id,project_id,title,display_title,created_at,last_activity_at) VALUES(?1,?2,?3,?4,?5,?6)", rusqlite::params![format!("chat-{i}"), if i == 6 { "global" } else { "project" }, format!("Original {i}"), format!("Conversa {i}"), i, if i == 2 { None } else { Some(10 + i) }]).unwrap();
        }
        db
    }

    #[test]
    fn lists_four_running_roots_and_four_recent_chats_across_projects_and_global() {
        let db = database();
        let active = (0..7)
            .map(|i| format!("chat-{i}"))
            .chain(["worker-not-in-library".into()])
            .collect();
        let chats = read_chats(&db, &active).unwrap();
        assert_eq!(
            chats
                .running
                .iter()
                .map(|c| c.id.as_str())
                .collect::<Vec<_>>(),
            ["chat-6", "chat-5", "chat-4", "chat-3"]
        );
        assert_eq!(chats.recent, chats.running);
        assert_eq!(chats.recent[0].title, "Conversa 6");
        assert_eq!(chats.recent[0].project, "Global");
        let idle = read_chats(&db, &Default::default()).unwrap();
        assert!(idle.running.is_empty());
        assert_eq!(idle.recent.len(), 4);
    }

    #[test]
    fn recency_uses_activity_then_creation_and_rowid_for_ties() {
        let db = database();
        db.execute(
            "UPDATE conversations SET last_activity_at=NULL,created_at=50 WHERE id='chat-2'",
            [],
        )
        .unwrap();
        db.execute(
            "UPDATE conversations SET last_activity_at=50 WHERE id IN ('chat-0','chat-1')",
            [],
        )
        .unwrap();
        let chats = read_chats(&db, &Default::default()).unwrap();
        assert_eq!(
            chats
                .recent
                .iter()
                .take(3)
                .map(|c| c.id.as_str())
                .collect::<Vec<_>>(),
            ["chat-2", "chat-1", "chat-0"]
        );
    }

    #[test]
    fn menu_keeps_the_requested_order_and_individually_truncates_both_columns() {
        let chat = Chat {
            id: "chat".into(),
            title: "Conversa com um título bastante longo 🦊".into(),
            project: "Projeto com nome bastante longo".into(),
        };
        let menu = menu_entries(&Chats {
            running: vec![chat.clone()],
            recent: vec![chat],
        });
        let labels: Vec<_> = menu
            .iter()
            .map(|item| match item {
                MenuEntry::Item { label, .. } => label.as_str(),
                MenuEntry::Separator => "---",
            })
            .collect();
        assert_eq!(
            labels,
            [
                "Rodando",
                "Conversa com um título ba...\tProjeto com nom...",
                "---",
                "Recentes",
                "Conversa com um título ba...\tProjeto com nom...",
                "---",
                "Abrir Jarvis",
                "---",
                "Configurações",
                "Sobre",
                "---",
                "Encerrar"
            ]
        );
        assert!(matches!(&menu[0], MenuEntry::Item { enabled: false, .. }));
        assert_eq!(short_name("🦊".repeat(30).as_str(), 28).chars().count(), 28);
        assert_eq!(short_name("Curto", 28), "Curto");
        assert_eq!(short_name("Linha\ncom\ttab", 28), "Linha com tab");
    }

    #[test]
    fn menu_routes_each_action_and_escapes_native_mnemonics() {
        assert_eq!(
            TrayAction::from_id("tray.running.chat"),
            Some(TrayAction::Chat("chat".into()))
        );
        assert_eq!(
            TrayAction::from_id("tray.recent.chat"),
            Some(TrayAction::Chat("chat".into()))
        );
        assert_eq!(TrayAction::from_id("tray.open"), Some(TrayAction::Open));
        assert_eq!(
            TrayAction::from_id("tray.settings"),
            Some(TrayAction::Settings)
        );
        assert_eq!(TrayAction::from_id("tray.about"), Some(TrayAction::About));
        assert_eq!(TrayAction::from_id("tray.quit"), Some(TrayAction::Quit));
        assert_eq!(TrayAction::from_id("tray.running."), None);
        assert_eq!(TrayAction::from_id("other"), None);
        let chats = Chats {
            running: vec![],
            recent: vec![Chat {
                id: "chat".into(),
                title: "R&D".into(),
                project: "A&B".into(),
            }],
        };
        assert!(menu_entries(&chats)
            .iter()
            .any(|item| matches!(item, MenuEntry::Item { label, .. } if label == "R&&D\tA&&B")));
    }

    #[test]
    fn hiding_requires_the_preference_and_a_tray_and_never_intercepts_confirmed_exit() {
        use crate::system::CloseBehavior;
        assert!(should_hide(CloseBehavior::Tray, false, true));
        assert!(!should_hide(CloseBehavior::Exit, false, true));
        assert!(!should_hide(CloseBehavior::Tray, true, true));
        assert!(!should_hide(CloseBehavior::Tray, false, false));
    }
}
