# Benchmark Report: Local AI Backends

Dưới đây là kết quả thử nghiệm tốc độ (Tokens/second) cho model `qwen2.5:1.5b` chạy local trên máy của user.

## Kết quả Baseline (H1)
1. **llama.cpp (CPU Mode)**: ~27.54 tokens/s
2. **Ollama (CPU Mode)**: ~25.75 tokens/s

*Ghi chú*: Cả 2 backend đều cho tốc độ tương đương nhau khi chạy bằng CPU, đủ đáp ứng trải nghiệm gõ chữ (streaming) mượt mà cho UI. Tốc độ này xấp xỉ tốc độ đọc trung bình.

## Các lỗi phần cứng (Hardware Acceleration)
- **Vulkan / SYCL**: Các bài thử nghiệm offload sang GPU bằng llama.cpp Vulkan và SYCL (Intel) đều báo lỗi khởi tạo hoặc crash. Nguyên nhân do driver hoặc cấu hình máy của user chưa đáp ứng đủ thư viện compute cho backend tương ứng. Do đó, dự án quyết định fallback hoàn toàn về CPU processing.

## H5 Context Budget Benchmark (Ollama API /api/chat)
Ngày thực hiện: 2026-09-17  
Mục tiêu: Kiểm tra budget/trim và đo ảnh hưởng của `num_ctx` bằng dữ liệu có thể tái lập.  
Script: `tools/benchmark/run-context-benchmark.ps1`

### Cấu hình đo đã sửa

- **Model**: `qwen2.5:1.5b`
- **URL**: `http://127.0.0.1:11435`
- **NumPredict**: 16 trong lượt kiểm tra này để giới hạn thời gian chạy fixture dài
- **Runs**: 5 mẫu đo sau 1 warm-up cho mỗi fixture
- **Median**: tính đúng; nếu số mẫu chẵn thì lấy trung bình hai giá trị giữa
- **RAM**: tổng `WorkingSet64` của các process có tên bắt đầu bằng `ollama`, không chỉ process broker; đây là working-set của tiến trình, không phải tổng RAM hệ thống

### Kết quả đã chạy

| num_ctx | Fixture | Estimated input | Median prompt eval | Median tokens/s | Median latency (ms) | Median Ollama RAM (MB) |
|---|---:|---:|---:|---:|---:|---:|
| 2048 | Empty | 38 | 29 | 23.22 | 986.82 | 84.57 |
| 2048 | Short | 126 | 112 | 18.06 | 1312.53 | 84.81 |
| 2048 | Medium | 2090 | 1943 | 13.85 | 1829.96 | 73.14 |
| 2048 | Long | 10854 | 1755 | 13.43 | 2406.49 | 57.40 |
| 4096 | Empty | 38 | 29 | 19.49 | 1343.88 | 65.74 |
| 4096 | Short | 126 | 112 | 19.25 | 1469.61 | 65.94 |
| 4096 | Medium | 2090 | 1943 | 14.23 | 1777.19 | 67.63 |
| 4096 | Long | 10854 | 3885 | 10.03 | 2361.37 | 51.58 |

Raw evidence:

- `Docs/benchmarks/raw/context_benchmark_20260917_211759.csv`
- `Docs/benchmarks/raw/context_benchmark_20260917_211759_summary.json`
- `Docs/benchmarks/raw/context_benchmark_20260917_212036.csv`
- `Docs/benchmarks/raw/context_benchmark_20260917_212036_summary.json`

### Kết quả trim và giới hạn benchmark

- Unit tests của `context_budget.rs` xác nhận history bị loại theo cặp, giữ đoạn liên tục mới nhất, giữ system/current user và từ chối history rỗng.
- Medium/Long đã vượt estimated budget ở 2048; prompt eval thực tế bị Ollama giới hạn quanh context tương ứng.
- Lượt `num_ctx=8192` với fixture Long mất hơn 3 phút trên CPU và đã được dừng, không dùng làm bằng chứng thành công.
- Vì vậy chưa có đủ bằng chứng để nâng default lên 8192. Mức 4096 có số liệu hoàn chỉnh và phù hợp hơn với mục tiêu giữ ngữ cảnh gần; fixture Long chậm hơn khoảng 25% so với 2048 nên được ghi nhận là giới hạn khi lịch sử rất dài.

### Quyết định tạm thời

Chốt `num_ctx=4096` cho cấu hình mặc định. So với 2048, các fixture Empty/Short/Medium vẫn nằm trong mức chấp nhận cho UI; RAM đo được không tăng trong các mẫu đã chạy. Fixture Long giảm còn khoảng 10.03 tokens/s và được xem là giới hạn chấp nhận được vì context budget sẽ trim lịch sử trước khi request thực tế đạt kích thước này.

## Script tái lập
Script tái lập benchmark được lưu tại: 
- `tools/benchmark/run-benchmark.ps1` (Generate thô)
- `tools/benchmark/run-context-benchmark.ps1` (Context Budget & Trimming)
Các raw data được lưu ở `Docs/benchmarks/raw/`

## Quyết định backend

- **Hiện tại**: tiếp tục dùng Ollama làm backend tham chiếu cho H1-H5 vì adapter `/api/chat`, streaming, cancel, persona, generation options và context trim đã được kiểm thử.
- **Mục tiêu đóng gói**: chuyển sang llama.cpp server trong một phase riêng, sau khi tạo provider interface dùng chung. Không rewrite trực tiếp H2-H5; các message role/content, persona, options và trim sẽ được giữ lại.
- **Lý do**: llama.cpp CPU đã nhanh hơn khoảng 7% trong baseline, nhưng việc bundling executable/model và adapter lifecycle cần được triển khai, kiểm thử và nghiệm thu riêng. Không trộn thay đổi đó vào H5.

---

## Benchmark A/B: qwen2.5:1.5b vs qwen2.5:3b (CPU)

**Môi trường thử nghiệm:**
- Tổng RAM hệ thống (System RAM): 8GB vật lý (7.68GB khả dụng).
- Các lượt chạy được thực hiện tuần tự, model được unload hoàn toàn trước khi chuyển sang model mới.
- RAM process Ollama (Working Set): Dao động ở mức ~22MB đến ~62MB, do phần lớn bộ nhớ model được quản lý qua cơ chế Memory-Mapped Files của hệ điều hành.

### Kết quả Benchmark (Median)

| Kịch bản (Context) | Tokens ước tính | `qwen2.5:1.5b` (Tốc độ / Độ trễ) | `qwen2.5:3b` (Tốc độ / Độ trễ) | Đánh giá chênh lệch |
| --- | --- | --- | --- | --- |
| **0_Empty** | 38 | 20.00 t/s — 2.3s | 7.90 t/s — 4.9s | 3b chậm hơn ~2.5 lần |
| **1_Short** | 126 | 17.62 t/s — 2.9s | 7.07 t/s — 9.6s | 3b chậm hơn ~2.5 lần |
| **2_Medium** | 2090 | 14.20 t/s — 2.9s | 7.47 t/s — 5.5s | 3b chậm hơn ~1.9 lần |
| **3_Long** | 10854 | 11.26 t/s — 4.4s | 5.77 t/s — 10.5s | 3b chậm hơn ~2.0 lần |

**Đánh giá sơ bộ:**
Trên cấu hình CPU thuần, model `qwen2.5:3b` có tốc độ sinh text khoảng 5.7 - 7.9 tokens/giây, chậm hơn rõ rệt (khoảng một nửa) so với baseline `1.5b`. Ở những phiên có ngữ cảnh dài, độ trễ phản hồi trước token đầu tiên (latency) của bản 3B có thể lên tới 10.5 giây. Không gộp kết quả benchmark này (hay SYCL) vào Baseline mặc định cho đến khi nghiệm thu phần kiểm thử thủ công (Mốc 3).

---

## Diagnostic — SYCL/Vulkan trial

Mục đích: Chẩn đoán tính khả thi của việc dùng GPU tích hợp Intel Iris Xe Graphics thông qua backend SYCL.

**Thông tin thử nghiệm:**
- **Model + quantization:** `qwen2.5:3b` (`Q4_K_M`)
- **GGUF SHA-256:** `5ee4f07cdb9beadbbb293e85803c569b01bd37ed059d2715faa7bb405f31caa6`
- **Release:** `llama.cpp b11026` (Windows SYCL x64)

**Quá trình chẩn đoán (Smoke test SYCL):**
Kiểm thử ép offload tối đa các tensor lên GPU thông qua lệnh:
```powershell
llama-server.exe -m "C:\Users\lctan\.ollama\models\blobs\sha256-5ee4f07cdb9beadbbb293e85803c569b01bd37ed059d2715faa7bb405f31caa6" --host 127.0.0.1 --port 18081 -ngl 99 -c 4096 -n 64
```

**Kết quả:**
- Lệnh `-ngl 99` làm server treo hơn 2 phút trong bước khởi tạo tính toán.
- Server báo lỗi `level_zero backend failed with error: 20 (UR_RESULT_ERROR_DEVICE_LOST)` và văng exception tại toán tử `MUL_MAT`.
- Không có log chứng minh offload hoàn tất. Không thể thu thập được số liệu tokens/s hoặc latency hợp lệ. Do đó, nhánh SYCL bị dừng ở Mốc GPU-3 và bỏ qua Mốc GPU-4.

**Kết luận chẩn đoán:**
SYCL/Level Zero không chạy ổn định trên cấu hình Iris Xe hiện tại; chưa đủ bằng chứng để khẳng định chính xác lỗi do driver, cơ chế Timeout Detection and Recovery (TDR) của hệ điều hành, hay do giới hạn bộ nhớ (memory mapping).

**Quyết định:** Không sử dụng SYCL GPU trong môi trường production hiện tại. Không gộp kết quả thất bại này vào baseline CPU. Lộ trình tiếp theo sẽ tập trung vào kiểm thử chất lượng 1.5B và 3B thuần CPU trước khi cân nhắc chuyển đổi model.

---

## 3. Mốc 3 — Kiểm thử chất lượng (Manual Quality Assessment)

**Phương pháp:** 
- Đánh giá hành vi trực tiếp thông qua endpoint `/api/chat` với Persona Chuki (không sử dụng UI frontend, chưa kiểm tra PrefixStripper/Cancel).
- Thử nghiệm các kịch bản: nhận diện bản thân, cách xưng hô (gọi "xếp", xưng "em"), chèn tiền tố "Báo cáo: ", và khả năng ghi nhớ history ngắn hạn.

**Kết quả ghi nhận sơ bộ:**
- **qwen2.5:3b**: Tuân thủ cấu trúc câu, tiền tố "Báo cáo: " và các chỉ thị persona tốt hơn đáng kể so với 1.5B. Câu trả lời tự nhiên và mạch lạc. Tuy nhiên thi thoảng vẫn lỗi xưng hô ("em An").
- **qwen2.5:1.5b**: Đáp ứng tốc độ xuất sắc, nhưng thường xuyên bỏ qua các chỉ thị tiền tố và quên cách xưng hô đúng chuẩn (gọi user là bạn thay vì xếp).
- Cả hai model đều có khả năng ghi nhớ history ở những lượt chat ngắn.
- *Lưu ý*: Độ trễ ~100ms trong bài test mô phỏng API giả lập chỉ phản ánh độ trễ first-token của model warm-up ở context cực kỳ ngắn (vài token), không đại diện cho latency thực tế khi chạy qua UI với history đầy đủ (vốn lên tới vài giây như benchmark Mốc 2).

**Kết luận cuối cùng (Model Selection):**
Chưa có đủ dữ liệu kiểm thử toàn diện qua UI để thay thế hoàn toàn model mặc định. Quyết định cuối cùng cho giai đoạn này:
- **qwen2.5:1.5b**: Giữ làm model **mặc định** (ưu tiên tốc độ phản hồi cho mọi cấu hình máy tính).
- **qwen2.5:3b**: Được cung cấp dưới dạng model **tùy chọn** (ưu tiên chất lượng câu trả lời, dành cho user không bận tâm về độ trễ).
- **Fallback an toàn**: Hệ thống cần có cơ chế fallback tự động về 1.5b nếu 3b bị lỗi tải hoặc không tồn tại trên máy người dùng.

*(Sẽ triển khai một phase riêng cho tính năng "Model Selection & Settings" để hoàn thiện UI chọn model, lưu cấu hình và cơ chế fallback).*
