#!/usr/bin/env bash
# Spec 064 Staging Backup, Restore, Deterministic Hash Verification & Additive Schema Check
# Requirements: FR-011, SC-006 (T004 / T021)
#
# SAFETY INVARIANTS:
# - Source database is accessed strictly READ-ONLY (pg_dump + SELECT fingerprinting).
# - Target database MUST be prefixed with `ovg_staging_` or `ovg_rehearsal_` so live
#   databases (`openvietgis_live`, `openvietgis_test`, `postgres`) can NEVER be dropped
#   or overwritten.
# - Never stops, restarts, or modifies live services.

set -euo pipefail
umask 077

SOURCE_CONTAINER="dev-postgres-test-1"
SOURCE_DB="openvietgis_test"
SOURCE_USER="openvietgis"
TARGET_CONTAINER="dev-postgres-test-1"
TARGET_DB=""
TARGET_USER="openvietgis"
BACKUP_DIR="/tmp/ovg-064-staging-backups"
DRY_RUN=0
KEEP_TARGET_DB=0
KEEP_DUMP=0
CANDIDATE_MIGRATION_DIR=""
EVIDENCE_OUT=""

usage() {
  cat <<'EOF'
Usage: staging-backup-restore.sh [options]

Options:
  --source-container NAME     Docker PostGIS container to dump from (default: dev-postgres-test-1)
  --source-db NAME            Source database name (default: openvietgis_test)
  --source-user NAME          Source PostgreSQL superuser/dump role (default: openvietgis)
  --target-container NAME     Docker PostGIS container for isolated restore (default: dev-postgres-test-1)
  --target-db NAME            Isolated target DB name (MUST start with ovg_staging_ or ovg_rehearsal_)
  --target-user NAME          Target PostgreSQL role (default: openvietgis)
  --backup-dir PATH           Directory for temporary .dump artifact (default: /tmp/ovg-064-staging-backups)
  --candidate-migrations DIR  Optional directory of SQL/Alembic files to scan for destructive DDL
  --evidence-out FILE         Optional file path to write the verification summary
  --keep-target-db            Keep the restored isolated target DB after verification
  --keep-dump                 Keep the .dump file after verification
  --dry-run                   Validate arguments, connectivity, safety guards, and print plan without mutating
  -h, --help                  Show this help message
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --source-container) SOURCE_CONTAINER="$2"; shift 2 ;;
    --source-db) SOURCE_DB="$2"; shift 2 ;;
    --source-user) SOURCE_USER="$2"; shift 2 ;;
    --target-container) TARGET_CONTAINER="$2"; shift 2 ;;
    --target-db) TARGET_DB="$2"; shift 2 ;;
    --target-user) TARGET_USER="$2"; shift 2 ;;
    --backup-dir) BACKUP_DIR="$2"; shift 2 ;;
    --candidate-migrations) CANDIDATE_MIGRATION_DIR="$2"; shift 2 ;;
    --evidence-out) EVIDENCE_OUT="$2"; shift 2 ;;
    --keep-target-db) KEEP_TARGET_DB=1; shift ;;
    --keep-dump) KEEP_DUMP=1; shift ;;
    --dry-run) DRY_RUN=1; shift ;;
    -h|--help) usage; exit 0 ;;
    *) echo "ERROR: unknown option: $1" >&2; usage >&2; exit 2 ;;
  esac
done

TS_UTC="$(date -u +%Y%m%dT%H%M%SZ)"
if [[ -z "$TARGET_DB" ]]; then
  TARGET_DB="ovg_rehearsal_${TS_UTC,,}"
fi

# --- HARD SAFETY GUARD ---
case "$TARGET_DB" in
  openvietgis_live|openvietgis_test|openvietgis|postgres|template0|template1|template_postgis)
    echo "FATAL: refusing to touch protected database '$TARGET_DB'" >&2
    exit 10
    ;;
esac
if [[ ! "$TARGET_DB" =~ ^ovg_(staging|rehearsal)_[a-z0-9_]+$ || ${#TARGET_DB} -gt 63 ]]; then
  echo "FATAL: --target-db '$TARGET_DB' must match ^ovg_(staging|rehearsal)_[a-z0-9_]+$" >&2
  exit 10
fi

# Only designated disposable PostGIS containers may receive writes.
case "$TARGET_CONTAINER" in
  dev-postgres-test-1|ovg-064-staging-postgres-staging-1) ;;
  *) echo "FATAL: target container is not an approved isolated test container" >&2; exit 10 ;;
esac
if [[ "$SOURCE_CONTAINER" != dev-postgres-test-1 && "$SOURCE_CONTAINER" != ovg-064-staging-postgres-staging-1 ]]; then
  echo "FATAL: source must be an isolated test container" >&2; exit 10
fi
if [[ ! "$SOURCE_DB" =~ ^[a-zA-Z0-9_]{1,63}$ || "$SOURCE_DB" == "$TARGET_DB" ]]; then
  echo "FATAL: invalid source name or source/target collision" >&2; exit 10
fi

log() {
  printf '[%s] %s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)" "$*"
}

require_container() {
  local ctr="$1"
  local state
  state="$(docker inspect -f '{{.State.Running}}' "$ctr" 2>/dev/null || true)"
  if [[ "$state" != "true" ]]; then
    echo "FATAL: Docker container '$ctr' is not running" >&2
    exit 11
  fi
}

# Deterministic schema + RLS + row-count fingerprint (pure read-only SELECTs)
fingerprint_db() {
  local ctr="$1"
  local user="$2"
  local db="$3"
  local out_prefix="$4"

  # 1. Core metadata: extensions, alembic versions, base tables, RLS counts
  docker exec -i "$ctr" psql -U "$user" -d "$db" -X -At -v ON_ERROR_STOP=1 <<'SQL' > "${out_prefix}.meta"
SELECT 'pg_version=' || current_setting('server_version');
SELECT 'postgis_version=' || COALESCE((SELECT extversion FROM pg_extension WHERE extname='postgis'), 'none');
SELECT 'core_alembic=' || COALESCE((SELECT string_agg(version_num, ',' ORDER BY version_num) FROM public.alembic_version), 'none');
SELECT to_regclass('public.alembic_version_house_numbering') IS NOT NULL AS has_hn \gset
\if :has_hn
SELECT 'hn_alembic=' || COALESCE((SELECT string_agg(version_num, ',' ORDER BY version_num) FROM public.alembic_version_house_numbering), 'none');
\else
SELECT 'hn_alembic=none';
\endif
SELECT 'base_table_count=' || count(*)
  FROM information_schema.tables
  WHERE table_schema NOT IN ('pg_catalog', 'information_schema') AND table_schema NOT LIKE 'pg_temp_%' AND table_type = 'BASE TABLE';
SELECT 'view_count=' || count(*)
  FROM information_schema.views
  WHERE table_schema NOT IN ('pg_catalog', 'information_schema') AND table_schema NOT LIKE 'pg_temp_%';
SELECT 'rls_table_count=' || count(*)
  FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace
  WHERE c.relkind = 'r' AND n.nspname NOT IN ('pg_catalog', 'information_schema') AND c.relrowsecurity = true;
SELECT 'rls_forced_count=' || count(*)
  FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace
  WHERE c.relkind = 'r' AND n.nspname NOT IN ('pg_catalog', 'information_schema') AND c.relforcerowsecurity = true;
SELECT 'rls_policy_count=' || count(*)
  FROM pg_policy p JOIN pg_class c ON c.oid = p.polrelid JOIN pg_namespace n ON n.oid = c.relnamespace
  WHERE n.nspname NOT IN ('pg_catalog', 'information_schema');
SQL

  # 2. Deterministic schema structure manifest (tables, columns, types, nullability, RLS policies)
  docker exec -i "$ctr" psql -U "$user" -d "$db" -X -At -v ON_ERROR_STOP=1 <<'SQL' | LC_ALL=C sort > "${out_prefix}.schema"
SELECT 'COL|' || table_schema || '.' || table_name || '.' || column_name || '|' || data_type || '|' || is_nullable
FROM information_schema.columns
WHERE table_schema NOT IN ('pg_catalog', 'information_schema') AND table_schema NOT LIKE 'pg_temp_%'
UNION ALL
SELECT 'RLS_FLAGS|' || n.nspname || '.' || c.relname || '|' || c.relrowsecurity || '|' || c.relforcerowsecurity
FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace
WHERE c.relkind IN ('r', 'p') AND n.nspname NOT IN ('pg_catalog', 'information_schema')
UNION ALL
SELECT 'RLS|' || jsonb_build_array(
  n.nspname, c.relname, p.polname, p.polcmd, p.polpermissive,
  (SELECT jsonb_agg(roles.name ORDER BY roles.name COLLATE "C")
   FROM (SELECT CASE WHEN role_id = 0 THEN 'PUBLIC' ELSE pg_get_userbyid(role_id) END AS name
         FROM unnest(p.polroles) AS policy_role(role_id)) AS roles),
  pg_get_expr(p.polqual, p.polrelid), pg_get_expr(p.polwithcheck, p.polrelid))::text
FROM pg_policy p
JOIN pg_class c ON c.oid = p.polrelid
JOIN pg_namespace n ON n.oid = c.relnamespace
WHERE n.nspname NOT IN ('pg_catalog', 'information_schema')
UNION ALL
SELECT 'ACL|' || n.nspname || '.' || c.relname || '|' || pg_get_userbyid(c.relowner)
       || '|' || COALESCE(c.relacl::text, '')
FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace
WHERE c.relkind IN ('r','p','v','m','S') AND n.nspname NOT IN ('pg_catalog','information_schema');
SQL

  # 3. Deterministic exact row counts across every user base table
  docker exec -i "$ctr" psql -U "$user" -d "$db" -X -At -v ON_ERROR_STOP=1 <<'SQL' > "${out_prefix}.rows"
DO $$
DECLARE
  r RECORD;
  cnt BIGINT;
BEGIN
  CREATE TEMP TABLE IF NOT EXISTS _ovg_row_counts(fqn TEXT PRIMARY KEY, row_count BIGINT);
  TRUNCATE _ovg_row_counts;
  FOR r IN
    SELECT table_schema, table_name
    FROM information_schema.tables
    WHERE table_schema NOT IN ('pg_catalog', 'information_schema') AND table_schema NOT LIKE 'pg_temp_%'
      AND table_type = 'BASE TABLE'
    ORDER BY table_schema, table_name
  LOOP
    EXECUTE format('SELECT count(*) FROM %I.%I', r.table_schema, r.table_name) INTO cnt;
    INSERT INTO _ovg_row_counts VALUES (r.table_schema || '.' || r.table_name, cnt);
  END LOOP;
END$$;
SELECT fqn || '=' || row_count FROM _ovg_row_counts ORDER BY fqn;
SQL

  # Content hashes, including duplicates; only digests leave the database.
  docker exec -i "$ctr" psql -U "$user" -d "$db" -X -qAt -v ON_ERROR_STOP=1 <<'SQL' > "${out_prefix}.content"
SET TIME ZONE 'UTC';
SET DateStyle = 'ISO, YMD';
SELECT format(
  'SELECT %L || ''|'' || encode(sha256(convert_to(to_jsonb(t)::text, ''UTF8'')), ''hex'') AS digest FROM %I.%I AS t ORDER BY 1',
  table_schema || '.' || table_name, table_schema, table_name)
FROM information_schema.tables
WHERE table_schema NOT IN ('pg_catalog', 'information_schema')
  AND table_schema NOT LIKE 'pg_temp_%' AND table_type = 'BASE TABLE'
ORDER BY table_schema, table_name
\gexec
SQL
  sha256sum "${out_prefix}.content" | awk '{print $1}' > "${out_prefix}.content.sha256"

  sha256sum "${out_prefix}.schema" | awk '{print $1}' > "${out_prefix}.schema.sha256"
  sha256sum "${out_prefix}.rows"   | awk '{print $1}' > "${out_prefix}.rows.sha256"
}

check_additive_migrations() {
  local dir="$1"
  local violations=0
  if [[ -z "$dir" ]]; then
    echo "additive_migration_scan=skipped(no --candidate-migrations dir)"
    return 0
  fi
  if [[ ! -d "$dir" ]]; then
    echo "FATAL: --candidate-migrations directory does not exist: $dir" >&2
    exit 12
  fi
  # Scan candidate SQL/migration scripts for destructive DDL prohibited before rollback gate (FR-011)
  local matches
  matches="$(grep -rnE -i '\b(DROP[[:space:]]+(TABLE|COLUMN|SCHEMA|POLICY)|DISABLE[[:space:]]+ROW[[:space:]]+LEVEL[[:space:]]+SECURITY|TRUNCATE[[:space:]]+TABLE)\b' "$dir" || true)"
  if [[ -n "$matches" ]]; then
    echo "ADDITIVE_CHECK_FAILED: destructive DDL detected in $dir:" >&2
    echo "$matches" >&2
    violations=1
  fi
  if [[ "$violations" -ne 0 ]]; then
    return 1
  fi
  echo "additive_migration_scan=PASS(dir=$dir, destructive_ddl_matches=0)"
}

require_container "$SOURCE_CONTAINER"
require_container "$TARGET_CONTAINER"

# Verify source DB exists and read its size
SRC_SIZE_BYTES="$(docker exec "$SOURCE_CONTAINER" psql -U "$SOURCE_USER" -d "$SOURCE_DB" -X -Atc "SELECT pg_database_size(current_database());")"
SRC_SIZE_PRETTY="$(docker exec "$SOURCE_CONTAINER" psql -U "$SOURCE_USER" -d "$SOURCE_DB" -X -Atc "SELECT pg_size_pretty(pg_database_size(current_database()));")"

log "Source: ${SOURCE_CONTAINER}/${SOURCE_DB} (size=${SRC_SIZE_PRETTY}, ${SRC_SIZE_BYTES} bytes)"
log "Target: ${TARGET_CONTAINER}/${TARGET_DB} (isolated rehearsal/staging DB)"

if [[ "$DRY_RUN" -eq 1 ]]; then
  log "DRY-RUN: safety guards passed; source and target containers reachable."
  TMP_DRY="$(mktemp -d /tmp/ovg-dryrun-XXXXXX)"
  trap 'rm -rf "$TMP_DRY"' EXIT
  fingerprint_db "$SOURCE_CONTAINER" "$SOURCE_USER" "$SOURCE_DB" "${TMP_DRY}/src"
  log "DRY-RUN source metadata:"
  sed 's/^/  /' "${TMP_DRY}/src.meta"
  log "DRY-RUN source schema_sha256=$(cat "${TMP_DRY}/src.schema.sha256")"
  log "DRY-RUN source rows_sha256=$(cat "${TMP_DRY}/src.rows.sha256")"
  if [[ -n "$CANDIDATE_MIGRATION_DIR" ]]; then
    check_additive_migrations "$CANDIDATE_MIGRATION_DIR"
  fi
  log "DRY-RUN complete. No backup written and no target database created."
  exit 0
fi

mkdir -p "$BACKUP_DIR"
FREE_BYTES="$(df -PB1 "$BACKUP_DIR" | awk 'NR==2 {print $4}')"
# Allow room for a dump and restore; this is a lower bound, not a capacity guarantee.
if [[ ! "$FREE_BYTES" =~ ^[0-9]+$ || "$FREE_BYTES" -lt $(( SRC_SIZE_BYTES * 2 + 1073741824 )) ]]; then
  echo "FATAL: insufficient free backup filesystem capacity" >&2; exit 12
fi
WORK_DIR="$(mktemp -d "${BACKUP_DIR}/run-${TS_UTC}-XXXXXX")"
TARGET_CREATED=0
VERIFIED=0
DUMP_FILE="${WORK_DIR}/${SOURCE_DB}_${TS_UTC}.dump"

cleanup() {
  if [[ "$TARGET_CREATED" -eq 1 && ( "$KEEP_TARGET_DB" -eq 0 || "$VERIFIED" -eq 0 ) ]]; then
    docker exec "$TARGET_CONTAINER" psql -U "$TARGET_USER" -d postgres -X -v ON_ERROR_STOP=1 \
      -c "SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE datname = '${TARGET_DB}' AND pid <> pg_backend_pid();" >/dev/null 2>&1 || true
    docker exec "$TARGET_CONTAINER" psql -U "$TARGET_USER" -d postgres -X -v ON_ERROR_STOP=1 \
      -c "DROP DATABASE IF EXISTS \"${TARGET_DB}\";" >/dev/null 2>&1 || true
  fi
  if [[ "$KEEP_DUMP" -eq 0 ]]; then
    rm -rf "$WORK_DIR"
  else
    rm -f "${WORK_DIR}"/*.meta "${WORK_DIR}"/*.schema "${WORK_DIR}"/*.rows "${WORK_DIR}"/*.content "${WORK_DIR}"/*.sha256
  fi
}
trap cleanup EXIT

T_START="$(date +%s)"

# Step 1: Fingerprint source DB before dump
check_additive_migrations "$CANDIDATE_MIGRATION_DIR" >/dev/null
log "Step 1/5: Computing deterministic source DB fingerprint (read-only)..."
fingerprint_db "$SOURCE_CONTAINER" "$SOURCE_USER" "$SOURCE_DB" "${WORK_DIR}/src"
SRC_SCHEMA_SHA="$(cat "${WORK_DIR}/src.schema.sha256")"
SRC_ROWS_SHA="$(cat "${WORK_DIR}/src.rows.sha256")"
SRC_CONTENT_SHA="$(cat "${WORK_DIR}/src.content.sha256")"

# Step 2: Custom-format pg_dump (-Fc -Z 6)
log "Step 2/5: Creating custom-format pg_dump backup -> ${DUMP_FILE}..."
T_DUMP_START="$(date +%s)"
docker exec "$SOURCE_CONTAINER" pg_dump -U "$SOURCE_USER" -d "$SOURCE_DB" \
  -Fc -Z 6 > "$DUMP_FILE"
T_DUMP_END="$(date +%s)"
DUMP_SECONDS=$((T_DUMP_END - T_DUMP_START))
DUMP_BYTES="$(stat -c %s "$DUMP_FILE")"
DUMP_SHA256="$(sha256sum "$DUMP_FILE" | awk '{print $1}')"
log "Backup complete: bytes=${DUMP_BYTES}, sha256=${DUMP_SHA256}, duration=${DUMP_SECONDS}s"

# Step 3: Provision fresh isolated target DB and restore
log "Step 3/5: Creating isolated target DB '${TARGET_DB}' in '${TARGET_CONTAINER}' and restoring..."
T_RESTORE_START="$(date +%s)"
docker exec -i "$TARGET_CONTAINER" psql -U "$TARGET_USER" -d postgres -X -v ON_ERROR_STOP=1 <<SQL >/dev/null
CREATE DATABASE "${TARGET_DB}" WITH TEMPLATE template0 ENCODING 'UTF8';
SQL
TARGET_CREATED=1

# Ensure non-superuser runtime role exists (NOBYPASSRLS) so RLS policies reference valid role
docker exec -i "$TARGET_CONTAINER" psql -U "$TARGET_USER" -d "$TARGET_DB" -X -v ON_ERROR_STOP=1 <<'SQL' >/dev/null
DO $$
BEGIN
  IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'openvietgis_runtime') THEN
    RAISE EXCEPTION 'restricted runtime role must already exist in the isolated cluster';
  END IF;
  IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'openvietgis_runtime'
             AND (rolsuper OR rolbypassrls OR rolcreaterole OR rolcreatedb)) THEN
    RAISE EXCEPTION 'unsafe runtime role';
  END IF;
END$$;
SQL

docker exec -i "$TARGET_CONTAINER" pg_restore -U "$TARGET_USER" -d "$TARGET_DB" \
  --exit-on-error < "$DUMP_FILE"
T_RESTORE_END="$(date +%s)"
RESTORE_SECONDS=$((T_RESTORE_END - T_RESTORE_START))
log "Restore complete: duration=${RESTORE_SECONDS}s"

# Step 4: Deterministic parity verification on restored DB
log "Step 4/5: Computing restored DB fingerprint and verifying schema/RLS/row-count parity..."
T_VERIFY_START="$(date +%s)"
fingerprint_db "$TARGET_CONTAINER" "$TARGET_USER" "$TARGET_DB" "${WORK_DIR}/dst"
DST_SCHEMA_SHA="$(cat "${WORK_DIR}/dst.schema.sha256")"
DST_ROWS_SHA="$(cat "${WORK_DIR}/dst.rows.sha256")"
DST_CONTENT_SHA="$(cat "${WORK_DIR}/dst.content.sha256")"
T_VERIFY_END="$(date +%s)"
VERIFY_SECONDS=$((T_VERIFY_END - T_VERIFY_START))

PARITY_STATUS="PASS"
if ! diff -u "${WORK_DIR}/src.meta" "${WORK_DIR}/dst.meta" > "${WORK_DIR}/meta.diff"; then
  PARITY_STATUS="FAIL(meta mismatch)"
  cat "${WORK_DIR}/meta.diff" >&2
fi
if [[ "$SRC_SCHEMA_SHA" != "$DST_SCHEMA_SHA" ]]; then
  PARITY_STATUS="FAIL(schema_sha256 mismatch)"
  diff -u "${WORK_DIR}/src.schema" "${WORK_DIR}/dst.schema" | head -n 40 >&2 || true
fi
if [[ "$SRC_ROWS_SHA" != "$DST_ROWS_SHA" ]]; then
  PARITY_STATUS="FAIL(rows_sha256 mismatch)"
  diff -u "${WORK_DIR}/src.rows" "${WORK_DIR}/dst.rows" | head -n 40 >&2 || true
fi

if [[ "$SRC_CONTENT_SHA" != "$DST_CONTENT_SHA" ]]; then
  PARITY_STATUS="FAIL(content_sha256 mismatch)"
fi

# Step 5: Additive-only migration check (if candidate dir provided) & schema preservation assertion
log "Step 5/5: Running additive-only compatibility check..."
ADDITIVE_RESULT="$(check_additive_migrations "$CANDIDATE_MIGRATION_DIR")"

T_END="$(date +%s)"
TOTAL_SECONDS=$((T_END - T_START))

REPORT="$(cat <<EOF
=== Spec 064 Staging Backup/Restore Verification Report ===
timestamp_utc=${TS_UTC}
source=${SOURCE_CONTAINER}/${SOURCE_DB}
target=${TARGET_CONTAINER}/${TARGET_DB}
source_size=${SRC_SIZE_PRETTY} (${SRC_SIZE_BYTES} bytes)
dump_size_bytes=${DUMP_BYTES}
dump_sha256=${DUMP_SHA256}
src_schema_sha256=${SRC_SCHEMA_SHA}
dst_schema_sha256=${DST_SCHEMA_SHA}
src_rows_sha256=${SRC_ROWS_SHA}
dst_rows_sha256=${DST_ROWS_SHA}
src_content_sha256=${SRC_CONTENT_SHA}
dst_content_sha256=${DST_CONTENT_SHA}
parity_status=${PARITY_STATUS}
${ADDITIVE_RESULT}
dump_duration_s=${DUMP_SECONDS}
restore_duration_s=${RESTORE_SECONDS}
verify_duration_s=${VERIFY_SECONDS}
total_recovery_duration_s=${TOTAL_SECONDS}
target_db_retained=${KEEP_TARGET_DB}
--- source_metadata ---
$(cat "${WORK_DIR}/src.meta")
EOF
)"

echo "$REPORT"
if [[ -n "$EVIDENCE_OUT" ]]; then
  mkdir -p "$(dirname "$EVIDENCE_OUT")"
  printf '%s\n' "$REPORT" > "$EVIDENCE_OUT"
  log "Report written to ${EVIDENCE_OUT}"
fi

if [[ "$PARITY_STATUS" != "PASS" ]]; then
  echo "FATAL: Staging restore verification failed: ${PARITY_STATUS}" >&2
  exit 20
fi
VERIFIED=1
log "SUCCESS: Staging backup/restore parity verified in ${TOTAL_SECONDS}s."

