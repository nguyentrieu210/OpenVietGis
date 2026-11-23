# Hướng dẫn chạy nghiệm thu cộng đồng

## 1. Chuẩn bị an toàn

- Chỉ dùng dữ liệu **giả / synthetic** của người thử nghiệm, không chia sẻ dataset có địa chỉ hoặc thông tin nhận dạng người dân.
- Dùng repo/branch có SHA cố định. Phiên bản `main` có thể thay đổi; không lấy ảnh chụp từ release khác để chứng minh.
- Không dùng database production, không chạy lệnh reset trên máy khách hàng. Truy vấn đọc production nếu được phép vẫn không thay thế test môi trường cô lập.
- Đọc [CONTRIBUTING](../../CONTRIBUTING.md); Node 26, pnpm 12.21.0, Python 3.13, uv, Docker và PostgreSQL 18/PostGIS 3.7 theo cấu hình repo.

## 2. Bootstrap local/synthetic

```bash
git clone https://github.com/nguyentrieu210/OpenVietGis.git
cd OpenVietGis
git rev-parse HEAD
pnpm install --frozen-lockfile
uv sync --locked --project apps/api
cp .env.example .env  # development-only; tuyệt đối không chép credentials thật
docker compose -f infra/dev/compose.yaml -f infra/dev/compose.identity.yaml up -d postgres postgres-test keycloak
uv run --project apps/api alembic -c apps/api/alembic.ini upgrade head
```

Sau đó chạy `pnpm dev:api` và `pnpm dev:web` ở hai terminal. Xem README cho các cấu hình/OIDC/port bắt buộc. Nếu test fail tại bootstrap, ghi `BLOCKED` cùng lỗi/log, **không** sửa các guard để ép chạy.

## 3. Kiểm thử tự động theo profile

```bash
pnpm validate:ts      # web/SDK lint, typecheck, unit, build
pnpm validate:py      # Python, modules, contracts
pnpm validate:arch    # kiểm tra boundary
pnpm validate:docs    # kiểm tra tài liệu/links thuộc bộ check của repo
pnpm validate          # tổng hợp, bao gồm PostGIS/browser/secret/tenancy checks
```

CI mặc định là `.github/workflows/ci.yml`; kết quả CI phải đọc trên **SHA của PR/candidate**, không lấy xanh ở commit cũ để cấp chứng nhận mới. Không đăng log có credentials, token hay dữ liệu cá nhân.

## 4. Kiểm thử thủ công

Chọn một nhóm testcase tại [ACCEPTANCE_MATRIX](ACCEPTANCE_MATRIX.md). Với mỗi testcase:

1. Đảm bảo fixture được mô tả, dự án/role hợp lệ, ghi chính xác SHA và tên artifact.
2. Ghi **Expected** trước; thực hiện ít nhất một luồng thành công và một luồng bị từ chối liên quan tới quyền, CRS, revision hoặc dữ liệu sai.
3. So sánh đầu ra qua oracle độc lập: GDAL/QGIS, PostGIS truy vấn chỉ đọc hoặc thư viện kiểm tra PDF/ảnh/DXF.
4. Chụp màn hình/clip có che dữ liệu riêng tư. Nếu có thể, cung cấp lệnh hoặc script tối thiểu tái hiện.
5. Ghi `PASS`, `FAIL`, `BLOCKED`, `NOT_RUN` hoặc `N/A`. Đừng lẫn `BLOCKED` và `PASS`.

Ví dụ: CQ-03 kiểm tra GeoJSON/GPKG/ZIP Shapefile qua import → xem layer → export → GDAL mở lại, kiểm count, schema, SRID và sai số tọa độ đã chốt. CQ-10 kiểm tra DXF riêng; không báo `PASS` cho DWG native nếu chưa có công cụ đó.

## 5. Phiếu bằng chứng bắt buộc

```text
Case ID:
Result: PASS | FAIL | BLOCKED | NOT_RUN | N/A
Date/time (timezone):
Repository + commit SHA:
Frontend build / backend build / schema migration:
OS / CPU / RAM / browser:
Test profile / URL (local synthetic only):
Actor role / project scope:
Fixture name, origin, CRS, feature count, SHA256 (nếu có):
Steps to reproduce:
Expected:
Actual:
Oracle / tolerance / independent validation:
Logs/screenshots (sanitized):
Regression / security / data-loss risk:
Tester:
Independent reviewer:
Known limitations / remaining work:
```

Không điền danh tính người dân hoặc thông tin định vị riêng tư vào Issue. Những trường không biết phải ghi `unknown`, không bịa.

## 6. Triage và vòng đời Issue

- `P0` mất dữ liệu, rò quyền tenant, RCE, xuất tọa độ sai có thể gây hậu quả nghiêm trọng: dừng phát hành, báo riêng nếu nhạy cảm.
- `P1` luồng chính không dùng được, edit/export sai, không rollback: blocker cho scope beta tương ứng.
- `P2` lỗi có workaround rõ, UX chưa tốt: ưu tiên theo ảnh hưởng.
- `P3` đề xuất/thẩm mỹ/tài liệu: không tự động chặn release.
- Người sửa cung cấp PR và test; người kiểm thử độc lập chạy lại trên **commit tích hợp cuối**, ghi kết quả mới; người giữ release quyết định đóng issue.

## 7. Chốt phiên nghiệm thu

Lập danh sách case **đã chạy**, **chưa chạy**, **lỗi chưa đóng**, SHA+profile, môi trường, link PR/Issue và kết luận có phạm vi. `PASS` cộng đồng **không** thay chữ ký release chính thức. Theo [RELEASE_POLICY](RELEASE_POLICY.md).
