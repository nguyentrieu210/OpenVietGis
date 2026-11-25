# E18 — Ops, deployment, backup, performance

Status: proposed; parent Spec 064, R015 R042 R043 R044 R045 R047. Baseline 3e78ac1. Depends on T001–T005; E01 authorization applies to all tenant paths.

## User story and scope
Health, migrations, logs, metrics, capacity, backup/restore, rollback, supply chain, release artifacts, benchmark and incident recovery; no production cutover without evidence.

## Source and ownership
Route families: health ops. See contracts/static-inventory.json for file and handler candidates. Existing supported behavior is mandatory; unverified or duplicate/legacy surfaces must be classified, not silently removed.

## Architecture candidate
Rust modular monolith, bounded resource budgets, CI separate from production.

## Acceptance
- E18-AC1: inventory each operation/command/algorithm and existing evidence; explicit classification (migrate, retain, replace, approved retire); map to source, contract and test IDs.
- E18-AC2: golden and negative tests against same baseline, fixtures and candidate; verify error/authorization/CRS/revision where relevant; preserve IDs and semantics.
- E18-AC3: measure representative cold/warm p50/p95/p99, peak RSS and errors where applicable; no invented speedup; document non-applicable metrics.
- E18-AC4: independent integration/consumer test, reviewed dependency/license/security and rollback implications; unresolved gaps block acceptance.

## Dependencies and edge cases
E01 for security; E02 for spatial/CRS; E03/E11 for mutation and sync where relevant. Specific dependency/order refined before implementation. Invalid input, permission denial, cancellation, concurrency, large payload and failure recovery tested where applicable.
