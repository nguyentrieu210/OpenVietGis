# Tasks — implementation waves
## W0 baseline and governance
- [x] T001 Inspect SPEC072 and constitution; establish extension boundaries.
- [x] T002 Pin main and 073–076 source snapshots; enumerate scoped Git trees.
- [x] T003 Classify every tracked CSS path in web src and identify existing primitives/theme/workbench/plugin interfaces.
- [x] T004 Define candidate ADR, ownership, migration and acceptance contracts.
- [x] T005 Implement dependency-free audit utility and unit tests; runtime execution tracked separately.
- [x] T006a Run static audit and unit tests: 6/6 scopes and 3/3 tests PASS.
- [x] T006b1 Pinned Web baseline build, six viewport screenshots/overflow and fixture transfer/CRUD smoke captured; corrected fixture context.
- [ ] T006b2 Complete renderer frame time, input latency, actual native/platform performance and app-wide visual baseline. No broad migration until qualified.
## W1 token system
- [x] T101a Define semantic tokens + validation; generate deterministic CSS/Tailwind/Dart; test light/dark/density/contrast and compatibility aliases.
- [x] T101b Run generator drift check, Node unit tests and isolated Chromium computed-style smoke.
- [x] T101c1 Tailwind compile and initial Web token activation tested on pinned Web fixture.
- [ ] T101c2 Flutter SDK test and coordinated native activation.
## W2 React foundation
- [x] T201a Candidate spike, exact Base UI/React pins, isolated npm lock and runtime notices; owned primitives and dialog/menu/tooltip/popover/form; compatibility facade.
- [x] T201b Isolated TypeScript, Vite build and Chromium Playwright 9/9; focus/keyboard/portal/overflow checks.
- [x] T201c1 Workspace dependency and SPEC072 compatibility facade activated on branch; frozen pnpm lock, Web build and 365 existing Web tests PASS.
- [ ] T201c2 Activate new primitives across all app surfaces; full visual/accessibility and final license review.
## W3 workbench
- [x] T301a Dockview 8.10.0 OSS adapter, registered panels, tabs/split/dock/float/resize/maximize/reset and scoped versioned persistence; window capabilities fail closed.
- [x] T301b TypeScript, production demo build, 4 W3 browser tests plus 9 W2 regression tests; synthetic canvas lifecycle and restore checks.
- [x] T301c1 Real MapLibre lifecycle fixture: same map mount, camera, selected feature state across tab/split/float/redock.
- [x] T301c2a Actual Studio opt-in Dockview beta: lazy loaded existing layers/map/inspector/table, host-owned presence, focus maximization, blocked layout controls and classic rollback; production browser smoke PASS.
- [ ] T301c2b Default adoption after Cesium/CAD lifecycle, unsaved edit/ACL parity, keyboard accessibility, performance and native IPC qualification.
## W4 GIS/CAD
- [x] T401a Shared GIS/CAD UI: LayerTree, Legend, CrsSelector, SymbolEditor/Preview, ColorRamp, CoordinateInput, SnapToolbar and PropertyEditor with typed host intents and scoped styles.
- [x] T401b Validate inputs, capabilities, revision-bound property updates; TypeScript, Vite and 18/18 isolated tests (5 W4 + 13 regressions).
- [x] T401c1 Existing CRS picker uses shared accent-insensitive CRS search; display-only semantics and VN-2000 smoke verified.
- [ ] T401c2 Remaining layer/style/symbol/snapping/CAD/print host adapters and parity.
## W5 data
- [x] T501a TanStack Table/Virtual grid and virtual tree candidate, host paging/filter/sort intents, selection, typed edits and synthetic 100k rows test.
- [x] T501b1 Shared TanStack row window in existing QGIS attribute table, preserving legacy selection, read ACL and cursor/paging code; scroll and selection smoke PASS.
- [ ] T501b2 Actual remote server cursor end-to-end, editable grid integration, tree keyboard semantics and large dataset latency benchmark.
## W6 Flutter
- [x] T601a Flutter package with W1 ThemeData, button/field/layer/status/adaptive workspace and widget test sources.
- [x] T601b1 Stacked PR #120: generated native light/dark/system theme, shared GPS survey controls, dependency vendor and host theme test.
- [ ] T601b2 Native build/device results, full component integration, shared icons, offline/GPS/renderer parity.
## W7 plugin UI
- [x] T701a React presentation adapter for SPEC075 read-only nodes and host session, bounds and invalid action tests; no independent signature/runtime.
- [x] T701b1 Stacked PR #121: canonical Web SPEC075 host consumes shared controlled document renderer; preserve parser/grant/proposal confirmation; build and SDK tests PASS.
- [ ] T701b2 End-to-end server/revocation/version negotiation, Flutter renderer/transport qualification, remaining plugin migration.
## W8 commands
- [x] T801a Command registry, palette, normalized shortcut collision, dynamic capability guard, vi/en labels and CoreCommand adapter.
- [x] T801b1 CoreCommandBar and ribbon consume shared discovery; exact host dispatch rechecks current enabled state and preserves arguments; legacy alias and keyboard behavior retained.
- [ ] T801b2 Full command registration for all native/plugin surfaces and keyboard/accessibility parity.
## W9 theming/accessibility
- [x] T901a Per-account preferences helpers and theme/density/locale/contrast/motion panel, tested dark/high contrast and normalization.
- [x] T901b1 Web AccountMenu stores theme/density/contrast/motion through shared per-account settings; migration and persistence smoke PASS.
- [ ] T901b2 Full bilingual application translation, screen reader, forced colors and native preference parity.
## W10 migration
- [x] T1001a First reversible Web slice: generated token imports, remove duplicate SPEC072 token blocks, shared compatibility facade and pnpm lock; fixture baseline/candidate and Web tests.
- [x] T1001b1 Web account settings, shared CRS search and table row window, modal/chat stacking fix; native and plugin stacked candidates.
- [x] T1001b2a Move SPEC072 compatibility CSS and preference control CSS to shared package; no global reset or deleted domain selectors; build and smoke PASS.
- [ ] T1001b2b Full per-domain adoption (GIS/CAD/DataGrid/Dockview/AI/photo/plugins), native branch convergence, verified legacy CSS removal.
## W11 qualification
- [x] T1101a Package exports, demo and release checklist, dedicated UI qualification workflow, local evidence.
- [x] T1101b1 Integrated local 21/21 UI regression, Web 314 Vitest + 53 Node, real MapLibre fixture, account/CRS/table/CRUD smoke. CI workflow duplicate key and observed Flutter import/focus failures repaired.
- [ ] T1101b2 Green remote CI on final commit, Flutter/Tauri build, renderer and device qualification, security/visual/performance parity and release artifacts.
