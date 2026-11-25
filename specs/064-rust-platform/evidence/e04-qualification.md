# E04 qualification and integration handover

Status: PARTIAL; do not use as cutover approval.

## Verified evidence

- Inventory: E04-T01 / AC1, all four R007 HTTP operations, baseline 3e78ac1, contract engines/e04.json and e04-inventory.md.
- Prior candidate b8568b352fb2d9d3d9bcfb5317b7deefdf45651b, [CI run 45670321367](https://github.com/nguyentrieu210/OpenVietGis/actions/runs/45670321367), job 139403521335: Rust binary build passed; all 50 E04 differential cases passed against real PostGIS and FastAPI; 11 failures were E02 feature routes not present on this branch (109 overall passes). This is not a green global gate.
- Job 139403521426: rustfmt and architecture isolation passed; Clippy failed with E0277 on QueryError not implementing Debug; workspace tests did not run. Agent 03 adds Debug and explicitly restricts JSON request bodies to objects (Serde sequence deserialization can otherwise accept arrays). Added negative array, offset and injection-shaped field/literal HTTP cases. Requalification must use the new commit, not the prior results.
- Job 139403520812: Cargo Audit failed on RUSTSEC-2026-0459 (libsqlite3-sys 0.30.1), RUSTSEC-2023-0071 (rsa 0.9.10); chrono 0.4.45 unmaintained advisory RUSTSEC-2026-0543. Workspace dependency owner must resolve exposure and version/toolchain compatibility; do not suppress audit.
- Benchmark harness: tests/rust-parity/e04_benchmark.py, dedicated E04 Query Benchmark workflow, deterministic 1k/30k PostGIS fixture, real loopback HTTP to separate servers, cold-ish first requests and warm 1/10/20 clients, raw samples/status/error rates, app RSS and candidate hash. The first requests do not imply cleared DB/OS caches. No comparative numbers yet; see workflow artifact after execution. The baseline report alone does not establish Rust speedup.

## Open acceptance gaps

- AC2: E01 shared bearer/service-principal auth is absent from the branch; baseline permits authenticated service actors subject to project permissions. Current HumanSession fails closed for bearer credentials. E04 must consume the shared E01 auth and transaction scope, not create a second credential implementation or bypass RLS. Service principal fingerprint must include credential ID instead of hardcoded None; add allow/deny/revocation fixtures.
- AC2: expand independent tests for typed decimal, timestamps, nulls, AST bounds, saved replay, concurrent source mutation, timeout/cancellation, and project viewer mutation denial; earlier tests are not exhaustive. Existing HTTP 25,000 and domain 50,000 bounds are intentionally distinct.
- AC3: benchmark execution and raw artifact review pending, including p95 target and no increase in error rate; no performance acceptance claimed.
- AC4: E01/E02 merge, workspace dependency review, independent consumer/rollback review and integrated CI pending. E04 reuses canonical migration 0010; no new DDL or data migration in this repair. Keep FastAPI reference active until parent gate passes.

Do not mark E04-T03/T04/T05 complete on the basis of implementation alone. No merge to main or production deployment.
