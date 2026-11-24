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

### Full OpenCADStudio integration (2026-11-24)
- Official upstream OpenCADStudio v2026.46 web release, archive SHA-256 66ebd8c5ef4cc415e85ab19d25f7b4ee7613f157cbf524216928ae13eaf5554e; embedded release metadata source commit 3976af345b0f244e59f5c5e8c40c0e42805a72d2 verified through GitHub. GPL-3.0 and exact upstream source archive published at https://cad.gpcds.site/ovg-cad-2026.46/NOTICE.txt; license and source alongside notice. Unmodified JS/WASM engine, adapted HTML and relay only. No third-party analytics or standalone CAD AI control script in embedded app. CAD origin is separate from GIS; relay pins source window, channel, parent origin and release capabilities. CSP restricts frame ancestry and network to the versioned CAD assets. No GIS credentials or project IDs passed to engine.
- Browser evidence using actual WASM on loopback and actual HTTPS relay: synthetic DXF and upstream DWG fixture each opened and saved with correlated acknowledgment. Public HTTPS relay test passed both formats with no failed or out-of-scope requests. GIS frontend fixture opened actual DWG canvas, saved version 2 with SHA-256/byte-count verification and unchanged original, opened historical read-only revision. Fixed host false dirty state after verified save. Separate browser test opened new blank R2018 DXF directly in full editor; independent ezdxf parser verified template. Tests use synthetic project/API fixtures, not customer data. Native selection events are not supported by this official release, and the custom source branch with native selection is not deployed. Full command-by-command CAD acceptance remains pending.

### CAD Studio-first UX (2026-11-24)
- Based on deployed ab38492, preserving spatial-performance/QGIS updates. Authorized full-engine CAD mode opens the latest drawing immediately, or an unsaved R2018 DXF draft if the project is empty and writable. Explicit source-link choices still use the version-pinned library path. Read-only empty projects do not create drafts. No storage write until Save; first save creates a document with verified receipt, subsequent saves retain optimistic revision checks. Upload opens the editor directly; library/history is accessible via Bản vẽ. Clean CAD can release the mode guard; dirty/saving state blocks switching. License/source moved to the collapsed Giới thiệu CAD menu; unsupported selection panel hidden. Existing native DXF fallback remains unchanged.
- Typecheck and build passed; separate browser smoke runs used official live WASM with synthetic project API fixtures: immediate DWG open, save v2, original preserved, historical read-only; empty project auto-opens blank DXF, zero storage writes until explicit Save, named draft persisted as v1; license link available in menu. Not customer-data acceptance.

### Integrated CAD toolbar and branding (2026-11-24)
- CAD document controls are portaled into the GIS/CAD/3D toolbar, removing the extra editor header. Compact save status and about menu retain access to license/source. Full official WASM browser fixtures passed blank draft save and DWG open/save/history, with toolbar location asserted.
- CAD web shell now uses OpenVietGIS mark, title and favicon. Exact wrapper changes published at https://cad.gpcds.site/ovg-cad-2026.46/branding-wrapper.patch, and notice updated. Official WASM unchanged (SHA-256 5a31c1c467ea698cf4502bff1e3e30c6f694324a4fe0edaf06d1ffa9eaffdc0d).
- Separate downstream source candidate on CAD VPS: branch openvietcad/branding-v1, commit 8c6b4b749. Logo and product name changes in Rust, Vietnamese defaults, original license/notices retained. Not yet compiled/accepted/published as a replacement WASM. No full upstream ribbon redesign or native selection acceptance claimed.

### Cesium geographic 3D renderer candidate (2026-11-24)
- Based on deployed bda9703. CesiumJS 1.147.0 (Apache-2.0) is pinned in the lockfile, lazy-loaded as the official prebuilt ESM distribution; Workers, Assets, Widgets, ThirdParty and license are self-hosted at the versioned /rewrite/cesium/1.147.0/ path. No Cesium ion token, CDN or external imagery is introduced. Natural Earth bundled imagery provides a low-resolution reference globe.
- The empty 3D mode opens a real globe; polygon extrusion uses Cesium entities with holes/multipolygon support, DEM and validated static GLB/GLTF are rendered as ECEF primitives. Camera/visibility, project selection, saved scene revision and scope revocation remain supported. Existing import limits and relative height semantics are unchanged. Model materials use bounded quantized colors, not full textured glTF. GIS 2D and CAD remain unchanged.
- Imported non-provenance 3D Tiles use Cesium3DTileset streaming through the project-scoped same-origin authenticated API; an explicit compatibility switch retains the MapLibre renderer for older GLB. Generated tiles with triangle-to-GIS/CAD provenance retain the previous renderer until native feature picking equivalence is established. No arbitrary external tile URLs. Native tile budget uses 64 MiB cache and 64 MiB overflow.
- Candidate evidence: TypeScript both packages passed; web-next production build passed; spatial-workspace 36/36 unit tests passed. Browser fixtures passed terrain + GLB + polygon + revocation; multi-model pixel-based pick and source GIS navigation; saved scene camera/visibility/revision round trip; native 3D Tiles with 2238 red pixels and same-project requests, compatibility switch, empty globe and revocation; GIS/CAD/3D/plugin regression; model vault immutable revisions. Synthetic API fixtures, real Chromium/WebGL/Cesium runtime, not authenticated customer data. No full textured-model/point-cloud/IFC acceptance or production deployment claimed.

### Earth-style Cesium navigation candidate
- Explicit left pan, wheel/right zoom, middle/Ctrl/Shift tilt, double-click flight, N/R and north/top/tilt controls. Target frame rate 60 instead of 30; no MSAA, request-render retained, bounded quality modes and DPR-aware resolution. Camera navigation uses globe ray intersection rather than synchronous depth picking; feature identification still uses scene.pick.
- Optional Sentinel-2 cloudless imagery from existing EOX provider (10 m, not parcel-resolution), scoped project raster imagery, bundled Natural Earth fallback. No unofficial Google tiles newly introduced. Raster selection uses authorized project URLs and rectangle bounds.
- Before depth-picking adjustment: 38/38 unit tests, Earth navigation/quality/project raster test and five browser regressions passed. Render pixel counts 710976 / 2502630 for smooth/sharp in fixture. After depth-picking adjustment typecheck/build passed and camera/quality/raster portion passed again; full drag benchmark and final revocation wait timed out under concurrent software-GPU workloads. Not a measured 60 FPS result, not accepted for production yet. Extended gestures opt-in with OVG_EARTH_GESTURES=1.

Release verification: reran final depth-picking-disabled candidate with bounded 115-second browser timeout; Earth navigation quality/raster/permission revocation passed (710976 to 2502630 pixels, 15 raster requests). Final 38/38 unit tests and both TypeScript checks/build passed. Native tiles (2240 visible red pixels), model selection and saved-scene regression passed. Extended drag benchmark and hardware FPS remain unverified.
