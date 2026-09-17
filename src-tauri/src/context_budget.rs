use crate::commands::AiChatMessage;
use crate::generation::GenerationConfig;
use std::cmp::max;

// Overhead cố định cho mỗi tin nhắn (ví dụ: thẻ bọc, ngắt dòng)
pub const OVERHEAD_PER_MESSAGE: u32 = 4;
pub const SAFETY_MARGIN_PERCENT: u32 = 10;
pub const MIN_SAFETY_MARGIN: u32 = 32;

/// Ước lượng bảo thủ (conservative estimate) bằng số ký tự / 3
pub fn estimate_message_tokens(content: &str) -> u32 {
    let char_count = content.chars().count() as f32;
    (char_count / 3.0).ceil() as u32 + OVERHEAD_PER_MESSAGE
}

/// Tính toán I (Input Budget) dựa trên num_ctx và num_predict
pub fn calculate_input_budget(num_ctx: u32, num_predict: u32) -> Result<u32, String> {
    if num_predict >= num_ctx {
        return Err("Lỗi cấu hình: num_predict không được lớn hơn hoặc bằng num_ctx".to_string());
    }

    let margin_by_percent = (num_ctx as f32 * (SAFETY_MARGIN_PERCENT as f32 / 100.0)).ceil() as u32;
    let safety_margin = max(MIN_SAFETY_MARGIN, margin_by_percent);

    let required = num_predict + safety_margin;
    if num_ctx <= required {
        return Err(
            "Lỗi cấu hình: num_ctx quá nhỏ không đủ cho num_predict và safety margin".to_string(),
        );
    }

    Ok(num_ctx - required)
}

/// Xác thực cấu trúc của mảng history từ frontend:
/// - Không có role system.
/// - Message cuối cùng phải là 'user'.
/// - Các message trước đó (nếu có) phải tạo thành các cặp 'user' -> 'assistant'.
pub fn validate_history_structure(history: &[AiChatMessage]) -> Result<(), String> {
    if history.is_empty() {
        return Ok(());
    }

    // Kiểm tra không có system
    if history.iter().any(|m| m.role == "system") {
        return Err("History chứa role 'system' từ frontend là không hợp lệ".to_string());
    }

    // Phần tử cuối cùng phải là user
    let last = history.last().unwrap();
    if last.role != "user" {
        return Err(format!(
            "Message cuối cùng phải có role 'user', nhưng lại nhận được '{}'",
            last.role
        ));
    }

    // Các phần tử trước đó phải là các cặp user -> assistant
    let history_before_last = &history[0..history.len() - 1];
    if history_before_last.len() % 2 != 0 {
        return Err(
            "Lịch sử không hợp lệ: Có message bị lẻ, không tạo thành cặp user-assistant hoàn chỉnh"
                .to_string(),
        );
    }

    let mut expect_user = true;
    for msg in history_before_last {
        if expect_user && msg.role != "user" {
            return Err("Lịch sử không hợp lệ: Lỗi thứ tự, mong đợi 'user'".to_string());
        }
        if !expect_user && msg.role != "assistant" {
            return Err("Lịch sử không hợp lệ: Lỗi thứ tự, mong đợi 'assistant'".to_string());
        }
        expect_user = !expect_user;
    }

    Ok(())
}

/// Cắt bớt history sao cho tổng số token ước lượng <= budget.
/// `history` bao gồm cả câu hỏi hiện tại (nằm ở cuối).
pub fn trim_history(
    system_msg: AiChatMessage,
    history: Vec<AiChatMessage>,
    config: &GenerationConfig,
) -> Result<(Vec<AiChatMessage>, u32, usize), String> {
    // 1. Validate cấu trúc history
    validate_history_structure(&history)?;

    if history.is_empty() {
        return Err("Request phải có current user message".to_string());
    }

    // 2. Tính budget I
    let budget_i = calculate_input_budget(config.num_ctx, config.num_predict)?;

    // 3. Phân tách history thành câu user hiện tại và phần lịch sử cũ (các cặp QA)
    let (current_user, qa_history) = history
        .split_last()
        .expect("history đã được kiểm tra không rỗng");

    // 4. Ước lượng token của system và current_user
    let sys_tokens = estimate_message_tokens(&system_msg.content);
    let curr_tokens = estimate_message_tokens(&current_user.content);

    if sys_tokens + curr_tokens > budget_i {
        return Err("Context quá nhỏ cho request hiện tại: System persona và User prompt đã vượt quá ngân sách đầu vào".to_string());
    }

    let mut budget_left = budget_i - sys_tokens - curr_tokens;
    let mut total_estimated_tokens = sys_tokens + curr_tokens;

    // 5. Tính toán các cặp QA từ mới nhất về cũ nhất
    let mut selected_pairs: Vec<(AiChatMessage, AiChatMessage)> = Vec::new();

    // qa_history có số lượng chẵn (do đã validate)
    // Duyệt từng cặp (User, Assistant)
    let chunks = qa_history.chunks_exact(2);
    // Duyệt ngược (mới nhất xử lý trước)
    for chunk in chunks.rev() {
        let user_msg = &chunk[0];
        let asst_msg = &chunk[1];

        let pair_tokens =
            estimate_message_tokens(&user_msg.content) + estimate_message_tokens(&asst_msg.content);

        if pair_tokens <= budget_left {
            // Còn đủ budget, thêm vào danh sách
            selected_pairs.push((user_msg.clone(), asst_msg.clone()));
            budget_left -= pair_tokens;
            total_estimated_tokens += pair_tokens;
        } else {
            // Giữ một đoạn history liên tục từ các lượt mới nhất.
            // Khi một cặp mới hơn không vừa, mọi cặp cũ hơn cũng bị loại.
            break;
        }
    }

    let mut selected_history = vec![];

    // selected_pairs chứa từ Mới -> Cũ. Cần đảo ngược lại Cũ -> Mới.
    selected_pairs.reverse();
    for (u, a) in selected_pairs {
        selected_history.push(u);
        selected_history.push(a);
    }

    // Đếm lại số bị drop (tổng số cặp ban đầu - số đã chọn)
    let total_pairs = qa_history.len() / 2;
    let dropped_turns = total_pairs - (selected_history.len() / 2);

    // 6. Ghép mảng cuối cùng
    let mut final_messages = vec![system_msg];
    final_messages.extend(selected_history);
    final_messages.push(current_user.clone());

    Ok((final_messages, total_estimated_tokens, dropped_turns))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_estimate_tokens() {
        // "ABC" -> 3 chars -> 3/3 = 1 + 4 = 5
        assert_eq!(estimate_message_tokens("ABC"), 5);

        // "A" -> 1 char -> 1/3 = 0.33 -> ceil = 1 + 4 = 5
        assert_eq!(estimate_message_tokens("A"), 5);

        // 9 chars -> 9/3 = 3 + 4 = 7
        assert_eq!(estimate_message_tokens("123456789"), 7);
    }

    #[test]
    fn test_calculate_budget() {
        let budget = calculate_input_budget(2048, 64).unwrap();
        // S = max(32, 2048*10% = 205) = 205
        // I = 2048 - 64 - 205 = 1779
        assert_eq!(budget, 1779);

        // num_predict >= num_ctx
        assert!(calculate_input_budget(2048, 2048).is_err());
        assert!(calculate_input_budget(128, 512).is_err());
    }

    #[test]
    fn test_validate_structure() {
        // OK: empty -> empty is checked in another flow, but function handles it
        assert!(validate_history_structure(&[]).is_ok());

        // OK: 1 user
        assert!(validate_history_structure(&[AiChatMessage {
            role: "user".to_string(),
            content: "hi".to_string()
        }])
        .is_ok());

        // OK: user, asst, user
        assert!(validate_history_structure(&[
            AiChatMessage {
                role: "user".to_string(),
                content: "hi".to_string()
            },
            AiChatMessage {
                role: "assistant".to_string(),
                content: "hello".to_string()
            },
            AiChatMessage {
                role: "user".to_string(),
                content: "hi2".to_string()
            }
        ])
        .is_ok());

        // ERR: end with assistant
        assert!(validate_history_structure(&[
            AiChatMessage {
                role: "user".to_string(),
                content: "hi".to_string()
            },
            AiChatMessage {
                role: "assistant".to_string(),
                content: "hello".to_string()
            }
        ])
        .is_err());

        // ERR: system in history
        assert!(validate_history_structure(&[
            AiChatMessage {
                role: "system".to_string(),
                content: "hi".to_string()
            },
            AiChatMessage {
                role: "user".to_string(),
                content: "hello".to_string()
            }
        ])
        .is_err());

        // ERR: bad pairs (user, user, user)
        assert!(validate_history_structure(&[
            AiChatMessage {
                role: "user".to_string(),
                content: "hi".to_string()
            },
            AiChatMessage {
                role: "user".to_string(),
                content: "hello".to_string()
            },
            AiChatMessage {
                role: "user".to_string(),
                content: "hello".to_string()
            }
        ])
        .is_err());
    }

    #[test]
    fn test_trim_history_logic() {
        let sys = AiChatMessage {
            role: "system".to_string(),
            content: "S".to_string(),
        }; // token = 1/3+4=5
        let curr = AiChatMessage {
            role: "user".to_string(),
            content: "U3".to_string(),
        }; // token = 2/3+4=5

        // Dùng config nhỏ để ép cắt
        let mut config = GenerationConfig::default();
        config.num_ctx = 300;
        config.num_predict = 10;
        // budget = 300 - 10 - max(32, 300*10%=30) = 258

        // Tạo history khổng lồ mỗi message tốn 100 tokens (96/3 + 4 = 36)
        // 96 * 3 = 288 chars => 288/3 + 4 = 100 tokens
        let mut big_content = String::new();
        for _ in 0..288 {
            big_content.push('A');
        }

        let h1_u = AiChatMessage {
            role: "user".to_string(),
            content: big_content.clone(),
        }; // 100 t
        let h1_a = AiChatMessage {
            role: "assistant".to_string(),
            content: big_content.clone(),
        }; // 100 t
        let h2_u = AiChatMessage {
            role: "user".to_string(),
            content: big_content.clone(),
        }; // 100 t
        let h2_a = AiChatMessage {
            role: "assistant".to_string(),
            content: big_content.clone(),
        }; // 100 t

        let history = vec![
            h1_u.clone(),
            h1_a.clone(),
            h2_u.clone(),
            h2_a.clone(),
            curr.clone(),
        ];

        // Budget = 258. Sys=5, Curr=5 => Left = 248.
        // H2 (cặp mới nhất) = 200 tokens. Thêm H2 => Left = 48.
        // H1 (cặp cũ) = 200 tokens > 48 => Bỏ H1.

        let (msgs, estimated, dropped) = trim_history(sys.clone(), history, &config).unwrap();

        assert_eq!(dropped, 1); // Bỏ 1 cặp H1
        assert_eq!(msgs.len(), 4); // sys, h2_u, h2_a, curr
        assert_eq!(msgs[1].content, h2_u.content); // Đảm bảo đúng h2
        assert_eq!(estimated, 5 + 200 + 5);
    }

    #[test]
    fn test_empty_history_is_rejected() {
        let system = AiChatMessage {
            role: "system".to_string(),
            content: "S".to_string(),
        };
        let config = GenerationConfig::default();
        assert!(trim_history(system, vec![], &config).is_err());
    }

    #[test]
    fn test_trim_keeps_contiguous_newest_suffix() {
        let system = AiChatMessage {
            role: "system".to_string(),
            content: "S".to_string(),
        };
        let mut config = GenerationConfig::default();
        config.num_ctx = 300;
        config.num_predict = 10;

        let mut large = String::new();
        for _ in 0..600 {
            large.push('A');
        }
        let small = "small".to_string();

        let history = vec![
            AiChatMessage {
                role: "user".to_string(),
                content: small.clone(),
            },
            AiChatMessage {
                role: "assistant".to_string(),
                content: small.clone(),
            },
            AiChatMessage {
                role: "user".to_string(),
                content: large.clone(),
            },
            AiChatMessage {
                role: "assistant".to_string(),
                content: large,
            },
            AiChatMessage {
                role: "user".to_string(),
                content: "current".to_string(),
            },
        ];

        let (messages, _, dropped) = trim_history(system, history, &config).unwrap();
        assert_eq!(dropped, 2);
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].role, "system");
        assert_eq!(messages[1].content, "current");
    }
}
