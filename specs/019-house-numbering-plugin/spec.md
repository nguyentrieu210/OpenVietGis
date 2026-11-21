# Feature Specification: Plugin quản lý và đánh số nhà

**Feature Branch**: `spec/019-house-numbering-plugin`

**Created**: 2026-11-21

**Status**: Draft; scope decisions delegated by owner; release gates remain binding.

**Input**: Port toàn bộ nghiệp vụ số nhà từ Bình Hưng vào plugin chính thức của OpenVietGIS, không sao chép kiến trúc cũ. Parent roadmap: R014 (child 019). [Grill và truy vết](../../docs/grill-house-numbering-plugin.md).

## Clarifications

### Session 2026-11-21

- Chủ dự án yêu cầu làm toàn bộ, tự quyết các lựa chọn kỹ thuật; không hỏi lại.
- Plugin dùng theo Project và không gắn cứng địa bàn Bình Hưng. Nghiệp vụ chung vẫn là nền GIS.
- Quy tắc ban đầu là thử nghiệm, không tự cấp số chính thức; căn cứ, mẫu biểu và quyền nhập dữ liệu thật là gate nghiệm thu.
- Dữ liệu cá nhân tuân thủ quyền tối thiểu và phạm vi dự án; không kế thừa quyền đọc rộng chỉ vì tài khoản đã đăng nhập.
- Chọn định dạng xuất do Core thực sự hỗ trợ; DWG chỉ được tuyên bố khi round-trip đã đo; DXF/GIS mở là đầu ra bắt buộc.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Kích hoạt plugin và quản lý địa bàn (Priority: P1)

Quản trị kích hoạt Số nhà trong dự án. Nhân viên thấy cây xã/ấp/tuyến/hẻm, bản đồ, danh sách và bảng số liệu cùng phạm vi. Có thể ghim, sửa vị trí và quản lý công trình, thửa tham chiếu.

**Why this priority**: Là nền tảng sử dụng module thật và phân quyền, không biến số nhà thành Core.

**Independent Test**: Kích hoạt hai dự án độc lập, tạo đường, hẻm, ghim ba nhà, chọn một hẻm, tải lại và kiểm tra chỉ dữ liệu tương ứng xuất hiện.

**Acceptance Scenarios**:
1. Given dự án chưa kích hoạt, when mở workspace, then không có công cụ số nhà và dữ liệu module không đọc được.
2. Given dự án được kích hoạt, when vẽ tuyến và ghim nhà, then geometry, nguồn, CRS và quan hệ được lưu; chọn cây địa bàn đồng bộ bản đồ/bảng.
3. Given tuyến có nhà hoặc hẻm, when xóa, then chặn và chỉ rõ ràng buộc; khi sửa chiều đã có số duyệt hoặc chờ duyệt, then chặn.
4. Given hai dự án/tổ chức, when truy cập trực tiếp hoặc thay đổi phân quyền, then không lộ dữ liệu ngoài phạm vi; disable không xóa dữ liệu.

### User Story 2 - Kiểm tra và lập số nhà trên bản đồ (Priority: P1)

Cán bộ bấm bất kỳ điểm hoặc chọn nhà, tra số hiện tại, thửa, tuyến, số liền kề, căn cứ và nhận đề xuất giải thích được; biên tập viên xem trước rồi áp dụng số cho từng hoặc nhiều nhà.

**Why this priority**: Công việc chính của cán bộ thụ lý hằng ngày.

**Independent Test**: Dùng tuyến mẫu 100, 102, 106, hẻm 155 và các điểm hai phía, kiểm tra đề xuất mà không ghi dữ liệu, rồi áp dụng.

**Acceptance Scenarios**:
1. Given nhà đã có số, when kiểm tra, then trả số hiện tại và căn cứ, không ghi số mới.
2. Given điểm giữa 100 và 106, when đề xuất, then trả 102 nếu trống; giữa 100 và 102 trả 100A, sau A trả B; quy tắc dạng gạch ngang trả 100-1.
3. Given hẻm/nested alley có tiền tố 155, when ghim và đánh số, then giữ tiền tố, thứ tự, parity theo chiều tuyến; quá ngưỡng gần tuyến báo không xác định.
4. Given ghim/tính lại hàng loạt, when xem trước, then có diff, vị trí, cảnh báo và lý do; hủy không ghi, xác nhận ghi nguyên tử; bản sửa lỗi thời bị từ chối.
5. Given có số đã duyệt hoặc đang trình, when chèn/xóa/dời, then số bảo vệ không đổi, hết khoảng trống báo xung đột; đất trống chỉ giữ số dự phòng nếu có.

### User Story 3 - Phương án, duyệt và lịch sử (Priority: P1)

Biên tập viên lập phương án theo tuyến/ấp, sửa số đề xuất, trình cho người có quyền duyệt; ghi nhận trạng thái gắn biển, lý do không đủ điều kiện, lịch sử.

**Why this priority**: Phân biệt đề xuất với số chính thức, ngăn cấp trùng.

**Independent Test**: Tạo phương án 4 nhà, thử trùng, từ chối có góp ý, sửa, trình lại, duyệt và xem lịch sử.

**Acceptance Scenarios**:
1. Given số trùng cùng tuyến hoặc dữ liệu thay đổi, when trình/duyệt, then chặn với lý do.
2. Given người duyệt đúng phạm vi, when từ chối, then đề xuất trở về đang soạn và giữ góp ý; khi duyệt, then ghi actor/thời điểm/căn cứ; giữ số cũ.
3. Given tách/nhập công trình, when đề xuất, then giữ lịch sử số gốc, không tự duyệt; công trình không đủ điều kiện không được cấp mới.

### User Story 4 - Nhập và đối soát ba nguồn (Priority: P1)

Quản trị nhập danh sách cấp đổi, bản vẽ và dữ liệu GIS, đối soát với khảo sát; biên tập xử lý khác biệt và trường hợp không ghép được.

**Independent Test**: Nạp hai file có xung đột, một file lặp và bộ dự liệu 7 nhóm rà soát; kiểm tra tổng và dòng không ghép.

**Why this priority**: Dữ liệu có căn cứ và kiểm soát chất lượng cho việc cấp số.

**Acceptance Scenarios**:
1. Given file có nguồn/lớp/CRS, when xem trước và nạp, then báo số đọc/nhận/loại, lỗi, Unicode, mâu thuẫn; không nạp lặp hoặc chấp nhận CRS không rõ.
2. Given chứng nhận số cũ và số mới khác CAD, when ghép, then dùng ID hoặc số cũ duy nhất trước; trường hợp mơ hồ vào hàng chờ, vị trí ước tính đánh dấu rõ.
3. Given ba nguồn, when rà soát, then mỗi nhà thuộc một nhóm: khớp, lệch số, CAD khác chứng nhận, phát sinh, không còn, chưa khảo sát; chứng nhận chưa ghép báo riêng. Tổng đếm khớp và bộ lọc địa bàn hoạt động.

### User Story 5 - Khảo sát thực địa và tiến độ (Priority: P2)

Khảo sát viên nhận địa bàn, dùng điện thoại chụp ảnh, GPS, nhập hiện trạng và phiếu; quản trị theo dõi chỉ tiêu theo ngày, ấp, tuyến.

**Why this priority**: Xác minh thực tế và điều phối đội hiện trường.

**Independent Test**: Gửi 50 phiếu tổng hợp có ảnh trong nhiều lần mất mạng, mở lại và đồng bộ; kiểm tra báo cáo.

**Acceptance Scenarios**:
1. Given mất mạng, when lưu phiếu và khởi động lại, then nháp tồn tại; nối mạng không mất/trùng phiếu/ảnh.
2. Given GPS lệch, dữ liệu trùng hoặc sửa đồng thời, when gửi, then cảnh báo/chặn phù hợp; thao tác giữ audit.
3. Given cấp phép và ghi nhận đồng ý, when nhập thông tin hộ, then tài khoản chỉ nhập không đọc lại dữ liệu nhạy cảm; thiếu CCCD vẫn hợp lệ; gửi hộ chỉ đúng người phụ trách.
4. Given các phiếu và chỉ tiêu, when xem tiến độ, then tính đúng số nhà/đất trống, người, ngày, địa bàn; chưa có chỉ tiêu không tính phần trăm; xuất Excel không có thông tin cá nhân.

### User Story 6 - Hồ sơ và giấy tờ (Priority: P2)

Cán bộ tạo phiếu kiểm tra Word từ điểm chọn (địa chính, không ảnh, lưới số), quản lý hồ sơ nguồn, in giấy chứng nhận, phiếu nhà và biển số từng nhà/hàng loạt.

**Why this priority**: Sản phẩm phục vụ cấp số và bàn giao.

**Independent Test**: Xuất phiếu cho điểm có/không ảnh, in giấy cho số duyệt và thử số chưa duyệt, đổi số và kiểm tra trạng thái cấp lại.

**Acceptance Scenarios**:
1. Given một điểm, when xuất Word, then đúng tọa độ, ba khung ảnh/bản đồ, căn cứ, nhận xét và số dự kiến; thiếu ảnh ghi rõ.
2. Given số được duyệt và mẫu được xác minh, when in, then lưu số giấy, ngày, người và audit; số chưa duyệt bị chặn; số thay đổi khiến giấy cũ cần cấp lại.
3. Given chọn tuyến, when in hàng loạt, then xuất đủ giấy hợp lệ và danh sách bỏ qua; mẫu chưa được xác minh chỉ xuất bản thử.

### User Story 7 - Tra cứu và bàn giao (Priority: P2)

Cán bộ tìm số cũ/mới, tuyến, ấp, tọa độ và hồ sơ; xuất dữ liệu theo phạm vi, lớp, định dạng, trường và CRS.

**Why this priority**: Dữ liệu dùng được và bàn giao được cho đơn vị khác.

**Independent Test**: Tìm số cũ, xuất DXF/GPKG và Excel theo hẻm, mở trong phần mềm độc lập, kiểm tra số lượng, tọa độ và không rò PII.

**Acceptance Scenarios**:
1. Given số cũ hoặc tọa độ, when tìm, then trả đúng nhà, vị trí, số hiện tại và nguồn; tìm tên chủ hộ chỉ trong quyền riêng.
2. Given gói khai thác, when xuất, then lớp/tọa độ/thuộc tính đúng, có metadata, không có PII.
3. Given gói gốc được ủy quyền, when yêu cầu, then kiểm soát quyền, danh mục, biên bản và audit; không tự đưa tệp gốc lên web công khai.

### Edge Cases

- Địa bàn không có tuyến, giao lộ nhiều tuyến, hình học lỗi, nghịch chiều hoặc CRS chưa biết: báo cần rà soát; không tự quyết.
- Số dạng C5/2, E14/..., 1L, giá trị không đủ điều kiện: giữ nguyên nguồn, không tự chuyển thành số.
- Nhà chia/gộp, chứng nhận thay đổi, hẻm số chưa có đường geometry, cấp lại giấy: bảo toàn lịch sử và gắn cờ.
- Replay import/phiếu/apply, token hết hạn, revision xung đột, thiếu ảnh: xử lý như acceptance; không đánh dấu verified giả.
- Khóa plugin, thất bại nhập/xuất, sự cố backup: phục hồi an toàn; dữ liệu vẫn truy cập theo quyền khi bật lại.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: Kích hoạt/khóa module theo dự án, quyền riêng, không thay đổi Core hoặc xóa dữ liệu khi khóa.
- **FR-002**: Cây địa bàn hỗ trợ khu vực hiện hành/lịch sử, tuyến/hẻm nhiều cấp và hẻm theo số; bản đồ, bảng, số liệu và tìm/xuất cùng phạm vi.
- **FR-003**: Quản lý vị trí công trình, thửa, tuyến có geometry và CRS rõ, ghim/sửa/xóa có lịch sử; chặn xóa tuyến còn phụ thuộc.
- **FR-004**: Lưu riêng số cũ, chứng nhận, CAD, khảo sát, đề xuất, duyệt; lưu provenance, trạng thái, lý do và lịch sử.
- **FR-005**: Kiểm tra điểm bất kỳ, trả vị trí/tọa độ, thửa, ấp, nhà, căn cứ, tuyến gần, kề trước/sau cùng phía, lưới và hồ sơ; không đổi dữ liệu.
- **FR-006**: Đánh số theo chiều, chẵn/lẻ, vị trí tương đối tuyến, số dự phòng, khoảng trống, số phụ cấu hình chữ hoặc gạch ngang, tiền tố hẻm nhiều cấp.
- **FR-007**: Quy tắc có phiên bản bất biến, ngưỡng khoảng cách, hướng, căn cứ; phiên bản thiếu căn cứ là thử nghiệm.
- **FR-008**: Ghim/đánh số hàng loạt, chèn/xóa/dời số và tính lưới; preview có diff/cảnh báo; apply có xác nhận, nguyên tử, chống lặp và xung đột.
- **FR-009**: Số được cấp/duyệt/đang trình và số dự phòng bất biến với thao tác đánh số khác; chặn trùng số chính thức trên cùng tuyến.
- **FR-010**: Trình, góp ý, từ chối, duyệt theo quyền và revision; lịch sử actor/time; theo dõi gắn biển, không đủ điều kiện, tách/nhập và cấp lại.
- **FR-011**: Nhập file CAD/GIS/Excel theo ánh xạ nguồn, CRS, lớp, mã tiếng Việt, ưu tiên, kiểm tra và xem trước; báo đọc/nhận/loại, lý do và xung đột; giữ bản gốc được ủy quyền.
- **FR-012**: Chỉ nhập dữ liệu legacy có quyền; ở trạng thái chờ rà soát, chống trùng nguồn và lần nạp; không coi tên/địa chỉ cũ đã xác minh.
- **FR-013**: Ghép chứng nhận ưu tiên ID/cặp số cũ duy nhất, theo tuyến và số mới khi đủ evidence, không tự ghép mơ hồ; vị trí ước tính phân biệt.
- **FR-014**: So sánh ba nguồn, nhóm kết quả loại trừ nhau như US4, chứng nhận chưa ghép riêng; lọc, ghi xử lý và đưa vào phương án.
- **FR-015**: Khảo sát mobile GPS/ảnh, hiện trạng, người/thời gian, chờ rà soát cho nhà mới, cảnh báo sai lệch, kiểm tra dữ liệu.
- **FR-016**: Nháp offline bền vững sau khởi động lại, đồng bộ không mất/duplicate, xung đột có xử lý, gửi hộ được ủy quyền.
- **FR-017**: Chỉ tiêu, phân công, nhật ký và tiến độ theo ấp/tuyến/ngày/công nhân; số liệu từ phiếu đã nhận, báo cáo Excel.
- **FR-018**: Phiếu kiểm tra Word gồm bản đồ địa chính, không ảnh (nếu có), lưới số và nhận xét/đề xuất; sổ phiếu, audit.
- **FR-019**: Hồ sơ nguồn có tài liệu, nguồn, quyền tải và audit; giữ liên kết tới vị trí, căn cứ.
- **FR-020**: Giấy chứng nhận/phiếu nhà/biển số, in riêng/lô, duyệt trước khi cấp, giữ lịch sử và cấp lại; mẫu chưa xác minh chỉ thử.
- **FR-021**: Tìm kiếm số cũ/mới/đề xuất, đường, địa bàn, tọa độ; tên/liên hệ chỉ cho vai trò được cấp quyền và audit.
- **FR-022**: Xuất phạm vi/lớp/trường/CRS sang định dạng được xác minh; DXF, GIS có thuộc tính, Excel/CSV, tài liệu và bản đồ; công bố rõ giới hạn DWG.
- **FR-023**: Hai gói bàn giao: khai thác đã loại PII, gốc chỉ qua ủy quyền và biên bản; dịch vụ bản đồ không đưa PII.
- **FR-024**: Dữ liệu hộ/CCCD tùy chọn và tách biệt; chỉ thu thực tế khi đủ căn cứ và đồng ý; giới hạn quyền thời vụ, quyền đọc/xuất.
- **FR-025**: Audit mọi đọc/xuất dữ liệu nhạy cảm, không sửa được bởi người thường; chống rò rỉ qua bản đồ/lỗi/log/gói xuất; cơ chế lưu giữ và xóa có phê duyệt.
- **FR-026**: Giao diện tiếng Việt, desktop/mobile, đồng bộ chọn map/table, trạng thái lỗi/quyền/loading rõ; dùng được sau reload.
- **FR-027**: Tương thích giữa plugin và nền tảng, cách ly tenant, kiểm thử hồi quy khi plugin không tồn tại hoặc Land Asset cùng bật.
- **FR-028**: Tồn tại lịch sử, backup/restore cả dữ liệu và artifact, rollback migration; báo rõ khả năng chưa đáp ứng; không external release khi gate chưa qua.

### Key Entities *(include if feature involves data)*

Project activation; area/road/alley; parcel reference; building geometry; address evidence & number state/history; numbering rule and grid/preview; proposal/item; source batch & reconciliation case; field survey/draft/photo; target/assignment; restricted household/identity; inspection dossier/certificate; export & audit event.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Hai project/tổ chức độc lập và một project không bật plugin vượt 100% thử quyền/disable/re-enable; Land Asset và Core không plugin vẫn chạy.
- **SC-002**: 100% bộ ca parity, gap/suffix, alley, direction, reserved và trùng số đạt; preview không ghi; apply stale hoặc thất bại không ghi một phần.
- **SC-003**: 0 số chính thức trùng cùng tuyến; 100% số đã duyệt/đang trình không đổi ngoài quy trình phê duyệt.
- **SC-004**: 100% dòng nhập có trạng thái nhận/loại kèm lý do; nhập lặp tạo 0 bản ghi trùng; 500 nhãn mẫu đạt tối thiểu 99% khi dữ liệu hợp lệ.
- **SC-005**: 100% nhóm đối soát đúng trên bộ ca vàng và tổng bằng số công trình; chứng nhận chưa ghép có danh sách riêng.
- **SC-006**: 50 phiếu kèm ảnh qua offline/restart/reconnect: 0 mất, 0 trùng; phiếu thường dưới 3 phút.
- **SC-007**: 100% số đếm tiến độ khớp phiếu; 100% giấy được phát hành có số đã duyệt; lô 100 giấy thử dưới 2 phút trên profile nghiệm thu.
- **SC-008**: 0 PII trong gói khai thác, bản đồ và lỗi; 100% truy cập nhạy cảm được audit; các test tấn công chéo project từ chối.
- **SC-009**: Mẫu 100 đối tượng round-trip giữ sai lệch không quá 0,05 m khi transform đã được kiểm chứng, số lượng/thuộc tính không mất ngoài giới hạn công bố.
- **SC-010**: Một cán bộ mới tra số cũ trong 60 giây; kiểm tra vị trí trong 3 giây p95 với 10k nhà mẫu; benchmark 100k nhà và 50 user, ghi cấu hình và kết quả.
- **SC-011**: Workflow bật → vẽ/ghim → kiểm tra → preview/apply → trình/duyệt → tra cứu/xuất → mở lại thành công trong browser; khôi phục đầy đủ dữ liệu/module artifact trên profile thử.

## Assumptions

- Phạm vi đầy đủ HN01–13; dùng GIS Core theo năng lực đã nghiệm thu, không tái tạo nền GIS; public citizen portal, riêng app native, tích hợp phần mềm thành phố và đánh số căn hộ chung cư ngoài phạm vi.
- Quy tắc và CRS khác theo địa phương; không gắn cứng dữ liệu hoặc thông tin Bình Hưng, không nhập PII thật trong test.
- Thiếu căn cứ/đồng ý/mẫu biểu, không cho cấp số chính thức hay thu dữ liệu nhạy cảm thật; vẫn phát triển/kiểm thử bằng fixtures tổng hợp.
- Các dependency R004/R006/R008/R010/R013/R014/R015 và audit 018 chưa qua tất cả gate. Kế hoạch này không phải xác nhận hoàn thành hoặc cho phép production release.
