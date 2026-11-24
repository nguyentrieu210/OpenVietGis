# Spec 045 — Release & Deployment
Parent R015; stabilization program Spec 030.

Build canonical frontend and deterministic QGIS artifact from same clean candidate. Verify artifact set, checksum, lockfiles and migration graph; never promote from local evidence alone.

Acceptance evidence: Candidate quality report and release_consolidation manifest/verify.

Remaining release obligations: Independent CI, signed backend/MCP images, database backup/restore, rollback, existing manifest compatibility, explicit deployment approval.
