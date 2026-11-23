# Agent 02 — Frontend Parity & Consolidation Audit (P0/B)

**Baseline audited:** `main@fc8b8de` · **Date:** 2026-11-23

## 1. Canonical Live Entrypoint (Verified)
- **Live public frontend:** `apps/web-next` (94 source files under `apps/web-next/src`), built with Vite (`--base=/rewrite/`) and served via `/var/www/openvietgis-hn019/rewrite` -> immutable release directory under `/var/www/openvietgis-releases/`.
- **Legacy SPA:** `apps/web` (146 source files under `apps/web/src`), retained for compatibility and root build checks; `apps/web-next` is now explicitly gated in root `validate:ts` (`package.json`, merged via PR #14 + PR #2).

## 2. Feature & Route Parity Matrix

| Capability | `apps/web-next` (Canonical Live) | `apps/web` (Legacy) | Evidence / Notes |
|---|---|---|---|
| OIDC / Session Login & Tenant Switch | **Owner** (`LiveApp.tsx`, `WorkspacePicker`) | Present (`features/auth/`) | Verified live on `openvietgis.cloud` |
| Spatial Layers, Map & Attribute Table | **Owner** (`LiveApp.tsx`, `QgisWorkspace.tsx`, `LayerProperties.tsx`) | Present (`features/map/`, `features/spatial/`) | Renderer imports now isolated through `apps/web-next/src/map-surface.ts` (`check_scope.py` = OK) |
| Editing, Snap, Ortho, Vertex/Split | **Owner** (`AdvancedEditor.tsx`, `CoreMapTools.tsx`, `ProfessionalTools.tsx`) | Present (`features/editing/`) | Active in Studio toolbar |
| CAD Import & DXF Exchange | **Owner** (`LiveApp.tsx`, `IntegrationCatalog.tsx`) | Present (`features/cad/`) | R026 AutoCAD 2D DXF export live |
| 3D Footprints, Terrain, 3D Tiles | **Owner** (`SpatialWorkspace.tsx` + `@openvietgis/spatial-workspace`) | Present | Renderer isolated via `packages/spatial-workspace/src/renderer3d.ts` |
| Raster / COG / Sentinel-2 / DEM | **Owner** (`RasterWorkbench.tsx`, `RasterReportWorkbench.tsx`, `SentinelWorkbench.tsx`, `ProjectRaster.tsx`) | Limited | Live in `web-next` |
| Layout Designer & Print Export | **Owner** (`LayoutDesigner.tsx`, `PrintDesigner.tsx`, `LayoutRender.ts`) | Present (`features/layout/`) | Live in `web-next` |
| AI Copilot | **Owner** (`ai-copilot/`, `AiCopilotPanel`) | Not primary | Live in `web-next` |
| Integration Hub (R026/R027) | **Owner** (`IntegrationCatalog.tsx`, `QgisBindingManager.tsx`, top-bar modal) | Not present | Dedicated top-bar button + 1520px modal |
| MicroStation V8i Bridge Panel (R025) | Not yet ported | Present (`PlatformHubPage.tsx`, `MicroStationBridgePanel.tsx`) | Read-only PoC in `apps/web` (PR #15) |
| Official & Partner Plugins (House Numbering) | **Owner** (`LiveApp.tsx` dynamic glob `../../../modules/*/frontend/studio.tsx`, `ExternalPluginHub`) | Present (`features/extensions/`) | Vertical decoupled from Core (`DeveloperPortal.tsx` cleaned) |
| Team Access & Developer Portal | **Owner** (`TeamAccess`, `DeveloperPortal.tsx`) | Present (`features/access/`) | Live in `web-next` |
| Offline Field Sync | **UNKNOWN (runtime)** — `OfflineFieldPage` chunk built | Present (`features/offline/`) | Needs explicit browser E2E verification |

## 3. Non-Breaking Consolidation Plan
1. Keep `apps/web-next` as the single canonical production entrypoint (`validate:ts` already gates `web-next` build).
2. Port `MicroStationBridgePanel.tsx` into `apps/web-next` Integration Hub when R025 moves beyond read-only PoC.
3. Freeze new feature work in `apps/web` and deprecate only after offline field E2E parity is verified in `apps/web-next`.
