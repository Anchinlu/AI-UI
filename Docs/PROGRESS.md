## Cập nhật lần cuối: 2026-09-19 / Hoàn thành mốc H7-4 & H7-5

### Trạng thái hiện tại
- Đã tích hợp thành công `llama.cpp` làm provider thay thế cho Ollama.
- Đã xử lý Lazy Initialization, Job Object cleanup, tự động quản lý vòng đời sidecar và fallback.
- Code đảm bảo passing 100% (51 tests) và 0 compiler warnings.
- Kiểm thử E2E Regression thủ công đã pass.

### Đã hoàn thành
- [x] Tích hợp cấu trúc `LlamaCppProvider` (H7-2, H7-3).
- [x] Fix rủi ro tiến trình rác khi đóng app (Job Object Windows) (H7-4).
- [x] Fix logic fallback & đọc path GGUF chính xác dựa vào tên model/SHA (H7-4).
- [x] Hoàn tất 5 kịch bản kiểm thử Regression E2E (H7-5).

### Đang làm / tiếp theo
- [ ] Chuyển sang mốc H7-6: Thực hiện Benchmark & So sánh hiệu năng giữa llama.cpp vs Ollama trên cùng một context.
- [ ] Xây dựng tính năng UI cho phép người dùng cấu hình chọn Provider trong tương lai.

### Quyết định đã chốt (không hỏi lại)
- Không lưu `llama-server.exe` vào cùng 1 repo (do lớn), tìm path thông qua `current_exe()` ở Prod hoặc fallback về `../llama-cpp/` ở Dev.
- Provider fallback chỉ hỗ trợ fallback cục bộ của chính llama.cpp, không tự động fallback xuyên suốt từ llama.cpp sang Ollama (vì hai API khác biệt và người dùng đã cấu hình Provider trong config.json).
- Llama-server chạy ẩn console thông qua `CREATE_NO_WINDOW` trên Windows.
