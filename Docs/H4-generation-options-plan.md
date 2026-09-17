# H4 — Generation Options cho Ollama

**Người thực hiện:** Dev AI  
**Người kiểm tra:** Codex  
**Tiền điều kiện:** H1, H2 và H3 đã được nghiệm thu Đạt  
**Phạm vi:** Quản lý và truyền các tham số sinh văn bản cho Ollama một cách cấu hình được.

## 1. Mục tiêu

Đưa các tham số generation ra khỏi giá trị ngầm của Ollama và cho phép cấu hình rõ ràng:

- temperature
- repeat_penalty
- num_ctx
- num_predict

Cùng một cấu hình hiệu lực phải được dùng cho cả ai_generate và ai_stream.

H4 chưa quyết định context window tối ưu cuối cùng và chưa triển khai token counting/history trimming; các việc đó thuộc H5.

## 2. Hiện trạng đã rà soát

- src-tauri/src/commands.rs đã có AiGenerateParams với request_id, max_tokens, temperature và stream.
- OllamaChatOptions hiện mới có temperature và num_predict.
- ai_generate và ai_stream đều tạo Ollama request riêng nhưng dùng cùng kiểu options.
- Frontend hiện gửi request_id và stream, chưa có UI chỉnh generation options.
- H3 đã có cơ chế đọc file cấu hình bên ngoài và fallback an toàn cho persona.

Dev phải tận dụng AiGenerateParams hiện có, không tạo một API generation thứ hai làm trùng logic.

## 3. Quyết định cấu hình

Tạo file cấu hình riêng cho generation, không trộn với persona:

- Tên biến môi trường: AI_TASKBAR_GENERATION_PATH
- Mặc định: generation.json trong app_config_dir của Tauri
- Không hardcode đường dẫn E:\\UI AI hoặc đường dẫn tài khoản Windows

Có thể dùng AI_TASKBAR_GENERATION_PATH để trỏ tới file trong workspace khi phát triển.

File mẫu placeholder:

    {
      "temperature": 0.2,
      "repeat_penalty": 1.1,
      "num_ctx": 2048,
      "num_predict": 64
    }

Đây là default đề xuất để giữ gần cấu hình benchmark hiện tại. Dev không tự ý tăng num_ctx hoặc num_predict trong H4.

## 4. Rust data model

Đề xuất tạo module riêng:

- src-tauri/src/generation.rs

Kiểu cấu hình:

    GenerationConfig {
        temperature: f32,
        repeat_penalty: f32,
        num_ctx: u32,
        num_predict: u32
    }

GenerationConfig cần có:

1. serde::Deserialize.
2. Default với các giá trị an toàn ở mục 3.
3. validate() kiểm tra giá trị hữu hạn và giới hạn.
4. load_generation_config(app) đọc file UTF-8 và parse JSON.
5. fallback_default_generation() khi file không tồn tại.

Không cache cấu hình trong H4. Đọc lại ở mỗi request để thay đổi file có hiệu lực từ request kế tiếp.

## 5. Validation

Giới hạn tối thiểu cần có:

- temperature: số hữu hạn, 0.0 đến 2.0.
- repeat_penalty: số hữu hạn, lớn hơn 0.0 và không vượt quá 3.0.
- num_ctx: số nguyên dương, tối thiểu 256; không tự tăng default vì H5 còn phải benchmark.
- num_predict: số nguyên dương; cần có giới hạn trên an toàn, đề xuất 2048 trong H4.

Giá trị NaN, vô cực, số âm, 0 không hợp lệ ở các trường cần số dương và giá trị vượt giới hạn phải trả lỗi.

Không tự động sửa giá trị sai về giá trị gần nhất. Trả lỗi rõ ràng để người dùng sửa file.

## 6. Chính sách file lỗi

- File không tồn tại: dùng default an toàn để chat vẫn hoạt động.
- JSON sai cú pháp: trả lỗi cấu hình lên frontend, không gửi request.
- File không đọc được: trả lỗi I/O, không dùng cấu hình cũ trong cache.
- Field sai kiểu hoặc thiếu field bắt buộc: trả lỗi validation hoặc dùng serde default theo schema đã thống nhất; phải báo rõ trong report.
- Không log prompt, user_info hoặc toàn bộ nội dung file.

Hành vi file lỗi phải nhất quán với chính sách H3: thiếu file được fallback, file tồn tại nhưng không hợp lệ thì fail-fast.

## 7. Mở rộng AiGenerateParams

Giữ nguyên các field đang có để không phá frontend:

- request_id
- temperature
- max_tokens
- stream

Quy tắc merge:

1. Đọc GenerationConfig làm giá trị nền.
2. Nếu params.temperature có giá trị, dùng nó làm override cho request hiện tại.
3. Nếu params.max_tokens có giá trị, ánh xạ sang num_predict cho request hiện tại.
4. repeat_penalty và num_ctx lấy từ file cấu hình trong H4.
5. Nếu sau này cần override hai field này, phải có thiết kế riêng và validation; không tự thêm ngoài phạm vi.

Không sửa model, URL hoặc request_id trong H4.

## 8. Ollama options contract

Mở rộng OllamaChatOptions để request gửi rõ các field:

    {
      "temperature": 0.2,
      "repeat_penalty": 1.1,
      "num_ctx": 2048,
      "num_predict": 64
    }

Yêu cầu:

- Cả ai_generate và ai_stream dùng cùng một hàm load/merge options.
- Không để ai_generate dùng default này còn ai_stream dùng default khác.
- Không bỏ qua options khi giá trị đến từ file.
- Không gửi thêm tham số không có trong contract.
- Nếu Ollama trả HTTP error, giữ nguyên cơ chế báo lỗi H1.

## 9. Tích hợp H1, H2 và H3

Thứ tự xử lý mỗi request:

1. Load generation config.
2. Validate và merge request params.
3. Load persona config.
4. Tạo system message persona.
5. Ghép history H2.
6. Gửi messages và options tới /api/chat.

Không đưa generation options vào committedMessages. Không đưa persona vào history. Không thay đổi streaming events, request_id, cancel hoặc cleanup.

## 10. Frontend

H4 không bắt buộc thêm panel UI chỉnh options.

Frontend chỉ cần:

- Giữ payload hiện tại tương thích.
- Không hardcode generation options mới trong TaskbarShell.tsx.
- Hiển thị lỗi cấu hình nếu backend trả lỗi.
- Không hiển thị user_info hoặc generation config trong event global.

Nếu muốn có UI chỉnh options, phải lập kế hoạch riêng; không tự mở rộng H4.

## 11. Kiểm thử tự động

Chạy:

    npm run lint
    npm run build
    cargo check
    cargo test

Unit test Rust tối thiểu:

1. Default config có đúng giá trị đề xuất.
2. Parse file đầy đủ.
3. Parse file sai kiểu hoặc sai cú pháp.
4. Mỗi giới hạn validation hoạt động.
5. Merge override temperature.
6. Merge max_tokens thành num_predict.
7. Cả generate và stream dùng options hợp nhất.
8. File thiếu fallback default.
9. File lỗi không dùng cache cũ.

## 12. Kiểm thử thủ công

1. Tạo generation.json với các giá trị hợp lệ.
2. Gửi một request và xác nhận Ollama nhận đủ bốn options.
3. Đổi temperature, gửi request mới và xác nhận giá trị mới được dùng.
4. Đổi repeat_penalty, num_ctx và num_predict; xác nhận từng giá trị được truyền.
5. Xoá file; xác nhận app dùng default an toàn.
6. Làm hỏng JSON; xác nhận UI báo lỗi và không gửi request.
7. Đặt giá trị vượt giới hạn; xác nhận validation chặn request.
8. Kiểm tra request có history H2 và persona H3 vẫn đúng thứ tự.
9. Kiểm tra streaming, stop và request_id không bị ảnh hưởng.
10. Xác nhận không có thay đổi AI Orb, radial menu hoặc system panels.

Việc đo RAM, tốc độ và quyết định num_ctx cuối cùng không làm trong H4; ghi nhận để H5.

## 13. Không làm trong H4

- Không triển khai token counting.
- Không trim hoặc tóm tắt history.
- Không tăng context mặc định để chạy thử tuỳ ý.
- Không thêm persona hoặc memory mới.
- Không đổi model/backend/port.
- Không thêm UI settings generation nếu chưa có kế hoạch riêng.
- Không benchmark kết luận hiệu năng mới thay cho H5.

## 14. Báo cáo Dev bắt buộc

Dev phải báo cáo riêng H4, không gộp H5:

- File đã sửa/thêm.
- Schema generation.json và giá trị default.
- Quy tắc resolve path và fallback.
- Quy tắc merge AiGenerateParams.
- Ví dụ options thực tế trong request.
- Kết quả validation file lỗi và file thiếu.
- Xác nhận generate/stream dùng cùng options.
- Kết quả lint, build, cargo check, cargo test.
- Kết quả manual test.
- Xác nhận chưa làm token budget/history trimming của H5.

Dev phải dừng sau báo cáo để Codex nghiệm thu.

## 15. Tiêu chí nghiệm thu H4

H4 chỉ đạt khi bốn options được cấu hình và truyền đúng cho cả generate/stream; giá trị sai bị chặn; file thiếu fallback đúng; file hỏng fail-fast; override hiện có hoạt động; H1, H2 và H3 không bị ảnh hưởng; không có thay đổi vượt sang H5; và toàn bộ kiểm thử đều pass.
