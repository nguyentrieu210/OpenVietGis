# ChatGPT and Claude — shared MCP endpoint

Production endpoint: https://openvietgis.cloud/mcp

## Configure ChatGPT (predefined OAuth client)
- Name: OpenVietGIS
- Authentication: OAuth (registered/static client)
- Client ID: openvietgis-mcp-chatgpt
- Client secret: none (public OAuth client).
- Scope: gis:live (default on the ChatGPT client); allow the actual ChatGPT request `openid email offline_access gis:live`. Email and offline-access scopes are optional and require user consent. Refresh tokens do not extend a running GIS browser session.
- Callback: https://chatgpt.com/connector_platform_oauth_redirect (exact, no wildcard).
- Open the GIS workspace and start the AI Copilot session; authenticate the ChatGPT connector using the same GIS identity.

The RFC 8414 discovery endpoint advertises S256 and public token exchange `none`, and deliberately does not advertise open dynamic client registration. It identifies the same Keycloak issuer as the MCP resource metadata and advertises the verified issuer response parameter. Keycloak's own OpenID discovery and other GIS OAuth clients are unchanged.

The MCP service accepts only the existing Claude and the new ChatGPT client, with the same strict access token issuer, audience, scope, signature, subject and expiry requirements. No shared developer API key or user password is sent to the model. The ChatGPT client grants no direct password, implicit or service-account flow; consent is required, code exchange uses PKCE S256, and the token lifetime is 900 seconds. GIS-side approval and access revalidation remain required.

## Evidence
Production HTTPS, 2026-11-23 (ICT): both disposable authorization-code flows succeeded using the corresponding exact callback intercepted locally (not the external ChatGPT/Claude service UI). Both actual signed tokens passed resource validation and enumerated all 36 production MCP tools. Missing PKCE receives an OAuth invalid_request error, and a different callback is denied with HTTP 400. Anonymous MCP is denied (401). The disposable Keycloak user was removed. An actual ChatGPT authorization attempt subsequently revealed the requested scopes `openid email offline_access gis:live`. These are now permitted for the dedicated client. A disposable full-scope flow confirmed real access and refresh tokens both enumerate all 36 tools through production HTTPS. Real ChatGPT client UI connection and real user GIS operation must still be checked by the user.

## Rollback
Production MCP systemd drop-in: /etc/systemd/system/openvietgis-mcp-public.service.d/60-chatgpt-client.conf. Before the modification, nginx snippet was backed up to /opt/openvietgis-mcp-runtime/chatgpt-oauth/nginx-before.conf. Disable the new dedicated Keycloak client, restore the previous MCP client allowlist and discovery snippet, then validate nginx, daemon-reload and restart the MCP sidecar. No changes to the GIS application or database.
