# 066 implementation slice 22 — T043 observability contract

Parent candidate: `76d624e9ef4d35e52d63f3a9cc7241ce442944a1`.
Status: partial; T043 remains unchecked pending release-workflow integration and
a real immutable candidate observation.

## Implemented

- The existing Rust API histogram, request/error counters and process RSS remain
  unlabeled; this slice does not change `apps/api-rust/src/metrics.rs`.
- The browser GIS session panel now reports p99 alongside p50/p95 and cache
  hit-rate. It remains bounded to 60 in-memory samples and is not uploaded.
- Added an allow-listed release-observation validator and quantile aggregator
  for build and attempted deployment durations. It rejects unknown fields,
  invalid timing, unpinned image references and inconsistent `not_run`
  deployments. Aggregated output omits commit and run IDs and includes failed
  deployment attempts to avoid success-only timing bias.
- Added source/query/privacy documentation under `tools/observability`.

## Validation

- Python standard-library unit tests: 6/6 PASS.
- Python bytecode compilation: PASS.
- Focused Vitest `src/SpatialPerformance.test.ts`: 1/1 PASS.
- Web-next TypeScript typecheck: PASS.
- `git diff --check`: PASS.
- The available development host runs Node 24.22.2 while the repository asks
  for Node 26; focused tests and typecheck passed with the engine warning.
  Full supported-runtime workflow validation remains required.

## Remaining evidence

The T040 workflow change is being prepared separately. No release artifact has
yet been generated on this branch, no build/deploy duration has been observed,
and no deployment occurred. Therefore this slice does not claim T043 complete
or any performance budget pass.
