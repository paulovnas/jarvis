//! File-backed desktop preferences, independent from projects and credentials.
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
use tauri::{Manager, PhysicalPosition, PhysicalSize};

const PANEL_IDS: [&str; 3] = [
    "home-sidebar-panel",
    "home-main-panel",
    "home-inspector-panel",
];

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct LayoutPreferences {
    pub panels: BTreeMap<String, f64>,
    pub inspector_tab: InspectorTab,
    pub settings_tab: SettingsTab,
    pub expanded_projects: BTreeMap<String, bool>,
    pub activity_sections: BTreeMap<String, bool>,
    pub sidebar_collapsed: bool,
    pub inspector_collapsed: bool,
    pub terminal_panels: BTreeMap<String, TerminalPanelPreferences>,
    pub file_tabs: BTreeMap<String, FileTabsPreferences>,
    pub item_order: BTreeMap<String, Vec<String>>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct FileTabsPreferences {
    pub paths: Vec<String>,
    pub active_path: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct TerminalPanelPreferences {
    pub open: bool,
    pub size: f64,
    pub active_terminal_id: Option<String>,
}

impl Default for TerminalPanelPreferences {
    fn default() -> Self {
        Self {
            open: false,
            size: 40.0,
            active_terminal_id: None,
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum InspectorTab {
    Details,
    #[default]
    Activities,
    Explorer,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum SettingsTab {
    Tools,
    #[default]
    General,
    Terminal,
    Providers,
    Agents,
    Skills,
    Mcps,
    Workspaces,
}

impl LayoutPreferences {
    fn validate(&self) -> Result<(), String> {
        if !self.panels.is_empty()
            && (self.panels.len() != 3
                || PANEL_IDS.iter().any(|id| {
                    self.panels
                        .get(*id)
                        .is_none_or(|n| !n.is_finite() || *n <= 0.0 || *n >= 100.0)
                })
                || (self.panels.values().sum::<f64>() - 100.0).abs() > 0.1)
        {
            return Err("Invalid panel dimensions".into());
        }
        if self.expanded_projects.len() > 10_000
            || self.item_order.len() > 10_000
            || self.activity_sections.len() > 20
            || self.terminal_panels.len() > 10_000
            || self.file_tabs.len() > 10_000
        {
            return Err("Too many layout entries".into());
        }
        if self.item_order.iter().any(|(key, ids)| {
            key.len() > 256
                || ids.len() > 10_000
                || ids.iter().any(|id| id.is_empty() || id.len() > 8192)
                || ids.iter().collect::<std::collections::BTreeSet<_>>().len() != ids.len()
        }) {
            return Err("Invalid item order".into());
        }
        if self
            .terminal_panels
            .values()
            .any(|panel| !panel.size.is_finite() || !(20.0..=65.0).contains(&panel.size))
        {
            return Err("Invalid terminal panel dimensions".into());
        }
        if self.file_tabs.values().any(|tabs| {
            tabs.paths.len() > 30
                || tabs
                    .paths
                    .iter()
                    .any(|path| path.is_empty() || path.len() > 4096)
                || tabs
                    .active_path
                    .as_ref()
                    .is_some_and(|path| !tabs.paths.contains(path))
        }) {
            return Err("Invalid file tabs".into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod ordering_tests {
    use super::*;
    #[test]
    fn item_order_and_workspace_settings_round_trip_and_reject_duplicates() {
        let mut layout: LayoutPreferences = serde_json::from_value(serde_json::json!({
            "settingsTab": "workspaces", "itemOrder": {"projects:w": ["b", "a"], "chats:p": ["c", "d"], "tabs:c": ["browser:web", "file:README.md"]}
        })).unwrap();
        assert!(layout.validate().is_ok());
        let restored: LayoutPreferences =
            serde_json::from_slice(&serde_json::to_vec(&layout).unwrap()).unwrap();
        assert_eq!(restored, layout);
        layout
            .item_order
            .insert("projects:w".into(), vec!["a".into(), "a".into()]);
        assert!(layout.validate().is_err());
        let legacy: LayoutPreferences = serde_json::from_str("{}").unwrap();
        assert!(legacy.item_order.is_empty());
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
struct Bounds {
    x: i32,
    y: i32,
    width: f64,
    height: f64,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
struct WindowPreferences {
    normal: Option<Bounds>,
    maximized: bool,
    fullscreen: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", default)]
struct Preferences {
    version: u32,
    layout: LayoutPreferences,
    window: WindowPreferences,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            version: 1,
            layout: LayoutPreferences::default(),
            window: WindowPreferences::default(),
        }
    }
}

struct Store {
    path: PathBuf,
    preferences: Preferences,
}

impl Store {
    fn open(path: PathBuf) -> Result<Self, String> {
        let preferences: Preferences = match fs::read(&path) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map_err(|e| format!("Invalid desktop preferences: {e}"))?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Preferences::default(),
            Err(e) => return Err(e.to_string()),
        };
        if preferences.version != 1 {
            return Err("Unsupported desktop preferences version".into());
        }
        preferences.layout.validate()?;
        Ok(Self { path, preferences })
    }

    fn save(&self) -> Result<(), String> {
        let write = || -> Result<(), Box<dyn std::error::Error>> {
            let parent = self.path.parent().ok_or("Missing preferences directory")?;
            fs::create_dir_all(parent)?;
            let mut file = tempfile::NamedTempFile::new_in(parent)?;
            file.write_all(&serde_json::to_vec_pretty(&self.preferences)?)?;
            file.as_file().sync_all()?;
            file.persist(&self.path)?;
            #[cfg(unix)]
            fs::File::open(parent)?.sync_all()?;
            Ok(())
        };
        write().map_err(|e| format!("Could not save desktop preferences: {e}"))
    }
}

#[derive(Clone, Default)]
pub struct DesktopState {
    store: Arc<Mutex<Option<Store>>>,
    revision: Arc<AtomicU64>,
}

#[derive(Clone, Copy)]
struct Screen {
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    scale: f64,
}

// Keep the complete window reachable after a display is removed or its scale changes.
fn fit_bounds(
    bounds: Bounds,
    screens: &[Screen],
) -> Option<(PhysicalPosition<i32>, PhysicalSize<u32>)> {
    let screen = screens
        .iter()
        .find(|s| {
            i64::from(bounds.x) >= i64::from(s.x)
                && i64::from(bounds.x) < i64::from(s.x) + i64::from(s.width)
                && i64::from(bounds.y) >= i64::from(s.y)
                && i64::from(bounds.y) < i64::from(s.y) + i64::from(s.height)
        })
        .or_else(|| screens.first())?;
    let width = (if bounds.width.is_finite() {
        bounds.width.clamp(1024.0, 10000.0)
    } else {
        1360.0
    } * screen.scale) as u32;
    let height = (if bounds.height.is_finite() {
        bounds.height.clamp(480.0, 10000.0)
    } else {
        768.0
    } * screen.scale) as u32;
    let width = width.min(screen.width);
    let height = height.min(screen.height);
    let x = i64::from(bounds.x).clamp(
        i64::from(screen.x),
        i64::from(screen.x) + i64::from(screen.width - width),
    ) as i32;
    let y = i64::from(bounds.y).clamp(
        i64::from(screen.y),
        i64::from(screen.y) + i64::from(screen.height - height),
    ) as i32;
    Some((
        PhysicalPosition::new(x, y),
        PhysicalSize::new(width, height),
    ))
}

pub fn setup(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(target_os = "macos")]
    crate::app_menu::install(app)?;
    let state = app.state::<DesktopState>();
    let path = app.path().home_dir()?.join(".jarvis/desktop.json");
    let window = app.get_window("main").ok_or("Missing main window")?;
    match Store::open(path) {
        Ok(store) => {
            let saved = &store.preferences.window;
            if let Some(bounds) = saved.normal {
                let mut monitors = window.available_monitors()?;
                if let Some(primary) = window.primary_monitor()? {
                    monitors.sort_by_key(|m| m.position() != primary.position());
                }
                let screens: Vec<_> = monitors
                    .iter()
                    .map(|m| {
                        let area = m.work_area();
                        Screen {
                            x: area.position.x,
                            y: area.position.y,
                            width: area.size.width,
                            height: area.size.height,
                            scale: m.scale_factor(),
                        }
                    })
                    .collect();
                if let Some((position, size)) = fit_bounds(bounds, &screens) {
                    window.set_size(size)?;
                    window.set_position(position)?;
                }
            }
            if saved.maximized {
                window.maximize()?;
            }
            if saved.fullscreen {
                window.set_fullscreen(true)?;
            }
            *state.store.lock().map_err(|_| "Desktop lock poisoned")? = Some(store);
        }
        // Preserve an unreadable/newer file. The UI can still open, and reports save failures.
        Err(error) => eprintln!("Desktop restoration unavailable: {error}"),
    }
    window.show()?;
    crate::updater::relaunch::signal_ready(app.handle());
    Ok(())
}

fn capture(window: &tauri::Window, state: &DesktopState) -> Result<(), String> {
    if window.is_minimized().map_err(|e| e.to_string())? {
        return Ok(());
    }
    let maximized = window.is_maximized().map_err(|e| e.to_string())?;
    let fullscreen = window.is_fullscreen().map_err(|e| e.to_string())?;
    let normal = if !maximized && !fullscreen {
        let pos = window.outer_position().map_err(|e| e.to_string())?;
        let size = window.inner_size().map_err(|e| e.to_string())?;
        let scale = window.scale_factor().map_err(|e| e.to_string())?;
        Some(Bounds {
            x: pos.x,
            y: pos.y,
            width: f64::from(size.width) / scale,
            height: f64::from(size.height) / scale,
        })
    } else {
        None
    };
    let mut guard = state.store.lock().map_err(|_| "Desktop lock poisoned")?;
    if let Some(store) = guard.as_mut() {
        let old = store.preferences.window.clone();
        let saved = &mut store.preferences.window;
        if let Some(normal) = normal {
            saved.normal = Some(normal);
        }
        saved.maximized = maximized;
        saved.fullscreen = fullscreen;
        if *saved != old {
            if let Err(error) = store.save() {
                store.preferences.window = old;
                return Err(error);
            }
        }
    }
    Ok(())
}

pub fn on_window_event(window: &tauri::Window, event: &tauri::WindowEvent) {
    if window.label() != "main" {
        return;
    }
    let state = window.state::<DesktopState>().inner().clone();
    match event {
        tauri::WindowEvent::CloseRequested { .. } => {
            state.revision.fetch_add(1, Ordering::SeqCst);
            if let Err(e) = capture(window, &state) {
                eprintln!("{e}");
            }
        }
        tauri::WindowEvent::Moved(_)
        | tauri::WindowEvent::Resized(_)
        | tauri::WindowEvent::ScaleFactorChanged { .. } => {
            let revision = state.revision.fetch_add(1, Ordering::SeqCst) + 1;
            let window = window.clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(Duration::from_millis(250)).await;
                if state.revision.load(Ordering::SeqCst) == revision {
                    if let Err(e) = capture(&window, &state) {
                        eprintln!("{e}");
                    }
                }
            });
        }
        _ => {}
    }
}

pub fn flush(app: &tauri::AppHandle) {
    if let Some(window) = app.get_window("main") {
        let state = app.state::<DesktopState>();
        state.revision.fetch_add(1, Ordering::SeqCst);
        if let Err(e) = capture(&window, &state) {
            eprintln!("{e}");
        }
    }
}

#[tauri::command]
pub fn get_desktop_layout(
    state: tauri::State<'_, DesktopState>,
) -> Result<LayoutPreferences, String> {
    let guard = state.store.lock().map_err(|_| "Desktop lock poisoned")?;
    guard
        .as_ref()
        .map(|s| s.preferences.layout.clone())
        .ok_or("Desktop preferences unavailable".into())
}

#[tauri::command]
pub async fn save_desktop_layout(
    state: tauri::State<'_, DesktopState>,
    layout: LayoutPreferences,
) -> Result<(), String> {
    layout.validate()?;
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut guard = state.store.lock().map_err(|_| "Desktop lock poisoned")?;
        let store = guard.as_mut().ok_or("Desktop preferences unavailable")?;
        let previous = std::mem::replace(&mut store.preferences.layout, layout);
        if let Err(error) = store.save() {
            store.preferences.layout = previous;
            return Err(error);
        }
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_and_window_survive_atomic_round_trip() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join(".jarvis/desktop.json");
        let mut store = Store::open(path.clone()).unwrap();
        store.preferences.window = WindowPreferences {
            normal: Some(Bounds {
                x: -1600,
                y: 40,
                width: 1280.0,
                height: 720.0,
            }),
            maximized: true,
            fullscreen: false,
        };
        store.preferences.layout.inspector_tab = InspectorTab::Explorer;
        store.preferences.layout.settings_tab = SettingsTab::Tools;
        store.preferences.layout.sidebar_collapsed = true;
        store.preferences.layout.inspector_collapsed = true;
        store.preferences.layout.terminal_panels.insert(
            "chat-1".into(),
            TerminalPanelPreferences {
                open: true,
                size: 57.0,
                active_terminal_id: Some("terminal-2".into()),
            },
        );
        store.preferences.layout.terminal_panels.insert(
            "chat-2".into(),
            TerminalPanelPreferences {
                open: false,
                size: 25.0,
                active_terminal_id: None,
            },
        );
        store
            .preferences
            .layout
            .expanded_projects
            .insert("project-1".into(), false);
        store.preferences.layout.file_tabs.insert(
            "project-1".into(),
            FileTabsPreferences {
                paths: vec!["src/ação.ts".into(), "README.md".into()],
                active_path: Some("src/ação.ts".into()),
            },
        );
        store.save().unwrap();
        let restored = Store::open(path).unwrap();
        assert_eq!(restored.preferences.window, store.preferences.window);
        assert_eq!(restored.preferences.layout, store.preferences.layout);
    }

    #[test]
    fn missing_preferences_use_defaults_but_invalid_files_are_preserved() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("desktop.json");
        assert_eq!(
            Store::open(path.clone()).unwrap().preferences.layout,
            LayoutPreferences::default()
        );
        for contents in ["invalid", "{\"version\":2}"] {
            fs::write(&path, contents).unwrap();
            assert!(Store::open(path.clone()).is_err());
            assert_eq!(fs::read_to_string(&path).unwrap(), contents);
        }
    }

    #[test]
    fn validates_panel_proportions() {
        let mut layout = LayoutPreferences {
            panels: PANEL_IDS
                .into_iter()
                .zip([20.0, 55.0, 25.0])
                .map(|(k, v)| (k.into(), v))
                .collect(),
            ..LayoutPreferences::default()
        };
        assert!(layout.validate().is_ok());
        layout.panels.insert(PANEL_IDS[0].into(), f64::NAN);
        assert!(layout.validate().is_err());
        layout.panels.insert(PANEL_IDS[0].into(), 80.0);
        assert!(layout.validate().is_err());
    }

    #[test]
    fn old_layouts_remain_readable_and_terminal_dimensions_are_validated() {
        let mut layout: LayoutPreferences =
            serde_json::from_str(r#"{"sidebarCollapsed":true}"#).unwrap();
        assert!(layout.sidebar_collapsed);
        assert!(layout.terminal_panels.is_empty());
        assert!(layout.file_tabs.is_empty());
        for size in [20.0, 40.0, 65.0] {
            layout.terminal_panels.insert(
                "chat".into(),
                TerminalPanelPreferences {
                    size,
                    ..Default::default()
                },
            );
            assert!(layout.validate().is_ok());
        }
        for size in [0.0, 19.0, 66.0, f64::NAN, f64::INFINITY] {
            layout.terminal_panels.insert(
                "chat".into(),
                TerminalPanelPreferences {
                    size,
                    ..Default::default()
                },
            );
            assert!(layout.validate().is_err());
        }
    }

    #[test]
    fn file_tabs_require_an_existing_active_tab_and_bounded_paths() {
        let mut layout = LayoutPreferences::default();
        let tabs = FileTabsPreferences {
            paths: vec!["src/main.ts".into()],
            active_path: None,
        };
        layout.file_tabs.insert("project".into(), tabs.clone());
        assert!(layout.validate().is_ok());
        for invalid in [
            FileTabsPreferences {
                active_path: Some("missing.ts".into()),
                ..tabs.clone()
            },
            FileTabsPreferences {
                paths: vec!["".into()],
                ..tabs.clone()
            },
            FileTabsPreferences {
                paths: vec!["a".repeat(4097)],
                ..tabs.clone()
            },
            FileTabsPreferences {
                paths: (0..31).map(|i| format!("file-{i}")).collect(),
                ..tabs
            },
        ] {
            layout.file_tabs.insert("project".into(), invalid);
            assert!(layout.validate().is_err());
        }
    }

    #[test]
    fn unplugged_display_and_bad_dimensions_are_fitted_to_available_screen() {
        let screen = Screen {
            x: 0,
            y: 50,
            width: 2880,
            height: 1750,
            scale: 2.0,
        };
        let (position, size) = fit_bounds(
            Bounds {
                x: -4000,
                y: 3000,
                width: 9000.0,
                height: -1.0,
            },
            &[screen],
        )
        .unwrap();
        assert_eq!(position, PhysicalPosition::new(0, 840));
        assert_eq!(size, PhysicalSize::new(2880, 960));
    }

    #[test]
    fn retains_negative_display_coordinates_and_logical_size() {
        let screen = Screen {
            x: -1920,
            y: 0,
            width: 1920,
            height: 1080,
            scale: 1.0,
        };
        let (position, size) = fit_bounds(
            Bounds {
                x: -1800,
                y: 60,
                width: 1280.0,
                height: 720.0,
            },
            &[screen],
        )
        .unwrap();
        assert_eq!(position, PhysicalPosition::new(-1800, 60));
        assert_eq!(size, PhysicalSize::new(1280, 720));
    }
}
