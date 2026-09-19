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
    pub model: String,   // effective_model
    pub requested_model: String,
    pub is_ready: bool,
    pub fallback: bool,
    pub reason: Option<String>,
    pub ram_usage: Option<u32>,
}

#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub struct AiGenerateParams {
    pub request_id: String,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
    pub stream: Option<bool>,
}

async fn get_requested_model(config_state: &tauri::State<'_, crate::settings::AppConfigState>) -> String {
    if let Ok(env_model) = std::env::var("OLLAMA_MODEL") {
        if env_model == "qwen2.5:1.5b" || env_model == "qwen2.5:3b" {
            return env_model;
        } else {
            log::warn!("OLLAMA_MODEL='{}' không hợp lệ. Bỏ qua và dùng config.", env_model);
        }
    }
    let config = config_state.lock().await;
    config.model.clone()
}

// OllamaTagResponse, OllamaModel, OllamaChatRequest, OllamaChatOptions,
// OllamaChatResponseMessage, OllamaChatResponse, and resolve_effective_model
// have been removed. Their responsibilities now live in:
//   crate::provider::ollama::OllamaProvider  (HTTP)
//   crate::provider::policy::ModelPolicy     (model selection & fallback)

#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub struct AiChatMessage {
    pub role: String,
    pub content: String,
}



fn get_effective_config(app: &tauri::AppHandle, params: &AiGenerateParams) -> Result<crate::generation::GenerationConfig, String> {
    let base_config = match crate::generation::load_generation_config(app) {
        Ok(c) => c,
        Err(e) => {
            if e == "NotFound" {
                crate::generation::fallback_default_generation()
            } else {
                return Err(format!("Lỗi cấu hình Generation: {}", e));
            }
        }
    };

    crate::generation::merge_with_params(
        base_config,
        params.temperature,
        params.max_tokens,
    )
}

fn build_chat_request_data(
    app: &tauri::AppHandle,
    history: Vec<AiChatMessage>,
    config: crate::generation::GenerationConfig,
) -> Result<(Vec<crate::provider::ChatMessage>, crate::provider::GenOptions, String), String> {
    // 2. Get system message
    let (system_content, prefix) = match crate::persona::load_persona(app) {
        Ok(persona_config) => {
            (crate::persona::build_system_message(&persona_config), persona_config.response_prefix)
        },
        Err(e) => {
            if e == "NotFound" {
                (crate::persona::fallback_neutral_persona(), String::new())
            } else {
                return Err(format!("Lỗi cấu hình Persona: {}", e));
            }
        }
    };
    let system_msg = crate::provider::ChatMessage {
        role: "system".to_string(),
        content: system_content,
    };

    // 3. Convert history to provider ChatMessage
    let history_converted: Vec<crate::provider::ChatMessage> = history
        .into_iter()
        .map(|m| crate::provider::ChatMessage { role: m.role, content: m.content })
        .collect();

    // 4. Trim history (context budget)
    let ai_system = AiChatMessage { role: system_msg.role.clone(), content: system_msg.content.clone() };
    let ai_history: Vec<AiChatMessage> = history_converted.iter()
        .map(|m| AiChatMessage { role: m.role.clone(), content: m.content.clone() })
        .collect();
    let (trimmed_ai, estimated, dropped) = crate::context_budget::trim_history(ai_system, ai_history, &config)?;
    if dropped > 0 {
        log::warn!("Context Budget: Đã cắt {} cặp hội thoại cũ. Ước lượng token: {}", dropped, estimated);
    } else {
        log::info!("Context Budget: Không cắt cặp hội thoại nào. Ước lượng token: {}", estimated);
    }

    // 5. Convert trimmed messages back to provider::ChatMessage
    let trimmed: Vec<crate::provider::ChatMessage> = trimmed_ai
        .into_iter()
        .map(|m| crate::provider::ChatMessage { role: m.role, content: m.content })
        .collect();

    // 6. Build GenOptions
    let options = crate::provider::GenOptions {
        temperature:    config.temperature,
        num_predict:    config.num_predict,
        repeat_penalty: config.repeat_penalty,
        num_ctx:        config.num_ctx,
    };

    Ok((trimmed, options, prefix))
}

/// Checks the status of the local AI server
#[command]
pub async fn ai_get_status(
    _app: tauri::AppHandle,
    registry: tauri::State<'_, crate::provider::ProviderRegistry>,
    config_state: tauri::State<'_, crate::settings::AppConfigState>,
) -> Result<AiServerStatus, String> {
    let provider_name = {
        let config = config_state.lock().await;
        config.provider.clone()
    };

    // Environment override for model
    let requested_model = get_requested_model(&config_state).await;

    let provider = registry.get(&provider_name).await.map_err(|e| e.to_string())?;
    let policy = crate::provider::policy::ModelPolicy::new(
        requested_model.clone(), "qwen2.5:1.5b"
    );
    let resolved = policy.resolve(provider.as_ref()).await.map_err(|e| e.to_string())?;

    Ok(AiServerStatus {
        backend: provider.provider_name().to_string(),
        model: resolved.effective_model,
        requested_model,
        is_ready: true,
        fallback: resolved.fallback,
        reason: resolved.reason,
        ram_usage: None,
    })
}

/// Generates a complete text response (blocking until done)
#[command]
pub async fn ai_generate(
    app: tauri::AppHandle, 
    registry: tauri::State<'_, crate::provider::ProviderRegistry>, 
    config_state: tauri::State<'_, crate::settings::AppConfigState>,
    messages: Vec<AiChatMessage>, 
    params: AiGenerateParams
) -> Result<String, String> {
    let provider_name = {
        let config = config_state.lock().await;
        config.provider.clone()
    };
    let requested_model = get_requested_model(&config_state).await;

    let config = get_effective_config(&app, &params)?;

    let provider = registry.get(&provider_name).await.map_err(|e| e.to_string())?;
    let policy = crate::provider::policy::ModelPolicy::new(
        requested_model.clone(), "qwen2.5:1.5b"
    );
    let resolved = policy.resolve(provider.as_ref()).await.map_err(|e| e.to_string())?;

    if resolved.fallback {
        use tauri::Emitter;
        let _ = app.emit("ai-model-resolved", serde_json::json!({
            "requested_model": &requested_model,
            "effective_model": &resolved.effective_model,
            "fallback": true,
            "reason": &resolved.reason,
        }));
    }

    let (messages_out, options_out, prefix) = build_chat_request_data(&app, messages, config)?;

    let chat_request = crate::provider::ChatRequest {
        model:      resolved.effective_model,
        messages:   messages_out,
        options:    options_out,
        request_id: params.request_id.clone(),
    };

    let mut final_text = provider.chat(chat_request).await.map_err(|e| e.to_string())?;

    // Apply prefix (same logic as before)
    let target = prefix.trim();
    if !target.is_empty() {
        if final_text.trim_start().starts_with(target) {
            final_text = final_text.trim_start()[target.len()..].trim_start().to_string();
        }
        if !final_text.is_empty() {
            final_text = format!("{} {}", target, final_text);
        }
    }

    Ok(final_text)
}

#[command]
pub async fn ai_stream(
    app: tauri::AppHandle,
    window: tauri::Window, 
    state: tauri::State<'_, crate::ai_task_manager::AiTaskManager>, 
    registry: tauri::State<'_, crate::provider::ProviderRegistry>, 
    config_state: tauri::State<'_, crate::settings::AppConfigState>,
    messages: Vec<AiChatMessage>, 
    params: AiGenerateParams
) -> Result<(), String> {
    let provider_name = {
        let config = config_state.lock().await;
        config.provider.clone()
    };
    let requested_model = get_requested_model(&config_state).await;

    let config = get_effective_config(&app, &params)?;

    let provider = registry.get(&provider_name).await.map_err(|e| e.to_string())?;
    let policy = crate::provider::policy::ModelPolicy::new(
        requested_model.clone(), "qwen2.5:1.5b"
    );
    let resolved = policy.resolve(provider.as_ref()).await.map_err(|e| e.to_string())?;

    if resolved.fallback {
        let _ = window.emit("ai-model-resolved", serde_json::json!({
            "requested_model": &requested_model,
            "effective_model": &resolved.effective_model,
            "fallback": true,
            "reason": &resolved.reason,
        }));
    }

    let (messages_out, options_out, prefix) = build_chat_request_data(&app, messages, config)?;
    let req_id = params.request_id.clone();

    let chat_request = crate::provider::ChatRequest {
        model:      resolved.effective_model,
        messages:   messages_out,
        options:    options_out,
        request_id: req_id.clone(),
    };

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

    let tasks_clone = state.tasks.clone();
    let req_id_clone = req_id.clone();
    let window_clone = window.clone();

    // Lock BEFORE spawning to prevent the task from cleaning up before insertion is done
    let mut tasks_lock = state.tasks.lock().await;

    let handle = tauri::async_runtime::spawn(async move {
        let mut stripper = PrefixStripper::new(prefix);
        let req_id_inner = req_id_clone.clone();

        // Build on_chunk closure: applies prefix-stripping and emits events
        let mut on_chunk = move |content: String, is_done: bool| {
            if let Some(text) = stripper.process(&content, is_done) {
                let _ = window_clone.emit("ai-stream-chunk", serde_json::json!({
                    "request_id": &req_id_inner,
                    "text": text,
                    "done": is_done,
                }));
            } else if is_done {
                // Stripper returned None but stream is done — emit done marker
                // so frontend cleanup always fires (matches previous behaviour).
                let _ = window_clone.emit("ai-stream-chunk", serde_json::json!({
                    "request_id": &req_id_inner,
                    "text": "",
                    "done": true,
                }));
            }
        };

        match provider.chat_stream(chat_request, &mut on_chunk).await {
            Ok(()) => {}
            Err(e) => {
                let _ = window.emit("ai-stream-error", serde_json::json!({
                    "request_id": &req_id_clone,
                    "error": e.to_string(),
                }));
            }
        }

        // Cleanup: remove task entry only if generation matches (avoids
        // removing a newer request that was registered after this one started).
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
// CONVERSATION LOGGER API (Giai đoạn 8)
// ==========================================

#[command]
pub async fn append_conversation_turn(
    app: tauri::AppHandle,
    state: tauri::State<'_, crate::conversation_logger::SessionLogState>,
    registry: tauri::State<'_, crate::provider::ProviderRegistry>,
    config_state: tauri::State<'_, crate::settings::AppConfigState>,
    user_message: String,
    assistant_message: String,
    duration_ms: Option<u64>,
) -> Result<(), ()> {
    use crate::conversation_logger::{SessionInfo, SessionMetadata, resolve_display_names, get_session_file_path, build_header};
    use crate::persona::load_persona;
    use crate::generation::load_generation_config;
    use tokio::io::AsyncWriteExt;

    // Resolve effective model using provider layer
    let provider_name = {
        let config = config_state.lock().await;
        config.provider.clone()
    };
    let requested_model = get_requested_model(&config_state).await;
    
    let gen_config = load_generation_config(&app).unwrap_or_else(|_| crate::generation::GenerationConfig {
        temperature: 0.7,
        num_ctx: 4096,
        repeat_penalty: 1.1,
        num_predict: 1024,
    });
    
    let effective_model = if let Ok(provider) = registry.get(&provider_name).await {
        let policy = crate::provider::policy::ModelPolicy::new(
            requested_model.clone(), "qwen2.5:1.5b"
        );
        match policy.resolve(provider.as_ref()).await {
            Ok(r) => r.effective_model,
            Err(_) => requested_model.clone(),
        }
    } else {
        requested_model.clone()
    };

    // Load configs
    let persona_config = load_persona(&app).unwrap_or_else(|_| crate::persona::PersonaConfig {
        assistant_name: "".to_string(),
        creator: "".to_string(),
        creator_info: "".to_string(),
        user_name: "".to_string(),
        user_address: "".to_string(),
        self_address: "".to_string(),
        tone_instruction: "".to_string(),
        response_prefix: "".to_string(),
    });
    
    // (gen_config is already loaded above)

    let current_meta = SessionMetadata {
        persona_name: persona_config.assistant_name.clone(),
        model: effective_model,
        temperature: gen_config.temperature,
        num_ctx: gen_config.num_ctx,
        repeat_penalty: gen_config.repeat_penalty,
    };

    let (user_name, assistant_name) = resolve_display_names(&persona_config);
    let time = chrono::Local::now();
    let time_str = time.format("%H:%M:%S");

    let duration_str = match duration_ms {
        Some(ms) => format!("(⏱ {:.1}s)", ms as f64 / 1000.0),
        None => String::new(),
    };

    let turn_content = format!(
        "### [{}] {}\n{}\n\n### [{}] {} {}\n{}\n\n",
        time_str, user_name, user_message,
        time_str, assistant_name, duration_str, assistant_message
    );

    let mut session_lock = state.lock().await;

    let path = match session_lock.as_mut() {
        Some(session) => {
            // Check meta changes
            let mut prefix = String::new();
            if session.metadata != current_meta {
                if session.metadata.model != current_meta.model && 
                   session.metadata.persona_name == current_meta.persona_name &&
                   session.metadata.temperature == current_meta.temperature &&
                   session.metadata.num_ctx == current_meta.num_ctx &&
                   session.metadata.repeat_penalty == current_meta.repeat_penalty {
                    // Only model changed
                    prefix = format!(
                        "> ⚠️ {} — Đổi model sang {}\n\n",
                        time_str, current_meta.model
                    );
                } else {
                    prefix = format!(
                        "> ⚠️ {} — Cấu hình đã thay đổi: Persona={}, Model={}, T={}, ctx={}\n\n",
                        time_str, current_meta.persona_name, current_meta.model, current_meta.temperature, current_meta.num_ctx
                    );
                }
                session.metadata = current_meta;
            }
            let file_path = session.path.clone();
            (file_path, format!("{}{}", prefix, turn_content))
        }
        None => {
            // Khởi tạo file mới
            if let Ok(app_data_dir) = app.path().app_data_dir() {
                let logs_dir = app_data_dir.join("logs");
                if let Err(e) = tokio::fs::create_dir_all(&logs_dir).await {
                    log::error!("Không thể tạo thư mục logs: {}", e);
                    return Ok(());
                }
                
                let file_path = get_session_file_path(&logs_dir, &time);
                let header = build_header(&time, &current_meta);
                
                *session_lock = Some(SessionInfo {
                    path: file_path.clone(),
                    metadata: current_meta,
                });
                
                (file_path, format!("{}{}", header, turn_content))
            } else {
                log::error!("Không tìm thấy app_data_dir");
                return Ok(());
            }
        }
    };

    // Ghi file (best-effort)
    match tokio::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path.0)
        .await 
    {
        Ok(mut file) => {
            if let Err(e) = file.write_all(path.1.as_bytes()).await {
                log::error!("Lỗi ghi file log (write_all): {}", e);
            }
            if let Err(e) = file.flush().await {
                log::error!("Lỗi flush file log: {}", e);
            }
        }
        Err(e) => {
            log::error!("Lỗi mở file log để ghi: {}", e);
        }
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

struct PrefixStripper {
    pub target_prefix: String,
    pub generated_so_far: String,
    pub state: u8, // 0: matching, 1: mismatched, 2: matched, 3: matched and trimming trailing spaces
    pub has_emitted_prefix: bool,
}

impl PrefixStripper {
    pub fn new(target_prefix: String) -> Self {
        let state = if target_prefix.trim().is_empty() { 2 } else { 0 };
        Self {
            target_prefix: target_prefix.trim().to_string(),
            generated_so_far: String::new(),
            state,
            has_emitted_prefix: false,
        }
    }

    pub fn process(&mut self, text: &str, is_done: bool) -> Option<String> {
        let mut emit_text = text;
        if self.state == 3 {
            emit_text = emit_text.trim_start();
            if !emit_text.is_empty() {
                self.state = 2; // finished trimming
            }
        }

        if self.state == 2 || self.state == 1 {
            if emit_text.is_empty() {
                if is_done && self.state == 2 && !self.has_emitted_prefix {
                    return self.prepend_prefix_if_needed("");
                }
                return None;
            }
            return self.prepend_prefix_if_needed(emit_text);
        }

        // state == 0
        self.generated_so_far.push_str(text);
        let trimmed = self.generated_so_far.trim_start();
        
        if self.target_prefix.starts_with(trimmed) {
            if self.target_prefix == trimmed {
                self.state = 3; // matched perfectly, now trim following spaces
            }
            
            if is_done {
                if trimmed.is_empty() {
                    return None;
                } else if self.state == 3 {
                    return self.prepend_prefix_if_needed("");
                } else {
                    self.state = 1;
                    let so_far = self.generated_so_far.clone();
                    return self.prepend_prefix_if_needed(&so_far);
                }
            }
            
            None
        } else if trimmed.starts_with(&self.target_prefix) {
            self.state = 2; // already passed prefix and trimmed spaces
            let remainder = &trimmed[self.target_prefix.len()..];
            let remainder = remainder.trim_start().to_string();
            if remainder.is_empty() && !is_done {
                self.state = 3; // wait for next chunks to trim spaces
                None
            } else {
                self.prepend_prefix_if_needed(&remainder)
            }
        } else {
            self.state = 1;
            let so_far = self.generated_so_far.clone();
            self.prepend_prefix_if_needed(&so_far)
        }
    }

    fn prepend_prefix_if_needed(&mut self, content: &str) -> Option<String> {
        let mut out = String::new();
        if !self.has_emitted_prefix && !self.target_prefix.is_empty() {
            if !content.is_empty() {
                out.push_str(&self.target_prefix);
                out.push_str(" ");
                self.has_emitted_prefix = true;
            }
        }
        out.push_str(content);
        if out.is_empty() { None } else { Some(out) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prefix_stripper_perfect_match() {
        let mut stripper = PrefixStripper::new("Báo cáo:".to_string());
        assert_eq!(stripper.process("Báo cáo:", false), None); // exact match
        assert_eq!(stripper.process("  Nội dung", true), Some("Báo cáo: Nội dung".to_string()));
    }

    #[test]
    fn test_prefix_stripper_multi_chunk() {
        let mut stripper = PrefixStripper::new("Báo cáo:".to_string());
        assert_eq!(stripper.process("Báo ", false), None); // partial
        assert_eq!(stripper.process("cáo:", false), None); // exact now
        assert_eq!(stripper.process(" \n\n", false), None); // trailing spaces
        assert_eq!(stripper.process("Nội dung", true), Some("Báo cáo: Nội dung".to_string()));
    }

    #[test]
    fn test_prefix_stripper_mismatch() {
        let mut stripper = PrefixStripper::new("Báo cáo:".to_string());
        assert_eq!(stripper.process("Báo ", false), None); // partial
        assert_eq!(stripper.process("đốm", true), Some("Báo cáo: Báo đốm".to_string()));
    }

    #[test]
    fn test_prefix_stripper_no_content() {
        let mut stripper = PrefixStripper::new("Báo cáo:".to_string());
        assert_eq!(stripper.process("", true), None);
    }

    #[test]
    fn test_prefix_stripper_abrupt_end_partial() {
        let mut stripper = PrefixStripper::new("Báo cáo:".to_string());
        assert_eq!(stripper.process("Báo", true), Some("Báo cáo: Báo".to_string()));
    }

    #[test]
    fn test_prefix_stripper_abrupt_end_perfect() {
        let mut stripper = PrefixStripper::new("Báo cáo:".to_string());
        assert_eq!(stripper.process("Báo cáo:", true), None);
    }

    #[test]
    fn test_prefix_stripper_overshoot() {
        let mut stripper = PrefixStripper::new("Báo cáo:".to_string());
        assert_eq!(stripper.process("Báo cáo: Nội dung", true), Some("Báo cáo: Nội dung".to_string()));
    }
}
