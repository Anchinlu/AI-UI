# Báo cáo Phân tích Benchmark Backend (H7-6R)

## 1. Thông số bài đo
- **Model:** `qwen2.5:1.5b`
- **Quantization:** `Q4_K_M`
- **Model Hash:** `sha256-183715c435899236895da3869489cc30ac241476b4971a20285b1a462818a5b4`
- **Prompt:** `"Xin chào. Hãy giới thiệu ngắn gọn về chính bạn trong khoảng 50 từ."`
- **Nhiệt độ (Temperature):** 0.0
- **Context (num_ctx):** 2048
- **Số Token sinh (num_predict/max_tokens):** 64
- **Repeat Penalty:** 1.1
- **Phương pháp đo:** API `/api/chat` (Ollama) và `/v1/chat/completions` (llama.cpp) với `stream: false`. Chạy 1 Cold Start, 5 Warm Start. Backend được kill sạch trước mỗi vòng.

## 2. CLI Command sử dụng

- **Ollama:** `ollama serve` (sau đó gửi REST Request)
- **llama.cpp:** `llama-server.exe --host 127.0.0.1 --port 8081 -ngl 0 -c 2048 -n 64 -m "C:\Users\lctan\.ollama\models\blobs\sha256-183715c435899236895da3869489cc30ac241476b4971a20285b1a462818a5b4"`

## 3. Bảng Kết quả Median (Từ 5 Warm Runs)

| Tiêu chí | Ollama | llama.cpp |
| --- | --- | --- |
| **Median TPS (Tokens/sec)** | **24.44** | 7.49 |
| **Median Latency (ms)** | **830.95** | 4141.00 |
| **Median RAM (MB)** | Unavailable* | ~1006 MB |
| **Median CPU Time (ms)** | Unavailable* | ~4000-5000 ms/run |

*(Ghi chú: Lệnh `Get-Process` trên Windows không tìm thấy tiến trình con `ollama_llama_server` trong lúc gọi request, dẫn đến RAM/CPU của Ollama được đánh dấu Unavailable. Bài đo đối với Ollama tạm xem là **benchmark throughput-only**. llama.cpp đã được đo bằng CPU delta (trước/sau request).)*

## 4. Chi tiết các lượt chạy (Warm Runs)

### Ollama (CPU)
| Lượt | TPS | TTFT (ms) | Total Latency (ms) | Status |
| --- | --- | --- | --- | --- |
| 1 | 18.45 | 86.26 | 1199.24 | OK |
| 2 | 24.44 | 55.33 | 830.95 | OK |
| 3 | 25.17 | 53.64 | 793.93 | OK |
| 4 | 24.57 | 49.84 | 817.00 | OK |
| 5 | 21.23 | 51.46 | 948.73 | OK |

### llama.cpp (CPU `-ngl 0`)
| Lượt | TPS | TTFT (ms) | Total Latency (ms) | RAM (MB) | Status |
| --- | --- | --- | --- | --- | --- |
| 1 | 7.49 | N/A | 4141.00 | 1014.42 | OK |
| 2 | 7.14 | N/A | 4343.00 | 1014.45 | OK |
| 3 | 5.99 | N/A | 5176.00 | 1006.45 | OK |
| 4 | 8.25 | N/A | 3756.00 | 1006.50 | OK |
| 5 | 8.18 | N/A | 3791.00 | 1006.54 | OK |

*(Ghi chú: llama.cpp OpenAI compat endpoint `/v1/chat/completions` không trả về `prompt_eval_duration` hay đối tượng `timings` trong response khi `stream=false` nếu không can thiệp sâu vào config, do đó TTFT hiển thị N/A và không được so sánh với Ollama).*

## 5. Sai lệch và Giới hạn (Limitations)
- Bài đo Ollama là **throughput-only** do không bắt được process con của Ollama để lấy RAM/CPU delta.
- `llama.cpp` thiếu `TTFT` ở endpoint `/v1/chat/completions` do đặc thù tương thích OpenAI spec. Không so sánh TTFT giữa 2 backend.
- Chỉ số `total_latency` bị ảnh hưởng một chút bởi round-trip HTTP, nhưng được đo lường thống nhất trên cả 2.

## 6. Kết luận tạm thời
Dữ liệu chuẩn hóa H7-6R đã khẳng định tính nhất quán với H7-6 ban đầu:
- **Ollama** có tốc độ nhả chữ (Median TPS ~24) vượt xa **llama.cpp** (Median TPS ~7-8) trên cùng thiết lập CPU.
- `llama.cpp` bản CPU thuần có dấu hiệu phân bổ thread/luồng chưa tối ưu so với bản build của Ollama.
- **Không tự ý thay đổi Provider mặc định.** Ứng dụng hiện vẫn dùng Ollama làm production backend. Sẽ xem xét tối ưu luồng cho llama.cpp ở Mốc H7-7.
