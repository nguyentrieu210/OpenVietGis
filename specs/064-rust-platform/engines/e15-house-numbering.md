# E15 — House numbering and land asset module

Status: Python retention approved by owner decision 2026-11-26; parent Spec 064, R014 R019 R051. Baseline 3e78ac1. Depends on T001–T005; E01 authorization applies to all tenant paths.

## User story and scope
Administrative hierarchy, route import, parent/child address, numbering, prefix, suggestions, verification, reconcile, cluster/topology correction, survey, geometry review, document/export, QR, audit; land_asset separate; 30k authorized reference or synthetic.

## Source and ownership
Route families: see inventory by component. See contracts/static-inventory.json for file and handler candidates. Existing supported behavior is mandatory; unverified or duplicate/legacy surfaces must be classified, not silently removed.

## Architecture candidate
Retain the existing FastAPI house_numbering runtime, land_asset reference module, frontend ABI, migrations and SDK contracts, per the owner hybrid decision in engine-scope.md (6a448a7). All E15 operations in the frozen inventory are classified retain; its historical migrate classification is superseded, not deleted. No Rust rewrite or dual write. The Rust prototype is not an authoritative public endpoint.
The hybrid gateway must send all project-scoped /modules/house-numbering and /extensions paths to FastAPI, including activation, record, download, import, survey and integration paths. E14 owns the shared extension boundary; E18 owns gateway configuration. Both services must preserve common identity, tenant and revision rules.

## Acceptance
- E15-AC1: inventory each operation/command/algorithm and existing evidence; explicit classification (migrate, retain, replace, approved retire); map to source, contract and test IDs.
- E15-AC2: golden and negative tests against same baseline, fixtures and candidate; verify error/authorization/CRS/revision where relevant; preserve IDs and semantics.
- E15-AC3: measure representative retained Python cold/warm p50/p95/p99, peak RSS and errors, including gateway overhead on synthetic 30k data; no Rust comparison required for an unchanged engine; document non-applicable metrics.
- E15-AC4: independent integration/consumer test, reviewed dependency/license/security and rollback implications; unresolved gaps block acceptance.

## Dependencies and edge cases
E01 for security; E02 for spatial/CRS; E03/E11 for mutation and sync where relevant. Specific dependency/order refined before implementation. Invalid input, permission denial, cancellation, concurrency, large payload and failure recovery tested where applicable.
