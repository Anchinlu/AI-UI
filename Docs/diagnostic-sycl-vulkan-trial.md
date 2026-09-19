# Diagnostic — SYCL/Vulkan trial

Ngày kiểm tra: 2026-09-18  
Phạm vi: chỉ xác minh runtime/device, chưa benchmark và chưa thay đổi backend production.

## Gói đã dùng

- Release: `llama.cpp b11026`
- Asset: `llama-b11026-bin-win-sycl-x64.zip`
- URL: `https://github.com/ggml-org/llama.cpp/releases/download/b11026/llama-b11026-bin-win-sycl-x64.zip`
- SHA-256: `FD41F842C55DD9E3AD4AC910DCF032255742BDA035C27F82B7F3D95BFB189311`
- Thư mục thử nghiệm: `E:\UI AI\diagnostics\llama-sycl-b11026\package`

Tại thời điểm kiểm tra, release mới hơn `b11028` đã có trên trang releases nhưng chưa có asset Windows SYCL tải xuống; vì vậy dùng asset SYCL mới nhất thực sự có thể tải là `b11026`.

## Xác nhận runtime đóng gói

Gói chứa trực tiếp các thành phần SYCL/Level Zero cần thiết, gồm:

- `sycl8.dll`
- `ur_adapter_level_zero.dll`
- `ur_adapter_level_zero_v2.dll`
- `ur_loader.dll`
- `sycl-ls.exe`
- `ggml-sycl.dll`

Không cài Intel oneAPI và không chạy `setvars.bat`.

## Lệnh và kết quả

### `sycl-ls.exe`

```text
[level_zero:gpu][level_zero:0] Intel(R) oneAPI Unified Runtime over Level-Zero, Intel(R) Iris(R) Xe Graphics 12.3.0 [1.6.34728]
[opencl:gpu][opencl:0] Intel(R) OpenCL Graphics, Intel(R) Iris(R) Xe Graphics OpenCL 3.0 NEO  [32.0.101.7088]
```

### `llama-server.exe --list-devices`

```text
Available devices:
  SYCL0: Intel(R) Iris(R) Xe Graphics (3245 MiB, 3094 MiB free)
```

Exit code: `0`.

## Kết luận bước 2–3

- **Đạt**: Iris Xe được nhận diện qua Level Zero.
- **Đạt**: OpenCL cũng nhận diện đúng GPU và khớp driver `32.0.101.7088`.
- **Đạt**: llama.cpp SYCL binary chạy được mà không cần cài oneAPI.
- **Chưa kết luận hiệu năng**: chưa chạy model, chưa chứng minh layer đã offload và chưa so sánh tokens/s với CPU.
- `llama-ls-sycl-device.exe` không nằm trong asset b11026; đã dùng `sycl-ls.exe` và `llama-server.exe --list-devices` thay thế.

## Bước tiếp theo được phép

Chạy smoke test với đúng model Qwen 2.5 1.5B Q4_K_M, xác nhận log offload thực tế rồi mới benchmark median. Không thay đổi backend production trước khi có số liệu.

Log đầy đủ của lần kiểm tra được lưu tại:

`E:\UI AI\diagnostics\llama-sycl-b11026\device-detection.txt`
