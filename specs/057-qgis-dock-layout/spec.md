# Feature Specification: Bố cục khung kiểu QGIS (đổi kích thước, gắn khung)

**Feature Branch**: `feat/057-resizable-docks`
**Created**: 2026-11-25
**Status**: Đang làm — bước A
**Parent**: R004 (Map Workspace).
**Input**: Chủ dự án: "cái resize và bố cục nữa … Qgis có gì nó có hết" — chọn: bảng thuộc tính, khung bên phải, gắn khung tự do, Trình duyệt & Lớp.

## Requirements

**Bước A — đổi kích thước**
- **FR-001**: Kéo mép trên bảng thuộc tính MUST đổi chiều cao (110–900 px, ≤ 80% màn hình).
- **FR-002**: Kéo mép trái khung thông tin đối tượng MUST đổi chiều rộng (220–720 px).
- **FR-003**: Kéo mép phải khung plugin (Địa bàn số nhà) MUST đổi chiều rộng (240–720 px).
- **FR-004**: Kéo vạch dưới Trình duyệt MUST đổi chiều cao phần Trình duyệt so với Lớp dữ liệu.
- **FR-005**: Mọi vạch kéo dùng được bằng bàn phím (mũi tên), nhấp đúp để về mặc định, nhớ kích thước trên máy; bản đồ tự vẽ lại sau khi đổi.

**Bước B — gắn khung tự do (đợt sau)**
- **FR-010**: Khung Lớp, Trình duyệt, Thông tin, Bảng thuộc tính, Plugin kéo gắn được sang trái / phải / dưới hoặc nổi; xếp chồng thành tab; ẩn/hiện qua menu Khung; lưu và khôi phục bố cục.

## Success Criteria
- **SC-001**: Đổi kích thước 4 khung, tải lại trang: kích thước giữ nguyên.
