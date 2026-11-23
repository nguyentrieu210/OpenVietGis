# R025 Tasks — MicroStation V8i Bridge

Base commit: `5baade9`; feature branch: `feature/025-microstation-v8i-bridge`.

## Phase A — Portable contract (Agent: contract/QA)
- [x] T001 Define Spec Kit spec, plan, research, data model and contract.
- [x] T002 [US2] Implement strict standalone snapshot validation, revision digest, non-destructive diff in `integrations/microstation-v8i-bridge/`.
- [x] T003 [US2] Add synthetic fixtures and repeatable unit tests for CRS, duplicate IDs, unsupported geometries and deletion candidates.

## Phase B — Windows adapter (Agent: windows)
- [ ] T004 [US1] Safe attach-only COM probe for **running** V8i on Windows, with clear offline diagnostics.
- [ ] T005 [US1/US2] Verify COM element traversal on an authorized V8i Windows machine; create capability matrix. Do not enable auto-sync based only on probe.
- [ ] T006 [US2] Map line/string/shape with verified V8i COM semantics; track unsupported entity types rather than silently dropping.

## Phase C — Backend connector (Agent: api/security)
- [ ] T007 [US3] Design device pairing and write-specific scopes, RLS/organization/project binding, revocation, secrets redaction and rate limiting; do not reuse `project.read` for edits.
- [ ] T008 [US3] Implement authenticated idempotent preview-only ingestion endpoint in new module; add integration tests with real PostGIS, tenant isolation and audit.
- [ ] T009 [US3] Map source CRS through independently verified transform/control points and topology checks; no implicit EPSG.
- [ ] T010 [US4] Add review-driven changeset sync as separately approved milestone, no automatic deletes.

## Phase D — Frontend and release (Agent: frontend/release)
- [ ] T011 [US3] Add project-scoped connector UI under current integrations settings; disabled capabilities clearly labelled.
- [ ] T012 [US3] Browser smoke for pairing, revoke, offline status, never exposing secret.
- [ ] T013 [US4] Real V8i Windows smoke: 100-parcel read, metadata coverage, bitness, ID stability, CRS control points, backup/restore.
- [ ] T014 Run merge-review, full quality gates, release with rollback; production deploy **only** after T008–T013 and required Windows evidence.

**Policy:** Existing R010 is read-only; main/live remain untouched. Use PR for review and do not merge automatically.
