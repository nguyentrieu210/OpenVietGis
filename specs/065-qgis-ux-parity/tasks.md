# Tasks: QGIS UX & GIS Functional Parity

**Input**: [spec](spec.md), [plan](plan.md), [matrix](feature-matrix.md), [research](research.md), [data model](data-model.md), [contracts](contracts/ux-contracts.md), [quickstart](quickstart.md).
**Status**: All tasks open; creation of documentation is not implementation. Each row must be audited first; if already passes, add evidence instead of rewriting. If capability unavailable, implement only reviewed scope or document approved defer. [P] only when files and contracts are independent.

## Phase 0 — baseline and foundation
- [ ] T001 Inventory current command registry and reachable route/capability IDs; map each matrix row to source and existing tests (apps/web-next/src, apps/api-rust/src).
- [ ] T002 Capture QGIS 3.44 and OVG authenticated baseline screenshots/flows using synthetic fixtures; save evidence and blockers.
- [ ] T003 Create reproducible vector/raster/CAD/CRS and two-user test fixtures under fixtures/qgis-parity; no production data.
- [ ] T004 Define benchmark profile, screenshot resolutions, task-time and click-count measurement; record baseline before optimization.
- [ ] T005 Audit legacy UI storage keys and shared map revision contract; map #76/#77/#79/#80/#81 regression tests.
- [ ] T006 Audit existing Design System/token and accessibility baseline; establish icon/typography/spacing tokens and contrast rules without visual rewrite.
- [ ] T007 Consolidate command descriptor contract and capability checks into existing CoreCommand/workbench registry; route menu/toolbar/context/shortcut to same handler.
- [ ] T008 Create CI acceptance evidence manifest and gate script; map every matrix row to test IDs and blockers.

## Wave 1 — US1: Application Shell & điều hướng
**Source boundary**: apps/web-next/src/LiveApp.tsx, SpatialWorkspace.tsx, QgisWorkspace.tsx, CoreCommandBar.tsx, hotkeys.ts.
- [ ] T009 [US1] [SHELL-01] Menu chức năng Project/Edit/View/Layer/Settings/Plugins/Vector/Raster/Processing/Help: xác minh và hoàn thiện Nhóm theo ngữ cảnh và capability; menu/toolbar/locator dùng cùng command ID; test/evidence theo contracts/ux-contracts.md (P1).
- [ ] T010 [US1] [SHELL-02] Toolbars theo nhóm, kéo thả, ẩn/hiện: xác minh và hoàn thiện Kiểm tra thứ tự, vị trí, reset, narrow screen và phím tắt; test/evidence theo contracts/ux-contracts.md (P1).
- [ ] T011 [US1] [SHELL-03] Locator / Ctrl+K tìm lệnh, dữ liệu, tọa độ: xác minh và hoàn thiện Locator đa nguồn có scope, tìm tên lệnh VN/EN, layer, tọa độ; không vượt quyền; test/evidence theo contracts/ux-contracts.md (P1).
- [ ] T012 [US1] [SHELL-04] Status bar tọa độ, CRS, tỷ lệ, render: xác minh và hoàn thiện Hiển thị CRS rõ ràng, tỷ lệ, đơn vị, trạng thái tải, tiến độ và lưu; test/evidence theo contracts/ux-contracts.md (P1).
- [ ] T013 [US1] [SHELL-05] Profile giao diện / cấu hình phím tắt: xác minh và hoàn thiện Profile theo tài khoản, cấu hình keybinding có xung đột, reset; giữ A theo lựa chọn; test/evidence theo contracts/ux-contracts.md (P1).
- [ ] T014 [US1] [SHELL-06] Map-only / fullscreen, đóng/mở panel: xác minh và hoàn thiện Phục hồi chính xác bố cục, không làm mất lựa chọn hoặc bản nháp; test/evidence theo contracts/ux-contracts.md (P1).
- [ ] T015 [US1] [SHELL-07] Mở lại project/mode/camera/tool sau F5: xác minh và hoàn thiện Phục hồi theo tài khoản/tổ chức/dự án, quyền hiện tại, không hiện GuestStudio; test/evidence theo contracts/ux-contracts.md (P1).
- [ ] T016 [US1] [SHELL-08] Empty/loading/error/session expired: xác minh và hoàn thiện Thông báo có hành động, retry có giới hạn, không hiển thị lỗi kỹ thuật mặc định; test/evidence theo contracts/ux-contracts.md (P1).
- [ ] T017 [US1] Add/update targeted browser tests in tests/e2e/qgis-parity-shell.spec.ts; run existing regression suite and store evidence for this wave.
- [ ] T018 [US1] Review wave changes against Constitution, capability/permission and performance gate; update matrix and deliver independently reviewable PR.

## Wave 2 — US2: Dock, Browser & Layers
**Source boundary**: apps/web-next/src/DockLayout.tsx, Splitter.tsx, QgisWorkspace.tsx, layer-tree.ts, CatalogManager.tsx, DsmPanes.tsx.
- [ ] T019 [US2] [DOCK-01] Dock trái/phải/dưới/nổi, resize: xác minh và hoàn thiện Kéo thả mọi panel theo capability; resize chuột và bàn phím, giới hạn viewport; test/evidence theo contracts/ux-contracts.md (P1).
- [ ] T020 [US2] [DOCK-02] Tab dock, lưu/đổi/reset layout: xác minh và hoàn thiện Tab stack, layout preset và keyboard alternatives; không rơi panel ngoài màn; test/evidence theo contracts/ux-contracts.md (P1).
- [ ] T021 [US2] [DOCK-03] Browser tree nguồn dữ liệu, metadata, drag/drop: xác minh và hoàn thiện Browser nguồn server, kéo layer vào bản đồ, xem metadata và lỗi provider; test/evidence theo contracts/ux-contracts.md (P1).
- [ ] T022 [US2] [DOCK-04] Data Source Manager: xác minh và hoàn thiện Luồng chọn nguồn, CRS, preview, import, tiến độ, kết quả; kiểm thử từng provider; test/evidence theo contracts/ux-contracts.md (P1).
- [ ] T023 [US2] [DOCK-05] Nhóm lớp lồng nhau, tri-state, kéo thả: xác minh và hoàn thiện Kiểm tra nhóm lồng, kéo nhiều lớp, đổi thứ tự và lưu dự án chung; test/evidence theo contracts/ux-contracts.md (P1).
- [ ] T024 [US2] [DOCK-06] Chọn nhiều layer Ctrl/Shift, menu ngữ cảnh: xác minh và hoàn thiện Chọn bằng bàn phím, thao tác batch, menu lớp/nhóm đúng capability; test/evidence theo contracts/ux-contracts.md (P1).
- [ ] T025 [US2] [DOCK-07] Legend cây lớp, lọc lớp, scale range: xác minh và hoàn thiện Legend đúng style; trạng thái ngoài tỷ lệ, lọc chỉ đổi view; test/evidence theo contracts/ux-contracts.md (P1).
- [ ] T026 [US2] [DOCK-08] Map themes / nhóm hiển thị độc quyền: xác minh và hoàn thiện Lưu, đổi, xóa theme; xác định cấu hình cá nhân/chia sẻ rõ ràng; test/evidence theo contracts/ux-contracts.md (P2).
- [ ] T027 [US2] [DOCK-09] Layer order riêng / map view nhiều khung: xác minh và hoàn thiện Đặc tả render order độc lập; thêm secondary view chỉ khi adapter hỗ trợ; test/evidence theo contracts/ux-contracts.md (P2).
- [ ] T028 [US2] [DOCK-10] Remove khỏi map vs xóa layer/source: xác minh và hoàn thiện Phân biệt hành động và hậu quả; chặn xóa trái quyền, có xác nhận; test/evidence theo contracts/ux-contracts.md (P1).
- [ ] T029 [US2] Add/update targeted browser tests in tests/e2e/qgis-parity-dock.spec.ts; run existing regression suite and store evidence for this wave.
- [ ] T030 [US2] Review wave changes against Constitution, capability/permission and performance gate; update matrix and deliver independently reviewable PR.

## Wave 3 — US3: Layer Properties, Symbology, Labels
**Source boundary**: apps/web-next/src/LayerProperties.tsx, style-io.ts, label-expr.ts, graduated.ts, ProfessionalTools.tsx.
- [ ] T031 [US3] [STYLE-01] Information / Source / Fields / Display: xác minh và hoàn thiện Thông tin nguồn/storage/display CRS, fields, rendering; Apply/OK/Cancel nhất quán; test/evidence theo contracts/ux-contracts.md (P1).
- [ ] T032 [US3] [STYLE-02] Single symbol / categorized / graduated: xác minh và hoàn thiện So khớp fixture phân loại null, breaks, màu, legend, export; test/evidence theo contracts/ux-contracts.md (P1).
- [ ] T033 [US3] [STYLE-03] Rule-based / symbol layers / data-defined overrides: xác minh và hoàn thiện Cấu trúc rule, nhiều symbol layer, kiểm tra biểu thức và render có bằng chứng; test/evidence theo contracts/ux-contracts.md (P2).
- [ ] T034 [US3] [STYLE-04] Labels theo trường/biểu thức, vị trí, buffer: xác minh và hoàn thiện Preview, placement theo geometry, ưu tiên/collision, font/halo; giới hạn rõ; test/evidence theo contracts/ux-contracts.md (P1).
- [ ] T035 [US3] [STYLE-05] Expression Builder dùng chung: xác minh và hoàn thiện AST typed, autocomplete field/function, preview, null, báo lỗi; không eval/SQL tùy ý; test/evidence theo contracts/ux-contracts.md (P1).
- [ ] T036 [US3] [STYLE-06] Styles QML/SLD, import/export, style presets: xác minh và hoàn thiện Round-trip tập con được hỗ trợ; cảnh báo thuộc tính không tương thích; test/evidence theo contracts/ux-contracts.md (P2).
- [ ] T037 [US3] [STYLE-07] Joins / relations / constraints / form layout: xác minh và hoàn thiện Join có cardinality, relation widget, constraint phía server, form thiết kế có scope; test/evidence theo contracts/ux-contracts.md (P2).
- [ ] T038 [US3] [STYLE-08] Metadata, diagrams, actions, map tips: xác minh và hoàn thiện Tách tab theo capability, không dựng nút giả; action sandbox allowlist; test/evidence theo contracts/ux-contracts.md (P2).
- [ ] T039 [US3] [STYLE-09] Layer styling panel live preview: xác minh và hoàn thiện Panel styling preview và apply rõ ràng; cancel không ghi đè dữ liệu; test/evidence theo contracts/ux-contracts.md (P2).
- [ ] T040 [US3] Add/update targeted browser tests in tests/e2e/qgis-parity-style.spec.ts; run existing regression suite and store evidence for this wave.
- [ ] T041 [US3] Review wave changes against Constitution, capability/permission and performance gate; update matrix and deliver independently reviewable PR.

## Wave 4 — US4: Attribute Table, Form & Selection
**Source boundary**: apps/web-next/src/QgisWorkspace.tsx, FeatureInfo.tsx, ProfessionalTools.tsx, AdvancedEditor.tsx.
- [ ] T042 [US4] [TABLE-01] Table gắn với active layer, map/table selection: xác minh và hoàn thiện Một selection store theo layer/revision; chọn ở map/table/inspector khớp; test/evidence theo contracts/ux-contracts.md (P1).
- [ ] T043 [US4] [TABLE-02] Tìm kiếm, phân trang, sort/filter toàn bộ nguồn: xác minh và hoàn thiện Server-side filter/sort trên toàn nguồn; ghi rõ loaded vs total; không lọc riêng trang gây hiểu nhầm; test/evidence theo contracts/ux-contracts.md (P1).
- [ ] T044 [US4] [TABLE-03] Chọn theo biểu thức, vị trí, vùng chọn: xác minh và hoàn thiện Select/add/remove/intersect/invert trên toàn tập hợp có permission và count; test/evidence theo contracts/ux-contracts.md (P1).
- [ ] T045 [US4] [TABLE-04] Form view / editor widget theo kiểu trường: xác minh và hoàn thiện Table/form toggle, typed date/enum/relation/null, validation field-level; test/evidence theo contracts/ux-contracts.md (P1).
- [ ] T046 [US4] [TABLE-05] Inline cell edit, multi-edit, save/rollback: xác minh và hoàn thiện Chỉnh sửa dòng/cột trong transaction ngắn, conflict, undo có receipt; test/evidence theo contracts/ux-contracts.md (P1).
- [ ] T047 [US4] [TABLE-06] Field Calculator / add field / virtual field: xác minh và hoàn thiện Preview công thức, phạm vi selected/all, nullable/kiểu dữ liệu, atomic update; test/evidence theo contracts/ux-contracts.md (P1).
- [ ] T048 [US4] [TABLE-07] Copy/paste, export selected, cấu hình cột: xác minh và hoàn thiện Copy dữ liệu đúng phạm vi; cột/width/order cá nhân, export selected có cảnh báo; test/evidence theo contracts/ux-contracts.md (P2).
- [ ] T049 [US4] [TABLE-08] Identify Results nhiều layer và statistics: xác minh và hoàn thiện Identify theo layer/mode và tổng hợp toàn nguồn, không đồng nhất mẫu với toàn bộ; test/evidence theo contracts/ux-contracts.md (P2).
- [ ] T050 [US4] Add/update targeted browser tests in tests/e2e/qgis-parity-table.spec.ts; run existing regression suite and store evidence for this wave.
- [ ] T051 [US4] Review wave changes against Constitution, capability/permission and performance gate; update matrix and deliver independently reviewable PR.

## Wave 5 — US5: Digitizing, Snapping & Topology
**Source boundary**: apps/web-next/src/CoreMapTools.tsx, AdvancedEditor.tsx, SnapMath.ts, ProfessionalTools.tsx; apps/api-rust/src/editing.rs.
- [ ] T052 [US5] [EDIT-01] Pan, zoom, identify, extent, Space-pan tạm: xác minh và hoàn thiện Không đổi công cụ đang chọn khi nhả Space; không capture khi input/IME; test/evidence theo contracts/ux-contracts.md (P1).
- [ ] T053 [US5] [EDIT-02] Point/Line/Polygon digitize, vertex editor: xác minh và hoàn thiện Create/move/insert/delete vertex, precision input, preview, save/cancel có test; test/evidence theo contracts/ux-contracts.md (P1).
- [ ] T054 [US5] [EDIT-03] Edit mode & unsaved changes, save/rollback: xác minh và hoàn thiện Layer edit state rõ; bản nháp khác shared revision; cảnh báo chuyển project/F5; test/evidence theo contracts/ux-contracts.md (P1).
- [ ] T055 [US5] [EDIT-04] Snapping vertex/segment, layer-specific tolerance: xác minh và hoàn thiện Chọn active/all/advanced theo layer, pixel/map unit, chỉ báo snap; fixture CRS; test/evidence theo contracts/ux-contracts.md (P1).
- [ ] T056 [US5] [EDIT-05] Topology checks, avoid overlap, shared boundary: xác minh và hoàn thiện Kiểm tra invalid/dangling/overlap/gap; không auto-fix nếu chưa duyệt; test/evidence theo contracts/ux-contracts.md (P1).
- [ ] T057 [US5] [EDIT-06] Split/merge, reshape, multipart, ring: xác minh và hoàn thiện Hỗ trợ phần còn thiếu theo geometry capability; rollback atomic, giữ ID chính sách; test/evidence theo contracts/ux-contracts.md (P2).
- [ ] T058 [US5] [EDIT-07] Advanced digitizing constraint distance/angle/ortho: xác minh và hoàn thiện Khóa góc/khoảng cách/trục; đơn vị CRS rõ, không đổi tọa độ âm thầm; test/evidence theo contracts/ux-contracts.md (P2).
- [ ] T059 [US5] [EDIT-08] Undo/redo, edit history, concurrent conflict: xác minh và hoàn thiện Phân biệt draft undo và persisted revert; revision pin, 2 user conflict; test/evidence theo contracts/ux-contracts.md (P1).
- [ ] T060 [US5] [EDIT-09] Measure distance/area/bearing: xác minh và hoàn thiện Đơn vị và phương pháp trắc địa/planar rõ; đối chiếu mẫu QGIS; test/evidence theo contracts/ux-contracts.md (P1).
- [ ] T061 [US5] Add/update targeted browser tests in tests/e2e/qgis-parity-edit.spec.ts; run existing regression suite and store evidence for this wave.
- [ ] T062 [US5] Review wave changes against Constitution, capability/permission and performance gate; update matrix and deliver independently reviewable PR.

## Wave 6 — US6: Processing, Raster & Analysis
**Source boundary**: apps/web-next/src/ProcessingToolbox.tsx, ProcessingModeler.tsx, RasterWorkbench.tsx, RasterReportWorkbench.tsx, TerrainEngineering.tsx; apps/api-rust/src/processing.rs.
- [ ] T063 [US6] [PROC-01] Processing Toolbox search, category/provider: xác minh và hoàn thiện Registry thuật toán theo capability, tìm/lọc, help, favorite/recent; không hiện thuật toán giả; test/evidence theo contracts/ux-contracts.md (P1).
- [ ] T064 [US6] [PROC-02] Algorithm parameter form / preview: xác minh và hoàn thiện Typed inputs, CRS, extent, output, validation, chạy lại cùng tham số; test/evidence theo contracts/ux-contracts.md (P1).
- [ ] T065 [US6] [PROC-03] Buffer/Clip/Dissolve, spatial joins & analysis: xác minh và hoàn thiện Golden fixtures với QGIS; mở rộng intersect/union/difference/reproject theo coverage; test/evidence theo contracts/ux-contracts.md (P1).
- [ ] T066 [US6] [PROC-04] Job progress, history, cancel, retry: xác minh và hoàn thiện Durable job id, trạng thái queued/running/succeeded/failed/canceled, retry idempotent; test/evidence theo contracts/ux-contracts.md (P1).
- [ ] T067 [US6] [PROC-05] Batch processing: xác minh và hoàn thiện Bảng input/output nhiều lần chạy, preview, giới hạn, lỗi từng hàng, export history; test/evidence theo contracts/ux-contracts.md (P2).
- [ ] T068 [US6] [PROC-06] Model Designer: xác minh và hoàn thiện Graph DAG, parameter mapping, named outputs, revision, cancel/partial results; test/evidence theo contracts/ux-contracts.md (P2).
- [ ] T069 [US6] [PROC-07] Raster properties, band, nodata, rendering: xác minh và hoàn thiện Band metadata, stretch, palette, opacity, nodata, CRS; capability từng provider; test/evidence theo contracts/ux-contracts.md (P2).
- [ ] T070 [US6] [PROC-08] Raster Calculator, NDVI/NDWI, DEM terrain: xác minh và hoàn thiện Kết quả theo fixture, nodata/CRS/extent, lưu output có lineage; test/evidence theo contracts/ux-contracts.md (P2).
- [ ] T071 [US6] [PROC-09] Georeferencer raster/vector GCP: xác minh và hoàn thiện GCP raster/vector, transform, residual, preview, xuất raster đã georef; test/evidence theo contracts/ux-contracts.md (P2).
- [ ] T072 [US6] [PROC-10] Temporal controller / mesh / point clouds: xác minh và hoàn thiện Khảo sát adapter độc lập; chưa hỗ trợ thì ghi rõ, không tạo lớp giả; test/evidence theo contracts/ux-contracts.md (P3).
- [ ] T073 [US6] Add/update targeted browser tests in tests/e2e/qgis-parity-proc.spec.ts; run existing regression suite and store evidence for this wave.
- [ ] T074 [US6] Review wave changes against Constitution, capability/permission and performance gate; update matrix and deliver independently reviewable PR.

## Wave 7 — US7: Print Layout, CRS & interoperability
**Source boundary**: apps/web-next/src/LayoutDesigner.tsx, PrintDesigner.tsx, LayoutRender.ts, ExportDialog.tsx, TransferTool.tsx, QgisBindingManager.tsx, crs-display.ts.
- [ ] T075 [US7] [OUTPUT-01] Layout manager, page, map frames: xác minh và hoàn thiện Danh sách layout, khổ giấy, trang, map frame, CRS, scale; lưu phiên bản; test/evidence theo contracts/ux-contracts.md (P1).
- [ ] T076 [US7] [OUTPUT-02] Item properties, guides/grid, alignment, undo: xác minh và hoàn thiện Dock item properties, align/distribute, guides, snap, lock, undo/redo; golden layout; test/evidence theo contracts/ux-contracts.md (P2).
- [ ] T077 [US7] [OUTPUT-03] Legend, north arrow, scale bar, label: xác minh và hoàn thiện Liên kết đúng map item, scale và legend; không sai đơn vị/CRS; test/evidence theo contracts/ux-contracts.md (P1).
- [ ] T078 [US7] [OUTPUT-04] Atlas / report nhiều trang: xác minh và hoàn thiện Coverage layer, lọc, trang theo đối tượng, batch export; không rò dữ liệu; test/evidence theo contracts/ux-contracts.md (P3).
- [ ] T079 [US7] [OUTPUT-05] PDF/PNG/SVG export, print preview: xác minh và hoàn thiện Kiểm tra PDF/PNG, font VN, DPI, clipping; SVG chỉ khi engine hỗ trợ; test/evidence theo contracts/ux-contracts.md (P1).
- [ ] T080 [US7] [OUTPUT-06] Project/source/display CRS, reprojection, VN-2000: xác minh và hoàn thiện Phân biệt display/transform, xác nhận hệ VN-2000 theo tỉnh/kinh tuyến, control points; test/evidence theo contracts/ux-contracts.md (P1).
- [ ] T081 [US7] [OUTPUT-07] Import/export GeoJSON/GPKG/SHP/CSV/DXF/raster: xác minh và hoàn thiện Kiểm tra từng format thực, CRS, schema/null/encoding, geometry, round-trip; không đoán parity; test/evidence theo contracts/ux-contracts.md (P1).
- [ ] T082 [US7] [OUTPUT-08] Đồng bộ QGIS desktop & style/project exchange: xác minh và hoàn thiện Sync hai chiều, revision, conflict; QGZ/QGS/QML subset rõ ràng; plugin Python QGIS ngoài scope; test/evidence theo contracts/ux-contracts.md (P2).
- [ ] T083 [US7] Add/update targeted browser tests in tests/e2e/qgis-parity-output.spec.ts; run existing regression suite and store evidence for this wave.
- [ ] T084 [US7] Review wave changes against Constitution, capability/permission and performance gate; update matrix and deliver independently reviewable PR.

## Wave 8 — US8: CAD/3D, collaboration, AI & cross-cutting
**Source boundary**: apps/web-next/src/SpatialWorkspace.tsx, CadEditorPanel.tsx, MapChat.tsx, OrgChat.tsx, AiCopilotPanel.tsx, ai-copilot/; apps/mcp-rust/.
- [ ] T085 [US8] [COLLAB-01] 3D map view, camera, layers, terrain: xác minh và hoàn thiện Giữ Cesium/MapLibre; camera/selection sync theo capability; không thay bằng QGIS engine; test/evidence theo contracts/ux-contracts.md (P2).
- [ ] T086 [US8] [COLLAB-02] CAD Model/Layout, save, document revision: xác minh và hoàn thiện Giữ OpenCAD engine; lưu đúng bản vẽ/phiên bản, dirty gate, không lệnh CAD giả; test/evidence theo contracts/ux-contracts.md (P1).
- [ ] T087 [US8] [COLLAB-03] Application Shell chung CAD/GIS/3D: xác minh và hoàn thiện Chat/task/AI luôn khả dụng theo quyền, chỉ toolbar chuyên ngành đổi; không remount thừa; test/evidence theo contracts/ux-contracts.md (P1).
- [ ] T088 [US8] [COLLAB-04] AI multi-workspace, trạng thái lệnh, consent: xác minh và hoàn thiện Rõ dự án/mode/doc/revision, history, hủy, status/expiry; không replay lệnh sau reload; test/evidence theo contracts/ux-contracts.md (P1).
- [ ] T089 [US8] [COLLAB-05] Chat và giao việc xuyên engine: xác minh và hoàn thiện Một nguồn dữ liệu, không mất tin nhắn/draft khi chuyển mode; chống trùng subscription; test/evidence theo contracts/ux-contracts.md (P1).
- [ ] T090 [US8] [COLLAB-06] Plugin lifecycle, menu & tool registry: xác minh và hoàn thiện Plugin dùng shell/dock/registry chính, capability, không SPA thứ hai; uninstall an toàn; test/evidence theo contracts/ux-contracts.md (P2).
- [ ] T091 [US8] [COLLAB-07] Accessibility keyboard/focus/contrast: xác minh và hoàn thiện WCAG 2.2 AA mục tiêu cho controls, tab order, keyboard alternatives cho drag, focus restore; test/evidence theo contracts/ux-contracts.md (P1).
- [ ] T092 [US8] [COLLAB-08] Responsive & hiệu suất bản đồ: xác minh và hoàn thiện Đo desktop 1280/1440/1920, 390px field workflow; baseline p95, map không trắng khi resize; test/evidence theo contracts/ux-contracts.md (P1).
- [ ] T093 [US8] [COLLAB-09] Offline chính thức vs Guest GIS: xác minh và hoàn thiện Giữ offline có scope/identity; tuyệt đối không tái tạo GuestStudio hoặc tự mở GIS cục bộ; test/evidence theo contracts/ux-contracts.md (P1).
- [ ] T094 [US8] Add/update targeted browser tests in tests/e2e/qgis-parity-collab.spec.ts; run existing regression suite and store evidence for this wave.
- [ ] T095 [US8] Review wave changes against Constitution, capability/permission and performance gate; update matrix and deliver independently reviewable PR.

## Final convergence and release
- [ ] T096 Reconcile all matrix rows and requirement IDs; no P1 blocker waived without owner decision.
- [ ] T097 Run full frontend build/test, Rust and PostGIS integration, browser visual/keyboard and multi-tenant tests, preserve offline and CAD dirty protections.
- [ ] T098 Verify QGIS paired fixtures, task time/click count, p95 regression and actual exported map/data artifacts; report measured results and deviations.
- [ ] T099 Review mobile/desktop keyboard focus, accessibility and language; check no guest/local alternate SPA or duplicate frontend build.
- [ ] T100 Create GitHub release artifact tied to approved commit; deploy DEV using artifact, authenticated smoke, rollback rehearsal; no production rollout without gates.
- [ ] T101 Promote approved artifact to live; validate real login/project/GIS/CAD/3D/AI flows and public health; record deployment evidence or blockers.
