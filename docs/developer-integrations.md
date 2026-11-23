# OpenVietGIS developer integrations — partner API v1

## Intended use

Server-to-server integrations for trusted partners. The Core remains tenant/project scoped; vertical modules implement their own data projections. This is not an anonymous public data service or a third-party plugin execution platform.

## Register a partner application

1. As organization owner/admin, open the account menu → **Tích hợp API**.
2. Create a named service application for one partner.
3. Select a project and explicitly grant the viewer role. Only share projects the partner is allowed to read. Viewer access is **not** limited to House Numbering: it allows ordinary readable project data, including spatial layers.
4. Issue a credential with ceiling `project.read` and 30/90/365-day expiration. Copy the secret once into the partner server's secret manager. Never put it in a public website, client-side JavaScript, Git, query string, or logs.
5. Use the connection test; review credential prefix, expiry, last use and revoke when no longer needed.
6. Separate partners and sensitive project scopes into different applications; an application's active grants define the scope of its credentials.

The database retains a hash, not the original credential. The management API lists metadata only (no hash or secret); secrets are returned once on issuance. The service credential is an opaque Bearer token, not an OAuth2 access token.

## API request

```sh
curl --fail-with-body \
  -H "Authorization: Bearer ${OVG_API_KEY}" \
  "https://openvietgis.cloud/api/v1/organizations/ORG_ID/projects/PROJECT_ID/layers"
```

Use the existing GIS Core project APIs for supported metadata, layer and feature reads. The current API routes and schemas can be inspected via FastAPI's OpenAPI document; use the project's UUID and organization's UUID.

### House Numbering approved-address lookup

`GET /api/v1/organizations/{organization_id}/projects/{project_id}/modules/house-numbering/integrations/v1/addresses?query=125&limit=20`

The plugin must be activated for the project. Search text is 2–120 characters and the limit is 1–100 (default 50). Query matches approved address number, route and area text; it does not expose drafts or old evidence. Coordinates are WGS84, using point-on-surface for non-point geometry.

```json
{
  "items": [{
    "address_id": "00000000-0000-4000-8000-000000000001",
    "house_number": "125",
    "street": "Example street",
    "area": "Example area",
    "longitude": 106.65,
    "latitude": 10.68
  }],
  "crs": "EPSG:4326"
}
```

Illustrative synthetic data. The response never contains household personal records, parcel references or draft numbers. The credential still has its separately granted general project read capability; for an address-only partner, a dedicated publication permission/endpoint allowlist and data-authority approval must be implemented before sharing a project credential.

Status: 401 invalid/expired/revoked credential; 403 out-of-scope; 404 absent resource or inactive module; 422 invalid request. A valid query with no approved matches returns an empty list.

## Release gates

- Credential listing and approved address lookup tested with real isolated PostgreSQL/PostGIS, bearer authentication, cross-tenant rejection, restricted private household access, draft exclusion, revoke.
- Developer portal browser smoke covers creation, viewer grant, issuing expiring credential, connection test, concealment and revoke with synthetic API fixtures.
- Typecheck, lint and isolated bundle build passed.
- Deploy only after integrated Core release checks; canonical frontend/API are not changed by this candidate.
- Production partner onboarding additionally requires per-application rate limiting, usage monitoring, lifecycle policy, threat model and explicit data-sharing approval. A dedicated address-only grant is not yet available.

## External plugin SDK

The partner plugin SDK 1.0 private-catalog candidate is described in [Partner Plugin SDK](partner-plugin-sdk.md). It isolates external UI in a sandbox, requires tenant registration and project activation, and does not pass credentials to the plugin. The self-service Developer Portal, signed public Marketplace, native webhook journal/worker and downloadable signing/SDK tools are described in the same document. OAuth2 client registration, event-only credentials and billing remain separate follow-on work.

## Integration Hub catalog candidate — R026 (2026-11-23)

The authorized project GET endpoint
"/api/v1/organizations/{org}/projects/{project}/integration-hub/catalog"
reports adapter lifecycle and required permissions. Only discovery is executable
through this new endpoint. WMS/WFS/COG remains on the existing R010 read-only
API. QGIS, ArcGIS, AutoCAD, MicroStation V8i, MapInfo, Vietnamese cadastral
formats, PostGIS bridges and additional software are marked candidate/planned
until accepted by independent tests. Do not interpret the catalog as a connected
device, a valid license, or an authorization to synchronize.

**Extension contract:** introduce a reviewed adapter manifest, tenant-scoped
binding and capability negotiation; enforce project + layer/feature/field rights,
explicit CRS, origin allowlist, revision pins, idempotency and audit on every
future data execution endpoint. Separate managed device credentials from user
sessions; default deny writes and deletions. See
[spec](../specs/026-integration-hub-policy/spec.md) and
[quickstart](../specs/026-integration-hub-policy/quickstart.md).
