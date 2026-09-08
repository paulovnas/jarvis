// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
mod agent;
#[cfg(target_os = "macos")]
mod app_menu;
mod background;
mod core;
mod desktop;
mod library;
mod mcp;
mod model_bindings;
mod openai_codex;
mod persistence;
mod secrets;
mod skills;
mod system;
mod updater;
#[cfg(feature = "browser-probe")]
pub fn run_browser_probe() {
    agent::browser::probe::run();
}

fn main_only(
    invoke: tauri::ipc::Invoke<tauri::Wry>,
    handler: fn(tauri::ipc::Invoke<tauri::Wry>) -> bool,
) -> bool {
    if invoke.message.webview().label() != "main" {
        invoke
            .resolver
            .reject("Application commands are unavailable in browser tabs.");
        return true;
    }
    handler(invoke)
}
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(desktop::DesktopState::default())
        .manage(core::CoreState::default())
        .manage(persistence::AppState::default())
        .manage(openai_codex::OpenAiCodexState::default())
        .manage(agent::AgentState::default())
        .manage(agent::browser::BrowserState::default())
        .manage(agent::dashboard::DashboardState::default())
        .manage(mcp::McpState::default())
        .manage(updater::UpdateState::default())
        .manage(system::SystemState::default())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app| {
            desktop::setup(app)?;
            core::health::start_monitor(app.handle());
            system::setup(app.handle())
        })
        .on_window_event(desktop::on_window_event)
        .invoke_handler(|invoke| {
            // Remote child views must never call privileged application commands,
            // even if a site navigates to a local URL matching the development origin.
            let handler: fn(tauri::ipc::Invoke<tauri::Wry>) -> bool = tauri::generate_handler![
                agent::browser::get_browser_tabs,
                agent::browser::browser_command,
                agent::browser::set_browser_viewport,
                greet,
                system::get_system_preferences,
                system::save_system_preferences,
                system::test_system_notification,
                system::unread::get_unread_conversations,
                system::unread::mark_conversation_read,
                updater::check_app_update,
                updater::install_app_update,
                core::get_core_status,
                core::check_core_updates,
                core::install_core_component,
                core::health::diagnose_core,
                core::health::repair_core_component,
                core::context7::configure_context7,
                desktop::get_desktop_layout,
                desktop::save_desktop_layout,
                skills::list_skills,
                skills::set_skills_agents,
                skills::set_skill_enabled,
                skills::delete_skill,
                skills::get_skill_detail,
                skills::browse_skill_marketplace,
                skills::get_marketplace_skill,
                skills::install_marketplace_skill,
                skills::check_skill_updates,
                skills::update_skills,
                mcp::list_mcp_servers,
                mcp::get_mcp_config,
                mcp::save_mcp_server,
                mcp::set_mcp_enabled,
                mcp::delete_mcp_server,
                mcp::runtime::test_mcp_server,
                persistence::get_app_config,
                persistence::complete_onboarding,
                agent::web_search::get_web_search_config,
                agent::vision::get_vision_config,
                agent::vision::set_vision_config,
                agent::image_generation::get_image_generation_config,
                agent::image_generation::set_image_generation_config,
                agent::attachments::import_chat_attachments,
                agent::attachments::get_chat_attachment_image,
                agent::attachments::save_chat_image,
                agent::web_search::set_web_search_config,
                library::get_library_snapshot,
                library::workspaces::move_project_workspace,
                library::workspaces::get_workspace_storage,
                agent::workflow::catalog::permissions::get_agent_tool_permissions,
                library::open_project_directory,
                library::open_conversation_path,
                library::files::list_project_directory,
                library::files::read_project_file,
                library::create_workspace,
                library::add_project,
                library::create_conversation,
                library::rename_project,
                library::rename_conversation,
                library::delete_library_item,
                library::select_library_item,
                library::get_conversation,
                agent::dashboard::get_project_metrics,
                core::beads::dashboard::get_project_beads,
                core::beads::dashboard::get_bead_detail,
                core::beads::dashboard::add_bead_comment,
                agent::get_chat,
                agent::processes::list_chat_processes,
                agent::processes::read_chat_process,
                agent::processes::stop_chat_process,
                agent::processes::remove_chat_process,
                agent::terminals::list_chat_terminals,
                agent::terminals::create_chat_terminal,
                agent::terminals::read_chat_terminal,
                agent::terminals::write_chat_terminal,
                agent::terminals::resize_chat_terminal,
                agent::terminals::rename_chat_terminal,
                agent::terminals::close_chat_terminal,
                agent::workflow::get_workflow,
                agent::workflow::catalog::get_workflow_catalog,
                agent::workflow::catalog::mutate_workflow_catalog,
                agent::workflow::validation::decide_workflow_validation,
                agent::workflow::validation::submit_workflow_validation,
                agent::workflow::settings::get_agent_models,
                agent::workflow::settings::get_agent_instructions,
                agent::workflow::settings::set_agent_model,
                agent::workflow::get_workflow_transcript,
                agent::workflow::approve_workflow_tool,
                agent::workflow::answer_workflow_question,
                agent::history::get_chat_history,
                agent::cleanup::preview_chat_cleanup,
                agent::cleanup::cleanup_old_chats,
                agent::get_agent_activity,
                agent::start_agent_turn,
                agent::resume_agent_queue,
                agent::maintenance::compact_agent_context,
                agent::queue::remove_queued_message,
                agent::diffs::get_agent_file_diff,
                agent::diffs::get_agent_file_changes,
                agent::cancel_agent_turn,
                agent::approve_agent_tool,
                agent::questions::answer_agent_question,
                openai_codex::list_provider_accounts,
                openai_codex::custom::save_custom_provider,
                openai_codex::custom::discovery::lookup_custom_model,
                openai_codex::set_provider_enabled,
                openai_codex::usage::get_provider_usage,
                openai_codex::usage::set_provider_usage_visibility,
                openai_codex::begin_openai_codex_connection,
                openai_codex::reauthorize_provider_account,
                openai_codex::wait_openai_codex_connection,
                openai_codex::cancel_openai_codex_connection,
                disconnect_provider_account,
                agent::provider_links::get_provider_removal_plan,
                agent::provider_links::get_provider_model_references,
                agent::provider_links::clear_chat_model_binding
            ];
            main_only(invoke, handler)
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            if matches!(event, tauri::RunEvent::ExitRequested { .. }) {
                prepare_exit(app);
            }
        });
}

// Windows updater installation exits directly, bypassing Tauri's ExitRequested event.
// Keep the same cleanup available to that hook and to normal application shutdown.
pub(crate) fn prepare_exit(app: &tauri::AppHandle) {
    use tauri::Manager;
    desktop::flush(app);
    shutdown_services(
        &app.state::<system::SystemState>(),
        &app.state::<agent::AgentState>(),
    );
}

fn shutdown_services(system: &system::SystemState, agent: &agent::AgentState) {
    system.shutdown();
    agent.processes.stop_all();
    agent.terminals.stop_all();
}

#[tauri::command]
async fn disconnect_provider_account(
    app: tauri::AppHandle,
    persistence_state: tauri::State<'_, persistence::AppState>,
    oauth_state: tauri::State<'_, openai_codex::OpenAiCodexState>,
    alias: String,
    revision: String,
    replacements: Vec<agent::provider_links::Replacement>,
) -> Result<agent::provider_links::RemovalResult, openai_codex::ProviderError> {
    agent::provider_links::remove(
        app,
        persistence_state.inner().clone(),
        oauth_state.inner().clone(),
        alias,
        revision,
        replacements,
    )
    .await
}
