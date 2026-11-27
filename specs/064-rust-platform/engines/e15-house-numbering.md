# E15 — House numbering and land asset module

Status: Rust-native control-plane migration approved by owner decision 2026-11-26; parent Spec 064, R014 R019 R051. Baseline 3e78ac1. Depends on T001–T005; E01 authorization applies to all tenant paths.

## User story and scope
Administrative hierarchy, route import, parent/child address, numbering, prefix, suggestions, verification, reconcile, cluster/topology correction, survey, geometry review, document/export, QR, audit; land_asset separate; 30k authorized reference or synthetic.

## Source and ownership
Route families: see inventory by component. See contracts/static-inventory.json for file and handler candidates. Existing supported behavior is mandatory; unverified or duplicate/legacy surfaces must be classified, not silently removed.

## Architecture candidate
Rust owns the OpenVietGIS House Numbering control plane: authorization, activation compatibility, registry/domain validation, lifecycle, revision, audit, persistence orchestration and stable public API contracts. Existing PostgreSQL/PostGIS schema, frontend ABI, module SDK and accepted specialist engines are retained beneath that boundary rather than rewritten for their own sake.
Migration is incremental and parity-gated. FastAPI remains the oracle/rollback path only for surfaces not yet accepted in Rust; it is not the final owner of E15. Thin Python/native adapters are allowed only where an external engine or platform binding requires them, and may not own tenant authorization or E15 business lifecycle semantics. E14 owns the shared extension boundary and E18 owns cutover/gateway configuration.

## Acceptance
- E15-AC1: inventory each operation/command/algorithm and existing evidence; explicit classification (migrate, retain, replace, approved retire); map to source, contract and test IDs.
- E15-AC2: golden and negative tests against same baseline, fixtures and candidate; verify error/authorization/CRS/revision where relevant; preserve IDs and semantics.
- E15-AC3: measure representative FastAPI oracle and Rust candidate cold/warm p50/p95/p99, peak RSS and errors on synthetic 30k data for migrated hot/control-plane paths; document non-applicable engine metrics where work remains in retained specialist adapters.
- E15-AC4: independent integration/consumer test, reviewed dependency/license/security and rollback implications; unresolved gaps block acceptance.

## Dependencies and edge cases
E01 for security; E02 for spatial/CRS; E03/E11 for mutation and sync where relevant. Specific dependency/order refined before implementation. Invalid input, permission denial, cancellation, concurrency, large payload and failure recovery tested where applicable.
