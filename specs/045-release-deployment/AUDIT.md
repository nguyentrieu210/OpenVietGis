# Release & Deployment — evidence policy

Implementation scope: Build canonical frontend and deterministic QGIS artifact from same clean candidate. Verify artifact set, checksum, lockfiles and migration graph; never promote from local evidence alone.

Run evidence: Candidate quality report and release_consolidation manifest/verify. See [candidate runbook](../../docs/community/CANDIDATE_RUNBOOK.md) for exact candidate logs. Local checks are not release approval.

Open: Independent CI, signed backend/MCP images, database backup/restore, rollback, existing manifest compatibility, explicit deployment approval.
