# Feature Specification: Xuất nhiều lớp ngay trong "Kết nối phần mềm"

**Feature Branch**: `feat/052-integration-batch-export`

**Created**: 2026-11-25

**Status**: Draft

**Parent**: R010/R026 Integration Hub (hộp thoại "⇄ Kết nối phần mềm").

**Input**: Chủ dự án: "Cho nút tải QGIS, AutoCAD trực tiếp trên nút kết nối phần mềm … cho nó cả vào xuất dữ liệu luôn: kết nối plugin kèm xuất dữ liệu, xuất nhiều lớp, xuất hàng loạt." Câu hỏi AutoCAD tải gì: bản vẽ DXF của dự án (chưa có plugin AutoCAD).

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Chọn nhiều lớp và tải một lần cho phần mềm đang dùng (Priority: P1)

Người dùng mở "Kết nối phần mềm", chọn tab QGIS, PostGIS hoặc AutoCAD/Map 3D. Ngay dưới phần kết nối plugin có khối "Xuất dữ liệu": chọn định dạng, tích các lớp (hoặc "Chọn tất cả"), bấm "Tải N lớp"; mỗi lớp được tải thành một tệp, có tiến độ và kết quả từng lớp.

**Why this priority**: Hiện chỉ xuất được từng lớp, phải qua hai bước chọn thao tác, và với QGIS còn nằm trong mục thu gọn.

**Independent Test**: Dự án có 8 lớp, chọn tất cả ở tab QGIS dạng GeoPackage, bấm tải: nhận 8 tệp; ở tab AutoCAD chỉ các lớp hệ mét được chọn, nhận DXF.

**Acceptance Scenarios**:

1. **Given** tab có xuất dữ liệu, **When** mở tab, **Then** khối "Xuất dữ liệu" hiện ngay, không cần chọn bước nào.
2. **Given** chọn 3 lớp, **When** bấm "Tải 3 lớp", **Then** tải lần lượt 3 tệp, hiện "Đang tải 2/3 …", cuối cùng báo số lớp thành công và tên lớp lỗi kèm lý do.
3. **Given** lớp không được phép xuất hoặc không hợp định dạng (DXF cần hệ tọa độ phẳng mét; GeoJSON cần EPSG:4326), **When** hiển thị, **Then** ô chọn bị khóa và ghi lý do ngắn.
4. **Given** đổi định dạng, **When** lớp đã chọn không còn hợp, **Then** lớp đó tự bỏ chọn.
5. **Given** đang tải, **When** đổi dự án hoặc đóng hộp thoại, **Then** dừng tải các lớp còn lại.

### Edge Cases

- Không có lớp nào xuất được: khối hiện thông báo, nút tải bị khóa.
- Một lớp lỗi không làm dừng các lớp sau.
- Trình duyệt hỏi cho phép tải nhiều tệp: tệp đầu vẫn tải, người dùng cho phép thì tải tiếp.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: Tab QGIS, PostGIS, AutoCAD/Map 3D MUST có khối "Xuất dữ liệu" luôn hiện dưới phần kết nối plugin.
- **FR-002**: Định dạng theo phần mềm: QGIS — GeoPackage, GeoJSON; PostGIS — SQL, GeoPackage; AutoCAD — DXF.
- **FR-003**: Người dùng MUST chọn được nhiều lớp và "Chọn tất cả" các lớp hợp lệ.
- **FR-004**: Chỉ lớp được phép xuất và hợp định dạng mới chọn được; quy tắc giống nút xuất từng lớp hiện có.
- **FR-005**: Tải hàng loạt MUST tải từng lớp một, báo tiến độ, tiếp tục khi một lớp lỗi, tổng kết thành công/lỗi.
- **FR-006**: Quyền và phạm vi dự án do máy chủ kiểm như xuất từng lớp; giao diện không mở thêm quyền nào.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Xuất toàn bộ lớp của một dự án cho một phần mềm trong tối đa 3 thao tác sau khi mở tab (chọn định dạng, chọn tất cả, tải).
- **SC-002**: 100% lớp không hợp lệ bị khóa kèm lý do; 0 yêu cầu xuất bị máy chủ từ chối vì giao diện cho chọn sai.
- **SC-003**: Với 8 lớp, một lớp lỗi vẫn ra đủ 7 tệp và báo đúng lớp lỗi.

## Assumptions

- Mỗi lớp một tệp; chưa gộp nhiều lớp vào một tệp GeoPackage/DXF hay một tệp nén (cần thay đổi máy chủ, làm sau nếu cần).
- Nút "Tải công cụ QGIS" và "Lấy mã kết nối" giữ nguyên ở phần kết nối QGIS; AutoCAD dùng DXF, chưa có plugin.
- Phần xuất từng lớp và đồng bộ .gpkg cũ giữ nguyên.
