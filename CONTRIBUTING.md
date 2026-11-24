# Đóng góp cho OpenVietGIS / Contributing

Cảm ơn bạn đã giúp OpenVietGIS trở thành một không gian làm việc GIS/CAD/3D đáng tin cậy. Dự án hiện là **public beta candidate**, **không phải bản đã nghiệm thu production**.

> **Lưu ý giấy phép (cập nhật 2026-11-24):** Dự án công bố **AGPL-3.0-or-later** tại [LICENSE](LICENSE). Việc sử dụng, sửa đổi và phân phối tuân theo giấy phép áp dụng; người gửi đóng góp phải có quyền đối với mã/tài sản cung cấp. Dữ liệu, plugin độc lập, bản đồ nền và thành phần bên thứ ba có thể có giấy phép riêng. Có file LICENSE **không đồng nghĩa** mọi quyền tác giả đã được rà soát hoặc sản phẩm được nghiệm thu production.

## Bắt đầu

1. Đọc [README](README.md), [định hướng sản phẩm](docs/product.md), [kiến trúc](docs/architecture.md) và [quy trình QA cộng đồng](docs/community/README.md).
2. Trước khi code, mở Issue: mô tả vấn đề, phiên bản/commit, kịch bản tái hiện và tiêu chí nghiệm thu.
3. Clone dự án. Yêu cầu: Node.js 26, pnpm 12.21.0, Python 3.13 + uv, Docker/Compose, PostgreSQL 18/PostGIS 3.7 theo tài liệu dự án.
4. Chạy trên môi trường **local/synthetic độc lập**. Không dùng bản production `openvietgis.cloud`, không đụng database/dữ liệu/định danh thật để thử các chức năng ghi.
5. Tạo nhánh từ commit `main` đã xác minh: `fix/<issue>-<slug>`, `feat/<issue>-<slug>` hoặc `docs/<slug>`. Chỉ sửa phạm vi Issue; không force-push `main`.
6. Chạy kiểm thử phù hợp trước khi gửi PR; giữ log đã xóa bí mật và thông tin cá nhân.

```bash
pnpm install --frozen-lockfile
uv sync --locked --project apps/api
pnpm validate:ts
pnpm validate:py
pnpm validate:arch
pnpm validate:docs
```

Các suite integration/E2E cần DB và OIDC test riêng, và một số phép thử dài. Hãy theo [README](README.md#bootstrap) để khởi tạo trước, hoặc dùng CI `.github/workflows/ci.yml`. `pnpm validate` là kiểm thử đầy đủ theo profile CI; **không phải chứng nhận phát hành**.

## Những đóng góp được ưu tiên

- Báo lỗi có thể tái hiện; trải nghiệm GIS/CAD/3D, phím tắt, mobile và accessibility.
- Kiểm thử độc lập import/export (CRS, schema, geometry), query, editing, layout, phân quyền.
- Tối ưu hiệu năng với benchmark trước/sau cùng cấu hình.
- Viết tài liệu, bản dịch, dữ liệu kiểm thử tổng hợp, test tự động.
- Nghiệp vụ plugin: mở issue riêng, không nhúng logic ngành dọc vào Core.

## Quy tắc Pull Request

- Một PR / một mục tiêu rõ ràng; nêu Issue, phạm vi, rủi ro, cách rollback và cách kiểm thử.
- Mọi chỉnh sửa GIS phải có kiểm tra CRS, revision/history, authorization/tenant isolation; migration phải có phương án phục hồi rõ.
- Thay đổi API/SDK/schema/UX yêu cầu cập nhật test, doc và ghi nhận backward compatibility.
- Không commit `.env`, token, API key, hộ khẩu/địa chỉ riêng tư, dữ liệu khách hàng, binary thật, backup, production logs.
- PR chưa qua kiểm thử độc lập phải giữ **Draft**; reviewer không đánh dấu PASS theo số checkbox hoặc build xanh.
- Không sửa trực tiếp production để chứng minh một PR.

Định dạng kết quả: [Ma trận nghiệm thu](docs/community/ACCEPTANCE_MATRIX.md) và [Runbook](docs/community/TEST_RUNBOOK.md). Xem thêm [chính sách release](docs/community/RELEASE_POLICY.md) và [báo cáo bảo mật](SECURITY.md).

### English quick note
OpenVietGIS is a public, not-yet-release-certified candidate. The project declares **AGPL-3.0-or-later** in [LICENSE](LICENSE); verify copyright notices and third-party licenses before redistribution. Please use synthetic local/test data, file reproducible issues, and submit scoped draft PRs with pinned-commit evidence. No production-data testing or unreviewed deployment.
