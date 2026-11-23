# Tasks: R027 — Bidirectional Integration Sync

**Status:** Planning backlog; all implementation/test/release work remains open.  
**Input:** [spec](spec.md) · [plan](plan.md) · [research](research.md) · [data-model](data-model.md) · [API contract](contracts/sync-api.yaml)  
**Format:** `[ID] [P?] [Story] Action and location`; [P] = parallelizable after shared gates.

## Phase 0 — Spec Kit and baseline

- [x] **T001** Create dedicated R027 branch and initialize specification, plan, research, data model, contract and tasks.
- [ ] **T002** Audit exact `main` + live R026 implementation against read-only catalog, GeoPackage preview/push, ACL and browser E2E; record release SHA evidence in `specs/027-bidirectional-integration-sync/evidence.md`.
- [ ] **T003** Review R006/R013/R011 transaction/outbox semantics and QGIS plugin callback support; record decisions in `research.md`.
- [ ] **T004** Author ADR on external mapping, durable event ownership, trusted agent and conditional writes in `docs/adr/`; approve migration and secret strategy.
- [ ] **T005** Set performance budgets, payload limits, event retention and explicit VN-2000 control-point tolerances with real datasets.

## Phase 1 — Foundation: blocking all writes

- [ ] **T006** [P] Implement scoped binding and profile data model under `apps/api/openvietgis_api/` and reviewed migration under `apps/api/migrations/`; add RLS and indexes.
- [ ] **T007** [P] Create typed R027 SDK under `packages/client-sdk/src/` and contract validation against `contracts/sync-api.yaml`.
- [ ] **T008** Implement secure credential refs, host allowlist/revocation/rotation and per-operation ACL/RLS enforcement in existing API security services.
- [ ] **T009** Implement immutable external identity mapping, CRS/field mapping validation and schema/geometry loss reporting in R027 domain service.
- [ ] **T010** Build durable inbox/outbox/run/cursor ledger consistent with existing R006 changesets; prohibit write paths around R006.
- [ ] **T011** [P] Negative security tests under `apps/api/tests/integration/`: anonymous, cross-org/project, layer/feature/field denial, service ceiling, approval expiry and revoke during retries.

## Phase 2 — US1/P1: connection and preview

- [ ] **T012** [US1] Implement binding draft/list/activate/revoke and capabilities endpoints under `apps/api/openvietgis_api/`; make all new bindings disabled and writes denied by default.
- [ ] **T013** [US1] Implement deterministic non-mutating `sync/preview`: remote ID, schema, geometry, CRS, two-sided revision pins, ACL and conflict classifications.
- [ ] **T014** [US1] Implement QGIS plugin/agent adapter under `integrations/qgis_plugin/openvietgis_connector/`, using HTTPS and secure credential storage.
- [ ] **T015** [P] [US1] Add binding wizard and preview/diff/permission/CRS UX to `apps/web-next/src/`, with honest candidate/offline badges (plain text status).
- [ ] **T016** [US1] End-to-end test real QGIS desktop and isolated real PostGIS; assert that preview produces no mutations.

## Phase 3 — US2/P1: approved writes in both directions

- [ ] **T017** [US2] Add preview-digest-bound approval and exact revision revalidation; route inbound changes through canonical R006 changesets.
- [ ] **T018** [US2] Add outbound conditional apply/ack for QGIS agent, cursor advance after ack, idempotency, origin-chain echo prevention.
- [ ] **T019** [US2] Build fault tests: duplicate requests, reordered events, race with same feature, policy revocation and event replay.
- [ ] **T020** [US2] Prove round-trip for geometry+attribute edits, data provenance and exact operation counts on synthetic acceptance dataset.

## Phase 4 — US3/P1: conflict, deletes and recovery

- [ ] **T021** [US3] Implement per-feature conflict ledger, redacted geometry/attribute diff and explicit resolution; no timestamp-based last-write-wins.
- [ ] **T022** [US3] Default deny deletion, quarantine tombstones and require separate impact preview/approval for any proposed destructive action.
- [ ] **T023** [US3] Test pause/resume, agent crash, network loss, cursor gaps, backup/restore and external rollback/compensation behavior.

## Phase 5 — US4/P2: automation (separately gated)

- [ ] **T024** [US4] Build event-driven delivery or safe polling with reconciliation, bounded retries/dead-letter and per-tenant/binding rate limits.
- [ ] **T025** [US4] Expose run history, last successful checkpoint, queue states, pause and retry controls in `apps/web-next/src/`.
- [ ] **T026** [US4] Security review for auto mode (create/update only); prove revocation takes effect before retries and deletes are held for review.

## Phase 6 — US5/P3: independent provider adapters

- [ ] **T027** [US5] Implement independent external PostGIS agent through HTTPS API; never use internal multi-tenant database credentials.
- [ ] **T028** [US5] Evaluate ArcGIS/AutoCAD/MapInfo/V8i capabilities, SDK/license dependencies and field/geometry round-trip guarantees; unverified stay candidate.
- [ ] **T029** [US5] Record provider-specific test matrices and promote each adapter only with separate acceptance evidence.

## Final convergence/release

- [ ] **T030** Map every FR/SC to unit, PostGIS integration, browser and real desktop tests; generate evidence/convergence and independent security review.
- [ ] **T031** Stage exact commit+image+DB migration, run rollback/restore drill and monitor credentials, queue, audit and data equality.
- [ ] **T032** Merge only after review and CI gates; production deploy and verify actual round-trip/rollback with explicitly authorized synthetic project data.

## Dependency order / parallel agent ownership

- **Architect**: T002–T005, ADR, contract semantics and CRS/revision assumptions.
- **Backend/security**: T006, T008–T013, T017–T018, T021–T024 with independent ACL reviewer.
- **QGIS/desktop**: T014, T016, T018, T020.
- **Frontend/SDK**: T007, T015, T025.
- **QA/release**: T011, T016, T019–T020, T023, T026, T029–T032.

T006–T011 block US1; US1 acceptance blocks data writes in US2; US2 + US3 proof blocks automatic mode. No feature claims, main merge or live deployment follow merely from task completion checkboxes.
