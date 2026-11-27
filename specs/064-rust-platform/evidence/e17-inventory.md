# E17 Inventory & Contract Evidence — Canonical web, desktop-ready shell

- **Spec**: `specs/064-rust-platform/engines/e17-frontend-workspace.md` (Parent refs: R037, R038, R062)
- **Baseline SHA**: `3e78ac142acfc050ec22d30da5c44ad0e6b8459a`
- **Extractor**: `specs/064-rust-platform/contracts/extract_client_module_inventory.py`
- **Contract JSON**: `specs/064-rust-platform/contracts/engines/e17.json`
- **AC1 Status**: `verified` (`E17-T01`)
- **Architecture Candidate**: Retain canonical React/MapLibre/Cesium web-next shell, client-sdk, and spatial-workspace; replace legacy apps/web router after parity verification.

## Exact Inventory Counts

| Metric | Count |
|---|---:|
| Runtime HTTP routes (mounted) | 0 |
| SSE streaming endpoints | 0 |
| WebSocket endpoints | 0 |
| MCP tools | 0 |
| Workbench commands | 111 |
| Client SDK / web API paths | 89 |
| Connectors / adapters / services | 85 |
| Migrations | 0 |
| Infra & operational scripts | 0 |
| Tracked source files | 370 |
| Mapped test files | 199 |
| Mapped test IDs | 438 |

## Classification Breakdown

| Classification | Source Files |
|---|---:|
| `migrate` | 0 |
| `retain` | 251 |
| `replace` | 119 |
| `approved_retire` | 0 |

## Auth & Tenant Boundaries

- **tenant_scope_counts**: `{}`
- **routes_with_project_access_guard**: 0
- **routes_with_csrf_enforcement**: 0
- **core_permissions_referenced**: none
- **extension_permissions_referenced**: none
- **frontend_auth_isolation**: SessionBoundary + CSRF header (x-openvietgis-csrf) + tenant/actor-scoped browser query & spatial cache isolation.

## Component, Connector, Module & Operation Contracts

### Canonical `apps/web-next` Routes (11, `retain`)
- `/` — Landing or LiveApp workspace (`apps/web-next/src/main.tsx`)
- `/?qgis_connect=1` — QgisConnectPage (`apps/web-next/src/main.tsx`)
- `/?auth_complete=1` — AuthComplete (`apps/web-next/src/main.tsx`)
- `/?marketplace=1` — MarketplacePage (`apps/web-next/src/main.tsx`)
- `/?developer=1` — PublisherPortal (`apps/web-next/src/main.tsx`)
- `/offline-field/:organizationId/:projectId` — OfflineFieldPage (`apps/web-next/src/main.tsx`)
- `/app/organizations/:organizationId` — Organization workspace (`apps/web-next/src/route-compat.ts`)
- `/app/organizations/:organizationId/projects/:projectId` — Project map workbench (`apps/web-next/src/route-compat.ts`)
- `/app/organizations/:organizationId/projects/:projectId/:tool` — Project tool route (spatial|transfers|editing|queries|processing|cartography|platform-hub|cad|external|collab) (`apps/web-next/src/route-compat.ts`)
- `/app/account` — Account view (alias: /identity) (`apps/web-next/src/route-compat.ts`)
- `/devui/*` — Dev prefix compatibility wrapper (`apps/web-next/src/route-compat.ts`)

### Legacy `apps/web` Router Routes (36, `replace`)
- Total legacy routes in `apps/web/src/app/router.tsx`: 36 (superseded by canonical `apps/web-next`; verify redirect parity before `approved_retire`).

### Registered Workbench Commands (111, `retain`)
| Alias | Label | Group | Source |
|---|---|---|---|
| `TERRAIN` | Địa hình kỹ thuật · DEM, TIN, đào đắp, dòng chảy | Phân tích | `apps/web-next/src/LiveApp.tsx` |
| `MODELER` | Processing Modeler · xây dựng quy trình GIS | Phân tích | `apps/web-next/src/LiveApp.tsx` |
| `RASTER` | Phân tích GeoTIFF / raster | Phân tích | `apps/web-next/src/LiveApp.tsx` |
| `NDVI` | NDVI · thảm thực vật | Phân tích | `apps/web-next/src/LiveApp.tsx` |
| `NDWI` | NDWI · nước mặt | Phân tích | `apps/web-next/src/LiveApp.tsx` |
| `NDBI` | NDBI · khu xây dựng | Phân tích | `apps/web-next/src/LiveApp.tsx` |
| `HILLSHADE` | Đổ bóng địa hình DEM | Phân tích | `apps/web-next/src/LiveApp.tsx` |
| `RASTERCALC` | Raster Calculator | Phân tích | `apps/web-next/src/LiveApp.tsx` |
| `RGB` | Tổ hợp màu RGB / đa phổ | Phân tích | `apps/web-next/src/LiveApp.tsx` |
| `RASTERCLIP` | Cắt raster theo ranh giới | Phân tích | `apps/web-next/src/LiveApp.tsx` |
| `CHANGE` | Biến động raster hai thời điểm | Phân tích | `apps/web-next/src/LiveApp.tsx` |
| `SLOPE` | Độ dốc địa hình | Phân tích | `apps/web-next/src/LiveApp.tsx` |
| `ASPECT` | Hướng dốc địa hình | Phân tích | `apps/web-next/src/LiveApp.tsx` |
| `MOSAIC` | Ghép raster | Phân tích | `apps/web-next/src/LiveApp.tsx` |
| `RESAMPLE` | Đổi độ phân giải raster | Phân tích | `apps/web-next/src/LiveApp.tsx` |
| `ZONAL` | Thống kê raster theo vùng | Phân tích | `apps/web-next/src/LiveApp.tsx` |
| `CONTOUR` | Tạo đường đồng mức | Phân tích | `apps/web-next/src/LiveApp.tsx` |
| `SENTINEL` | Sentinel-2 · tìm ảnh và phân tích đa phổ | Phân tích | `apps/web-next/src/LiveApp.tsx` |
| `SURVEY` | Trắc địa · tọa độ phẳng VN-2000 | CAD | `apps/web-next/src/LiveApp.tsx` |
| `TOOLBOX` | Hộp công cụ xử lý | Phân tích | `apps/web-next/src/LiveApp.tsx` |
| `ADD` | Thêm dữ liệu | Layer | `apps/web-next/src/CoreMapTools.tsx` |
| `LA` | Quản lý layer / nguồn / CRS | Layer | `apps/web-next/src/CoreMapTools.tsx` |
| `LABEL` | Thiết lập nhãn layer | Layer | `apps/web-next/src/CoreMapTools.tsx` |
| `LAYER` | Thuộc tính layer đang chọn | Layer | `apps/web-next/src/CoreMapTools.tsx` |
| `UP` | Đưa layer lên | Layer | `apps/web-next/src/CoreMapTools.tsx` |
| `DOWN` | Đưa layer xuống | Layer | `apps/web-next/src/CoreMapTools.tsx` |
| `HIDE` | Ẩn layer đang chọn | Layer | `apps/web-next/src/CoreMapTools.tsx` |
| `SHOW` | Hiện tất cả layer | Layer | `apps/web-next/src/CoreMapTools.tsx` |
| `LAYERS` | Ẩn / hiện bảng layer | Layer | `apps/web-next/src/CoreMapTools.tsx` |
| `ATTR` | Bảng thuộc tính | Layer | `apps/web-next/src/CoreMapTools.tsx` |
| `ZE` | Zoom toàn bản đồ | Bản đồ | `apps/web-next/src/CoreMapTools.tsx` |
| `ZL` | Zoom layer đang chọn | Bản đồ | `apps/web-next/src/CoreMapTools.tsx` |
| `ZS` | Zoom đối tượng đang chọn | Bản đồ | `apps/web-next/src/CoreMapTools.tsx` |
| `ZI` | Phóng to | Bản đồ | `apps/web-next/src/CoreMapTools.tsx` |
| `ZO` | Thu nhỏ | Bản đồ | `apps/web-next/src/CoreMapTools.tsx` |
| `ZOOM` | Đặt mức zoom | Bản đồ | `apps/web-next/src/CoreMapTools.tsx` |
| `PAN` | Đến tọa độ | Bản đồ | `apps/web-next/src/CoreMapTools.tsx` |
| `CLR` | Bỏ chọn | Bản đồ | `apps/web-next/src/CoreMapTools.tsx` |
| `SAVE` | Lưu bản đồ | Bản đồ | `apps/web-next/src/CoreMapTools.tsx` |
| `VIEW` | Lưu góc nhìn mặc định | Bản đồ | `apps/web-next/src/CoreMapTools.tsx` |
| `HISTORY` | Lịch sử cấu hình bản đồ | Bản đồ | `apps/web-next/src/CoreMapTools.tsx` |
| `RELOAD` | Tải lại dữ liệu | Bản đồ | `apps/web-next/src/CoreMapTools.tsx` |
| `EDIT` | Bật chỉnh sửa lớp đang chọn | Biên tập | `apps/web-next/src/CoreMapTools.tsx` |
| `MOVE` | Di chuyển điểm đang chọn | Biên tập | `apps/web-next/src/CoreMapTools.tsx` |
| `POINT` | Vẽ điểm | Biên tập | `apps/web-next/src/CoreMapTools.tsx` |
| `LINE` | Vẽ tuyến | Biên tập | `apps/web-next/src/CoreMapTools.tsx` |
| `POLYGON` | Vẽ vùng | Biên tập | `apps/web-next/src/CoreMapTools.tsx` |
| `ERASE` | Xóa đối tượng (xác nhận) | Biên tập | `apps/web-next/src/CoreMapTools.tsx` |
| `UNDO` | Hoàn tác | Biên tập | `apps/web-next/src/CoreMapTools.tsx` |
| `REDO` | Làm lại | Biên tập | `apps/web-next/src/CoreMapTools.tsx` |
| `EDITHISTORY` | Lịch sử biên tập layer | Biên tập | `apps/web-next/src/CoreMapTools.tsx` |
| `DIST` | Đo khoảng cách | Đo đạc | `apps/web-next/src/CoreMapTools.tsx` |
| `AREA` | Đo diện tích | Đo đạc | `apps/web-next/src/CoreMapTools.tsx` |
| `END` | Hoàn tất phép đo / bản vẽ | Đo đạc | `apps/web-next/src/CoreMapTools.tsx` |
| `ESC` | Hủy phép đo / bản vẽ | Đo đạc | `apps/web-next/src/CoreMapTools.tsx` |
| `QUERY` | Truy vấn thuộc tính / không gian | Phân tích | `apps/web-next/src/CoreMapTools.tsx` |
| `BUFFER` | Vùng đệm Buffer | Phân tích | `apps/web-next/src/CoreMapTools.tsx` |
| `CLIP` | Cắt lớp Clip | Phân tích | `apps/web-next/src/CoreMapTools.tsx` |
| `DISSOLVE` | Gộp vùng Dissolve | Phân tích | `apps/web-next/src/CoreMapTools.tsx` |
| `JOBS` | Tác vụ xử lý / hủy / chạy lại | Phân tích | `apps/web-next/src/CoreMapTools.tsx` |
| `DXF` | Nhập / xuất DXF | CAD | `apps/web-next/src/CoreMapTools.tsx` |
| `COGO` | Trắc địa COGO | CAD | `apps/web-next/src/CoreMapTools.tsx` |
| `CRS` | Hệ tọa độ / VN-2000 | CAD | `apps/web-next/src/CoreMapTools.tsx` |
| `IMPORT` | Nhập dữ liệu vector | Xuất bản | `apps/web-next/src/CoreMapTools.tsx` |
| `EXPORT` | Xuất dữ liệu… | Xuất bản | `apps/web-next/src/CoreMapTools.tsx` |
| `STYLE` | Ký hiệu / chú giải | Xuất bản | `apps/web-next/src/CoreMapTools.tsx` |
| `PLOT` | Bố cục in PDF / PNG | Xuất bản | `apps/web-next/src/CoreMapTools.tsx` |
| `SNAP` | Bật / tắt snapping | Biên tập | `apps/web-next/src/CoreMapTools.tsx` |
| `WORKSPACE` | Tùy chỉnh workspace | Bản đồ | `apps/web-next/src/CoreMapTools.tsx` |
| `BASE` | Bật / tắt bản đồ nền | Bản đồ | `apps/web-next/src/CoreMapTools.tsx` |
| `WMS` | Nguồn ngoài WMS / WMTS / COG | Khác | `apps/web-next/src/CoreMapTools.tsx` |
| `FIELD` | Thực địa / offline | Khác | `apps/web-next/src/CoreMapTools.tsx` |
| `REVIEW` | Cộng tác / phê duyệt | Khác | `apps/web-next/src/CoreMapTools.tsx` |
| `PLUGIN` | Quản lý plugin | Khác | `apps/web-next/src/CoreMapTools.tsx` |
| `OPS` | Trạng thái nền tảng | Khác | `apps/web-next/src/CoreMapTools.tsx` |
| `NEWLAYER` | Tạo layer rỗng | Layer | `apps/web-next/src/AdvancedEditor.tsx` |
| `VERTEX` | Chỉnh sửa đỉnh | Biên tập | `apps/web-next/src/AdvancedEditor.tsx` |
| `SPLIT` | Cắt đối tượng | Biên tập | `apps/web-next/src/AdvancedEditor.tsx` |
| `MERGE` | Gộp đối tượng | Biên tập | `apps/web-next/src/AdvancedEditor.tsx` |
| `BULK` | Sửa thuộc tính hàng loạt | Biên tập | `apps/web-next/src/AdvancedEditor.tsx` |
| `DELSEL` | Xóa các đối tượng đang chọn (một lớp) | Biên tập | `apps/web-next/src/ProfessionalTools.tsx` |
| `TOPOLOGY` | Kiểm tra topology | Phân tích | `apps/web-next/src/ProfessionalTools.tsx` |
| `SCHEMA` | Thêm / đổi tên trường | Layer | `apps/web-next/src/ProfessionalTools.tsx` |
| `CALC` | Field Calculator | Layer | `apps/web-next/src/ProfessionalTools.tsx` |
| `JOIN` | Ghép thuộc tính từ Excel / CSV | Layer | `apps/web-next/src/ProfessionalTools.tsx` |
| `STATS` | Thống kê theo trường / nhóm | Layer | `apps/web-next/src/ProfessionalTools.tsx` |
| `SELRECT` | Chọn vùng chữ nhật | Bản đồ | `apps/web-next/src/ProfessionalTools.tsx` |
| `SELPOLY` | Chọn theo polygon | Bản đồ | `apps/web-next/src/ProfessionalTools.tsx` |
| `DESELECT` | Bỏ chọn nhiều đối tượng | Bản đồ | `apps/web-next/src/ProfessionalTools.tsx` |
| `ORTHO` | Vẽ vuông góc | CAD | `apps/web-next/src/ProfessionalTools.tsx` |
| `OSNAP` | Bắt đỉnh / cạnh | CAD | `apps/web-next/src/ProfessionalTools.tsx` |
| `TILES` | Chế độ bản đồ vector tiles | Bản đồ | `apps/web-next/src/ProfessionalTools.tsx` |
| `LAYOUT` | Layout Designer · thiết kế bản đồ in | Xuất bản | `apps/web-next/src/ProfessionalTools.tsx` |
| `PRINTCLASSIC` | Print Layout · mẫu in cơ bản | Xuất bản | `apps/web-next/src/ProfessionalTools.tsx` |
| `MOVESET` | Di chuyển nhiều đối tượng | CAD | `apps/web-next/src/ProfessionalTools.tsx` |
| `COPY` | Sao chép | CAD | `apps/web-next/src/ProfessionalTools.tsx` |
| `OFFSET` | Đường song song | CAD | `apps/web-next/src/ProfessionalTools.tsx` |
| `ROTATE` | Xoay quanh trọng tâm | CAD | `apps/web-next/src/ProfessionalTools.tsx` |
| `SCALE` | Thay đổi kích thước | CAD | `apps/web-next/src/ProfessionalTools.tsx` |
| `STRETCH` | Kéo giãn đỉnh tuyến | CAD | `apps/web-next/src/ProfessionalTools.tsx` |
| `EXTEND` | Kéo dài đoạn cuối | CAD | `apps/web-next/src/ProfessionalTools.tsx` |
| `TRIM` | Cắt đoạn thừa | CAD | `apps/web-next/src/ProfessionalTools.tsx` |
| `FILLET` | Bo góc | CAD | `apps/web-next/src/ProfessionalTools.tsx` |
| `INTERSECTION` | Giao hình học | Phân tích | `apps/web-next/src/ProfessionalTools.tsx` |
| `UNION` | Hợp hình học | Phân tích | `apps/web-next/src/ProfessionalTools.tsx` |
| `DIFFERENCE` | Hiệu hình học | Phân tích | `apps/web-next/src/ProfessionalTools.tsx` |
| `CENTROID` | Trọng tâm | Phân tích | `apps/web-next/src/ProfessionalTools.tsx` |
| `HULL` | Bao lồi | Phân tích | `apps/web-next/src/ProfessionalTools.tsx` |
| `SIMPLIFY` | Đơn giản hình học | Phân tích | `apps/web-next/src/ProfessionalTools.tsx` |
| `NEAREST` | Đối tượng gần nhất | Phân tích | `apps/web-next/src/ProfessionalTools.tsx` |
| `SPJOIN` | Ghép không gian | Phân tích | `apps/web-next/src/ProfessionalTools.tsx` |

### Hotkeys (21) & Command Aliases (22)
- Shortcuts defined in `apps/web-next/src/hotkeys.ts`: 21
- Short aliases in `apps/web-next/src/CoreCommandBar.tsx`: 22

### Client SDK & Spatial Workspace (`retain`)
- `packages/client-sdk/src/` modules: 18 (+ 6 generated OpenAPI schemas)
- Unique API paths in `client-sdk`: 76; direct API paths in `web-next`: 14
- `packages/spatial-workspace/src/` modules: 20

## Mapped Existing Test Coverage (199 files, 438 test IDs)

| Test File | Category | Test IDs | Sample Identifiers |
|---|---|---:|---|
| `apps/web-next/account-smoke.mjs` | `smoke` | 1 | `smoke_entrypoint` |
| `apps/web-next/activation-migration-smoke.mjs` | `smoke` | 1 | `smoke_entrypoint` |
| `apps/web-next/advanced-editor-smoke.mjs` | `smoke` | 1 | `smoke_entrypoint` |
| `apps/web-next/ai-copilot-integration-smoke.mjs` | `smoke` | 1 | `smoke_entrypoint` |
| `apps/web-next/auth-modal-style-smoke.mjs` | `smoke` | 1 | `smoke_entrypoint` |
| `apps/web-next/auth-theme-smoke.mjs` | `smoke` | 1 | `smoke_entrypoint` |
| `apps/web-next/basemap-smoke.mjs` | `smoke` | 1 | `smoke_entrypoint` |
| `apps/web-next/boundary-nav-smoke.mjs` | `smoke` | 1 | `smoke_entrypoint` |
| `apps/web-next/brand-smoke.mjs` | `smoke` | 1 | `smoke_entrypoint` |
| `apps/web-next/cad-editor-georef-smoke.mjs` | `smoke` | 1 | `smoke_entrypoint` |
| `apps/web-next/cad-spatial-linkage-smoke.mjs` | `smoke` | 1 | `smoke_entrypoint` |
| `apps/web-next/cesium-tiles-smoke.mjs` | `smoke` | 1 | `smoke_entrypoint` |
| `apps/web-next/cloud-google-smoke.mjs` | `smoke` | 1 | `smoke_entrypoint` |
| `apps/web-next/cloud-modal-smoke.mjs` | `smoke` | 1 | `smoke_entrypoint` |
| `apps/web-next/cloud-public-modal-smoke.mjs` | `smoke` | 1 | `smoke_entrypoint` |
| `apps/web-next/command-floating-smoke.mjs` | `smoke` | 1 | `smoke_entrypoint` |
| `apps/web-next/core-plugin-smoke.mjs` | `smoke` | 1 | `smoke_entrypoint` |
| `apps/web-next/core-toolkit-smoke.mjs` | `smoke` | 1 | `smoke_entrypoint` |
| `apps/web-next/crs-display.test.mjs` | `unit` | 3 | `UTM 48N: central meridian, meridian arc and symmetry`, `Web Mercator origin and geographic pass-through`, `VN-2000 TM-3 differs from WGS 84 TM-3 by the datum shift only (tens to a few hundred metres)` |
| `apps/web-next/desktop-dialog-smoke.mjs` | `smoke` | 1 | `smoke_entrypoint` |
| `apps/web-next/developer-portal-smoke.mjs` | `smoke` | 1 | `smoke_entrypoint` |
| `apps/web-next/domain-switch-smoke.mjs` | `smoke` | 1 | `smoke_entrypoint` |
| `apps/web-next/dsm-sources.test.mjs` | `unit` | 1 | `csv with quotes, semicolons and decimal commas becomes points` |
| `apps/web-next/earth-navigation-smoke.mjs` | `smoke` | 1 | `smoke_entrypoint` |
| `apps/web-next/earth-orbit-smoke.mjs` | `smoke` | 1 | `smoke_entrypoint` |
| `apps/web-next/export-formats.test.mjs` | `unit` | 1 | `writers keep geometry and escape text` |
| `apps/web-next/external-plugin-smoke.mjs` | `smoke` | 1 | `smoke_entrypoint` |
| `apps/web-next/geom-measure.test.mjs` | `unit` | 2 | `1 km x 1 km square near the equator ≈ 1 000 000 m²`, `line length and point` |
| `apps/web-next/graduated.test.mjs` | `unit` | 1 | `equal and quantile breaks, class lookup and colour ramp` |
| `apps/web-next/hotkeys.test.mjs` | `unit` | 1 | `combos map to QGIS-style shortcuts` |
| `apps/web-next/house-numbering-address-batch-smoke.mjs` | `smoke` | 1 | `smoke_entrypoint` |
| `apps/web-next/house-numbering-address-suggestion-smoke.mjs` | `smoke` | 1 | `smoke_entrypoint` |
| `apps/web-next/house-numbering-address-verification-smoke.mjs` | `smoke` | 1 | `smoke_entrypoint` |
| `apps/web-next/house-numbering-audit-smoke.mjs` | `smoke` | 1 | `smoke_entrypoint` |
| `apps/web-next/house-numbering-batch-smoke.mjs` | `smoke` | 1 | `smoke_entrypoint` |
| `apps/web-next/house-numbering-catalog-smoke.mjs` | `smoke` | 1 | `smoke_entrypoint` |
| `apps/web-next/house-numbering-correction-smoke.mjs` | `smoke` | 1 | `smoke_entrypoint` |
| `apps/web-next/house-numbering-geometry-review-smoke.mjs` | `smoke` | 1 | `smoke_entrypoint` |
| `apps/web-next/house-numbering-numbering-audit-smoke.mjs` | `smoke` | 1 | `smoke_entrypoint` |
| `apps/web-next/house-numbering-prefix-smoke.mjs` | `smoke` | 1 | `smoke_entrypoint` |
| `apps/web-next/house-numbering-proposal-packet-smoke.mjs` | `smoke` | 1 | `smoke_entrypoint` |
| `apps/web-next/house-numbering-qr-navigation-smoke.mjs` | `smoke` | 1 | `smoke_entrypoint` |
| `apps/web-next/house-numbering-recovery-smoke.mjs` | `smoke` | 1 | `smoke_entrypoint` |
| `apps/web-next/house-numbering-survey-review-smoke.mjs` | `smoke` | 1 | `smoke_entrypoint` |
| `apps/web-next/house-numbering-topology-smoke.mjs` | `smoke` | 1 | `smoke_entrypoint` |
| `apps/web-next/house-numbering-verified-link-smoke.mjs` | `smoke` | 1 | `smoke_entrypoint` |
| `apps/web-next/house-numbering-workbench-smoke.mjs` | `smoke` | 1 | `smoke_entrypoint` |
| `apps/web-next/house-qr-link-smoke.mjs` | `smoke` | 1 | `smoke_entrypoint` |
| `apps/web-next/integration-hub-smoke.mjs` | `smoke` | 1 | `smoke_entrypoint` |
| `apps/web-next/invite-smoke.mjs` | `smoke` | 1 | `smoke_entrypoint` |
| `apps/web-next/label-expr.test.mjs` | `unit` | 1 | `label expression functions` |
| `apps/web-next/landing-google-smoke.mjs` | `smoke` | 1 | `smoke_entrypoint` |
| `apps/web-next/landing-modal-smoke.mjs` | `smoke` | 1 | `smoke_entrypoint` |
| `apps/web-next/landing-swarm-smoke.mjs` | `smoke` | 1 | `smoke_entrypoint` |
| `apps/web-next/landing-v3-smoke.mjs` | `smoke` | 1 | `smoke_entrypoint` |
| `apps/web-next/layer-properties-smoke.mjs` | `smoke` | 1 | `smoke_entrypoint` |
| `apps/web-next/layer-tree.test.mjs` | `unit` | 2 | `groups nest by path and sit at their first layer's position`, `rename moves sub-groups; ungroup goes one level up` |
| `apps/web-next/layout-smoke.mjs` | `smoke` | 1 | `smoke_entrypoint` |
| `apps/web-next/live-smoke.mjs` | `smoke` | 1 | `smoke_entrypoint` |
| `apps/web-next/logout-switch-smoke.mjs` | `smoke` | 1 | `smoke_entrypoint` |
| `apps/web-next/map-progressive-smoke.mjs` | `smoke` | 1 | `smoke_entrypoint` |
| `apps/web-next/model-vault-smoke.mjs` | `smoke` | 1 | `smoke_entrypoint` |
| `apps/web-next/modeler-smoke.mjs` | `smoke` | 1 | `smoke_entrypoint` |
| `apps/web-next/multi-model-smoke.mjs` | `smoke` | 1 | `smoke_entrypoint` |
| `apps/web-next/native-dxf-smoke.mjs` | `smoke` | 1 | `smoke_entrypoint` |
| `apps/web-next/native-dxf.test.mjs` | `unit` | 3 | `create real R12 line, circle, point and edit coordinates without changing other groups`, `existing imported synthetic CAD is viewable, never silently rewritten`, `unsafe or unsupported drawings fail closed` |
| `apps/web-next/opencad-production-smoke.mjs` | `smoke` | 1 | `smoke_entrypoint` |
| `apps/web-next/org-chat-smoke.mjs` | `smoke` | 1 | `smoke_entrypoint` |
| `apps/web-next/partner-contributions-smoke.mjs` | `smoke` | 1 | `smoke_entrypoint` |
| `apps/web-next/partner-gis-smoke.mjs` | `smoke` | 1 | `smoke_entrypoint` |
| `apps/web-next/partner-platform-smoke.mjs` | `smoke` | 1 | `smoke_entrypoint` |
| `apps/web-next/professional-smoke.mjs` | `smoke` | 1 | `smoke_entrypoint` |
| `apps/web-next/qgis-workspace-smoke.mjs` | `smoke` | 1 | `smoke_entrypoint` |
| `apps/web-next/renderer-boundary-smoke.mjs` | `smoke` | 1 | `smoke_entrypoint` |
| `apps/web-next/run-canonical-smoke.mjs` | `smoke` | 1 | `smoke_entrypoint` |
| `apps/web-next/sample-smoke.mjs` | `smoke` | 1 | `smoke_entrypoint` |
| `apps/web-next/saved-scene-smoke.mjs` | `smoke` | 1 | `smoke_entrypoint` |
| `apps/web-next/single-frontend-processing-smoke.mjs` | `smoke` | 1 | `smoke_entrypoint` |
| `apps/web-next/single-frontend-route-smoke.mjs` | `smoke` | 1 | `smoke_entrypoint` |
| `apps/web-next/smoke.mjs` | `smoke` | 1 | `smoke_entrypoint` |
| `apps/web-next/snap-math-smoke.mjs` | `smoke` | 1 | `smoke_entrypoint` |
| `apps/web-next/space-layout-smoke.mjs` | `smoke` | 1 | `smoke_entrypoint` |
| `apps/web-next/spatial-cache-smoke.mjs` | `smoke` | 1 | `smoke_entrypoint` |
| `apps/web-next/spatial-mode-smoke.mjs` | `smoke` | 1 | `smoke_entrypoint` |
| `apps/web-next/spatial-performance.test.mjs` | `unit` | 1 | `bounded anonymous timings and percentiles` |
| `apps/web-next/spatial-sync-core.test.mjs` | `unit` | 4 | `exact revision chain covers missed events in order`, `incomplete, oversized, reverted and cyclic chains require full layer reload`, `initial and incremental reads respect concurrency bound`, `one selection / edit among 35000 points produces one render update` |
| `apps/web-next/src/route-compat.test.ts` | `unit` | 2 | `ignores unknown and prototype names`, `rejects malformed encoding` |
| `apps/web-next/src/workbenches/app/protected-query-cache.test.ts` | `unit` | 2 | `does not expose a warm successful payload while a real mount refetch is unsettled or denied`, `does not expose warm data during a paused refetch and only restores a new settled payload` |
| `apps/web-next/src/workbenches/app/protected-query.test.ts` | `unit` | 4 | `renders only a successful, settled read`, `fails closed even if a result unexpectedly has both an error and success`, `isolates identity link caches between signed-in actors`, `always revalidates protected data on remount, focus and reconnect` |
| `apps/web-next/src/workbenches/app/session/return-destination.test.ts` | `unit` | 3 | `accepts an application-relative destination`, `rejects protocol-relative external destinations`, `rejects absolute external destinations` |
| `apps/web-next/src/workbenches/features/offline/offline-store.test.ts` | `unit` | 4 | `requires complete, scope-matched projection and GeoPackage checksums`, `queues a stable group and prohibits intent mutation after attempted submission`, `normalizes decimal precision without floating point coercion`, `enforces type/nullability` |
| `apps/web-next/src/workbenches/features/platform/editing-geometry.test.ts` | `unit` | 7 | `preserves original coordinate units through the viewport transform`, `closes a simple ring only on explicit sketch completion and rejects degenerate drafts`, `rejects unsupported dimensions, unclosed rings, and unsupported geometry`, `refuses graphical modification of a polygon with holes rather than silently dropping rings` (+3 more) |
| `apps/web-next/src/workbenches/features/platform/platform-release-state.test.ts` | `unit` | 5 | `does not assert production acceptance for absent or unverified evidence`, `rejects even optimistic enabled/status flags without a pinned verification`, `distinguishes unsupported, scope-disabled and unverified capabilities`, `does not advertise provider retrieval for configured or revoked connections` (+1 more) |
| `apps/web-next/src/workbenches/features/platform/query-filters.test.ts` | `unit` | 5 | `composes typed AND/NOT conditions without pagination or pin fields`, `uses literal empty text, typed null, and OR`, `canonicalizes exact decimals without conversion through Number`, `supports an unfiltered query and a reference-layer relation` (+1 more) |
| `apps/web-next/style-io.test.mjs` | `unit` | 2 | `SLD: single, categorized and graduated rules with escaped values`, `QML: renderer types and category / range lists` |
| `apps/web-next/survey-math-smoke.mjs` | `smoke` | 1 | `smoke_entrypoint` |
| `apps/web-next/survey-nav-smoke.mjs` | `smoke` | 1 | `smoke_entrypoint` |
| `apps/web-next/survey-terrain-smoke.mjs` | `smoke` | 1 | `smoke_entrypoint` |
| `apps/web-next/team-collaboration-smoke.mjs` | `smoke` | 1 | `smoke_entrypoint` |
| `apps/web-next/terra-draft-validation-smoke.mjs` | `smoke` | 1 | `smoke_entrypoint` |
| `apps/web-next/terrain-engineering-smoke.mjs` | `smoke` | 1 | `smoke_entrypoint` |
| `apps/web-next/terrain-model-smoke.mjs` | `smoke` | 1 | `smoke_entrypoint` |
| `apps/web-next/unified-gis-smoke.mjs` | `smoke` | 1 | `smoke_entrypoint` |
| `apps/web-next/workspace-access-smoke.mjs` | `smoke` | 1 | `smoke_entrypoint` |
| `apps/web-next/workspace-flex-smoke.mjs` | `smoke` | 1 | `smoke_entrypoint` |
| `apps/web-next/workspace-latest-smoke.mjs` | `smoke` | 1 | `smoke_entrypoint` |
| `apps/web-next/zip.test.mjs` | `unit` | 2 | `crc32 matches the standard check value`, `zip has readable entries with UTF-8 names and unique file names` |
| `apps/web/src/app/protected-query-cache.test.ts` | `unit` | 2 | `does not expose a warm successful payload while a real mount refetch is unsettled or denied`, `does not expose warm data during a paused refetch and only restores a new settled payload` |
| `apps/web/src/app/protected-query.test.ts` | `unit` | 4 | `renders only a successful, settled read`, `fails closed even if a result unexpectedly has both an error and success`, `isolates identity link caches between signed-in actors`, `always revalidates protected data on remount, focus and reconnect` |
| `apps/web/src/app/router.test.tsx` | `unit` | 3 | `maps organization routes to canonical /app routes`, `maps every project capability route without losing ids`, `maps static identity and invitation routes` |
| `apps/web/src/app/session/SessionBoundary.test.tsx` | `unit` | 2 | `keeps authenticated shell content mounted during background /me revalidation`, `redirects to login with a return destination after confirmed 401` |
| `apps/web/src/app/session/return-destination.test.ts` | `unit` | 3 | `accepts an application-relative destination`, `rejects protocol-relative external destinations`, `rejects absolute external destinations` |
| `apps/web/src/app/session/session-state.test.ts` | `unit` | 3 | `keeps a confirmed account while a background check runs`, `clears the account on confirmed authorization loss`, `keeps chrome identity after a transient revalidation failure` |
| `apps/web/src/features/auth/AccountMenu.test.tsx` | `unit` | 2 | `exposes only safe account display fields`, `represents ambiguous network revalidation without claiming logout` |
| `apps/web/src/features/dashboard/AppDashboard.test.tsx` | `unit` | 3 | `renders human-readable organization and project cards without primary UUID labels`, `renders bootstrap gate for an uninitialized installation`, `renders a useful no-project empty state` |
| `apps/web/src/features/map/FeatureInspector.test.tsx` | `unit` | 2 | `shows observed feature response revisions only inside a secondary labeled details surface`, `does not disclose old detail revisions after selection or authorization is lost` |
| `apps/web/src/features/map/accessibility-summary.test.ts` | `unit` | 3 | `summarizes project, active layer, visible and selected counts`, `handles missing active/focused data without inventing details`, `does not expose hidden layer names as the active layer` |
| `apps/web/src/features/map/add-data.test.ts` | `unit` | 7 | `marks existing references without offering a duplicate add`, `offers an authorized renderable catalog layer`, `distinguishes permission denial from unsupported rendering`, `never uses a closed, pending, paused or denied catalog payload` (+3 more) |
| `apps/web/src/features/map/commands.test.ts` | `unit` | 4 | `shares execution and availability across every UI entry point`, `separates current permission from technical capability and blocks unknown operations`, `offers only current named layers and permitted access navigation`, `uses one command model for layer zoom, reorder, visibility and remove` |
| `apps/web/src/features/map/feature-presentation.test.ts` | `unit` | 2 | `localizes all spatial geometry enum states and does not invent a known state`, `distinguishes missing, explicit null, empty and zero without value coercion` |
| `apps/web/src/features/map/layer-view.test.ts` | `unit` | 5 | `distinguishes permission denial from unsupported capability`, `does not show stale or invented layer names after a denied or mismatched read`, `preserves partial and stale states explicitly`, `distinguishes viewport loading and empty from a usable layer` (+1 more) |
| `apps/web/src/features/map/map-engine-loader.test.ts` | `unit` | 2 | `does not eagerly request a renderer; concurrent callers share one import`, `does not keep a permanently rejected loader promise` |
| `apps/web/src/features/map/map-initialization.test.ts` | `unit` | 2 | `suppresses duplicate camera events but not any actual camera component change`, `retains the existing viewport jitter tolerance without guessing initial bounds` |
| `apps/web/src/features/map/maplibre-engine.test.ts` | `unit` | 7 | `permits only the local grid and cannot construct remote raster sources even through a direct adapter call`, `loads three geometry sources once and leaves an identical settled frame alone`, `installs all three sources directly in exact draw order, without redundant moves`, `detects in-place changes to untrusted mutable inputs rather than assuming identity is safe` (+3 more) |
| `apps/web/src/features/map/render-frame.test.ts` | `unit` | 4 | `prepares an exact authorized response once within the same success generation`, `does not reuse prior actor/pin context and never returns geometry for a delayed or mismatched response`, `copies exact geometry and creates a fresh immutable frame on every new success generation`, `copies line and polygon vertices exactly, rejects malformed current geometry without returning a frame` |
| `apps/web/src/features/map/responsive-mode.test.ts` | `unit` | 2 | `uses the accepted desktop/tablet/mobile thresholds`, `fails safely to mobile for invalid widths` |
| `apps/web/src/features/map/save-indicator.test.ts` | `unit` | 2 | `only claims saved after an authoritative committed outcome`, `keeps conflict and unresolved outcomes honest` |
| `apps/web/src/features/map/save.test.ts` | `unit` | 2 | `retains exact operation and intent through ambiguous transport and retries`, `never transforms conflicts, denial or known validation failures into a fresh retry` |
| `apps/web/src/features/map/technical-details.test.ts` | `unit` | 3 | `exposes only scoped technical facts needed for support/debugging`, `distinguishes capability support, permission and availability`, `does not invent active-layer metadata when no layer is selected` |
| `apps/web/src/features/map/workspace-layout.test.ts` | `unit` | 3 | `normalizes every canonical field and clamps panel dimensions`, `falls back safely for invalid persisted values`, `persists only normalized non-sensitive layout data` |
| `apps/web/src/features/map/workspace.test.ts` | `unit` | 7 | `clears previously authorized render buffers while any protected viewport is loading`, `distinguishes settled invalid outer map scope from active loading and denial`, `binds selections to actor, tenant, logical layer, source and independent pins`, `retains selected IDs across pages, caps at100 and toggles without substitution` (+3 more) |
| `apps/web/src/features/offline/offline-store.test.ts` | `unit` | 4 | `requires complete, scope-matched projection and GeoPackage checksums`, `queues a stable group and prohibits intent mutation after attempted submission`, `normalizes decimal precision without floating point coercion`, `enforces type/nullability` |
| `apps/web/src/features/platform/editing-geometry.test.ts` | `unit` | 7 | `preserves original coordinate units through the viewport transform`, `closes a simple ring only on explicit sketch completion and rejects degenerate drafts`, `rejects unsupported dimensions, unclosed rings, and unsupported geometry`, `refuses graphical modification of a polygon with holes rather than silently dropping rings` (+3 more) |
| `apps/web/src/features/platform/platform-release-state.test.ts` | `unit` | 5 | `does not assert production acceptance for absent or unverified evidence`, `rejects even optimistic enabled/status flags without a pinned verification`, `distinguishes unsupported, scope-disabled and unverified capabilities`, `does not advertise provider retrieval for configured or revoked connections` (+1 more) |
| `apps/web/src/features/platform/query-filters.test.ts` | `unit` | 5 | `composes typed AND/NOT conditions without pagination or pin fields`, `uses literal empty text, typed null, and OR`, `canonicalizes exact decimals without conversion through Number`, `supports an unfiltered query and a reference-layer relation` (+1 more) |
| `apps/web/src/features/spatial/catalog-history.test.ts` | `unit` | 6 | `isolates account, organization, project and server continuation under the mutation prefix`, `requires current exact Project metadata, permits archived reads and hides deleted scope`, `rejects mismatched outer or item scopes, oversized, unordered and invalid continuation pages`, `moves only via server continuation, returns to the exact previous cursor` (+2 more) |
| `apps/web/src/features/spatial/crs-profiles.test.ts` | `unit` | 4 | `offers native and direct approved edges only, never definition-only or graph transit`, `isolates discovery cache and checks both Project scope IDs`, `separates transformed cache keys and restarts pagination while retaining stable selection`, `requires exact operation evidence and target axes before exposing transformed data` |
| `apps/web/src/features/spatial/feature-inspector.test.ts` | `unit` | 8 | `isolates every protected scope and pin, and separates pages from exact detail IDs`, `uses server continuations with reversible bounded history, never chooses an implicit ID`, `never carries a previous actor, project, Layer or source's selection across scope changes`, `checks the response scope, both pins and native CRS before exposing a payload` (+4 more) |
| `packages/client-sdk/tests/auth-session.test.ts` | `unit` | 3 | `loads providers, current user, and organizations`, `loads CSRF then sends it on organization creation`, `reuses the cached CSRF token and refreshes it once after a 403` |
| `packages/client-sdk/tests/bootstrap.test.ts` | `unit` | 1 | `sends authority only in the explicit initialization header with CSRF` |
| `packages/client-sdk/tests/grants-invitations.test.ts` | `unit` | 4 | `keeps membership and grant operations explicitly organization scoped`, `encodes both permission-context IDs and returns server permission codes`, `issues once, sends the secret in a POST body only, and uses CSRF`, `does not swallow forbidden mutations, even with bodyless response handling` |
| `packages/client-sdk/tests/http.test.ts` | `unit` | 5 | `sends cookies and CSRF on unsafe requests`, `does not add CSRF to safe requests`, `keeps a single configured base for REST and event URLs`, `rejects ambiguous or already-prefixed paths before sending` (+1 more) |
| `packages/client-sdk/tests/integration-bindings.test.ts` | `unit` | 4 | `lists project drafts without a write method or bearer token`, `fetches project-scoped eligible QGIS fields only`, `creates only disabled draft metadata with CSRF, never credentials`, `pauses with bound revision, never offers activate` |
| `packages/client-sdk/tests/integration-hub.test.ts` | `unit` | 12 | `fetches project catalog via GET with encoded path parameters and preserves truthful policy fields`, `fetches adapter profile via GET with encoded adapterKey and preserves sync policy flags and layer metadata`, `adds repeated commune/hamlet and route scope parameters to an export path`, `builds URL-encoded read-only layer export paths for qgis, postgis, and autocad dxf` (+8 more) |
| `packages/client-sdk/tests/managed-sync.test.ts` | `unit` | 1 | `uses one API prefix and correct methods, identifiers, and CSRF` |
| `packages/client-sdk/tests/maps.test.ts` | `unit` | 8 | `uses the mandatory generated wire schemas`, `encodes each scope/operation component and never writes while reading`, `sends an exact explicit save intent with PUT and CSRF; returns only the receipt`, `preserves explicit set/clear/preserve viewpoint choices instead of substituting camera` (+4 more) |
| `packages/client-sdk/tests/platform.test.ts` | `unit` | 3 | `constructs protected download URLs and invokes R006/R007/R008/R015 endpoints`, `keeps project scope, active manifest and expected record revision in protected requests`, `requests scoped preview and submits its exact output settings and digest` |
| `packages/client-sdk/tests/service-principals.test.ts` | `unit` | 1 | `keeps management tenant scoped and returns a credential secret once` |
| `packages/client-sdk/tests/spatial-changes.test.ts` | `unit` | 3 | `exports the generated wire types and preserves retired resource evidence`, `encodes the project scope and only sends explicitly supplied pagination`, `preserves a recognized fail-closed spatial error code without retry` |
| `packages/client-sdk/tests/spatial-crs.test.ts` | `unit` | 3 | `encodes the Project scope and preserves definition-only profiles and operation evidence`, `keeps discovery current permission errors typed`, `sends only the explicitly selected response CRS with the original revision pins` |
| `packages/client-sdk/tests/spatial-features.test.ts` | `unit` | 3 | `requires both pins and uses generated page/detail response types`, `encodes each scoped ID and cursor; sends both pins and only explicit options`, `preserves pinned context, exact typed attributes and the omitted geometry property` |
| `packages/client-sdk/tests/spatial-layers.test.ts` | `unit` | 2 | `encodes scope, cursor and layer ID; preserves the exact mutation wire shape`, `preserves independent stale layer conflict` |
| `packages/client-sdk/tests/spatial.test.ts` | `unit` | 3 | `always encodes both scope IDs and the source ID and preserves the cursor`, `uses exact mutation shape and existing CSRF transport`, `preserves typed spatial conflicts without swallowing R002 authorization errors` |
| `packages/client-sdk/tests/tenant-foundation.test.ts` | `unit` | 1 | `checks readiness and creates and reads a real tenant record using explicit IDs` |
| `packages/client-sdk/tests/transfers.test.ts` | `unit` | 3 | `uploads multipart asset without forcing application/json header`, `wraps structured transfer errors with code and detail`, `confirms import/export, lists jobs, and downloads verified artifact bytes` |
| `packages/client-sdk/tests/workspace-project.test.ts` | `unit` | 2 | `uses organization-scoped collection paths`, `preserves omitted-vs-null workspace move semantics` |
| `packages/spatial-workspace/tests/browser/3d-smoke.mjs` | `smoke` | 1 | `smoke_entrypoint` |
| `packages/spatial-workspace/tests/browser/cad-draft-smoke.mjs` | `smoke` | 1 | `smoke_entrypoint` |
| `packages/spatial-workspace/tests/browser/cad-selection-smoke.mjs` | `smoke` | 1 | `smoke_entrypoint` |
| `packages/spatial-workspace/tests/browser/cad-smoke.mjs` | `smoke` | 1 | `smoke_entrypoint` |
| `packages/spatial-workspace/tests/browser/generator-smoke.mjs` | `smoke` | 1 | `smoke_entrypoint` |
| `packages/spatial-workspace/tests/browser/multi-scene-smoke.mjs` | `smoke` | 1 | `smoke_entrypoint` |
| `packages/spatial-workspace/tests/browser/scene-smoke.mjs` | `smoke` | 1 | `smoke_entrypoint` |
| `packages/spatial-workspace/tests/browser/selection-smoke.mjs` | `smoke` | 1 | `smoke_entrypoint` |
| `packages/spatial-workspace/tests/browser/smoke.mjs` | `smoke` | 1 | `smoke_entrypoint` |
| `packages/spatial-workspace/tests/browser/tiles-smoke.mjs` | `smoke` | 1 | `smoke_entrypoint` |
| `packages/spatial-workspace/tests/cad-georeference.test.mjs` | `unit` | 4 | `millimetre CAD rotation and translation with verified residual`, `missing/degree CRS rejected`, `wrong unit and collinear points rejected`, `inconsistent control points and insufficient count rejected` |
| `packages/spatial-workspace/tests/coordinator.test.mjs` | `unit` | 9 | `only prepared selected mode activates; prior engine disposed`, `stale completion is disposed, not activated`, `scope change aborts and disposes previous active engine`, `revocation and edits during loading prevent activation` (+5 more) |
| `packages/spatial-workspace/tests/footprint3d.test.mjs` | `unit` | 4 | `explicit base and height form actual top, identity retained`, `missing or invalid heights/duplicate ids rejected`, `non-4326 or unspecified vertical reference rejected`, `unclosed polygon and geographic out of range rejected` |
| `packages/spatial-workspace/tests/mesh-picking.test.mjs` | `unit` | 2 | `triangle hit uses nearest projected depth independent of winding`, `scene budget bounded on count and aggregate triangles` |
| `packages/spatial-workspace/tests/session.test.mjs` | `unit` | 11 | `GIS is default; reject incomplete scope`, `capability denial preserves current mode`, `dirty and saving block request`, `load is requested without activating mode until ready` (+7 more) |
| `packages/spatial-workspace/tests/shared-math.test.mjs` | `unit` | 2 | `snap is deterministic, bounded, and handles empty input`, `survey math preserves projected units and rejects invalid coordinates` |
| `packages/spatial-workspace/tests/terrain-analysis.test.mjs` | `unit` | 15 | `flat reference plane cut/fill, metric area near equator`, `cut-fill crossing triangle integrated, net zero`, `slope and aspect on planar DEM, missing cells remain null`, `D8 strictly downhill, no cycles, accumulation reaches terminal` (+11 more) |
| `packages/spatial-workspace/tests/terrain-model.test.mjs` | `unit` | 3 | `terrain validation, negative metres preserved, same triangle interpolation, holes and outside`, `GLB and self-contained GLTF same transformed indices, material and geometry`, `GLB corrupt header and unsupported data rejected` |
| `packages/spatial-workspace/tests/tileset.test.mjs` | `unit` | 3 | `region bounds, REPLACE refinement, capped deterministic selection`, `explicit ECEF transform, glTF y-up and WGS84 ellipsoid control points`, `reject unsupported URI, refinement and nested bounds` |
| `tests/e2e/app-shell-accessibility.spec.ts` | `playwright_e2e` | 3 | `public shell exposes skip navigation and visible keyboard focus`, `authenticated shell stays within viewport at reference widths`, `primary account and command controls meet minimum touch target` |
| `tests/e2e/app-shell-visual.spec.ts` | `playwright_e2e` | 3 | `landing visual baseline`, `login visual baseline`, `authenticated dashboard and account menu visual baselines` |
| `tests/e2e/app-shell.spec.ts` | `playwright_e2e` | 3 | `background /me revalidation keeps authenticated shell mounted`, `authenticated shell persists across organization and project navigation`, `legacy organization route redirects to canonical /app route` |
| `tests/e2e/auth-navigation.spec.ts` | `playwright_e2e` | 4 | `public landing and dedicated login are separate surfaces`, `protected deep link returns to the intended canonical destination`, `external return_to is rejected and falls back to /app`, `sign out from a deep route invalidates the application session` |
| `tests/e2e/bootstrap-service-identity.spec.ts` | `playwright_e2e` | 1 | `authenticates through OIDC and initializes the first organization exactly once` |
| `tests/e2e/extension-lifecycle.spec.ts` | `playwright_e2e` | 1 | `R014 actual browser activates, edits, exports and retains extension history` |
| `tests/e2e/house-numbering.spec.ts` | `playwright_e2e` | 1 | `House Numbering official module lifecycle and seven UI journeys` |
| `tests/e2e/import-export-m1.spec.ts` | `playwright_e2e` | 1 | `M1 end-to-end: import GeoJSON, view/select/save in Map Workspace, export fixed-revision GeoPackage and download` |
| `tests/e2e/invitation-revocation.spec.ts` | `playwright_e2e` | 1 | `recipient-bound invitation grants one project and revocation stops the next protected request` |
| `tests/e2e/map-large-reference-renderer.spec.ts` | `playwright_e2e` | 1 | `three real 10000-feature reference layers settle and pick a real rendered feature` |
| `tests/e2e/map-performance.spec.ts` | `playwright_e2e` | 1 | `measures 100 actual map-open and select-to-authorized-inspector trials on the 10000-feature Core profile` |
| `tests/e2e/map-protected-regression.spec.ts` | `playwright_e2e` | 2 | `a delayed detail result cannot replace a new layer selection and reconnect revocation clears a warm catalog`, `one mismatched metadata response is concealed without blocking the other authorized references` |
| `tests/e2e/map-workspace-accessibility.spec.ts` | `playwright_e2e` | 1 | `supports keyboard and textual alternatives across the GIS workspace` |
| `tests/e2e/map-workspace-responsive.spec.ts` | `playwright_e2e` | 1 | `adapts GIS workspace across desktop tablet and mobile reference widths` |
| `tests/e2e/map-workspace-visual.spec.ts` | `playwright_e2e` | 4 | `empty GIS workspace visual baseline`, `populated GIS workspace visual baseline`, `technical drawer closed and open visual baselines`, `dirty saved and conflict save-state visual baselines` |
| `tests/e2e/map-workspace.spec.ts` | `playwright_e2e` | 8 | `opens a real empty grid and arranges, renders, selects and saves catalog Layers without changing data`, `technical details drawer returns focus and clears on active-layer scope change`, `preserves the designated camera and reconciles a genuinely committed lost acknowledgement after a later save`, `two real actors receive CAS conflicts and revoked access clears a warm rendered selection` (+4 more) |
| `tests/e2e/native-editing.spec.ts` | `playwright_e2e` | 2 | `native graphical point drawing, visual snap, revision-pinned update/delete/revert and stale draft`, `native line and polygon sketches are committed and reread as original CRS geometry` |
| `tests/e2e/platform-m2-m3.spec.ts` | `playwright_e2e` | 1 | `M2+M3 end-to-end: R006 Editing, R007 Query, R008 Processing, R009 Cartography, and R010–R015 Hub` |
| `tests/e2e/protected-identity-membership.spec.ts` | `playwright_e2e` | 4 | `identity links never render a private cached payload during or after a denied refetch`, `unlinking the active identity revalidates the account even when the DELETE succeeds`, `membership administration is unmounted while prior permissions are revalidated`, `invitation acceptance keeps the shell stable during account refetch and preserves login return-to on denial` |
| `tests/e2e/r013-offline-real.spec.ts` | `playwright_e2e` | 1 | `R013 real OIDC + PostGIS: disconnected durable group, lost server acknowledgment, conflict and revocation` |
| `tests/e2e/spatial-catalog.spec.ts` | `playwright_e2e` | 3 | `registers and inspects stable pinned features, explicit CRS and immutable catalog history`, `blocks stale feature payloads until explicit retry and rejects a mismatched Project URL`, `removes a warm catalog before a real deleted-Project refetch returns` |
| `tests/e2e/tenant-core.spec.ts` | `playwright_e2e` | 2 | `logs in, creates organizations, and switches explicit organization context`, `direct tenant URL substitution cannot resolve another organization's project` |
| `tests/e2e/tenant-foundation.spec.ts` | `playwright_e2e` | 1 | `checks readiness and reloads a persisted real tenant project` |
| `tools/tests/test_canonical_frontend_gate.py` | `unit` | 3 | `test_ci_validation_builds_canonical_production_frontend`, `test_canonical_frontend_has_build_script`, `test_ci_runs_built_workspace_and_mcp_regression` |

## E17-AC1 Completion Evidence

- [x] All runtime routes / tools / commands / connectors / scripts extracted directly from source and live FastAPI app introspection (no guessed routes).
- [x] Every source artifact and operation carries an explicit classification (`migrate` | `retain` | `replace` | `approved_retire`).
- [x] Auth, CSRF, OAuth, and tenant/project isolation boundaries documented per operation.
- [x] Existing unit, integration, contract, E2E, and smoke test files and test IDs mapped in `contracts/engines/e17.json`.
