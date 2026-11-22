# OpenVietGIS — GIS Core / plugin boundary (2026-11-23)

## Production composition

The platform is a modular monolith. GIS Core is independently deployable and does not import house-numbering implementation. The project workspace is composed with a selected, activated official plugin without moving to a different page or duplicating the map.

- Core: identity, organizations/projects, team and permissions, spatial sources/layers, map viewport, selection, attributes, geometry editing, import/transfer, spatial and processing APIs, generic extension manager.
- Plugin: house-numbering catalogs (areas, roads/alleys, buildings, rules), survey filters/legend/labels, commands, inspection, numbering, proposals, reconciliation, field survey, reports and handover. The Bình Hưng project is a tenant deployment, not Core.
- Backend module: `modules/house_numbering/runtime.py`, its own `manifest.json`, domain code and migrations in the `house_numbering` schema. Runtime discovery uses generic official-module manifests; module calls the public server SDK.
- Frontend module entry: `modules/<extension_key>/frontend/studio.tsx`, exports `StudioPlugin` from the public frontend host contract in `apps/web-next/src/studio-contract.ts`. Dynamic manifest discovery uses Vite glob imports; code and CSS are lazy-loaded only when project activation and chosen workspace match the module. New projects without activation use only Core.
- Public host contract: read spatial rows, layer/selection context, project permissions, refresh, map selection/fit and safe mutation wrapper. Contributions: toolbar, map panel, inspector, modal, filter/paint/rank/label, search and layer grouping. `mapRevision` signals changes to map projection.
- Generic plugin manager uses Core activation APIs. Install/register and activate a plugin for a project; open Core without disabling the plugin; disable without deleting GIS layers or plugin data. Lifecycle mutating endpoints enforce server-side authorization. Each selected workspace is keyed by the plugin id; the contribution hook remains fixed for that mount.

## Safety and test strategy

No data migration required; keep spatial data, module records, projects and activation unchanged. Production frontend no longer includes the earlier demo bootstrap or sample import panel. Originals backed up before rollout.

- Typecheck Core and plugin separately.
- Run Core-only build with the plugin directory absent: no plugin JS/CSS assets emitted.
- Run integrated build: own lazy module JS/CSS chunk.
- Browser contract test `apps/web-next/core-plugin-smoke.mjs`: fixture project starts Core-only, enables plugin, verifies plugin API calls and layer grouping/filter, closes plugin to open Core, disables plugin with mock API. This does not mutate production.
- Backend unit gates: extension metadata boundary and history.
- Rollback: restore prior source and prior index.html from the pre-cutover backup; versioned assets retained.
