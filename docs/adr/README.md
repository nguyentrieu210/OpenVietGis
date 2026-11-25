# OpenVietGIS Architecture Decision Records

ADRs record consequential technical decisions that are expensive to reverse. They do not replace
the Constitution or feature specifications.

## Status lifecycle

- **Proposed**: under review and not yet binding.
- **Accepted**: current architecture decision.
- **Superseded**: replaced by a newer ADR; retained as historical context.
- **Deprecated**: no longer recommended but may still describe deployed legacy behavior.

To replace an accepted decision, create a new ADR that explains the new context and explicitly names
the older ADR it supersedes. Then change the older ADR status to `Superseded by ADR-NNN`. Never
rewrite historical rationale as though the original decision was never made.

## Current accepted ADRs

- [ADR-001 — Monorepo](ADR-001-monorepo.md)
- [ADR-002 — Modular monolith](ADR-002-modular-monolith.md)
- [ADR-003 — React SPA](ADR-003-react-spa.md)
- [ADR-004 — Python/FastAPI](ADR-004-python-fastapi.md)
- [ADR-005 — PostgreSQL/PostGIS](ADR-005-postgis.md)
- [ADR-006 — Layer capabilities](ADR-006-layer-capabilities.md)
- [ADR-007 — ChangeSet](ADR-007-changeset.md)
- [ADR-008 — Module architecture](ADR-008-module-architecture.md)
- [ADR-009 — Identity](ADR-009-identity.md)
- [ADR-010 — Object storage](ADR-010-object-storage.md)
- [ADR-011 — Processing jobs](ADR-011-processing-jobs.md)
- [ADR-012 — Deployment](ADR-012-deployment.md)
- [ADR-013 — OIDC-backed application session](ADR-013-oidc-application-session.md)
- [ADR-014 — Explicit tenant context + RLS](ADR-014-explicit-tenant-context-rls.md)

## Proposed ADRs

- [ADR-015 — Rust-native platform migration](ADR-015-rust-platform.md) (proposed; ADR-004 remains accepted)
