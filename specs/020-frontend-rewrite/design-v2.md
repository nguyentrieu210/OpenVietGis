# OpenVietGIS 020 — Studio V2: Persistent map workspace

Owner feedback: "các tính năng là tích hợp và tương tác thẳng trên bản đồ ... chọn 1 điểm có thể di chuyển tọa độ ... thêm dữ liệu ... bằng modal chứ k nhảy trang". V2 supersedes V1 desktop Studio shell and separate tool pages as a UX direction; auth/team/project hub remain outside Studio. Owner visual approval still open. Do not implement production in this design step.

## Figma prototype

[Studio V2 — start](https://www.figma.com/proto/x2P6nknCfXyklDuiGEh83e?node-id=8-2026) | [Editable design](https://www.figma.com/design/x2P6nknCfXyklDuiGEh83e?node-id=8-2026)

| State | Figma node | Behavior |
|---|---|---|
| Base | 8:2026 | Persistent canvas, floating layer tree, compact tools, map points |
| Selected | 8:2745 | Click address 128, open contextual coordinate inspector |
| Moved | 8:3489 | New location preview; dirty transaction, previous position |
| Saved | 8:4234 | Save confirmation |
| Added | 8:4977 | New layer and sample points on same map |
| Add data | 8:5709 | True Figma modal overlay on canvas, sample file/CRS/name |
| Edit coordinate | 8:5739 | Modal with X/Y and explicit CRS |
| Query | 8:5759 | Overlay, select result on map |
| CAD / COGO | 8:5778 | Overlay, preview precision edit |
| Plugin | 8:5797 | Overlay, selected road context |
| Attributes | 8:5812 | Overlay table, row-to-map selection |
| Export | 8:5831 | Overlay for format and extent |

12 new frames, 74 verified prototype reactions; 7 overlay entry actions per workspace state. Editable schematic vector basemap, reusable shell, icon components, semantic variables; selected frame visually verified and all text uses Be Vietnam Pro. The schematic is not actual geographic data. Figma Starter rate limit blocked final drag wiring and rewiring existing hub/screen map links. Click "Di chuyển" and coordinate edit simulate movement; direct drag interaction is not yet verified or wired. Existing old entry links still lead to V1; use specific V2 link above. Prototype inputs and GIS results are illustrative, no real persistence or API.

## Interaction contract (mandatory for web-next)

A single project Studio route mounts one persistent map instance; command operations open in map context. No remount, route navigation, viewport reset, map reload, selection loss, or clearing unrelated unsaved state. Project/tenant switch, public auth, and project administrative shell are exceptions. URL deep links may encode UI state without remounting.

| Action | UI surface | Required result |
|---|---|---|
| Select point/line/polygon | Canvas hit test, table row, search | Bind one shared selected feature ID and geometry; draw highlight/vertex handles; inspector contextual |
| Move point | Drag handle, move command, coordinate edit | Live position/coordinate preview; explicit confirm, undo/redo and save; cancel/Esc restores origin; prevent pan conflict |
| Edit line/polygon | Vertex handles and drawing tools | Add/remove/move vertices; snap and topology validity; preview before revision commit |
| Add feature | Active layer + draw tool | Click canvas for geometry, modal for required attributes, show draft at same location |
| Add data | Modal, file drop, WMS/WFS/COG or create layer tabs | Validate CRS/geometry/schema/permissions; show progress and errors; successful result adds layer without navigation or loss of viewport/selection |
| Attributes | Resizable bottom dock | Two-way row/map selection, zoom to selected optional; no navigation |
| Query/filter | Popover or side drawer | Preview and highlight matches, optional zoom; don't overwrite pending edits |
| Spatial processing | Drawer/job controls | Preview inputs and output, track status, attach result layer only when job succeeds |
| CAD / COGO | Tool popover and command line | Snap, precision input, units, preview; explicit confirm; escape cancels |
| Plugin | Registry-injected toolbar/panel/modal | Share active project/layers/selection via capability-scoped bridge, no engine replacement or plugin-specific full page |
| Export/print | Modal or dedicated layout overlay | Snapshot extent or selected features; report job failure; retain canvas state |

Coordinate editing: display CRS and axis order, transform map/display/storage CRS using backend/engine contracts; respect precision, round only for display. Geometry preview stays client-side. On commit validate capability, feature revision, CRS/topology and tenant ownership; show server response before claiming save. On conflict offer reload/reconcile, do not overwrite silently. Cache/revision invalidation refreshes changed feature/layer only.

## Acceptance gates (add to T018–T028 fixtures)

- Open project; capture route, map instance identity, center, zoom, bearing, visible layers and selection. Open/close modal and query/CAD/plugin panel; assert unchanged unless user explicitly changes them.
- Select point by click; edit X/Y, drag to new position, confirm/undo/redo, save, reload and compare persisted geometry within CRS tolerance. Escape and cancel leave persisted geometry unchanged. A 403/409/invalid CRS must not claim success.
- Create point, line, polygon; edit vertices; invalid polygon is rejected, user can recover.
- Open import modal from toolbar or layer tree; upload valid fixture → new layer and imported geometries on canvas; invalid CRS/file or forbidden action stays recoverable without route change. Preserve existing edits/selection and map view.
- Select table row ⇄ map object, query results on map, preview analysis and export; plugin number/address workflow stays in map context.
- Responsive desktop/tablet: hide/resize panels, keyboard focus, keyboard equivalent to drag, avoid overlay focus trapping and pan conflict. Field mobile separate shell, not full desktop parity.
- No mock-only acceptance; run against real authenticated server on staging.

## Visual direction V2.1 — Vibrant cartography (owner requested)
Replace monotonous teal with a clear modern cartographic palette. Proposed, pending Figma application and visual review (Starter MCP rate limit).

| Role | Color |
|---|---|
| Header / navy | #172A50 |
| Primary action / cobalt | #2159D1 |
| Hover / active tool | #E6EEFF |
| Surface | #FFFFFF |
| Map neutral ground | #F3F7FA |
| Land parcel fill / warm sand | #F7E4BC |
| Vegetation / agricultural parcel | #C8E7A6 |
| Buildings | #C8B8A8 |
| Rivers / water | #79C7ED |
| Roads | #FFFFFF with #F4B45F centerlines |
| Address points | #7253CC |
| Selection / active geometry | #F07824 with pale orange halo |
| Save success / sync | #16834E |
| Warning | #B45309 |
| Error | #C53030 |

Layer legend swatches match canvas symbology; selected feature has thicker outline/handles and a label in addition to color. Maintain readable text contrast; map colors are layer symbol defaults overridable per project, not global status colors. Preserve V2 persistent map, compact toolbar, contextual inspector and modal interactions; no changes to production yet.

## Open items
Finish actual ON_DRAG reaction (valid Smart Animate transition), old entry links, modal source tabs, direct add feature/vertex-edit states, resizeable table and plugin 019 detailed workflows; exhaustive V2 visual/interaction QA. No rollout.
