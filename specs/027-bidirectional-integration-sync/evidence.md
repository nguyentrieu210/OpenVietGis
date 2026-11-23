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