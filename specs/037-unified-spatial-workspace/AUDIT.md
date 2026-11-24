# Workspace standardization evidence
Baseline 6646115. Isolated candidate; no live change.

## Findings and changes
- LiveStudio required no active business extension and no feature loading before opening CAD/3D even after project read permission was known. The mode availability now uses verified unrestricted project read. Removed the redundant Core-only button and navigation prop. Business plugin remains active on return to GIS; existing plugin filters are preserved. Linked-feature requests retain exact organization/project/source/layer/revision and response CRS checks, including when a plugin is active.
- Inactive CAD candidate disposal previously cleared shared document state. Cleanup now only writes CAD state after activation. 3D candidate cleanup checks current engine ownership before clearing shared state. Tile pick/info callbacks and footprint/model selection callbacks require active engine state. Empty 3D remains usable without configured data; configured scene still requires camera and valid inputs.
- Existing coordinator rejects mode changes with dirty/saving blockers, cancels superseded requests, and revalidates permissions. Retained rather than replacing. No write permission expanded.

## Verification
- 38 spatial-workspace package tests passed, including stale preparation, scope change, revocation, failed activation and dirty/saving blockers.
- web-next TypeScript and production build passed.
- Chromium smoke on loopback preview with synthetic API and no external network: empty 3D and configuration entry, actual MapLibre 3D footprint canvas, CAD vault upload and version 2, mode switch blocked while mock save held pending, return to GIS, revoke and restore access, enabled house_numbering plugin → CAD → empty 3D → GIS without switching Core. No captured page errors.
- CAD save/version metadata exercised against mock API only; not a real CAD relay editing or backend persistence test.

## Remaining
CAD relay deployment/configuration, DWG/DXF editing fidelity, actual PostGIS-backed cross-mode selection and imports, DEM/glTF/3D Tiles real dataset performance, plugin filter interactions and independent scope/revocation acceptance are still open. GIS data snapshot changes may invalidate a configured scene by design; refresh-in-place and global project navigation dirty-state policy need further work. Full v1 acceptance remains gated.
