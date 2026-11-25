# Tasks
- [x] T001 Inventory entrypoints, old bridge and tooling dependencies
- [x] T002 Write specification, plan, acceptance and tasks
- [x] T003 Migrate reachable feature dependency closure to canonical frontend (26 source files; superseded catalog and transfer pages excluded)
- [x] T004 Remove cross-app bridge; enforce Vite and architecture boundary; canonical build PASS
- [ ] T005 API client consolidated; protected-query unit tests PASS; authenticated lifecycle acceptance pending
- [ ] T006 Native UI modernization for migrated workbenches: forms, tables, tool search, status and dialog behavior
- [ ] T007 Preserve standalone offline routes, worker, existing drafts and sync; verify
- [ ] T008 Map historical routes and replace legacy end-to-end tests with equivalent canonical tests
- [ ] T009 Consolidate pnpm manifest/lockfile/dev scripts/Docker/CI/architecture checks
- [ ] T010 Retire apps/web after independent build and parity gate
- [ ] T011 GIS: QGIS-grade editing, snap, selection, attributes, layer and navigation improvements
- [ ] T012 CAD: verify engine capabilities, align command/Osnap/Ortho/Polar/layers/open-save controls
- [ ] T013 Processing: searchable algorithms, parameter validation, job lifecycle/output/Model Builder
- [ ] T014 Layout/catalog/3D/plugin UI consistency and regression
- [ ] T015 Authenticated acceptance, security, offline, desktop/mobile screenshot QA and DEV release

## Implementation evidence
- Canonical frontend builds without importing apps/web source; Dockerfile now copies only its package manifest pending workspace retirement.
- Architecture checks: 11 tests PASS.
- DEV browser fixtures: desktop modal and Space/layout regressions PASS.
- Processing: server algorithm catalog, parameter validation, Buffer segments, status filtering, duplicate submission guard, and job polling implemented. Browser fixture PASS.
- Offline: standalone route and static-only service worker migrated. Loopback browser test with network disconnect, reload, retained draft, identical retry, receipt, and local lock PASS on rerun. First run timed out awaiting worker control. Actual account and DEV-scoped service worker remain unverified.
- Legacy workspace package and old route-specific test suite remain until compatibility replacement and deletion gate. Production unchanged.

## Second implementation checkpoint
- One ApiClient instance shared by canonical entrypoints and migrated workbenches; existing protected-read checks retained.
- Default pnpm dev:web and build:web select web-next. Loopback dev/preview API proxy configured.
- Historical project URLs select an accessible organization/project and open the matching workbench. Processing deep-link verified with browser fixture; account deep-link opens account menu. Full route parity not yet established.
- Migrated seven unit suites; 55 Vitest cases and 24 Node tests PASS. Typecheck and DEV build PASS.
- Workspace picker uses shared modal keyboard/focus behavior.
- Canonical processing deep-link and modal browser fixture PASS.
- Historical logout-switch test uses a different host and failed on an old login selector; no authenticated DEV logout acceptance.
- 23 legacy E2E spec files still require migration and acceptance. Old package/workspace retained; no claim of complete retirement or production readiness.
