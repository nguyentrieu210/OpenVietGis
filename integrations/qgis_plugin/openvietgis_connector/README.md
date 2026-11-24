# OpenVietGIS Connector for QGIS Desktop (3.34+, Python 3.10+)

**Phiên bản 0.9.2 (R026/R027/R029):** hỗ trợ tải GeoPackage, đồng bộ hai chiều kiểm soát qua R006 Changeset, và **R029 Managed Realtime** không cần tệp GeoPackage. Đây vẫn là plugin thử nghiệm; chỉ dùng với dự án/tài khoản được cấp quyền. Với R027, các lần ghi không chắc kết quả được giữ lại để xác minh, tuyệt đối không tự gửi lại mù quáng. Xem [R027-MANUAL-SYNC.md](R027-MANUAL-SYNC.md) cho cơ chế khôi phục.

**Sửa lỗi 0.9.2:** backend sử dụng phạm vi đọc hợp lệ khi tạo/thu hồi phiên R029 và xử lý lệnh desktop; thao tác thay đổi GIS vẫn phải có quyền `project.edit`. Khi một lượt ghi R027 bị từ chối rõ ràng bằng HTTP 403, plugin dừng tự đồng bộ, giữ bản chỉnh sửa và cho phép khôi phục sau khi xác minh bản trên Web không bị thay đổi.

**Nếu đang gặp HTTP 403:** mở đúng Organization/Project trên Web → mục kết nối QGIS → tạo mã ghép nối mới và kiểm tra quyền `project.read`, `project.edit` (nếu cần ghi), cùng GIS ACL. **Không xóa các thư mục `ovg_qgis_...` hoặc tệp `working.gpkg` để xử lý lỗi.** Dùng `Công cụ nâng cao → Khôi phục giao dịch` nếu có pending. Các trạng thái pending từ phiên bản cũ chưa xác định kết quả phải được xác minh trước khi tiếp tục; bản mới không tự bỏ bảo vệ đó.

**Đặc tả:** R026 + R027 + R029. **Chính sách mặc định:** quyền ghi bị từ chối trừ khi được ủy quyền hợp lệ; các phép xóa vẫn bị chặn theo mặc định.

---

## 1. Giới hạn bảo mật & Kiến trúc / Security Boundaries & Architecture

| Nguyên tắc / Principle | Cam kết thực thi / Enforcement |
| :--- | :--- |
| **Chỉ đọc mặc định / Read-Only by Default** | Chính sách gốc luôn là `default_write_policy: "deny"`. Mọi lớp GeoPackage tải về qua nút mở lớp đều được gắn cờ `layer.setReadOnly(True)` trong QGIS. |
| **Đồng bộ ngược hai bước có kiểm soát / Controlled Two-Step Sync (R006)** | Mọi thay đổi từ GeoPackage cục bộ (tối đa `16 MiB`) phải đi qua **2 bước bắt buộc**: (1) `POST .../sync-preview` để tính diff không ghi dữ liệu và khóa mã băm `diff_sha256`; (2) xác nhận tường minh `POST .../sync-push` với quyền `project.edit`, đúng `base_content_revision`/`base_schema_revision`, đúng `expected_diff_sha256`, và `operation_id` UUID mới (không dùng tiền tố `field-sync-`). Mọi thay đổi tạo **R006 Changeset** có nhật ký kiểm toán. |
| **Chặn xóa mặc định / Deletions Blocked by Default** | `deletions_blocked: true` luôn được thực thi mặc định. Các đối tượng có trên máy chủ nhưng vắng mặt trong tệp `.gpkg` cục bộ chỉ được thống kê ở `omitted_server_feature_count` và **không bao giờ** bị tự động xóa trên máy chủ. |
| **Không truy cập CSDL nội bộ / No Direct Internal DB Access** | OpenVietGIS **không bao giờ** cấp host, cổng, hay tài khoản PostgreSQL/PostGIS nội bộ. Mọi trao đổi đều đi qua HTTPS API có xác thực, RLS, và kiểm soát ACL theo từng lớp/đối tượng. |
| **Bảo toàn hệ tọa độ VN-2000 / Explicit CRS Preserved** | Chỉ xuất và đồng bộ các lớp có CRS tường minh theo danh mục R003 (ví dụ: `EPSG:4756`, `EPSG:5899`, `EPSG:4326`, `EPSG:3857`). Không bao giờ tự đoán CRS từ tọa độ; các lớp thiếu CRS hoặc chỉ ở mức khai báo (`EPSG:3405`, `EPSG:3406`) bị chặn xuất và chặn đồng bộ. |
| **Kiểm tra toàn vẹn SHA-256 / Fail-Closed SHA-256** | Mọi tệp `.gpkg` tải về bắt buộc có header `X-Content-SHA256` hợp lệ và khớp chính xác trước khi ghi nguyên tử ra đĩa và nạp vào QGIS. |
| **An toàn khóa API / Bearer Key Hygiene** | Khóa Service Principal nhập qua ô ẩn mật khẩu, chỉ giữ trong bộ nhớ tạm khi mở hộp thoại, tự xóa khi đóng và **không bao giờ** lưu dạng văn bản thuần vào tệp dự án QGIS (`.qgs`/`.qgz`). |
| **Tách biệt phạm vi R010 & R025 / Scope Boundaries** | Kết nối WMS/WMTS/WFS/COG từ xa thuộc R010 (`external-sources`); cầu nối MicroStation V8i thuộc nhánh R025 Windows riêng biệt. Không liên kết thư viện nhị phân độc quyền của hãng thứ ba. |

---

## 2. Tạo Service Principal API Key / Create a Service Principal API Key

### Tiếng Việt
1. Đăng nhập vào **OpenVietGIS Developer Portal** bằng tài khoản quản trị dự án hoặc tổ chức.
2. Mở mục **Service Principals / API Keys** trong tổ chức mục tiêu.
3. Tạo mới một Service Principal gán phạm vi tài nguyên đúng **Project** cần kết nối:
   - **Chỉ tải & xem (khuyến nghị tối thiểu quyền):** đặt trần quyền (**Permission Ceiling**) là **`project.read`**.
   - **Đồng bộ ngược GeoPackage hai bước:** chỉ cấp trần quyền **`project.edit`** cho nhân sự/máy trạm được ủy quyền chỉnh sửa lớp.
4. Tạo **Bearer API Key**, sao chép một lần duy nhất vào trình quản lý mật khẩu an toàn. Ghi lại **Organization UUID** và **Project UUID**.
5. Có thể thu hồi (revoke) hoặc xoay vòng khóa ngay lập tức từ Developer Portal khi cần.

### English
1. Sign in to the **OpenVietGIS Developer Portal** with an authorized organization or project admin account.
2. Open **Service Principals / API Keys** under your target organization.
3. Create a dedicated Service Principal scoped strictly to the target **Project**:
   - **Read-only inspection & export (recommended least privilege):** set the **Permission Ceiling** to **`project.read`**.
   - **Controlled two-step GeoPackage sync:** grant **`project.edit`** only to trusted editors authorized to commit R006 changesets on the project.
4. Generate the **Bearer API Key**, store it once in a secure password manager, and record your **Organization UUID** and **Project UUID**.
5. Revoke or rotate the key immediately in the Developer Portal whenever access is no longer needed.

---

## 3. Cài đặt Plugin vào QGIS 3.34+ / Python 3.10+ / Install into QGIS 3.34+ / Python 3.10+

### Cách 1: Cài từ tệp ZIP (Khuyến nghị) / Option A: Install from ZIP (Recommended)

1. Nén thư mục `openvietgis_connector/` thành tệp `openvietgis_connector.zip` sao cho bên trong tệp ZIP có thư mục gốc `openvietgis_connector/metadata.txt`:
   - **Windows (PowerShell):**
     ```powershell
     Compress-Archive -Path integrations/qgis_plugin/openvietgis_connector -DestinationPath openvietgis_connector.zip -Force
     ```
   - **Linux / macOS:**
     ```bash
     (cd integrations/qgis_plugin && zip -r ../../openvietgis_connector.zip openvietgis_connector)
     ```
2. Mở **QGIS Desktop (3.34+, Python 3.10+)** -> chọn **`Plugins`** -> **`Manage and Install Plugins...`** -> tab **`Install from ZIP`**.
3. Chọn tệp `openvietgis_connector.zip` và bấm **`Install Plugin`**.
4. Chuyển sang tab **`Installed`** và bật **`OpenVietGIS Connector`**.

### Cách 2: Sao chép thủ công vào thư mục Plugins / Option B: Copy Directly to QGIS Plugins Folder

Sao chép toàn bộ thư mục `integrations/qgis_plugin/openvietgis_connector` vào thư mục plugins của QGIS profile hiện tại:

- **Windows:** `%APPDATA%\QGIS\QGIS3\profiles\default\python\plugins\openvietgis_connector`
- **Linux:** `~/.local/share/QGIS/QGIS3/profiles/default/python/plugins/openvietgis_connector`
- **macOS:** `~/Library/Application Support/QGIS/QGIS3/profiles/default/python/plugins/openvietgis_connector`

Khởi động lại QGIS và bật plugin trong **`Plugins -> Manage and Install Plugins -> Installed`**.

---

## 4. Sử dụng trong QGIS / Using the Plugin in QGIS

### 4.1. Tải & mở lớp chỉ đọc / Download & Open Read-Only Layer

1. Mở plugin từ menu **`Web -> OpenVietGIS -> OpenVietGIS Connector (R026)`** hoặc biểu tượng trên thanh công cụ.
2. Nhập **Server URL (HTTPS)** *(chỉ bật `http://localhost` / `http://127.0.0.1` khi phát triển nội bộ)*, **Organization UUID**, **Project UUID**, **Service Principal Bearer Key**, và **Local GeoPackage Folder**.
3. Bấm **`Tải danh sách lớp / Load Authorized Layers`** để xem các lớp được phép truy cập cùng dạng hình học, mã EPSG tường minh (ví dụ `EPSG:4756`, `EPSG:5899`, `EPSG:4326`), và trạng thái cho phép xuất.
4. Chọn một lớp có trạng thái **`Export Ready (Read-Only)`** và bấm **`Tải & mở lớp vào QGIS (Chỉ đọc)`**.
   - Plugin tải tệp `.gpkg`, xác minh `X-Content-SHA256`, ghi nguyên tử vào thư mục cục bộ, ghim cặp `content_revision`/`schema_revision`, và nạp lớp qua driver `ogr` với `layer.setReadOnly(True)`.

### 4.2. Đồng bộ GeoPackage hai bước (Preview -> Confirm Changeset) / Controlled Two-Step GeoPackage Sync

> **Yêu cầu quyền / Required Permission:** `project.edit` (cộng với quyền chỉnh sửa theo lớp trong GIS ACL).  
> **Giới hạn tệp / Upload Limit:** Tối đa `16 MiB` cho mỗi tệp `.gpkg`.  
> **Chính sách xóa / Deletion Policy:** **Chặn xóa mặc định (`deletions_blocked: true`)** — mọi đối tượng thiếu trong tệp `.gpkg` cục bộ chỉ được báo cáo ở `omitted_server_feature_count` và KHÔNG bị xóa trên máy chủ.

#### Tiếng Việt
1. Tạo bản sao cục bộ của tệp `.gpkg` đã tải về, chỉnh sửa hình học/thuộc tính trong QGIS (giữ nguyên mã EPSG gốc của lớp và không thay đổi tên bảng/cột schema).
2. Trong hộp thoại plugin, chọn **lớp đích** trong bảng danh sách (plugin tự dùng cặp `content_revision` và `schema_revision` đã ghim) và chọn **tệp `.gpkg` đã chỉnh sửa** tại mục **Edited GeoPackage (.gpkg)**.
3. **Bước 1 — Bấm `1. Xem trước thay đổi (Diff)`** (`POST .../adapters/qgis/layers/{layer_id}/sync-preview`):
   - Máy chủ tính toán diff nhưng **không** ghi dữ liệu.
   - Hộp thoại hiển thị: `create_count`, `update_count`, `unchanged_count`, `omitted_server_feature_count` *(có ghi chú rõ xóa bị chặn mặc định)*, `stale_revision`, và mã băm rút gọn `diff_sha256`.
   - Nếu `stale_revision=True` (dữ liệu máy chủ đã có phiên bản mới hơn) hoặc không có thay đổi hợp lệ (`can_commit=False`), nút bước 2 vẫn bị **khóa (disabled)** — hãy tải lại danh sách lớp và xuất bản chụp mới.
4. **Bước 2 — Bấm `2. Xác nhận đẩy Changeset`** (`POST .../adapters/qgis/layers/{layer_id}/sync-push`):
   - Nút chỉ được mở khi `preview.can_commit == True` và bị khóa ngay nếu bạn đổi lớp đích hoặc đổi tệp `.gpkg`.
   - Xác nhận hộp thoại câu hỏi (`Yes/No`) để gửi yêu cầu kèm mã `operation_id` UUID mới và đúng `expected_diff_sha256` đã xem trước.
   - Khi thành công (`201 Created`), máy chủ trả về `changeset_id`, `resulting_content_revision`, `operation_count`, và `audit_event_id` theo chuẩn lịch sử thay đổi R006.

#### English
1. Work on an editable local copy of the exported `.gpkg` in QGIS while preserving the layer's explicit EPSG CRS and schema.
2. Select the **target layer** in the plugin table (pinned to its `content_revision` and `schema_revision`) and browse to the edited `.gpkg` under **Edited GeoPackage (.gpkg)**.
3. **Step 1 — Click `1. Xem trước thay đổi (Diff)`** (`POST .../adapters/qgis/layers/{layer_id}/sync-preview`):
   - Runs a non-mutating server-side diff and displays `create_count`, `update_count`, `unchanged_count`, `omitted_server_feature_count` *(deletions are blocked by default)*, `stale_revision`, and short `diff_sha256`.
   - If `stale_revision` is `True` or `can_commit` is `False`, Step 2 stays **disabled**. Reload layers and re-export if the server revision moved ahead.
4. **Step 2 — Click `2. Xác nhận đẩy Changeset`** (`POST .../adapters/qgis/layers/{layer_id}/sync-push`):
   - Enabled **only** when `preview.can_commit` is `True` (and automatically disabled if the target layer or `.gpkg` path changes).
   - Prompts an explicit confirmation dialog before committing with a fresh UUID `operation_id` and the exact previewed `diff_sha256`.
   - Returns an audit-backed R006 changeset receipt (`changeset_id`, `resulting_content_revision`, `operation_count`, `audit_event_id`).

---

## 5. Xuất AutoCAD DXF chỉ đọc (`format=dxf`) / AutoCAD DXF Read-Only Export

### Tiếng Việt
R026 Integration Hub hỗ trợ xuất bản chụp **chỉ đọc** sang định dạng **AutoCAD DXF (`application/dxf`)** qua endpoint:

```text
GET /api/v1/organizations/{organization_id}/projects/{project_id}/integration-hub/adapters/{adapter_key}/layers/{layer_id}/export?format=dxf
```

- **Quyền yêu cầu:** `project.read` (cùng RLS và GIS ACL theo từng lớp/đối tượng).
- **Hệ tọa độ chiếu hệ mét tường minh:** Chỉ hỗ trợ lớp sử dụng CRS chiếu đơn vị mét đã xác minh (`EPSG:5899`, `EPSG:3857`); hệ tọa độ địa lý theo độ (`EPSG:4326`, `EPSG:4756`) bị từ chối rõ ràng để tránh sai lệch đơn vị CAD. Chỉ xuất hình học 2D đơn phần (`Point`, `LineString`, `Polygon` không lỗ, từ `1` đến `10.000` đối tượng). Header phản hồi gồm `X-Content-SHA256`, `X-OpenVietGIS-CRS`, `X-OpenVietGIS-Content-Revision`, `X-OpenVietGIS-Schema-Revision`, và `X-OpenVietGIS-Feature-Count`.
- **Cảnh báo mất thuộc tính bảng (`X-OpenVietGIS-Attribute-Loss: true`):** DXF là định dạng trao đổi hình học CAD nên không lưu thuộc tính GIS dạng bảng trong thực thể modelspace; máy chủ luôn gửi header `X-OpenVietGIS-Attribute-Loss: true`. Hãy dùng **GeoPackage (`.gpkg`)** hoặc **PostGIS SQL (`.sql`)** nếu cần bảo toàn toàn bộ bảng thuộc tính.
- **Giới hạn định dạng CAD:** Chỉ hỗ trợ xuất DXF chỉ đọc (adapter `autocad` chỉ nhận `format=dxf`); định dạng nhị phân `.dwg` gốc không được hỗ trợ và không liên kết thư viện nhị phân độc quyền.

### English
The R026 Integration Hub supports **read-only** export to **AutoCAD DXF (`application/dxf`)** via:

```text
GET /api/v1/organizations/{organization_id}/projects/{project_id}/integration-hub/adapters/{adapter_key}/layers/{layer_id}/export?format=dxf
```

- **Required permission:** `project.read` (enforcing PostgreSQL RLS, service principal ceiling, and per-layer/per-feature GIS ACL).
- **Explicit projected metre CRS required:** Exports native coordinates in verified projected metre CRS only (`EPSG:5899` or `EPSG:3857`; geographic degree CRS such as `EPSG:4326`/`EPSG:4756` is rejected). Supports 2D single-part `Point`, `LineString`, and `Polygon` layers (1–10,000 features) with `X-Content-SHA256`, `X-OpenVietGIS-CRS`, `X-OpenVietGIS-Content-Revision`, `X-OpenVietGIS-Schema-Revision`, and `X-OpenVietGIS-Feature-Count`.
- **Attribute loss notice (`X-OpenVietGIS-Attribute-Loss: true`):** Because DXF modelspace entities do not store tabular GIS attributes, the API emits `X-OpenVietGIS-Attribute-Loss: true`. Use **GeoPackage (`.gpkg`)** or **PostGIS SQL (`.sql`)** when full tabular schema fidelity is required.
- **Scope note:** DXF export is read-only (`autocad` adapter accepts `format=dxf` only); native proprietary `.dwg` binaries are not supported and zero vendor binaries are linked.

---

## 6. Nhập tệp `.sql` xuất từ OpenVietGIS vào PostGIS cục bộ hoặc bên ngoài / Import Exported `.sql` into Local or External PostGIS

Khi sử dụng định dạng xuất `format=postgis_sql` của R026 Integration Hub (`GET .../adapters/postgis/layers/{layer_id}/export?format=postgis_sql`), tệp `.sql` được thiết kế để nạp vào **cơ sở dữ liệu PostGIS riêng của bạn (cục bộ hoặc máy chủ ngoài)** — không bao giờ kết nối trực tiếp vào CSDL nội bộ của OpenVietGIS.

### Bước 1: Kiểm tra mã băm SHA-256 / Verify SHA-256 Digest First

So sánh mã băm của tệp `.sql` đã tải với header `X-Content-SHA256`:

```bash
# Linux / macOS
sha256sum <file>.sql
```

```powershell
# Windows PowerShell
Get-FileHash -Algorithm SHA256 <file>.sql
```

### Bước 2: Đảm bảo PostGIS extension đã bật và nạp bằng `psql` / Ensure PostGIS Extension & Import with `psql`

```bash
psql -v ON_ERROR_STOP=1 -f <file>.sql
```

Với đầy đủ tham số kết nối tới cơ sở dữ liệu PostGIS của bạn:

```bash
psql -v ON_ERROR_STOP=1 -h <postgis_host> -p 5432 -U <postgis_user> -d <target_database> -f <file>.sql
```

> **Lưu ý / Note:** Cờ `-v ON_ERROR_STOP=1` giúp `psql` dừng ngay lập tức nếu gặp lỗi SQL (ví dụ thiếu extension `postgis` hoặc trùng bảng), đảm bảo dữ liệu không bị nạp dở dang. Hệ tọa độ EPSG gốc (bao gồm VN-2000 `EPSG:4756`, `EPSG:5899`) được giữ nguyên theo SRID khai báo trong tệp `.sql`.