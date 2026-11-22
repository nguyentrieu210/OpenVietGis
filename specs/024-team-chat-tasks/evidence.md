# Validation evidence — candidate
- Base: c4337e2 (integrated V1 preflight; branch descended from verified main).
- Python integration: test_team_chat_and_task_isolation passed against a dedicated loopback PostGIS test container with the restricted runtime DB role. Full Alembic upgrade from base to 0029_team_chat_tasks succeeded. Test verifies real persistence, human session/CSRF, editor eligibility, project/tenant denial, scoped-member denial, map position validation, status mutation and stale revision.
- Frontend: pnpm --filter @openvietgis/web-next build passed (TypeScript and Vite).
- Browser: team-collaboration-smoke.mjs passed in real Chromium and MapLibre with deterministic mocked HTTP endpoints. Exercises message rendering, CSRF, task creation and map position, account switching, My tasks, status update, and restricted UI. API behavior is separately verified on the real database; this does not claim full production browser-to-API acceptance.
- Release remains pending: independent CAD migration also extends 0028; converge migration ancestry, perform code/security review, promote prerequisite candidate, run full E2E and release gates before live deployment.
