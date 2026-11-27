# Spec 064 PostGIS parity

Run from the repository with the built Rust binary and the API virtualenv.

~~~sh
apps/api/.venv/bin/python tests/rust-parity/run.py \
  --candidate target/debug/openvietgis-api-rust
~~~

On VPS1 the installed API virtualenv is /opt/openvietgis/apps/api/.venv. The
runner uses the loopback development PostGIS test cluster on port 54330 by
default. Set OVG_PARITY_PG_ADMIN_URL only to point to another loopback development
cluster with the same template database and restricted runtime role.

The runner creates a uniquely named disposable database from template_postgis,
applies the current Alembic head, seeds synthetic data, and compares the
FastAPI reference with a loopback Rust process. Both APIs use the restricted
database role against the same state. It stops the candidate and drops the
test database on exit. The Rust pool uses one connection to exercise transaction context
cleanup across alternating requests.

Status codes, JSON bodies and Cache-Control headers are compared. Independent
assertions check that the runtime role has no visibility without tenant scope,
a restricted user sees exactly one permitted feature in the layer count, and
layer bounds and source provenance stay hidden. The suite exercises expiry,
revocation, tenant and project boundaries, GIS rule revocation, validation and
layer pagination. The output includes the tested Rust binary SHA-256.

The suite covers the layer and source catalogs, project CRS profile inventory,
and Ops liveness and readiness. It checks that a missing artifact directory
and privileged runtime database credentials both fail readiness. Only the
value of the UTC readiness timestamp may differ between implementations.

This runner is a correctness check. Run the benchmark harness separately for
performance.

CI always enables `--include-features`; missing E02 routes must fail rather than
silently skip parity. The DB-free harness regression tests can be run separately:

~~~sh
apps/api/.venv/bin/python -m unittest discover -s tests/rust-parity -p 'test_*.py'
~~~

GitHub Actions uploads parity logs and Cargo Audit JSON even on failure. See
`specs/064-rust-platform/evidence/ci-parity-agent18.md` for the observed baseline
and remaining cross-engine gates.
