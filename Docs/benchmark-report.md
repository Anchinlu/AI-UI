# Benchmark Report: Local AI Backends

Dưới đây là kết quả thử nghiệm tốc độ (Tokens/second) cho model `qwen2.5:1.5b` chạy local trên máy của user.

## Kết quả
1. **llama.cpp (CPU Mode)**: ~27.54 tokens/s
2. **Ollama (CPU Mode)**: ~25.75 tokens/s

*Ghi chú*: Cả 2 backend đều cho tốc độ tương đương nhau khi chạy bằng CPU, đủ đáp ứng trải nghiệm gõ chữ (streaming) mượt mà cho UI. Tốc độ này xấp xỉ tốc độ đọc trung bình.

## Các lỗi phần cứng (Hardware Acceleration)
- **Vulkan / SYCL**: Các bài thử nghiệm offload sang GPU bằng llama.cpp Vulkan và SYCL (Intel) đều báo lỗi khởi tạo hoặc crash. Nguyên nhân do driver hoặc cấu hình máy của user chưa đáp ứng đủ thư viện compute cho backend tương ứng. Do đó, dự án quyết định fallback hoàn toàn về CPU processing.

## Script tái lập
Script tái lập benchmark được lưu tại: `tools/benchmark/run-benchmark.ps1`.
