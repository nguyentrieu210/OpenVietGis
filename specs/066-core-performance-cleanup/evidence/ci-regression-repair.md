# Spec 066 regression repair — shared native build and route probes

Candidate 2a2bff94bab6185d104add32ff637ee7ca0dd384 (PR #82):
- 9/11 workflows passed. Native Rust build, differential PostGIS suites,
  Cargo Audit, QGIS, MCP, E05, E06/E08, E11, E15, E17 and E18 checks passed.
- Both root CI consumers failed only the legacy assertion that backend had
  no job dependency (1 failed/1338 passed and 1 failed/1355 passed).
  Backend and frontend now share native-api, while backend remains entirely
  independent of JavaScript setup/install. The test asserts that dependency.
- Rust workspace tests failed before route inventory evaluation: POST to
  GET-only health returned the sentinel 418, not 405. This was a flawed
  route-layer probe, not evidence that an API or an inventory row was absent.
  The replacement probes Axum's unsupported-method fallback and verifies
  the Allow header, without invoking actual business handlers. Per-engine
  frozen operation counts (363 HTTP rows plus one presence upgrade) are
  checked individually, and inventory changes trigger Rust platform CI.
- All original business, architecture and parity assertions remain enabled.

No final-candidate test result or release approval is claimed here.
