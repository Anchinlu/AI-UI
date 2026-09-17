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
            let scale_factor = window.scale_factor().unwrap_or(1.0);

            // Window size in physical pixels
            let window_width_physical = 480.0 * scale_factor;

            // Center horizontally on the primary monitor
            let x = monitor_position.x as f64
                + (monitor_size.width as f64 - window_width_physical) / 2.0;
            let y = monitor_position.y as f64;

            // Use PhysicalPosition for pixel-perfect placement
            let pos = tauri::PhysicalPosition::new(x as i32, y as i32);
            let _ = window.set_position(pos);
            let _ = window.show();

            log::info!(
                "Window centered at physical ({}, {}), monitor: {}x{}, scale: {:.2}",
                x as i32,
                y as i32,
                monitor_size.width,
                monitor_size.height,
                scale_factor
            );
        } else {
            // Fallback: just show the window
            let _ = window.show();
            log::warn!("Could not get primary monitor info, window position may be off");
        }
    }
}
