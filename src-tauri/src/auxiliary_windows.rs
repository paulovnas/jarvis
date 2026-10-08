//! Dedicated local settings and about windows. Creation is serialized by the
//! native event loop, so repeated requests reuse a single window per surface.
use serde::Deserialize;
#[cfg(target_os = "macos")]
use tauri::Emitter;
use tauri::Manager;

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub(crate) enum AuxiliaryWindowKind {
    Settings,
    About,
}

#[derive(Clone, Copy, Debug)]
struct WindowSpec {
    label: &'static str,
    title: &'static str,
    width: f64,
    height: f64,
    min_width: f64,
    min_height: f64,
}

impl AuxiliaryWindowKind {
    fn spec(self) -> WindowSpec {
        match self {
            Self::Settings => WindowSpec {
                label: "settings",
                title: "Configurações · Jarvis",
                width: 1400.0,
                height: 900.0,
                min_width: 960.0,
                min_height: 640.0,
            },
            Self::About => WindowSpec {
                label: "about",
                title: "Sobre o Jarvis",
                width: 680.0,
                height: 760.0,
                min_width: 520.0,
                min_height: 600.0,
            },
        }
    }
}

// This supplements Tauri's plugin capabilities. Application IPC is registered
// centrally and otherwise available to every local webview by default.
pub(crate) fn command_allowed(label: &str, command: &str) -> bool {
    match label {
        "settings" => matches!(
            command,
            "open_auxiliary_window"
                | "get_app_config"
                | "get_system_preferences"
                | "save_system_preferences"
                | "test_system_notification"
                | "list_provider_accounts"
                | "begin_openai_codex_connection"
                | "reauthorize_provider_account"
                | "wait_openai_codex_connection"
                | "cancel_openai_codex_connection"
                | "disconnect_provider_account"
                | "set_provider_enabled"
                | "set_provider_model_enabled"
                | "refresh_provider_models"
                | "set_provider_usage_visibility"
                | "set_provider_usage_alert"
                | "get_provider_usage"
                | "get_provider_model_references"
                | "get_provider_removal_plan"
                | "save_custom_provider"
                | "save_opencode_go_provider"
                | "lookup_custom_model"
                | "get_opencode_go_free_models"
                | "get_claude_runtime"
                | "refresh_claude_runtime"
                | "get_claude_usage"
                | "save_claude_provider_preferences"
                | "get_agent_models"
                | "set_agent_model"
                | "get_agent_instructions"
                | "get_agent_tool_permissions"
                | "get_workflow_catalog"
                | "mutate_workflow_catalog"
                | "get_library_snapshot"
                | "get_workspace_storage"
                | "delete_library_item"
                | "get_core_status"
                | "check_core_updates"
                | "install_core_component"
                | "cancel_core_installation"
                | "diagnose_core"
                | "repair_core_component"
                | "configure_context7"
                | "get_openmontage_configuration"
                | "save_openmontage_configuration"
                | "install_openmontage_optional_package"
                | "list_mcp_servers"
                | "get_mcp_config"
                | "save_mcp_server"
                | "set_mcp_enabled"
                | "delete_mcp_server"
                | "test_mcp_server"
                | "list_hooks"
                | "save_hook"
                | "delete_hook"
                | "list_plugins"
                | "preview_plugin_change"
                | "apply_plugin_change"
                | "cancel_plugin_change"
                | "start_mcp_oauth"
                | "wait_mcp_oauth"
                | "cancel_mcp_oauth"
                | "mcp_oauth_status"
                | "disconnect_mcp_oauth"
                | "plugin_mcp_requirements"
                | "configure_plugin_mcp"
                | "list_skills"
                | "check_skill_updates"
                | "update_skills"
                | "set_skills_agents"
                | "set_skill_enabled"
                | "delete_skill"
                | "get_skill_detail"
                | "browse_skill_marketplace"
                | "get_marketplace_skill"
                | "install_marketplace_skill"
                | "get_skill_cache_status"
                | "clear_skill_cache"
                | "preview_chat_cleanup"
                | "cleanup_old_chats"
                | "get_journal_maintenance_status"
                | "optimize_journals"
                | "export_settings_backup"
                | "inspect_settings_backup"
                | "import_settings_backup"
                | "get_web_search_config"
                | "set_web_search_config"
                | "get_vision_config"
                | "set_vision_config"
                | "get_image_generation_config"
                | "set_image_generation_config"
                | "get_browser_extension_status"
                | "prepare_browser_extension"
                | "open_browser_extension_directory"
                | "revoke_browser_extension"
                | "open_browser_application"
                | "get_voice_settings"
                | "save_voice_settings"
                | "install_voice_model"
                | "cancel_voice_download"
                | "start_voice_session"
                | "control_voice_session"
                | "get_remote_status"
                | "set_remote_enabled"
                | "refresh_remote_pairing"
                | "revoke_remote_device"
        ),
        "about" => matches!(
            command,
            "open_auxiliary_window"
                | "get_system_preferences"
                | "check_app_update"
                | "get_app_shutdown_status"
                | "install_app_update"
        ),
        _ => false,
    }
}

fn local_navigation(url: &tauri::Url, development: Option<&tauri::Url>) -> bool {
    // Tauri simplifies App("index.html") to tauri://localhost on macOS,
    // whose custom-scheme URL has an empty path rather than a trailing slash.
    if !matches!(url.path(), "" | "/" | "/index.html")
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return false;
    }
    (url.scheme() == "tauri" && url.host_str() == Some("localhost") && url.port().is_none())
        || (matches!(url.scheme(), "http" | "https")
            && url.host_str() == Some("tauri.localhost")
            && url.port().is_none())
        || (cfg!(debug_assertions)
            && development.is_some_and(|origin| origin.origin() == url.origin()))
}

fn fitted_size(spec: WindowSpec, work_width: f64, work_height: f64) -> (f64, f64, f64, f64) {
    // Reserve space for the native title bar, borders and a margin to the work area.
    let available_width = (work_width - 32.0).max(1.0);
    let available_height = (work_height - 64.0).max(1.0);
    (
        spec.width.min(available_width),
        spec.height.min(available_height),
        spec.min_width.min(available_width),
        spec.min_height.min(available_height),
    )
}

fn create_or_focus(app: &tauri::AppHandle, kind: AuxiliaryWindowKind) -> Result<(), String> {
    let spec = kind.spec();
    if let Some(window) = app.get_webview_window(spec.label) {
        window.unminimize().map_err(|error| error.to_string())?;
        window.show().map_err(|error| error.to_string())?;
        return window.set_focus().map_err(|error| error.to_string());
    }
    let monitor = app
        .get_webview_window("main")
        .and_then(|main| main.current_monitor().ok().flatten())
        .or_else(|| app.primary_monitor().ok().flatten());
    let development_origin = app.config().build.dev_url.clone();
    let mut builder = tauri::WebviewWindowBuilder::new(
        app,
        spec.label,
        tauri::WebviewUrl::App("index.html".into()),
    )
    .on_navigation(move |url| local_navigation(url, development_origin.as_ref()))
    .on_new_window(|_, _| tauri::webview::NewWindowResponse::Deny)
    .title(spec.title)
    .decorations(true)
    .resizable(true)
    .background_color(tauri::utils::config::Color(24, 27, 32, 255))
    .visible(false)
    .inner_size(spec.width, spec.height)
    .min_inner_size(spec.min_width, spec.min_height);
    if let Some(monitor) = monitor.as_ref() {
        let area = monitor.work_area();
        let scale = monitor.scale_factor();
        let (width, height, min_width, min_height) = fitted_size(
            spec,
            f64::from(area.size.width) / scale,
            f64::from(area.size.height) / scale,
        );
        builder = builder
            .inner_size(width, height)
            .min_inner_size(min_width, min_height)
            .position(
                f64::from(area.position.x) / scale
                    + (f64::from(area.size.width) / scale - width) / 2.0,
                f64::from(area.position.y) / scale
                    + (f64::from(area.size.height) / scale - height) / 2.0,
            );
    } else {
        builder = builder.center();
    }
    let window = builder.build().map_err(|error| error.to_string())?;
    if let Some(monitor) = monitor {
        let area = monitor.work_area();
        let outer = window.outer_size().map_err(|error| error.to_string())?;
        window
            .set_position(tauri::PhysicalPosition::new(
                area.position.x + area.size.width.saturating_sub(outer.width) as i32 / 2,
                area.position.y + area.size.height.saturating_sub(outer.height) as i32 / 2,
            ))
            .map_err(|error| error.to_string())?;
    }
    window.show().map_err(|error| error.to_string())?;
    window.set_focus().map_err(|error| error.to_string())
}

#[tauri::command]
pub(crate) async fn open_auxiliary_window(
    app: tauri::AppHandle,
    kind: AuxiliaryWindowKind,
) -> Result<(), String> {
    let (sender, receiver) = tokio::sync::oneshot::channel();
    let handle = app.clone();
    app.run_on_main_thread(move || {
        let _ = sender.send(create_or_focus(&handle, kind));
    })
    .map_err(|_| "Não foi possível abrir a janela do Jarvis.".to_owned())?;
    receiver
        .await
        .map_err(|_| "Não foi possível abrir a janela do Jarvis.".to_owned())?
}

#[cfg(target_os = "macos")]
pub(crate) fn open_from_menu(app: &tauri::AppHandle, kind: AuxiliaryWindowKind) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        if let Err(error) = open_auxiliary_window(app.clone(), kind).await {
            eprintln!("Auxiliary window unavailable: {error}");
            let _ = app.emit_to("main", "app:auxiliary-error", error);
        }
    });
}

#[cfg(test)]
mod tests;
