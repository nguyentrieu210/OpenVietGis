# Agent 05 — Automated QA and release gates

## Scope and ownership
Own: tools/tests/test_control_kit_audit.py, docs/qa/agent-05-qa.md.
May enhance tests only; do not modify production code, CI credentials or application behavior.

## Task
1. Review tools/control_kit_audit.py for false PASS/FAIL cases and path traversal or stale ref hazards.
2. Improve offline unit tests for malformed manifest data and Cloudflare beacon scope if feasible.
3. Identify existing CI workflows and missing coverage for canonical frontend, plugin, auth, CAD/3D.
4. Map acceptance tests to R001-R015 and public login/editor/AI/plugin flows.
5. Run only lightweight tests. Do not install dependencies or run full E2E on the agent machine.

## Deliverable
Produce docs/qa/agent-05-qa.md, focused tests and observed test outputs; label untouched checks NOT_VERIFIED.
