# Agent 02 — Canonical frontend regression review

## Scope and ownership
Own: documentation report docs/qa/agent-02-frontend.md only.
Read: apps/web-next, packages/client-sdk, packages/spatial-workspace, infra/prod, current architecture docs and the legacy history index.
The removed apps/web tree is historical evidence only and must not be restored.

## Task
1. Identify the canonical live entrypoint and release manifest assumptions.
2. Review route/feature regressions: login, layers, editing, CAD, 3D, AI/copilot, layout/print, house numbering, offline, roles and import/export.
3. Label UNKNOWN where runtime evidence is absent.
4. Trace concrete imports, route definitions, state and API calls.
5. Propose narrow regression tests; do not create a second frontend.

## Deliverable
Update docs/qa/agent-02-frontend.md only when a new dated audit is requested.
Do not rewrite historical evidence and do not claim a capability is live solely because code exists.
