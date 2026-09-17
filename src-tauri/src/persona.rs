use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tauri::Manager;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PersonaConfig {
    #[serde(default)]
    pub assistant_name: String,
    #[serde(default)]
    pub creator: String,
    #[serde(default)]
    pub user_name: String,
    #[serde(default)]
    pub user_info: String,
    #[serde(default)]
    pub tone_instruction: String,
}

impl PersonaConfig {
    /// Validates the lengths of the config fields.
    pub fn validate(&self) -> Result<(), String> {
        if self.assistant_name.chars().count() > 80 {
            return Err("assistant_name exceeds 80 characters".to_string());
        }
        if self.creator.chars().count() > 120 {
            return Err("creator exceeds 120 characters".to_string());
        }
        if self.user_name.chars().count() > 120 {
            return Err("user_name exceeds 120 characters".to_string());
        }
        if self.user_info.chars().count() > 2000 {
            return Err("user_info exceeds 2000 characters".to_string());
        }
        if self.tone_instruction.chars().count() > 2000 {
            return Err("tone_instruction exceeds 2000 characters".to_string());
        }
        Ok(())
    }
}

/// System message trung tính mặc định
pub fn fallback_neutral_persona() -> String {
    "You are a helpful assistant.".to_string()
}

/// Tìm đường dẫn persona theo thứ tự: biến môi trường -> thư mục config ứng dụng
pub fn resolve_persona_path(app: &tauri::AppHandle) -> PathBuf {
    if let Ok(path_str) = std::env::var("AI_TASKBAR_PERSONA_PATH") {
        let p = PathBuf::from(path_str);
        if p.exists() {
            return p;
        }
    }
    app.path().app_config_dir().unwrap_or_default().join("persona.json")
}

/// Load và parse JSON persona
pub fn load_persona(app: &tauri::AppHandle) -> Result<PersonaConfig, String> {
    let path = resolve_persona_path(app);
    if !path.exists() {
        // Trả về lỗi NotFound để commands.rs dùng fallback (không in ra lỗi quá đáng sợ)
        return Err("NotFound".to_string());
    }

    let content = fs::read_to_string(&path)
        .map_err(|e| format!("Lỗi I/O khi đọc file persona: {}", e))?;

    let config: PersonaConfig = serde_json::from_str(&content)
        .map_err(|e| format!("Lỗi cú pháp JSON persona: {}", e))?;

    config.validate()?;

    Ok(config)
}

/// Render system message thông minh bỏ qua các trường trống
pub fn build_system_message(config: &PersonaConfig) -> String {
    let mut parts = Vec::new();

    if !config.assistant_name.trim().is_empty() {
        parts.push(format!("You are {}.", config.assistant_name.trim()));
    } else {
        parts.push("You are a helpful assistant.".to_string());
    }

    if !config.creator.trim().is_empty() {
        parts.push(format!("Created by {}.", config.creator.trim()));
    }

    if !config.user_name.trim().is_empty() {
        parts.push(format!("The current user is {}.", config.user_name.trim()));
    }

    if !config.user_info.trim().is_empty() {
        parts.push(format!("User information: {}.", config.user_info.trim()));
    }

    if !config.tone_instruction.trim().is_empty() {
        parts.push(format!("Response style: {}.", config.tone_instruction.trim()));
    }

    parts.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_system_message_full() {
        let config = PersonaConfig {
            assistant_name: "AI Assistant".to_string(),
            creator: "Anchin".to_string(),
            user_name: "Alice".to_string(),
            user_info: "Developer".to_string(),
            tone_instruction: "Be concise".to_string(),
        };
        let msg = build_system_message(&config);
        assert_eq!(
            msg,
            "You are AI Assistant.\nCreated by Anchin.\nThe current user is Alice.\nUser information: Developer.\nResponse style: Be concise."
        );
    }

    #[test]
    fn test_build_system_message_empty() {
        let config = PersonaConfig {
            assistant_name: "".to_string(),
            creator: "".to_string(),
            user_name: "".to_string(),
            user_info: "".to_string(),
            tone_instruction: "".to_string(),
        };
        let msg = build_system_message(&config);
        assert_eq!(msg, "You are a helpful assistant.");
    }

    #[test]
    fn test_validate_length() {
        let mut config = PersonaConfig {
            assistant_name: "".to_string(),
            creator: "".to_string(),
            user_name: "".to_string(),
            user_info: "".to_string(),
            tone_instruction: "".to_string(),
        };
        
        // Tạo chuỗi 81 ký tự
        let long_str: String = std::iter::repeat('A').take(81).collect();
        config.assistant_name = long_str;
        assert!(config.validate().is_err());
    }
}
