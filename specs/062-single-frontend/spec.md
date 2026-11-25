# Feature Specification: Single Frontend and Professional Desktop UX
**Feature**: 062-single-frontend
**Date**: 2026-11-25
**Target**: apps/web-next
**Status**: Implementation started on DEV

## Objective
One runnable frontend, one authenticated application shell, one build output, one shared design system. Retire apps/web only after all reachable workflows and offline routes have verified replacements. Preserve the API, data, permissions and existing project URLs.

## User stories
- P0: GIS operator works with QGIS-style navigation, selection, layers, editing, snapping, attribute tables, forms and layouts without switching to an old UI.
- P0: Administrator uses consistent dialogs, error states, permission controls and session lifecycle.
- P0: Existing field operator can reopen offline packages, preserve drafts and synchronize under the correct identity without losing cached data.
- P1: CAD operator uses AutoCAD-style command line, layers, snapping and document lifecycle with the existing independent WASM engine.
- P1: Analyst uses an ArcGIS Pro-style searchable processing catalog, parameter forms, job history and output layers.
- P1: Plugins use the same shared panels and service boundaries; no secondary SPA.

## Requirements
- FR-001: Eliminate cross-app imports from apps/web-next into apps/web. Move only reachable feature modules and their dependency closure into the canonical app; no duplicate entrypoint or router.
- FR-002: Use one API client/session integration where practical; retain fail-closed protected queries, CSRF, tenant scope and offline identity protections.
- FR-003: Preserve editing/query/processing/cartography/collaboration/field/CAD import/external preview functionality, including real API contracts.
- FR-004: Replace old presentation shells with compact native workbench UI, shared modal focus, keyboard, loading, failure, validation and responsive states.
- FR-005: Preserve legacy public URLs and offline static-shell behavior through explicit compatibility routes; no API responses/private records in service-worker caches.
- FR-006: One workspace package, development entrypoint, production build, Docker image and deployment pipeline. Retire old tests only after equivalent replacement coverage is demonstrated.
- FR-007: Keep engine-specific functionality separate: MapLibre 2D, Cesium 3D, OpenCAD WASM. Do not expose fake controls for unavailable capabilities.
- FR-008: Upgrade layer/attribute panels, editing/navigation, commands, processing, layouts, CAD controls and plugin lifecycle by verified capability, not by cosmetic claims.
- FR-009: Prevent regressions in existing login, organization/project switching, realtime, chat, permissions, export and source-data integrity.
- FR-010: No production rollout until DEV regression, real authenticated workflow and offline acceptance gates pass.

## Acceptance
- SC-001: Canonical build succeeds without apps/web sources, no runtime cross-app imports.
- SC-002: Repository has one frontend SPA/package after migration and removal; updated lockfile, Docker, CI, dev and test configs.
- SC-003: Every previously reachable workflow has a mapped test and real API verification or a documented blocker; no fake parity claim.
- SC-004: Dialog keyboard trap, focus restore, Escape, dangerous action handling, scrolling and desktop/mobile widths verified.
- SC-005: QGIS Space-only pan, selection and editing, export, CAD document open/save, processing job and plugin workflows pass.
- SC-006: Offline cold launch and drafts/sync tested without cross-account leakage; existing drafts preserved.
- SC-007: No production data mutation, no API security weakening, rollback release retained.
