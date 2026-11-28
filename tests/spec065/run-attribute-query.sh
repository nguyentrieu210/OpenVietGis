#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."
: "${OVG_SPEC065_PYTHON:?Set migration Python executable}"
: "${OVG_SPEC065_BINARY:?Set candidate Rust executable}"
name=ovg065-query-postgis
if docker ps -a --format '{{.Names}}' | grep -qx "$name" || ss -ltn | grep -q ':54332 '; then
  echo "Fixture name or port already occupied; refusing reuse" >&2; exit 1
fi
image="${OVG_SPEC065_IMAGE:-openvietgis-live-postgres:latest}"
case "$image" in
  openvietgis-live-postgres:latest|postgis/postgis@sha256:fe440a27c6567aefa932e988962e5827bb2c29e4b97dac2ca5268f5eafe10262) ;;
  *) echo "Unapproved fixture image" >&2; exit 2 ;;
esac
docker run --rm -d --name "$name" --memory 768m --cpus 1 -p 127.0.0.1:54332:5432 --tmpfs /var/lib/postgresql:rw,size=512m -e POSTGRES_USER=openvietgis -e POSTGRES_PASSWORD=openvietgis -e POSTGRES_DB=openvietgis_bench_spec065_query -e RUNTIME_DB_PASSWORD=openvietgis-runtime-dev --mount "type=bind,source=$PWD/infra/dev/postgres/001-runtime-role.sql,target=/docker-entrypoint-initdb.d/099-spec065-runtime.sql,readonly" "$image" >/dev/null
trap 'docker stop -t 3 "$name" >/dev/null 2>&1 || true' EXIT
for i in $(seq 1 40); do
  if docker exec "$name" pg_isready -h 127.0.0.1 -U openvietgis -d openvietgis_bench_spec065_query >/dev/null 2>&1; then break; fi
  sleep 1
done
extra=()
case "${1:-attribute}" in
  attribute) suite=attribute_query ;;
  integrated) suite=edit_processing ;;
  processing) suite=edit_processing; extra=(--suite processing); if [[ -n "${OVG_SPEC065_EVIDENCE:-}" ]]; then extra+=(--evidence "$OVG_SPEC065_EVIDENCE"); fi ;;
  collaboration) suite=edit_processing; extra=(--suite collaboration) ;;
  raster) suite=edit_processing; extra=(--suite raster) ;;
  topology) suite=edit_processing; extra=(--suite topology) ;;
  native-import) suite=edit_processing; extra=(--suite native-import) ;;
  *) echo "Unknown fixture suite" >&2; exit 2 ;;
esac
PYTHONPATH="$PWD/apps/api:$PWD" "$OVG_SPEC065_PYTHON" "tests/spec065/$suite.py" --candidate "$OVG_SPEC065_BINARY" "${extra[@]}"
