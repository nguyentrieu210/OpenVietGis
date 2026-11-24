# Implementation plan — unified v1.0

1. **Foundation:** preserve identity/RLS boundaries, secure credential storage, exact-payload idempotency, delta ordering and bootstrap revision consistency. Review session ownership, local pending recovery and provider failure handling.
2. **Desktop authentication:** extend the existing identity service with a dedicated desktop OAuth public-client flow, PKCE and explicit consent. No embedded Google password entry, cookie copying, long-lived service keys or relaxed ACL. Verify token rotation, revoke, expiry and project discovery with both read-only and editor identities.
3. **GIS and collaboration:** canonical web-next E2E using real isolated PostGIS/OIDC; verify import/export, editing, topology/query, modeler, layout, team and offline workflows. Use synthetic fixtures.
4. **Advanced capabilities:** review existing CAD/3D, raster, AI/MCP, plugin SDK and marketplace implementations against spec.md. Extend only verified gaps; keep feature flags and dependency/license boundaries.
5. **Release candidate:** one pinned source/profile, full CI, clean bootstrap, migration from supported previous schema, performance benchmark, backup/restore and rollback. Independent Windows QGIS and GIS-user tests; basemap/source compliance and security review.
6. **Promotion:** only after recorded release approval. Replace old live entrypoint atomically; retain rollback release and database backup. Never erase historical releases or user data as a version-management shortcut.

Worktree and feature branches are implementation isolation, not separate product versions. Parallel work must converge on one release candidate before promotion.