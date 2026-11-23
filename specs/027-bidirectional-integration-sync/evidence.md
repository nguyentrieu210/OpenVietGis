# R027 implementation evidence — 2026-11-23

**Scope:** Branch-only Foundation Slice A; not production, not an end-to-end two-way connection.
**Branch:** `feature/027-bidirectional-integration-sync` (rebased logically via merge from verified `origin/main@cdcd5ff` in an isolated worktree).

## Implemented in this slice

1. Pure, deterministic, side-effect-free **three-way reconciliation planner** (`apps/api/openvietgis_api/domains/integration_hub/bidirectional.py`): compares baseline, OpenVietGIS and external snapshots. Detects conflicts at field/geometry level, holds deletion candidates, rejects duplicate/partial data and unsupported geometry. Proposes safe disjoint attribute/geometry edits in both directions, with revision-bound SHA-256 plan digest. It **does not execute** any writes or manage real external revisions.
2. Human project-manager **QGIS binding draft registry** with new PostgreSQL migration `0039_r027_connector_bindings`. Stores only metadata and authorized field IDs, exact source revision pins and validated CRS. State is `draft`, `paused` or `revoked`; the database CHECK and API both prohibit `active`. No credentials, outbound traffic, agent pairing, data import, deletes or direct DB access are introduced.
3. Scoped `GET/POST .../integration-hub/bindings` and `POST .../bindings/{binding_id}/lifecycle` for draft/pause/revoke. Manager-only, CSRF on writes, RLS and GIS-scope deny; optimistic revisions; duplicate active binding identities rejected. All responses include `connected:false`, `writes_allowed:false`, `deletes_allowed:false` and explicit non-pairing reason.
4. Typed `packages/client-sdk/src/integration-hub.ts` methods for those safe metadata endpoints.

## Tests performed

- R027 three-way planner: **14 unit test cases passed** on the isolated working tree.
- All `apps/api/tests/unit/integration_hub` suites: **PASS**; includes R026 regression coverage.
- `apps/api/tests/integration/integration_hub`: **PASS**, including R027 real HTTP + PostgreSQL 18/PostGIS 3.7 integration case with migration `0039`, owner, cross-tenant denial, anonymous denial, duplicate-binding 409, invalid CRS and fields, stale lifecycle revision, disable and revoke.
- SDK `packages/client-sdk/tests/integration-bindings.test.ts`: **3 tests passed**; full SDK suite: **78/78 tests passed** (17 suites).
- SDK `tsc --noEmit` and ESLint against modified SDK files: **PASS**.
- API Ruff changed-file lint: **PASS**.
- The original pre-existing shared test database had a stale Alembic revision `0019_extension_lifecycle`; tests were deliberately rerun against a separate disposable, loopback-only PostGIS 18/3.7 container (`openvietgis_test`) without modifying that shared database.
- **No production migrations or live deployment executed.**

## Gaps and release blockers

- No verified QGIS desktop agent authentication/pairing or actual remote events.
- No durable external identity map, remote baseline storage, transactional inbox/outbox, consumer cursor or conditional receiver acknowledgment.
- Reconciliation planner is not yet wired into a trusted provider-specific sync preview endpoint; existing R026 GeoPackage preview/push remains a distinct single-direction route.
- No general bidirectional write execution, conflict resolution UI, automatic retries, deletion policy, backup/restore drill or real QGIS acceptance.
- Pending ADR/security review and source/target revision proof before any write activation. Do **not** set `writes_allowed` or `connected` true based only on a draft binding.

## Next development slice

Implement agent trust + durable baseline mapping and conditional source/target API contract; then wire planner to non-mutating preview and human-approved R006 changeset/local-QGIS apply with idempotent acknowledgments. Run end-to-end QGIS, real PostGIS/RLS and network-failure acceptance before any production release.

## 2026-11-23 — Foundation Slice B: QGIS manual bilateral workspace (experimental)

- Added pure-stdlib `integrations/qgis_plugin/openvietgis_connector/workspace.py`: SHA-verified common baseline, editable local GeoPackage, remote snapshot read, per-field/geometry three-way reconciliation, deletion/conflict quarantine, staged local apply, R026 server diff and R006 confirmed write, server read-back, recovery of ambiguous network acknowledgments.
- Integrated optional manual workflow into QGIS plugin UI: create/open workspace, preview, confirm sync and recover; metadata version 0.3.0 marked experimental, scrollable dialog.
- Added `apps/api/tests/unit/integration_hub/test_qgis_workspace.py` with actual GeoPackage read/write and simulated server calls, including independent two-way field/geometry edits, conflicts, deleted features, file drift, remote-only pull and uncertain write recovery.
- **No autonomous sync, server-side pairing, durable inbox/outbox, remote conditional agent API or cross-provider bridge.** QGIS native runtime acceptance and actual authenticated PostGIS HTTP round-trip remain separate release blockers.
- Real **HTTP + disposable PostGIS 18 / PostGIS 3.7 E2E passed** on a loopback-only test container: `apps/api/tests/integration/integration_hub/test_r027_manual_workspace.py`. The test creates a manual spatial layer, commits its baseline through real R006, edits one field locally in a GeoPackage, independently edits another via authenticated R006 HTTP, then uses the R027 workspace with R026 HTTP preview/push and server read-back to verify both edits are preserved. This uses a FastAPI TestClient session transport adapter (not a native running QGIS GUI and not a real bearer-authenticated network client).
- Full R026 + R027 Integration Hub HTTP/PostGIS integration suite: **7/7 passed** (including the new R027 end-to-end case) in an isolated database that migrated to `0039`.
- Workspace unit suite: **13/13 passed**, including conflict and pending-ack recovery cases; full Integration Hub unit suite: PASS. Ruff, compileall and architecture boundary check: PASS.
- No external connector activation, headless QGIS installation, desktop UI acceptance or production deployment occurred. **R027 desktop acceptance, ADR/security review and recovery drill remain release blockers.**
