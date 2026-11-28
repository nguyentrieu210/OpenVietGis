# Wave 01: persistent shell and public studio SDK

- Moved shared StudioHost contracts, dialogs, splitter and CSS to public `@openvietgis/spatial-workspace/plugin/*` exports; existing Core imports are compatibility re-exports. Ten module files no longer import app internals.
- Isolated plugin hook lifecycle in a keyed, memoized runtime child; the GIS shell is independent of plugin loading and activation. Access-scope and permission changes still intentionally invalidate the shell for security. Activation refresh errors clear extension state.
- Architecture checker scans module frontends and rejects reverse app imports; 13 architecture tests passed; repository architecture check passed.
- Core and module TypeScript checks passed. Local browser smoke (`plugin-runtime-smoke.mjs`, mocked tenant and API data) passed activation, Core switch, plugin reopen and removal while preserving map/canvas identity; two feature requests for two initial layers.
- Scope: E01/E02/E05/E11 advanced; not all acceptance criteria met. Full draft/camera/undo/permission-change and multi-plugin regressions, isolated module build and migration remain open. Initial active-plugin selection retained for compatibility with existing projects; no main/live rollout yet.
- Frozen offline pnpm lock verification blocked by missing cached geotiff metadata; dependency links and TypeScript validated using installed dependencies, not a fresh install.
