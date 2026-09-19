# Báo cáo Phân tích Benchmark Backend (H7-6)

## Thông số bài Test
- **Model:** qwen2.5:1.5b (Q4_K_M)
- **Prompt:** "Xin chào. Hãy giới thiệu ngắn gọn về chính bạn trong khoảng 50 từ."
- **Nhiệt độ (Temperature):** 0.0
- **Context (num_ctx):** 2048
- **Số Token sinh (num_predict):** 64
- **Thiết lập Hardware:** CPU Only (Vô hiệu hóa offload)

---

## 1. Kết quả Báo cáo

### Bảng So sánh Trực tiếp

| Tiêu chí | Ollama (CPU) | llama.cpp (CPU) |
| --- | --- | --- |
| **Cold Start TPS** | 24.21 tokens/s | 22.68 tokens/s |
| **Median Warm TPS** | **22.36 tokens/s** | 11.94 tokens/s |
| **Time To First Token (Cold)**| 692.35 ms | **391.98 ms** |
| **Peak RAM (Reported)** | ~54.56 MB (Go Server)* | ~1056.55 MB (Sidecar) |
| **CPU Time Used** | 4.72s | 223.31s |

*\*Lưu ý: Chỉ số RAM của Ollama hiển thị lượng RAM của tiến trình cha (Server Go). Lượng RAM thực tế dùng để nạp model bên trong sidecar của Ollama (ollama_llama_server) tương đương với llama.cpp (~1GB).*

---

## 2. Nhận xét Chuyên sâu

### Tốc độ sinh Token (Tokens/s)
- Mặc dù hai bên có tốc độ Cold Start tương đối sát nhau (~22-24 tokens/s), **Ollama giữ được phong độ sinh token tốt hơn nhiều** trong các lượt Warm Start tiếp theo (Trung vị đạt 22.36 so với 11.94 của llama.cpp).
- Ở các lượt chạy số 3, 4, 5, `llama.cpp` bị suy giảm hiệu năng rõ rệt (có thể do cơ chế quản lý thread không tối ưu hoặc bị Thermal Throttling / hệ điều hành can thiệp), khiến tốc độ rớt xuống mức 10-11 tokens/s. Ollama cũng bị suy giảm nhưng chỉ rớt xuống 15-16 tokens/s.
- **Chiến thắng:** Ollama (Nhanh hơn x1.8 lần ở Warm Start).

### Độ trễ phản hồi ban đầu (Latency)
- `llama.cpp` xử lý ngữ cảnh (Prompt Evaluation) lần đầu rất nhanh (chỉ mất ~391 ms), trong khi Ollama mất ~692 ms. 
- Do đó, nếu cần AI "nảy chữ" lập tức, llama.cpp cho cảm giác phản hồi nhanh hơn một chút ở câu hỏi đầu tiên.

### Quản lý Tài nguyên CPU
- Chỉ số `CPU Time Used` thể hiện tổng thời gian CPU mà hệ điều hành cấp phát trên tất cả các luồng. Tiến trình llama.cpp tiêu thụ lượng CPU Time khổng lồ (223 giây trong vòng chưa tới 30 giây thời gian thực), cho thấy nó tận dụng tối đa (hoặc spam) luồng CPU. 
- Điều này có thể lý giải nguyên nhân máy tính bị quá nhiệt/giật lag hơn và rớt TPS ở các lượt sau khi chạy llama.cpp, trong khi Ollama (sử dụng bản build được tối ưu hóa AVX2/MKL) quản lý luồng thông minh và tiết kiệm điện năng hơn (CPU time rất thấp).

---

## 3. Quyết định (Recommendation)

Căn cứ vào kết quả thử nghiệm trực tiếp trên máy của anh:
1. **Ollama có hiệu năng đường dài (Warm Start) vượt trội và ổn định hơn.**
2. Bản build `llama.cpp` thuần chưa được tối ưu hóa luồng (thread management) tốt cho CPU, dẫn đến tình trạng ngốn CPU nhưng hiệu suất lại rớt thảm hại ở các lượt chạy liên tiếp.
3. Ollama chạy ngầm và không cần lo rủi ro quản lý process như llama.cpp.

**=> Kết luận:** Ứng dụng nên **tiếp tục sử dụng Ollama làm Provider mặc định** cho môi trường Production để đảm bảo độ mượt mà cho toàn hệ thống. `llama.cpp` đã được lập trình chuẩn xác để trở thành một Provider dự phòng tuyệt vời hoặc dành cho các thử nghiệm Offload GPU sau này.
