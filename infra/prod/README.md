# Proposed small-team self-host container fixture

**Status: proposed fixture, isolated build/start and HTTPS smoke verified.
Not an approved production release.** No images have been independently
certified, no live installation has been migrated by this remediation, and
dependency readiness is not a release capability gate. See the exact-candidate
[verification checkpoint](../../specs/018-full-product-audit/remediation/v1-preflight-20261123.md).

## Configuration contract

- Python 3.13, PostGIS 18 / 3.7, Node 26 and the locked pnpm version follow the
  product's declared development/validation baseline. The isolated image
  build and start drill succeeded on the pinned candidate recorded in the
  checkpoint. Independent digest selection, OS dependency/SBOM review,
  vulnerability review and production deployment approval remain outstanding.
- Create a fresh dedicated Compose project/volumes for verification. PostgreSQL
  18's volume is mounted at `/var/lib/postgresql`. The initialization script
  provisions a restricted `openvietgis_runtime` role only on a fresh volume.
  Do not apply the fixture to an existing database or reset a volume.
- Set all secrets from an external protected secret store. The proposed
  fixture uses Compose interpolation; do not print rendered configuration with
  real secrets, commit secret environment files, or copy them into backups.
  Use distinct unpredictable session, CSRF, bootstrap and database credentials.
- Required variables: `POSTGRES_PASSWORD`, `RUNTIME_DB_PASSWORD`,
  `RUNTIME_DATABASE_URL`, `MIGRATION_DATABASE_URL`, `SESSION_SECRET`,
  `CSRF_SECRET`, `BOOTSTRAP_SECRET`, `PUBLIC_HTTPS_ORIGIN`,
  `OIDC_PROVIDERS_JSON`. Optional: `POSTGRES_USER`, `POSTGRES_DB`,
  `HTTP_PORT`. Database URLs are explicit SQLAlchemy
  `postgresql+psycopg` URIs to the same database at host `postgres`,
  with correctly URL-encoded passwords matching the corresponding role.
  Do not include query overrides. The API receives the restricted runtime URL
  only; the separate migration service receives the owner URL, applies Core
  and official module migrations, and must finish successfully before the API
  starts. Runtime role verification remains active. The API image runs as UID
  10001; writable artifact storage is separate from source.
- The frontend is bound only to loopback; a separately configured TLS reverse
  proxy must serve the public HTTPS origin and forward requests to that
  listener. Production Settings require HTTPS origins, secure cookies, and
  distinct application secrets of at least 32 characters. The production API
  CORS policy admits only the configured web origin. Production Settings also
  reject missing or non-HTTPS OIDC issuers. The web image serves current
  Studio at / and /app with SPA fallback for legacy invitation URLs. Hashed
  asset paths return 404 when absent; Vite .mjs workers have a JavaScript MIME
  type; SSE buffering is disabled and WebSocket upgrade is forwarded. The
  separate CAD relay and remote MCP service are not provisioned or enabled
  by this four-service fixture.
- Configure an independent HTTPS OIDC provider and approved issuer/client
  settings, including the exact application redirect and trusted TLS chain.
  This fixture deliberately does not deploy or claim to configure Keycloak.
  Authentication, bootstrap and tenant workflows with the selected provider,
  TLS termination, network restrictions, monitoring, rotation, backup
  retention, and operator approval remain acceptance gates.

## Verification boundaries

Validate configuration with dummy inputs only, e.g. using
`docker compose --env-file /dev/null -f infra/prod/docker-compose.prod.yml config --quiet`
after supplying a complete **synthetic** environment. Do not run this against
production secrets or a deployed Compose project. Configuration parse and Settings validation alone do not demonstrate that
images build, services start, migrations finish, external identity works, or
a production RPO/RTO is met.

For an already-running **synthetic** loopback HTTPS fixture with a locally
trusted test certificate, run the read-only packaging smoke. It deliberately
rejects public origins and refuses unverified fixture readiness masquerading
as release certification:

```sh
uv run --project apps/api --locked python tools/check_self_host_fixture.py \
  --origin https://localhost:43291 --ca-cert /path/to/synthetic-localhost.crt
```

Use a fresh Compose project and volumes, unique port and synthetic data only.
The successful drill used a separate temporary TLS terminator and synthetic
session fixtures; it did not contact an external identity provider. It verified
real HTTP and WebSocket/SSE traffic, native GIS changes, CAD bytes, and Studio
browser modes. Stop the isolated project after verification and retain evidence
without retaining active sessions or publishing test credentials.

The recovery procedure uses signed v4 manifests with externally held keys; see
[recovery authentication](../../specs/018-full-product-audit/remediation/G010-recoverable-backup/authenticated-recovery.md).
No release certification is granted by this profile.
