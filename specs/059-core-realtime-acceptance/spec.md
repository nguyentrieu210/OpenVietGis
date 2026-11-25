# Spec 059 — Core, Realtime & Reference Plugin Acceptance

**Branch:** `spec/059-core-realtime-acceptance` | **Created:** 2026-11-25
**Status:** Implementation in progress. 11 of 34 tasks have supporting evidence. Release acceptance remains open.
**Initial baseline:** `5cdb16ccb567121095acd691c5de6481e4963ded` (main and live at 18:44 ICT).
**Synchronized main:** `4e721a9f04ea933324da1d5ff4f433e40f75dbb9` (verified on main and live during the implementation run).
**Parent:** R031 engine-first, R032–R047 convergence; roadmap R003/R004/R005/R006/R007/R011/R013/R014/R015. Integrates acceptance of R027/R029 and R039/R040; respects R051/053 house-numbering scope.
**Input:** Owner asks to open the next Spec Kit round after architecture deployment; prioritize depth and stability over new functions.

## Scope and non-goals
Reuse PostGIS, existing MapLibre/Cesium, managed QGIS, changesets, existing module/SDK APIs. No engine replacement, new module, mass rewrite or speculative infrastructure. No real personal data on public demo; only synthetic/deidentified or separately authorized private acceptance. No production mutations, deployment, removal or migration without explicit authorization and rollback evidence. Do not change approval or numbering business rules; R051/053 own domain parity.

## User scenarios
- US1/P0: An authenticated editor opens a large project, navigates, filters all authorized rows, reloads unchanged, and edits a single feature without reloading unrelated layers. Verify data counts, revision, cache scope, network trace and resource budgets.
- US2/P0: Two independent clients (Web and native QGIS) edit, disconnect, reconnect, replay and encounter stale revisions or revoked permissions. Verify exact IDs, monotonic cursor, once-only durable receipt, preserved drafts and no leaked or overwritten data.
- US3/P0: An authorized operator selects a house on the map and its business record in the plugin; edits permitted geometry through Core, sees updated link/validation, and retains approved numbering until explicit authorized review. Disable/re-enable module without losing GIS capability or data.
- US4/P0: A maintainer validates one pinned candidate across backend, frontend, desktop package, SDK and live-compatible schema; reviews UI differences, rehearses recovery and records evidence for release decisions.
- US5/P1: A developer activates a second generic reference module through documented extension points, while a denied or incompatible module fails closed. Core continues without vertical imports.

## Requirements
- FR-001 Authoritative geometry and revisions stay in Core/PostGIS; no second authoritative geometry copy; module references stable project-scoped source/feature IDs, not names or positional indexes.
- FR-002 Authorize before snapshot/tile/delta display and before commit; invalidate caches on permission change; isolate tenants/accounts/projects and handle restricted scopes without widening.
- FR-003 Revision-pinned snapshots, bounded viewport reads, keyset pagination, safe MVT and delta application; gap or stale cursor triggers scoped recovery, never silent corruption.
- FR-004 Real server-side attribute filtering/sorting/search for supported remote datasets; accurately disclose unsupported semantics.
- FR-005 Common ChangeSet, operation ID, receipt, cursor and conflict contract for Web/QGIS/module clients. Replays preserve receipt identity. Unknown result never becomes success.
- FR-006 Durable local drafts and reconnect UX; revoked credentials block writes and access, preserve only appropriately protected local drafts per policy. No unauthenticated offline authorization by inference.
- FR-007 Explicit CRS and independently validated control points, round-trip and topology; reject ambiguous VN-2000 zone/parameters. No silent transform.
- FR-008 Core selection/event/editing/permission contracts exposed via documented adapter; module lifecycle and linked feature deletion guard use public contracts. Approved business records not overwritten by geometry events.
- FR-009 Test cross-client event propagation, cross-mode selection and CAD/3D regression without expanding CAD/3D capabilities.
- FR-010 Exact-SHA CI and UI screenshot review; retain failed or NOT_RUN evidence, no blanket disabling or blind baseline regeneration. Backup/restore and rollback rehearsal on isolated profile; no production write.
- FR-011 Report p50/p95, requests, bytes, memory, CPU, dataset, concurrency, CRS, fixture hash, hardware and network; separate SQL-only and end-to-end measurements.
- FR-012 Maintain traceability to open R031–R047 tasks; close only on reproducible evidence, explicit deferred items and independent review.

## Measurable acceptance
- AC01: eligible unchanged warm reload makes zero full-feature-page calls; one-feature update makes zero unrelated layer refreshes; verify no stale/unauthorized cache display.
- AC02: 10k, 100k, 1M synthetic fixtures; optionally 10M only on isolated budget-approved environment. Run repeated cold/warm and viewport/edit traces, compare against pinned baseline. Target p95 warm map reopen <=2s and commit-to-visible <=2s on declared LAN profile; no universal SLA. Record if unmet, do not mark pass.
- AC03: 100 mixed mutations over two clients, disconnect/reconnect and replay, exact final IDs and revisions, zero duplicates/lost edits; stale and permission-revoked writes rejected. Include actual Windows QGIS and real network; mocks separately labeled.
- AC04: source/feature identity stable through rename and style changes; feature deletion/approval/geometry update cases preserve referential and business constraints; plugin-disabled Core smoke passes.
- AC05: supported EPSG:4326, 3857 and specified VN-2000 fixtures pass independently agreed numeric tolerance against reference controls; ambiguous CRS fails closed.
- AC06: zero cross-tenant data exposures in authorization, tile, cache, SDK and change tests. Validate runtime RLS role, not superuser.
- AC07: full exact candidate CI green, reviewed screenshot diffs, desktop package checksum, isolated recovery drill, release gate and independent review. Unavailable Windows/authorized datasets remain BLOCKED, not PASS.
- AC08: reference plugin plus second materially different module validate SDK extension points; record supported compatibility matrix without claiming general SDK stability before ADR-008 criteria.

## Decisions and open constraints
Thresholds above are proposed engineering acceptance targets for a documented LAN profile, not measured live values. Native Windows/QGIS access, independent reviewers, private dataset authorization, test capacity and CRS control fixtures may require external cooperation. Defer nonblocking CAD/3D depth to existing specs; document every residual gate.
