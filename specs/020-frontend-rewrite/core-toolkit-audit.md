# GIS Core — toolkit audit and integration (2026-11-23)

## Boundary
Core owns generic map rendering/navigation, sources/layers, CRS, editing, queries, processing, transfer, cartography, CAD/COGO, collaboration, external OGC, field/offline and platform operations. House numbering keeps domain-specific territory/road/address rules, survey filters, labels and API calls in its lazy extension. Its activation is project scoped. No migration or modification of survey data accompanies this UI change.

## Reconnected capabilities
| Core surface | Existing service | Interaction |
|---|---|---|
| Navigation, zoom, visibility, layer order and style | Map configuration, pinned feature reads, display extent | Menu, layer toolbar, commands; explicit SAVE or VIEW with revision receipt |
| Map configuration history | map history | HISTORY |
| Attribute table/selection | Layer features | ATTR, map selection |
| Layer/source/catalog/CRS | Spatial catalog and capability contract | LA, CRS, ADD, layer properties |
| Create, move, delete features; changeset history and revert | Editing service | POINT/LINE/POLYGON, MOVE, ERASE; EDIT/UNDO for native CRS and history |
| Measure | Client geographic calculation | DIST, AREA; click or coordinates, END/ESC |
| Query | Layer and saved query API | QUERY |
| Processing | Algorithm/jobs API | BUFFER, CLIP, DISSOLVE, JOBS |
| File import/export | Import/export API | IMPORT, EXPORT, ADD |
| Style/print layout/export | Cartography API | STYLE, PLOT |
| CAD/DXF/COGO | Platform CAD tools | DXF, COGO |
| External services | External connection registry | WMS (WMS/WMTS/COG capabilities) |
| Field and collaboration | Offline field and collaboration services | FIELD, REVIEW |
| Operations | Platform availability and operational records | OPS |

All workbenches mount lazily in an overlay within the existing map route; no page navigation. Core uses the existing server authorization, protected query and revision contracts. No new API endpoint or placeholder processing is added.

## Explicit limits
- This is not feature parity with every QGIS/AutoCAD module. Commands are meaningful aliases, not a CAD scripting interpreter.
- Direct sketch currently supports editable XY EPSG:4326 Point/LineString/Polygon sources only; for other CRS use the native editing workbench. Measurement is geodesic/spherical and is not a cadastral adjustment.
- Buffer/Clip/Dissolve are the existing governed backend algorithms. Other algorithms require implementation and test, not dummy buttons.
- Feature reads retain the original per-layer pagination cap. Saved map configuration is subject to API limits, current capabilities and revision/permissions. Persisting the OSM basemap is not covered by the existing grid-only map configuration contract.
- Existing plugin styles may override generic symbol settings for its own layers.
- Existing external/printing/offline facilities depend on server capabilities and permissions; opening a workbench is not evidence that every external provider or operation works.
- Smoke test uses intercepted fixture APIs to exercise UI and request paths, not production mutation. Production deployment validation is read-only or login/logout only.
