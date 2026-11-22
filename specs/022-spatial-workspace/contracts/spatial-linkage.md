# Local VN-2000 control placement and source-object linkage (025)

## Requirement and scope

FR-007/008, SC-005 from spec 022: one authorized project, traceable source identity across engines, explicit CRS and precision. This slice does not expand R003's trusted-fixture datum-transform allowlist. Existing CAD release trust and production gates remain closed.

## VN-2000 placement

User chooses EPSG:3857 or EPSG:5899 (VN-2000 / TM-3 107°45′). 5899 is the only VN-2000 grid in the current readable registry; it is not a general province/central-meridian selector. The request includes 3–100 paired noncollinear controls in declared CAD units and grid metres; when 5899 is selected, `web_controls` additionally supplies the same CAD coordinates in the same order with independently known EPSG:3857 grid positions. They must describe the same locations.

Each pair set is independently fit with the bounded, non-reflecting local similarity algorithm. Both fits enforce the user's scale deviation and residual thresholds. For 5899 only, controls and every transformed geometry vertex must lie within 10 km of each fit's target centroid; native E/N sanity bounds and a coarse cross-grid location check reject grossly unrelated control positions (150 km guard, not an accuracy claim). The coarse check is not used for placement. The result is a derived EPSG:3857 GIS layer, not native 5899 vector storage or an automatic VN-2000 datum transformation. It retains the original immutable CAD version plus both fits, both extents, residuals, local radius, input digest and fingerprint in conversion metadata. Changing any control requires reinspection. Never infer 5899 from coordinates alone, convert other VN-2000 projections, claim survey accuracy or invent known controls. Existing EPSG:3857 fingerprints remain unchanged.

## Object-link contract

GET /api/v1/organizations/{org}/projects/{proj}/cad/documents/links?source_id={uuid}&feature_id={id}
GET /api/v1/organizations/{org}/projects/{proj}/cad/documents/{document_id}/versions/{version}/links?handle={hex}

Unrestricted human project.read; scope and permission verified before any lookup. Link authority is the immutable successful import conversion and matching document/version SHA, not editable feature attributes. Return items with document_id, version, filename, source_id, layer_id, feature_id, handle, layer_name, original_revision. Check active layer/source, existing feature and mapped or approximated entity; retired/removed items absent. If source revision differs from import, mark historical linkage (not geometric equality). Bounded 100 matching links, conflict if more; never choose a guessed result.

GIS and 3D share current single-feature key (source ID + FID); the 3D adapter highlights selected extrusion, and valid 3D pointer picks update GIS. For linked GIS or 3D feature, CAD source button opens the exact vault version/handle. In CAD vault, type handle for a selected version, resolve linked GIS objects and return to a currently visible authorized layer/feature; no auto-unhide, no silent context switch if blocked. This is source provenance and navigation, not live native CAD entity highlight, synchronization of geometry or write-back. Native CAD embed selection protocol is still unavailable/unverified. Bulk selection and 3D camera synchronization remain separate scope.

## Acceptance and rollback

- Disposable migrated PostGIS: matched dual controls, both extents, native metadata, actual stored EPSG:3857 geometry, source read EPSG:4326; reject missing or misordered/unrelated controls, stale fingerprint, unknown handle, unauthorized project; existing CAD vault regression.
- Real Chromium WebGL pick and disposal, candidate host smoke over mocked API: explicit two-grid form, GIS selection carried to 3D, exact version/handle CAD navigation and reverse link, authorization revocation. No production data.
- No DDL, new dependency or production config. Revert scoped commit to roll back candidate; old 3857 artifact compatibility maintained. Any already published derived layers retain metadata and remain readable with previous generic spatial source API. No new private-file egress.
