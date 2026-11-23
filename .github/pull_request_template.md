## Mục đích / Linked Issue

Closes #

## Phạm vi thay đổi

- [ ] API / schema / migration
- [ ] Frontend / UX
- [ ] GIS / CAD / 3D / raster
- [ ] Plugin
- [ ] Documentation / tests

## Chứng cứ và môi trường

- Base SHA:
- Head SHA:
- OS / Node / Python / DB / PostGIS:
- Fixture tổng hợp, CRS, quy mô:
- Lệnh kiểm thử đã chạy và kết quả:
- Link Issue CQ-xx / bằng chứng (đã che dữ liệu):

## Review checklist

- [ ] Không thêm dữ liệu production, thông tin người dân, tokens hoặc credentials
- [ ] Core không phụ thuộc module ngành dọc; permission/tenant/revision vẫn được bảo vệ
- [ ] Cập nhật doc/test và kiểm tra tương thích API/CRS nếu liên quan
- [ ] Có kế hoạch rollback/migration nếu thay đổi dữ liệu hoặc deployment
- [ ] Test trên commit cuối; không dùng test ở SHA cũ làm bằng chứng phát hành
- [ ] Những điểm chưa chạy/FAIL/BLOCKED được ghi rõ
- [ ] Không deploy tự động production từ PR này

## Giới hạn và rủi ro

Mô tả điều chưa nghiệm thu, các giới hạn quan sát, rủi ro còn mở và người review độc lập. PR **Draft** đến khi sẵn sàng review.
