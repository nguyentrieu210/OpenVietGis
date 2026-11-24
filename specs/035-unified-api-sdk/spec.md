# Spec 035 — Unified API and SDK
Parent R002–R006/R015; depends on Spec 033/034.

Objective: make the existing public API transport consistent without changing backend route or permission semantics.

- ApiClient owns the API base exactly once. SDK request paths are relative to that base, with a leading slash; invalid/ambiguous or already-prefixed paths fail before fetch.
- Use shared project path construction and encode scope identifiers; event streams use the same configured base.
- Keep cookie and CSRF behavior unchanged. Never automatically retry uncertain writes.
- Normalize existing code/detail/message/validation error envelopes while preserving status, response body and specialized error subclasses.
- Fix managed QGIS list, revoke and desktop-command URL prefix duplication.
- Migrate scoped export and two project event streams to shared transport URL handling.
- Preserve six existing generated OpenAPI contracts; record hand-written and separately hosted interfaces for subsequent parity work.
- No production deployment, new API version, backend authentication policy changes or full API coverage claim.

Acceptance: SDK regression, typecheck and lint, OpenAPI validation and generated parity, frontend build, and architecture checks. Live end-to-end and complete route parity remain separate.
