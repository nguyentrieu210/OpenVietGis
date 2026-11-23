# R025 Bridge Exchange Contract v1

A local snapshot is JSON with `schema_version: ovg.v8i.snapshot.v1`, explicit `source_crs` like `EPSG:3405` (illustrative only; select real CRS per parcel dataset), `drawing_id`, `model_id`, and `elements` with unique `element_id`, GeoJSON-style geometry and bounded scalar properties.

**Important:** GeoJSON geometry syntax does **not** imply EPSG:4326 in this exchange contract. These are raw coordinates in `source_crs`; do not serve this snapshot as standards-compliant GeoJSON until transformed and verified.

The normalized snapshot has a deterministic SHA-256 digest. Diff operations are `upsert`, `unchanged`, `delete_candidate`; there is no API to apply a delete. Unsupported entities prevent claiming completeness.

The `fixtures` use synthetic geometry and identifiers; no real cadastral/private records.
