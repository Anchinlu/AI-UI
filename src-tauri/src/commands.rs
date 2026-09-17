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
pub fn expand_window(_window: tauri::Window, _height: Option<f64>) {
    // No-op in Full-Screen mode
}

#[command]
pub fn shrink_window(_window: tauri::Window) {
    // No-op in Full-Screen mode
}

// ==========================================
// AI ADAPTER API CONTRACT (Giai đoạn 6)
// ==========================================

#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub struct AiServerStatus {
    pub backend: String, // "llama.cpp" or "ollama"
    pub model: String,
    pub is_ready: bool,
    pub ram_usage: Option<u32>,
}

#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub struct AiGenerateParams {
    pub request_id: String,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
    pub stream: Option<bool>,
}

fn get_model_name() -> String {
    std::env::var("OLLAMA_MODEL").unwrap_or_else(|_| "qwen2.5:1.5b".to_string())
}

async fn get_valid_ollama_url(state: &tauri::State<'_, crate::ai_task_manager::AiTaskManager>) -> Result<String, String> {
    {
        let cached = state.active_url.lock().await;
        if let Some(url) = &*cached {
            return Ok(url.clone());
        }
    }

    let env_url = std::env::var("OLLAMA_URL").ok();
    let ports = if let Some(url) = env_url {
        vec![url]
    } else {
        vec!["http://127.0.0.1:11435".to_string(), "http://127.0.0.1:11434".to_string()]
    };

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(2))
        .build()
        .map_err(|e| format!("Lỗi HTTP client: {}", e))?;

    for url in ports {
        if let Ok(res) = client.get(format!("{}/api/tags", url)).send().await {
            if res.status().is_success() {
                let mut cached = state.active_url.lock().await;
                *cached = Some(url.clone());
                return Ok(url);
            }
        }
    }

    Err("Không thể kết nối đến Ollama trên bất kỳ cổng nào (11435, 11434)".into())
}

#[derive(serde::Deserialize)]
struct OllamaTagResponse {
    models: Vec<OllamaModel>,
}

#[derive(serde::Deserialize)]
struct OllamaModel {
    name: String,
}

#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub struct AiChatMessage {
    pub role: String,
    pub content: String,
}

#[derive(serde::Serialize, Clone)]
struct OllamaChatOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    num_predict: Option<u32>,
}

#[derive(serde::Serialize, Clone)]
struct OllamaChatRequest {
    model: String,
    messages: Vec<AiChatMessage>,
    stream: bool,
    options: OllamaChatOptions,
}

#[derive(serde::Deserialize)]
struct OllamaChatResponseMessage {
    content: String,
}

#[derive(serde::Deserialize)]
struct OllamaChatResponse {
    message: OllamaChatResponseMessage,
}

/// Validates message roles and prepends a neutral system message.
fn build_ollama_messages(history: Vec<AiChatMessage>) -> Result<Vec<AiChatMessage>, String> {
    for msg in &history {
        if msg.role != "user" && msg.role != "assistant" {
            return Err(format!("Role không hợp lệ: '{}'. Chỉ chấp nhận 'user' hoặc 'assistant'.", msg.role));
        }
    }
    let mut messages = vec![
        AiChatMessage {
            role: "system".to_string(),
            content: "You are a helpful assistant.".to_string(),
        },
    ];
    messages.extend(history);
    Ok(messages)
}

/// Checks the status of the local AI server
#[command]
pub async fn ai_get_status(state: tauri::State<'_, crate::ai_task_manager::AiTaskManager>) -> Result<AiServerStatus, String> {
    let ollama_url = get_valid_ollama_url(&state).await?;

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(2))
        .build()
        .map_err(|e| format!("Lỗi khởi tạo HTTP client: {}", e))?;

    let res = client.get(format!("{}/api/tags", ollama_url))
        .send()
        .await
        .map_err(|e| format!("Server chưa chạy hoặc lỗi kết nối: {}", e))?;

    if !res.status().is_success() {
        return Err(format!("Lỗi HTTP từ Ollama: {}", res.status()));
    }

    let tags: OllamaTagResponse = res.json()
        .await
        .map_err(|e| format!("Không thể parse response: {}", e))?;

    let model_name = get_model_name();
    let is_ready = tags.models.iter().any(|m| m.name == model_name);

    if !is_ready {
        return Err(format!("Model '{}' chưa được tải.", model_name));
    }

    Ok(AiServerStatus {
        backend: "ollama".into(),
        model: model_name,
        is_ready: true,
        ram_usage: None,
    })
}

/// Generates a complete text response (blocking until done)
#[command]
pub async fn ai_generate(state: tauri::State<'_, crate::ai_task_manager::AiTaskManager>, messages: Vec<AiChatMessage>, params: AiGenerateParams) -> Result<String, String> {
    let ollama_url = get_valid_ollama_url(&state).await?;
    let ollama_messages = build_ollama_messages(messages)?;

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .map_err(|e| format!("Lỗi khởi tạo HTTP client: {}", e))?;

    let req_body = OllamaChatRequest {
        model: get_model_name(),
        messages: ollama_messages,
        stream: false,
        options: OllamaChatOptions {
            temperature: params.temperature,
            num_predict: params.max_tokens,
        },
    };

    let res = client.post(format!("{}/api/chat", ollama_url))
        .json(&req_body)
        .send()
        .await
        .map_err(|e| format!("Lỗi gọi Ollama API (timeout hoặc server down): {}", e))?;

    if !res.status().is_success() {
        return Err(format!("Lỗi HTTP từ Ollama: {}", res.status()));
    }

    let chat_res: OllamaChatResponse = res.json()
        .await
        .map_err(|e| format!("Lỗi parse kết quả JSON (expected message.content): {}", e))?;

    Ok(chat_res.message.content)
}

#[command]
pub async fn ai_stream(
    window: tauri::Window, 
    state: tauri::State<'_, crate::ai_task_manager::AiTaskManager>, 
    messages: Vec<AiChatMessage>, 
    params: AiGenerateParams
) -> Result<(), String> {
    let ollama_url = get_valid_ollama_url(&state).await?;
    let ollama_messages = build_ollama_messages(messages)?;
    let req_id = params.request_id.clone();

    // Generate a new generation token for this request
    let generation = {
        let mut gen = state.next_generation.lock().await;
        *gen += 1;
        *gen
    };

    // Abort existing task with same ID if any
    {
        let mut tasks = state.tasks.lock().await;
        if let Some(existing) = tasks.remove(&req_id) {
            existing.handle.abort();
        }
    }

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .map_err(|e| format!("Lỗi khởi tạo HTTP client: {}", e))?;

    let req_body = OllamaChatRequest {
        model: get_model_name(),
        messages: ollama_messages,
        stream: true,
        options: OllamaChatOptions {
            temperature: params.temperature,
            num_predict: params.max_tokens,
        },
    };

    let tasks_clone = state.tasks.clone();
    let req_id_clone = req_id.clone();
    
    // Lock BEFORE spawning to prevent the task from cleaning up before insertion is done
    let mut tasks_lock = state.tasks.lock().await;
    
    let handle = tauri::async_runtime::spawn(async move {
        let mut buffer: Vec<u8> = Vec::new();
        match client.post(format!("{}/api/chat", ollama_url)).json(&req_body).send().await {
            Ok(mut res) => {
                if !res.status().is_success() {
                    let status = res.status();
                    let body = res.text().await.unwrap_or_default();
                    let _ = window.emit("ai-stream-error", serde_json::json!({
                        "request_id": &req_id_clone,
                        "error": format!("Lỗi HTTP từ Ollama: {} — {}", status, body)
                    }));
                } else {
                loop {
                    match res.chunk().await {
                        Ok(Some(chunk)) => {
                            buffer.extend_from_slice(&chunk);
                            
                            while let Some(idx) = buffer.iter().position(|&b| b == b'\n') {
                                let line = buffer.drain(..=idx).collect::<Vec<u8>>();
                                if line.trim_ascii().is_empty() { continue; }
                                
                                match serde_json::from_slice::<serde_json::Value>(&line) {
                                    Ok(json) => {
                                        let content = json
                                            .get("message")
                                            .and_then(|m| m.get("content"))
                                            .and_then(|v| v.as_str())
                                            .unwrap_or("");
                                        let is_done = json.get("done").and_then(|v| v.as_bool()).unwrap_or(false);
                                        let _ = window.emit("ai-stream-chunk", serde_json::json!({
                                            "request_id": &req_id_clone,
                                            "text": content,
                                            "done": is_done
                                        }));
                                    }
                                    Err(e) => {
                                        let _ = window.emit("ai-stream-error", serde_json::json!({
                                            "request_id": &req_id_clone,
                                            "error": format!("Lỗi parse JSON: {}", e)
                                        }));
                                    }
                                }
                            }
                        }
                        Ok(None) => {
                            // End of stream — parse any remaining buffer
                            if !buffer.trim_ascii().is_empty() {
                                match serde_json::from_slice::<serde_json::Value>(&buffer) {
                                    Ok(json) => {
                                        let content = json
                                            .get("message")
                                            .and_then(|m| m.get("content"))
                                            .and_then(|v| v.as_str())
                                            .unwrap_or("");
                                        let is_done = json.get("done").and_then(|v| v.as_bool()).unwrap_or(false);
                                        let _ = window.emit("ai-stream-chunk", serde_json::json!({
                                            "request_id": &req_id_clone,
                                            "text": content,
                                            "done": is_done
                                        }));
                                    }
                                    Err(e) => {
                                        let _ = window.emit("ai-stream-error", serde_json::json!({
                                            "request_id": &req_id_clone,
                                            "error": format!("Lỗi parse JSON buffer dư: {}", e)
                                        }));
                                    }
                                }
                            }
                            break;
                        }
                        Err(e) => {
                            let _ = window.emit("ai-stream-error", serde_json::json!({
                                "request_id": &req_id_clone,
                                "error": format!("Lỗi đọc dữ liệu mạng: {}", e)
                            }));
                            break;
                        }
                    }
                }
                } // end else (status ok)
            }
            Err(e) => {
                let _ = window.emit("ai-stream-error", serde_json::json!({
                    "request_id": &req_id_clone,
                    "error": format!("Lỗi gọi API: {}", e)
                }));
            }
        }

        // Cleanup after task is fully complete
        let mut tasks = tasks_clone.lock().await;
        if let Some(entry) = tasks.get(&req_id_clone) {
            if entry.generation == generation {
                tasks.remove(&req_id_clone);
            }
        }
    });

    tasks_lock.insert(req_id, crate::ai_task_manager::AiTaskEntry {
        generation,
        handle,
    });
    
    Ok(())
}

/// Cancels an ongoing generation request
#[command]
pub async fn ai_stop(state: tauri::State<'_, crate::ai_task_manager::AiTaskManager>, request_id: String) -> Result<(), String> {
    let entry_handle = {
        let mut tasks = state.tasks.lock().await;
        tasks.remove(&request_id)
    }; // Lock is dropped here

    if let Some(entry) = entry_handle {
        entry.handle.abort();
    }
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


