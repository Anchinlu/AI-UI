# H3 — Persona từ file cấu hình bên ngoài

**Người thực hiện:** Dev AI  
**Người kiểm tra:** Codex  
**Tiền điều kiện:** H1 và H2 đã được nghiệm thu Đạt  
**Phạm vi:** Đọc persona bên ngoài source code và chuyển thành một system message cho Ollama.

## 1. Mục tiêu

Cho phép thay đổi tên trợ lý, người tạo, tên người dùng, thông tin mô tả và phong cách trả lời bằng file cấu hình. Khi sửa file, không cần biên dịch lại ứng dụng.

H3 chỉ cá nhân hoá system prompt. Đây chưa phải hệ thống ghi nhớ lâu dài và không thay đổi cách lưu conversation history của H2.

## 2. Hiện trạng đã rà soát

- Backend hiện dùng system message trung tính trong src-tauri/src/commands.rs.
- H2 đã gửi history user/assistant theo thứ tự và backend tự thêm system message.
- Chưa có persona config, PersonaStore hoặc đường dẫn persona hardcode.
- src/state/aiState.ts không liên quan đến persona và không cần sửa trong H3.
- serde và serde_json đã có trong Cargo.toml; không cần thêm thư viện JSON mới.

## 3. Quyết định vị trí file

Không được hardcode đường dẫn như E:\\UI AI hoặc đường dẫn tài khoản Windows.

Thứ tự tìm file đề xuất:

1. Biến môi trường AI_TASKBAR_PERSONA_PATH nếu được thiết lập.
2. File persona.json trong thư mục cấu hình ứng dụng của Tauri, lấy bằng app.path().app_config_dir().
3. Nếu không tìm thấy file, dùng persona trung tính mặc định trong memory.

Không tự động đọc file từ current working directory một cách mơ hồ. Nếu cần tiện phát triển, Dev có thể dùng AI_TASKBAR_PERSONA_PATH để trỏ tới config/persona.json trong workspace.

Đường dẫn thực tế phải được báo cáo khi chạy app, nhưng không ghi thông tin cá nhân hoặc nội dung persona đầy đủ vào log.

## 4. Schema persona.json

File mẫu chỉ dùng placeholder, không ghi persona thật của người dùng vào source:

    {
      "assistant_name": "AI Assistant",
      "creator": "",
      "user_name": "",
      "user_info": "",
      "tone_instruction": "Trả lời rõ ràng, ngắn gọn và hữu ích."
    }

Các trường:

- assistant_name: tên hiển thị/xưng danh của trợ lý.
- creator: người tạo hoặc chủ sở hữu trợ lý.
- user_name: tên người dùng muốn dùng trong hội thoại.
- user_info: thông tin mô tả bổ sung do người dùng tự cung cấp.
- tone_instruction: hướng dẫn phong cách trả lời.

Yêu cầu schema:

- Tất cả trường có thể để chuỗi rỗng.
- Không cho phép giá trị null, object hoặc array ở các trường chuỗi.
- Không cho frontend gửi hoặc ghi đè system message.
- Chỉ file local do người dùng cấu hình mới được đọc.
- Giới hạn độ dài trường để tránh tạo system prompt bất thường: assistant_name 80 ký tự, creator 120, user_name 120, user_info 2000, tone_instruction 2000.

## 5. Rust implementation

Tạo module riêng, đề xuất:

- src-tauri/src/persona.rs

Module cần có:

1. PersonaConfig với serde::Deserialize và Clone.
2. Hàm resolve_persona_path(app) theo thứ tự ưu tiên ở mục 3.
3. Hàm load_persona(app) để đọc UTF-8, parse JSON và validate.
4. Hàm build_system_message(config) để tạo đúng một system message.
5. Hàm fallback_neutral_persona() khi file không tồn tại.

Không đặt logic persona trực tiếp trong ai_generate hoặc ai_stream ngoài việc gọi module dùng chung.

## 6. Quy tắc fallback và lỗi

- File không tồn tại: dùng system message trung tính để AI vẫn hoạt động.
- File JSON sai cú pháp: trả lỗi cấu hình rõ ràng, không gửi persona dở dang lên Ollama.
- File không đọc được vì quyền hoặc lỗi I/O: trả lỗi rõ ràng, không im lặng dùng dữ liệu cũ.
- Trường vượt giới hạn: trả lỗi validation.
- Không log toàn bộ user_info hoặc tone_instruction.
- Không giữ persona cũ trong cache nếu lần đọc mới thất bại.

Fallback trung tính phải không chứa tên người dùng, tên creator hoặc dữ liệu cá nhân.

## 7. Tích hợp với H1 và H2

Trong cả ai_generate và ai_stream:

1. Load persona một lần ở đầu request.
2. Tạo system message từ persona.
3. Ghép theo thứ tự system persona, history user/assistant.
4. Không thêm system message trung tính thứ hai.
5. Không thay đổi event ai-stream-chunk, ai-stream-error hoặc request_id.
6. Không thay đổi AiTaskManager, generation cleanup hoặc ai_stop.

Backend vẫn phải từ chối role system do frontend gửi lên. System message duy nhất phải do backend tạo từ persona config.

Ví dụ request sau khi có persona:

    [
      { role: "system", content: "...persona đã render..." },
      { role: "user", content: "..." },
      { role: "assistant", content: "..." },
      { role: "user", content: "..." }
    ]

## 8. Cách render system message

Không nối chuỗi bằng các dòng trống hoặc nhãn có giá trị rỗng. Chỉ đưa phần có dữ liệu vào system message.

Cấu trúc gợi ý:

    You are [assistant_name].
    Created by [creator].
    The current user is [user_name].
    User information: [user_info].
    Response style: [tone_instruction].

Nếu các trường cá nhân để trống, chỉ dùng phần hướng dẫn trung tính. Không dùng câu gây hiểu nhầm rằng trợ lý có bộ nhớ lâu dài.

Persona là prompt instruction, không phải executable code. Không được dùng persona để tạo đường dẫn, chạy lệnh hoặc bỏ qua whitelist system command.

## 9. Frontend

H3 không bắt buộc thêm màn hình chỉnh persona.

Nếu Dev thêm trạng thái hiển thị, chỉ hiển thị:

- Persona đang hoạt động hoặc fallback trung tính.
- Tên file hoặc trạng thái cấu hình.

Không hiển thị user_info trong log, tooltip hoặc event global.

Không đưa persona vào committedMessages. Persona thuộc system context, không phải message người dùng.

## 10. Reload và lifecycle

Ở H3, đọc file một lần cho mỗi request là phương án ưu tiên:

- Không cần file watcher.
- Thay đổi persona có hiệu lực từ request kế tiếp.
- Tránh cache cũ khi người dùng chỉnh file.
- File nhỏ nên chi phí đọc không đáng kể.

Không cần restart app sau khi sửa persona.json. Không tự động ghi đè file người dùng.

## 11. Kiểm thử tự động

Chạy:

    npm run lint
    npm run build
    cargo check

Unit test Rust tối thiểu:

1. Parse file hợp lệ với đủ trường.
2. Parse file có trường rỗng.
3. File thiếu trường được xử lý theo schema đã chọn.
4. JSON sai cú pháp trả lỗi.
5. Chuỗi vượt giới hạn bị từ chối.
6. Role system từ frontend bị từ chối.
7. System message được tạo đúng một lần.
8. Persona lỗi không làm giữ lại config cũ.
9. Fallback không chứa dữ liệu cá nhân.

## 12. Kiểm thử thủ công

1. Tạo persona.json ngoài source với assistant_name là tên thử nghiệm.
2. Chạy app và gửi prompt mới; xác nhận AI dùng đúng tên/phong cách.
3. Gửi câu hỏi tiếp theo; xác nhận history H2 vẫn hoạt động.
4. Sửa tone_instruction rồi gửi request mới; xác nhận thay đổi có hiệu lực.
5. Xoá file; xác nhận app dùng fallback trung tính và vẫn chat được.
6. Tạo JSON sai; xác nhận UI nhận lỗi rõ ràng, không gửi request persona dở dang.
7. Thử cấu hình có role system trong messages từ frontend; backend phải từ chối.
8. Restart app; persona được đọc lại từ file, còn conversation history vẫn bị xoá.
9. Kiểm tra log không chứa user_info hoặc toàn bộ tone_instruction.
10. Xác nhận AI Orb, radial menu, audio/display/connectivity và click-through không bị ảnh hưởng.

## 13. Không làm trong H3

- Không triển khai memory lâu dài hoặc lưu thông tin người dùng tự động.
- Không tự trích xuất facts từ hội thoại.
- Không lưu history vào disk.
- Không thêm token counting hoặc history trimming; thuộc H5.
- Không thêm generation options mới; thuộc H4.
- Không thay đổi model, port hoặc benchmark backend.
- Không thêm tool calling hoặc system action mới.

## 14. Báo cáo Dev bắt buộc

Dev phải báo cáo riêng H3, không gộp H4–H5:

- File đã sửa/thêm.
- Schema persona cuối cùng.
- Quy tắc resolve đường dẫn và fallback.
- Cách validate, giới hạn độ dài và xử lý lỗi.
- Ví dụ system message sau khi render, dùng dữ liệu placeholder.
- Xác nhận system message chỉ xuất hiện một lần.
- Xác nhận history H2, request_id, cancel và streaming không bị thay đổi.
- Kết quả lint, build, cargo check và unit test.
- Kết quả manual test sửa, xoá và làm hỏng config.
- Xác nhận chưa làm H4 và H5.

Dev phải dừng sau báo cáo để Codex nghiệm thu.

## 15. Tiêu chí nghiệm thu H3

H3 chỉ đạt khi persona được đọc từ file bên ngoài, không hardcode dữ liệu cá nhân; đường dẫn không phụ thuộc máy phát triển; system message được tạo đúng một lần trước history; thay đổi file có hiệu lực ở request kế tiếp; fallback và lỗi cấu hình an toàn; không làm hỏng H1/H2; không lưu history hoặc persona ngoài chủ đích; và toàn bộ kiểm thử đều pass.
