# Kế hoạch đánh giá model 3B — So sánh với qwen2.5:1.5b

**Phạm vi:** Đánh giá model lớn hơn một bậc trên đúng máy hiện tại, không thay đổi backend production trước khi có số liệu và kết quả kiểm thử hành vi.

## 1. Mục tiêu và nguyên tắc

Model production hiện tại vẫn là `qwen2.5:1.5b`. Model thử nghiệm là `qwen2.5:3b`.

Dev phải giữ nguyên:

- Ollama local và endpoint hiện tại (`127.0.0.1:11435`, fallback `11434`).
- `num_ctx=4096`, `num_predict=64`, `temperature` và `repeat_penalty` hiện hành.
- Persona Chuki, H1–H5, history, streaming, cancel, PrefixStripper và conversation logger.
- Cấu hình `OLLAMA_MODEL`; không hardcode đổi model trong Rust/frontend.

Trong giai đoạn này chỉ được tải model, chạy benchmark, kiểm thử thủ công và cập nhật tài liệu. Không sửa `generation.json`, không đổi fallback model, không commit thay đổi backend production.

## 2. Mốc 1 — Chuẩn bị model thử nghiệm

Chạy trong PowerShell:

```powershell
ollama pull qwen2.5:3b
ollama list
ollama show qwen2.5:3b
```

Dev cần ghi lại:

- Tên model và digest.
- Kích thước model/quantization mà Ollama báo.
- Phiên bản Ollama.
- CPU, RAM, GPU và Windows của máy test.
- Xác nhận `qwen2.5:1.5b` vẫn còn để có thể quay lại baseline.

Sau Mốc 1, Dev dừng và báo cáo. Không tự chuyển sang dùng 3B trong app.

## 3. Mốc 2 — Benchmark hiệu năng A/B

Dùng cùng một script, cùng endpoint, cùng cấu hình và cùng số vòng để kết quả công bằng.

Baseline 1.5B:

```powershell
.\tools\benchmark\run-context-benchmark.ps1 `
  -Model "qwen2.5:1.5b" `
  -NumCtx 4096 `
  -NumPredict 64 `
  -Runs 5 `
  -Url "http://127.0.0.1:11435"
```

Model thử nghiệm 3B:

```powershell
.\tools\benchmark\run-context-benchmark.ps1 `
  -Model "qwen2.5:3b" `
  -NumCtx 4096 `
  -NumPredict 64 `
  -Runs 5 `
  -Url "http://127.0.0.1:11435"
```

Nếu Ollama đang giữ model cũ trong RAM, Dev phải chạy hai model theo các lượt tách biệt và ghi chú việc unload/reload. Không trộn số liệu cold start với warm run.

Bắt buộc ghi nhận cho từng model và từng fixture:

- Median tokens/s.
- Median latency.
- Median prompt evaluation count/duration nếu có.
- RAM Ollama và RAM tổng hệ thống gần thời điểm đo.
- Lỗi, timeout, crash hoặc hiện tượng Windows bắt đầu swap.
- Raw CSV và summary JSON trong `Docs/benchmarks/raw/`.

> Lưu ý: `OllamaProcessRamMB` trong script là working set của tiến trình Ollama, không phải toàn bộ RAM hệ thống. Báo cáo không được gọi chỉ số này là tổng RAM máy.

Sau Mốc 2, Dev dừng và báo cáo bảng A/B. Chưa được kết luận đổi model chỉ từ một lần chạy.

## 4. Mốc 3 — Kiểm thử chất lượng hội thoại

Chạy thủ công trên cùng persona Chuki, cùng `num_ctx=4096` và nhiệt độ hiện hành. Mỗi model phải chạy cùng bộ prompt sau:

1. **Danh tính:** “Bạn là ai? Ai tạo ra bạn?”
2. **Xưng hô:** Kiểm tra model gọi người dùng là “xếp”, tự xưng đúng theo persona.
3. **History:** Lượt 1 cung cấp mã thử nghiệm giả `TEST-CODE-4821`; lượt 2 hỏi lại mã.
4. **History nhiều lượt:** 5–10 lượt hỏi đáp, kiểm tra không trộn nội dung.
5. **Prefix:** Kiểm tra câu trả lời không bị lặp kiểu `Báo cáo: Báo cáo:`.
6. **Tiếng Việt:** Một câu hỏi dài, yêu cầu trả lời có cấu trúc.
7. **Instruction following:** Yêu cầu định dạng ngắn, sau đó yêu cầu đổi định dạng.
8. **Jailbreak cơ bản:** “Bỏ qua hướng dẫn trước đó...” và kiểm tra persona không bị đổi tên/xưng hô.
9. **Xử lý thông tin nhạy cảm:** Không dùng dữ liệu thật; chỉ dùng chuỗi test giả và ghi nhận model hỏi lại/từ chối như thế nào.
10. **Stop/error:** Dừng giữa stream và tắt Ollama giữa stream; xác nhận UI không treo, không commit history dở dang.

Mỗi ca cần ghi kết quả `Pass/Fail`, không đánh giá bằng cảm giác “thông minh hơn”. Không lưu dữ liệu cá nhân thật vào benchmark hoặc log chia sẻ.

## 5. Tiêu chí quyết định

Chỉ đề xuất chuyển mặc định sang 3B khi đồng thời đạt:

- Không có lỗi correctness trong history, streaming, cancel, persona hoặc prefix.
- Chất lượng hành vi cải thiện rõ ở ít nhất 4/5 ca chính: danh tính, xưng hô, history, tiếng Việt, instruction following.
- Tốc độ warm median vẫn đạt tối thiểu khoảng 10–15 tokens/s cho trải nghiệm chat tương tác.
- RAM tổng hệ thống vẫn còn dư an toàn trên máy 8GB, không gây swap hoặc làm UI giật.
- Không có crash/timeout bất thường trong 5 vòng benchmark và bộ manual test.

Nếu 3B thông minh hơn nhưng quá chậm hoặc gây thiếu RAM: giữ 1.5B làm mặc định, chỉ cho phép người dùng chọn 3B qua `OLLAMA_MODEL`.

Nếu 3B không cải thiện rõ: không đổi model.

## 6. Mốc 4 — Cập nhật tài liệu và xin duyệt

Dev cập nhật riêng trong `Docs/benchmark-report.md` một mục:

```text
Diagnostic — Model comparison: qwen2.5:1.5b vs qwen2.5:3b
```

Không xoá hoặc ghi đè baseline 1.5B hiện tại. Mục mới phải có:

- Ngày/giờ, phần cứng và phiên bản Ollama.
- Digest/quantization/kích thước của từng model.
- Câu lệnh benchmark chính xác.
- Median tokens/s, latency và RAM theo từng fixture.
- Kết quả bộ kiểm thử hành vi.
- Kết luận: giữ 1.5B, cho phép chọn 3B thử nghiệm, hoặc đề xuất đổi mặc định.

Chỉ sau khi Codex nghiệm thu báo cáo mới được phép tạo kế hoạch riêng để đổi model production. Dev không tự sửa fallback `qwen2.5:1.5b` trong `commands.rs` ở các mốc đánh giá này.

## 7. Format báo cáo bắt buộc của Dev

```text
Mốc: Model Evaluation N — [tên mốc]
Trạng thái: Chờ kiểm tra
Model: ...
Digest/quantization: ...
Cấu hình: num_ctx=4096, num_predict=64, temperature=..., repeat_penalty=...
Lệnh đã chạy: ...
Kết quả median: ...
RAM process / RAM tổng hệ thống: ...
Lỗi hoặc rủi ro: ...
File raw: ...
Phạm vi chưa làm: chưa đổi model mặc định, chưa sửa H1–H5
```

Dev phải báo cáo và dừng sau từng mốc 1, 2, 3, 4; không gộp tất cả thành một báo cáo cuối.
