# 020 — Map-first frontend preview release evidence

Preview https://vps1.gpcds.site:2087/rewrite/ (isolated static subpath, synthetic data, no production API or email). Source apps/web-next; old apps/web and /app unaffected.

## Map-first interaction

A project opens directly into one MapLibre canvas. Toolbar, tree, table, inspector and commands operate on a single state. Modal import/coordinate/numbering/query/export never navigates away. House number plugin is contextual; future production integration must use the generic module registry, not hard-code it into the Core shell.

| 019 capability | New UX surface | Current preview |
|---|---|---|
| US1 area/road/alley/building | Hierarchical xã/ấp/tuyến/hẻm tree; click map/row; draw geometry and create modal | Interactive sample, no API |
| US2 inspect/propose | Select point → inspector → coordinate/move/numbering preview | Drag, coordinate, sample proposal; production rules/API pending |
| US3 proposal/approval/history/plaque | Context drawer for route or selected set; preview, submit, approve/reject per rights | Map state visual; API pending |
| US4 imports/reconciliation | Add-data modal with CRS and preview; side/bottom dock discrepancy list | Local GeoJSON FeatureCollection import; CAD/Excel/three-source reconciliation pending |
| US5 survey/assignment/progress | Field shell GPS/survey; map progress by area/route | Desktop scope counts; actual mobile/offline pending |
| US6 documents | Object/route context document drawer | Pending |
| US7 search/export | Search box + result table and selection; export current scope | Local search and GeoJSON export; server export/DXF/handover pending |

## Verification (2026-11-22)
- TypeScript check passed, Vite production build passed.
- Chromium Playwright smoke (apps/web-next/smoke.mjs): public preview served HTTP 200, MapLibre ready, route filtering, attribute table selection, coordinate modification, draggable map marker, local GeoJSON upload, new layer in existing map, zero uncaught page errors.
- Desktop 1440×900 screenshot inspected: full map, left tree, compact blue toolbar, colorful layer geometry, no UI navigation.
- Browser-only session save, no server data. Imported data is read in client and not uploaded. Do not input real PII here.
- Test-only build artifact deployed to /var/www/openvietgis-hn019/rewrite/. Existing nginx config unchanged. Rebuild requires installed node 26 and existing dependency links; this initial preview package is not yet wired into monorepo pnpm workspace/CI. Production dependency integration, lazy chunking, keyboard parity, and all 020 release gates remain open.
- Spec Kit 020 T003 design review now transitions toward live preview; T006–T028 are not complete. Do not present mocked numbering as approved official number, nor claim auth/Resend or backend workflows verified.
