# OpenVietGIS Read-Only Connector for QGIS Desktop (3.28+)

**Phiên bản / Version:** `0.1.0`  
**Đặc tả / Specification:** `R026 — Integration Hub & Policy Gate`  
**Chế độ truy cập / Access Mode:** Chỉ đọc (`default_write_policy: "deny"`, `read_only_exchange`)

---

## 1. Giới hạn bảo mật & Kiến trúc / Security Boundaries & Architecture

| Nguyên tắc / Principle | Cam kết thực thi / Enforcement |
| :--- | :--- |
| **Chỉ đọc tuyệt đối / Strictly Read-Only** | Plugin không chứa bất kỳ chức năng ghi, đẩy dữ liệu, hay đồng bộ ngược (`write` / `push` / `sync`) nào. Mọi lớp GeoPackage mở vào QGIS đều được gắn cờ `layer.setReadOnly(True)`. |
| **Không truy cập CSDL nội bộ / No Direct Internal DB Access** | OpenVietGIS **không bao giờ** cấp host, cổng, hay tài khoản PostgreSQL/PostGIS nội bộ. Mọi trao đổi đều đi qua HTTPS API có xác thực, RLS, và kiểm soát ACL theo từng lớp/đối tượng. |
| **Bảo toàn hệ tọa độ VN-2000 / Explicit CRS Preserved** | Chỉ xuất các lớp đã xác minh CRS tường minh theo danh mục R003 (ví dụ: `EPSG:4756`, `EPSG:5899`, `EPSG:4326`, `EPSG:3857`). Không bao giờ tự đoán CRS từ tọa độ; các lớp thiếu CRS hoặc chỉ ở mức khai báo (`EPSG:3405`, `EPSG:3406`) sẽ bị chặn xuất (`export_permitted: false`). |
| **Kiểm tra toàn vẹn SHA-256 / Fail-Closed SHA-256** | Mọi tệp `.gpkg` tải về đều bắt buộc có header `X-Content-SHA256` hợp lệ và khớp chính xác trước khi ghi ra đĩa và nạp vào QGIS. |
| **An toàn khóa API / Bearer Key Hygiene** | Khóa Service Principal chỉ nhập qua ô ẩn mật khẩu, giữ trong bộ nhớ tạm khi mở hộp thoại, tự xóa khi đóng và **không bao giờ** lưu dạng văn bản thuần vào tệp dự án QGIS (`.qgs`/`.qgz`). |
| **Tách biệt phạm vi R010 & R025 / Scope Boundaries** | Kết nối WMS/WMTS/WFS/COG từ xa thuộc R010 (`external-sources`); cầu nối MicroStation V8i thuộc nhánh R025 Windows riêng biệt. Plugin này chỉ giao tiếp với R026 `qgis` adapter. |

---

## 2. Tạo Service Principal API Key chỉ đọc (`project.read`) / Create a Read-Only Service Principal API Key

### Tiếng Việt
1. Đăng nhập vào **OpenVietGIS Developer Portal** bằng tài khoản quản trị dự án hoặc tổ chức.
2. Mở mục **Service Principals / API Keys** trong tổ chức mục tiêu.
3. Tạo mới một Service Principal (ví dụ: `qgis-desktop-readonly`) và gán phạm vi tài nguyên đúng **Project** cần kết nối.
4. Thiết lập trần quyền (**Permission Ceiling**) tối thiểu là **`project.read`** (không cấp `project.edit` hay `project.manage`).
5. Tạo **Bearer API Key**, sao chép một lần duy nhất và lưu trong trình quản lý mật khẩu an toàn. Ghi lại **Organization UUID** và **Project UUID**.
6. Có thể thu hồi (revoke) khóa ngay lập tức từ Developer Portal nếu thiết bị bị mất hoặc thay đổi nhân sự.

### English
1. Sign in to the **OpenVietGIS Developer Portal** with an authorized organization or project admin account.
2. Open **Service Principals / API Keys** under your target organization.
3. Create a dedicated Service Principal (for example `qgis-desktop-readonly`) scoped strictly to the target **Project**.
4. Set the **Permission Ceiling** to **`project.read`** only (least privilege; do not grant `project.edit` or `project.manage`).
5. Generate the **Bearer API Key**, copy it once into a secure password manager, and note your **Organization UUID** and **Project UUID**.
6. Revoke or rotate the key immediately in the Developer Portal whenever access is no longer needed.

---

## 3. Cài đặt Plugin vào QGIS 3.28+ / Install into QGIS 3.28+

### Cách 1: Cài từ tệp ZIP (Khuyến nghị) / Option A: Install from ZIP (Recommended)

1. Nén thư mục `openvietgis_connector/` thành tệp `openvietgis_connector.zip` sao cho bên trong tệp ZIP có thư mục gốc `openvietgis_connector/metadata.txt`.
   - **Windows (PowerShell):**
     ```powershell
     Compress-Archive -Path integrations/qgis_plugin/openvietgis_connector -DestinationPath openvietgis_connector.zip -Force
     ```
   - **Linux / macOS:**
     ```bash
     (cd integrations/qgis_plugin && zip -r ../../openvietgis_connector.zip openvietgis_connector)
     ```
2. Mở **QGIS Desktop (3.28+)** -> chọn **`Plugins`** -> **`Manage and Install Plugins...`** -> tab **`Install from ZIP`**.
3. Chọn tệp `openvietgis_connector.zip` và bấm **`Install Plugin`**.
4. Chuyển sang tab **`Installed`** và bật **`OpenVietGIS Read-Only Connector`**.

### Cách 2: Sao chép thủ công vào thư mục Plugins / Option B: Copy Directly to QGIS Plugins Folder

Sao chép toàn bộ thư mục `integrations/qgis_plugin/openvietgis_connector` vào thư mục plugins của QGIS profile hiện tại:

- **Windows:** `%APPDATA%\QGIS\QGIS3\profiles\default\python\plugins\openvietgis_connector`
- **Linux:** `~/.local/share/QGIS/QGIS3/profiles/default/python/plugins/openvietgis_connector`
- **macOS:** `~/Library/Application Support/QGIS/QGIS3/profiles/default/python/plugins/openvietgis_connector`

Khởi động lại QGIS và bật plugin trong **`Plugins -> Manage and Install Plugins -> Installed`**.

---

## 4. Sử dụng trong QGIS / Using the Plugin in QGIS

1. Mở plugin từ menu **`Web -> OpenVietGIS -> OpenVietGIS Read-Only Connector`** hoặc biểu tượng trên thanh công cụ.
2. Nhập:
   - **Server URL (HTTPS):** ví dụ `https://gis.example.vn` *(chỉ bật tùy chọn `http://localhost` / `http://127.0.0.1` khi phát triển nội bộ)*.
   - **Organization UUID** và **Project UUID**.
   - **Service Principal Bearer Key** (`project.read`).
   - **Local GeoPackage Folder:** thư mục lưu bản chụp `.gpkg` trên máy trạm.
3. Bấm **`Tải danh sách lớp / Load Authorized Layers`** để xem các lớp được phép truy cập cùng dạng hình học, mã EPSG tường minh (ví dụ `EPSG:4756`, `EPSG:5899`, `EPSG:4326`), và trạng thái cho phép xuất.
4. Chọn một lớp có trạng thái **`Export Ready (Read-Only)`** và bấm **`Tải & mở lớp vào QGIS (Chỉ đọc)`**.
   - Plugin tải tệp `.gpkg`, xác minh `X-Content-SHA256`, ghi nguyên tử vào thư mục cục bộ, và nạp lớp qua driver `ogr` với `layer.setReadOnly(True)`.

---

## 5. Nhập tệp `.sql` xuất từ OpenVietGIS vào PostGIS cục bộ hoặc bên ngoài / Import Exported `.sql` into Local or External PostGIS

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
