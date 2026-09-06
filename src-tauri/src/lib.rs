// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
mod agent;
#[cfg(target_os = "macos")]
mod app_menu;
mod core;
mod desktop;
mod library;
mod mcp;
mod openai_codex;
mod persistence;
mod skills;
mod system;
mod updater;
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
        .manage(agent::dashboard::DashboardState::default())
        .manage(mcp::McpState::default())
        .manage(updater::UpdateState::default())
        .manage(system::SystemState::default())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app| {
            desktop::setup(app)?;
            system::setup(app.handle())
        })
        .on_window_event(desktop::on_window_event)
        .invoke_handler(tauri::generate_handler![
            greet,
            system::get_system_preferences,
            system::save_system_preferences,
            system::test_system_notification,
            updater::check_app_update,
            updater::install_app_update,
            core::get_core_status,
            core::check_core_updates,
            core::install_core_component,
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
            agent::attachments::import_chat_attachments,
            agent::attachments::get_chat_attachment_image,
            agent::web_search::set_web_search_config,
            library::get_library_snapshot,
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
            agent::workflow::get_workflow,
            agent::workflow::validation::decide_workflow_validation,
            agent::workflow::validation::submit_workflow_validation,
            agent::workflow::settings::get_agent_models,
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
            openai_codex::wait_openai_codex_connection,
            openai_codex::cancel_openai_codex_connection,
            disconnect_provider_account
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            if matches!(event, tauri::RunEvent::ExitRequested { .. }) {
                desktop::flush(app);
                use tauri::Manager;
                app.state::<system::SystemState>().shutdown();
                app.state::<agent::AgentState>().processes.stop_all();
            }
        });
}

#[tauri::command]
async fn disconnect_provider_account(
    app: tauri::AppHandle,
    persistence_state: tauri::State<'_, persistence::AppState>,
    oauth_state: tauri::State<'_, openai_codex::OpenAiCodexState>,
    alias: String,
) -> Result<(), openai_codex::ProviderError> {
    openai_codex::disconnect_provider_account_command(app, persistence_state, oauth_state, alias)
        .await
}
