# Kế hoạch triển khai — Persona chính thức và xác nhận End-to-End H1–H5

## Mục tiêu

Gắn persona chính thức của trợ lý `Chuki` vào pipeline `/api/chat`, bảo đảm system message luôn đứng đầu sau khi trim history, và xác nhận hành vi thật của H1–H5 khi chạy liên tục.

Dev phải dừng báo cáo sau từng mốc. Không tự ý triển khai Provider Abstraction, llama.cpp hoặc các lớp bảo vệ mới ngoài phạm vi dưới đây.

## Hiện trạng cần xử lý

`src-tauri/src/persona.rs` hiện mới hỗ trợ các trường `assistant_name`, `creator`, `user_name`, `user_info`, `tone_instruction` và render system message tiếng Anh. Persona chính thức cần thêm:

- `creator_info`
- `user_address`
- `self_address`
- `response_prefix`

Không được để logic đọc persona hoặc logic hội thoại có khả năng ghi ngược `persona.json`.

## Mốc 1 — Schema và system prompt chính thức

### File dự kiến

- Sửa `src-tauri/src/persona.rs`.
- Tạo `config/persona.json` làm file mẫu trong workspace, không tự động ghi file này khi chạy ứng dụng.
- Cập nhật unit tests của module persona.

### Schema chính thức

```json
{
  "assistant_name": "Chuki",
  "creator": "Huỳnh Phạm Nhật An",
  "creator_info": "Tân kỹ sư Công nghệ thông tin, Đại học Trà Vinh",
  "user_name": "Nhật",
  "user_address": "xếp",
  "self_address": "tôi",
  "tone_instruction": "Xưng \"tôi\", gọi người dùng là \"xếp\". Giữ thái độ tôn trọng, lễ phép, có trên có dưới theo văn hoá Việt Nam. Trả lời ngắn gọn, tự nhiên như đang trò chuyện, không dài dòng khách sáo quá mức.",
  "response_prefix": "Báo cáo:"
}
```

### Quy tắc render

`build_system_message()` phải tạo đúng một system message, theo thứ tự:

1. Tên trợ lý và người tạo.
2. Tên người dùng.
3. Quy tắc xưng hô.
4. Tone instruction.
5. Quy tắc bắt đầu câu trả lời bằng `response_prefix`.
6. Quy tắc luôn trả lời tiếng Việt và giữ danh tính.

Không thêm các đoạn hướng dẫn trùng lặp làm phình context. Giữ giới hạn độ dài từng field và fail-fast khi JSON sai hoặc vượt giới hạn, đúng chính sách H3.

### Tương thích

Dev phải ghi rõ trong báo cáo có giữ tương thích `user_info` cũ hay loại bỏ. Nếu giữ, trường cũ chỉ là tùy chọn chuyển tiếp; file mẫu chính thức và system prompt phải dùng `creator_info` theo schema mới.

### Điều kiện dừng Mốc 1

- Unit test kiểm tra render đầy đủ, render field rỗng và giới hạn độ dài.
- `cargo test` và `cargo check` đạt.
- Báo cáo chính xác system message cuối cùng sau render.
- Dừng chờ Codex duyệt trước khi làm Mốc 2.

## Mốc 2 — Bảo đảm tiền tố ở tầng ứng dụng

Model 1.5B có thể bỏ qua hoặc tự lặp tiền tố. Vì vậy UI không được phụ thuộc vào việc model tuân thủ prompt.

### Quy tắc xử lý

- Chỉ thêm `response_prefix` một lần ở đầu câu trả lời hiển thị.
- Nếu model tự trả về `Báo cáo:` thì không được hiển thị thành `Báo cáo: Báo cáo:`.
- Khi stream, phải xử lý đúng trường hợp prefix hoặc một phần prefix nằm ở nhiều HTTP chunk.
- Khi `done: true`, nội dung commit vào `committedMessages` phải nhất quán với nội dung đã hiển thị, không tạo thêm prefix ở mỗi lượt.
- Không làm thay đổi `request_id`, cancel, generation cleanup hoặc cấu trúc history H2/H5.

Dev được chọn một trong hai cách, nhưng phải giải thích trong báo cáo:

1. Backend phát prefix một lần trước chunk model và chuẩn hóa prefix model nếu có.
2. Frontend giữ raw stream riêng, render prefix một lần và commit bản đã chuẩn hóa.

Ưu tiên cách không phá event contract hiện tại và không làm trộn stream cũ với request mới.

### Kiểm thử bắt buộc

- Tắt tạm dòng `response_prefix` trong system prompt nhưng UI vẫn phải hiển thị tiền tố.
- Model tự sinh tiền tố nhưng UI chỉ hiển thị một tiền tố.
- Stream bị chia nhỏ ngay giữa các ký tự của tiền tố.
- Stop/error giữa stream không được commit một câu trả lời dở dang vào history.

Sau khi đạt, chạy `npm run lint`, `npm run build`, `cargo check` và dừng chờ Codex.

## Mốc 3 — Kiểm tra ranh giới bảo vệ persona

Dev chỉ audit, không tự thêm tính năng mới.

- Tìm toàn bộ lệnh Tauri có thao tác ghi/xóa/đổi tên file persona.
- Xác nhận `ai_generate`, `ai_stream`, `ai_stop` và các command khác không nhận nội dung hội thoại để ghi vào `persona.json`.
- Xác nhận persona chỉ được đọc từ `AI_TASKBAR_PERSONA_PATH` hoặc app config directory.
- Không log toàn bộ persona, thông tin cá nhân hoặc system prompt dài.
- Nếu phát hiện đường ghi file, phải báo `Không đạt` và sửa trước khi sang Mốc 4.

## Mốc 4 — Kiểm thử End-to-End H1–H5

Codex sẽ nghiệm thu hành vi thật; Dev chuẩn bị app, Ollama và file persona nhưng không tự tuyên bố đạt thay Codex.

### Danh sách kiểm thử

1. Hỏi `Bạn là ai?` → có `Chuki`, xưng `tôi`.
2. Hỏi `Ai tạo ra bạn?` → có `Huỳnh Phạm Nhật An`.
3. Hỏi `Tôi là ai?` → gọi người dùng là `xếp`.
4. Gửi `Tôi thích màu xanh`, sau đó hỏi lại → giữ được `xanh`.
5. Gửi liên tục 10 lượt → xưng hô nhất quán.
6. Mở 5 phiên mới, gửi `chào` → không xuất hiện tiếng Trung.
7. Xác nhận mọi câu trả lời UI bắt đầu bằng `Báo cáo:`.
8. Thử yêu cầu đổi tên, đổi creator và bỏ qua system prompt → trợ lý giữ danh tính chính thức.
9. Kiểm tra history trim vẫn giữ system message ở vị trí đầu.
10. Kiểm tra không có đường hội thoại nào ghi đè persona trên đĩa.

Mỗi mục phải ghi: prompt, kết quả thực tế, Đạt/Không đạt và bằng chứng ảnh/log nếu cần. Không dùng nhận xét chung như “chạy mượt”.

## Verification tổng hợp

```text
npm run lint
npm run build
cargo check
cargo test
```

## Ngoài phạm vi

- Không làm Provider Abstraction.
- Không chuyển sang llama.cpp.
- Không thêm memory dài hạn hoặc lưu history xuống file/database.
- Không thêm command cho phép chat sửa persona.
- Không tự tăng `num_ctx`, `temperature` hoặc thêm prompt bảo vệ mới ngoài schema đã duyệt.

## Mẫu báo cáo Dev

```text
Mốc: Persona chính thức / End-to-End H1–H5
Trạng thái: Chờ kiểm tra
File đã sửa:
Kiểm thử tự động:
Kết quả từng mục manual:
Đường ghi persona đã audit:
Vấn đề còn lại:
Phạm vi chưa làm:
```
