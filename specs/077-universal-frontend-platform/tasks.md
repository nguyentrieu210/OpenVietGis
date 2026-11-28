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
- [ ] T301c Real MapLibre/Cesium/CAD lifecycle, keyboard drag/accessibility, layout performance, native IPC window qualification and application integration/rollback after W0/W1 gates.
## W4 GIS/CAD
- [x] T401a Shared GIS/CAD UI: LayerTree, Legend, CrsSelector, SymbolEditor/Preview, ColorRamp, CoordinateInput, SnapToolbar and PropertyEditor with typed host intents and scoped styles.
- [x] T401b Validate inputs, capabilities, revision-bound property updates; TypeScript, Vite and 18/18 isolated tests (5 W4 + 13 regressions).
- [ ] T401c Wire host adapters to existing map/layer/style/CRS/digitizing/CAD services; verify real render/event/edit-state, print isolation and legacy parity after integration gates.
## W5 data
- [x] T501a TanStack Table/Virtual grid and virtual tree candidate, host paging/filter/sort intents, selection, typed edits and synthetic 100k rows test.
- [ ] T501b Real server cursor paging, tree keyboard semantics, performance benchmark and app wiring.
## W6 Flutter
- [x] T601a Flutter package with W1 ThemeData, button/field/layer/status/adaptive workspace and widget test sources.
- [ ] T601b Flutter compile/device tests, shared icons, offline/GPS and actual mobile app integration.
## W7 plugin UI
- [x] T701a React presentation adapter for SPEC075 read-only nodes and host session, bounds and invalid action tests; no independent signature/runtime.
- [ ] T701b Wire canonical SPEC075 parsers/session, version negotiation and existing Flutter renderer, verify cross-platform session/revocation and migrate plugins.
## W8 commands
- [x] T801a Command registry, palette, normalized shortcut collision, dynamic capability guard, vi/en labels and CoreCommand adapter.
- [ ] T801b Integrate existing CoreCommandBar/ribbon/menu and all legacy aliases/context keys; keyboard/accessibility tests.
## W9 theming/accessibility
- [x] T901a Per-account preferences helpers and theme/density/locale/contrast/motion panel, tested dark/high contrast and normalization.
- [ ] T901b Integrate existing account preference and translation systems; screen reader, forced colors and native preference parity.
## W10 migration
- [x] T1001a First reversible Web slice: generated token imports, remove duplicate SPEC072 token blocks, shared compatibility facade and pnpm lock; fixture baseline/candidate and Web tests.
- [ ] T1001b Full per-domain adoption (GIS/CAD/DataGrid/Dockview/AI/photo/plugins), native branch convergence, verified legacy CSS removal.
## W11 qualification
- [x] T1101a Package exports, demo and release checklist, dedicated UI qualification workflow, local evidence.
- [ ] T1101b Full integrated CI result, Flutter/Tauri build, renderer and device qualification, security/visual/performance parity and release artifacts; no deployment implied.
