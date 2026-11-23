# Feature Specification: R026 — Integration Hub & Policy Gate (Slice 1)

**Branch:** feature/026-integration-hub-policy
**Date:** 2026-11-23
**Status:** implementation candidate; production release pending
**Parents:** R002 (identity), R003 (CRS), R006 (changesets), R010 (external sources), R012 (CAD), R014 (SDK).

## Objective
A single project-scoped integration catalog for GIS, CAD, cadastral and survey
software. Identify adapters, available operations, authorization requirements
and verification state without falsely claiming that native desktop SDKs are installed.

## User stories
1. A project reader sees supported provider families, file/protocol modes, verified
   status and why an operation is not executable.
2. A manager sees the same catalog with policy eligibility, but cannot execute
   unverified connector writes simply by having a project role.
3. Cross-project / cross-tenant requests are rejected server-side. Service
   principals remain restricted by the permission ceiling and any resource scopes.
4. A technician can contribute a future adapter through the documented manifest
   contract without linking proprietary vendor binaries.

## Acceptance
- Authenticated, project-scoped GET endpoint; no anonymous data access.
- RLS and project.read check before returning catalog.
- Catalog is descriptive, not a connection credential registry or write endpoint.
- Entries have distinct lifecycle, integration mode, operations, and permission
  matrix; unimplemented operations must report disabled.
- Existing WMS/WFS connectors remain in their R010 API with current read-only,
  SSRF and allowlist enforcement; no mutation here.
- Never infer CRS, silently discard unsupported CAD elements or auto-replay
  deletion changes. Per-layer/feature/field permission checks required for any
  future bound data operation, beyond project-level roles.
- Tests assert denied permissions, unknown operations and planned adapters.
- No DB schema change, migrations, connector secrets or live data changes in Slice 1.

## Follow-on slices (not implemented)
- Binding registry: adapter/version/device, organization/project, approved host,
  capabilities, tokens, rotation/revoke, last sync status.
- Read/import previews with strict CRS, mapping, topology and audit.
- Explicit, idempotent revision-based sync approval with per-feature ACL, conflict
  resolution, rate limit, backups and rollback.
- Device connector for licensed V8i on Windows (R025), no cloud-side DCOM.
- Tested GIS desktop plugins and optional bidirectional sync; no promised
  remote control of offline desktop files.
