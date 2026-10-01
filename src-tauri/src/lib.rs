// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
mod agent;
mod app_exit;
#[cfg(target_os = "macos")]
mod app_menu;
mod background;
mod backup;
mod claude;
mod companion;
mod core;
mod data_dir;
mod desktop;
mod diagnostics;
mod http_client;
mod library;
mod mcp;
mod model_bindings;
mod openai_codex;
mod optional_tools;
mod persistence;
#[cfg_attr(target_os = "linux", path = "secrets_linux.rs")]
mod secrets;
mod skills;
mod system;
mod updater;
#[cfg(feature = "browser-probe")]
pub fn run_browser_probe() {
    initialize_tls();
    agent::browser::probe::run();
}

fn initialize_tls() {
    // HTTP and WebSocket dependencies enable different providers; choose one
    // before either client runs. An existing process default is preserved.
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
}

fn main_only(
    invoke: tauri::ipc::Invoke<tauri::Wry>,
    handler: fn(tauri::ipc::Invoke<tauri::Wry>) -> bool,
) -> bool {
    if !application_command_allowed(invoke.message.webview().label(), invoke.message.command()) {
        invoke
            .resolver
            .reject("Application commands are unavailable in browser tabs.");
        return true;
    }
    handler(invoke)
}

fn application_command_allowed(label: &str, command: &str) -> bool {
    label == "main"
        || (label == "companion"
            && matches!(
                command,
                "get_companion_snapshot"
                    | "ack_companion_item"
                    | "get_companion_usage"
                    | "set_companion_expanded"
                    | "set_companion_bubble"
                    | "companion_start_drag"
                    | "companion_set_interacting"
                    | "companion_open_conversation"
                    | "companion_answer_question"
                    | "companion_pause_question"
                    | "get_companion_chat"
                    | "send_companion_message"
                    | "confirm_companion_project"
                    | "get_companion_conversations"
                    | "stop_companion_chat"
                    | "get_companion_models"
            ))
}

#[cfg(test)]
mod companion_policy_tests {
    use super::application_command_allowed;
    #[test]
    fn auxiliary_window_has_only_its_commands_and_browser_tabs_have_none() {
        assert!(application_command_allowed(
            "main",
            "save_system_preferences"
        ));
        for command in [
            "get_companion_snapshot",
            "ack_companion_item",
            "get_companion_usage",
            "set_companion_bubble",
            "companion_answer_question",
            "companion_open_conversation",
            "get_companion_chat",
            "send_companion_message",
            "confirm_companion_project",
            "get_companion_conversations",
            "stop_companion_chat",
            "get_companion_models",
        ] {
            assert!(application_command_allowed("companion", command));
            assert!(!application_command_allowed("browser-1", command));
        }
        for command in [
            "save_system_preferences",
            "browser_command",
            "get_chat",
            "subscribe_chat",
            "run_shell",
            "disconnect_provider_account",
        ] {
            assert!(!application_command_allowed("companion", command));
            assert!(!application_command_allowed("other", command));
        }
    }
}
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    initialize_tls();
    let builder = tauri::Builder::default();
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    let builder = builder.plugin(tauri_plugin_single_instance::init(|app, _, _| {
        use tauri::Manager;
        diagnostics::record_single_instance_conflict();
        if let Some(window) = app.get_webview_window("main") {
            let _ = window.show();
            let _ = window.unminimize();
            let _ = window.set_focus();
        }
    }));
    builder
        .manage(desktop::DesktopState::default())
        .manage(companion::CompanionState::default())
        .manage(core::CoreState::default())
        .manage(persistence::AppState::default())
        .manage(openai_codex::OpenAiCodexState::default())
        .manage(claude::ClaudeState::default())
        .manage(agent::AgentState::default())
        .manage(agent::knowledge::generation::KnowledgeJobs::default())
        .manage(agent::learning::capture::LearningJobs::default())
        .manage(agent::browser::BrowserState::default())
        .manage(agent::browser::extension::ExtensionState::default())
        .manage(http_client::HttpState::default())
        .manage(agent::dashboard::DashboardState::default())
        .manage(mcp::McpState::default())
        .manage(updater::UpdateState::default())
        .manage(app_exit::ExitState::default())
        .manage(system::SystemState::default())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app| {
            use tauri::Manager;
            let home = app.path().home_dir()?;
            let profile = data_dir::configure(&app.config().identifier)?;
            let lease = data_dir::Lease::acquire(&home, profile)?;
            debug_assert_eq!(lease.root(), data_dir::root(&home));
            let diagnostics =
                diagnostics::initialize(lease.root(), &app.package_info().version.to_string());
            agent::telemetry::initialize(lease.root(), &app.package_info().version.to_string());
            app.state::<agent::AgentState>()
                .setup_execution_grants(lease.root())
                .map_err(std::io::Error::other)?;
            if !app.manage(lease) {
                return Err(std::io::Error::other(
                    "O controle exclusivo do diretório de dados já foi configurado.",
                )
                .into());
            }
            if !app.manage(diagnostics) {
                return Err(
                    std::io::Error::other("O diagnóstico local já foi configurado.").into(),
                );
            }
            desktop::setup(app)?;
            skills::setup(&home).map_err(|error| std::io::Error::other(error.message))?;
            core::health::start_monitor(app.handle());
            system::setup(app.handle())?;
            companion::setup(app.handle());
            let agent = app.state::<agent::AgentState>();
            agent
                .terminals
                .configure(&home, app.state::<persistence::AppState>().inner().clone())
                .map_err(|error| std::io::Error::other(error.message().to_owned()))?;
            if let Err(error) = agent
                .terminals
                .restore(agent::terminals::events(app.handle().clone()))
            {
                // A damaged snapshot must not prevent access to projects or settings.
                eprintln!(
                    "Não foi possível restaurar os terminais: {}",
                    error.message()
                );
            }
            agent::browser::extension::start_if_configured(app.handle());
            Ok(())
        })
        .on_window_event(|window, event| {
            desktop::on_window_event(window, event);
            companion::on_window_event(window, event);
        })
        .invoke_handler(|invoke| {
            // Remote child views must never call privileged application commands,
            // even if a site navigates to a local URL matching the development origin.
            let handler: fn(tauri::ipc::Invoke<tauri::Wry>) -> bool = tauri::generate_handler![
                companion::get_companion_snapshot,
                companion::ack_companion_item,
                companion::get_companion_usage,
                companion::set_companion_expanded,
                companion::set_companion_bubble,
                companion::companion_start_drag,
                companion::companion_set_interacting,
                companion::companion_open_conversation,
                companion::companion_answer_question,
                companion::companion_pause_question,
                agent::companion_chat::get_companion_chat,
                agent::companion_chat::send_companion_message,
                agent::companion_chat::confirm_companion_project,
                agent::companion_chat::get_companion_conversations,
                agent::companion_chat::stop_companion_chat,
                agent::companion_chat::get_companion_models,
                http_client::get_project_http_settings,
                http_client::save_project_http_settings,
                http_client::get_http_snapshot,
                http_client::save_http_draft,
                http_client::close_http_draft,
                http_client::save_http_request,
                http_client::delete_http_request,
                http_client::send_http_request,
                http_client::cancel_http_request,
                http_client::get_http_result,
                http_client::save_http_response,
                http_client::import_http_file,
                http_client::export_project_http,
                http_client::import_project_http,
                agent::browser::get_browser_tabs,
                agent::browser::browser_command,
                agent::browser::set_browser_viewport,
                agent::browser::extension::get_browser_extension_status,
                agent::browser::extension::prepare_browser_extension,
                agent::browser::extension::open_browser_extension_directory,
                agent::browser::extension::revoke_browser_extension,
                agent::browser::extension::open_browser_application,
                greet,
                agent::knowledge::get_project_knowledge,
                agent::learning::get_project_learning,
                agent::learning::set_project_learning,
                agent::learning::save_project_lesson,
                agent::learning::delete_project_lesson,
                agent::learning::export_project_learning,
                agent::learning::preview_project_learning_import,
                agent::learning::import_project_learning,
                agent::knowledge::save_project_knowledge,
                agent::knowledge::link_project_knowledge,
                agent::knowledge::import_project_knowledge,
                agent::knowledge::generation::generate_project_knowledge,
                agent::knowledge::generation::cancel_project_knowledge_generation,
                claude::get_claude_runtime,
                claude::refresh_claude_runtime,
                claude::get_claude_usage,
                system::save_claude_provider_preferences,
                system::get_system_preferences,
                system::save_system_preferences,
                system::test_system_notification,
                optional_tools::get_optional_tools_status,
                optional_tools::install_optional_tool,
                backup::export_settings_backup,
                backup::inspect_settings_backup,
                backup::import_settings_backup,
                system::unread::get_unread_conversations,
                system::unread::mark_conversation_read,
                updater::check_app_update,
                app_exit::get_app_shutdown_status,
                app_exit::get_pending_app_exit,
                app_exit::confirm_app_exit,
                app_exit::cancel_app_exit,
                updater::install_app_update,
                core::get_core_status,
                core::check_core_updates,
                core::install_core_component,
                core::health::diagnose_core,
                core::health::repair_core_component,
                diagnostics::get_diagnostic_summary,
                diagnostics::check_database_integrity,
                diagnostics::export_diagnostic_bundle,
                agent::telemetry::export_harness_trace,
                agent::telemetry::get_harness_report,
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
                skills::get_skill_cache_status,
                skills::clear_skill_cache,
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
                agent::publication::get_project_publication_settings,
                agent::publication::save_project_publication_settings,
                agent::attachments::import_chat_attachments,
                agent::attachments::get_chat_attachment_image,
                agent::attachments::save_chat_image,
                agent::response_export::save_markdown_document,
                agent::web_search::set_web_search_config,
                library::get_library_snapshot,
                library::workspaces::move_project_workspace,
                library::workspaces::get_workspace_storage,
                agent::workflow::catalog::permissions::get_agent_tool_permissions,
                library::open_project_directory,
                library::repositories::get_project_repositories,
                library::repositories::save_project_repository,
                library::repositories::delete_project_repository,
                library::open_conversation_path,
                library::files::list_project_directory,
                library::files::read_project_file,
                library::files::get_project_video,
                library::files::save_project_video,
                library::files::open_project_video,
                library::create_workspace,
                library::add_project,
                library::create_conversation,
                library::update_project,
                library::rename_conversation,
                library::delete_library_item,
                library::select_library_item,
                library::get_conversation,
                agent::dashboard::get_project_metrics,
                core::beads::dashboard::get_project_beads,
                core::beads::dashboard::get_bead_detail,
                core::beads::dashboard::add_bead_comment,
                core::beads::dashboard::close_conversation_plan,
                agent::get_chat,
                agent::subscribe_chat,
                agent::terminals::get_terminal_activity,
                agent::terminals::list_project_terminals,
                agent::terminals::create_project_terminal,
                agent::terminals::read_project_terminal,
                agent::terminals::write_project_terminal,
                agent::terminals::resize_project_terminal,
                agent::terminals::rename_project_terminal,
                agent::terminals::close_project_terminal,
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
                agent::workflow::pause_workflow_question,
                agent::workflow::answer_workflow_authoring,
                agent::history::get_chat_history,
                agent::history::get_chat_tool_call,
                agent::cleanup::preview_chat_cleanup,
                agent::cleanup::cleanup_old_chats,
                agent::journal_maintenance::get_journal_maintenance_status,
                agent::journal_maintenance::optimize_journals,
                agent::get_agent_activity,
                agent::start_agent_turn,
                agent::resume_agent_queue,
                agent::retry_agent_turn,
                agent::resume_interrupted_workflow,
                agent::maintenance::compact_agent_context,
                agent::queue::remove_queued_message,
                agent::queue::delete_queued_message,
                agent::queue::reorder_queued_messages,
                agent::queue::send_queued_message_now,
                agent::diffs::get_agent_file_diff,
                agent::diffs::get_agent_file_changes,
                agent::cancel_agent_turn,
                agent::approve_agent_tool,
                agent::list_execution_grants,
                agent::revoke_execution_grant,
                agent::questions::answer_agent_question,
                agent::questions::pause_agent_question,
                agent::authoring::answer_agent_authoring,
                openai_codex::list_provider_accounts,
                openai_codex::refresh_provider_models,
                openai_codex::set_provider_model_enabled,
                openai_codex::custom::save_custom_provider,
                openai_codex::custom::discovery::lookup_custom_model,
                openai_codex::set_provider_enabled,
                openai_codex::usage::get_provider_usage,
                openai_codex::usage::set_provider_usage_visibility,
                openai_codex::usage::set_provider_usage_alert,
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
            #[cfg(target_os = "macos")]
            if let tauri::RunEvent::Reopen { .. } = &event {
                use tauri::Manager;
                // A visible companion does not mean the user's main window is visible.
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.unminimize();
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
            if let tauri::RunEvent::ExitRequested { api, .. } = event {
                if app_exit::request(app) {
                    api.prevent_exit();
                }
            }
        });
}

// The Windows installer can fail after its exit hook, so keep background
// services and the normal exit confirmation available until it actually exits.
pub(crate) fn prepare_exit_for_installer(app: &tauri::AppHandle) -> Result<(), String> {
    use tauri::Manager;
    desktop::flush(app);
    app.state::<agent::AgentState>()
        .terminals
        .shutdown()
        .map_err(|error| error.message().to_owned())
}

pub(crate) fn prepare_exit(app: &tauri::AppHandle) -> Result<(), String> {
    prepare_exit_with_reason(app, diagnostics::ShutdownReason::UserExit)
}

pub(crate) fn prepare_exit_for_update(app: &tauri::AppHandle) -> Result<(), String> {
    prepare_exit_with_reason(app, diagnostics::ShutdownReason::Update)
}

fn prepare_exit_with_reason(
    app: &tauri::AppHandle,
    reason: diagnostics::ShutdownReason,
) -> Result<(), String> {
    use tauri::Manager;
    desktop::flush(app);
    shutdown_services(
        &app.state::<system::SystemState>(),
        &app.state::<agent::AgentState>(),
    )?;
    app_exit::allow(app);
    diagnostics::finish(reason);
    Ok(())
}

fn shutdown_services(
    system: &system::SystemState,
    agent: &agent::AgentState,
) -> Result<(), String> {
    agent
        .terminals
        .shutdown()
        .map_err(|error| error.message().to_owned())?;
    system.shutdown();
    Ok(())
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
