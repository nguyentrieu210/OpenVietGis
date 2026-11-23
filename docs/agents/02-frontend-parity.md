# Agent 02 — Frontend parity and consolidation

## Scope and ownership
Own: documentation report docs/qa/agent-02-frontend.md only.
Read: apps/web, apps/web-next, packages/client-sdk, infra/prod and public routing docs.
No deploy, bundler installs or heavy builds. No deletion of either frontend.

## Task
1. Identify live entrypoint from docs/qa/2026-11-23-control-kit-audit.md, deployment manifest assumptions and Nginx configs if available.
2. Build evidence-based route/feature parity: login, layers, editing, CAD, 3D, AI/copilot, layout/print, plugin number-house, offline, account roles, import/export.
3. Record which frontend owns a feature, and label UNKNOWN where only labels/components exist without verified runtime.
4. Trace concrete imports/route definitions, application state and API calls.
5. Propose a non-breaking deprecation strategy and acceptance tests for one canonical frontend.

## Deliverable
Produce docs/qa/agent-02-frontend.md with feature matrix, cited paths and P0 blockers.
Do not claim a capability is live solely because code exists.
