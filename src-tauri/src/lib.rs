//! lib.rs — Tauri application entry point.
//!
//! Initializes the app, positions the window at top-center,
//! and starts the global mouse tracker on a background thread.
//! Phase 6.1: InteractionState shared between mouse_tracker and commands.

pub mod commands;
pub mod mouse_tracker;
pub mod ai_task_manager;
pub mod persona;
pub mod generation;
pub mod context_budget;
pub mod conversation_logger;
pub mod settings;
pub mod provider;
mod system;
mod window_manager;

use commands::{get_phase_info, check_initial_state};
use mouse_tracker::{InteractionState, SharedInteractionState};
use std::sync::{Arc, Mutex};
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            // Logging (debug only)
            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .targets([
                            tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::Stdout),
                            tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::Webview),
                        ])
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }

            // Phase 6.1: Create shared InteractionState
            let interaction_state: SharedInteractionState =
                Arc::new(Mutex::new(InteractionState::new()));

            // Store in Tauri's managed state so commands can access it
            app.manage(interaction_state.clone());

            // Phase 7: AI Task Manager
            app.manage(ai_task_manager::AiTaskManager::new());

            // Phase 9: Settings Manager
            let app_config = settings::load_config(app.handle());
            let app_config_state: settings::AppConfigState = Arc::new(tokio::sync::Mutex::new(app_config));
            app.manage(app_config_state);

            // Phase 6: Provider Registry
            app.manage(provider::ProviderRegistry::new());

            // Phase 8: Conversation Logger
            app.manage(conversation_logger::new_session_log_state());

            // Position window at top-center of screen
            window_manager::center_window_at_top(app.handle());

            // Start global mouse tracker (background thread) with shared state
            let _tracker_handle =
                mouse_tracker::start_mouse_tracker(app.handle().clone(), interaction_state);

            log::info!("AI Taskbar initialized — Phase 6.1 Native Interaction Safety");

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_phase_info, 
            check_initial_state,
            commands::get_system_info,
            commands::set_click_through,
            commands::set_interaction_mode,
            commands::update_interactive_zones,
            commands::expand_window,
            commands::shrink_window,
            settings::get_config,
            settings::set_model_config,
            commands::ai_get_status,
            commands::ai_generate,
            commands::ai_stream,
            commands::ai_stop,
            commands::get_audio_status,
            commands::set_volume,
            commands::set_mute,
            commands::get_display_status,
            commands::set_brightness,
            commands::get_connectivity_status,
            commands::launch_application,
            commands::execute_quick_action,
            commands::append_conversation_turn
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
