# E16 — AI and MCP

Status: proposed; parent Spec 064, R041 R021. Baseline 3e78ac1. Depends on T001–T005; E01 authorization applies to all tenant paths.

## User story and scope
MCP tools, OAuth, scopes, bridge, partner actions, consent, workspace actions, audit; no unapproved mutation; compatibility with existing external clients.

## Source and ownership
Route families: see inventory by component. See contracts/static-inventory.json for file and handler candidates. Existing supported behavior is mandatory; unverified or duplicate/legacy surfaces must be classified, not silently removed.

## Architecture candidate
Migrate server internals behind contract; existing protocol/adapter retained until parity.

## Acceptance
- E16-AC1: inventory each operation/command/algorithm and existing evidence; explicit classification (migrate, retain, replace, approved retire); map to source, contract and test IDs.
- E16-AC2: golden and negative tests against same baseline, fixtures and candidate; verify error/authorization/CRS/revision where relevant; preserve IDs and semantics.
- E16-AC3: measure representative cold/warm p50/p95/p99, peak RSS and errors where applicable; no invented speedup; document non-applicable metrics.
- E16-AC4: independent integration/consumer test, reviewed dependency/license/security and rollback implications; unresolved gaps block acceptance.

## Dependencies and edge cases
E01 for security; E02 for spatial/CRS; E03/E11 for mutation and sync where relevant. Specific dependency/order refined before implementation. Invalid input, permission denial, cancellation, concurrency, large payload and failure recovery tested where applicable.
