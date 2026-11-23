# OpenVietGIS 020 — Screen map & prototype

Status: V1 historical screen inventory; desktop Studio UX is superseded by [design-v2.md](design-v2.md) following owner feedback. V2 awaiting owner review. No implementation or deployment authorized by this document.

- Design file: https://www.figma.com/design/x2P6nknCfXyklDuiGEh83e
- [Screen map](https://www.figma.com/design/x2P6nknCfXyklDuiGEh83e?node-id=2-2482): 26 linked cards.
- 25 desktop frames (1440×900) and one field mobile frame (390×844); 190 valid navigation reactions.
- Six verified prototype starting points: Screen Map, Tài khoản và dự án, Quản lý team, GIS Studio, Plugin số nhà, Thực địa mobile.
- Editable native Figma layers, semantic color variables, reusable components and existing design-system input/navigation instances. Be Vietnam Pro primary typography; neutral light UI, teal action color; studio is a canvas-first professional workstation.
- Screenshots visually inspected: board, A01 login, S01 Studio. Structural inspection also covered P01 and F01. Detailed state/viewport visual audit remains open.

## Screen inventory

| ID | Screen | Figma |
|---|---|---|
| A01 | Đăng nhập | [2:235](https://www.figma.com/design/x2P6nknCfXyklDuiGEh83e?node-id=2-235) |
| A02 | Tạo tài khoản | [2:267](https://www.figma.com/design/x2P6nknCfXyklDuiGEh83e?node-id=2-267) |
| A03 | Xác minh email | [2:303](https://www.figma.com/design/x2P6nknCfXyklDuiGEh83e?node-id=2-303) |
| A04 | Quên mật khẩu | [2:323](https://www.figma.com/design/x2P6nknCfXyklDuiGEh83e?node-id=2-323) |
| A05 | Kiểm tra email | [2:347](https://www.figma.com/design/x2P6nknCfXyklDuiGEh83e?node-id=2-347) |
| A06 | Lời mời tham gia team | [2:364](https://www.figma.com/design/x2P6nknCfXyklDuiGEh83e?node-id=2-364) |
| P01 | Dự án của tôi | [2:383](https://www.figma.com/design/x2P6nknCfXyklDuiGEh83e?node-id=2-383) |
| P02 | Tạo dự án | [2:453](https://www.figma.com/design/x2P6nknCfXyklDuiGEh83e?node-id=2-453) |
| P03 | Tạo team | [2:515](https://www.figma.com/design/x2P6nknCfXyklDuiGEh83e?node-id=2-515) |
| T01 | Thành viên | [2:571](https://www.figma.com/design/x2P6nknCfXyklDuiGEh83e?node-id=2-571) |
| T02 | Mời thành viên | [2:638](https://www.figma.com/design/x2P6nknCfXyklDuiGEh83e?node-id=2-638) |
| T03 | Cài đặt team | [2:700](https://www.figma.com/design/x2P6nknCfXyklDuiGEh83e?node-id=2-700) |
| S01 | Studio bản đồ | [2:756](https://www.figma.com/design/x2P6nknCfXyklDuiGEh83e?node-id=2-756) |
| S02 | Biên tập & lưu | [2:924](https://www.figma.com/design/x2P6nknCfXyklDuiGEh83e?node-id=2-924) |
| S03 | CAD / COGO | [2:1089](https://www.figma.com/design/x2P6nknCfXyklDuiGEh83e?node-id=2-1089) |
| D01 | Danh mục dữ liệu | [2:1257](https://www.figma.com/design/x2P6nknCfXyklDuiGEh83e?node-id=2-1257) |
| D02 | Nhập dữ liệu | [2:1326](https://www.figma.com/design/x2P6nknCfXyklDuiGEh83e?node-id=2-1326) |
| D03 | Truy vấn & lọc | [2:1388](https://www.figma.com/design/x2P6nknCfXyklDuiGEh83e?node-id=2-1388) |
| D04 | Phân tích không gian | [2:1553](https://www.figma.com/design/x2P6nknCfXyklDuiGEh83e?node-id=2-1553) |
| D05 | Trình bày & in | [2:1718](https://www.figma.com/design/x2P6nknCfXyklDuiGEh83e?node-id=2-1718) |
| D06 | Nguồn dữ liệu bên thứ ba | [2:1883](https://www.figma.com/design/x2P6nknCfXyklDuiGEh83e?node-id=2-1883) |
| D07 | Cộng tác & lịch sử | [2:1945](https://www.figma.com/design/x2P6nknCfXyklDuiGEh83e?node-id=2-1945) |
| X01 | Plugin dự án | [2:2010](https://www.figma.com/design/x2P6nknCfXyklDuiGEh83e?node-id=2-2010) |
| X02 | Plugin số nhà | [2:2070](https://www.figma.com/design/x2P6nknCfXyklDuiGEh83e?node-id=2-2070) |
| X03 | Phương án số nhà | [2:2238](https://www.figma.com/design/x2P6nknCfXyklDuiGEh83e?node-id=2-2238) |
| F01 | Thực địa mobile | [2:2403](https://www.figma.com/design/x2P6nknCfXyklDuiGEh83e?node-id=2-2403) |

## Core journeys for review

- Account: A01 → A02 → A03; A01 → A04 → A05; invited recipient entry A06 → authenticated project/team context. Authentication, registration, reset, and verification submit to the identity-provider/server contract; no passwords or tokens stored in Figma.
- Team/project: P01 → P02; P03 team creation; T01 → T02 and T03. Invitation lifecycle is server-generated and server-authorized; Resend server adapter only, verified sender, expiration, recipient binding, no API key in UI or Figma.
- Studio: P01 → S01 → S02/S03, then data, query, cartography and plugin surfaces; map is synthetic vector demonstration, not actual GIS engine or real cadastral data.
- GIS: D01–D07 cover catalog/import/query/spatial processing/print/external sources/collaboration.
- Extension: X01 → X02 → X03, F01 is mobile field shell. Plugin-specific state, permission and job binding remains in its module contract.
- Every primary screen is directly accessible from the screen map. Active-navigation items pointing at their own screen deliberately need no transition.

## Design constraints to validate before coding

- Maintain coherent single top-level shell; avoid duplicate navigation/branding; ensure map canvas is dominant, panels collapsible/resizable, and precision controls keyboard accessible.
- Define and review state variants: loading/empty/error/retry, 401/403/expiry, verification/reset expired links, invitation pending/accepted/expired/revoked, role restrictions, unsaved edits, save conflict, jobs queued/running/failed, offline/reconnect, accessibility focus and reduced motion.
- Reconcile deep 001–019 workflows against coverage.md; in particular the full 019 house-numbering workflow, result validation, conflict/approval, audit and export. Add detailed frames for individual subflows if needed.
- Review tablet/mobile desktop variants, dark theme, contrast, responsive behavior and all task acceptance criteria before implementation.
- Prototype navigation is illustrative only; no live auth, email, real data, API, role enforcement, GIS edits or persisted state.
- T003: initial screen map/prototype and IDs delivered; comparative pattern review and full current-product audit remain outstanding. Do not mark T003 complete until those are evidenced.
- Owner approval is the implementation gate; no production frontend changes or rollout occurred during the design step.
