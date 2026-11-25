# Draft adapter contract — conceptual, not a new public SDK
Reuse existing command registry, client SDK, processing jobs and ChangeSet contracts. Introduce a seam only when a tested replacement needs it.

- Descriptor: engineId, version, artifactDigest, capabilityIds, supported formats/CRS, read/write semantics, budgets, readiness, license reference.
- Execution request: registered command ID, trusted server-derived actor/tenant/project scope, resource identifiers, expected revision, idempotency ID, validated parameters, cancellation and trace ID.
- Result: status (ok/unsupported/denied/conflict/cancelled/failed), typed output or proposed ChangeSet, warnings, provenance (engine version, input revisions, CRS/units, parameters), resource usage.
- Lifecycle: prepare, activate, cancel, dispose; one scope and generation token; ignore late callbacks. Browser never authorizes writes; server rechecks current grants and revision transactionally. No credentials in CAD relay messages. Worker gets short-lived narrow access, no unrestricted network.
- Deterministic output fixture and error mapping per capability; preview is not commit; retries with same idempotency ID must not duplicate writes. Rollback uses configuration only where data compatibility established.
- Capability flags: legacy default, shadow (no side-effect), isolated candidate, accepted. No flag auto-enables from a dependency install. No dual writes.
