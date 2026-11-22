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
or convert model subscriptions into API keys. Remote user OAuth and live browser pairing
are later release gates.
