# Studio sample data — staging

Imported using authenticated Core import API and linked through House Numbering API in the pre-existing activated staging test project (Map team 1795327408970-hn / Map project 1795327408970-hn). All features are synthetic, not real cadastral or address data. No production data changed.

- Ranh giới xã và ấp: 3 polygons.
- Tuyến đường và hẻm: 4 lines (2 streets, 2 nested alleys).
- Thửa đất: 154 polygons.
- Mặt bằng công trình: 154 polygons.
- Điểm số nhà: 13 points, synthetic number/status attributes.
- Total: 328 Core features. 3 sample HN areas, 4 sample HN routes, 13 HN building links, draft sample rule. The previous unrelated E2E area and rule are preserved.
- UI: Thêm dữ liệu → Nạp bộ dữ liệu mẫu is available to users with project edit permission in empty/sample-only projects. New projects without active house numbering load geometry only; HN activation is separately blocked by the documented schema revision mismatch. In active projects the sample registry is linked without fabricating IDs.
- Import skips and verifies existing sample layers; never overwrites other layers. Explicit sample label on every layer. No legal/official house numbers or real household data asserted.
- Browser smoke tested layer counts and real API registry counts, tree navigation and MapLibre load. Source: apps/web-next/src/sampleData.ts, apps/web-next/sample-smoke.mjs.
- Actual legacy Bình Hưng data was not located on this VPS. The sample cannot substitute for the original files/export.
