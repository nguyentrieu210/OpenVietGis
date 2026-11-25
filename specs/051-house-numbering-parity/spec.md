# Feature Specification: Số nhà đúng nghiệp vụ Bình Hưng và thuận tay người dùng

**Feature Branch**: `spec/051-house-numbering-parity`

**Created**: 2026-11-25

**Status**: Draft — chờ chủ dự án duyệt

**Parent**: R014 → [Spec 019 plugin số nhà](../019-house-numbering-plugin/spec.md). Spec này sửa nghiệp vụ và trải nghiệm của plugin `house_numbering` đã có; không tạo plugin mới, không đưa số nhà vào Core.

**Input**: Chủ dự án: "Cần phát triển plugin này đúng nghiệp vụ và thuận tiện cho người sử dụng." Chuẩn nghiệp vụ là app số nhà Bình Hưng đang chạy (sonha.gpcds.site, repo `binh-hung-webgis` nhánh main e9e4f0d). Người dùng: đơn vị tư vấn/xác minh, cán bộ xã/phường, đội khảo sát hiện trường. Ưu tiên: rà soát số hiện hữu, đề xuất và cấp số mới, in phiếu/hồ sơ/lưới, khảo sát hiện trường, di chuyển điểm số nhà và cập nhật số hiện tại. Được nạp dữ liệu thật Bình Hưng vào một dự án riêng tư để nghiệm thu.

## Clarifications

### Session 2026-11-25 (chủ dự án)

- Chuẩn "đúng nghiệp vụ" là app Bình Hưng đang chạy, kể cả mẫu phiếu kiểm tra của xã. Quy tắc khác địa phương thì cấu hình theo dự án, mặc định theo Bình Hưng.
- Thiết kế cho cả ba nhóm người dùng; nhóm hiện trường dùng điện thoại, có lúc mất sóng.
- Được nạp dữ liệu thật Bình Hưng (có tên chủ) vào một dự án riêng tư, chỉ người được mời mới vào.
- **Thay quyết định "không kế thừa quyền đọc rộng" của Spec 019 cho dự án dùng chế độ Bình Hưng:** thành viên đã đăng nhập của dự án thấy tên chủ trong ứng dụng, kể cả nhãn trên bản đồ, mỗi lượt xem có nhật ký; CCCD và số điện thoại vẫn cần quyền riêng. Tên và CCCD không bao giờ ra bản đồ công khai, dịch vụ bản đồ cho bên ngoài hay gói khai thác; chỉ "gói gốc" bàn giao cho người có quyền mới chứa chúng.

### Bằng chứng khoảng trống (25/11)

Đối chiếu mã hai bên (plugin tại `release/unified-validation` d294e69) cho thấy: gợi ý số chỉ dùng công trình đã liên kết và chẵn/lẻ cứng; chưa có chấm điểm, lớp gộp số hiện hữu, vị trí ước tính chứng nhận, xuất lưới theo phạm vi; phiếu kiểm tra thiếu trường mẫu xã; số đề xuất xếp trên số chứng nhận; nhãn trạng thái chấm điểm sai nghĩa. Giao diện là khoảng 20 biểu mẫu trong hộp thoại che bản đồ, nhiều chữ tiếng Anh, UUID, JSON, sửa ghim bằng hộp hỏi UUID, khảo sát không có ô số nhà và không chụp ảnh khi mất mạng.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Bấm vào nhà là biết số và có số gợi ý (Priority: P1)

Người dùng bấm bất kỳ điểm, nhà hay thửa nào trên bản đồ. Khung bên cạnh hiện ngay: thửa, ấp, tuyến gần nhất, bên đường, số hiện tại (theo giấy chứng nhận), số cũ, nhãn số trên bản vẽ, số chấm điểm, các nhà liền trước và liền sau cùng bên, số gợi ý kèm căn cứ. Một nút "Áp dụng số gợi ý", cho sửa số trước khi áp dụng.

**Why this priority**: Đây là thao tác cán bộ làm nhiều nhất; hiện nay plugin bắt chọn tuyến và quy tắc trước, không cho thấy các số đang có và gợi ý sai với dữ liệu thật.

**Independent Test**: Trên dữ liệu Bình Hưng, bấm 200 điểm chọn ngẫu nhiên, so số gợi ý và căn cứ với app Bình Hưng trên cùng điểm.

**Acceptance Scenarios**:

1. **Given** một điểm đã có số, **When** bấm vào, **Then** hiện số đó, không ghi gì vào dữ liệu.
2. **Given** điểm nằm giữa nhà 100 và 106 cùng bên, **When** bấm, **Then** gợi ý 102 nếu chưa dùng; giữa 100 và 102 thì gợi ý 100A, rồi 100B nếu A đã dùng.
3. **Given** tuyến vẽ ngược chiều đánh số (như Đê bao HTX 88), **When** bấm, **Then** bên lẻ và chiều tăng được suy từ các số đang có, gợi ý vẫn đúng.
4. **Given** tuyến chỉ có nhãn số trên bản vẽ, chưa có nhà liên kết, **When** bấm, **Then** nhãn trong phạm vi 35 m được dùng làm mốc, không gợi ý lại từ 1/2.
5. **Given** trên một bên có một số sai chẵn/lẻ hoặc không tăng dần, **When** bấm, **Then** bỏ qua số đó, vẫn gợi ý và cảnh báo rõ số bị bỏ qua.
6. **Given** điểm trong hẻm có tiền tố như 155/22 hoặc có chữ Đ, **When** bấm, **Then** gợi ý giữ đúng tiền tố hẻm.
7. **Given** nhà là đa giác, **When** bấm, **Then** dùng tâm của nhà để kiểm tra, không khóa nút.

---

### User Story 2 - Kéo điểm số nhà và sửa số hiện tại ngay trên bản đồ (Priority: P1)

Người dùng chọn một điểm số nhà, kéo tới đúng vị trí, sửa số trực tiếp hoặc trong bảng thuộc tính bên dưới bản đồ, lấy số theo lưới, đánh dấu giữ/loại/mở lại; mọi thay đổi có lịch sử và hoàn tác được.

**Why this priority**: Chủ dự án nêu riêng việc này; app Bình Hưng có "Sửa số nhà kiểu QGIS" còn plugin chỉ có hộp hỏi UUID.

**Independent Test**: Kéo 10 điểm, sửa 5 số, hoàn tác 2 lần, tải lại trang, kiểm vị trí, số và lịch sử đúng.

**Acceptance Scenarios**:

1. **Given** điểm được chọn, **When** kéo sang vị trí mới và thả, **Then** vị trí được lưu, có người sửa, thời điểm và vị trí cũ trong lịch sử.
2. **Given** đang sửa, **When** bấm hoàn tác, **Then** điểm và số trở về như trước, không để lại bản ghi dở.
3. **Given** hai người cùng sửa một điểm, **When** người sau lưu, **Then** được báo dữ liệu đã đổi, không ghi đè mất thay đổi của người trước.
4. **Given** dòng chọn trong bảng thuộc tính, **When** chọn, **Then** bản đồ chọn và đưa tới đúng điểm, và ngược lại.

---

### User Story 3 - Rà soát số hiện hữu với chứng nhận, bản vẽ và chấm điểm (Priority: P1)

Người dùng xem một lớp gộp "Số nhà hiện hữu" (số đã duyệt, số chứng nhận, nhãn bản vẽ, chứng nhận có vị trí ước tính) và lớp "Chấm điểm" đã được đối soát tự động; lọc theo trạng thái, xử lý từng điểm lệch, trùng, chưa có số.

**Why this priority**: Rà soát là việc đầu tiên trước khi cấp số; plugin chưa có đối soát chấm điểm và chưa có lớp gộp.

**Independent Test**: Nạp dữ liệu Bình Hưng, so số đếm theo từng trạng thái và từng nhóm đối soát với app Bình Hưng trên cùng dữ liệu.

**Acceptance Scenarios**:

1. **Given** lớp chấm điểm đã nạp, **When** đối soát, **Then** mỗi điểm thuộc đúng một trạng thái: khớp, lệch, sai số, không có trong lưới, chưa có số, chưa có vị trí; số điểm chấm trùng được đếm riêng.
2. **Given** dòng chứng nhận có số cũ trùng nhau, **When** ghép, **Then** bỏ qua số cũ trùng và ghép tiếp theo số mới, không dừng cả lô.
3. **Given** dòng chứng nhận không ghép được vị trí, **When** hiển thị, **Then** có vị trí ước tính, ghi rõ là ước tính và cách ước tính.
4. **Given** đối soát ba nguồn, **When** tổng hợp, **Then** dùng đúng 7 nhóm BRD và các tổng cộng lại khớp.
5. **Given** có cả số chứng nhận và số đề xuất, **When** hiển thị hoặc xuất, **Then** số hiện tại là số chứng nhận; số đề xuất ở cột riêng.

---

### User Story 4 - Đánh số cả tuyến hoặc cả hẻm ngay trên bản đồ (Priority: P1)

Người dùng chọn một tuyến hay hẻm trên bản đồ hoặc trên cây địa bàn, bấm "Đánh số cả tuyến"; bản đồ hiện nhãn cũ → mới, tô màu chỗ có cảnh báo; một nút "Lưu và trình duyệt" tạo luôn phương án.

**Why this priority**: Hiện mất khoảng 7 bước, xem trước chỉ là bảng, và phải chọn lại từng nhà ở bước trình duyệt.

**Independent Test**: Đánh số một hẻm 30 nhà và một nhóm điểm chưa có số chưa vẽ tuyến; đếm thao tác và so kết quả với app Bình Hưng.

**Acceptance Scenarios**:

1. **Given** một hẻm có điểm chưa có số nhưng chưa vẽ tuyến, **When** đánh số theo hẻm, **Then** trục hẻm được dựng từ chính các điểm và gợi ý số theo tiền tố hẻm.
2. **Given** xem trước, **When** chưa bấm lưu, **Then** không có gì được ghi.
3. **Given** số đã duyệt, đã trình hoặc giữ chỗ, **When** đánh số lại, **Then** không bị dời âm thầm; nếu không còn chỗ thì báo xung đột kèm cách xử lý.
4. **Given** bấm "Lưu và trình duyệt", **When** dữ liệu tuyến đã bị người khác đổi, **Then** không ghi gì và báo phải xem trước lại.

---

### User Story 5 - Duyệt số đề xuất (Priority: P2)

Người duyệt thấy danh sách nhà và số đề xuất cùng vị trí trên bản đồ, góp ý, duyệt hoặc trả lại; hỏi lại trước khi duyệt.

**Why this priority**: Đã có luồng trạng thái nhưng người duyệt phải tải Excel/PDF mới thấy nội dung.

**Independent Test**: Trình một phương án 20 nhà, người duyệt mở, xem trên bản đồ, trả lại 2 nhà, duyệt phần còn lại.

**Acceptance Scenarios**:

1. **Given** quy tắc chưa có văn bản căn cứ, **When** duyệt, **Then** vẫn duyệt được như app Bình Hưng nhưng phương án và mọi giấy tờ in ra ghi "quy tắc thử nghiệm" / "MẪU THỬ".
2. **Given** số trùng với số đã duyệt, **When** duyệt, **Then** bị chặn kèm nhà đang giữ số đó.

---

### User Story 6 - In phiếu kiểm tra, xuất lưới và gói bàn giao (Priority: P2)

Người dùng in phiếu kiểm tra theo mẫu xã cho nhà đang chọn hoặc hàng loạt theo tuyến; xuất lưới theo ấp, tuyến hoặc hẻm; tạo gói bàn giao.

**Why this priority**: Phiếu và lưới là sản phẩm nộp cho xã; plugin thiếu trường mẫu, thiếu lưới liền kề và không xuất theo phạm vi.

**Independent Test**: In phiếu cho hẻm 155 và so với mẫu xã (155/18, 20, 22 thửa 457, 24); xuất lưới một hẻm ra bốn định dạng và mở lại được.

**Acceptance Scenarios**:

1. **Given** một nhà, **When** in phiếu, **Then** phiếu có đủ: mã hồ sơ, ngày, chủ hồ sơ và CCCD, tờ/thửa, xã/tỉnh, bản đồ địa chính, ảnh nền kèm nguồn (hoặc ghi rõ không có), căn cứ, nhận xét, phương án, lưới số liền kề với bảng (thứ tự, cách đầu tuyến, bên, tờ, thửa, số, chủ theo chứng nhận, tình trạng), người lập, tọa độ VN-2000.
2. **Given** lưới số liền kề, **When** dựng, **Then** mỗi thửa giáp tuyến một ô, ô trống có số dự kiến từ đầu tuyến; tuyến ≤ 250 m lấy cả tuyến, dài hơn lấy đoạn 200 m.
3. **Given** chọn một ấp, tuyến hoặc hẻm, **When** xuất lưới, **Then** có PDF A3 tỷ lệ 1/500–1/1000 kèm bảng, DXF chia lớp, Excel, GeoJSON.
4. **Given** gói bàn giao, **When** tạo, **Then** "gói khai thác" không có tên/CCCD; "gói gốc" chỉ người có quyền tạo được và có nhật ký.

---

### User Story 7 - Khảo sát hiện trường trên điện thoại (Priority: P2)

Người khảo sát mở "Khu vực của tôi", chạm bản đồ để chọn nhà hoặc ghim nhà mới, nhập số nhà, chụp ảnh, lấy GPS; làm được khi mất sóng và tự gửi khi có sóng lại.

**Why this priority**: Phiếu khảo sát hiện tại không có ô số nhà, chọn nhà bằng danh sách cả dự án, không chụp ảnh khi mất mạng.

**Independent Test**: Trên điện thoại ở chế độ máy bay, mở lại ứng dụng, làm 50 phiếu kèm ảnh, bật mạng, kiểm 0 phiếu mất, 0 phiếu trùng.

**Acceptance Scenarios**:

1. **Given** mất sóng, **When** mở lại ứng dụng, **Then** vẫn vào được khu vực đã tải và làm phiếu.
2. **Given** chụp ảnh khi mất sóng, **When** có sóng lại, **Then** ảnh gửi kèm phiếu, không cần làm lại.
3. **Given** phiếu bị xung đột với dữ liệu mới hơn, **When** gửi, **Then** được báo và chọn cách xử lý, không mất phiếu.

---

### User Story 8 - Nạp dữ liệu thật Bình Hưng vào dự án riêng tư (Priority: P1 cho nghiệm thu)

Người quản trị nạp tuyến, nhà, thửa, nhãn bản vẽ, chứng nhận, chấm điểm và số cũ của Bình Hưng vào một dự án riêng tư để nghiệm thu với người dùng thật.

**Why this priority**: Mọi tiêu chí nghiệm thu ở trên đo bằng dữ liệu này.

**Independent Test**: So số đếm sau nạp với app Bình Hưng.

**Acceptance Scenarios**:

1. **Given** chữ trên bản vẽ dùng mã VNI hoặc TCVN3, **When** nạp, **Then** được đổi sang Unicode đúng dấu.
2. **Given** nguồn không rõ hệ tọa độ, **When** nạp, **Then** bị từ chối, không đoán.
3. **Given** dự án riêng tư, **When** người ngoài dự án gọi thẳng đường dẫn dữ liệu, **Then** bị từ chối.

### Edge Cases

- Một tuyến có cả kiểu số phụ chữ (100A) và kiểu gạch (100-1): đọc được cả hai; số mới theo kiểu của quy tắc.
- Chữ số phụ bỏ I, J, O, W, Z (dãy A B C D E F G H K L M N P Q R S T U V X Y).
- Số đặc biệt không phải số (ví dụ "Lô B2") được giữ nguyên, không bị bỏ.
- Hai số trống giữa hai nhà: chọn số trống nhỏ nhất (BR-02).
- Nhà nằm gần hai tuyến cách đều: báo mơ hồ, cho người dùng chọn tuyến.
- Tuyến nhiều đoạn cùng tên: gộp thành một tuyến khi đánh số và trên cây địa bàn.
- Phiếu kiểm tra khi không có ảnh nền: ghi rõ "không có ảnh", không tạo ảnh giả.
- Số đã duyệt bị đổi: giấy chứng nhận cũ bị đánh dấu cần cấp lại.
- Tắt plugin: dữ liệu số nhà giữ nguyên, bật lại thấy đủ.

## Requirements *(mandatory)*

### Functional Requirements

**Gợi ý và kiểm tra số**

- **FR-001**: Hệ thống MUST kiểm tra được bất kỳ điểm nào trên bản đồ mà không bắt người dùng chọn trước tuyến hay quy tắc; tự tìm thửa, ấp, tuyến gần nhất và bên đường.
- **FR-002**: Kết quả kiểm tra MUST hiện số hiện tại, số cũ, nhãn bản vẽ, số chấm điểm, nhà liền trước/sau cùng bên, số gợi ý và căn cứ.
- **FR-003**: Hệ thống MUST suy bên lẻ và chiều tăng từ các số đang có trên tuyến; cấu hình quy tắc chỉ dùng khi tuyến chưa đủ số.
- **FR-004**: Hệ thống MUST dùng nhãn số trên bản vẽ trong 35 m làm mốc như số của nhà đã liên kết.
- **FR-005**: Số sai chẵn/lẻ, không tăng dần hoặc không đọc được MUST bị bỏ qua có cảnh báo, không làm mất gợi ý của cả bên đường.
- **FR-006**: Số phụ MUST dùng dãy chữ bỏ I, J, O, W, Z; hệ thống MUST đọc được cả kiểu chữ và kiểu gạch trên cùng tuyến.
- **FR-007**: Tiền tố hẻm MUST nhận chữ tiếng Việt có dấu.
- **FR-008**: Số hiện tại MUST là số theo giấy chứng nhận; số đề xuất ở trường riêng trong mọi màn hình và tệp xuất.

**Biên tập trên bản đồ**

- **FR-010**: Người có quyền sửa MUST kéo được điểm số nhà tới vị trí mới và sửa số ngay trên bản đồ hoặc trong bảng thuộc tính.
- **FR-011**: Mọi lần kéo, sửa, giữ, loại, mở lại MUST có lịch sử (ai, khi nào, giá trị cũ) và hoàn tác được trong phiên.
- **FR-012**: Chọn trên bản đồ và trên bảng MUST đồng bộ hai chiều.
- **FR-013**: Lưu khi dữ liệu đã bị người khác đổi MUST bị từ chối kèm thông báo, không ghi đè.

**Rà soát**

- **FR-020**: Hệ thống MUST cung cấp lớp gộp "Số nhà hiện hữu" gồm số đã duyệt, số chứng nhận, nhãn bản vẽ và chứng nhận có vị trí ước tính, mỗi điểm ghi nguồn.
- **FR-021**: Hệ thống MUST nạp và đối soát lớp chấm điểm theo 6 trạng thái (khớp, lệch, sai số, không có trong lưới, chưa có số, chưa có vị trí) và đếm điểm chấm trùng, với nhãn đúng nghĩa như app Bình Hưng.
- **FR-022**: Ghép chứng nhận MUST thử số cũ trước, số mới sau, bỏ qua số cũ trùng; ghép theo tên đường qua nhiều đoạn.
- **FR-023**: Dòng chứng nhận không ghép được MUST có vị trí ước tính theo 4 cách của app Bình Hưng, gắn nhãn "ước tính".
- **FR-024**: Đối soát ba nguồn MUST dùng 7 nhóm BRD và tổng các nhóm MUST khớp tổng nhà.

**Đánh số theo tuyến và duyệt**

- **FR-030**: Người dùng MUST chọn được tuyến/hẻm trên bản đồ hoặc cây địa bàn rồi đánh số cả tuyến từ cùng khung đó.
- **FR-031**: Hệ thống MUST gợi ý số theo hẻm cho điểm chưa có số kể cả khi chưa vẽ tuyến, bằng cách dựng trục hẻm từ các điểm.
- **FR-032**: Xem trước MUST hiện trên bản đồ dạng nhãn cũ → mới, không ghi dữ liệu; áp dụng MUST ghi tất cả hoặc không ghi gì.
- **FR-033**: "Lưu và trình duyệt" MUST tạo phương án từ đúng các nhà trong bản xem trước, không chọn lại.
- **FR-034**: Người duyệt MUST xem được danh sách nhà, số và vị trí trên bản đồ, góp ý từng nhà, được hỏi lại trước khi duyệt.
- **FR-035**: Quy tắc chưa có văn bản căn cứ MUST vẫn duyệt được; phương án và giấy tờ MUST ghi "quy tắc thử nghiệm"/"MẪU THỬ".

**In, xuất, bàn giao**

- **FR-040**: Phiếu kiểm tra MUST theo mẫu xã với đủ các trường ở US6 kịch bản 1, in được cho nhà đang chọn và hàng loạt theo tuyến.
- **FR-041**: Phiếu MUST có lưới số liền kề theo quy tắc ở US6 kịch bản 2.
- **FR-042**: Xuất lưới MUST nhận phạm vi ấp, tuyến hoặc hẻm và ra PDF A3 (1/500–1/1000, có bảng), DXF chia lớp, Excel, GeoJSON.
- **FR-043**: Gói bàn giao MUST có gói khai thác (không tên, không CCCD) và gói gốc (chỉ người có quyền, có nhật ký).

**Khảo sát hiện trường**

- **FR-050**: Ứng dụng MUST mở được khi mất sóng sau lần tải trước và làm phiếu trong khu vực được giao.
- **FR-051**: Phiếu khảo sát MUST có ô số nhà, chọn nhà bằng chạm bản đồ, ghim nhà mới, chụp ảnh và GPS kể cả khi mất sóng.
- **FR-052**: Phiếu và ảnh MUST tự gửi khi có sóng, không mất, không trùng; xung đột được báo cho người khảo sát.

**Khung làm việc và địa bàn**

- **FR-060**: Mọi công cụ số nhà MUST mở trong khung bên cạnh bản đồ, không che bản đồ; đóng/mở lại không mất phần đang làm.
- **FR-061**: Điểm vào của plugin MUST gom lại còn bốn nhóm: Đánh số, Rà soát, Khảo sát, In/Xuất.
- **FR-062**: Giao diện MUST dùng tiếng Việt; không hiện mã nội bộ, UUID, JSON, mã hệ tọa độ hay trạng thái thô cho người dùng thường.
- **FR-063**: Cây địa bàn MUST tự dựng Xã → Đường → Hẻm → hẻm theo số và Xã → Ấp từ dữ liệu, gộp đoạn cùng tên, có số liệu từng nút.
- **FR-064**: Giao diện MUST dùng được trên điện thoại (nút đủ lớn, không cuộn ngang) và bằng bàn phím.

**Dữ liệu và quyền**

- **FR-070**: Hệ thống MUST nạp được tuyến, nhà, thửa, nhãn bản vẽ (đổi VNI/TCVN3 sang Unicode), chứng nhận, chấm điểm, số cũ; nguồn thiếu hệ tọa độ MUST bị từ chối.
- **FR-071**: Dự án dùng chế độ Bình Hưng MUST cho thành viên đã đăng nhập thấy tên chủ trong ứng dụng và trên nhãn bản đồ, ghi nhật ký mỗi lượt xem; CCCD và số điện thoại cần quyền riêng.
- **FR-072**: Tên, CCCD, số điện thoại MUST NOT xuất hiện ở bản đồ công khai, dịch vụ bản đồ cho bên ngoài, gói khai thác hay tệp xuất thường.
- **FR-073**: Mọi đọc, sửa, xuất MUST giới hạn trong tổ chức và dự án; người ngoài dự án gọi thẳng đường dẫn MUST bị từ chối.

### Key Entities

- **Điểm số nhà**: vị trí, số hiện tại (chứng nhận), số cũ, số đề xuất, nguồn, trạng thái giữ/loại, lịch sử sửa.
- **Tuyến / Hẻm**: tên, loại, tuyến cha, tiền tố hẻm, các đoạn cùng tên, chiều và bên lẻ suy ra.
- **Giấy chứng nhận**: số cũ, số mới, chủ, tờ/thửa, vị trí ghép hoặc ước tính kèm cách ước tính.
- **Nhãn bản vẽ**: chữ số đã đổi Unicode, vị trí, bản vẽ nguồn.
- **Điểm chấm**: số chấm, vị trí, trạng thái đối soát, cờ trùng.
- **Phương án**: các nhà và số đề xuất, quy tắc (thử nghiệm hay có căn cứ), góp ý, trạng thái, người duyệt.
- **Phiếu kiểm tra / Lưới liền kề**: nhà, các trường theo mẫu xã, lưới ô theo thửa giáp tuyến.
- **Phiếu khảo sát**: nhà, số, ảnh, GPS, người khảo sát, trạng thái gửi.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Trên 200 điểm Bình Hưng chọn ngẫu nhiên, số gợi ý trùng app Bình Hưng ở ít nhất 95% điểm; mọi điểm khác có lý do ghi rõ.
- **SC-002**: Số đếm theo 6 trạng thái chấm điểm và 7 nhóm đối soát khớp đúng app Bình Hưng trên cùng dữ liệu.
- **SC-003**: Nạp đủ 3.705 dòng chứng nhận và 4.375 điểm chấm; số dòng chứng nhận ghép được không thấp hơn app Bình Hưng (3.535), phần còn lại đều có vị trí ước tính.
- **SC-004**: Bấm một nhà thấy đủ các số và gợi ý trong vòng 2 giây; áp số gợi ý cho một nhà tối đa 2 thao tác.
- **SC-005**: Đánh số một hẻm 30 nhà và trình duyệt tối đa 5 thao tác, không rời bản đồ.
- **SC-006**: Phiếu kiểm tra hẻm 155 khớp mẫu xã: đủ 100% trường, lưới ra 155/18, 20, 22 (thửa 457), 24.
- **SC-007**: 50 phiếu khảo sát kèm ảnh làm khi mất sóng gửi đủ, 0 mất, 0 trùng; ứng dụng mở lại được khi mất sóng.
- **SC-008**: Kiểm tự động không tìm thấy tên, CCCD hay số điện thoại trong bản đồ công khai, dịch vụ bản đồ bên ngoài và gói khai thác; 100% lượt xem tên có nhật ký.
- **SC-009**: Không còn chữ tiếng Anh, UUID, JSON hay mã trạng thái thô trên các màn hình số nhà (kiểm tự động theo danh sách nhãn).
- **SC-010**: Thử với người dùng thật, mỗi nhóm ít nhất 2 người: ít nhất 80% hoàn thành nhiệm vụ chính của nhóm mình lần đầu mà không cần hướng dẫn.

## Assumptions

- App Bình Hưng tại main e9e4f0d là bản tham chiếu; nếu bản đang chạy khác thì lấy bản đang chạy và ghi lại.
- Kéo điểm và bảng thuộc tính dùng hợp đồng biên tập của Core (R006/GIS Studio); thiếu gì ở Core thì ghi thành điều kiện trước, không viết lại trong plugin.
- Văn bản quy định đánh số của xã chưa có; mọi quy tắc mặc định là "thử nghiệm" cho tới khi có.
- Mẫu phiếu kiểm tra lấy từ tệp mẫu của xã đã dùng ở Bình Hưng.
- Dữ liệu thật chỉ nằm trong dự án riêng tư; môi trường thử và kiểm tự động dùng dữ liệu giả, không chứa dữ liệu cá nhân thật.
- Không làm cổng tra cứu cho người dân, không kết nối hệ thống của thành phố, không làm DWG.
