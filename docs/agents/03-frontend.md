# Agent frontend — Integrations UI

Governing spec R025, tasks T011–T012. Use existing settings/integrations UX and project RBAC. Build a disabled-by-default MicroStation V8i connector card with clear status: experimental, Windows Bridge needed, no live sync until backend capability is implemented.

Do not ship fake success claims or expose API keys, local file paths or cadastral private records. Prefer project-scoped pairing UI after backend contract exists; until then show clear readiness and safe download instructions.

Owned scope: web-next client views/components, related tests and `docs/qa/r025-frontend.md`. Do not alter Core external-source write semantics.
