# Feature Specification: Unified Spatial Workspace
**Feature Branch**: `feature/022-spatial-workspace`
**Created**: 2026-11-23
**Status**: Specification ready; implementation in progress
**Input**: Parent roadmap: [ROADMAP.md, entry R004-A](../../ROADMAP.md#r004). Một project với ba chế độ GIS 2D, CAD và 3D; không làm lại hệ thống GIS hiện hữu.

## User Scenarios & Testing
### US1 — Switch modes in one project (P1)
A user opens a project and chooses GIS, CAD or 3D without switching account, organization or project.
**Independent Test**: open a project, select a layer/feature, switch among available modes, return to GIS.
1. Given GIS is ready, when an authorized available mode is selected, then the project and selected resource identities remain unchanged.
2. Given a mode is not enabled or ready, then it is unavailable with a truthful reason; GIS remains usable.
3. Given a dirty edit or pending save, switching mode is blocked until the user explicitly saves or discards using existing edit workflow.
### US2 — Safe engine lifecycle (P1)
A user loads the engine only when choosing its mode; failure does not replace working GIS or lose context.
**Independent Test**: select available mode twice, fail its initialization, retry, switch account.
1. On successful initialization only the selected engine is active; returning restores the saved GIS camera and selection.
2. On load failure keep the prior active mode and offer retry.
3. On account/org/project change, clear mode state and ignore stale pending load callbacks; no protected data from prior scope remains.
### US3 — Cross-mode interoperability (P2)
A user opens an authorized native CAD asset or geographic 3D layer and sees it linked to the shared project.
**Independent Test**: in a test project locate a building footprint with linked CAD and model assets and inspect identity in all supported modes.
1. An undeclared CRS, drawing units or vertical reference does not silently produce an overlay.
2. Native assets retain their original information. Derived representations carry references and provenance.
3. Access follows the existing project/resource authorization; unsupported operations are disabled.
### Edge Cases
Rapid tab switching, unavailable engine, revoked access, stale callbacks, unsaved edits, unlinked CAD, unknown CRS/units, WebGL failure, mobile device resource limit, deep link to unavailable mode.

## Requirements
### Functional Requirements
- FR-001: Shared project context and GIS default; expose exactly GIS 2D, CAD, 3D.
- FR-002: Mode availability derives from actual enabled capabilities and authorization, not names.
- FR-003: Preserve scoped selection and explicitly referenced camera state across mode switching; never invent a coordinate transform.
- FR-004: Do not switch on unresolved dirty edits or pending saves.
- FR-005: Request engine initialization on demand, handle loading/error and retain previous active mode until success.
- FR-006: Cancel/ignore stale requests on rapid switching or scope changes; clear protected state on scope change.
- FR-007: CAD/3D resources reuse existing authorization, identifiers and lifecycle; no credentials passed to unapproved origins.
- FR-008: Link original/derived asset and feature IDs by project; CRS/units/vertical datum must be explicit before spatial placement.
- FR-009: Maintain current frontend features, plugin boundary and GIS state; no migrations for mode-shell slice.
- FR-010: Provide automated state, browser, authorization/failure and regression evidence before release.
### Key Entities
ProjectScope, SpatialMode, ModeCapability, WorkspaceSession, CameraReference, ScopedSelection, ModeLoadRequest, AssetLink.
## Success Criteria
- SC-001: Switching available modes keeps project ID and selection in all three tested routes; GIS returns to original camera.
- SC-002: All dirty/pending, unavailable, denied, failed and stale request cases preserve current authorized state; 0 cross-scope leaks.
- SC-003: No CAD or 3D engine loaded before explicit mode activation; only one additional engine active after successful transition.
- SC-004: Existing project edit and plugin flows pass unchanged; no severities P0/P1 in browser verification.
- SC-005: At least one authorized georeferenced CAD/model fixture succeeds with documented tolerance before R012-A/R004-B acceptance; otherwise modes remain honestly unavailable.
## Assumptions and boundaries
V1 is mode shell and adapter contract; native CAD editing and genuine 3D rendering are separately gated child slices, not fabricated previews. Existing CAD engine is external, subject to connectivity/authorization verification. Spec 020 is an active concurrent rewrite; integration only after a clean reviewed baseline. No publication/deployment or permission expansion in this feature.
