# Feature Specification: OpenVietGIS — viết mới toàn bộ frontend

**Feature Branch**: `spec/020-frontend-rewrite` (đích triển khai)
**Created**: 2026-11-22
**Status**: Draft — owner review; chưa triển khai
**Input**: Viết mới hoàn toàn UI/UX frontend OpenVietGIS; giữ nghiệp vụ, phân quyền, dữ liệu đã có; đăng ký, đăng nhập, mời team qua email Resend; map-first GIS/CAD; sử dụng combo công cụ thiết kế và kiểm thử.

## Phạm vi và vấn đề
- Frontend hiện có nhiều lớp điều hướng, login trung gian, dashboard quảng cáo, menu đổi theo URL, công cụ trùng lặp và thư mời dạng JSON.
- Viết lại presentation, routing, navigation, visual system, form/table, desktop workstation, field shell và UI tích hợp plugin. Không sao chép CSS/layout cũ.
- Không viết lại backend GIS, dữ liệu PostGIS, engine, API/SDK ổn định, session/auth, domain rules. Chỉ bổ sung hợp đồng onboarding/email thiếu và được review; giữ tương thích.
- 020 là bản thay thế UI của 016/017, không hủy các acceptance nghiệp vụ R001–R015, audit 018, plugin 019. Mỗi năng lực cần mapping và evidence; không đánh dấu hoàn thành chỉ vì có nút.

## User Scenarios & Testing
### US1 — Đăng nhập, đăng ký và vào app (P1)
Người dùng mới tạo hoặc xác minh tài khoản, người dùng cũ đăng nhập một lần, quay về đúng nơi.
**Independent test:** tài khoản mới, cũ, phiên hết hạn, deep link.
1. Given chưa đăng nhập, when mở app, then có một trải nghiệm đăng nhập rõ thương hiệu, không có trang chọn provider thừa khi chỉ có một provider.
2. Given chưa có tài khoản, when đăng ký và xác minh email, then có thể đăng nhập; chưa xác minh không được quyền nhạy cảm.
3. Given đăng nhập thành công, then quay lại deep link hợp lệ hoặc trang dự án, không rơi vào landing lặp.
4. Given quên mật khẩu, then có luồng khôi phục, thông báo trung tính không lộ tài khoản.
5. Given 401/logout, then dữ liệu bảo vệ biến mất, session cũ không dùng lại; lỗi mạng không giả vờ logout.
### US2 — Dự án và team (P1)
Người dùng tạo/chọn tổ chức, workspace, project; quản trị mời người theo email và quyền.
**Independent test:** tạo team, mời user mới và cũ, chấp nhận, thu hồi.
1. Given không có team, then onboarding rõ ràng: tạo tổ chức hoặc nhận lời mời; không tự cấp quyền ngoài phạm vi.
2. Given có một dự án khả dụng, then mở dự án gần nhất bằng một thao tác; nhiều dự án có danh sách tìm/lọc.
3. Given quyền mời, when nhập email/role, then gửi lời mời có link an toàn, trạng thái gửi/chờ/chấp nhận/hết hạn được phân biệt; không yêu cầu copy JSON.
4. Given người nhận mở link, then đăng ký/đăng nhập nếu cần, chấp nhận đúng email đã xác minh và đúng team, chỉ cấp role đã mời.
5. Given hết hạn, đã dùng, bị thu hồi hoặc sai email, then không cấp quyền; gửi lại do quản trị, không cấp quyền kép.
### US3 — GIS/CAD workstation (P1)
Người dùng mở project và thao tác bản đồ ngay, không đi qua nhiều dashboard.
**Independent test:** add layer → chọn → sửa → lưu → reload.
1. Given project có dữ liệu, then canvas là trung tâm, layer tree trái, inspector phải, attribute table dưới có thể đóng/mở/kéo.
2. Given lệnh qua menu/ribbon/shortcut/command palette, then cùng hành vi/capability; không mất trạng thái khi đổi tool.
3. Given lựa chọn trên map/table, then đồng bộ selection; pan/zoom không tự ghi dữ liệu.
4. Given sửa geometry/map config, then phân biệt nháp/lưu thật, quyền edit, lỗi revision và undo/redo theo hợp đồng.
5. Given dùng CAD, thì tọa độ, snap, COGO, DXF theo đúng capability đã chứng minh; không hứa DWG nếu chưa hỗ trợ.
### US4 — Dữ liệu và công cụ chuyên môn (P1)
Nhập/xuất, CRS, catalog, query, processing, cartography, WMS/COG, lịch sử, collaboration vận hành trong cùng context project.
**Independent test:** nhập dữ liệu có CRS → add layer → query → xử lý → xuất và kiểm tra kết quả.
1. Công cụ hiện theo quyền và khả năng thật; unsupported được giải thích.
2. Job có progress, cancel/retry nếu hỗ trợ, kết quả và lỗi; không tự đặt thành công.
3. Import/export có preview/validation, CRS rõ ràng, thông báo kết quả và truy xuất được.
4. Duyệt/change review, history và conflict giữ audit và scope.
### US5 — Plugin và field (P2)
Plugin độc lập, được bật theo project; màn thực địa tách shell, có offline.
**Independent test:** bật Số nhà → thao tác thật → tắt; khảo sát offline → sync.
1. Plugin chưa bật không hiện tool; bật chỉ qua manifest/slot/permission, không hard-code nghiệp vụ vào Core.
2. Plugin Số nhà giữ toàn bộ luồng theo 019; mở bằng panel/route phù hợp, không biến GIS thành ứng dụng số nhà.
3. Field mobile xem bản đồ, lớp, ghi nhận theo capability; mất mạng giữ nháp và trạng thái sync xác thực.
### US6 — UX đồng nhất, dễ sử dụng (P1)
Người dùng đọc tên và hành động là hiểu, không thấy thuật ngữ nội bộ vô ích.
**Independent test:** review các màn chính ở desktop/tablet/mobile và keyboard.
1. Một hệ token, typography, icon, spacing, form/table, focus; light mặc định, dark tùy chọn.
2. Không hero quảng cáo trong app, không card lồng/card thừa, không UUID/raw error nổi ở luồng chính.
3. Project switcher hiển thị tên; navigation quản trị và workstation tách biệt.
4. Loading/empty/forbidden/error/success/unsaved có trạng thái rõ, responsive không overflow ngang ngoài bảng/canvas có chủ đích.
5. Mọi thao tác có nhãn, keyboard focus, contrast; shortcut không cướp input.
### US7 — Chuyển đổi an toàn (P1)
Người dùng giữ mọi dữ liệu, quyền và chức năng sau khi thay frontend.
**Independent test:** chạy song song legacy/v2 cùng dữ liệu test và đối chiếu.
1. Deep link cũ vẫn đến đúng tài nguyên hoặc báo quyền rõ.
2. Không thất lạc dữ liệu, tenant isolation, CSRF, revision, module isolation.
3. Chỉ cutover sau nghiệm thu browser thật; rollback được bằng artifact frontend trước đó.

## Requirements
### Functional Requirements
- **FR-001** Một entry đăng nhập thương hiệu OpenVietGIS; tự chuyển tới provider nếu duy nhất, giữ safe deep link.
- **FR-002** Đăng ký, xác minh email, đăng nhập, quên mật khẩu, logout và quản lý phiên theo identity provider được phép.
- **FR-003** Khi chỉ có một project, cho mở ngay; nhiều project chọn/tìm/chuyển; lưu project gần nhất theo user không chứa dữ liệu bảo vệ.
- **FR-004** Tổ chức/workspace/project và membership quản lý tập trung, không ép người dùng đi qua cả ba cấp mỗi lần.
- **FR-005** Mời team theo email và role hợp lệ, server gửi mail; nhận link có hạn, xác minh đúng người nhận, revoke/resend, audit.
- **FR-006** Không hiển thị khóa mail, token provider hoặc bí mật invitation trong UI/log; lỗi gửi được phân biệt với quyền membership.
- **FR-007** Ba shell độc lập: public/auth, project hub/admin, workstation/field phù hợp context; không lồng sidebar quản trị trong map.
- **FR-008** Map-first docked layout, resize/collapse, toolbar/menu/context/command thống nhất.
- **FR-009** Tree layer, map, inspector và attribute table dùng cùng context/selection.
- **FR-010** Mọi thay đổi vật chất phải dùng contract hiện có; hiển thị permission, capability, revision, save và conflict.
- **FR-011** Bảo toàn luồng catalog, CRS, import/export, query, editing, processing, cartography, CAD, 3rd-party, collaboration và offline theo specs 001–015.
- **FR-012** Plugin qua module registry, slot, quyền, không import trực tiếp module domain vào Core.
- **FR-013** Thiết kế tiếng Việt mặc định, nhất quán light/dark, responsive desktop/tablet/mobile, trợ năng.
- **FR-014** UI lỗi/empty/loading/denied/saved có hành động recovery; dữ liệu không được trình bày như đã tải khi chưa có evidence.
- **FR-015** Bảo toàn safe redirects, server authorization, CSRF, cache isolation khi đổi account/tenant.
- **FR-016** Bắt buộc kiểm thử browser thực, ảnh visual, trace API/capability và bảng coverage yêu cầu; không có mock-only release.
- **FR-017** Rollout song song stage, không phá bản đang chạy; cutover/canary/rollback rõ.
- **FR-018** Design system và pattern research có version, nguồn tham chiếu và điều kiện nghiệm thu; không dịch nguyên mẫu SaaS sang GIS.
- **FR-019** Studio duy trì một map instance và một project context: mở công cụ bằng modal/drawer/panel/popover/dock, không chuyển trang, remount bản đồ hoặc reset viewport/selection/nháp ngoài thao tác chủ động. Auth, đổi project/tenant và quản trị nằm ngoài Studio.
- **FR-020** Chọn trực tiếp từng điểm/đường/vùng trên canvas; kéo điểm hoặc chỉnh X/Y theo CRS rõ ràng, sửa vertex, snap và xem trước; đồng bộ inspector, bảng thuộc tính và selection. Hủy/Esc, undo/redo và save/revision phải có hành vi rõ.
- **FR-021** Thêm dữ liệu, tạo lớp, tạo đối tượng mới bằng modal/draw tool ngay tại map; kiểm tra CRS/schema/quyền, trạng thái tiến trình/lỗi; thành công thì gắn layer/feature lên map đang mở, không mất trạng thái khác.
- **FR-022** Query, CAD/COGO, phân tích, in/xuất và plugin chạy qua command registry và surfaces trong map; kết quả preview/highlight/layer, không điều hướng trang độc lập cho mỗi công cụ. Xem design-v2.md.
- **FR-023** E2E kiểm tra route và identity của map instance không đổi khi mở/đóng công cụ; xác minh hình học lưu thật, rollback khi cancel, 403/409/CRS lỗi và bảo toàn viewport/layer/selection.
### Key Entities
Account, Team/Organization, Workspace, Project, Invitation, MailDelivery, WorkspaceSession, LayerSelection, Command, ModuleManifest, UI preference.
### Edge Cases
Session hết hạn giữa sửa, đổi tenant/account, lỗi provider/email, verify/replay link, project bị archive, plugin bị khóa, mất mạng, CRS sai, save conflict, loading một phần, màn 375px, direct legacy URL, asset/cache trộn version.

## Success Criteria
- **SC-001** Người đã có phiên mở project gần nhất ≤2 hành động từ entry; đăng nhập chỉ một màn nhập thông tin xác thực.
- **SC-002** 100% kịch bản đăng ký, reset, mời, nhận, revoke, hết hạn và sai email vượt qua test; không rò quyền.
- **SC-003** Canvas chiếm tối thiểu 60% diện tích workstation khi mở dock chuẩn ở 1440×900; đóng dock tăng diện tích.
- **SC-004** 100% yêu cầu đã nghiệm thu 001–019 có mapping giữ luồng/coverage hoặc gap có chủ xử lý; không còn blocker trước cutover.
- **SC-005** 100% workflow chính có ảnh desktop và browser evidence; không overflow layout tại 1440, 1280, 768, 390px ngoài vùng cuộn chủ đích.
- **SC-006** 0 lỗi console nghiêm trọng, cross-tenant, P0/P1, token lộ; đạt keyboard/contrast cho thao tác chính.
- **SC-007** Khi fixture và mạng chuẩn đã tải, p95 tương tác shell ≤100ms; không tăng >15% thời gian mở project/bundle mà không có rationale; ghi baseline trước.
- **SC-008** Cutover và rollback đã diễn tập trên staging, không đổi dữ liệu GIS hoặc schema ngoài phạm vi onboarding đã review.

## Assumptions
Giữ OIDC/Keycloak làm reference identity; team là cách gọi UX cho organization hiện có. Resend là nhà cung cấp mail giao dịch phía server. Chưa gửi email thật, chưa triển khai, chưa chốt domain sender. Scope desktop chuyên nghiệp; mobile field không hứa full CAD parity. Mọi thiếu hụt backend phải có contract change và security review.
