# Thirteenth implementation and verification slice
Base 9d78b827b88ab3f13f45b799f746eab964a275ae.

- OUTPUT-01/02: strict canonical document validation (UUID, bounded geometry, allowlisted fields), align one item to page or multiple to selection bounds, equal-gap distribution, 80-step document undo/redo, drag recorded as one change, nonprinting guide grid, direct-edit lock for this editor session. History and locks are local, not persisted; page resize and history can still transform a locked item. Undo invalidates cached map images.
- OUTPUT-03: existing linked scale/north and per-frame camera preserved; legend explicitly described as current visible-layer list, not categorized per-frame parity.
- OUTPUT-05: physical-size SVG export with embedded map PNG, vector page elements and text, no editor selection/guide overlay. Raster/PDF export explicitly rasterizes at target pixel size; checks 40-million-pixel budget and exposes effective frame DPI cap. PDF is a flattened image.
- OUTPUT-06: display-only CRS explicitly read-only for projected coordinates and DMS/extent; conservative projection-domain warnings, status extent denotes two corners, not projected bounding box. Independent PROJ cs2cs control-point outputs for WGS84 UTM48N and explicitly specified VN2000 TM3 106.25 match within 0.01m. Not a surveyed cadastral control point or legal CRS confirmation.

Verification: TypeScript PASS, 83 Vitest + 37 Node tests PASS, production build exit 0 (3902e10362339cb9e1954f29). Actual browser downloads from synthetic fixture: PDF A4 landscape MediaBox 841.8898 x 595.2756 pt, PNG A4 300 DPI dimensions and pHYs metadata, SVG 297 x 210mm with Vietnamese text and embedded map image, no editor guides. Alignment/history/lock and two-frame save/reopen with intercepted revision API PASS. Final targeted parity Chromium exit 0 including projected CRS read-only, previous batch/modeler/edit/digitizing tests. Layout smoke executed before the display-only CRS change; no production API writes or real data.

No paired QGIS Desktop, authenticated layout conflict test, atlas export, categorized per-frame legend, full CRS roundtrip, or performance gate. No acceptance promotion.
