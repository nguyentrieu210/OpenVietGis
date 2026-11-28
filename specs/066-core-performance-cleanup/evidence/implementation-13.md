# 066 implementation slice 13 — lifecycle soak and privacy-safe performance telemetry

Parent candidate: 4d35774a3f0a675d3fe746321cb8f200a0caf6b3.

## T019 — lifecycle cleanup and repeated switching
- A deterministic ModeCoordinator soak runs 5 phases × 1,000 sequential CAD/3D switches (5,000 total). Each prepared engine owns an 8,192-record synthetic object graph so missed disposal retains measurable heap.
- Acceptance invariants: exactly one active engine after a completed switch, at most two during synchronous handoff, every activated engine disposed, no double dispose, <=12 MiB retained heap at a phase boundary, <=8 MiB after final dispose.
- Exact rerun on Node 26.13.1: 5,000 activated, 5,000 disposed, max active 2, final retained JS heap +97,304 bytes. The checked evidence run records +97,488 bytes; both are far below the bound.
- Spatial workspace regression suite PASS: 54/54, including stale completion disposal, scope-change abort/disposal, failed activation disposal, active capability revocation teardown, CAD dirty/saving gates, 3D/terrain bounds and engine lifecycle.
- This is a control-plane/JS heap soak, not a GPU-driver leak certification. Browser/driver memory remains part of final load/soak T044.

## T024 — aggregate cache/startup telemetry
- Browser SpatialPerformance remains workspace-local, memory-only and bounded to the latest 60 samples. It records open/sync/render elapsed time, page count, cache hit/miss, cache bytes read, invalidation count, delta layers and tiles.
- Summary exposes p50/p95, cache hit-rate, aggregate cache bytes and invalidations. It contains no URL, account, organization, project, layer or feature identifiers. Focused Vitest verifies 60-sample retention, 2/3 hit rate and ID-free serialization.
- GIS open metrics now count a cache hit only when cached rows are actually reused. Missing cache, scope/revision mismatch and unusable incremental recovery are counted as misses; invalid cached entries are counted as invalidations.
- Rust API metrics record request latency as an unlabeled Prometheus histogram with 5/10/25/50/100/250/500/1000ms/+Inf buckets, request count, 5xx count and process RSS. No route, path, actor or tenant labels are emitted, avoiding high-cardinality/sensitive dimensions.
- Middleware measures the request around the existing Axum next call. Metrics unit test verifies cumulative buckets, 5xx count, RSS line and absence of organization/project/actor/path/route labels.
- Validation: Rust 1.85.1 offline cargo check PASS; metrics unit test PASS (1/1; 81 other API tests filtered); web production build PASS; web TypeScript PASS; focused browser telemetry/cache Vitest PASS (3/3). rustfmt is not installed in the pinned offline image, so no formatting claim is made; git diff check PASS.

## T043 remains partial
- HTTP histogram supports downstream p95/p99 derivation; browser telemetry supplies session p95/cache; process RSS and 5xx are exposed.
- Actual build/deploy duration from an immutable release run has not been observed because the main-only GHCR workflow has not been dispatched and no approved release digest exists. T043 therefore remains open.

No production service, registry, database or live deployment was modified.
