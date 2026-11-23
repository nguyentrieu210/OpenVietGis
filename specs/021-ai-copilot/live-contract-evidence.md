# Spec 021 — independent live contract verification
Date: 2026-11-23 (Vietnam)

## Acceptance
- `apps/mcp/.venv/bin/python -m pytest -q test_mcp.py test_live_bridge.py test_remote.py` in `apps/mcp`: 27 tests passed against synthetic data, including real HTTP MCP protocol, signed token claims and rejection of invalid identity/scope.
- `node apps/mcp/web/browser-smoke.mjs`: PASS. First-party connection, no manual token, deny, fresh permission check, camera navigation, disconnect. Browser used fixture bridge.
- `apps/mcp/.venv/bin/python apps/mcp/verify_live_contract.py`: PASS against canonical HTTPS production. OIDC issuer and all authorization/token/JWKS endpoints match, S256 PKCE advertised, core readiness 200, identity links and project membership requests without a session rejected with 401. Remote MCP endpoint remained 404 pending approved deployment.
- Production identity link issuer aggregation (no subjects or credentials collected): 5 links use the canonical Keycloak issuer. This validates the exact issuer comparison in GisIdentity, not a real user session.

## Release gate
After configuring the authorized OAuth client, scope/audience, first-party GIS integration and HTTPS reverse proxy, run the credential-free verifier with `--expect-mcp`. It requires protected resource metadata and an HTTP 401 for anonymous MCP tool listing. Then run an authorized user-session acceptance of the project, with consent for each command; do not use a service token or bypass user authentication for this test.

No project data, identity settings, secrets, or production reverse proxy were changed by this verification. This evidence does not mark T004 or T006 complete.
