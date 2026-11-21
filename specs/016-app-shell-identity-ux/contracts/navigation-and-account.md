# UI Contract: Navigation & Account

## Primary authenticated navigation

The persistent shell provides these conceptual destinations when applicable:

- Tổng quan
- Dự án
- Dữ liệu GIS
- Bản đồ
- Công cụ
- Thành viên & phân quyền

Availability depends on current organization/project context and accepted capabilities. The shell never labels entries with roadmap IDs.

## Top bar

- Current human-readable breadcrumb/context
- Optional command/search entry
- Account menu

## Account menu

Must expose:

- Trang làm việc
- Tài khoản & phiên đăng nhập
- Lời mời
- Đăng xuất

Sign-out uses the existing application logout contract, not a visual-only state change.

## Technical metadata

UUIDs/revisions/diagnostics are excluded from primary navigation. A later/current feature may expose them through an explicit technical details surface.
