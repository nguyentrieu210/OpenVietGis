# Feature Specification: QGIS UX & GIS Functional Parity

**Feature Branch**: `065-qgis-ux-parity`
**Created**: 2026-11-27
**Status**: Draft — specification / source audit only
**Baseline**: `5bfc83826161acd83b98a925e85690cebde53ae1`
**Input**: Chủ dự án yêu cầu review UI/UX, lấy QGIS làm mẫu chuẩn, so sánh tính năng và lập Spec Kit. Kế thừa Spec 017, 037, 038, 056, 057, 062, 064; không tạo frontend mới.

## Mục tiêu
Chuẩn hóa trải nghiệm GIS desktop trên web theo QGIS Desktop 3.44; đạt tương đương **hành vi của các luồng được nghiệm thu**, không chỉ tương đồng bề ngoài. Giữ nền tảng đa người dùng và phân quyền, Rust Core, React/TypeScript, CAD WASM, Cesium/MapLibre, một frontend apps/web-next. Các chức năng hiện hữu và bản sửa lỗi #76/#77/#79/#80/#81 phải không hồi quy.

## User Scenarios & Testing
### US1 — Application Shell & điều hướng (Priority: P1)
**Why**: Người sử dụng phải vào đúng dự án, biết trạng thái và tìm lệnh nhanh.
**Independent test**: Thực hiện nhóm fixture SHELL trong [ma trận](feature-matrix.md), đối chiếu QGIS và OpenVietGIS theo baseline, ghi kết quả từng ID.
**Acceptance scenarios**:
- **Given** tài khoản có quyền và fixture tương ứng, **When** thực hiện Menu chức năng Project/Edit/View/Layer/Settings/Plugins/Vector/Raster/Processing/Help, **Then** Nhóm theo ngữ cảnh và capability; menu/toolbar/locator dùng cùng command ID (SHELL-01).
- **Given** tài khoản có quyền và fixture tương ứng, **When** thực hiện Toolbars theo nhóm, kéo thả, ẩn/hiện, **Then** Kiểm tra thứ tự, vị trí, reset, narrow screen và phím tắt (SHELL-02).
- **Given** tài khoản có quyền và fixture tương ứng, **When** thực hiện Locator / Ctrl+K tìm lệnh, dữ liệu, tọa độ, **Then** Locator đa nguồn có scope, tìm tên lệnh VN/EN, layer, tọa độ; không vượt quyền (SHELL-03).
- **Given** tài khoản có quyền và fixture tương ứng, **When** thực hiện Status bar tọa độ, CRS, tỷ lệ, render, **Then** Hiển thị CRS rõ ràng, tỷ lệ, đơn vị, trạng thái tải, tiến độ và lưu (SHELL-04).
- **Given** tài khoản có quyền và fixture tương ứng, **When** thực hiện Profile giao diện / cấu hình phím tắt, **Then** Profile theo tài khoản, cấu hình keybinding có xung đột, reset; giữ A theo lựa chọn (SHELL-05).
- **Given** tài khoản có quyền và fixture tương ứng, **When** thực hiện Map-only / fullscreen, đóng/mở panel, **Then** Phục hồi chính xác bố cục, không làm mất lựa chọn hoặc bản nháp (SHELL-06).
- **Given** tài khoản có quyền và fixture tương ứng, **When** thực hiện Mở lại project/mode/camera/tool sau F5, **Then** Phục hồi theo tài khoản/tổ chức/dự án, quyền hiện tại, không hiện GuestStudio (SHELL-07).
- **Given** tài khoản có quyền và fixture tương ứng, **When** thực hiện Empty/loading/error/session expired, **Then** Thông báo có hành động, retry có giới hạn, không hiển thị lỗi kỹ thuật mặc định (SHELL-08).

### US2 — Dock, Browser & Layers (Priority: P1)
**Why**: Tổ chức dữ liệu và diện tích canvas là thao tác hằng ngày.
**Independent test**: Thực hiện nhóm fixture DOCK trong [ma trận](feature-matrix.md), đối chiếu QGIS và OpenVietGIS theo baseline, ghi kết quả từng ID.
**Acceptance scenarios**:
- **Given** tài khoản có quyền và fixture tương ứng, **When** thực hiện Dock trái/phải/dưới/nổi, resize, **Then** Kéo thả mọi panel theo capability; resize chuột và bàn phím, giới hạn viewport (DOCK-01).
- **Given** tài khoản có quyền và fixture tương ứng, **When** thực hiện Tab dock, lưu/đổi/reset layout, **Then** Tab stack, layout preset và keyboard alternatives; không rơi panel ngoài màn (DOCK-02).
- **Given** tài khoản có quyền và fixture tương ứng, **When** thực hiện Browser tree nguồn dữ liệu, metadata, drag/drop, **Then** Browser nguồn server, kéo layer vào bản đồ, xem metadata và lỗi provider (DOCK-03).
- **Given** tài khoản có quyền và fixture tương ứng, **When** thực hiện Data Source Manager, **Then** Luồng chọn nguồn, CRS, preview, import, tiến độ, kết quả; kiểm thử từng provider (DOCK-04).
- **Given** tài khoản có quyền và fixture tương ứng, **When** thực hiện Nhóm lớp lồng nhau, tri-state, kéo thả, **Then** Kiểm tra nhóm lồng, kéo nhiều lớp, đổi thứ tự và lưu dự án chung (DOCK-05).
- **Given** tài khoản có quyền và fixture tương ứng, **When** thực hiện Chọn nhiều layer Ctrl/Shift, menu ngữ cảnh, **Then** Chọn bằng bàn phím, thao tác batch, menu lớp/nhóm đúng capability (DOCK-06).
- **Given** tài khoản có quyền và fixture tương ứng, **When** thực hiện Legend cây lớp, lọc lớp, scale range, **Then** Legend đúng style; trạng thái ngoài tỷ lệ, lọc chỉ đổi view (DOCK-07).
- **Given** tài khoản có quyền và fixture tương ứng, **When** thực hiện Map themes / nhóm hiển thị độc quyền, **Then** Lưu, đổi, xóa theme; xác định cấu hình cá nhân/chia sẻ rõ ràng (DOCK-08).
- **Given** tài khoản có quyền và fixture tương ứng, **When** thực hiện Layer order riêng / map view nhiều khung, **Then** Đặc tả render order độc lập; thêm secondary view chỉ khi adapter hỗ trợ (DOCK-09).
- **Given** tài khoản có quyền và fixture tương ứng, **When** thực hiện Remove khỏi map vs xóa layer/source, **Then** Phân biệt hành động và hậu quả; chặn xóa trái quyền, có xác nhận (DOCK-10).

### US3 — Layer Properties, Symbology, Labels (Priority: P1)
**Why**: Hiển thị dữ liệu đúng ý nghĩa nghiệp vụ.
**Independent test**: Thực hiện nhóm fixture STYLE trong [ma trận](feature-matrix.md), đối chiếu QGIS và OpenVietGIS theo baseline, ghi kết quả từng ID.
**Acceptance scenarios**:
- **Given** tài khoản có quyền và fixture tương ứng, **When** thực hiện Information / Source / Fields / Display, **Then** Thông tin nguồn/storage/display CRS, fields, rendering; Apply/OK/Cancel nhất quán (STYLE-01).
- **Given** tài khoản có quyền và fixture tương ứng, **When** thực hiện Single symbol / categorized / graduated, **Then** So khớp fixture phân loại null, breaks, màu, legend, export (STYLE-02).
- **Given** tài khoản có quyền và fixture tương ứng, **When** thực hiện Rule-based / symbol layers / data-defined overrides, **Then** Cấu trúc rule, nhiều symbol layer, kiểm tra biểu thức và render có bằng chứng (STYLE-03).
- **Given** tài khoản có quyền và fixture tương ứng, **When** thực hiện Labels theo trường/biểu thức, vị trí, buffer, **Then** Preview, placement theo geometry, ưu tiên/collision, font/halo; giới hạn rõ (STYLE-04).
- **Given** tài khoản có quyền và fixture tương ứng, **When** thực hiện Expression Builder dùng chung, **Then** AST typed, autocomplete field/function, preview, null, báo lỗi; không eval/SQL tùy ý (STYLE-05).
- **Given** tài khoản có quyền và fixture tương ứng, **When** thực hiện Styles QML/SLD, import/export, style presets, **Then** Round-trip tập con được hỗ trợ; cảnh báo thuộc tính không tương thích (STYLE-06).
- **Given** tài khoản có quyền và fixture tương ứng, **When** thực hiện Joins / relations / constraints / form layout, **Then** Join có cardinality, relation widget, constraint phía server, form thiết kế có scope (STYLE-07).
- **Given** tài khoản có quyền và fixture tương ứng, **When** thực hiện Metadata, diagrams, actions, map tips, **Then** Tách tab theo capability, không dựng nút giả; action sandbox allowlist (STYLE-08).
- **Given** tài khoản có quyền và fixture tương ứng, **When** thực hiện Layer styling panel live preview, **Then** Panel styling preview và apply rõ ràng; cancel không ghi đè dữ liệu (STYLE-09).

### US4 — Attribute Table, Form & Selection (Priority: P1)
**Why**: Dữ liệu hình học và thuộc tính phải đồng bộ.
**Independent test**: Thực hiện nhóm fixture TABLE trong [ma trận](feature-matrix.md), đối chiếu QGIS và OpenVietGIS theo baseline, ghi kết quả từng ID.
**Acceptance scenarios**:
- **Given** tài khoản có quyền và fixture tương ứng, **When** thực hiện Table gắn với active layer, map/table selection, **Then** Một selection store theo layer/revision; chọn ở map/table/inspector khớp (TABLE-01).
- **Given** tài khoản có quyền và fixture tương ứng, **When** thực hiện Tìm kiếm, phân trang, sort/filter toàn bộ nguồn, **Then** Server-side filter/sort trên toàn nguồn; ghi rõ loaded vs total; không lọc riêng trang gây hiểu nhầm (TABLE-02).
- **Given** tài khoản có quyền và fixture tương ứng, **When** thực hiện Chọn theo biểu thức, vị trí, vùng chọn, **Then** Select/add/remove/intersect/invert trên toàn tập hợp có permission và count (TABLE-03).
- **Given** tài khoản có quyền và fixture tương ứng, **When** thực hiện Form view / editor widget theo kiểu trường, **Then** Table/form toggle, typed date/enum/relation/null, validation field-level (TABLE-04).
- **Given** tài khoản có quyền và fixture tương ứng, **When** thực hiện Inline cell edit, multi-edit, save/rollback, **Then** Chỉnh sửa dòng/cột trong transaction ngắn, conflict, undo có receipt (TABLE-05).
- **Given** tài khoản có quyền và fixture tương ứng, **When** thực hiện Field Calculator / add field / virtual field, **Then** Preview công thức, phạm vi selected/all, nullable/kiểu dữ liệu, atomic update (TABLE-06).
- **Given** tài khoản có quyền và fixture tương ứng, **When** thực hiện Copy/paste, export selected, cấu hình cột, **Then** Copy dữ liệu đúng phạm vi; cột/width/order cá nhân, export selected có cảnh báo (TABLE-07).
- **Given** tài khoản có quyền và fixture tương ứng, **When** thực hiện Identify Results nhiều layer và statistics, **Then** Identify theo layer/mode và tổng hợp toàn nguồn, không đồng nhất mẫu với toàn bộ (TABLE-08).

### US5 — Digitizing, Snapping & Topology (Priority: P1)
**Why**: Chỉnh sửa phải chính xác và không mất dữ liệu.
**Independent test**: Thực hiện nhóm fixture EDIT trong [ma trận](feature-matrix.md), đối chiếu QGIS và OpenVietGIS theo baseline, ghi kết quả từng ID.
**Acceptance scenarios**:
- **Given** tài khoản có quyền và fixture tương ứng, **When** thực hiện Pan, zoom, identify, extent, Space-pan tạm, **Then** Không đổi công cụ đang chọn khi nhả Space; không capture khi input/IME (EDIT-01).
- **Given** tài khoản có quyền và fixture tương ứng, **When** thực hiện Point/Line/Polygon digitize, vertex editor, **Then** Create/move/insert/delete vertex, precision input, preview, save/cancel có test (EDIT-02).
- **Given** tài khoản có quyền và fixture tương ứng, **When** thực hiện Edit mode & unsaved changes, save/rollback, **Then** Layer edit state rõ; bản nháp khác shared revision; cảnh báo chuyển project/F5 (EDIT-03).
- **Given** tài khoản có quyền và fixture tương ứng, **When** thực hiện Snapping vertex/segment, layer-specific tolerance, **Then** Chọn active/all/advanced theo layer, pixel/map unit, chỉ báo snap; fixture CRS (EDIT-04).
- **Given** tài khoản có quyền và fixture tương ứng, **When** thực hiện Topology checks, avoid overlap, shared boundary, **Then** Kiểm tra invalid/dangling/overlap/gap; không auto-fix nếu chưa duyệt (EDIT-05).
- **Given** tài khoản có quyền và fixture tương ứng, **When** thực hiện Split/merge, reshape, multipart, ring, **Then** Hỗ trợ phần còn thiếu theo geometry capability; rollback atomic, giữ ID chính sách (EDIT-06).
- **Given** tài khoản có quyền và fixture tương ứng, **When** thực hiện Advanced digitizing constraint distance/angle/ortho, **Then** Khóa góc/khoảng cách/trục; đơn vị CRS rõ, không đổi tọa độ âm thầm (EDIT-07).
- **Given** tài khoản có quyền và fixture tương ứng, **When** thực hiện Undo/redo, edit history, concurrent conflict, **Then** Phân biệt draft undo và persisted revert; revision pin, 2 user conflict (EDIT-08).
- **Given** tài khoản có quyền và fixture tương ứng, **When** thực hiện Measure distance/area/bearing, **Then** Đơn vị và phương pháp trắc địa/planar rõ; đối chiếu mẫu QGIS (EDIT-09).

### US6 — Processing, Raster & Analysis (Priority: P2)
**Why**: Phân tích phải tái lập được.
**Independent test**: Thực hiện nhóm fixture PROC trong [ma trận](feature-matrix.md), đối chiếu QGIS và OpenVietGIS theo baseline, ghi kết quả từng ID.
**Acceptance scenarios**:
- **Given** tài khoản có quyền và fixture tương ứng, **When** thực hiện Processing Toolbox search, category/provider, **Then** Registry thuật toán theo capability, tìm/lọc, help, favorite/recent; không hiện thuật toán giả (PROC-01).
- **Given** tài khoản có quyền và fixture tương ứng, **When** thực hiện Algorithm parameter form / preview, **Then** Typed inputs, CRS, extent, output, validation, chạy lại cùng tham số (PROC-02).
- **Given** tài khoản có quyền và fixture tương ứng, **When** thực hiện Buffer/Clip/Dissolve, spatial joins & analysis, **Then** Golden fixtures với QGIS; mở rộng intersect/union/difference/reproject theo coverage (PROC-03).
- **Given** tài khoản có quyền và fixture tương ứng, **When** thực hiện Job progress, history, cancel, retry, **Then** Durable job id, trạng thái queued/running/succeeded/failed/canceled, retry idempotent (PROC-04).
- **Given** tài khoản có quyền và fixture tương ứng, **When** thực hiện Batch processing, **Then** Bảng input/output nhiều lần chạy, preview, giới hạn, lỗi từng hàng, export history (PROC-05).
- **Given** tài khoản có quyền và fixture tương ứng, **When** thực hiện Model Designer, **Then** Graph DAG, parameter mapping, named outputs, revision, cancel/partial results (PROC-06).
- **Given** tài khoản có quyền và fixture tương ứng, **When** thực hiện Raster properties, band, nodata, rendering, **Then** Band metadata, stretch, palette, opacity, nodata, CRS; capability từng provider (PROC-07).
- **Given** tài khoản có quyền và fixture tương ứng, **When** thực hiện Raster Calculator, NDVI/NDWI, DEM terrain, **Then** Kết quả theo fixture, nodata/CRS/extent, lưu output có lineage (PROC-08).
- **Given** tài khoản có quyền và fixture tương ứng, **When** thực hiện Georeferencer raster/vector GCP, **Then** GCP raster/vector, transform, residual, preview, xuất raster đã georef (PROC-09).
- **Given** tài khoản có quyền và fixture tương ứng, **When** thực hiện Temporal controller / mesh / point clouds, **Then** Khảo sát adapter độc lập; chưa hỗ trợ thì ghi rõ, không tạo lớp giả (PROC-10).

### US7 — Print Layout, CRS & interoperability (Priority: P2)
**Why**: Xuất bản đồ đúng tỷ lệ/CRS và trao đổi dữ liệu an toàn.
**Independent test**: Thực hiện nhóm fixture OUTPUT trong [ma trận](feature-matrix.md), đối chiếu QGIS và OpenVietGIS theo baseline, ghi kết quả từng ID.
**Acceptance scenarios**:
- **Given** tài khoản có quyền và fixture tương ứng, **When** thực hiện Layout manager, page, map frames, **Then** Danh sách layout, khổ giấy, trang, map frame, CRS, scale; lưu phiên bản (OUTPUT-01).
- **Given** tài khoản có quyền và fixture tương ứng, **When** thực hiện Item properties, guides/grid, alignment, undo, **Then** Dock item properties, align/distribute, guides, snap, lock, undo/redo; golden layout (OUTPUT-02).
- **Given** tài khoản có quyền và fixture tương ứng, **When** thực hiện Legend, north arrow, scale bar, label, **Then** Liên kết đúng map item, scale và legend; không sai đơn vị/CRS (OUTPUT-03).
- **Given** tài khoản có quyền và fixture tương ứng, **When** thực hiện Atlas / report nhiều trang, **Then** Coverage layer, lọc, trang theo đối tượng, batch export; không rò dữ liệu (OUTPUT-04).
- **Given** tài khoản có quyền và fixture tương ứng, **When** thực hiện PDF/PNG/SVG export, print preview, **Then** Kiểm tra PDF/PNG, font VN, DPI, clipping; SVG chỉ khi engine hỗ trợ (OUTPUT-05).
- **Given** tài khoản có quyền và fixture tương ứng, **When** thực hiện Project/source/display CRS, reprojection, VN-2000, **Then** Phân biệt display/transform, xác nhận hệ VN-2000 theo tỉnh/kinh tuyến, control points (OUTPUT-06).
- **Given** tài khoản có quyền và fixture tương ứng, **When** thực hiện Import/export GeoJSON/GPKG/SHP/CSV/DXF/raster, **Then** Kiểm tra từng format thực, CRS, schema/null/encoding, geometry, round-trip; không đoán parity (OUTPUT-07).
- **Given** tài khoản có quyền và fixture tương ứng, **When** thực hiện Đồng bộ QGIS desktop & style/project exchange, **Then** Sync hai chiều, revision, conflict; QGZ/QGS/QML subset rõ ràng; plugin Python QGIS ngoài scope (OUTPUT-08).

### US8 — CAD/3D, collaboration, AI & cross-cutting (Priority: P2)
**Why**: Không mất ngữ cảnh khi chuyển engine, cộng tác hoặc dùng AI.
**Independent test**: Thực hiện nhóm fixture COLLAB trong [ma trận](feature-matrix.md), đối chiếu QGIS và OpenVietGIS theo baseline, ghi kết quả từng ID.
**Acceptance scenarios**:
- **Given** tài khoản có quyền và fixture tương ứng, **When** thực hiện 3D map view, camera, layers, terrain, **Then** Giữ Cesium/MapLibre; camera/selection sync theo capability; không thay bằng QGIS engine (COLLAB-01).
- **Given** tài khoản có quyền và fixture tương ứng, **When** thực hiện CAD Model/Layout, save, document revision, **Then** Giữ OpenCAD engine; lưu đúng bản vẽ/phiên bản, dirty gate, không lệnh CAD giả (COLLAB-02).
- **Given** tài khoản có quyền và fixture tương ứng, **When** thực hiện Application Shell chung CAD/GIS/3D, **Then** Chat/task/AI luôn khả dụng theo quyền, chỉ toolbar chuyên ngành đổi; không remount thừa (COLLAB-03).
- **Given** tài khoản có quyền và fixture tương ứng, **When** thực hiện AI multi-workspace, trạng thái lệnh, consent, **Then** Rõ dự án/mode/doc/revision, history, hủy, status/expiry; không replay lệnh sau reload (COLLAB-04).
- **Given** tài khoản có quyền và fixture tương ứng, **When** thực hiện Chat và giao việc xuyên engine, **Then** Một nguồn dữ liệu, không mất tin nhắn/draft khi chuyển mode; chống trùng subscription (COLLAB-05).
- **Given** tài khoản có quyền và fixture tương ứng, **When** thực hiện Plugin lifecycle, menu & tool registry, **Then** Plugin dùng shell/dock/registry chính, capability, không SPA thứ hai; uninstall an toàn (COLLAB-06).
- **Given** tài khoản có quyền và fixture tương ứng, **When** thực hiện Accessibility keyboard/focus/contrast, **Then** WCAG 2.2 AA mục tiêu cho controls, tab order, keyboard alternatives cho drag, focus restore (COLLAB-07).
- **Given** tài khoản có quyền và fixture tương ứng, **When** thực hiện Responsive & hiệu suất bản đồ, **Then** Đo desktop 1280/1440/1920, 390px field workflow; baseline p95, map không trắng khi resize (COLLAB-08).
- **Given** tài khoản có quyền và fixture tương ứng, **When** thực hiện Offline chính thức vs Guest GIS, **Then** Giữ offline có scope/identity; tuyệt đối không tái tạo GuestStudio hoặc tự mở GIS cục bộ (COLLAB-09).

## Functional requirements
- FR-001: Chỉ một authenticated shell và một canonical web build; không tái tạo GIS cục bộ, GuestStudio, router/SPA thứ hai; offline được cấp quyền theo Spec 062 vẫn hoạt động.
- FR-002: Command registry chung cho menu, ribbon, toolbar, context menu, shortcut, locator; mỗi lệnh có ID ổn định, label VN/EN, engine, capability, disabled reason, quyền và test. Backend giữ quyền quyết định.
- FR-003: Thiết lập bố cục cá nhân theo account/project/mode, version và migrate dữ liệu cũ; cấu hình map dùng chung có revision và save rõ ràng; không lưu private data/credential trong preference.
- FR-004: Đảm bảo focus, keyboard, IME tiếng Việt, drag alternative, tooltips, role/state, contrast, Escape và focus restore; không chặn input của browser/CAD/3D.
- FR-005: Lớp dữ liệu có identity/source/storage/display CRS và capability; mọi thao tác mang scope và pin revision hợp lệ; không suy luận CRS âm thầm.
- FR-006: Attribute filter/sort/count/selection cho toàn nguồn dùng server-side hợp lệ; UI phân biệt trang đã tải và toàn tập; không thay đổi số liệu hiển thị thành số liệu tổng.
- FR-007: Draft, preview, persisted changeset và revert khác nhau; unsaved/save/error/conflict/retry/recovery rõ ràng, không tự ghi đè hoặc replay.
- FR-008: Processing catalog chỉ công bố thuật toán chạy được; output có lineage, job status và cancel/retry theo capability; golden fixtures dùng QGIS chuẩn.
- FR-009: Chat/task/AI dùng chung application shell; GIS/CAD/3D chỉ đổi toolset chuyên ngành; AI xác định project/mode/session/document/revision, consent phù hợp, không ghi lệnh nhầm tab.
- FR-010: Không thêm Python server cho capability Rust đã thay thế; tái sử dụng API/Rust contracts hiện tại, thay đổi API chỉ qua ADR + schema + migration/test.
- FR-011: Đạt từng dòng matrix trước khi đánh dấu parity; ghi rõ U/defer/blocker, không dùng nút giả.
- FR-012: Không sao chép logo thương hiệu QGIS hoặc asset không rõ license; tham khảo hành vi, tên thuật ngữ và bố cục.
- FR-013: Chuyển đổi từng wave dưới feature flag/rollout có kiểm soát, build một lần từ GitHub, kiểm tra DEV trước khi production; không reset DB, không thay đổi dữ liệu production để test.

## Key Entities
UserProfile, WorkspaceScope, DockLayout, CommandDescriptor, Capability, MapConfiguration, LayerRevision, DraftChange, JobReceipt, EvidenceRecord. Chi tiết data-model.md; không tự tạo bảng DB nếu chỉ cần preference client.

## Success criteria (targets, chưa đo)
- SC-001: 100% dòng P1 được nghiệm thu V hoặc có quyết định defer được chủ dự án chấp thuận; không gọi parity nếu P1 còn blocker. Mọi dòng P2/P3 có test hoặc quyết định phạm vi.
- SC-002: 10 lần F5, 20 lần chuyển GIS/CAD/3D liên tiếp trên fixture giữ đúng dự án, công cụ, camera/bản vẽ và draft theo quy tắc; không false dirty, không render GuestStudio.
- SC-003: Đo task-time và click count cùng kịch bản QGIS/OVG (mở layer, style, select, edit, export); mục tiêu median OVG ≤1.2 lần QGIS với cùng dữ liệu và người dùng thử; tác vụ online ghi riêng network latency; không đạt thì có blocker, không nới ngưỡng âm thầm.
- SC-004: 1280×720, 1440×900, 1920×1080, 390×844; không tràn trang desktop, panel resize/keyboard và mobile field path dùng được, không che lệnh lưu/hủy. WCAG 2.2 AA mục tiêu cho UI điều khiển; kiểm thử tự động và thủ công.
- SC-005: p95 phản hồi mở menu/toggle nội bộ ≤100 ms trên profile định nghĩa ở plan; thao tác map/feature phải đo baseline cùng fixture; không hồi quy p95 >10% sau mỗi wave (có phê duyệt ngoại lệ).
- SC-006: Không lỗi cross-tenant, silent CRS mismatch, duplicate mutation, mất draft hoặc migration mất dữ liệu. Các test negative thực API/PostGIS phải pass.
- SC-007: Build/lint/test frontend, Rust contract, browser E2E và visual comparison theo wave pass; ghi commit/artifact ID; rollback frontend an toàn; zero claim live nếu chưa smoke test production.

## Non-goals
Không clone QGIS nguyên xi, không tái viết renderer, không triển khai guest local GIS, không xóa QGIS desktop Python plugin đang hỗ trợ, không tuyên bố tương đương mọi provider/plugin/thuật toán. Không triển khai code ở lượt lập spec này.
