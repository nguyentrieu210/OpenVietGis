# Validation evidence — candidate
- Base: c4337e2 (integrated V1 preflight; branch descended from verified main).
- Python integration: test_team_chat_and_task_isolation passed against a dedicated loopback PostGIS test container with the restricted runtime DB role. Full Alembic upgrade from base to 0029_team_chat_tasks succeeded. Test verifies real persistence, human session/CSRF, editor eligibility, project/tenant denial, scoped-member denial, map position validation, status mutation and stale revision.
- Frontend: pnpm --filter @openvietgis/web-next build passed (TypeScript and Vite).
- Browser: team-collaboration-smoke.mjs passed in real Chromium and MapLibre with deterministic mocked HTTP endpoints. Exercises message rendering, CSRF, task creation and map position, account switching, My tasks, status update, and restricted UI. API behavior is separately verified on the real database; this does not claim full production browser-to-API acceptance.
- Release remains pending: independent CAD migration also extends 0028; converge migration ancestry, perform code/security review, promote prerequisite candidate, run full E2E and release gates before live deployment.

## V2 real-time verification
- Three integration tests passed on dedicated PostGIS database through 0030 migration; includes real project authorization, private chat participant isolation (including organization owner), persistent read receipts, public-only message linkage, CSRF, task revisions and existing V1 regression.
- Real Uvicorn HTTP SSE test received committed chat and task events within the 4-second acceptance budget, with no periodic client polling.
- TypeScript/Vite build passed. Real Chromium browser test served built frontend and a persistent HTTP SSE stream with deterministic fixture responses; verified external push updates without refresh, private chat, message-to-task conversion, notification read, map location, Kanban drag/drop and restricted UI.
- Still not deployed: live reverse-proxy/production event-stream acceptance, independent CAD migration convergence and broader release gate required.
