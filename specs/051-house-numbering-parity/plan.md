# Plan: Spec 051 — đợt 1 trạng thái đối chiếu (FR-025…028)

Phạm vi đợt 1 chỉ gồm trạng thái đối chiếu theo điểm, tiến độ và ký khóa theo ấp/đường. Các câu chuyện khác của spec làm ở các đợt sau.

## Thiết kế

- **Dữ liệu điểm**: lớp Core "Khảo sát số nhà …" (trường `ap`, `ten_duong`, `trang_thai`, `xu_ly`, `so_nha`, `so_sua`). Nút trên điểm gửi changeset `update` của Core với toàn bộ thuộc tính hiện có cộng phần đổi; Core lo lịch sử, hoàn tác, xung đột phiên bản (FR-011, FR-013).
- **Ký và khóa**: bảng `house_numbering.survey_signoffs` (migration `hn_0011`), RLS theo tổ chức/dự án, chỉ một chữ ký hiệu lực mỗi phạm vi. Ký/mở khóa cần `project.manage` (giống duyệt phương án), có khóa tư vấn theo nguồn để hai người duyệt không cùng ký vượt kiểm tra.
- **Khóa ở CSDL**: trigger `SECURITY DEFINER` trên `spatial.reference_features` chặn UPDATE thật sự đổi `attributes`/`geometry` và DELETE của điểm thuộc phạm vi đã ký; không chặn việc Core nâng `content_revision` cho cả nguồn. Lỗi mang SQLSTATE `OV409`; Core có bộ xử lý chung trả 409 `DATA_LOCKED` kèm câu của trigger.
- **API module**: `GET/POST …/survey-signoffs`, `POST …/survey-signoffs/{id}/revoke`.
- **Giao diện**: khung Địa bàn hiện tiến độ ấp/đường và nút ký/mở khóa; khung điểm hiện các số, trạng thái và nút đối chiếu. Nhận lớp khảo sát gộp "Khảo sát số nhà Bình Hưng" và ranh "Ấp hiện hành"/"Ranh xã, phường".

## Kiểm thử

- `modules/house_numbering/tests/integration/test_survey_signoff.py`: ký sớm bị từ chối, ký đúng, khóa sửa/xóa, sửa ngoài phạm vi vẫn được, mở khóa, ký cả ấp, người ngoài dự án bị chặn.
- `modules/house_numbering/frontend/progress.test.ts`: quy tắc cần đối chiếu, tiến độ, khớp chữ ký ấp/đường.
