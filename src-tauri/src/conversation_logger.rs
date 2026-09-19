use crate::persona::PersonaConfig;
use std::path::{Path, PathBuf};

#[derive(Clone, PartialEq, Debug)]
pub struct SessionMetadata {
    pub persona_name: String,
    pub model: String,
    pub temperature: f32,
    pub num_ctx: u32,
    pub repeat_penalty: f32,
}

#[derive(Clone, Debug)]
pub struct SessionInfo {
    pub path: PathBuf,
    pub metadata: SessionMetadata,
}

pub type SessionLogState = std::sync::Arc<tauri::async_runtime::Mutex<Option<SessionInfo>>>;

pub fn new_session_log_state() -> SessionLogState {
    std::sync::Arc::new(tauri::async_runtime::Mutex::new(None))
}

/// Trả về (tên_user, tên_assistant) theo độ ưu tiên
pub fn resolve_display_names(persona: &PersonaConfig) -> (String, String) {
    let user_name = if !persona.user_address.is_empty() {
        persona.user_address.clone()
    } else if !persona.user_name.is_empty() {
        persona.user_name.clone()
    } else {
        "Người dùng".to_string()
    };

    let assistant_name = if !persona.assistant_name.is_empty() {
        persona.assistant_name.clone()
    } else {
        "AI Assistant".to_string()
    };

    // Capitalize first letter of user_name for better aesthetics in Markdown
    let mut c = user_name.chars();
    let user_name = match c.next() {
        None => String::new(),
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
    };

    (user_name, assistant_name)
}

/// Sinh đường dẫn an toàn không trùng lặp: phien_YYYY-MM-DD_HHmmss.md, hoặc _01, _02 nếu trùng
pub fn get_session_file_path(logs_dir: &Path, time: &chrono::DateTime<chrono::Local>) -> PathBuf {
    let base_name = time.format("phien_%Y-%m-%d_%H%M%S").to_string();
    let mut candidate = logs_dir.join(format!("{}.md", base_name));
    
    let mut counter = 1;
    while candidate.exists() {
        candidate = logs_dir.join(format!("{}_{:02}.md", base_name, counter));
        counter += 1;
        if counter > 99 {
            // Safety limit
            break;
        }
    }
    
    candidate
}

pub fn build_header(
    time: &chrono::DateTime<chrono::Local>,
    metadata: &SessionMetadata,
) -> String {
    format!(
        "# Phiên trò chuyện — {}\n\n\
        ## Cấu hình lúc bắt đầu\n\
        - Persona: {}\n\
        - Model: {}\n\
        - temperature: {}\n\
        - num_ctx: {}\n\
        - repeat_penalty: {}\n\n\
        ## Hội thoại\n\n",
        time.format("%Y-%m-%d %H:%M:%S"),
        metadata.persona_name,
        metadata.model,
        metadata.temperature,
        metadata.num_ctx,
        metadata.repeat_penalty
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_resolve_display_names() {
        let mut p = PersonaConfig {
            assistant_name: "".to_string(),
            creator: "".to_string(),
            creator_info: "".to_string(),
            user_name: "".to_string(),
            user_address: "".to_string(),
            self_address: "".to_string(),
            tone_instruction: "".to_string(),
            response_prefix: "".to_string(),
        };

        // Fallbacks
        let (u, a) = resolve_display_names(&p);
        assert_eq!(u, "Người dùng");
        assert_eq!(a, "AI Assistant");

        // user_name set
        p.user_name = "Nhật".to_string();
        let (u, _) = resolve_display_names(&p);
        assert_eq!(u, "Nhật");

        // user_address overrides user_name
        p.user_address = "xếp".to_string();
        let (u, _) = resolve_display_names(&p);
        assert_eq!(u, "Xếp"); // Tự động viết hoa chữ cái đầu

        // assistant name set
        p.assistant_name = "Chuki".to_string();
        let (_, a) = resolve_display_names(&p);
        assert_eq!(a, "Chuki");
    }

    #[test]
    fn test_get_session_file_path_no_conflict() {
        let dir = tempdir().unwrap();
        let time = chrono::Local::now();
        let path = get_session_file_path(dir.path(), &time);
        assert_eq!(
            path.file_name().unwrap().to_str().unwrap(),
            format!("{}.md", time.format("phien_%Y-%m-%d_%H%M%S"))
        );
    }

    #[test]
    fn test_get_session_file_path_conflict() {
        let dir = tempdir().unwrap();
        let time = chrono::Local::now();
        
        // Tạo file gốc để giả lập trùng lặp
        let base_name = time.format("phien_%Y-%m-%d_%H%M%S").to_string();
        let path1 = dir.path().join(format!("{}.md", base_name));
        fs::write(&path1, "").unwrap();

        // Kiểm tra _01
        let path2 = get_session_file_path(dir.path(), &time);
        assert_eq!(
            path2.file_name().unwrap().to_str().unwrap(),
            format!("{}_01.md", base_name)
        );
        fs::write(&path2, "").unwrap();

        // Kiểm tra _02
        let path3 = get_session_file_path(dir.path(), &time);
        assert_eq!(
            path3.file_name().unwrap().to_str().unwrap(),
            format!("{}_02.md", base_name)
        );
    }
}
