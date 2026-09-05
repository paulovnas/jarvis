// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
mod agent;
mod library;
mod openai_codex;
mod persistence;
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(persistence::AppState::default())
        .manage(openai_codex::OpenAiCodexState::default())
        .manage(agent::AgentState::default())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            greet,
            persistence::get_app_config,
            persistence::complete_onboarding,
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
            agent::cancel_agent_turn,
            agent::approve_agent_tool,
            openai_codex::list_provider_accounts,
            openai_codex::begin_openai_codex_connection,
            openai_codex::wait_openai_codex_connection,
            openai_codex::cancel_openai_codex_connection,
            disconnect_provider_account
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
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
