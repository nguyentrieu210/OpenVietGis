# GeoLibre-WASM 1.8.4 — initial library evaluation

Exact npm package pinned in isolated PoC. MIT metadata and packaged LICENSE and
THIRD-PARTY-NOTICES exist; transitive license/security review is still required.
Library WASM is about 6.70 MB, CLI WASM about 24.42 MB (uncompressed); lazy load
only if a future candidate is approved.

Node WASM library verified against a local synthetic GeoJSON fixture: geometry,
Vietnamese text and typed scalar properties survived. The WGS84 to EPSG:3857
control point agrees with the independent spherical Mercator formula at 1 mm.
However, conversion omits the input GeoJSON Feature.id. This is a blocking
failure for authoritative editing/import/delta paths. Preserve existing PostGIS
and GDAL processing. Evaluate preview-only bounded raster/local processing
in a browser worker separately; never assume the WASI CLI has been tested.

See geolibre-smoke.json for 30 warm 10k-feature parsing samples. This measures
Node conversion plus JSON parsing, not browser paint, full application latency
or comparison with existing engine. No app dependencies or runtime changes.
