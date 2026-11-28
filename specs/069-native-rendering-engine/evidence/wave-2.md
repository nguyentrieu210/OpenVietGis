# Wave 2 candidate evidence — 2026-11-28

- Frontend TypeScript: PASS; spatial-workspace TypeScript: PASS (isolated workspace dependency links).
- Frontend Vitest: 84 files, 297 tests passed. Four baseline failures fixed in test fixtures: active lifecycle and complete geometry/read context; upstream LAS 1.4 point-format-7 fixture legacy count normalized only in tests. Production guards unchanged.
- Headless Chromium 156, 1000x750, Linux, SwiftShader software renderer: native interaction smoke 1000 vertex handles, 400 task pins, zero DOM markers, zero map errors, cleanup verified. GPU label smoke passed Vietnamese point/line/polygon, zoom, background, no camera-driven geometry writes.
- 100k-vertex index: cold build 318.8 ms, warm index-only query p95 0.1 ms (1000 queries). Not end-to-end snap latency. Software renderer frame interval p95 39.3 ms (26 frames); does not meet target and does not establish physical GPU FPS. API not running during isolated browser smoke.
- Added table windowing on loaded pages, map measurement labels, GPU snap cue, native tile feature state, adjacent edge drag preview, geometry metrics worker and bounded 3D cache/LOD. Need browser acceptance of these flows.
- Remaining: general-purpose geometry WASM, complete CAD engine overlay, all collaboration overlays, cold index off-main-thread, real hardware performance/1M feature tile dataset, full integrated main/release qualification. Not accepted for production.
