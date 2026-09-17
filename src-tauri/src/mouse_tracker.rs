//! mouse_tracker.rs — Global mouse position tracker + Native hit-test system.
//!
//! Two responsibilities:
//! 1. Auto-hide: Poll cursor → emit show-bar / hide-bar events.
//! 2. Hit-test (Phase 6.1): When Radial is open, dynamically toggle
//!    set_ignore_cursor_events based on whether the cursor is over
//!    an interactive zone (Orb, satellites, panel).
//!
//! The InteractionState is the SINGLE SOURCE OF TRUTH for click-through.
//! React never calls set_ignore_cursor_events directly.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};

#[cfg(windows)]
use windows::Win32::Foundation::POINT;
#[cfg(windows)]
use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;

// ─── Constants ───────────────────────────────────────────────────────

/// Height of the hit-zone at the top of the screen (pixels, physical).
const HIT_ZONE_HEIGHT: i32 = 8;

/// How far below the taskbar the mouse can go before triggering hide (physical px).
const TASKBAR_EXTENDED_ZONE: i32 = 120;

/// Debounce duration before sending hide-bar event (ms).
const HIDE_DEBOUNCE_MS: u64 = 400;

/// Polling interval for hit-test in Radial mode (ms). ~30Hz.
/// Also used as base sleep interval for the unified tracker loop.
const HIT_TEST_INTERVAL_MS: u64 = 33;

/// Padding around each interactive zone (logical px) to prevent flicker.
const ZONE_PADDING: f64 = 12.0;

// ─── InteractionMode & State ─────────────────────────────────────────

#[derive(Clone, Debug, PartialEq)]
pub enum InteractionMode {
    Hidden,   // Click-through everything
    Bar,      // Only the bar area receives input
    Dragging, // Entire window receives input (temporary)
    Radial,   // Hit-test dynamic zones
}

/// A rectangle in CSS/logical pixel coordinates (relative to the window).
#[derive(Clone, Debug, serde::Deserialize)]
pub struct InteractiveZone {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// Shared state between mouse_tracker thread and Tauri commands.
pub struct InteractionState {
    pub mode: InteractionMode,
    pub zones: Vec<InteractiveZone>,
    pub last_ignore_state: Option<bool>,
    pub version: u64, // Used to prevent race conditions during hit testing
}

impl InteractionState {
    pub fn new() -> Self {
        Self {
            mode: InteractionMode::Hidden,
            zones: Vec::new(),
            last_ignore_state: None,
            version: 1,
        }
    }
}

/// Thread-safe handle to the shared interaction state.
pub type SharedInteractionState = Arc<Mutex<InteractionState>>;

// ─── Cursor helpers ──────────────────────────────────────────────────

#[cfg(windows)]
fn get_cursor_position() -> Option<(i32, i32)> {
    let mut point = POINT { x: 0, y: 0 };
    unsafe {
        if GetCursorPos(&mut point).is_ok() {
            Some((point.x, point.y))
        } else {
            None
        }
    }
}

#[cfg(not(windows))]
fn get_cursor_position() -> Option<(i32, i32)> {
    None
}

// ─── Public API ──────────────────────────────────────────────────────

/// Get physical screen info and calculate hit-zone boundaries from Tauri's monitor info.
/// Returns (screen_width, hit_zone_left, hit_zone_right) to correctly support multi-monitor setups (e.g. negative X coords).
pub fn get_screen_and_hitzone(app_handle: &AppHandle) -> (i32, i32, i32) {
    if let Some(window) = app_handle.get_webview_window("main") {
        if let Ok(Some(monitor)) = window.primary_monitor() {
            let screen_w = monitor.size().width as i32;
            let screen_x = monitor.position().x as i32;
            let scale = window.scale_factor().unwrap_or(1.0);
            
            // Hit zone width: 560 logical px × scale factor, capped to screen width
            let hit_w = (560.0 * scale) as i32;
            let hit_w_capped = hit_w.min(screen_w);
            
            // Center the hit zone relative to the monitor's starting X coordinate
            let hit_zone_left = screen_x + (screen_w - hit_w_capped) / 2;
            let hit_zone_right = hit_zone_left + hit_w_capped;
            
            return (screen_w, hit_zone_left, hit_zone_right);
        }
    }
    // Fallback if monitor info is unavailable
    let fallback_w = 1920;
    let fallback_hit = 700;
    let fallback_left = (fallback_w - fallback_hit) / 2;
    (fallback_w, fallback_left, fallback_left + fallback_hit)
}

pub fn is_in_hit_zone(app_handle: &AppHandle) -> bool {
    let (_, hit_zone_left, hit_zone_right) = get_screen_and_hitzone(app_handle);

    if let Some((cx, cy)) = get_cursor_position() {
        cy <= HIT_ZONE_HEIGHT && cx >= hit_zone_left && cx <= hit_zone_right
    } else {
        false
    }
}

/// Starts the unified mouse tracker + hit-test loop on a background thread.
pub fn start_mouse_tracker(
    app_handle: AppHandle,
    interaction_state: SharedInteractionState,
) -> Arc<AtomicBool> {
    let running = Arc::new(AtomicBool::new(true));
    let running_clone = running.clone();

    let (screen_width, hit_zone_left, hit_zone_right) = get_screen_and_hitzone(&app_handle);

    std::thread::spawn(move || {
        run_tracker_loop(
            app_handle,
            running_clone,
            screen_width,
            hit_zone_left,
            hit_zone_right,
            interaction_state,
        );
    });

    running
}

// ─── Main loop ───────────────────────────────────────────────────────

fn run_tracker_loop(
    app_handle: AppHandle,
    running: Arc<AtomicBool>,
    screen_width: i32,
    hit_zone_left: i32,
    hit_zone_right: i32,
    interaction_state: SharedInteractionState,
) {
    let mut is_bar_visible = false;
    let mut hide_timer: Option<Instant> = None;

    let ext_margin = 60;
    let ext_left = hit_zone_left - ext_margin;
    let ext_right = hit_zone_right + ext_margin;

    log::info!(
        "Mouse tracker started. Screen width: {}px, Hit zone: x=[{}..{}], y=[0..{}]",
        screen_width,
        hit_zone_left,
        hit_zone_right,
        HIT_ZONE_HEIGHT,
    );

    let mut last_hit_test = Instant::now();

    while running.load(Ordering::Relaxed) {
        // Use the shorter interval (hit-test rate) as the base sleep
        std::thread::sleep(Duration::from_millis(HIT_TEST_INTERVAL_MS));

        let Some((cx, cy)) = get_cursor_position() else {
            continue;
        };

        // ── 1. Hit-test for click-through (every ~33ms = 30Hz) ──
        if last_hit_test.elapsed() >= Duration::from_millis(HIT_TEST_INTERVAL_MS) {
            last_hit_test = Instant::now();
            do_hit_test(&app_handle, cx, cy, &interaction_state);
        }

        // ── 2. Auto-hide logic (effective ~60ms due to sleep being 33ms) ──
        let in_hit_zone =
            cy <= HIT_ZONE_HEIGHT && cx >= hit_zone_left && cx <= hit_zone_right;

        let in_extended_zone = cy <= (HIT_ZONE_HEIGHT + TASKBAR_EXTENDED_ZONE)
            && cx >= ext_left
            && cx <= ext_right;

        // When Radial is open, suppress auto-hide entirely
        let mode = {
            if let Ok(state) = interaction_state.lock() {
                state.mode.clone()
            } else {
                InteractionMode::Hidden
            }
        };

        if mode == InteractionMode::Radial || mode == InteractionMode::Dragging {
            // Don't auto-hide while Radial or dragging
            hide_timer = None;
            continue;
        }

        if in_hit_zone && !is_bar_visible {
            is_bar_visible = true;
            hide_timer = None;
            let _ = app_handle.emit("show-bar", ());
            log::info!("show-bar emitted (cursor at {}, {})", cx, cy);
        } else if is_bar_visible && !in_extended_zone {
            if hide_timer.is_none() {
                hide_timer = Some(Instant::now());
            }

            if let Some(timer_start) = hide_timer {
                if timer_start.elapsed() >= Duration::from_millis(HIDE_DEBOUNCE_MS) {
                    is_bar_visible = false;
                    hide_timer = None;
                    let _ = app_handle.emit("hide-bar", ());
                    log::info!("hide-bar emitted (cursor at {}, {})", cx, cy);
                }
            }
        } else if is_bar_visible && in_extended_zone {
            hide_timer = None;
        }
    }

    log::info!("Mouse tracker stopped.");
}

// ─── Hit-test logic ──────────────────────────────────────────────────

fn do_hit_test(
    app_handle: &AppHandle,
    cursor_phys_x: i32,
    cursor_phys_y: i32,
    interaction_state: &SharedInteractionState,
) {
    let Some(window) = app_handle.get_webview_window("main") else {
        return;
    };

    // Phase 1: Lock the mutex and copy the current state
    let (mode, zones, state_version) = {
        let Ok(state) = interaction_state.lock() else {
            return;
        };
        (state.mode.clone(), state.zones.clone(), state.version)
    };

    // We do NOT hold the lock while querying the OS/Tauri window state.
    // This prevents deadlocks and lock contention.
    let scale = window.scale_factor().unwrap_or(1.0);
    let win_pos = window.outer_position().unwrap_or(tauri::PhysicalPosition::new(0, 0));
    let win_size = window.outer_size().unwrap_or(tauri::PhysicalSize::new(480, 96));

    // Phase 2: Compute should_ignore based on the snapshot we took
    let should_ignore = match mode {
        InteractionMode::Hidden => true,
        InteractionMode::Bar => {
            let rel_y = (cursor_phys_y - win_pos.y) as f64 / scale;
            let rel_x = (cursor_phys_x - win_pos.x) as f64 / scale;
            let logical_w = win_size.width as f64 / scale;
            
            let bar_w = 480.0;
            let bar_h = 96.0;
            let bar_x = (logical_w - bar_w) / 2.0;
            
            !(rel_x >= bar_x && rel_x <= bar_x + bar_w && rel_y >= 0.0 && rel_y <= bar_h)
        }
        InteractionMode::Dragging => false,
        InteractionMode::Radial => {
            if zones.is_empty() {
                true // Safe default: click-through if no zones defined
            } else {
                let logical_x = (cursor_phys_x - win_pos.x) as f64 / scale;
                let logical_y = (cursor_phys_y - win_pos.y) as f64 / scale;

                let in_any_zone = zones.iter().any(|z| {
                    logical_x >= (z.x - ZONE_PADDING)
                        && logical_x <= (z.x + z.w + ZONE_PADDING)
                        && logical_y >= (z.y - ZONE_PADDING)
                        && logical_y <= (z.y + z.h + ZONE_PADDING)
                });

                !in_any_zone
            }
        }
    };

    // Phase 3: Lock the mutex again to verify version and conditionally update
    let mut should_apply = false;
    {
        let Ok(mut state) = interaction_state.lock() else {
            return;
        };

        // If the version has changed since we took our snapshot, our computation is stale.
        // We discard it and let the next 30Hz tick try again.
        if state.version == state_version {
            if state.last_ignore_state != Some(should_ignore) {
                state.last_ignore_state = Some(should_ignore);
                should_apply = true;
            }
        }
    }

    // Phase 4: Apply the native API call if needed, WITHOUT holding the lock
    if should_apply {
        let _ = window.set_ignore_cursor_events(should_ignore);
    }
}
