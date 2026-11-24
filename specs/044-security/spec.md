# Spec 044 — Security
Parent R002/R006/R011/R015; stabilization program Spec 030.

Reauthenticate managed SSE with fresh scoped DB session off event loop, invalidate permission epoch on GIS rule/group mutation, suppress revocation notifications for still-valid sessions, allowlist event names.

Acceptance evidence: Real isolated credential revocation, actor/tenant isolation and ACL epoch regression; existing security and secret checker tests.

Remaining release obligations: Independent security review, dependency/history scans, credential-instance binding, revocation races, local-copy handling, legal audit.
