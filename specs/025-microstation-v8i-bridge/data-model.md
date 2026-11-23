# Data Model (proposed)

## Local snapshot (implemented contract in P1)
- `schema_version`: `ovg.v8i.snapshot.v1`
- `drawing_id`: operator-controlled stable identifier (not raw local path).
- `model_id`: CAD model name/identifier.
- `source_crs`: explicit EPSG code as string, or authoritative WKT in later revision; source units preserved.
- `elements[]`: unique `element_id`, `geometry.type`, `geometry.coordinates`, map of non-sensitive scalar `properties`.
- `unsupported_count`: number of elements not losslessly represented.
- `revision_digest`: reproducible hash of validated canonical content (derived, not caller asserted).

## Proposed backend tables (NOT migrated)
- `integration.device_connections`: id, organization_id, project_id, token_digest, revoked_at, last_seen, capabilities.
- `integration.drawing_bindings`: id, connection_id, drawing_fingerprint, model_id, source_crs, target_layer_id, active.
- `integration.cad_element_mapping`: binding_id, element_id, feature_id, last_digest, revision.
- `integration.cad_sync_batches`: id, binding_id, idempotency_key, snapshot_digest, status, requested_by, reviewed_by, review_time, failure_reason.

Each backend table requires FORCE RLS + explicit access checks and audit. Device-provided tenant or project identifiers may only be hints, never authority.
