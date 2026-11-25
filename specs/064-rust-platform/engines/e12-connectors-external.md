# E12 — QGIS, external sources, integration

Status: proposed; parent Spec 064, R010 R027. Baseline 3e78ac1. Depends on T001–T005; E01 authorization applies to all tenant paths.

## User story and scope
QGIS login, layer tree/dock, snapshot/delta, sync conflict, desktop credential handling, external basemap/service policies, bridge capability; real QGIS and retry tests.

## Source and ownership
Route families: external_sources integration_hub integration_bindings external_provider_transport. See contracts/static-inventory.json for file and handler candidates. Existing supported behavior is mandatory; unverified or duplicate/legacy surfaces must be classified, not silently removed.

## Architecture candidate
Keep PyQGIS and Windows bridge adapters; Rust server/optional native binding.

## Acceptance
- E12-AC1: inventory each operation/command/algorithm and existing evidence; explicit classification (migrate, retain, replace, approved retire); map to source, contract and test IDs.
- E12-AC2: golden and negative tests against same baseline, fixtures and candidate; verify error/authorization/CRS/revision where relevant; preserve IDs and semantics.
- E12-AC3: measure representative cold/warm p50/p95/p99, peak RSS and errors where applicable; no invented speedup; document non-applicable metrics.
- E12-AC4: independent integration/consumer test, reviewed dependency/license/security and rollback implications; unresolved gaps block acceptance.

## Dependencies and edge cases
E01 for security; E02 for spatial/CRS; E03/E11 for mutation and sync where relevant. Specific dependency/order refined before implementation. Invalid input, permission denial, cancellation, concurrency, large payload and failure recovery tested where applicable.
