# Feature Specification: Phòng trò chuyện tổ chức (chat rooms)

**Feature Branch**: `feat/060-chat-rooms`
**Created**: 2026-11-25
**Status**: Đang làm — phía máy chủ
**Input**: Chủ dự án: trò chuyện cấp tổ chức — phòng chung, nhóm, nhắn riêng, danh bạ, thả cảm xúc, nhận tin ngay.

## Requirements
Gốc API: `/api/v1/organizations/{org_id}/chat`. Mọi lệnh cần thành viên đang hoạt động của tổ chức; lệnh ghi cần CSRF. Không phải thành viên phòng → 404.

- **FR-001**: Mỗi tổ chức có đúng một phòng chung (`org`), tạo lười khi gọi `GET /rooms`, an toàn khi gọi đồng thời; mọi thành viên đang hoạt động đều ở trong phòng.
- **FR-002**: Thành viên tạo phòng nhóm (tên 1–80 ký tự, 1–49 người mời là thành viên đang hoạt động); người tạo là quản trị phòng. Tối đa 50 người/phòng.
- **FR-003**: Thành viên nào trong nhóm cũng mời được người; chỉ quản trị phòng đổi tên hoặc gỡ người; ai cũng tự rời được. Quản trị cuối cùng rời thì người vào sớm nhất thành quản trị.
- **FR-004**: Phòng riêng 1–1 là duy nhất cho mỗi cặp người trong tổ chức; không tạo với chính mình.
- **FR-005**: `GET /rooms` trả phòng của người gọi theo hoạt động gần nhất, kèm `member_count`, `unread`, `mentioned`, `last_message`, `peer{id,display_name,has_avatar}`.
- **FR-006**: Tin nhắn 1–4000 ký tự (cắt khoảng trắng), phân trang `limit` 1–100 (mặc định 50) và `before`, trả tăng dần; `POST /read` đánh dấu đã đọc.
- **FR-007**: Trả lời (`reply_to_id` cùng phòng) và nhắc tên (`mention_ids` ≤ 20, phải là thành viên phòng).
- **FR-008**: Thả cảm xúc bật/tắt theo (tin, người, emoji ≤ 16 ký tự), lưu bảng riêng.
- **FR-009**: Ghim tin: thành viên phòng ghim; bỏ ghim — nhóm: quản trị hoặc tác giả; phòng chung và riêng: ai cũng được. Tối đa 20 ghim/phòng (409).
- **FR-010**: Danh bạ toàn cục theo cặp người; chỉ gửi lời mời cho người chung tổ chức; gửi lại không tạo trùng; nếu bên kia đã mời thì tự chấp nhận; chỉ người nhận chấp nhận/từ chối; xoá liên hệ được.
- **FR-011**: `GET /people` tìm thành viên (không gồm mình) không phân biệt hoa thường, dấu; tối đa 50; có `has_avatar`, `contact`, `request`.
- **FR-012**: `GET /people/{user_id}/avatar` chỉ cho người cùng tổ chức, `Cache-Control: private, max-age=300`.
- **FR-013**: `GET /events` (SSE) phát `event: chat` `{room_id}` khi có tin, cảm xúc, ghim trong phòng người gọi thuộc về; `event: contacts` khi lời mời/liên hệ của người gọi đổi. Cùng giới hạn kết nối, nhịp tim và kiểm tra lại quyền như `team_realtime`.
- **FR-014**: Ghi nhật ký kiểm toán khi tạo/đổi tên phòng, thêm/gỡ người, lời mời và liên hệ.

## Success Criteria
- **SC-001**: Kiểm thử tích hợp phủ: phòng chung tự vào, nhóm tạo/mời/rời/đổi tên, 404 cho người ngoài phòng, phòng riêng duy nhất, thứ tự tin + chưa đọc + đã đọc, cảm xúc, ghim, tìm người, lời mời → chấp nhận/từ chối/tự chấp nhận, cách ly giữa tổ chức.
- **SC-002**: Tin mới tới trình duyệt khác qua SSE không cần tải lại.

## Security
- RLS: bảng phòng/tin/cảm xúc/ghim gắn `organization_id` của phiên; danh bạ và lời mời chỉ thấy được khi người gọi là một trong hai bên.
- Ảnh đại diện: thêm chính sách đọc cho người cùng tổ chức đang hoạt động; chính sách chỉ-mình giữ nguyên.
- NOTIFY chỉ chứa mã, không chứa nội dung tin; SSE lọc theo phòng sau khi xác thực lại.
- Không lộ người ngoài tổ chức: mọi id người dùng nhận vào đều kiểm là thành viên đang hoạt động.
