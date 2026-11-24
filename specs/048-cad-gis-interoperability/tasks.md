# Tasks: R048 CAD–GIS–3D interoperability

**Prerequisites:** spec.md, plan.md, research.md, data-model.md, contracts/interop.md. Tests mandatory; [P] only isolated work, not concurrent edits to shared files. See evidence/implementation-20261124.md for bounded completed work and remaining gates. Checkmarks do not certify production.

## Phase 1 — setup and evidence
- [x] T001 Audit existing R006/R027/R029/R031 code and record pinned source inventory in evidence/
- [x] T002 Inventory actual OpenVietCAD commands, browser support, file formats and capabilities in evidence/
- [x] T003 Create synthetic geometry/CRS/DWG/DXF fixtures and expected results under apps/api/tests/integration/spatial/
- [ ] T004 Reconcile R031 reviewed merged contract and main before changing shared adapter; record gate in evidence/

## Phase 2 — US6 independent CAD stability
- [ ] T005 [P] [US6] Benchmark cold/warm WASM and worker bytes/start on fixed network/device; OpenVietCAD CI evidence
- [x] T006 [P] [US6] Add representative drawing/edit/snap/undo/3D and DWG/DXF browser fixtures in OpenVietCAD ci/ovg/
- [ ] T007 [US6] Test runtime branding/theme/locale preferences; preserve old settings and license; OpenVietCAD web/ and src/app/
- [x] T008 [US6] Add pinned Cargo caching and artifact performance gates in OpenVietCAD .github/workflows/
- [ ] T009 [US6] Verify lazy CAD initialization and hashed caching in packages/spatial-workspace/src/cad-editor.ts (after R031 if shared contract affected)
- [x] T010 [US6] Run CAD regression, license/source archive, rollback test; evidence/

## Phase 3 — US1 identity and navigation (R031 gate)
- [ ] T011 [P] [US1] Write link identity, cardinality, stale/denied and generation tests in apps/api/tests/integration/spatial/
- [ ] T012 [US1] Extend current CAD link model and cad_repository.py, reuse existing scoped revisions; additive migration only if needed
- [ ] T013 [US1] Adapt R031 descriptor and existing cad-editor.ts capability negotiation; no second registry
- [ ] T014 [US1] Implement authorized link resolution and selection/zoom in CadEditorPanel.tsx and SpatialWorkspace.tsx
- [ ] T015 [US1] Test linked GIS 2D/3D/CAD real browser selection, dirty guard and revoke in apps/web-next/

## Phase 4 — US2 precise interchange
- [x] T016 [P] [US2] Add independent projected/VN-2000 control and units/Z tests in apps/api/tests/integration/spatial/
- [x] T017 [US2] Extend georeference.py and current placement contract; reject unknown transform and report residuals
- [ ] T018 [US2] Implement versioned entity/layer/attribute/style mapping + loss manifest using existing CAD domain
- [ ] T019 [US2] Validate topology, schema, precision and native blob lineage; test unsupported curves/blocks/solids
- [x] T020 [US2] Build preview and approved export UI using existing CAD panel and client SDK
- [ ] T021 [US2] Qualify DXF roundtrip and separate DWG adapter capability; compare geometry and metadata independently

## Phase 5 — US3 two-way reviewed edits
- [ ] T022 [P] [US3] Add idempotency, stale revision, permission and partial failure tests for existing CAD/R006 routes
- [ ] T023 [US3] Bind R027 direction, authority and mapping revision; reuse policy gates
- [ ] T024 [US3] Stage CAD blobs and publish version metadata/receipt with durable recovery in cad_repository.py
- [ ] T025 [US3] Implement preview-to-R006 commit and GIS-to-CAD proposed version through current SDK/routes
- [ ] T026 [US3] Add explicit delete impact review, revert and immutable audit using existing R006
- [ ] T027 [US3] Test true two-way edits, duplicate request, lost response and failure between stores

## Phase 6 — US4 realtime/conflict
- [x] T028 [P] [US4] Prove native CAD entity-delta support and handle stability; gate unsupported operations
- [ ] T029 [US4] Reuse R029 cursor/outbox and origin dedup; filter deltas on current ACL
- [ ] T030 [US4] Apply incremental qualified changes or mark document stale; preserve dirty draft
- [ ] T031 [US4] Reuse conflict records and authorized resolution; advisory leases optional
- [ ] T032 [US4] Run two-user, reconnect, expiry, duplicate/reorder and 100-case convergence suite

## Phase 7 — US5 governance/offline/AI
- [ ] T033 [US5] Apply tenant/project/layer/field/region policy to read/preview/commit/replay; fail closed on file-level unsupported policy
- [ ] T034 [US5] Implement revocation, permission epoch, scoped cache quarantine and visible read-only state
- [ ] T035 [US5] Reuse existing offline draft/outbox policy; no authoritative offline writes in managed mode
- [ ] T036 [US5] Register qualified commands in current MCP allowlist with preview/approval; test forged calls
- [ ] T037 [US5] Qualify QGIS/V8i adapter separately; document unsupported capabilities and Windows tests

## Phase 8 — release gate
- [ ] T038 [P] Run all negative security tests and relay origin/channel/size checks
- [ ] T039 [P] Measure SC-001..006 with published fixture/device/network and exact artifact digests
- [ ] T040 Validate real PostGIS and actual WASM cross-engine workflow, migration/restore/rollback
- [ ] T041 Update capability docs, corresponding source/license notices, evidence and explicit remaining limitations
- [ ] T042 Run exact-SHA quality gate and human acceptance; deploy only with separate authorization
