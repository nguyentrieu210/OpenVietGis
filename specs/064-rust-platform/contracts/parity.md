# Contract acceptance matrix
Inventory legacy OpenAPI, SSE/WebSocket, connector, SDK and browser workflows pinned to baseline; map method/path, schema, status/error, permission, transaction/revision, pagination, cursor and client test to Rust implementation. Exact routes and schema must be extracted, not guessed.
- Identity: session, OIDC, CSRF, organization/resource permissions; deny cross-tenant and revoked credentials.
- Spatial: project/layer metadata, bounded feature queries, stable IDs, CRS, edits/deletes, export; real PostGIS golden fixtures.
- Tiles: scoped access, no shared private cache, revoke/invalidate, bounded zoom/geometry.
- Sync: preserve existing SSE contract via adapter where necessary; durable replay, duplicate/reorder/reconnect/conflict tests.
- Processing/modules: scoped jobs and outputs, plugin lifecycle/SDK, house-numbering, no core reverse dependency.
- Clients: canonical frontend, QGIS, CAD, 3D, layout, terrain, offline, MCP, chat; browser and integration regressions.
- Operations: health, observability, limits, migration/rollback, backup restore.
Every matrix row requires baseline reference, test ID and candidate result. Missing inventory or unapproved gap blocks cutover.
