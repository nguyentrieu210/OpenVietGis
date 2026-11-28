#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."
: "${OVG_SPEC065_PYTHON:?Set migration Python executable}"
: "${OVG_SPEC065_BINARY:?Set candidate Rust executable}"
name=ovg065-query-postgis
if docker ps -a --format '{{.Names}}' | grep -qx "$name" || ss -ltn | grep -q ':54332 '; then
  echo "Fixture name or port already occupied; refusing reuse" >&2; exit 1
fi
docker run --rm -d --name "$name" --memory 768m --cpus 1 -p 127.0.0.1:54332:5432 --tmpfs /var/lib/postgresql:rw,size=512m -e POSTGRES_USER=openvietgis -e POSTGRES_PASSWORD=openvietgis -e POSTGRES_DB=openvietgis_bench_spec065_query -e RUNTIME_DB_PASSWORD=openvietgis-runtime-dev openvietgis-live-postgres:latest >/dev/null
trap 'docker stop -t 3 "$name" >/dev/null 2>&1 || true' EXIT
for i in $(seq 1 40); do
  if docker exec "$name" pg_isready -h 127.0.0.1 -U openvietgis -d openvietgis_bench_spec065_query >/dev/null 2>&1; then break; fi
  sleep 1
done
PYTHONPATH="$PWD/apps/api:$PWD" "$OVG_SPEC065_PYTHON" tests/spec065/attribute_query.py --candidate "$OVG_SPEC065_BINARY"
