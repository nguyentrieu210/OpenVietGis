# Unified v1.0 — tracked work

All tracks below are required; partial checks are not release acceptance.

- [x] Consolidate former v1.1 scope into the single v1.0 target.
- [ ] Connector credential storage hardening — candidate implementation and focused tests; Windows DPAPI runtime acceptance pending.
- [ ] Connector idempotency/delta/bootstrap hardening — candidate implementation and focused tests; native QGIS, concurrency and transport failure tests pending.
- [ ] Durable draft/outbox and uncertain-commit recovery across QGIS restart.
- [ ] Desktop OAuth code+PKCE, secure credential persistence, project picker and revocation.
- [ ] Real web-next/PostGIS/OIDC E2E for all core GIS and team workflows.
- [ ] Topology, spatial join, processing, modeler and layout acceptance.
- [ ] CAD/3D acceptance including safe lifecycle and asset permissions.
- [ ] Raster, NDVI and change-analysis acceptance.
- [ ] AI/MCP real authorization and controlled-write acceptance.
- [ ] Plugin SDK/marketplace security, compatibility and distribution acceptance.
- [ ] Basemap registry, rights review and map-publication controls.
- [ ] Clean install, migration, performance, backup/restore and rollback.
- [ ] Independent user acceptance, Windows QGIS, full exact-SHA CI, security/license review.
- [ ] Approved stable release and atomic live replacement.

## Evidence — 2026-11-24

The connector hardening candidate based on main 8d6a914 passed 65 focused Python tests (including six new regression cases) and targeted Ruff checks. These include exact-payload retry identity, changed-payload denial, no insecure Base64 fallback and static cursor checks. Native QGIS/Windows and complete release evidence are still missing. No live deployment or stable tag is implied.

### GIS/CAD/3D workspace usability — candidate evidence (2026-11-24)
- An authorized project can enter a 3D workspace before choosing height, terrain, models or tiles. The empty state provides configuration entry points; source validation remains mandatory before rendering actual data.
- CAD/3D disabled states explain loading, access restrictions, missing permission or an active business plugin, with an explicit return to GIS Core. No authorization grants are widened.
- Project Studio remounts on a confirmed scope/permission change so stale alternate-mode state is discarded.
- Isolated candidate checks: web-next TypeScript and production build passed; spatial-workspace unit suite 36/36 passed; Playwright spatial-mode smoke with synthetic API fixtures passed empty 3D, configured 3D, CAD upload/revisions, GIS return, permission revocation and access restoration. These are not production or real PostGIS/OIDC acceptance.

- Follow-up candidate: direct CAD/3D mode requests from an authorized business plugin switch to GIS Core automatically. Browser fixture tests for both modes passed; restricted access remains denied.

- Core/plugin coexistence candidate: removed plugin-based mode and source-navigation gates, removed automatic plugin dismissal and Core prompt, retained project.read and scoped-access checks, blocked mode changes while a plugin modal is open. Browser fixture verifies CAD and 3D with the active plugin, preserved navigator state on return to GIS, and restricted access denial. Typecheck, build and 36 spatial workspace unit tests passed.

### Native DXF 2D fallback and real PostGIS checks (2026-11-24)
- Native ASCII DXF preview for supported planar LINE/CIRCLE/POINT and straight LWPOLYLINE, bounded to 4 MiB/5,000 rendered entities. New simple R12 drawings allow numeric-coordinate drawing and editing of LINE/CIRCLE/POINT, with verified SHA-256 and optimistic version receipts. Imported complex drawings are read-only; DWG/binary DXF and external editor remain unavailable without a verified engine.
- Node parser tests 3/3; independent ezdxf read and audit of generated LINE/CIRCLE/POINT file; browser fixture created a drawing, saved real DXF bytes, reopened, edited and saved another version while preserving the original. Typecheck and build passed.
- Nine existing CAD/3D integration tests passed against a fresh, isolated PostGIS 18/3.7 container (CAD versions and georeference, DEM and raster terrain, model vault, saved scenes, tileset upload and generation). The test container was removed; no production data used. These are not full authenticated production acceptance.
