# Spec 021 — AI Copilot / MCP
Parent: ROADMAP R014; depends on R002/R003/R004. Independent of vertical modules.

A GIS operator connects an MCP-capable AI client to a project and asks about the saved map,
layers and features. V1a uses a locally configured, expiring, project-read credential and
stdio. It does not offer arbitrary HTTP, SQL, mutation, provider credentials, or public access.

## Requirements
- FR001: expose typed, stable read tools over MCP and reuse existing project API.
- FR002: pin organization/project at startup; validate IDs, revision pins and limits.
- FR003: retain Core authorization and revocation on every tool call; no token in tool output.
- FR004: bounded API requests and results, no redirects, HTTPS production transport.
- FR005: live-browser pairing is a separate slice: authorize a specific user/project/tab,
  short-lived grant, allowlisted view-only commands, acknowledgements, logout/revoke cleanup,
  no persistent write or browser token transfer; deny cross-project/session commands.
- FR006: remote MCP OAuth requires a standards-compliant resource server and user delegation.
  Never substitute the GIS service key for a user's OAuth identity or infer model subscriptions.

## Acceptance
V1a: official SDK tool discovery and calls, mock transport verifies auth, scope, pins, bounds,
fail-closed API errors and no secrets. Real GIS integration validation required before release.
Live controls: real browser test demonstrates acknowledged command without reload, isolated
tabs and cancellation; not accepted in V1a.
