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
- [ ] T101 Define semantic tokens + validation; generate deterministic CSS/Tailwind/Dart; test light/dark/density/contrast and compatibility aliases.
## W2 React foundation
- [ ] T201 Candidate spike/pins/license notices; extract primitives and dialog/menu/tooltip/popover/form; focus/keyboard/portal/overflow tests; facade SPEC072.
## W3 workbench
- [ ] T301 Compare Dockview adapter with DockLayout; tab/split/dock/resize/restore, versioned persistence, map lifecycle, IPC window capability; rollback.
## W4 GIS/CAD
- [ ] T401 Layer tree/legend/CRS/symbol/property/CAD controls; map event and edit-state parity; isolate renderer and print styling.
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
