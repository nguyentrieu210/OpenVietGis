# Plan: Spec 055

- **Dự án / tổ chức**: dùng PATCH `status=deleted` có sẵn (R002); máy chủ kiểm `project.delete` / `organization.delete`. Hộp thoại trong `apps/web-next/src/LiveApp.tsx` (Tenant), mục menu trong `AccountMenu.tsx`. So khớp tên sau NFC.
- **Một đối tượng**: nút trong khung thông tin chạy lệnh ERASE đã qua lớp kiểm tra của `commands` (quyền phạm vi, chế độ đặc biệt, đang sửa).
- **Nhiều đối tượng**: lệnh DELSEL trong `ProfessionalTools.tsx`; một nguồn, tối đa 2.000 đối tượng, một changeset; giữ kế hoạch và mã thao tác tới khi thành công hoặc lỗi 4xx.
- **Không đổi backend, không migration.**
- **Kiểm thử**: tsc web-next, vite build, `tools/check_scope.py`, node tests; CI e2e.
