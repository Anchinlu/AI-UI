use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tauri::Manager;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct GenerationConfig {
    pub temperature: f32,
    pub repeat_penalty: f32,
    pub num_ctx: u32,
    pub num_predict: u32,
}

impl Default for GenerationConfig {
    fn default() -> Self {
        Self {
            temperature: 0.2,
            repeat_penalty: 1.1,
            num_ctx: 4096,
            num_predict: 1024,
        }
    }
}

impl GenerationConfig {
    /// Kiểm tra các giới hạn giá trị an toàn
    pub fn validate(&self) -> Result<(), String> {
        if !self.temperature.is_finite() || self.temperature < 0.0 || self.temperature > 2.0 {
            return Err("temperature phải nằm trong khoảng [0.0, 2.0]".to_string());
        }
        if !self.repeat_penalty.is_finite()
            || self.repeat_penalty <= 0.0
            || self.repeat_penalty > 3.0
        {
            return Err("repeat_penalty phải lớn hơn 0 và không vượt quá 3.0".to_string());
        }
        if self.num_ctx < 256 || self.num_ctx > 8192 {
            return Err("num_ctx phải nằm trong khoảng [256, 8192]".to_string());
        }
        if self.num_predict == 0 || self.num_predict > 2048 {
            return Err("num_predict phải lớn hơn 0 và không vượt quá 2048".to_string());
        }
        Ok(())
    }
}

/// Fallback cấu hình mặc định an toàn
pub fn fallback_default_generation() -> GenerationConfig {
    GenerationConfig::default()
}

/// Tìm đường dẫn generation theo thứ tự:
/// 1. Biến môi trường AI_TASKBAR_GENERATION_PATH
/// 2. Thư mục config/ cạnh Cargo.toml (chỉ trong debug build)
/// 3. app_config_dir() của Tauri (production)
pub fn resolve_generation_path(app: &tauri::AppHandle) -> PathBuf {
    // 1. Biến môi trường override
    if let Ok(path_str) = std::env::var("AI_TASKBAR_GENERATION_PATH") {
        let p = PathBuf::from(path_str);
        if p.exists() {
            log::debug!("Generation source resolved: env var");
            return p;
        }
    }

    // 2. Dev config (chỉ trong debug build, không chạy ở production)
    #[cfg(debug_assertions)]
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dev_path) = crate::persona::resolve_dev_config_path(&exe, "generation.json") {
            log::debug!("Generation source resolved: dev config");
            return dev_path;
        }
    }

    // 3. Production: app_config_dir
    log::debug!("Generation source resolved: app_config_dir");
    app.path()
        .app_config_dir()
        .unwrap_or_default()
        .join("generation.json")
}

/// Load và parse JSON generation
pub fn load_generation_config(app: &tauri::AppHandle) -> Result<GenerationConfig, String> {
    let path = resolve_generation_path(app);
    if !path.exists() {
        return Err("NotFound".to_string());
    }

    let content =
        fs::read_to_string(&path).map_err(|e| format!("Lỗi I/O khi đọc file generation: {}", e))?;

    // Serde mặc định sẽ lỗi nếu thiếu bất kì field nào vì ta không dùng #[serde(default)]
    let config: GenerationConfig = serde_json::from_str(&content)
        .map_err(|e| format!("Lỗi cấu trúc/cú pháp JSON generation: {}", e))?;

    // Validate config được cấu hình trong file
    config.validate()?;

    Ok(config)
}

/// Override field theo `AiGenerateParams` và validate lại
pub fn merge_with_params(
    mut config: GenerationConfig,
    param_temp: Option<f32>,
    param_max_tokens: Option<u32>,
) -> Result<GenerationConfig, String> {
    if let Some(t) = param_temp {
        config.temperature = t;
    }
    if let Some(m) = param_max_tokens {
        config.num_predict = m;
    }

    // Yêu cầu quan trọng của Codex: Validate lại SAU KHI override
    config.validate()?;
    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_values() {
        let config = GenerationConfig::default();
        assert_eq!(config.temperature, 0.2);
        assert_eq!(config.repeat_penalty, 1.1);
        assert_eq!(config.num_ctx, 4096);
        assert_eq!(config.num_predict, 1024);
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_missing_field_parsing() {
        let json = r#"{ "temperature": 0.5, "num_ctx": 1024 }"#; // Thiếu repeat_penalty và num_predict
        let result: Result<GenerationConfig, _> = serde_json::from_str(json);
        assert!(result.is_err()); // Phải fail-fast nếu thiếu field
    }

    #[test]
    fn test_validation_limits() {
        let mut config = GenerationConfig::default();

        // Temp > 2.0
        config.temperature = 2.1;
        assert!(config.validate().is_err());

        // num_ctx > 8192
        config.temperature = 0.5;
        config.num_ctx = 10000;
        assert!(config.validate().is_err());

        // num_predict = 0
        config.num_ctx = 2048;
        config.num_predict = 0;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_merge_with_params() {
        let config = GenerationConfig::default();

        let merged = merge_with_params(config, Some(0.8), Some(128)).unwrap();
        assert_eq!(merged.temperature, 0.8);
        assert_eq!(merged.num_predict, 128);
        assert_eq!(merged.num_ctx, 4096); // field này giữ nguyên
    }

    #[test]
    fn test_merge_invalid_override() {
        let config = GenerationConfig::default();

        // Frontend cố tình truyền max_tokens quá lớn
        let merged = merge_with_params(config, None, Some(9999));
        assert!(merged.is_err()); // Phải fail validation sau override
    }
}
