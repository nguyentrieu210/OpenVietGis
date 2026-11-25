# Plan: 060 chat rooms

- Migration `0043_chat_rooms`: bảng `collaboration.chat_rooms`, `chat_room_members`, `chat_messages`, `chat_reactions`, `chat_pins`, `chat_contacts`, `chat_contact_requests`; RLS + quyền cho `openvietgis_runtime`; trigger NOTIFY kênh `ovg_chat`; chính sách xem avatar cùng tổ chức.
- Router `adapters/http/org_chat.py` (kiểu `team_chat` + `team_realtime`), đăng ký trong `main.py` (cấp tổ chức, không qua `project_access_guard`).
- Quyền: `require_member_context` (thành viên đang hoạt động) → 404 nếu không.
- Kiểm thử: `tests/integration/spatial/test_org_chat.py`, chạy trên VPS với Postgres dùng một lần.
