use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{AppHandle, Manager, State};
use tokio::sync::Mutex;

fn default_provider() -> String {
    "ollama".to_string()
}

/// Official H6 schema: `{ "provider": "ollama", "model": "qwen2.5:1.5b" }`
///
/// Migration path:
///   v0 (no provider, no model):     `{ "selected_model": "..." }`
///   v1 (partial H6, wrong schema):  `{ "selected_model": "...", "provider": "..." }`
///   v2 (current H6 final schema):   `{ "provider": "...", "model": "..." }`
///
/// `load_config_from_path` detects v0/v1 by checking whether the raw JSON
/// has a `"model"` key, and rewrites the file to v2 on migration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default = "default_provider")]
    pub provider: String,
    pub model: String,
}

impl AppConfig {
    pub fn validate(&self) -> Result<(), String> {
        match self.model.as_str() {
            "qwen2.5:1.5b" | "qwen2.5:3b" => {}
            _ => return Err("Model không nằm trong allowlist".into()),
        }
        match self.provider.as_str() {
            "ollama" | "llama.cpp" => Ok(()),
            _ => Err("Provider không hợp lệ (chỉ hỗ trợ ollama và llama.cpp)".into()),
        }
    }
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            provider: default_provider(),
            model: "qwen2.5:1.5b".to_string(),
        }
    }
}

pub type AppConfigState = Arc<Mutex<AppConfig>>;

fn get_config_path(app_handle: &AppHandle) -> Result<PathBuf, String> {
    app_handle
        .path()
        .app_data_dir()
        .map(|dir| dir.join("config.json"))
        .map_err(|e| format!("Failed to get app data dir: {}", e))
}

pub fn load_config(app_handle: &AppHandle) -> AppConfig {
    let config_path = match get_config_path(app_handle) {
        Ok(path) => path,
        Err(e) => {
            log::warn!("Could not get config path, using default config: {}", e);
            return AppConfig::default();
        }
    };
    load_config_from_path(&config_path)
}

fn load_config_from_path(config_path: &PathBuf) -> AppConfig {
    if config_path.exists() {
        let data = match fs::read_to_string(config_path) {
            Ok(d) => d,
            Err(e) => {
                log::warn!("Cannot read config file: {}. Using default.", e);
                return AppConfig::default();
            }
        };

        // Parse raw JSON first to detect schema version.
        let raw: serde_json::Value = match serde_json::from_str(&data) {
            Ok(v) => v,
            Err(_) => {
                // Broken JSON: preserve file so user can repair it.
                log::warn!(
                    "Config file has invalid JSON — using default in memory. \
                     Original file is preserved at {:?}.",
                    config_path
                );
                return AppConfig::default();
            }
        };

        let has_model_key = raw.get("model").is_some();

        if !has_model_key {
            // ---- Migration from v0/v1 schema ----
            // Extract model from `selected_model` (v0/v1) or use default.
            let model = raw
                .get("selected_model")
                .and_then(|v| v.as_str())
                .unwrap_or("qwen2.5:1.5b")
                .to_string();
            let provider = raw
                .get("provider")
                .and_then(|v| v.as_str())
                .unwrap_or("ollama")
                .to_string();

            let migrated = AppConfig { provider, model };
            if migrated.validate().is_ok() {
                log::info!(
                    "Config migrated to H6 schema: provider=\"{}\", model=\"{}\"",
                    migrated.provider,
                    migrated.model
                );
                let _ = save_config_to_path(config_path, &migrated);
                return migrated;
            } else {
                log::warn!(
                    "Migrated config failed validation (model={:?}). Resetting to default.",
                    migrated.model
                );
                let default_config = AppConfig::default();
                let _ = save_config_to_path(config_path, &default_config);
                return default_config;
            }
        }

        // ---- New v2 schema path ----
        match serde_json::from_str::<AppConfig>(&data) {
            Ok(config) => {
                if config.validate().is_ok() {
                    return config;
                }
                log::warn!("Config validation failed. Resetting to default.");
                let default_config = AppConfig::default();
                let _ = save_config_to_path(config_path, &default_config);
                default_config
            }
            Err(e) => {
                log::warn!("Failed to deserialize config: {}. Using default in memory.", e);
                AppConfig::default() // preserve file
            }
        }
    } else {
        // File does not exist: create with defaults.
        let default_config = AppConfig::default();
        let _ = save_config_to_path(config_path, &default_config);
        default_config
    }
}

pub fn save_config(app_handle: &AppHandle, config: &AppConfig) -> Result<(), String> {
    let config_path = get_config_path(app_handle)?;
    save_config_to_path(&config_path, config)
}

fn save_config_to_path(config_path: &PathBuf, config: &AppConfig) -> Result<(), String> {
    if let Some(parent) = config_path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Failed to create config dir: {}", e))?;
    }
    let data = serde_json::to_string_pretty(config)
        .map_err(|e| format!("Failed to serialize config: {}", e))?;
    fs::write(config_path, data).map_err(|e| format!("Failed to write config file: {}", e))?;
    Ok(())
}

#[tauri::command]
pub async fn get_config(state: State<'_, AppConfigState>) -> Result<AppConfig, String> {
    let config = state.lock().await;
    Ok(config.clone())
}

#[tauri::command]
pub async fn set_model_config(
    app_handle: AppHandle,
    state: State<'_, AppConfigState>,
    model: String,
) -> Result<(), String> {
    // Allowlist enforced here in addition to validate()
    if model != "qwen2.5:1.5b" && model != "qwen2.5:3b" {
        return Err("Model không hợp lệ. Chỉ chấp nhận qwen2.5:1.5b hoặc qwen2.5:3b.".into());
    }

    let mut config = state.lock().await;
    config.model = model;

    save_config(&app_handle, &config)?;
    Ok(())
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static TEST_COUNTER: AtomicUsize = AtomicUsize::new(0);

    fn get_temp_config_path() -> PathBuf {
        let mut dir = env::temp_dir();
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let counter = TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
        dir.push(format!("ai_taskbar_test_{}_{}", timestamp, counter));
        dir.push("config.json");
        dir
    }

    // -------------------------------------------------------------------------
    // Basic v2 schema tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_valid_model() {
        let path = get_temp_config_path();
        let config = AppConfig {
            provider: "ollama".to_string(),
            model: "qwen2.5:3b".to_string(),
        };
        assert!(save_config_to_path(&path, &config).is_ok());

        let loaded = load_config_from_path(&path);
        assert_eq!(loaded.model, "qwen2.5:3b");
        assert_eq!(loaded.provider, "ollama");
    }

    #[test]
    fn test_invalid_model_from_file() {
        let path = get_temp_config_path();
        // v2 schema with invalid model
        let invalid_json = r#"{"provider": "ollama", "model": "model-la-q"}"#;
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, invalid_json).unwrap();

        let loaded = load_config_from_path(&path);
        // Should fallback to default
        assert_eq!(loaded.model, "qwen2.5:1.5b");
    }

    #[test]
    fn test_broken_json() {
        let path = get_temp_config_path();
        let original_content = r#"{model: oops"#;
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, original_content).unwrap();

        let loaded = load_config_from_path(&path);
        assert_eq!(loaded.model, "qwen2.5:1.5b");
        // IMPORTANT: broken file must be preserved
        let still_broken = fs::read_to_string(&path).unwrap();
        assert_eq!(still_broken, original_content, "Broken JSON file must not be overwritten");
    }

    #[test]
    fn test_file_not_exists() {
        let path = get_temp_config_path();
        let _ = fs::remove_file(&path);

        let loaded = load_config_from_path(&path);
        assert_eq!(loaded.model, "qwen2.5:1.5b");
        assert_eq!(loaded.provider, "ollama");
        assert!(path.exists(), "Default config file must be created");
    }

    #[test]
    fn test_persistence_after_restart() {
        let path = get_temp_config_path();
        let config = AppConfig {
            provider: "ollama".to_string(),
            model: "qwen2.5:3b".to_string(),
        };
        assert!(save_config_to_path(&path, &config).is_ok());

        let loaded = load_config_from_path(&path);
        assert_eq!(loaded.model, "qwen2.5:3b");
        assert_eq!(loaded.provider, "ollama");
    }

    // -------------------------------------------------------------------------
    // Migration tests
    // -------------------------------------------------------------------------

    /// v0 schema: only `selected_model`, no provider, no model key.
    #[test]
    fn test_v0_migration_selected_model_only() {
        let path = get_temp_config_path();
        let old_json = r#"{"selected_model": "qwen2.5:3b"}"#;
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, old_json).unwrap();

        let loaded = load_config_from_path(&path);
        // Model must be preserved from selected_model
        assert_eq!(loaded.model, "qwen2.5:3b",
            "model must be migrated from selected_model");
        assert_eq!(loaded.provider, "ollama");

        // File must be rewritten to v2 schema
        let written = fs::read_to_string(&path).unwrap();
        let v: serde_json::Value = serde_json::from_str(&written).unwrap();
        assert!(v.get("model").is_some(), "Migrated file must have 'model' key");
        assert!(v.get("provider").is_some(), "Migrated file must have 'provider' key");
        assert!(v.get("selected_model").is_none(), "Migrated file must NOT have 'selected_model' key");
    }

    /// v1 schema: `selected_model` + `provider`, but no `model` key (partial H6).
    #[test]
    fn test_v1_partial_h6_migration() {
        let path = get_temp_config_path();
        let v1_json = r#"{"selected_model": "qwen2.5:1.5b", "provider": "ollama"}"#;
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, v1_json).unwrap();

        let loaded = load_config_from_path(&path);
        assert_eq!(loaded.model, "qwen2.5:1.5b");
        assert_eq!(loaded.provider, "ollama");

        // File rewritten to v2
        let written = fs::read_to_string(&path).unwrap();
        let v: serde_json::Value = serde_json::from_str(&written).unwrap();
        assert!(v.get("model").is_some());
        assert!(v.get("selected_model").is_none());
    }

    /// v0 with invalid model → reset to default, not crash.
    #[test]
    fn test_v0_migration_invalid_model_resets() {
        let path = get_temp_config_path();
        let bad_json = r#"{"selected_model": "malicious-model"}"#;
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, bad_json).unwrap();

        let loaded = load_config_from_path(&path);
        assert_eq!(loaded.model, "qwen2.5:1.5b");
    }

    /// v2 schema loads cleanly with no migration triggered.
    #[test]
    fn test_v2_loads_cleanly() {
        let path = get_temp_config_path();
        let config = AppConfig {
            provider: "ollama".to_string(),
            model: "qwen2.5:1.5b".to_string(),
        };
        save_config_to_path(&path, &config).unwrap();

        let loaded = load_config_from_path(&path);
        assert_eq!(loaded.model, "qwen2.5:1.5b");
        assert_eq!(loaded.provider, "ollama");
    }

    /// Broken JSON file must not be overwritten; default returned in memory only.
    #[test]
    fn test_broken_json_resets_to_default_in_memory_only() {
        let path = get_temp_config_path();
        let broken = r#"{"provider": "ollama", "model": "qwen2.5:3b", BROKEN"#;
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, broken).unwrap();

        let loaded = load_config_from_path(&path);
        assert_eq!(loaded.model, "qwen2.5:1.5b");
        let on_disk = fs::read_to_string(&path).unwrap();
        assert_eq!(on_disk, broken, "Broken JSON file must not be overwritten");
    }
}
