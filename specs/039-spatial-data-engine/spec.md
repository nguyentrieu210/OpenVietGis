# Spatial Data Engine

Goal: authenticated projects reopen without downloading every feature. PostGIS remains authoritative. WebGIS, QGIS and MCP use revision-pinned APIs; caches are disposable, never the authority for edits.

## Implemented acceptance slice
- Web: IndexedDB snapshots scoped to account/organization/project/layer. Current access and revision metadata required before display. Restricted scope bypasses full snapshots. 24h expiry, bounded 96 MiB total, 24 MiB per layer, 20k rows per layer. Cache I/O and serialization on a dedicated worker; no bearer token stored.
- Changed Web revision: bounded complete changeset chain, feature-detail reads, fallback to layer refresh when chain cannot be proven. MapLibre incremental renderer retained.
- Camera: per-account/project local position and zoom. Neutral GIS Core chooses existing MVT mode for a supported layer over 20k features. Specialist plugin retains full feature mode to preserve business semantics.
- MVT: existing PostGIS GiST, bounding-box selection, point clusters and geometry simplification. Private ETag revalidation after auth and revision pin; scoped access remains no-store. No public CDN or shared cross-user cache.
- Attribute table: keyset pagination, 100 records per request for tile layers. Filter is explicitly page-local for remote pages.
- Desktop: SQLite confirmed-snapshot cache keyed by hashed credential and project/layer; fresh server bootstrap probe mandatory; bounded delta catch-up with final authorization/revision probe, fallback to network. 24h TTL and bounded disk. Existing draft journal and idempotent edit receipts remain separate.
- Unified build retains CAD/3D from latest local production frontend source and includes matching QGIS package and backend.

## Gates and measured results
- Browser compiled-app mock API: cold load 2 feature pages; unchanged reload 0; one changed feature reload 0 full pages, 1 history read, 1 feature detail. Restricted scope reload 2 pages. No JS errors.
- 74 targeted Python tests; 3 isolated real PostGIS integration tests (device session, bootstrap/commit/delta, MVT ETag and authorization); 4 web logic tests. TypeScript and production Vite build pass.
- Windows native PyQGIS 3.44.15 smoke pass, 8k synthetic points build/snapshot/edit 0.83s excluding network. No changes to user project.
- Package SHA256 f17a7e96500bf5e8f38a011768d8fb659194306c46dc17d4c55e2952028708b5.

## Remaining release gates
- Do not deploy without production approval. Test authenticated real Bình Hưng cold/warm reopen and 2-client edits, measure p50/p95 and request counts. Verify large layer MVT navigation and table on real data.
- Tile eligibility currently limited to native EPSG:4326 and neutral Core. Specialist rendering, projected CRS and server-wide attribute filtering require additional product work.
- SQLite snapshot cache is not an editable GeoPackage provider; migration to persistent native GeoPackage editing, offline authenticated policy, outbox replay and reconciliation require further design and tests.
- No claims of 1M/10M capacity or subsecond internet latency before load tests. Measure peak RAM/CPU, FPS, input latency, cache hits, network bytes and event-to-render p95. Add continuous metrics and load-test gates. Redis/shared server tile cache only if benchmarks justify it, with ACL-safe invalidation.

## Production and performance evidence, 2026-11-24
- Release af02574 served matching Web/backend/QGIS ZIP; public asset, checksum and readiness checks passed. Remote exact tree: d2baff192fef0aada291bad6e9c1ddabcaeeefda.
- Production read-only metadata: 42 counted sources, 40,195 features, largest source 9,000; aggregate metadata query 3.37 ms. Five public readiness requests: 315–471 ms total. Not an end-to-end sync measurement.
- Isolated PostGIS benchmark with one CPU/640 MiB memory: 1,000,000 synthetic point rows, 142 MB total storage; GiST query returned 676 points in 1.314 ms execution; high zoom MVT query produced 49 points/1,414 bytes in 2.603 ms after initialization. No user project mutation. Not a full API/RLS/browser benchmark.
- Client diagnostics: bounded anonymous open/sync/render timing samples, cache hits, API page counts, delta layer counts, p50/p95; accessible from GIS status footer. Chromium smoke confirms unchanged F5 shows zero full pages and two cache hits. No server-wide telemetry or end-to-end paint measurement claimed.
