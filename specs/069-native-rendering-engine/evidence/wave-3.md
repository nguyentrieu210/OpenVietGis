# Wave 3 candidate evidence

- GPU collaboration overlays: task pins, comment pins, peer cursors and lease labels. Accessible comment selection remains available.
- Buffered GIS drawing: transient pointer gestures render without React state writes per pointer move; completed gesture becomes one history action.
- Rust 1.85.1 geometry WASM kernel for spherical length/area, invoked from worker with JS fallback. Reproducible source/build under tools/geometry-wasm.
- Packed spatial hierarchy with bounded asynchronous worker construction; exact cold fallback and worker parity. Failed worker initialization is not retried for every pointer event.
- Local TypeScript, 85 Vitest files / 299 tests, production Vite build and offline shell: PASS (Node 24.22.2; CI targets Node 26).
- Chromium 156 headless SwiftShader, 1000x750: 1,000 vertex handles, 400 task pins, comment and cursor overlays; zero DOM markers; one drag commit; cleanup true; no map errors; worker parity true; WASM engine used and length 109.505777 m. GPU label smoke also passed.
- Synthetic snap measurement: synchronous cold 148.6 ms, async cold first query 28.5 ms, warm query p95 0.1 ms. Earlier candidate cold measurement 319 ms. Software-rendered frame interval p95 32.2 ms, 27 frames. Not a hardware GPU or end-to-end million-feature benchmark.

Remaining acceptance: cold-query 10 ms target not met; hardware GPU 60 FPS and actual million-feature backend/tile workload not certified; independent native CAD relay path is not changed by this GIS work; integrated app, security, touch/cancellation, comparison and release gates remain pending. No production deployment or merge implied.
