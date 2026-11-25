# E06 — Symbology, labels, layout

Status: proposed; parent Spec 064, R009. Baseline 3e78ac1. Depends on T001–T005; E01 authorization applies to all tenant paths.

## User story and scope
Single/categorized/graduated/rule styles as supported, label expressions/placement, legends, scale, print A0–A4, export PDF/image, revision-pinned layout; visual and semantic fixtures.

## Source and ownership
Route families: cartography layout_designer. See contracts/static-inventory.json for file and handler candidates. Existing supported behavior is mandatory; unverified or duplicate/legacy surfaces must be classified, not silently removed.

## Architecture candidate
Retain current renderer/TS UI; Rust style models; no claim all QGIS styling supported.

## Acceptance
- E06-AC1: inventory each operation/command/algorithm and existing evidence; explicit classification (migrate, retain, replace, approved retire); map to source, contract and test IDs.
- E06-AC2: golden and negative tests against same baseline, fixtures and candidate; verify error/authorization/CRS/revision where relevant; preserve IDs and semantics.
- E06-AC3: measure representative cold/warm p50/p95/p99, peak RSS and errors where applicable; no invented speedup; document non-applicable metrics.
- E06-AC4: independent integration/consumer test, reviewed dependency/license/security and rollback implications; unresolved gaps block acceptance.

## Dependencies and edge cases
E01 for security; E02 for spatial/CRS; E03/E11 for mutation and sync where relevant. Specific dependency/order refined before implementation. Invalid input, permission denial, cancellation, concurrency, large payload and failure recovery tested where applicable.
