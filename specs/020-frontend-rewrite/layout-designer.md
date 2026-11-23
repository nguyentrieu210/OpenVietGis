# Layout Designer — implementation and acceptance (2026-11-23)

## Scope
Open with GIS command `LAYOUT`; existing `PRINTCLASSIC` remains unchanged. Design portrait/landscape A0–A4, move/resize in millimeters, snap to 1 mm, stack items, configure one to four independent map camera frames (center, zoom, bearing), WGS84 graticule, text, legend, north arrow, linked scale bar and rectangle. Store up to 50 versioned templates per project, import/export JSON, and render the current composition to PDF/PNG. 150 DPI for A0–A2; 150/300 DPI for A3/A4. PDF is raster, scale is approximate at frame center, and underlying map images may be capped for memory safety.

## Implementation
- `apps/web-next/src/LayoutDesigner.tsx`, `LayoutRender.ts`, `layout-designer.css`
- `apps/api/openvietgis_api/adapters/http/layout_designer.py`
- `apps/api/migrations/versions/0026_layout_designer.py`
- `apps/web-next/layout-smoke.mjs`
- `apps/api/tests/integration/spatial/test_layout_designer.py`

## Acceptance results
- Dedicated disposable PostGIS test: PASS. Create/list/update/delete, document bounds, linked-map validity, rejection of injected camera URL, optimistic revision conflicts, authorization denial.
- Chromium mocked API / deterministic raster tile test: PASS. Two independently saved frames, camera zoom, grid, PDF download, A4 landscape MediaBox, PNG download at 300 DPI (3508 × 2480 with pHYs 11811 pixels/meter), subsequent save revision 2 and reopen.
- PNG encoder independently tested in Chromium at 300 DPI: image decodes, dimensions and pHYs exact. The smoke uses smaller map frames at 300 DPI to limit software WebGL cost without reducing page output dimensions.
- TypeScript and Vite candidate build passed. Evidence files on server: `/tmp/ovg-layout-smoke.pdf`, `/tmp/ovg-layout-smoke.png`, `/tmp/ovg-layout-run/smoke.log` (RESULT:0).
- Deployed jointly with Partner Platform / Marketplace on 2026-11-23 after 8 backend integration tests, TypeScript/Vite build and both partner and layout browser smoke tests passed. Migrations 0026 and 0027 applied, API ready, canonical HTTPS index checksum matched candidate, assets HTTP 200, unauthenticated layout API HTTP 401, public Marketplace and login modal/Google provider visible. Verified backup at `/var/backups/openvietgis/release-0027-20261123`; pre/post project/layer revision baselines match, including 10 layers in Bình Hưng. No live project data mutated. Authenticated live-project write/export acceptance remains for an authorized project session.
