# Data Model
ProjectScope: accountId, organizationId, projectId (each nonempty and matched exactly). Scope change resets mode, selection and pending requests.
SpatialMode: gis, cad, 3d. ModeCapability.available boolean and reason when unavailable. Host must calculate actual authorization and readiness.
WorkspaceSession: scope, active mode, pending request or null, monotonically increasing sequence, error or null. Current active mode stays until matching request succeeds.
ModeLoadRequest: mode, sequence, scope. Only a matching current pending request may complete.
ScopedSelection and CameraReference: host-owned and explicitly scoped; geographic camera includes CRS and optional height datum. Never transfer local drawing coordinates implicitly.
AssetLink: future bridge; project-owned native and derived asset IDs plus provenance and georeference metadata. No DB changes in first increment.
