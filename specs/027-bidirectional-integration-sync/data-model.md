# Data Model — R027 Bidirectional Sync

**Status:** Draft, subject to ADR and PostgreSQL migration review. No tables exist as a result of this document.

## Scope and invariants

All persistent entities are owned by one organization and project; every lookup and worker execution is tenant-scoped under PostgreSQL RLS and the existing GIS ACL. No connector may grant itself access to a layer/field/feature excluded by the current server policy. Use UUID primary keys; identify every external system explicitly and reject ambiguous external identity rather than matching by geometry/name.

### ConnectorBinding

`id`, `organization_id`, `project_id`, `layer_id`, `adapter_key`, `adapter_version`, `external_dataset_ref` (opaque, redacted for ordinary viewers), `connection_mode` (agent/HTTPS/manual), `lifecycle` (draft/disabled/active/paused/revoked/error), `verified_capabilities`, `credential_ref_id`, `created_by`, `approved_by`, `created_at`, `updated_at`, `revision`. Unique candidate constraint: tenant+project+layer+adapter+external dataset among active bindings, with explicit multi-binding policy. Activation is separate and audited.

### SyncProfile

`binding_id`, `direction` (`pull_only`/`push_only`/`bidirectional_manual`/`bidirectional_auto`), `write_grants` (default none), `allowed_fields`, `field_mapping`, `external_primary_key`, `crs_source`, `crs_target`, `verified_transform_ref`, `geometry_policy`, `conflict_policy` (default pause and review), `delete_policy` (default deny), `max_batch_size`, `event_mode`, `schedule`, `revision`. Changes invalidate any in-flight approvals.

### ExternalIdentityMap

`id`, `binding_id`, `external_dataset_id`, `external_feature_id`, `ovg_feature_id`, `mapping_status` (active/tombstone/ambiguous), `mapping_revision`, `created_at`, `updated_at`. Unique on binding+external dataset+external feature ID, and on binding+OVG feature ID unless a reviewed split/merge workflow is used. External IDs are never derived from display text or row order.

### SyncCursor

`binding_id`, `side` (ovg/external), `source_epoch`, `last_seen_sequence`, `last_acked_sequence`, `checkpoint_digest`, `gap_state`, `updated_at`, `revision`. Monotonic advances only after durable applied/acknowledged outcomes; an epoch change forces reconciliation.

### ChangeEnvelope / SyncOperation

`operation_id` (UUID idempotency key), `binding_id`, `source_side`, `source_event_id`, `source_feature_id`, `origin_chain` (bounded), `action` (create/update/delete_candidate), `base_source_revision`, `base_target_revision`, `source_schema_revision`, `target_schema_revision`, `payload_digest`, `payload_ref` (encrypted/restricted/expiring), `status` (pending/previewed/approved/applied/acknowledged/conflicted/rejected/held), `created_at`. Immutable identity + digest; duplicate operation ID with mismatched digest is a hard error.

### SyncRun / SyncConflict / SyncApproval

- **SyncRun**: `id`, binding, run mode, initiating actor, source and target cursors, UTC timestamps, state, counters, versioned summary; preserves partial completion state.
- **SyncConflict**: `id`, run, feature mapping, stale revisions, type (geometry/attribute/schema/delete/permission), redacted diff reference, resolution state, resolution approver and audited decision; no automatic resolution of contradictory geometry.
- **SyncApproval**: `id`, run, exact preview digest, approved operation IDs, actor, signature/authorization context, expiry, one-time state. Reject if any input/policy revision changed.
- **DeliveryAttempt**: operation ID, side/adapter, bounded attempt index, next attempt, response digest, retry classification and final receipt. Do not store credentials or full private payloads in logs.

### ConnectorCredentialRef

`id`, `binding_id`, credential kind, managed-secret locator, scope fingerprint, created/expired/rotated/revoked timestamps. The returned API representation is always metadata-only.

## Transactions and ordering

1. Stage incoming envelope and identity mapping in a scoped, durable inbox; verify idempotency, origin and permissions.
2. Obtain current source/target revisions; create deterministic preview with a digest and unresolved conflicts.
3. On approved commit, verify unexpired approval and policy/revision pins; submit canonical OpenVietGIS mutations through R006 only.
4. Record operation receipt and an immutable event/outbox entry atomically with the applied local transaction (where supported by existing R006 implementation).
5. Deliver outbound to a trusted conditional-update receiver; persist acknowledged revision before advancing cursor. On delivery error retry safely with bounded backoff.
6. Revocation stops dequeueing new work and invalidates approvals immediately; revalidation at worker execution is mandatory.

## Retention/recovery

- Exact retention periods require a dedicated security/data-governance decision before migrations.
- Audit/change history must be logically immutable to ordinary users.
- Payloads contain least-privilege fields; encrypted blobs expire and are purged after configured retention. Ledger metadata must remain sufficient to prove delivery outcomes.
- Backup/restore drill must cover database, outbox/inbox, external-ID journal, provider checkpoint and credential re-provisioning.
- No destructive data migration without compatibility plan and rollback evidence.
