# Acceptance checklist

Candidate implementation is not production acceptance. All items require code review and integrated tests.

- [ ] E01 label browser parity, collision/rotation/zoom across zoom thresholds.
- [ ] E02 task pin GPU path implemented; comment/peer/lease GPU overlays added; isolated browser smoke passed; full accessibility and integrated browser tests pending.
- [ ] E03 primary GeoJSON feature state implemented; tile feature state added; tile/comparison browser acceptance pending.
- [ ] E04 GPU handle and single-point drag overlay implemented; adjacent-edge preview added; 1,000-handle drag smoke passed; touch and cancellation/undo acceptance pending.
- [ ] E05 packed worker hierarchy and parity implemented; synthetic warm p95 0.1 ms, cold first query 28.5 ms (10 ms target not met).
- [ ] E06 passive rubber-band GPU preview implemented; GPU snap indicator and measurement label added; buffered GIS gesture history implemented; independent native CAD path pending.
- [ ] E07 bounded workers and GPU chart textures implemented; table virtualization, adaptive 3D LOD/cache, geometry metrics worker added; Rust WASM measurement kernel added and worker verified; integrated browser acceptance pending.
- [ ] E08 299 frontend tests, TypeScript, production build and 2 isolated Chromium smokes passed; physical GPU/1M tile benchmark, full integration and release pending.

[Wave 2 evidence](evidence/wave-2.md).

[Wave 3 evidence](evidence/wave-3.md).
