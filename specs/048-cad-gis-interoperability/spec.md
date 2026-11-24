# R048 — CAD–GIS–3D interoperability

**Feature Branch**: `048-cad-gis-interoperability` | **Created**: 2026-11-24 | **Status**: Draft, specification only
**Input**: Two independent specialist engines, shared project identity, precise exchange, authorized two-way incremental synchronization and AI/plugin interoperability.

## Scope and dependencies
Extend R003 CRS/catalog, R006 ChangeSet, R012 CAD precision, R022 workspace, R026/R027 integration bindings, R029 managed realtime, R030 unified v1 acceptance and R031 engine-first adapter. No parallel spatial core, authorization store, event bus or public SDK. R031 PR #21 remains draft; adapter integration is gated on its reviewed merge. Independent CAD quality work may proceed against pinned OpenVietCAD `2bc3537346ff2a844039685186a1b32da7149d6f`. No live change in this spec.

## User scenarios & testing
### US1 — Locate the same object in GIS 2D/3D and CAD (P1)
Given an authorized project, a valid placement and a revision-pinned link, selecting a parcel/pipe in one view highlights its counterpart and offers zoom in the other. Missing/stale links show a reason, never guess by proximity. Dirty CAD drafts prevent destructive mode switches. Test synthetic linked, unlinked, stale and revoked objects with real browser relay.
### US2 — Exchange geometry and metadata without silent loss (P1)
Given a drawing with explicit units, axes, CRS or reviewed control-point placement, preview selected entities/layers, CRS residuals and mapping losses. Approve only valid output; preserve source bytes, native handles and metadata. Reverse export to DXF first, DWG only after adapter qualification. Test point/line/polygon, curves, Z, text, block, hatch and invalid topology; compare independent geometry and metadata.
### US3 — Edit both ways with explicit review (P1)
Given a linked feature and current CAD/GIS revisions, preview create/update/delete deltas from either engine, validate topology and scope, approve, receive durable receipt. Preserve CAD document versions and canonical GIS history. No implicit delete or last-write-wins. Test duplicate requests, version mismatch, network failure and denied operations.
### US4 — Realtime and conflict recovery (P2)
Given multiple authorized sessions, accepted changes are visible via ordered R029 deltas; retry, restart, loss/reordering and cursor expiry converge without echo loops. CAD may mark document stale and offer reviewed reload/rebase until safe incremental entity mutation is proven. Never overwrite local unsaved edits. Test two users plus Web/QGIS; external adapters qualify separately.
### US5 — Governance, offline and AI (P3)
Admin controls binding capabilities, authority, scope and revocation; offline drafts never count as committed, reconnect rechecks rights; AI and plugins preview/approve via same registered commands and policy. Test revoked/expired sessions, forged clients, cross-tenant and field/region restrictions. Unsupported offline writes fail closed.
### US6 — Fast, maintainable CAD runtime (P1; independent)
CAD opens on demand, with versioned assets, measurable cold/warm load and real tool fixtures; theme/locale/branding runtime configuration preserves preferences and license notices. Independent Rust/Cargo CI caching and pinned artifacts; GIS-only build does not recompile CAD. Test fresh and existing browser profiles, rollback, DWG/DXF roundtrip and representative commands.

## Functional requirements
- FR-001: Shared project-scoped stable identity mapping supports one-to-many links, immutable native handles per document revision, lineage on split/merge/delete, no automatic relinking by proximity.
- FR-002: Explicit CRS, local origin, axis order, units, Z/vertical datum, control points and transform pipeline. Unknown data blocked; report residuals and tolerance; no survey-grade claim.
- FR-003: Reversible mapping profile for point/line/polyline/polygon, layer, attributes, style, text, block, hatch, curves, mesh and solid. Native bytes and loss manifest retained; unsupported entities read-only or excluded explicitly.
- FR-004: Preview geometry validity, ring closure, topology and schema; independent comparison; user approves exact revision-pinned output.
- FR-005: Selection/zoom with generation tokens, link cardinality, stale states and capability negotiation; no unauthorized IDs or data.
- FR-006: R006 exclusive GIS mutation path and versioned CAD saves; cross-store staged bytes plus transactional publication/receipt or explicit recoverable saga; no dual writes or false atomicity.
- FR-007: Reuse R027 bindings and R029 cursor/event delivery, idempotent operations and origin deduplication; SSE wakeup only, authorized delta retrieval; no full reload steady state for supported deltas.
- FR-008: Conflict review, optional advisory leases, compensating authorized revert, immutable audit and dependency impact review; deletes default disabled.
- FR-009: Server-derived actor/tenant/project, current ACL on reads/previews/commits/replay, revocation and scope filtering. No credentials via isolated relay. Unsupported fine-grained policy fails closed; file-level access cannot redact arbitrary CAD entities.
- FR-010: CAD document version conflicts, dirty draft guards and explicit authority per binding; uncommitted work never overwritten; offline outbox only when approved and encrypted as appropriate.
- FR-011: R031 capability descriptor/command adapter/lifecycle, R021 MCP approval and allowlist; QGIS/V8i adapters independently gated, no arbitrary command execution or implicit desktop support.
- FR-012: Lazy loading, immutable hashed assets, measured cache and performance budgets; no new dependency without review. Preserve upstream GPL-3.0 notices and corresponding source; release exact pinned artifacts.
- FR-013: User-facing states distinguish pending, accepted, stale, denied, offline, conflict, unsupported and failed; telemetry excludes secrets and denied data.
- FR-014: Default-off flags per binding/capability; rollback and fixture-based security/performance/recovery gates; no production data for acceptance.

## Success criteria (targets, not results)
- SC-001: All linked selection scenarios pass both directions; p95 <= 500 ms after local views ready on 100 ms RTT, 1000 mapped objects; no stale/denied highlights.
- SC-002: 100% supported simple entities roundtrip within declared tolerance (reference projected fixture 1 mm; geographic tolerance documented); 100% unsupported/lossy entities explicitly reported, original bytes recoverable. VN-2000 requires explicit approved transform and independent controls.
- SC-003: 100 scripted duplicate/reorder/restart/conflict scenarios yield zero lost/duplicate accepted edits and no silent overwrite; every accepted operation has audit receipt.
- SC-004: For qualified incremental capabilities, p95 propagation <= 2 s (10 clients, 10k features, 1 edit/s, RTT <= 100 ms), no full snapshot during steady state. Document-only mode labeled separately.
- SC-005: All negative ACL/tenant/revocation/forged relay/AI tests reject; no unauthorized payload; next write denied and active client becomes read-only <= 15 s on reference network.
- SC-006: Benchmark cold/warm CAD start and compressed bytes on fixed reference device/network; target >= 30% cold improvement over measured branded baseline and no regression in DWG/DXF/precision. Warm immutable engine asset transferred zero bytes on normal cached reload. Report limitations rather than fabricate results.
- SC-007: Real browser CAD/GIS/3D scenario, exact release SHA/artifact hashes, restore/rollback drill and human acceptance before stable enablement; QGIS/V8i native acceptance independent.

## Non-goals
Full AutoCAD parity, direct database credentials, automatic lossless mapping of all native entities, realtime joint CAD authoring before safe entity mutation, CAD dimension/paper-space duplication in GIS, new engine core or production deployment during spec.
