# Feature Specification: Nút xóa dự án, tổ chức, đối tượng

**Feature Branch**: `feat/055-delete-controls`
**Created**: 2026-11-25
**Status**: Đang làm
**Input**: Chủ dự án: "cho tao nút xoá dự án xoá tổ chức xoá điểm và đối tượng". Chọn: xóa mềm, gõ tên để xác nhận.

## Clarifications

### Session 2026-11-25

- Xóa dự án / tổ chức trên giao diện là **xóa mềm** (trạng thái `deleted`, đã có trong Core): ẩn ngay với mọi thành viên, dữ liệu còn trong CSDL để cứu khi bấm nhầm. Người dùng phải **gõ đúng tên** mới bấm được.
- Xóa vĩnh viễn (dọn CSDL) không làm trên giao diện; chỉ quản trị hệ thống làm theo yêu cầu.

## User Scenarios

1. Chủ hoặc quản trị tổ chức mở menu tài khoản → "Xóa dự án…" → gõ tên dự án → dự án biến mất khỏi danh sách, mở dự án còn lại.
2. Chủ tổ chức → "Xóa tổ chức…" → gõ tên → tổ chức biến mất, chuyển sang tổ chức còn lại.
3. Người có quyền sửa lớp bấm một đối tượng → "🗑 Xóa đối tượng" → xác nhận → đối tượng bị xóa qua changeset (có lịch sử, khôi phục được).
4. Người dùng chọn nhiều đối tượng (chọn theo khung / đa giác) → nút "🗑 Xóa N đã chọn" trên thanh công cụ → xác nhận → xóa theo từng lớp.

## Requirements

- **FR-001**: Menu tài khoản MUST có "Xóa dự án…" cho chủ/quản trị tổ chức và "Xóa tổ chức…" chỉ cho chủ; máy chủ kiểm lại quyền `project.delete` / `organization.delete`.
- **FR-002**: Hộp thoại xóa MUST nói rõ phạm vi bị xóa và chỉ cho bấm khi tên gõ vào trùng đúng tên.
- **FR-003**: Khung thông tin đối tượng MUST có nút "🗑 Xóa đối tượng" (dùng lệnh ERASE có xác nhận của Core), tắt khi không có quyền sửa lớp.
- **FR-004**: Khi có đối tượng đang chọn, thanh công cụ MUST có "🗑 Xóa N đã chọn"; xác nhận nêu số đối tượng và tên lớp; mỗi lớp một changeset; từ chối cả thao tác nếu có lớp không có quyền sửa.
- **FR-005**: Điểm thuộc ấp/tuyến đã ký đối chiếu (spec 051) vẫn bị khóa; xóa trả lỗi 409 bằng tiếng Việt.

## Success Criteria

- **SC-001**: Mỗi thao tác xóa tối đa 3 bước từ màn hình bản đồ.
- **SC-002**: Không thể xóa dự án/tổ chức khi gõ sai tên (nút tắt), và máy chủ từ chối người không có quyền.
