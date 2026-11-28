# Ma trận đối chiếu QGIS Desktop 3.44 ↔ OpenVietGIS

Baseline OpenVietGIS: `5bfc83826161acd83b98a925e85690cebde53ae1` (27/11/2026). Đối chiếu source và tài liệu, chưa phải audit bằng ảnh hoặc kiểm thử trình duyệt đăng nhập. Không lấy số lượng file/nút làm bằng chứng parity.

**Trạng thái:** D = có implementation nhận diện được; P = một phần hoặc cần kiểm tra độ sâu; U = chưa xác nhận được implementation phù hợp (không khẳng định tuyệt đối là không có). V = đã nghiệm thu end-to-end theo acceptance (chưa gán V cho bất kỳ dòng nào trong đợt baseline). Tổng 71 tiêu chí: D 19, P 48, U 4. Đây là số tiêu chí phân loại, không phải % hoàn thành toàn bộ QGIS.

Mỗi dòng phải được gắn command ID/capability, fixture, test ID, ảnh QGIS/OVG và kết quả đo trong evidence. Chỉ đổi sang V khi đủ bằng chứng; U phải kiểm tra trước khi phát triển mới, tránh code trùng. P1 = ưu tiên nghiệp vụ nền tảng; P2 = nâng cao; P3 = khảo sát mở rộng. Nguồn QGIS theo chương tại research.md.

## Application Shell & điều hướng (SHELL)

| ID | QGIS baseline / mục tiêu | Hiện trạng | Bằng chứng source OVG | Điều kiện tương đương theo phạm vi | Ưu tiên |
|---|---|---|---|---|---|
| SHELL-01 | Menu chức năng Project/Edit/View/Layer/Settings/Plugins/Vector/Raster/Processing/Help | P | CoreRibbon, CoreCommandBar | Nhóm theo ngữ cảnh và capability; menu/toolbar/locator dùng cùng command ID | P1 |
| SHELL-02 | Toolbars theo nhóm, kéo thả, ẩn/hiện | D | QgisTools; toolbar nổi, compact/full | Kiểm tra thứ tự, vị trí, reset, narrow screen và phím tắt | P1 |
| SHELL-03 | Locator / Ctrl+K tìm lệnh, dữ liệu, tọa độ | P | CoreCommandBar, CoreRibbon tìm công cụ | Locator đa nguồn có scope, tìm tên lệnh VN/EN, layer, tọa độ; không vượt quyền | P1 |
| SHELL-04 | Status bar tọa độ, CRS, tỷ lệ, render | P | QgisMapStatus, CrsPicker | Hiển thị CRS rõ ràng, tỷ lệ, đơn vị, trạng thái tải, tiến độ và lưu | P1 |
| SHELL-05 | Profile giao diện / cấu hình phím tắt | P | hotkeys.ts, DockLayout.tsx | Profile theo tài khoản, cấu hình keybinding có xung đột, reset; giữ A theo lựa chọn | P1 |
| SHELL-06 | Map-only / fullscreen, đóng/mở panel | P | LiveApp fullscreen, DockLayout | Phục hồi chính xác bố cục, không làm mất lựa chọn hoặc bản nháp | P1 |
| SHELL-07 | Mở lại project/mode/camera/tool sau F5 | P | LiveApp, SpatialWorkspace, readResume | Phục hồi theo tài khoản/tổ chức/dự án, quyền hiện tại, không hiện GuestStudio | P1 |
| SHELL-08 | Empty/loading/error/session expired | P | LiveApp, studio-ui.tsx | Thông báo có hành động, retry có giới hạn, không hiển thị lỗi kỹ thuật mặc định | P1 |

## Dock, Browser & Layers (DOCK)

| ID | QGIS baseline / mục tiêu | Hiện trạng | Bằng chứng source OVG | Điều kiện tương đương theo phạm vi | Ưu tiên |
|---|---|---|---|---|---|
| DOCK-01 | Dock trái/phải/dưới/nổi, resize | D | DockLayout, Splitter | Kéo thả mọi panel theo capability; resize chuột và bàn phím, giới hạn viewport | P1 |
| DOCK-02 | Tab dock, lưu/đổi/reset layout | P | DockLayout tabs và localStorage | Tab stack, layout preset và keyboard alternatives; không rơi panel ngoài màn | P1 |
| DOCK-03 | Browser tree nguồn dữ liệu, metadata, drag/drop | P | CatalogManager, DsmPanes, QgisLayerDock | Browser nguồn server, kéo layer vào bản đồ, xem metadata và lỗi provider | P1 |
| DOCK-04 | Data Source Manager | D | DsmPanes, TransferTool, ExternalPluginHub | Luồng chọn nguồn, CRS, preview, import, tiến độ, kết quả; kiểm thử từng provider | P1 |
| DOCK-05 | Nhóm lớp lồng nhau, tri-state, kéo thả | D | layer-tree.ts, QgisLayerDock | Kiểm tra nhóm lồng, kéo nhiều lớp, đổi thứ tự và lưu dự án chung | P1 |
| DOCK-06 | Chọn nhiều layer Ctrl/Shift, menu ngữ cảnh | D | QgisLayerDock | Chọn bằng bàn phím, thao tác batch, menu lớp/nhóm đúng capability | P1 |
| DOCK-07 | Legend cây lớp, lọc lớp, scale range | D | QgisLayerDock, LayerProperties | Legend đúng style; trạng thái ngoài tỷ lệ, lọc chỉ đổi view | P1 |
| DOCK-08 | Map themes / nhóm hiển thị độc quyền | D | QgisLayerDock, LayerProperties | Lưu, đổi, xóa theme; xác định cấu hình cá nhân/chia sẻ rõ ràng | P2 |
| DOCK-09 | Layer order riêng / map view nhiều khung | U | Không xác nhận qua inventory hiện tại | Đặc tả render order độc lập; thêm secondary view chỉ khi adapter hỗ trợ | P2 |
| DOCK-10 | Remove khỏi map vs xóa layer/source | P | QgisLayerDock, CatalogManager | Phân biệt hành động và hậu quả; chặn xóa trái quyền, có xác nhận | P1 |

## Layer Properties, Symbology, Labels (STYLE)

| ID | QGIS baseline / mục tiêu | Hiện trạng | Bằng chứng source OVG | Điều kiện tương đương theo phạm vi | Ưu tiên |
|---|---|---|---|---|---|
| STYLE-01 | Information / Source / Fields / Display | D | LayerProperties 7 tab | Thông tin nguồn/storage/display CRS, fields, rendering; Apply/OK/Cancel nhất quán | P1 |
| STYLE-02 | Single symbol / categorized / graduated | D | LayerProperties, graduated.ts | So khớp fixture phân loại null, breaks, màu, legend, export | P1 |
| STYLE-03 | Rule-based / symbol layers / data-defined overrides | U | Chưa xác nhận bộ renderer tương ứng | Cấu trúc rule, nhiều symbol layer, kiểm tra biểu thức và render có bằng chứng | P2 |
| STYLE-04 | Labels theo trường/biểu thức, vị trí, buffer | P | LayerProperties, label-expr.ts | Preview, placement theo geometry, ưu tiên/collision, font/halo; giới hạn rõ | P1 |
| STYLE-05 | Expression Builder dùng chung | P | label-expr.ts, Field Calculator, filter | AST typed, autocomplete field/function, preview, null, báo lỗi; không eval/SQL tùy ý | P1 |
| STYLE-06 | Styles QML/SLD, import/export, style presets | P | style-io.ts, LayerProperties | Round-trip tập con được hỗ trợ; cảnh báo thuộc tính không tương thích | P2 |
| STYLE-07 | Joins / relations / constraints / form layout | P | ExcelJoin.ts, ProfessionalTools, RelationInspector, RelationValuePicker, editing.relations/changesets và deferred DB triggers; có code chọn khóa và ràng buộc dùng chung, chờ nghiệm thu | Join có cardinality, relation widget, constraint phía server, form thiết kế có scope | P2 |
| STYLE-08 | Metadata, diagrams, actions, map tips | U | Metadata cơ bản; phần còn lại chưa xác nhận | Tách tab theo capability, không dựng nút giả; action sandbox allowlist | P2 |
| STYLE-09 | Layer styling panel live preview | P | LayerProperties modal, LayerLabels | Panel styling preview và apply rõ ràng; cancel không ghi đè dữ liệu | P2 |

## Attribute Table, Form & Selection (TABLE)

| ID | QGIS baseline / mục tiêu | Hiện trạng | Bằng chứng source OVG | Điều kiện tương đương theo phạm vi | Ưu tiên |
|---|---|---|---|---|---|
| TABLE-01 | Table gắn với active layer, map/table selection | D | QgisAttributeTable | Một selection store theo layer/revision; chọn ở map/table/inspector khớp | P1 |
| TABLE-02 | Tìm kiếm, phân trang, sort/filter toàn bộ nguồn | P | QgisAttributeTable lọc inputRows/cursor | Server-side filter/sort trên toàn nguồn; ghi rõ loaded vs total; không lọc riêng trang gây hiểu nhầm | P1 |
| TABLE-03 | Chọn theo biểu thức, vị trí, vùng chọn | P | ProfessionalTools, CoreMapTools query | Select/add/remove/intersect/invert trên toàn tập hợp có permission và count | P1 |
| TABLE-04 | Form view / editor widget theo kiểu trường | P | FeatureInfo, AdvancedEditor | Table/form toggle, typed date/enum/relation/null, validation field-level | P1 |
| TABLE-05 | Inline cell edit, multi-edit, save/rollback | P | AdvancedEditor bulkSave | Chỉnh sửa dòng/cột trong transaction ngắn, conflict, undo có receipt | P1 |
| TABLE-06 | Field Calculator / add field / virtual field | P | ProfessionalTools Field Calculator | Preview công thức, phạm vi selected/all, nullable/kiểu dữ liệu, atomic update | P1 |
| TABLE-07 | Copy/paste, export selected, cấu hình cột | P | ExportDialog, ProfessionalTools tsv | Copy dữ liệu đúng phạm vi; cột/width/order cá nhân, export selected có cảnh báo | P2 |
| TABLE-08 | Identify Results nhiều layer và statistics | P | FeatureInfo, ProfessionalTools statistics | Identify theo layer/mode và tổng hợp toàn nguồn, không đồng nhất mẫu với toàn bộ | P2 |

## Digitizing, Snapping & Topology (EDIT)

| ID | QGIS baseline / mục tiêu | Hiện trạng | Bằng chứng source OVG | Điều kiện tương đương theo phạm vi | Ưu tiên |
|---|---|---|---|---|---|
| EDIT-01 | Pan, zoom, identify, extent, Space-pan tạm | D | QgisTools, hotkeys.ts, LiveApp | Không đổi công cụ đang chọn khi nhả Space; không capture khi input/IME | P1 |
| EDIT-02 | Point/Line/Polygon digitize, vertex editor | D | CoreMapTools, AdvancedEditor | Create/move/insert/delete vertex, precision input, preview, save/cancel có test | P1 |
| EDIT-03 | Edit mode & unsaved changes, save/rollback | P | CoreMapTools, LiveApp, AdvancedEditor | Layer edit state rõ; bản nháp khác shared revision; cảnh báo chuyển project/F5 | P1 |
| EDIT-04 | Snapping vertex/segment, layer-specific tolerance | P | SnapMath, LayerProperties WorkspacePrefs | Chọn active/all/advanced theo layer, pixel/map unit, chỉ báo snap; fixture CRS | P1 |
| EDIT-05 | Topology checks, avoid overlap, shared boundary | P | ProfessionalTools topology, Rust editing | Kiểm tra invalid/dangling/overlap/gap; không auto-fix nếu chưa duyệt | P1 |
| EDIT-06 | Split/merge, reshape, multipart, ring | P | AdvancedEditor split/merge | Hỗ trợ phần còn thiếu theo geometry capability; rollback atomic, giữ ID chính sách | P2 |
| EDIT-07 | Advanced digitizing constraint distance/angle/ortho | P | CoreMapTools, ProfessionalTools, SurveyTerrain | Khóa góc/khoảng cách/trục; đơn vị CRS rõ, không đổi tọa độ âm thầm | P2 |
| EDIT-08 | Undo/redo, edit history, concurrent conflict | D | CoreMapTools, AdvancedEditor, EditHistoryDetails | Phân biệt draft undo và persisted revert; revision pin, 2 user conflict | P1 |
| EDIT-09 | Measure distance/area/bearing | D | CoreMapTools, geom-measure.test.mjs | Đơn vị và phương pháp trắc địa/planar rõ; đối chiếu mẫu QGIS | P1 |

## Processing, Raster & Analysis (PROC)

| ID | QGIS baseline / mục tiêu | Hiện trạng | Bằng chứng source OVG | Điều kiện tương đương theo phạm vi | Ưu tiên |
|---|---|---|---|---|---|
| PROC-01 | Processing Toolbox search, category/provider | P | ProcessingToolbox | Registry thuật toán theo capability, tìm/lọc, help, favorite/recent; không hiện thuật toán giả | P1 |
| PROC-02 | Algorithm parameter form / preview | P | ProcessingToolbox, CoreWorkbench, RasterWorkbench | Typed inputs, CRS, extent, output, validation, chạy lại cùng tham số | P1 |
| PROC-03 | Buffer/Clip/Dissolve, spatial joins & analysis | P | CoreMapTools, AdvancedEditor, Rust processing; 8 thuật toán, Union/Spatial Join/Reproject có code, chờ golden/CI | Golden fixtures với QGIS; mở rộng intersect/union/difference/reproject theo coverage | P1 |
| PROC-04 | Job progress, history, cancel, retry | P | CoreMapTools job controls, Rust processing | Durable job id, trạng thái queued/running/succeeded/failed/canceled, retry idempotent | P1 |
| PROC-05 | Batch processing | P | ProcessingWorkbenchPage + processing-batch.ts: tối đa 20 hàng, chạy tuần tự, hiển thị đầu vào/tham số/trạng thái/lỗi từng hàng, phục hồi yêu cầu chưa xác minh, xuất lịch sử JSON | Chưa có xác nhận Rust/PostGIS có xác thực và đối chiếu QGIS; chưa phải parity batch đầy đủ | P2 |
| PROC-06 | Model Designer | D | ProcessingModeler, Rust processing_models.rs | Graph DAG, parameter mapping, named outputs, revision, cancel/partial results | P2 |
| PROC-07 | Raster properties, band, nodata, rendering | P | ProjectRaster, RasterWorkbench | Band metadata, stretch, palette, opacity, nodata, CRS; capability từng provider | P2 |
| PROC-08 | Raster Calculator, NDVI/NDWI, DEM terrain | D | RasterWorkbench, RasterReportWorkbench, TerrainEngineering | Kết quả theo fixture, nodata/CRS/extent, lưu output có lineage | P2 |
| PROC-09 | Georeferencer raster/vector GCP | P | Raster affine GCP + north-up nearest/bilinear cho TIFF giới hạn; vector XY GeoJSON affine WGS84, RMS và xuất tệp | Kiểm thử mới, topology vector, mask/palette raster mở rộng, nhập vào project và QGIS acceptance còn thiếu | P2 |
| PROC-10 | Temporal controller / mesh / point clouds | P | TemporalMapView + temporal-plan.ts: phát khung ngày UTC hoặc khoảng/bước UTC tùy chỉnh, có trường kết thúc tùy chọn, cho lớp vector đã tải đủ ≤5.000 đối tượng, có trường timestamp và quyền đọc; read-only secondary map | Raster temporal series, mesh providers và point cloud LAS/LAZ/COPC/EPT chưa hỗ trợ; acceptance QGIS pending | P3 |

## Print Layout, CRS & interoperability (OUTPUT)

| ID | QGIS baseline / mục tiêu | Hiện trạng | Bằng chứng source OVG | Điều kiện tương đương theo phạm vi | Ưu tiên |
|---|---|---|---|---|---|
| OUTPUT-01 | Layout manager, page, map frames | D | LayoutDesigner, PrintDesigner | Danh sách layout, khổ giấy, trang, map frame, CRS, scale; lưu phiên bản | P1 |
| OUTPUT-02 | Item properties, guides/grid, alignment, undo | P | LayoutDesigner | Dock item properties, align/distribute, guides, snap, lock, undo/redo; golden layout | P2 |
| OUTPUT-03 | Legend, north arrow, scale bar, label | D | LayoutDesigner, LayoutRender | Liên kết đúng map item, scale và legend; không sai đơn vị/CRS | P1 |
| OUTPUT-04 | Atlas / report nhiều trang | U | Chưa xác nhận | Coverage layer, lọc, trang theo đối tượng, batch export; không rò dữ liệu | P3 |
| OUTPUT-05 | PDF/PNG/SVG export, print preview | P | LayoutDesigner, PrintDesigner | Kiểm tra PDF/PNG, font VN, DPI, clipping; SVG chỉ khi engine hỗ trợ | P1 |
| OUTPUT-06 | Project/source/display CRS, reprojection, VN-2000 | P | CrsPicker, crs-display.ts; Rust regional_crs: fixture/lineage verification for approved Quang Ninh edges, native feature/viewport/reproject; unverified provinces fail closed | Phân biệt display/transform, xác nhận hệ VN-2000 theo tỉnh/kinh tuyến, control points | P1 |
| OUTPUT-07 | Import/export GeoJSON/GPKG/SHP/CSV/DXF/raster | P | TransferTool, ExportDialog, Rust transfers | Kiểm tra từng format thực, CRS, schema/null/encoding, geometry, round-trip; không đoán parity | P1 |
| OUTPUT-08 | Đồng bộ QGIS desktop & style/project exchange | P | QgisBindingManager, QgisConnectPage | Sync hai chiều, revision, conflict; QGZ/QGS/QML subset rõ ràng; plugin Python QGIS ngoài scope | P2 |

## CAD/3D, collaboration, AI & cross-cutting (COLLAB)

| ID | QGIS baseline / mục tiêu | Hiện trạng | Bằng chứng source OVG | Điều kiện tương đương theo phạm vi | Ưu tiên |
|---|---|---|---|---|---|
| COLLAB-01 | 3D map view, camera, layers, terrain | P | SpatialWorkspace, Spatial3DSettings, packages/spatial-workspace | Giữ Cesium/MapLibre; camera/selection sync theo capability; không thay bằng QGIS engine | P2 |
| COLLAB-02 | CAD Model/Layout, save, document revision | P | CadEditorPanel, cad-editor.ts | Giữ OpenCAD engine; lưu đúng bản vẽ/phiên bản, dirty gate, không lệnh CAD giả | P1 |
| COLLAB-03 | Application Shell chung CAD/GIS/3D | P | LiveApp ovg-shared-tools, SpatialWorkspace | Chat/task/AI luôn khả dụng theo quyền, chỉ toolbar chuyên ngành đổi; không remount thừa | P1 |
| COLLAB-04 | AI multi-workspace, trạng thái lệnh, consent | P | AiCopilotPanel, first-party.js, apps/mcp-rust | Rõ dự án/mode/doc/revision, history, hủy, status/expiry; không replay lệnh sau reload | P1 |
| COLLAB-05 | Chat và giao việc xuyên engine | D | MapChat, OrgChat, TeamCollaboration, TaskReminder | Một nguồn dữ liệu, không mất tin nhắn/draft khi chuyển mode; chống trùng subscription | P1 |
| COLLAB-06 | Plugin lifecycle, menu & tool registry | P | AppMarket, ExternalPluginHub, workbench-registry | Plugin dùng shell/dock/registry chính, capability, không SPA thứ hai; uninstall an toàn | P2 |
| COLLAB-07 | Accessibility keyboard/focus/contrast | P | aria attributes, tests/e2e/*accessibility* | WCAG 2.2 AA mục tiêu cho controls, tab order, keyboard alternatives cho drag, focus restore | P1 |
| COLLAB-08 | Responsive & hiệu suất bản đồ | P | SpatialPerformance, SpatialCache, SpatialIncremental | Đo desktop 1280/1440/1920, 390px field workflow; baseline p95, map không trắng khi resize | P1 |
| COLLAB-09 | Offline chính thức vs Guest GIS | P | SpatialCache, workbenches/features/offline, Spec 062 | Giữ offline có scope/identity; tuyệt đối không tái tạo GuestStudio hoặc tự mở GIS cục bộ | P1 |

## Ngoài phạm vi parity trực tiếp
QGIS Desktop có hệ sinh thái provider/plugin, PyQGIS, GRASS và nhiều định dạng/thuật toán. Không hứa tái tạo mọi plugin, mọi renderer hoặc toàn bộ thuật toán QGIS. Mỗi provider/thuật toán ngoài inventory phải có quyết định migrate/retain/adapter/defer và lý do; không đưa vào UI nếu backend chưa hỗ trợ. CAD/3D, chat, nhiệm vụ và AI là yêu cầu tích hợp OpenVietGIS, không được tính là điểm vượt QGIS nếu chưa có phép đo tương đương.
