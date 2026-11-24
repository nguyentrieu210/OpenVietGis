# Plan
1. Inspect inherited candidate and relevant contracts.
2. Implement bounded scope: Reauthenticate managed SSE with fresh scoped DB session off event loop, invalidate permission epoch on GIS rule/group mutation, suppress revocation notifications for still-valid sessions, allowlist event names.
3. Run and record: Real isolated credential revocation, actor/tenant isolation and ACL epoch regression; existing security and secret checker tests.
4. Preserve blockers: Independent security review, dependency/history scans, credential-instance binding, revocation races, local-copy handling, legal audit.
