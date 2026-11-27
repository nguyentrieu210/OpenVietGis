# Candidate release procedure — do not apply automatically
1. Merge the approved frontend/API checkpoint and this branch, then run all tests against the merged release.
2. Register the dedicated public OAuth client and optional client scope in existing Keycloak using
   the administrator workflow. The JSON files are individual examples, not a replacement realm.
   Use exact Claude callback and PKCE S256, authorization code only, consent enabled, short access
   tokens, no implicit/direct/service grants. Never expose the existing GIS web client secret.
3. Verify the actual signed Keycloak access token has issuer, resource audience, allowed azp,
   gis:live scope, subject and expiry; test a wrong client/audience and logged-out user.
   Confirm MCP resource discovery, client discovery and a real Claude authorization-code exchange.
4. Run a single non-root process at loopback port 9187 using the pinned uv environment:
   uvicorn --factory remote_server:create_from_env --host 127.0.0.1 --port 9187 --workers 1 --no-access-log
   Set remote.env.example values through the service environment, no provider secrets required
   by the resource server. Verify loopback Core API 8019 is the correct trusted service.
5. Add reviewed Nginx locations in canonical openvietgis.cloud HTTPS configuration; define
   appropriate per-source request zones and monitor quotas. Test configuration and rollback before
   reload. Do not add /mcp to the legacy host. TLS and DNS already exist; not enabled by this branch.
6. Integrate the first-party panel using the section below; deploy frontend through the normal
   GIS release. Do not change the external plugin iframe permissions or expose its pairing secret.
7. With a real authorized test project, add a Claude custom connector using the exact MCP endpoint
   and registered client ID; authorize as the same user. Test navigation and refusal, project
   switch, revocation/logout, expiry, second tab, user isolation. Record evidence before promotion.
8. For release rollback remove the new Nginx locations and widget, stop sidecar and disable the
   dedicated Keycloak client. Existing GIS login and API stay untouched.

## Frontend integration for web-next

The canonical panel is already integrated in `apps/web-next/src/AiCopilotPanel.tsx`.
Its implementation lives in `apps/web-next/src/ai-copilot/`; use this source for
changes and smoke tests rather than creating a second copy under `apps/mcp/web/`.
The legacy scoped hotfix utility and browser checks remain for controlled recovery
and verification. Normal releases build the canonical frontend.
