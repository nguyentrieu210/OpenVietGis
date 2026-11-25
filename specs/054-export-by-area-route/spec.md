# Feature Specification: Xuất dữ liệu theo ấp và theo tuyến đường

**Feature Branch**: `spec/053-binh-hung-data-complete` (spec 054 viết cùng nhánh)

**Created**: 2026-11-25

**Status**: Draft

**Parent**: R026 Integration Hub → [Spec 052](../052-integration-batch-export/spec.md) (khối "Xuất dữ liệu" trong "Kết nối phần mềm").

**Input**: Chủ dự án: "Thêm tính năng xuất dữ liệu theo ấp, theo tuyến đường." Câu hỏi "theo tuyến lấy gì": theo mã/tên tuyến (như app Bình Hưng); lớp không có trường tuyến thì theo khoảng cách.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Xuất một ấp (Priority: P1)

Trong khối "Xuất dữ liệu", người dùng chọn Phạm vi "Theo ấp", chọn một hay nhiều ấp từ danh sách (lấy từ lớp ranh ấp của dự án), tích các lớp, bấm tải; mỗi lớp chỉ chứa đối tượng nằm trong ấp đã chọn.

**Why this priority**: Hồ sơ nộp xã và giao việc khảo sát làm theo ấp.

**Independent Test**: Chọn Ấp 27, xuất lớp khảo sát và thửa đất; so số đối tượng với đếm trên bản đồ.

**Acceptance Scenarios**:

1. **Given** chọn Ấp 27, **When** xuất, **Then** chỉ có đối tượng giao với ranh Ấp 27; đối tượng trên ranh giới hai ấp được tính cho cả hai.
2. **Given** chọn nhiều ấp, **When** xuất, **Then** là hợp của các ấp, không trùng đối tượng.
3. **Given** dự án chưa có lớp ranh ấp, **When** mở phạm vi, **Then** "Theo ấp" bị khóa kèm lý do.

### User Story 2 - Xuất một tuyến đường (Priority: P1)

Người dùng chọn Phạm vi "Theo tuyến", tìm và chọn một tuyến (tên đường/hẻm); lớp có trường tuyến (`tuyen_id` hoặc `ten_duong`) lấy đúng đối tượng của tuyến đó; lớp không có trường tuyến lấy đối tượng trong khoảng cách chọn được (mặc định 30 m) tới đường tuyến.

**Why this priority**: Lưới số nhà và phiếu kiểm tra làm theo tuyến/hẻm.

**Independent Test**: Chọn "Đường Phạm Hùng", xuất lớp khảo sát (có trường tuyến) và thửa đất (không có); kiểm số lượng.

**Acceptance Scenarios**:

1. **Given** lớp có `tuyen_id`, **When** xuất theo tuyến, **Then** lấy đúng các đối tượng có `tuyen_id` của tuyến; không có `tuyen_id` thì so `ten_duong` không phân biệt dấu/hoa thường.
2. **Given** lớp không có trường tuyến, **When** xuất theo tuyến, **Then** lấy đối tượng cách tuyến không quá khoảng cách đã chọn, và tệp/kết quả ghi rõ "lọc theo khoảng cách N m".
3. **Given** tên tuyến có nhiều đoạn, **When** chọn, **Then** gộp đủ các đoạn cùng tên.

### Edge Cases

- Phạm vi không có đối tượng nào: báo "0 đối tượng", không tạo tệp rỗng.
- Tên tệp ghi phạm vi, ví dụ `khao-sat-so-nha-binh-hung-ap-27-qgis.gpkg`, `…-duong-pham-hung-…`.
- Quyền và giới hạn đối tượng giữ như xuất toàn lớp; lọc chạy ở máy chủ, không tải cả lớp về trình duyệt.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: Khối "Xuất dữ liệu" MUST có Phạm vi: Toàn bộ / Theo ấp / Theo tuyến.
- **FR-002**: "Theo ấp" MUST lọc theo giao với ranh ấp đã chọn (một hoặc nhiều), lấy từ lớp ranh ấp của dự án.
- **FR-003**: "Theo tuyến" MUST lọc theo trường tuyến khi lớp có, ngược lại theo khoảng cách tới tuyến (mặc định 30 m, chỉnh được 1–200 m).
- **FR-004**: Lọc MUST chạy ở máy chủ, áp dụng cho mọi định dạng xuất (GeoPackage, GeoJSON, SQL, DXF) và tôn trọng quyền xuất như hiện tại.
- **FR-005**: Tên tệp MUST ghi phạm vi bằng chữ không dấu.
- **FR-006**: Kết quả MUST báo số đối tượng mỗi lớp và cách lọc đã dùng.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Xuất một ấp cho 3 lớp trong tối đa 4 thao tác sau khi mở tab.
- **SC-002**: Số đối tượng xuất theo ấp khớp đếm không gian trên cùng dữ liệu (sai lệch 0).
- **SC-003**: Xuất theo tuyến cho lớp khảo sát khớp 100% với lọc `tuyen_id` trên dữ liệu.

## Assumptions

- Lớp ranh ấp là "Ranh giới ấp Bình Hưng" (39 ấp), lớp tuyến là các lớp tuyến của dự án; dự án khác chọn lớp ranh/tuyến tương ứng.
- Phụ thuộc Spec 052 (đã live) và giới hạn đối tượng của Spec 053.
