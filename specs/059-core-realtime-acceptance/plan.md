# Implementation plan — Spec 059

## Design
Keep ADR-005 PostGIS authority, ADR-007 ChangeSet, ADR-008 module isolation, ADR-014 tenant context/RLS. Reuse existing adapters and contracts. No ADR replacement intended.
- Data path: existing spatial domain + HTTP adapters + PostGIS -> revision-pinned API -> scoped snapshots/MVT/delta -> WebGIS/QGIS.
- Core/plugin path: generic feature/source identity, permissions, selection and revision events -> plugin-specific link/approval validation; no Core imports of house_numbering.
- Existing touchpoints (inspect before changing): apps/api/openvietgis_api/domains/{spatial,editing,extensions,authorization,integration_hub}; apps/web-next/src/{SpatialCache,SpatialSyncCore,SpatialIncremental,QgisWorkspace,LiveApp}; packages/{client-sdk,plugin-sdk,spatial-workspace}; integrations/qgis_plugin; modules/house_numbering.
- QA: pinned fixtures and profiles; real API + disposable PostGIS/OIDC, Chromium, native QGIS. Review legacy visual baseline diffs, not silent refresh. Keep synthetic only by default.
- Parallelism: isolate changes by worktree after contracts frozen, no concurrent heavy builds on host. Integrate on exact SHA; independent review and security gate. Approval required for live promotion.

## Gates
G0 baseline/traceability -> G1 data/CRS/ACL -> G2 realtime/durable drafts -> G3 reference plugin/SDK -> G4 full integration/recovery -> G5 independent acceptance and release decision. If a gate fails, log cause, repair within scope, rerun; blocked remains blocked. Link evidence to original R031–R047 requirements.
