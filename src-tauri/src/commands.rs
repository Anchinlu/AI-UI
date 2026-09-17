//! commands.rs — Tauri commands exposed to the frontend.
//!
//! Phase 1-2: Minimal. No commands needed yet since mouse tracking
//! communicates via Tauri events, not commands.
//!
//! Phase 3+: Will add get_system_time, setAiState, etc.

use tauri::{command, AppHandle, Emitter, Manager};

/// Placeholder command for future use.
/// Returns the current phase for debugging purposes.
#[command]
pub fn get_phase_info() -> String {
    "Phase 1+2: Scaffold + Auto-Hide Prototype".to_string()
}

/// Checks if the cursor is currently in the hit zone.
/// Used by the frontend on startup to handle the case where the
/// mouse is already in the hit zone before React loads.
#[command]
pub fn check_initial_state(app_handle: AppHandle) -> bool {
    crate::mouse_tracker::is_in_hit_zone(&app_handle)
}

#[derive(serde::Serialize)]
pub struct SystemInfo {
    time: String,
    date: String,
    battery_percent: u8,
    is_charging: bool,
}

#[command]
pub fn get_system_info() -> SystemInfo {
    let local_time = chrono::Local::now();
    let time = local_time.format("%I:%M %p").to_string();
    let date = local_time.format("%d Th%m").to_string(); // e.g. 12 Th10

    let mut battery_percent = 100;
    let mut is_charging = false;

    #[cfg(windows)]
    {
        use windows::Win32::System::Power::GetSystemPowerStatus;
        use windows::Win32::System::Power::SYSTEM_POWER_STATUS;
        
        let mut status = SYSTEM_POWER_STATUS::default();
        unsafe {
            if GetSystemPowerStatus(&mut status).is_ok() {
                if status.BatteryLifePercent <= 100 {
                    battery_percent = status.BatteryLifePercent;
                }
                if status.ACLineStatus == 1 {
                    is_charging = true;
                }
            }
        }
    }

    SystemInfo {
        time,
        date,
        battery_percent,
        is_charging,
    }
}
/// DEPRECATED: Kept for backward compatibility.
/// Use set_interaction_mode instead. Rust InteractionState is the single source of truth.
#[command]
pub fn set_click_through(_window: tauri::Window, _ignore: bool) {
    // No-op: click-through is now managed exclusively by InteractionState + mouse_tracker.
    // This command is intentionally disabled to prevent race conditions.
    log::warn!("set_click_through called but is deprecated. Use set_interaction_mode instead.");
}

/// Set the current interaction mode. Rust will decide click-through based on this.
#[command]
pub fn set_interaction_mode(
    app_handle: tauri::AppHandle,
    mode: String,
) -> Result<(), String> {
    use crate::mouse_tracker::InteractionMode;

    let parsed_mode = match mode.as_str() {
        "hidden" => InteractionMode::Hidden,
        "bar" => InteractionMode::Bar,
        "dragging" => InteractionMode::Dragging,
        "radial" => InteractionMode::Radial,
        _ => return Err(format!("Unknown interaction mode: {}", mode)),
    };

    let state = app_handle
        .try_state::<crate::mouse_tracker::SharedInteractionState>()
        .ok_or("InteractionState not initialized")?;

    if let Ok(mut s) = state.lock() {
        // When switching to Radial, clear zones so default is click-through (safe default)
        if parsed_mode == InteractionMode::Radial && s.mode != InteractionMode::Radial {
            s.zones.clear();
            // Force re-evaluation on next hit-test cycle
            s.last_ignore_state = None;
        }
        
        // When leaving Radial/Dragging, force re-evaluation
        if parsed_mode != s.mode {
            s.last_ignore_state = None;
            s.version += 1;
        }
        
        s.mode = parsed_mode;
        log::info!("Interaction mode set to: {}", mode);
    }
    Ok(())
}

/// Update interactive zones (CSS/logical pixel rects relative to window).
/// Called by React to tell Rust where clickable UI elements are.
#[command]
pub fn update_interactive_zones(
    app_handle: tauri::AppHandle,
    zones: Vec<crate::mouse_tracker::InteractiveZone>,
) -> Result<(), String> {
    let state = app_handle
        .try_state::<crate::mouse_tracker::SharedInteractionState>()
        .ok_or("InteractionState not initialized")?;

    if let Ok(mut s) = state.lock() {
        s.zones = zones;
        s.version += 1;
    }
    Ok(()
    )
}

#[command]
pub fn expand_window(window: tauri::Window, height: f64) {
    if let Ok(Some(monitor)) = window.primary_monitor() {
        let max_height = monitor.size().height as f64;
        let scale_factor = window.scale_factor().unwrap_or(1.0);
        let max_logical = max_height / scale_factor;
        
        let target_h = height.min(max_logical);
        let _ = window.set_size(tauri::LogicalSize::new(800.0, target_h));
        
        // Re-center horizontally
        let monitor_pos = monitor.position();
        let x = monitor_pos.x as f64 + (monitor.size().width as f64 - (800.0 * scale_factor)) / 2.0;
        let _ = window.set_position(tauri::PhysicalPosition::new(x as i32, monitor_pos.y));
    }
}

#[command]
pub fn shrink_window(window: tauri::Window) {
    if let Ok(Some(monitor)) = window.primary_monitor() {
        let scale_factor = window.scale_factor().unwrap_or(1.0);
        let _ = window.set_size(tauri::LogicalSize::new(480.0, 96.0));
        
        // Re-center horizontally
        let monitor_pos = monitor.position();
        let x = monitor_pos.x as f64 + (monitor.size().width as f64 - (480.0 * scale_factor)) / 2.0;
        let _ = window.set_position(tauri::PhysicalPosition::new(x as i32, monitor_pos.y));
    }
}

// ==========================================
// AI ADAPTER API CONTRACT (Giai đoạn 6)
// ==========================================

#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub struct AiServerStatus {
    pub backend: String, // "llama.cpp" or "ollama"
    pub model: String,
    pub is_ready: bool,
    pub ram_usage: u32,
}

#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub struct AiGenerateParams {
    pub request_id: String,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
    pub stream: Option<bool>,
}

/// Checks the status of the local AI server
#[command]
pub async fn ai_get_status() -> Result<AiServerStatus, String> {
    // TODO: Implement actual health check against 127.0.0.1:8080 (llama.cpp) or 11435 (Ollama)
    Ok(AiServerStatus {
        backend: "llama.cpp".into(),
        model: "qwen2.5:1.5b".into(),
        is_ready: true,
        ram_usage: 1056,
    })
}

/// Generates a complete text response (blocking until done)
#[command]
pub async fn ai_generate(prompt: String, params: AiGenerateParams) -> Result<String, String> {
    // TODO: Implement HTTP POST to backend
    println!("Mocking ai_generate for request_id: {}", params.request_id);
    Ok(format!("Mock response to: {}", prompt))
}

/// Starts streaming a text response via Tauri events
#[command]
pub async fn ai_stream(window: tauri::Window, _prompt: String, params: AiGenerateParams) -> Result<(), String> {
    // TODO: Implement HTTP streaming to backend
    // Will emit events like `ai-stream-chunk` using window.emit()
    println!("Mocking ai_stream for request_id: {}", params.request_id);
    
    let req_id = params.request_id.clone();
    
    // Spawn a standard thread for mocking sleep without tokio dependency
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(500));
        let _ = window.emit("ai-stream-chunk", serde_json::json!({
            "request_id": req_id,
            "text": "Xin chào, ",
            "done": false
        }));
        
        std::thread::sleep(std::time::Duration::from_millis(500));
        let _ = window.emit("ai-stream-chunk", serde_json::json!({
            "request_id": req_id,
            "text": "đây là phản hồi thử nghiệm.",
            "done": true
        }));
    });
    
    Ok(())
}

/// Cancels an ongoing generation request
#[command]
pub async fn ai_stop(request_id: String) -> Result<(), String> {
    // TODO: Implement cancellation logic (e.g., dropping the Future or sending cancel signal)
    println!("Cancelled request_id: {}", request_id);
    Ok(())
}

// ==========================================
// SYSTEM CONTROL API CONTRACT (Giai đoạn 6)
// ==========================================

#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub struct AudioStatus {
    pub volume: u8,
    pub muted: bool,
    pub output_device: Option<String>,
}

#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub struct DisplayInfo {
    pub id: String,
    pub name: String,
    pub is_primary: bool,
    pub brightness: Option<u8>,
    pub supports_brightness: bool,
    pub is_wmi: bool,
}

#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub struct DisplayStatus {
    pub monitors: Vec<DisplayInfo>,
}

#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub struct ConnectedDevice {
    pub name: String,
    pub device_type: String, // "wifi" | "bluetooth"
    pub connection_state: String, // "connected" | "detected" | "unknown"
}

#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub struct ConnectivityStatus {
    pub wifi_enabled: bool,
    pub bluetooth_enabled: bool,
    pub detected_devices: Vec<ConnectedDevice>,
}

#[command]
pub async fn get_audio_status() -> Result<AudioStatus, String> {
    #[cfg(windows)]
    {
        tauri::async_runtime::spawn_blocking(|| {
            let status = crate::system::windows::audio::get_audio_status()?;
            Ok(AudioStatus {
                volume: status.volume,
                muted: status.muted,
                output_device: status.output_device,
            })
        })
        .await
        .map_err(|e| format!("Task join error: {}", e))?
    }
    #[cfg(not(windows))]
    {
        Err("Audio control is only supported on Windows".into())
    }
}

#[command]
pub async fn set_volume(level: u8) -> Result<(), String> {
    if level > 100 {
        return Err("Volume level must be between 0 and 100".into());
    }
    #[cfg(windows)]
    {
        tauri::async_runtime::spawn_blocking(move || {
            crate::system::windows::audio::set_volume(level)
        })
        .await
        .map_err(|e| format!("Task join error: {}", e))?
    }
    #[cfg(not(windows))]
    {
        Err("Audio control is only supported on Windows".into())
    }
}

#[command]
pub async fn set_mute(muted: bool) -> Result<(), String> {
    #[cfg(windows)]
    {
        tauri::async_runtime::spawn_blocking(move || {
            crate::system::windows::audio::set_mute(muted)
        })
        .await
        .map_err(|e| format!("Task join error: {}", e))?
    }
    #[cfg(not(windows))]
    {
        Err("Audio control is only supported on Windows".into())
    }
}

#[command]
pub async fn get_display_status() -> Result<DisplayStatus, String> {
    #[cfg(windows)]
    {
        tauri::async_runtime::spawn_blocking(|| {
            let status = crate::system::windows::display::get_display_status()?;
            Ok(DisplayStatus {
                monitors: status.monitors.into_iter().map(|m| DisplayInfo {
                    id: m.id,
                    name: m.name,
                    is_primary: m.is_primary,
                    brightness: m.brightness,
                    supports_brightness: m.supports_brightness,
                    is_wmi: m.is_wmi,
                }).collect(),
            })
        })
        .await
        .map_err(|e| format!("Task join error: {}", e))?
    }
    #[cfg(not(windows))]
    {
        Err("Display control is only supported on Windows".into())
    }
}

#[command]
pub async fn set_brightness(monitor_id: String, level: u8) -> Result<(), String> {
    if level > 100 {
        return Err("Brightness level must be between 0 and 100".into());
    }
    #[cfg(windows)]
    {
        tauri::async_runtime::spawn_blocking(move || {
            crate::system::windows::display::set_brightness(&monitor_id, level)
        })
        .await
        .map_err(|e| format!("Task join error: {}", e))?
    }
    #[cfg(not(windows))]
    {
        Err("Display control is only supported on Windows".into())
    }
}

#[command]
pub async fn get_connectivity_status() -> Result<ConnectivityStatus, String> {
    #[cfg(windows)]
    {
        tauri::async_runtime::spawn_blocking(|| {
            crate::system::windows::connectivity::get_connectivity_status()
        })
        .await
        .map_err(|e| format!("Task join error: {}", e))?
    }
    #[cfg(not(windows))]
    {
        Err("Connectivity reading is only supported on Windows".into())
    }
}

#[command]
pub async fn launch_application(app_id: String) -> Result<(), String> {
    let whitelist = ["browser", "files", "notes", "settings", "terminal"];
    if !whitelist.contains(&app_id.as_str()) {
        return Err(format!("Application '{}' is not in the allowed whitelist.", app_id));
    }
    println!("Mock launch_application: {}", app_id);
    Ok(())
}

#[command]
pub async fn execute_quick_action(action_id: String) -> Result<(), String> {
    let whitelist = ["screenshot", "dnd", "lock", "clipboard"];
    if !whitelist.contains(&action_id.as_str()) {
        return Err(format!("Quick action '{}' is not in the allowed whitelist.", action_id));
    }
    println!("Mock execute_quick_action: {}", action_id);
    Ok(())
}


