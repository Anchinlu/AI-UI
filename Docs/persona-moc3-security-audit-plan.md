# Mốc 3 — Audit bảo vệ file Persona

## Mục tiêu

Xác nhận hội thoại AI không có bất kỳ đường nào ghi đè, xóa, đổi tên hoặc tạo lại file `persona.json`. Persona chỉ được đọc vào bộ nhớ khi tạo system message.

## Phạm vi Dev được phép thực hiện

### 1. Audit toàn bộ command Rust

Rà soát:

- `src-tauri/src/commands.rs`
- `src-tauri/src/persona.rs`
- `src-tauri/src/lib.rs`
- Các module command khác nếu có quyền thao tác filesystem.

Tìm và ghi nhận mọi lời gọi:

```text
fs::write
File::create
remove_file
remove_dir
rename
copy
OpenOptions
```

Đặc biệt xác nhận `ai_generate`, `ai_stream`, `ai_stop` và các command được expose qua Tauri không nhận nội dung hội thoại để ghi vào `persona.json`.

### 2. Xác nhận đường đọc Persona

Persona chỉ được resolve theo:

1. `AI_TASKBAR_PERSONA_PATH` nếu tồn tại.
2. `app_config_dir()/persona.json`.

Không thêm đường dẫn do prompt hoặc message history cung cấp. Không tự động tạo file persona khi file chưa tồn tại.

### 3. Kiểm tra dữ liệu nhạy cảm

- Không log toàn bộ persona hoặc system prompt.
- Không đưa đường dẫn persona vào event global không cần thiết.
- Không expose command đọc/ghi persona cho frontend trong Mốc 3.

### 4. Test bắt buộc

- `cargo test`
- `cargo check`
- `npm run lint`
- `npm run build`
- Audit bằng `rg` và ghi lại kết quả, không chỉ báo “đã kiểm tra”.

Nếu không có đường ghi persona, không cần sửa code. Nếu phát hiện đường ghi, phải sửa và báo rõ file/lý do.

## Không được làm trong Mốc 3

- Không làm End-to-End H1–H5.
- Không thêm giao diện chỉnh persona.
- Không thêm memory dài hạn.
- Không chuyển sang llama.cpp.
- Không thay đổi PrefixStripper, history hoặc generation options.

## Format báo cáo

```text
Mốc: Mốc 3 — Audit bảo vệ file Persona
Trạng thái: Chờ kiểm tra
Các file đã audit:
Các API/command có quyền ghi file:
Kết quả tìm fs::write/File::create/remove/rename/copy:
Kết quả test:
Vấn đề phát hiện:
Phạm vi chưa làm:
```

Dev dừng lại sau báo cáo Mốc 3, chờ Codex nghiệm thu trước khi làm Mốc 4.
