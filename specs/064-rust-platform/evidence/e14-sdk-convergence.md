# E14 Agent 13 — SDK slice convergence
Date: 2026-11-26. Base: 1a5cb7b40a8a0c8117b8aa56c1522e35d01a5b3b.
Scope: internal trusted native module SDK only; no public API routing or DB schema changes.

## Delivered
- Validate key, semantic version, supported API range, bounded unique permissions, commands and capabilities; reject partner namespace for compiled-in official runtime.
- Pin validated manifest at registration; prevent a dynamic manifest from altering dispatch permission checks.
- Deny dispatch without project scope or declared grant; check response command ID.
- Keep existing Python marketplace and plugin SDK unchanged, including signature checks and lifecycle APIs.
- Shared synthetic fixture file covers nine positive/negative manifest cases consumed by both real validation implementations. Tests assert acceptance parity, not exact HTTP error envelopes.

## Evidence
- E14-T01: existing inventory covers 27 mounted HTTP routes, 39 mapped test IDs, 17 sources. Reviewed inventory and route adapters.
- E14-T02: partial: shared validation cases and SDK permission/manifest tests. Missing public route parity, database-backed cross-tenant and lifecycle differential tests.
- E14-T03: partial: native SDK hardening. The 27 HTTP routes remain Python-owned; no native persistent installation/activation, publisher review, signed catalog, partner bridge, sandbox or quota implementation. No public Rust command dispatch added. E01 verified grants and persistent project activation are required before exposure.
- E14-T04: open: no comparable API candidate; no p50/p95/p99, RSS, error-rate comparison. No speedup claimed.
- E14-T05: partial: module consumer test and targeted lint passed; independent review, dependency audit, full integration, rollback rehearsal and convergence gate remain open.

## Executed
- Rust 1.85.1 Docker, one CPU, 650 MiB memory, locked offline dependencies after cache fill:
  cargo test --locked --offline -p ovg-module-sdk -p ovg-module-house-numbering
  Result: 4 SDK tests + 5 house-numbering tests passed.
- cargo clippy --locked --offline -p ovg-module-sdk --all-targets -- -D warnings: passed.
- Python reference: pytest -q apps/api/tests/unit/test_e14_manifest_parity.py apps/api/tests/unit/test_extension_history_domain.py apps/api/tests/unit/test_extension_metadata_boundary.py apps/api/tests/unit/test_official_module_discovery.py packages/plugin-sdk/test_native_webhook_worker.py packages/plugin-sdk/test_webhook_relay.py
  Result: 38 passed.
- No new dependencies. No deployment, data migration or changes to production routing. Revert this scoped commit to roll back SDK changes.
- Shared VPS disk approximately 2 GB free; full concurrent workspace builds avoided.

## Integration decision
Retain the accepted Python HTTP implementation until native endpoints satisfy the same authorization, audit, revision, portability, signing and browser contracts. Internal SDK validates only its projected manifest, not entity schemas/UI contributions, and trusted callers remain responsible for resolving grants. Do not treat tests of the SDK as proof of tenant isolation at the API boundary. No approved retirement or cutover.
