# Tasks — R026

- [x] T001 Isolated branch/worktree from verified main; review constitution.
- [x] T002 Spec + plan + agent kit tasks + Slice 2 read-only architecture (`qgis-postgis-architecture.md`).
- [x] T003 Adapter catalog schema, read-only exchange profiles (`profiles.py`), and deny-by-default policy.
- [x] T004 Project-scoped, read-only HTTP catalog and adapter profile endpoints (`/catalog`, `/adapters/{adapter_key}`).
- [x] T005 Unit tests for lifecycle, authorization, explicit CRS preservation, `.gpkg`/`.sql`/`.dxf` export, QGIS plugin client, and two-step `sync_diff` (127 `integration_hub` + 27 `authorization` = 154 passing).
- [x] T006 Isolated integration test with real tenant/RLS, service principal ceiling, `gis_access` scoped layer/feature ACL, AutoCAD DXF CRS guard, and QGIS editable-layer sync push — **5/5 passed** on isolated PostgreSQL 18.7 / PostGIS 3.7.0 (`postgis/postgis@sha256:fe440a27c6567aefa932e988962e5827bb2c29e4b97dac2ca5268f5eafe10262`, 25.80s on VPS Milion, commit `cdfa3dc`).
- [x] T007 Catalog, adapter profile, ACL-gated read-only export (`.gpkg`/`.sql`/`.geojson`/`.dxf`), and two-step QGIS Changeset sync panel inside Project Plugin manager.
- [ ] T007b Typed SDK (`packages/client-sdk/src/integration-hub.ts`), OpenAPI 3.1 contract (`contracts/integration-hub-api.yaml`), and browser E2E test — *OpenAPI 3.1 (`OPENAPI_OK`), `@openvietgis/client-sdk` (`eslint`, `tsc`, **75/75 Vitest passed** on Node 26.13.1), `@openvietgis/web-next` (`tsc`, Vite 8.3.8 production build), and `integration-hub-smoke.mjs` passed at `5caec1f`; live Playwright browser E2E (`OVG_TEST_URL`) still pending.*
- [ ] T008 Independent security review and release gates — *independent security review (`security-review.md`), zero-migration rollback plan (`rollback-plan.md`), conflict-free `origin/main` check, and live PostGIS T006 (5/5) complete; full main-branch CI reconciliation still required before merge.*
- [ ] T009 Production deploy with release SHA and rollback verification — *NOT deployed to staging or production; blocked until T006, T007b, T008, and live staging rollback drill pass.*
