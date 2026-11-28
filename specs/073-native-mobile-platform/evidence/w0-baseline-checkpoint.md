# W0 baseline checkpoint — T001 in progress
Baseline main: a4ae85a5e3d956de1870dcf65ff3fb21da7f0367. Initial SPEC head: 36a46d34210fd4580b1e38a4f267695f104de149. PR #103. Source inventory reviewed at that exact head; no task closure.

| Source / reusable symbol | Disposition / dependency |
|---|---|
| crates/ovg-core/src/{crs,domain,error}.rs; CrsProfile, Feature, AppError | candidate reuse; inspect target graph and semantics |
| crates/ovg-spatial/src/geometry.rs; parse_geojson, parse_wkb, validate_geometry, encode_wkb | candidate reuse; geometry fixtures and limits before mobile claims |
| crates/ovg-spatial/src/transform.rs; PortableTransformKernel, TransformBackend, project_native_vn2000 | candidate reuse; existing r003_crs_controls.json and vn2000_postgis_differential.json are starting fixtures, not mobile qualification |
| crates/ovg-processing/src/{registry,executor,job}.rs; AlgorithmId, algorithm_registry, validate_algorithm_parameters, JobExecutor | metadata/parameters reuse; executor has Tokio/Rayon; actual platform graph and local execution adapters need review |
| crates/ovg-sync/src/{field,outbox,fanout,activity_cursor}.rs; validate_offline_ops, PackageLifecycle, IdempotencyKey, StreamCursor | reuse contracts; in-memory outbox is not durable mobile storage; Tokio dependency |
| crates/ovg-storage | server-only SQLx/Postgres; never link to native engine |
| crates/ovg-module-sdk | existing trusted module contracts, not downloadable plugin runtime |
| packages/client-sdk/src/index.ts and generated contracts | canonical SDK source, reuse contract semantics for Dart |
| apps/api-rust/src/routes.rs | existing router merges identity, catalog, editing, managed sync, field, extensions, processing, navigation, collaboration and other families |
| apps/api-rust/src/extensions/mod.rs::authenticate_actor | Bearer is service credential; human session separate, CSRF enforced; mobile human auth not verified |
| apps/api-rust/src/managed_sync.rs | existing scoped sessions/bootstrap/delta/commits/receipt/events; inspect storage transactions and actual schemas before mobile binding |
| apps/api-rust/src/extensions/validation.rs | marketplace v1 Ed25519, strict ten-field manifest, sorted JSON; preserve v1; separate v2 contract |
 
Repository graph contains 2,980 tree entries at initial head. Matrix parsed: 78 parents, 238 leaves, zero checked; T001 only initially ready. Cycle and unknown-edge negative checks executed in orchestration JavaScript; server-package guard negative checked with synthetic unit input (not native evidence). Actual cargo graph, complete symbol/route/fixture inventory, independently reviewed owner paths and licenses remain open. Rust workspace pins 1.85.1. No mobile SDK/NDK/toolchain pin or A0 acceptance claimed.

Run from checkout with Node 22 and pinned Rust: `node --test tools/mobile/spec073-baseline.test.mjs`, then `node tools/mobile/spec073-baseline.mjs /tmp/ovg-spec073-baseline`. Report contains exact checkout SHA, matrix digest, candidate dependency trees for Android/iOS, metadata and errors. A graph pass is not a native build or device result.

Execution blocker: VPS1 deploy-only; observed 2.9 GiB available RAM, full swap and 17 GB free disk. Local connector unavailable and isolated chat runtime could not resolve github.com. Do not bypass deploy restriction. Use approved GitHub runner; keep production untouched. No autonomous coding-agent launch facility verified in this session. T001 is active, T002/T005/T006/T007 await accepted T001; other dependencies remain as in execution matrix.

## Executed audit evidence
Commit db2ef12c08a9943ee805cec1ebb78725665f4a34; [GitHub Actions run 46166872664](https://github.com/nguyentrieu210/OpenVietGis/actions/runs/46166872664) completed successfully. Job 141046244802: Node tests 3 passed, 0 failed; graph command completed successfully and uploaded artifact 15271903109 (179109 bytes, sha256:97e52c19ed60aac7c90b148c476374c1f14b1fdcb3654aea301ddac8c890244c). Both target metadata and five candidate dependency trees per target resolved; no deny-list hit. These are static resolved graphs, not native binaries. Raw report still requires independent inspection; T001 remains open. Existing general CI, e17-web-parity and rust-mcp workflows reported failure on the original spec-only commit as well as audit commit; root cause not established and not waived.
