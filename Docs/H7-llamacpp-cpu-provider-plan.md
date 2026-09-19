# H7 — Tích hợp llama.cpp CPU Provider và sidecar local

**Phạm vi:** Thêm `LlamaCppProvider` chạy CPU, dùng kiến trúc H6 để giảm phụ thuộc Ollama và chuẩn bị đóng gói độc lập.

**Trạng thái ban đầu:** Ollama vẫn là provider mặc định và production backend. H7 không thay đổi mặc định trước khi hoàn tất kiểm thử.

## 1. Mục tiêu và giới hạn

### Mục tiêu

- Implement `LlamaCppProvider` theo `ChatProvider` đã nghiệm thu ở H6.
- Chạy `llama-server.exe` local bằng CPU với `-ngl 0`.
- Hỗ trợ status, model list, chat một lần, streaming và hủy request thông qua cơ chế `JoinHandle::abort()` hiện có.
- Giữ nguyên Persona, H2 History, H4 Generation Options, H5 Context Budget, PrefixStripper, Logger và fallback policy.
- Chuẩn bị sidecar/resource để app có thể chạy mà không cần người dùng cài Ollama.

### Không làm trong H7

- Không dùng SYCL, Vulkan hoặc GPU Iris Xe.
- Không viết lại H1–H6.
- Không đổi Ollama thành mặc định trước khi có benchmark và E2E.
- Không thêm memory dài hạn.
- Không tự tải model từ Internet trong background.
- Không xóa model Ollama hoặc binary hiện tại.

## 2. Tài nguyên hiện có

Binary CPU hiện có trong workspace:

```text
llama-cpp/llama-server.exe
```

Dev phải kiểm tra và ghi lại:

- Version/commit của binary.
- Các DLL đi kèm cần thiết (`llama.dll`, `ggml.dll`, v.v.).
- SHA-256 của binary và GGUF.
- Model GGUF Q4_K_M dùng cho test 1.5B và 3B.

Không lấy đường dẫn `C:\Users\...\.ollama\models\blobs\...` làm đường dẫn production cố định. Đó chỉ là nguồn kiểm thử trên máy dev.

## 3. Mốc H7-1 — Smoke test llama-server độc lập

Chạy server ngoài app bằng host local và port riêng:

```powershell
<workspace>\llama-cpp\llama-server.exe `
  -m "<MODEL_GGUF>" `
  --host 127.0.0.1 `
  --port 18180 `
  -ngl 0 `
  -c 4096 `
  -n 64
```

Xác nhận:

- Server khởi động ổn định.
- Health/status endpoint trả kết quả ready.
- Model được load đúng file và đúng SHA-256.
- Request chat một lần trả nội dung.
- Request streaming trả nhiều chunk và kết thúc đúng.
- Dừng server không để process con chạy sót.

Dev phải xác định API endpoint/JSON thực tế của đúng binary đang dùng; không giả định endpoint chỉ dựa trên tài liệu của một release khác.

**Sau H7-1: báo cáo và dừng.** Chưa sửa provider registry.

## 4. Mốc H7-2 — Implement `LlamaCppProvider`

Tạo provider mới, ví dụ:

```text
src-tauri/src/provider/llama_cpp.rs
```

Provider phải:

- Implement toàn bộ `ChatProvider`.
- Tự quản lý base URL/server client của llama.cpp.
- Chuyển `ChatRequest` chung thành payload llama.cpp.
- Chuyển response non-stream thành `String`.
- Chuyển stream thành callback `(text, done)` chung.
- Trả `ProviderError` khi HTTP lỗi, timeout, JSON lỗi, EOF bất thường hoặc server chết.
- Không chứa allowlist, fallback, persona, trim history hoặc logger.
- Không phát trực tiếp event UI; `commands.rs` giữ event contract hiện tại.

Không copy nguyên parser Ollama nếu format llama.cpp khác. Parser phải có buffer chống ranh giới chunk và không nuốt lỗi.

### Kiểm thử H7-2

- Test payload có model/messages/options đúng.
- Test response một lần.
- Test stream chia chunk và chunk UTF-8.
- Test HTTP error/JSON error/stream error.
- Test provider thỏa `dyn ChatProvider`.

**Sau H7-2: báo cáo và dừng.**

## 5. Mốc H7-3 — Quản lý sidecar process

Tạo module riêng, ví dụ:

```text
src-tauri/src/llama_server_manager.rs
```

Trách nhiệm:

- Tìm binary theo thứ tự dev resource → packaged resource.
- Tìm model theo đường dẫn cấu hình an toàn.
- Chọn port local trống hoặc port cấu hình cố định trong phiên.
- Khởi chạy với `127.0.0.1`, `-ngl 0`, `-c` và `-n` từ effective generation config.
- Ẩn cửa sổ console trên Windows.
- Capture stdout/stderr để log chẩn đoán.
- Chờ health ready với timeout hữu hạn.
- Dừng process và process con khi app đóng hoặc provider bị đổi.
- Không yêu cầu Administrator.

Nếu binary/model thiếu, trả lỗi rõ ràng để policy quyết định fallback sang Ollama; không crash app.

**Sau H7-3: báo cáo và dừng.**

## 6. Mốc H7-4 — Tích hợp ProviderRegistry

Mở rộng `ProviderRegistry` để có factory/instance cho `llama.cpp`.

Luồng request phải là:

```text
config.provider
  → ProviderRegistry
  → provider.list_models()/status()
  → ModelPolicy.resolve()
  → build ChatRequest chung
  → provider.chat() hoặc provider.chat_stream()
```

Yêu cầu:

- `commands.rs` không biết endpoint, payload hoặc parser của llama.cpp.
- Ollama path cũ vẫn hoạt động không thay đổi.
- Fallback model/provider không làm mất lựa chọn trong `config.json`.
- `effective_model` tiếp tục được truyền cho Logger.
- `ai-model-resolved`, `ai-stream-error`, request ID và cancel giữ nguyên.

Ở mốc này có thể cho phép config provider `llama.cpp` trong môi trường dev, nhưng **Ollama vẫn là default** và UI production chưa cần thêm provider selector nếu chưa được duyệt.

**Sau H7-4: báo cáo và dừng.**

## 7. Mốc H7-5 — Regression và E2E

Chạy với cả Ollama và llama.cpp CPU:

- Persona Chuki và prefix `Báo cáo:`.
- History 2–10 lượt, gồm token test giả `@testtoken`.
- Context trimming H5.
- Generation options H4.
- Stream chunk boundary và UTF-8.
- Stop giữa stream.
- Tắt server giữa stream.
- Không commit history khi stream lỗi/đang dở.
- Logger ghi đúng `effective_model` và dòng đổi provider/model.
- Migration `config.json` vẫn giữ nguyên model.

Tất cả kết quả phải ghi riêng theo provider; không trộn log Ollama và llama.cpp.

## 8. Mốc H7-6 — Benchmark và quyết định

Giữ cố định:

- Cùng GGUF, SHA-256 và prompt.
- `num_ctx=4096`, `num_predict=64`.
- Cùng số vòng, tối thiểu 5 warm runs và ghi cold start riêng.
- CPU-only: Ollama CPU và llama.cpp `-ngl 0`.

Đo:

- Median generation tokens/s.
- Median latency/TTFT nếu backend cung cấp.
- RAM process và RAM tổng hệ thống.
- CPU utilization.
- Tỷ lệ lỗi/timeout/crash.
- Thời gian khởi động và dừng sidecar.

Raw output lưu tại:

```text
Docs/benchmarks/raw/h7_<provider>_<model>_<timestamp>.csv
Docs/benchmarks/raw/h7_<provider>_<model>_<timestamp>_summary.json
```

Chỉ đề xuất đổi backend mặc định nếu llama.cpp:

- Pass toàn bộ regression/E2E.
- Không làm mất tính năng H1–H6.
- Chạy ổn định qua các vòng benchmark.
- Có lợi ích hiệu năng hoặc đóng gói rõ ràng.
- Không tạo gánh nặng RAM/khởi động không chấp nhận được.

Nếu không đạt, giữ Ollama production và ghi llama.cpp ở trạng thái experimental.

## 9. Đóng gói sau khi provider đạt

Chỉ thực hiện sau H7-6:

- Đưa `llama-server.exe` và DLL cần thiết vào Tauri resources/sidecar đúng chuẩn target Windows.
- Không hardcode đường dẫn dev.
- Model 1.5B phải có chiến lược phân phối rõ ràng: bundled resource hoặc tải/copy có kiểm soát vào `app_data_dir/models`.
- Kiểm tra cài app trong `Program Files`.
- Kiểm tra máy Windows khác không có Ollama vẫn chạy được.
- Ghi SHA-256/version của binary và model vào tài liệu release.

## 10. Format báo cáo Dev

```text
Mốc: H7-N — [tên mốc]
Trạng thái: Chờ kiểm tra
Provider: ollama / llama.cpp
llama.cpp version/commit: ...
Model + quantization + SHA-256: ...
Lệnh thực tế: ...
Endpoint/API contract đã xác minh: ...
Kết quả: ...
Median/latency/RAM: ...
Lỗi hoặc rủi ro: ...
Raw log: ...
Regression đã chạy: ...
Phạm vi chưa làm: ...
```

Dev phải báo cáo và dừng sau từng mốc H7-1 đến H7-6. Không tự đổi provider production và không tự thêm GPU/SYCL/Vulkan vào phase này.
