# Engine scope registry

All slices are specification candidates, not implemented. Scope = existing supported behavior on frozen baseline plus explicitly approved improvements; static presence is not evidence of operation. Optional desktop and custom GPU renderer are discovery-only.

| Engine | Scope | Parent references | Child spec |
|---|---|---|---|
| E01 | Nền tảng, identity, tenant, security | R001 R002 R015 | [spec](engines/e01-platform-security.md) |
| E02 | Project, catalog, data, CRS | R003 R004 | [spec](engines/e02-project-data-crs.md) |
| E03 | Geometry, editing, topology | R006 | [spec](engines/e03-geometry-editing.md) |
| E04 | Queries, expression, attribute table | R007 | [spec](engines/e04-query-attributes.md) |
| E05 | Map rendering, tile, cache | R004 R043 | [spec](engines/e05-render-tiles.md) |
| E06 | Symbology, labels, layout | R009 | [spec](engines/e06-cartography-layout.md) |
| E07 | Import/export and interoperability formats | R005 R010 | [spec](engines/e07-formats-transfer.md) |
| E08 | Processing, models, jobs | R008 | [spec](engines/e08-processing-jobs.md) |
| E09 | Raster, terrain, survey | R008 R012 | [spec](engines/e09-raster-terrain.md) |
| E10 | CAD, 3D, model and scene | R012 R037 | [spec](engines/e10-cad-3d.md) |
| E11 | Realtime, edit leases, chat, tasks | R011 R040 | [spec](engines/e11-sync-collaboration.md) |
| E12 | QGIS, external sources, integration | R010 R027 | [spec](engines/e12-connectors-external.md) |
| E13 | Field, offline, mobile | R013 | [spec](engines/e13-field-offline.md) |
| E14 | Plugin SDK, marketplace, modules | R014 | [spec](engines/e14-plugin-marketplace.md) |
| E15 | House numbering and land asset module | R014 R019 R051 | [spec](engines/e15-house-numbering.md) |
| E16 | AI and MCP | R041 R021 | [spec](engines/e16-ai-mcp.md) |
| E17 | Canonical web, desktop-ready shell | R037 R038 R062 | [spec](engines/e17-frontend-workspace.md) |
| E18 | Ops, deployment, backup, performance | R015 R042 R043 R044 R045 R047 | [spec](engines/e18-ops-release.md) |
