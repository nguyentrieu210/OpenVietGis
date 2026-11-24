# Plan
1. Inspect managed session, desktop commands, actor context, project RLS, GIS ACL and credential paths.
2. Centralize session access guard and enforce endpoint-specific owner/manager policy.
3. Protect project-wide epoch mutation; reject stale session operations and preserve delta reset.
4. Use conservative reset for feature-scoped delta rather than exposing historical hidden IDs.
5. Align UI/SDK revoke behavior; run identity/authorization, isolated PostGIS and focused SDK tests, frontend build.
6. Document residual authorization risks, commit and retain live state.
