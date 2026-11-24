# Security — evidence policy

Implementation scope: Reauthenticate managed SSE with fresh scoped DB session off event loop, invalidate permission epoch on GIS rule/group mutation, suppress revocation notifications for still-valid sessions, allowlist event names.

Run evidence: Real isolated credential revocation, actor/tenant isolation and ACL epoch regression; existing security and secret checker tests. See [candidate runbook](../../docs/community/CANDIDATE_RUNBOOK.md) for exact candidate logs. Local checks are not release approval.

Open: Independent security review, dependency/history scans, credential-instance binding, revocation races, local-copy handling, legal audit.
