# AI Copilot — GIS workspace integration acceptance (2026-11-23)

## Frontend
- First-party Copilot panel integrated into `apps/web-next/src/LiveApp.tsx`, map-top next to search (`AiCopilotPanel.tsx`, `src/ai-copilot/`). It is permission-gated on `project.read`, non-restricted project and canonical protected-resource metadata, and tears down on lifecycle changes.
- Production TypeScript/Vite build: PASS.
- `apps/web-next/ai-copilot-integration-smoke.mjs`: PASS against candidate bundle with mocked API and a real MapLibre map. Covered unavailable service, restricted project, tokenless UI, denial, per-action prepare/approval, actual map navigation and viewport readback, disconnect.
- Main worktree contains unrelated active frontend/backend changes. Candidate build is not promoted to public GIS; use coordinated merged backend/frontend release gates rather than deploying this dirty tree independently.

## Remote service
- Pinned branch checkpoint `65b1198` staged at `/opt/openvietgis-mcp-runtime/65b1198`, dependency lock installed in copied venv, single non-root dynamic-user systemd process `openvietgis-mcp.service` (not boot-enabled, loopback only `127.0.0.1:9187`).
- Local protected-resource metadata returns 200 (canonical GIS issuer, resource and `gis:live` scope). Anonymous MCP request returns 401 with `WWW-Authenticate` resource metadata. No public proxy change.
- Public Keycloak Authorization Code + S256 request for `openvietgis-mcp-claude` with exact Claude callback renders login successfully; nonexistent control client returns 400. This verifies client recognition, not a signed access token or actual Claude login.
- Existing public core live-contract preflight: PASS, public `/mcp` remains 404. Remote branch currently has active edits for real Keycloak smoke; do not promote stale checkpoint over that work.

## Remaining acceptance
Coordinate updated remote runtime with real signed token checks (issuer/audience/client/scope/expiry/isolation), reviewed ingress, actual Claude consent and connection, and merged release with frontend. No manual tokens or secrets in UI. Existing live GIS unchanged by staged service.
