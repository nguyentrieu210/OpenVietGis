# Tasks — R029 (all pending)
## Gate A — architecture and native spike
- [x] T001 [US1,US2] Inventory R002/R006/R027/R021 source and tests; ADR exact transaction/ACL paths.
- [ ] T002 [US2] Native QGIS provider/interception spike: save, rejection, offline, undo; record Linux/Windows evidence; choose transparent provider or explicit managed save fallback.
- [ ] T003 [US3] ADR commit-ordered cursor and atomic event coverage; map schema/catalog/ACL visibility changes.
- [ ] T004 [US1] Verify human auth provider PKCE/device flow and session threat model.
- [ ] T005 [US1,US3] Freeze protocol and contract tests; approve Phase 0 before implementation.
## Gate B — server (depends T005)
- [ ] T006 [US1] Session/binding capabilities, permission epoch, admin revocation, no plaintext secrets.
- [ ] T007 [US3] Migration + transactional durable events integrated with R006; no duplicate mutations.
- [ ] T008 [US3] Scoped bootstrap/delta, ordered pagination, visibility reset, retention.
- [ ] T009 [US2,US4] R006 adapter/receipt lookup, revision conflicts, idempotency and deletion policy.
- [ ] T010 [US1,US3] SSE bounded auth recheck, heartbeat, cursor reconnect; rate limit.
- [ ] T011 [US1,US2,US3] Real PostGIS RLS/ACL, tenant and service ceiling adversarial tests.
## Gate C — QGIS (provider decision T002; contracts T005)
- [ ] T012 [US1] Managed session controller + auth UX, independent of dock; read-only on invalid auth.
- [ ] T013 [US2] Server-acknowledged save pipeline and staged edit recovery; native editing behavior as qualified.
- [ ] T014 [US3] Async scoped delta apply, cursor checkpoint, no full snapshot steady state.
- [ ] T015 [US4] Conflict UI, schema/CRS reset, optional advisory lease separate.
- [ ] T016 [US1,US2] Offline/revoke/expiry tests; no false saved state; cache cleanup policy.
- [ ] T017 [US2,US3] Native QGIS tests on Linux/Windows + R027 regression.
## Gate D — web/admin and AI
- [ ] T018 [US6] Integration Hub/QgisBindingManager managed session and permission controls (depends T006).
- [ ] T019 [US3,US6] Web event refresh/delta convergence, audit/status and UX smoke (depends T008).
- [ ] T020 [US5] Typed bridge threat model, session handshake, allowlist and consent (depends T005).
- [ ] T021 [US5] QGIS native tool integration and server MCP approval path (depends T012,T013,T020).
- [ ] T022 [US5] Bridge tests: revoked session, unsafe tool denied, consent rejected, no arbitrary execution.
## Gate E — verification and release
- [ ] T023 [US1-US4] E2E two QGIS + web + AI, real DB, reconnect, conflict and permission change.
- [ ] T024 [US3] Load reference workload, p95 <=2s target, no full layer transfer, retention/duplicate tests.
- [ ] T025 [US6] CI packaging, version compatibility, signed/checksummed artifact, feature flag default off, rollback.
- [ ] T026 [US6] Evidence matrix for every FR/SC, no feature claims without tests; release sign-off.
