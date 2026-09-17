//! window_manager.rs — Window positioning and management.
//!
//! Centers the overlay window at the top edge of the primary monitor.
//! Uses Tauri's monitor API which handles DPI scaling correctly.

use tauri::{AppHandle, Manager};

/// Position the main window at the horizontal center of the top screen edge.
/// Uses Tauri's monitor API to correctly handle DPI scaling.
pub fn center_window_at_top(app_handle: &AppHandle) {
    if let Some(window) = app_handle.get_webview_window("main") {
        // Use Tauri's monitor API for DPI-aware positioning
        if let Ok(Some(monitor)) = window.primary_monitor() {
            let monitor_size = monitor.size();
            let monitor_position = monitor.position();

            // Set window size to match the entire monitor for safe hit-testing
            let _ = window.set_size(*monitor_size);
            let _ = window.set_position(*monitor_position);
            let _ = window.show();

            log::info!(
                "Window expanded to full monitor bounds ({}, {}) {}x{}",
                monitor_position.x,
                monitor_position.y,
                monitor_size.width,
                monitor_size.height
            );
        } else {
            // Fallback: just show the window
            let _ = window.show();
            log::warn!("Could not get primary monitor info, window position may be off");
        }
    }
}
