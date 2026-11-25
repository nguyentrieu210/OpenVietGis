# Phase 1 — Logical model (reuse first; no migration approved)
- Binding (extend R027): id, organization/project, CAD document/layer, GIS source/layer, directions, authority, mapping profile revision, capability set, permission epoch, status.
- SpatialLink: existing CAD version + native handle ↔ source/feature/revision, stable link id, lineage, transform revision, cardinality, state; unique active scoped tuple; no cross-tenant FK. Preserve prior links; stale/tombstone explicit.
- Placement: existing CadPlacement + explicit CRS pipeline, axis/units, Z/datum, controls/residuals, tolerance, immutable version.
- MappingProfile: versioned entity/attribute/style mappings, native payload refs, loss/unsupported policy.
- Preview: scope, base CAD digest/version and GIS content/schema revisions, exact operations, warnings/loss, approval scope/expiry. Preview never mutates.
- SyncOperation: existing R006/R027 operation id and actor, origin, expected revisions, payload digest, state (staged/approved/committing/accepted/conflict/denied/failed), receipt, audit/trace. Reused id with different payload rejected.
- Native blob: immutable content-addressed bytes, tenant scoped authorization; staged orphan cleanup with retention; metadata publication only after verification.
- Delta/cursor: existing R029 durable ordered stream, authorized scoped delivery, ack after apply; gap triggers scoped recovery.
- Conflict: existing conflict record with revisions, permitted diff, status and resolution changeset; no implicit merge.
- Local draft: uncommitted, encrypted where needed, scoped to user/device; no credentials; revocation quarantine and policy-controlled disposal.

State invariants: server rechecks rights + revision inside mutation; canonical GIS via R006 only; CAD document save alone does not mutate GIS; cross-store failure leaves recoverable state not accepted; deletion requires explicit approval; no source revision guessed. Schema changes must be additive and reviewed after R031.
