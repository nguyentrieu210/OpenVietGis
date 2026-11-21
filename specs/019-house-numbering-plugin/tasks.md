# Tasks: House Numbering Plugin (R014 child 019)
**Input**: [spec](spec.md), [plan](plan.md), [research](research.md), [data model](data-model.md), [contract](contracts/extension-contract.md).
Tests required; each runtime task must be backed by actual test evidence. No production edits by this branch.

## Phase 1: Setup / independent rule library (can execute before integration gate)
- [X] T001 Create module-local folders and unit test file, no Core imports: modules/house_numbering/domain.py, tests/modules/test_house_numbering_domain.py.
- [X] T002 Write failing pure domain tests for FR-006/007/009/014; tests/modules/test_house_numbering_domain.py.
- [X] T003 Implement deterministic rule, parity, gap, suffix, nested prefix and locked number logic (data-model.md: Rule.version, Rule.direction/parity, Rule.max distance, Rule.authority citation; AddressNumber.number string, AddressNumber.kind; Route.prefix) in modules/house_numbering/domain.py.
- [X] T004 Implement deterministic three-source classification and unmatched case logic (data-model.md: ReconcileCase.class, Evidence.origin, Evidence.raw number) in modules/house_numbering/domain.py.
- [X] T005 Implement pure proposal transition policy and authority gate (data-model.md: Proposal.status, Proposal.rule version, Building.eligibility, Building.reason; AddressNumber.kind) in modules/house_numbering/domain.py.
- [X] T006 Run unit tests and record exact evidence in specs/019-house-numbering-plugin/evidence/domain.md.
**Checkpoint**: 17 tests and Ruff passed; [domain evidence](evidence/domain.md). Additional pure helpers cover route projection, unique source match, progress counts, export allowlist and certificate gating; all integrated tasks remain open.

## Phase 2: Core prerequisites (BLOCKS all integrated story tasks)
- [ ] T007 Verify exact promoted Core baseline and resolve audit-018 G001/G003/G004/G005/G009/G010 in specs/019-house-numbering-plugin/evidence/dependency-gate.md; do not mark done before proof.
- [ ] T008 Define and test generic command/UI/lifecycle capabilities and module-owned migration dispatch without vertical dependency in apps/api/openvietgis_api/domains/extensions/ and packages/public-contract/.
- [ ] T009 Add generic activation/disable, permission and revision support with RLS and concurrency tests in apps/api/openvietgis_api/adapters/http/extensions.py and tests/modules/test_house_numbering_integration.py.
- [ ] T010 Design typed module-owned transactional storage and migrations covering all entity ownership/revision/actor invariants in modules/house_numbering/migrations/ and persistence.py.
- [ ] T011 Enforce protected data isolation and audit; data-model.md RestrictedHousehold.purpose, consent/basis, protected identity fields, access scope and expiry; Export/Audit.scope, explicit fields/format, actor/action/time in modules/house_numbering/persistence.py.
## Phase 3: US1 — Activation, spatial registry
- [ ] T012 [US1] Test module absent, enabled, disabled, other project/organization, Land Asset coexistence in tests/modules/test_house_numbering_integration.py.
- [ ] T013 [US1] Implement manifest and generic contributions; data-model.md Activation.manifest version/state, project in modules/house_numbering/manifest.json and application.py.
- [ ] T014 [US1] Add area/route/building references/invariants: data-model.md Area.kind/parent/geometry ref, Route.type/parent/prefix/geometry ref/direction/source/status, Building.geometry ref/route/area/parcel ref/eligibility/reason/state/revision in modules/house_numbering/persistence.py.
- [ ] T015 [US1] Implement lifecycle, scope tree, CRUD commands with Core geometry and change-history contract in modules/house_numbering/application.py.
- [ ] T016 [US1] Build localized area tree, map-linked table, inspector, pin/edit commands in modules/house_numbering/frontend/Workspace.tsx.
- [ ] T017 [US1] Run US1 browser acceptance and direct unauthorized route tests in tests/e2e/house-numbering.spec.ts.
## Phase 4: US2 — Inspect and numbering
- [ ] T018 [US2] Write real geometry tests for oriented route side/measure, nearest tie, nested alley, no CRS and threshold in tests/modules/test_house_numbering_integration.py.
- [ ] T019 [US2] Implement inspection including evidence, neighbors and parcel via Core queries in modules/house_numbering/application.py.
- [ ] T020 [US2] Implement protected numbering storage: data-model.md AddressNumber.route/building/number string/kind/rule version, Rule.immutable version/direction/parity/suffix/max distance/authority citation in modules/house_numbering/persistence.py.
- [ ] T021 [US2] Implement atomic preview/apply, idempotency, stale conflict and bulk changes: data-model.md Preview.operation/base revision(s)/candidate changes/warnings/expiry/token in modules/house_numbering/application.py.
- [ ] T022 [US2] Implement map pick, preview diff, confirm, bulk pins and grid display in modules/house_numbering/frontend/NumberingPanel.tsx.
- [ ] T023 [US2] Verify US2 browser and atomicity scenarios in tests/e2e/house-numbering.spec.ts.
## Phase 5: US3 — Approval
- [ ] T024 [US3] Test duplicate, revision race, nonauthoritative rule, withdrawal/decline, role revocation and historic retention in tests/modules/test_house_numbering_integration.py.
- [ ] T025 [US3] Implement proposal/items and approved uniqueness: data-model.md Proposal.status/items/comments/rule version/actor; AddressNumber.kind; Building.eligibility/reason in modules/house_numbering/persistence.py.
- [ ] T026 [US3] Implement authorized review transitions, plaque status, split/merge and immutable history in modules/house_numbering/application.py.
- [ ] T027 [US3] Implement submission and review queue UI in modules/house_numbering/frontend/ProposalPanel.tsx.
- [ ] T028 [US3] Run US3 browser tests in tests/e2e/house-numbering.spec.ts.
## Phase 6: US4 — Import and reconciliation
- [ ] T029 [US4] Add synthetic CAD, spreadsheets, GIS/CRS, encoding, duplicate and uncertain-match fixtures and integration tests in tests/modules/test_house_numbering_integration.py.
- [ ] T030 [US4] Implement staged imports and accounting: data-model.md ImportBatch.source fingerprint/CRS/mapping/counts/original artifact ref/status and Evidence.origin/raw number/date/source ID/confidence/private metadata reference in modules/house_numbering/imports.py.
- [ ] T031 [US4] Implement old-before-new unique evidence matching and manual queue: data-model.md ReconcileCase.evidence refs/class/estimate flag/resolution/actor in modules/house_numbering/reconciliation.py.
- [ ] T032 [US4] Implement import mapping/preview and three-source review UI in modules/house_numbering/frontend/ReconcilePanel.tsx.
- [ ] T033 [US4] Verify replay, counts, privacy and 7-group browser evidence in tests/e2e/house-numbering.spec.ts.
## Phase 7: US5 — Survey and progress
- [ ] T034 [US5] Test 50 offline synthetic surveys/photo replay, GPS warning, conflict/delegation and privacy in tests/e2e/house-numbering.spec.ts.
- [ ] T035 [US5] Implement scoped survey/assignment storage: data-model.md Survey.client UUID/building/point/CRS/photos/state/recorded by/submitted by and Assignment/Target.scope/actor/date interval/target in modules/house_numbering/survey.py.
- [ ] T036 [US5] Implement Core offline sync adapter, attachment checks and delegated recovery in modules/house_numbering/survey.py.
- [ ] T037 [US5] Implement mobile survey, queue, consent control and progress dashboard in modules/house_numbering/frontend/SurveyPanel.tsx.
- [ ] T038 [US5] Implement accurate scoped metrics and safe Excel report in modules/house_numbering/progress.py.
## Phase 8: US6 — Dossiers and output
- [ ] T039 [US6] Test approved-only issuance, missing imagery, template gate, reissue, batch 100 and audit in tests/modules/test_house_numbering_integration.py.
- [ ] T040 [US6] Implement inspection/certificate ledger: data-model.md Inspection/Certificate.scope/building/number/source/case/template revision/status/artifact/issue/reissue link in modules/house_numbering/documents.py.
- [ ] T041 [US6] Implement protected dossier lookup, three-map Word output, PDF/plate batch issue using Core artifact services in modules/house_numbering/documents.py.
- [ ] T042 [US6] Implement inspection form, document history and print UI in modules/house_numbering/frontend/DocumentsPanel.tsx.
- [ ] T043 [US6] Run independent template/export validation in tests/e2e/house-numbering.spec.ts.
## Phase 9: US7 — Search and handover
- [ ] T044 [US7] Test scoped address/coordinate/old-number search, redacted standard outputs, unauthorized private disclosure and CAD/GIS round-trip in tests/modules/test_house_numbering_integration.py.
- [ ] T045 [US7] Implement search and scoped export adapters using verified Core formats; data-model.md Export/Audit.scope/explicit fields/format/artifact and manifest/actor/action/time in modules/house_numbering/exports.py.
- [ ] T046 [US7] Implement standard metadata/public package vs explicitly authorized original with read/export audit in modules/house_numbering/exports.py.
- [ ] T047 [US7] Implement search and export dialog in modules/house_numbering/frontend/SearchExportPanel.tsx.
- [ ] T048 [US7] Run independent output-open and browser validation in tests/e2e/house-numbering.spec.ts.
## Final Phase: Polish / convergence
- [ ] T049 Run architecture guard and regression including Core without module, Land Asset, load/reopen and mobile in tests/architecture/ and tests/e2e/.
- [ ] T050 Run exact candidate benchmark 10k/100k records, 50 concurrent users, and record p95 evidence in specs/019-house-numbering-plugin/evidence/performance.md.
- [ ] T051 Run actual isolated backup/restore both database and artifact with rollback, record in specs/019-house-numbering-plugin/evidence/recovery.md.
- [ ] T052 Run security reviews, PII-leak scan, authorization negative and full acceptance quickstart; record gaps in specs/019-house-numbering-plugin/evidence/convergence.md.
- [ ] T053 Update ROADMAP.md R014 child 019 status only based on verified evidence; do not promote externally without owner/legal/data gates.
## Dependencies & execution order
Phase 1 pure, no Core dependency. Phase 2 blocks all integrated tasks; US1 precedes US2/3 and provides map scope. US4 uses US1/2 domain; US5 needs field/offline gate and US1/4 validated ingestion. US6 needs US2/3 and actual output/artifact contracts. US7 requires US1/3/4. Each story independently tests its scope. Unit tests before rules; integration and E2E tests before feature implementation. Distinct stories can be implemented independently only after their prerequisites and with isolated worktrees.
## Implementation strategy
MVP is US1–3 with real map, inspect, preview, apply and authorized approval; deliver later increments US4–7 with full gates. No automatic change to production.
