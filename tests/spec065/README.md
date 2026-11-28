# Spec 065 authenticated acceptance subsets

Use a newly built candidate executable from this checkout and the repository's Python migration/test environment. The runner creates and removes a disposable PostGIS instance on loopback 54332, refusing an already occupied name or port. Never point it at a shared database. The synthetic users and credentials are fixture-only.

```sh
export OVG_SPEC065_PYTHON=/path/to/migration-venv/bin/python
export OVG_SPEC065_BINARY=/path/to/newly-built/openvietgis-api-rust
bash tests/spec065/run-attribute-query.sh attribute
bash tests/spec065/run-attribute-query.sh integrated
bash tests/spec065/run-attribute-query.sh collaboration
```

The integrated suite needs Linux, root orchestration to launch the fixture container and drop privileges, setpriv, system Python GDAL and NumPy. The copied API process runs as UID/GID 65534 inside its own scratch-directory scope (not a network sandbox); the runtime database role is checked for RLS. The test server's database pool has one connection, so concurrent HTTP calls exercise stale-base handling but do not prove parallel transaction lock races. The default container image is the locally available OpenVietGIS PostgreSQL image. No production data or live API is used. The suite does not replace QGIS Desktop comparison or full release acceptance.
