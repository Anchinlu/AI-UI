# H5 — Token Budget, Context Management và Benchmark

**Người thực hiện:** Dev AI  
**Người kiểm tra:** Codex  
**Tiền điều kiện:** H1, H2, H3 và H4 đã được nghiệm thu Đạt  
**Phạm vi:** Giới hạn context history an toàn và đo hiệu năng thực tế trước khi quyết định tăng num_ctx.

## 1. Mục tiêu

H5 phải bảo đảm mỗi request Ollama không vượt context budget của model, không làm mất system message và không cắt dở một lượt hội thoại.

Đồng thời phải benchmark các mức context bằng cùng model, backend và phần cứng để quyết định cấu hình cuối cùng dựa trên median, RAM và latency.

## 2. Hiện trạng đã rà soát

- H2 đang giữ committedMessages ở frontend và gửi toàn bộ history.
- H4 đã có num_ctx tối đa 8192 và num_predict tối đa 2048.
- Default hiện tại là num_ctx=2048, num_predict=64.
- Backend chưa có token estimator, budget calculator hoặc history trimming.
- tools/benchmark/run-benchmark.ps1 hiện là benchmark cũ dùng /api/generate; không dùng nó làm bằng chứng trực tiếp cho H5 /api/chat.
- Docs/benchmark-report.md mới có baseline trước khi thêm history/options.

## 3. Quyết định kiến trúc

### 3.1. Trimming thực hiện ở Rust

Frontend gửi committed history và user message hiện tại. Rust là nơi cuối cùng quyết định context gửi Ollama:

1. Load generation config một lần cho request.
2. Merge AiGenerateParams và validate effective config.
3. Load persona một lần cho request.
4. Tạo system message.
5. Estimate budget và trim history.
6. Dùng cùng messages/options cho ai_generate và ai_stream.

Lý do: không tin hoàn toàn vào số token do frontend tính và bảo đảm hai command có cùng hành vi.

### 3.2. Không lưu memory mới

- History vẫn chỉ tồn tại trong phiên như H2.
- H5 không ghi history ra file.
- H5 không tự trích xuất facts hoặc tạo memory lâu dài.
- H5 không thay đổi persona H3.

## 4. Token estimation

Hiện dự án chưa có tokenizer Qwen 2.5 được bundle. Không được gọi phép ước lượng ký tự là token count chính xác.

Phương án H5 giai đoạn đầu:

- Ước lượng bảo thủ theo số ký tự Unicode.
- Cộng overhead cho mỗi message và phần system.
- Dùng safety margin để giảm rủi ro prompt_eval thực tế lớn hơn ước lượng.

Có thể dùng công thức được tài liệu hoá, ví dụ:

    estimated_content_tokens = ceil(character_count / 3)
    estimated_message_tokens = estimated_content_tokens + 4
    estimated_total = sum(estimated_message_tokens)

Dev phải đặt công thức và safety factor thành hằng số có tên rõ ràng, không rải số magic trong code.

Yêu cầu quan trọng:

- Ghi rõ đây là conservative estimate, không phải tokenizer chính xác.
- Dùng prompt_eval_count từ response benchmark để so sánh và hiệu chỉnh.
- Không log nội dung prompt để debug budget; chỉ log số message, estimated tokens, num_ctx và số lượt bị loại.

Nếu sau này cần độ chính xác tuyệt đối, phải lập kế hoạch riêng để bundle tokenizer tương thích Qwen; không tự thêm một tokenizer không tương thích trong H5.

## 5. Công thức budget

Với effective config:

- C = num_ctx
- O = num_predict
- S = safety margin
- I = budget dành cho input

Công thức:

    I = C - O - S

Khuyến nghị ban đầu:

- S tối thiểu 32 token hoặc 10 phần trăm C, lấy giá trị lớn hơn.
- Không cho I nhỏ hơn 1.
- System message và user message hiện tại luôn phải được kiểm tra trước.

Nếu system message cộng user message hiện tại đã vượt I:

- Không cắt system message.
- Không tự cắt nội dung user.
- Trả lỗi rõ ràng: context quá nhỏ cho request hiện tại.

Trước khi tính budget, phải bảo đảm O < C. Nếu num_predict >= num_ctx thì trả lỗi cấu hình.

## 6. Thuật toán trim history

Input gồm system message, committedMessages từ H2 và user message hiện tại.

Thuật toán:

1. Giữ system message đầu tiên.
2. Giữ user message hiện tại.
3. Xác định các lượt đã hoàn tất trong committedMessages theo cặp user rồi assistant.
4. Duyệt từ lượt mới nhất về cũ nhất.
5. Thêm từng cặp hoàn chỉnh nếu vẫn nằm trong I.
6. Dừng khi cặp tiếp theo làm vượt budget.
7. Đảo lại thứ tự các cặp đã chọn.
8. Ghép thành system, history được giữ lại, user hiện tại.

Không được:

- Giữ assistant mà không có user tương ứng.
- Cắt giữa content của một message để cố nhét cho vừa.
- Đưa pendingTurn từ H2 vào request.
- Loại system message.
- Đổi thứ tự user/assistant.
- Im lặng bỏ user hiện tại.

History bị loại phải là các lượt cũ nhất trước. Số lượt bị trim có thể phát trong dev diagnostic nhưng không được đưa nội dung hội thoại vào log.

## 7. Module và API đề xuất

Tạo module thuần để dễ test:

- src-tauri/src/context_budget.rs

Module nên có các thành phần:

- estimate_message_tokens(message)
- calculate_input_budget(num_ctx, num_predict)
- trim_history(system_message, history, current_user, config)
- kiểu kết quả chứa messages đã chọn, estimated_tokens và dropped_turns

Các hàm pure không phụ thuộc Tauri window hoặc HTTP. commands.rs chỉ chịu trách nhiệm load config/persona và gọi module.

## 8. Tích hợp với H4

Generation config phải được load và validate trước khi trim.

Quy tắc:

- num_ctx lấy từ GenerationConfig.
- num_predict lấy từ GenerationConfig hoặc max_tokens override sau merge.
- temperature và repeat_penalty không ảnh hưởng phép trim.
- Nếu override num_predict làm input budget âm, trả lỗi thay vì tự giảm âm thầm.
- Cả ai_generate và ai_stream dùng cùng effective config và cùng thuật toán trim.
- Không thay đổi giới hạn H4 nếu chưa có kết quả benchmark H5.

Để tránh đọc file hai lần với hai kết quả khác nhau, nên load GenerationConfig một lần rồi truyền reference/value vào builder messages và builder options.

## 9. Benchmark bắt buộc

Không dùng benchmark /api/generate cũ để kết luận cho H5. Tạo script riêng hoặc mở rộng script có tham số rõ ràng để gọi /api/chat.

Đề xuất:

- tools/benchmark/run-context-benchmark.ps1

Script phải hỗ trợ:

- Backend ollama.
- Port và model từ tham số/env.
- num_ctx.
- num_predict.
- Số lượt history mẫu.
- Số iterations.
- Warm-up và cold-start được ghi riêng.
- Tính median, không chỉ lấy một lần chạy.
- Lưu raw JSON/CSV vào Docs/benchmarks/raw/.
- Ghi lệnh chạy đầy đủ vào report.

Các biến thử nghiệm tối thiểu:

1. num_ctx=2048, num_predict=64.
2. num_ctx=4096, num_predict=64.
3. num_ctx=8192, num_predict=64 nếu máy vẫn ổn định.
4. Mỗi mức thử history rỗng, ngắn, trung bình và dài.
5. Ít nhất 5 warm iterations sau một cold run.

Giữ nguyên:

- Model qwen2.5:1.5b.
- Backend CPU Ollama hiện đang dùng.
- temperature và repeat_penalty giữa các nhóm, trừ khi báo cáo ghi rõ.
- Prompt và history fixture giữa các nhóm để so sánh công bằng.

## 10. Chỉ số phải thu thập

Mỗi nhóm phải ghi:

- Model, digest và quantization nếu xác định được.
- num_ctx, num_predict, temperature, repeat_penalty.
- Số message và estimated input tokens.
- Ollama prompt_eval_count.
- Ollama prompt_eval_duration.
- Ollama eval_count.
- Ollama eval_duration.
- total_duration và time-to-first-response nếu đo được.
- Generation tokens per second.
- Median, min, max và các outlier.
- RAM hệ thống hoặc working set của các process Ollama liên quan.
- CPU utilization nếu có thể thu thập nhất quán.
- Lỗi, timeout, OOM hoặc server reset.

Tốc độ generation nên tính từ eval_count / eval_duration khi Ollama cung cấp đủ số liệu. Không dùng riêng wall-clock nếu muốn so sánh tokens/s giữa các context.

RAM phải ghi rõ cách đo, process nào được cộng và thời điểm lấy mẫu.

## 11. Quyết định cấu hình

Không mặc định chọn 4096 hoặc 8192 chỉ vì context lớn hơn.

Khuyến nghị chọn cấu hình cuối dựa trên:

1. Không OOM/crash trong toàn bộ bài test.
2. History dài được xử lý đúng và trim đúng.
3. Median latency chấp nhận được.
4. Median generation speed không giảm quá mức cần thiết.
5. RAM còn phù hợp máy 8GB.
6. Có khoảng an toàn cho app, WebView và tác vụ Windows khác.

Nếu tăng num_ctx không đem lại lợi ích rõ ràng hoặc làm chậm/RAM tăng mạnh, giữ default 2048.

## 12. Kiểm thử tự động

Chạy:

    cargo fmt --check
    cargo check
    cargo test
    npm run lint
    npm run build

Unit test context_budget tối thiểu:

1. Budget cơ bản với num_ctx=2048, num_predict=64.
2. num_predict >= num_ctx bị từ chối.
3. System luôn được giữ.
4. Current user luôn được giữ.
5. History rỗng.
6. History vừa đủ budget.
7. History vượt budget loại bỏ lượt cũ nhất.
8. Không giữ assistant lẻ.
9. Không cắt content giữa message.
10. System/current user quá lớn trả lỗi.
11. Thứ tự message sau trim chính xác.
12. Request config override được tính vào budget.

## 13. Kiểm thử thủ công

1. Chat nhiều lượt để tạo history dài.
2. Xác nhận request vẫn trả lời đúng khi history chưa vượt budget.
3. Tạo history vượt budget; xác nhận lượt cũ bị loại theo cặp.
4. Hỏi dựa trên lượt gần nhất; xác nhận context gần vẫn được giữ.
5. Xác nhận system persona H3 luôn còn trong request.
6. Đặt num_predict lớn; xác nhận input budget giảm tương ứng.
7. Đặt num_ctx=2048, 4096 và 8192; ghi latency/RAM/tốc độ thực tế.
8. Làm system hoặc user message vượt budget; xác nhận lỗi rõ ràng.
9. Stop và request_id vẫn hoạt động khi đang dùng history dài.
10. Clear conversation làm request sau quay về history rỗng.
11. Không có nội dung hội thoại nhạy cảm trong log benchmark hoặc app.

## 14. Báo cáo và tài liệu benchmark

Cập nhật Docs/benchmark-report.md bằng kết quả H5, không xoá baseline cũ.

Report phải có:

- Ngày giờ và phần cứng.
- Model, hash/digest và backend.
- Lệnh benchmark chính xác.
- Fixture prompt/history.
- Các mức num_ctx/num_predict.
- Median và cách tính.
- RAM/CPU/latency.
- Kết quả trim.
- Lý do chọn cấu hình cuối cùng.
- Các lỗi hoặc giới hạn còn lại.
- Link tới raw output trong Docs/benchmarks/raw/.

## 15. Không làm trong H5

- Không thêm persona hoặc memory lâu dài.
- Không đổi model nếu chưa có chỉ thị riêng.
- Không bật GPU offload.
- Không thay đổi UI ngoài thông báo cần thiết cho lỗi context.
- Không tự động tóm tắt history bằng AI.
- Không cắt message giữa chừng.
- Không tuyên bố token count chính xác nếu chưa dùng tokenizer tương thích.

## 16. Báo cáo Dev bắt buộc

Dev phải báo cáo riêng H5, gồm:

- File đã sửa/thêm.
- Công thức estimate và safety margin.
- Thuật toán trim và ví dụ trước/sau.
- Cách xử lý system/current user quá lớn.
- Xác nhận effective num_predict được tính vào budget.
- Script benchmark và raw output.
- Bảng median theo từng num_ctx/history.
- RAM, CPU, latency và tokens/s.
- Lý do chọn cấu hình cuối.
- Kết quả test tự động và thủ công.
- Xác nhận không thêm memory/persona mới.

Dev phải dừng sau báo cáo để Codex nghiệm thu.

## 17. Tiêu chí nghiệm thu H5

H5 chỉ đạt khi history được giới hạn an toàn theo context budget; system và current user luôn được xử lý đúng; các lượt cũ bị loại theo cặp, không cắt message; generate/stream dùng cùng options và trim logic; benchmark /api/chat có median và raw evidence; RAM/tốc độ/latency được ghi lại; cấu hình cuối có lý do dựa trên số liệu; không có dữ liệu nhạy cảm trong log; và toàn bộ kiểm thử đều pass.
