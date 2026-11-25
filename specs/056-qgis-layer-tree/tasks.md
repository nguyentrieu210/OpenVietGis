# Tasks: Spec 056

## Đợt 1 — cây lớp
- [x] T001 Backend: `LayerReference.group` (domain, HTTP, repository, contract, SDK type) + unit test
- [x] T002 `layer-tree.ts`: dựng cây theo đường dẫn nhóm, đổi tên, bỏ nhóm + node test
- [x] T003 `CoreMapTools`: tải/lưu nhóm cùng cấu hình bản đồ
- [x] T004 `QgisLayerDock`: nhóm lồng nhau, kéo vào/ra nhóm, chọn nhiều, menu lớp/nhóm, mở/thu tất cả
- [ ] T005 Nghiệm thu SC-001/SC-002 trên live

## Đợt 2
- [x] T006 Ký hiệu theo khoảng giá trị (phân vị / khoảng đều, dải màu, chú giải trong cây) + node test
- [x] T007 Phạm vi hiển thị ghi kèm tỷ lệ 1:N
- (đã có sẵn: một màu, theo giá trị, độ mờ, nét, cỡ điểm, phạm vi zoom)

## Đợt 3
- [ ] Nhãn, chú giải lồng trong cây, lưu kiểu QML/SLD

## Đợt 3 (một phần)
- [x] T008 Kiểu ký hiệu, nhãn, bộ lọc, phạm vi tỷ lệ lưu chung theo dự án (trường `display` trong cấu hình bản đồ, ≤ 16 000 byte) + unit test
- [x] T009 Xuất SLD 1.0 / QML (một màu, phân loại, theo khoảng) + node test; nhập SLD/QML giữ đúng mốc chia
- [ ] T010 Test tự động phần nhập SLD/QML (cần DOM trong trình duyệt/Playwright)
