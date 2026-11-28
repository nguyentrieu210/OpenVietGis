# Acceptance checklist

Candidate implementation is not production acceptance. All items require code review and integrated tests.

- [ ] E01 label browser parity, collision/rotation/zoom across zoom thresholds.
- [ ] E02 task pin GPU path implemented; other bulk collaboration overlays and accessibility/browser tests pending.
- [ ] E03 primary GeoJSON feature state implemented; tile feature state added; tile/comparison browser acceptance pending.
- [ ] E04 GPU handle and single-point drag overlay implemented; adjacent-edge preview added; touch, cancellation/undo and high-density browser tests pending.
- [ ] E05 cached bounding-box index implemented; parity and cold/warm benchmark required.
- [ ] E06 passive rubber-band GPU preview implemented; GPU snap indicator and measurement label added; freehand history, native CAD path pending.
- [ ] E07 bounded workers and GPU chart textures implemented; table virtualization, adaptive 3D LOD/cache, geometry metrics worker added; general WASM and browser acceptance pending.
- [ ] E08 297 frontend tests and 2 isolated Chromium smokes passed; physical GPU/1M tile benchmark, full integration and release pending.

[Wave 2 evidence](evidence/wave-2.md).
