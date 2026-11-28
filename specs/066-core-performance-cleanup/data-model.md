# Operational model (design, not schema migration)
CacheIdentity: classification, tenant_id, actor_scope, permission_revision, project_id, resource_id, query_hash, data_revision, CRS/style/version, build_id; omit scope fields only for explicitly public immutable resources.
RevisionEvent: tenant/project/resource, revision, sequence, operation, changed_extent, idempotency_key, committed_at; reuse existing outbox model rather than duplicate table.
SessionEpoch: identity and permission scope, route generation, active engine; stale response rejected.
CleanupRecord: path, role, consumers, replacement, parity evidence, owner, decision, rollback.
BenchmarkRecord: candidate SHA/image digest, environment, dataset hash, cold/warm condition, samples, percentiles, CPU/RAM/GPU/network, budget, outcome.
No schema change assumed. Any required additive migration needs separate compatibility/rollback review.
