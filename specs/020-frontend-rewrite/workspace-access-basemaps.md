# Workspace, imagery and granular GIS access

Updated 2026-11-23 (Asia/Ho_Chi_Minh).

## Workspace UX
- Compact organization/project breadcrumb with searchable pickers, project/org creation actions and directly accessible access-management modal.
- Account popover exposes identity, organization role, project, sign-out, and self-owned avatar upload. Images are decoded in browser, cropped to 192×192 JPEG, re-encoded server-side, size-limited, CSRF protected and stored in RLS-protected identity.user_avatars.
- The layer dock has exclusive basemap choices, context operations, source-preserving retire confirmation, and no redundant layer status footer. Footer presents editable coordinate, scale, rotation, magnification and CRS controls.
- Retiring a layer changes only its lifecycle: source and objects are retained. Server uses expected layer revision.

## Basemaps
- OSM default. EOX Terrain Light and historic 2016 Sentinel-2 Cloudless are separate raster sources with attribution.
- Sentinel-2 L2A by month: search Planetary Computer STAC at current map center and month, choose scene with lowest reported scene cloud cover, display acquisition date and cloud cover. This is a selected real acquisition, not a cloud-free monthly composite. Load explicitly when changing area/month. Resolution is limited by source imagery.
- Google Satellite and Hybrid use legacy public endpoints at user request; opt-in only, no proxy, no harvesting, attribution displayed. These are not officially supported Map Tiles API and availability is not guaranteed. Official integration requires valid configured provider credentials.
- Preserve data/overlays while changing raster source. Source choice stored per user/project on device.

## Authorization
- Existing organization roles and organization/workspace/project grants remain the base authorization. Organization owner/admin can manage rules; users still require a base grant.
- Scoped membership opt-in makes unmatched GIS layer/object/action inaccessible. Rule subjects are user or project group; rule predicates combine layer, exact feature IDs, JSON attribute equality and WGS84 polygon coverage. Actions: read, create, geometry, attributes, delete, export, approve; read is required with each rule. Rules are additive allow rules within base permission.
- Removing all rules or group membership does not restore broad access. Owner/admin remains unrestricted. New rules do not affect unconfigured members; no production rules are seeded.
- Database RLS filters GIS catalog, source fields, features. Project guard denies all unreviewed project endpoints to scoped users (including plugin-specific records, offline packages and legacy exports). Scoped export has a dedicated filtered GeoJSON/CSV endpoint. Changesets validate before and after snapshots; changing an object's attributes or moving it outside permitted geometry is forbidden. Internal edit mode is only used by validated change transactions to maintain revision pins.
- Review applies to committed changesets where every operation is within the reviewer's scope; it records decisions and does not automatically publish, roll back, or overwrite. Reviewers cannot approve their own changesets.
- Access modal tabs: members, project grants, GIS rules, groups, review, audit and invitation. Open directly via header, layer context menu or selected object. Rule creation can start with object IDs / current viewport bounds. Scoped users can view their rules and review data if authorized.
- Production migration: 0023_user_avatar then 0023_gis_access. Pre-migration pg_dump archives retained, data and activations are not changed.

## Validation
- Isolated PostgreSQL tests: avatar private CRUD, tenant access regressions, scoped feature reads and writes, rejection of hidden objects / attribute and regional boundary changes, filtered export, group removal, review, manual and professional tools, cross-tenant checks.
- Browser smoke: source switch and persistence, actual legacy Google image response, real Sentinel STAC month, plugin activation, account image, grants, retire confirmation. Browser smoke mutations use fixtures, not the production survey.
