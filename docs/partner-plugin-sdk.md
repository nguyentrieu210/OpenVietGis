# OpenVietGIS Partner Plugin SDK 1.0

## Scope and architecture

This private organization catalog is distinct from trusted official modules. Third-party partners host their own UI over HTTPS; there is no Python import, dynamic JavaScript injection into GIS Core, or server-side fetching of a partner URL. The organization administrator approves and registers a versioned manifest, a project manager activates that exact manifest ID, then members explicitly open the sandboxed iframe in **Plugin → Kho plugin đối tác**.

The registry uses existing extensions APIs and audit/activation records; no database migration is required. Project users with restrictive GIS access do not open third-party plugins. The UI rechecks activation and catalog every 15 seconds while open and closes the frame on failure or disable. This is a private catalog MVP, not a public app store.

## Manifest

POST `/api/v1/organizations/{organizationId}/extensions/manifests` with an authorized human administrator session and CSRF token.

```json
{
  "extension_key": "partner_asset_tracker",
  "version": "1.0.0",
  "api_compatibility": "1.x",
  "display_name": "Asset tracker",
  "description": "External asset dashboard",
  "permissions": [],
  "commands": [],
  "capabilities": ["host_context", "host_notice"],
  "entity_schemas": {},
  "ui_contributions": [{
    "id": "partner_panel",
    "slot": "project_panel",
    "label": "Asset tracker",
    "route_suffix": "/partner",
    "runtime": "sandboxed_iframe",
    "url": "https://plugins.example.com/releases/1.0.0/index.html",
    "sdk_version": "1.0"
  }]
}
```

Only the reserved `partner_` namespace supports this exact external runtime shape. The URL must use an external HTTPS DNS hostname with no explicit port, credentials, query, fragment or platform domain. The registration never fetches it. A versioned URL and manifest are pinned metadata, not a cryptographic guarantee that the partner will not change served content; administrators must independently review the publisher and deployment process. Version switches require the existing verified transition API; implicit replacement is blocked.

Activate via POST `/api/v1/organizations/{organizationId}/projects/{projectId}/extensions/activations` with `{"manifest_id":"..."}` and project.manage. Disable via POST `.../extensions/{extensionKey}/disable`.

## Partner-side code

Copy `packages/plugin-sdk/openvietgis-partner.js` to the partner's own versioned HTTPS site. Use the example in `packages/plugin-sdk/examples/index.html`. The site must permit embedding by the intended GIS origin (configure its Content-Security-Policy frame-ancestors accordingly). The SDK requires exact platform origin (default `https://openvietgis.cloud`); development environments explicitly configure hostOrigin.

```html
<script src="./openvietgis-partner.js"></script>
<script>
OpenVietGISPartner.connect().then(async sdk => {
  if (sdk.capabilities.includes("host_context")) {
    console.log(await sdk.getContext()); // { locale: "vi-VN", sdkVersion: "1.0" }
  }
  if (sdk.capabilities.includes("host_notice")) {
    await sdk.notice("Plugin đã kết nối");
  }
});
</script>
```

Host RPC validates the iframe Window, opaque origin, protocol, session nonce, request ID and declared capabilities. Notice messages are plain text, bounded and rate limited; unrelated windows and unknown methods are ignored or denied. The iframe uses sandbox=allow-scripts only (not allow-same-origin), no referrer, and restricted device permissions. A partner iframe cannot directly read platform DOM, session tokens or datasets. Project identifiers, attributes, API keys and CSRF tokens are never sent to the frame. Viewport and selected geometry are available only through explicitly approved GIS calls documented below.

An iframe is not a network firewall: it can communicate with its own remote server, load remote resources, and render arbitrary UI. Do not trust it with secrets or private data. The administrator must vet third-party content, and the partner's site must work inside an opaque-origin sandbox. Browser and proxy framing policy must permit the deployment. Remote URLs may redirect or change; curated Marketplace releases now sign the manifest and catalog, not the changing bytes served at the remote URL. Private registered plugins are still not curated Marketplace releases.

## GIS capabilities (backward-compatible SDK 1.0 additions)

Declare each capability in the manifest (organization administrator approval). A project manager must activate that exact version. Each request prompts the logged-in user in the trusted GIS host, never inside the plugin. The host rechecks the current manifest, activation, project.read and unrestricted project access immediately before performing the operation. A restricted, busy, editing, unloaded or unauthorized workspace denies GIS operations.

| Capability | SDK call | Result |
|---|---|---|
| map_view | `await sdk.getMapView()` | `{crs:"EPSG:4326",center:[lng,lat],bbox:[west,south,east,north],zoom}`; bounds may be unwrapped at the antimeridian |
| map_selection | `await sdk.getSelection()` | `{crs:"EPSG:4326",geometry:{type,coordinates}}` for the currently selected, visible readable object; up to 2048 2D positions, no names, IDs or attributes |
| map_navigate | `await sdk.navigateMap({longitude:106.6,latitude:10.6,zoom:15})` | null; changes the view only, never data |
| map_overlay | `await sdk.setMapOverlay({title,geojson})` | null; one temporary WGS84 overlay, no stored layer |
| tool_register | `await sdk.registerTool({id,label})` | null; up to four host toolbar buttons, callbacks via `sdk.onToolClick(listener)` |

Each map request expires after 30 seconds; one outstanding request per frame and a two-second interval between accepted requests. Decline, timeout, frame reload/close or permission revocation rejects the promise. GIS responses use a transferred MessagePort tied to the requesting document, rather than broadcasting data back to an opaque iframe Window. Explicitly approved geometry and viewport coordinates may be retained by the remote partner; consent UI warns the user.

Navigation accepts finite WGS84 longitude ±180°, latitude ±85°, zoom 0–22. Selected geometry is limited to Point, MultiPoint, LineString, MultiLineString, Polygon and MultiPolygon; the host removes any extra coordinate dimensions and excludes filtered/hidden objects. No direct records CRUD, persistent dataset loading, token transfer, persistent grants or live continuous tracking. A plugin can request one temporary, host-colored overlay (100 features, 4096 total 2D positions; sanitized properties; no source URLs). No arbitrary styles or external resource loading into the map. The dock beside the map permits interaction with the map while the iframe is open. The user can remove the temporary overlay and tool buttons, and the host clears them on close, iframe reset, plugin disable or detected revocation. Tool labels are bounded plain text, prefixed “Đối tác ·”; clicks recheck permission and send only a tool ID to the plugin document.

```js
if (sdk.capabilities.includes("map_view")) {
  const view = await sdk.getMapView();
  console.log(view.center);
}
// Call from an explicit partner-side button; GIS still asks for confirmation.
```

## Third-party business system API

For actual GIS business integration, use the existing Developer Portal and narrowly granted, expiring machine credential from a separate partner backend. Never place a credential in iframe JS, the manifest, an URL, a browser build or postMessage. See `docs/developer-integrations.md`. Address lookup is project-scoped and approved-only. This plugin SDK deliberately does not grant direct GIS CRUD or unrestricted map access. The project can persist the bounded plugin overlay/tool definitions separately from core GIS data; the native event worker and signed Marketplace workflow are described below. For the legacy partner-operated webhook delivery, see the relay below.

## Partner-operated webhook relay

`packages/plugin-sdk/webhook_relay.py` is a standard-library Python process run by the partner backend. It polls the existing scoped collaboration activities endpoint with a service credential, checkpoints the latest seen activity, persists a queue in SQLite, and sends signed POST requests to the partner's explicitly configured HTTPS endpoint. This is a polling relay, not a platform-managed push subscription/event bus. It starts from the current head and sends newly observed activity entries only (no history backfill); the activity feed is a projection, not an immutable event stream, so changes to an older entry are not guaranteed to produce a new notification.

Configure these environment variables via a secure secret store:

- `OVG_API_BASE`: HTTPS API root including `/api/v1`, e.g. `https://openvietgis.cloud/api/v1`
- `OVG_ORG_ID`, `OVG_PROJECT_ID`: authorized UUIDs
- `OVG_SERVICE_TOKEN`: expiring project-read credential, never browser-side
- `WEBHOOK_URL`: fixed HTTPS partner receiver
- `WEBHOOK_SECRET`: shared HMAC secret, at least 32 bytes
- `RELAY_STATE_DB`: private SQLite path, one relay instance per subscription/project
- `POLL_SECONDS`: 10–3600 seconds, default 30

Run `python3 webhook_relay.py` on the partner backend. The receiver verifies `X-OVG-Timestamp` (allow a short freshness window), `X-OVG-Signature` (constant-time compare against HMAC-SHA256 of timestamp + "." + raw request body), and deduplicates by signed body `id` / `X-OVG-Event-Id`. Accept with 2xx only after durable processing. Payload contains event ID, kind, project and resource IDs, revision and creation time; excludes actor details, geometry, summary and arbitrary metadata. Supported kinds are newly observed changeset committed/reverted and review requested/commented/approved/changes requested. No redirects or environmental proxy; the partner host operator controls endpoint configuration.

Relay retries with bounded backoff, validates authorization on each poll before flushing queued notifications, and delivers at least once (duplicates possible after crash between POST and acknowledgment). It will not silently advance a checkpoint if unavailable or after a >50-page backlog; operator intervention is then required. Do not delete the state DB without understanding that a new relay starts from the latest head. Keep the DB private and plan retention/monitoring for production. The project-read credential has general project read access, not an event-only scope; explicit authorization is required before provisioning.

## Project-persistent contributions

The project manager opens an active plugin, approves the overlay/tool requests and clicks **Lưu / thay thế đóng góp cho dự án**. The host saves sanitized geometry and up to four plain-text tool definitions per exact manifest version (no scripts, URLs, feature attributes, or source references). Stored overlays render for unrestricted readers when the plugin is active, including after reloading or closing its iframe. Reopening a plugin allows the user to explicitly restore the saved tool buttons; callbacks require the live iframe. Disabling the plugin hides all of its contributions, without deleting the saved document. These are host overlay contributions, not editable core GIS datasets.

`GET /api/v1/organizations/{org}/projects/{project}/extensions/partner-contributions` lists active contributions. `PUT /api/v1/organizations/{org}/projects/{project}/extensions/partner-contributions/{manifest_id}` requires human project.manage, CSRF and `expected_revision` (0 for initial save) with `document: {overlay: null | {title,geojson}, tools: [{id,label}]}`. A concurrent revision returns a conflict. Save an empty document to clear. The actor must have unrestricted GIS access; app and database both enforce scope.

## Curated public Marketplace (operator release workflow)

The public route `/?marketplace=1` and the authenticated plugin hub consume `GET /api/v1/marketplace/catalog` (no login required for catalog metadata). Organization administrators may install a version explicitly listed in the trusted catalog using `POST /api/v1/organizations/{org}/marketplace/install/{publisher_id}/{extension_key}/{version}`. A project manager activates it separately. An ordinary organization cannot publish a private registration to the public catalog.

The developer generates an Ed25519 key and signs the exact validated manifest. The operator verifies identity, public key, namespace `partner_{publisher_id}_...`, capabilities, URL, sandbox behavior, and release evidence; then maintains the allowlisted catalog and signs it using a distinct operator key. From the repo root, with API virtualenv (cryptography installed):

```sh
apps/api/.venv/bin/python tools/marketplace_release.py keygen publisher-private.pem publisher-public.txt
apps/api/.venv/bin/python tools/marketplace_release.py sign-release acme manifest.json publisher-private.pem release.json
apps/api/.venv/bin/python tools/marketplace_release.py keygen operator-private.pem operator-public.txt
apps/api/.venv/bin/python tools/marketplace_release.py publish-reviewed reviewed.json operator-private.pem catalog.json --reviewed
```

`reviewed.json` is an object with `schema: "openvietgis.marketplace.v1"`, `publishers: [{id,name,public_key}]` (base64 raw public key from the text file), and `releases: [...]` (the signed objects from `release.json`). Store private keys outside repositories and application containers; use a private directory. The CLI checks publisher signatures and contract before operator signing. Set `OPENVIETGIS_MARKETPLACE_CATALOG_PATH` to the read-only signed catalog and `OPENVIETGIS_MARKETPLACE_OPERATOR_PUBLIC_KEY` to the trusted operator public value. Both signatures are checked on every catalog load and install; missing trust config yields an empty catalog, invalid config or signatures fail closed. Removing and re-signing a release stops new installations; installed versions remain until disabled by their administrator. The signature authenticates manifest metadata including its URL, not the changing contents fetched from that URL. This is an operator-reviewed workflow, not an automated developer account, self-service approval or publishing service.

## Self-service Developer Portal

Open `/?developer=1` from the public Marketplace, account menu, or organization API portal. Any authenticated human account can become a developer, independently of organization admin roles. A developer has one immutable publisher identity with server-generated `dev_<24 hex>` namespace. No organization/app credential can access this platform review workflow.

1. Download the portable `marketplace_sign.py` and JavaScript SDK directly from the portal. The signing tool requires only Python and cryptography; no Core checkout, API token, or network connection after installation.
2. Generate an Ed25519 key pair with `python marketplace_sign.py keygen publisher-private.pem publisher-public.txt`. Keep the private PEM offline (0600); never upload it.
3. Copy your account UUID from the portal and run `python marketplace_sign.py publisher-proof ACCOUNT_UUID "Publisher name" publisher-public.txt publisher-private.pem registration.json`. Upload only registration.json. The API verifies proof bound to the human account ID, name and public key before assigning the namespace.
4. Build and download a manifest using the portal. Host the UI on a versioned external HTTPS hostname. Use `partner_<assigned-id>_<slug>` and select only needed SDK capabilities. Run `python marketplace_sign.py sign-release PUBLISHER_ID manifest.json publisher-private.pem release.json`. Send only release.json.
5. Follow pending/approved/rejected/withdrawn status and reviewer feedback in the portal. A pending version may be withdrawn. Submitted namespaced key/version and signed manifest are immutable; a correction requires a new semver. The initial quota is 30 versions per publisher (operator assistance needed beyond it). Key rotation and publisher ownership transfer require an operator-managed migration; not self-service yet.

The API routes are `GET /api/v1/developer/marketplace/me`, `POST /publisher`, `POST /submissions`, `POST /submissions/{id}/withdraw` under the same prefix. All writes use authenticated human session + CSRF. Owner-only RLS limits rows; no cross-tenant disclosure. The portal never requests or uploads a private signing key.

### Platform reviewer and release

Set `OPENVIETGIS_MARKETPLACE_REVIEWER_USER_IDS` to a JSON array of explicit human UUIDs (default empty), not an organization role. The separate **Xét duyệt Marketplace** tab then appears. The platform reviewer can browse/paginate the queue, inspect exact signatures/manifests, approve/reject with a mandatory note, and export the approved record. Self-review, repeated decisions and changes to signed release fields are blocked. Review does not publish. **The operator private key remains outside the API and browser.**

Export one or several approved records. To preserve all previously published releases, use the offline operator CLI:

```sh
# Initial catalog (no --base):
apps/api/.venv/bin/python tools/marketplace_release.py merge-reviewed --output reviewed.json approved-entry.json
# Or merge into a verified existing operator-signed catalog:
apps/api/.venv/bin/python tools/marketplace_release.py merge-reviewed --base catalog.json --operator-public operator-public.txt --output reviewed.json approved-entry.json
apps/api/.venv/bin/python tools/marketplace_release.py publish-reviewed reviewed.json operator-private.pem catalog-next.json --reviewed
```

The merge verifies existing operator and incoming publisher signatures, preserves existing releases, and rejects mismatched public keys or replacement payload for the same key/version. The operator must independently confirm exports belong to real approved decisions; the export itself is not an operator signature. Deploy the new signed file atomically. Portal reports published only when the approved record exactly matches the verified active catalog. Removing a release from the signed catalog stops new installs and removes the public status. Existing installations require administrator disable if necessary. Reserve `dev_` publisher namespace for this workflow, not manual allocation.

Migration `0028_marketplace_developers` adds the profile and immutable submission queue. The browser/real DB acceptance is in `apps/api/tests/integration/m3/test_marketplace_developers.py` and `test_marketplace_developer_browser.py`. Production operator onboarding still requires configured reviewer IDs, review policy, legitimate publisher verification, and the offline signing key. Do not enable a public publisher by merely setting reviewer privileges.

## Native outbound webhook worker

`packages/plugin-sdk/native_webhook_worker.py` uses the append-only, project-scoped `GET /api/v1/organizations/{org}/projects/{project}/integrations/events?after=0&limit=200` journal (migration `0027_partner_platform`). Editing changesets and collaboration activities append metadata-only events transactionally; IDs and sequence cursors survive reload/retry. No backfill of pre-migration history. The operator provisions a separate worker and private state DB for each approved project/receiver pair. Configure `OVG_API_BASE`, `OVG_ORG_ID`, `OVG_PROJECT_ID`, `OVG_SERVICE_TOKEN`, `WEBHOOK_URL`, `WEBHOOK_SECRET`, `RELAY_STATE_DB` as above; `POLL_SECONDS` defaults to 10 (5–300), and optionally `REPLAY_JOURNAL=1` on first startup for historical journal events. Without replay, the initial cursor starts at the current journal head. Worker requests use the same scoped API authentication as other service clients; there is no event-only credential yet.

Run `python3 packages/plugin-sdk/native_webhook_worker.py` with an operator-managed secret store and supervisor (one process per state DB). Receiver URL must be fixed public DNS HTTPS on port 443, no query/fragment/credentials, no redirects or proxies. Every resolved DNS answer is checked for public range, then a public IPv4 is pinned with hostname TLS validation. The worker enqueues before committing its cursor, delivers in order, retries with capped backoff, rechecks access before processing queued delivery, and uses the same HMAC receiver contract as the previous relay. Body uses `openvietgis.integration.event.v1` and adds `sequence`; deduplicate by signed event ID. At-least-once semantics; supervise health and queue growth. Do not share the SQLite file among recipients. Operator provisioning and actual third-party delivery require a valid endpoint, scoped service token and shared secret; no self-service subscription form or external send is implied by installing the code. For VPS operation, `infra/prod/openvietgis-webhook@.service.example` and `infra/prod/webhook.env.example` provide a hardened single-instance systemd template and environment layout; provision a non-login service user first and keep each environment file root-owned with mode 0600.

For container deployments, mount the signed catalog as a read-only file into the API container, and pass the two `OPENVIETGIS_MARKETPLACE_*` settings to the API service. Only the public key goes into the application environment.

## Verification

- API integration: `apps/api/tests/integration/m3/test_partner_plugin.py` with isolated migrated PostGIS, plus official lifecycle regression.
- Browser fixture: `apps/web-next/external-plugin-smoke.mjs` (run against isolated Vite preview). Real sandboxed iframe, mocked external partner/network API fixtures, checks SDK handshake and denied capability.
- GIS browser fixture: `apps/web-next/partner-gis-smoke.mjs` checks trusted consent, view/navigation, bounded geometry-only sharing, invalid target and fresh permission revocation.
- Contribution browser fixture: `apps/web-next/partner-contributions-smoke.mjs` checks real canvas overlay rendering, dock, toolbar callback, malformed geometry, removal and disable cleanup.
- Persistent contributions and trusted catalog: `apps/api/tests/integration/m3/test_partner_platform.py` tests scoped storage, optimistic revisions, deactivate/reactivate, tampering, admin install, and the CLI key/sign/verify round trip. Editing changeset and service-account regressions cover immutable journal order and project-scoped access.
- Public catalog + persistent contribution browser fixture: `apps/web-next/partner-platform-smoke.mjs` verifies Marketplace browsing/search, installation, iframe activation, rendering on the actual canvas after close and reopening Studio, tool restoration, and disable cleanup.
- Relay and native worker: `python3 -m unittest -v test_webhook_relay test_native_webhook_worker` in `packages/plugin-sdk`; integration confirms service credential can read its authorized journal/activity feed but cannot read another project. Worker tests use synthetic deliveries and mocked DNS; no third-party endpoint is contacted.
