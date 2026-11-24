# Spec 036 — Identity and access control
Parent R002/R006/R015; depends on Spec 034/035.

Objective: distinguish project authorization, managed desktop actor ownership, management authority and GIS object visibility, without broadening any existing grant.

- Server resolves authenticated actor and current project permission; supplied session ID is not a credential.
- Session list and inspection: own actor, or project manager. Bootstrap, delta, commit, event subscription and desktop command response: own actor only. Queue commands for another actor only with project management permission; existing consent and tool allowlist retained.
- Reject stale permission epoch for active session operations; delta returns reset instead. Revoke own session without global epoch change; revoke others or bump project epoch only with management permission.
- Require session ID on managed bootstrap. Legacy authorized exports remain separate.
- Feature-scoped delta must not reveal operation IDs of invisible features; return reset requiring fresh ACL-filtered bootstrap.
- Match receipt actor type and ID. Normal UI revoke targets one session and acknowledges only server result.
- Validate with synthetic actors and isolated real PostGIS. No live grants, accounts, deployments, schema changes or credential rotation.

Full OAuth/credential binding, continuous SSE reauthorization, native QGIS revoke and MCP end-to-end are separate acceptance obligations.
