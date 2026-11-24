# Acceptance runbook (future execution)
1. Pin GIS main after R031 merge, CAD artifact digest and source SHA, dependency locks and migration head; use isolated test database and synthetic DWG/DXF with explicit CRS.
2. Run existing API integration CAD/georef/realtime/changeset tests against PostGIS; spatial-workspace package tests and web-next CAD/spatial smoke; add R048 tests from tasks. No production credentials/data.
3. Generate fixtures: projected simple geometry with independent reference, VN-2000 control set, curves/blocks/text/Z/invalid topology, linked parcel, conflicting edits and read-only user. Record original hashes and expected tolerance.
4. Run actual WASM browser selection/zoom, import/export preview, save version, both directions, GIS 3D, read-only/dirty guards; compare geometry/attributes independently. Record browser/device/network/engine digest.
5. Run two-user plus Web/QGIS event replay, disconnect, reordered/duplicate messages, expired cursor, revocation and tenant isolation; independently qualify V8i native adapter. No full snapshot steady-state for qualified deltas.
6. Benchmark cold/warm bytes and p50/p95, compare to baseline; test cached and new browser profiles, rollback and restore; record commands/logs/fixtures in evidence. Use SC targets, not claims.
7. Exact candidate quality gate + user acceptance; only then enable individual capability. Rollback flags and previous artifact; preserve data and migrations. Production rollout requires separate authorization.
