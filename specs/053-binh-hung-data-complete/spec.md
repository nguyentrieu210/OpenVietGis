# Feature Specification: Đủ dữ liệu Bình Hưng trong một dự án và lớp lớn dùng được

**Feature Branch**: `spec/053-binh-hung-data-complete`

**Created**: 2026-11-25

**Status**: Draft

**Parent**: R005 Import & Export, R004 Map Workspace, R010/R026 Integration Hub, R014 → [Spec 051](../051-house-numbering-parity/spec.md) (US8 nạp dữ liệu thật).

**Input**: Chủ dự án (25/11): "Gộp toàn bộ 4 lớp số nhà vào thành 1 lớp tên là Khảo sát số nhà Bình Hưng" (chọn "gộp thật, nâng ngưỡng"); "Xem bản cũ còn nhiều lớp chưa import" — dữ liệu cũ ở máy local.

## Clarifications

### Session 2026-11-25

- App Bình Hưng cũ đã bị gỡ khỏi máy chủ cũ; nguồn dữ liệu là bản dump đầy đủ ngày 17/11 trên máy của chủ dự án (PostgreSQL 16, có schema `gis_public` là các view không chứa dữ liệu cá nhân).
- Bốn lớp "Khảo sát số nhà · Phần 01–04" (33.497 điểm) là lớp "Chấm điểm số nhà" của app cũ (33.499 dòng), bị chia vì giới hạn 10.000–25.000 đối tượng của bản đồ, xuất và đồng bộ QGIS.
- Chủ dự án chọn gộp thật thành một lớp và nâng giới hạn, không chỉ gom hiển thị.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Một lớp "Khảo sát số nhà Bình Hưng" dùng như mọi lớp (Priority: P1)

Người dùng thấy một lớp "Khảo sát số nhà Bình Hưng" thay cho bốn phần; bật, xem nhãn số nhà, bấm chọn, tìm kiếm, sửa, xuất GeoPackage/DXF và đồng bộ QGIS như với lớp nhỏ.

**Why this priority**: Đây là yêu cầu trực tiếp; danh sách bốn phần làm rối và mọi thao tác phải làm bốn lần.

**Independent Test**: Trên bản sao dữ liệu live, gộp bốn phần, mở bản đồ, kiểm số điểm, nhãn, tìm "190", xuất GPKG và DXF, ghép lại kết nối QGIS và đồng bộ một thay đổi.

**Acceptance Scenarios**:

1. **Given** bốn phần đang có, **When** gộp, **Then** có một lớp 33.497 điểm (cộng các điểm bị thiếu nếu tìm lại được), đủ 23 trường, mỗi điểm giữ mã đối tượng và lịch sử truy vết về phần cũ.
2. **Given** lớp gộp, **When** mở bản đồ ở mức phường/xã, **Then** điểm và nhãn số nhà hiện như với lớp nhỏ, không chuyển sang chế độ mất nhãn.
3. **Given** lớp gộp, **When** xuất GeoPackage hoặc DXF, **Then** ra đủ điểm trong một tệp.
4. **Given** hai kết nối QGIS đang gắn với Phần 01 và Phần 03, **When** gộp, **Then** các máy đó được chuyển sang lớp mới hoặc được báo rõ phải ghép lại, không mất chỉnh sửa đang chờ.
5. **Given** bốn phần cũ, **When** gộp xong, **Then** chúng bị ngừng dùng (khôi phục được), không bị xóa cứng.

### User Story 2 - Nhập đủ các lớp còn thiếu của app cũ (Priority: P1)

Dự án "Khảo sát số nhà xã Bình Hưng" có thêm các lớp app cũ đã có: Thửa đất, Số nhà theo chứng nhận, Số nhà trên bản vẽ lưới, Số nhà theo nhà (đề xuất, duyệt), Nhà (công trình), Số nhà cũ, Khu có hồ sơ số nhà, Lưới số nhà (mốc), Hiện trạng sử dụng đất, Ấp cũ.

**Why this priority**: Rà soát và đánh số (Spec 051) cần các lớp này; app cũ không còn chạy.

**Independent Test**: So số đối tượng từng lớp sau nhập với bản dump.

**Acceptance Scenarios**:

1. **Given** bản dump, **When** nhập, **Then** mỗi lớp có đúng số đối tượng như view tương ứng trong `gis_public` của app cũ, đúng hệ tọa độ, đúng tên trường.
2. **Given** dữ liệu có tên chủ, CCCD, số điện thoại, **When** nhập lớp, **Then** các trường đó không có trong lớp (chỉ nhập từ các view không chứa dữ liệu cá nhân); việc đưa tên chủ vào theo Spec 051 FR-071 làm riêng.
3. **Given** lớp đã tồn tại tương đương (ranh xã, ranh ấp, tuyến), **When** nhập, **Then** không tạo bản trùng; chênh lệch số lượng (tuyến 2.286 so với 2.287) được ghi rõ.

### User Story 3 - Lớp lớn đến 50.000 đối tượng dùng được bình thường (Priority: P1)

Bản đồ, nhãn, chọn, xuất, truy vấn và đồng bộ QGIS hoạt động với lớp đến 50.000 đối tượng.

**Why this priority**: Là điều kiện để gộp (33.497) và để nhập Thửa đất (19.064), Số nhà cũ (14.428), Nhãn bản vẽ (10.570) mà không phải chia nhỏ.

**Independent Test**: Lớp 50.000 điểm có nhãn: mở bản đồ, bấm, tìm, xuất GPKG/DXF, đồng bộ QGIS; đo thời gian.

**Acceptance Scenarios**:

1. **Given** lớp 50.000 điểm, **When** mở bản đồ, **Then** điểm và nhãn hiện ở mức zoom làm việc; ở mức nhìn rộng được phép gộp/ẩn nhãn nhưng không mất điểm.
2. **Given** lớp 50.000 điểm, **When** xuất GeoPackage, GeoJSON, SQL hoặc DXF, **Then** ra đủ đối tượng hoặc báo rõ lý do, không cắt im lặng.
3. **Given** lớp lớn hơn giới hạn mới, **When** thao tác, **Then** báo rõ giới hạn và gợi ý, không treo trình duyệt.

### Edge Cases

- Hai điểm khảo sát thiếu (33.499 so với 33.497): tìm theo mã gốc, ghi lý do, bổ sung nếu hợp lệ.
- Điểm trùng mã giữa các phần: không được trùng sau gộp; nếu có thì báo và giữ cả hai có đánh dấu.
- Đang có người sửa một phần lúc gộp: gộp chờ hoặc dừng, không mất thay đổi.
- Bản dump khác phiên bản PostgreSQL/PostGIS: khôi phục ở môi trường tạm riêng, không vào CSDL live.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: Hệ thống MUST gộp nhiều lớp cùng cấu trúc thành một lớp mới có tên do người dùng đặt, giữ mã đối tượng, thuộc tính và nguồn gốc.
- **FR-002**: Gộp MUST là một thay đổi có lịch sử, sao lưu được và đảo ngược được; lớp cũ ngừng dùng, không xóa cứng.
- **FR-003**: Kết nối đồng bộ QGIS gắn với lớp cũ MUST được chuyển sang lớp mới hoặc báo rõ cần ghép lại, không mất thay đổi đang chờ.
- **FR-004**: Bản đồ MUST hiển thị điểm và nhãn cho lớp đến 50.000 đối tượng ở mức zoom làm việc.
- **FR-005**: Xuất (GeoPackage, GeoJSON, SQL, DXF), truy vấn, xử lý và đồng bộ QGIS MUST chấp nhận lớp đến 50.000 đối tượng.
- **FR-006**: Vượt giới hạn MUST báo rõ, không cắt im lặng.
- **FR-007**: Hệ thống MUST nhập được 10 lớp còn thiếu từ bản dump app cũ qua đường nhập dữ liệu chuẩn (có nguồn gốc, kiểm hệ tọa độ, ghi lịch sử).
- **FR-008**: Dữ liệu cá nhân MUST NOT đi vào các lớp nhập ở spec này.
- **FR-009**: Mọi ghi vào CSDL live MUST có sao lưu trước và cách quay lại được ghi lại.

### Key Entities

- **Lớp gộp**: tên, nguồn, các lớp nguồn, số đối tượng, nguồn gốc từng đối tượng.
- **Lớp nhập từ app cũ**: tên, view nguồn, số đối tượng, hệ tọa độ, trường.
- **Kết nối đồng bộ QGIS**: lớp đích, máy, thay đổi đang chờ.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Danh sách lớp dự án Bình Hưng có một lớp khảo sát thay cho bốn; số điểm khớp tổng bốn phần (cộng phần bổ sung có ghi lý do).
- **SC-002**: Mỗi lớp nhập có số đối tượng khớp 100% với view nguồn trong bản dump.
- **SC-003**: Lớp 50.000 điểm: mở bản đồ có nhãn trong dưới 5 giây trên máy văn phòng thông thường; bấm chọn phản hồi dưới 1 giây.
- **SC-004**: Xuất GPKG và DXF lớp gộp thành công trong dưới 60 giây.
- **SC-005**: Hai máy QGIS đang kết nối tiếp tục đồng bộ với lớp gộp, 0 thay đổi bị mất.
- **SC-006**: Kiểm tự động không thấy tên, CCCD, số điện thoại trong các lớp nhập.

## Assumptions

- Giới hạn mới 50.000 đối tượng/lớp đủ cho Bình Hưng (lớp lớn nhất 33.499); lớp lớn hơn để sau.
- Bản dump 17/11 là bản mới nhất; thay đổi trên app cũ sau ngày đó (nếu có) không còn lấy được.
- Khôi phục bản dump và chuyển đổi chạy trên VPS trong môi trường tạm tách biệt, xóa sau khi xong.
- Ranh xã, ranh ấp, tuyến đã nhập trước đó giữ nguyên.
