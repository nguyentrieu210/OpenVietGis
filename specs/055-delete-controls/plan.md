# Plan: Spec 055

- **Dự án / tổ chức**: dùng PATCH  có sẵn (R002); máy chủ kiểm  / . Hộp thoại trong  (Tenant), mục menu trong . So khớp tên sau NFC.
- **Một đối tượng**: nút trong khung thông tin chạy lệnh ERASE đã qua lớp kiểm tra của  (quyền phạm vi, chế độ đặc biệt, đang sửa).
- **Nhiều đối tượng**: lệnh DELSEL trong ; một nguồn, ≤ 2.000 đối tượng, một changeset; giữ kế hoạch + mã thao tác tới khi thành công hoặc lỗi 4xx.
- **Không đổi backend, không migration.**
- **Kiểm thử**: tsc web-next, vite build, check_scope, node tests; CI e2e.
