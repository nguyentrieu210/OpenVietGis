# Ma trận nghiệm thu cộng đồng — OpenVietGIS

**Bản chuẩn v0.1, 2026-11-23.** Đây là **bộ testcase mở để chạy**, không phải bảng kết quả nghiệm thu hiện tại.

| ID | Nhóm | Kiểm thử độc lập và tiêu chí PASS | Mức | Trạng thái mặc định |
| --- | --- | --- | --- | --- |
| CQ-00 | Cài đặt local | Clone sạch, khóa dependency, bootstrap DB/OIDC tổng hợp, mở Project, không có secret thật | P0 | NOT_RUN |
| CQ-01 | Identity / R002 | Hai tenant độc lập; viewer không thể sửa hoặc đọc chéo; logout/revoke chặn truy cập ngay | P0 | NOT_RUN |
| CQ-02 | Bản đồ / R003–R004 | Mở layer Point/Line/Polygon, ẩn/hiện, reorder, lưu/reload extent, inspect thuộc tính và đúng CRS | P0 | NOT_RUN |
| CQ-03 | Import/export / R005 | GeoJSON, GeoPackage và ZIP Shapefile; đếm feature/schema/geometry/CRS trước-sau bằng GDAL/QGIS | P0 | NOT_RUN |
| CQ-04 | Biên tập / R006 | Create/update/delete trên layer giả; snap, undo/revert, stale revision, lịch sử và quyền thao tác | P0 | NOT_RUN |
| CQ-05 | Query / R007 | Lọc thuộc tính typed, spatial intersects/within/contains; so với PostGIS/QGIS, không lọt scope | P0 | NOT_RUN |
| CQ-06 | Processing / R008 | Buffer, Clip, Dissolve sinh **layer kết quả đọc được**; diện tích/shape đối chiếu oracle | P1 | NOT_RUN |
| CQ-07 | Cartography / R009 | Nhãn và ký hiệu; A4 PDF/PNG, tỷ lệ, legend, north, grid, reopen/size phù hợp dữ liệu nguồn | P1 | NOT_RUN |
| CQ-08 | External raster / R010 | Source được phép, COG/raster hiển thị đúng; cắt dữ liệu, CRS, chặn URL lạ/thiếu quyền | P1 | NOT_RUN |
| CQ-09 | Collaboration / R011 | Hai user: comment/review/chat/task realtime, cập nhật/revoke/conflict; không lộ tenant | P0 | NOT_RUN |
| CQ-10 | CAD / R012 | DXF 2D import–edit–export–reopen; tọa độ/đơn vị/arc loss rõ; các lệnh offset/trim/rotate | P1 | NOT_RUN |
| CQ-11 | Offline / R013 | Mất mạng, chỉnh dữ liệu giả, restart, sync/replay, revoke và conflict không làm hỏng dữ liệu | P1 | NOT_RUN |
| CQ-12 | Plugin / R014 | Plugin số nhà bật theo Project, phân quyền đúng, không sửa Core; dữ liệu thử hoàn toàn giả | P1 | NOT_RUN |
| CQ-13 | Ops / R015 | Build phiên bản được ghim, migrations độc lập, backup gồm artifact, restore cách ly, rollback | P0 release | NOT_RUN |
| CQ-14 | GIS/CAD/3D | Chuyển ba chế độ trên một Project, selection context; WebGL thực, engine fail/revoke/dirty state | P1 | NOT_RUN |
| CQ-15 | AI / MCP | OAuth đúng chủ thể/quyền, consent, tool query thật, ghi có phê duyệt, lỗi quyền fail closed | P1 | NOT_RUN |
| CQ-16 | UX & accessibility | Mobile 360–430px, keyboard/focus, toolbar docking, layer panel, attribute table và message lỗi | P1 | NOT_RUN |
| CQ-17 | Hiệu năng | 10.000 đối tượng test, 100 lượt mẫu sau warmup; lưu số p50/p95, tải host và timeout | P0 release | NOT_RUN |

## Quy tắc kiểm thử

- Ghi **SHA, commit/build frontend/backend, role, profile và input fixture** cho từng CQ; nếu lệch thì kết quả không áp cho một release duy nhất.
- `CQ-03`: đối chiếu theo chính xác ma trận định dạng thực sự hỗ trợ, không đánh dấu thất bại vì chức năng ngoài scope chưa công bố; **không** tự tuyên bố DWG native.
- `CQ-05`: toán tử và CRS phải được ghi rõ; không dùng EPSG:3857 như thước đo chính xác khảo sát.
- `CQ-07`: file tải về phải kiểm chứng bằng ứng dụng độc lập, không chỉ xuất hiện nút Download.
- `CQ-10`: ghi rõ mất mát dữ liệu DXF/curve/unit; chỉ kết luận survey-grade khi có chứng cứ kiểm soát.
- `CQ-12`: không tự động xác nhận tuyến, đánh số, cấp số nhà hoặc giả mạo phê duyệt cơ quan chức năng.
- `CQ-13`: chỉ người vận hành có quyền và fixture cô lập mới chạy restore; tester cộng đồng không thao tác production.
- `CQ-17`: ngưỡng phải là ngưỡng đã phê duyệt trên profile cố định, không tự đặt lại sau khi thất bại.

Các mẫu thực thi, trường evidence, cách báo lỗi: [TEST_RUNBOOK](TEST_RUNBOOK.md). Quyết định phát hành: [RELEASE_POLICY](RELEASE_POLICY.md).

### Hiện trạng có thể đối chiếu, không phải PASS của ma trận này

Các báo cáo lịch sử cho biết có nhiều test tự động và kết quả kỹ thuật từng phần; nhưng R018 chưa có bằng chứng nghiệm thu độc lập đầy đủ, các gate release đang mở. **Tất cả CQ mặc định NOT_RUN trên bản cộng đồng này** cho đến khi tester xác thực trên candidate cụ thể.
