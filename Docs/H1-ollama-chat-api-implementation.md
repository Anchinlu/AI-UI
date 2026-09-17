# H1 — Chuyển Ollama sang Chat API

**Người thực hiện:** Dev AI  
**Người kiểm tra:** Codex  
**Phạm vi:** Chỉ triển khai H1 trong tài liệu “Hạ tầng bắt buộc trước khi cá nhân hoá trợ lý AI”.

## 1. Mục tiêu

Chuyển adapter Ollama từ endpoint /api/generate sang /api/chat. Backend phải gửi danh sách messages có role và content, đồng thời đọc đúng message.content ở cả chế độ trả lời một lần và streaming.

H1 không triển khai conversation history, persona thật, generation config mới hoặc token trimming.

## 2. Kết quả rà soát code hiện tại

Các điểm đã xác nhận:

- src-tauri/src/commands.rs đang dùng OllamaGenerateRequest gồm model, prompt và stream.
- ai_generate đang gọi /api/generate và đọc trường response.
- ai_stream đang gọi /api/generate và đọc trường response trong NDJSON.
- AiGenerateParams đã có request_id, max_tokens, temperature và stream.
- _params trong ai_generate hiện chưa được sử dụng.
- ai_task_manager.rs đã có generation token, JoinHandle và cơ chế cancel/cleanup.
- TaskbarShell.tsx đã lọc event theo activeRequestId.
- Chưa có conversation history và chưa có persona config.

Trạng thái nền trước khi sửa:

- npm run lint: PASS
- npm run build: PASS
- cargo check: PASS

## 3. Rust contract bắt buộc

Trong src-tauri/src/commands.rs, tạo các kiểu dữ liệu tương đương:

    OllamaChatMessage {
        role: String,
        content: String
    }

    OllamaChatOptions {
        temperature: Option<f32>,
        num_predict: Option<u32>
    }

    OllamaChatRequest {
        model: String,
        messages: Vec<OllamaChatMessage>,
        stream: bool,
        options: OllamaChatOptions
    }

Quy tắc:

1. Giữ nguyên chữ ký Tauri command hiện tại để không làm vỡ frontend.
2. Prompt hiện tại trở thành message role=user.
3. Thêm một system message trung tính, ví dụ: You are a helpful assistant.
4. Không đưa tên người dùng, tên trợ lý, thông tin cá nhân hoặc persona thật vào source code.
5. Dùng AiGenerateParams.temperature cho options.temperature.
6. Ánh xạ AiGenerateParams.max_tokens sang options.num_predict.
7. Không thêm repeat_penalty, num_ctx hoặc cơ chế cấu hình ngoài; các phần đó thuộc H4.
8. Giữ stream=false cho ai_generate và stream=true cho ai_stream.

## 4. Sửa ai_generate

Đổi request thành:

    POST {ollama_url}/api/chat

Request tối thiểu phải có model, messages, stream và options. Response không streaming có dạng:

    {
      "message": {
        "role": "assistant",
        "content": "..."
      },
      "done": true
    }

ai_generate phải trả về message.content. Không còn đọc trường response.

Nếu status HTTP không phải 2xx, trả lỗi rõ ràng. Nếu JSON không đúng schema, trả lỗi parse rõ ràng.

## 5. Sửa ai_stream

Đổi endpoint sang /api/chat, giữ nguyên các cơ chế đang hoạt động:

- request_id trong mọi event.
- AiTaskManager, generation token và JoinHandle.
- ai_stop gọi abort task thật.
- Frontend lọc request cũ theo activeRequestId.
- Byte buffer chống cắt JSON và cắt UTF-8.

Mỗi dòng NDJSON của Ollama có thể có dạng:

    {"message":{"role":"assistant","content":"Xin"},"done":false}
    {"message":{"role":"assistant","content":" chào"},"done":false}
    {"message":{"role":"assistant","content":""},"done":true}

Parser phải:

1. Chỉ parse sau khi đã nhận đủ một dòng kết thúc bằng newline.
2. Lấy text từ message.content.
3. Phát ai-stream-chunk với request_id, text và done.
4. Vẫn phát event khi done=true dù content rỗng, để frontend kết thúc streaming.
5. Parse phần byte dư cuối stream nếu hợp lệ.
6. Emit ai-stream-error nếu chunk mạng hoặc JSON parse bị lỗi; không được nuốt lỗi.

Không được copy logic cũ rồi tiếp tục tìm trường response.

## 6. Frontend

H1 chỉ sửa tối thiểu nếu cần:

- Giữ payload gọi ai_stream và request_id.
- Giữ listener lọc request_id.
- Hiển thị text từ trường text do backend phát.
- Kết thúc trạng thái khi nhận done=true.
- Hiển thị lỗi khi nhận ai-stream-error.

Không được thêm trong H1:

- Conversation history.
- persona.json hoặc persona thật.
- repeat_penalty, num_ctx hoặc hệ thống trim token.
- Tính năng UI mới ngoài việc tương thích response của /api/chat.

## 7. URL, model và bảo mật

Giữ nguyên cơ chế hiện tại:

- Ưu tiên biến môi trường OLLAMA_URL.
- Nếu không có, thử 127.0.0.1:11435 rồi fallback 127.0.0.1:11434.
- Chỉ kết nối loopback 127.0.0.1.
- Model lấy từ OLLAMA_MODEL, fallback qwen2.5:1.5b.
- Không ghi prompt hoặc nội dung nhạy cảm vào log production.

## 8. Kiểm thử bắt buộc

Chạy:

    npm run lint
    npm run build

Trong src-tauri:

    cargo check

Kiểm thử thủ công:

1. Mở Ollama và bảo đảm qwen2.5:1.5b đã sẵn sàng.
2. Mở app, click AI Orb và nhập Xin chào.
3. Xác nhận app gọi /api/chat, không gọi /api/generate.
4. Xác nhận trả lời một lần đọc được message.content.
5. Xác nhận streaming hiển thị đủ các mảnh text.
6. Xác nhận dòng done=true kết thúc trạng thái dù content rỗng.
7. Thử prompt tiếng Việt dài để kiểm tra UTF-8/chunk boundary.
8. Gửi prompt thứ hai khi prompt thứ nhất đang chạy; không được trộn kết quả.
9. Bấm stop; request đang chạy phải bị huỷ.
10. Tắt Ollama giữa stream; UI phải nhận ai-stream-error.

Có thể kiểm tra request trực tiếp bằng POST JSON tới http://127.0.0.1:11435/api/chat với messages gồm một system message trung tính và một user message.

## 9. Báo cáo Dev phải gửi

Báo cáo riêng H1, không gộp H2–H5, gồm:

1. File đã sửa.
2. Xác nhận cả ai_generate và ai_stream dùng /api/chat.
3. Xác nhận parser dùng message.content và không còn response.
4. Cách xử lý done=true không có content.
5. Cách bảo toàn request_id, cancel và generation cleanup.
6. Kết quả lint, build và cargo check.
7. Kết quả test prompt tiếng Việt, streaming, stop và lỗi server.
8. Xác nhận chưa làm H2, H3, H4, H5.

Sau khi báo cáo, Dev phải dừng chờ Codex nghiệm thu.

## 10. Tiêu chí nghiệm thu

H1 chỉ được đánh giá Đạt khi:

- Không còn request AI của app dùng /api/generate.
- /api/chat nhận đúng messages có role và content.
- ai_generate đọc đúng message.content.
- ai_stream đọc đúng message.content theo từng dòng NDJSON.
- Chunk bị cắt không làm hỏng UTF-8 hoặc JSON.
- Event done luôn được phát.
- Request cũ không ghi đè request mới.
- Cancel vẫn hoạt động.
- Lint, build và cargo check đều pass.
- Không phát sinh persona thật, conversation history hoặc thay đổi H2–H5.

