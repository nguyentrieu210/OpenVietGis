# Agent 04 — Release provenance, deploy and rollback

## Scope and ownership
Own: documentation report docs/qa/agent-04-release.md only.
Read: infra, docs/adr/ADR-012-deployment.md, tools/control_kit_audit.py and CI.
No service restart, environment changes, data mutations, symlink changes or push.

## Task
1. Compare existing documented release procedure with immutable build, release manifest and source SHA.
2. Distinguish dev and preview services from the canonical public runtime. Do not guess by names.
3. Define deploy safety gates (checksum, backend, migrations, plugin, auth, E2E).
4. Define a rollback rehearsal plan including DB backward-compatibility restrictions.
5. Identify monitoring gaps and ensure structure-only audit is never portrayed as product certification.

## Deliverable
Produce docs/qa/agent-04-release.md with validated statements and explicit unknowns.
