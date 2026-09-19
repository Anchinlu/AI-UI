# Kế hoạch chẩn đoán GPU Iris Xe — llama.cpp SYCL/Vulkan

**Phạm vi:** Kiểm tra liệu Intel Iris Xe có giúp tăng tốc model local hay không. Đây là thử nghiệm chẩn đoán độc lập; chưa thay Ollama, chưa sửa backend production và chưa thay đổi H1–H5.

## 1. Trạng thái nền đã có

Tài liệu `Docs/diagnostic-sycl-vulkan-trial.md` đã ghi nhận:

- Gói Windows SYCL `b11026` chạy được.
- `sycl-ls.exe` nhận diện Intel Iris Xe qua Level Zero.
- `llama-server.exe --list-devices` nhận diện `SYCL0: Intel(R) Iris(R) Xe Graphics`.
- Không cần cài Intel oneAPI cho binary release này.

Dev không cần cài lại oneAPI hoặc lặp lại bước nhận diện, trừ khi runtime hiện tại không chạy được.

## 2. Nguyên tắc an toàn

Không được:

- Đổi `OLLAMA_MODEL` mặc định.
- Đổi `OLLAMA_URL`, `generation.json`, `num_ctx` hoặc `num_predict` production.
- Đưa `llama-server.exe` vào Tauri app.
- Xóa model CPU hoặc dữ liệu Ollama hiện tại.
- Kết luận GPU đang chạy chỉ vì Task Manager hiển thị shared GPU memory.

Mọi thử nghiệm phải chạy ngoài app, bằng `llama-server.exe` riêng. Khi kết thúc phải dừng process server thử nghiệm.

## 3. Mốc GPU-1 — Chuẩn bị model và file kiểm chứng

1. Dùng đúng model tương đương với benchmark Mốc 2:
   - Ưu tiên `qwen2.5:3b` Q4_K_M.
   - Có thể chạy thêm `qwen2.5:1.5b` để đối chiếu với baseline cũ.
2. Xác định đường dẫn GGUF thật, không dùng đường dẫn tương đối mơ hồ.
3. Ghi SHA-256 file GGUF bằng:

```powershell
Get-FileHash -Algorithm SHA256 -Path "<MODEL_PATH>"
```

4. Ghi phiên bản gói llama.cpp, tên executable, driver Intel và ngày kiểm tra.
5. Chạy lại tối thiểu:

```powershell
<SYCL_DIR>\sycl-ls.exe
<SYCL_DIR>\llama-server.exe --list-devices
```

**Điều kiện qua mốc:** Có thiết bị `SYCL0` hoặc thiết bị GPU Intel tương đương; không có lỗi DLL/runtime.

Dev báo cáo và dừng tại đây.

## 4. Mốc GPU-2 — Smoke test CPU đối chứng

Chạy cùng file GGUF bằng CPU thuần:

```powershell
<SYCL_DIR>\llama-server.exe `
  -m "<MODEL_PATH>" `
  --host 127.0.0.1 `
  --port 18080 `
  -ngl 0 `
  -c 4096 `
  -n 64
```

Gửi một prompt ngắn qua endpoint của `llama-server`, xác nhận:

- Server trả HTTP 200.
- Model sinh được text.
- Log xác nhận không offload layer lên GPU (`-ngl 0`).
- Không crash và không treo quá 60 giây.

Nếu server dùng endpoint/flag khác theo release thực tế, Dev phải ghi lại lệnh thực tế, không tự sửa số liệu.

Dev báo cáo và dừng.

## 5. Mốc GPU-3 — Smoke test SYCL offload

Chạy cùng model, cùng context và cùng prompt, chỉ đổi offload:

```powershell
<SYCL_DIR>\llama-server.exe `
  -m "<MODEL_PATH>" `
  --host 127.0.0.1 `
  --port 18081 `
  -ngl 99 `
  -c 4096 `
  -n 64
```

Phải có bằng chứng trong log:

- Thiết bị `SYCL0` được chọn.
- Có layer/tensor được offload lên GPU.
- Có buffer GPU hoặc thông tin backend SYCL thực sự được dùng.
- Request hoàn tất và sinh text đúng.

Nếu chỉ thấy server chạy nhưng log không có offload, kết quả phải ghi là **Không chứng minh được GPU acceleration**, không được gọi là GPU benchmark thành công.

Dev báo cáo và dừng.

## 6. Mốc GPU-4 — Benchmark CPU và SYCL

Chỉ chạy khi smoke test SYCL đã chứng minh offload thật.

Giữ cố định:

- Cùng GGUF, SHA-256 và model.
- `num_ctx=4096`.
- `num_predict=64`.
- Cùng prompt hoặc cùng bộ fixture.
- 1 lượt warm-up không tính vào median.
- Tối thiểu 5 lượt đo cho mỗi cấu hình.
- Dừng các server cũ trước khi chạy cấu hình tiếp theo.

Đo riêng:

- CPU `-ngl 0`.
- SYCL `-ngl 99`.
- Vulkan chỉ chạy sau SYCL, nếu cần kiểm tra thêm; không gộp kết quả Vulkan vào SYCL.

Bắt buộc ghi:

- Prompt/eval tokens/s.
- Median latency và median generation tokens/s.
- RAM tổng hệ thống trước/trong/sau.
- Working set/private memory của `llama-server` và các process liên quan.
- CPU utilization.
- GPU utilization và GPU memory, kèm log backend làm bằng chứng.
- Số lần lỗi, timeout, crash hoặc shader compilation hang.

Raw output lưu vào:

```text
Docs/benchmarks/raw/gpu_offload_<backend>_<model>_<timestamp>.json
Docs/benchmarks/raw/gpu_offload_<backend>_<model>_<timestamp>.csv
```

## 7. Tiêu chí quyết định

Chỉ đề xuất dùng GPU nếu đồng thời:

- Offload được chứng minh bằng log backend/device, không chỉ bằng Task Manager.
- Tốc độ median tăng ít nhất 15–20% so với CPU cùng điều kiện.
- Không crash, không treo shader, không timeout bất thường.
- RAM tổng 8GB vẫn còn dư an toàn, không paging làm UI giật.
- Kết quả lặp lại ổn định qua tối thiểu 5 lượt.

Nếu GPU chạy được nhưng chậm hơn hoặc không ổn định: giữ CPU làm backend chính và ghi kết quả vào mục `Diagnostic — SYCL/Vulkan trial` trong `Docs/benchmark-report.md`.

## 8. Báo cáo bắt buộc của Dev

```text
Mốc: GPU-N — [tên mốc]
Trạng thái: Chờ kiểm tra
Backend: CPU / SYCL / Vulkan
llama.cpp release: ...
Model + quantization: ...
GGUF SHA-256: ...
Lệnh thực tế: ...
Bằng chứng device/offload: ...
Median tokens/s: ...
Median latency: ...
RAM process / RAM tổng hệ thống: ...
CPU/GPU utilization: ...
Lỗi hoặc rủi ro: ...
File raw/log: ...
Phạm vi chưa làm: chưa đổi backend production
```

Dev phải dừng sau từng mốc GPU-1, GPU-2, GPU-3 và GPU-4. Không gộp báo cáo và không tự chuyển Ollama sang GPU.
