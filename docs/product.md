# OpenVietGIS — Định hướng sản phẩm

**Baseline**: 2026-11-18

**Phạm vi**: định hướng toàn sản phẩm; chi tiết từng tính năng nằm trong spec tương ứng.

**Bản demo trực tuyến**: https://openvietgis.cloud  
**Mã nguồn công khai**: https://github.com/nguyentrieu210/OpenVietGis

> Bản demo phục vụ trải nghiệm và lấy phản hồi cộng đồng. Các khả năng của từng nhóm tính năng cần được xác minh theo roadmap, đặc tả và bằng chứng nghiệm thu; việc có mặt trên giao diện không mặc nhiên đồng nghĩa với sẵn sàng vận hành sản xuất.

## Mục tiêu

OpenVietGIS là không gian làm việc GIS trên web cho cá nhân chuyên môn và nhóm.
Người dùng tổ chức project, đưa dữ liệu vào, xem và truy vấn trên bản đồ, biên tập,
xử lý và xuất kết quả để tiếp tục sử dụng trong các hệ thống khác.

Sản phẩm học các năng lực có giá trị từ QGIS, MapInfo, ArcGIS, AutoCAD và các hệ thống
liên quan. Mục tiêu là hoàn thành những luồng công việc đáng tin cậy trên web;
không cam kết sao chép toàn bộ tính năng hay giao diện của các sản phẩm đó.

- Lộ trình và phụ thuộc: [ROADMAP.md](../ROADMAP.md).
- Nguyên tắc bắt buộc: [Constitution](../.specify/memory/constitution.md).
- Quyết định kỹ thuật: [ADR index](adr/README.md).
- Ranh giới mã nguồn: [Architecture](architecture.md).

## Người dùng và cách triển khai đầu tiên

Người dùng ưu tiên là chủ dự án và một nhóm người đã biết làm việc với dữ liệu GIS.
Mốc đầu chạy local để kiểm chứng bằng dữ liệu thật trước khi triển khai cho nhiều tổ chức.
Local không đồng nghĩa với bỏ tổ chức, phân quyền hoặc tenant isolation.

Cloud, self-host và enterprise dùng chung mô hình sản phẩm. Organization sở hữu tài
nguyên; Workspace là cách nhóm Project và có thể là phạm vi cấp quyền. Project luôn
thuộc trực tiếp một Organization, có hoặc không có Workspace.

Người dùng hiện trường và người dùng nghiệp vụ ít kiến thức GIS được phục vụ bằng
những luồng hoặc module chuyên biệt sau khi Core đủ năng lực.

## Luồng giá trị

1. Chọn Organization và tạo/mở Project.
2. Nhập hoặc đăng ký nguồn dữ liệu, kiểm tra cấu trúc và CRS.
3. Thêm Layer, thiết lập cách hiển thị và xem thuộc tính.
4. Truy vấn, chỉnh sửa hoặc xử lý trong phạm vi quyền được cấp.
5. Lưu Project cùng các thay đổi dữ liệu theo những ranh giới rõ ràng.
6. Xuất dữ liệu/kết quả và mở lại ở hệ thống khác để kiểm chứng.

Bản dùng thử đầu tiên chứng minh bước nhập, xem, lưu/mở lại và xuất. Biên tập đầy đủ,
xử lý và cộng tác là các mốc tiếp theo, không được coi là đã có khi chỉ hoàn tất nền.

## Các nhóm năng lực

| Nhóm | Giá trị phải chứng minh | Roadmap |
| --- | --- | --- |
| Nền và tổ chức | Cài/chạy được; xác định đúng người, tổ chức, tài nguyên và quyền | R001–R002 |
| Dữ liệu và CRS | Nguồn/layer rõ ràng, tọa độ có ý nghĩa, nhập–xuất bảo toàn thông tin | R003, R005 |
| Bản đồ và truy vấn | Hiển thị, chọn, xem, lọc và tìm dữ liệu phục vụ công việc | R004, R007 |
| Biên tập đáng tin cậy | Thay đổi có attribution, revision, kiểm tra hình học và xử lý xung đột | R006 |
| Phân tích và xử lý | Thuật toán có input/output, trạng thái và kết quả kiểm chứng được | R008 |
| Trình bày và trao đổi | Ký hiệu, nhãn, bố cục, đầu ra bản đồ và nguồn dữ liệu ngoài | R009–R010 |
| Cộng tác | Nhiều người cùng làm với quyền, thay đổi và cơ chế phối hợp rõ | R011 |
| CAD và hiện trường | Trao đổi bản vẽ, thao tác chính xác, làm việc ngoài hiện trường/offline | R012–R013 |
| Module và phát hành | Mở rộng nghiệp vụ; vận hành cùng mô hình trên các cách triển khai | R014–R015 |

## Các mốc sản phẩm

- **M0 — Nền**: môi trường, persistence, ranh giới kiến trúc và kiểm thử đã có bằng chứng.
- **M1 — Dùng thử GIS local**: người dùng có quyền nhập dữ liệu hỗ trợ, xem trên bản đồ
  và bảng thuộc tính, lưu/mở lại Project, xuất và kiểm chứng ở công cụ khác.
- **M2 — Làm việc thực tế**: biên tập, lịch sử/xung đột, truy vấn và đầu ra bản đồ đáp ứng
  ít nhất một quy trình tham chiếu được chọn; không cần module nghiệp vụ để Core hữu ích.
- **M3 — Phát hành cho team**: nhiều người làm việc có kiểm soát; quyền được thực thi
  xuyên suốt dữ liệu, tác vụ và đầu ra; quy trình triển khai, backup/restore và recovery
  được diễn tập cho cấu hình phát hành đã chọn.

Bảng phụ thuộc và điều kiện nghiệm thu nằm trong ROADMAP.md. M0–M3 là các mốc kiểm
chứng năng lực, không phải ngày hẹn, phần trăm hoàn thành hay cam kết tương đương desktop GIS.
Phân tích nâng cao, nguồn ngoài, CAD, hiện trường và module được ưu tiên theo nhu cầu thật;
chúng không mặc nhiên phải hoàn tất trước mọi bản phát hành cho team.

## Quy tắc chất lượng xuyên suốt

- **Quyền và ownership**: áp dụng từ tài nguyên tenant đầu tiên; local và cloud cùng semantics.
- **CRS**: biết CRS nguồn, lưu trữ và hiển thị; thiếu thông tin phải được làm rõ. VN-2000
  là yêu cầu trọng yếu; mỗi phép chuyển đổi cần bộ kiểm chứng tương ứng.
- **Dữ liệu mở**: có đường xuất khả dụng, thông báo rõ chuyển đổi hoặc mất thông tin.
- **Lịch sử**: import, sửa, xử lý sinh dữ liệu và schema mutation có actor/provenance
  cùng change/revision tối thiểu ngay tại tính năng giới thiệu chúng.
- **Khả năng thực tế**: Layer khác DataSource và khác bảng DB; công cụ dùng capability
  để quyết định thao tác nào hợp lệ.
- **Vận hành**: migration, recovery, log và bảo vệ secrets xuất hiện cùng tính năng;
  R015 hoàn thiện mức phát hành, không trì hoãn các yêu cầu này.
- **Hiệu năng**: mỗi spec nhạy hiệu năng chốt dữ liệu benchmark, môi trường, ngân sách
  thời gian/tài nguyên và bằng chứng trước khi nghiệm thu.

## Các quyết định phải được chốt ở spec liên quan

Roadmap không tự chọn thay thiết kế chi tiết. Các vấn đề dưới đây chưa được coi là
cam kết hỗ trợ chỉ vì đã xuất hiện trong thảo luận:

| Quyết định | Nơi phải chốt |
| --- | --- |
| Bộ dữ liệu tham chiếu, kích thước, loại hình học và benchmark | R003–R005, sau đó từng tính năng |
| Bộ CRS/VN-2000 cụ thể, control points và quy tắc biến đổi | R003, R005 và mỗi nguồn mới |
| Ma trận đọc/ghi GeoJSON, GeoPackage, Shapefile; giới hạn và sai khác round-trip | R005 |
| Cách biểu diễn dữ liệu đã nhập trong PostGIS và lưu file gốc/derived artifacts | R003–R005; ADR nếu quyết định khó đảo ngược |
| Phạm vi vẽ/sửa, snapping, undo, topology và edit engine | R006; nâng cao chính xác ở R012 |
| Danh sách thuật toán, nguồn ngoài và phạm vi CAD được hỗ trợ | R008, R010, R012 |
| Quy trình tham chiếu để đạt M2 và cấu hình phát hành để đạt M3 | Chốt trước khi mở các spec của mốc đó |
| Billing, gói thương mại, quota và entitlement cụ thể | Nhánh thương mại của R015 |

Các quyết định kiến trúc đã Accepted trong ADR tiếp tục có hiệu lực. Tài liệu này không
sửa Constitution hay thay thế spec/plan đang được thực hiện.
