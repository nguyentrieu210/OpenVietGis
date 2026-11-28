# 066 implementation slice 12 — PostGIS qualification, bounded terrain workers, differential safety and retention dry-run

Candidate parent: 7c77b75160add170bd3a7d4b14f2af22e99571d7.

## T027 — PostGIS EXPLAIN / query qualification
- Existing isolated PostGIS evidence covers 30k, 100k and 1m source points plus a 100k foreign-tenant decoy population. Five EXPLAIN ANALYZE samples per case record raw plans, indexes, returned rows and ordered result hashes.
- The existing GiST eligibility path materially reduces the qualified narrow-bbox SQL time while preserving ordered results. A proposed alternate viewport SQL was rejected because restricted-scope latency regressed, so no unsafe query/index change was promoted.
- Slice 10 adds same-database Python/Rust HTTP comparison at concurrency 20. Attribute 1k and bbox 30k preserve response parity and error rate; Rust p95 ratios are 0.395 and 0.524 relative to the Python oracle.
- No index removal or production database change was made. This satisfies T027 as a qualification/optimization decision.

## T032 — bounded and stale-safe workers
- Spatial cache RPC remains capped at 24 outstanding jobs. Overflow fails closed. A 15-second timeout terminates the worker, resolves outstanding callers safely and permits a clean worker generation.
- Hydraulic terrain simulation now binds callbacks to the exact current worker generation. Results/errors from replaced or terminated generations are ignored; mode/grid/options changes terminate the current worker; unmount terminates the worker and aborts outstanding requests.
- Targeted terrain worker tests verify stale-generation suppression and cancellation rejects late callbacks.
- Current web-next suite: 22 Vitest files / 105 tests PASS plus 40 Node tests PASS. TypeScript passed in focused validation.

## T033 — differential CRS / geometry / export / security
- Current Rust parity evidence on 7c77b751 includes the 288-case core differential HTTP suite and E06/E07/E09/E10/E11/E12/E13/E14/E15/E18 parity suites.
- Earlier exact geometry evidence verifies WKB/EWKB child ordering, SRID preservation, multipart round-trip and rejection of non-finite coordinates; CRS control tests pass.
- Transfer parity retains independently tested GeoPackage/Shapefile boundaries rather than replacing them with fake native handlers. CAD DXF/georeference, field package/offline, tenant/RLS and house-numbering negative boundaries are exercised on isolated databases.
- No result in this task authorizes removal of retained Python/GDAL engine boundaries.

## T041 — retention dry-run
- runtime-retention.json is transformed by a fail-closed dry-run tool only; the tool contains no delete/prune operation.
- Current plan: 20 runtime objects protected, 27 entries require manual owner review, 0 entries eligible for automatic deletion.
- Live/rollback images, active containers, volumes and active worktrees are protected. Unknown ownership remains manual review even when Docker reports reclaimable bytes.
- 17 targeted ops tests PASS across retention, immutable image-manifest and build-pipeline policy tests. Ruff PASS. No cache/image/worktree/volume/log was deleted.

No production service, production database, image registry or live deployment was modified by this slice.
