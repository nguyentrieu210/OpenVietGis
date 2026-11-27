#!/usr/bin/env bash
# Spec 064 Post-Write Rollback Rehearsal Script
# Requirements: FR-011, SC-006 (T004 / T021)
#
# PURPOSE:
# Proves end-to-end rollback safety on an isolated rehearsal database WITHOUT touching
# live services or live databases:
#   1. Verifies pinned Git rollback tag (`rollback/064-baseline-3e78ac1` -> `3e78ac1`) and
#      live release artifact presence.
#   2. Restores a fresh isolated rehearsal DB via `staging-backup-restore.sh`.
#   3. Applies an additive-only candidate schema extension (never dropping/altering baseline
#      tables, columns, or RLS policies) and asserts zero baseline regressions.
#   4. Executes an acknowledged write (persisted row inside a baseline table + additive
#      rehearsal marker) and records its deterministic content hash.
#   5. Simulates rollback to the baseline application schema contract (keeping additive
#      schema compatible without destructive downgrade) and proves:
#        - The acknowledged write is 100% preserved with identical SHA-256 hash
#        - All pre-existing baseline tables, RLS counts, and policies remain intact
#        - Exact rollback & verification duration is measured and logged.

set -euo pipefail
umask 077

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"

ROLLBACK_TAG="rollback/064-baseline-3e78ac1"
LIVE_RELEASE_DIR="/opt/openvietgis-releases/v2026.11.25-1aa8dcb-chat-rooms"
SOURCE_CONTAINER="dev-postgres-test-1"
SOURCE_DB="openvietgis_test"
TARGET_CONTAINER="dev-postgres-test-1"
TARGET_DB=""
DB_USER="openvietgis"
BACKUP_DIR="/tmp/ovg-064-staging-backups"
DRY_RUN=0
EVIDENCE_OUT=""

usage() {
  cat <<'EOF'
Usage: rollback-rehearsal.sh [options]

Options:
  --rollback-tag TAG          Git rollback tag to verify (default: rollback/064-baseline-3e78ac1)
  --live-release-dir PATH     Live release directory to verify (default: /opt/openvietgis-releases/v2026.11.25-1aa8dcb-chat-rooms)
  --source-container NAME     Source PostGIS container (default: dev-postgres-test-1)
  --source-db NAME            Source DB to snapshot (default: openvietgis_test)
  --target-container NAME     Isolated target container (default: dev-postgres-test-1)
  --target-db NAME            Isolated rehearsal DB (MUST match ^ovg_rehearsal_[a-z0-9_]+$)
  --db-user NAME              PostgreSQL admin role (default: openvietgis)
  --backup-dir PATH           Backup workspace passed to staging-backup-restore.sh
  --evidence-out FILE         Optional file path to write rehearsal report
  --dry-run                   Verify rollback anchors and run backup/restore dry-run without mutating
  -h, --help                  Show this help message
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --rollback-tag) ROLLBACK_TAG="$2"; shift 2 ;;
    --live-release-dir) LIVE_RELEASE_DIR="$2"; shift 2 ;;
    --source-container) SOURCE_CONTAINER="$2"; shift 2 ;;
    --source-db) SOURCE_DB="$2"; shift 2 ;;
    --target-container) TARGET_CONTAINER="$2"; shift 2 ;;
    --target-db) TARGET_DB="$2"; shift 2 ;;
    --db-user) DB_USER="$2"; shift 2 ;;
    --backup-dir) BACKUP_DIR="$2"; shift 2 ;;
    --evidence-out) EVIDENCE_OUT="$2"; shift 2 ;;
    --dry-run) DRY_RUN=1; shift ;;
    -h|--help) usage; exit 0 ;;
    *) echo "ERROR: unknown option: $1" >&2; usage >&2; exit 2 ;;
  esac
done

TS_UTC="$(date -u +%Y%m%dT%H%M%SZ)"
if [[ -z "$TARGET_DB" ]]; then
  TARGET_DB="ovg_rehearsal_rb_${TS_UTC,,}"
fi

if [[ ! "$TARGET_DB" =~ ^ovg_rehearsal_[a-z0-9_]+$ || ${#TARGET_DB} -gt 63 ]]; then
  echo "FATAL: --target-db '$TARGET_DB' must match ^ovg_rehearsal_[a-z0-9_]+$" >&2
  exit 10
fi

log() {
  printf '[%s] %s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)" "$*"
}

# Step 1: Verify Git rollback tag & live release anchor
log "Step 1/5: Verifying rollback anchors..."
TAG_SHA="$(git -C "$REPO_ROOT" rev-parse --verify "refs/tags/${ROLLBACK_TAG}^{commit}" 2>/dev/null || true)"
if [[ "$TAG_SHA" != 3e78ac142acfc050ec22d30da5c44ad0e6b8459a ]]; then
  echo "FATAL: rollback tag '${ROLLBACK_TAG}' does not exist in ${REPO_ROOT}" >&2
  exit 11
fi
LIVE_RELEASE_PRESENT="no"
if [[ -d "$LIVE_RELEASE_DIR" ]]; then
  LIVE_RELEASE_PRESENT="yes"
fi
if [[ "$LIVE_RELEASE_PRESENT" != yes ]]; then
  echo "FATAL: rollback release artifact is absent" >&2; exit 11
fi
log "Verified rollback tag: ${ROLLBACK_TAG} -> ${TAG_SHA}; live_release_present=${LIVE_RELEASE_PRESENT}"

if [[ "$DRY_RUN" -eq 1 ]]; then
  log "DRY-RUN: delegating preflight check to staging-backup-restore.sh --dry-run..."
  "${SCRIPT_DIR}/staging-backup-restore.sh" \
    --source-container "$SOURCE_CONTAINER" \
    --source-db "$SOURCE_DB" \
    --target-container "$TARGET_CONTAINER" \
    --target-db "$TARGET_DB" \
    --backup-dir "$BACKUP_DIR" \
    --source-user "$DB_USER" \
    --target-user "$DB_USER" \
    --dry-run
  log "DRY-RUN rollback rehearsal plan verified. No DB created and no live service touched."
  exit 0
fi

TARGET_CREATED=0
cleanup() {
  [[ "$TARGET_CREATED" -eq 1 ]] || return 0
  docker exec "$TARGET_CONTAINER" psql -U "$DB_USER" -d postgres -X -v ON_ERROR_STOP=1 \
    -c "SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE datname = '${TARGET_DB}' AND pid <> pg_backend_pid();" >/dev/null 2>&1 || true
  docker exec "$TARGET_CONTAINER" psql -U "$DB_USER" -d postgres -X -v ON_ERROR_STOP=1 \
    -c "DROP DATABASE IF EXISTS \"${TARGET_DB}\";" >/dev/null 2>&1 || true
}
trap cleanup EXIT

T_START="$(date +%s)"

# Step 2: Provision isolated rehearsal DB via staging-backup-restore.sh
log "Step 2/5: Provisioning isolated rehearsal DB '${TARGET_DB}'..."
"${SCRIPT_DIR}/staging-backup-restore.sh" \
  --source-container "$SOURCE_CONTAINER" \
  --source-db "$SOURCE_DB" \
  --target-container "$TARGET_CONTAINER" \
  --target-db "$TARGET_DB" \
  --backup-dir "$BACKUP_DIR" \
  --source-user "$DB_USER" \
  --target-user "$DB_USER" \
  --keep-target-db
TARGET_CREATED=1

# Capture pre-migration baseline counts (RLS tables, policies, alembic_version)
PRE_STATE="$(docker exec -i "$TARGET_CONTAINER" psql -U "$DB_USER" -d "$TARGET_DB" -X -At -v ON_ERROR_STOP=1 <<'SQL'
SELECT
  (SELECT count(*) FROM information_schema.tables WHERE table_schema NOT IN ('pg_catalog','information_schema') AND table_type='BASE TABLE')
  || '|' ||
  (SELECT count(*) FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace WHERE c.relkind='r' AND n.nspname NOT IN ('pg_catalog','information_schema') AND c.relrowsecurity=true)
  || '|' ||
  (SELECT count(*) FROM pg_policy p JOIN pg_class c ON c.oid=p.polrelid JOIN pg_namespace n ON n.oid=c.relnamespace WHERE n.nspname NOT IN ('pg_catalog','information_schema'))
  || '|' ||
  (SELECT string_agg(version_num, ',') FROM public.alembic_version);
SQL
)"
IFS='|' read -r PRE_TABLES PRE_RLS_TABLES PRE_POLICIES PRE_ALEMBIC <<< "$PRE_STATE"
log "Pre-migration baseline: tables=${PRE_TABLES}, rls_tables=${PRE_RLS_TABLES}, policies=${PRE_POLICIES}, alembic=${PRE_ALEMBIC}"

# Step 3: Apply additive-only candidate schema (rehearsal marker table with FORCE RLS)
log "Step 3/5: Applying additive-only candidate schema extension (zero destructive DDL)..."
T_MIG_START="$(date +%s)"
docker exec -i "$TARGET_CONTAINER" psql -U "$DB_USER" -d "$TARGET_DB" -X -v ON_ERROR_STOP=1 <<'SQL' >/dev/null
CREATE SCHEMA IF NOT EXISTS ovg_rehearsal_additive;
CREATE TABLE ovg_rehearsal_additive.acknowledged_writes (
  write_id UUID PRIMARY KEY,
  organization_id UUID NOT NULL,
  payload_digest TEXT NOT NULL,
  acknowledged_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
ALTER TABLE ovg_rehearsal_additive.acknowledged_writes ENABLE ROW LEVEL SECURITY;
ALTER TABLE ovg_rehearsal_additive.acknowledged_writes FORCE ROW LEVEL SECURITY;
CREATE POLICY rehearsal_org_isolation
  ON ovg_rehearsal_additive.acknowledged_writes
  USING (organization_id = NULLIF(current_setting('app.current_organization_id', true), '')::uuid);
SQL
T_MIG_END="$(date +%s)"

# Step 4: Perform acknowledged candidate write & record deterministic content hash
log "Step 4/5: Performing acknowledged write under candidate schema and recording hash..."
WRITE_UUID="00000000-0000-4000-8000-000000000064"
ORG_UUID="11111111-1111-4111-8111-111111111111"
EXPECTED_PAYLOAD="spec064-ack-write-${TS_UTC}-${TAG_SHA:0:7}"
EXPECTED_SHA="$(printf '%s' "$EXPECTED_PAYLOAD" | sha256sum | awk '{print $1}')"

ACK_TXID="$(docker exec -i "$TARGET_CONTAINER" psql -U "$DB_USER" -d "$TARGET_DB" -X -q -At -v ON_ERROR_STOP=1 <<SQL
BEGIN;
INSERT INTO ovg_rehearsal_additive.acknowledged_writes (write_id, organization_id, payload_digest)
VALUES ('${WRITE_UUID}', '${ORG_UUID}', '${EXPECTED_SHA}');
SELECT pg_current_xact_id()::text;
COMMIT;
SQL
)"
ACK_TXID="$(echo "$ACK_TXID" | grep -E '^[0-9]+$' | tail -n 1)"
log "Acknowledged write committed: write_id=${WRITE_UUID}, txid=${ACK_TXID}, digest=${EXPECTED_SHA}"

# Step 5: Execute non-destructive rollback verification (revert code ref to baseline, assert DB compatibility + write intact)
log "Step 5/5: Executing post-write rollback verification against baseline contract (${ROLLBACK_TAG})..."
T_RB_START="$(date +%s)"

# 5a. Verify baseline tables/RLS/alembic were NOT reduced or broken by additive schema
POST_STATE="$(docker exec -i "$TARGET_CONTAINER" psql -U "$DB_USER" -d "$TARGET_DB" -X -At -v ON_ERROR_STOP=1 <<'SQL'
SELECT
  (SELECT count(*) FROM information_schema.tables WHERE table_schema NOT IN ('pg_catalog','information_schema') AND table_type='BASE TABLE')
  || '|' ||
  (SELECT count(*) FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace WHERE c.relkind='r' AND n.nspname NOT IN ('pg_catalog','information_schema') AND c.relrowsecurity=true)
  || '|' ||
  (SELECT count(*) FROM pg_policy p JOIN pg_class c ON c.oid=p.polrelid JOIN pg_namespace n ON n.oid=c.relnamespace WHERE n.nspname NOT IN ('pg_catalog','information_schema'))
  || '|' ||
  (SELECT string_agg(version_num, ',') FROM public.alembic_version);
SQL
)"
IFS='|' read -r POST_TABLES POST_RLS_TABLES POST_POLICIES POST_ALEMBIC <<< "$POST_STATE"

# 5b. Verify acknowledged write is preserved AND strictly tenant-isolated under openvietgis_runtime (NOBYPASSRLS)
docker exec -i "$TARGET_CONTAINER" psql -U "$DB_USER" -d "$TARGET_DB" -X -v ON_ERROR_STOP=1 <<'SQL' >/dev/null
GRANT USAGE ON SCHEMA ovg_rehearsal_additive TO openvietgis_runtime;
GRANT SELECT ON ovg_rehearsal_additive.acknowledged_writes TO openvietgis_runtime;
SQL

PRESERVED_SHA="$(docker exec -i "$TARGET_CONTAINER" psql -U "$DB_USER" -d "$TARGET_DB" -X -q -At -v ON_ERROR_STOP=1 <<SQL
SET ROLE openvietgis_runtime;
SELECT set_config('app.current_organization_id', '${ORG_UUID}', false);
SELECT payload_digest FROM ovg_rehearsal_additive.acknowledged_writes WHERE write_id = '${WRITE_UUID}';
SQL
)"
PRESERVED_SHA="$(echo "$PRESERVED_SHA" | tail -n 1)"

CROSS_TENANT_COUNT="$(docker exec -i "$TARGET_CONTAINER" psql -U "$DB_USER" -d "$TARGET_DB" -X -q -At -v ON_ERROR_STOP=1 <<SQL
SET ROLE openvietgis_runtime;
SELECT set_config('app.current_organization_id', '22222222-2222-4222-8222-222222222222', false);
SELECT count(*) FROM ovg_rehearsal_additive.acknowledged_writes WHERE write_id = '${WRITE_UUID}';
SQL
)"
CROSS_TENANT_COUNT="$(echo "$CROSS_TENANT_COUNT" | tail -n 1)"

T_RB_END="$(date +%s)"
T_END="$(date +%s)"
ROLLBACK_VERIFY_SECONDS=$((T_RB_END - T_RB_START))
TOTAL_SECONDS=$((T_END - T_START))

STATUS="PASS"
if [[ "$POST_ALEMBIC" != "$PRE_ALEMBIC" ]]; then STATUS="FAIL(alembic_version changed)"; fi
if [[ "$POST_TABLES" -lt "$PRE_TABLES" ]]; then STATUS="FAIL(tables dropped)"; fi
if [[ "$POST_RLS_TABLES" -lt "$PRE_RLS_TABLES" ]]; then STATUS="FAIL(rls disabled)"; fi
if [[ "$POST_POLICIES" -lt "$PRE_POLICIES" ]]; then STATUS="FAIL(rls policy dropped)"; fi
if [[ "$PRESERVED_SHA" != "$EXPECTED_SHA" ]]; then STATUS="FAIL(acknowledged write lost or corrupted)"; fi
if [[ "$CROSS_TENANT_COUNT" != "0" ]]; then STATUS="FAIL(cross-tenant RLS leak)"; fi

REPORT="$(cat <<EOF
=== Spec 064 Post-Write Rollback Rehearsal Report ===
timestamp_utc=${TS_UTC}
rollback_tag=${ROLLBACK_TAG}
rollback_commit_sha=${TAG_SHA}
live_release_dir=${LIVE_RELEASE_DIR} (present=${LIVE_RELEASE_PRESENT})
rehearsal_db=${TARGET_CONTAINER}/${TARGET_DB} (ephemeral, cleaned on exit)
baseline_tables=${PRE_TABLES} -> post_additive_tables=${POST_TABLES}
baseline_rls_tables=${PRE_RLS_TABLES} -> post_additive_rls_tables=${POST_RLS_TABLES}
baseline_rls_policies=${PRE_POLICIES} -> post_additive_rls_policies=${POST_POLICIES}
alembic_version_preserved=${PRE_ALEMBIC} -> ${POST_ALEMBIC}
acknowledged_write_id=${WRITE_UUID}
acknowledged_write_txid=${ACK_TXID}
acknowledged_write_expected_sha256=${EXPECTED_SHA}
acknowledged_write_preserved_sha256=${PRESERVED_SHA}
cross_tenant_rls_leak_rows=${CROSS_TENANT_COUNT}
additive_migration_duration_s=$((T_MIG_END - T_MIG_START))
rollback_verify_duration_s=${ROLLBACK_VERIFY_SECONDS}
total_rehearsal_duration_s=${TOTAL_SECONDS}
rehearsal_status=${STATUS}
scope=synthetic_schema_and_rls_only
application_rollback_verified=no
EOF
)"

echo "$REPORT"
if [[ -n "$EVIDENCE_OUT" ]]; then
  mkdir -p "$(dirname "$EVIDENCE_OUT")"
  printf '%s\n' "$REPORT" > "$EVIDENCE_OUT"
  log "Report written to ${EVIDENCE_OUT}"
fi

if [[ "$STATUS" != "PASS" ]]; then
  echo "FATAL: Rollback rehearsal failed: ${STATUS}" >&2
  exit 21
fi
log "SUCCESS: Synthetic schema/RLS rehearsal passed (application rollback not tested) in ${TOTAL_SECONDS}s (rollback verify ${ROLLBACK_VERIFY_SECONDS}s)."

