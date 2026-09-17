# H2 — Conversation History cho trợ lý AI

**Người thực hiện:** Dev AI  
**Người kiểm tra:** Codex  
**Tiền điều kiện:** H1 đã được nghiệm thu Đạt

## 1. Mục tiêu

Thêm lịch sử hội thoại trong phiên chạy hiện tại. Khi người dùng gửi lượt mới, backend phải nhận được các lượt user/assistant đã hoàn tất trước đó qua Ollama /api/chat.

H2 chưa triển khai persona, file cấu hình, generation options mới, token counting hoặc history trimming.

## 2. Hiện trạng đã rà soát

- Backend trong src-tauri/src/commands.rs đã dùng /api/chat nhưng hiện chỉ nhận một prompt.
- Hàm build_chat_messages hiện tạo system message trung tính và một user message.
- TaskbarShell.tsx hiện chỉ có prompt và resultText, chưa có conversation history.
- Event ai-stream-chunk và ai-stream-error đã có request_id, text và done.
- AiTaskManager đã có generation token, JoinHandle và ai_stop.

## 3. Kiến trúc được duyệt

### 3.1. Frontend là nguồn dữ liệu history

- Giữ history trong React state, không đưa history vào AiTaskManager.
- Backend stateless giữa các request.
- Không ghi history vào file, registry, localStorage hoặc database trong H2.
- Đóng hoặc restart app thì history được xoá.
- System message không hiển thị trong UI.

### 3.2. Tách history hoàn tất và lượt đang chạy

Dùng hai nhóm dữ liệu:

- committedMessages: các message user/assistant đã hoàn tất.
- pendingTurn: requestId, user content và assistant content đang stream.

Chỉ khi nhận done=true và requestId vẫn là active request thì mới commit cặp user/assistant vào history.

Nếu stop, lỗi hoặc bị request mới thay thế thì discard pendingTurn. Không đưa assistant response chưa hoàn chỉnh vào history.

## 4. Frontend data model

Tạo type tương đương:

    type ChatRole = 'user' | 'assistant';
    type ConversationMessage = { id: string; role: ChatRole; content: string };

Có thể tách logic vào src/hooks/useConversation.ts. Nếu giữ trong TaskbarShell.tsx, phải tách rõ các hàm beginTurn, appendAssistantChunk, commitTurn, discardTurn và clearConversation.

Không thêm role system vào history UI.

## 5. Thay đổi Tauri command

Đổi ai_generate và ai_stream để nhận danh sách message thay cho prompt đơn:

    ai_stream(state, messages: Vec<AiChatMessage>, params: AiGenerateParams)

AiChatMessage gồm role và content. Tên field phải thống nhất giữa TypeScript và Rust.

Backend phải:

1. Nhận history user/assistant từ frontend.
2. Thêm system message trung tính ở đầu danh sách.
3. Chỉ chấp nhận role user và assistant; role khác phải trả lỗi.
4. Giữ nguyên thứ tự message.
5. Không tự nhân đôi user message hiện tại.
6. Không thêm assistant giả.

Request thứ hai phải có thứ tự: system, user A, assistant A, user B.

## 6. Luồng gửi message

1. Kiểm tra prompt không rỗng.
2. Stop request cũ nếu đang chạy.
3. Sinh UUID mới.
4. Tạo requestMessages từ committedMessages và user message mới.
5. Tạo pendingTurn gắn với UUID.
6. Gửi requestMessages qua ai_stream.
7. Nối chunk đúng requestId vào pending assistant.
8. Khi done=true, commit lượt chat.
9. Khi lỗi hoặc stop, discard pendingTurn.

Frontend phải giữ nguyên lọc activeRequestId. Chunk muộn của request cũ không được cập nhật pendingTurn hoặc committedMessages.

## 7. UI bắt buộc

- Hiển thị các message user và assistant đã hoàn tất.
- Hiển thị assistant response đang stream.
- Hiển thị lỗi ngoài history.
- Có nút Clear conversation.
- Clear phải stop request hiện tại, xoá committedMessages và pendingTurn.
- Không hiển thị system message.
- Không thay đổi AI Orb, radial menu, drag hoặc click-through.

## 8. Không làm trong H2

- Không thêm persona.json hoặc persona thật.
- Không thêm repeat_penalty, num_ctx, num_predict mới.
- Không trim theo token hoặc context; việc này thuộc H5.
- Không lưu history lâu dài.
- Không đổi model, port hoặc backend benchmark.
- Không thêm function calling hoặc tool calling.

## 9. Kiểm thử tự động

Chạy từ ai-taskbar:

    npm run lint
    npm run build

Chạy từ ai-taskbar/src-tauri:

    cargo check

Kiểm tra logic: history rỗng, nhiều lượt đúng thứ tự, role sai bị từ chối, done mới commit, request cũ không commit, stop không làm bẩn history và clear xoá sạch state.

## 10. Kiểm thử thủ công

1. Gửi: Tên tôi là An.
2. Chờ trả lời hoàn tất.
3. Gửi: Tôi vừa nói tên gì?
4. Xác nhận request thứ hai có lượt đầu và AI trả lời đúng.
5. Thử nhiều lượt liên tiếp.
6. Stop giữa stream; response dở không được commit.
7. Gửi request mới khi request cũ đang chạy; không được trộn text.
8. Tắt Ollama giữa stream; pending turn bị huỷ và lỗi hiển thị riêng.
9. Clear conversation; request tiếp theo không còn history cũ.
10. Restart app; history không được khôi phục từ ổ đĩa.

## 11. Báo cáo Dev bắt buộc

Dev phải báo cáo riêng H2, không gộp H3–H5:

- File đã sửa/thêm.
- Nơi lưu history và lý do.
- Contract mới của ai_generate và ai_stream.
- Payload thực tế của request thứ hai.
- Cách xử lý committedMessages và pendingTurn.
- Cách chống request cũ ghi đè.
- Kết quả stop, lỗi stream và clear.
- Kết quả lint, build, cargo check và từng manual test.
- Xác nhận chưa làm H3, H4, H5.

Dev phải dừng sau báo cáo để Codex nghiệm thu.

## 12. Tiêu chí nghiệm thu

H2 chỉ đạt khi request mới nhận đúng các lượt đã hoàn tất; thứ tự system, user, assistant, user chính xác; không trộn request; stop/lỗi không làm bẩn history; clear hoạt động; history không ghi ra ổ đĩa; không phát sinh H3–H5; và toàn bộ kiểm thử đều pass.
