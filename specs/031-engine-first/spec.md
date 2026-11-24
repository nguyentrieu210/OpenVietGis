# Feature Specification: Engine-first migration (R031)
**Feature branch:** `031-engine-first`
**Created:** 2026-11-24
**Status:** Draft / initial inventory; implementation and release gates open.
**Parent:** [R031](../../ROADMAP.md#r031); supports [unified v1.0](../030-v1-unified-release/spec.md).
**Baseline inspected:** `27f0b35af966b7a46e7b17929395d9bc01515e51`.

## Goal
Prefer proven upstream spatial engines over bespoke algorithms where an evidence-based comparison demonstrates value. Preserve OpenVietGIS as the authoritative project, data, identity, authorization, revision, collaboration and integration platform. Improve stability, speed and maintainability without changing user ownership or abandoning existing work.

## User scenarios
### US1 — Reliable GIS editing and processing (P1)
An editor imports a representative dataset, draws/snaps/edits, runs buffer/clip/dissolve, reviews results, saves and exports. Independently compare geometry, attributes, CRS and audit history with the existing engine and reference fixtures. Unsupported operations are explained, not simulated.
### US2 — CAD/3D within one workspace (P2)
Open a supported drawing or georeferenced model, select features, switch modes and save authorized changes without losing drafts, changing identity or silently transforming data. Verify the existing OpenCAD relay before changing it. Compare a dedicated 3D engine against the existing bounded renderer.
### US3 — Predictable performance and synchronization (P1)
A user reopens a project, edits on web or QGIS and observes ordered deltas on the other client; no full project reload is required for an ordinary feature change. Reconnect/retry preserves changes, and permission revocation denies reads/writes. A failed optional engine does not break GIS.
### US4 — Maintainable engine replacement (P1)
A maintainer registers a version-pinned adapter behind an existing capability boundary, compares it with legacy on the same fixtures, disables it by configuration and rolls back without data loss.

## Functional requirements
- FR-001: Inventory every existing command/capability with source paths, runtime dependencies, upstream candidates, tests, decision (KEEP/REUSE/REPLACE/REMOVE/DEFER), owner and evidence; no claim of full audit until coverage is verified.
- FR-002: Reuse-first rule: package/official API before fork; no custom algorithm or new engine dependency without recorded trade-off. Preserve current supported features until replacement parity is proven.
- FR-003: Preserve R002/R003/R006/R014 contracts, tenant/actor/project ACL and RLS, revision/ChangeSet and provenance. Engines cannot write storage directly or bypass policy. Browser output is untrusted input.
- FR-004: Reuse stable command/capability registry and lazy lifecycle from R004/R022. Add only real adapter seams. Capability state reflects actual readiness, permission, input and format support.
- FR-005: Compare browser editing/processing candidates and PostGIS reference operations; support deterministic fixtures, cancellation, limits, versioned outputs and meaningful errors.
- FR-006: Audit existing GDAL/OGR, PROJ, PostGIS and ezdxf paths; do not introduce duplicate stacks. Validate explicit CRS, axis order, units, grids, control points and native/derived asset provenance.
- FR-007: Keep OpenCAD dedicated-origin boundary and validate relay version, actual WASM artifact, license, read-only, save/retry and supported roundtrip. No unsupported DWG or CAD accuracy claims.
- FR-008: Compare existing bounded 3D/glTF/tiles renderer with a pinned upstream candidate; enforce project-scoped asset fetching, no unrestricted external URI resolution, bounded memory and disposal.
- FR-009: Improve data delivery independently of engine selection: revision-keyed caches, viewport/pagination, ordered deltas, outbox/idempotency, reconnect and scoped invalidation. Full reload only for documented recovery.
- FR-010: AI/MCP, connectors and UI use the same authorized command service and approval rules; no direct engine write paths.
- FR-011: Require dependency/license/SBOM review including WASM binaries, transitive assets, third-party notices and security updates. No mandatory paid cloud service or vendor tokens for core offline/self-host scenarios.
- FR-012: Promote per capability via disabled-by-default flag, shadow comparison and opt-in isolated candidate. Keep compatibility adapter until acceptance and rollback drill. No schema deletion or irreversible migration.
- FR-013: Keep R030 release scope and R018 independent evidence requirements. Freeze net-new features outside stabilization/required parity; report blocked features honestly.
- FR-014: Retire code only with callsite/dependency audit, compatibility tests, measured benefit, review and recovery path.

## Success criteria
- SC-001: 100% of registered commands and mode capabilities inventoried, unmapped items fail audit; every replacement decision has pinned upstream/license and reviewed evidence.
- SC-002: All selected critical workflow fixtures pass old/new comparison: exact feature IDs/counts/attributes, explicit CRS, geometry/topology tolerance fixed before test; no silent loss or unauthorized change.
- SC-003: 0 cross-tenant access, stale-revision overwrites, duplicate committed operations or lost acknowledged edits in adversarial tests.
- SC-004: Baseline and candidate p50/p95 latency, bytes, memory, startup and bundle recorded on fixed profiles; P1 regressions over 10% require reviewed exception. Target >=30% improvement for the bottleneck being replaced, or documented maintainability/correctness benefit without material regression.
- SC-005: 0 full-layer downloads for ordinary single-feature deltas after bootstrap in controlled two-client test; 100 sequential mutations converge, including reconnect/replay; target p95 <=2s on specified LAN profile.
- SC-006: GIS loads without optional CAD/3D/processing bundles; 20 mode-switch cycles do not leak active workers/WebGL contexts or lose dirty drafts; failure keeps prior usable mode.
- SC-007: Per-engine flag rollback and exact candidate restore proven on synthetic data; old and new read existing project artifacts; R030 mandatory tracks remain gated.

## Non-goals
No whole-app fork, no big-bang rewrite, no new parallel project/identity database, no broad feature parity promise with QGIS/AutoCAD, no unreviewed upstream binary, no production deployment or production data test from this spec alone.
