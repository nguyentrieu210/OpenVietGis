# Feature Specification: Khung lớp dữ liệu đúng kiểu QGIS

**Feature Branch**: `feat/056-layer-tree`
**Created**: 2026-11-25
**Status**: Đang làm — đợt 1
**Parent**: R004 (Map Workspace: layer order/visibility, lưu/mở lại cấu hình bản đồ).
**Input**: Chủ dự án: "các tính năng ở đây chưa đúng như cái qgis toàn bộ đúc lại tính năng layer". Đồng ý chia 3 đợt; nhóm và thứ tự lớp lưu chung theo dự án.

## Clarifications

### Session 2026-11-25

- Nhóm lớp và thứ tự lớp **lưu chung theo dự án** trong cấu hình bản đồ (giống tệp dự án QGIS), không lưu riêng từng người.
- Đợt 1: cây lớp có nhóm lồng nhau, kéo thả vào/ra nhóm, chọn nhiều lớp (Ctrl/Shift), menu chuột phải cho lớp và nhóm, mở/thu tất cả.
- Đợt 2: hộp Thuộc tính lớp (ký hiệu một màu / theo giá trị / theo khoảng), độ mờ, phạm vi tỷ lệ.
- Đợt 3: nhãn, chú giải lồng trong cây, lưu kiểu lớp (QML/SLD).

## Requirements (đợt 1)

- **FR-001**: Cấu hình bản đồ MUST lưu cho mỗi lớp một đường dẫn nhóm tùy chọn (`group`, các cấp nối bằng `/`, tối đa 4 cấp, 200 ký tự); lớp không thuộc nhóm thì không có trường này để các phiên bản cũ giữ nguyên.
- **FR-002**: Cây lớp MUST hiện nhóm lồng nhau; nhóm nằm ở vị trí lớp đầu tiên của nó theo thứ tự vẽ; nhóm có hộp bật/tắt ba trạng thái và tổng số đối tượng.
- **FR-003**: Kéo lớp thả lên một lớp khác MUST đổi thứ tự và đưa lớp vào cùng nhóm với lớp đích; thả lên dòng nhóm MUST đưa lớp vào nhóm đó.
- **FR-004**: Ctrl+bấm và Shift+bấm MUST chọn nhiều lớp; kéo, đưa vào nhóm, bỏ khỏi nhóm áp dụng cho cả nhóm lớp đã chọn.
- **FR-005**: Menu chuột phải của lớp MUST có thêm: Đưa vào nhóm…, Bỏ khỏi nhóm, Đổi tên lớp…; menu của nhóm: Hiện/ẩn cả nhóm, Mở nhóm, Đổi tên nhóm…, Thêm nhóm con…, Bỏ nhóm.
- **FR-006**: Thanh công cụ khung lớp MUST có: Thêm nhóm, Mở tất cả nhóm, Thu tất cả nhóm.
- **FR-007**: Nhóm do plugin cung cấp (ví dụ điểm khảo sát số nhà) vẫn hiện khi lớp chưa có nhóm của người dùng.
- **FR-008**: Thay đổi nhóm đánh dấu "Chưa lưu nhóm layer" và lưu cùng nút Lưu cấu hình layer như thứ tự và kiểu.

## Success Criteria

- **SC-001**: Tạo nhóm 2 cấp, kéo 3 lớp vào, lưu, tải lại trang: cây giữ nguyên với mọi thành viên dự án.
- **SC-002**: Bản đồ đã lưu trước tính năng này mở lại không đổi phiên bản, không lỗi.
