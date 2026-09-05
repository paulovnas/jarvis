// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
mod agent;
mod desktop;
mod library;
mod mcp;
mod openai_codex;
mod persistence;
mod skills;
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(desktop::DesktopState::default())
        .manage(persistence::AppState::default())
        .manage(openai_codex::OpenAiCodexState::default())
        .manage(agent::AgentState::default())
        .manage(mcp::McpState::default())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(desktop::setup)
        .on_window_event(desktop::on_window_event)
        .invoke_handler(tauri::generate_handler![
            greet,
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
            agent::get_chat,
            agent::get_agent_activity,
            agent::start_agent_turn,
            agent::resume_agent_queue,
            agent::maintenance::compact_agent_context,
            agent::queue::remove_queued_message,
            agent::diffs::get_agent_file_diff,
            agent::cancel_agent_turn,
            agent::approve_agent_tool,
            agent::questions::answer_agent_question,
            openai_codex::list_provider_accounts,
            openai_codex::set_provider_enabled,
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
