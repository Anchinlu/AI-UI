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
- **RAM**: tổng `WorkingSet64` của các process có tên bắt đầu bằng `ollama`, không chỉ process broker

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
- Vì vậy chưa có đủ bằng chứng để nâng default lên 8192. Default đã được giữ an toàn ở **`num_ctx: 2048`**.

### Quyết định tạm thời

Giữ `num_ctx=2048` cho cấu hình mặc định. H5 chỉ được đóng chính thức sau khi có benchmark 8192 hợp lý hơn hoặc có lý do rõ ràng để loại mức này, cùng với số liệu RAM đầy đủ và fixture có thể hoàn tất trong thời gian chấp nhận được.

## Script tái lập
Script tái lập benchmark được lưu tại: 
- `tools/benchmark/run-benchmark.ps1` (Generate thô)
- `tools/benchmark/run-context-benchmark.ps1` (Context Budget & Trimming)
Các raw data được lưu ở `Docs/benchmarks/raw/`
