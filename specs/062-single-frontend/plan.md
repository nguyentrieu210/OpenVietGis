# Plan: Single frontend
1. Inventory dependency graph and entrypoints; classify reachable code vs obsolete shell, with explicit test mapping.
2. Move reachable modules into apps/web-next/src/workbenches; consolidate lazy imports and retain security/offline contracts.
3. Adapt presentation to canonical UI (shared dialogs, forms, compact dock, data tables, tool search, status and error handling).
4. Integrate standalone offline route and worker lifecycle; preserve public compatibility paths, API and identity behavior.
5. Update workspace manifest, lockfile, Docker, local dev proxy, Playwright, architecture/scope checks and documentation. Remove old app only after independent build/acceptance.
6. Upgrade QGIS navigation/editing/layers, AutoCAD command/layer/navigation and ArcGIS processing/catalog/layout in focused regression-tested slices.
7. Run typecheck/build, browser fixtures, real API integration, offline and security checks; review screenshots and performance. Deploy DEV only; production is a separate gate.
