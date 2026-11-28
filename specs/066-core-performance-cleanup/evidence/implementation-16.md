# 066 implementation slice 16 — lazy Cesium / LOD / GPU lifecycle measurement

Parent candidate: c43996b49af8eea59be00ac79c87ff537c5ea575.

## T031 — lazy Cesium, LOD and texture/GPU lifecycle
- Existing route/engine isolation tests prove the default GIS workspace does not request the Cesium bridge chunk or initialize a 3D canvas. Cesium is imported only when the 3D engine is activated.
- Production Cesium configuration is demand-rendered: requestRenderMode=true, maximumRenderTimeChange=Infinity, targetFrameRate=60, no animation loop, bounded resolutionScale by pixel budget, adaptive globe screen-space error, and explicit widget/tileset destroy on dispose.
- Vector-to-3D conversion is bounded to 5,000 rows and 100,000 vertices. 3D Tiles use maximumScreenSpaceError=24 with 64 MiB cacheBytes and 64 MiB maximum overflow. The existing lifecycle soak separately proves every prepared engine is disposed across 5,000 switches.
- A reproducible loopback benchmark loads the exact self-hosted Cesium 1.147.0 production artifact under Chromium SwiftShader, blocks every external request, performs 10 warmup + 120 measured camera rotations, times request-to-scene-postRender, then destroys the widget.
- Result: 0 external requests, 0 page errors, 120 samples, median 90.9 ms, p95 179.3 ms, p99 1029.2 ms, max 1240.7 ms. This is a headless SwiftShader lab proxy and is not a claim of 60 FPS field performance.
- The exact resources requested by the fixture total 5,725,864 raw bytes / 1,623,808 gzip bytes across 10 files. The largest resource is self-hosted Cesium index.js; no CDN/provider payload is included.
- Dispose verification PASS: the Cesium widget is destroyed and the fixture host is removed after measurement.

This closes T031's acceptance of no eager 3D initialization plus measured frame/resource behavior and bounded lifecycle. Final field/GPU/load qualification remains T044.
