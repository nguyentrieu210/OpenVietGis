# Security policy / Báo cáo bảo mật

OpenVietGIS hiện là **public candidate**, chưa được chứng nhận là bản phát hành production.

## Báo cáo riêng tư

Không đăng công khai lỗ hổng đang có khả năng bị khai thác, thông tin đăng nhập, token, đường dẫn nội bộ, ảnh chụp dữ liệu người dùng hoặc bản ghi dữ liệu vị trí nhạy cảm. Nếu GitHub hiển thị mục **Report a vulnerability** trong tab Security, dùng kênh đó. Nếu chưa có, liên hệ riêng chủ repo qua hồ sơ GitHub để xin kênh tiếp nhận an toàn trước khi gửi chi tiết. Không tạo Issue công khai chứa bằng chứng khai thác.

Báo cáo nên chứa: phạm vi ảnh hưởng, phiên bản hoặc SHA, điều kiện tái hiện bằng **dữ liệu giả**, mức độ tác động, cách giảm thiểu và phương thức liên hệ. Không thực hiện pentest hoặc thay đổi dữ liệu trên `openvietgis.cloud` khi chưa có ủy quyền riêng.

## Phạm vi cần chú ý

- OIDC/OAuth, CSRF, service identity và kết nối AI/MCP;
- tenant/project isolation, RLS, revocation và quyền plugin;
- import file GeoJSON, GPKG, Shapefile, DXF và external GIS/OGC URLs;
- xuất file, artifact, backup/restore, CAD relay, logs và secrets.

Chỉ sử dụng repo test/local hoặc fixture riêng đã được chủ dự án cho phép. Báo lỗi không nhạy cảm tại GitHub Issues với template tương ứng. Maintainer xác nhận phạm vi và khắc phục trước khi công bố chi tiết kỹ thuật.
