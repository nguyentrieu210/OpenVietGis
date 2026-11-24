# Plan
1. Review registry, native geometry, import/export, revision and managed bootstrap contracts.
2. Derive backend known/readable CRS sets and Mercator bound from existing registry.
3. Extract testable QGIS paginated snapshot validation; use in actual plugin loading; reject geometry decode failure.
4. Run regression suites and rebuild deterministic QGIS ZIP and canonical frontend.
5. Document remaining risks and commit isolated candidate.
6. Follow up with independent PostGIS concurrency, Windows QGIS native geometry fidelity and surveyed CRS control points.
