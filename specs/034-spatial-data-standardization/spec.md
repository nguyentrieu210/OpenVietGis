# Spec 034 — Spatial data and CRS standardization
Parent: R003/R005/R006; depends on Spec 030/033.

Objective: make existing spatial identity, CRS and revision contracts consistent across Core, transfers and desktop clients without silently changing stored coordinates.

- Use the existing immutable CRS registry as the backend definition of supported and readable CRS; definition-only profiles are not operational approval.
- Native coordinates retain their declared CRS and XY order. Display coordinates and derived projections must be explicitly distinguished; no guessing CRS or applying display coordinates as native edits.
- Identify an object by tenant/project/source/feature ID; layer ID is a presentation reference, not a globally unique feature identity. Source content and schema revisions remain separate.
- Managed QGIS bootstrap must reject inconsistent layer/source/CRS/dimensions/schema/revisions/permission epoch, duplicate feature IDs, reserved field collisions and invalid pagination before materializing the affected layer.
- Non-null geometry decode failure must not silently become NULL.
- Validate existing spatial/transfer/editing tests, focused managed snapshot tests, and regenerated connector package.

Non-goals: no migration, CRS coverage expansion, production deployment or claim of complete geodetic/native QGIS acceptance. Further server-side transaction snapshot, alias-layer event routing, authoritative WKT verification and native desktop geometry roundtrip need separate evidence.
