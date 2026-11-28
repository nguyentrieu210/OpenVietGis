# CSS consolidation and integration with main
- Ribbon dropdown, locator search, nested plugin menu now share ui-menu.css. Removed the duplicated fixed menu rules in core-toolkit.css, qgis-theme.css and qgis-workspace.css; legacy base styles outside these fixed menu surfaces remain.
- Dialog shell geometry belongs to desktop-dialog.css; removed generic geometry from layer-properties.css and moved dialog inset/header/footer rules out of ui-density.css. Domain and renderer styles remain separate.
- Integrated main 4f778869 (OSM place search and navigation). Resolved ROADMAP and LiveApp conflicts preserving both feature sets; two SPEC 070 directories have distinct slugs and remain documented.
- TypeScript, 88 test files / 312 tests, production build, offline shell and synthetic Chromium SPEC070/071/072 passed on combined tree. Six viewport widths, modal spacing/overflow, theme persistence and layer dialog resizing verified.
- This is an incremental legacy consolidation, not proof that every CSS rule or dark-mode surface has migrated. Full screenshot regression and third-party UI SDK still open. No production deployment performed by this change.
