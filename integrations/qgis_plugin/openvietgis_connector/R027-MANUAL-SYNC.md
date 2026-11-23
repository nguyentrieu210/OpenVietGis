# Hướng dẫn cài đặt & đồng bộ 2 chiều QGIS ↔ OpenVietGIS (Plugin 0.3.0)

> **Lưu ý nhanh:** Đây là luồng đồng bộ **2 chiều thủ công qua GeoPackage** (bấm xem trước rồi bấm xác nhận), **không phải** đồng bộ tự động chạy ngầm. Mọi thay đổi đẩy lên OpenVietGIS đều đi qua HTTPS, kiểm tra phân quyền dự án và ghi bằng R006 Changeset (không mở cổng database trực tiếp).

---

## 1. Yêu cầu trước khi dùng

- **QGIS Desktop 3.34 trở lên** (có sẵn Python 3.10+ / PyQGIS).
- **Khóa API (Service Bearer Key)** tạo trong **Developer Portal** trên `https://openvietgis.cloud`:
  - Cần quyền **`project.read`** để tải lớp về QGIS.
  - Cần thêm quyền **`project.edit`** nếu muốn đẩy chỉnh sửa từ QGIS ngược lên OpenVietGIS.
- **Lớp dữ liệu hợp lệ**:
  - Hình học **2D đơn phần** (`Point`, `LineString` hoặc `Polygon`), đã có hệ tọa độ (CRS) rõ ràng.
  - Giới hạn an toàn: tệp tải lên tối đa **16 MiB**, tối đa **25.000 đối tượng**, tối đa **1.000 thao tác** mỗi lần đồng bộ.
  - Hai bên phải cùng hệ tọa độ và cấu trúc cột (không tự ý đổi CRS, không tự ý xóa đối tượng).

---

## 2. Cách cài Plugin vào QGIS Desktop (3 bước)

1. **Tải file plugin ZIP** trực tiếp tại:
   - 👉 [openvietgis_connector-0.3.0.zip](https://openvietgis.cloud/rewrite/downloads/openvietgis_connector-0.3.0.zip)
   - (Tùy chọn kiểm tra toàn vẹn tệp: [openvietgis_connector-0.3.0.zip.sha256](https://openvietgis.cloud/rewrite/downloads/openvietgis_connector-0.3.0.zip.sha256))
2. Mở **QGIS Desktop** → vào menu **Plugins → Manage and Install Plugins… → Install from ZIP**.
3. Chọn file `openvietgis_connector-0.3.0.zip` vừa tải → bấm **Install Plugin**.
4. Sau khi cài xong, mở plugin từ menu **Web → OpenVietGIS**.

---

## 3. Quy trình đồng bộ 2 chiều thủ công (từng bước)

### Bước 1 — Kết nối và tạo bản làm việc
1. Mở **Web → OpenVietGIS**, điền 4 thông tin:
   - **URL máy chủ**: `https://openvietgis.cloud`
   - **Organization UUID** (mã tổ chức)
   - **Project UUID** (mã dự án)
   - **API Key (Bearer Token)** — khóa chỉ nằm trong RAM khi hộp thoại đang mở, **không bao giờ lưu xuống ổ cứng**.
2. Bấm tải danh sách lớp và chọn lớp cần làm việc.
3. Trong khung **R027**, bấm **1. Tạo bản làm việc**. Plugin sẽ tải GeoPackage (có kiểm tra SHA-256) và tạo thư mục `ovg_qgis_<layer_uuid>/` gồm:
   - `baseline.gpkg`: bản gốc mốc so sánh gần nhất.
   - `working.gpkg`: bản làm việc tự động nạp sẵn lên bản đồ QGIS để bạn biên tập.
   - `workspace.json`: lưu mã phiên bản (revision) và mã băm SHA-256 (**không chứa API key**).

### Bước 2 — Biên tập dữ liệu trên QGIS
- Chỉnh sửa hình học hoặc thuộc tính trực tiếp trên lớp `working.gpkg`.
- **Quan trọng**:
  - Giữ nguyên cột **`ovg_feature_id`** (duy nhất, không để trống). Nếu thêm đối tượng mới, nhớ điền `ovg_feature_id` duy nhất trước khi lưu.
  - Không đổi tên bảng, tên cột hay hệ tọa độ (CRS).
  - Sau khi sửa xong, **bấm Lưu (Save Layer Edits) và tắt chế độ biên tập (Toggle Editing)** trên QGIS.

### Bước 3 — Xem trước chênh lệch 2 chiều
- Bấm **2. Xem trước 2 chiều**.
- Plugin sẽ tải bản mới nhất từ OpenVietGIS vào thư mục tạm, đối chiếu **3 chiều** (`bản gốc` vs `QGIS` vs `OpenVietGIS`) và báo rõ:
  - Số thay đổi sẽ đẩy **QGIS → OpenVietGIS**.
  - Số thay đổi sẽ kéo **OpenVietGIS → QGIS**.
- Nếu có bất kỳ xung đột nào, nút xác nhận sẽ **tự động khóa** và **không ghi gì lên máy chủ**.

### Bước 4 — Xác nhận đồng bộ
- Kiểm tra lại số lượng thay đổi và bấm **3. Xác nhận đồng bộ**.
- Plugin sẽ:
  1. Gộp các thay đổi không xung đột từ máy chủ vào bản tạm trên máy.
  2. Gọi API `sync-preview` kiểm tra đúng số thao tác, rồi gửi **1 lệnh `sync-push` (R006)** kèm mã giao dịch chống trùng lặp.
  3. Đọc lại dữ liệu từ máy chủ để kiểm tra khớp hoàn toàn, sau đó mới cập nhật `working.gpkg`, `baseline.gpkg` và nạp lại lớp trên QGIS.

---

## 4. Quy tắc xử lý xung đột & khôi phục khi rớt mạng

- **Hai bên sửa 2 trường khác nhau trên cùng 1 đối tượng**: Tự động gộp sạch cả hai thay đổi.
- **Hai bên cùng sửa 1 trường khác giá trị, hoặc cùng sửa hình học khác nhau**: Báo **Xung đột (Conflict)** và giữ nguyên dữ liệu hai bên, không ghi đè.
- **Một bên xóa đối tượng**: Đưa vào danh sách **nghi vấn xóa (delete candidate)** — hệ thống **không bao giờ tự xóa dữ liệu**.
- **Rớt mạng / timeout giữa chừng khi đang đẩy lên**:
  - Plugin lưu trạng thái `.pending-merged.gpkg` và **không tự gửi lại mù quáng**.
  - Khi có mạng lại (hoặc sau khi mở lại QGIS bằng nút **Mở workspace**), bấm **Khôi phục giao dịch**: plugin sẽ đối chiếu với máy chủ và chỉ hoàn tất nếu dữ liệu trên máy chủ đã khớp đúng bản vừa đẩy.
- **Lỗi khóa tệp SQLite/OGR**: Hãy tắt chế độ biên tập (Edit Mode) trên QGIS và đóng các lớp khác đang mở cùng file `working.gpkg` trước khi đồng bộ.

---

## 5. Giới hạn hiện tại (cần biết)

- Chưa có chế độ đồng bộ tự động chạy ngầm hay tự ghép nối (pairing) không cần dán API key.
- Plugin này chỉ dùng cho **QGIS Desktop** (không ghi trực tiếp sang PostGIS ngoài, AutoCAD hay MicroStation).
- Không chép API key hay tệp bí mật vào thư mục cài đặt plugin.

---

## 6. Dành cho lập trình viên (chạy kiểm thử & đóng gói ZIP)

```bash
# Chạy kiểm thử unit workspace & integration hub
uv run --project apps/api python -m pytest -q \
  apps/api/tests/unit/integration_hub/test_qgis_workspace.py
uv run --project apps/api python -m pytest -q \
  apps/api/tests/unit/integration_hub

# Kiểm tra lint plugin QGIS
uv run --project apps/api ruff check \
  integrations/qgis_plugin/openvietgis_connector/{client.py,workspace.py,plugin.py}

# Đóng gói tệp ZIP và kiểm tra SHA-256
python tools/package_qgis_plugin.py --output dist/openvietgis_connector-0.3.0.zip
cd dist && sha256sum -c openvietgis_connector-0.3.0.zip.sha256
```
