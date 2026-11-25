# Spec 064 hybrid qualification

Use the `064-convergence` branch and record the exact candidate SHA. GitHub Actions builds the Rust and FastAPI backends and starts an isolated PostGIS stack in **Hybrid Staging Runtime**. **Rust Platform CI** compares the native endpoints directly with FastAPI against PostGIS. **Hybrid Staging Gateway** exercises method dispatch, query strings, authentication headers and backend failure handling.

## Isolated staging

Build the three images in CI or a dedicated build host, using the same source SHA.

```bash
docker build -f infra/rust-platform/Dockerfile.api-rust -t ovg-064-api-rust:SHA .
docker build -f infra/prod/Dockerfile.api -t ovg-064-api-python:SHA .
docker build -f infra/rust-platform/Dockerfile.postgis-staging -t ovg-064-postgis:SHA .
```

Set the corresponding `OVG_API_RUST_IMAGE`, `OVG_API_PYTHON_IMAGE` and `OVG_STAGING_POSTGIS_IMAGE` image refs. Supply separate migration and restricted runtime credentials for the Compose service `postgres-staging`, database `ovg_staging_064`. Use `postgresql+psycopg://` for `STAGING_MIGRATION_DATABASE_URL` and `STAGING_RUNTIME_DATABASE_URL`, and `postgresql://` for `STAGING_RUST_DATABASE_URL`. Both runtime URLs must identify `openvietgis_runtime`. Set `STAGING_POSTGRES_PASSWORD` and `STAGING_RUNTIME_PASSWORD` to the matching raw credentials.

Set `STAGING_WEB_ORIGIN` to an HTTPS origin and configure `STAGING_OIDC_PROVIDERS_JSON` for its provider. Set distinct `STAGING_SESSION_SECRET`, `STAGING_CSRF_SECRET` and `STAGING_BOOTSTRAP_SECRET` values of at least 32 characters. Keep the variables in an environment file with restricted permissions.

```bash
docker compose --env-file /path/to/staging.env -f infra/rust-platform/docker-compose.hybrid.yml config --quiet
docker compose --env-file /path/to/staging.env -f infra/rust-platform/docker-compose.hybrid.yml up -d --wait
curl -fsS http://127.0.0.1:18064/healthz
curl -fsS http://127.0.0.1:18064/api/v1/health/ready
```

The migration service applies the shared schema before the backends start. Only the gateway listens on the host, at a loopback port. Its allowlist moves selected session-authenticated hot paths and the native MVT route to Rust. FastAPI is the default for other paths, including bearer-authenticated legacy calls, OIDC, transfer workflows requiring format negotiation, and changeset writes until native writes have realtime parity.

Before promoting routes, check parity, real browser and QGIS workflows, dependency audit, comparative benchmarks, failure recovery, and rollback against the exact release SHA. Production cutover follows Spec 064's approval gate.
