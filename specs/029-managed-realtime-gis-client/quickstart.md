# Acceptance quickstart (planned)
1. Use isolated test database and two organizations with viewer/editor/admin users, approved managed binding and one explicit-CRS layer with 10k fixtures. Enable test flag only.
2. Install candidate plugin on native QGIS Linux and Windows; connect as separate named users. Open WebGIS and authorized MCP session.
3. AI creates/edits feature A after approval; both QGIS clients update. QGIS editor stages feature B and saves; confirm R006 receipt and WebGIS update, no full GPKG reload. Check audit actor and source.
4. Viewer and forged raw API attempts denied. Restrict field and region; attempt forbidden edits; revoke editor with connection open; verify read-only and denial. Disconnect during save; test response loss and original operation receipt.
5. Edit same feature concurrently; confirm conflict, preserve pending edits; reconcile explicitly. Restart server and drop SSE; replay durable cursor, test retention reset and visibility reset.
6. Verify explicit CRS, schema change, deletion default denied, AI consent rejection, no arbitrary tools, no credential leakage. Record native QGIS versions/OS, p50/p95/p99 and logs. Run legacy R027 regression and rollback feature flag.
No production data or deployment required for initial qualification.
