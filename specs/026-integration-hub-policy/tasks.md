# Tasks — R026

- [x] T001 Isolated branch/worktree from verified main; review constitution.
- [x] T002 Spec + plan + agent kit tasks + Slice 2 read-only architecture (`qgis-postgis-architecture.md`).
- [x] T003 Adapter catalog schema, read-only exchange profiles (`profiles.py`), and deny-by-default policy.
- [x] T004 Project-scoped, read-only HTTP catalog and adapter profile endpoints (`/catalog`, `/adapters/{adapter_key}`).
- [x] T005 Unit tests for lifecycle, authorization, explicit CRS preservation, and no false write capability (30 `integration_hub` + 27 `authorization` = 57 passing).
- [ ] T006 Isolated integration test with real tenant/RLS, service principal ceiling, and `gis_access` scoped layer ACL — *tests authored & collected cleanly (`apps/api/tests/integration/integration_hub/test_integration_hub_rls_acl.py`, 3 tests); live PostGIS execution pending Docker/CI.*
- [x] T007 Read-only catalog and adapter profile panel inside Project Plugin manager; no mutation forms or write buttons.
- [ ] T007b Typed SDK (`packages/client-sdk/src/integration-hub.ts`), OpenAPI 3.1 contract (`contracts/integration-hub-api.yaml`), and browser E2E test — *typed SDK, OpenAPI 3.1 contract (`OPENAPI_OK`), and offline contract/UI smoke (`integration-hub-smoke.mjs`) complete; live browser E2E (`OVG_TEST_URL`) and `pnpm validate:ts` pending running stack/CI.*
- [ ] T008 Independent security review and release gates — *independent security review (`security-review.md`), zero-migration rollback plan (`rollback-plan.md`), and conflict-free `origin/main` reconciliation check complete; full CI + live PostGIS gate still required before merge.*
- [ ] T009 Production deploy with release SHA and rollback verification — *NOT deployed to staging or production; blocked until T006, T007b, T008, and live staging rollback drill pass.*
