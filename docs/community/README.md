# OpenVietGIS Community QA & Beta Program

**Trạng thái ngày 2026-11-23:** mời cộng đồng kiểm thử và đóng góp; **chưa nghiệm thu sản phẩm hay phát hành production**. Repo public không đồng nghĩa được cấp giấy phép OSS; xem [CONTRIBUTING](../../CONTRIBUTING.md).

## Mục tiêu

Giúp cộng đồng kiểm chứng bằng hành vi **có thể tái hiện**, thay vì chỉ đánh giá giao diện hoặc độ dài danh sách tính năng. Dự án học các workflow từ QGIS, MapInfo, ArcGIS và AutoCAD, **không tuyên bố tương đương toàn bộ** các sản phẩm đó.

## Quy trình cho người tham gia

1. Chọn testcase trong [ACCEPTANCE_MATRIX](ACCEPTANCE_MATRIX.md) tương ứng kỹ năng GIS/CAD/3D/plugin/QA.
2. Đọc [TEST_RUNBOOK](TEST_RUNBOOK.md). Ghi chính xác **commit SHA**, web/API build nếu khác nhau, hệ điều hành, trình duyệt, profile dữ liệu và vai trò.
3. Dùng **fixture tổng hợp** trên local/test hoặc demo sandbox được công bố riêng. **Không thử ghi/xóa/import hàng loạt lên production**; `openvietgis.cloud` không mặc định là sandbox cộng đồng.
4. Thực hiện từng bước, lưu ảnh/video/log **đã ẩn bí mật/dữ liệu cá nhân**, kỳ vọng và kết quả thực tế.
5. Mở GitHub Issue bằng mẫu **Community Acceptance** khi có bằng chứng, mẫu **Bug report** khi phát hiện lỗi. Nếu có lỗi bảo mật, dùng [SECURITY](../../SECURITY.md) thay vì Issues.
6. Tình nguyện viên khác độc lập tái hiện; người phụ trách release quyết định scope và nghiệm thu, không tự suy từ kết quả của một người.

## Quy ước trạng thái

- `PASS`: thao tác chạy đúng *và* có bằng chứng đối chiếu, đúng SHA/profile.
- `FAIL`: quan sát sai hoặc vượt ngưỡng đã công bố; kèm bước tái hiện.
- `BLOCKED`: thiếu fixture/quyền/biểu mẫu/adapter hoặc xác nhận; **không phải PASS**.
- `NOT_RUN`: chưa thực hiện.
- `N/A`: ngoài phạm vi bản candidate, phải nêu lý do và được reviewer phê duyệt.

Dòng `PASS` của một bộ dữ liệu **không** chứng minh mọi biến thể, mọi tenant hoặc mọi môi trường production. Không chuyển trạng thái một yêu cầu sang `accepted` nếu không có review và nguồn bằng chứng trên đúng bản build.

## Nguồn kiểm chứng gốc

- [ROADMAP](../../ROADMAP.md) — phạm vi và dependency R001–R015.
- [R018 row-audit index](../../specs/018-full-product-audit/remediation/row-audit-index.md) — 656 yêu cầu gốc, **0 accepted** tại checkpoint ghi nhận.
- [V1 preflight 23/11](../../specs/018-full-product-audit/remediation/v1-preflight-20261123.md) — trạng thái gate/candidate, không được suy từ test fixture sang live.
- [House Numbering acceptance](../qa/2026-11-23-house-numbering-acceptance.md) — dữ liệu Bình Hưng và ranh giới quyền phê duyệt hành chính.

## Phân quyền đóng góp

**Community tester:** báo lỗi, kiểm thử UX, đọc dữ liệu giả.
**Developer:** PR sửa lỗi / bổ sung test trên nhánh riêng.
**Independent reviewer:** tái hiện trên cùng SHA và dữ liệu chuẩn, xác nhận bằng chứng.
**Maintainer/release owner:** chốt license, phạm vi bản beta, CI, phân quyền sandbox và quyết định phát hành.

Không yêu cầu tester quyền admin, SSH VPS, token production hoặc dữ liệu hộ gia đình.
