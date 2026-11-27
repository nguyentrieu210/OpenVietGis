# E01 — Nền tảng, identity, tenant, security

Status: proposed; parent Spec 064, R001 R002 R015. Baseline 3e78ac1. Depends on T001–T005; E01 authorization applies to all tenant paths.

## User story and scope
OIDC, session/CSRF, invitation, service identity, permission layer/object, RLS, audit; negative tests per tenant and revoke.

## Source and ownership
Route families: auth bootstrap me organizations workspaces projects grants gis_access invitations service_principals audit authorization tenant_access security tenant_serialization gis_access_guard. See contracts/static-inventory.json for file and handler candidates. Existing supported behavior is mandatory; unverified or duplicate/legacy surfaces must be classified, not silently removed.

## Architecture candidate
Rust Axum/SQLx; retain OIDC provider; preserve token/cookie contract.

## Acceptance
- E01-AC1: inventory each operation/command/algorithm and existing evidence; explicit classification (migrate, retain, replace, approved retire); map to source, contract and test IDs.
- E01-AC2: golden and negative tests against same baseline, fixtures and candidate; verify error/authorization/CRS/revision where relevant; preserve IDs and semantics.
- E01-AC3: measure representative cold/warm p50/p95/p99, peak RSS and errors where applicable; no invented speedup; document non-applicable metrics.
- E01-AC4: independent integration/consumer test, reviewed dependency/license/security and rollback implications; unresolved gaps block acceptance.

## Dependencies and edge cases
E01 for security; E02 for spatial/CRS; E03/E11 for mutation and sync where relevant. Specific dependency/order refined before implementation. Invalid input, permission denial, cancellation, concurrency, large payload and failure recovery tested where applicable.
