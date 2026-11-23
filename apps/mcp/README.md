# OpenVietGIS MCP (experimental)
Local stdio MCP adapter. No production endpoint, no browser pairing, no write tools.

Use a dedicated expiring project-read service credential issued by the GIS administrator.
Set OVG_ORG_ID, OVG_PROJECT_ID, OVG_SERVICE_TOKEN in a protected local environment,
optionally OVG_API_BASE (HTTPS /api/v1). Never commit or paste credentials into a prompt.
The server cannot choose another organization/project; the GIS API authorizes every call.

Run: `uv run --project apps/mcp python apps/mcp/server.py` from repo root.
Configure a compatible local MCP host to launch that command with protected variables.
Use a separate credential for each project and revoke it when no longer needed.

AI provider login is managed by the MCP host. This adapter does not supply a model
or convert model subscriptions into API keys. A separate remote OAuth candidate is available below; production deployment remains gated.

## Live workspace candidate (OpenCAD-style)
The separate live MCP server needs no GIS service credential. The user opens the approved
AI Copilot external plugin and presses Kết nối AI. It shows a 15-minute pairing capability,
bound only to that iframe session. Configure a local MCP host to launch:

`uv run --project apps/mcp python apps/mcp/live_server.py`

Inject the three variables displayed in the plugin as protected MCP host environment variables:
OVG_BRIDGE_BASE, OVG_BRIDGE_SESSION, OVG_BRIDGE_TOKEN. No browser GIS session or provider
token is transferred. The model host handles its own account/model. The AI can request
gis_live_view, gis_live_selection, and gis_live_navigate. Each invokes the existing GIS host's
trusted consent and fresh permission checks. Successful navigation is a camera command only;
the next viewport read verifies it without refresh. Selection returns bounded WGS84 geometry
without IDs or attributes. Disconnect explicitly revokes the pairing; closing the iframe
stops heartbeats, and the capability expires within 15 seconds. Pairs and results reside only
in memory; no log of payloads; single worker only.

## Isolated browser test
Run the bridge on loopback only with OVG_HOST_ORIGIN set to the test WebGIS origin and
OVG_PARTNER_SDK_PATH set to the approved partner SDK 1.0 from the source checkout.
`uv run --project apps/mcp uvicorn --app-dir apps/mcp bridge_http:app --host 127.0.0.1 --port 9186 --no-access-log`
Run the web-next Vite fixture on 4198 with base /rewrite/, then
`node apps/mcp/live-browser-smoke.mjs` (OVG_SOURCE_ROOT defaults to /opt/openvietgis).
The smoke routes the example HTTPS partner host to the loopback relay and mocks GIS data;
it runs a real local stdio MCP client and real MapLibre host consent/navigation.
OVG_BRIDGE_DEV=1 permits loopback HTTP only for this fixture.

## Remote OAuth candidate
The first-party panel removes manual pairing keys. It validates the logged-in GIS account,
linked Keycloak subject and unrestricted project.read via the existing Core API before
creating a tab session or acknowledging an operation. The remote Streamable HTTP server
validates Keycloak RS256 access tokens with a dedicated audience, approved client and scope.
The token subject can only address its own connected tab. No web session token is exposed to AI.
Each approved operation rechecks current Core authorization and command expiry. The previous
local/partner plugin mode remains independent.

27 backend/MCP tests and a first-party browser fixture passed. The HTTP tests use signed
fixture tokens; they are not evidence of a real Claude OAuth login. See deploy/README.md
for dedicated Keycloak client/scope, Nginx locations and first-party UI integration.
The canonical /mcp route is not enabled yet.

## Public release — NOT enabled
The sample manifest is illustrative: the catalog requires a separately approved external
HTTPS hostname, not openvietgis.cloud or its subdomains. Pin and review the SDK artifact,
manifest and hosted assets. A public release additionally needs authenticated session
issuance, gateway rate limits, per-user quotas, expiry/abuse monitoring, TLS, OAuth for
compatible remote MCP clients, and secure multi-process storage or an expressly provisioned
single process. A sandbox Origin:null header is NOT proof of GIS identity.
No production route or tenant plugin is registered by this candidate.
