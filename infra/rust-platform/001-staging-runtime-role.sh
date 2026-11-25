#!/usr/bin/env bash
# Run through the Postgres entrypoint on a fresh Spec 064 staging volume.
(
  set -eu
  : "${STAGING_RUNTIME_PASSWORD:?set STAGING_RUNTIME_PASSWORD}"
  psql -X --quiet --set=ON_ERROR_STOP=1 --username "$POSTGRES_USER" --dbname "$POSTGRES_DB" <<'SQL'
\getenv runtime_password STAGING_RUNTIME_PASSWORD
CREATE ROLE openvietgis_runtime LOGIN PASSWORD :'runtime_password'
    NOSUPERUSER NOCREATEDB NOCREATEROLE NOINHERIT NOBYPASSRLS;
SQL
)
