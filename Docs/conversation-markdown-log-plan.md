# Kế hoạch triển khai — Ghi log hội thoại theo phiên vào Markdown

## Phạm vi

Thêm khả năng lưu trữ local để người dùng xem lại hội thoại. Tính năng này không đưa log trở lại AI làm memory, không thay đổi H1–H5, persona, history trim, streaming hoặc PrefixStripper.

Dev dừng và báo cáo sau mốc này; không tự làm rotate log, tìm kiếm log, memory dài hạn hoặc UI xem log.

## Mốc 1 — Backend session logger

### Module và state

Tạo `src-tauri/src/conversation_logger.rs` với:

- `SessionLogState` được `.manage()` trong `lib.rs`.
- State dùng `Arc<tokio::sync::Mutex<...>>` để serialize việc khởi tạo và append.
- Một session chỉ có một đường dẫn file trong suốt vòng đời app.
- Session hiện tại bắt đầu khi ghi cặp hỏi–đáp hoàn chỉnh đầu tiên; không tạo file rỗng chỉ vì mở app.

### Đường dẫn bắt buộc

```rust
let log_dir = app_handle.path().app_data_dir()?.join("logs");
```

- Dùng `create_dir_all` bất đồng bộ.
- Không dùng `Docs/logs`, đường dẫn tương đối hoặc ổ đĩa hardcode.
- Tên file: `phien_YYYY-MM-DD_HHmmss.md`.
- Dùng `chrono::Local`; `chrono` đã có trong `Cargo.toml`.

### Nội dung file

Khi tạo file lần đầu, ghi header đúng một lần:

```markdown
# Phiên trò chuyện — YYYY-MM-DD HH:mm:ss

## Cấu hình lúc bắt đầu
- Persona: ...
- Model: ...
- temperature: ...
- num_ctx: ...
- repeat_penalty: ...

## Hội thoại
```

Metadata phải là snapshot lúc file được tạo, không cập nhật lại header ở các lượt sau. Không ghi system prompt đầy đủ vào file cấu hình đầu phiên.

## Mốc 2 — Command append best-effort

Thêm command Tauri nội bộ, ví dụ `append_conversation_turn`, nhận:

- `user_message: String`
- `assistant_message: String`

Backend tự lấy `AppHandle` và session state. Metadata đầu phiên lấy từ cấu hình/model hiện hành; không để frontend tự truyền đường dẫn file.

Luồng xử lý:

1. Lock session state.
2. Nếu chưa có file, tạo thư mục và file, ghi header.
3. Mở file bằng `tokio::fs::OpenOptions` với `create(true)` và `append(true)`.
4. Ghi một cặp hoàn chỉnh:

```markdown
### [HH:mm:ss] Xếp
<user_message>

### [HH:mm:ss] Chuki
<assistant_message>
```

5. Flush/đóng file sau lượt ghi.

Không dùng `std::fs` hoặc I/O đồng bộ trong command.

### Best-effort bắt buộc

- Lỗi tạo thư mục, mở file hoặc ghi file chỉ `log::error!`/console.
- Không trả lỗi làm chuyển trạng thái chat thành error.
- Không làm crash hoặc hủy kết quả AI đã hiển thị.
- Không gọi append cho chunk streaming, request bị stop, HTTP error, parse error hoặc response rỗng.

## Mốc 3 — Nối với ConversationState H2

Chỉ gọi command sau khi frontend đã nhận `done: true`, có `assistantContent.trim() !== ''` và vừa commit thành công cặp user/assistant vào history.

Điểm nối dự kiến: nhánh `setCommittedMessages` trong `TaskbarShell.tsx`. Gọi logger fire-and-forget:

```ts
invoke('append_conversation_turn', {
  userMessage: pt.userContent,
  assistantMessage: pt.assistantContent,
}).catch(console.error);
```

Việc log thất bại không được đi vào nhánh xử lý lỗi AI. Không ghi theo từng `ai-stream-chunk`.

Trong phiên bản đầu, nút Clear chỉ xóa history trên UI; không tự xoay file session mới nếu chưa có yêu cầu riêng. Session kết thúc khi app đóng.

## Mốc 4 — Đăng ký command và kiểm thử

Đăng ký state/command trong `lib.rs`. Không expose command cho phép frontend chọn path hoặc ghi tùy ý.

Kiểm thử tự động:

- `cargo test`
- `cargo check`
- `npm run lint`
- `npm run build`

Unit test backend nên kiểm tra:

- Header chỉ xuất hiện một lần.
- Hai lượt append tạo đúng thứ tự.
- Tên file đúng format.
- Nội dung Unicode/Tiếng Việt không bị hỏng.
- Lỗi I/O được xử lý best-effort.

## Manual acceptance

1. Chạy app, chat nhiều lượt, đóng app.
2. Xác nhận file nằm tại `app_data_dir()/logs/`, không nằm trong workspace `Docs`.
3. Xác nhận mỗi phiên có đúng một file và header không lặp lại.
4. Xác nhận chỉ lượt hoàn chỉnh được ghi; stop/error/response rỗng không tạo mục dở dang.
5. Chạy bản đã cài trong `Program Files`, xác nhận vẫn ghi được vào app data directory.
6. Tắt Ollama hoặc rút mạng giữa stream, xác nhận UI báo lỗi bình thường và Markdown không có lượt dở dang.
7. Kiểm tra file log plaintext local; không có HTTP request nào gửi log ra ngoài.

## Quyền riêng tư phải ghi trong báo cáo

Log chứa nguyên văn hội thoại và có thể chứa dữ liệu cá nhân. File chỉ lưu local dạng plaintext. Mốc này không tự động xóa, rotate hoặc giới hạn dung lượng log.

## Mẫu báo cáo Dev

```text
Mốc: Conversation Markdown Log
Trạng thái: Chờ kiểm tra
Đường dẫn thực tế đã kiểm tra:
File mẫu:
Kết quả append nhiều lượt:
Kết quả stop/error/response rỗng:
Kết quả test Program Files:
Kết quả cargo/npm:
Vấn đề còn lại:
Phạm vi chưa làm:
```
