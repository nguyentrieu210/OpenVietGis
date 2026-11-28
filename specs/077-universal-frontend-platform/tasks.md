# Tasks — implementation waves
## W0 baseline and governance
- [x] T001 Inspect SPEC072 and constitution; establish extension boundaries.
- [x] T002 Pin main and 073–076 source snapshots; enumerate scoped Git trees.
- [x] T003 Classify every tracked CSS path in web src and identify existing primitives/theme/workbench/plugin interfaces.
- [x] T004 Define candidate ADR, ownership, migration and acceptance contracts.
- [x] T005 Implement dependency-free audit utility and unit tests; runtime execution tracked separately.
- [x] T006a Run static audit and unit tests: 6/6 scopes and 3/3 tests PASS.
- [ ] T006b Capture baseline runtime screenshots/performance on a provisioned runner and close W0 qualification. No UI migration until baseline and candidate gates.
## W1 token system
- [x] T101a Define semantic tokens + validation; generate deterministic CSS/Tailwind/Dart; test light/dark/density/contrast and compatibility aliases.
- [x] T101b Run generator drift check, Node unit tests and isolated Chromium computed-style smoke.
- [ ] T101c Qualify Tailwind compiler integration and Flutter SDK tests on provisioned runners after W0 runtime gate; activate adapters through coordinated integration.
## W2 React foundation
- [x] T201a Candidate spike, exact Base UI/React pins, isolated npm lock and runtime notices; owned primitives and dialog/menu/tooltip/popover/form; compatibility facade.
- [x] T201b Isolated TypeScript, Vite build and Chromium Playwright 9/9; focus/keyboard/portal/overflow checks.
- [ ] T201c Activate workspace dependency and facade re-exports after W0/W1 gates; integrate pnpm lock, app-wide visual/accessibility and map lifecycle tests, final license review.
## W3 workbench
- [x] T301a Dockview 8.10.0 OSS adapter, registered panels, tabs/split/dock/float/resize/maximize/reset and scoped versioned persistence; window capabilities fail closed.
- [x] T301b TypeScript, production demo build, 4 W3 browser tests plus 9 W2 regression tests; synthetic canvas lifecycle and restore checks.
- [ ] T301c Real MapLibre/Cesium/CAD lifecycle, keyboard drag/accessibility, layout performance, native IPC window qualification and application integration/rollback after W0/W1 gates.
## W4 GIS/CAD
- [x] T401a Shared GIS/CAD UI: LayerTree, Legend, CrsSelector, SymbolEditor/Preview, ColorRamp, CoordinateInput, SnapToolbar and PropertyEditor with typed host intents and scoped styles.
- [x] T401b Validate inputs, capabilities, revision-bound property updates; TypeScript, Vite and 18/18 isolated tests (5 W4 + 13 regressions).
- [ ] T401c Wire host adapters to existing map/layer/style/CRS/digitizing/CAD services; verify real render/event/edit-state, print isolation and legacy parity after integration gates.
## W5 data
- [ ] T501 Virtual table/tree, server paging, filters, selection, editable cells, typed validation and large dataset benchmark.
## W6 Flutter
- [ ] T601 ThemeData generation, shared icons, buttons/forms/navigation/bottom sheets, field GPS/offline/map and tablet layout; native semantics tests.
## W7 plugin UI
- [ ] T701 Extend SPEC075 schema/version negotiation and both renderers; capability enforcement, schema bounds and invalid payload tests; first/third-party migration.
## W8 commands
- [ ] T801 Extend CoreCommand registry, shortcut collision, context menus/ribbon/palette; localizations and command permission tests.
## W9 theming/accessibility
- [ ] T901 Per-account preferences, high contrast, reduced motion, bilingual strings, keyboard and screen reader checks.
## W10 migration
- [ ] T1001 Migrate each legacy surface by owner; Web/Desktop/Flutter integration, AI and photo; remove only verified redundant rules with reversible commits.
## W11 qualification
- [ ] T1101 SDK exports/examples/docs, visual baselines, performance/security/compatibility matrix, CI, artifacts and release evidence; no deployment implied.
