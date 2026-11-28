# Seventeenth verification slice
Base d38317f9be18829cc396e8b91e5b754db4f46727; candidate Rust binary from the identical source tree previously built with Rust 1.85.1. No production runtime code changes in this slice.

New reproducible integration suite tests/spec065/edit_processing.py, run through the existing isolated PostGIS runner. Actual Rust API uses a disposable copied binary under UID/GID 65534, a non-superuser/NOBYPASSRLS runtime role and synthetic tenant identities. Fresh migrations through 0046 on isolated loopback 54332. Final integrated invocation exit 0:
- RLS without tenant scope exposes zero spatial layers.
- Two separately authenticated authorized editors submit different changesets against the same base; one 201, one 409. Identical replay returns the same ID; altered intent conflicts; invalid mixed changeset rolls back atomically; persisted revert and replay restore original attributes. Pool size 1, so parallel HTTP requests do not prove concurrent database transaction races.
- Buffer point radius 10m, 8 quad segments, area matches planar analytical oracle. Clip two polygons total 10000m2; grouped dissolve two results total 30000m2. Result rows measured in real PostGIS, exact replay and conflicting intent checked; foreign tenant denied.
- Queued job, cancel, retry and succeeded terminal cancellation rejection.
- Raster NDVI real GDAL worker: 3x3 EPSG:3857, 8 valid/1 NoData, mean .5; returns actual GeoTIFF. Persistent NIR/Red and derived NDVI preserve input IDs and statistics, stored file downloads; DEM slope 45 and aspect 270; cross-tenant analysis and download denied.
- Layout two independent cameras, second authorized editor updates to revision 2, stale revision 1 rejected, invalid map link rejected and foreign read denied.
- GeoJSON upload/inspect/confirm/export/reimport via existing E07 oracle: Unicode, zero, false, empty string and null preserved, CRS 4326, artifact SHA256, stale inspection, CSRF and tenant boundaries, replay.
- Earlier failed test attempts: fixture reference keys needed underscores; persisted revert status is "reverted", not "committed". Corrected test assertions to match existing wire contract. Final full suite PASS. No source code or runtime configuration was altered to bypass security.
- QGIS Desktop paired screenshots/outputs, large-source geometry algorithms, full transfer format matrix, collaboration, full canonical/release gates and real simultaneous transaction races remain. The fixture container was removed; no live changes.
