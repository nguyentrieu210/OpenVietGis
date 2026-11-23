# Community Beta và chính sách nghiệm thu phát hành

**Hiện trạng 2026-11-23:** public repository, **không có LICENSE**, chưa có chứng nhận production/release. Chỉ công bố khả năng đã có bằng chứng theo đúng scope/version/profile.

## Quyền quyết định

Cộng đồng có thể thử nghiệm, báo lỗi, xác minh độc lập, đề xuất thay đổi và review Pull Request. Quyết định đưa lên `main`, tạo release, đổi cấu hình production, migrate database, cập nhật DNS, cấp quyền OAuth, hoặc hợp thức hóa số nhà thuộc chủ dự án/người có thẩm quyền. **Không dùng vote/star/fork làm tiêu chí chất lượng.**

## Điều kiện trước khi gắn nhãn `community-beta`

1. **Pháp lý:** Chủ sở hữu quyết định license được phép phân phối (MIT/Apache-2.0 hoặc lựa chọn khác với xác minh quyền tác giả); có file LICENSE phù hợp. Không giả định public = OSS.
2. **An toàn:** Không commit secret hay dữ liệu hộ gia đình; quét lịch sử Git, rà soát auth/tenant/RLS, vulnerability workflow và dữ liệu demo. Có cách báo cáo bảo mật riêng.
3. **Tái hiện:** README + bootstrap từ máy sạch; fixture synthetic có provenance; mọi artifact cùng một SHA/profile; CI pass trên candidate cuối.
4. **Chức năng tối thiểu:** CQ-00/01/02/03/04/05/09 và các nhóm mà beta tuyên bố hỗ trợ được kiểm thử độc lập. Module CAD, 3D, raster, AI chỉ quảng bá ở mức đã nghiệm thu.
5. **Vận hành:** Kịch bản cài/upgrade/restore trong môi trường cô lập, chữ ký/backup và rollback được chứng minh; bản live không bị ảnh hưởng.
6. **Đồng thuận:** Maintainer và reviewer ghi quyết định, các loại trừ, blocker còn mở, kết quả manual và release notes. Không tự đặt status `done` cho 656 yêu cầu vì các subtest chạy xanh.

## Cổng release dùng kết quả độc lập

| Gate | Mục đích | Trạng thái lúc thiết lập chương trình |
| --- | --- | --- |
| G-LICENSE | Có giấy phép và quyền sở hữu rõ | BLOCKED — chưa có LICENSE |
| G-SECURITY | Quét secret/history, tenant/RLS/consent | NOT_RUN trên bản beta cộng đồng |
| G-REPRO | Máy sạch clone/setup/build/test | NOT_RUN trên bản beta cộng đồng |
| G-MINIMUM | Kiểm thử CQ-P0 bằng 2 người và fixture | NOT_RUN |
| G-EXTENDED | CAD/3D/raster/plugin/AI theo tuyên bố | NOT_RUN |
| G-OPERATIONS | Backup/restore và rollback theo profile phát hành | NOT_RUN |
| G-REVIEW | Reviewer độc lập + chủ release ký trên SHA cuối | NOT_RUN |

Các gate này **không thay thế** G001–G010/R018 đang có và không giảm rào kiểm thử của repo. Nhóm nào chưa đạt phải giới hạn phạm vi quảng bá tương ứng; có lỗi bảo mật/mất dữ liệu hoặc thiếu quyền rõ ràng thì không publish production.

## Release record tối thiểu

- Version/tag; source SHA; build image digests; migration/schema version; danh sách tính năng bật và bị loại trừ.
- Build/CI URL; fixtures+checksum; bảng kết quả CQ; bằng chứng manual; review độc lập.
- Rủi ro P0/P1 còn mở, việc chấp nhận ngoại lệ có chữ ký người có quyền; backup/restore/rollback; release date.
- Người xác nhận và ngày; link hồ sơ phê duyệt ở Issue/PR.

## Nguyên tắc liên quan dự án Số nhà

Không thể suy kết quả chạy thuật toán từ fixture thành chứng thực hành chính. Xác nhận tuyến, điều kiện nhà đủ cấp số, quy tắc đánh số và quyết định chính thức phải do cơ quan/nhân sự có thẩm quyền xác nhận. Tài liệu xuất tự động là bản dự thảo chưa ký nếu chưa có phê duyệt.
